use crate::providers::{ChatMessage, ChatRequest, Provider, ToolCall};
use crate::tools::{Tool, ToolSpec};
use anyhow::Result;

/// Stateless dispatcher for one agent turn:
/// model response -> tool call(s) -> tool result message(s) -> next model response.
pub struct AgentDispatcher<'a> {
    provider: &'a dyn Provider,
    tools: &'a [Box<dyn Tool>],
    tool_specs: &'a [ToolSpec],
    max_tool_iterations: usize,
}

impl<'a> AgentDispatcher<'a> {
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
            max_tool_iterations,
        }
    }

    /// Run the tool-calling loop against `messages` and return final assistant text.
    pub async fn run(&self, messages: &mut Vec<ChatMessage>) -> Result<String> {
        let iteration_cap = self.max_tool_iterations.max(1);

        for _ in 0..iteration_cap {
            let response = self
                .provider
                .chat(ChatRequest {
                    messages,
                    tools: if self.tool_specs.is_empty() {
                        None
                    } else {
                        Some(self.tool_specs)
                    },
                })
                .await?;

            if response.tool_calls.is_empty() {
                let text = response.text.unwrap_or_default();
                messages.push(ChatMessage::assistant(&text));
                return Ok(text);
            }

            let assistant_payload = serde_json::json!({
                "content": response.text,
                "reasoning_content": response.reasoning_content,
                "tool_calls": response.tool_calls,
            })
            .to_string();
            messages.push(ChatMessage::assistant(assistant_payload));

            for tool_call in response.tool_calls {
                let tool_result = self.execute_tool_call(&tool_call).await?;
                let tool_payload = serde_json::json!({
                    "tool_call_id": tool_call.id,
                    "content": tool_result,
                })
                .to_string();
                messages.push(ChatMessage::tool(tool_payload));
            }
        }

        Ok("tool call loop limit reached".to_string())
    }

    async fn execute_tool_call(&self, tool_call: &ToolCall) -> Result<String> {
        let tool = self
            .tools
            .iter()
            .find(|t| t.name() == tool_call.name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {}", tool_call.name))?;

        let args: serde_json::Value = serde_json::from_str(&tool_call.arguments)
            .map_err(|e| anyhow::anyhow!("Invalid tool arguments JSON: {e}"))?;
        let result = tool.execute(args).await?;

        if result.success {
            Ok(result.output)
        } else {
            Ok(result
                .error
                .unwrap_or_else(|| "tool execution failed".to_string()))
        }
    }
}
