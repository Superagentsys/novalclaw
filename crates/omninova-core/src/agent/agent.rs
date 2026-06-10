use crate::config::AgentConfig;
use crate::agent::dispatcher::AgentDispatcher;
use crate::agent::prompt::bootstrap_system_messages;
use crate::memory::{Memory, MemoryCategory};
use crate::providers::{ChatMessage, Provider};
use crate::security::SecurityContext;
use crate::tools::{Tool, ToolSpec};
use anyhow::Result;
use std::sync::Arc;

pub struct Agent {
    provider: Box<dyn Provider>,
    tools: Vec<Box<dyn Tool>>,
    tool_specs: Vec<ToolSpec>,
    memory: Arc<dyn Memory>,
    config: AgentConfig,
    security: SecurityContext,
    messages: Vec<ChatMessage>,
}

impl Agent {
    pub fn new(
        provider: Box<dyn Provider>,
        tools: Vec<Box<dyn Tool>>,
        memory: Arc<dyn Memory>,
        config: AgentConfig,
        security: SecurityContext,
    ) -> Self {
        let tool_specs = tools.iter().map(|t| t.spec()).collect();
        Self {
            provider,
            tools,
            tool_specs,
            memory,
            config,
            security,
            messages: Vec::new(),
        }
    }

    pub async fn process_message(&mut self, message: &str) -> Result<String> {
        self.process_message_with_images(message, &[]).await
    }

    pub async fn process_message_with_images(
        &mut self,
        message: &str,
        images: &[String],
    ) -> Result<String> {
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

        if images.is_empty() {
            self.messages.push(ChatMessage::user(message));
        } else {
            self.messages
                .push(ChatMessage::user_with_images(message, images.to_vec()));
        }
        let dispatcher = AgentDispatcher::new(
            self.provider.as_ref(),
            &self.tools,
            &self.tool_specs,
            self.config.max_tool_iterations,
            &self.security,
        );
        dispatcher.run(&mut self.messages).await
    }

    pub fn import_messages(&mut self, messages: Vec<ChatMessage>) {
        self.messages = messages;
    }

    pub fn export_messages(&self) -> Vec<ChatMessage> {
        self.messages.clone()
    }
}
