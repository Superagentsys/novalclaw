//! Agent Dispatcher for Tool-Calling Loop
//!
//! This module provides a stateless dispatcher that handles the iterative
//! conversation flow between the AI model and tool execution.

use crate::providers::{ChatMessage, ChatRequest, Provider, ToolCall};
use crate::tools::{Tool, ToolSpec};
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, error, info, instrument, warn};

/// Errors that can occur during dispatch operations
#[derive(Debug, Error)]
pub enum DispatchError {
    /// The requested agent was not found
    #[error("代理未找到: {0}")]
    AgentNotFound(i64),

    /// Session-related error
    #[error("会话错误: {0}")]
    SessionError(String),

    /// Provider/API error
    #[error("提供商错误: {0}")]
    ProviderError(String),

    /// Tool execution error
    #[error("工具执行错误: {0}")]
    ToolExecutionError(String),

    /// Context timeout exceeded
    #[error("上下文超时")]
    ContextTimeout,

    /// Maximum iterations exceeded
    #[error("工具调用迭代次数超限")]
    MaxIterationsExceeded,

    /// Unknown tool requested
    #[error("未知工具: {0}")]
    UnknownTool(String),

    /// Invalid tool arguments
    #[error("无效的工具参数: {0}")]
    InvalidToolArguments(String),

    /// Retry limit exceeded
    #[error("重试次数超限: {0} 次尝试后仍失败")]
    RetryLimitExceeded(u32),
}

impl From<serde_json::Error> for DispatchError {
    fn from(e: serde_json::Error) -> Self {
        DispatchError::InvalidToolArguments(e.to_string())
    }
}

/// Configuration for dispatcher behavior
#[derive(Debug, Clone)]
pub struct DispatcherConfig {
    /// Maximum number of tool-calling iterations
    pub max_tool_iterations: usize,
    /// Maximum time to wait for a single LLM response
    pub response_timeout: Duration,
    /// Number of retry attempts for transient failures
    pub retry_attempts: u32,
    /// Delay between retry attempts
    pub retry_delay: Duration,
}

impl Default for DispatcherConfig {
    fn default() -> Self {
        Self {
            max_tool_iterations: 10,
            response_timeout: Duration::from_secs(120),
            retry_attempts: 3,
            retry_delay: Duration::from_millis(500),
        }
    }
}

/// Stateless dispatcher for one agent turn:
/// model response -> tool call(s) -> tool result message(s) -> next model response.
pub struct AgentDispatcher<'a> {
    provider: &'a dyn Provider,
    tools: &'a [Box<dyn Tool>],
    tool_specs: &'a [ToolSpec],
    config: DispatcherConfig,
}

impl<'a> AgentDispatcher<'a> {
    /// Creates a new dispatcher with default configuration
    pub fn new(
        provider: &'a dyn Provider,
        tools: &'a [Box<dyn Tool>],
        tool_specs: &'a [ToolSpec],
        max_tool_iterations: usize,
    ) -> Self {
        Self {
            provider,
            tools,
            tool_specs,
            config: DispatcherConfig {
                max_tool_iterations,
                ..Default::default()
            },
        }
    }

    /// Creates a new dispatcher with custom configuration
    pub fn with_config(
        provider: &'a dyn Provider,
        tools: &'a [Box<dyn Tool>],
        tool_specs: &'a [ToolSpec],
        config: DispatcherConfig,
    ) -> Self {
        Self {
            provider,
            tools,
            tool_specs,
            config,
        }
    }

    /// Run the tool-calling loop against `messages` and return final assistant text.
    #[instrument(skip(self, messages), fields(tool_count = self.tools.len()))]
    pub async fn run(&self, messages: &mut Vec<ChatMessage>) -> Result<String, DispatchError> {
        let iteration_cap = self.config.max_tool_iterations.max(1);

        info!(
            iteration_cap = iteration_cap,
            message_count = messages.len(),
            "Starting dispatch loop"
        );

        for iteration in 0..iteration_cap {
            debug!(iteration = iteration, "Dispatch iteration");

            let response = self
                .call_provider_with_retry(messages)
                .await?;

            if response.tool_calls.is_empty() {
                let text = response.text.unwrap_or_default();
                messages.push(ChatMessage::assistant(&text));

                info!(
                    iteration = iteration,
                    response_length = text.len(),
                    "Dispatch completed successfully"
                );

                return Ok(text);
            }

            // Process tool calls
            let assistant_payload = serde_json::json!({
                "content": response.text,
                "reasoning_content": response.reasoning_content,
                "tool_calls": response.tool_calls,
            })
            .to_string();
            messages.push(ChatMessage::assistant(assistant_payload.clone()));

            debug!(
                tool_call_count = response.tool_calls.len(),
                "Processing tool calls"
            );

            for tool_call in &response.tool_calls {
                match self.execute_tool_call(tool_call).await {
                    Ok(tool_result) => {
                        let tool_payload = serde_json::json!({
                            "tool_call_id": tool_call.id,
                            "content": tool_result,
                        })
                        .to_string();
                        messages.push(ChatMessage::tool(tool_payload));
                    }
                    Err(e) => {
                        error!(error = %e, tool_name = %tool_call.name, "Tool execution failed");
                        // Still add a tool result message with the error
                        let tool_payload = serde_json::json!({
                            "tool_call_id": tool_call.id,
                            "content": format!("Error: {}", e),
                            "is_error": true,
                        })
                        .to_string();
                        messages.push(ChatMessage::tool(tool_payload));
                    }
                }
            }
        }

        warn!(
            max_iterations = iteration_cap,
            "Tool call iteration limit reached"
        );

        Err(DispatchError::MaxIterationsExceeded)
    }

    /// Call the provider with retry logic for transient failures
    async fn call_provider_with_retry(
        &self,
        messages: &[ChatMessage],
    ) -> Result<crate::providers::ChatResponse, DispatchError> {
        for attempt in 0..=self.config.retry_attempts {
            match self
                .provider
                .chat(ChatRequest {
                    messages,
                    tools: if self.tool_specs.is_empty() {
                        None
                    } else {
                        Some(self.tool_specs)
                    },
                })
                .await
            {
                Ok(response) => {
                    if attempt > 0 {
                        info!(attempt = attempt, "Provider call succeeded after retry");
                    }
                    return Ok(response);
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    warn!(
                        attempt = attempt,
                        max_attempts = self.config.retry_attempts,
                        error = %error_msg,
                        "Provider call failed"
                    );

                    // Don't retry on the last attempt
                    if attempt < self.config.retry_attempts {
                        tokio::time::sleep(self.config.retry_delay).await;
                    } else {
                        // Return error with context on final failed attempt
                        return Err(DispatchError::RetryLimitExceeded(
                            self.config.retry_attempts + 1,
                        ));
                    }
                }
            }
        }

        // This should never be reached, but just in case
        Err(DispatchError::RetryLimitExceeded(
            self.config.retry_attempts + 1,
        ))
    }

    async fn execute_tool_call(&self, tool_call: &ToolCall) -> Result<String, DispatchError> {
        let tool = self
            .tools
            .iter()
            .find(|t| t.name() == tool_call.name)
            .ok_or_else(|| DispatchError::UnknownTool(tool_call.name.clone()))?;

        debug!(tool_name = %tool_call.name, "Executing tool");

        let args: serde_json::Value = serde_json::from_str(&tool_call.arguments)?;
        let result = tool.execute(args).await.map_err(|e| {
            error!(error = %e, tool_name = %tool_call.name, "Tool execution error");
            DispatchError::ToolExecutionError(e.to_string())
        })?;

        if result.success {
            debug!(tool_name = %tool_call.name, "Tool execution successful");
            Ok(result.output)
        } else {
            let error_msg = result
                .error
                .unwrap_or_else(|| "tool execution failed".to_string());
            warn!(tool_name = %tool_call.name, error = %error_msg, "Tool execution returned error");
            Ok(error_msg)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispatch_error_display() {
        let err = DispatchError::AgentNotFound(42);
        assert!(err.to_string().contains("42"));

        let err = DispatchError::ProviderError("API error".to_string());
        assert!(err.to_string().contains("API error"));

        let err = DispatchError::MaxIterationsExceeded;
        assert!(err.to_string().contains("迭代次数"));
    }

    #[test]
    fn test_dispatcher_config_default() {
        let config = DispatcherConfig::default();
        assert_eq!(config.max_tool_iterations, 10);
        assert_eq!(config.retry_attempts, 3);
    }

    #[test]
    fn test_serde_error_conversion() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid");
        assert!(json_err.is_err());

        let dispatch_err: DispatchError = json_err.unwrap_err().into();
        assert!(matches!(dispatch_err, DispatchError::InvalidToolArguments(_)));
    }
}