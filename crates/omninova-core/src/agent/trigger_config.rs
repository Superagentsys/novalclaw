//! Agent Trigger Keywords Configuration
//!
//! Defines trigger keyword settings for AI agents.
//! [Source: Story 7.3 - 触发关键词配置]

use serde::{Deserialize, Serialize};
use crate::channels::behavior::{MatchType, TriggerKeyword, TriggerKeywordMatcher};

/// Agent-level trigger keywords configuration
///
/// Controls when an agent should respond to incoming messages.
/// If enabled and keywords are configured, the agent only responds
/// when a message matches one of the trigger keywords.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentTriggerConfig {
    /// List of trigger keywords for this agent
    #[serde(default)]
    pub keywords: Vec<TriggerKeyword>,
    /// Whether triggers are enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Default match type for new keywords
    #[serde(default)]
    pub default_match_type: MatchType,
    /// Whether to use case-sensitive matching by default
    #[serde(default)]
    pub default_case_sensitive: bool,
}

fn default_enabled() -> bool {
    true
}

impl Default for AgentTriggerConfig {
    fn default() -> Self {
        Self {
            keywords: Vec::new(),
            enabled: true,
            default_match_type: MatchType::default(),
            default_case_sensitive: false,
        }
    }
}

impl AgentTriggerConfig {
    /// Create a new trigger config with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a message matches any trigger keyword
    ///
    /// Returns `true` if:
    /// - Triggers are disabled
    /// - No keywords are configured (empty list means accept all)
    /// - At least one keyword matches the message
    pub fn matches(&self, message: &str) -> bool {
        if !self.enabled || self.keywords.is_empty() {
            return true; // No filter when disabled or empty
        }
        TriggerKeywordMatcher::check_triggers(message, &self.keywords)
    }

    /// Find all keywords that match the message
    pub fn find_matches<'a>(&'a self, message: &str) -> Vec<&'a TriggerKeyword> {
        TriggerKeywordMatcher::find_matching(message, &self.keywords)
    }

    /// Add a new trigger keyword
    ///
    /// Returns `true` if the keyword was added, `false` if it already exists
    pub fn add_keyword(&mut self, keyword: TriggerKeyword) -> bool {
        if self.keywords.iter().any(|k| k.keyword == keyword.keyword && k.match_type == keyword.match_type) {
            false
        } else {
            self.keywords.push(keyword);
            true
        }
    }

    /// Remove a trigger keyword by index
    pub fn remove_keyword(&mut self, index: usize) -> Option<TriggerKeyword> {
        if index < self.keywords.len() {
            Some(self.keywords.remove(index))
        } else {
            None
        }
    }

    /// Remove a trigger keyword by keyword string and match type
    pub fn remove_keyword_by_value(&mut self, keyword: &str, match_type: MatchType) -> Option<TriggerKeyword> {
        let pos = self.keywords.iter().position(|k| k.keyword == keyword && k.match_type == match_type)?;
        Some(self.keywords.remove(pos))
    }

    /// Clear all keywords
    pub fn clear_keywords(&mut self) {
        self.keywords.clear();
    }

    /// Check if any keywords are configured
    pub fn has_keywords(&self) -> bool {
        !self.keywords.is_empty()
    }

    /// Get the number of configured keywords
    pub fn keyword_count(&self) -> usize {
        self.keywords.len()
    }

    /// Create a new trigger keyword using default settings
    pub fn create_keyword_with_defaults(&self, keyword: impl Into<String>) -> TriggerKeyword {
        let mut kw = TriggerKeyword::new(keyword);
        kw.match_type = self.default_match_type;
        kw.case_sensitive = self.default_case_sensitive;
        kw
    }

    /// Set whether triggers are enabled
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set the default match type
    pub fn with_default_match_type(mut self, match_type: MatchType) -> Self {
        self.default_match_type = match_type;
        self
    }

    /// Set the default case sensitivity
    pub fn with_default_case_sensitive(mut self, sensitive: bool) -> Self {
        self.default_case_sensitive = sensitive;
        self
    }

    /// Add a keyword using the builder pattern
    pub fn with_keyword(mut self, keyword: TriggerKeyword) -> Self {
        self.add_keyword(keyword);
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
}

/// Test result for trigger keyword matching
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerTestResult {
    /// Whether the message matched
    pub matched: bool,
    /// The keywords that matched
    pub matched_keywords: Vec<MatchedKeywordInfo>,
}

/// Information about a matched keyword
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchedKeywordInfo {
    /// The keyword that matched
    pub keyword: String,
    /// The match type used
    pub match_type: MatchType,
    /// Whether case-sensitive matching was used
    pub case_sensitive: bool,
}

impl TriggerTestResult {
    /// Create a new test result
    pub fn new(matched: bool, matched_keywords: Vec<MatchedKeywordInfo>) -> Self {
        Self { matched, matched_keywords }
    }

    /// Create a result indicating no match
    pub fn no_match() -> Self {
        Self {
            matched: false,
            matched_keywords: Vec::new(),
        }
    }
}

/// Determine if an agent should respond to a message.
///
/// This function implements the priority logic for trigger keywords:
/// 1. If channel has trigger keywords configured, use channel triggers
/// 2. If channel has no trigger keywords, fall back to agent triggers
/// 3. If neither has triggers configured, accept all messages
///
/// # Arguments
///
/// * `message` - The incoming message text
/// * `channel_keywords` - Trigger keywords from channel behavior config (can be empty)
/// * `agent_config` - Agent's trigger keywords configuration
///
/// # Returns
///
/// `true` if the agent should respond to this message
///
/// # Example
///
/// ```rust
/// use omninova_core::agent::AgentTriggerConfig;
/// use omninova_core::channels::behavior::TriggerKeyword;
///
/// let agent_config = AgentTriggerConfig::new();
/// let channel_keywords: Vec<TriggerKeyword> = vec![];
///
/// // Empty channel keywords means use agent config (or accept all if agent also empty)
/// assert!(omninova_core::agent::should_agent_respond("hello", &channel_keywords, &agent_config));
/// ```
pub fn should_agent_respond(
    message: &str,
    channel_keywords: &[TriggerKeyword],
    agent_config: &AgentTriggerConfig,
) -> bool {
    // Priority 1: Channel triggers take precedence
    if !channel_keywords.is_empty() {
        return TriggerKeywordMatcher::check_triggers(message, channel_keywords);
    }

    // Priority 2: Fall back to agent triggers
    agent_config.matches(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AgentTriggerConfig::default();
        assert!(config.enabled);
        assert!(config.keywords.is_empty());
        assert_eq!(config.default_match_type, MatchType::Exact);
        assert!(!config.default_case_sensitive);
    }

    #[test]
    fn test_matches_empty_keywords() {
        let config = AgentTriggerConfig::new();
        // Empty keywords should match everything
        assert!(config.matches("any message"));
        assert!(config.matches("@help"));
    }

    #[test]
    fn test_matches_disabled() {
        let mut config = AgentTriggerConfig::new();
        config.add_keyword(TriggerKeyword::new("help"));
        config.enabled = false;
        // Disabled should match everything
        assert!(config.matches("any message"));
        assert!(config.matches("no trigger here"));
    }

    #[test]
    fn test_matches_with_keywords() {
        let mut config = AgentTriggerConfig::new();
        config.add_keyword(TriggerKeyword::new("help"));
        config.add_keyword(TriggerKeyword::new("assist"));

        assert!(config.matches("I need help"));
        assert!(config.matches("assist me please"));
        assert!(!config.matches("hello there"));
    }

    #[test]
    fn test_add_keyword() {
        let mut config = AgentTriggerConfig::new();

        // Add first keyword
        assert!(config.add_keyword(TriggerKeyword::new("help")));
        assert_eq!(config.keywords.len(), 1);

        // Adding duplicate should not add
        assert!(!config.add_keyword(TriggerKeyword::new("help")));
        assert_eq!(config.keywords.len(), 1);

        // Add different keyword
        assert!(config.add_keyword(TriggerKeyword::new("assist")));
        assert_eq!(config.keywords.len(), 2);
    }

    #[test]
    fn test_add_keyword_different_match_type() {
        let mut config = AgentTriggerConfig::new();

        // Same keyword with different match type should be allowed
        assert!(config.add_keyword(TriggerKeyword::new("help")));
        assert!(config.add_keyword(TriggerKeyword::with_match_type("help", MatchType::Prefix)));
        assert_eq!(config.keywords.len(), 2);
    }

    #[test]
    fn test_remove_keyword_by_index() {
        let mut config = AgentTriggerConfig::new();
        config.add_keyword(TriggerKeyword::new("help"));
        config.add_keyword(TriggerKeyword::new("assist"));

        let removed = config.remove_keyword(0);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().keyword, "help");
        assert_eq!(config.keywords.len(), 1);
        assert_eq!(config.keywords[0].keyword, "assist");

        // Remove out of bounds
        let removed = config.remove_keyword(10);
        assert!(removed.is_none());
    }

    #[test]
    fn test_remove_keyword_by_value() {
        let mut config = AgentTriggerConfig::new();
        config.add_keyword(TriggerKeyword::new("help"));
        config.add_keyword(TriggerKeyword::new("assist"));

        let removed = config.remove_keyword_by_value("help", MatchType::Exact);
        assert!(removed.is_some());
        assert_eq!(config.keywords.len(), 1);

        // Remove non-existent
        let removed = config.remove_keyword_by_value("nonexistent", MatchType::Exact);
        assert!(removed.is_none());
    }

    #[test]
    fn test_find_matches() {
        let mut config = AgentTriggerConfig::new();
        config.add_keyword(TriggerKeyword::new("help"));
        config.add_keyword(TriggerKeyword::new("please"));

        let matches = config.find_matches("please help me");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_builder_pattern() {
        let config = AgentTriggerConfig::new()
            .with_enabled(false)
            .with_default_match_type(MatchType::Prefix)
            .with_default_case_sensitive(true)
            .with_keyword(TriggerKeyword::new("test"));

        assert!(!config.enabled);
        assert_eq!(config.default_match_type, MatchType::Prefix);
        assert!(config.default_case_sensitive);
        assert_eq!(config.keywords.len(), 1);
    }

    #[test]
    fn test_create_keyword_with_defaults() {
        let config = AgentTriggerConfig::new()
            .with_default_match_type(MatchType::Contains)
            .with_default_case_sensitive(true);

        let keyword = config.create_keyword_with_defaults("test");
        assert_eq!(keyword.keyword, "test");
        assert_eq!(keyword.match_type, MatchType::Contains);
        assert!(keyword.case_sensitive);
    }

    #[test]
    fn test_serialization() {
        let mut config = AgentTriggerConfig::new();
        config.add_keyword(TriggerKeyword::new("help"));
        config.enabled = true;

        let json = config.to_json().unwrap();
        assert!(json.contains("\"keywords\""));
        assert!(json.contains("\"enabled\":true"));

        let parsed: AgentTriggerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn test_from_json() {
        let json = r#"{"keywords":[{"keyword":"help","matchType":"exact","caseSensitive":false}],"enabled":true,"defaultMatchType":"exact","defaultCaseSensitive":false}"#;
        let config = AgentTriggerConfig::from_json(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.keywords.len(), 1);
        assert_eq!(config.keywords[0].keyword, "help");
    }

    #[test]
    fn test_keyword_count() {
        let mut config = AgentTriggerConfig::new();
        assert_eq!(config.keyword_count(), 0);
        assert!(!config.has_keywords());

        config.add_keyword(TriggerKeyword::new("help"));
        assert_eq!(config.keyword_count(), 1);
        assert!(config.has_keywords());

        config.add_keyword(TriggerKeyword::new("assist"));
        assert_eq!(config.keyword_count(), 2);
    }

    #[test]
    fn test_clear_keywords() {
        let mut config = AgentTriggerConfig::new();
        config.add_keyword(TriggerKeyword::new("help"));
        config.add_keyword(TriggerKeyword::new("assist"));
        assert_eq!(config.keywords.len(), 2);

        config.clear_keywords();
        assert!(config.keywords.is_empty());
    }

    #[test]
    fn test_regex_match() {
        let mut config = AgentTriggerConfig::new();
        config.add_keyword(TriggerKeyword::with_match_type(r"@bot\s+\w+", MatchType::Regex));

        assert!(config.matches("@bot help"));
        assert!(config.matches("@bot status"));
        assert!(!config.matches("bot help")); // Missing @
        assert!(!config.matches("@bothelp")); // Missing space
    }

    #[test]
    fn test_should_agent_respond_no_triggers() {
        // No channel keywords, no agent keywords = accept all
        let agent_config = AgentTriggerConfig::new();
        let channel_keywords: Vec<TriggerKeyword> = vec![];

        assert!(should_agent_respond("hello", &channel_keywords, &agent_config));
        assert!(should_agent_respond("any message", &channel_keywords, &agent_config));
    }

    #[test]
    fn test_should_agent_respond_channel_priority() {
        // Channel keywords take priority
        let mut agent_config = AgentTriggerConfig::new();
        agent_config.add_keyword(TriggerKeyword::new("agent"));

        let channel_keywords = vec![TriggerKeyword::new("channel")];

        // Message matches channel keyword
        assert!(should_agent_respond("channel help", &channel_keywords, &agent_config));

        // Message matches agent keyword but not channel - should NOT match because channel has priority
        assert!(!should_agent_respond("agent help", &channel_keywords, &agent_config));

        // Message matches neither
        assert!(!should_agent_respond("hello there", &channel_keywords, &agent_config));
    }

    #[test]
    fn test_should_agent_respond_agent_fallback() {
        // No channel keywords means fall back to agent
        let mut agent_config = AgentTriggerConfig::new();
        agent_config.add_keyword(TriggerKeyword::new("help"));

        let channel_keywords: Vec<TriggerKeyword> = vec![];

        assert!(should_agent_respond("I need help", &channel_keywords, &agent_config));
        assert!(!should_agent_respond("hello there", &channel_keywords, &agent_config));
    }

    #[test]
    fn test_should_agent_respond_agent_disabled() {
        // Agent triggers disabled = accept all when falling back to agent
        let mut agent_config = AgentTriggerConfig::new();
        agent_config.add_keyword(TriggerKeyword::new("help"));
        agent_config.enabled = false;

        let channel_keywords: Vec<TriggerKeyword> = vec![];

        // Should accept all messages when agent triggers are disabled
        assert!(should_agent_respond("hello there", &channel_keywords, &agent_config));
        assert!(should_agent_respond("any message", &channel_keywords, &agent_config));
    }
}