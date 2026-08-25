use super::types::{Task, TaskCheckpoint, TaskStatus};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    goal TEXT NOT NULL,
    session_id TEXT,
    status TEXT NOT NULL,
    checkpoint_json TEXT NOT NULL,
    wake_schedule TEXT NOT NULL,
    max_rounds INTEGER NOT NULL,
    deadline_at INTEGER,
    max_total_tokens INTEGER,
    rounds_used INTEGER NOT NULL DEFAULT 0,
    tokens_used INTEGER NOT NULL DEFAULT 0,
    lease_until INTEGER,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tasks_session ON tasks(session_id);
CREATE TABLE IF NOT EXISTS todos (
    session_id TEXT PRIMARY KEY,
    items_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
";

#[derive(Clone)]
pub struct TaskStore {
    connection: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl TaskStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!("failed to create task directory: {}", parent.display())
                })?;
            }
        }
        let connection = Connection::open(&path)
            .with_context(|| format!("failed to open task database: {}", path.display()))?;
        let _ = connection.pragma_update(None, "journal_mode", "WAL");
        let _ = connection.pragma_update(None, "synchronous", "NORMAL");
        connection
            .execute_batch(SCHEMA)
            .context("failed to initialize task schema")?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            path,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn upsert(&self, task: &Task) -> Result<()> {
        let conn = self.connection.lock().expect("task db lock poisoned");
        let checkpoint = serde_json::to_string(&task.checkpoint)?;
        conn.execute(
            "INSERT INTO tasks (
                id, goal, session_id, status, checkpoint_json, wake_schedule,
                max_rounds, deadline_at, max_total_tokens, rounds_used, tokens_used,
                lease_until, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(id) DO UPDATE SET
                goal=excluded.goal,
                session_id=excluded.session_id,
                status=excluded.status,
                checkpoint_json=excluded.checkpoint_json,
                wake_schedule=excluded.wake_schedule,
                max_rounds=excluded.max_rounds,
                deadline_at=excluded.deadline_at,
                max_total_tokens=excluded.max_total_tokens,
                rounds_used=excluded.rounds_used,
                tokens_used=excluded.tokens_used,
                lease_until=excluded.lease_until,
                updated_at=excluded.updated_at",
            params![
                task.id,
                task.goal,
                task.session_id,
                task.status.as_str(),
                checkpoint,
                task.wake_schedule,
                task.max_rounds as i64,
                task.deadline_at,
                task.max_total_tokens.map(|v| v as i64),
                task.rounds_used as i64,
                task.tokens_used as i64,
                task.lease_until,
                task.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<Task>> {
        let conn = self.connection.lock().expect("task db lock poisoned");
        conn.query_row(
            "SELECT id, goal, session_id, status, checkpoint_json, wake_schedule,
                    max_rounds, deadline_at, max_total_tokens, rounds_used, tokens_used,
                    lease_until, updated_at
             FROM tasks WHERE id = ?1",
            params![id],
            row_to_task,
        )
        .optional()
        .context("failed to load task")
    }

    pub fn get_by_session(&self, session_id: &str) -> Result<Option<Task>> {
        let conn = self.connection.lock().expect("task db lock poisoned");
        conn.query_row(
            "SELECT id, goal, session_id, status, checkpoint_json, wake_schedule,
                    max_rounds, deadline_at, max_total_tokens, rounds_used, tokens_used,
                    lease_until, updated_at
             FROM tasks WHERE session_id = ?1
             ORDER BY updated_at DESC LIMIT 1",
            params![session_id],
            row_to_task,
        )
        .optional()
        .context("failed to load task by session")
    }

    pub fn put_todos(&self, session_id: &str, items: &[serde_json::Value]) -> Result<()> {
        let conn = self.connection.lock().expect("task db lock poisoned");
        let items_json = serde_json::to_string(items)?;
        let now = now_unix_ts();
        conn.execute(
            "INSERT INTO todos (session_id, items_json, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET items_json=excluded.items_json, updated_at=excluded.updated_at",
            params![session_id, items_json, now],
        )?;
        Ok(())
    }

    pub fn get_todos(&self, session_id: &str) -> Result<Vec<serde_json::Value>> {
        let conn = self.connection.lock().expect("task db lock poisoned");
        let raw: Option<String> = conn
            .query_row(
                "SELECT items_json FROM todos WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .optional()?;
        match raw {
            Some(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
            None => Ok(Vec::new()),
        }
    }
}

fn row_to_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    let checkpoint_json: String = row.get(4)?;
    let checkpoint: TaskCheckpoint =
        serde_json::from_str(&checkpoint_json).unwrap_or_default();
    let status: String = row.get(3)?;
    Ok(Task {
        id: row.get(0)?,
        goal: row.get(1)?,
        session_id: row.get(2)?,
        status: TaskStatus::parse(&status),
        checkpoint,
        wake_schedule: row.get(5)?,
        max_rounds: row.get::<_, i64>(6)? as u32,
        deadline_at: row.get(7)?,
        max_total_tokens: row.get::<_, Option<i64>>(8)?.map(|v| v as u64),
        rounds_used: row.get::<_, i64>(9)? as u32,
        tokens_used: row.get::<_, i64>(10)? as u64,
        lease_until: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

pub(crate) fn now_unix_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
