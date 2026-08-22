use crate::config::TransportMode;
use crate::providers::traits::{
    ChatMessage, ChatRequest as ProviderChatRequest, ChatResponse as ProviderChatResponse, Provider,
    TokenUsage, ToolCall as ProviderToolCall,
};
use crate::tools::ToolSpec;
use async_trait::async_trait;
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
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

/// Application-level stream completion state used to decide whether a
/// transport failure may be replayed safely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResponseCompletionState {
    NoOutput,
    Partial,
    Complete,
}

impl ResponseCompletionState {
    fn as_str(self) -> &'static str {
        match self {
            Self::NoOutput => "no_output",
            Self::Partial => "partial",
            Self::Complete => "complete",
        }
    }
}

/// Whether a retry should use the persistent provider client or a brand-new
/// client (and therefore a fresh connection pool).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportProfile {
    Persistent,
    Fresh,
}

impl TransportProfile {
    fn as_str(self) -> &'static str {
        match self {
            Self::Persistent => "persistent",
            Self::Fresh => "fresh",
        }
    }
}

/// Total provider attempts for one logical request: the original call plus a
/// single automatic replay. Never unbounded.
const MAX_PROVIDER_ATTEMPTS: u32 = 2;

/// Bounded pause before the single replay.
const PROVIDER_RETRY_BACKOFF: Duration = Duration::from_millis(1500);

/// Surfaced when every attempt died from a transient transport fault. This is
/// the one outcome the user can act on, so it replaces the low-level wording.
const TRANSIENT_DISCONNECT_MESSAGE: &str = "模型渠道在生成过程中意外断开连接，请重试或切换模型。";

/// OS-level error kinds that mean the connection died in flight. Preferred
/// over text matching because it is immune to platform and locale differences.
fn is_transient_io_kind(kind: std::io::ErrorKind) -> bool {
    matches!(
        kind,
        std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::UnexpectedEof
            | std::io::ErrorKind::BrokenPipe
    )
}

/// Transport signatures that unambiguously mean "the connection died in
/// flight" but carry no OS error to inspect, such as a TLS peer close or
/// hyper's incomplete-message error. Deliberately narrow: anything not listed
/// here, and not covered by [`is_transient_io_kind`], is non-retryable.
const TRANSIENT_TRANSPORT_SIGNATURES: &[&str] = &[
    "peer closed connection",
    "close_notify",
    "unexpected eof",
    "connection reset",
    // hyper's wording when the peer hangs up before the response completes.
    "connection closed before message completed",
];

/// Gateway-level HTTP failures that are safe to replay.
///
/// 500 is excluded because it is usually a genuine upstream application error,
/// and 429 is excluded because honouring `Retry-After` is out of scope for V1;
/// replaying it on a fixed backoff would only add rate-limit pressure.
fn transient_http_reason(status: u16) -> Option<&'static str> {
    match status {
        502 => Some("http_502"),
        503 => Some("http_503"),
        504 => Some("http_504"),
        _ => None,
    }
}

/// Classifies a reqwest failure as a transient mid-flight disconnect.
///
/// All of the following must hold, otherwise the failure is non-retryable:
/// - not a timeout: connect, stream-idle and request-total timeouts have their
///   own handling and must never be replayed;
/// - not a connect error: a refused or unreachable endpoint is not transient
///   for our purposes;
/// - reqwest attributes it to the request or body side;
/// - the failure is a known disconnect, identified structurally through the
///   OS error kind where possible and only otherwise by a narrow text match.
///
/// Note that not every `io::Error` qualifies: only the kinds listed in
/// [`is_transient_io_kind`] do.
fn transient_transport_reason(diagnostics: &NetworkErrorDiagnostics) -> Option<&'static str> {
    if diagnostics.is_timeout || diagnostics.is_connect {
        return None;
    }
    if !diagnostics.is_request && !diagnostics.is_body {
        return None;
    }
    if diagnostics.io_error_kind.is_some_and(is_transient_io_kind) {
        return Some("transport_disconnect");
    }
    // Fallback for disconnects that carry no OS error, such as a TLS peer
    // close. Only the category is reported, never the matched fragment.
    let matched = diagnostics.source_chain.iter().any(|fragment| {
        let lower = fragment.to_ascii_lowercase();
        TRANSIENT_TRANSPORT_SIGNATURES
            .iter()
            .any(|signature| lower.contains(signature))
    });
    matched.then_some("transport_disconnect")
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
    attempts: u32,
    elapsed_ms: Option<u64>,
    timeout_ms: Option<u64>,
    http_status: Option<u16>,
) {
    tracing::error!(
        target: "omninova_core::providers::timeout",
        model = %model,
        stage = %stage,
        error_kind = %error_kind,
        attempts = %attempts,
        elapsed_ms = %optional_field(elapsed_ms),
        timeout_ms = %optional_field(timeout_ms),
        http_status = %optional_field(http_status),
        "[provider-error]"
    );
}

/// Replaces the low-level wording once every attempt has failed on a transient
/// transport fault, which is the only case the user can act on.
fn final_provider_error(failure: AttemptError, attempts: u32) -> anyhow::Error {
    if failure.retry_reason.is_some() {
        return anyhow::anyhow!(
            "{TRANSIENT_DISCONNECT_MESSAGE}（共尝试 {attempts} 次）；详情见 provider-network-error 日志"
        );
    }
    failure.error
}

/// Safe per-attempt transport telemetry: category-level metadata only.
fn log_provider_transport(model: &str, transport_mode: &str, attempt: u32, connection: &str) {
    tracing::debug!(
        target: "omninova_core::providers::timeout",
        model = %model,
        transport_mode = %transport_mode,
        attempt = %attempt,
        connection = %connection,
        "[provider-transport]"
    );
}

/// Safe retry telemetry: category-level metadata only.
fn log_provider_retry(
    model: &str,
    attempt: u32,
    reason: &str,
    backoff_ms: u64,
    transport_mode: &str,
    connection: &str,
) {
    tracing::warn!(
        target: "omninova_core::providers::timeout",
        model = %model,
        attempt = %attempt,
        max_attempts = %MAX_PROVIDER_ATTEMPTS,
        reason = %reason,
        backoff_ms = %backoff_ms,
        transport_mode = %transport_mode,
        connection = %connection,
        // Retrying is only ever reached when nothing was emitted yet.
        completion_state = "no_output",
        "[provider-retry]"
    );
}

/// Safe structured success metrics for streaming responses (debug level).
fn log_provider_stream_ok(
    model: &str,
    first_delta_ms: u64,
    total_elapsed_ms: u64,
    completion_state: &str,
    finish_reason: Option<&str>,
) {
    tracing::debug!(
        target: "omninova_core::providers::timeout",
        model = %model,
        first_delta_ms = %first_delta_ms,
        total_elapsed_ms = %total_elapsed_ms,
        completion_state = %completion_state,
        finish_reason = %finish_reason.unwrap_or("-"),
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
    /// First OS-level error kind found in the source chain. Structural, so
    /// disconnect detection does not depend on platform or locale: Windows
    /// renders ECONNRESET as a localized message.
    io_error_kind: Option<std::io::ErrorKind>,
    source_chain: Vec<String>,
}

impl NetworkErrorDiagnostics {
    fn from_reqwest(base_url: &str, error: &reqwest::Error) -> Self {
        let host = reqwest::Url::parse(base_url)
            .ok()
            .and_then(|url| url.host_str().map(ToString::to_string))
            .unwrap_or_else(|| "unknown".to_string());
        let mut source_chain = Vec::new();
        let mut io_error_kind = None;
        let mut source = std::error::Error::source(error);
        while let Some(current) = source {
            source_chain.push(sanitize_network_error_fragment(&current.to_string()));
            if io_error_kind.is_none() {
                io_error_kind = current
                    .downcast_ref::<std::io::Error>()
                    .map(std::io::Error::kind);
            }
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
            io_error_kind,
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
    retry_backoff: Duration,
    transport_mode: TransportMode,
    client: Client,
}

struct ConsumeOutcome {
    text: String,
    reasoning: String,
    tool_accum: Vec<(String, String, String)>,
    usage: Option<TokenUsage>,
    finish_reason: Option<String>,
    completion_state: ResponseCompletionState,
}

#[derive(Debug)]
enum StreamConsumeError<E> {
    Idle {
        secs: u64,
    },
    /// The stream ended without a protocol-complete response (for example a
    /// clean EOF before `[DONE]`/finish_reason, or an incomplete tool call).
    Incomplete {
        completion_state: ResponseCompletionState,
    },
    /// The stream's own error, passed through unchanged so the caller keeps
    /// full `reqwest::Error` classification. `completion_state` records whether
    /// anything had already been accumulated or forwarded, which decides
    /// whether replaying the request is safe.
    Transport {
        error: E,
        completion_state: ResponseCompletionState,
    },
}

/// One failed provider attempt: the user-facing error plus the metadata needed
/// to decide whether replaying the request is safe.
struct AttemptError {
    error: anyhow::Error,
    stage: &'static str,
    error_kind: &'static str,
    http_status: Option<u16>,
    /// The budget that elapsed, for timeout failures only.
    timeout_ms: Option<u64>,
    /// `Some(category)` only when the failure is transient *and* produced no
    /// user-visible output, so a single replay cannot duplicate text or tools.
    retry_reason: Option<&'static str>,
}

impl AttemptError {
    fn permanent(error: anyhow::Error, stage: &'static str, error_kind: &'static str) -> Self {
        Self {
            error,
            stage,
            error_kind,
            http_status: None,
            timeout_ms: None,
            retry_reason: None,
        }
    }

    fn with_status(mut self, status: u16) -> Self {
        self.http_status = Some(status);
        self
    }

    fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    fn retryable(mut self, reason: Option<&'static str>) -> Self {
        self.retry_reason = reason;
        self
    }

    /// Drops the retry permission unless the failed attempt had no output at
    /// all. Complete and partial responses are never replayed.
    fn block_retry_unless_no_output(mut self, state: ResponseCompletionState) -> Self {
        if state != ResponseCompletionState::NoOutput {
            self.retry_reason = None;
        }
        self
    }
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
            Err(_) => {
                let state = completion_state(
                    &text,
                    &reasoning,
                    &tool_accum,
                    finish_reason.as_deref(),
                    done,
                );
                if state == ResponseCompletionState::Complete {
                    break;
                }
                return Err(StreamConsumeError::Idle {
                    secs: idle_timeout.as_secs(),
                });
            }
        };
        let Some(item) = next else { break };
        let bytes = match item {
            Ok(bytes) => bytes,
            Err(error) => {
                let state = completion_state(
                    &text,
                    &reasoning,
                    &tool_accum,
                    finish_reason.as_deref(),
                    done,
                );
                // A trailing transport error after a strictly complete
                // response is accepted; it must not turn the run into a
                // failure.
                if state == ResponseCompletionState::Complete {
                    break;
                }
                return Err(StreamConsumeError::Transport {
                    error,
                    completion_state: state,
                });
            }
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

    let state = completion_state(
        &text,
        &reasoning,
        &tool_accum,
        finish_reason.as_deref(),
        done,
    );
    if state == ResponseCompletionState::Complete {
        Ok(ConsumeOutcome {
            text,
            reasoning,
            tool_accum,
            usage,
            finish_reason,
            completion_state: state,
        })
    } else {
        Err(StreamConsumeError::Incomplete {
            completion_state: state,
        })
    }
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

fn has_any_output(text: &str, reasoning: &str, tool_accum: &[(String, String, String)]) -> bool {
    !text.is_empty() || !reasoning.is_empty() || has_started_tool_call(tool_accum)
}

fn tool_call_is_complete(slot: &(String, String, String)) -> bool {
    let (id, name, arguments) = slot;
    !id.is_empty()
        && !name.is_empty()
        && serde_json::from_str::<serde_json::Value>(arguments).is_ok()
}

fn tool_calls_are_complete(tool_accum: &[(String, String, String)]) -> bool {
    tool_accum.iter().all(|slot| {
        let started = !slot.0.is_empty() || !slot.1.is_empty() || !slot.2.is_empty();
        if !started {
            return true;
        }
        tool_call_is_complete(slot)
    })
}

fn completion_state(
    text: &str,
    reasoning: &str,
    tool_accum: &[(String, String, String)],
    finish_reason: Option<&str>,
    done: bool,
) -> ResponseCompletionState {
    let protocol_complete = done || finish_reason.is_some();
    let tools_complete = !has_started_tool_call(tool_accum) || tool_calls_are_complete(tool_accum);
    if protocol_complete && tools_complete {
        ResponseCompletionState::Complete
    } else if has_any_output(text, reasoning, tool_accum) {
        ResponseCompletionState::Partial
    } else {
        ResponseCompletionState::NoOutput
    }
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

fn semantic_provider_error_code(body: &str) -> Option<&'static str> {
    const NON_RETRYABLE_CODES: &[&str] = &[
        "model_not_found",
        "context_length_exceeded",
        "content_filter",
        "invalid_request_error",
        "malformed_request",
    ];
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let error = value.get("error")?;
    let code = error
        .get("code")
        .and_then(serde_json::Value::as_str)
        .or_else(|| error.get("type").and_then(serde_json::Value::as_str))
        .or_else(|| error.get("message").and_then(serde_json::Value::as_str))?;
    NON_RETRYABLE_CODES
        .iter()
        .find(|known| code.to_ascii_lowercase().contains(*known))
        .copied()
}

async fn api_error(provider_name: &str, response: Response) -> AttemptError {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let retry_reason = transient_http_reason(status.as_u16())
        .filter(|_| semantic_provider_error_code(&text).is_none());
    AttemptError::permanent(
        anyhow::anyhow!("{provider_name} API error ({status}): {text}"),
        "request",
        "http_error",
    )
    .with_status(status.as_u16())
    .retryable(retry_reason)
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
        let transport_mode = TransportMode::default();
        let client = Self::build_client(timeouts.connect, transport_mode);

        Self {
            base_url,
            credential: credential.map(ToString::to_string),
            model: model.into(),
            temperature,
            max_tokens: max_tokens.filter(|v| *v > 0),
            timeouts,
            retry_backoff: PROVIDER_RETRY_BACKOFF,
            transport_mode,
            client,
        }
    }

    /// Sets the per-provider HTTP transport mode. Default is `Auto`; the
    /// client is rebuilt so the chosen mode takes effect from the next request.
    pub fn with_transport_mode(mut self, mode: TransportMode) -> Self {
        self.transport_mode = mode;
        self.client = Self::build_client(self.timeouts.connect, mode);
        self
    }

    fn build_client(connect_timeout: Duration, mode: TransportMode) -> Client {
        let mut builder = Client::builder()
            // A broken proxy or unreachable provider should fail quickly instead
            // of leaving the desktop UI in an endless "running" state.
            .connect_timeout(connect_timeout);
        match mode {
            TransportMode::Auto => {}
            TransportMode::Http1 => {
                builder = builder.http1_only();
            }
            TransportMode::Http2 => {
                builder = builder.http2_prior_knowledge();
            }
        }
        builder
            .build()
            .expect("failed to build reqwest client for configured transport mode")
    }

    /// Test seam that keeps the retry tests fast without exposing the backoff
    /// as user-facing configuration.
    #[cfg(test)]
    fn with_retry_backoff(mut self, backoff: Duration) -> Self {
        self.retry_backoff = backoff;
        self
    }

    /// 构建 POST 请求。本地推理服务（如 Ollama / LM Studio）通常无需 API Key，
    /// 因此仅在存在 credential 时附加 `Authorization` 头，否则不带鉴权直接请求。
    fn authorized_post(&self, client: &Client, path: &str) -> reqwest::RequestBuilder {
        let req = client.post(self.endpoint_url(path));
        match self.credential.as_ref() {
            Some(credential) => req.header("Authorization", format!("Bearer {credential}")),
            None => req,
        }
    }

    /// 构建 GET 请求，鉴权头处理同 [`authorized_post`]。
    fn authorized_get(&self, client: &Client, path: &str) -> reqwest::RequestBuilder {
        let req = client.get(self.endpoint_url(path));
        match self.credential.as_ref() {
            Some(credential) => req.header("Authorization", format!("Bearer {credential}")),
            None => req,
        }
    }

    fn endpoint_url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn request_error(&self, operation: &str, error: &reqwest::Error) -> AttemptError {
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
            io_error_kind = %optional_field(diagnostics.io_error_kind.map(|kind| format!("{kind:?}"))),
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
        if connect_timeout {
            // A connect timeout is a timeout, never a transient disconnect.
            return AttemptError::permanent(
                anyhow::anyhow!(timeout_error_message(
                    ProviderTimeoutKind::Connect,
                    self.timeouts.connect.as_secs()
                )),
                "connect",
                error_kind,
            )
            .with_timeout_ms(self.timeouts.connect.as_millis() as u64);
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
        AttemptError::permanent(
            anyhow::anyhow!(
                "{operation}：{category}（provider=openai-compatible, host={}）；详情见 provider-network-error 日志",
                diagnostics.host
            ),
            "request",
            error_kind,
        )
        .retryable(transient_transport_reason(&diagnostics))
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
        let attempts = AtomicU32::new(0);
        let driver = self.run_with_transient_retry(started, &attempts, |client| {
            self.chat_inner(request, client)
        });
        match tokio::time::timeout(self.timeouts.request, driver).await {
            Ok(result) => result,
            Err(_) => Err(self.request_timeout_error(started, &attempts)),
        }
    }

    async fn chat_stream(
        &self,
        request: ProviderChatRequest<'_>,
        token_tx: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> anyhow::Result<ProviderChatResponse> {
        let started = Instant::now();
        let first_delta_ms = AtomicU64::new(0);
        let attempts = AtomicU32::new(0);
        let driver = self.run_with_transient_retry(started, &attempts, |client| {
            self.chat_stream_inner(request, token_tx.clone(), &first_delta_ms, started, client)
        });
        match tokio::time::timeout(self.timeouts.request, driver).await {
            Ok(result) => {
                if let Ok(response) = &result {
                    log_provider_stream_ok(
                        &self.model,
                        first_delta_ms.load(Ordering::Relaxed),
                        started.elapsed().as_millis() as u64,
                        "complete",
                        response.finish_reason.as_deref(),
                    );
                }
                result
            }
            Err(_) => Err(self.request_timeout_error(started, &attempts)),
        }
    }

    async fn health_check(&self) -> bool {
        // 无论是否配置 API Key 都真实探测 `/models`；本地服务（Ollama 等）
        // 未启动时应如实返回不健康，而非因缺少 Key 而假报健康。
        self.authorized_get(&self.client, "/models")
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}

impl OpenAiProvider {
    /// Runs one provider operation with at most one automatic replay.
    ///
    /// A replay happens only when the failed attempt was both clearly
    /// transient and known to have produced no user-visible output, so it can
    /// never duplicate text or tool actions. The caller wraps this in the
    /// request-total timeout, so both attempts plus the backoff share that one
    /// deadline and retrying cannot extend it. Cancellation drops this future
    /// outright, so a cancelled run is never replayed.
    async fn run_with_transient_retry<F, Fut>(
        &self,
        started: Instant,
        attempts: &AtomicU32,
        mut attempt: F,
    ) -> anyhow::Result<ProviderChatResponse>
    where
        F: FnMut(Client) -> Fut,
        Fut: std::future::Future<Output = Result<ProviderChatResponse, AttemptError>>,
    {
        loop {
            let attempt_number = attempts.fetch_add(1, Ordering::Relaxed) + 1;
            let profile = if attempt_number == 1 {
                TransportProfile::Persistent
            } else {
                TransportProfile::Fresh
            };
            // The first attempt uses the provider's normal persistent client.
            // Every retry builds a brand-new reqwest client, so it cannot reuse
            // the failed connection-pool entry.
            let client = if attempt_number == 1 {
                self.client.clone()
            } else {
                Self::build_client(self.timeouts.connect, self.transport_mode)
            };
            log_provider_transport(
                &self.model,
                self.transport_mode.as_str(),
                attempt_number,
                profile.as_str(),
            );
            let failure = match attempt(client).await {
                Ok(response) => return Ok(response),
                Err(failure) => failure,
            };
            match failure.retry_reason {
                Some(reason) if attempt_number < MAX_PROVIDER_ATTEMPTS => {
                    log_provider_retry(
                        &self.model,
                        attempt_number,
                        reason,
                        self.retry_backoff.as_millis() as u64,
                        self.transport_mode.as_str(),
                        TransportProfile::Fresh.as_str(),
                    );
                    tokio::time::sleep(self.retry_backoff).await;
                }
                _ => {
                    log_provider_error(
                        &self.model,
                        failure.stage,
                        failure.error_kind,
                        attempt_number,
                        Some(started.elapsed().as_millis() as u64),
                        failure.timeout_ms,
                        failure.http_status,
                    );
                    return Err(final_provider_error(failure, attempt_number));
                }
            }
        }
    }

    fn request_timeout_error(&self, started: Instant, attempts: &AtomicU32) -> anyhow::Error {
        log_provider_error(
            &self.model,
            "request",
            ProviderTimeoutKind::Request.as_str(),
            attempts.load(Ordering::Relaxed).max(1),
            Some(started.elapsed().as_millis() as u64),
            Some(self.timeouts.request.as_millis() as u64),
            None,
        );
        anyhow::anyhow!(timeout_error_message(
            ProviderTimeoutKind::Request,
            self.timeouts.request.as_secs()
        ))
    }

    async fn chat_inner(
        &self,
        request: ProviderChatRequest<'_>,
        client: Client,
    ) -> Result<ProviderChatResponse, AttemptError> {
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
            .authorized_post(&client, "/chat/completions")
            .json(&native_request)
            .send()
            .await
            .map_err(|error| self.request_error("请求失败", &error))?;

        if !response.status().is_success() {
            return Err(api_error("OpenAI", response).await);
        }

        // Routed through `request_error` so a body cut mid-response is
        // classified (and sanitized) like any other transport failure.
        let native_response: NativeChatResponse = response
            .json()
            .await
            .map_err(|error| self.request_error("响应解析失败", &error))?;
        let usage = native_response.usage.map(|u| TokenUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
        });
        let choice = native_response.choices.into_iter().next().ok_or_else(|| {
            AttemptError::permanent(
                anyhow::anyhow!("No response from OpenAI"),
                "request",
                "empty_response",
            )
        })?;
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
        client: Client,
    ) -> Result<ProviderChatResponse, AttemptError> {
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
            .authorized_post(&client, "/chat/completions")
            .json(&native_request)
            .send()
            .await
            .map_err(|error| self.request_error("流式请求失败", &error))?;

        if !response.status().is_success() {
            return Err(api_error("OpenAI", response).await);
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
            ..
        } = match outcome {
            Ok(outcome) => outcome,
            // A stream idle timeout is a timeout, never a transient
            // disconnect, so it is never replayed.
            Err(StreamConsumeError::Idle { secs }) => {
                return Err(AttemptError::permanent(
                    anyhow::anyhow!(timeout_error_message(ProviderTimeoutKind::StreamIdle, secs)),
                    "stream",
                    ProviderTimeoutKind::StreamIdle.as_str(),
                )
                .with_timeout_ms(self.timeouts.stream_idle.as_millis() as u64));
            }
            Err(StreamConsumeError::Incomplete { completion_state }) => {
                return Err(AttemptError::permanent(
                    anyhow::anyhow!(
                        "模型响应不完整（completion_state={}），已保留已生成的部分输出",
                        completion_state.as_str()
                    ),
                    "stream",
                    "incomplete_response",
                )
                .block_retry_unless_no_output(completion_state));
            }
            Err(StreamConsumeError::Transport {
                error,
                completion_state,
            }) => {
                // Once any model/tool output has started, a transport failure
                // is an explicit incomplete response: never replay it and do
                // not hide the partial-output state behind a generic network
                // error.
                if completion_state == ResponseCompletionState::Partial {
                    return Err(AttemptError::permanent(
                        anyhow::anyhow!(
                            "模型响应不完整（completion_state={}），已保留已生成的部分输出",
                            completion_state.as_str()
                        ),
                        "stream",
                        "incomplete_response",
                    )
                    .block_retry_unless_no_output(completion_state));
                }
                return Err(self
                    .request_error("流式读取失败", &error)
                    .block_retry_unless_no_output(completion_state));
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
    use std::sync::Arc;

    #[test]
    fn chat_completions_url_is_normalized_without_duplicate_slashes() {
        for base_url in ["https://api.example.com/v1", "https://api.example.com/v1/"] {
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
                "https://api.example.com/v1/chat/completions"
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
    async fn idle_timeout_after_finish_reason_preserves_received_answer() {
        // Services that signal finish_reason but omit [DONE] and keep the
        // connection open must not lose the answer already arrived: idle after
        // a strict completion signal -> success.
        let complete_chunk = sse_line(
            r#"{"id":"t1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"完成"},"finish_reason":"stop"}]}"#,
        );
        let mut stream = Box::pin(
            stream::iter(vec![Ok(complete_chunk)])
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
        assert_eq!(outcome.completion_state, ResponseCompletionState::Complete);
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
        use tokio::io::AsyncWriteExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback listener");
        let addr = listener.local_addr().expect("listener address");
        tokio::spawn(async move {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            if read_request(&mut socket).await.is_none() {
                return;
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

    // ---------------------------------------------------------------------
    // Transient retry: classification (deterministic, no network)
    // ---------------------------------------------------------------------

    /// Builds diagnostics without needing a real `reqwest::Error`.
    fn diagnostics(
        flags: (bool, bool, bool, bool),
        source_chain: &[&str],
    ) -> NetworkErrorDiagnostics {
        let (is_connect, is_timeout, is_request, is_body) = flags;
        NetworkErrorDiagnostics {
            host: "provider.test".to_string(),
            is_connect,
            is_timeout,
            is_request,
            is_body,
            is_decode: false,
            io_error_kind: None,
            source_chain: source_chain.iter().map(ToString::to_string).collect(),
        }
    }

    /// Regression fixture for a production-style disconnect response.
    fn production_disconnect() -> NetworkErrorDiagnostics {
        diagnostics(
            (false, false, true, false),
            &[
                "client error (SendRequest)",
                "connection error",
                "peer closed connection without sending TLS close_notify",
            ],
        )
    }

    #[test]
    fn tls_peer_close_is_classified_as_transient() {
        // A TLS peer close carries no OS error, so only the text fallback can
        // recognize it.
        let diagnostics = production_disconnect();
        assert_eq!(diagnostics.io_error_kind, None);
        assert_eq!(
            transient_transport_reason(&diagnostics),
            Some("transport_disconnect")
        );
    }

    #[test]
    fn os_level_disconnects_are_classified_structurally() {
        // Windows renders ECONNRESET with a localized message, so text
        // matching cannot see it; the error kind must decide.
        for kind in [
            std::io::ErrorKind::ConnectionReset,
            std::io::ErrorKind::ConnectionAborted,
            std::io::ErrorKind::UnexpectedEof,
            std::io::ErrorKind::BrokenPipe,
        ] {
            let mut localized = diagnostics(
                (false, false, true, false),
                &[
                    "client error (SendRequest)",
                    "远程主机强迫关闭了一个现有的连接。",
                ],
            );
            localized.io_error_kind = Some(kind);
            assert_eq!(
                transient_transport_reason(&localized),
                Some("transport_disconnect"),
                "{kind:?} must be recognized without text matching"
            );
        }
    }

    #[test]
    fn unrelated_io_error_kinds_are_not_retried() {
        // The structural check is narrow: an io error alone is not enough.
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::InvalidData,
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::AddrNotAvailable,
        ] {
            let mut other = diagnostics((false, false, true, false), &["transport error"]);
            other.io_error_kind = Some(kind);
            assert_eq!(
                transient_transport_reason(&other),
                None,
                "{kind:?} must not be replayed"
            );
        }
    }

    #[test]
    fn timeouts_and_connect_failures_are_never_transient() {
        // Same source chain, but reqwest attributes it to a timeout or to
        // connection setup: both have dedicated handling and must not replay.
        let mut timed_out = production_disconnect();
        timed_out.is_timeout = true;
        assert_eq!(transient_transport_reason(&timed_out), None);

        let mut refused = production_disconnect();
        refused.is_connect = true;
        assert_eq!(transient_transport_reason(&refused), None);
    }

    #[test]
    fn transport_failures_without_a_known_signature_are_not_transient() {
        // V1 rule: uncertain classification means no retry.
        assert_eq!(
            transient_transport_reason(&diagnostics(
                (false, false, true, false),
                &["invalid certificate", "unknown ca"],
            )),
            None
        );
        // A recognized signature that reqwest blames on neither the request
        // nor the body side stays non-retryable too.
        assert_eq!(
            transient_transport_reason(&diagnostics(
                (false, false, false, false),
                &["connection reset"],
            )),
            None
        );
    }

    #[test]
    fn every_documented_transport_signature_is_matched_case_insensitively() {
        for signature in TRANSIENT_TRANSPORT_SIGNATURES {
            let fragment = signature.to_ascii_uppercase();
            assert_eq!(
                transient_transport_reason(&diagnostics(
                    (false, false, true, false),
                    &[fragment.as_str()],
                )),
                Some("transport_disconnect"),
                "signature {signature} must match regardless of case"
            );
        }
    }

    #[test]
    fn only_gateway_statuses_are_transient() {
        assert_eq!(transient_http_reason(502), Some("http_502"));
        assert_eq!(transient_http_reason(503), Some("http_503"));
        assert_eq!(transient_http_reason(504), Some("http_504"));
        for permanent in [400, 401, 403, 404, 422, 429, 500] {
            assert_eq!(
                transient_http_reason(permanent),
                None,
                "HTTP {permanent} must never be replayed"
            );
        }
    }

    #[test]
    fn retry_policy_is_one_replay_after_a_bounded_backoff() {
        assert_eq!(MAX_PROVIDER_ATTEMPTS, 2);
        assert_eq!(PROVIDER_RETRY_BACKOFF, Duration::from_millis(1500));
    }

    #[test]
    fn partial_or_complete_output_revokes_retry_permission() {
        let retryable = || {
            AttemptError::permanent(anyhow::anyhow!("boom"), "stream", "network_error")
                .retryable(Some("transport_disconnect"))
        };
        assert!(retryable()
            .block_retry_unless_no_output(ResponseCompletionState::NoOutput)
            .retry_reason
            .is_some());
        assert!(retryable()
            .block_retry_unless_no_output(ResponseCompletionState::Partial)
            .retry_reason
            .is_none());
        assert!(retryable()
            .block_retry_unless_no_output(ResponseCompletionState::Complete)
            .retry_reason
            .is_none());
    }

    #[test]
    fn exhausted_transient_retries_surface_the_disconnect_message() {
        let failure = AttemptError::permanent(
            anyhow::anyhow!("流式读取失败：网络请求失败（provider=openai-compatible, host=x）"),
            "stream",
            "network_error",
        )
        .retryable(Some("transport_disconnect"));
        let message = final_provider_error(failure, 2).to_string();
        assert!(message.contains(TRANSIENT_DISCONNECT_MESSAGE));
        assert!(message.contains("共尝试 2 次"));

        // Permanent failures keep their original, more specific wording.
        let permanent = AttemptError::permanent(
            anyhow::anyhow!("OpenAI API error (400)"),
            "request",
            "http_error",
        );
        assert_eq!(
            final_provider_error(permanent, 1).to_string(),
            "OpenAI API error (400)"
        );
    }

    // ---------------------------------------------------------------------
    // Transient retry: output-started tracking (paused clock, no network)
    // ---------------------------------------------------------------------

    /// Runs `consume_sse_stream` over chunks that end in a transport error and
    /// reports the application-level completion state at the failure point.
    async fn completion_state_after_error(chunks: Vec<Vec<u8>>) -> ResponseCompletionState {
        let mut stream = Box::pin(
            stream::iter(chunks.into_iter().map(Ok))
                .chain(stream::iter(vec![Err(TestStreamError)])),
        );
        let (token_tx, _token_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let first_delta_ms = AtomicU64::new(0);
        let outcome = consume_sse_stream(
            &mut stream,
            Duration::from_secs(300),
            &token_tx,
            &first_delta_ms,
            Instant::now(),
        )
        .await;
        match outcome {
            Err(StreamConsumeError::Transport {
                completion_state, ..
            }) => completion_state,
            Err(StreamConsumeError::Incomplete {
                completion_state, ..
            }) => completion_state,
            Err(StreamConsumeError::Idle { secs }) => {
                panic!("expected a transport error, got an idle timeout after {secs}s")
            }
            Ok(_) => panic!("expected a transport error, got a completed stream"),
        }
    }

    fn tool_call_chunk() -> Vec<u8> {
        sse_line(
            r#"{"id":"t1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"read_file","arguments":"{\"pa"}}]},"finish_reason":null}]}"#,
        )
    }

    fn finish_chunk(reason: &str) -> Vec<u8> {
        sse_line(&format!(
            r#"{{"id":"t1","object":"chat.completion.chunk","choices":[{{"index":0,"delta":{{}},"finish_reason":"{reason}"}}]}}"#
        ))
    }

    fn full_tool_call_chunk() -> Vec<u8> {
        sse_line(
            r#"{"id":"t1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"/tmp/a\"}"}}]},"finish_reason":null}]}"#,
        )
    }

    #[tokio::test(start_paused = true)]
    async fn stream_error_before_any_output_allows_a_retry() {
        assert_eq!(
            completion_state_after_error(vec![]).await,
            ResponseCompletionState::NoOutput
        );
    }

    #[tokio::test(start_paused = true)]
    async fn stream_error_after_a_started_tool_call_blocks_retry() {
        // A partially accumulated tool call must be Partial: replaying could
        // run the tool twice, so retry permission must be revoked.
        assert_eq!(
            completion_state_after_error(vec![tool_call_chunk()]).await,
            ResponseCompletionState::Partial
        );
        assert_eq!(
            completion_state_after_error(vec![delta_chunk("已生成"), tool_call_chunk()]).await,
            ResponseCompletionState::Partial
        );
    }

    // ---------------------------------------------------------------------
    // Transient retry: end-to-end attempt counting (loopback, no network)
    // ---------------------------------------------------------------------

    /// One scripted response for the loopback provider.
    ///
    /// Disconnects are simulated with a clean FIN rather than an RST: hyper
    /// then reports the platform-independent "connection closed before message
    /// completed", whereas an RST surfaces as an OS-specific message.
    #[derive(Clone)]
    enum Scripted {
        /// Hang up after reading the request, before any response bytes: the
        /// shape of a provider-side mid-flight disconnect.
        DropBeforeResponse,
        /// Same, but forced to an RST via `SO_LINGER = 0`, which is what a
        /// load balancer killing the connection looks like. Deterministic now
        /// that the request is fully drained first.
        ResetBeforeResponse,
        /// Reply with a status line and JSON body.
        Status(u16, &'static str),
        /// Reply with a complete SSE stream.
        Sse(Vec<Vec<u8>>),
        /// Reply with chunk-framed SSE data and hang up before the terminating
        /// chunk, so the client sees a genuine transport error *after* output.
        SseTruncated(Vec<Vec<u8>>),
        /// Reply with SSE headers, then stay silent so the idle timer decides.
        SseThenSilence,
    }

    /// Reads one complete HTTP request, honouring `Content-Length`, so the
    /// socket holds no unread data. Returns `None` if the peer went away.
    async fn read_request(socket: &mut tokio::net::TcpStream) -> Option<()> {
        use tokio::io::AsyncReadExt;

        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            if socket.read(&mut byte).await.unwrap_or(0) != 1 {
                return None;
            }
            head.push(byte[0]);
            if head.ends_with(b"\r\n\r\n") {
                break;
            }
        }

        let text = String::from_utf8_lossy(&head).to_ascii_lowercase();
        let length = text
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if length > 0 {
            let mut body = vec![0u8; length];
            socket.read_exact(&mut body).await.ok()?;
        }
        Some(())
    }

    /// Serves `script` in order, one entry per request, and reports how many
    /// requests the client actually issued.
    async fn serve_scripted(script: Vec<Scripted>) -> (String, Arc<AtomicU32>) {
        use tokio::io::AsyncWriteExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback listener");
        let addr = listener.local_addr().expect("listener address");
        let requests = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&requests);
        tokio::spawn(async move {
            for step in script {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                // The whole request must be consumed, head *and* body: closing
                // a socket that still holds unread data makes Windows send an
                // RST instead of a FIN, which would make every disconnect
                // scenario below nondeterministic.
                if read_request(&mut socket).await.is_none() {
                    return;
                }
                counter.fetch_add(1, Ordering::Relaxed);

                match step {
                    Scripted::DropBeforeResponse => {
                        let _ = socket.shutdown().await;
                    }
                    Scripted::ResetBeforeResponse => {
                        // Deprecated because `SO_LINGER` can block the thread
                        // on drop, which a zero timeout cannot do: it discards
                        // the send buffer and emits an RST immediately. That
                        // is the only portable way to force a reset here.
                        #[allow(deprecated)]
                        let _ = socket.set_linger(Some(Duration::ZERO));
                        drop(socket);
                    }
                    Scripted::Status(status, body) => {
                        let response = format!(
                            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = socket.write_all(response.as_bytes()).await;
                        let _ = socket.shutdown().await;
                    }
                    Scripted::Sse(chunks) => {
                        let _ = socket
                            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n")
                            .await;
                        for chunk in chunks {
                            let _ = socket.write_all(&chunk).await;
                            let _ = socket.flush().await;
                        }
                        let _ = socket.shutdown().await;
                    }
                    Scripted::SseTruncated(chunks) => {
                        let _ = socket
                            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n")
                            .await;
                        for chunk in chunks {
                            let _ = socket
                                .write_all(format!("{:x}\r\n", chunk.len()).as_bytes())
                                .await;
                            let _ = socket.write_all(&chunk).await;
                            let _ = socket.write_all(b"\r\n").await;
                            let _ = socket.flush().await;
                        }
                        // No terminating `0\r\n\r\n`: the message is
                        // incomplete, so the client reports a transport error.
                        let _ = socket.shutdown().await;
                    }
                    Scripted::SseThenSilence => {
                        let _ = socket
                            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n")
                            .await;
                        std::future::pending::<()>().await;
                    }
                }
            }
        });
        (format!("http://{addr}"), requests)
    }

    /// Streams one request against the scripted loopback provider and returns
    /// the result, the forwarded deltas and the delivered attempt count.
    async fn stream_against_script(
        script: Vec<Scripted>,
        timeouts: ProviderTimeouts,
    ) -> (anyhow::Result<ProviderChatResponse>, Vec<String>, u32) {
        let (base_url, requests) = serve_scripted(script).await;
        let provider: Box<dyn Provider> = Box::new(
            OpenAiProvider::new(Some(&base_url), None, "retry-model", 0.0, None, timeouts)
                // Keeps the suite fast; the production value is asserted
                // separately.
                .with_retry_backoff(Duration::from_millis(1)),
        );
        let messages = vec![ChatMessage::user("hi")];
        let request = ProviderChatRequest {
            messages: &messages,
            tools: None,
        };
        let (token_tx, mut token_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let result = provider.chat_stream(request, token_tx).await;
        let deltas = drain_deltas(&mut token_rx);
        (result, deltas, requests.load(Ordering::Relaxed))
    }

    fn ok_sse() -> Scripted {
        Scripted::Sse(vec![
            delta_chunk("He"),
            delta_chunk("llo"),
            sse_line("[DONE]"),
        ])
    }

    #[tokio::test]
    async fn disconnect_before_any_delta_is_retried_once_and_succeeds() {
        let (result, deltas, attempts) = stream_against_script(
            vec![Scripted::DropBeforeResponse, ok_sse()],
            ProviderTimeouts::default(),
        )
        .await;
        let response = result.expect("the replay must succeed");
        assert_eq!(response.text.as_deref(), Some("Hello"));
        assert_eq!(deltas, vec!["He", "llo"]);
        assert_eq!(attempts, 2, "exactly one replay");
    }

    #[tokio::test]
    async fn a_connection_reset_is_retried_once_and_succeeds() {
        // End-to-end proof of the structural classification: on Windows the
        // ECONNRESET message is localized, so only the `io::ErrorKind` can
        // recognize this.
        let (result, deltas, attempts) = stream_against_script(
            vec![Scripted::ResetBeforeResponse, ok_sse()],
            ProviderTimeouts::default(),
        )
        .await;
        assert_eq!(
            result.expect("the replay must succeed").text.as_deref(),
            Some("Hello")
        );
        assert_eq!(deltas, vec!["He", "llo"]);
        assert_eq!(attempts, 2);
    }

    #[tokio::test]
    async fn non_streaming_chat_shares_the_same_retry_policy() {
        // Both provider entry points go through one driver, so callers that
        // use plain `chat` get the same single replay.
        const COMPLETION: &str = r#"{"id":"c1","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":"Hello"},"finish_reason":"stop"}]}"#;
        let (base_url, requests) = serve_scripted(vec![
            Scripted::DropBeforeResponse,
            Scripted::Status(200, COMPLETION),
        ])
        .await;
        let provider: Box<dyn Provider> = Box::new(
            OpenAiProvider::new(
                Some(&base_url),
                None,
                "retry-model",
                0.0,
                None,
                ProviderTimeouts::default(),
            )
            .with_retry_backoff(Duration::from_millis(1)),
        );
        let messages = vec![ChatMessage::user("hi")];
        let response = provider
            .chat(ProviderChatRequest {
                messages: &messages,
                tools: None,
            })
            .await
            .expect("the replay must succeed");
        assert_eq!(response.text.as_deref(), Some("Hello"));
        assert_eq!(requests.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn http_503_is_retried_once_and_succeeds() {
        let (result, _deltas, attempts) = stream_against_script(
            vec![
                Scripted::Status(503, "{\"error\":\"upstream busy\"}"),
                ok_sse(),
            ],
            ProviderTimeouts::default(),
        )
        .await;
        assert_eq!(
            result.expect("the replay must succeed").text.as_deref(),
            Some("Hello")
        );
        assert_eq!(attempts, 2);
    }

    #[tokio::test]
    async fn client_and_semantic_errors_are_never_retried() {
        for (status, body) in [
            (400u16, "{\"error\":{\"message\":\"malformed request\"}}"),
            (401, "{\"error\":{\"message\":\"invalid api key\"}}"),
            (403, "{\"error\":{\"message\":\"forbidden\"}}"),
            (
                400,
                "{\"error\":{\"code\":\"content_filter\",\"message\":\"blocked\"}}",
            ),
            (
                400,
                "{\"error\":{\"code\":\"context_length_exceeded\",\"message\":\"too long\"}}",
            ),
            (404, "{\"error\":{\"code\":\"model_not_found\"}}"),
            (429, "{\"error\":{\"message\":\"rate limited\"}}"),
            (500, "{\"error\":{\"message\":\"internal\"}}"),
        ] {
            let (result, _deltas, attempts) = stream_against_script(
                vec![Scripted::Status(status, body), ok_sse()],
                ProviderTimeouts::default(),
            )
            .await;
            assert!(result.is_err(), "HTTP {status} must fail");
            assert_eq!(attempts, 1, "HTTP {status} must not be replayed");
        }
    }

    #[tokio::test]
    async fn two_transient_failures_stop_after_the_single_replay() {
        let (result, _deltas, attempts) = stream_against_script(
            vec![
                Scripted::DropBeforeResponse,
                Scripted::DropBeforeResponse,
                ok_sse(),
            ],
            ProviderTimeouts::default(),
        )
        .await;
        let message = result.expect_err("both attempts failed").to_string();
        assert_eq!(attempts, 2, "retries are bounded at one replay");
        assert!(
            message.contains(TRANSIENT_DISCONNECT_MESSAGE),
            "unexpected message: {message}"
        );
    }

    /// One captured `tracing` event, flattened to strings.
    struct CapturedEvent {
        message: String,
        fields: Vec<(String, String)>,
    }

    impl CapturedEvent {
        fn field(&self, name: &str) -> Option<&str> {
            self.fields
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
        }
    }

    struct CaptureLayer(Arc<std::sync::Mutex<Vec<CapturedEvent>>>);

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CaptureLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            struct Visitor(CapturedEvent);
            impl tracing::field::Visit for Visitor {
                fn record_debug(
                    &mut self,
                    field: &tracing::field::Field,
                    value: &dyn std::fmt::Debug,
                ) {
                    let rendered = format!("{value:?}").trim_matches('"').to_string();
                    if field.name() == "message" {
                        self.0.message = rendered;
                    } else {
                        self.0.fields.push((field.name().to_string(), rendered));
                    }
                }
            }
            let mut visitor = Visitor(CapturedEvent {
                message: String::new(),
                fields: Vec::new(),
            });
            event.record(&mut visitor);
            self.0.lock().unwrap().push(visitor.0);
        }
    }

    #[tokio::test]
    async fn retry_telemetry_is_complete_and_leaks_nothing() {
        use tracing_subscriber::layer::SubscriberExt;

        const SECRET: &str = "sk-never-log-this-secret";
        const PROMPT: &str = "confidential contract clause 7.3";

        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::registry().with(CaptureLayer(Arc::clone(&events)));
        let _guard = tracing::subscriber::set_default(subscriber);

        // `tracing` caches callsite interest process-wide. Sibling tests run
        // without a subscriber, so these callsites may already be cached as
        // "no interest" for the whole test binary. Rebuilding the cache while
        // this local subscriber is installed re-enables them; the warm-up then
        // proves the sink really receives events, so this test fails loudly
        // instead of passing vacuously. Test-only concern: production tracing
        // setup is untouched.
        for _ in 0..5 {
            tracing::callsite::rebuild_interest_cache();
            log_provider_retry("warm-up", 0, "warm-up", 0, "auto", "persistent");
            log_provider_error("warm-up", "warm-up", "warm-up", 0, None, None, None);
            if events.lock().unwrap().len() >= 2 {
                break;
            }
        }
        assert!(
            events.lock().unwrap().len() >= 2,
            "tracing capture is unavailable, the assertions below would be vacuous"
        );
        events.lock().unwrap().clear();

        let (base_url, requests) = serve_scripted(vec![
            Scripted::DropBeforeResponse,
            Scripted::DropBeforeResponse,
        ])
        .await;
        let provider: Box<dyn Provider> = Box::new(
            OpenAiProvider::new(
                Some(&base_url),
                Some(SECRET),
                "retry-model",
                0.0,
                None,
                ProviderTimeouts::default(),
            )
            .with_retry_backoff(Duration::from_millis(1)),
        );
        let messages = vec![ChatMessage::user(PROMPT)];
        let (token_tx, _token_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let _ = provider
            .chat_stream(
                ProviderChatRequest {
                    messages: &messages,
                    tools: None,
                },
                token_tx,
            )
            .await;
        assert_eq!(requests.load(Ordering::Relaxed), 2);

        let captured = events.lock().unwrap();
        let retries: Vec<_> = captured
            .iter()
            .filter(|event| event.message == "[provider-retry]")
            .collect();
        assert_eq!(retries.len(), 1, "exactly one retry must be announced");
        let retry = retries[0];
        assert_eq!(retry.field("model"), Some("retry-model"));
        assert_eq!(retry.field("attempt"), Some("1"));
        assert_eq!(retry.field("max_attempts"), Some("2"));
        assert!(
            matches!(
                retry.field("reason"),
                Some("transport_disconnect" | "http_502")
            ),
            "unexpected retry reason: {:?}",
            retry.field("reason")
        );
        assert_eq!(retry.field("backoff_ms"), Some("1"));
        assert_eq!(retry.field("completion_state"), Some("no_output"));
        assert_eq!(retry.field("transport_mode"), Some("auto"));
        assert_eq!(retry.field("connection"), Some("fresh"));

        let errors: Vec<_> = captured
            .iter()
            .filter(|event| event.message == "[provider-error]")
            .collect();
        assert_eq!(errors.len(), 1, "the final failure is reported once");
        assert_eq!(errors[0].field("attempts"), Some("2"));
        assert_eq!(errors[0].field("model"), Some("retry-model"));

        // Nothing sensitive may reach any log line, including the per-attempt
        // network diagnostics.
        for event in captured.iter() {
            for (key, value) in &event.fields {
                for forbidden in [SECRET, PROMPT, "Bearer", "Authorization"] {
                    assert!(
                        !value.contains(forbidden),
                        "field {key} leaked {forbidden}: {value}"
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn a_stream_idle_timeout_is_never_retried() {
        let (result, _deltas, attempts) = stream_against_script(
            vec![Scripted::SseThenSilence, ok_sse()],
            ProviderTimeouts {
                stream_idle: Duration::from_millis(150),
                ..ProviderTimeouts::default()
            },
        )
        .await;
        let message = result.expect_err("idle timeout must fail").to_string();
        assert!(
            message.contains("stream idle timeout"),
            "unexpected message: {message}"
        );
        assert_eq!(attempts, 1, "timeouts must never be replayed");
    }

    #[tokio::test]
    async fn a_request_total_timeout_is_never_retried() {
        let (result, _deltas, attempts) = stream_against_script(
            vec![Scripted::SseThenSilence, ok_sse()],
            ProviderTimeouts {
                request: Duration::from_millis(150),
                ..ProviderTimeouts::default()
            },
        )
        .await;
        let message = result.expect_err("request timeout must fail").to_string();
        assert!(
            message.contains("request timeout"),
            "unexpected message: {message}"
        );
        assert_eq!(attempts, 1, "timeouts must never be replayed");
    }

    #[tokio::test]
    async fn a_stream_that_dies_after_partial_output_is_never_replayed() {
        // The failure here is the same transport class that *is* retried when
        // it happens before any output (see the test above), so this isolates
        // the output-started veto. The second script entry answers with
        // different text, making any replay visible.
        let (result, deltas, attempts) = stream_against_script(
            vec![
                Scripted::SseTruncated(vec![delta_chunk("部分"), tool_call_chunk()]),
                Scripted::Sse(vec![delta_chunk("REPLAYED"), sse_line("[DONE]")]),
            ],
            ProviderTimeouts::default(),
        )
        .await;
        assert_eq!(attempts, 1, "partial output must veto a replay");
        assert_eq!(deltas, vec!["部分"]);
        let message = result
            .expect_err("a truncated stream must fail")
            .to_string();
        assert!(
            !message.contains("REPLAYED"),
            "unexpected message: {message}"
        );
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

    #[tokio::test]
    async fn complete_response_with_trailing_eof_is_accepted_without_retry() {
        let (result, deltas, attempts) = stream_against_script(
            vec![Scripted::SseTruncated(vec![
                delta_chunk("He"),
                delta_chunk("llo"),
                finish_chunk("stop"),
                sse_line("[DONE]"),
            ])],
            ProviderTimeouts::default(),
        )
        .await;
        let response = result.expect("complete response must be accepted");
        assert_eq!(response.text.as_deref(), Some("Hello"));
        assert_eq!(deltas, vec!["He", "llo"]);
        assert_eq!(attempts, 1);
    }

    #[tokio::test]
    async fn partial_text_with_eof_is_explicit_incomplete_and_not_retried() {
        let (result, deltas, attempts) = stream_against_script(
            vec![Scripted::SseTruncated(vec![delta_chunk("部分")]), ok_sse()],
            ProviderTimeouts::default(),
        )
        .await;
        let message = result
            .expect_err("partial text must fail explicitly")
            .to_string();
        assert_eq!(deltas, vec!["部分"]);
        assert_eq!(attempts, 1);
        assert!(
            message.contains("响应不完整") || message.contains("incomplete"),
            "unexpected message: {message}"
        );
    }

    #[tokio::test]
    async fn no_output_transient_failure_retries_on_fresh_connection_and_succeeds() {
        let (result, deltas, attempts) = stream_against_script(
            vec![Scripted::DropBeforeResponse, ok_sse()],
            ProviderTimeouts::default(),
        )
        .await;
        let response = result.expect("fresh retry must succeed");
        assert_eq!(response.text.as_deref(), Some("Hello"));
        assert_eq!(deltas, vec!["He", "llo"]);
        assert_eq!(attempts, 2);
    }

    #[tokio::test]
    async fn partial_tool_call_is_not_replayed_and_not_executed() {
        let (result, _deltas, attempts) = stream_against_script(
            vec![Scripted::SseTruncated(vec![tool_call_chunk()]), ok_sse()],
            ProviderTimeouts::default(),
        )
        .await;
        assert!(
            result.is_err(),
            "partial tool call must not produce a response"
        );
        assert_eq!(attempts, 1);
    }

    #[tokio::test]
    async fn complete_tool_call_preserves_normal_behavior() {
        let (result, _deltas, attempts) = stream_against_script(
            vec![Scripted::Sse(vec![
                full_tool_call_chunk(),
                finish_chunk("tool_calls"),
                sse_line("[DONE]"),
            ])],
            ProviderTimeouts::default(),
        )
        .await;
        let response = result.expect("complete tool call must succeed");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].name, "read_file");
        assert!(
            serde_json::from_str::<serde_json::Value>(&response.tool_calls[0].arguments).is_ok(),
            "tool call arguments must be valid JSON"
        );
        assert_eq!(attempts, 1);
    }

    #[test]
    fn transport_modes_build_clients_for_auto_http1_http2() {
        for mode in [
            TransportMode::Auto,
            TransportMode::Http1,
            TransportMode::Http2,
        ] {
            let provider = OpenAiProvider::new(
                Some("https://example.invalid/v1"),
                None,
                "mode-test",
                0.0,
                None,
                ProviderTimeouts::default(),
            )
            .with_transport_mode(mode);
            assert_eq!(provider.transport_mode, mode);
            assert_eq!(mode.as_str(), provider.transport_mode.as_str());
        }
    }

    #[tokio::test]
    async fn retry_regression_status_matrix() {
        for status in [400u16, 401, 403, 404, 422, 429, 500] {
            let (result, _deltas, attempts) = stream_against_script(
                vec![Scripted::Status(status, "{\"error\":{}}"), ok_sse()],
                ProviderTimeouts::default(),
            )
            .await;
            assert!(result.is_err(), "HTTP {status} must fail");
            assert_eq!(attempts, 1, "HTTP {status} must not retry");
        }

        for status in [502u16, 503, 504] {
            let (result, _deltas, attempts) = stream_against_script(
                vec![Scripted::Status(status, "{\"error\":{}}"), ok_sse()],
                ProviderTimeouts::default(),
            )
            .await;
            assert!(result.is_ok(), "HTTP {status} replay must succeed");
            assert_eq!(attempts, 2, "HTTP {status} must retry once");
        }
    }

    #[tokio::test]
    async fn http_503_with_model_not_found_is_not_retried() {
        let (result, _deltas, attempts) = stream_against_script(
            vec![
                Scripted::Status(
                    503,
                    "{\"error\":{\"code\":\"model_not_found\",\"message\":\"missing\"}}",
                ),
                ok_sse(),
            ],
            ProviderTimeouts::default(),
        )
        .await;
        assert!(result.is_err(), "semantic model_not_found must fail");
        assert_eq!(
            attempts, 1,
            "semantic provider error must override HTTP retry"
        );
    }
}
