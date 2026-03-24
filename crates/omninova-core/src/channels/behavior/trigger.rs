//! Trigger Keyword Types and Matcher
//!
//! Defines trigger keywords for channel activation and matching logic.

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Match type for trigger keywords
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MatchType {
    /// Exact match
    #[default]
    Exact,
    /// Prefix match
    Prefix,
    /// Contains match
    Contains,
    /// Regular expression match
    Regex,
}

impl std::fmt::Display for MatchType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exact => write!(f, "exact"),
            Self::Prefix => write!(f, "prefix"),
            Self::Contains => write!(f, "contains"),
            Self::Regex => write!(f, "regex"),
        }
    }
}

/// Trigger keyword configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerKeyword {
    /// The keyword or pattern to match
    pub keyword: String,

    /// How to match the keyword
    #[serde(default)]
    pub match_type: MatchType,

    /// Whether matching is case-sensitive
    #[serde(default)]
    pub case_sensitive: bool,
}

impl TriggerKeyword {
    /// Create a new trigger keyword with exact matching
    pub fn new(keyword: impl Into<String>) -> Self {
        Self {
            keyword: keyword.into(),
            match_type: MatchType::Exact,
            case_sensitive: false,
        }
    }

    /// Create a trigger keyword with a specific match type
    pub fn with_match_type(keyword: impl Into<String>, match_type: MatchType) -> Self {
        Self {
            keyword: keyword.into(),
            match_type,
            case_sensitive: false,
        }
    }

    /// Set case sensitivity
    pub fn with_case_sensitive(mut self, sensitive: bool) -> Self {
        self.case_sensitive = sensitive;
        self
    }

    /// Check if this keyword matches the given text
    pub fn matches(&self, text: &str) -> bool {
        let search_text = if self.case_sensitive {
            text.to_string()
        } else {
            text.to_lowercase()
        };

        let keyword = if self.case_sensitive {
            self.keyword.clone()
        } else {
            self.keyword.to_lowercase()
        };

        match self.match_type {
            MatchType::Exact => search_text.split_whitespace().any(|word| word == keyword),
            MatchType::Prefix => search_text
                .split_whitespace()
                .any(|word| word.starts_with(&keyword)),
            MatchType::Contains => search_text.contains(&keyword),
            MatchType::Regex => {
                Regex::new(&self.keyword)
                    .map(|re| re.is_match(text))
                    .unwrap_or(false)
            }
        }
    }
}

/// Matcher for trigger keywords
pub struct TriggerKeywordMatcher;

impl TriggerKeywordMatcher {
    /// Check if any of the keywords match the message
    ///
    /// Returns true if any keyword matches, false otherwise.
    /// If the keywords list is empty, returns true (no filter).
    pub fn check_triggers(message: &str, keywords: &[TriggerKeyword]) -> bool {
        // Empty keywords means no filter - accept all messages
        if keywords.is_empty() {
            return true;
        }

        keywords.iter().any(|k| k.matches(message))
    }

    /// Find all matching keywords in the message
    pub fn find_matching<'a>(message: &str, keywords: &'a [TriggerKeyword]) -> Vec<&'a TriggerKeyword> {
        keywords.iter().filter(|k| k.matches(message)).collect()
    }

    /// Find the first matching keyword
    pub fn find_first<'a>(message: &str, keywords: &'a [TriggerKeyword]) -> Option<&'a TriggerKeyword> {
        keywords.iter().find(|k| k.matches(message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let keyword = TriggerKeyword::new("help");
        assert!(keyword.matches("I need help please"));
        assert!(keyword.matches("HELP me"));
        assert!(!keyword.matches("helper"));
    }

    #[test]
    fn test_exact_match_case_sensitive() {
        let keyword = TriggerKeyword::new("Help").with_case_sensitive(true);
        assert!(keyword.matches("Help me"));
        assert!(!keyword.matches("help me"));
        assert!(!keyword.matches("HELP me"));
    }

    #[test]
    fn test_prefix_match() {
        let keyword = TriggerKeyword::with_match_type("help", MatchType::Prefix);
        assert!(keyword.matches("I need help"));
        assert!(keyword.matches("helper needed"));
        assert!(!keyword.matches("no assistance"));
    }

    #[test]
    fn test_contains_match() {
        let keyword = TriggerKeyword::with_match_type("help", MatchType::Contains);
        assert!(keyword.matches("please help me"));
        assert!(keyword.matches("I need some help"));
        assert!(keyword.matches("helper"));
        assert!(!keyword.matches("no assistance"));
    }

    #[test]
    fn test_regex_match() {
        let keyword = TriggerKeyword::with_match_type(r"help\s+\w+", MatchType::Regex);
        assert!(keyword.matches("help me"));
        assert!(keyword.matches("help now please"));
        assert!(!keyword.matches("just help"));
    }

    #[test]
    fn test_check_triggers_empty() {
        let message = "any message";
        assert!(TriggerKeywordMatcher::check_triggers(message, &[]));
    }

    #[test]
    fn test_check_triggers_match() {
        let keywords = vec![
            TriggerKeyword::new("help"),
            TriggerKeyword::new("assist"),
        ];
        assert!(TriggerKeywordMatcher::check_triggers("I need help", &keywords));
        assert!(TriggerKeywordMatcher::check_triggers("assist me", &keywords));
        assert!(!TriggerKeywordMatcher::check_triggers("hello", &keywords));
    }

    #[test]
    fn test_find_matching() {
        let keywords = vec![
            TriggerKeyword::new("help"),
            TriggerKeyword::new("please"),
        ];
        let matches = TriggerKeywordMatcher::find_matching("please help me", &keywords);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_serialization() {
        let keyword = TriggerKeyword {
            keyword: "test".to_string(),
            match_type: MatchType::Prefix,
            case_sensitive: true,
        };

        let json = serde_json::to_string(&keyword).unwrap();
        let deserialized: TriggerKeyword = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.keyword, "test");
        assert_eq!(deserialized.match_type, MatchType::Prefix);
        assert!(deserialized.case_sensitive);
    }
}