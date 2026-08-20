use crate::providers::traits::{
    ChatMessage, ChatRequest as ProviderChatRequest, ChatResponse as ProviderChatResponse, Provider,
    TokenUsage, ToolCall as ProviderToolCall,
};
use crate::tools::ToolSpec;
use async_trait::async_trait;
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const NETWORK_ERROR_SOURCE_LIMIT: usize = 4;
const NETWORK_ERROR_FRAGMENT_LIMIT: usize = 240;

/// Bounded timeouts applied to provider network calls.
///
/// - `connect`: max time establishing the provider connection.
/// - `stream_idle`: max continuous period without valid stream activity;
///   reset whenever stream data is received.
/// - `request`: max wall-clock duration of one complete model request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderTimeouts {
    pub connect: Duration,
    pub stream_idle: Duration,
    pub request: Duration,
}

impl ProviderTimeouts {
    pub fn from_config(config: &crate::config::ProviderRuntimeConfig) -> Self {
        Self {
            connect: config.connect_timeout(),
            stream_idle: config.stream_idle_timeout(),
            request: config.request_timeout(),
        }
    }
}

impl Default for ProviderTimeouts {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(crate::config::DEFAULT_CONNECT_TIMEOUT_SECS),
            stream_idle: Duration::from_secs(crate::config::DEFAULT_STREAM_IDLE_TIMEOUT_SECS),
            request: Duration::from_secs(crate::config::DEFAULT_REQUEST_TIMEOUT_SECS),
        }
    }
}

/// Distinct timeout classes preserved through the internal error path so
/// callers/diagnostics can tell them apart instead of collapsing everything
/// into a generic provider failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderTimeoutKind {
    Connect,
    StreamIdle,
    Request,
}

impl ProviderTimeoutKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Connect => "connect_timeout",
            Self::StreamIdle => "stream_idle_timeout",
            Self::Request => "request_timeout",
        }
    }
}

fn timeout_error_message(kind: ProviderTimeoutKind, secs: u64) -> String {
    match kind {
        ProviderTimeoutKind::Connect => {
            format!("provider connect timeout ({}s): could not establish a connection to the model service", secs)
        }
        ProviderTimeoutKind::StreamIdle => {
            format!("provider stream idle timeout ({}s): no stream data from the model service", secs)
        }
        ProviderTimeoutKind::Request => {
            format!("provider request timeout ({}s): the model service did not finish in time", secs)
        }
    }
}

/// Renders an optional measurement as `-` when it does not apply, so a real
/// `0` is never confused with "no value".
fn optional_field(value: Option<impl ToString>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

/// Safe structured provider error diagnostics. Never logs API keys,
/// credentials, prompts, skill instructions, or document contents.
fn log_provider_error(
    model: &str,
    stage: &str,
    error_kind: &str,
    elapsed_ms: Option<u64>,
    timeout_ms: Option<u64>,
    http_status: Option<u16>,
) {
    tracing::error!(
        target: "omninova_core::providers::timeout",
        model = %model,
        stage = %stage,
        error_kind = %error_kind,
        elapsed_ms = %optional_field(elapsed_ms),
        timeout_ms = %optional_field(timeout_ms),
        http_status = %optional_field(http_status),
        "[provider-error]"
    );
}

/// Safe structured success metrics for streaming responses (debug level).
fn log_provider_stream_ok(model: &str, first_delta_ms: u64, total_elapsed_ms: u64) {
    tracing::debug!(
        target: "omninova_core::providers::timeout",
        model = %model,
        first_delta_ms = %first_delta_ms,
        total_elapsed_ms = %total_elapsed_ms,
        "[provider-stream]"
    );
}

#[derive(Debug, PartialEq, Eq)]
struct NetworkErrorDiagnostics {
    host: String,
    is_connect: bool,
    is_timeout: bool,
    is_request: bool,
    is_body: bool,
    is_decode: bool,
    source_chain: Vec<String>,
}

impl NetworkErrorDiagnostics {
    fn from_reqwest(base_url: &str, error: &reqwest::Error) -> Self {
        let host = reqwest::Url::parse(base_url)
            .ok()
            .and_then(|url| url.host_str().map(ToString::to_string))
            .unwrap_or_else(|| "unknown".to_string());
        let mut source_chain = Vec::new();
        let mut source = std::error::Error::source(error);
        while let Some(current) = source {
            source_chain.push(sanitize_network_error_fragment(&current.to_string()));
            if source_chain.len() >= NETWORK_ERROR_SOURCE_LIMIT {
                break;
            }
            source = current.source();
        }

        Self {
            host,
            is_connect: error.is_connect(),
            is_timeout: error.is_timeout(),
            is_request: error.is_request(),
            is_body: error.is_body(),
            is_decode: error.is_decode(),
            source_chain,
        }
    }
}

fn sanitize_network_error_fragment(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    if [
        "authorization",
        "bearer ",
        "api_key",
        "api-key",
        "access_token",
        "app_secret",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return "<redacted-sensitive-error-source>".to_string();
    }

    let mut sanitized = String::with_capacity(input.len());
    let mut rest = input;
    loop {
        let http = rest.find("http://");
        let https = rest.find("https://");
        let next = match (http, https) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (Some(index), None) | (None, Some(index)) => Some(index),
            (None, None) => None,
        };
        let Some(index) = next else {
            sanitized.push_str(rest);
            break;
        };
        sanitized.push_str(&rest[..index]);
        sanitized.push_str("<url-redacted>");
        let url_tail = &rest[index..];
        let end = url_tail
            .find(char::is_whitespace)
            .unwrap_or(url_tail.len());
        rest = &url_tail[end..];
    }

    let mut chars = sanitized.chars();
    let mut truncated = chars
        .by_ref()
        .take(NETWORK_ERROR_FRAGMENT_LIMIT)
        .collect::<String>();
    if chars.next().is_some() {
        truncated.push_str("...");
    }
    truncated
}

pub struct OpenAiProvider {
    base_url: String,
    credential: Option<String>,
    model: String,
    temperature: f64,
    max_tokens: Option<u32>,
    timeouts: ProviderTimeouts,
    client: Client,
}

struct ConsumeOutcome {
    text: String,
    reasoning: String,
    tool_accum: Vec<(String, String, String)>,
    usage: Option<TokenUsage>,
    finish_reason: Option<String>,
}

#[derive(Debug)]
enum StreamConsumeError<E> {
    Idle { secs: u64 },
    /// The stream's own error, passed through unchanged so the caller keeps
    /// full `reqwest::Error` classification.
    Transport(E),
}

/// Consumes an SSE byte stream, applying the idle timeout per chunk and
/// forwarding text deltas through `token_tx`. Extracted from
/// `chat_stream_inner` so the timeout semantics are unit-testable without
/// any network access; it is generic over the stream item and error types so
/// the production path can hand over `bytes_stream()` untouched.
///
/// The idle timer resets whenever a chunk arrives (valid or not — the same
/// semantics as a raw byte stream); only a completely silent stream trips it.
/// If text has already arrived, an idle timeout or transport error preserves
/// the received answer (some services omit `[DONE]` or keep the connection
/// open after the final text), matching the original behavior.
async fn consume_sse_stream<S, B, E>(
    stream: &mut S,
    idle_timeout: Duration,
    token_tx: &tokio::sync::mpsc::UnboundedSender<String>,
    first_delta_ms: &AtomicU64,
    started: Instant,
) -> Result<ConsumeOutcome, StreamConsumeError<E>>
where
    S: futures_util::Stream<Item = Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
{
    use futures_util::StreamExt;

    let mut buf = String::new();
    let mut text = String::new();
    let mut reasoning = String::new();
    // (id, name, arguments) accumulated per tool-call index.
    let mut tool_accum: Vec<(String, String, String)> = Vec::new();
    let mut usage: Option<TokenUsage> = None;
    let mut finish_reason: Option<String> = None;
    let mut done = false;

    loop {
        let next = match tokio::time::timeout(idle_timeout, stream.next()).await {
            Ok(next) => next,
            Err(_)
                if (!text.is_empty() || !reasoning.is_empty())
                    && !has_started_tool_call(&tool_accum) =>
            {
                // Some OpenAI-compatible services omit [DONE] or keep the
                // connection open after the final text. Preserve the answer
                // that has already arrived rather than spinning forever.
                break;
            }
            Err(_) => {
                return Err(StreamConsumeError::Idle {
                    secs: idle_timeout.as_secs(),
                });
            }
        };
        let Some(item) = next else { break };
        let bytes = match item {
            Ok(bytes) => bytes,
            Err(_)
                if (!text.is_empty() || !reasoning.is_empty())
                    && !has_started_tool_call(&tool_accum) =>
            {
                // If a transport closes after a complete text answer, return
                // the received answer. Incomplete tool calls remain errors.
                break;
            }
            Err(error) => return Err(StreamConsumeError::Transport(error)),
        };
        buf.push_str(&String::from_utf8_lossy(bytes.as_ref()));

        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim_end_matches('\r').trim().to_string();
            buf.drain(..=pos);
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                done = true;
                break;
            }
            let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) else {
                continue;
            };
            if let Some(u) = chunk.usage {
                usage = Some(TokenUsage {
                    input_tokens: u.prompt_tokens,
                    output_tokens: u.completion_tokens,
                });
            }
            if let Some(choice) = chunk.choices.into_iter().next() {
                if let Some(reason) = choice.finish_reason {
                    finish_reason = Some(reason);
                }
                if let Some(c) = choice.delta.content {
                    if !c.is_empty() {
                        if first_delta_ms.load(Ordering::Relaxed) == 0 {
                            first_delta_ms
                                .store(started.elapsed().as_millis() as u64, Ordering::Relaxed);
                        }
                        text.push_str(&c);
                        let _ = token_tx.send(c);
                    }
                }
                if let Some(r) = choice.delta.reasoning_content {
                    reasoning.push_str(&r);
                }
                if let Some(tcs) = choice.delta.tool_calls {
                    for tc in tcs {
                        let idx = tc.index.unwrap_or(0);
                        while tool_accum.len() <= idx {
                            tool_accum.push((String::new(), String::new(), String::new()));
                        }
                        let slot = &mut tool_accum[idx];
                        if let Some(id) = tc.id {
                            if !id.is_empty() {
                                slot.0 = id;
                            }
                        }
                        if let Some(f) = tc.function {
                            if let Some(n) = f.name {
                                if !n.is_empty() {
                                    slot.1 = n;
                                }
                            }
                            if let Some(a) = f.arguments {
                                slot.2.push_str(&a);
                            }
                        }
                    }
                }
            }
        }
        if done {
            break;
        }
    }

    Ok(ConsumeOutcome {
        text,
        reasoning,
        tool_accum,
        usage,
        finish_reason,
    })
}

#[derive(Debug, Serialize)]
struct NativeChatRequest {
    model: String,
    messages: Vec<NativeMessage>,
    temperature: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<NativeToolSpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

// --- Streaming (SSE) chunk shapes ---
#[derive(Debug, Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<UsageInfo>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<StreamToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct StreamToolCallDelta {
    #[serde(default)]
    index: Option<usize>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<StreamFuncDelta>,
}

#[derive(Debug, Default, Deserialize)]
struct StreamFuncDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

fn has_started_tool_call(tool_accum: &[(String, String, String)]) -> bool {
    tool_accum
        .iter()
        .any(|(id, name, arguments)| !id.is_empty() || !name.is_empty() || !arguments.is_empty())
}

#[derive(Debug, Serialize)]
struct NativeMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<NativeToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_content: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NativeToolSpec {
    #[serde(rename = "type")]
    kind: String,
    function: NativeToolFunctionSpec,
}

#[derive(Debug, Serialize, Deserialize)]
struct NativeToolFunctionSpec {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct NativeToolCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    function: NativeFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct NativeFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct NativeChatResponse {
    choices: Vec<NativeChoice>,
    #[serde(default)]
    usage: Option<UsageInfo>,
}

#[derive(Debug, Deserialize)]
struct UsageInfo {
    #[serde(default)]
    prompt_tokens: Option<u64>,
    #[serde(default)]
    completion_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct NativeChoice {
    message: NativeResponseMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NativeResponseMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<NativeToolCall>>,
}

impl NativeResponseMessage {
    fn effective_content(&self) -> Option<String> {
        match &self.content {
            Some(c) if !c.is_empty() => Some(c.clone()),
            _ => self.reasoning_content.clone(),
        }
    }
}

async fn api_error(provider_name: &str, model: &str, response: Response) -> anyhow::Error {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    log_provider_error(
        model,
        "request",
        "http_error",
        None,
        None,
        Some(status.as_u16()),
    );
    anyhow::anyhow!("{provider_name} API error ({status}): {text}")
}

impl OpenAiProvider {
    pub fn new(
        base_url: Option<&str>,
        credential: Option<&str>,
        model: impl Into<String>,
        temperature: f64,
        max_tokens: Option<u32>,
        timeouts: ProviderTimeouts,
    ) -> Self {
        let base_url = base_url
            .map(|u| u.trim_end_matches('/').to_string())
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        let client = Client::builder()
            // A broken proxy or unreachable provider should fail quickly instead
            // of leaving the desktop UI in an endless "running" state.
            .connect_timeout(timeouts.connect)
            .build()
            .expect("failed to build reqwest client");

        Self {
            base_url,
            credential: credential.map(ToString::to_string),
            model: model.into(),
            temperature,
            max_tokens: max_tokens.filter(|v| *v > 0),
            timeouts,
            client,
        }
    }

    /// 构建 POST 请求。本地推理服务（如 Ollama / LM Studio）通常无需 API Key，
    /// 因此仅在存在 credential 时附加 `Authorization` 头，否则不带鉴权直接请求。
    fn authorized_post(&self, path: &str) -> reqwest::RequestBuilder {
        let req = self.client.post(self.endpoint_url(path));
        match self.credential.as_ref() {
            Some(credential) => req.header("Authorization", format!("Bearer {credential}")),
            None => req,
        }
    }

    /// 构建 GET 请求，鉴权头处理同 [`authorized_post`]。
    fn authorized_get(&self, path: &str) -> reqwest::RequestBuilder {
        let req = self.client.get(self.endpoint_url(path));
        match self.credential.as_ref() {
            Some(credential) => req.header("Authorization", format!("Bearer {credential}")),
            None => req,
        }
    }

    fn endpoint_url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn request_error(&self, operation: &str, error: &reqwest::Error) -> anyhow::Error {
        let diagnostics = NetworkErrorDiagnostics::from_reqwest(&self.base_url, error);
        let source_0 = diagnostics
            .source_chain
            .first()
            .map(String::as_str)
            .unwrap_or("none");
        let source_1 = diagnostics
            .source_chain
            .get(1)
            .map(String::as_str)
            .unwrap_or("none");
        let source_2 = diagnostics
            .source_chain
            .get(2)
            .map(String::as_str)
            .unwrap_or("none");
        let source_3 = diagnostics
            .source_chain
            .get(3)
            .map(String::as_str)
            .unwrap_or("none");
        tracing::error!(
            target: "omninova_core::providers::network",
            provider = "openai-compatible",
            host = %diagnostics.host,
            is_connect = diagnostics.is_connect,
            is_timeout = diagnostics.is_timeout,
            is_request = diagnostics.is_request,
            is_body = diagnostics.is_body,
            is_decode = diagnostics.is_decode,
            source_0 = %source_0,
            source_1 = %source_1,
            source_2 = %source_2,
            source_3 = %source_3,
            "[provider-network-error]"
        );

        let connect_timeout = diagnostics.is_connect && diagnostics.is_timeout;
        let error_kind = if connect_timeout {
            ProviderTimeoutKind::Connect.as_str()
        } else if diagnostics.is_timeout {
            "timeout"
        } else if diagnostics.is_connect {
            "connect_error"
        } else if diagnostics.is_body {
            "body_error"
        } else if diagnostics.is_decode {
            "decode_error"
        } else if diagnostics.is_request {
            "request_error"
        } else {
            "network_error"
        };
        log_provider_error(
            &self.model,
            if connect_timeout { "connect" } else { "request" },
            error_kind,
            None,
            connect_timeout.then(|| self.timeouts.connect.as_millis() as u64),
            None,
        );

        if connect_timeout {
            return anyhow::anyhow!(timeout_error_message(
                ProviderTimeoutKind::Connect,
                self.timeouts.connect.as_secs()
            ));
        }

        let category = if diagnostics.is_timeout {
            "请求超时"
        } else if diagnostics.is_connect {
            "连接失败"
        } else if diagnostics.is_body {
            "响应流读取失败"
        } else if diagnostics.is_decode {
            "响应解码失败"
        } else if diagnostics.is_request {
            "请求构建或发送失败"
        } else {
            "网络请求失败"
        };
        anyhow::anyhow!(
            "{operation}：{category}（provider=openai-compatible, host={}）；详情见 provider-network-error 日志",
            diagnostics.host
        )
    }

    fn convert_tools(tools: Option<&[ToolSpec]>) -> Option<Vec<NativeToolSpec>> {
        tools
            .filter(|items| !items.is_empty())
            .map(|items| {
                items
                    .iter()
                    .map(|tool| NativeToolSpec {
                        kind: "function".to_string(),
                        function: NativeToolFunctionSpec {
                            name: tool.name.clone(),
                            description: tool.description.clone(),
                            parameters: tool.parameters.clone(),
                        },
                    })
                    .collect()
            })
    }

    fn user_content_value(message: &ChatMessage) -> serde_json::Value {
        let images = message.images.as_deref().unwrap_or_default();
        if images.is_empty() {
            return serde_json::Value::String(message.content.clone());
        }

        let mut parts = vec![serde_json::json!({
            "type": "text",
            "text": message.content,
        })];
        for url in images {
            parts.push(serde_json::json!({
                "type": "image_url",
                "image_url": { "url": url },
            }));
        }
        serde_json::Value::Array(parts)
    }

    fn convert_messages(messages: &[ChatMessage]) -> Vec<NativeMessage> {
        messages
            .iter()
            .filter_map(|m| {
                if m.role == "assistant" {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&m.content) {
                        if let Some(tool_calls_value) = value.get("tool_calls") {
                            if let Ok(parsed_calls) =
                                serde_json::from_value::<Vec<ProviderToolCall>>(
                                    tool_calls_value.clone(),
                                )
                            {
                                if !parsed_calls.is_empty() {
                                    let tool_calls = parsed_calls
                                        .into_iter()
                                        .map(|tc| NativeToolCall {
                                            id: Some(tc.id),
                                            kind: Some("function".to_string()),
                                            function: NativeFunctionCall {
                                                name: tc.name,
                                                arguments: tc.arguments,
                                            },
                                        })
                                        .collect::<Vec<_>>();
                                    let content = value
                                        .get("content")
                                        .and_then(serde_json::Value::as_str)
                                        .map(ToString::to_string);
                                    let reasoning_content = value
                                        .get("reasoning_content")
                                        .and_then(serde_json::Value::as_str)
                                        .map(ToString::to_string);
                                    return Some(NativeMessage {
                                        role: "assistant".to_string(),
                                        content: content.map(serde_json::Value::String),
                                        tool_call_id: None,
                                        tool_calls: Some(tool_calls),
                                        reasoning_content,
                                    });
                                }
                            }
                        }
                    }
                }

                if m.role == "tool" {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&m.content) {
                        let tool_call_id = value
                            .get("tool_call_id")
                            .and_then(serde_json::Value::as_str)
                            .filter(|id| !id.is_empty())
                            .map(ToString::to_string);
                        let content = value
                            .get("content")
                            .and_then(serde_json::Value::as_str)
                            .map(ToString::to_string);
                        if tool_call_id.is_some() {
                            return Some(NativeMessage {
                                role: "tool".to_string(),
                                content: content.map(serde_json::Value::String),
                                tool_call_id,
                                tool_calls: None,
                                reasoning_content: None,
                            });
                        }
                    }
                    return None;
                }

                let content = if m.role == "user" {
                    Some(Self::user_content_value(m))
                } else {
                    Some(serde_json::Value::String(m.content.clone()))
                };

                Some(NativeMessage {
                    role: m.role.clone(),
                    content,
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_content: None,
                })
            })
            .collect()
    }

    fn parse_native_response(
        message: NativeResponseMessage,
        finish_reason: Option<String>,
    ) -> ProviderChatResponse {
        let text = message.effective_content();
        let reasoning_content = message.reasoning_content.clone();
        let tool_calls = message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|tc| ProviderToolCall {
                id: tc.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                name: tc.function.name,
                arguments: tc.function.arguments,
            })
            .collect::<Vec<_>>();

        ProviderChatResponse {
            text,
            tool_calls,
            usage: None,
            reasoning_content,
            finish_reason,
        }
    }
}

pub struct MockProvider {
    name: String,
}

impl MockProvider {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn chat(&self, _request: ProviderChatRequest<'_>) -> anyhow::Result<ProviderChatResponse> {
        Ok(ProviderChatResponse {
            text: Some("Mock response from provider".to_string()),
            tool_calls: vec![],
            usage: None,
            reasoning_content: None,
            finish_reason: Some("stop".to_string()),
        })
    }

    async fn health_check(&self) -> bool {
        true
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    async fn chat(&self, request: ProviderChatRequest<'_>) -> anyhow::Result<ProviderChatResponse> {
        let started = Instant::now();
        match tokio::time::timeout(self.timeouts.request, self.chat_inner(request)).await {
            Ok(result) => result,
            Err(_) => {
                log_provider_error(
                    &self.model,
                    "request",
                    ProviderTimeoutKind::Request.as_str(),
                    Some(started.elapsed().as_millis() as u64),
                    Some(self.timeouts.request.as_millis() as u64),
                    None,
                );
                Err(anyhow::anyhow!(timeout_error_message(
                    ProviderTimeoutKind::Request,
                    self.timeouts.request.as_secs()
                )))
            }
        }
    }

    async fn chat_stream(
        &self,
        request: ProviderChatRequest<'_>,
        token_tx: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> anyhow::Result<ProviderChatResponse> {
        let started = Instant::now();
        let first_delta_ms = AtomicU64::new(0);
        let outcome = tokio::time::timeout(
            self.timeouts.request,
            self.chat_stream_inner(request, token_tx, &first_delta_ms, started),
        )
        .await;
        match outcome {
            Ok(result) => {
                if result.is_ok() {
                    log_provider_stream_ok(
                        &self.model,
                        first_delta_ms.load(Ordering::Relaxed),
                        started.elapsed().as_millis() as u64,
                    );
                }
                result
            }
            Err(_) => {
                log_provider_error(
                    &self.model,
                    "request",
                    ProviderTimeoutKind::Request.as_str(),
                    Some(started.elapsed().as_millis() as u64),
                    Some(self.timeouts.request.as_millis() as u64),
                    None,
                );
                Err(anyhow::anyhow!(timeout_error_message(
                    ProviderTimeoutKind::Request,
                    self.timeouts.request.as_secs()
                )))
            }
        }
    }

    async fn health_check(&self) -> bool {
        // 无论是否配置 API Key 都真实探测 `/models`；本地服务（Ollama 等）
        // 未启动时应如实返回不健康，而非因缺少 Key 而假报健康。
        self.authorized_get("/models")
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}

impl OpenAiProvider {
    async fn chat_inner(
        &self,
        request: ProviderChatRequest<'_>,
    ) -> anyhow::Result<ProviderChatResponse> {
        let tools = Self::convert_tools(request.tools);
        let native_request = NativeChatRequest {
            model: self.model.clone(),
            messages: Self::convert_messages(request.messages),
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            tool_choice: tools.as_ref().map(|_| "auto".to_string()),
            tools,
            stream: None,
        };

        let response = self
            .authorized_post("/chat/completions")
            .json(&native_request)
            .send()
            .await
            .map_err(|error| self.request_error("请求失败", &error))?;

        if !response.status().is_success() {
            return Err(api_error("OpenAI", &self.model, response).await);
        }

        let native_response: NativeChatResponse = response.json().await?;
        let usage = native_response.usage.map(|u| TokenUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
        });
        let choice = native_response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No response from OpenAI"))?;
        let mut result = Self::parse_native_response(choice.message, choice.finish_reason);
        result.usage = usage;
        Ok(result)
    }

    async fn chat_stream_inner(
        &self,
        request: ProviderChatRequest<'_>,
        token_tx: tokio::sync::mpsc::UnboundedSender<String>,
        first_delta_ms: &AtomicU64,
        started: Instant,
    ) -> anyhow::Result<ProviderChatResponse> {
        let tools = Self::convert_tools(request.tools);
        let native_request = NativeChatRequest {
            model: self.model.clone(),
            messages: Self::convert_messages(request.messages),
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            tool_choice: tools.as_ref().map(|_| "auto".to_string()),
            tools,
            stream: Some(true),
        };

        let response = self
            .authorized_post("/chat/completions")
            .json(&native_request)
            .send()
            .await
            .map_err(|error| self.request_error("流式请求失败", &error))?;

        if !response.status().is_success() {
            return Err(api_error("OpenAI", &self.model, response).await);
        }

        let mut stream = response.bytes_stream();
        let outcome = consume_sse_stream(
            &mut stream,
            self.timeouts.stream_idle,
            &token_tx,
            first_delta_ms,
            started,
        )
        .await;

        let ConsumeOutcome {
            text,
            reasoning,
            tool_accum,
            usage,
            finish_reason,
        } = match outcome {
            Ok(outcome) => outcome,
            Err(StreamConsumeError::Idle { secs }) => {
                log_provider_error(
                    &self.model,
                    "stream",
                    ProviderTimeoutKind::StreamIdle.as_str(),
                    Some(started.elapsed().as_millis() as u64),
                    Some(self.timeouts.stream_idle.as_millis() as u64),
                    None,
                );
                return Err(anyhow::anyhow!(timeout_error_message(
                    ProviderTimeoutKind::StreamIdle,
                    secs
                )));
            }
            Err(StreamConsumeError::Transport(error)) => {
                return Err(self.request_error("流式读取失败", &error));
            }
        };

        let tool_calls = tool_accum
            .into_iter()
            .filter(|(_, name, _)| !name.is_empty())
            .map(|(id, name, arguments)| ProviderToolCall {
                id: if id.is_empty() {
                    uuid::Uuid::new_v4().to_string()
                } else {
                    id
                },
                name,
                arguments,
            })
            .collect::<Vec<_>>();

        let final_text = if text.is_empty() && !reasoning.is_empty() {
            Some(reasoning.clone())
        } else if text.is_empty() {
            None
        } else {
            Some(text)
        };

        Ok(ProviderChatResponse {
            text: final_text,
            tool_calls,
            usage,
            reasoning_content: if reasoning.is_empty() {
                None
            } else {
                Some(reasoning)
            },
            finish_reason,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use futures_util::stream;

    #[test]
    fn chat_completions_url_is_normalized_without_duplicate_slashes() {
        for base_url in ["https://wetoken.pro/v1", "https://wetoken.pro/v1/"] {
            let provider = OpenAiProvider::new(
                Some(base_url),
                None,
                "deepseek-v4-flash",
                0.7,
                None,
                ProviderTimeouts::default(),
            );
            assert_eq!(
                provider.endpoint_url("/chat/completions"),
                "https://wetoken.pro/v1/chat/completions"
            );
        }
    }

    #[test]
    fn network_error_source_sanitizer_redacts_urls_and_credentials() {
        let sanitized = sanitize_network_error_fragment(
            "connect to https://user:password@example.test/v1 failed",
        );
        assert_eq!(sanitized, "connect to <url-redacted> failed");

        let sensitive = sanitize_network_error_fragment(
            "Authorization: Bearer never-log-this-secret",
        );
        assert_eq!(sensitive, "<redacted-sensitive-error-source>");
    }

    #[test]
    fn network_error_source_sanitizer_truncates_on_char_boundaries() {
        let input = "连接失败".repeat(NETWORK_ERROR_FRAGMENT_LIMIT);
        let sanitized = sanitize_network_error_fragment(&input);
        assert!(sanitized.ends_with("..."));
        assert_eq!(
            sanitized.trim_end_matches("...").chars().count(),
            NETWORK_ERROR_FRAGMENT_LIMIT
        );
    }

    // ---------------------------------------------------------------------
    // Timeout semantics (deterministic: paused clock, no network)
    // ---------------------------------------------------------------------

    /// Stand-in for a transport failure. `consume_sse_stream` is generic over
    /// the stream's error type, so tests never fabricate a `reqwest::Error`.
    #[derive(Debug)]
    struct TestStreamError;

    /// Virtual guard for tests whose stream should end on its own. It only
    /// fires if the code under test forgot to arm a timer, turning a would-be
    /// hang into a readable failure.
    const VIRTUAL_GUARD: Duration = Duration::from_secs(86_400);

    fn sse_line(payload: &str) -> Vec<u8> {
        format!("data: {payload}\n\n").into_bytes()
    }

    fn delta_chunk(content: &str) -> Vec<u8> {
        sse_line(&format!(
            r#"{{"id":"t1","object":"chat.completion.chunk","choices":[{{"index":0,"delta":{{"content":"{content}"}},"finish_reason":null}}]}}"#
        ))
    }

    fn drain_deltas(rx: &mut tokio::sync::mpsc::UnboundedReceiver<String>) -> Vec<String> {
        let mut deltas = Vec::new();
        while let Ok(delta) = rx.try_recv() {
            deltas.push(delta);
        }
        deltas
    }

    /// Emits `chunks` with `gap` of virtual time before each one, then stays
    /// silent forever so the idle timer decides how the stream ends.
    fn paced_stream(
        chunks: Vec<Vec<u8>>,
        gap: Duration,
    ) -> impl futures_util::Stream<Item = Result<Vec<u8>, TestStreamError>> + Unpin {
        Box::pin(
            stream::iter(chunks)
                .then(move |chunk| async move {
                    tokio::time::sleep(gap).await;
                    Ok(chunk)
                })
                .chain(stream::pending()),
        )
    }

    #[tokio::test(start_paused = true)]
    async fn stream_idle_timeout_fires_when_no_data_arrives() {
        // A stream that never yields: the idle timer must trip and surface the
        // distinct idle-timeout error instead of waiting forever.
        let mut silent = stream::pending::<Result<Vec<u8>, TestStreamError>>();
        let (token_tx, _token_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let first_delta_ms = AtomicU64::new(0);
        let outcome = tokio::time::timeout(
            VIRTUAL_GUARD,
            consume_sse_stream(
                &mut silent,
                Duration::from_secs(300),
                &token_tx,
                &first_delta_ms,
                Instant::now(),
            ),
        )
        .await
        .expect("idle timeout must fire before the guard");
        assert!(matches!(
            outcome,
            Err(StreamConsumeError::Idle { secs: 300 })
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn stream_idle_timer_resets_on_activity() {
        // Chunks arrive 100s apart under a 300s idle window. Total elapsed
        // time (400s) exceeds the window, so this only passes if every chunk
        // resets the timer.
        let mut slow = paced_stream(
            vec![
                delta_chunk("a"),
                delta_chunk("b"),
                delta_chunk("c"),
                sse_line("[DONE]"),
            ],
            Duration::from_secs(100),
        );
        let (token_tx, mut token_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let first_delta_ms = AtomicU64::new(0);
        let outcome = tokio::time::timeout(
            VIRTUAL_GUARD,
            consume_sse_stream(
                &mut slow,
                Duration::from_secs(300),
                &token_tx,
                &first_delta_ms,
                Instant::now(),
            ),
        )
        .await
        .expect("stream must complete before the guard")
        .expect("no idle timeout with regular activity");
        assert_eq!(outcome.text, "abc");
        assert_eq!(drain_deltas(&mut token_rx), vec!["a", "b", "c"]);
    }

    #[tokio::test(start_paused = true)]
    async fn slow_first_token_within_the_idle_window_does_not_time_out() {
        // 120s time-to-first-token: past the old 60s idle default, well inside
        // the 300s one. Slow-TTFT models must not be killed.
        let mut slow = paced_stream(
            vec![delta_chunk("late"), sse_line("[DONE]")],
            Duration::from_secs(120),
        );
        let (token_tx, mut token_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let first_delta_ms = AtomicU64::new(0);
        let outcome = tokio::time::timeout(
            VIRTUAL_GUARD,
            consume_sse_stream(
                &mut slow,
                Duration::from_secs(300),
                &token_tx,
                &first_delta_ms,
                Instant::now(),
            ),
        )
        .await
        .expect("stream must complete before the guard")
        .expect("120s TTFT is inside the 300s idle window");
        assert_eq!(outcome.text, "late");
        assert_eq!(drain_deltas(&mut token_rx), vec!["late"]);
    }

    #[tokio::test(start_paused = true)]
    async fn idle_timeout_after_text_preserves_received_answer() {
        // Services that omit [DONE] and keep the connection open must not
        // lose the answer that already arrived: idle after text -> success.
        let mut stream = Box::pin(
            stream::iter(vec![Ok(delta_chunk("完成"))])
                .chain(stream::pending::<Result<Vec<u8>, TestStreamError>>()),
        );
        let (token_tx, _token_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let first_delta_ms = AtomicU64::new(0);
        let outcome = tokio::time::timeout(
            VIRTUAL_GUARD,
            consume_sse_stream(
                &mut stream,
                Duration::from_secs(300),
                &token_tx,
                &first_delta_ms,
                Instant::now(),
            ),
        )
        .await
        .expect("grace path must return before the guard")
        .expect("grace path preserves the answer");
        assert_eq!(outcome.text, "完成");
    }

    #[tokio::test(start_paused = true)]
    async fn request_timeout_is_independent_of_stream_activity() {
        // An endless stream with steady activity never trips the idle timer,
        // so only the request wall-clock cap can end the call. The outer
        // timeout here mirrors the wrapper in `chat_stream`.
        let mut steady = Box::pin(stream::iter(0u64..).then(|index| async move {
            tokio::time::sleep(Duration::from_secs(30)).await;
            Ok::<_, TestStreamError>(delta_chunk(&index.to_string()))
        }));
        let (token_tx, _token_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let first_delta_ms = AtomicU64::new(0);
        let outcome = tokio::time::timeout(
            Duration::from_secs(900),
            consume_sse_stream(
                &mut steady,
                Duration::from_secs(300),
                &token_tx,
                &first_delta_ms,
                Instant::now(),
            ),
        )
        .await;
        assert!(
            outcome.is_err(),
            "request timeout must terminate an endless but active stream"
        );
    }

    // ---------------------------------------------------------------------
    // Trait-object dispatch guard (loopback socket, no external network)
    // ---------------------------------------------------------------------

    /// Serves one SSE response on loopback and returns its base URL.
    async fn serve_one_sse_response(chunks: Vec<Vec<u8>>) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback listener");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            // Read just the request head so the client's write side completes.
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while socket.read(&mut byte).await.unwrap_or(0) == 1 {
                head.push(byte[0]);
                if head.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            let _ = socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
                )
                .await;
            for chunk in chunks {
                let _ = socket.write_all(&chunk).await;
                let _ = socket.flush().await;
            }
            let _ = socket.shutdown().await;
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn dyn_provider_streaming_uses_the_sse_override() {
        // Regression guard: `chat_stream` must stay inside
        // `impl Provider for OpenAiProvider`. If it slips back into an
        // inherent impl, trait-object dispatch silently falls back to the
        // default `Provider::chat_stream`, which replays the answer as one
        // combined delta instead of streaming.
        let base_url = serve_one_sse_response(vec![
            delta_chunk("He"),
            delta_chunk("llo"),
            sse_line("[DONE]"),
        ])
        .await;
        let provider: Box<dyn Provider> = Box::new(OpenAiProvider::new(
            Some(&base_url),
            None,
            "stream-guard-model",
            0.0,
            None,
            ProviderTimeouts::default(),
        ));

        let messages = vec![ChatMessage::user("hi")];
        let request = ProviderChatRequest {
            messages: &messages,
            tools: None,
        };
        let (token_tx, mut token_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let response = provider
            .chat_stream(request, token_tx)
            .await
            .expect("streaming through &dyn Provider must succeed");

        assert_eq!(drain_deltas(&mut token_rx), vec!["He", "llo"]);
        assert_eq!(response.text.as_deref(), Some("Hello"));
    }

    /// Forwards `chat` only, so it inherits the default `Provider::chat_stream`.
    struct DefaultStreamWrapper(OpenAiProvider);

    #[async_trait]
    impl Provider for DefaultStreamWrapper {
        fn name(&self) -> &str {
            "default-stream-wrapper"
        }

        async fn chat(
            &self,
            request: ProviderChatRequest<'_>,
        ) -> anyhow::Result<ProviderChatResponse> {
            self.0.chat(request).await
        }

        async fn health_check(&self) -> bool {
            self.0.health_check().await
        }
    }

    #[tokio::test]
    async fn trait_default_chat_stream_cannot_read_an_sse_body() {
        // Counterpart to the guard above: the default implementation issues a
        // non-streaming request and cannot parse an SSE body, so the guard
        // really does fail if `chat_stream` ever falls back to it.
        let base_url = serve_one_sse_response(vec![
            delta_chunk("He"),
            delta_chunk("llo"),
            sse_line("[DONE]"),
        ])
        .await;
        let provider: Box<dyn Provider> = Box::new(DefaultStreamWrapper(OpenAiProvider::new(
            Some(&base_url),
            None,
            "stream-guard-model",
            0.0,
            None,
            ProviderTimeouts::default(),
        )));

        let messages = vec![ChatMessage::user("hi")];
        let request = ProviderChatRequest {
            messages: &messages,
            tools: None,
        };
        let (token_tx, mut token_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let result = provider.chat_stream(request, token_tx).await;
        assert!(
            result.is_err(),
            "the default chat_stream must not be able to consume an SSE body"
        );
        assert!(drain_deltas(&mut token_rx).is_empty());
    }

    #[test]
    fn provider_timeouts_default_to_300s_idle_and_900s_request() {
        let defaults = ProviderTimeouts::default();
        assert_eq!(defaults.connect, Duration::from_secs(30));
        assert_eq!(defaults.stream_idle, Duration::from_secs(300));
        assert_eq!(defaults.request, Duration::from_secs(900));
    }

    #[test]
    fn timeout_error_messages_are_distinct_per_kind() {
        let connect = timeout_error_message(ProviderTimeoutKind::Connect, 30);
        let idle = timeout_error_message(ProviderTimeoutKind::StreamIdle, 300);
        let request = timeout_error_message(ProviderTimeoutKind::Request, 900);
        assert!(connect.contains("connect timeout"));
        assert!(idle.contains("stream idle timeout"));
        assert!(request.contains("request timeout"));
        assert_ne!(connect, idle);
        assert_ne!(idle, request);
        assert_ne!(request, connect);
    }
}
