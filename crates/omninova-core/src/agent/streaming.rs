//! Streaming Response Infrastructure
//!
//! This module provides types and utilities for handling streaming responses
//! from LLM providers, including event emission for Tauri's event system.

use crate::providers::{ChatStreamChunk, StreamError, TokenUsage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Maximum time to wait for the first token (3 seconds SLA from AC5)
pub const FIRST_TOKEN_TIMEOUT_SECS: u64 = 3;

/// Events emitted during streaming for Tauri event system.
///
/// These events are serialized and sent to the frontend via `window.emit()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StreamEvent {
    /// Stream has started.
    #[serde(rename = "start")]
    Start {
        /// Session ID for the conversation.
        session_id: i64,
        /// Unique identifier for this streaming request.
        request_id: String,
    },

    /// Incremental content delta received.
    #[serde(rename = "delta")]
    Delta {
        /// Text content delta.
        delta: String,
        /// Optional reasoning content (for thinking models like DeepSeek R1).
        #[serde(skip_serializing_if = "Option::is_none")]
        reasoning: Option<String>,
    },

    /// Tool call requested during streaming.
    #[serde(rename = "toolCall")]
    ToolCall {
        /// Name of the tool being called.
        tool_name: String,
        /// Arguments for the tool call.
        tool_args: serde_json::Value,
    },

    /// Stream completed successfully.
    #[serde(rename = "done")]
    Done {
        /// Session ID for the conversation.
        session_id: i64,
        /// Message ID of the persisted assistant message.
        message_id: i64,
        /// Token usage statistics (if available).
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<TokenUsage>,
    },

    /// Stream encountered an error.
    #[serde(rename = "error")]
    Error {
        /// Error code for programmatic handling.
        code: String,
        /// Human-readable error message (in Chinese per project convention).
        message: String,
        /// Partial content received before the error.
        #[serde(skip_serializing_if = "Option::is_none")]
        partial_content: Option<String>,
    },
}

impl StreamEvent {
    /// Create a start event.
    pub fn start(session_id: i64) -> Self {
        Self::Start {
            session_id,
            request_id: Uuid::new_v4().to_string(),
        }
    }

    /// Create a start event with a specific request ID.
    pub fn start_with_request_id(session_id: i64, request_id: String) -> Self {
        Self::Start {
            session_id,
            request_id,
        }
    }

    /// Create a delta event.
    pub fn delta(text: impl Into<String>) -> Self {
        Self::Delta {
            delta: text.into(),
            reasoning: None,
        }
    }

    /// Create a delta event with reasoning content.
    pub fn delta_with_reasoning(text: impl Into<String>, reasoning: impl Into<String>) -> Self {
        Self::Delta {
            delta: text.into(),
            reasoning: Some(reasoning.into()),
        }
    }

    /// Create a tool call event.
    pub fn tool_call(name: impl Into<String>, args: serde_json::Value) -> Self {
        Self::ToolCall {
            tool_name: name.into(),
            tool_args: args,
        }
    }

    /// Create a done event.
    pub fn done(session_id: i64, message_id: i64) -> Self {
        Self::Done {
            session_id,
            message_id,
            usage: None,
        }
    }

    /// Create a done event with token usage.
    pub fn done_with_usage(session_id: i64, message_id: i64, usage: TokenUsage) -> Self {
        Self::Done {
            session_id,
            message_id,
            usage: Some(usage),
        }
    }

    /// Create an error event.
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
            partial_content: None,
        }
    }

    /// Create an error event with partial content.
    pub fn error_with_partial(
        code: impl Into<String>,
        message: impl Into<String>,
        partial_content: impl Into<String>,
    ) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
            partial_content: Some(partial_content.into()),
        }
    }

    /// Get the event type name for Tauri event emission.
    pub fn event_name(&self) -> &'static str {
        match self {
            StreamEvent::Start { .. } => "stream:start",
            StreamEvent::Delta { .. } => "stream:delta",
            StreamEvent::ToolCall { .. } => "stream:toolCall",
            StreamEvent::Done { .. } => "stream:done",
            StreamEvent::Error { .. } => "stream:error",
        }
    }
}

/// Accumulated content from a streaming response.
#[derive(Debug, Clone, Default)]
pub struct StreamAccumulator {
    /// Accumulated text content.
    pub content: String,
    /// Accumulated reasoning content.
    pub reasoning: String,
    /// Token usage (from final chunk).
    pub usage: Option<TokenUsage>,
    /// Finish reason (from final chunk).
    pub finish_reason: Option<String>,
    /// Whether the stream was cancelled.
    pub cancelled: bool,
}

impl StreamAccumulator {
    /// Create a new empty accumulator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a stream chunk and accumulate its content.
    pub fn process_chunk(&mut self, chunk: &ChatStreamChunk) {
        if let Some(ref delta) = chunk.delta {
            self.content.push_str(delta);
        }
        if let Some(ref reasoning) = chunk.reasoning_content {
            self.reasoning.push_str(reasoning);
        }
        if let Some(ref usage) = chunk.usage {
            self.usage = Some(usage.clone());
        }
        if let Some(ref reason) = chunk.finish_reason {
            self.finish_reason = Some(reason.clone());
        }
    }

    /// Mark the stream as cancelled.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// Check if there is any accumulated content.
    pub fn has_content(&self) -> bool {
        !self.content.is_empty() || !self.reasoning.is_empty()
    }

    /// Get the final content for persistence.
    pub fn final_content(&self) -> String {
        self.content.clone()
    }
}

/// State for an active streaming session.
#[derive(Debug, Clone)]
pub struct ActiveStream {
    /// Session ID for this stream.
    pub session_id: i64,
    /// Request ID for this stream.
    pub request_id: String,
    /// When the stream started.
    pub started_at: Instant,
    /// Whether the stream has been cancelled.
    pub cancelled: bool,
}

impl ActiveStream {
    /// Create a new active stream state.
    pub fn new(session_id: i64) -> Self {
        Self {
            session_id,
            request_id: Uuid::new_v4().to_string(),
            started_at: Instant::now(),
            cancelled: false,
        }
    }

    /// Create a new active stream with a specific request ID.
    pub fn with_request_id(session_id: i64, request_id: String) -> Self {
        Self {
            session_id,
            request_id,
            started_at: Instant::now(),
            cancelled: false,
        }
    }

    /// Mark the stream as cancelled.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// Get the elapsed time since the stream started.
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Check if first token latency exceeds the SLA.
    pub fn exceeds_first_token_latency(&self) -> bool {
        self.elapsed() > Duration::from_secs(FIRST_TOKEN_TIMEOUT_SECS)
    }
}

/// Manager for tracking active streams across sessions.
///
/// This is used to support stream cancellation and to track
/// streaming state per window/session.
#[derive(Debug, Default, Clone)]
pub struct StreamManager {
    /// Active streams keyed by session ID.
    streams: Arc<RwLock<HashMap<i64, ActiveStream>>>,
}

impl StreamManager {
    /// Create a new stream manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new active stream.
    pub async fn register(&self, session_id: i64) -> ActiveStream {
        let stream = ActiveStream::new(session_id);
        self.streams.write().await.insert(session_id, stream.clone());
        stream
    }

    /// Register a stream with a specific request ID.
    pub async fn register_with_request_id(
        &self,
        session_id: i64,
        request_id: String,
    ) -> ActiveStream {
        let stream = ActiveStream::with_request_id(session_id, request_id);
        self.streams.write().await.insert(session_id, stream.clone());
        stream
    }

    /// Cancel an active stream.
    pub async fn cancel(&self, session_id: i64) -> bool {
        if let Some(stream) = self.streams.write().await.get_mut(&session_id) {
            stream.cancel();
            true
        } else {
            false
        }
    }

    /// Check if a stream is cancelled.
    pub async fn is_cancelled(&self, session_id: i64) -> bool {
        self.streams
            .read()
            .await
            .get(&session_id)
            .map(|s| s.cancelled)
            .unwrap_or(false)
    }

    /// Get an active stream by session ID.
    pub async fn get(&self, session_id: i64) -> Option<ActiveStream> {
        self.streams.read().await.get(&session_id).cloned()
    }

    /// Remove a completed stream.
    pub async fn remove(&self, session_id: i64) -> Option<ActiveStream> {
        self.streams.write().await.remove(&session_id)
    }
}

/// Error codes for streaming operations.
pub mod error_codes {
    /// Provider error during streaming.
    pub const PROVIDER_ERROR: &str = "PROVIDER_ERROR";
    /// Rate limit exceeded.
    pub const RATE_LIMIT: &str = "RATE_LIMIT";
    /// Context length exceeded.
    pub const CONTEXT_LENGTH: &str = "CONTEXT_LENGTH";
    /// Connection error.
    pub const CONNECTION_ERROR: &str = "CONNECTION_ERROR";
    /// Stream was cancelled by user.
    pub const CANCELLED: &str = "CANCELLED";
    /// Timeout waiting for first token.
    pub const FIRST_TOKEN_TIMEOUT: &str = "FIRST_TOKEN_TIMEOUT";
    /// Internal error.
    pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
}

/// Convert StreamError to error code and message.
pub fn stream_error_to_code_and_message(error: &StreamError) -> (String, String) {
    match error {
        StreamError::Connection(msg) => (
            error_codes::CONNECTION_ERROR.to_string(),
            format!("连接错误: {}", msg),
        ),
        StreamError::Api(msg) => (
            error_codes::PROVIDER_ERROR.to_string(),
            format!("API 错误: {}", msg),
        ),
        StreamError::Parse(msg) => (
            error_codes::PROVIDER_ERROR.to_string(),
            format!("解析错误: {}", msg),
        ),
        StreamError::Interrupted(msg) => (
            error_codes::CANCELLED.to_string(),
            format!("流中断: {}", msg),
        ),
        StreamError::RateLimit => (
            error_codes::RATE_LIMIT.to_string(),
            "请求频率超限，请稍后重试".to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // StreamEvent Tests
    // ============================================================================

    #[test]
    fn test_stream_event_start() {
        let event = StreamEvent::start(42);
        match event {
            StreamEvent::Start { session_id, .. } => {
                assert_eq!(session_id, 42);
            }
            _ => panic!("Expected Start event"),
        }
        assert_eq!(event.event_name(), "stream:start");
    }

    #[test]
    fn test_stream_event_delta() {
        let event = StreamEvent::delta("Hello");
        assert_eq!(event.event_name(), "stream:delta");
        match event {
            StreamEvent::Delta { delta, reasoning } => {
                assert_eq!(delta, "Hello");
                assert!(reasoning.is_none());
            }
            _ => panic!("Expected Delta event"),
        }
    }

    #[test]
    fn test_stream_event_delta_with_reasoning() {
        let event = StreamEvent::delta_with_reasoning("Answer", "Thinking...");
        match event {
            StreamEvent::Delta { delta, reasoning } => {
                assert_eq!(delta, "Answer");
                assert_eq!(reasoning, Some("Thinking...".to_string()));
            }
            _ => panic!("Expected Delta event"),
        }
    }

    #[test]
    fn test_stream_event_tool_call() {
        let args = serde_json::json!({"location": "Beijing"});
        let event = StreamEvent::tool_call("get_weather", args.clone());
        assert_eq!(event.event_name(), "stream:toolCall");
        match event {
            StreamEvent::ToolCall { tool_name, tool_args } => {
                assert_eq!(tool_name, "get_weather");
                assert_eq!(tool_args, args);
            }
            _ => panic!("Expected ToolCall event"),
        }
    }

    #[test]
    fn test_stream_event_done() {
        let event = StreamEvent::done(42, 100);
        assert_eq!(event.event_name(), "stream:done");
        match event {
            StreamEvent::Done {
                session_id,
                message_id,
                usage,
            } => {
                assert_eq!(session_id, 42);
                assert_eq!(message_id, 100);
                assert!(usage.is_none());
            }
            _ => panic!("Expected Done event"),
        }
    }

    #[test]
    fn test_stream_event_done_with_usage() {
        let usage = TokenUsage {
            input_tokens: Some(10),
            output_tokens: Some(20),
        };
        let event = StreamEvent::done_with_usage(42, 100, usage.clone());
        match event {
            StreamEvent::Done {
                session_id,
                message_id,
                usage: u,
            } => {
                assert_eq!(session_id, 42);
                assert_eq!(message_id, 100);
                assert_eq!(u, Some(usage));
            }
            _ => panic!("Expected Done event"),
        }
    }

    #[test]
    fn test_stream_event_error() {
        let event = StreamEvent::error("PROVIDER_ERROR", "提供商错误");
        assert_eq!(event.event_name(), "stream:error");
        match event {
            StreamEvent::Error {
                code,
                message,
                partial_content,
            } => {
                assert_eq!(code, "PROVIDER_ERROR");
                assert_eq!(message, "提供商错误");
                assert!(partial_content.is_none());
            }
            _ => panic!("Expected Error event"),
        }
    }

    #[test]
    fn test_stream_event_error_with_partial() {
        let event = StreamEvent::error_with_partial(
            "CANCELLED",
            "用户取消",
            "Partial response...",
        );
        match event {
            StreamEvent::Error {
                code,
                message,
                partial_content,
            } => {
                assert_eq!(code, "CANCELLED");
                assert_eq!(message, "用户取消");
                assert_eq!(partial_content, Some("Partial response...".to_string()));
            }
            _ => panic!("Expected Error event"),
        }
    }

    #[test]
    fn test_stream_event_serialize() {
        let event = StreamEvent::delta("Hello");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"delta\""));
        assert!(json.contains("\"delta\":\"Hello\""));
    }

    #[test]
    fn test_stream_event_deserialize() {
        let json = r#"{"type":"delta","delta":"Hello"}"#;
        let event: StreamEvent = serde_json::from_str(json).unwrap();
        match event {
            StreamEvent::Delta { delta, .. } => assert_eq!(delta, "Hello"),
            _ => panic!("Expected Delta event"),
        }
    }

    // ============================================================================
    // StreamAccumulator Tests
    // ============================================================================

    #[test]
    fn test_accumulator_new() {
        let acc = StreamAccumulator::new();
        assert!(acc.content.is_empty());
        assert!(acc.reasoning.is_empty());
        assert!(acc.usage.is_none());
        assert!(acc.finish_reason.is_none());
        assert!(!acc.cancelled);
        assert!(!acc.has_content());
    }

    #[test]
    fn test_accumulator_process_chunk() {
        let mut acc = StreamAccumulator::new();
        let chunk = ChatStreamChunk::text_delta("Hello");
        acc.process_chunk(&chunk);
        assert_eq!(acc.content, "Hello");
        assert!(acc.has_content());
    }

    #[test]
    fn test_accumulator_process_multiple_chunks() {
        let mut acc = StreamAccumulator::new();
        acc.process_chunk(&ChatStreamChunk::text_delta("Hello"));
        acc.process_chunk(&ChatStreamChunk::text_delta(" "));
        acc.process_chunk(&ChatStreamChunk::text_delta("World"));
        assert_eq!(acc.content, "Hello World");
    }

    #[test]
    fn test_accumulator_process_chunk_with_reasoning() {
        let mut acc = StreamAccumulator::new();
        let chunk = ChatStreamChunk {
            delta: Some("Answer".to_string()),
            reasoning_content: Some("Thinking...".to_string()),
            ..Default::default()
        };
        acc.process_chunk(&chunk);
        assert_eq!(acc.content, "Answer");
        assert_eq!(acc.reasoning, "Thinking...");
    }

    #[test]
    fn test_accumulator_cancel() {
        let mut acc = StreamAccumulator::new();
        acc.process_chunk(&ChatStreamChunk::text_delta("Partial"));
        acc.cancel();
        assert!(acc.cancelled);
        assert_eq!(acc.final_content(), "Partial");
    }

    // ============================================================================
    // ActiveStream Tests
    // ============================================================================

    #[test]
    fn test_active_stream_new() {
        let stream = ActiveStream::new(42);
        assert_eq!(stream.session_id, 42);
        assert!(!stream.request_id.is_empty());
        assert!(!stream.cancelled);
    }

    #[test]
    fn test_active_stream_cancel() {
        let mut stream = ActiveStream::new(42);
        stream.cancel();
        assert!(stream.cancelled);
    }

    #[test]
    fn test_active_stream_elapsed() {
        let stream = ActiveStream::new(42);
        // Should be very fast
        assert!(stream.elapsed() < Duration::from_millis(100));
    }

    // ============================================================================
    // StreamManager Tests
    // ============================================================================

    #[tokio::test]
    async fn test_stream_manager_register() {
        let manager = StreamManager::new();
        let stream = manager.register(42).await;
        assert_eq!(stream.session_id, 42);

        let retrieved = manager.get(42).await;
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_stream_manager_cancel() {
        let manager = StreamManager::new();
        manager.register(42).await;

        let cancelled = manager.cancel(42).await;
        assert!(cancelled);

        let is_cancelled = manager.is_cancelled(42).await;
        assert!(is_cancelled);
    }

    #[tokio::test]
    async fn test_stream_manager_cancel_nonexistent() {
        let manager = StreamManager::new();
        let cancelled = manager.cancel(42).await;
        assert!(!cancelled);
    }

    #[tokio::test]
    async fn test_stream_manager_remove() {
        let manager = StreamManager::new();
        manager.register(42).await;

        let removed = manager.remove(42).await;
        assert!(removed.is_some());

        let retrieved = manager.get(42).await;
        assert!(retrieved.is_none());
    }

    // ============================================================================
    // Error Code Tests
    // ============================================================================

    #[test]
    fn test_stream_error_conversion() {
        let error = StreamError::Connection("timeout".to_string());
        let (code, msg) = stream_error_to_code_and_message(&error);
        assert_eq!(code, error_codes::CONNECTION_ERROR);
        assert!(msg.contains("连接错误"));

        let error = StreamError::RateLimit;
        let (code, msg) = stream_error_to_code_and_message(&error);
        assert_eq!(code, error_codes::RATE_LIMIT);
        assert!(msg.contains("频率超限"));
    }
}