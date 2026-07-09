use async_trait::async_trait;
use crate::agent::AgentCancellationToken;
use serde::{Deserialize, Serialize};

/// Result of a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

/// Description of a tool for the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Channel sender for streaming tool output.
/// Each tuple is `(content, is_stderr)`:
///   - `(line, false)` → stdout line
///   - `(line, true)`  → stderr line
///
/// Completion is signaled by `tool_completed` / `tool_failed`, never by this channel.
pub type OutputSender = tokio::sync::mpsc::UnboundedSender<(String, bool)>;

/// Core tool trait — implement for any capability
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name (used in LLM function calling)
    fn name(&self) -> &str;

    /// Human-readable description
    fn description(&self) -> &str;

    /// JSON schema for parameters
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool with given arguments
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult>;

    /// Execute the tool with a cancellation token.
    /// Default implementation checks before and after the regular execution.
    async fn execute_with_cancel(
        &self,
        args: serde_json::Value,
        cancel_token: AgentCancellationToken,
    ) -> anyhow::Result<ToolResult> {
        cancel_token.check()?;
        let result = self.execute(args).await;
        cancel_token.check()?;
        result
    }

    /// Streaming variant: sends output chunks via `output_tx`.
    /// Default implementation falls back to `execute` (no streaming).
    async fn execute_streaming(
        &self,
        args: serde_json::Value,
        _output_tx: OutputSender,
    ) -> anyhow::Result<ToolResult> {
        self.execute(args).await
    }

    /// Streaming variant with cancellation.
    async fn execute_streaming_with_cancel(
        &self,
        args: serde_json::Value,
        output_tx: OutputSender,
        cancel_token: AgentCancellationToken,
    ) -> anyhow::Result<ToolResult> {
        cancel_token.check()?;
        let result = self.execute_streaming(args, output_tx).await;
        cancel_token.check()?;
        result
    }

    /// Get the full spec for LLM registration
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters_schema(),
        }
    }
}
