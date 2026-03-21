//! Session and message storage layer for database operations
//!
//! This module provides CRUD operations for Session and Message records
//! using the SQLite connection pool.

use crate::db::migrations::create_builtin_runner;
use crate::db::{DbConnection, DbPool};
use crate::session::model::{Message, MessageRole, NewMessage, NewSession, Session, SessionUpdate};
use anyhow::{Context, Result};
use rusqlite::params;

/// Error type for session and message operations
#[derive(Debug, thiserror::Error)]
pub enum SessionStoreError {
    #[error("Session not found: {0}")]
    NotFound(i64),

    #[error("Message not found: {0}")]
    MessageNotFound(i64),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Pool error: {0}")]
    Pool(String),

    #[error("Validation error: {0}")]
    Validation(String),
}

/// Session storage handler
#[derive(Clone)]
pub struct SessionStore {
    pool: DbPool,
}

impl SessionStore {
    /// Create a new SessionStore with the given connection pool
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Get a connection from the pool
    fn get_conn(&self) -> Result<DbConnection, SessionStoreError> {
        self.pool.get().map_err(|e| SessionStoreError::Pool(e.to_string()))
    }

    /// Initialize the database with migrations
    pub fn initialize(&self) -> Result<()> {
        let conn = self.get_conn()?;
        let runner = create_builtin_runner();
        runner.run(&conn).context("Failed to run migrations")?;
        Ok(())
    }

    /// Create a new session
    pub fn create(&self, new_session: &NewSession) -> Result<Session, SessionStoreError> {
        let conn = self.get_conn()?;
        let timestamp = NewSession::current_timestamp();

        conn.execute(
            "INSERT INTO sessions (agent_id, title, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                new_session.agent_id,
                new_session.title,
                timestamp,
                timestamp,
            ],
        )?;

        let id = conn.last_insert_rowid();
        Ok(Session {
            id,
            agent_id: new_session.agent_id,
            title: new_session.title.clone(),
            created_at: timestamp,
            updated_at: timestamp,
        })
    }

    /// Find a session by database ID
    pub fn find_by_id(&self, id: i64) -> Result<Option<Session>, SessionStoreError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, title, created_at, updated_at
             FROM sessions WHERE id = ?1"
        )?;

        let result = stmt.query_row(params![id], |row| {
            Ok(Session {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                title: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        });

        match result {
            Ok(session) => Ok(Some(session)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Find all sessions for a specific agent
    pub fn find_by_agent(&self, agent_id: i64) -> Result<Vec<Session>, SessionStoreError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, title, created_at, updated_at
             FROM sessions WHERE agent_id = ?1
             ORDER BY updated_at DESC"
        )?;

        let sessions = stmt.query_map(params![agent_id], |row| {
            Ok(Session {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                title: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;

        let mut result = Vec::new();
        for session in sessions {
            result.push(session?);
        }
        Ok(result)
    }

    /// Find all sessions
    pub fn find_all(&self) -> Result<Vec<Session>, SessionStoreError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, title, created_at, updated_at
             FROM sessions ORDER BY updated_at DESC"
        )?;

        let sessions = stmt.query_map([], |row| {
            Ok(Session {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                title: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;

        let mut result = Vec::new();
        for session in sessions {
            result.push(session?);
        }
        Ok(result)
    }

    /// Update a session by ID
    pub fn update(&self, id: i64, updates: &SessionUpdate) -> Result<Session, SessionStoreError> {
        // First check if session exists
        let existing = self.find_by_id(id)?
            .ok_or(SessionStoreError::NotFound(id))?;

        let conn = self.get_conn()?;
        let timestamp = NewSession::current_timestamp();

        let new_title = updates.title.as_ref().or(existing.title.as_ref());

        conn.execute(
            "UPDATE sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![new_title, timestamp, id],
        )?;

        Ok(Session {
            id: existing.id,
            agent_id: existing.agent_id,
            title: new_title.cloned(),
            created_at: existing.created_at,
            updated_at: timestamp,
        })
    }

    /// Delete a session by ID (cascade deletes messages)
    pub fn delete(&self, id: i64) -> Result<(), SessionStoreError> {
        let conn = self.get_conn()?;
        let rows_affected = conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;

        if rows_affected == 0 {
            return Err(SessionStoreError::NotFound(id));
        }

        Ok(())
    }
}

/// Message storage handler
#[derive(Clone)]
pub struct MessageStore {
    pool: DbPool,
}

impl MessageStore {
    /// Create a new MessageStore with the given connection pool
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Get a connection from the pool
    fn get_conn(&self) -> Result<DbConnection, SessionStoreError> {
        self.pool.get().map_err(|e| SessionStoreError::Pool(e.to_string()))
    }

    /// Create a new message
    pub fn create(&self, new_message: &NewMessage) -> Result<Message, SessionStoreError> {
        // Validate the message data
        new_message.validate().map_err(|e| SessionStoreError::Validation(e.to_string()))?;

        let conn = self.get_conn()?;
        let timestamp = NewMessage::current_timestamp();
        let role_str = new_message.role.to_string();

        conn.execute(
            "INSERT INTO messages (session_id, role, content, created_at, quote_message_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                new_message.session_id,
                role_str,
                new_message.content,
                timestamp,
                new_message.quote_message_id,
            ],
        )?;

        // Update session's updated_at timestamp
        conn.execute(
            "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
            params![timestamp, new_message.session_id],
        )?;

        let id = conn.last_insert_rowid();
        Ok(Message {
            id,
            session_id: new_message.session_id,
            role: new_message.role,
            content: new_message.content.clone(),
            created_at: timestamp,
            quote_message_id: new_message.quote_message_id,
            is_marked: false,
        })
    }

    /// Find a message by database ID
    pub fn find_by_id(&self, id: i64) -> Result<Option<Message>, SessionStoreError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, created_at, quote_message_id, is_marked
             FROM messages WHERE id = ?1"
        )?;

        let result = stmt.query_row(params![id], |row| {
            let role_str: String = row.get(2)?;
            let role = MessageRole::from_db_string(&role_str, 2)?;
            Ok(Message {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role,
                content: row.get(3)?,
                created_at: row.get(4)?,
                quote_message_id: row.get(5)?,
                is_marked: row.get::<_, i64>(6)? != 0,
            })
        });

        match result {
            Ok(message) => Ok(Some(message)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Find all messages for a specific session, ordered by creation time
    pub fn find_by_session(&self, session_id: i64) -> Result<Vec<Message>, SessionStoreError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, created_at, quote_message_id, is_marked
             FROM messages WHERE session_id = ?1
             ORDER BY created_at ASC"
        )?;

        let messages = stmt.query_map(params![session_id], |row| {
            let role_str: String = row.get(2)?;
            let role = MessageRole::from_db_string(&role_str, 2)?;
            Ok(Message {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role,
                content: row.get(3)?,
                created_at: row.get(4)?,
                quote_message_id: row.get(5)?,
                is_marked: row.get::<_, i64>(6)? != 0,
            })
        })?;

        let mut result = Vec::new();
        for message in messages {
            result.push(message?);
        }
        Ok(result)
    }

    /// Get the latest N messages for a session
    pub fn find_latest_by_session(&self, session_id: i64, limit: usize) -> Result<Vec<Message>, SessionStoreError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, created_at, quote_message_id, is_marked
             FROM messages WHERE session_id = ?1
             ORDER BY created_at DESC LIMIT ?2"
        )?;

        let messages = stmt.query_map(params![session_id, limit as i64], |row| {
            let role_str: String = row.get(2)?;
            let role = MessageRole::from_db_string(&role_str, 2)?;
            Ok(Message {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role,
                content: row.get(3)?,
                created_at: row.get(4)?,
                quote_message_id: row.get(5)?,
                is_marked: row.get::<_, i64>(6)? != 0,
            })
        })?;

        let mut result = Vec::new();
        for message in messages {
            result.push(message?);
        }
        // Reverse to maintain chronological order
        result.reverse();
        Ok(result)
    }

    /// Delete all messages for a session
    pub fn delete_by_session(&self, session_id: i64) -> Result<(), SessionStoreError> {
        let conn = self.get_conn()?;
        conn.execute("DELETE FROM messages WHERE session_id = ?1", params![session_id])?;
        Ok(())
    }

    /// Delete a message by ID
    pub fn delete(&self, id: i64) -> Result<(), SessionStoreError> {
        let conn = self.get_conn()?;
        let rows_affected = conn.execute("DELETE FROM messages WHERE id = ?1", params![id])?;

        if rows_affected == 0 {
            return Err(SessionStoreError::MessageNotFound(id));
        }

        Ok(())
    }

    /// Count messages in a session
    pub fn count_by_session(&self, session_id: i64) -> Result<i64, SessionStoreError> {
        let conn = self.get_conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Set the marked status of a message
    ///
    /// Marked messages receive higher importance scores
    /// when stored to episodic memory (L2).
    ///
    /// [Source: Story 5.8 - 重要片段标记功能]
    pub fn set_marked(&self, id: i64, is_marked: bool) -> Result<bool, SessionStoreError> {
        let conn = self.get_conn()?;
        let rows_affected = conn.execute(
            "UPDATE messages SET is_marked = ?1 WHERE id = ?2",
            params![is_marked as i64, id],
        )?;

        Ok(rows_affected > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_pool, DbPoolConfig};
    use tempfile::{tempdir, TempDir};

    fn setup_test_db() -> (DbPool, i64, TempDir) {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let pool = create_pool(&db_path, DbPoolConfig::default()).expect("Failed to create pool");

        let conn = pool.get().expect("Failed to get connection");
        let runner = create_builtin_runner();
        runner.run(&conn).expect("Failed to run migrations");

        // Create a test agent first
        conn.execute(
            "INSERT INTO agents (agent_uuid, name, status) VALUES ('test-agent-uuid', 'Test Agent', 'active')",
            [],
        )
        .expect("Failed to create test agent");
        let agent_id = conn.last_insert_rowid();

        (pool, agent_id, dir)
    }

    #[test]
    fn test_create_session() {
        let (pool, agent_id, _dir) = setup_test_db();
        let store = SessionStore::new(pool);

        let new_session = NewSession {
            agent_id,
            title: Some("Test Session".to_string()),
        };

        let created = store.create(&new_session).expect("Failed to create session");

        assert!(created.id > 0);
        assert_eq!(created.agent_id, agent_id);
        assert_eq!(created.title, Some("Test Session".to_string()));
    }

    #[test]
    fn test_create_session_without_title() {
        let (pool, agent_id, _dir) = setup_test_db();
        let store = SessionStore::new(pool);

        let new_session = NewSession {
            agent_id,
            title: None,
        };

        let created = store.create(&new_session).expect("Failed to create session");

        assert!(created.id > 0);
        assert_eq!(created.agent_id, agent_id);
        assert!(created.title.is_none());
    }

    #[test]
    fn test_find_session_by_id() {
        let (pool, agent_id, _dir) = setup_test_db();
        let store = SessionStore::new(pool);

        let new_session = NewSession {
            agent_id,
            title: Some("Findable Session".to_string()),
        };

        let created = store.create(&new_session).expect("Failed to create session");

        // Find the session
        let found = store.find_by_id(created.id).expect("Failed to find session");
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.title, Some("Findable Session".to_string()));

        // Try to find non-existent session
        let not_found = store.find_by_id(99999).expect("Query failed");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_find_sessions_by_agent() {
        let (pool, agent_id, _dir) = setup_test_db();
        let store = SessionStore::new(pool);

        // Create multiple sessions for the same agent
        for i in 0..3 {
            store.create(&NewSession {
                agent_id,
                title: Some(format!("Session {}", i)),
            }).expect("Failed to create session");
        }

        let sessions = store.find_by_agent(agent_id).expect("Failed to find sessions");
        assert_eq!(sessions.len(), 3);

        // Verify all sessions are present (order may vary if timestamps are equal)
        let titles: Vec<_> = sessions.iter().filter_map(|s| s.title.as_ref()).collect();
        assert!(titles.contains(&&"Session 0".to_string()));
        assert!(titles.contains(&&"Session 1".to_string()));
        assert!(titles.contains(&&"Session 2".to_string()));
    }

    #[test]
    fn test_find_all_sessions() {
        let (pool, agent_id, _dir) = setup_test_db();
        let store = SessionStore::new(pool);

        // Create multiple sessions
        for i in 0..3 {
            store.create(&NewSession {
                agent_id,
                title: Some(format!("Session {}", i)),
            }).expect("Failed to create session");
        }

        let all = store.find_all().expect("Failed to find all");
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_update_session() {
        let (pool, agent_id, _dir) = setup_test_db();
        let store = SessionStore::new(pool);

        let created = store.create(&NewSession {
            agent_id,
            title: Some("Original Title".to_string()),
        }).expect("Failed to create session");

        let updates = SessionUpdate {
            title: Some("Updated Title".to_string()),
        };

        let updated = store.update(created.id, &updates).expect("Failed to update session");
        assert_eq!(updated.title, Some("Updated Title".to_string()));
        assert!(updated.updated_at >= created.updated_at);

        // Verify persisted
        let found = store.find_by_id(created.id).expect("Failed to find").unwrap();
        assert_eq!(found.title, Some("Updated Title".to_string()));
    }

    #[test]
    fn test_update_nonexistent_session() {
        let (pool, _, _dir) = setup_test_db();
        let store = SessionStore::new(pool);

        let result = store.update(99999, &SessionUpdate::default());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionStoreError::NotFound(_)));
    }

    #[test]
    fn test_delete_session() {
        let (pool, agent_id, _dir) = setup_test_db();
        let store = SessionStore::new(pool);

        let created = store.create(&NewSession {
            agent_id,
            title: Some("To Delete".to_string()),
        }).expect("Failed to create session");

        // Delete the session
        store.delete(created.id).expect("Failed to delete session");

        // Verify it's gone
        let found = store.find_by_id(created.id).expect("Query failed");
        assert!(found.is_none());
    }

    #[test]
    fn test_delete_nonexistent_session() {
        let (pool, _, _dir) = setup_test_db();
        let store = SessionStore::new(pool);

        let result = store.delete(99999);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionStoreError::NotFound(_)));
    }

    #[test]
    fn test_create_message() {
        let (pool, agent_id, _dir) = setup_test_db();
        let session_store = SessionStore::new(pool.clone());
        let message_store = MessageStore::new(pool);

        // Create a session first
        let session = session_store.create(&NewSession {
            agent_id,
            title: Some("Test Session".to_string()),
        }).expect("Failed to create session");

        let new_message = NewMessage {
            session_id: session.id,
            role: MessageRole::User,
            content: "Hello, world!".to_string(),
            quote_message_id: None,
        };

        let created = message_store.create(&new_message).expect("Failed to create message");

        assert!(created.id > 0);
        assert_eq!(created.session_id, session.id);
        assert_eq!(created.role, MessageRole::User);
        assert_eq!(created.content, "Hello, world!");
    }

    #[test]
    fn test_create_message_updates_session_timestamp() {
        let (pool, agent_id, _dir) = setup_test_db();
        let session_store = SessionStore::new(pool.clone());
        let message_store = MessageStore::new(pool);

        let session = session_store.create(&NewSession {
            agent_id,
            title: Some("Test Session".to_string()),
        }).expect("Failed to create session");

        let original_updated_at = session.updated_at;

        // Sleep to ensure timestamp difference (SQLite uses 1-second resolution)
        std::thread::sleep(std::time::Duration::from_secs(2));

        message_store.create(&NewMessage {
            session_id: session.id,
            role: MessageRole::User,
            content: "Hello".to_string(),
            quote_message_id: None,
        }).expect("Failed to create message");

        // Check that session's updated_at was updated
        let updated_session = session_store.find_by_id(session.id)
            .expect("Failed to find session")
            .expect("Session not found");

        assert!(updated_session.updated_at > original_updated_at);
    }

    #[test]
    fn test_find_message_by_id() {
        let (pool, agent_id, _dir) = setup_test_db();
        let session_store = SessionStore::new(pool.clone());
        let message_store = MessageStore::new(pool);

        let session = session_store.create(&NewSession {
            agent_id,
            title: None,
        }).expect("Failed to create session");

        let created = message_store.create(&NewMessage {
            session_id: session.id,
            role: MessageRole::Assistant,
            content: "Response".to_string(),
            quote_message_id: None,
        }).expect("Failed to create message");

        let found = message_store.find_by_id(created.id).expect("Failed to find message");
        assert!(found.is_some());
        assert_eq!(found.unwrap().content, "Response");

        let not_found = message_store.find_by_id(99999).expect("Query failed");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_find_messages_by_session() {
        let (pool, agent_id, _dir) = setup_test_db();
        let session_store = SessionStore::new(pool.clone());
        let message_store = MessageStore::new(pool);

        let session = session_store.create(&NewSession {
            agent_id,
            title: None,
        }).expect("Failed to create session");

        // Create multiple messages
        message_store.create(&NewMessage {
            session_id: session.id,
            role: MessageRole::User,
            content: "First".to_string(),
            quote_message_id: None,
        }).expect("Failed to create message");

        std::thread::sleep(std::time::Duration::from_millis(10));

        message_store.create(&NewMessage {
            session_id: session.id,
            role: MessageRole::Assistant,
            content: "Second".to_string(),
            quote_message_id: None,
        }).expect("Failed to create message");

        let messages = message_store.find_by_session(session.id).expect("Failed to find messages");
        assert_eq!(messages.len(), 2);

        // Verify ordering (by created_at ASC)
        assert_eq!(messages[0].content, "First");
        assert_eq!(messages[1].content, "Second");
    }

    #[test]
    fn test_find_latest_messages() {
        let (pool, agent_id, _dir) = setup_test_db();
        let session_store = SessionStore::new(pool.clone());
        let message_store = MessageStore::new(pool);

        let session = session_store.create(&NewSession {
            agent_id,
            title: None,
        }).expect("Failed to create session");

        // Create multiple messages with enough delay to ensure different timestamps
        // SQLite uses Unix timestamps (1 second resolution)
        for i in 0..5 {
            std::thread::sleep(std::time::Duration::from_millis(1100));
            message_store.create(&NewMessage {
                session_id: session.id,
                role: MessageRole::User,
                content: format!("Message {}", i),
                quote_message_id: None,
            }).expect("Failed to create message");
        }

        // Get latest 3
        let latest = message_store.find_latest_by_session(session.id, 3).expect("Failed to find latest");
        assert_eq!(latest.len(), 3);

        // Should be ordered chronologically (oldest first in result after reversal)
        // The last 3 messages are 2, 3, 4 (indices)
        assert_eq!(latest[0].content, "Message 2");
        assert_eq!(latest[1].content, "Message 3");
        assert_eq!(latest[2].content, "Message 4");
    }

    #[test]
    fn test_delete_messages_by_session() {
        let (pool, agent_id, _dir) = setup_test_db();
        let session_store = SessionStore::new(pool.clone());
        let message_store = MessageStore::new(pool);

        let session = session_store.create(&NewSession {
            agent_id,
            title: None,
        }).expect("Failed to create session");

        // Create messages
        for i in 0..3 {
            message_store.create(&NewMessage {
                session_id: session.id,
                role: MessageRole::User,
                content: format!("Message {}", i),
                quote_message_id: None,
            }).expect("Failed to create message");
        }

        // Delete all messages
        message_store.delete_by_session(session.id).expect("Failed to delete messages");

        // Verify all messages are gone
        let messages = message_store.find_by_session(session.id).expect("Failed to find messages");
        assert!(messages.is_empty());
    }

    #[test]
    fn test_delete_message() {
        let (pool, agent_id, _dir) = setup_test_db();
        let session_store = SessionStore::new(pool.clone());
        let message_store = MessageStore::new(pool);

        let session = session_store.create(&NewSession {
            agent_id,
            title: None,
        }).expect("Failed to create session");

        let created = message_store.create(&NewMessage {
            session_id: session.id,
            role: MessageRole::User,
            content: "To Delete".to_string(),
            quote_message_id: None,
        }).expect("Failed to create message");

        // Delete the message
        message_store.delete(created.id).expect("Failed to delete message");

        // Verify it's gone
        let found = message_store.find_by_id(created.id).expect("Query failed");
        assert!(found.is_none());
    }

    #[test]
    fn test_delete_nonexistent_message() {
        let (pool, _, _dir) = setup_test_db();
        let message_store = MessageStore::new(pool);

        let result = message_store.delete(99999);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionStoreError::MessageNotFound(_)));
    }

    #[test]
    fn test_count_messages_by_session() {
        let (pool, agent_id, _dir) = setup_test_db();
        let session_store = SessionStore::new(pool.clone());
        let message_store = MessageStore::new(pool);

        let session = session_store.create(&NewSession {
            agent_id,
            title: None,
        }).expect("Failed to create session");

        // Create messages
        for i in 0..5 {
            message_store.create(&NewMessage {
                session_id: session.id,
                role: MessageRole::User,
                content: format!("Message {}", i),
                quote_message_id: None,
            }).expect("Failed to create message");
        }

        let count = message_store.count_by_session(session.id).expect("Failed to count");
        assert_eq!(count, 5);
    }

    #[test]
    fn test_message_validation_empty_content() {
        let (pool, agent_id, _dir) = setup_test_db();
        let session_store = SessionStore::new(pool.clone());
        let message_store = MessageStore::new(pool);

        let session = session_store.create(&NewSession {
            agent_id,
            title: None,
        }).expect("Failed to create session");

        let result = message_store.create(&NewMessage {
            session_id: session.id,
            role: MessageRole::User,
            content: "   ".to_string(), // Whitespace only
            quote_message_id: None,
        });

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionStoreError::Validation(_)));
    }

    #[test]
    fn test_session_cascade_delete_messages() {
        let (pool, agent_id, _dir) = setup_test_db();
        let session_store = SessionStore::new(pool.clone());
        let message_store = MessageStore::new(pool);

        let session = session_store.create(&NewSession {
            agent_id,
            title: None,
        }).expect("Failed to create session");

        // Create messages
        for i in 0..3 {
            message_store.create(&NewMessage {
                session_id: session.id,
                role: MessageRole::User,
                content: format!("Message {}", i),
                quote_message_id: None,
            }).expect("Failed to create message");
        }

        // Delete the session (should cascade delete messages)
        session_store.delete(session.id).expect("Failed to delete session");

        // Verify messages are also deleted (foreign key cascade)
        let messages = message_store.find_by_session(session.id).expect("Failed to find messages");
        assert!(messages.is_empty());
    }

    #[test]
    fn test_create_message_with_quote() {
        let (pool, agent_id, _dir) = setup_test_db();
        let session_store = SessionStore::new(pool.clone());
        let message_store = MessageStore::new(pool);

        let session = session_store.create(&NewSession {
            agent_id,
            title: Some("Quote Test Session".to_string()),
        }).expect("Failed to create session");

        // Create the original message
        let original = message_store.create(&NewMessage {
            session_id: session.id,
            role: MessageRole::User,
            content: "Original message to be quoted".to_string(),
            quote_message_id: None,
        }).expect("Failed to create original message");

        // Create a reply message that quotes the original
        let reply = message_store.create(&NewMessage {
            session_id: session.id,
            role: MessageRole::Assistant,
            content: "This is a reply to the quoted message".to_string(),
            quote_message_id: Some(original.id),
        }).expect("Failed to create reply message");

        assert!(reply.id > 0);
        assert_eq!(reply.quote_message_id, Some(original.id));

        // Verify we can retrieve it with the quote reference
        let found = message_store.find_by_id(reply.id)
            .expect("Failed to find message")
            .expect("Message not found");
        assert_eq!(found.quote_message_id, Some(original.id));
    }

    #[test]
    fn test_find_messages_by_session_includes_quote_id() {
        let (pool, agent_id, _dir) = setup_test_db();
        let session_store = SessionStore::new(pool.clone());
        let message_store = MessageStore::new(pool);

        let session = session_store.create(&NewSession {
            agent_id,
            title: None,
        }).expect("Failed to create session");

        // Create messages
        let msg1 = message_store.create(&NewMessage {
            session_id: session.id,
            role: MessageRole::User,
            content: "First".to_string(),
            quote_message_id: None,
        }).expect("Failed to create message");

        let msg2 = message_store.create(&NewMessage {
            session_id: session.id,
            role: MessageRole::Assistant,
            content: "Second (replying to first)".to_string(),
            quote_message_id: Some(msg1.id),
        }).expect("Failed to create message");

        let messages = message_store.find_by_session(session.id).expect("Failed to find messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].quote_message_id, None);
        assert_eq!(messages[1].quote_message_id, Some(msg1.id));
    }
}