//! Episodic Memory Store
//!
//! L2 long-term memory storage for important conversations and events.
//! Persists to SQLite database with multi-dimensional querying.
//!
//! [Source: Story 5.2 - L2 情景记忆层实现]

use anyhow::{Context, Result};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::db::pool::{DbPool, DbConnection};

/// An episodic memory entry representing a significant conversation or event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicMemory {
    /// Unique identifier
    pub id: i64,
    /// Agent this memory belongs to
    pub agent_id: i64,
    /// Session this memory originated from (optional)
    pub session_id: Option<i64>,
    /// Memory content
    pub content: String,
    /// Importance score (1-10, higher is more important)
    pub importance: u8,
    /// Whether this memory is marked as important by user
    pub is_marked: bool,
    /// Additional metadata as JSON
    pub metadata: Option<String>,
    /// Unix timestamp of creation
    pub created_at: i64,
}

/// Data for creating a new episodic memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewEpisodicMemory {
    /// Agent this memory belongs to
    pub agent_id: i64,
    /// Session this memory originated from (optional)
    pub session_id: Option<i64>,
    /// Memory content
    pub content: String,
    /// Importance score (1-10)
    pub importance: u8,
    /// Whether this memory is marked as important by user
    #[serde(default)]
    pub is_marked: bool,
    /// Additional metadata as JSON (optional)
    pub metadata: Option<String>,
}

impl NewEpisodicMemory {
    /// Create a new episodic memory entry with validation
    pub fn new(
        agent_id: i64,
        session_id: Option<i64>,
        content: String,
        importance: u8,
        is_marked: bool,
        metadata: Option<String>,
    ) -> Result<Self> {
        // Validate importance range (1-10)
        if importance < 1 || importance > 10 {
            anyhow::bail!("Importance must be between 1 and 10, got {}", importance);
        }

        Ok(Self {
            agent_id,
            session_id,
            content,
            importance,
            is_marked,
            metadata,
        })
    }
}

/// Update data for an existing episodic memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicMemoryUpdate {
    /// New content (optional)
    pub content: Option<String>,
    /// New importance score (optional)
    pub importance: Option<u8>,
    /// New metadata (optional)
    pub metadata: Option<String>,
}

/// Statistics about episodic memories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicMemoryStats {
    /// Total number of memories
    pub total_count: usize,
    /// Average importance score
    pub avg_importance: f64,
    /// Memory count by agent
    pub by_agent: std::collections::HashMap<i64, usize>,
}

/// L2 Episodic Memory Store
///
/// Manages long-term memory storage using SQLite database.
/// Supports multi-dimensional queries, batch operations, and export/import.
pub struct EpisodicMemoryStore {
    db: Arc<DbPool>,
}

impl EpisodicMemoryStore {
    /// Create a new EpisodicMemoryStore
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { db }
    }

    /// Get a connection from the pool
    fn get_conn(&self) -> Result<DbConnection> {
        self.db.get().context("Failed to get database connection")
    }

    /// Helper function to map a row to EpisodicMemory
    /// [Source: Story 5.8 - 重要片段标记功能]
    fn row_to_memory(row: &rusqlite::Row) -> rusqlite::Result<EpisodicMemory> {
        Ok(EpisodicMemory {
            id: row.get(0)?,
            agent_id: row.get(1)?,
            session_id: row.get(2)?,
            content: row.get(3)?,
            importance: row.get(4)?,
            is_marked: row.get::<_, i64>(5)? != 0,
            metadata: row.get(6)?,
            created_at: row.get(7)?,
        })
    }

    /// Create a new episodic memory entry
    pub fn create(&self, entry: &NewEpisodicMemory) -> Result<i64> {
        // Validate importance range (1-10)
        if entry.importance < 1 || entry.importance > 10 {
            anyhow::bail!("Importance must be between 1 and 10, got {}", entry.importance);
        }

        let conn = self.get_conn()?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("Failed to get current timestamp")?
            .as_secs() as i64;

        conn.execute(
            "INSERT INTO episodic_memories (agent_id, session_id, content, importance, is_marked, metadata, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                entry.agent_id,
                entry.session_id,
                entry.content,
                entry.importance,
                entry.is_marked as i64,
                entry.metadata,
                now
            ],
        )
        .context("Failed to insert episodic memory")?;

        Ok(conn.last_insert_rowid())
    }

    /// Get an episodic memory by ID
    pub fn get(&self, id: i64) -> Result<Option<EpisodicMemory>> {
        let conn = self.get_conn()?;

        let mut stmt = conn
            .prepare(
                "SELECT id, agent_id, session_id, content, importance, is_marked, metadata, created_at
                 FROM episodic_memories WHERE id = ?1"
            )
            .context("Failed to prepare statement")?;

        let result = stmt
            .query_row(rusqlite::params![id], Self::row_to_memory)
            .optional()
            .context("Failed to query episodic memory")?;

        Ok(result)
    }

    /// Find memories by agent ID with pagination
    pub fn find_by_agent(&self, agent_id: i64, limit: usize, offset: usize) -> Result<Vec<EpisodicMemory>> {
        let conn = self.get_conn()?;

        let mut stmt = conn
            .prepare(
                "SELECT id, agent_id, session_id, content, importance, is_marked, metadata, created_at
                 FROM episodic_memories
                 WHERE agent_id = ?1
                 ORDER BY created_at DESC
                 LIMIT ?2 OFFSET ?3"
            )
            .context("Failed to prepare statement")?;

        let entries = stmt
            .query_map(rusqlite::params![agent_id, limit as i64, offset as i64], Self::row_to_memory)
            .context("Failed to query episodic memories")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("Failed to collect episodic memories")?;

        Ok(entries)
    }

    /// Find memories by session ID
    pub fn find_by_session(&self, session_id: i64) -> Result<Vec<EpisodicMemory>> {
        let conn = self.get_conn()?;

        let mut stmt = conn
            .prepare(
                "SELECT id, agent_id, session_id, content, importance, is_marked, metadata, created_at
                 FROM episodic_memories
                 WHERE session_id = ?1
                 ORDER BY created_at DESC"
            )
            .context("Failed to prepare statement")?;

        let entries = stmt
            .query_map(rusqlite::params![session_id], Self::row_to_memory)
            .context("Failed to query episodic memories")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("Failed to collect episodic memories")?;

        Ok(entries)
    }

    /// Find memories within a time range
    pub fn find_by_time_range(&self, start: i64, end: i64, agent_id: Option<i64>) -> Result<Vec<EpisodicMemory>> {
        let conn = self.get_conn()?;

        match agent_id {
            Some(aid) => {
                let mut stmt = conn
                    .prepare(
                        "SELECT id, agent_id, session_id, content, importance, is_marked, metadata, created_at
                         FROM episodic_memories
                         WHERE created_at >= ?1 AND created_at <= ?2 AND agent_id = ?3
                         ORDER BY created_at DESC"
                    )
                    .context("Failed to prepare statement")?;

                let iter = stmt.query_map(rusqlite::params![start, end, aid], Self::row_to_memory)
                    .context("Failed to query episodic memories")?;

                iter.collect::<std::result::Result<Vec<_>, _>>()
                    .context("Failed to collect episodic memories")
            }
            None => {
                let mut stmt = conn
                    .prepare(
                        "SELECT id, agent_id, session_id, content, importance, is_marked, metadata, created_at
                         FROM episodic_memories
                         WHERE created_at >= ?1 AND created_at <= ?2
                         ORDER BY created_at DESC"
                    )
                    .context("Failed to prepare statement")?;

                let iter = stmt.query_map(rusqlite::params![start, end], Self::row_to_memory)
                    .context("Failed to query episodic memories")?;

                iter.collect::<std::result::Result<Vec<_>, _>>()
                    .context("Failed to collect episodic memories")
            }
        }
    }

    /// Find memories with minimum importance
    pub fn find_by_importance(&self, min_importance: u8, limit: usize) -> Result<Vec<EpisodicMemory>> {
        let conn = self.get_conn()?;

        let mut stmt = conn
            .prepare(
                "SELECT id, agent_id, session_id, content, importance, is_marked, metadata, created_at
                 FROM episodic_memories
                 WHERE importance >= ?1
                 ORDER BY importance DESC, created_at DESC
                 LIMIT ?2"
            )
            .context("Failed to prepare statement")?;

        let entries = stmt
            .query_map(rusqlite::params![min_importance, limit as i64], Self::row_to_memory)
            .context("Failed to query episodic memories")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("Failed to collect episodic memories")?;

        Ok(entries)
    }

    /// Find marked memories
    /// [Source: Story 5.8 - 重要片段标记功能]
    pub fn find_marked(&self, agent_id: i64, limit: usize) -> Result<Vec<EpisodicMemory>> {
        let conn = self.get_conn()?;

        let mut stmt = conn
            .prepare(
                "SELECT id, agent_id, session_id, content, importance, is_marked, metadata, created_at
                 FROM episodic_memories
                 WHERE agent_id = ?1 AND is_marked = 1
                 ORDER BY created_at DESC
                 LIMIT ?2"
            )
            .context("Failed to prepare statement")?;

        let entries = stmt
            .query_map(rusqlite::params![agent_id, limit as i64], Self::row_to_memory)
            .context("Failed to query marked episodic memories")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("Failed to collect marked episodic memories")?;

        Ok(entries)
    }

    /// Set the marked status of a memory
    /// [Source: Story 5.8 - 重要片段标记功能]
    pub fn set_marked(&self, id: i64, is_marked: bool) -> Result<bool> {
        let conn = self.get_conn()?;

        let changes = conn
            .execute(
                "UPDATE episodic_memories SET is_marked = ?1 WHERE id = ?2",
                rusqlite::params![is_marked as i64, id],
            )
            .context("Failed to update memory marked status")?;

        Ok(changes > 0)
    }

    /// Batch insert multiple memories
    pub fn batch_insert(&self, entries: &[NewEpisodicMemory]) -> Result<usize> {
        let conn = self.get_conn()?;
        let tx = conn.unchecked_transaction().context("Failed to begin transaction")?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("Failed to get current timestamp")?
            .as_secs() as i64;

        let mut count = 0;
        for entry in entries {
            // Validate importance range (1-10)
            if entry.importance < 1 || entry.importance > 10 {
                tracing::warn!(
                    "Skipping entry with invalid importance: {} for agent {}",
                    entry.importance,
                    entry.agent_id
                );
                continue;
            }

            tx.execute(
                "INSERT INTO episodic_memories (agent_id, session_id, content, importance, is_marked, metadata, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    entry.agent_id,
                    entry.session_id,
                    entry.content,
                    entry.importance,
                    entry.is_marked as i64,
                    entry.metadata,
                    now
                ],
            )
            .context("Failed to insert episodic memory in batch")?;
            count += 1;
        }

        tx.commit().context("Failed to commit transaction")?;

        Ok(count)
    }

    /// Batch get multiple memories by IDs
    pub fn batch_get(&self, ids: &[i64]) -> Result<Vec<EpisodicMemory>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.get_conn()?;

        // Build placeholders for IN clause
        let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "SELECT id, agent_id, session_id, content, importance, is_marked, metadata, created_at
             FROM episodic_memories
             WHERE id IN ({})
             ORDER BY created_at DESC",
            placeholders.join(",")
        );

        let mut stmt = conn.prepare(&sql).context("Failed to prepare statement")?;

        let params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();

        let entries = stmt
            .query_map(params.as_slice(), Self::row_to_memory)
            .context("Failed to query episodic memories")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("Failed to collect episodic memories")?;

        Ok(entries)
    }

    /// Update an existing memory
    pub fn update(&self, id: i64, update: &EpisodicMemoryUpdate) -> Result<bool> {
        let conn = self.get_conn()?;

        // Build dynamic update query
        let mut sets = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref content) = update.content {
            sets.push("content = ?");
            params.push(Box::new(content.clone()));
        }
        if let Some(importance) = update.importance {
            sets.push("importance = ?");
            params.push(Box::new(importance as i64));
        }
        if let Some(ref metadata) = update.metadata {
            sets.push("metadata = ?");
            params.push(Box::new(metadata.clone()));
        }

        if sets.is_empty() {
            return Ok(false);
        }

        params.push(Box::new(id));

        let sql = format!(
            "UPDATE episodic_memories SET {} WHERE id = ?",
            sets.join(", ")
        );

        let changes = conn
            .execute(&sql, rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())))
            .context("Failed to update episodic memory")?;

        Ok(changes > 0)
    }

    /// Delete a memory by ID
    pub fn delete(&self, id: i64) -> Result<bool> {
        let conn = self.get_conn()?;

        let changes = conn
            .execute("DELETE FROM episodic_memories WHERE id = ?1", rusqlite::params![id])
            .context("Failed to delete episodic memory")?;

        Ok(changes > 0)
    }

    /// Count memories by agent
    pub fn count_by_agent(&self, agent_id: i64) -> Result<usize> {
        let conn = self.get_conn()?;

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM episodic_memories WHERE agent_id = ?1",
                rusqlite::params![agent_id],
                |row| row.get(0),
            )
            .context("Failed to count episodic memories")?;

        Ok(count as usize)
    }

    /// Count all memories
    pub fn count(&self) -> Result<usize> {
        let conn = self.get_conn()?;

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM episodic_memories", [], |row| row.get(0))
            .context("Failed to count episodic memories")?;

        Ok(count as usize)
    }

    /// Default limit for export operations to prevent memory issues
    const DEFAULT_EXPORT_LIMIT: usize = 10_000;

    /// Export memories for an agent to JSON
    ///
    /// Uses a reasonable limit (10,000 records) to prevent memory exhaustion.
    /// For larger exports, consider using pagination with `find_by_agent`.
    pub fn export_to_json(&self, agent_id: i64) -> Result<String> {
        let entries = self.find_by_agent(agent_id, Self::DEFAULT_EXPORT_LIMIT, 0)?;

        if entries.len() == Self::DEFAULT_EXPORT_LIMIT {
            tracing::warn!(
                "Export reached limit of {} records for agent {}. Consider using pagination.",
                Self::DEFAULT_EXPORT_LIMIT,
                agent_id
            );
        }

        serde_json::to_string(&entries).context("Failed to serialize episodic memories to JSON")
    }

    /// Export memories for an agent with custom limit
    pub fn export_to_json_with_limit(&self, agent_id: i64, limit: usize) -> Result<String> {
        let entries = self.find_by_agent(agent_id, limit, 0)?;
        serde_json::to_string(&entries).context("Failed to serialize episodic memories to JSON")
    }

    /// Import memories from JSON
    ///
    /// # Arguments
    /// * `json` - JSON string containing an array of EpisodicMemory objects
    /// * `skip_duplicates` - If true, skip entries that would create duplicates (same agent_id, session_id, content)
    ///
    /// # Returns
    /// The number of memories imported
    pub fn import_from_json(&self, json: &str, skip_duplicates: bool) -> Result<usize> {
        let entries: Vec<EpisodicMemory> =
            serde_json::from_str(json).context("Failed to deserialize episodic memories from JSON")?;

        if entries.is_empty() {
            return Ok(0);
        }

        if skip_duplicates {
            self.import_with_duplicate_handling(&entries)
        } else {
            let new_entries: Vec<NewEpisodicMemory> = entries
                .into_iter()
                .filter_map(|e| {
                    // Validate importance range
                    if e.importance < 1 || e.importance > 10 {
                        tracing::warn!("Skipping entry with invalid importance: {}", e.importance);
                        return None;
                    }
                    Some(NewEpisodicMemory {
                        agent_id: e.agent_id,
                        session_id: e.session_id,
                        content: e.content,
                        importance: e.importance,
                        is_marked: e.is_marked,
                        metadata: e.metadata,
                    })
                })
                .collect();

            self.batch_insert(&new_entries)
        }
    }

    /// Import with duplicate detection
    fn import_with_duplicate_handling(&self, entries: &[EpisodicMemory]) -> Result<usize> {
        let conn = self.get_conn()?;
        let tx = conn.unchecked_transaction().context("Failed to begin transaction")?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("Failed to get current timestamp")?
            .as_secs() as i64;

        let mut count = 0;
        for entry in entries {
            // Validate importance range
            if entry.importance < 1 || entry.importance > 10 {
                tracing::warn!("Skipping entry with invalid importance: {}", entry.importance);
                continue;
            }

            // Check for existing duplicate (same agent_id, session_id, content)
            let existing: Option<i64> = tx
                .query_row(
                    "SELECT id FROM episodic_memories
                     WHERE agent_id = ?1
                     AND (session_id = ?2 OR (session_id IS NULL AND ?2 IS NULL))
                     AND content = ?3",
                    rusqlite::params![entry.agent_id, entry.session_id, entry.content],
                    |row| row.get(0),
                )
                .optional()
                .context("Failed to check for duplicates")?;

            if existing.is_some() {
                tracing::debug!(
                    "Skipping duplicate memory for agent {}, session {:?}",
                    entry.agent_id,
                    entry.session_id
                );
                continue;
            }

            tx.execute(
                "INSERT INTO episodic_memories (agent_id, session_id, content, importance, metadata, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    entry.agent_id,
                    entry.session_id,
                    entry.content,
                    entry.importance,
                    entry.metadata,
                    now
                ],
            )
            .context("Failed to insert episodic memory during import")?;
            count += 1;
        }

        tx.commit().context("Failed to commit transaction")?;

        Ok(count)
    }

    /// Get memory statistics
    pub fn stats(&self) -> Result<EpisodicMemoryStats> {
        let conn = self.get_conn()?;

        let total_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM episodic_memories", [], |row| row.get(0))
            .context("Failed to count memories")?;

        let avg_importance: f64 = if total_count > 0 {
            conn.query_row("SELECT AVG(importance) FROM episodic_memories", [], |row| row.get(0))
                .context("Failed to calculate average importance")?
        } else {
            0.0
        };

        // Get counts by agent
        let mut stmt = conn
            .prepare("SELECT agent_id, COUNT(*) FROM episodic_memories GROUP BY agent_id")
            .context("Failed to prepare statement")?;

        let by_agent: std::collections::HashMap<i64, usize> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get::<_, i64>(1)? as usize)))
            .context("Failed to query agent counts")?
            .filter_map(|r| r.ok())
            .collect();

        Ok(EpisodicMemoryStats {
            total_count: total_count as usize,
            avg_importance,
            by_agent,
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::create_builtin_runner;
    use crate::db::pool::{create_pool, DbPoolConfig};
    use tempfile::tempdir;

    fn create_test_pool() -> Arc<DbPool> {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test_episodic.db");

        let pool = create_pool(&db_path, DbPoolConfig::default()).expect("Failed to create pool");

        // Run migrations
        let conn = pool.get().expect("Failed to get connection");
        create_builtin_runner().run(&conn).expect("Failed to run migrations");

        // Create test agents (needed for foreign key constraints)
        conn.execute(
            "INSERT OR IGNORE INTO agents (id, name, agent_uuid, status) VALUES (1, 'Test Agent 1', 'test-uuid-1', 'active')",
            [],
        ).expect("Failed to create test agent 1");
        conn.execute(
            "INSERT OR IGNORE INTO agents (id, name, agent_uuid, status) VALUES (2, 'Test Agent 2', 'test-uuid-2', 'active')",
            [],
        ).expect("Failed to create test agent 2");

        // Create test sessions (needed for foreign key constraints)
        conn.execute(
            "INSERT OR IGNORE INTO sessions (id, agent_id, title) VALUES (1, 1, 'Test Session 1')",
            [],
        ).expect("Failed to create test session 1");
        conn.execute(
            "INSERT OR IGNORE INTO sessions (id, agent_id, title) VALUES (2, 1, 'Test Session 2')",
            [],
        ).expect("Failed to create test session 2");
        conn.execute(
            "INSERT OR IGNORE INTO sessions (id, agent_id, title) VALUES (3, 1, 'Test Session 3')",
            [],
        ).expect("Failed to create test session 3");
        conn.execute(
            "INSERT OR IGNORE INTO sessions (id, agent_id, title) VALUES (4, 1, 'Test Session 4')",
            [],
        ).expect("Failed to create test session 4");
        conn.execute(
            "INSERT OR IGNORE INTO sessions (id, agent_id, title) VALUES (5, 1, 'Test Session 5')",
            [],
        ).expect("Failed to create test session 5");
        conn.execute(
            "INSERT OR IGNORE INTO sessions (id, agent_id, title) VALUES (42, 1, 'Test Session 42')",
            [],
        ).expect("Failed to create test session 42");
        conn.execute(
            "INSERT OR IGNORE INTO sessions (id, agent_id, title) VALUES (99, 1, 'Test Session 99')",
            [],
        ).expect("Failed to create test session 99");
        conn.execute(
            "INSERT OR IGNORE INTO sessions (id, agent_id, title) VALUES (100, 1, 'Test Session 100')",
            [],
        ).expect("Failed to create test session 100");

        // Keep temp dir alive by leaking it (test only)
        std::mem::forget(dir);

        Arc::new(pool)
    }

    fn create_test_store() -> EpisodicMemoryStore {
        let pool = create_test_pool();
        EpisodicMemoryStore::new(pool)
    }

    #[test]
    fn test_create_and_get() {
        let store = create_test_store();

        let new_memory = NewEpisodicMemory {
            agent_id: 1,
            session_id: Some(100),
            content: "Test memory content".to_string(),
            importance: 7,
            is_marked: false,
            metadata: Some(r#"{"key": "value"}"#.to_string()),
        };

        let id = store.create(&new_memory).expect("Failed to create memory");
        assert!(id > 0);

        let retrieved = store.get(id).expect("Failed to get memory");
        assert!(retrieved.is_some());

        let memory = retrieved.unwrap();
        assert_eq!(memory.agent_id, 1);
        assert_eq!(memory.session_id, Some(100));
        assert_eq!(memory.content, "Test memory content");
        assert_eq!(memory.importance, 7);
    }

    #[test]
    fn test_find_by_agent() {
        let store = create_test_store();

        // Create multiple memories for different agents
        for i in 1..=5 {
            let memory = NewEpisodicMemory {
                agent_id: 1,
                session_id: Some(i),
                content: format!("Memory {}", i),
                importance: 5,
                is_marked: false,
                metadata: None,
            };
            store.create(&memory).expect("Failed to create memory");
        }

        for i in 1..=3 {
            let memory = NewEpisodicMemory {
                agent_id: 2,
                session_id: Some(i),
                content: format!("Agent 2 Memory {}", i),
                importance: 5,
                is_marked: false,
                metadata: None,
            };
            store.create(&memory).expect("Failed to create memory");
        }

        let agent1_memories = store.find_by_agent(1, 10, 0).expect("Failed to find memories");
        assert_eq!(agent1_memories.len(), 5);

        let agent2_memories = store.find_by_agent(2, 10, 0).expect("Failed to find memories");
        assert_eq!(agent2_memories.len(), 3);
    }

    #[test]
    fn test_find_by_agent_pagination() {
        let store = create_test_store();

        // Create 10 memories for agent 1
        for i in 1..=10 {
            let memory = NewEpisodicMemory {
                agent_id: 1,
                session_id: None,
                content: format!("Memory {}", i),
                importance: 5,
                is_marked: false,
                metadata: None,
            };
            store.create(&memory).expect("Failed to create memory");
        }

        // Get first 5
        let page1 = store.find_by_agent(1, 5, 0).expect("Failed to find memories");
        assert_eq!(page1.len(), 5);

        // Get next 5
        let page2 = store.find_by_agent(1, 5, 5).expect("Failed to find memories");
        assert_eq!(page2.len(), 5);

        // Total should be 10
        let count = store.count_by_agent(1).expect("Failed to count");
        assert_eq!(count, 10);
    }

    #[test]
    fn test_find_by_session() {
        let store = create_test_store();

        // Create memories for specific session
        for i in 1..=3 {
            let memory = NewEpisodicMemory {
                agent_id: 1,
                session_id: Some(42),
                content: format!("Session memory {}", i),
                importance: 5,
                is_marked: false,
                metadata: None,
            };
            store.create(&memory).expect("Failed to create memory");
        }

        // Create memory for different session
        let memory = NewEpisodicMemory {
            agent_id: 1,
            session_id: Some(99),
            content: "Different session".to_string(),
            importance: 5,
            is_marked: false,
            metadata: None,
        };
        store.create(&memory).expect("Failed to create memory");

        let session_memories = store.find_by_session(42).expect("Failed to find memories");
        assert_eq!(session_memories.len(), 3);

        let other_session = store.find_by_session(99).expect("Failed to find memories");
        assert_eq!(other_session.len(), 1);
    }

    #[test]
    fn test_find_by_time_range() {
        let store = create_test_store();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Create a memory
        let memory = NewEpisodicMemory {
            agent_id: 1,
            session_id: None,
            content: "Time test memory".to_string(),
            importance: 5,
            is_marked: false,
            metadata: None,
        };
        store.create(&memory).expect("Failed to create memory");

        // Query within range (last hour to next hour)
        let memories = store
            .find_by_time_range(now - 3600, now + 3600, Some(1))
            .expect("Failed to find memories");
        assert_eq!(memories.len(), 1);

        // Query outside range (far future)
        let memories = store
            .find_by_time_range(now + 100000, now + 200000, None)
            .expect("Failed to find memories");
        assert_eq!(memories.len(), 0);
    }

    #[test]
    fn test_find_by_importance() {
        let store = create_test_store();

        // Create memories with different importance levels
        for i in 1..=5 {
            let memory = NewEpisodicMemory {
                agent_id: 1,
                session_id: None,
                content: format!("Importance {} memory", i * 2),
                importance: i * 2,
                is_marked: false,
                metadata: None,
            };
            store.create(&memory).expect("Failed to create memory");
        }

        // Find memories with importance >= 6
        let important = store.find_by_importance(6, 10).expect("Failed to find memories");
        assert_eq!(important.len(), 3); // importance 6, 8, and 10

        // Find memories with importance >= 2
        let all_important = store.find_by_importance(2, 10).expect("Failed to find memories");
        assert_eq!(all_important.len(), 5);
    }

    #[test]
    fn test_batch_insert() {
        let store = create_test_store();

        let entries = vec![
            NewEpisodicMemory {
                agent_id: 1,
                session_id: None,
                content: "Batch 1".to_string(),
                importance: 5,
                is_marked: false,
                metadata: None,
            },
            NewEpisodicMemory {
                agent_id: 1,
                session_id: None,
                content: "Batch 2".to_string(),
                importance: 5,
                is_marked: false,
                metadata: None,
            },
            NewEpisodicMemory {
                agent_id: 1,
                session_id: None,
                content: "Batch 3".to_string(),
                importance: 5,
                is_marked: false,
                metadata: None,
            },
        ];

        let count = store.batch_insert(&entries).expect("Failed to batch insert");
        assert_eq!(count, 3);

        let all = store.find_by_agent(1, 10, 0).expect("Failed to find memories");
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_batch_get() {
        let store = create_test_store();

        // Create memories and collect IDs
        let mut ids = Vec::new();
        for i in 1..=5 {
            let memory = NewEpisodicMemory {
                agent_id: 1,
                session_id: None,
                content: format!("Memory {}", i),
                importance: 5,
                is_marked: false,
                metadata: None,
            };
            let id = store.create(&memory).expect("Failed to create memory");
            ids.push(id);
        }

        // Get subset by IDs
        let subset_ids = vec![ids[0], ids[2], ids[4]];
        let memories = store.batch_get(&subset_ids).expect("Failed to batch get");
        assert_eq!(memories.len(), 3);
    }

    #[test]
    fn test_update() {
        let store = create_test_store();

        let memory = NewEpisodicMemory {
            agent_id: 1,
            session_id: None,
            content: "Original content".to_string(),
            importance: 5,
            is_marked: false,
            metadata: None,
        };
        let id = store.create(&memory).expect("Failed to create memory");

        // Update content
        let update = EpisodicMemoryUpdate {
            content: Some("Updated content".to_string()),
            importance: Some(9),
            metadata: None,
        };
        let updated = store.update(id, &update).expect("Failed to update memory");
        assert!(updated);

        let retrieved = store.get(id).expect("Failed to get memory").unwrap();
        assert_eq!(retrieved.content, "Updated content");
        assert_eq!(retrieved.importance, 9);
    }

    #[test]
    fn test_delete() {
        let store = create_test_store();

        let memory = NewEpisodicMemory {
            agent_id: 1,
            session_id: None,
            content: "To be deleted".to_string(),
            importance: 5,
            is_marked: false,
            metadata: None,
        };
        let id = store.create(&memory).expect("Failed to create memory");

        // Verify exists
        let retrieved = store.get(id).expect("Failed to get memory");
        assert!(retrieved.is_some());

        // Delete
        let deleted = store.delete(id).expect("Failed to delete memory");
        assert!(deleted);

        // Verify deleted
        let retrieved = store.get(id).expect("Failed to get memory");
        assert!(retrieved.is_none());

        // Delete non-existent
        let deleted = store.delete(999999).expect("Failed to delete memory");
        assert!(!deleted);
    }

    #[test]
    fn test_count() {
        let store = create_test_store();

        assert_eq!(store.count().expect("Failed to count"), 0);

        for i in 1..=3 {
            let memory = NewEpisodicMemory {
                agent_id: 1,
                session_id: None,
                content: format!("Memory {}", i),
                importance: 5,
                is_marked: false,
                metadata: None,
            };
            store.create(&memory).expect("Failed to create memory");
        }

        for i in 1..=2 {
            let memory = NewEpisodicMemory {
                agent_id: 2,
                session_id: None,
                content: format!("Agent 2 Memory {}", i),
                importance: 5,
                is_marked: false,
                metadata: None,
            };
            store.create(&memory).expect("Failed to create memory");
        }

        assert_eq!(store.count_by_agent(1).expect("Failed to count"), 3);
        assert_eq!(store.count_by_agent(2).expect("Failed to count"), 2);
        assert_eq!(store.count().expect("Failed to count"), 5);
    }

    #[test]
    fn test_export_import_json() {
        let store = create_test_store();

        // Create memories
        for i in 1..=3 {
            let memory = NewEpisodicMemory {
                agent_id: 1,
                session_id: Some(i),
                content: format!("Memory {}", i),
                importance: i as u8,
                is_marked: false,
                metadata: Some(format!(r#"{{"index": {}}}"#, i)),
            };
            store.create(&memory).expect("Failed to create memory");
        }

        // Export
        let json = store.export_to_json(1).expect("Failed to export");
        assert!(json.contains("Memory 1"));
        assert!(json.contains("Memory 2"));
        assert!(json.contains("Memory 3"));

        // Import to a different store (simulate different agent)
        let store2 = create_test_store();
        let count = store2.import_from_json(&json, false).expect("Failed to import");
        assert_eq!(count, 3);

        // Verify imported
        let imported = store2.find_by_agent(1, 10, 0).expect("Failed to find memories");
        assert_eq!(imported.len(), 3);
    }

    #[test]
    fn test_import_with_skip_duplicates() {
        let store = create_test_store();

        // Create initial memory
        let memory = NewEpisodicMemory {
            agent_id: 1,
            session_id: Some(1),
            content: "Duplicate test".to_string(),
            importance: 5,
            is_marked: false,
            metadata: None,
        };
        store.create(&memory).expect("Failed to create memory");

        // Export
        let json = store.export_to_json(1).expect("Failed to export");

        // Import with skip_duplicates=true should skip the existing entry
        let count = store.import_from_json(&json, true).expect("Failed to import");
        assert_eq!(count, 0, "Should skip duplicate");

        // Import with skip_duplicates=false should create duplicate
        let count = store.import_from_json(&json, false).expect("Failed to import");
        assert_eq!(count, 1, "Should create duplicate when skip_duplicates=false");
    }

    #[test]
    fn test_importance_validation() {
        let store = create_test_store();

        // Test invalid importance values
        let invalid_low = NewEpisodicMemory {
            agent_id: 1,
            session_id: None,
            content: "Invalid low".to_string(),
            importance: 0,
            is_marked: false,
            metadata: None,
        };
        let result = store.create(&invalid_low);
        assert!(result.is_err(), "Should reject importance < 1");

        let invalid_high = NewEpisodicMemory {
            agent_id: 1,
            session_id: None,
            content: "Invalid high".to_string(),
            importance: 11,
            is_marked: false,
            metadata: None,
        };
        let result = store.create(&invalid_high);
        assert!(result.is_err(), "Should reject importance > 10");

        // Valid importance should work
        let valid = NewEpisodicMemory {
            agent_id: 1,
            session_id: None,
            content: "Valid".to_string(),
            importance: 5,
            is_marked: false,
            metadata: None,
        };
        let result = store.create(&valid);
        assert!(result.is_ok(), "Should accept importance 1-10");
    }

    #[test]
    fn test_stats() {
        let store = create_test_store();

        // Create memories with varying importance
        for i in 1..=5 {
            let memory = NewEpisodicMemory {
                agent_id: 1,
                session_id: None,
                content: format!("Memory {}", i),
                importance: i as u8,
                is_marked: false,
                metadata: None,
            };
            store.create(&memory).expect("Failed to create memory");
        }

        for i in 1..=3 {
            let memory = NewEpisodicMemory {
                agent_id: 2,
                session_id: None,
                content: format!("Agent 2 Memory {}", i),
                importance: 7,
                is_marked: false,
                metadata: None,
            };
            store.create(&memory).expect("Failed to create memory");
        }

        let stats = store.stats().expect("Failed to get stats");
        assert_eq!(stats.total_count, 8);
        assert!(stats.avg_importance > 0.0);
        assert_eq!(stats.by_agent.get(&1), Some(&5));
        assert_eq!(stats.by_agent.get(&2), Some(&3));
    }
}