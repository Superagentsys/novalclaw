//! Anthropic Claude provider implementation.
//!
//! This module implements a native Anthropic Messages API client (not OpenAI-compatible).
//! It supports streaming, tool calls, and proper error handling for Claude models.

use crate::providers::traits::{
    ChatMessage, ChatRequest, ChatResponse, ChatStream, ChatStreamChunk, EmbeddingRequest,
    EmbeddingResponse, ModelInfo, Provider, StreamError, TokenUsage, ToolCall,
};
use crate::tools::ToolSpec;
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio_stream::wrappers::ReceiverStream;

// ============================================================================
// Anthropic Native Types
// ============================================================================

/// Anthropic Messages API request.
#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

/// Anthropic message in the messages array.
#[derive(Debug, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContentBlock>,
}

/// Anthropic content block.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: ImageSource },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
        content: String,
    },
}

/// Image source for multimodal requests.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct ImageSource {
    #[serde(rename = "type")]
    source_type: String,
    media_type: String,
    data: String,
}

/// Anthropic tool definition.
#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    input_schema: serde_json::Value,
}

/// Anthropic Messages API response.
#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    #[serde(rename = "type")]
    response_type: String,
    role: String,
    content: Vec<AnthropicContentBlock>,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequence: Option<String>,
    usage: AnthropicUsage,
}

/// Anthropic usage statistics.
#[derive(Debug, Deserialize, Clone)]
struct AnthropicUsage {
    input_tokens: u64,
    output_tokens: u64,
}

/// Anthropic error response.
#[derive(Debug, Deserialize)]
struct AnthropicErrorResponse {
    #[serde(rename = "type")]
    error_type: Option<String>,
    error: Option<AnthropicErrorDetail>,
}

#[derive(Debug, Deserialize, Clone)]
struct AnthropicErrorDetail {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

// ============================================================================
// Streaming Types
// ============================================================================

/// Anthropic streaming event types.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicStreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: AnthropicResponse },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: u32,
        content_block: AnthropicContentBlock,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta {
        index: u32,
        delta: AnthropicDelta,
    },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: u32 },
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: AnthropicMessageDelta,
        usage: Option<AnthropicUsage>,
    },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "error")]
    Error { error: AnthropicErrorDetail },
}

/// Delta content in streaming.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicDelta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
}

/// Message delta in streaming.
#[derive(Debug, Deserialize)]
struct AnthropicMessageDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequence: Option<String>,
}

// ============================================================================
// Anthropic Provider
// ============================================================================

/// Anthropic Claude provider with native Messages API support.
pub struct AnthropicProvider {
    base_url: String,
    credential: Option<String>,
    model: String,
    temperature: f64,
    max_tokens: u32,
    client: Client,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider.
    ///
    /// # Arguments
    /// * `base_url` - Optional custom base URL (defaults to Anthropic API)
    /// * `api_key` - Anthropic API key
    /// * `model` - Model identifier (e.g., "claude-3-5-sonnet-20241022")
    /// * `temperature` - Sampling temperature (0.0 to 1.0)
    /// * `max_tokens` - Maximum tokens in response (required by Anthropic)
    pub fn new(
        base_url: Option<&str>,
        api_key: Option<&str>,
        model: impl Into<String>,
        temperature: f64,
        max_tokens: Option<u32>,
    ) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            base_url: base_url
                .unwrap_or("https://api.anthropic.com")
                .trim_end_matches('/')
                .to_string(),
            credential: api_key.map(|s| s.to_string()),
            model: model.into(),
            temperature,
            max_tokens: max_tokens.unwrap_or(4096),
            client,
        }
    }

    /// Convert generic chat messages to Anthropic format.
    fn convert_messages(
        messages: &[ChatMessage],
    ) -> (Option<String>, Vec<AnthropicMessage>) {
        let mut system_prompt: Option<String> = None;
        let mut anthropic_messages: Vec<AnthropicMessage> = Vec::new();

        for msg in messages {
            match msg.role.as_str() {
                "system" => {
                    // Anthropic uses a separate system parameter
                    system_prompt = Some(msg.content.clone());
                }
                "user" => {
                    anthropic_messages.push(AnthropicMessage {
                        role: "user".to_string(),
                        content: vec![AnthropicContentBlock::Text {
                            text: msg.content.clone(),
                        }],
                    });
                }
                "assistant" => {
                    anthropic_messages.push(AnthropicMessage {
                        role: "assistant".to_string(),
                        content: vec![AnthropicContentBlock::Text {
                            text: msg.content.clone(),
                        }],
                    });
                }
                "tool" => {
                    // Tool results should be attached to the last user message
                    // For simplicity, we create a user message with tool_result
                    if let Some(last_msg) = anthropic_messages.last_mut() {
                        if last_msg.role == "user" {
                            last_msg.content.push(AnthropicContentBlock::ToolResult {
                                tool_use_id: String::new(), // Would need tool_call_id from context
                                is_error: None,
                                content: msg.content.clone(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        (system_prompt, anthropic_messages)
    }

    /// Convert tool specifications to Anthropic format.
    fn convert_tools(tools: &[ToolSpec]) -> Vec<AnthropicTool> {
        tools
            .iter()
            .map(|t| AnthropicTool {
                name: t.name.clone(),
                description: Some(t.description.clone()),
                input_schema: t.parameters.clone(),
            })
            .collect()
    }

    /// Parse Anthropic response into generic ChatResponse.
    fn parse_response(response: AnthropicResponse) -> ChatResponse {
        let mut text: Option<String> = None;
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        for block in response.content {
            match block {
                AnthropicContentBlock::Text { text: t } => {
                    text = Some(match text {
                        Some(existing) => existing + &t,
                        None => t,
                    });
                }
                AnthropicContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments: serde_json::to_string(&input).unwrap_or_default(),
                    });
                }
                _ => {}
            }
        }

        ChatResponse {
            text,
            tool_calls,
            usage: Some(TokenUsage {
                input_tokens: Some(response.usage.input_tokens),
                output_tokens: Some(response.usage.output_tokens),
            }),
            reasoning_content: None,
        }
    }

    /// Create an error from an HTTP response.
    async fn api_error(response: reqwest::Response) -> anyhow::Error {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());

        // Try to parse Anthropic error format
        if let Ok(error_resp) = serde_json::from_str::<AnthropicErrorResponse>(&body) {
            // Extract error type from either nested error or top-level type
            let error_type = error_resp
                .error
                .as_ref()
                .map(|e| e.error_type.as_str())
                .or(error_resp.error_type.as_deref())
                .unwrap_or("unknown_error");

            // Extract message, cloning if needed
            let message = error_resp
                .error
                .clone()
                .map(|e| e.message)
                .unwrap_or_else(|| body.clone());

            return match error_type {
                "authentication_error" => {
                    anyhow::anyhow!("认证失败：Anthropic API 密钥无效，请检查 API Key 配置")
                }
                "permission_error" => {
                    anyhow::anyhow!("权限不足：您的 API Key 没有访问此资源的权限")
                }
                "rate_limit_error" => {
                    anyhow::anyhow!("请求过于频繁：已达到 API 速率限制，请稍后重试")
                }
                "overloaded_error" => {
                    anyhow::anyhow!("服务过载：Anthropic 服务当前负载过高，请稍后重试")
                }
                "invalid_request_error" => {
                    anyhow::anyhow!("请求格式错误：{}", message)
                }
                _ => anyhow::anyhow!("Anthropic API 错误（{}）：{}", error_type, message),
            };
        }

        // Fallback to generic error
        match status.as_u16() {
            401 => anyhow::anyhow!("认证失败：Anthropic API 密钥无效，请检查 API Key 配置"),
            403 => anyhow::anyhow!("权限不足：您的 API Key 没有访问权限"),
            429 => anyhow::anyhow!("请求过于频繁：已达到 API 速率限制，请稍后重试"),
            500 => anyhow::anyhow!("服务器错误：Anthropic 服务内部错误，请稍后重试"),
            502 | 503 => anyhow::anyhow!("服务不可用：Anthropic 服务暂时不可用，请稍后重试"),
            _ => anyhow::anyhow!("请求失败：HTTP {} - {}", status, body),
        }
    }

    /// Detect model capabilities from model ID.
    fn detect_model_capabilities(model_id: &str) -> ModelInfo {
        let id = model_id.trim().to_ascii_lowercase();
        let is_claude_3 = id.starts_with("claude-3");
        let is_claude_35 = id.contains("3-5") || id.contains("3.5");

        // All Claude 3+ models have tools and streaming
        let supports_tools = is_claude_3 || is_claude_35;
        let supports_streaming = true;

        // Vision support varies
        let supports_vision = is_claude_3 || is_claude_35;
        // claude-3-5-haiku doesn't support vision
        let supports_vision = supports_vision && !id.contains("claude-3-5-haiku");

        // Context length is 200K for all Claude 3+ models
        let context_length = if is_claude_3 || is_claude_35 {
            Some(200_000)
        } else {
            None
        };

        let name = match () {
            _ if id.contains("opus") => "Claude 3 Opus",
            _ if id.contains("sonnet") && is_claude_35 => "Claude 3.5 Sonnet",
            _ if id.contains("sonnet") => "Claude 3 Sonnet",
            _ if id.contains("haiku") && is_claude_35 => "Claude 3.5 Haiku",
            _ if id.contains("haiku") => "Claude 3 Haiku",
            _ => "Claude",
        };

        ModelInfo {
            id: model_id.to_string(),
            name: name.to_string(),
            description: None,
            context_length,
            supports_tools,
            supports_vision,
            supports_streaming,
        }
    }

    /// Get the list of available Claude models.
    fn get_claude_models() -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "claude-3-opus-20240229".to_string(),
                name: "Claude 3 Opus".to_string(),
                description: Some("Most capable Claude model for complex tasks".to_string()),
                context_length: Some(200_000),
                supports_tools: true,
                supports_vision: true,
                supports_streaming: true,
            },
            ModelInfo {
                id: "claude-3-sonnet-20240229".to_string(),
                name: "Claude 3 Sonnet".to_string(),
                description: Some("Balanced performance and speed".to_string()),
                context_length: Some(200_000),
                supports_tools: true,
                supports_vision: true,
                supports_streaming: true,
            },
            ModelInfo {
                id: "claude-3-haiku-20240307".to_string(),
                name: "Claude 3 Haiku".to_string(),
                description: Some("Fast and efficient for simple tasks".to_string()),
                context_length: Some(200_000),
                supports_tools: true,
                supports_vision: true,
                supports_streaming: true,
            },
            ModelInfo {
                id: "claude-3-5-sonnet-20241022".to_string(),
                name: "Claude 3.5 Sonnet".to_string(),
                description: Some("Latest Sonnet with improved capabilities".to_string()),
                context_length: Some(200_000),
                supports_tools: true,
                supports_vision: true,
                supports_streaming: true,
            },
            ModelInfo {
                id: "claude-3-5-haiku-20241022".to_string(),
                name: "Claude 3.5 Haiku".to_string(),
                description: Some("Latest Haiku with improved speed".to_string()),
                context_length: Some(200_000),
                supports_tools: true,
                supports_vision: false,
                supports_streaming: true,
            },
        ]
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn chat(&self, request: ChatRequest<'_>) -> anyhow::Result<ChatResponse> {
        let credential = self.credential.as_ref().ok_or_else(|| {
            anyhow::anyhow!("认证失败：未配置 Anthropic API Key，请设置 ANTHROPIC_API_KEY 或在配置中指定 api_key")
        })?;

        let (system, messages) = Self::convert_messages(request.messages);

        let anthropic_request = AnthropicRequest {
            model: self.model.clone(),
            messages,
            max_tokens: self.max_tokens,
            system,
            tools: request.tools.map(|t| Self::convert_tools(t)),
            temperature: if self.temperature >= 0.0 {
                Some(self.temperature)
            } else {
                None
            },
            stream: None,
        };

        let response = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", credential)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&anthropic_request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    anyhow::anyhow!(
                        "请求超时：调用 Anthropic API 超时（120s），请检查网络连通性"
                    )
                } else if e.is_connect() {
                    anyhow::anyhow!(
                        "连接失败：无法连接到 {}，请检查网络设置",
                        self.base_url
                    )
                } else {
                    anyhow::anyhow!("网络请求失败：{}", e)
                }
            })?;

        if !response.status().is_success() {
            return Err(Self::api_error(response).await);
        }

        let anthropic_response: AnthropicResponse = response.json().await.map_err(|e| {
            anyhow::anyhow!("解析响应失败：无法解析 Anthropic API 响应 - {}", e)
        })?;

        Ok(Self::parse_response(anthropic_response))
    }

    async fn health_check(&self) -> bool {
        // Anthropic doesn't have a simple health check endpoint
        // We can try to make a minimal request to check connectivity
        if self.credential.is_none() {
            return false;
        }

        // Simple check: just verify the API key is set
        // For a more thorough check, we could make a minimal messages request
        true
    }

    async fn chat_stream(&self, request: ChatRequest<'_>) -> anyhow::Result<ChatStream> {
        let credential = self.credential.as_ref().ok_or_else(|| {
            anyhow::anyhow!("认证失败：未配置 Anthropic API Key")
        })?;

        let (system, messages) = Self::convert_messages(request.messages);

        let anthropic_request = AnthropicRequest {
            model: self.model.clone(),
            messages,
            max_tokens: self.max_tokens,
            system,
            tools: request.tools.map(|t| Self::convert_tools(t)),
            temperature: if self.temperature >= 0.0 {
                Some(self.temperature)
            } else {
                None
            },
            stream: Some(true),
        };

        let response = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", credential)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&anthropic_request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    anyhow::anyhow!("请求超时：调用 Anthropic API 超时（120s）")
                } else if e.is_connect() {
                    anyhow::anyhow!("连接失败：无法连接到 {}", self.base_url)
                } else {
                    anyhow::anyhow!("网络请求失败：{}", e)
                }
            })?;

        if !response.status().is_success() {
            return Err(Self::api_error(response).await);
        }

        let (tx, rx) = tokio::sync::mpsc::channel(32);

        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut current_tool_calls: Vec<(u32, String, String)> = Vec::new();
            let mut accumulated_usage: Option<TokenUsage> = None;

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));

                        // Process complete SSE events
                        while let Some(event_end) = buffer.find("\n\n") {
                            let event_data = buffer[..event_end].to_string();
                            buffer = buffer[event_end + 2..].to_string();

                            // Parse SSE event
                            if let Some(event) = parse_sse_event(&event_data) {
                                match event {
                                    AnthropicStreamEvent::MessageStart { message } => {
                                        accumulated_usage = Some(TokenUsage {
                                            input_tokens: Some(message.usage.input_tokens),
                                            output_tokens: Some(message.usage.output_tokens),
                                        });
                                    }
                                    AnthropicStreamEvent::ContentBlockStart {
                                        index,
                                        content_block,
                                    } => {
                                        match content_block {
                                            AnthropicContentBlock::ToolUse { id, name, .. } => {
                                                current_tool_calls.push((index, id, name));
                                            }
                                            _ => {}
                                        }
                                    }
                                    AnthropicStreamEvent::ContentBlockDelta { index, delta } => {
                                        match delta {
                                            AnthropicDelta::TextDelta { text } => {
                                                let chunk = ChatStreamChunk {
                                                    delta: Some(text),
                                                    tool_calls: Vec::new(),
                                                    usage: None,
                                                    reasoning_content: None,
                                                    finish_reason: None,
                                                };
                                                if tx.send(Ok(chunk)).await.is_err() {
                                                    return;
                                                }
                                            }
                                            AnthropicDelta::InputJsonDelta { partial_json } => {
                                                // Accumulate tool input JSON
                                                if let Some(tool) = current_tool_calls
                                                    .iter_mut()
                                                    .find(|(i, _, _)| *i == index)
                                                {
                                                    tool.2.push_str(&partial_json);
                                                }
                                            }
                                        }
                                    }
                                    AnthropicStreamEvent::ContentBlockStop { index } => {
                                        // Finalize tool call if any
                                        if let Some(pos) = current_tool_calls
                                            .iter()
                                            .position(|(i, _, _)| *i == index)
                                        {
                                            let (_, id, json) =
                                                current_tool_calls.remove(pos);
                                            let chunk = ChatStreamChunk {
                                                delta: None,
                                                tool_calls: vec![ToolCall {
                                                    id,
                                                    name: String::new(),
                                                    arguments: json,
                                                }],
                                                usage: None,
                                                reasoning_content: None,
                                                finish_reason: None,
                                            };
                                            if tx.send(Ok(chunk)).await.is_err() {
                                                return;
                                            }
                                        }
                                    }
                                    AnthropicStreamEvent::MessageDelta { delta, usage } => {
                                        if let Some(u) = usage {
                                            accumulated_usage = Some(TokenUsage {
                                                input_tokens: Some(u.input_tokens),
                                                output_tokens: Some(u.output_tokens),
                                            });
                                        }

                                        if let Some(stop_reason) = delta.stop_reason {
                                            let chunk = ChatStreamChunk {
                                                delta: None,
                                                tool_calls: Vec::new(),
                                                usage: accumulated_usage.clone(),
                                                reasoning_content: None,
                                                finish_reason: Some(stop_reason),
                                            };
                                            if tx.send(Ok(chunk)).await.is_err() {
                                                return;
                                            }
                                        }
                                    }
                                    AnthropicStreamEvent::MessageStop => {
                                        // Stream complete
                                        return;
                                    }
                                    AnthropicStreamEvent::Error { error } => {
                                        let _ = tx
                                            .send(Err(StreamError::Api(error.message)))
                                            .await;
                                        return;
                                    }
                                    AnthropicStreamEvent::Ping => {
                                        // Ignore ping events
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(StreamError::Connection(e.to_string()))).await;
                        return;
                    }
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    async fn embeddings(&self, _request: EmbeddingRequest<'_>) -> anyhow::Result<EmbeddingResponse> {
        anyhow::bail!("Anthropic 不支持 Embeddings API，请使用其他提供商（如 OpenAI）")
    }

    async fn list_models(&self) -> anyhow::Result<Vec<ModelInfo>> {
        // Return static list of Claude models
        // Anthropic doesn't have a /models endpoint
        Ok(Self::get_claude_models())
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_embeddings(&self) -> bool {
        false
    }
}

/// Parse an SSE event from the stream.
fn parse_sse_event(event_data: &str) -> Option<AnthropicStreamEvent> {
    for line in event_data.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            if let Ok(event) = serde_json::from_str::<AnthropicStreamEvent>(data) {
                return Some(event);
            }
        }
    }
    None
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // AnthropicContentBlock Tests
    // ============================================================================

    #[test]
    fn test_content_block_text_serialize() {
        let block = AnthropicContentBlock::Text {
            text: "Hello".to_string(),
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains(r#""type":"text""#));
        assert!(json.contains(r#""text":"Hello""#));
    }

    #[test]
    fn test_content_block_text_deserialize() {
        let json = r#"{"type":"text","text":"Hello"}"#;
        let block: AnthropicContentBlock = serde_json::from_str(json).unwrap();
        match block {
            AnthropicContentBlock::Text { text } => assert_eq!(text, "Hello"),
            _ => panic!("Expected Text variant"),
        }
    }

    #[test]
    fn test_content_block_tool_use_deserialize() {
        let json = r#"{"type":"tool_use","id":"tool_123","name":"get_weather","input":{"city":"Beijing"}}"#;
        let block: AnthropicContentBlock = serde_json::from_str(json).unwrap();
        match block {
            AnthropicContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "tool_123");
                assert_eq!(name, "get_weather");
                assert_eq!(input["city"], "Beijing");
            }
            _ => panic!("Expected ToolUse variant"),
        }
    }

    #[test]
    fn test_content_block_tool_result_serialize() {
        let block = AnthropicContentBlock::ToolResult {
            tool_use_id: "tool_123".to_string(),
            is_error: Some(false),
            content: "Result".to_string(),
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains(r#""type":"tool_result""#));
        assert!(json.contains(r#""tool_use_id":"tool_123""#));
    }

    // ============================================================================
    // AnthropicRequest Tests
    // ============================================================================

    #[test]
    fn test_anthropic_request_serialize() {
        let request = AnthropicRequest {
            model: "claude-3-5-sonnet-20241022".to_string(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: vec![AnthropicContentBlock::Text {
                    text: "Hello".to_string(),
                }],
            }],
            max_tokens: 1024,
            system: Some("You are helpful.".to_string()),
            tools: None,
            temperature: Some(0.7),
            stream: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""model":"claude-3-5-sonnet-20241022""#));
        assert!(json.contains(r#""max_tokens":1024"#));
        assert!(json.contains(r#""system":"You are helpful.""#));
        assert!(json.contains(r#""temperature":0.7"#));
    }

    #[test]
    fn test_anthropic_request_skip_optional_fields() {
        let request = AnthropicRequest {
            model: "claude-3-haiku-20240307".to_string(),
            messages: vec![],
            max_tokens: 4096,
            system: None,
            tools: None,
            temperature: None,
            stream: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(!json.contains("system"));
        assert!(!json.contains("tools"));
        assert!(!json.contains("temperature"));
    }

    // ============================================================================
    // AnthropicResponse Tests
    // ============================================================================

    #[test]
    fn test_anthropic_response_deserialize() {
        let json = r#"{
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "Hello!"}],
            "model": "claude-3-5-sonnet-20241022",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        }"#;
        let response: AnthropicResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "msg_123");
        assert_eq!(response.model, "claude-3-5-sonnet-20241022");
        assert_eq!(response.stop_reason, Some("end_turn".to_string()));
        assert_eq!(response.usage.input_tokens, 10);
    }

    #[test]
    fn test_anthropic_response_with_tool_use() {
        let json = r#"{
            "id": "msg_456",
            "type": "message",
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Let me check."},
                {"type": "tool_use", "id": "tool_1", "name": "search", "input": {"query": "test"}}
            ],
            "model": "claude-3-opus-20240229",
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 20, "output_tokens": 15}
        }"#;
        let response: AnthropicResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.content.len(), 2);
        assert_eq!(response.stop_reason, Some("tool_use".to_string()));
    }

    // ============================================================================
    // AnthropicStreamEvent Tests
    // ============================================================================

    #[test]
    fn test_stream_event_message_start() {
        let json = r#"{
            "type": "message_start",
            "message": {
                "id": "msg_123",
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": "claude-3-5-sonnet-20241022",
                "usage": {"input_tokens": 10, "output_tokens": 0}
            }
        }"#;
        let event: AnthropicStreamEvent = serde_json::from_str(json).unwrap();
        match event {
            AnthropicStreamEvent::MessageStart { message } => {
                assert_eq!(message.id, "msg_123");
            }
            _ => panic!("Expected MessageStart variant"),
        }
    }

    #[test]
    fn test_stream_event_content_block_delta() {
        let json = r#"{
            "type": "content_block_delta",
            "index": 0,
            "delta": {"type": "text_delta", "text": "Hello"}
        }"#;
        let event: AnthropicStreamEvent = serde_json::from_str(json).unwrap();
        match event {
            AnthropicStreamEvent::ContentBlockDelta { index, delta } => {
                assert_eq!(index, 0);
                match delta {
                    AnthropicDelta::TextDelta { text } => assert_eq!(text, "Hello"),
                    _ => panic!("Expected TextDelta"),
                }
            }
            _ => panic!("Expected ContentBlockDelta variant"),
        }
    }

    #[test]
    fn test_stream_event_message_delta() {
        let json = r#"{
            "type": "message_delta",
            "delta": {"stop_reason": "end_turn"},
            "usage": {"input_tokens": 10, "output_tokens": 20}
        }"#;
        let event: AnthropicStreamEvent = serde_json::from_str(json).unwrap();
        match event {
            AnthropicStreamEvent::MessageDelta { delta, usage } => {
                assert_eq!(delta.stop_reason, Some("end_turn".to_string()));
                assert!(usage.is_some());
            }
            _ => panic!("Expected MessageDelta variant"),
        }
    }

    #[test]
    fn test_stream_event_message_stop() {
        let json = r#"{"type": "message_stop"}"#;
        let event: AnthropicStreamEvent = serde_json::from_str(json).unwrap();
        matches!(event, AnthropicStreamEvent::MessageStop);
    }

    #[test]
    fn test_stream_event_error() {
        let json = r#"{
            "type": "error",
            "error": {"type": "overloaded_error", "message": "Service overloaded"}
        }"#;
        let event: AnthropicStreamEvent = serde_json::from_str(json).unwrap();
        match event {
            AnthropicStreamEvent::Error { error } => {
                assert_eq!(error.error_type, "overloaded_error");
                assert_eq!(error.message, "Service overloaded");
            }
            _ => panic!("Expected Error variant"),
        }
    }

    // ============================================================================
    // AnthropicProvider Tests
    // ============================================================================

    #[test]
    fn test_provider_new() {
        let provider = AnthropicProvider::new(
            None,
            Some("test-key"),
            "claude-3-5-sonnet-20241022",
            0.7,
            Some(2048),
        );
        assert_eq!(provider.name(), "anthropic");
        assert_eq!(provider.model, "claude-3-5-sonnet-20241022");
        assert_eq!(provider.temperature, 0.7);
        assert_eq!(provider.max_tokens, 2048);
    }

    #[test]
    fn test_provider_default_max_tokens() {
        let provider = AnthropicProvider::new(None, None, "claude-3-haiku", 0.5, None);
        assert_eq!(provider.max_tokens, 4096);
    }

    #[test]
    fn test_provider_custom_base_url() {
        let provider = AnthropicProvider::new(
            Some("https://custom.anthropic.com/"),
            None,
            "claude-3-opus",
            1.0,
            None,
        );
        assert_eq!(provider.base_url, "https://custom.anthropic.com");
    }

    #[test]
    fn test_provider_supports_flags() {
        let provider = AnthropicProvider::new(None, None, "claude-3-sonnet", 0.5, None);
        assert!(provider.supports_streaming());
        assert!(!provider.supports_embeddings());
    }

    // ============================================================================
    // Message Conversion Tests
    // ============================================================================

    #[test]
    fn test_convert_messages_with_system() {
        let messages = vec![
            ChatMessage::system("You are helpful."),
            ChatMessage::user("Hello"),
        ];
        let (system, anthropic_messages) = AnthropicProvider::convert_messages(&messages);

        assert_eq!(system, Some("You are helpful.".to_string()));
        assert_eq!(anthropic_messages.len(), 1);
        assert_eq!(anthropic_messages[0].role, "user");
    }

    #[test]
    fn test_convert_messages_without_system() {
        let messages = vec![
            ChatMessage::user("Hello"),
            ChatMessage::assistant("Hi there!"),
        ];
        let (system, anthropic_messages) = AnthropicProvider::convert_messages(&messages);

        assert!(system.is_none());
        assert_eq!(anthropic_messages.len(), 2);
    }

    // ============================================================================
    // Response Parsing Tests
    // ============================================================================

    #[test]
    fn test_parse_response_text_only() {
        let response = AnthropicResponse {
            id: "msg_123".to_string(),
            response_type: "message".to_string(),
            role: "assistant".to_string(),
            content: vec![AnthropicContentBlock::Text {
                text: "Hello!".to_string(),
            }],
            model: "claude-3-5-sonnet-20241022".to_string(),
            stop_reason: Some("end_turn".to_string()),
            stop_sequence: None,
            usage: AnthropicUsage {
                input_tokens: 10,
                output_tokens: 5,
            },
        };

        let chat_response = AnthropicProvider::parse_response(response);
        assert_eq!(chat_response.text, Some("Hello!".to_string()));
        assert!(chat_response.tool_calls.is_empty());
        assert!(chat_response.usage.is_some());
        assert_eq!(chat_response.usage.unwrap().input_tokens, Some(10));
    }

    #[test]
    fn test_parse_response_with_tool_calls() {
        let response = AnthropicResponse {
            id: "msg_456".to_string(),
            response_type: "message".to_string(),
            role: "assistant".to_string(),
            content: vec![
                AnthropicContentBlock::Text {
                    text: "Let me check.".to_string(),
                },
                AnthropicContentBlock::ToolUse {
                    id: "tool_1".to_string(),
                    name: "search".to_string(),
                    input: serde_json::json!({"query": "test"}),
                },
            ],
            model: "claude-3-opus-20240229".to_string(),
            stop_reason: Some("tool_use".to_string()),
            stop_sequence: None,
            usage: AnthropicUsage {
                input_tokens: 20,
                output_tokens: 15,
            },
        };

        let chat_response = AnthropicProvider::parse_response(response);
        assert_eq!(chat_response.text, Some("Let me check.".to_string()));
        assert_eq!(chat_response.tool_calls.len(), 1);
        assert_eq!(chat_response.tool_calls[0].id, "tool_1");
        assert_eq!(chat_response.tool_calls[0].name, "search");
    }

    // ============================================================================
    // Model Capabilities Tests
    // ============================================================================

    #[test]
    fn test_detect_model_capabilities_opus() {
        let info = AnthropicProvider::detect_model_capabilities("claude-3-opus-20240229");
        assert_eq!(info.name, "Claude 3 Opus");
        assert_eq!(info.context_length, Some(200_000));
        assert!(info.supports_tools);
        assert!(info.supports_vision);
        assert!(info.supports_streaming);
    }

    #[test]
    fn test_detect_model_capabilities_sonnet_35() {
        let info = AnthropicProvider::detect_model_capabilities("claude-3-5-sonnet-20241022");
        assert_eq!(info.name, "Claude 3.5 Sonnet");
        assert_eq!(info.context_length, Some(200_000));
        assert!(info.supports_tools);
        assert!(info.supports_vision);
    }

    #[test]
    fn test_detect_model_capabilities_haiku_35() {
        let info = AnthropicProvider::detect_model_capabilities("claude-3-5-haiku-20241022");
        assert_eq!(info.name, "Claude 3.5 Haiku");
        assert_eq!(info.context_length, Some(200_000));
        assert!(info.supports_tools);
        assert!(!info.supports_vision); // 3.5 Haiku doesn't support vision
    }

    #[test]
    fn test_detect_model_capabilities_haiku_3() {
        let info = AnthropicProvider::detect_model_capabilities("claude-3-haiku-20240307");
        assert_eq!(info.name, "Claude 3 Haiku");
        assert!(info.supports_vision);
    }

    // ============================================================================
    // List Models Tests
    // ============================================================================

    #[tokio::test]
    async fn test_list_models() {
        let provider = AnthropicProvider::new(None, None, "claude-3-sonnet", 0.5, None);
        let models = provider.list_models().await.unwrap();

        assert_eq!(models.len(), 5);
        assert!(models.iter().any(|m| m.id == "claude-3-opus-20240229"));
        assert!(models.iter().any(|m| m.id == "claude-3-5-sonnet-20241022"));
    }

    // ============================================================================
    // Error Handling Tests
    // ============================================================================

    #[test]
    fn test_anthropic_error_response_deserialize() {
        let json = r#"{
            "type": "error",
            "error": {
                "type": "overloaded_error",
                "message": "Overloaded"
            }
        }"#;
        let error: AnthropicErrorResponse = serde_json::from_str(json).unwrap();
        assert!(error.error.is_some());
        let detail = error.error.unwrap();
        assert_eq!(detail.error_type, "overloaded_error");
        assert_eq!(detail.message, "Overloaded");
    }

    #[test]
    fn test_anthropic_error_response_minimal() {
        let json = r#"{
            "type": "invalid_request_error"
        }"#;
        let error: AnthropicErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(error.error_type, Some("invalid_request_error".to_string()));
        assert!(error.error.is_none());
    }

    // ============================================================================
    // Tool Conversion Tests
    // ============================================================================

    #[test]
    fn test_convert_tools() {
        let tools = vec![
            ToolSpec {
                name: "get_weather".to_string(),
                description: "Get weather info".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "location": {"type": "string"}
                    }
                }),
            },
        ];

        let anthropic_tools = AnthropicProvider::convert_tools(&tools);
        assert_eq!(anthropic_tools.len(), 1);
        assert_eq!(anthropic_tools[0].name, "get_weather");
        assert!(anthropic_tools[0].description.is_some());
    }

    // ============================================================================
    // Health Check Tests
    // ============================================================================

    #[tokio::test]
    async fn test_health_check_no_credential() {
        let provider = AnthropicProvider::new(None, None, "claude-3-sonnet", 0.5, None);
        assert!(!provider.health_check().await);
    }

    #[tokio::test]
    async fn test_health_check_with_credential() {
        let provider = AnthropicProvider::new(
            None,
            Some("test-key"),
            "claude-3-sonnet",
            0.5,
            None,
        );
        assert!(provider.health_check().await);
    }

    // ============================================================================
    // Embeddings Error Test
    // ============================================================================

    #[tokio::test]
    async fn test_embeddings_not_supported() {
        let provider = AnthropicProvider::new(
            None,
            Some("test-key"),
            "claude-3-sonnet",
            0.5,
            None,
        );
        let request = EmbeddingRequest::new("test");
        let result = provider.embeddings(request).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("不支持 Embeddings"));
    }

    // ============================================================================
    // SSE Parsing Tests
    // ============================================================================

    #[test]
    fn test_parse_sse_event_valid() {
        let event_data = "data: {\"type\":\"message_stop\"}";
        let event = parse_sse_event(event_data);
        assert!(event.is_some());
        matches!(event.unwrap(), AnthropicStreamEvent::MessageStop);
    }

    #[test]
    fn test_parse_sse_event_text_delta() {
        let event_data = "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}";
        let event = parse_sse_event(event_data);
        assert!(event.is_some());
    }

    #[test]
    fn test_parse_sse_event_empty_data() {
        let event_data = "data: ";
        let event = parse_sse_event(event_data);
        assert!(event.is_none());
    }

    #[test]
    fn test_parse_sse_event_done() {
        let event_data = "data: [DONE]";
        let event = parse_sse_event(event_data);
        assert!(event.is_none());
    }
}