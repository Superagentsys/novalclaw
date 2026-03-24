//! Context Overflow Processor
//!
//! Handles context window overflow by applying strategies like truncation or summarization.
//! [Source: Story 7.2 - 上下文窗口配置]

use crate::agent::{ContextWindowConfig, OverflowStrategy, TokenCounter};
use crate::session::Message;
use anyhow::{bail, Result};

/// Context overflow processor
///
/// Handles cases where the context window exceeds the configured limit
/// by applying the selected overflow strategy.
pub struct ContextOverflowProcessor {
    /// Token counter for estimating token usage
    token_counter: TokenCounter,
}

impl ContextOverflowProcessor {
    /// Create a new overflow processor
    pub fn new() -> Self {
        Self {
            token_counter: TokenCounter::new(),
        }
    }

    /// Create with a custom token counter
    pub fn with_token_counter(token_counter: TokenCounter) -> Self {
        Self { token_counter }
    }

    /// Process messages when context exceeds limit
    ///
    /// # Arguments
    /// * `messages` - The list of messages to process
    /// * `config` - The context window configuration
    /// * `system_prompt` - Optional system prompt to include in token count
    ///
    /// # Returns
    /// The processed list of messages that fits within the token limit
    pub fn process(
        &self,
        messages: Vec<Message>,
        config: &ContextWindowConfig,
        system_prompt: Option<&str>,
    ) -> Result<Vec<Message>> {
        if messages.is_empty() {
            return Ok(messages);
        }

        let max_tokens = config.effective_message_tokens();
        if max_tokens == 0 {
            // No effective limit (or misconfigured), return as-is
            return Ok(messages);
        }

        let current_tokens = self.count_tokens(&messages, system_prompt, config.include_system_prompt);

        if current_tokens <= max_tokens {
            return Ok(messages);
        }

        match config.overflow_strategy {
            OverflowStrategy::Truncate => {
                self.truncate_oldest(messages, max_tokens, system_prompt, config.include_system_prompt)
            }
            OverflowStrategy::Summarize => {
                // For MVP, summarization falls back to truncation
                // TODO: Implement actual summarization with LLM
                self.truncate_oldest(messages, max_tokens, system_prompt, config.include_system_prompt)
            }
            OverflowStrategy::Error => {
                bail!(
                    "Context window exceeded ({} > {} tokens)",
                    current_tokens,
                    max_tokens
                )
            }
        }
    }

    /// Count tokens for messages with optional system prompt
    fn count_tokens(
        &self,
        messages: &[Message],
        system_prompt: Option<&str>,
        include_system: bool,
    ) -> usize {
        let system = system_prompt.unwrap_or("");
        TokenCounter::count_context(system, messages, include_system)
    }

    /// Truncate oldest messages to fit within token limit
    ///
    /// Removes messages from the beginning of the conversation until
    /// the remaining messages fit within the token limit.
    fn truncate_oldest(
        &self,
        mut messages: Vec<Message>,
        max_tokens: usize,
        system_prompt: Option<&str>,
        include_system: bool,
    ) -> Result<Vec<Message>> {
        let system = system_prompt.unwrap_or("");

        // Keep removing oldest messages until under limit
        while !messages.is_empty() {
            let current_tokens = self.count_tokens(&messages, system_prompt, include_system);
            if current_tokens <= max_tokens {
                break;
            }
            messages.remove(0);
        }

        // Safety check: if we still exceed the limit with a single message
        if messages.len() == 1 {
            let tokens = self.count_tokens(&messages, system_prompt, include_system);
            if tokens > max_tokens {
                // Even a single message is too large
                // This is a degenerate case; we return empty rather than error
                return Ok(vec![]);
            }
        }

        Ok(messages)
    }

    /// Check if messages would exceed the context limit
    pub fn would_exceed(
        &self,
        messages: &[Message],
        config: &ContextWindowConfig,
        system_prompt: Option<&str>,
    ) -> bool {
        if messages.is_empty() {
            return false;
        }

        let max_tokens = config.effective_message_tokens();
        if max_tokens == 0 {
            return false;
        }

        let current_tokens = self.count_tokens(messages, system_prompt, config.include_system_prompt);
        current_tokens > max_tokens
    }

    /// Get the current token count for messages
    pub fn get_token_count(
        &self,
        messages: &[Message],
        config: &ContextWindowConfig,
        system_prompt: Option<&str>,
    ) -> usize {
        self.count_tokens(messages, system_prompt, config.include_system_prompt)
    }

    /// Get the number of messages that can be added before overflow
    pub fn available_space(
        &self,
        messages: &[Message],
        config: &ContextWindowConfig,
        system_prompt: Option<&str>,
    ) -> usize {
        let max_tokens = config.effective_message_tokens();
        if max_tokens == 0 {
            return usize::MAX;
        }

        let current_tokens = self.count_tokens(messages, system_prompt, config.include_system_prompt);
        if current_tokens >= max_tokens {
            return 0;
        }

        max_tokens - current_tokens
    }
}

impl Default for ContextOverflowProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::MessageRole;

    fn create_test_message(id: i64, content: &str) -> Message {
        Message {
            id,
            session_id: 1,
            role: MessageRole::User,
            content: content.to_string(),
            created_at: 0,
            quote_message_id: None,
            is_marked: false,
        }
    }

    fn create_test_messages(count: usize, content_per_message: &str) -> Vec<Message> {
        (0..count)
            .map(|i| create_test_message(i as i64, content_per_message))
            .collect()
    }

    #[test]
    fn test_process_empty_messages() {
        let processor = ContextOverflowProcessor::new();
        let config = ContextWindowConfig::default();
        let messages: Vec<Message> = vec![];

        let result = processor.process(messages.clone(), &config, None).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_process_within_limit() {
        let processor = ContextOverflowProcessor::new();
        let config = ContextWindowConfig::new(10000); // Large limit

        let messages = create_test_messages(3, "Hello");

        let result = processor.process(messages.clone(), &config, None).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_process_truncate_strategy() {
        let processor = ContextOverflowProcessor::new();
        // Use a very small max_tokens to force truncation
        let config = ContextWindowConfig::new(20)
            .with_response_reserve(0)
            .with_overflow_strategy(OverflowStrategy::Truncate);

        // Create messages that will exceed the limit
        let messages = create_test_messages(10, "This is a longer message that will take more tokens.");

        let result = processor.process(messages.clone(), &config, None).unwrap();
        // Should have fewer messages after truncation
        assert!(result.len() < 10);
    }

    #[test]
    fn test_process_error_strategy() {
        let processor = ContextOverflowProcessor::new();
        // Use a very small max_tokens to force overflow
        let config = ContextWindowConfig::new(20)
            .with_response_reserve(0)
            .with_overflow_strategy(OverflowStrategy::Error);

        let messages = create_test_messages(5, "This is a test message with enough content to exceed.");

        let result = processor.process(messages.clone(), &config, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Context window exceeded"));
    }

    #[test]
    fn test_process_summarize_strategy_falls_back_to_truncate() {
        let processor = ContextOverflowProcessor::new();
        // Use a very small max_tokens to force truncation
        let config = ContextWindowConfig::new(20)
            .with_response_reserve(0)
            .with_overflow_strategy(OverflowStrategy::Summarize);

        let messages = create_test_messages(10, "This is a longer message that will take more tokens.");

        // Summarize should fall back to truncate for MVP
        let result = processor.process(messages.clone(), &config, None).unwrap();
        assert!(result.len() < 10);
    }

    #[test]
    fn test_would_exceed_false() {
        let processor = ContextOverflowProcessor::new();
        let config = ContextWindowConfig::new(10000);

        let messages = create_test_messages(3, "Hello");

        assert!(!processor.would_exceed(&messages, &config, None));
    }

    #[test]
    fn test_would_exceed_true() {
        let processor = ContextOverflowProcessor::new();
        // Use a very small max_tokens to force overflow
        let config = ContextWindowConfig::new(20)
            .with_response_reserve(0);

        let messages = create_test_messages(5, "This is a longer message to exceed the limit.");

        assert!(processor.would_exceed(&messages, &config, None));
    }

    #[test]
    fn test_get_token_count() {
        let processor = ContextOverflowProcessor::new();
        let config = ContextWindowConfig::default();

        let messages = create_test_messages(2, "Hello world");

        let count = processor.get_token_count(&messages, &config, None);
        assert!(count > 0);
    }

    #[test]
    fn test_available_space() {
        let processor = ContextOverflowProcessor::new();
        let config = ContextWindowConfig::new(1000);

        let messages = create_test_messages(2, "Hello");

        let space = processor.available_space(&messages, &config, None);
        assert!(space > 0);
    }

    #[test]
    fn test_available_space_zero_when_exceeded() {
        let processor = ContextOverflowProcessor::new();
        // Use a very small max_tokens to force overflow
        let config = ContextWindowConfig::new(20)
            .with_response_reserve(0);

        let messages = create_test_messages(5, "This is a longer message to exceed.");

        let space = processor.available_space(&messages, &config, None);
        assert_eq!(space, 0);
    }

    #[test]
    fn test_process_with_system_prompt() {
        let processor = ContextOverflowProcessor::new();
        let config = ContextWindowConfig::new(100)
            .with_include_system_prompt(true);

        let messages = create_test_messages(3, "Test message");
        let system_prompt = "You are a helpful assistant.";

        let result = processor.process(messages.clone(), &config, Some(system_prompt)).unwrap();
        // Should process successfully
        assert!(!result.is_empty() || messages.is_empty());
    }

    #[test]
    fn test_process_without_system_prompt_in_count() {
        let processor = ContextOverflowProcessor::new();
        let config = ContextWindowConfig::new(100)
            .with_include_system_prompt(false);

        let messages = create_test_messages(3, "Test message");
        let system_prompt = "You are a helpful assistant.";

        let result = processor.process(messages.clone(), &config, Some(system_prompt)).unwrap();
        // Should process successfully
        assert!(!result.is_empty() || messages.is_empty());
    }

    #[test]
    fn test_truncate_preserves_newest_messages() {
        let processor = ContextOverflowProcessor::new();
        let config = ContextWindowConfig::new(50)
            .with_overflow_strategy(OverflowStrategy::Truncate);

        let messages = vec![
            create_test_message(1, "Oldest message to be removed"),
            create_test_message(2, "Middle message"),
            create_test_message(3, "Newest message to keep"),
        ];

        let result = processor.process(messages.clone(), &config, None).unwrap();
        // The newest message should be preserved
        if !result.is_empty() {
            assert!(result.last().map(|m| m.content.contains("Newest")).unwrap_or(false));
        }
    }

    #[test]
    fn test_processor_default() {
        let processor = ContextOverflowProcessor::default();
        let config = ContextWindowConfig::new(10000);
        let messages = create_test_messages(2, "Hello");

        let result = processor.process(messages, &config, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_zero_effective_tokens_returns_as_is() {
        let processor = ContextOverflowProcessor::new();
        // Misconfigured: response_reserve >= max_tokens
        let config = ContextWindowConfig::new(100)
            .with_response_reserve(200);

        let messages = create_test_messages(5, "Hello world");

        // Should return messages as-is when effective_message_tokens is 0
        let result = processor.process(messages.clone(), &config, None).unwrap();
        assert_eq!(result.len(), 5);
    }
}