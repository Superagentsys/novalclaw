use crate::config::AgentConfig;
use crate::memory::{Memory, MemoryCategory};
use crate::providers::{ChatMessage, Provider, ToolCall};
use crate::tools::{Tool, ToolSpec};
use anyhow::Result;
use std::sync::Arc;

pub struct Agent {
    provider: Box<dyn Provider>,
    tools: Vec<Box<dyn Tool>>,
    tool_specs: Vec<ToolSpec>,
    memory: Arc<dyn Memory>,
    config: AgentConfig,
    messages: Vec<ChatMessage>,
}

impl Agent {
    pub fn new(
        provider: Box<dyn Provider>,
        tools: Vec<Box<dyn Tool>>,
        memory: Arc<dyn Memory>,
        config: AgentConfig,
    ) -> Self {
        let tool_specs = tools.iter().map(|t| t.spec()).collect();
        Self {
            provider,
            tools,
            tool_specs,
            memory,
            config,
            messages: Vec::new(),
        }
    }

    pub async fn process_message(&mut self, message: &str) -> Result<String> {
        if self.messages.is_empty() {
            if let Some(system_prompt) = self.config.system_prompt.as_deref() {
                if !system_prompt.trim().is_empty() {
                    self.messages.push(ChatMessage::system(system_prompt));
                }
            }
        }

        let _ = self
            .memory
            .store(
                &format!("conversation/{}", uuid::Uuid::new_v4()),
                message,
                MemoryCategory::Conversation,
                None,
            )
            .await;

        self.messages.push(ChatMessage::user(message));

        for _ in 0..8 {
            let response = self
                .provider
                .chat(crate::providers::ChatRequest {
                    messages: &self.messages,
                    tools: Some(&self.tool_specs),
                })
                .await?;

            if response.tool_calls.is_empty() {
                let text = response.text.unwrap_or_default();
                self.messages.push(ChatMessage::assistant(&text));
                return Ok(text);
            }

            let assistant_payload = serde_json::json!({
                "content": response.text,
                "reasoning_content": response.reasoning_content,
                "tool_calls": response.tool_calls,
            })
            .to_string();
            self.messages
                .push(ChatMessage::assistant(assistant_payload));

            for tool_call in response.tool_calls {
                let tool_result = self.execute_tool_call(&tool_call).await?;
                let tool_payload = serde_json::json!({
                    "tool_call_id": tool_call.id,
                    "content": tool_result,
                })
                .to_string();
                self.messages.push(ChatMessage::tool(tool_payload));
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
