//! Privacy Configuration Service
//!
//! Service for managing agent privacy settings and data retention.
//! [Source: Story 7.4 - 数据处理与隐私设置]

use super::privacy_config::{AgentPrivacyConfig, DataRetentionPolicy, ExclusionRule, SensitiveDataFilter};
use chrono::{DateTime, Utc, Duration};

/// Service for managing privacy configuration
pub struct PrivacyConfigService;

impl PrivacyConfigService {
    /// Create a new privacy config service
    pub fn new() -> Self {
        Self
    }

    /// Filter sensitive information from content using the provided filter config
    pub fn filter_content(filter: &SensitiveDataFilter, content: &str) -> String {
        if !filter.enabled {
            return content.to_string();
        }

        let mut result = content.to_string();

        // Filter email addresses
        if filter.filter_email {
            result = Self::mask_email(&result);
        }

        // Filter phone numbers
        if filter.filter_phone {
            result = Self::mask_phone(&result);
        }

        // Filter ID card numbers
        if filter.filter_id_card {
            result = Self::mask_id_card(&result);
        }

        // Filter bank card numbers
        if filter.filter_bank_card {
            result = Self::mask_bank_card(&result);
        }

        // Filter IP addresses
        if filter.filter_ip_address {
            result = Self::mask_ip(&result);
        }

        // Apply custom patterns
        for pattern in &filter.custom_patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                result = re.replace_all(&result, "***FILTERED***").to_string();
            }
        }

        result
    }

    /// Check if content should be excluded from storage
    pub fn should_exclude(rules: &[ExclusionRule], content: &str) -> bool {
        rules.iter().any(|rule| rule.matches(content))
    }

    /// Calculate the cutoff timestamp for data retention
    pub fn calculate_retention_cutoff(policy: &DataRetentionPolicy) -> RetentionCutoff {
        let now = Utc::now();

        let episodic_cutoff = if policy.episodic_memory_days == 0 {
            None // Forever retention
        } else {
            Some(now - Duration::days(policy.episodic_memory_days as i64))
        };

        let working_cutoff = now - Duration::hours(policy.working_memory_hours as i64);

        RetentionCutoff {
            episodic_cutoff,
            working_cutoff,
        }
    }

    /// Validate an exclusion rule pattern
    pub fn validate_exclusion_pattern(pattern: &str) -> Result<(), String> {
        regex::Regex::new(pattern)
            .map(|_| ())
            .map_err(|e| format!("Invalid regex pattern: {}", e))
    }

    /// Validate a custom filter pattern
    pub fn validate_filter_pattern(pattern: &str) -> Result<(), String> {
        regex::Regex::new(pattern)
            .map(|_| ())
            .map_err(|e| format!("Invalid regex pattern: {}", e))
    }

    fn mask_email(text: &str) -> String {
        let pattern = r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}";
        regex::Regex::new(pattern)
            .map(|re| re.replace_all(text, "***@***.***").to_string())
            .unwrap_or_else(|_| text.to_string())
    }

    fn mask_phone(text: &str) -> String {
        // Match phone numbers with word boundaries to avoid matching within ID cards
        // Chinese mobile: 1[3-9]xxxxxxxxx (11 digits, optionally with +86 prefix)
        // Other formats: area code + number
        let pattern = r"\b(?:\+?86[-\s]?)?1[3-9]\d{9}\b|\b(?:\+?\d{1,3}[-\s]?)?\(?\d{3,4}\)?[-\s]?\d{7,8}\b";
        regex::Regex::new(pattern)
            .map(|re| re.replace_all(text, "***-****-****").to_string())
            .unwrap_or_else(|_| text.to_string())
    }

    fn mask_id_card(text: &str) -> String {
        // Match 18-digit Chinese ID card with word boundaries
        let pattern = r"\b\d{17}[\dXx]\b";
        regex::Regex::new(pattern)
            .map(|re| re.replace_all(text, "******************").to_string())
            .unwrap_or_else(|_| text.to_string())
    }

    fn mask_bank_card(text: &str) -> String {
        let pattern = r"\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{0,3}";
        regex::Regex::new(pattern)
            .map(|re| re.replace_all(text, "****-****-****-****").to_string())
            .unwrap_or_else(|_| text.to_string())
    }

    fn mask_ip(text: &str) -> String {
        let pattern = r"\b(?:(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\b";
        regex::Regex::new(pattern)
            .map(|re| re.replace_all(text, "***.***.***.***").to_string())
            .unwrap_or_else(|_| text.to_string())
    }
}

impl Default for PrivacyConfigService {
    fn default() -> Self {
        Self::new()
    }
}

/// Retention cutoff timestamps for different memory layers
#[derive(Debug, Clone)]
pub struct RetentionCutoff {
    /// Cutoff for episodic memory (None means forever)
    pub episodic_cutoff: Option<DateTime<Utc>>,
    /// Cutoff for working memory
    pub working_cutoff: DateTime<Utc>,
}

impl RetentionCutoff {
    /// Check if a timestamp is before the episodic cutoff (should be cleaned)
    pub fn is_episodic_expired(&self, timestamp: DateTime<Utc>) -> bool {
        self.episodic_cutoff
            .map(|cutoff| timestamp < cutoff)
            .unwrap_or(false)
    }

    /// Check if a timestamp is before the working memory cutoff
    pub fn is_working_expired(&self, timestamp: DateTime<Utc>) -> bool {
        timestamp < self.working_cutoff
    }
}

/// Result of applying retention policy
#[derive(Debug, Clone, Default)]
pub struct RetentionResult {
    /// Number of episodic memories cleaned
    pub episodic_cleaned: usize,
    /// Number of working memory items cleaned
    pub working_cleaned: usize,
}

impl RetentionResult {
    /// Create a new empty retention result
    pub fn new() -> Self {
        Self::default()
    }

    /// Get total items cleaned
    pub fn total_cleaned(&self) -> usize {
        self.episodic_cleaned + self.working_cleaned
    }

    /// Check if any items were cleaned
    pub fn has_cleaned(&self) -> bool {
        self.total_cleaned() > 0
    }
}

/// Data retention status
#[derive(Debug, Clone)]
pub struct RetentionStatus {
    /// Total number of memories
    pub total_memories: usize,
    /// Timestamp of oldest memory
    pub oldest_memory: Option<DateTime<Utc>>,
    /// Timestamp of newest memory
    pub newest_memory: Option<DateTime<Utc>>,
    /// Estimated storage size in bytes
    pub estimated_size_bytes: u64,
}

impl RetentionStatus {
    /// Create a new retention status
    pub fn new(
        total_memories: usize,
        oldest_memory: Option<DateTime<Utc>>,
        newest_memory: Option<DateTime<Utc>>,
        estimated_size_bytes: u64,
    ) -> Self {
        Self {
            total_memories,
            oldest_memory,
            newest_memory,
            estimated_size_bytes,
        }
    }

    /// Check if there are any memories
    pub fn has_memories(&self) -> bool {
        self.total_memories > 0
    }

    /// Get memory span in days
    pub fn memory_span_days(&self) -> Option<i64> {
        match (self.oldest_memory, self.newest_memory) {
            (Some(oldest), Some(newest)) => Some((newest - oldest).num_days()),
            _ => None,
        }
    }
}

/// Filter test result
#[derive(Debug, Clone)]
pub struct FilterTestResult {
    /// Original content
    pub original: String,
    /// Filtered content
    pub filtered: String,
    /// Number of replacements made
    pub replacements: usize,
    /// List of detected sensitive items
    pub detected_items: Vec<DetectedSensitiveItem>,
}

/// Detected sensitive item
#[derive(Debug, Clone)]
pub struct DetectedSensitiveItem {
    /// Type of sensitive data
    pub data_type: String,
    /// Original value
    pub original_value: String,
    /// Position in text
    pub position: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_email() {
        let mut filter = SensitiveDataFilter::new();
        filter.enabled = true;

        let result = PrivacyConfigService::filter_content(&filter, "Contact: test@example.com");
        assert!(result.contains("***@***.***"));
        assert!(!result.contains("test@example.com"));
    }

    #[test]
    fn test_filter_phone() {
        let mut filter = SensitiveDataFilter::new();
        filter.enabled = true;

        let result = PrivacyConfigService::filter_content(&filter, "Phone: 13812345678");
        assert!(result.contains("***-****-****"));
    }

    #[test]
    fn test_filter_id_card() {
        let mut filter = SensitiveDataFilter::new();
        filter.enabled = true;

        let result = PrivacyConfigService::filter_content(&filter, "ID: 110101199001011234");
        // The ID card 110101199001011234 should be replaced with asterisks
        assert!(!result.contains("110101199001011234"), "Original ID should be filtered out");
        assert!(result.contains("******************"), "Should contain masked ID, got: {}", result);
    }

    #[test]
    fn test_filter_disabled() {
        let filter = SensitiveDataFilter::new(); // disabled by default

        let original = "Email: test@example.com";
        let result = PrivacyConfigService::filter_content(&filter, original);
        assert_eq!(result, original);
    }

    #[test]
    fn test_should_exclude() {
        let rules = vec![
            ExclusionRule::new("password", r"password\s*[:=]\s*\S+"),
        ];

        assert!(PrivacyConfigService::should_exclude(&rules, "password = secret"));
        assert!(!PrivacyConfigService::should_exclude(&rules, "no password here"));
    }

    #[test]
    fn test_calculate_retention_cutoff() {
        let policy = DataRetentionPolicy::new()
            .with_episodic_days(30)
            .with_working_hours(24);

        let cutoff = PrivacyConfigService::calculate_retention_cutoff(&policy);

        assert!(cutoff.episodic_cutoff.is_some());
        assert!(cutoff.working_cutoff < Utc::now());
    }

    #[test]
    fn test_retention_cutoff_forever() {
        let policy = DataRetentionPolicy::new()
            .with_episodic_days(0); // Forever

        let cutoff = PrivacyConfigService::calculate_retention_cutoff(&policy);

        assert!(cutoff.episodic_cutoff.is_none());
    }

    #[test]
    fn test_validate_pattern_valid() {
        assert!(PrivacyConfigService::validate_exclusion_pattern(r"\d+").is_ok());
    }

    #[test]
    fn test_validate_pattern_invalid() {
        assert!(PrivacyConfigService::validate_exclusion_pattern(r"[invalid").is_err());
    }

    #[test]
    fn test_retention_result() {
        let result = RetentionResult {
            episodic_cleaned: 5,
            working_cleaned: 3,
        };

        assert_eq!(result.total_cleaned(), 8);
        assert!(result.has_cleaned());
    }

    #[test]
    fn test_retention_status() {
        let status = RetentionStatus::new(
            100,
            Some(Utc::now() - Duration::days(30)),
            Some(Utc::now()),
            1024 * 1024,
        );

        assert!(status.has_memories());
        assert_eq!(status.memory_span_days(), Some(30));
    }

    #[test]
    fn test_filter_multiple_types() {
        let mut filter = SensitiveDataFilter::new();
        filter.enabled = true;
        filter.filter_ip_address = true;

        let content = "Contact: test@example.com, Phone: 13812345678, IP: 192.168.1.1";
        let result = PrivacyConfigService::filter_content(&filter, content);

        assert!(!result.contains("test@example.com"));
        assert!(!result.contains("13812345678"));
        assert!(!result.contains("192.168.1.1"));
    }

    #[test]
    fn test_filter_custom_pattern() {
        let mut filter = SensitiveDataFilter::new();
        filter.enabled = true;
        filter.custom_patterns.push(r"API[_-]?KEY[_-]?\w+".to_string());

        let result = PrivacyConfigService::filter_content(&filter, "API_KEY_SECRET=abc123");
        assert!(result.contains("***FILTERED***"));
    }

    #[test]
    fn test_is_episodic_expired() {
        let policy = DataRetentionPolicy::new().with_episodic_days(30);
        let cutoff = PrivacyConfigService::calculate_retention_cutoff(&policy);

        let old_timestamp = Utc::now() - Duration::days(60);
        let new_timestamp = Utc::now() - Duration::days(10);

        assert!(cutoff.is_episodic_expired(old_timestamp));
        assert!(!cutoff.is_episodic_expired(new_timestamp));
    }
}