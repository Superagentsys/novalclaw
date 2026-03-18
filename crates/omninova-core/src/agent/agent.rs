use crate::config::AgentConfig;
use crate::agent::dispatcher::AgentDispatcher;
use crate::agent::prompt::bootstrap_system_messages;
use crate::memory::{Memory, MemoryCategory};
use crate::providers::{ChatMessage, Provider};
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
            self.messages.extend(bootstrap_system_messages(&self.config));
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
        let dispatcher = AgentDispatcher::new(
            self.provider.as_ref(),
            &self.tools,
            &self.tool_specs,
            self.config.max_tool_iterations,
        );
        dispatcher.run(&mut self.messages).await.map_err(|e| anyhow::anyhow!(e.to_string()))
    }

    pub fn import_messages(&mut self, messages: Vec<ChatMessage>) {
        self.messages = messages;
    }

    pub fn export_messages(&self) -> Vec<ChatMessage> {
        self.messages.clone()
    }
}
