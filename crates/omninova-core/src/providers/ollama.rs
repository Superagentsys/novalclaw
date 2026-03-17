//! Ollama local model provider implementation.
//!
//! This module implements a native Ollama API client for running open-source
//! models locally. It supports chat, streaming, embeddings, and model listing.

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
// Ollama Native Types
// ============================================================================

/// Ollama chat request.
#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OllamaTool>>,
}

/// Ollama message.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

/// Ollama tool call in a message.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct OllamaToolCall {
    function: OllamaFunctionCall,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OllamaFunctionCall {
    name: String,
    arguments: serde_json::Value,
}

/// Ollama model options.
#[derive(Debug, Serialize, Default)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_ctx: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<i32>,
}

/// Ollama tool definition.
#[derive(Debug, Serialize)]
struct OllamaTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OllamaFunctionDef,
}

#[derive(Debug, Serialize)]
struct OllamaFunctionDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/// Ollama chat response.
#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    model: String,
    created_at: String,
    message: OllamaMessage,
    done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    done_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    load_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_eval_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    eval_count: Option<u64>,
}

/// Ollama models list response from /api/tags.
#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

/// Ollama model info.
#[derive(Debug, Deserialize, Clone)]
struct OllamaModel {
    name: String,
    modified_at: String,
    size: u64,
    digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<OllamaModelDetails>,
}

/// Ollama model details.
#[derive(Debug, Deserialize, Clone)]
struct OllamaModelDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameter_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quantization_level: Option<String>,
}

/// Ollama embedding request.
#[derive(Debug, Serialize)]
struct OllamaEmbeddingRequest {
    model: String,
    prompt: String,
}

/// Ollama embedding response.
#[derive(Debug, Deserialize)]
struct OllamaEmbeddingResponse {
    embedding: Vec<f32>,
}

/// Ollama error response.
#[derive(Debug, Deserialize)]
struct OllamaErrorResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

// ============================================================================
// Ollama Provider
// ============================================================================

/// Ollama provider for local model inference.
pub struct OllamaProvider {
    base_url: String,
    model: String,
    temperature: f64,
    client: Client,
}

impl OllamaProvider {
    /// Create a new Ollama provider.
    ///
    /// # Arguments
    /// * `base_url` - Optional custom base URL (defaults to http://localhost:11434)
    /// * `model` - Model identifier (e.g., "llama3.2", "mistral", "qwen2.5")
    /// * `temperature` - Sampling temperature (0.0 to 1.0)
    pub fn new(
        base_url: Option<&str>,
        model: impl Into<String>,
        temperature: f64,
    ) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(300)) // Longer timeout for local inference
            .build()
            .expect("Failed to create HTTP client");

        Self {
            base_url: base_url
                .unwrap_or("http://localhost:11434")
                .trim_end_matches('/')
                .to_string(),
            model: model.into(),
            temperature,
            client,
        }
    }

    /// Convert generic chat messages to Ollama format.
    fn convert_messages(messages: &[ChatMessage]) -> Vec<OllamaMessage> {
        messages
            .iter()
            .map(|msg| OllamaMessage {
                role: msg.role.clone(),
                content: msg.content.clone(),
                images: None,
                tool_calls: None,
            })
            .collect()
    }

    /// Convert tool specifications to Ollama format.
    fn convert_tools(tools: &[ToolSpec]) -> Vec<OllamaTool> {
        tools
            .iter()
            .map(|t| OllamaTool {
                tool_type: "function".to_string(),
                function: OllamaFunctionDef {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            })
            .collect()
    }

    /// Parse Ollama response into generic ChatResponse.
    fn parse_response(response: OllamaChatResponse) -> ChatResponse {
        let mut text: Option<String> = None;
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        // Extract text content
        if !response.message.content.is_empty() {
            text = Some(response.message.content);
        }

        // Extract tool calls if present
        if let Some(calls) = &response.message.tool_calls {
            for tc in calls {
                tool_calls.push(ToolCall {
                    id: format!("call_{}", uuid::Uuid::new_v4()),
                    name: tc.function.name.clone(),
                    arguments: serde_json::to_string(&tc.function.arguments)
                        .unwrap_or_default(),
                });
            }
        }

        ChatResponse {
            text,
            tool_calls,
            usage: response.eval_count.map(|output| TokenUsage {
                input_tokens: response.prompt_eval_count,
                output_tokens: Some(output),
            }),
            reasoning_content: None,
        }
    }

    /// Create an error from an HTTP response or connection error.
    async fn api_error(response: reqwest::Response) -> anyhow::Error {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());

        // Try to parse Ollama error format
        if let Ok(error_resp) = serde_json::from_str::<OllamaErrorResponse>(&body) {
            if let Some(error_msg) = error_resp.error {
                return match status.as_u16() {
                    404 => anyhow::anyhow!("模型未找到：{}", error_msg),
                    400 => anyhow::anyhow!("请求错误：{}", error_msg),
                    _ => anyhow::anyhow!("Ollama API 错误：{}", error_msg),
                };
            }
        }

        // Fallback to generic error
        match status.as_u16() {
            404 => anyhow::anyhow!("模型未找到：指定的模型不存在，请使用 /api/tags 查看可用模型"),
            400 => anyhow::anyhow!("请求错误：{}", body),
            500 => anyhow::anyhow!("服务器错误：Ollama 服务内部错误"),
            _ => anyhow::anyhow!("请求失败：HTTP {} - {}", status, body),
        }
    }

    /// Create a connection error message.
    fn connection_error(e: &reqwest::Error, base_url: &str) -> anyhow::Error {
        if e.is_connect() {
            anyhow::anyhow!(
                "连接失败：无法连接到 Ollama 服务（{}），请确认 Ollama 是否正在运行。\n\
                 提示：运行 'ollama serve' 启动服务，或访问 https://ollama.ai 安装 Ollama",
                base_url
            )
        } else if e.is_timeout() {
            anyhow::anyhow!(
                "请求超时：Ollama 响应超时（300s），模型可能正在加载或推理时间过长"
            )
        } else {
            anyhow::anyhow!("网络请求失败：{}", e)
        }
    }

    /// Detect model capabilities from model name.
    fn detect_model_capabilities(model_name: &str) -> ModelInfo {
        let name = model_name.trim().to_ascii_lowercase();

        // Extract base model name (remove tag)
        let base_name = name.split(':').next().unwrap_or(&name);

        // Determine capabilities based on model family
        let supports_tools = matches!(
            base_name,
            "llama3.1" | "llama3.2" | "llama3.3" | "qwen2.5" | "mistral" | "mixtral"
        );

        let supports_vision = matches!(
            base_name,
            "llava" | "llama3.2-vision" | "bakllava" | "moondream" | "minicpm-v"
        );

        // Context length varies by model
        let context_length = if base_name.starts_with("llama3.1") {
            Some(128_000)
        } else if base_name.starts_with("llama3.2") || base_name.starts_with("llama3.3") {
            Some(128_000)
        } else if base_name.starts_with("qwen2.5") {
            Some(128_000)
        } else if base_name.starts_with("mistral") || base_name.starts_with("mixtral") {
            Some(32_000)
        } else if base_name.starts_with("codellama") {
            Some(16_000)
        } else {
            Some(4_096) // Default context length
        };

        // Create a human-readable display name
        let display_name = base_name
            .split('-')
            .map(|s| {
                let mut chars = s.chars();
                match chars.next() {
                    Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        ModelInfo {
            id: model_name.to_string(),
            name: display_name,
            description: None,
            context_length,
            supports_tools,
            supports_vision,
            supports_streaming: true,
        }
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn chat(&self, request: ChatRequest<'_>) -> anyhow::Result<ChatResponse> {
        let messages = Self::convert_messages(request.messages);

        let ollama_request = OllamaChatRequest {
            model: self.model.clone(),
            messages,
            stream: Some(false),
            options: Some(OllamaOptions {
                temperature: if self.temperature >= 0.0 {
                    Some(self.temperature)
                } else {
                    None
                },
                ..Default::default()
            }),
            tools: request.tools.map(|t| Self::convert_tools(t)),
        };

        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&ollama_request)
            .send()
            .await
            .map_err(|e| Self::connection_error(&e, &self.base_url))?;

        if !response.status().is_success() {
            return Err(Self::api_error(response).await);
        }

        let ollama_response: OllamaChatResponse = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("解析响应失败：无法解析 Ollama API 响应 - {}", e))?;

        Ok(Self::parse_response(ollama_response))
    }

    async fn health_check(&self) -> bool {
        // Try to connect to Ollama and list models
        match self.client.get(format!("{}/api/tags", self.base_url)).send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    async fn chat_stream(&self, request: ChatRequest<'_>) -> anyhow::Result<ChatStream> {
        let messages = Self::convert_messages(request.messages);

        let ollama_request = OllamaChatRequest {
            model: self.model.clone(),
            messages,
            stream: Some(true),
            options: Some(OllamaOptions {
                temperature: if self.temperature >= 0.0 {
                    Some(self.temperature)
                } else {
                    None
                },
                ..Default::default()
            }),
            tools: request.tools.map(|t| Self::convert_tools(t)),
        };

        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&ollama_request)
            .send()
            .await
            .map_err(|e| Self::connection_error(&e, &self.base_url))?;

        if !response.status().is_success() {
            return Err(Self::api_error(response).await);
        }

        let (tx, rx) = tokio::sync::mpsc::channel(32);

        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));

                        // Process complete lines (NDJSON format)
                        while let Some(newline_pos) = buffer.find('\n') {
                            let line = buffer[..newline_pos].trim().to_string();
                            buffer = buffer[newline_pos + 1..].to_string();

                            if line.is_empty() {
                                continue;
                            }

                            // Parse NDJSON line
                            match serde_json::from_str::<OllamaChatResponse>(&line) {
                                Ok(ollama_response) => {
                                    let delta = if ollama_response.message.content.is_empty() {
                                        None
                                    } else {
                                        Some(ollama_response.message.content)
                                    };

                                    let finish_reason = if ollama_response.done {
                                        ollama_response.done_reason.clone()
                                    } else {
                                        None
                                    };

                                    let usage = ollama_response.eval_count.map(|output| {
                                        TokenUsage {
                                            input_tokens: ollama_response.prompt_eval_count,
                                            output_tokens: Some(output),
                                        }
                                    });

                                    let chunk = ChatStreamChunk {
                                        delta,
                                        tool_calls: Vec::new(),
                                        usage,
                                        reasoning_content: None,
                                        finish_reason,
                                    };

                                    if tx.send(Ok(chunk)).await.is_err() {
                                        return;
                                    }
                                }
                                Err(e) => {
                                    let _ = tx
                                        .send(Err(StreamError::Parse(format!(
                                            "Failed to parse line: {}",
                                            e
                                        ))))
                                        .await;
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

    async fn embeddings(&self, request: EmbeddingRequest<'_>) -> anyhow::Result<EmbeddingResponse> {
        let model = request.model.unwrap_or(&self.model);

        let ollama_request = OllamaEmbeddingRequest {
            model: model.to_string(),
            prompt: request.text.to_string(),
        };

        let response = self
            .client
            .post(format!("{}/api/embeddings", self.base_url))
            .json(&ollama_request)
            .send()
            .await
            .map_err(|e| Self::connection_error(&e, &self.base_url))?;

        if !response.status().is_success() {
            return Err(Self::api_error(response).await);
        }

        let ollama_response: OllamaEmbeddingResponse = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("解析响应失败：无法解析 Ollama Embeddings 响应 - {}", e))?;

        Ok(EmbeddingResponse {
            embedding: ollama_response.embedding,
            model: model.to_string(),
            usage: None,
        })
    }

    async fn list_models(&self) -> anyhow::Result<Vec<ModelInfo>> {
        let response = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map_err(|e| Self::connection_error(&e, &self.base_url))?;

        if !response.status().is_success() {
            return Err(Self::api_error(response).await);
        }

        let tags_response: OllamaTagsResponse = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("解析响应失败：无法解析 Ollama 模型列表 - {}", e))?;

        let models: Vec<ModelInfo> = tags_response
            .models
            .into_iter()
            .map(|m| {
                let mut info = Self::detect_model_capabilities(&m.name);
                info.description = m.details.as_ref().map(|d| {
                    format!(
                        "Family: {}, Size: {}, Format: {}",
                        d.family.as_deref().unwrap_or("unknown"),
                        d.parameter_size.as_deref().unwrap_or("unknown"),
                        d.format.as_deref().unwrap_or("unknown")
                    )
                });
                info
            })
            .collect();

        Ok(models)
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_embeddings(&self) -> bool {
        true
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // OllamaMessage Tests
    // ============================================================================

    #[test]
    fn test_ollama_message_serialize() {
        let msg = OllamaMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
            images: None,
            tool_calls: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""role":"user""#));
        assert!(json.contains(r#""content":"Hello""#));
    }

    #[test]
    fn test_ollama_message_deserialize() {
        let json = r#"{"role":"assistant","content":"Hi there!"}"#;
        let msg: OllamaMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "Hi there!");
    }

    #[test]
    fn test_ollama_message_with_images() {
        let msg = OllamaMessage {
            role: "user".to_string(),
            content: "What's in this image?".to_string(),
            images: Some(vec!["base64imagedata".to_string()]),
            tool_calls: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""images":["base64imagedata"]"#));
    }

    // ============================================================================
    // OllamaChatRequest Tests
    // ============================================================================

    #[test]
    fn test_ollama_chat_request_serialize() {
        let request = OllamaChatRequest {
            model: "llama3.2".to_string(),
            messages: vec![OllamaMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
                images: None,
                tool_calls: None,
            }],
            stream: Some(false),
            options: Some(OllamaOptions {
                temperature: Some(0.7),
                ..Default::default()
            }),
            tools: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""model":"llama3.2""#));
        assert!(json.contains(r#""stream":false"#));
        assert!(json.contains(r#""temperature":0.7"#));
    }

    // ============================================================================
    // OllamaChatResponse Tests
    // ============================================================================

    #[test]
    fn test_ollama_chat_response_deserialize() {
        let json = r#"{
            "model": "llama3.2",
            "created_at": "2024-01-01T00:00:00Z",
            "message": {"role": "assistant", "content": "Hello!"},
            "done": true,
            "done_reason": "stop",
            "eval_count": 10,
            "prompt_eval_count": 5
        }"#;
        let response: OllamaChatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.model, "llama3.2");
        assert_eq!(response.message.content, "Hello!");
        assert!(response.done);
        assert_eq!(response.eval_count, Some(10));
    }

    #[test]
    fn test_ollama_chat_response_streaming() {
        let json = r#"{
            "model": "llama3.2",
            "created_at": "2024-01-01T00:00:00Z",
            "message": {"role": "assistant", "content": "Hello"},
            "done": false
        }"#;
        let response: OllamaChatResponse = serde_json::from_str(json).unwrap();
        assert!(!response.done);
    }

    // ============================================================================
    // OllamaTagsResponse Tests
    // ============================================================================

    #[test]
    fn test_ollama_tags_response_deserialize() {
        let json = r#"{
            "models": [
                {
                    "name": "llama3.2:latest",
                    "modified_at": "2024-01-01T00:00:00Z",
                    "size": 4661224676,
                    "digest": "abc123",
                    "details": {
                        "format": "gguf",
                        "family": "llama",
                        "parameter_size": "3B",
                        "quantization_level": "Q4_K_M"
                    }
                }
            ]
        }"#;
        let response: OllamaTagsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.models.len(), 1);
        assert_eq!(response.models[0].name, "llama3.2:latest");
        assert_eq!(response.models[0].size, 4661224676);
        assert!(response.models[0].details.is_some());
    }

    // ============================================================================
    // OllamaEmbeddingRequest/Response Tests
    // ============================================================================

    #[test]
    fn test_ollama_embedding_request_serialize() {
        let request = OllamaEmbeddingRequest {
            model: "nomic-embed-text".to_string(),
            prompt: "Hello world".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""model":"nomic-embed-text""#));
        assert!(json.contains(r#""prompt":"Hello world""#));
    }

    #[test]
    fn test_ollama_embedding_response_deserialize() {
        let json = r#"{"embedding": [0.1, 0.2, 0.3]}"#;
        let response: OllamaEmbeddingResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.embedding.len(), 3);
        assert_eq!(response.embedding[0], 0.1);
    }

    // ============================================================================
    // OllamaProvider Tests
    // ============================================================================

    #[test]
    fn test_provider_new() {
        let provider = OllamaProvider::new(None, "llama3.2", 0.7);
        assert_eq!(provider.name(), "ollama");
        assert_eq!(provider.model, "llama3.2");
        assert_eq!(provider.temperature, 0.7);
        assert_eq!(provider.base_url, "http://localhost:11434");
    }

    #[test]
    fn test_provider_custom_base_url() {
        let provider = OllamaProvider::new(
            Some("http://192.168.1.100:11434/"),
            "mistral",
            0.5,
        );
        assert_eq!(provider.base_url, "http://192.168.1.100:11434");
    }

    #[test]
    fn test_provider_supports_flags() {
        let provider = OllamaProvider::new(None, "llama3.2", 0.5);
        assert!(provider.supports_streaming());
        assert!(provider.supports_embeddings());
    }

    // ============================================================================
    // Message Conversion Tests
    // ============================================================================

    #[test]
    fn test_convert_messages() {
        let messages = vec![
            ChatMessage::system("You are helpful."),
            ChatMessage::user("Hello"),
            ChatMessage::assistant("Hi there!"),
        ];
        let ollama_messages = OllamaProvider::convert_messages(&messages);

        assert_eq!(ollama_messages.len(), 3);
        assert_eq!(ollama_messages[0].role, "system");
        assert_eq!(ollama_messages[1].role, "user");
        assert_eq!(ollama_messages[2].role, "assistant");
    }

    // ============================================================================
    // Response Parsing Tests
    // ============================================================================

    #[test]
    fn test_parse_response_text_only() {
        let response = OllamaChatResponse {
            model: "llama3.2".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            message: OllamaMessage {
                role: "assistant".to_string(),
                content: "Hello!".to_string(),
                images: None,
                tool_calls: None,
            },
            done: true,
            done_reason: Some("stop".to_string()),
            total_duration: Some(1000000),
            load_duration: None,
            prompt_eval_count: Some(5),
            eval_count: Some(10),
        };

        let chat_response = OllamaProvider::parse_response(response);
        assert_eq!(chat_response.text, Some("Hello!".to_string()));
        assert!(chat_response.tool_calls.is_empty());
        assert!(chat_response.usage.is_some());
        assert_eq!(chat_response.usage.unwrap().output_tokens, Some(10));
    }

    #[test]
    fn test_parse_response_with_tool_calls() {
        let response = OllamaChatResponse {
            model: "llama3.1".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            message: OllamaMessage {
                role: "assistant".to_string(),
                content: String::new(),
                images: None,
                tool_calls: Some(vec![OllamaToolCall {
                    function: OllamaFunctionCall {
                        name: "get_weather".to_string(),
                        arguments: serde_json::json!({"location": "Beijing"}),
                    },
                }]),
            },
            done: true,
            done_reason: Some("stop".to_string()),
            total_duration: None,
            load_duration: None,
            prompt_eval_count: None,
            eval_count: None,
        };

        let chat_response = OllamaProvider::parse_response(response);
        assert!(chat_response.text.is_none() || chat_response.text.as_ref().unwrap().is_empty());
        assert_eq!(chat_response.tool_calls.len(), 1);
        assert_eq!(chat_response.tool_calls[0].name, "get_weather");
    }

    // ============================================================================
    // Model Capabilities Tests
    // ============================================================================

    #[test]
    fn test_detect_model_capabilities_llama32() {
        let info = OllamaProvider::detect_model_capabilities("llama3.2:latest");
        assert!(info.name.contains("Llama"));
        assert_eq!(info.context_length, Some(128_000));
        assert!(info.supports_tools);
        assert!(!info.supports_vision);
    }

    #[test]
    fn test_detect_model_capabilities_llama31() {
        let info = OllamaProvider::detect_model_capabilities("llama3.1:70b");
        assert_eq!(info.context_length, Some(128_000));
        assert!(info.supports_tools);
    }

    #[test]
    fn test_detect_model_capabilities_mistral() {
        let info = OllamaProvider::detect_model_capabilities("mistral:latest");
        assert_eq!(info.context_length, Some(32_000));
        assert!(info.supports_tools);
    }

    #[test]
    fn test_detect_model_capabilities_llava() {
        let info = OllamaProvider::detect_model_capabilities("llava:latest");
        assert!(info.supports_vision);
    }

    #[test]
    fn test_detect_model_capabilities_qwen() {
        let info = OllamaProvider::detect_model_capabilities("qwen2.5:72b");
        assert_eq!(info.context_length, Some(128_000));
        assert!(info.supports_tools);
    }

    #[test]
    fn test_detect_model_capabilities_unknown() {
        let info = OllamaProvider::detect_model_capabilities("unknown-model");
        assert_eq!(info.context_length, Some(4_096));
        assert!(!info.supports_tools);
        assert!(!info.supports_vision);
    }

    // ============================================================================
    // Tool Conversion Tests
    // ============================================================================

    #[test]
    fn test_convert_tools() {
        let tools = vec![ToolSpec {
            name: "get_weather".to_string(),
            description: "Get weather info".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "location": {"type": "string"}
                }
            }),
        }];

        let ollama_tools = OllamaProvider::convert_tools(&tools);
        assert_eq!(ollama_tools.len(), 1);
        assert_eq!(ollama_tools[0].tool_type, "function");
        assert_eq!(ollama_tools[0].function.name, "get_weather");
    }

    // ============================================================================
    // Error Handling Tests
    // ============================================================================

    #[test]
    fn test_ollama_error_response_deserialize() {
        let json = r#"{"error": "model 'unknown' not found"}"#;
        let error: OllamaErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(error.error, Some("model 'unknown' not found".to_string()));
    }

    #[test]
    fn test_ollama_error_response_empty() {
        let json = r#"{}"#;
        let error: OllamaErrorResponse = serde_json::from_str(json).unwrap();
        assert!(error.error.is_none());
    }

    // ============================================================================
    // OllamaOptions Tests
    // ============================================================================

    #[test]
    fn test_ollama_options_serialize() {
        let options = OllamaOptions {
            temperature: Some(0.7),
            num_ctx: Some(4096),
            num_predict: Some(100),
        };
        let json = serde_json::to_string(&options).unwrap();
        assert!(json.contains(r#""temperature":0.7"#));
        assert!(json.contains(r#""num_ctx":4096"#));
        assert!(json.contains(r#""num_predict":100"#));
    }

    #[test]
    fn test_ollama_options_default() {
        let options = OllamaOptions::default();
        assert!(options.temperature.is_none());
        assert!(options.num_ctx.is_none());
        assert!(options.num_predict.is_none());
    }

    // ============================================================================
    // OllamaModelDetails Tests
    // ============================================================================

    #[test]
    fn test_ollama_model_details_deserialize() {
        let json = r#"{
            "format": "gguf",
            "family": "llama",
            "parameter_size": "3B",
            "quantization_level": "Q4_K_M"
        }"#;
        let details: OllamaModelDetails = serde_json::from_str(json).unwrap();
        assert_eq!(details.format, Some("gguf".to_string()));
        assert_eq!(details.family, Some("llama".to_string()));
        assert_eq!(details.parameter_size, Some("3B".to_string()));
    }
}