//! Provider configuration storage layer for database operations
//!
//! This module provides CRUD operations for provider configuration records
//! using the SQLite connection pool.

use crate::db::{DbConnection, DbPool};
use crate::providers::config::{
    ApiProtocol, NewProviderConfig, ProviderConfig, ProviderConfigUpdate, ProviderConfigValidationError,
    ProviderType,
};
use anyhow::Result;
use rusqlite::params;

/// Error type for provider configuration operations
#[derive(Debug, thiserror::Error)]
pub enum ProviderStoreError {
    #[error("Provider configuration not found: {0}")]
    NotFound(String),

    #[error("Provider configuration with name '{0}' already exists")]
    DuplicateName(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Pool error: {0}")]
    Pool(String),

    #[error("Validation error: {0}")]
    Validation(#[from] ProviderConfigValidationError),
}

/// Provider configuration storage handler
#[derive(Clone)]
pub struct ProviderStore {
    pool: DbPool,
}

impl ProviderStore {
    /// Create a new ProviderStore with the given connection pool
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Get a connection from the pool
    fn get_conn(&self) -> Result<DbConnection, ProviderStoreError> {
        self.pool
            .get()
            .map_err(|e| ProviderStoreError::Pool(e.to_string()))
    }

    /// Create a new provider configuration
    pub fn create(&self, config: &NewProviderConfig) -> Result<ProviderConfig, ProviderStoreError> {
        // Validate the configuration data
        config.validate()?;

        let conn = self.get_conn()?;
        let id = NewProviderConfig::generate_id();
        let timestamp = NewProviderConfig::current_timestamp();

        // If this is set as default, clear other defaults first
        if config.is_default {
            conn.execute("UPDATE provider_configs SET is_default = 0", [])?;
        }

        // Resolve default model if not specified
        let default_model = config
            .default_model
            .clone()
            .unwrap_or_else(|| config.provider_type.default_model().to_string());

        // Resolve base URL if not specified
        let base_url = config
            .base_url
            .clone()
            .or_else(|| config.provider_type.default_base_url().map(|s| s.to_string()));

        // Resolve API protocol (only for custom provider)
        let api_protocol = config.api_protocol.unwrap_or(ApiProtocol::Openai);

        conn.execute(
            "INSERT INTO provider_configs (id, name, provider_type, api_key_ref, base_url, default_model, settings, is_default, api_protocol, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                &id,
                &config.name,
                config.provider_type.to_string(),
                &config.api_key_ref,
                &base_url,
                &Some(default_model.clone()),
                &config.settings,
                config.is_default as i32,
                api_protocol.to_string(),
                timestamp,
                timestamp,
            ],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed: provider_configs.name") {
                ProviderStoreError::DuplicateName(config.name.clone())
            } else {
                ProviderStoreError::Database(e)
            }
        })?;

        Ok(ProviderConfig {
            id,
            name: config.name.clone(),
            provider_type: config.provider_type,
            api_key_ref: config.api_key_ref.clone(),
            base_url,
            default_model: Some(default_model),
            settings: config.settings.clone(),
            is_default: config.is_default,
            api_protocol: Some(api_protocol),
            created_at: timestamp,
            updated_at: timestamp,
        })
    }

    /// Find a provider configuration by ID
    pub fn find_by_id(&self, id: &str) -> Result<Option<ProviderConfig>, ProviderStoreError> {
        let conn = self.get_conn()?;
        let result = conn.query_row(
            "SELECT id, name, provider_type, api_key_ref, base_url, default_model, settings, is_default, api_protocol, created_at, updated_at
             FROM provider_configs WHERE id = ?1",
            params![id],
            |row| {
                let provider_type_str: String = row.get(2)?;
                let provider_type = ProviderType::from_db_string(&provider_type_str, 2)?;
                let is_default: i32 = row.get(7)?;
                let api_protocol_str: Option<String> = row.get(8)?;
                let api_protocol = api_protocol_str
                    .as_deref()
                    .and_then(|s| s.parse::<ApiProtocol>().ok());
                Ok(ProviderConfig {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    provider_type,
                    api_key_ref: row.get(3)?,
                    base_url: row.get(4)?,
                    default_model: row.get(5)?,
                    settings: row.get(6)?,
                    is_default: is_default != 0,
                    api_protocol,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            },
        );

        match result {
            Ok(config) => Ok(Some(config)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Find a provider configuration by name
    pub fn find_by_name(&self, name: &str) -> Result<Option<ProviderConfig>, ProviderStoreError> {
        let conn = self.get_conn()?;
        let result = conn.query_row(
            "SELECT id, name, provider_type, api_key_ref, base_url, default_model, settings, is_default, api_protocol, created_at, updated_at
             FROM provider_configs WHERE name = ?1",
            params![name],
            |row| {
                let provider_type_str: String = row.get(2)?;
                let provider_type = ProviderType::from_db_string(&provider_type_str, 2)?;
                let is_default: i32 = row.get(7)?;
                let api_protocol_str: Option<String> = row.get(8)?;
                let api_protocol = api_protocol_str
                    .as_deref()
                    .and_then(|s| s.parse::<ApiProtocol>().ok());
                Ok(ProviderConfig {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    provider_type,
                    api_key_ref: row.get(3)?,
                    base_url: row.get(4)?,
                    default_model: row.get(5)?,
                    settings: row.get(6)?,
                    is_default: is_default != 0,
                    api_protocol,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            },
        );

        match result {
            Ok(config) => Ok(Some(config)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Find all provider configurations
    pub fn find_all(&self) -> Result<Vec<ProviderConfig>, ProviderStoreError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, provider_type, api_key_ref, base_url, default_model, settings, is_default, api_protocol, created_at, updated_at
             FROM provider_configs ORDER BY is_default DESC, created_at ASC",
        )?;

        let configs = stmt.query_map([], |row| {
            let provider_type_str: String = row.get(2)?;
            let provider_type = ProviderType::from_db_string(&provider_type_str, 2)?;
            let is_default: i32 = row.get(7)?;
            let api_protocol_str: Option<String> = row.get(8)?;
            let api_protocol = api_protocol_str
                .as_deref()
                .and_then(|s| s.parse::<ApiProtocol>().ok());
            Ok(ProviderConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                provider_type,
                api_key_ref: row.get(3)?,
                base_url: row.get(4)?,
                default_model: row.get(5)?,
                settings: row.get(6)?,
                is_default: is_default != 0,
                api_protocol,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })?;

        let mut result = Vec::new();
        for config in configs {
            result.push(config?);
        }
        Ok(result)
    }

    /// Find the default provider configuration
    pub fn find_default(&self) -> Result<Option<ProviderConfig>, ProviderStoreError> {
        let conn = self.get_conn()?;
        let result = conn.query_row(
            "SELECT id, name, provider_type, api_key_ref, base_url, default_model, settings, is_default, api_protocol, created_at, updated_at
             FROM provider_configs WHERE is_default = 1 LIMIT 1",
            [],
            |row| {
                let provider_type_str: String = row.get(2)?;
                let provider_type = ProviderType::from_db_string(&provider_type_str, 2)?;
                let api_protocol_str: Option<String> = row.get(8)?;
                let api_protocol = api_protocol_str
                    .as_deref()
                    .and_then(|s| s.parse::<ApiProtocol>().ok());
                Ok(ProviderConfig {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    provider_type,
                    api_key_ref: row.get(3)?,
                    base_url: row.get(4)?,
                    default_model: row.get(5)?,
                    settings: row.get(6)?,
                    is_default: true,
                    api_protocol,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            },
        );

        match result {
            Ok(config) => Ok(Some(config)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Find provider configurations by type
    pub fn find_by_type(
        &self,
        provider_type: ProviderType,
    ) -> Result<Vec<ProviderConfig>, ProviderStoreError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, provider_type, api_key_ref, base_url, default_model, settings, is_default, api_protocol, created_at, updated_at
             FROM provider_configs WHERE provider_type = ?1 ORDER BY created_at ASC",
        )?;

        let configs = stmt.query_map(params![provider_type.to_string()], |row| {
            let provider_type_str: String = row.get(2)?;
            let parsed_type = ProviderType::from_db_string(&provider_type_str, 2)?;
            let is_default: i32 = row.get(7)?;
            let api_protocol_str: Option<String> = row.get(8)?;
            let api_protocol = api_protocol_str
                .as_deref()
                .and_then(|s| s.parse::<ApiProtocol>().ok());
            Ok(ProviderConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                provider_type: parsed_type,
                api_key_ref: row.get(3)?,
                base_url: row.get(4)?,
                default_model: row.get(5)?,
                settings: row.get(6)?,
                is_default: is_default != 0,
                api_protocol,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })?;

        let mut result = Vec::new();
        for config in configs {
            result.push(config?);
        }
        Ok(result)
    }

    /// Update a provider configuration
    pub fn update(
        &self,
        id: &str,
        updates: &ProviderConfigUpdate,
    ) -> Result<ProviderConfig, ProviderStoreError> {
        // First check if config exists
        let existing = self
            .find_by_id(id)?
            .ok_or_else(|| ProviderStoreError::NotFound(id.to_string()))?;

        let conn = self.get_conn()?;
        let timestamp = NewProviderConfig::current_timestamp();

        // If setting as default, clear other defaults first
        if updates.is_default == Some(true) {
            conn.execute("UPDATE provider_configs SET is_default = 0", [])?;
        }

        let new_name = updates.name.as_ref().unwrap_or(&existing.name);
        let new_api_key_ref = updates.api_key_ref.as_ref().or(existing.api_key_ref.as_ref());
        let new_base_url = updates.base_url.as_ref().or(existing.base_url.as_ref());
        let new_default_model = updates
            .default_model
            .as_ref()
            .or(existing.default_model.as_ref());
        let new_settings = updates.settings.as_ref().or(existing.settings.as_ref());
        let new_is_default = updates.is_default.unwrap_or(existing.is_default);
        let new_api_protocol = updates.api_protocol.or(existing.api_protocol);

        conn.execute(
            "UPDATE provider_configs
             SET name = ?1, api_key_ref = ?2, base_url = ?3, default_model = ?4, settings = ?5, is_default = ?6, api_protocol = ?7, updated_at = ?8
             WHERE id = ?9",
            params![
                new_name,
                new_api_key_ref,
                new_base_url,
                new_default_model,
                new_settings,
                new_is_default as i32,
                new_api_protocol.map(|p| p.to_string()),
                timestamp,
                id,
            ],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed: provider_configs.name") {
                ProviderStoreError::DuplicateName(new_name.clone())
            } else {
                ProviderStoreError::Database(e)
            }
        })?;

        Ok(ProviderConfig {
            id: existing.id,
            name: new_name.clone(),
            provider_type: existing.provider_type,
            api_key_ref: new_api_key_ref.cloned(),
            base_url: new_base_url.cloned(),
            default_model: new_default_model.cloned(),
            settings: new_settings.cloned(),
            is_default: new_is_default,
            api_protocol: new_api_protocol,
            created_at: existing.created_at,
            updated_at: timestamp,
        })
    }

    /// Delete a provider configuration
    pub fn delete(&self, id: &str) -> Result<(), ProviderStoreError> {
        let conn = self.get_conn()?;
        let rows_affected = conn.execute("DELETE FROM provider_configs WHERE id = ?1", params![id])?;

        if rows_affected == 0 {
            return Err(ProviderStoreError::NotFound(id.to_string()));
        }

        Ok(())
    }

    /// Set a provider as the default
    pub fn set_default(&self, id: &str) -> Result<(), ProviderStoreError> {
        // First check if config exists
        let _existing = self
            .find_by_id(id)?
            .ok_or_else(|| ProviderStoreError::NotFound(id.to_string()))?;

        let conn = self.get_conn()?;
        let timestamp = NewProviderConfig::current_timestamp();

        // Clear all defaults
        conn.execute("UPDATE provider_configs SET is_default = 0", [])?;

        // Set new default
        conn.execute(
            "UPDATE provider_configs SET is_default = 1, updated_at = ?1 WHERE id = ?2",
            params![timestamp, id],
        )?;

        Ok(())
    }

    /// Check if a provider configuration exists by name
    pub fn exists_by_name(&self, name: &str) -> Result<bool, ProviderStoreError> {
        let conn = self.get_conn()?;
        let count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM provider_configs WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_builtin_runner, create_pool, DbPoolConfig};
    use crate::providers::config::ProviderType;
    use tempfile::tempdir;

    fn create_test_store() -> ProviderStore {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let pool = create_pool(&db_path, DbPoolConfig::default()).expect("Failed to create pool");
        let conn = pool.get().expect("Failed to get connection");
        let runner = create_builtin_runner();
        runner.run(&conn).expect("Failed to run migrations");
        ProviderStore::new(pool)
    }

    #[test]
    fn test_create_provider_config() {
        let store = create_test_store();

        let new_config = NewProviderConfig {
            name: "Test OpenAI".to_string(),
            provider_type: ProviderType::OpenAI,
            api_key_ref: Some("keychain://openai-key".to_string()),
            base_url: None,
            default_model: Some("gpt-4o".to_string()),
            settings: None,
            api_protocol: None,
            is_default: true,
        };

        let created = store.create(&new_config).expect("Failed to create config");
        assert!(!created.id.is_empty());
        assert_eq!(created.name, "Test OpenAI");
        assert_eq!(created.provider_type, ProviderType::OpenAI);
        assert!(created.is_default);
    }

    #[test]
    fn test_create_provider_config_with_defaults() {
        let store = create_test_store();

        let new_config = NewProviderConfig {
            name: "My Ollama".to_string(),
            provider_type: ProviderType::Ollama,
            api_key_ref: None,
            base_url: None, // Should use default
            default_model: None, // Should use default
            settings: None,
            api_protocol: None,
            is_default: false,
        };

        let created = store.create(&new_config).expect("Failed to create config");
        assert_eq!(created.default_model, Some("llama3.2".to_string()));
        assert_eq!(created.base_url, Some("http://localhost:11434/v1".to_string()));
    }

    #[test]
    fn test_create_duplicate_name_fails() {
        let store = create_test_store();

        let config1 = NewProviderConfig {
            name: "Duplicate".to_string(),
            provider_type: ProviderType::OpenAI,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            api_protocol: None,
            is_default: false,
        };

        store.create(&config1).expect("Failed to create first config");

        let config2 = NewProviderConfig {
            name: "Duplicate".to_string(),
            provider_type: ProviderType::Anthropic,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            api_protocol: None,
            is_default: false,
        };

        let result = store.create(&config2);
        assert!(matches!(result, Err(ProviderStoreError::DuplicateName(_))));
    }

    #[test]
    fn test_find_by_id() {
        let store = create_test_store();

        let new_config = NewProviderConfig {
            name: "Find Me".to_string(),
            provider_type: ProviderType::Anthropic,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            api_protocol: None,
            is_default: false,
        };

        let created = store.create(&new_config).expect("Failed to create config");
        let found = store.find_by_id(&created.id).expect("Failed to find config");

        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Find Me");
    }

    #[test]
    fn test_find_by_id_not_found() {
        let store = create_test_store();
        let found = store.find_by_id("nonexistent-id").expect("Failed to query");
        assert!(found.is_none());
    }

    #[test]
    fn test_find_by_name() {
        let store = create_test_store();

        let new_config = NewProviderConfig {
            name: "Unique Name".to_string(),
            provider_type: ProviderType::Gemini,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            api_protocol: None,
            is_default: false,
        };

        store.create(&new_config).expect("Failed to create config");
        let found = store.find_by_name("Unique Name").expect("Failed to find config");

        assert!(found.is_some());
        assert_eq!(found.unwrap().provider_type, ProviderType::Gemini);
    }

    #[test]
    fn test_find_all() {
        let store = create_test_store();

        for i in 0..3 {
            let config = NewProviderConfig {
                name: format!("Provider {}", i),
                provider_type: ProviderType::OpenAI,
                api_key_ref: None,
                base_url: None,
                default_model: None,
                settings: None,
            api_protocol: None,
                is_default: i == 0,
            };
            store.create(&config).expect("Failed to create config");
        }

        let all = store.find_all().expect("Failed to find all");
        assert_eq!(all.len(), 3);
        // Default should be first
        assert!(all[0].is_default);
    }

    #[test]
    fn test_find_default() {
        let store = create_test_store();

        // No default initially
        let no_default = store.find_default().expect("Failed to query");
        assert!(no_default.is_none());

        // Create with default
        let config = NewProviderConfig {
            name: "Default Provider".to_string(),
            provider_type: ProviderType::DeepSeek,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            api_protocol: None,
            is_default: true,
        };
        store.create(&config).expect("Failed to create config");

        let default = store.find_default().expect("Failed to find default");
        assert!(default.is_some());
        assert_eq!(default.unwrap().name, "Default Provider");
    }

    #[test]
    fn test_find_by_type() {
        let store = create_test_store();

        // Create providers of different types
        let config1 = NewProviderConfig {
            name: "OpenAI 1".to_string(),
            provider_type: ProviderType::OpenAI,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            api_protocol: None,
            is_default: false,
        };
        let config2 = NewProviderConfig {
            name: "Anthropic 1".to_string(),
            provider_type: ProviderType::Anthropic,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            api_protocol: None,
            is_default: false,
        };
        let config3 = NewProviderConfig {
            name: "OpenAI 2".to_string(),
            provider_type: ProviderType::OpenAI,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            api_protocol: None,
            is_default: false,
        };

        store.create(&config1).expect("Failed to create");
        store.create(&config2).expect("Failed to create");
        store.create(&config3).expect("Failed to create");

        let openai_configs = store.find_by_type(ProviderType::OpenAI).expect("Failed to find");
        assert_eq!(openai_configs.len(), 2);

        let anthropic_configs = store.find_by_type(ProviderType::Anthropic).expect("Failed to find");
        assert_eq!(anthropic_configs.len(), 1);
    }

    #[test]
    fn test_update_provider_config() {
        let store = create_test_store();

        let new_config = NewProviderConfig {
            name: "Original Name".to_string(),
            provider_type: ProviderType::OpenAI,
            api_key_ref: None,
            base_url: None,
            default_model: Some("gpt-3.5-turbo".to_string()),
            settings: None,
            api_protocol: None,
            is_default: false,
        };

        let created = store.create(&new_config).expect("Failed to create");

        let update = ProviderConfigUpdate {
            name: Some("Updated Name".to_string()),
            default_model: Some("gpt-4o".to_string()),
            is_default: Some(true),
            ..Default::default()
        };

        let updated = store.update(&created.id, &update).expect("Failed to update");
        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.default_model, Some("gpt-4o".to_string()));
        assert!(updated.is_default);
    }

    #[test]
    fn test_update_not_found() {
        let store = create_test_store();

        let update = ProviderConfigUpdate {
            name: Some("New Name".to_string()),
            ..Default::default()
        };

        let result = store.update("nonexistent-id", &update);
        assert!(matches!(result, Err(ProviderStoreError::NotFound(_))));
    }

    #[test]
    fn test_delete_provider_config() {
        let store = create_test_store();

        let new_config = NewProviderConfig {
            name: "To Delete".to_string(),
            provider_type: ProviderType::OpenAI,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            api_protocol: None,
            is_default: false,
        };

        let created = store.create(&new_config).expect("Failed to create");
        store.delete(&created.id).expect("Failed to delete");

        let found = store.find_by_id(&created.id).expect("Failed to query");
        assert!(found.is_none());
    }

    #[test]
    fn test_delete_not_found() {
        let store = create_test_store();
        let result = store.delete("nonexistent-id");
        assert!(matches!(result, Err(ProviderStoreError::NotFound(_))));
    }

    #[test]
    fn test_set_default() {
        let store = create_test_store();

        // Create two configs, first as default
        let config1 = NewProviderConfig {
            name: "First".to_string(),
            provider_type: ProviderType::OpenAI,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            api_protocol: None,
            is_default: true,
        };
        let config2 = NewProviderConfig {
            name: "Second".to_string(),
            provider_type: ProviderType::Anthropic,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            api_protocol: None,
            is_default: false,
        };

        let created1 = store.create(&config1).expect("Failed to create");
        let created2 = store.create(&config2).expect("Failed to create");

        // Verify first is default
        let default_before = store.find_default().expect("Failed to find default");
        assert_eq!(default_before.unwrap().name, "First");

        // Set second as default
        store.set_default(&created2.id).expect("Failed to set default");

        // Verify second is now default
        let default_after = store.find_default().expect("Failed to find default");
        assert_eq!(default_after.unwrap().name, "Second");

        // Verify first is no longer default
        let first = store.find_by_id(&created1.id).expect("Failed to find").unwrap();
        assert!(!first.is_default);
    }

    #[test]
    fn test_exists_by_name() {
        let store = create_test_store();

        assert!(!store.exists_by_name("Nonexistent").expect("Failed to check"));

        let config = NewProviderConfig {
            name: "Existing".to_string(),
            provider_type: ProviderType::OpenAI,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            api_protocol: None,
            is_default: false,
        };
        store.create(&config).expect("Failed to create");

        assert!(store.exists_by_name("Existing").expect("Failed to check"));
    }

    #[test]
    fn test_create_clears_other_defaults() {
        let store = create_test_store();

        // Create first as default
        let config1 = NewProviderConfig {
            name: "First Default".to_string(),
            provider_type: ProviderType::OpenAI,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            api_protocol: None,
            is_default: true,
        };
        let created1 = store.create(&config1).expect("Failed to create");

        // Create second as default
        let config2 = NewProviderConfig {
            name: "Second Default".to_string(),
            provider_type: ProviderType::Anthropic,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            api_protocol: None,
            is_default: true,
        };
        store.create(&config2).expect("Failed to create");

        // Verify first is no longer default
        let first = store.find_by_id(&created1.id).expect("Failed to find").unwrap();
        assert!(!first.is_default);

        // Verify second is default
        let default = store.find_default().expect("Failed to find default");
        assert_eq!(default.unwrap().name, "Second Default");
    }
}