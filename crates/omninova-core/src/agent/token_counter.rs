//! Token Counter Service
//!
//! Provides token counting for context window management.
//! [Source: Story 7.2 - 上下文窗口配置]

use crate::session::{Message, MessageRole};

/// Token counter for estimating token usage
///
/// Uses a simplified estimation method:
/// - English: ~4 characters per token
/// - Chinese/CJK: ~2 characters per token
pub struct TokenCounter;

impl TokenCounter {
    /// Create a new TokenCounter instance
    pub fn new() -> Self {
        Self
    }

    /// Estimate token count for text
    ///
    /// Uses simplified heuristic:
    /// - ASCII text: ~4 characters per token
    /// - CJK characters: ~2 characters per token
    pub fn estimate(text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }

        let mut ascii_count = 0;
        let mut cjk_count = 0;

        for ch in text.chars() {
            if Self::is_cjk(ch) {
                cjk_count += 1;
            } else {
                ascii_count += 1;
            }
        }

        // ASCII: ~4 chars per token, CJK: ~2 chars per token
        let ascii_tokens = (ascii_count + 3) / 4; // ceiling division
        let cjk_tokens = (cjk_count + 1) / 2; // ceiling division

        ascii_tokens + cjk_tokens
    }

    /// Check if a character is CJK (Chinese, Japanese, Korean)
    fn is_cjk(ch: char) -> bool {
        let cp = ch as u32;
        // CJK Unified Ideographs
        (cp >= 0x4E00 && cp <= 0x9FFF)
            // CJK Unified Ideographs Extension A
            || (cp >= 0x3400 && cp <= 0x4DBF)
            // CJK Unified Ideographs Extension B-F
            || (cp >= 0x20000 && cp <= 0x2CEAF)
            // CJK Compatibility Ideographs
            || (cp >= 0xF900 && cp <= 0xFAFF)
            // Japanese Hiragana and Katakana
            || (cp >= 0x3040 && cp <= 0x30FF)
            // Korean Hangul
            || (cp >= 0xAC00 && cp <= 0xD7AF)
    }

    /// Count tokens for a conversation (list of messages)
    ///
    /// Each message adds overhead for role and formatting (~4 tokens)
    pub fn count_conversation(messages: &[Message]) -> usize {
        messages
            .iter()
            .map(|m| Self::estimate(&m.content) + 4) // +4 for role overhead
            .sum()
    }

    /// Count tokens for agent context (system prompt + messages)
    ///
    /// # Arguments
    /// * `system_prompt` - The system prompt text
    /// * `messages` - List of conversation messages
    /// * `include_system` - Whether to include system prompt in count
    pub fn count_context(
        system_prompt: &str,
        messages: &[Message],
        include_system: bool,
    ) -> usize {
        let mut total = 0;

        if include_system && !system_prompt.is_empty() {
            total += Self::estimate(system_prompt) + 4; // +4 for role overhead
        }

        total += Self::count_conversation(messages);
        total
    }

    /// Estimate tokens for a single message with role
    pub fn count_message(message: &Message) -> usize {
        Self::estimate(&message.content) + 4 // +4 for role overhead
    }

    /// Count tokens for a string slice (convenience method)
    pub fn count_text(text: &str) -> usize {
        Self::estimate(text)
    }
}

impl Default for TokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_ascii() {
        // "Hello, world!" = 13 chars, ~4 tokens
        let count = TokenCounter::estimate("Hello, world!");
        assert!(count >= 3 && count <= 4);

        // Simple English text
        let count = TokenCounter::estimate("This is a simple test.");
        assert!(count > 0);
    }

    #[test]
    fn test_estimate_cjk() {
        // Chinese characters
        let count = TokenCounter::estimate("你好世界"); // 4 CJK chars
        assert!(count >= 2 && count <= 3);

        // Japanese
        let count = TokenCounter::estimate("こんにちは"); // 5 hiragana
        assert!(count > 0);

        // Korean
        let count = TokenCounter::estimate("안녕하세요"); // 5 hangul
        assert!(count > 0);
    }

    #[test]
    fn test_estimate_mixed() {
        // Mixed ASCII and CJK
        let count = TokenCounter::estimate("Hello 你好 World 世界");
        assert!(count > 0);
    }

    #[test]
    fn test_estimate_empty() {
        assert_eq!(TokenCounter::estimate(""), 0);
    }

    #[test]
    fn test_estimate_whitespace() {
        let count = TokenCounter::estimate("   ");
        assert!(count >= 1);
    }

    #[test]
    fn test_count_conversation_empty() {
        let messages: Vec<Message> = vec![];
        assert_eq!(TokenCounter::count_conversation(&messages), 0);
    }

    #[test]
    fn test_count_conversation_single() {
        let messages = vec![Message {
            id: 1,
            session_id: 1,
            role: MessageRole::User,
            content: "Hello".to_string(),
            created_at: 0,
            quote_message_id: None,
            is_marked: false,
        }];
        let count = TokenCounter::count_conversation(&messages);
        // "Hello" = ~2 tokens + 4 overhead = ~6
        assert!(count >= 5 && count <= 7);
    }

    #[test]
    fn test_count_conversation_multiple() {
        let messages = vec![
            Message {
                id: 1,
                session_id: 1,
                role: MessageRole::User,
                content: "Hello".to_string(),
                created_at: 0,
                quote_message_id: None,
                is_marked: false,
            },
            Message {
                id: 2,
                session_id: 1,
                role: MessageRole::Assistant,
                content: "Hi there!".to_string(),
                created_at: 0,
                quote_message_id: None,
                is_marked: false,
            },
        ];
        let count = TokenCounter::count_conversation(&messages);
        // Each message has ~4 token overhead
        assert!(count > 8); // At least 8 for overhead
    }

    #[test]
    fn test_count_context_with_system() {
        let messages = vec![Message {
            id: 1,
            session_id: 1,
            role: MessageRole::User,
            content: "Hello".to_string(),
            created_at: 0,
            quote_message_id: None,
            is_marked: false,
        }];
        let count = TokenCounter::count_context(
            "You are a helpful assistant.",
            &messages,
            true,
        );
        assert!(count > 0);
    }

    #[test]
    fn test_count_context_without_system() {
        let messages = vec![Message {
            id: 1,
            session_id: 1,
            role: MessageRole::User,
            content: "Hello".to_string(),
            created_at: 0,
            quote_message_id: None,
            is_marked: false,
        }];
        let count_with = TokenCounter::count_context(
            "You are a helpful assistant.",
            &messages,
            true,
        );
        let count_without = TokenCounter::count_context(
            "You are a helpful assistant.",
            &messages,
            false,
        );
        assert!(count_with > count_without);
    }

    #[test]
    fn test_is_cjk() {
        // Chinese
        assert!(TokenCounter::is_cjk('中'));
        assert!(TokenCounter::is_cjk('文'));

        // Japanese
        assert!(TokenCounter::is_cjk('あ'));
        assert!(TokenCounter::is_cjk('ア'));

        // Korean
        assert!(TokenCounter::is_cjk('한'));
        assert!(TokenCounter::is_cjk('국'));

        // ASCII
        assert!(!TokenCounter::is_cjk('A'));
        assert!(!TokenCounter::is_cjk('z'));
        assert!(!TokenCounter::is_cjk('0'));
    }

    #[test]
    fn test_count_message() {
        let message = Message {
            id: 1,
            session_id: 1,
            role: MessageRole::User,
            content: "Hello, world!".to_string(),
            created_at: 0,
            quote_message_id: None,
            is_marked: false,
        };
        let count = TokenCounter::count_message(&message);
        // "Hello, world!" ~4 tokens + 4 overhead = ~8
        assert!(count >= 7 && count <= 9);
    }

    #[test]
    fn test_count_text() {
        let count = TokenCounter::count_text("Hello, world!");
        assert!(count > 0);
    }

    #[test]
    fn test_token_counter_default() {
        let _counter = TokenCounter::default();
        let count = TokenCounter::estimate("Test");
        assert!(count > 0);
    }
}