use crate::tools::ToolSpec;
use async_trait::async_trait;
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
        }
    }

    pub fn tool(content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: content.into(),
        }
    }
}

/// A tool call requested by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Raw token counts from a single LLM API response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

/// An LLM response that may contain text, tool calls, or both.
#[derive(Debug, Clone)]
pub struct ChatResponse {
    /// Text content of the response (may be empty if only tool calls).
    pub text: Option<String>,
    /// Tool calls requested by the LLM.
    pub tool_calls: Vec<ToolCall>,
    /// Token usage reported by the provider, if available.
    pub usage: Option<TokenUsage>,
    /// Raw reasoning/thinking content from thinking models (e.g. DeepSeek-R1,
    /// Kimi K2.5, GLM-4.7). Preserved as an opaque pass-through so it can be
    /// sent back in subsequent API requests — some providers reject tool-call
    /// history that omits this field.
    pub reasoning_content: Option<String>,
}

impl ChatResponse {
    /// True when the LLM wants to invoke at least one tool.
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }

    /// Convenience: return text content or empty string.
    pub fn text_or_empty(&self) -> &str {
        self.text.as_deref().unwrap_or("")
    }
}

/// Request payload for provider chat calls.
#[derive(Debug, Clone, Copy)]
pub struct ChatRequest<'a> {
    pub messages: &'a [ChatMessage],
    pub tools: Option<&'a [ToolSpec]>,
}

/// A tool result to feed back to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultMessage {
    pub tool_call_id: String,
    pub content: String,
}

/// A message in a multi-turn conversation, including tool interactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ConversationMessage {
    /// Regular chat message (system, user, assistant).
    Chat(ChatMessage),
    /// Tool calls from the assistant (stored for history fidelity).
    AssistantToolCalls {
        text: Option<String>,
        tool_calls: Vec<ToolCall>,
        /// Raw reasoning content from thinking models, preserved for round-trip
        /// fidelity with provider APIs that require it.
        reasoning_content: Option<String>,
    },
    /// Results of tool executions, fed back to the LLM.
    ToolResults(Vec<ToolResultMessage>),
}

// ============================================================================
// Streaming Support
// ============================================================================

/// A single chunk in a streaming response.
///
/// Each chunk represents a partial update to the response,
/// which may contain text delta, tool calls, or completion signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatStreamChunk {
    /// Incremental text content (may be None for tool-only chunks).
    pub delta: Option<String>,
    /// Tool calls requested by the LLM (usually in final chunk).
    pub tool_calls: Vec<ToolCall>,
    /// Token usage (usually in final chunk).
    pub usage: Option<TokenUsage>,
    /// Reasoning content from thinking models.
    pub reasoning_content: Option<String>,
    /// Finish reason (e.g., "stop", "tool_calls", "length").
    pub finish_reason: Option<String>,
}

impl ChatStreamChunk {
    /// Create a text delta chunk.
    pub fn text_delta(text: impl Into<String>) -> Self {
        Self {
            delta: Some(text.into()),
            tool_calls: Vec::new(),
            usage: None,
            reasoning_content: None,
            finish_reason: None,
        }
    }

    /// Create a completion chunk.
    pub fn finish(reason: impl Into<String>) -> Self {
        Self {
            delta: None,
            tool_calls: Vec::new(),
            usage: None,
            reasoning_content: None,
            finish_reason: Some(reason.into()),
        }
    }

    /// Check if this is the final chunk.
    pub fn is_finished(&self) -> bool {
        self.finish_reason.is_some()
    }

    /// Check if the chunk contains any content.
    pub fn has_content(&self) -> bool {
        self.delta.is_some() || !self.tool_calls.is_empty()
    }
}

/// Error type for streaming operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum StreamError {
    /// Network or connection error.
    #[error("Connection error: {0}")]
    Connection(String),
    /// API error response.
    #[error("API error: {0}")]
    Api(String),
    /// Parsing error.
    #[error("Parse error: {0}")]
    Parse(String),
    /// Stream was interrupted.
    #[error("Stream interrupted: {0}")]
    Interrupted(String),
    /// Rate limit exceeded.
    #[error("Rate limit exceeded")]
    RateLimit,
}

/// Type alias for a boxed stream of chat chunks.
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatStreamChunk, StreamError>> + Send>>;

// ============================================================================
// Embeddings Support
// ============================================================================

/// Request for text embeddings.
#[derive(Debug, Clone)]
pub struct EmbeddingRequest<'a> {
    /// Text to embed.
    pub text: &'a str,
    /// Optional model override.
    pub model: Option<&'a str>,
}

impl<'a> EmbeddingRequest<'a> {
    /// Create a new embedding request for the given text.
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            model: None,
        }
    }

    /// Specify a model for the embedding.
    pub fn with_model(mut self, model: &'a str) -> Self {
        self.model = Some(model);
        self
    }
}

/// Response containing text embeddings.
#[derive(Debug, Clone)]
pub struct EmbeddingResponse {
    /// The embedding vector.
    pub embedding: Vec<f32>,
    /// Model used for embedding.
    pub model: String,
    /// Token usage if available.
    pub usage: Option<TokenUsage>,
}

impl EmbeddingResponse {
    /// Get the dimension of the embedding vector.
    pub fn dimension(&self) -> usize {
        self.embedding.len()
    }
}

// ============================================================================
// Model Information
// ============================================================================

/// Information about an available model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier (e.g., "gpt-4o-mini").
    pub id: String,
    /// Human-readable model name.
    pub name: String,
    /// Model description.
    pub description: Option<String>,
    /// Context window size.
    pub context_length: Option<u64>,
    /// Whether the model supports tool calls.
    pub supports_tools: bool,
    /// Whether the model supports vision.
    pub supports_vision: bool,
    /// Whether the model supports streaming.
    pub supports_streaming: bool,
}

impl ModelInfo {
    /// Create a new model info with just an ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: String::new(),
            description: None,
            context_length: None,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
        }
    }

    /// Set the human-readable name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set the context length.
    pub fn with_context_length(mut self, length: u64) -> Self {
        self.context_length = Some(length);
        self
    }

    /// Mark as supporting tools.
    pub fn with_tools(mut self) -> Self {
        self.supports_tools = true;
        self
    }

    /// Mark as supporting vision.
    pub fn with_vision(mut self) -> Self {
        self.supports_vision = true;
        self
    }
}

/// Core provider trait — implement for any LLM API (OpenAI, Anthropic, etc.)
#[async_trait]
pub trait Provider: Send + Sync {
    /// Provider name (e.g., "openai", "anthropic")
    fn name(&self) -> &str;

    /// Send a chat request to the LLM
    async fn chat(&self, request: ChatRequest<'_>) -> anyhow::Result<ChatResponse>;

    /// Check if the provider is healthy
    async fn health_check(&self) -> bool;

    /// Send a streaming chat request to the LLM.
    ///
    /// Returns a stream of chunks that can be iterated asynchronously.
    /// Default implementation returns an error indicating streaming is not supported.
    async fn chat_stream(&self, _request: ChatRequest<'_>) -> anyhow::Result<ChatStream> {
        anyhow::bail!("Streaming not supported by provider '{}'", self.name())
    }

    /// Generate embeddings for text.
    ///
    /// Default implementation returns an error indicating embeddings are not supported.
    async fn embeddings(&self, _request: EmbeddingRequest<'_>) -> anyhow::Result<EmbeddingResponse> {
        anyhow::bail!("Embeddings not supported by provider '{}'", self.name())
    }

    /// List available models for this provider.
    ///
    /// Default implementation returns an empty list.
    async fn list_models(&self) -> anyhow::Result<Vec<ModelInfo>> {
        Ok(Vec::new())
    }

    /// Check if the provider supports streaming.
    fn supports_streaming(&self) -> bool {
        false
    }

    /// Check if the provider supports embeddings.
    fn supports_embeddings(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // ChatStreamChunk Tests
    // ============================================================================

    #[test]
    fn test_chat_stream_chunk_text_delta() {
        let chunk = ChatStreamChunk::text_delta("Hello");
        assert_eq!(chunk.delta, Some("Hello".to_string()));
        assert!(chunk.tool_calls.is_empty());
        assert!(chunk.usage.is_none());
        assert!(chunk.finish_reason.is_none());
        assert!(!chunk.is_finished());
        assert!(chunk.has_content());
    }

    #[test]
    fn test_chat_stream_chunk_finish() {
        let chunk = ChatStreamChunk::finish("stop");
        assert!(chunk.delta.is_none());
        assert_eq!(chunk.finish_reason, Some("stop".to_string()));
        assert!(chunk.is_finished());
        assert!(!chunk.has_content());
    }

    #[test]
    fn test_chat_stream_chunk_with_tool_calls() {
        let tool_call = ToolCall {
            id: "call_123".to_string(),
            name: "get_weather".to_string(),
            arguments: r#"{"location": "Beijing"}"#.to_string(),
        };
        let chunk = ChatStreamChunk {
            delta: None,
            tool_calls: vec![tool_call],
            usage: None,
            reasoning_content: None,
            finish_reason: Some("tool_calls".to_string()),
        };
        assert!(chunk.is_finished());
        assert!(chunk.has_content());
        assert_eq!(chunk.tool_calls.len(), 1);
    }

    #[test]
    fn test_chat_stream_chunk_with_usage() {
        let usage = TokenUsage {
            input_tokens: Some(10),
            output_tokens: Some(20),
        };
        let chunk = ChatStreamChunk {
            delta: None,
            tool_calls: Vec::new(),
            usage: Some(usage.clone()),
            reasoning_content: None,
            finish_reason: Some("stop".to_string()),
        };
        assert!(chunk.usage.is_some());
        assert_eq!(chunk.usage.unwrap().input_tokens, Some(10));
    }

    #[test]
    fn test_chat_stream_chunk_with_reasoning() {
        let chunk = ChatStreamChunk {
            delta: Some("Answer".to_string()),
            tool_calls: Vec::new(),
            usage: None,
            reasoning_content: Some("Thinking...".to_string()),
            finish_reason: None,
        };
        assert_eq!(chunk.reasoning_content, Some("Thinking...".to_string()));
    }

    #[test]
    fn test_chat_stream_chunk_serialize_deserialize() {
        let chunk = ChatStreamChunk::text_delta("test content");
        let json = serde_json::to_string(&chunk).unwrap();
        let deserialized: ChatStreamChunk = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.delta, Some("test content".to_string()));
    }

    // ============================================================================
    // StreamError Tests
    // ============================================================================

    #[test]
    fn test_stream_error_connection() {
        let err = StreamError::Connection("timeout".to_string());
        assert_eq!(err.to_string(), "Connection error: timeout");
    }

    #[test]
    fn test_stream_error_api() {
        let err = StreamError::Api("rate limited".to_string());
        assert_eq!(err.to_string(), "API error: rate limited");
    }

    #[test]
    fn test_stream_error_parse() {
        let err = StreamError::Parse("invalid JSON".to_string());
        assert_eq!(err.to_string(), "Parse error: invalid JSON");
    }

    #[test]
    fn test_stream_error_interrupted() {
        let err = StreamError::Interrupted("user cancelled".to_string());
        assert_eq!(err.to_string(), "Stream interrupted: user cancelled");
    }

    #[test]
    fn test_stream_error_rate_limit() {
        let err = StreamError::RateLimit;
        assert_eq!(err.to_string(), "Rate limit exceeded");
    }

    // ============================================================================
    // EmbeddingRequest Tests
    // ============================================================================

    #[test]
    fn test_embedding_request_new() {
        let req = EmbeddingRequest::new("Hello world");
        assert_eq!(req.text, "Hello world");
        assert!(req.model.is_none());
    }

    #[test]
    fn test_embedding_request_with_model() {
        let req = EmbeddingRequest::new("Hello world").with_model("text-embedding-3-small");
        assert_eq!(req.text, "Hello world");
        assert_eq!(req.model, Some("text-embedding-3-small"));
    }

    // ============================================================================
    // EmbeddingResponse Tests
    // ============================================================================

    #[test]
    fn test_embedding_response_dimension() {
        let resp = EmbeddingResponse {
            embedding: vec![0.1, 0.2, 0.3, 0.4, 0.5],
            model: "text-embedding-3-small".to_string(),
            usage: None,
        };
        assert_eq!(resp.dimension(), 5);
    }

    #[test]
    fn test_embedding_response_with_usage() {
        let usage = TokenUsage {
            input_tokens: Some(10),
            output_tokens: None,
        };
        let resp = EmbeddingResponse {
            embedding: vec![0.1; 1536],
            model: "text-embedding-3-small".to_string(),
            usage: Some(usage),
        };
        assert!(resp.usage.is_some());
        assert_eq!(resp.usage.unwrap().input_tokens, Some(10));
    }

    // ============================================================================
    // ModelInfo Tests
    // ============================================================================

    #[test]
    fn test_model_info_new() {
        let info = ModelInfo::new("gpt-4o-mini");
        assert_eq!(info.id, "gpt-4o-mini");
        assert!(info.name.is_empty());
        assert!(info.description.is_none());
        assert!(info.context_length.is_none());
        assert!(!info.supports_tools);
        assert!(!info.supports_vision);
        assert!(info.supports_streaming);
    }

    #[test]
    fn test_model_info_with_name() {
        let info = ModelInfo::new("gpt-4o-mini").with_name("GPT-4o Mini");
        assert_eq!(info.name, "GPT-4o Mini");
    }

    #[test]
    fn test_model_info_with_context_length() {
        let info = ModelInfo::new("gpt-4o-mini").with_context_length(128000);
        assert_eq!(info.context_length, Some(128000));
    }

    #[test]
    fn test_model_info_with_tools() {
        let info = ModelInfo::new("gpt-4o-mini").with_tools();
        assert!(info.supports_tools);
    }

    #[test]
    fn test_model_info_with_vision() {
        let info = ModelInfo::new("gpt-4o-mini").with_vision();
        assert!(info.supports_vision);
    }

    #[test]
    fn test_model_info_builder_chain() {
        let info = ModelInfo::new("gpt-4o")
            .with_name("GPT-4o")
            .with_context_length(128000)
            .with_tools()
            .with_vision();
        assert_eq!(info.id, "gpt-4o");
        assert_eq!(info.name, "GPT-4o");
        assert_eq!(info.context_length, Some(128000));
        assert!(info.supports_tools);
        assert!(info.supports_vision);
    }

    #[test]
    fn test_model_info_serialize_deserialize() {
        let info = ModelInfo::new("gpt-4o")
            .with_name("GPT-4o")
            .with_context_length(128000)
            .with_tools()
            .with_vision();
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: ModelInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "gpt-4o");
        assert_eq!(deserialized.name, "GPT-4o");
        assert!(deserialized.supports_tools);
        assert!(deserialized.supports_vision);
    }

    // ============================================================================
    // Provider Trait Default Implementation Tests
    // ============================================================================

    struct TestProvider;

    #[async_trait]
    impl Provider for TestProvider {
        fn name(&self) -> &str {
            "test"
        }

        async fn chat(&self, _request: ChatRequest<'_>) -> anyhow::Result<ChatResponse> {
            Ok(ChatResponse {
                text: Some("test response".to_string()),
                tool_calls: Vec::new(),
                usage: None,
                reasoning_content: None,
            })
        }

        async fn health_check(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn test_provider_default_streaming_not_supported() {
        let provider = TestProvider;
        let messages = vec![ChatMessage::user("Hello")];
        let request = ChatRequest {
            messages: &messages,
            tools: None,
        };
        let result = provider.chat_stream(request).await;
        match result {
            Err(e) => assert!(e.to_string().contains("Streaming not supported")),
            Ok(_) => panic!("Expected error for streaming not supported"),
        }
    }

    #[tokio::test]
    async fn test_provider_default_embeddings_not_supported() {
        let provider = TestProvider;
        let request = EmbeddingRequest::new("test");
        let result = provider.embeddings(request).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Embeddings not supported"));
    }

    #[tokio::test]
    async fn test_provider_default_list_models_empty() {
        let provider = TestProvider;
        let models = provider.list_models().await.unwrap();
        assert!(models.is_empty());
    }

    #[test]
    fn test_provider_default_supports_flags() {
        let provider = TestProvider;
        assert!(!provider.supports_streaming());
        assert!(!provider.supports_embeddings());
    }
}
