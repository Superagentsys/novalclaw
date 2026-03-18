//! Agent storage layer for database operations
//!
//! This module provides CRUD operations for Agent records
//! using the SQLite connection pool.

use crate::agent::model::{AgentModel, AgentStatus, AgentUpdate, NewAgent};
use crate::db::migrations::create_builtin_runner;
use crate::db::{DbConnection, DbPool};
use anyhow::{Context, Result};
use rusqlite::params;

/// Error type for agent operations
#[derive(Debug, thiserror::Error)]
pub enum AgentStoreError {
    #[error("Agent not found: {0}")]
    NotFound(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Pool error: {0}")]
    Pool(String),

    #[error("Validation error: {0}")]
    Validation(#[from] crate::agent::model::AgentValidationError),
}

/// Agent storage handler
#[derive(Clone)]
pub struct AgentStore {
    pool: DbPool,
}

impl AgentStore {
    /// Create a new AgentStore with the given connection pool
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Get a connection from the pool
    fn get_conn(&self) -> Result<DbConnection, AgentStoreError> {
        self.pool.get().map_err(|e| AgentStoreError::Pool(e.to_string()))
    }

    /// Initialize the database with migrations
    pub fn initialize(&self) -> Result<()> {
        let conn = self.get_conn()?;
        let runner = create_builtin_runner();
        runner.run(&conn).context("Failed to run migrations")?;
        Ok(())
    }

    /// Create a new agent
    pub fn create(&self, agent: &NewAgent) -> Result<AgentModel, AgentStoreError> {
        // Validate the agent data
        agent.validate()?;

        let conn = self.get_conn()?;
        let uuid = NewAgent::generate_uuid();
        let timestamp = NewAgent::current_timestamp();

        conn.execute(
            "INSERT INTO agents (agent_uuid, name, description, domain, mbti_type, system_prompt, status, default_provider_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                &uuid,
                &agent.name,
                &agent.description,
                &agent.domain,
                &agent.mbti_type,
                &agent.system_prompt,
                "active", // default status
                &agent.default_provider_id,
                timestamp,
                timestamp,
            ],
        )?;

        let id = conn.last_insert_rowid();
        Ok(AgentModel {
            id,
            agent_uuid: uuid,
            name: agent.name.clone(),
            description: agent.description.clone(),
            domain: agent.domain.clone(),
            mbti_type: agent.mbti_type.clone(),
            system_prompt: agent.system_prompt.clone(),
            status: AgentStatus::Active,
            default_provider_id: agent.default_provider_id.clone(),
            created_at: timestamp,
            updated_at: timestamp,
        })
    }

    /// Find an agent by UUID
    pub fn find_by_uuid(&self, uuid: &str) -> Result<Option<AgentModel>, AgentStoreError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, agent_uuid, name, description, domain, mbti_type, system_prompt, status, default_provider_id, created_at, updated_at
             FROM agents WHERE agent_uuid = ?1"
        )?;

        let result = stmt.query_row(params![uuid], |row| {
            let status_str: String = row.get(7)?;
            let status = AgentStatus::from_db_string(&status_str, 7)?;
            Ok(AgentModel {
                id: row.get(0)?,
                agent_uuid: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                domain: row.get(4)?,
                mbti_type: row.get(5)?,
                system_prompt: row.get(6)?,
                status,
                default_provider_id: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        });

        match result {
            Ok(agent) => Ok(Some(agent)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Find an agent by database ID
    pub fn find_by_id(&self, id: i64) -> Result<Option<AgentModel>, AgentStoreError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, agent_uuid, name, description, domain, mbti_type, system_prompt, status, default_provider_id, created_at, updated_at
             FROM agents WHERE id = ?1"
        )?;

        let result = stmt.query_row(params![id], |row| {
            let status_str: String = row.get(7)?;
            let status = AgentStatus::from_db_string(&status_str, 7)?;
            Ok(AgentModel {
                id: row.get(0)?,
                agent_uuid: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                domain: row.get(4)?,
                mbti_type: row.get(5)?,
                system_prompt: row.get(6)?,
                status,
                default_provider_id: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        });

        match result {
            Ok(agent) => Ok(Some(agent)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Find all agents
    pub fn find_all(&self) -> Result<Vec<AgentModel>, AgentStoreError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, agent_uuid, name, description, domain, mbti_type, system_prompt, status, default_provider_id, created_at, updated_at
             FROM agents ORDER BY created_at DESC"
        )?;

        let agents = stmt.query_map([], |row| {
            let status_str: String = row.get(7)?;
            let status = AgentStatus::from_db_string(&status_str, 7)?;
            Ok(AgentModel {
                id: row.get(0)?,
                agent_uuid: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                domain: row.get(4)?,
                mbti_type: row.get(5)?,
                system_prompt: row.get(6)?,
                status,
                default_provider_id: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })?;

        let mut result = Vec::new();
        for agent in agents {
            result.push(agent?);
        }
        Ok(result)
    }

    /// Update an agent by UUID
    pub fn update(&self, uuid: &str, updates: &AgentUpdate) -> Result<AgentModel, AgentStoreError> {
        // First check if agent exists
        let existing = self.find_by_uuid(uuid)?
            .ok_or_else(|| AgentStoreError::NotFound(uuid.to_string()))?;

        let conn = self.get_conn()?;
        let timestamp = NewAgent::current_timestamp();

        let new_name = updates.name.as_ref().unwrap_or(&existing.name);
        let new_description = updates.description.as_ref().or(existing.description.as_ref());
        let new_domain = updates.domain.as_ref().or(existing.domain.as_ref());
        let new_mbti = updates.mbti_type.as_ref().or(existing.mbti_type.as_ref());
        let new_prompt = updates.system_prompt.as_ref().or(existing.system_prompt.as_ref());
        let new_status = updates.status.unwrap_or(existing.status);
        let new_provider_id = updates.default_provider_id.as_ref().or(existing.default_provider_id.as_ref());

        conn.execute(
            "UPDATE agents SET name = ?1, description = ?2, domain = ?3, mbti_type = ?4, system_prompt = ?5, status = ?6, default_provider_id = ?7, updated_at = ?8
             WHERE agent_uuid = ?9",
            params![
                new_name,
                new_description,
                new_domain,
                new_mbti,
                new_prompt,
                new_status.to_string(),
                new_provider_id,
                timestamp,
                uuid,
            ],
        )?;

        Ok(AgentModel {
            id: existing.id,
            agent_uuid: existing.agent_uuid,
            name: new_name.clone(),
            description: new_description.cloned(),
            domain: new_domain.cloned(),
            mbti_type: new_mbti.cloned(),
            system_prompt: new_prompt.cloned(),
            status: new_status,
            default_provider_id: new_provider_id.cloned(),
            created_at: existing.created_at,
            updated_at: timestamp,
        })
    }

    /// Delete an agent by UUID
    pub fn delete(&self, uuid: &str) -> Result<(), AgentStoreError> {
        let conn = self.get_conn()?;
        let rows_affected = conn.execute("DELETE FROM agents WHERE agent_uuid = ?1", params![uuid])?;

        if rows_affected == 0 {
            return Err(AgentStoreError::NotFound(uuid.to_string()));
        }

        Ok(())
    }

    /// Update agent status
    pub fn update_status(&self, uuid: &str, status: AgentStatus) -> Result<AgentModel, AgentStoreError> {
        self.update(uuid, &AgentUpdate {
            status: Some(status),
            ..Default::default()
        })
    }

    /// Duplicate an agent, creating a copy with a new UUID
    ///
    /// # Arguments
    /// * `uuid` - The UUID of the agent to duplicate
    ///
    /// # Returns
    /// * `Result<AgentModel>` - The newly created duplicate agent
    ///
    /// # Behavior
    /// - Generates a new UUID for the duplicate
    /// - Appends " (副本)" to the original name
    /// - Copies all configuration fields (description, domain, mbti_type, system_prompt, default_provider_id)
    /// - Sets status to 'active' regardless of original status
    /// - Sets new created_at and updated_at timestamps
    pub fn duplicate(&self, uuid: &str) -> Result<AgentModel, AgentStoreError> {
        // Get the original agent
        let original = self.find_by_uuid(uuid)?
            .ok_or_else(|| AgentStoreError::NotFound(uuid.to_string()))?;

        // Create the duplicate with new UUID and "(副本)" suffix
        let duplicated_name = format!("{} (副本)", original.name);
        let timestamp = NewAgent::current_timestamp();
        let new_uuid = NewAgent::generate_uuid();

        let conn = self.get_conn()?;
        conn.execute(
            "INSERT INTO agents (agent_uuid, name, description, domain, mbti_type, system_prompt, status, default_provider_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                &new_uuid,
                &duplicated_name,
                &original.description,
                &original.domain,
                &original.mbti_type,
                &original.system_prompt,
                "active", // Always set status to active
                &original.default_provider_id,
                timestamp,
                timestamp,
            ],
        )?;

        let id = conn.last_insert_rowid();
        Ok(AgentModel {
            id,
            agent_uuid: new_uuid,
            name: duplicated_name,
            description: original.description,
            domain: original.domain,
            mbti_type: original.mbti_type,
            system_prompt: original.system_prompt,
            status: AgentStatus::Active,
            default_provider_id: original.default_provider_id,
            created_at: timestamp,
            updated_at: timestamp,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_pool, DbPoolConfig};
    use tempfile::tempdir;

    fn create_test_store() -> AgentStore {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let pool = create_pool(&db_path, DbPoolConfig::default()).expect("Failed to create pool");
        let store = AgentStore::new(pool);
        store.initialize().expect("Failed to initialize");
        store
    }

    #[test]
    fn test_create_agent() {
        let store = create_test_store();

        let new_agent = NewAgent {
            name: "Test Agent".to_string(),
            description: Some("A test agent".to_string()),
            domain: Some("coding".to_string()),
            mbti_type: Some("INTJ".to_string()),
            system_prompt: Some("You are a helpful assistant.".to_string()),
            default_provider_id: Some("openai-provider".to_string()),
        };

        let created = store.create(&new_agent).expect("Failed to create agent");

        assert!(created.id > 0);
        assert!(!created.agent_uuid.is_empty());
        assert_eq!(created.name, "Test Agent");
        assert_eq!(created.status, AgentStatus::Active);
        assert_eq!(created.default_provider_id, Some("openai-provider".to_string()));
    }

    #[test]
    fn test_find_by_uuid() {
        let store = create_test_store();

        let new_agent = NewAgent {
            name: "Findable Agent".to_string(),
            description: None,
            domain: None,
            mbti_type: Some("ENFP".to_string()),
            system_prompt: None,
            default_provider_id: Some("anthropic-provider".to_string()),
        };

        let created = store.create(&new_agent).expect("Failed to create agent");

        // Find the agent
        let found = store.find_by_uuid(&created.agent_uuid).expect("Failed to find agent");
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.name, "Findable Agent");
        assert_eq!(found.mbti_type, Some("ENFP".to_string()));
        assert_eq!(found.default_provider_id, Some("anthropic-provider".to_string()));

        // Try to find non-existent agent
        let not_found = store.find_by_uuid("non-existent-uuid").expect("Query failed");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_find_all() {
        let store = create_test_store();

        // Create multiple agents
        for i in 0..3 {
            store.create(&NewAgent {
                name: format!("Agent {}", i),
                description: None,
                domain: None,
                mbti_type: None,
                system_prompt: None,
                default_provider_id: None,
            }).expect("Failed to create agent");
        }

        let all = store.find_all().expect("Failed to find all");
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_update_agent() {
        let store = create_test_store();

        let created = store.create(&NewAgent {
            name: "Original Name".to_string(),
            description: Some("Original description".to_string()),
            domain: None,
            mbti_type: None,
            system_prompt: None,
            default_provider_id: Some("original-provider".to_string()),
        }).expect("Failed to create agent");

        let updates = AgentUpdate {
            name: Some("Updated Name".to_string()),
            status: Some(AgentStatus::Inactive),
            default_provider_id: Some("new-provider".to_string()),
            ..Default::default()
        };

        let updated = store.update(&created.agent_uuid, &updates).expect("Failed to update agent");
        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.status, AgentStatus::Inactive);
        assert_eq!(updated.description, Some("Original description".to_string())); // unchanged
        assert_eq!(updated.default_provider_id, Some("new-provider".to_string()));

        // Verify persisted
        let found = store.find_by_uuid(&created.agent_uuid).expect("Failed to find").unwrap();
        assert_eq!(found.name, "Updated Name");
        assert_eq!(found.default_provider_id, Some("new-provider".to_string()));
    }

    #[test]
    fn test_update_nonexistent_agent() {
        let store = create_test_store();

        let result = store.update("non-existent-uuid", &AgentUpdate::default());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AgentStoreError::NotFound(_)));
    }

    #[test]
    fn test_delete_agent() {
        let store = create_test_store();

        let created = store.create(&NewAgent {
            name: "To Delete".to_string(),
            description: None,
            domain: None,
            mbti_type: None,
            system_prompt: None,
            default_provider_id: None,
        }).expect("Failed to create agent");

        // Delete the agent
        store.delete(&created.agent_uuid).expect("Failed to delete agent");

        // Verify it's gone
        let found = store.find_by_uuid(&created.agent_uuid).expect("Query failed");
        assert!(found.is_none());
    }

    #[test]
    fn test_delete_nonexistent_agent() {
        let store = create_test_store();

        let result = store.delete("non-existent-uuid");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AgentStoreError::NotFound(_)));
    }

    #[test]
    fn test_update_status() {
        let store = create_test_store();

        let created = store.create(&NewAgent {
            name: "Status Test".to_string(),
            description: None,
            domain: None,
            mbti_type: None,
            system_prompt: None,
            default_provider_id: None,
        }).expect("Failed to create agent");

        let updated = store.update_status(&created.agent_uuid, AgentStatus::Archived).expect("Failed to update status");
        assert_eq!(updated.status, AgentStatus::Archived);
    }

    #[test]
    fn test_find_by_id() {
        let store = create_test_store();

        let created = store.create(&NewAgent {
            name: "ID Test".to_string(),
            description: None,
            domain: None,
            mbti_type: None,
            system_prompt: None,
            default_provider_id: None,
        }).expect("Failed to create agent");

        let found = store.find_by_id(created.id).expect("Failed to find by id");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "ID Test");

        let not_found = store.find_by_id(99999).expect("Query failed");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_create_agent_validation_empty_name() {
        let store = create_test_store();

        let invalid_agent = NewAgent {
            name: "   ".to_string(), // Whitespace only
            description: None,
            domain: None,
            mbti_type: None,
            system_prompt: None,
            default_provider_id: None,
        };

        let result = store.create(&invalid_agent);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AgentStoreError::Validation(_)));
    }

    #[test]
    fn test_create_agent_validation_name_too_long() {
        let store = create_test_store();

        let invalid_agent = NewAgent {
            name: "x".repeat(101), // Exceeds 100 char limit
            description: None,
            domain: None,
            mbti_type: None,
            system_prompt: None,
            default_provider_id: None,
        };

        let result = store.create(&invalid_agent);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AgentStoreError::Validation(_)));
    }

    #[test]
    fn test_duplicate_agent() {
        let store = create_test_store();

        let new_agent = NewAgent {
            name: "Original Agent".to_string(),
            description: Some("Original description".to_string()),
            domain: Some("coding".to_string()),
            mbti_type: Some("INTJ".to_string()),
            system_prompt: Some("You are helpful.".to_string()),
            default_provider_id: None,
        };

        let created = store.create(&new_agent).expect("Failed to create agent");

        // Duplicate the agent
        let duplicated = store.duplicate(&created.agent_uuid).expect("Failed to duplicate agent");

        // Verify new UUID was generated
        assert_ne!(duplicated.agent_uuid, created.agent_uuid);
        assert!(!duplicated.agent_uuid.is_empty());

        // Verify name has "(副本)" suffix
        assert_eq!(duplicated.name, "Original Agent (副本)");

        // Verify all config fields were copied
        assert_eq!(duplicated.description, Some("Original description".to_string()));
        assert_eq!(duplicated.domain, Some("coding".to_string()));
        assert_eq!(duplicated.mbti_type, Some("INTJ".to_string()));
        assert_eq!(duplicated.system_prompt, Some("You are helpful.".to_string()));

        // Verify status is active
        assert_eq!(duplicated.status, AgentStatus::Active);

        // Verify new timestamps
        assert!(duplicated.created_at >= created.created_at);
        assert_eq!(duplicated.created_at, duplicated.updated_at);

        // Verify it was persisted to database
        let found = store.find_by_uuid(&duplicated.agent_uuid).expect("Failed to find duplicated");
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.name, "Original Agent (副本)");
    }

    #[test]
    fn test_duplicate_agent_always_active() {
        let store = create_test_store();

        let created = store.create(&NewAgent {
            name: "Inactive Agent".to_string(),
            description: None,
            domain: None,
            mbti_type: None,
            system_prompt: None,
            default_provider_id: None,
        }).expect("Failed to create agent");

        // Set original agent to inactive
        store.update_status(&created.agent_uuid, AgentStatus::Inactive).expect("Failed to update status");

        // Duplicate should still be active
        let duplicated = store.duplicate(&created.agent_uuid).expect("Failed to duplicate agent");
        assert_eq!(duplicated.status, AgentStatus::Active);
    }

    #[test]
    fn test_duplicate_agent_archived_becomes_active() {
        let store = create_test_store();

        let created = store.create(&NewAgent {
            name: "Archived Agent".to_string(),
            description: None,
            domain: None,
            mbti_type: None,
            system_prompt: None,
            default_provider_id: None,
        }).expect("Failed to create agent");

        // Archive the original
        store.update_status(&created.agent_uuid, AgentStatus::Archived).expect("Failed to update status");

        // Duplicate should be active
        let duplicated = store.duplicate(&created.agent_uuid).expect("Failed to duplicate agent");
        assert_eq!(duplicated.status, AgentStatus::Active);
    }

    #[test]
    fn test_duplicate_nonexistent_agent() {
        let store = create_test_store();

        let result = store.duplicate("non-existent-uuid");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AgentStoreError::NotFound(_)));
    }

    #[test]
    fn test_duplicate_agent_increases_count() {
        let store = create_test_store();

        store.create(&NewAgent {
            name: "Agent 1".to_string(),
            description: None,
            domain: None,
            mbti_type: None,
            system_prompt: None,
            default_provider_id: None,
        }).expect("Failed to create agent");

        let all = store.find_all().expect("Failed to find all");
        assert_eq!(all.len(), 1);

        // Duplicate should add a new agent
        store.duplicate(&all[0].agent_uuid).expect("Failed to duplicate");

        let all = store.find_all().expect("Failed to find all");
        assert_eq!(all.len(), 2);
    }
}