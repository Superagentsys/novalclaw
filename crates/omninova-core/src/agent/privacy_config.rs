//! Agent Privacy Configuration
//!
//! Defines privacy and data processing settings for AI agents.
//! [Source: Story 7.4 - 数据处理与隐私设置]

use serde::{Deserialize, Serialize};

/// Memory sharing scope for agent memories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum MemorySharingScope {
    /// Memory only available within the current session
    #[default]
    SingleSession,
    /// Memory can be shared across different sessions
    CrossSession,
    /// Memory can be shared across different agents
    CrossAgent,
}

impl std::fmt::Display for MemorySharingScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemorySharingScope::SingleSession => write!(f, "singleSession"),
            MemorySharingScope::CrossSession => write!(f, "crossSession"),
            MemorySharingScope::CrossAgent => write!(f, "crossAgent"),
        }
    }
}

impl std::str::FromStr for MemorySharingScope {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "singleSession" => Ok(MemorySharingScope::SingleSession),
            "crossSession" => Ok(MemorySharingScope::CrossSession),
            "crossAgent" => Ok(MemorySharingScope::CrossAgent),
            _ => Err(format!("Invalid memory sharing scope: {}", s)),
        }
    }
}

/// Data retention policy configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataRetentionPolicy {
    /// Number of days to retain episodic memories (0 = forever)
    #[serde(default = "default_episodic_retention")]
    pub episodic_memory_days: u32,
    /// Number of hours to retain working memory
    #[serde(default = "default_working_retention")]
    pub working_memory_hours: u32,
    /// Whether to automatically cleanup expired data
    #[serde(default = "default_true")]
    pub auto_cleanup: bool,
}

fn default_episodic_retention() -> u32 {
    90
}

fn default_working_retention() -> u32 {
    24
}

fn default_true() -> bool {
    true
}

impl Default for DataRetentionPolicy {
    fn default() -> Self {
        Self {
            episodic_memory_days: 90,
            working_memory_hours: 24,
            auto_cleanup: true,
        }
    }
}

impl DataRetentionPolicy {
    /// Create a new retention policy with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if episodic memory retention is forever
    pub fn is_episodic_forever(&self) -> bool {
        self.episodic_memory_days == 0
    }

    /// Get retention duration in seconds for episodic memory
    pub fn episodic_retention_seconds(&self) -> u64 {
        self.episodic_memory_days as u64 * 24 * 60 * 60
    }

    /// Get retention duration in seconds for working memory
    pub fn working_retention_seconds(&self) -> u64 {
        self.working_memory_hours as u64 * 60 * 60
    }

    /// Set episodic memory retention days
    pub fn with_episodic_days(mut self, days: u32) -> Self {
        self.episodic_memory_days = days;
        self
    }

    /// Set working memory retention hours
    pub fn with_working_hours(mut self, hours: u32) -> Self {
        self.working_memory_hours = hours;
        self
    }

    /// Set auto cleanup
    pub fn with_auto_cleanup(mut self, auto_cleanup: bool) -> Self {
        self.auto_cleanup = auto_cleanup;
        self
    }
}

/// Sensitive data filter configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensitiveDataFilter {
    /// Whether to enable sensitive data filtering
    #[serde(default)]
    pub enabled: bool,
    /// Filter email addresses
    #[serde(default = "default_true")]
    pub filter_email: bool,
    /// Filter phone numbers
    #[serde(default = "default_true")]
    pub filter_phone: bool,
    /// Filter ID card numbers
    #[serde(default = "default_true")]
    pub filter_id_card: bool,
    /// Filter bank card numbers
    #[serde(default = "default_true")]
    pub filter_bank_card: bool,
    /// Filter IP addresses
    #[serde(default)]
    pub filter_ip_address: bool,
    /// Custom regex patterns for filtering
    #[serde(default)]
    pub custom_patterns: Vec<String>,
}

impl Default for SensitiveDataFilter {
    fn default() -> Self {
        Self {
            enabled: false,
            filter_email: true,
            filter_phone: true,
            filter_id_card: true,
            filter_bank_card: true,
            filter_ip_address: false,
            custom_patterns: Vec::new(),
        }
    }
}

impl SensitiveDataFilter {
    /// Create a new sensitive data filter with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable sensitive data filtering
    pub fn enable(mut self) -> Self {
        self.enabled = true;
        self
    }

    /// Disable sensitive data filtering
    pub fn disable(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Set email filtering
    pub fn with_filter_email(mut self, filter: bool) -> Self {
        self.filter_email = filter;
        self
    }

    /// Set phone filtering
    pub fn with_filter_phone(mut self, filter: bool) -> Self {
        self.filter_phone = filter;
        self
    }

    /// Set ID card filtering
    pub fn with_filter_id_card(mut self, filter: bool) -> Self {
        self.filter_id_card = filter;
        self
    }

    /// Set bank card filtering
    pub fn with_filter_bank_card(mut self, filter: bool) -> Self {
        self.filter_bank_card = filter;
        self
    }

    /// Set IP address filtering
    pub fn with_filter_ip_address(mut self, filter: bool) -> Self {
        self.filter_ip_address = filter;
        self
    }

    /// Add a custom filter pattern
    pub fn with_custom_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.custom_patterns.push(pattern.into());
        self
    }
}

/// Exclusion rule for data storage
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExclusionRule {
    /// Rule name
    pub name: String,
    /// Rule description
    #[serde(default)]
    pub description: Option<String>,
    /// Regex pattern to match content
    pub pattern: String,
    /// Whether the rule is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl ExclusionRule {
    /// Create a new exclusion rule
    pub fn new(name: impl Into<String>, pattern: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            pattern: pattern.into(),
            enabled: true,
        }
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set enabled state
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Check if content matches this rule
    pub fn matches(&self, content: &str) -> bool {
        if !self.enabled {
            return false;
        }
        regex::Regex::new(&self.pattern)
            .map(|re| re.is_match(content))
            .unwrap_or(false)
    }
}

/// Agent-level privacy configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentPrivacyConfig {
    /// Data retention policy
    #[serde(default)]
    pub data_retention: DataRetentionPolicy,
    /// Sensitive data filter configuration
    #[serde(default)]
    pub sensitive_filter: SensitiveDataFilter,
    /// Memory sharing scope
    #[serde(default)]
    pub memory_sharing_scope: MemorySharingScope,
    /// Exclusion rules for data storage
    #[serde(default)]
    pub exclusion_rules: Vec<ExclusionRule>,
    /// Whether to log detailed data processing info
    #[serde(default)]
    pub verbose_logging: bool,
}

impl AgentPrivacyConfig {
    /// Create a new privacy config with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if content should be excluded from storage
    pub fn should_exclude(&self, content: &str) -> bool {
        self.exclusion_rules.iter().any(|rule| rule.matches(content))
    }

    /// Filter sensitive information from content
    pub fn filter_sensitive(&self, content: &str) -> String {
        if !self.sensitive_filter.enabled {
            return content.to_string();
        }

        let mut result = content.to_string();

        // Filter email addresses
        if self.sensitive_filter.filter_email {
            result = Self::mask_email(&result);
        }

        // Filter phone numbers
        if self.sensitive_filter.filter_phone {
            result = Self::mask_phone(&result);
        }

        // Filter ID card numbers (Chinese 18-digit ID)
        if self.sensitive_filter.filter_id_card {
            result = Self::mask_id_card(&result);
        }

        // Filter bank card numbers
        if self.sensitive_filter.filter_bank_card {
            result = Self::mask_bank_card(&result);
        }

        // Filter IP addresses
        if self.sensitive_filter.filter_ip_address {
            result = Self::mask_ip(&result);
        }

        // Apply custom patterns
        for pattern in &self.sensitive_filter.custom_patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                result = re.replace_all(&result, "***FILTERED***").to_string();
            }
        }

        result
    }

    fn mask_email(text: &str) -> String {
        // Match email addresses: local@domain.tld
        let pattern = r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}";
        regex::Regex::new(pattern)
            .map(|re| re.replace_all(text, "***@***.***").to_string())
            .unwrap_or_else(|_| text.to_string())
    }

    fn mask_phone(text: &str) -> String {
        // Match common phone number formats with word boundaries
        // Chinese mobile: 1[3-9]xxxxxxxxx (11 digits)
        // International: +[country code][number]
        // General: 3-4 digit area code + 7-8 digit number
        let pattern = r"\b(?:\+?86[-\s]?)?1[3-9]\d{9}\b|\b(?:\+?\d{1,3}[-\s]?)?\(?\d{3,4}\)?[-\s]?\d{7,8}\b";
        regex::Regex::new(pattern)
            .map(|re| re.replace_all(text, "***-****-****").to_string())
            .unwrap_or_else(|_| text.to_string())
    }

    fn mask_id_card(text: &str) -> String {
        // Match Chinese 18-digit ID card number with word boundaries
        let pattern = r"\b\d{17}[\dXx]\b";
        regex::Regex::new(pattern)
            .map(|re| re.replace_all(text, "******************").to_string())
            .unwrap_or_else(|_| text.to_string())
    }

    fn mask_bank_card(text: &str) -> String {
        // Match bank card numbers (16-19 digits, possibly with spaces)
        let pattern = r"\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{0,3}";
        regex::Regex::new(pattern)
            .map(|re| re.replace_all(text, "****-****-****-****").to_string())
            .unwrap_or_else(|_| text.to_string())
    }

    fn mask_ip(text: &str) -> String {
        // Match IPv4 addresses
        let pattern = r"\b(?:(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\b";
        regex::Regex::new(pattern)
            .map(|re| re.replace_all(text, "***.***.***.***").to_string())
            .unwrap_or_else(|_| text.to_string())
    }

    /// Add an exclusion rule
    pub fn add_exclusion_rule(&mut self, rule: ExclusionRule) {
        if !self.exclusion_rules.iter().any(|r| r.name == rule.name) {
            self.exclusion_rules.push(rule);
        }
    }

    /// Remove an exclusion rule by name
    pub fn remove_exclusion_rule(&mut self, name: &str) -> Option<ExclusionRule> {
        let pos = self.exclusion_rules.iter().position(|r| r.name == name)?;
        Some(self.exclusion_rules.remove(pos))
    }

    /// Set data retention policy
    pub fn with_data_retention(mut self, policy: DataRetentionPolicy) -> Self {
        self.data_retention = policy;
        self
    }

    /// Set sensitive data filter
    pub fn with_sensitive_filter(mut self, filter: SensitiveDataFilter) -> Self {
        self.sensitive_filter = filter;
        self
    }

    /// Set memory sharing scope
    pub fn with_memory_sharing_scope(mut self, scope: MemorySharingScope) -> Self {
        self.memory_sharing_scope = scope;
        self
    }

    /// Set verbose logging
    pub fn with_verbose_logging(mut self, verbose: bool) -> Self {
        self.verbose_logging = verbose;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AgentPrivacyConfig::default();
        assert_eq!(config.data_retention.episodic_memory_days, 90);
        assert_eq!(config.data_retention.working_memory_hours, 24);
        assert!(config.data_retention.auto_cleanup);
        assert!(!config.sensitive_filter.enabled);
        assert!(config.sensitive_filter.filter_email);
        assert!(config.sensitive_filter.filter_phone);
        assert_eq!(config.memory_sharing_scope, MemorySharingScope::SingleSession);
        assert!(config.exclusion_rules.is_empty());
    }

    #[test]
    fn test_memory_sharing_scope_serialization() {
        let scope = MemorySharingScope::CrossSession;
        let json = serde_json::to_string(&scope).unwrap();
        assert_eq!(json, "\"crossSession\"");

        let parsed: MemorySharingScope = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, MemorySharingScope::CrossSession);
    }

    #[test]
    fn test_data_retention_policy() {
        let policy = DataRetentionPolicy::new()
            .with_episodic_days(30)
            .with_working_hours(12)
            .with_auto_cleanup(false);

        assert_eq!(policy.episodic_memory_days, 30);
        assert_eq!(policy.working_memory_hours, 12);
        assert!(!policy.auto_cleanup);
        assert!(!policy.is_episodic_forever());
    }

    #[test]
    fn test_data_retention_forever() {
        let policy = DataRetentionPolicy::new().with_episodic_days(0);
        assert!(policy.is_episodic_forever());
    }

    #[test]
    fn test_sensitive_filter_enabled() {
        let filter = SensitiveDataFilter::new().enable();
        assert!(filter.enabled);
    }

    #[test]
    fn test_filter_email() {
        let mut config = AgentPrivacyConfig::new();
        config.sensitive_filter.enabled = true;
        let filtered = config.filter_sensitive("Contact me at test@example.com for details");
        assert!(!filtered.contains("test@example.com"));
        assert!(filtered.contains("***@***.***"));
    }

    #[test]
    fn test_filter_phone() {
        let mut config = AgentPrivacyConfig::new();
        config.sensitive_filter.enabled = true;
        let filtered = config.filter_sensitive("Call me at 13812345678");
        assert!(!filtered.contains("13812345678"));
    }

    #[test]
    fn test_filter_id_card() {
        let mut config = AgentPrivacyConfig::new();
        config.sensitive_filter.enabled = true;
        let filtered = config.filter_sensitive("ID: 110101199001011234");
        assert!(!filtered.contains("110101199001011234"));
    }

    #[test]
    fn test_filter_bank_card() {
        let mut config = AgentPrivacyConfig::new();
        config.sensitive_filter.enabled = true;
        let filtered = config.filter_sensitive("Card: 6222021234567890123");
        assert!(!filtered.contains("6222021234567890123"));
    }

    #[test]
    fn test_filter_ip_address() {
        let mut config = AgentPrivacyConfig::new();
        config.sensitive_filter.enabled = true;
        config.sensitive_filter.filter_ip_address = true;
        let filtered = config.filter_sensitive("Server IP: 192.168.1.100");
        assert!(!filtered.contains("192.168.1.100"));
    }

    #[test]
    fn test_filter_disabled() {
        let config = AgentPrivacyConfig::new(); // filter disabled by default
        let original = "Email: test@example.com, Phone: 13812345678";
        let filtered = config.filter_sensitive(original);
        assert_eq!(filtered, original);
    }

    #[test]
    fn test_exclusion_rule() {
        let rule = ExclusionRule::new("password", r"password\s*[:=]\s*\S+")
            .with_description("Exclude password strings");

        assert!(rule.matches("password = secret123"));
        assert!(!rule.matches("no password here"));
    }

    #[test]
    fn test_exclusion_rule_disabled() {
        let rule = ExclusionRule::new("test", r"secret")
            .with_enabled(false);

        assert!(!rule.matches("this is a secret"));
    }

    #[test]
    fn test_should_exclude() {
        let mut config = AgentPrivacyConfig::new();
        config.add_exclusion_rule(ExclusionRule::new("password", r"password\s*[:=]\s*\S+"));

        assert!(config.should_exclude("password = secret123"));
        assert!(!config.should_exclude("no sensitive content"));
    }

    #[test]
    fn test_add_remove_exclusion_rule() {
        let mut config = AgentPrivacyConfig::new();

        config.add_exclusion_rule(ExclusionRule::new("rule1", r"pattern1"));
        config.add_exclusion_rule(ExclusionRule::new("rule2", r"pattern2"));
        assert_eq!(config.exclusion_rules.len(), 2);

        // Adding duplicate should not increase count
        config.add_exclusion_rule(ExclusionRule::new("rule1", r"pattern1"));
        assert_eq!(config.exclusion_rules.len(), 2);

        let removed = config.remove_exclusion_rule("rule1");
        assert!(removed.is_some());
        assert_eq!(config.exclusion_rules.len(), 1);

        let removed = config.remove_exclusion_rule("nonexistent");
        assert!(removed.is_none());
    }

    #[test]
    fn test_serialization() {
        let config = AgentPrivacyConfig::new()
            .with_data_retention(DataRetentionPolicy::new().with_episodic_days(60))
            .with_memory_sharing_scope(MemorySharingScope::CrossSession)
            .with_verbose_logging(true);

        let json = config.to_json().unwrap();
        assert!(json.contains("\"dataRetention\""));
        assert!(json.contains("\"episodicMemoryDays\":60"));
        assert!(json.contains("\"crossSession\""));
        assert!(json.contains("\"verboseLogging\":true"));

        let parsed: AgentPrivacyConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn test_from_json() {
        let json = r#"{"dataRetention":{"episodicMemoryDays":30,"workingMemoryHours":12,"autoCleanup":true},"sensitiveFilter":{"enabled":true,"filterEmail":true,"filterPhone":true,"filterIdCard":false,"filterBankCard":true,"filterIpAddress":false,"customPatterns":[]},"memorySharingScope":"crossAgent","exclusionRules":[],"verboseLogging":false}"#;
        let config = AgentPrivacyConfig::from_json(json).unwrap();

        assert_eq!(config.data_retention.episodic_memory_days, 30);
        assert_eq!(config.data_retention.working_memory_hours, 12);
        assert!(config.sensitive_filter.enabled);
        assert!(!config.sensitive_filter.filter_id_card);
        assert_eq!(config.memory_sharing_scope, MemorySharingScope::CrossAgent);
    }

    #[test]
    fn test_builder_pattern() {
        let config = AgentPrivacyConfig::new()
            .with_data_retention(DataRetentionPolicy::new().with_episodic_days(365))
            .with_sensitive_filter(SensitiveDataFilter::new().enable())
            .with_memory_sharing_scope(MemorySharingScope::CrossAgent)
            .with_verbose_logging(true);

        assert_eq!(config.data_retention.episodic_memory_days, 365);
        assert!(config.sensitive_filter.enabled);
        assert_eq!(config.memory_sharing_scope, MemorySharingScope::CrossAgent);
        assert!(config.verbose_logging);
    }

    #[test]
    fn test_custom_filter_pattern() {
        let mut config = AgentPrivacyConfig::new();
        config.sensitive_filter.enabled = true;
        config.sensitive_filter.custom_patterns.push(r"API[_-]?KEY[_-]?\w+".to_string());

        let filtered = config.filter_sensitive("API_KEY_SECRET=abc123");
        assert!(filtered.contains("***FILTERED***"));
    }
}