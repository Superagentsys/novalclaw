//! SQLite-backed long-term memory.
//!
//! Unlike [`crate::memory::backend::JsonFileMemory`], writes are incremental:
//! a single `INSERT OR REPLACE` per entry instead of re-serializing the whole
//! store. WAL mode keeps concurrent readers from blocking the writer, so the
//! gateway and desktop app can share one file.
//!
//! The `embedding` column is written by [`crate::memory::semantic`]; keyword
//! search ignores it so a store without embeddings behaves normally.

use crate::memory::search::{rank_entries_with_options, SearchOptions};
use crate::memory::traits::{Memory, MemoryCategory, MemoryEntry};
use anyhow::{Context, Result};
use async_trait::async_trait;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS memories (
    key        TEXT PRIMARY KEY,
    id         TEXT NOT NULL,
    content    TEXT NOT NULL,
    category   TEXT NOT NULL,
    timestamp  TEXT NOT NULL,
    ts_num     INTEGER NOT NULL DEFAULT 0,
    session_id TEXT,
    embedding  BLOB
);
CREATE INDEX IF NOT EXISTS idx_memories_session ON memories(session_id);
CREATE INDEX IF NOT EXISTS idx_memories_category ON memories(category);
CREATE INDEX IF NOT EXISTS idx_memories_ts ON memories(ts_num DESC);
";

/// Rows pulled into memory before keyword ranking. Ranking needs the full
/// candidate set to score, but an unbounded `SELECT *` would defeat the point
/// of moving off the JSON store.
const RANK_CANDIDATE_LIMIT: usize = 2_000;

pub struct SqliteMemory {
    connection: Arc<Mutex<Connection>>,
    path: PathBuf,
    search_options: SearchOptions,
}

impl SqliteMemory {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        Self::open_with_options(path, SearchOptions::default())
    }

    pub fn open_with_options(
        path: impl Into<PathBuf>,
        search_options: SearchOptions,
    ) -> Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!("failed to create memory directory: {}", parent.display())
                })?;
            }
        }
        let connection = Connection::open(&path)
            .with_context(|| format!("failed to open memory database: {}", path.display()))?;
        // WAL survives crashes better than the default rollback journal and
        // lets the desktop UI read while the gateway writes.
        let _ = connection.pragma_update(None, "journal_mode", "WAL");
        let _ = connection.pragma_update(None, "synchronous", "NORMAL");
        connection
            .execute_batch(SCHEMA)
            .context("failed to initialize memory schema")?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            path,
            search_options,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| anyhow::anyhow!("memory database lock poisoned"))
    }

    /// Attach a vector to an existing entry. Used by the semantic layer after
    /// the embedding request returns, so a failed embedding never blocks the
    /// write of the entry itself.
    pub fn set_embedding(&self, key: &str, vector: &[f32]) -> Result<()> {
        let blob = encode_vector(vector);
        let connection = self.lock()?;
        connection.execute(
            "UPDATE memories SET embedding = ?1 WHERE key = ?2",
            params![blob, key],
        )?;
        Ok(())
    }

    /// The most recent entries that carry an embedding, for cosine ranking.
    pub fn embedded_entries(
        &self,
        session_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<(MemoryEntry, Vec<f32>)>> {
        let limit = if limit == 0 {
            i64::MAX
        } else {
            limit as i64
        };
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT key, id, content, category, timestamp, session_id, embedding \
             FROM memories WHERE embedding IS NOT NULL \
               AND (?1 IS NULL OR session_id = ?1) \
             ORDER BY ts_num DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![session_id, limit], row_to_entry_with_vector)?;
        let mut out = Vec::new();
        for row in rows {
            let (entry, vector) = row?;
            if let Some(vector) = vector {
                out.push((entry, vector));
            }
        }
        Ok(out)
    }

    fn candidates(&self, session_id: Option<&str>) -> Result<Vec<MemoryEntry>> {
        let connection = self.lock()?;
        let mut out = Vec::new();
        match session_id {
            Some(sid) => {
                let mut statement = connection.prepare(
                    "SELECT key, id, content, category, timestamp, session_id FROM memories \
                     WHERE session_id = ?1 ORDER BY ts_num DESC LIMIT ?2",
                )?;
                let rows = statement
                    .query_map(params![sid, RANK_CANDIDATE_LIMIT as i64], row_to_entry)?;
                for row in rows {
                    out.push(row?);
                }
            }
            None => {
                let mut statement = connection.prepare(
                    "SELECT key, id, content, category, timestamp, session_id FROM memories \
                     ORDER BY ts_num DESC LIMIT ?1",
                )?;
                let rows = statement.query_map(params![RANK_CANDIDATE_LIMIT as i64], row_to_entry)?;
                for row in rows {
                    out.push(row?);
                }
            }
        }
        Ok(out)
    }
}

#[async_trait]
impl Memory for SqliteMemory {
    fn name(&self) -> &str {
        "sqlite"
    }

    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> Result<()> {
        let timestamp = now_timestamp();
        let ts_num = timestamp.parse::<i64>().unwrap_or(0);
        let id = format!("mem-{}", uuid::Uuid::new_v4());
        let connection = self.lock()?;
        // Replacing by key keeps the JSON store's semantics (key is the unique
        // identity) while leaving any previously computed embedding stale, so
        // the embedding is cleared here and recomputed by the semantic layer.
        connection.execute(
            "INSERT INTO memories (key, id, content, category, timestamp, ts_num, session_id, embedding) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL) \
             ON CONFLICT(key) DO UPDATE SET \
               content = excluded.content, \
               category = excluded.category, \
               timestamp = excluded.timestamp, \
               ts_num = excluded.ts_num, \
               session_id = excluded.session_id, \
               embedding = NULL",
            params![
                key,
                id,
                content,
                category.to_string(),
                timestamp,
                ts_num,
                session_id,
            ],
        )?;
        Ok(())
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
    ) -> Result<Vec<MemoryEntry>> {
        let needle = query.trim().to_lowercase();
        let candidates = self.candidates(session_id)?;
        let filtered = candidates
            .into_iter()
            .filter(|entry| needle.is_empty() || entry.content.to_lowercase().contains(&needle))
            .collect::<Vec<_>>();
        let mut ranked = rank_entries_with_options(query, filtered, &self.search_options);
        if limit > 0 {
            ranked.truncate(limit);
        }
        Ok(ranked)
    }

    async fn get(&self, key: &str) -> Result<Option<MemoryEntry>> {
        let connection = self.lock()?;
        let entry = connection
            .query_row(
                "SELECT key, id, content, category, timestamp, session_id FROM memories WHERE key = ?1",
                params![key],
                row_to_entry,
            )
            .optional()?;
        Ok(entry)
    }

    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> Result<Vec<MemoryEntry>> {
        let category = category.map(ToString::to_string);
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT key, id, content, category, timestamp, session_id FROM memories \
             WHERE (?1 IS NULL OR category = ?1) AND (?2 IS NULL OR session_id = ?2) \
             ORDER BY ts_num DESC",
        )?;
        let rows = statement.query_map(params![category, session_id], row_to_entry)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    async fn forget(&self, key: &str) -> Result<bool> {
        let connection = self.lock()?;
        let affected = connection.execute("DELETE FROM memories WHERE key = ?1", params![key])?;
        Ok(affected > 0)
    }

    async fn count(&self) -> Result<usize> {
        let connection = self.lock()?;
        let total: i64 =
            connection.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?;
        Ok(total.max(0) as usize)
    }

    async fn health_check(&self) -> bool {
        self.lock()
            .and_then(|connection| {
                connection
                    .query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
                    .map_err(Into::into)
            })
            .is_ok()
    }
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryEntry> {
    Ok(MemoryEntry {
        key: row.get(0)?,
        id: row.get(1)?,
        content: row.get(2)?,
        category: parse_category(&row.get::<_, String>(3)?),
        timestamp: row.get(4)?,
        session_id: row.get(5)?,
        score: None,
    })
}

fn row_to_entry_with_vector(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(MemoryEntry, Option<Vec<f32>>)> {
    let entry = MemoryEntry {
        key: row.get(0)?,
        id: row.get(1)?,
        content: row.get(2)?,
        category: parse_category(&row.get::<_, String>(3)?),
        timestamp: row.get(4)?,
        session_id: row.get(5)?,
        score: None,
    };
    let blob: Option<Vec<u8>> = row.get(6)?;
    Ok((entry, blob.as_deref().and_then(decode_vector)))
}

fn parse_category(raw: &str) -> MemoryCategory {
    match raw {
        "core" => MemoryCategory::Core,
        "daily" => MemoryCategory::Daily,
        "conversation" => MemoryCategory::Conversation,
        other => MemoryCategory::Custom(other.to_string()),
    }
}

fn now_timestamp() -> String {
    time::OffsetDateTime::now_utc().unix_timestamp().to_string()
}

fn encode_vector(vector: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(vector.len() * 4);
    for value in vector {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

fn decode_vector(blob: &[u8]) -> Option<Vec<f32>> {
    if blob.is_empty() || !blob.len().is_multiple_of(4) {
        return None;
    }
    Some(
        blob.chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("omninova-memory-{label}-{nonce}.db"))
    }

    #[tokio::test]
    async fn entries_survive_reopen() {
        let path = temp_db("reopen");
        {
            let memory = SqliteMemory::open(&path).unwrap();
            memory
                .store("pref/lang", "用户偏好中文", MemoryCategory::Core, None)
                .await
                .unwrap();
        }

        let reopened = SqliteMemory::open(&path).unwrap();
        let entry = reopened.get("pref/lang").await.unwrap().unwrap();

        assert_eq!(entry.content, "用户偏好中文");
        assert_eq!(reopened.count().await.unwrap(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn store_replaces_by_key_without_duplicating() {
        let path = temp_db("replace");
        let memory = SqliteMemory::open(&path).unwrap();

        memory
            .store("k", "first", MemoryCategory::Core, None)
            .await
            .unwrap();
        memory
            .store("k", "second", MemoryCategory::Core, None)
            .await
            .unwrap();

        assert_eq!(memory.count().await.unwrap(), 1);
        assert_eq!(memory.get("k").await.unwrap().unwrap().content, "second");
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn recall_scopes_to_session_and_ranks_matches() {
        let path = temp_db("recall");
        let memory = SqliteMemory::open(&path).unwrap();

        memory
            .store("a", "deploy runbook", MemoryCategory::Core, Some("s1"))
            .await
            .unwrap();
        memory
            .store("b", "deploy notes", MemoryCategory::Core, Some("s2"))
            .await
            .unwrap();

        let scoped = memory.recall("deploy", 10, Some("s1")).await.unwrap();
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].key, "a");

        let global = memory.recall("deploy", 10, None).await.unwrap();
        assert_eq!(global.len(), 2);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn custom_category_round_trips() {
        let path = temp_db("category");
        let memory = SqliteMemory::open(&path).unwrap();

        memory
            .store(
                "k",
                "v",
                MemoryCategory::Custom("project-notes".into()),
                None,
            )
            .await
            .unwrap();

        let listed = memory
            .list(Some(&MemoryCategory::Custom("project-notes".into())), None)
            .await
            .unwrap();
        assert_eq!(listed.len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn embedding_round_trips_through_blob() {
        let path = temp_db("embedding");
        let memory = SqliteMemory::open(&path).unwrap();
        memory
            .store("k", "content", MemoryCategory::Core, None)
            .await
            .unwrap();

        memory.set_embedding("k", &[0.5, -1.5, 2.0]).unwrap();
        let embedded = memory.embedded_entries(None, 10).unwrap();

        assert_eq!(embedded.len(), 1);
        assert_eq!(embedded[0].1, vec![0.5, -1.5, 2.0]);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn restore_clears_stale_embedding() {
        let path = temp_db("stale");
        let memory = SqliteMemory::open(&path).unwrap();
        memory
            .store("k", "old", MemoryCategory::Core, None)
            .await
            .unwrap();
        memory.set_embedding("k", &[1.0, 0.0]).unwrap();

        memory
            .store("k", "new", MemoryCategory::Core, None)
            .await
            .unwrap();

        assert!(memory.embedded_entries(None, 10).unwrap().is_empty());
        let _ = std::fs::remove_file(path);
    }
}
