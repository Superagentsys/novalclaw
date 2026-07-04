use crate::providers::traits::{
    ChatMessage, ChatRequest as ProviderChatRequest, ChatResponse as ProviderChatResponse, Provider,
    TokenUsage, ToolCall as ProviderToolCall,
};
use crate::tools::ToolSpec;
use async_trait::async_trait;
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};

pub struct OpenAiProvider {
    base_url: String,
    credential: Option<String>,
    model: String,
    temperature: f64,
    max_tokens: Option<u32>,
    client: Client,
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

async fn api_error(provider_name: &str, response: Response) -> anyhow::Error {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    anyhow::anyhow!("{provider_name} API error ({status}): {text}")
}

impl OpenAiProvider {
    pub fn new(
        base_url: Option<&str>,
        credential: Option<&str>,
        model: impl Into<String>,
        temperature: f64,
        max_tokens: Option<u32>,
    ) -> Self {
        let base_url = base_url
            .map(|u| u.trim_end_matches('/').to_string())
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        let client = Client::builder()
            .build()
            .expect("failed to build reqwest client");

        Self {
            base_url,
            credential: credential.map(ToString::to_string),
            model: model.into(),
            temperature,
            max_tokens: max_tokens.filter(|v| *v > 0),
            client,
        }
    }

    /// 构建 POST 请求。本地推理服务（如 Ollama / LM Studio）通常无需 API Key，
    /// 因此仅在存在 credential 时附加 `Authorization` 头，否则不带鉴权直接请求。
    fn authorized_post(&self, path: &str) -> reqwest::RequestBuilder {
        let req = self.client.post(format!("{}{}", self.base_url, path));
        match self.credential.as_ref() {
            Some(credential) => req.header("Authorization", format!("Bearer {credential}")),
            None => req,
        }
    }

    /// 构建 GET 请求，鉴权头处理同 [`authorized_post`]。
    fn authorized_get(&self, path: &str) -> reqwest::RequestBuilder {
        let req = self.client.get(format!("{}{}", self.base_url, path));
        match self.credential.as_ref() {
            Some(credential) => req.header("Authorization", format!("Bearer {credential}")),
            None => req,
        }
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

    fn parse_native_response(message: NativeResponseMessage) -> ProviderChatResponse {
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
            .map_err(|e| {
                if e.is_timeout() {
                    anyhow::anyhow!(
                        "请求超时：调用 {} 超时，请检查网络连通性或 API 服务可用性",
                        self.base_url
                    )
                } else if e.is_connect() {
                    anyhow::anyhow!(
                        "连接失败：无法连接到 {}，请检查 Base URL 配置和网络连通性",
                        self.base_url
                    )
                } else {
                    anyhow::anyhow!("网络请求失败：{}", e)
                }
            })?;

        if !response.status().is_success() {
            return Err(api_error("OpenAI", response).await);
        }

        let native_response: NativeChatResponse = response.json().await?;
        let usage = native_response.usage.map(|u| TokenUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
        });
        let message = native_response
            .choices
            .into_iter()
            .next()
            .map(|c| c.message)
            .ok_or_else(|| anyhow::anyhow!("No response from OpenAI"))?;
        let mut result = Self::parse_native_response(message);
        result.usage = usage;
        Ok(result)
    }

    async fn chat_stream(
        &self,
        request: ProviderChatRequest<'_>,
        token_tx: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> anyhow::Result<ProviderChatResponse> {
        use futures_util::StreamExt;

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
            .map_err(|e| anyhow::anyhow!("流式请求失败：{e}"))?;

        if !response.status().is_success() {
            return Err(api_error("OpenAI", response).await);
        }

        let mut stream = response.bytes_stream();
        let mut buf = String::new();
        let mut text = String::new();
        let mut reasoning = String::new();
        // (id, name, arguments) accumulated per tool-call index.
        let mut tool_accum: Vec<(String, String, String)> = Vec::new();
        let mut usage: Option<TokenUsage> = None;
        let mut done = false;

        while let Some(item) = stream.next().await {
            let bytes = item.map_err(|e| anyhow::anyhow!("流式读取失败：{e}"))?;
            buf.push_str(&String::from_utf8_lossy(&bytes));

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
                    if let Some(c) = choice.delta.content {
                        if !c.is_empty() {
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
        })
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
