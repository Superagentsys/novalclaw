//! Channel Behavior Configuration Types
//!
//! Core configuration structures for channel behavior customization.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::{ResponseDelay, ResponseStyle, TriggerKeyword, WorkingHours};

/// Channel behavior configuration
///
/// Controls how the agent responds on a specific channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelBehaviorConfig {
    /// Response style for this channel
    #[serde(default)]
    pub response_style: ResponseStyle,

    /// Trigger keywords that activate the agent
    #[serde(default)]
    pub trigger_keywords: Vec<TriggerKeyword>,

    /// Maximum response length in characters (0 = no limit)
    #[serde(default)]
    pub max_response_length: usize,

    /// Response delay configuration
    #[serde(default)]
    pub response_delay: ResponseDelay,

    /// Working hours configuration (None = always active)
    #[serde(default)]
    pub working_hours: Option<WorkingHours>,
}

impl Default for ChannelBehaviorConfig {
    fn default() -> Self {
        Self {
            response_style: ResponseStyle::default(),
            trigger_keywords: Vec::new(),
            max_response_length: 0,
            response_delay: ResponseDelay::default(),
            working_hours: None,
        }
    }
}

impl ChannelBehaviorConfig {
    /// Create a new behavior config with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the response style
    pub fn with_style(mut self, style: ResponseStyle) -> Self {
        self.response_style = style;
        self
    }

    /// Add a trigger keyword
    pub fn with_trigger(mut self, keyword: TriggerKeyword) -> Self {
        self.trigger_keywords.push(keyword);
        self
    }

    /// Set trigger keywords
    pub fn with_triggers(mut self, keywords: Vec<TriggerKeyword>) -> Self {
        self.trigger_keywords = keywords;
        self
    }

    /// Set maximum response length
    pub fn with_max_length(mut self, length: usize) -> Self {
        self.max_response_length = length;
        self
    }

    /// Set response delay
    pub fn with_delay(mut self, delay: ResponseDelay) -> Self {
        self.response_delay = delay;
        self
    }

    /// Set working hours
    pub fn with_working_hours(mut self, hours: WorkingHours) -> Self {
        self.working_hours = Some(hours);
        self
    }

    /// Check if any trigger keywords are configured
    pub fn has_triggers(&self) -> bool {
        !self.trigger_keywords.is_empty()
    }

    /// Check if response length is limited
    pub fn has_length_limit(&self) -> bool {
        self.max_response_length > 0
    }

    /// Check if working hours are configured
    pub fn has_working_hours(&self) -> bool {
        self.working_hours.is_some()
    }
}

/// Trait for storing and loading channel behavior configuration
pub trait ChannelBehaviorStore: Send + Sync {
    /// Save behavior configuration for a channel
    fn save(&self, channel_id: &str, config: &ChannelBehaviorConfig) -> Result<(), crate::channels::ChannelError>;

    /// Load behavior configuration for a channel
    fn load(&self, channel_id: &str) -> Result<Option<ChannelBehaviorConfig>, crate::channels::ChannelError>;

    /// Delete behavior configuration for a channel
    fn delete(&self, channel_id: &str) -> Result<(), crate::channels::ChannelError>;
}

/// SQLite implementation of ChannelBehaviorStore
pub struct SqliteBehaviorStore {
    conn: std::sync::Mutex<Connection>,
}

impl SqliteBehaviorStore {
    /// Create a new SQLite behavior store
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: std::sync::Mutex::new(conn),
        }
    }

    /// Create an in-memory store for testing
    pub fn in_memory() -> Result<Self, crate::channels::ChannelError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| crate::channels::ChannelError::ConfigurationError(e.to_string()))?;

        // Run migrations
        let migrations = crate::db::get_builtin_migrations();
        let runner = crate::db::MigrationRunner::new()
            .add_migrations(migrations)
            .map_err(|e| crate::channels::ChannelError::ConfigurationError(e.to_string()))?;

        runner.run(&conn)
            .map_err(|e| crate::channels::ChannelError::ConfigurationError(e.to_string()))?;

        Ok(Self::new(conn))
    }
}

impl ChannelBehaviorStore for SqliteBehaviorStore {
    fn save(&self, channel_id: &str, config: &ChannelBehaviorConfig) -> Result<(), crate::channels::ChannelError> {
        let conn = self.conn.lock()
            .map_err(|e| crate::channels::ChannelError::ConfigurationError(e.to_string()))?;

        let config_json = serde_json::to_string(config)
            .map_err(|e| crate::channels::ChannelError::ConfigurationError(e.to_string()))?;

        conn.execute(
            "INSERT OR REPLACE INTO channel_behavior_config (channel_id, config, updated_at)
             VALUES (?1, ?2, strftime('%s', 'now'))",
            rusqlite::params![channel_id, config_json],
        )
        .map_err(|e| crate::channels::ChannelError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    fn load(&self, channel_id: &str) -> Result<Option<ChannelBehaviorConfig>, crate::channels::ChannelError> {
        let conn = self.conn.lock()
            .map_err(|e| crate::channels::ChannelError::ConfigurationError(e.to_string()))?;

        let result = conn.query_row(
            "SELECT config FROM channel_behavior_config WHERE channel_id = ?1",
            rusqlite::params![channel_id],
            |row| row.get::<_, String>(0),
        );

        match result {
            Ok(config_json) => {
                let config: ChannelBehaviorConfig = serde_json::from_str(&config_json)
                    .map_err(|e| crate::channels::ChannelError::ConfigurationError(e.to_string()))?;
                Ok(Some(config))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(crate::channels::ChannelError::DatabaseError(e.to_string())),
        }
    }

    fn delete(&self, channel_id: &str) -> Result<(), crate::channels::ChannelError> {
        let conn = self.conn.lock()
            .map_err(|e| crate::channels::ChannelError::ConfigurationError(e.to_string()))?;

        conn.execute(
            "DELETE FROM channel_behavior_config WHERE channel_id = ?1",
            rusqlite::params![channel_id],
        )
        .map_err(|e| crate::channels::ChannelError::DatabaseError(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::TriggerKeyword;

    #[test]
    fn test_default_config() {
        let config = ChannelBehaviorConfig::default();
        assert_eq!(config.response_style, ResponseStyle::Detailed);
        assert!(config.trigger_keywords.is_empty());
        assert_eq!(config.max_response_length, 0);
        assert!(matches!(config.response_delay, ResponseDelay::None));
        assert!(config.working_hours.is_none());
    }

    #[test]
    fn test_builder_pattern() {
        let config = ChannelBehaviorConfig::new()
            .with_style(ResponseStyle::Concise)
            .with_max_length(500);

        assert_eq!(config.response_style, ResponseStyle::Concise);
        assert_eq!(config.max_response_length, 500);
    }

    #[test]
    fn test_serialization() {
        let config = ChannelBehaviorConfig::new()
            .with_style(ResponseStyle::Casual)
            .with_max_length(1000);

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ChannelBehaviorConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.response_style, ResponseStyle::Casual);
        assert_eq!(deserialized.max_response_length, 1000);
    }

    #[test]
    fn test_sqlite_store_save_and_load() {
        let store = SqliteBehaviorStore::in_memory().expect("Failed to create store");

        let config = ChannelBehaviorConfig::new()
            .with_style(ResponseStyle::Formal)
            .with_max_length(2000)
            .with_trigger(TriggerKeyword::new("help"));

        // Save
        store.save("channel-1", &config).expect("Failed to save");

        // Load
        let loaded = store.load("channel-1").expect("Failed to load");
        assert!(loaded.is_some());
        let loaded_config = loaded.unwrap();
        assert_eq!(loaded_config.response_style, ResponseStyle::Formal);
        assert_eq!(loaded_config.max_response_length, 2000);
        assert_eq!(loaded_config.trigger_keywords.len(), 1);
    }

    #[test]
    fn test_sqlite_store_load_nonexistent() {
        let store = SqliteBehaviorStore::in_memory().expect("Failed to create store");

        let loaded = store.load("nonexistent").expect("Failed to load");
        assert!(loaded.is_none());
    }

    #[test]
    fn test_sqlite_store_delete() {
        let store = SqliteBehaviorStore::in_memory().expect("Failed to create store");

        let config = ChannelBehaviorConfig::new()
            .with_style(ResponseStyle::Casual);

        // Save
        store.save("channel-to-delete", &config).expect("Failed to save");

        // Verify saved
        let loaded = store.load("channel-to-delete").expect("Failed to load");
        assert!(loaded.is_some());

        // Delete
        store.delete("channel-to-delete").expect("Failed to delete");

        // Verify deleted
        let loaded = store.load("channel-to-delete").expect("Failed to load");
        assert!(loaded.is_none());
    }

    #[test]
    fn test_sqlite_store_update() {
        let store = SqliteBehaviorStore::in_memory().expect("Failed to create store");

        // Save initial config
        let config1 = ChannelBehaviorConfig::new()
            .with_style(ResponseStyle::Formal);
        store.save("channel-update", &config1).expect("Failed to save");

        // Update with new config
        let config2 = ChannelBehaviorConfig::new()
            .with_style(ResponseStyle::Casual)
            .with_max_length(500);
        store.save("channel-update", &config2).expect("Failed to update");

        // Verify updated
        let loaded = store.load("channel-update").expect("Failed to load");
        assert!(loaded.is_some());
        let loaded_config = loaded.unwrap();
        assert_eq!(loaded_config.response_style, ResponseStyle::Casual);
        assert_eq!(loaded_config.max_response_length, 500);
    }
}