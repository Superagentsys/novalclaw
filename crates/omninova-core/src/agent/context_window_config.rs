//! Agent Context Window Configuration
//!
//! Defines context window settings for AI agents.
//! [Source: Story 7.2 - 上下文窗口配置]

use serde::{Deserialize, Serialize};

/// Overflow strategy when context window is exceeded
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OverflowStrategy {
    /// Truncate oldest messages when limit is reached
    #[default]
    Truncate,
    /// Summarize old messages (requires LLM call)
    Summarize,
    /// Return error when limit is reached
    Error,
}

impl std::fmt::Display for OverflowStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncate => write!(f, "truncate"),
            Self::Summarize => write!(f, "summarize"),
            Self::Error => write!(f, "error"),
        }
    }
}

impl std::str::FromStr for OverflowStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "truncate" => Ok(Self::Truncate),
            "summarize" => Ok(Self::Summarize),
            "error" => Ok(Self::Error),
            _ => Err(format!("Invalid overflow strategy: {}", s)),
        }
    }
}

/// Context window configuration for controlling token limits
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextWindowConfig {
    /// Maximum context window size in tokens (0 = use model default)
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    /// Strategy when context exceeds limit
    #[serde(default)]
    pub overflow_strategy: OverflowStrategy,
    /// Whether to include system prompt in token count
    #[serde(default = "default_include_system_prompt")]
    pub include_system_prompt: bool,
    /// Reserved tokens for model response
    #[serde(default = "default_response_reserve")]
    pub response_reserve: usize,
}

fn default_max_tokens() -> usize {
    4096
}

fn default_include_system_prompt() -> bool {
    true
}

fn default_response_reserve() -> usize {
    1024
}

impl Default for ContextWindowConfig {
    fn default() -> Self {
        Self {
            max_tokens: default_max_tokens(),
            overflow_strategy: OverflowStrategy::default(),
            include_system_prompt: default_include_system_prompt(),
            response_reserve: default_response_reserve(),
        }
    }
}

impl ContextWindowConfig {
    /// Create a new context window config with the given max tokens
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            ..Default::default()
        }
    }

    /// Set the max tokens
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Set the overflow strategy
    pub fn with_overflow_strategy(mut self, strategy: OverflowStrategy) -> Self {
        self.overflow_strategy = strategy;
        self
    }

    /// Set whether to include system prompt
    pub fn with_include_system_prompt(mut self, include: bool) -> Self {
        self.include_system_prompt = include;
        self
    }

    /// Set the response reserve tokens
    pub fn with_response_reserve(mut self, reserve: usize) -> Self {
        self.response_reserve = reserve;
        self
    }

    /// Parse from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Get the effective max tokens for messages (after reserving response tokens)
    pub fn effective_message_tokens(&self) -> usize {
        if self.max_tokens > self.response_reserve {
            self.max_tokens - self.response_reserve
        } else {
            0
        }
    }
}

/// Model-specific context window recommendations
/// Returns (recommended_max, absolute_max) for common models
pub fn get_model_context_recommendation(model_name: &str) -> Option<(usize, usize)> {
    let model_lower = model_name.to_lowercase();

    // GPT-4 family
    if model_lower.contains("gpt-4-turbo") || model_lower.contains("gpt-4o") {
        Some((128000, 128000))
    } else if model_lower.contains("gpt-4-32k") {
        Some((32768, 32768))
    } else if model_lower.contains("gpt-4") {
        Some((8192, 8192))
    }
    // GPT-3.5 family
    else if model_lower.contains("gpt-3.5-turbo") || model_lower.contains("gpt-3.5") {
        Some((16385, 16385))
    }
    // Claude 3 family
    else if model_lower.contains("claude-3-5") || model_lower.contains("claude-3.5") {
        Some((200000, 200000))
    } else if model_lower.contains("claude-3") {
        Some((200000, 200000))
    }
    // Llama family
    else if model_lower.contains("llama-3") || model_lower.contains("llama3") {
        Some((8192, 8192))
    } else if model_lower.contains("llama-2") || model_lower.contains("llama2") {
        Some((4096, 4096))
    }
    // Mistral family
    else if model_lower.contains("mistral") || model_lower.contains("mixtral") {
        Some((32768, 32768))
    }
    // Default for unknown models
    else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ContextWindowConfig::default();
        assert_eq!(config.max_tokens, 4096);
        assert_eq!(config.overflow_strategy, OverflowStrategy::Truncate);
        assert!(config.include_system_prompt);
        assert_eq!(config.response_reserve, 1024);
    }

    #[test]
    fn test_builder_pattern() {
        let config = ContextWindowConfig::new(8192)
            .with_overflow_strategy(OverflowStrategy::Summarize)
            .with_include_system_prompt(false)
            .with_response_reserve(2048);

        assert_eq!(config.max_tokens, 8192);
        assert_eq!(config.overflow_strategy, OverflowStrategy::Summarize);
        assert!(!config.include_system_prompt);
        assert_eq!(config.response_reserve, 2048);
    }

    #[test]
    fn test_serialization() {
        let config = ContextWindowConfig::new(16384)
            .with_overflow_strategy(OverflowStrategy::Error);

        let json = config.to_json().unwrap();
        assert!(json.contains("\"maxTokens\":16384"));
        assert!(json.contains("\"overflowStrategy\":\"error\""));
        assert!(json.contains("\"includeSystemPrompt\":true"));

        let parsed: ContextWindowConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn test_from_json() {
        let json = r#"{"maxTokens":8192,"overflowStrategy":"summarize","includeSystemPrompt":false,"responseReserve":512}"#;
        let config = ContextWindowConfig::from_json(json).unwrap();
        assert_eq!(config.max_tokens, 8192);
        assert_eq!(config.overflow_strategy, OverflowStrategy::Summarize);
        assert!(!config.include_system_prompt);
        assert_eq!(config.response_reserve, 512);
    }

    #[test]
    fn test_overflow_strategy_display() {
        assert_eq!(format!("{}", OverflowStrategy::Truncate), "truncate");
        assert_eq!(format!("{}", OverflowStrategy::Summarize), "summarize");
        assert_eq!(format!("{}", OverflowStrategy::Error), "error");
    }

    #[test]
    fn test_overflow_strategy_from_str() {
        assert_eq!("truncate".parse::<OverflowStrategy>().unwrap(), OverflowStrategy::Truncate);
        assert_eq!("SUMMARIZE".parse::<OverflowStrategy>().unwrap(), OverflowStrategy::Summarize);
        assert_eq!("Error".parse::<OverflowStrategy>().unwrap(), OverflowStrategy::Error);
        assert!("invalid".parse::<OverflowStrategy>().is_err());
    }

    #[test]
    fn test_effective_message_tokens() {
        let config = ContextWindowConfig::new(4096).with_response_reserve(1024);
        assert_eq!(config.effective_message_tokens(), 3072);

        // Edge case: response reserve >= max tokens
        let config = ContextWindowConfig::new(100).with_response_reserve(200);
        assert_eq!(config.effective_message_tokens(), 0);
    }

    #[test]
    fn test_model_recommendations() {
        // GPT-4
        assert_eq!(get_model_context_recommendation("gpt-4"), Some((8192, 8192)));
        assert_eq!(get_model_context_recommendation("gpt-4-turbo"), Some((128000, 128000)));
        assert_eq!(get_model_context_recommendation("gpt-4-32k"), Some((32768, 32768)));

        // GPT-3.5
        assert_eq!(get_model_context_recommendation("gpt-3.5-turbo"), Some((16385, 16385)));

        // Claude
        assert_eq!(get_model_context_recommendation("claude-3-opus"), Some((200000, 200000)));
        assert_eq!(get_model_context_recommendation("claude-3-5-sonnet"), Some((200000, 200000)));

        // Llama
        assert_eq!(get_model_context_recommendation("llama-3-70b"), Some((8192, 8192)));
        assert_eq!(get_model_context_recommendation("llama2"), Some((4096, 4096)));

        // Mistral
        assert_eq!(get_model_context_recommendation("mistral-7b"), Some((32768, 32768)));

        // Unknown model
        assert_eq!(get_model_context_recommendation("unknown-model"), None);
    }
}