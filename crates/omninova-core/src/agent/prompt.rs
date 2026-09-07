use crate::agent::history::SUMMARY_MARKER;
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

/// Rebuild the model-visible conversation for session-open projection.
///
/// This is side-effect-free: it does not append a user message, execute tools,
/// call a Provider, prune, compact, or otherwise mutate durable session state.
pub fn reconstruct_model_visible_messages(
    config: &AgentConfig,
    history: Vec<ChatMessage>,
) -> Vec<ChatMessage> {
    let mut messages = crate::skills::sanitize_skill_working_context(history);
    let bootstrap = bootstrap_system_messages(config);
    if bootstrap.is_empty() {
        return messages;
    }
    if let Some(first) = messages.first() {
        if first.role == "system" && !first.content.starts_with(SUMMARY_MARKER) {
            messages[0] = bootstrap.into_iter().next().unwrap();
            return messages;
        }
    }
    messages.splice(0..0, bootstrap);
    messages
}
