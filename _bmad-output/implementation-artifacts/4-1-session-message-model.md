# Story 4.1: 会话与消息数据模型

Status: done

## Story

As a **用户**,
I want **我的对话被保存为会话**,
so that **我可以回顾和继续之前的对话**.

## Acceptance Criteria

1. **AC1: Sessions Table** - sessions 表已创建，包含字段：id, agent_id, title, created_at, updated_at
2. **AC2: Messages Table** - messages 表已创建，包含字段：id, session_id, role, content, created_at
3. **AC3: Rust Structs** - Session 和 Message 结构体已在 Rust 中定义
4. **AC4: CRUD Operations** - CRUD 操作已实现（create_session, get_session, list_sessions, delete_session）
5. **AC5: Message Ordering** - 消息按创建时间排序检索
6. **AC6: TypeScript Types** - 前端 TypeScript 类型已定义
7. **AC7: Tauri Commands** - 会话管理 Tauri 命令已实现

## Tasks / Subtasks

- [x] Task 1: Session & Message Rust Models (AC: #3)
  - [x] 1.1 Create `crates/omninova-core/src/session/mod.rs` with module exports
  - [x] 1.2 Create `crates/omninova-core/src/session/model.rs` with Session, NewSession, SessionUpdate structs
  - [x] 1.3 Create Message, NewMessage structs in model.rs
  - [x] 1.4 Add MessageRole enum (user, assistant, system)
  - [x] 1.5 Add Serde serialization with `#[serde(rename_all = "camelCase")]`
  - [x] 1.6 Add helper methods (generate_uuid, current_timestamp)
  - [x] 1.7 Add validation methods for Session and Message

- [x] Task 2: Session Store Implementation (AC: #4, #5)
  - [x] 2.1 Create `crates/omninova-core/src/session/store.rs` with SessionStore struct
  - [x] 2.2 Implement `create_session(&self, agent_id: i64, title: Option<&str>) -> Result<Session>`
  - [x] 2.3 Implement `find_by_id(&self, id: i64) -> Result<Option<Session>>`
  - [x] 2.4 Implement `find_by_uuid(&self, uuid: &str) -> Result<Option<Session>>` (not needed - schema has no UUID)
  - [x] 2.5 Implement `list_by_agent(&self, agent_id: i64) -> Result<Vec<Session>>`
  - [x] 2.6 Implement `update_session(&self, id: i64, update: SessionUpdate) -> Result<Session>`
  - [x] 2.7 Implement `delete_session(&self, id: i64) -> Result<()>`
  - [x] 2.8 Add error handling with thiserror

- [x] Task 3: Message Store Implementation (AC: #4, #5)
  - [x] 3.1 Add MessageStore struct to `store.rs`
  - [x] 3.2 Implement `create_message(&self, session_id: i64, role: MessageRole, content: &str) -> Result<Message>`
  - [x] 3.3 Implement `list_by_session(&self, session_id: i64) -> Result<Vec<Message>>` with ORDER BY created_at ASC
  - [x] 3.4 Implement `delete_by_session(&self, session_id: i64) -> Result<()>`
  - [x] 3.5 Implement `get_latest_messages(&self, session_id: i64, limit: usize) -> Result<Vec<Message>>`

- [x] Task 4: Update lib.rs and Module Exports (AC: #3)
  - [x] 4.1 Add `pub mod session;` to `crates/omninova-core/src/lib.rs`
  - [x] 4.2 Re-export Session, Message, SessionStore, MessageStore from lib.rs

- [x] Task 5: TypeScript Type Definitions (AC: #6)
  - [x] 5.1 Create `apps/omninova-tauri/src/types/session.ts`
  - [x] 5.2 Define Session, NewSession, SessionUpdate interfaces
  - [x] 5.3 Define Message, NewMessage interfaces
  - [x] 5.4 Define MessageRole type
  - [x] 5.5 Add JSDoc comments for documentation

- [x] Task 6: Tauri Commands for Session Management (AC: #7)
  - [x] 6.1 Add `create_session` Tauri command in `lib.rs`
  - [x] 6.2 Add `get_session` Tauri command
  - [x] 6.3 Add `list_sessions_by_agent` Tauri command
  - [x] 6.4 Add `update_session` Tauri command
  - [x] 6.5 Add `delete_session` Tauri command
  - [x] 6.6 Add `create_message` Tauri command
  - [x] 6.7 Add `list_messages_by_session` Tauri command
  - [x] 6.8 Add proper error handling with Chinese error messages

- [x] Task 7: Unit Tests (All ACs)
  - [x] 7.1 Add Rust unit tests for Session model
  - [x] 7.2 Add Rust unit tests for Message model
  - [x] 7.3 Add Rust unit tests for SessionStore CRUD
  - [x] 7.4 Add Rust unit tests for MessageStore CRUD
  - [x] 7.5 Add Rust unit tests for message ordering
  - [x] 7.6 Follow existing test patterns with tempfile for test databases

## Dev Notes

### Database Schema - ALREADY EXISTS

**IMPORTANT:** The sessions and messages tables already exist in the initial schema migration (`migrations.rs` - migration 001_initial). DO NOT create new migrations for these tables.

Existing schema from `crates/omninova-core/src/db/migrations.rs`:

```sql
-- Sessions table: Conversation sessions
CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id INTEGER NOT NULL,
    title TEXT,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE
);

-- Messages table: Individual messages within sessions
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_sessions_agent_id ON sessions(agent_id);
CREATE INDEX IF NOT EXISTS idx_messages_session_id ON messages(session_id);
```

### Session Model Pattern

Follow the AgentModel pattern from `crates/omninova-core/src/agent/model.rs`:

```rust
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Message role enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Session model (matches sessions table)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: i64,
    pub session_uuid: String,
    pub agent_id: i64,
    pub title: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// New session data for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSession {
    pub agent_id: i64,
    pub title: Option<String>,
}

/// Session update data
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdate {
    pub title: Option<String>,
}

/// Message model (matches messages table)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: i64,
    pub session_id: i64,
    pub role: MessageRole,
    pub content: String,
    pub created_at: i64,
}

/// New message data for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMessage {
    pub session_id: i64,
    pub role: MessageRole,
    pub content: String,
}
```

### SessionStore Pattern

Follow the AgentStore pattern from `crates/omninova-core/src/agent/store.rs`:

```rust
use crate::db::DbPool;
use crate::session::{Session, NewSession, SessionUpdate, Message, NewMessage, MessageRole};
use rusqlite::params;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("Session not found: {0}")]
    NotFound(i64),
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Invalid session data: {0}")]
    InvalidData(String),
}

pub struct SessionStore {
    db: Arc<DbPool>,
}

impl SessionStore {
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { db }
    }

    pub fn create(&self, new_session: NewSession) -> Result<Session, SessionError> {
        let conn = self.db.get()?;
        let now = chrono::Utc::now().timestamp();
        let uuid = uuid::Uuid::new_v4().to_string();

        conn.execute(
            "INSERT INTO sessions (session_uuid, agent_id, title, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![uuid, new_session.agent_id, new_session.title, now, now],
        )?;

        let id = conn.last_insert_rowid();
        Ok(Session {
            id,
            session_uuid: uuid,
            agent_id: new_session.agent_id,
            title: new_session.title,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn find_by_id(&self, id: i64) -> Result<Option<Session>, SessionError> {
        let conn = self.db.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_uuid, agent_id, title, created_at, updated_at
             FROM sessions WHERE id = ?1"
        )?;

        let session = stmt.query_row(params![id], |row| {
            Ok(Session {
                id: row.get(0)?,
                session_uuid: row.get(1)?,
                agent_id: row.get(2)?,
                title: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        }).optional()?;

        Ok(session)
    }

    pub fn list_by_agent(&self, agent_id: i64) -> Result<Vec<Session>, SessionError> {
        let conn = self.db.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_uuid, agent_id, title, created_at, updated_at
             FROM sessions WHERE agent_id = ?1
             ORDER BY updated_at DESC"
        )?;

        let sessions = stmt.query_map(params![agent_id], |row| {
            Ok(Session {
                id: row.get(0)?,
                session_uuid: row.get(1)?,
                agent_id: row.get(2)?,
                title: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;

        Ok(sessions)
    }

    pub fn update(&self, id: i64, update: SessionUpdate) -> Result<Session, SessionError> {
        let now = chrono::Utc::now().timestamp();
        let conn = self.db.get()?;

        conn.execute(
            "UPDATE sessions SET title = COALESCE(?1, title), updated_at = ?2 WHERE id = ?3",
            params![update.title, now, id],
        )?;

        self.find_by_id(id)?.ok_or(SessionError::NotFound(id))
    }

    pub fn delete(&self, id: i64) -> Result<(), SessionError> {
        let conn = self.db.get()?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(())
    }
}
```

### MessageStore Pattern

```rust
pub struct MessageStore {
    db: Arc<DbPool>,
}

impl MessageStore {
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { db }
    }

    pub fn create(&self, new_message: NewMessage) -> Result<Message, SessionError> {
        let conn = self.db.get()?;
        let now = chrono::Utc::now().timestamp();
        let role_str = match new_message.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
        };

        conn.execute(
            "INSERT INTO messages (session_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![new_message.session_id, role_str, new_message.content, now],
        )?;

        let id = conn.last_insert_rowid();
        Ok(Message {
            id,
            session_id: new_message.session_id,
            role: new_message.role,
            content: new_message.content,
            created_at: now,
        })
    }

    pub fn list_by_session(&self, session_id: i64) -> Result<Vec<Message>, SessionError> {
        let conn = self.db.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, created_at
             FROM messages WHERE session_id = ?1
             ORDER BY created_at ASC"
        )?;

        let messages = stmt.query_map(params![session_id], |row| {
            let role_str: String = row.get(2)?;
            let role = match role_str.as_str() {
                "user" => MessageRole::User,
                "assistant" => MessageRole::Assistant,
                "system" => MessageRole::System,
                _ => MessageRole::User,
            };
            Ok(Message {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role,
                content: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;

        Ok(messages)
    }
}
```

### TypeScript Types

Create `apps/omninova-tauri/src/types/session.ts`:

```typescript
/**
 * 消息角色类型
 */
export type MessageRole = 'user' | 'assistant' | 'system';

/**
 * 会话模型（与后端 Session 一致）
 */
export interface Session {
  id: number;
  sessionUuid: string;
  agentId: number;
  title?: string;
  createdAt: number;
  updatedAt: number;
}

/**
 * 新会话数据
 */
export interface NewSession {
  agentId: number;
  title?: string;
}

/**
 * 会话更新数据
 */
export interface SessionUpdate {
  title?: string;
}

/**
 * 消息模型（与后端 Message 一致）
 */
export interface Message {
  id: number;
  sessionId: number;
  role: MessageRole;
  content: string;
  createdAt: number;
}

/**
 * 新消息数据
 */
export interface NewMessage {
  sessionId: number;
  role: MessageRole;
  content: string;
}
```

### Tauri Commands Pattern

Follow existing patterns from `src-tauri/src/lib.rs`:

```rust
#[tauri::command]
async fn create_session(
    agent_id: i64,
    title: Option<String>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Session, String> {
    let store = SessionStore::new(state.db.clone());
    let new_session = NewSession { agent_id, title };
    store.create(new_session).map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_sessions_by_agent(
    agent_id: i64,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<Session>, String> {
    let store = SessionStore::new(state.db.clone());
    store.list_by_agent(agent_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_messages_by_session(
    session_id: i64,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<Message>, String> {
    let store = MessageStore::new(state.db.clone());
    store.list_by_session(session_id).map_err(|e| e.to_string())
}
```

### Testing Standards

Following Stories 3.1-3.7 patterns:

1. **Unit Tests** - Use tempfile for test databases
2. **Test Coverage** - Cover CRUD operations, edge cases, error handling
3. **Test Pattern:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbPool;
    use tempfile::NamedTempFile;

    fn setup_test_db() -> Arc<DbPool> {
        let temp_file = NamedTempFile::new().unwrap();
        let db = DbPool::new(temp_file.path().to_str().unwrap()).unwrap();
        db.run_migrations().unwrap();
        Arc::new(db)
    }

    #[test]
    fn test_create_session() {
        let db = setup_test_db();
        let store = SessionStore::new(db);
        // ... test implementation
    }
}
```

### Files to Create

- `crates/omninova-core/src/session/mod.rs` - Module exports
- `crates/omninova-core/src/session/model.rs` - Session and Message structs
- `crates/omninova-core/src/session/store.rs` - SessionStore and MessageStore
- `apps/omninova-tauri/src/types/session.ts` - TypeScript types

### Files to Modify

- `crates/omninova-core/src/lib.rs` - Add session module export
- `apps/omninova-tauri/src-tauri/src/lib.rs` - Add Tauri commands

### Files to Reference

- `crates/omninova-core/src/agent/model.rs` - Model pattern reference
- `crates/omninova-core/src/agent/store.rs` - Store pattern reference
- `crates/omninova-core/src/db/migrations.rs` - Existing schema (DO NOT MODIFY)
- `apps/omninova-tauri/src/types/agent.ts` - TypeScript type pattern reference

## References

- [Source: _bmad-output/planning-artifacts/epics.md#L754-L768] - Story 4.1 requirements
- [Source: crates/omninova-core/src/db/migrations.rs] - Existing sessions and messages schema
- [Source: crates/omninova-core/src/agent/model.rs] - Model pattern for Session/Message structs
- [Source: crates/omninova-core/src/agent/store.rs] - Store pattern for SessionStore/MessageStore
- [Source: apps/omninova-tauri/src/types/agent.ts] - TypeScript type pattern
- [Source: _bmad-output/planning-artifacts/architecture.md] - Naming conventions and structure

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

N/A

### Completion Notes List

Story file created with comprehensive context including:
- Database schema already exists in migration 001_initial
- Session and Message model patterns following AgentModel
- SessionStore and MessageStore patterns following AgentStore
- TypeScript type definitions
- Tauri commands for session management
- Testing patterns with tempfile

**Code Review Fixes (2026-03-18):**

Issue #1 Fixed: Tauri Commands refactored from JSON string parameters to idiomatic direct struct parameters:
- `create_session(session_json: String) -> Result<String>` → `create_session(new_session: NewSession) -> Result<Session>`
- `get_session(id: i64) -> Result<String>` → `get_session(id: i64) -> Result<Option<Session>>`
- `list_sessions_by_agent(agent_id: i64) -> Result<String>` → `list_sessions_by_agent(agent_id: i64) -> Result<Vec<Session>>`
- `update_session(id: i64, update_json: String) -> Result<String>` → `update_session(id: i64, update: SessionUpdate) -> Result<Session>`
- `create_message(message_json: String) -> Result<String>` → `create_message(new_message: NewMessage) -> Result<Message>`
- `list_messages_by_session(session_id: i64) -> Result<String>` → `list_messages_by_session(session_id: i64) -> Result<Vec<Message>>`

Issue #2 Noted: Git working directory contains unrelated changes from other stories/features:
- Story 4.1 related files: `src-tauri/src/lib.rs`, `omninova-core/src/lib.rs`, `session/`, `session.ts`
- Unrelated modifications: Agent components, provider types, backup, migrations, vitest config
- Unrelated untracked: Chat components, provider hooks, stores
- Recommendation: Commit Story 4.1 changes separately when ready

### File List

**To Create:**
- `crates/omninova-core/src/session/mod.rs`
- `crates/omninova-core/src/session/model.rs`
- `crates/omninova-core/src/session/store.rs`
- `apps/omninova-tauri/src/types/session.ts`

**To Modify:**
- `crates/omninova-core/src/lib.rs`
- `apps/omninova-tauri/src-tauri/src/lib.rs`