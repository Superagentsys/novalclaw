use crate::tools::ToolSpec;
use crate::providers::context_budget::ContextBudget;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// OpenAI 兼容视觉输入：`data:image/...;base64,...` 或 https URL。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
    /// When a tool result is pruned for the model-visible context, this holds
    /// the full original tool output so durable session storage can preserve
    /// it without sending it to the Provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_tool_content: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
            images: None,
            original_tool_content: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
            images: None,
            original_tool_content: None,
        }
    }

    pub fn user_with_images(content: impl Into<String>, images: Vec<String>) -> Self {
        let images = if images.is_empty() {
            None
        } else {
            Some(images)
        };
        Self {
            role: "user".into(),
            content: content.into(),
            images,
            original_tool_content: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            images: None,
            original_tool_content: None,
        }
    }

    pub fn tool(content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: content.into(),
            images: None,
            original_tool_content: None,
        }
    }

    /// 持久化会话历史时去掉截图，避免 JSON 膨胀。
    pub fn strip_images_for_history(mut self) -> Self {
        self.images = None;
        self
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
#[derive(Debug, Clone, Default)]
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
    /// Provider-reported completion reason, when the API exposes one (for
    /// example `stop`, `tool_calls`, or `length`). This is diagnostic metadata;
    /// it does not by itself mean that user-visible text was produced.
    pub finish_reason: Option<String>,
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

/// Core provider trait — implement for any LLM API (OpenAI, Anthropic, etc.)
#[async_trait]
pub trait Provider: Send + Sync {
    /// Provider name (e.g., "openai", "anthropic")
    fn name(&self) -> &str;

    /// Optional model id this provider will send. Default is unknown.
    fn model(&self) -> Option<&str> {
        None
    }

    /// Send a chat request to the LLM
    async fn chat(&self, request: ChatRequest<'_>) -> anyhow::Result<ChatResponse>;

    /// Streaming variant: forwards text deltas over `token_tx` as they arrive
    /// and returns the fully-assembled response (text + tool calls).
    ///
    /// The default implementation falls back to the non-streaming [`chat`] and
    /// emits the whole text as a single delta, so providers without streaming
    /// support still work transparently.
    async fn chat_stream(
        &self,
        request: ChatRequest<'_>,
        token_tx: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> anyhow::Result<ChatResponse> {
        let response = self.chat(request).await?;
        if let Some(text) = response.text.as_deref() {
            if !text.is_empty() {
                let _ = token_tx.send(text.to_string());
            }
        }
        Ok(response)
    }

    /// Check if the provider is healthy
    async fn health_check(&self) -> bool;

    /// Returns the authoritative context budget if one is known.
    fn context_budget(&self) -> Option<ContextBudget> {
        None
    }
}
