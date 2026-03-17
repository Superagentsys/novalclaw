use crate::providers::traits::{
    ChatMessage, ChatRequest as ProviderChatRequest, ChatResponse as ProviderChatResponse,
    ChatStream, ChatStreamChunk, EmbeddingRequest, EmbeddingResponse, ModelInfo, Provider,
    StreamError, TokenUsage, ToolCall as ProviderToolCall,
};
use crate::tools::ToolSpec;
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::{Client, Response, StatusCode};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<NativeToolSpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize)]
struct NativeMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
struct NativeToolCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    function: NativeFunctionCall,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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
    #[serde(default)]
    total_tokens: Option<u64>,
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

// ============================================================================
// Streaming Support Types
// ============================================================================

/// Response structure for streaming chat completions
#[derive(Debug, Deserialize)]
struct NativeStreamResponse {
    #[serde(default)]
    choices: Vec<NativeStreamChoice>,
    #[serde(default)]
    usage: Option<UsageInfo>,
}

/// A single choice in a streaming response
#[derive(Debug, Deserialize)]
struct NativeStreamChoice {
    #[serde(default)]
    delta: NativeStreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
    #[serde(default)]
    index: u32,
}

/// Delta content in a streaming chunk
#[derive(Debug, Deserialize, Default)]
struct NativeStreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<NativeToolCall>>,
}

// ============================================================================
// Embeddings Support Types
// ============================================================================

/// Request for embeddings API
#[derive(Debug, Serialize)]
struct NativeEmbeddingRequest {
    model: String,
    input: String,
}

/// Response from embeddings API
#[derive(Debug, Deserialize)]
struct NativeEmbeddingResponse {
    data: Vec<NativeEmbeddingData>,
    model: String,
    usage: Option<UsageInfo>,
}

/// Single embedding data
#[derive(Debug, Deserialize)]
struct NativeEmbeddingData {
    embedding: Vec<f32>,
    index: u32,
}

// ============================================================================
// Models API Types
// ============================================================================

/// Response from models list API
#[derive(Debug, Deserialize)]
struct NativeModelsResponse {
    data: Vec<NativeModelData>,
}

/// Single model data
#[derive(Debug, Deserialize)]
struct NativeModelData {
    id: String,
    #[serde(default)]
    owned_by: Option<String>,
    #[serde(default)]
    created: Option<u64>,
}

// ============================================================================
// Error Handling
// ============================================================================

/// OpenAI API error response
#[derive(Debug, Deserialize)]
struct NativeErrorResponse {
    error: Option<NativeErrorDetail>,
}

#[derive(Debug, Deserialize)]
struct NativeErrorDetail {
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    code: Option<String>,
}

async fn api_error(provider_name: &str, response: Response) -> anyhow::Error {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();

    // Try to parse structured error
    if let Ok(error_response) = serde_json::from_str::<NativeErrorResponse>(&text) {
        if let Some(error) = error_response.error {
            let message = error.message.unwrap_or_else(|| "Unknown error".to_string());
            return anyhow::anyhow!("{} API 错误 ({}): {}", provider_name, status, message);
        }
    }

    anyhow::anyhow!("{} API 错误 ({}): {}", provider_name, status, text)
}

/// Parse rate limit retry-after header
fn parse_retry_after(response: &Response) -> Option<u64> {
    response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
}

// ============================================================================
// Provider Implementation
// ============================================================================

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
            .timeout(std::time::Duration::from_secs(60))
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

    fn convert_messages(messages: &[ChatMessage]) -> Vec<NativeMessage> {
        messages
            .iter()
            .map(|m| {
                if m.role == "assistant" {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&m.content) {
                        if let Some(tool_calls_value) = value.get("tool_calls") {
                            if let Ok(parsed_calls) =
                                serde_json::from_value::<Vec<ProviderToolCall>>(
                                    tool_calls_value.clone(),
                                )
                            {
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
                                return NativeMessage {
                                    role: "assistant".to_string(),
                                    content,
                                    tool_call_id: None,
                                    tool_calls: Some(tool_calls),
                                    reasoning_content,
                                };
                            }
                        }
                    }
                }

                if m.role == "tool" {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&m.content) {
                        let tool_call_id = value
                            .get("tool_call_id")
                            .and_then(serde_json::Value::as_str)
                            .map(ToString::to_string);
                        let content = value
                            .get("content")
                            .and_then(serde_json::Value::as_str)
                            .map(ToString::to_string);
                        return NativeMessage {
                            role: "tool".to_string(),
                            content,
                            tool_call_id,
                            tool_calls: None,
                            reasoning_content: None,
                        };
                    }
                }

                NativeMessage {
                    role: m.role.clone(),
                    content: Some(m.content.clone()),
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_content: None,
                }
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

    fn request_temperature(&self) -> Option<f64> {
        if model_requires_default_temperature(&self.model) {
            return None;
        }

        Some(self.temperature)
    }

    /// Get the default embedding model for this provider
    fn default_embedding_model(&self) -> &str {
        "text-embedding-3-small"
    }

    /// Parse SSE line into a stream chunk
    fn parse_sse_line(line: &str) -> Option<Result<ChatStreamChunk, StreamError>> {
        let line = line.trim();

        // Skip empty lines
        if line.is_empty() {
            return None;
        }

        // Handle [DONE] marker
        if line == "data: [DONE]" {
            return Some(Ok(ChatStreamChunk::finish("stop")));
        }

        // Parse data line
        if let Some(json_str) = line.strip_prefix("data: ") {
            match serde_json::from_str::<NativeStreamResponse>(json_str) {
                Ok(response) => {
                    if let Some(choice) = response.choices.first() {
                        let delta = &choice.delta;

                        // Build the chunk
                        let chunk = ChatStreamChunk {
                            delta: delta.content.clone(),
                            tool_calls: delta.tool_calls.clone().unwrap_or_default().into_iter().map(|tc| {
                                ProviderToolCall {
                                    id: tc.id.unwrap_or_default(),
                                    name: tc.function.name,
                                    arguments: tc.function.arguments,
                                }
                            }).collect(),
                            usage: response.usage.as_ref().map(|u| TokenUsage {
                                input_tokens: u.prompt_tokens,
                                output_tokens: u.completion_tokens,
                            }),
                            reasoning_content: delta.reasoning_content.clone(),
                            finish_reason: choice.finish_reason.clone(),
                        };

                        return Some(Ok(chunk));
                    }
                }
                Err(e) => {
                    return Some(Err(StreamError::Parse(format!("JSON 解析失败: {}", e))));
                }
            }
        }

        None
    }

    /// Detect model capabilities from model ID
    fn detect_model_capabilities(model_id: &str) -> ModelInfo {
        let id = model_id.to_lowercase();
        let mut info = ModelInfo::new(model_id);

        // Detect tools support
        if id.contains("gpt-4") || id.contains("gpt-3.5") || id.contains("chatgpt-4") {
            info = info.with_tools();
        }

        // Detect vision support
        if id.contains("gpt-4o") || id.contains("gpt-4-turbo") || id.contains("chatgpt-4o") {
            info = info.with_vision();
        }

        // Set context length based on model
        if id.starts_with("gpt-4o") || id.starts_with("gpt-4-turbo") || id.starts_with("chatgpt-4o") {
            info = info.with_context_length(128000);
        } else if id.starts_with("gpt-4-32k") {
            info = info.with_context_length(32768);
        } else if id.starts_with("gpt-4") {
            info = info.with_context_length(8192);
        } else if id.starts_with("gpt-3.5-turbo-16k") {
            info = info.with_context_length(16384);
        } else if id.starts_with("gpt-3.5") {
            info = info.with_context_length(4096);
        } else if id.starts_with("o1") || id.starts_with("o3") {
            info = info.with_context_length(200000);
        }

        info
    }
}

fn model_requires_default_temperature(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    normalized.starts_with("gpt-5")
        || normalized.starts_with("o1")
        || normalized.starts_with("o3")
        || normalized.starts_with("o4")
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

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_embeddings(&self) -> bool {
        true
    }

    async fn chat(&self, request: ProviderChatRequest<'_>) -> anyhow::Result<ProviderChatResponse> {
        let credential = self.credential.as_ref().ok_or_else(|| {
            anyhow::anyhow!("OpenAI API key 未设置。请设置 OPENAI_API_KEY 或配置 api_key。")
        })?;

        let tools = Self::convert_tools(request.tools);
        let native_request = NativeChatRequest {
            model: self.model.clone(),
            messages: Self::convert_messages(request.messages),
            temperature: self.request_temperature(),
            max_tokens: self.max_tokens,
            tool_choice: tools.as_ref().map(|_| "auto".to_string()),
            tools,
            stream: None,
        };

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {credential}"))
            .json(&native_request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    anyhow::anyhow!(
                        "请求超时：调用 {} 超时（60s），请检查网络连通性或 API 服务可用性",
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

        let status = response.status();
        if !status.is_success() {
            // Handle specific error codes
            if status == StatusCode::UNAUTHORIZED {
                return Err(anyhow::anyhow!(
                    "认证失败：API key 无效或已过期，请检查 API key 配置"
                ));
            }
            if status == StatusCode::TOO_MANY_REQUESTS {
                let retry_after = parse_retry_after(&response);
                let msg = match retry_after {
                    Some(secs) => format!("速率限制：请求过于频繁，请在 {} 秒后重试", secs),
                    None => "速率限制：请求过于频繁，请稍后重试".to_string(),
                };
                return Err(anyhow::anyhow!(msg));
            }
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
            .ok_or_else(|| anyhow::anyhow!("OpenAI 未返回响应内容"))?;
        let mut result = Self::parse_native_response(message);
        result.usage = usage;
        Ok(result)
    }

    async fn chat_stream(
        &self,
        request: ProviderChatRequest<'_>,
    ) -> anyhow::Result<ChatStream> {
        let credential = self
            .credential
            .clone()
            .ok_or_else(|| anyhow::anyhow!("OpenAI API key 未设置。请设置 OPENAI_API_KEY 或配置 api_key。"))?;

        let tools = Self::convert_tools(request.tools);
        let native_request = NativeChatRequest {
            model: self.model.clone(),
            messages: Self::convert_messages(request.messages),
            temperature: self.request_temperature(),
            max_tokens: self.max_tokens,
            tool_choice: tools.as_ref().map(|_| "auto".to_string()),
            tools,
            stream: Some(true),
        };

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {credential}"))
            .json(&native_request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    anyhow::anyhow!(
                        "请求超时：调用 {} 超时（60s），请检查网络连通性",
                        self.base_url
                    )
                } else if e.is_connect() {
                    anyhow::anyhow!(
                        "连接失败：无法连接到 {}，请检查 Base URL 配置",
                        self.base_url
                    )
                } else {
                    anyhow::anyhow!("网络请求失败：{}", e)
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            if status == StatusCode::UNAUTHORIZED {
                return Err(anyhow::anyhow!(
                    "认证失败：API key 无效或已过期"
                ));
            }
            if status == StatusCode::TOO_MANY_REQUESTS {
                return Err(anyhow::anyhow!(
                    "速率限制：请求过于频繁，请稍后重试"
                ));
            }
            return Err(api_error("OpenAI", response).await);
        }

        // Create a channel for streaming chunks
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<ChatStreamChunk, StreamError>>(32);

        // Spawn a task to process the SSE stream
        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        // Convert bytes to string and append to buffer
                        let chunk_str = String::from_utf8_lossy(&bytes);
                        buffer.push_str(&chunk_str);

                        // Process complete lines
                        while let Some(newline_pos) = buffer.find('\n') {
                            let line = buffer[..newline_pos].to_string();
                            buffer = buffer[newline_pos + 1..].to_string();

                            if let Some(chunk_result) = Self::parse_sse_line(&line) {
                                if tx.send(chunk_result).await.is_err() {
                                    // Receiver dropped, stop processing
                                    return;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Err(StreamError::Connection(e.to_string())))
                            .await;
                        return;
                    }
                }
            }

            // Process any remaining data in buffer
            if !buffer.trim().is_empty() {
                if let Some(chunk_result) = Self::parse_sse_line(&buffer) {
                    let _ = tx.send(chunk_result).await;
                }
            }
        });

        // Convert receiver to stream
        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Box::pin(stream))
    }

    async fn embeddings(
        &self,
        request: EmbeddingRequest<'_>,
    ) -> anyhow::Result<EmbeddingResponse> {
        let credential = self.credential.as_ref().ok_or_else(|| {
            anyhow::anyhow!("OpenAI API key 未设置。请设置 OPENAI_API_KEY 或配置 api_key。")
        })?;

        let model = request
            .model
            .unwrap_or(self.default_embedding_model())
            .to_string();

        let native_request = NativeEmbeddingRequest {
            model: model.clone(),
            input: request.text.to_string(),
        };

        let response = self
            .client
            .post(format!("{}/embeddings", self.base_url))
            .header("Authorization", format!("Bearer {credential}"))
            .json(&native_request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    anyhow::anyhow!("请求超时：调用 embeddings API 超时")
                } else if e.is_connect() {
                    anyhow::anyhow!("连接失败：无法连接到 embeddings API")
                } else {
                    anyhow::anyhow!("网络请求失败：{}", e)
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            if status == StatusCode::UNAUTHORIZED {
                return Err(anyhow::anyhow!("认证失败：API key 无效"));
            }
            if status == StatusCode::TOO_MANY_REQUESTS {
                return Err(anyhow::anyhow!("速率限制：请稍后重试"));
            }
            return Err(api_error("OpenAI Embeddings", response).await);
        }

        let native_response: NativeEmbeddingResponse = response.json().await?;

        let embedding_data = native_response
            .data
            .first()
            .ok_or_else(|| anyhow::anyhow!("Embedding 响应中未找到数据"))?;

        Ok(EmbeddingResponse {
            embedding: embedding_data.embedding.clone(),
            model: native_response.model,
            usage: native_response.usage.map(|u| TokenUsage {
                input_tokens: u.prompt_tokens,
                output_tokens: None,
            }),
        })
    }

    async fn list_models(&self) -> anyhow::Result<Vec<ModelInfo>> {
        let credential = self.credential.as_ref().ok_or_else(|| {
            anyhow::anyhow!("OpenAI API key 未设置。请设置 OPENAI_API_KEY 或配置 api_key。")
        })?;

        let response = self
            .client
            .get(format!("{}/models", self.base_url))
            .header("Authorization", format!("Bearer {credential}"))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    anyhow::anyhow!("请求超时：调用 models API 超时")
                } else if e.is_connect() {
                    anyhow::anyhow!("连接失败：无法连接到 models API")
                } else {
                    anyhow::anyhow!("网络请求失败：{}", e)
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            if status == StatusCode::UNAUTHORIZED {
                return Err(anyhow::anyhow!("认证失败：API key 无效"));
            }
            return Err(api_error("OpenAI Models", response).await);
        }

        let native_response: NativeModelsResponse = response.json().await?;

        // Filter and transform models
        let models: Vec<ModelInfo> = native_response
            .data
            .into_iter()
            .filter(|m| {
                // Filter to only include chat models
                let id = m.id.to_lowercase();
                id.starts_with("gpt-")
                    || id.starts_with("o1-")
                    || id.starts_with("o3-")
                    || id.starts_with("chatgpt-")
            })
            .map(|m| Self::detect_model_capabilities(&m.id))
            .collect();

        Ok(models)
    }

    async fn health_check(&self) -> bool {
        if let Some(credential) = self.credential.as_ref() {
            let response = self
                .client
                .get(format!("{}/models", self.base_url))
                .header("Authorization", format!("Bearer {credential}"))
                .send()
                .await;
            return response.map(|r| r.status().is_success()).unwrap_or(false);
        }
        true
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // Temperature Tests
    // ============================================================================

    #[test]
    fn fixed_temperature_models_do_not_send_temperature() {
        assert!(model_requires_default_temperature("gpt-5"));
        assert!(model_requires_default_temperature("gpt-5-mini"));
        assert!(model_requires_default_temperature("o3"));
        assert!(model_requires_default_temperature("o4-mini"));
    }

    #[test]
    fn regular_models_keep_temperature_control() {
        assert!(!model_requires_default_temperature("gpt-4o"));
        assert!(!model_requires_default_temperature("claude-3-5-sonnet-latest"));
        assert!(!model_requires_default_temperature("deepseek-chat"));
    }

    // ============================================================================
    // Model Capability Detection Tests
    // ============================================================================

    #[test]
    fn test_detect_gpt4o_capabilities() {
        let info = OpenAiProvider::detect_model_capabilities("gpt-4o-mini");
        assert!(info.supports_tools);
        assert!(info.supports_vision);
        assert_eq!(info.context_length, Some(128000));
    }

    #[test]
    fn test_detect_gpt4_turbo_capabilities() {
        let info = OpenAiProvider::detect_model_capabilities("gpt-4-turbo");
        assert!(info.supports_tools);
        assert!(info.supports_vision);
        assert_eq!(info.context_length, Some(128000));
    }

    #[test]
    fn test_detect_gpt4_capabilities() {
        let info = OpenAiProvider::detect_model_capabilities("gpt-4");
        assert!(info.supports_tools);
        assert!(!info.supports_vision);
        assert_eq!(info.context_length, Some(8192));
    }

    #[test]
    fn test_detect_gpt4_32k_capabilities() {
        let info = OpenAiProvider::detect_model_capabilities("gpt-4-32k");
        assert!(info.supports_tools);
        assert!(!info.supports_vision);
        assert_eq!(info.context_length, Some(32768));
    }

    #[test]
    fn test_detect_gpt35_capabilities() {
        let info = OpenAiProvider::detect_model_capabilities("gpt-3.5-turbo");
        assert!(info.supports_tools);
        assert!(!info.supports_vision);
        assert_eq!(info.context_length, Some(4096));
    }

    #[test]
    fn test_detect_gpt35_16k_capabilities() {
        let info = OpenAiProvider::detect_model_capabilities("gpt-3.5-turbo-16k");
        assert!(info.supports_tools);
        assert!(!info.supports_vision);
        assert_eq!(info.context_length, Some(16384));
    }

    #[test]
    fn test_detect_o1_capabilities() {
        let info = OpenAiProvider::detect_model_capabilities("o1-preview");
        assert!(!info.supports_tools); // o1 models don't support tools by default
        assert_eq!(info.context_length, Some(200000));
    }

    #[test]
    fn test_detect_chatgpt_4o_capabilities() {
        let info = OpenAiProvider::detect_model_capabilities("chatgpt-4o-latest");
        assert!(info.supports_tools);
        assert!(info.supports_vision);
        assert_eq!(info.context_length, Some(128000));
    }

    // ============================================================================
    // SSE Parsing Tests
    // ============================================================================

    #[test]
    fn test_parse_sse_line_text_delta() {
        let line = r#"data: {"choices":[{"delta":{"content":"Hello"},"index":0}]}"#;
        let result = OpenAiProvider::parse_sse_line(line);

        assert!(result.is_some());
        let chunk = result.unwrap().unwrap();
        assert_eq!(chunk.delta, Some("Hello".to_string()));
        assert!(chunk.tool_calls.is_empty());
        assert!(!chunk.is_finished());
    }

    #[test]
    fn test_parse_sse_line_finish() {
        let line = r#"data: {"choices":[{"delta":{},"finish_reason":"stop","index":0}]}"#;
        let result = OpenAiProvider::parse_sse_line(line);

        assert!(result.is_some());
        let chunk = result.unwrap().unwrap();
        assert!(chunk.delta.is_none());
        assert_eq!(chunk.finish_reason, Some("stop".to_string()));
        assert!(chunk.is_finished());
    }

    #[test]
    fn test_parse_sse_line_done_marker() {
        let line = "data: [DONE]";
        let result = OpenAiProvider::parse_sse_line(line);

        assert!(result.is_some());
        let chunk = result.unwrap().unwrap();
        assert!(chunk.is_finished());
        assert_eq!(chunk.finish_reason, Some("stop".to_string()));
    }

    #[test]
    fn test_parse_sse_line_empty() {
        let result = OpenAiProvider::parse_sse_line("");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_sse_line_with_reasoning_content() {
        let line = r#"data: {"choices":[{"delta":{"reasoning_content":"Thinking..."},"index":0}]}"#;
        let result = OpenAiProvider::parse_sse_line(line);

        assert!(result.is_some());
        let chunk = result.unwrap().unwrap();
        assert_eq!(chunk.reasoning_content, Some("Thinking...".to_string()));
    }

    #[test]
    fn test_parse_sse_line_with_tool_calls() {
        let line = r#"data: {"choices":[{"delta":{"tool_calls":[{"id":"call_123","function":{"name":"get_weather","arguments":"{}"}}]},"index":0}]}"#;
        let result = OpenAiProvider::parse_sse_line(line);

        assert!(result.is_some());
        let chunk = result.unwrap().unwrap();
        assert_eq!(chunk.tool_calls.len(), 1);
        assert_eq!(chunk.tool_calls[0].name, "get_weather");
    }

    #[test]
    fn test_parse_sse_line_with_usage() {
        let line = r#"data: {"choices":[{"delta":{},"finish_reason":"stop","index":0}],"usage":{"prompt_tokens":10,"completion_tokens":20}}"#;
        let result = OpenAiProvider::parse_sse_line(line);

        assert!(result.is_some());
        let chunk = result.unwrap().unwrap();
        assert!(chunk.usage.is_some());
        let usage = chunk.usage.unwrap();
        assert_eq!(usage.input_tokens, Some(10));
        assert_eq!(usage.output_tokens, Some(20));
    }

    #[test]
    fn test_parse_sse_invalid_json() {
        let line = "data: {invalid json}";
        let result = OpenAiProvider::parse_sse_line(line);

        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    // ============================================================================
    // Provider Support Flags Tests
    // ============================================================================

    #[test]
    fn test_openai_provider_supports_streaming() {
        let provider = OpenAiProvider::new(None, Some("test-key"), "gpt-4o", 0.7, None);
        assert!(provider.supports_streaming());
    }

    #[test]
    fn test_openai_provider_supports_embeddings() {
        let provider = OpenAiProvider::new(None, Some("test-key"), "gpt-4o", 0.7, None);
        assert!(provider.supports_embeddings());
    }

    #[test]
    fn test_openai_provider_name() {
        let provider = OpenAiProvider::new(None, Some("test-key"), "gpt-4o", 0.7, None);
        assert_eq!(provider.name(), "openai");
    }

    #[test]
    fn test_openai_provider_default_embedding_model() {
        let provider = OpenAiProvider::new(None, Some("test-key"), "gpt-4o", 0.7, None);
        assert_eq!(provider.default_embedding_model(), "text-embedding-3-small");
    }

    // ============================================================================
    // Constructor Tests
    // ============================================================================

    #[test]
    fn test_openai_provider_new_with_defaults() {
        let provider = OpenAiProvider::new(None, None, "gpt-4o-mini", 0.7, None);
        assert_eq!(provider.base_url, "https://api.openai.com/v1");
        assert_eq!(provider.model, "gpt-4o-mini");
        assert!(provider.credential.is_none());
        assert_eq!(provider.temperature, 0.7);
        assert!(provider.max_tokens.is_none());
    }

    #[test]
    fn test_openai_provider_new_with_custom_url() {
        let provider = OpenAiProvider::new(
            Some("https://custom.api.com/v1/"),
            None,
            "gpt-4o",
            0.5,
            Some(1000),
        );
        assert_eq!(provider.base_url, "https://custom.api.com/v1");
        assert_eq!(provider.temperature, 0.5);
        assert_eq!(provider.max_tokens, Some(1000));
    }

    #[test]
    fn test_openai_provider_max_tokens_zero_filtered() {
        let provider = OpenAiProvider::new(None, None, "gpt-4o", 0.7, Some(0));
        assert!(provider.max_tokens.is_none());
    }

    // ============================================================================
    // Native Types Tests
    // ============================================================================

    #[test]
    fn test_native_chat_request_serialization() {
        let request = NativeChatRequest {
            model: "gpt-4o".to_string(),
            messages: vec![NativeMessage {
                role: "user".to_string(),
                content: Some("Hello".to_string()),
                tool_call_id: None,
                tool_calls: None,
                reasoning_content: None,
            }],
            temperature: Some(0.7),
            max_tokens: None,
            tools: None,
            tool_choice: None,
            stream: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"model\":\"gpt-4o\""));
        assert!(json.contains("\"temperature\":0.7"));
        assert!(!json.contains("\"max_tokens\""));
        assert!(!json.contains("\"stream\""));
    }

    #[test]
    fn test_native_chat_request_stream_serialization() {
        let request = NativeChatRequest {
            model: "gpt-4o".to_string(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            tools: None,
            tool_choice: None,
            stream: Some(true),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"stream\":true"));
    }

    #[test]
    fn test_native_embedding_request_serialization() {
        let request = NativeEmbeddingRequest {
            model: "text-embedding-3-small".to_string(),
            input: "Hello world".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"model\":\"text-embedding-3-small\""));
        assert!(json.contains("\"input\":\"Hello world\""));
    }

    #[test]
    fn test_native_embedding_response_deserialization() {
        let json = r#"{
            "data": [{"embedding": [0.1, 0.2, 0.3], "index": 0}],
            "model": "text-embedding-3-small",
            "usage": {"prompt_tokens": 5, "total_tokens": 5}
        }"#;

        let response: NativeEmbeddingResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.data.len(), 1);
        assert_eq!(response.model, "text-embedding-3-small");
        assert_eq!(response.data[0].embedding, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn test_native_models_response_deserialization() {
        let json = r#"{
            "data": [
                {"id": "gpt-4o-mini", "owned_by": "system"},
                {"id": "gpt-4-turbo", "owned_by": "system"}
            ]
        }"#;

        let response: NativeModelsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.data.len(), 2);
        assert_eq!(response.data[0].id, "gpt-4o-mini");
    }

    // ============================================================================
    // Message Conversion Tests
    // ============================================================================

    #[test]
    fn test_convert_simple_message() {
        let messages = vec![
            ChatMessage::system("You are helpful"),
            ChatMessage::user("Hello"),
            ChatMessage::assistant("Hi there!"),
        ];

        let native = OpenAiProvider::convert_messages(&messages);

        assert_eq!(native.len(), 3);
        assert_eq!(native[0].role, "system");
        assert_eq!(native[0].content, Some("You are helpful".to_string()));
        assert_eq!(native[1].role, "user");
        assert_eq!(native[2].role, "assistant");
    }

    #[test]
    fn test_convert_tool_result_message() {
        let messages = vec![ChatMessage::tool(
            r#"{"tool_call_id":"call_123","content":"result"}"#,
        )];

        let native = OpenAiProvider::convert_messages(&messages);

        assert_eq!(native[0].role, "tool");
        assert_eq!(native[0].tool_call_id, Some("call_123".to_string()));
        assert_eq!(native[0].content, Some("result".to_string()));
    }

    // ============================================================================
    // Error Response Tests
    // ============================================================================

    #[test]
    fn test_native_error_response_deserialization() {
        let json = r#"{
            "error": {
                "message": "Invalid API key",
                "type": "invalid_request_error",
                "code": "invalid_api_key"
            }
        }"#;

        let response: NativeErrorResponse = serde_json::from_str(json).unwrap();
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.message, Some("Invalid API key".to_string()));
        assert_eq!(error.code, Some("invalid_api_key".to_string()));
    }

    // ============================================================================
    // Mock Provider Tests
    // ============================================================================

    #[test]
    fn test_mock_provider_name() {
        let provider = MockProvider::new("test-mock");
        assert_eq!(provider.name(), "test-mock");
    }

    #[tokio::test]
    async fn test_mock_provider_chat() {
        let provider = MockProvider::new("mock");
        let messages = vec![ChatMessage::user("Hello")];
        let request = ProviderChatRequest {
            messages: &messages,
            tools: None,
        };

        let response = provider.chat(request).await.unwrap();
        assert!(response.text.is_some());
        assert!(response.tool_calls.is_empty());
    }

    #[tokio::test]
    async fn test_mock_provider_health_check() {
        let provider = MockProvider::new("mock");
        assert!(provider.health_check().await);
    }
}