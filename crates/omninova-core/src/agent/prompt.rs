use crate::config::AgentConfig;
use crate::providers::ChatMessage;

/// Build the initial system messages for a conversation.
pub fn bootstrap_system_messages(config: &AgentConfig) -> Vec<ChatMessage> {
    let mut messages = Vec::new();
    if let Some(system_prompt) = config.system_prompt.as_deref() {
        if !system_prompt.trim().is_empty() {
            messages.push(ChatMessage::system(system_prompt));
        }
    }
    messages
}
