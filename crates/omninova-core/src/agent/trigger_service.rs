//! Trigger Configuration Service
//!
//! Service for managing and testing trigger keyword configurations.
//! [Source: Story 7.3 - 触发关键词配置]

use crate::channels::behavior::{MatchType, TriggerKeyword, TriggerKeywordMatcher};

use super::trigger_config::{MatchedKeywordInfo, TriggerTestResult};

/// Service for managing trigger keyword configuration
pub struct TriggerConfigService;

impl TriggerConfigService {
    /// Test a single trigger keyword against sample text
    ///
    /// Returns detailed information about the match result.
    pub fn test_trigger(keyword: &TriggerKeyword, text: &str) -> TriggerTestResult {
        let is_match = keyword.matches(text);
        let matched_keywords = if is_match {
            vec![MatchedKeywordInfo {
                keyword: keyword.keyword.clone(),
                match_type: keyword.match_type,
                case_sensitive: keyword.case_sensitive,
            }]
        } else {
            vec![]
        };

        TriggerTestResult {
            matched: is_match,
            matched_keywords,
        }
    }

    /// Test all keywords against sample text
    ///
    /// Returns a combined result with all matching keywords.
    pub fn test_all_keywords(keywords: &[TriggerKeyword], text: &str) -> TriggerTestResult {
        let matching = TriggerKeywordMatcher::find_matching(text, keywords);

        let matched_keywords: Vec<MatchedKeywordInfo> = matching
            .into_iter()
            .map(|k| MatchedKeywordInfo {
                keyword: k.keyword.clone(),
                match_type: k.match_type,
                case_sensitive: k.case_sensitive,
            })
            .collect();

        TriggerTestResult {
            matched: !matched_keywords.is_empty(),
            matched_keywords,
        }
    }

    /// Validate a trigger keyword pattern
    ///
    /// Returns Ok(()) if the keyword is valid, Err with message otherwise.
    pub fn validate_keyword(keyword: &TriggerKeyword) -> Result<(), String> {
        // Check keyword is not empty
        if keyword.keyword.trim().is_empty() {
            return Err("关键词不能为空".to_string());
        }

        // Validate regex if match type is Regex
        if keyword.match_type == MatchType::Regex {
            regex::Regex::new(&keyword.keyword)
                .map_err(|e| format!("无效的正则表达式: {}", e))?;
        }

        Ok(())
    }

    /// Create a trigger keyword from user input with validation
    pub fn create_keyword(
        keyword: impl Into<String>,
        match_type: MatchType,
        case_sensitive: bool,
    ) -> Result<TriggerKeyword, String> {
        let kw = TriggerKeyword {
            keyword: keyword.into(),
            match_type,
            case_sensitive,
        };

        Self::validate_keyword(&kw)?;
        Ok(kw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_trigger_exact_match() {
        let keyword = TriggerKeyword::new("help");
        let result = TriggerConfigService::test_trigger(&keyword, "I need help please");

        assert!(result.matched);
        assert_eq!(result.matched_keywords.len(), 1);
        assert_eq!(result.matched_keywords[0].keyword, "help");
        assert_eq!(result.matched_keywords[0].match_type, MatchType::Exact);
    }

    #[test]
    fn test_test_trigger_no_match() {
        let keyword = TriggerKeyword::new("help");
        let result = TriggerConfigService::test_trigger(&keyword, "hello there");

        assert!(!result.matched);
        assert!(result.matched_keywords.is_empty());
    }

    #[test]
    fn test_test_all_keywords_multiple_matches() {
        let keywords = vec![
            TriggerKeyword::new("help"),
            TriggerKeyword::new("please"),
        ];
        let result = TriggerConfigService::test_all_keywords(&keywords, "please help me");

        assert!(result.matched);
        assert_eq!(result.matched_keywords.len(), 2);
    }

    #[test]
    fn test_test_all_keywords_no_match() {
        let keywords = vec![
            TriggerKeyword::new("help"),
            TriggerKeyword::new("assist"),
        ];
        let result = TriggerConfigService::test_all_keywords(&keywords, "hello there");

        assert!(!result.matched);
        assert!(result.matched_keywords.is_empty());
    }

    #[test]
    fn test_validate_keyword_valid() {
        let keyword = TriggerKeyword::new("help");
        assert!(TriggerConfigService::validate_keyword(&keyword).is_ok());
    }

    #[test]
    fn test_validate_keyword_empty() {
        let keyword = TriggerKeyword::new("   ");
        assert!(TriggerConfigService::validate_keyword(&keyword).is_err());
    }

    #[test]
    fn test_validate_keyword_invalid_regex() {
        let keyword = TriggerKeyword::with_match_type("[invalid", MatchType::Regex);
        assert!(TriggerConfigService::validate_keyword(&keyword).is_err());
    }

    #[test]
    fn test_validate_keyword_valid_regex() {
        let keyword = TriggerKeyword::with_match_type(r"help\s+\w+", MatchType::Regex);
        assert!(TriggerConfigService::validate_keyword(&keyword).is_ok());
    }

    #[test]
    fn test_create_keyword_valid() {
        let keyword = TriggerConfigService::create_keyword("help", MatchType::Prefix, true);
        assert!(keyword.is_ok());
        let kw = keyword.unwrap();
        assert_eq!(kw.keyword, "help");
        assert_eq!(kw.match_type, MatchType::Prefix);
        assert!(kw.case_sensitive);
    }

    #[test]
    fn test_create_keyword_invalid_regex() {
        let keyword = TriggerConfigService::create_keyword("[invalid", MatchType::Regex, false);
        assert!(keyword.is_err());
    }

    #[test]
    fn test_test_trigger_prefix_match() {
        let keyword = TriggerKeyword::with_match_type("help", MatchType::Prefix);
        let result = TriggerConfigService::test_trigger(&keyword, "helper needed");

        assert!(result.matched);
        assert_eq!(result.matched_keywords[0].match_type, MatchType::Prefix);
    }

    #[test]
    fn test_test_trigger_contains_match() {
        let keyword = TriggerKeyword::with_match_type("help", MatchType::Contains);
        let result = TriggerConfigService::test_trigger(&keyword, "please help me");

        assert!(result.matched);
        assert_eq!(result.matched_keywords[0].match_type, MatchType::Contains);
    }

    #[test]
    fn test_test_trigger_regex_match() {
        let keyword = TriggerKeyword::with_match_type(r"help\s+\w+", MatchType::Regex);
        let result = TriggerConfigService::test_trigger(&keyword, "help me please");

        assert!(result.matched);
        assert_eq!(result.matched_keywords[0].match_type, MatchType::Regex);
    }

    #[test]
    fn test_test_trigger_case_sensitive() {
        let mut keyword = TriggerKeyword::new("Help");
        keyword.case_sensitive = true;

        // Should match exact case
        let result = TriggerConfigService::test_trigger(&keyword, "Help me");
        assert!(result.matched);

        // Should not match different case
        let result = TriggerConfigService::test_trigger(&keyword, "help me");
        assert!(!result.matched);
    }
}