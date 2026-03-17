//! Privacy settings module for OmniNova Claw
//!
//! Provides data structures for privacy and security settings including:
//! - Data encryption settings
//! - Data retention policies
//! - Cloud sync settings (reserved for future)
//! - Storage information and management

pub mod storage;

use serde::{Deserialize, Serialize};

/// Privacy settings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PrivacySettings {
    /// Whether encryption is enabled
    pub encryption_enabled: bool,
    /// Data retention policy
    pub data_retention: DataRetentionPolicy,
    /// Cloud sync settings (reserved for future)
    pub cloud_sync: CloudSyncSettings,
    /// Last updated timestamp
    pub updated_at: i64,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            encryption_enabled: false,
            data_retention: DataRetentionPolicy::default(),
            cloud_sync: CloudSyncSettings::default(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl PrivacySettings {
    /// Create new privacy settings with encryption enabled
    pub fn with_encryption() -> Self {
        Self {
            encryption_enabled: true,
            ..Self::default()
        }
    }

    /// Update the timestamp
    pub fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Check if auto cleanup is enabled
    pub fn is_auto_cleanup_enabled(&self) -> bool {
        self.data_retention.auto_cleanup_enabled
    }

    /// Get conversation retention days (0 means forever)
    pub fn conversation_retention_days(&self) -> u32 {
        self.data_retention.conversation_retention_days
    }
}

/// Data retention policy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataRetentionPolicy {
    /// Conversation history retention days (0 = keep forever)
    pub conversation_retention_days: u32,
    /// Whether to automatically clean up expired data
    pub auto_cleanup_enabled: bool,
    /// Maximum storage size in MB (0 = no limit)
    pub max_storage_mb: u32,
}

impl Default for DataRetentionPolicy {
    fn default() -> Self {
        Self {
            conversation_retention_days: 0, // Keep forever by default
            auto_cleanup_enabled: false,
            max_storage_mb: 0, // No limit
        }
    }
}

impl DataRetentionPolicy {
    /// Create a new retention policy with specified days
    pub fn with_retention_days(days: u32) -> Self {
        Self {
            conversation_retention_days: days,
            auto_cleanup_enabled: days > 0,
            ..Self::default()
        }
    }

    /// Check if retention is unlimited
    pub fn is_unlimited(&self) -> bool {
        self.conversation_retention_days == 0
    }

    /// Get retention period as human-readable string
    pub fn retention_description(&self) -> String {
        if self.conversation_retention_days == 0 {
            "永久保留".to_string()
        } else {
            format!("保留 {} 天", self.conversation_retention_days)
        }
    }
}

/// Cloud sync settings (reserved for future implementation)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloudSyncSettings {
    /// Whether cloud sync is enabled
    pub enabled: bool,
    /// Sync scope
    pub sync_scope: SyncScope,
    /// Last sync timestamp
    pub last_sync_at: Option<i64>,
    /// Sync endpoint (if custom)
    pub sync_endpoint: Option<String>,
}

impl Default for CloudSyncSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            sync_scope: SyncScope::default(),
            last_sync_at: None,
            sync_endpoint: None,
        }
    }
}

/// Sync scope for cloud synchronization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SyncScope {
    /// Sync everything
    All,
    /// Sync only agents and settings
    AgentsAndSettings,
    /// Sync only conversation history
    ConversationsOnly,
    /// No sync
    None,
}

impl Default for SyncScope {
    fn default() -> Self {
        Self::None
    }
}

impl SyncScope {
    /// Check if agents should be synced
    pub fn includes_agents(&self) -> bool {
        matches!(self, Self::All | Self::AgentsAndSettings)
    }

    /// Check if conversations should be synced
    pub fn includes_conversations(&self) -> bool {
        matches!(self, Self::All | Self::ConversationsOnly)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_privacy_settings_default() {
        let settings = PrivacySettings::default();
        assert!(!settings.encryption_enabled);
        assert!(!settings.data_retention.auto_cleanup_enabled);
        assert!(!settings.cloud_sync.enabled);
    }

    #[test]
    fn test_privacy_settings_with_encryption() {
        let settings = PrivacySettings::with_encryption();
        assert!(settings.encryption_enabled);
    }

    #[test]
    fn test_privacy_settings_touch() {
        let mut settings = PrivacySettings::default();
        let old_time = settings.updated_at;
        // Sleep for 1 second to ensure timestamp difference
        std::thread::sleep(std::time::Duration::from_secs(1));
        settings.touch();
        assert!(settings.updated_at > old_time);
    }

    #[test]
    fn test_data_retention_policy_default() {
        let policy = DataRetentionPolicy::default();
        assert!(policy.is_unlimited());
        assert!(!policy.auto_cleanup_enabled);
    }

    #[test]
    fn test_data_retention_policy_with_days() {
        let policy = DataRetentionPolicy::with_retention_days(30);
        assert_eq!(policy.conversation_retention_days, 30);
        assert!(policy.auto_cleanup_enabled);
        assert!(!policy.is_unlimited());
    }

    #[test]
    fn test_data_retention_description() {
        let policy_unlimited = DataRetentionPolicy::default();
        assert_eq!(policy_unlimited.retention_description(), "永久保留");

        let policy_limited = DataRetentionPolicy::with_retention_days(7);
        assert_eq!(policy_limited.retention_description(), "保留 7 天");
    }

    #[test]
    fn test_sync_scope_includes() {
        let all = SyncScope::All;
        assert!(all.includes_agents());
        assert!(all.includes_conversations());

        let agents_only = SyncScope::AgentsAndSettings;
        assert!(agents_only.includes_agents());
        assert!(!agents_only.includes_conversations());

        let conv_only = SyncScope::ConversationsOnly;
        assert!(!conv_only.includes_agents());
        assert!(conv_only.includes_conversations());

        let none = SyncScope::None;
        assert!(!none.includes_agents());
        assert!(!none.includes_conversations());
    }

    #[test]
    fn test_privacy_settings_serialization() {
        let settings = PrivacySettings {
            encryption_enabled: true,
            data_retention: DataRetentionPolicy::with_retention_days(30),
            cloud_sync: CloudSyncSettings {
                enabled: true,
                sync_scope: SyncScope::All,
                last_sync_at: Some(1234567890),
                sync_endpoint: None,
            },
            updated_at: 1234567890,
        };

        let json = serde_json::to_string(&settings).expect("Serialization should succeed");
        let deserialized: PrivacySettings =
            serde_json::from_str(&json).expect("Deserialization should succeed");

        assert_eq!(settings, deserialized);
    }

    #[test]
    fn test_cloud_sync_settings_default() {
        let settings = CloudSyncSettings::default();
        assert!(!settings.enabled);
        assert_eq!(settings.sync_scope, SyncScope::None);
        assert!(settings.last_sync_at.is_none());
    }
}

// Re-export storage types
pub use storage::{
    ClearOptions, ClearResult, ClearScope, DateRange, StorageBreakdown, StorageInfo,
    calculate_directory_size, calculate_file_size, format_size, get_config_directory,
    get_data_directory,
};