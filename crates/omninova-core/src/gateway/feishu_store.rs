//! Feishu event and job persistence using SQLite.
//!
//! This module provides:
//! - SQLite database initialization and migrations
//! - Event deduplication with UNIQUE constraint
//! - Job state persistence
//! - Privacy filters (hashing, preview truncation)
//!
//! Database path: {config_dir}/state.sqlite

use rusqlite::{Connection, params, OptionalExtension};
use sha2::{Sha256, Digest};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use thiserror::Error;

/// Maximum length for text preview
const MAX_TEXT_PREVIEW_LEN: usize = 80;

/// Maximum length for reply preview
const MAX_REPLY_PREVIEW_LEN: usize = 120;

/// Secrets to redact from metadata
const SECRET_KEYS: &[&str] = &[
    "app_secret",
    "tenant_access_token",
    "access_token",
    "authorization",
    "app_access_token",
    "refresh_token",
    "secret",
    "password",
];

/// Event status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventStatus {
    Received,
    Skipped,
    Duplicate,
    Queued,
    Processing,
    Processed,
    Failed,
}

impl EventStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventStatus::Received => "RECEIVED",
            EventStatus::Skipped => "SKIPPED",
            EventStatus::Duplicate => "DUPLICATE",
            EventStatus::Queued => "QUEUED",
            EventStatus::Processing => "PROCESSING",
            EventStatus::Processed => "PROCESSED",
            EventStatus::Failed => "FAILED",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "RECEIVED" => Some(EventStatus::Received),
            "SKIPPED" => Some(EventStatus::Skipped),
            "DUPLICATE" => Some(EventStatus::Duplicate),
            "QUEUED" => Some(EventStatus::Queued),
            "PROCESSING" => Some(EventStatus::Processing),
            "PROCESSED" => Some(EventStatus::Processed),
            "FAILED" => Some(EventStatus::Failed),
            _ => None,
        }
    }
}

/// Job status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Queued,
    Processing,
    Sent,
    Completed,
    Failed,
    Dead,
    Cancelled,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Pending => "PENDING",
            JobStatus::Queued => "QUEUED",
            JobStatus::Processing => "PROCESSING",
            JobStatus::Sent => "SENT",
            JobStatus::Completed => "COMPLETED",
            JobStatus::Failed => "FAILED",
            JobStatus::Dead => "DEAD",
            JobStatus::Cancelled => "CANCELLED",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "PENDING" => Some(JobStatus::Pending),
            "QUEUED" => Some(JobStatus::Queued),
            "PROCESSING" => Some(JobStatus::Processing),
            "SENT" => Some(JobStatus::Sent),
            "COMPLETED" => Some(JobStatus::Completed),
            "FAILED" => Some(JobStatus::Failed),
            "DEAD" => Some(JobStatus::Dead),
            "CANCELLED" => Some(JobStatus::Cancelled),
            _ => None,
        }
    }
}

/// Outbox status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxStatus {
    Pending,
    Sending,
    Sent,
    Failed,
    Dead,
    Skipped,
    /// Privacy protected - cannot be retried because full reply body not stored
    Abandoned,
    /// Same as Abandoned but retains recoverable metadata
    FailedPrivacyNoRetry,
    /// Reconstruction of reply body failed (missing result_json or required fields)
    FailedReconstructIncomplete,
}

impl OutboxStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutboxStatus::Pending => "PENDING",
            OutboxStatus::Sending => "SENDING",
            OutboxStatus::Sent => "SENT",
            OutboxStatus::Failed => "FAILED",
            OutboxStatus::Dead => "DEAD",
            OutboxStatus::Skipped => "SKIPPED",
            OutboxStatus::Abandoned => "ABANDONED",
            OutboxStatus::FailedPrivacyNoRetry => "FAILED_PRIVACY_NO_RETRY",
            OutboxStatus::FailedReconstructIncomplete => "FAILED_RECONSTRUCT_INCOMPLETE",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "PENDING" => Some(OutboxStatus::Pending),
            "SENDING" => Some(OutboxStatus::Sending),
            "SENT" => Some(OutboxStatus::Sent),
            "FAILED" => Some(OutboxStatus::Failed),
            "DEAD" => Some(OutboxStatus::Dead),
            "SKIPPED" => Some(OutboxStatus::Skipped),
            "ABANDONED" => Some(OutboxStatus::Abandoned),
            "FAILED_PRIVACY_NO_RETRY" => Some(OutboxStatus::FailedPrivacyNoRetry),
            "FAILED_RECONSTRUCT_INCOMPLETE" => Some(OutboxStatus::FailedReconstructIncomplete),
            _ => None,
        }
    }
}

/// Reply kind classification for retryability
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyKind {
    /// Static template reply - retryable by reconstructing from kind
    Progress,
    /// Static template reply - retryable by reconstructing from kind
    Timeout,
    /// Static template reply - retryable by reconstructing from kind
    Failure,
    /// Static template reply - retryable by reconstructing from kind
    ChatOnlyBlocked,
    /// Static template reply - retryable by reconstructing from kind
    Unsupported,
    /// Restructurable from result_json (e.g., /monitor results)
    MonitorFinal,
    /// Free-form LLM response - NOT retryable due to privacy
    LlmFinal,
}

impl ReplyKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReplyKind::Progress => "progress_reply",
            ReplyKind::Timeout => "timeout_reply",
            ReplyKind::Failure => "failure_reply",
            ReplyKind::ChatOnlyBlocked => "chat_only_blocked_reply",
            ReplyKind::Unsupported => "unsupported_reply",
            ReplyKind::MonitorFinal => "monitor_final",
            ReplyKind::LlmFinal => "llm_final",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "progress_reply" => Some(ReplyKind::Progress),
            "timeout_reply" => Some(ReplyKind::Timeout),
            "failure_reply" => Some(ReplyKind::Failure),
            "chat_only_blocked_reply" => Some(ReplyKind::ChatOnlyBlocked),
            "unsupported_reply" => Some(ReplyKind::Unsupported),
            "monitor_final" => Some(ReplyKind::MonitorFinal),
            "llm_final" => Some(ReplyKind::LlmFinal),
            _ => None,
        }
    }

    /// Can this reply be reconstructed for retry?
    pub fn is_retryable(&self) -> bool {
        match self {
            ReplyKind::Progress
            | ReplyKind::Timeout
            | ReplyKind::Failure
            | ReplyKind::ChatOnlyBlocked
            | ReplyKind::Unsupported
            | ReplyKind::MonitorFinal => true,
            ReplyKind::LlmFinal => false,
        }
    }
}

/// Persistence errors
#[derive(Error, Debug)]
pub enum StoreError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    
    #[error("Event already exists: {0}")]
    DuplicateEvent(String),
    
    #[error("Job not found: {0}")]
    JobNotFound(String),
    
    #[error("Outbox not found: {0}")]
    OutboxNotFound(String),
    
    #[error("Database not initialized")]
    NotInitialized,
    
    #[error("Store lock poisoned - previous operation panicked")]
    PoisonedLock,
}

/// Feishu event record
#[derive(Debug, Clone)]
pub struct FeishuEvent {
    pub id: i64,
    pub event_key: String,
    pub channel: String,
    pub event_id: Option<String>,
    pub message_id: Option<String>,
    pub chat_id: Option<String>,
    pub user_id_hash: Option<String>,
    pub event_type: Option<String>,
    pub sender_type: Option<String>,
    pub message_type: Option<String>,
    pub text_hash: Option<String>,
    pub text_preview: Option<String>,
    pub status: EventStatus,
    pub skip_reason: Option<String>,
    pub received_at: i64,
    pub updated_at: i64,
    pub metadata_json: Option<String>,
}

/// Feishu job record
#[derive(Debug, Clone)]
pub struct FeishuJob {
    pub id: i64,
    pub job_id: String,
    pub event_key: String,
    pub channel: String,
    pub mode: String,
    pub slash_command: Option<String>,
    pub status: JobStatus,
    pub attempts: i32,
    pub max_attempts: i32,
    pub next_attempt_at: Option<i64>,
    pub locked_at: Option<i64>,
    pub locked_by: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub payload_json: Option<String>,
}

/// Feishu outbox record
#[derive(Debug, Clone)]
pub struct FeishuOutbox {
    pub id: i64,
    pub outbound_id: String,
    pub job_id: Option<String>,
    pub event_key: Option<String>,
    pub channel: String,
    pub chat_id: Option<String>,
    pub reply_kind: Option<String>,
    pub status: OutboxStatus,
    pub attempts: i32,
    pub max_attempts: i32,
    pub next_attempt_at: Option<i64>,
    pub platform_message_id: Option<String>,
    pub reply_hash: Option<String>,
    pub reply_preview: Option<String>,
    /// Structured result data (for /monitor results, etc.)
    pub result_json: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub sent_at: Option<i64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

/// SQLite store for Feishu persistence
pub struct FeishuStore {
    conn: Mutex<Connection>,
    db_path: PathBuf,
}

impl FeishuStore {
    /// Open or create the Feishu store database
    pub fn open(config_dir: &Path) -> Result<Self, StoreError> {
        let db_dir = config_dir.to_path_buf();
        std::fs::create_dir_all(&db_dir).map_err(|e| {
            StoreError::Sqlite(rusqlite::Error::InvalidPath(db_dir.clone()))
        })?;
        
        let db_path = db_dir.join("state.sqlite");
        println!("[feishu-store] opened path={}", db_path.display());
        
        let conn = Connection::open(&db_path)?;
        let store = Self {
            conn: Mutex::new(conn),
            db_path,
        };
        
        store.run_migrations()?;
        
        Ok(store)
    }

    /// Run database migrations
    fn run_migrations(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        
        // Create tables
        conn.execute_batch(
            r#"
            -- feishu_events table
            CREATE TABLE IF NOT EXISTS feishu_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_key TEXT NOT NULL UNIQUE,
                channel TEXT NOT NULL,
                event_id TEXT,
                message_id TEXT,
                chat_id TEXT,
                user_id_hash TEXT,
                event_type TEXT,
                sender_type TEXT,
                message_type TEXT,
                text_hash TEXT,
                text_preview TEXT,
                status TEXT NOT NULL DEFAULT 'RECEIVED',
                skip_reason TEXT,
                received_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                metadata_json TEXT
            );
            
            CREATE INDEX IF NOT EXISTS idx_feishu_events_status ON feishu_events(status);
            CREATE INDEX IF NOT EXISTS idx_feishu_events_channel ON feishu_events(channel);
            CREATE INDEX IF NOT EXISTS idx_feishu_events_received_at ON feishu_events(received_at DESC);
            
            -- feishu_jobs table
            CREATE TABLE IF NOT EXISTS feishu_jobs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                job_id TEXT NOT NULL UNIQUE,
                event_key TEXT NOT NULL,
                channel TEXT NOT NULL,
                mode TEXT NOT NULL,
                slash_command TEXT,
                status TEXT NOT NULL DEFAULT 'PENDING',
                attempts INTEGER NOT NULL DEFAULT 0,
                max_attempts INTEGER NOT NULL DEFAULT 3,
                next_attempt_at INTEGER,
                locked_at INTEGER,
                locked_by TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                started_at INTEGER,
                completed_at INTEGER,
                error_code TEXT,
                error_message TEXT,
                payload_json TEXT
            );
            
            CREATE INDEX IF NOT EXISTS idx_feishu_jobs_job_id ON feishu_jobs(job_id);
            CREATE INDEX IF NOT EXISTS idx_feishu_jobs_status ON feishu_jobs(status);
            CREATE INDEX IF NOT EXISTS idx_feishu_jobs_event_key ON feishu_jobs(event_key);
            CREATE INDEX IF NOT EXISTS idx_feishu_jobs_locked_at ON feishu_jobs(locked_at);
            
            -- feishu_outbox table (base schema - additional columns added via ALTER TABLE migration)
            CREATE TABLE IF NOT EXISTS feishu_outbox (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                outbound_id TEXT NOT NULL UNIQUE,
                job_id TEXT,
                event_key TEXT,
                channel TEXT NOT NULL,
                chat_id TEXT,
                reply_kind TEXT,
                status TEXT NOT NULL DEFAULT 'PENDING',
                attempts INTEGER NOT NULL DEFAULT 0,
                max_attempts INTEGER NOT NULL DEFAULT 3,
                next_attempt_at INTEGER,
                platform_message_id TEXT,
                reply_hash TEXT,
                reply_preview TEXT,
                result_json TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                sent_at INTEGER,
                error_code TEXT,
                error_message TEXT
            );
            
            CREATE INDEX IF NOT EXISTS idx_feishu_outbox_outbound_id ON feishu_outbox(outbound_id);
            CREATE INDEX IF NOT EXISTS idx_feishu_outbox_status ON feishu_outbox(status);
            CREATE INDEX IF NOT EXISTS idx_feishu_outbox_job_id ON feishu_outbox(job_id);
            CREATE INDEX IF NOT EXISTS idx_feishu_outbox_next_attempt ON feishu_outbox(next_attempt_at);
            CREATE INDEX IF NOT EXISTS idx_feishu_outbox_reply_kind ON feishu_outbox(reply_kind);
            
            -- schema version tracking
            CREATE TABLE IF NOT EXISTS feishu_schema_version (
                version INTEGER PRIMARY KEY,
                applied_at INTEGER NOT NULL
            );
            "#,
        )?;
        
        // Record migration
        let version: Option<i64> = conn
            .query_row(
                "SELECT version FROM feishu_schema_version ORDER BY version DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .unwrap_or(None);
        
        let current_version = version.unwrap_or(0);
        if current_version < 2 {
            // v2: Add result_json column for /monitor results
            let has_column: i32 = conn
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('feishu_outbox') WHERE name='result_json'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            
            if has_column == 0 {
                // Add result_json column (SQLite allows ALTER TABLE ADD COLUMN)
                let _ = conn.execute("ALTER TABLE feishu_outbox ADD COLUMN result_json TEXT", []);
            }
            
            // Add reply_kind index
            let _ = conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_feishu_outbox_reply_kind ON feishu_outbox(reply_kind)",
                [],
            );
            
            conn.execute(
                "INSERT OR REPLACE INTO feishu_schema_version (version, applied_at) VALUES (2, ?)",
                params![chrono_timestamp()],
            )?;
        }
        if current_version < 1 {
            conn.execute(
                "INSERT OR REPLACE INTO feishu_schema_version (version, applied_at) VALUES (1, ?)",
                params![chrono_timestamp()],
            )?;
        }
        
        println!("[feishu-store] migrated version=2");
        
        Ok(())
    }

    /// Compute SHA256 hash of text
    pub fn hash_text(text: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        let result = hasher.finalize();
        hex::encode(&result[..16]) // First 16 bytes for shorter hash
    }

    /// Truncate text to max CHARACTERS (not bytes) for UTF-8 safety.
    /// Uses char-based truncation to avoid splitting multi-byte characters.
    pub fn truncate_preview(text: &str, max_chars: usize) -> String {
        let chars_count = text.chars().count();
        if chars_count <= max_chars {
            text.to_string()
        } else {
            text.chars().take(max_chars.saturating_sub(3)).collect::<String>() + "..."
        }
    }

    /// Redact secrets from JSON string
    pub fn redact_secrets(json_str: &str) -> String {
        if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(json_str) {
            redact_secrets_from_value(&mut value);
            value.to_string()
        } else {
            json_str.to_string()
        }
    }

    /// Insert a new event, returning Ok(()) if successful or Err(DuplicateEvent) if already exists
    pub fn insert_event(&self, event: &FeishuEventInput) -> Result<FeishuEvent, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        
        let text_hash = event.text.as_ref().map(|t| Self::hash_text(t));
        let text_preview = event.text.as_ref().map(|t| Self::truncate_preview(t, MAX_TEXT_PREVIEW_LEN));
        let metadata_json = event.metadata_json.as_ref().map(|m| Self::redact_secrets(m));
        
        conn.execute(
            r#"
            INSERT INTO feishu_events (
                event_key, channel, event_id, message_id, chat_id, user_id_hash,
                event_type, sender_type, message_type, text_hash, text_preview,
                status, skip_reason, received_at, updated_at, metadata_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            params![
                event.event_key,
                event.channel,
                event.event_id,
                event.message_id,
                event.chat_id,
                event.user_id_hash,
                event.event_type,
                event.sender_type,
                event.message_type,
                text_hash,
                text_preview,
                EventStatus::Received.as_str(),
                event.skip_reason,
                now,
                now,
                metadata_json,
            ],
        )?;
        
        let id = conn.last_insert_rowid();
        println!("[feishu-store] event_inserted event_key={}", event.event_key);
        
        Ok(FeishuEvent {
            id,
            event_key: event.event_key.clone(),
            channel: event.channel.clone(),
            event_id: event.event_id.clone(),
            message_id: event.message_id.clone(),
            chat_id: event.chat_id.clone(),
            user_id_hash: event.user_id_hash.clone(),
            event_type: event.event_type.clone(),
            sender_type: event.sender_type.clone(),
            message_type: event.message_type.clone(),
            text_hash,
            text_preview,
            status: EventStatus::Received,
            skip_reason: event.skip_reason.clone(),
            received_at: now,
            updated_at: now,
            metadata_json,
        })
    }

    /// Check if event exists (for dedupe)
    pub fn event_exists(&self, event_key: &str) -> Result<bool, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM feishu_events WHERE event_key = ?",
            params![event_key],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Update event status
    pub fn update_event_status(&self, event_key: &str, status: EventStatus) -> Result<(), StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        
        conn.execute(
            "UPDATE feishu_events SET status = ?, updated_at = ? WHERE event_key = ?",
            params![status.as_str(), now, event_key],
        )?;
        
        Ok(())
    }

    /// Insert a new job
    pub fn insert_job(&self, job: &FeishuJobInput) -> Result<FeishuJob, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        
        conn.execute(
            r#"
            INSERT INTO feishu_jobs (
                job_id, event_key, channel, mode, slash_command, status,
                attempts, max_attempts, created_at, updated_at, payload_json
            ) VALUES (?, ?, ?, ?, ?, ?, 0, 3, ?, ?, ?)
            "#,
            params![
                job.job_id,
                job.event_key,
                job.channel,
                job.mode,
                job.slash_command,
                JobStatus::Pending.as_str(),
                now,
                now,
                job.payload_json,
            ],
        )?;
        
        let id = conn.last_insert_rowid();
        println!("[feishu-store] job_inserted job_id={}", job.job_id);
        
        Ok(FeishuJob {
            id,
            job_id: job.job_id.clone(),
            event_key: job.event_key.clone(),
            channel: job.channel.clone(),
            mode: job.mode.clone(),
            slash_command: job.slash_command.clone(),
            status: JobStatus::Pending,
            attempts: 0,
            max_attempts: 3,
            next_attempt_at: None,
            locked_at: None,
            locked_by: None,
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            error_code: None,
            error_message: None,
            payload_json: job.payload_json.clone(),
        })
    }

    /// Update job status to processing
    pub fn job_start_processing(&self, job_id: &str, instance_id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        
        conn.execute(
            r#"
            UPDATE feishu_jobs SET 
                status = ?, 
                attempts = attempts + 1, 
                locked_at = ?,
                locked_by = ?,
                updated_at = ?,
                started_at = COALESCE(started_at, ?)
            WHERE job_id = ?
            "#,
            params![
                JobStatus::Processing.as_str(),
                now,
                instance_id,
                now,
                now,
                job_id,
            ],
        )?;
        
        println!("[feishu-store] job_processing job_id={}", job_id);
        Ok(())
    }

    /// Mark job as completed
    pub fn job_completed(&self, job_id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        
        conn.execute(
            r#"
            UPDATE feishu_jobs SET 
                status = ?, 
                completed_at = ?,
                updated_at = ?
            WHERE job_id = ?
            "#,
            params![
                JobStatus::Completed.as_str(),
                now,
                now,
                job_id,
            ],
        )?;
        
        println!("[feishu-store] job_completed job_id={}", job_id);
        Ok(())
    }

    /// Mark job as failed
    pub fn job_failed(&self, job_id: &str, error_code: &str, error_message: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        
        conn.execute(
            r#"
            UPDATE feishu_jobs SET 
                status = ?,
                error_code = ?,
                error_message = ?,
                updated_at = ?
            WHERE job_id = ?
            "#,
            params![
                JobStatus::Failed.as_str(),
                error_code,
                error_message,
                now,
                job_id,
            ],
        )?;
        
        println!("[feishu-store] job_failed job_id={} error_code={}", job_id, error_code);
        Ok(())
    }

    /// Get pending jobs for recovery
    pub fn get_recoverable_jobs(&self) -> Result<Vec<FeishuJob>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        let stale_threshold = now - 300; // 5 minutes
        
        let mut stmt = conn.prepare(
            r#"
            SELECT id, job_id, event_key, channel, mode, slash_command, status,
                   attempts, max_attempts, next_attempt_at, locked_at, locked_by,
                   created_at, updated_at, started_at, completed_at, error_code, 
                   error_message, payload_json
            FROM feishu_jobs
            WHERE status IN ('PENDING', 'QUEUED', 'PROCESSING')
              AND (status IN ('PENDING', 'QUEUED') OR locked_at < ?)
            ORDER BY created_at ASC
            LIMIT 100
            "#,
        )?;
        
        let jobs = stmt.query_map(params![stale_threshold], |row| {
            Ok(FeishuJob {
                id: row.get(0)?,
                job_id: row.get(1)?,
                event_key: row.get(2)?,
                channel: row.get(3)?,
                mode: row.get(4)?,
                slash_command: row.get(5)?,
                status: JobStatus::from_str(&row.get::<_, String>(6)?).unwrap_or(JobStatus::Pending),
                attempts: row.get(7)?,
                max_attempts: row.get(8)?,
                next_attempt_at: row.get(9)?,
                locked_at: row.get(10)?,
                locked_by: row.get(11)?,
                created_at: row.get(12)?,
                updated_at: row.get(13)?,
                started_at: row.get(14)?,
                completed_at: row.get(15)?,
                error_code: row.get(16)?,
                error_message: row.get(17)?,
                payload_json: row.get(18)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(jobs)
    }

    /// Insert outbound message
    pub fn insert_outbox(&self, outbox: &FeishuOutboxInput) -> Result<FeishuOutbox, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        
        // Privacy guard: truncate reply to preview only (max 120 chars)
        let reply_hash = outbox.reply.as_ref().map(|r| Self::hash_text(r));
        let reply_preview = outbox.reply.as_ref().map(|r| Self::truncate_preview(r, MAX_REPLY_PREVIEW_LEN));
        
        conn.execute(
            r#"
            INSERT INTO feishu_outbox (
                outbound_id, job_id, event_key, channel, chat_id, reply_kind, status,
                attempts, max_attempts, reply_hash, reply_preview, result_json,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, 0, 3, ?, ?, ?, ?, ?)
            "#,
            params![
                outbox.outbound_id,
                outbox.job_id,
                outbox.event_key,
                outbox.channel,
                outbox.chat_id,
                outbox.reply_kind,
                OutboxStatus::Pending.as_str(),
                reply_hash,
                reply_preview,
                outbox.result_json,
                now,
                now,
            ],
        )?;
        
        let id = conn.last_insert_rowid();
        println!("[feishu-outbox] inserted outbound_id={}", outbox.outbound_id);
        
        Ok(FeishuOutbox {
            id,
            outbound_id: outbox.outbound_id.clone(),
            job_id: outbox.job_id.clone(),
            event_key: outbox.event_key.clone(),
            channel: outbox.channel.clone(),
            chat_id: outbox.chat_id.clone(),
            reply_kind: outbox.reply_kind.clone(),
            status: OutboxStatus::Pending,
            attempts: 0,
            max_attempts: 3,
            next_attempt_at: None,
            platform_message_id: None,
            reply_hash,
            reply_preview,
            result_json: outbox.result_json.clone(),
            created_at: now,
            updated_at: now,
            sent_at: None,
            error_code: None,
            error_message: None,
        })
    }

    /// Insert outbound as abandoned (cannot retry due to privacy)
    /// Used for LLM replies that we cannot reconstruct
    pub fn insert_outbox_abandoned(&self, outbox: &FeishuOutboxInput, reason: &str) -> Result<FeishuOutbox, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        
        let reply_hash = outbox.reply.as_ref().map(|r| Self::hash_text(r));
        let reply_preview = outbox.reply.as_ref().map(|r| Self::truncate_preview(r, MAX_REPLY_PREVIEW_LEN));
        
        conn.execute(
            r#"
            INSERT INTO feishu_outbox (
                outbound_id, job_id, event_key, channel, chat_id, reply_kind, status,
                attempts, max_attempts, reply_hash, reply_preview, result_json,
                error_code, error_message, created_at, updated_at, sent_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, 0, 0, ?, ?, ?, 'privacy_no_full_body', ?, ?, ?, ?)
            "#,
            params![
                outbox.outbound_id,
                outbox.job_id,
                outbox.event_key,
                outbox.channel,
                outbox.chat_id,
                outbox.reply_kind,
                OutboxStatus::Abandoned.as_str(),
                reply_hash,
                reply_preview,
                outbox.result_json,
                reason,
                now,
                now,
                now,
            ],
        )?;
        
        let id = conn.last_insert_rowid();
        println!("[feishu-outbox] abandoned outbound_id={} reason=privacy_no_full_body", outbox.outbound_id);
        
        Ok(FeishuOutbox {
            id,
            outbound_id: outbox.outbound_id.clone(),
            job_id: outbox.job_id.clone(),
            event_key: outbox.event_key.clone(),
            channel: outbox.channel.clone(),
            chat_id: outbox.chat_id.clone(),
            reply_kind: outbox.reply_kind.clone(),
            status: OutboxStatus::Abandoned,
            attempts: 0,
            max_attempts: 0,
            next_attempt_at: None,
            platform_message_id: None,
            reply_hash,
            reply_preview,
            result_json: outbox.result_json.clone(),
            created_at: now,
            updated_at: now,
            sent_at: Some(now),
            error_code: Some("privacy_no_full_body".to_string()),
            error_message: Some(reason.to_string()),
        })
    }

    /// Mark outbound as sent
    pub fn outbox_sent(&self, outbound_id: &str, platform_message_id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        
        conn.execute(
            r#"
            UPDATE feishu_outbox SET 
                status = ?,
                platform_message_id = ?,
                sent_at = ?,
                updated_at = ?
            WHERE outbound_id = ?
            "#,
            params![
                OutboxStatus::Sent.as_str(),
                platform_message_id,
                now,
                now,
                outbound_id,
            ],
        )?;
        
        println!("[feishu-outbox] sent outbound_id={} platform_message_id_present=true", outbound_id);
        Ok(())
    }

    /// Mark outbound as sending
    pub fn outbox_sending(&self, outbound_id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        
        conn.execute(
            r#"
            UPDATE feishu_outbox SET 
                status = ?,
                updated_at = ?
            WHERE outbound_id = ?
            "#,
            params![
                OutboxStatus::Sending.as_str(),
                now,
                outbound_id,
            ],
        )?;
        
        println!("[feishu-outbox] sending outbound_id={}", outbound_id);
        Ok(())
    }

    /// Mark outbound as failed
    pub fn outbox_failed(&self, outbound_id: &str, error_code: &str, error_message: &str) -> Result<bool, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        
        // Get current attempts
        let attempts: i32 = conn.query_row(
            "SELECT attempts FROM feishu_outbox WHERE outbound_id = ?",
            params![outbound_id],
            |row| row.get(0),
        )?;
        
        let new_attempts = attempts + 1;
        let max_attempts: i32 = conn.query_row(
            "SELECT max_attempts FROM feishu_outbox WHERE outbound_id = ?",
            params![outbound_id],
            |row| row.get(0),
        )?;
        
        let (status, next_attempt) = if new_attempts >= max_attempts {
            (OutboxStatus::Dead.as_str(), None)
        } else {
            // Exponential backoff: 2^attempts seconds, max 5 minutes
            let backoff_secs = std::cmp::min(300, 2i64.pow(new_attempts as u32));
            (OutboxStatus::Failed.as_str(), Some(now + backoff_secs))
        };
        
        conn.execute(
            r#"
            UPDATE feishu_outbox SET 
                status = ?,
                attempts = ?,
                next_attempt_at = ?,
                error_code = ?,
                error_message = ?,
                updated_at = ?
            WHERE outbound_id = ?
            "#,
            params![
                status,
                new_attempts,
                next_attempt,
                error_code,
                error_message,
                now,
                outbound_id,
            ],
        )?;
        
        let retryable = next_attempt.is_some();
        println!("[feishu-outbox] failed outbound_id={} retryable={}", outbound_id, retryable);
        
        Ok(retryable)
    }

    /// Mark outbox as RETRYABLE: this is a template or restructurable reply
    /// that can be sent again on recovery.
    pub fn outbox_mark_retryable(&self, outbound_id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        
        conn.execute(
            r#"
            UPDATE feishu_outbox SET 
                status = ?,
                attempts = 0,
                next_attempt_at = ?,
                error_code = NULL,
                error_message = NULL,
                updated_at = ?
            WHERE outbound_id = ?
            "#,
            params![
                OutboxStatus::Pending.as_str(),
                now,
                now,
                outbound_id,
            ],
        )?;
        
        println!("[feishu-outbox] retryable outbound_id={}", outbound_id);
        
        Ok(())
    }

    /// Mark outbox as FAILED_PRIVACY_NO_RETRY: the reply content cannot be reconstructed
    /// due to privacy policy (e.g., free-form LLM reply).
    pub fn outbox_mark_failed_privacy(&self, outbound_id: &str, reason: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        
        conn.execute(
            r#"
            UPDATE feishu_outbox SET 
                status = ?,
                attempts = 0,
                next_attempt_at = NULL,
                error_code = 'privacy_no_full_body',
                error_message = ?,
                updated_at = ?
            WHERE outbound_id = ?
            "#,
            params![
                OutboxStatus::FailedPrivacyNoRetry.as_str(),
                reason,
                now,
                outbound_id,
            ],
        )?;
        
        println!("[feishu-outbox] privacy_no_retry outbound_id={}", outbound_id);
        
        Ok(())
    }

    /// Mark outbox as FAILED_RECONSTRUCT_INCOMPLETE because reconstruction
    /// of reply body failed (e.g., result_json missing required fields).
    /// Never pretend to retry - we cannot send a fake reply.
    pub fn outbox_mark_reconstruct_incomplete(&self, outbound_id: &str, reason: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        
        conn.execute(
            r#"
            UPDATE feishu_outbox SET 
                status = ?,
                attempts = 0,
                next_attempt_at = NULL,
                error_code = 'reconstruct_incomplete',
                error_message = ?,
                updated_at = ?
            WHERE outbound_id = ?
            "#,
            params![
                OutboxStatus::FailedReconstructIncomplete.as_str(),
                reason,
                now,
                outbound_id,
            ],
        )?;
        
        println!("[feishu-outbox] reconstruct_incomplete outbound_id={} reason={}", outbound_id, reason);
        
        Ok(())
    }

    /// Increment attempts and set status to SENDING for a retry attempt.
    /// Returns true if the retry is allowed (attempts < max_attempts), false otherwise.
    pub fn outbox_begin_retry(&self, outbound_id: &str) -> Result<bool, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        
        // Read current state
        let (attempts, max_attempts, status, sent): (i32, i32, String, Option<String>) = conn.query_row(
            "SELECT attempts, max_attempts, status, platform_message_id FROM feishu_outbox WHERE outbound_id = ?",
            params![outbound_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        
        // If already SENT with platform_message_id, do not retry
        if status == OutboxStatus::Sent.as_str() && sent.is_some() && !sent.as_deref().unwrap_or("").is_empty() && sent.as_deref() != Some(&"skipped".to_string()) {
            return Ok(false);
        }
        
        let new_attempts = attempts + 1;
        if new_attempts > max_attempts {
            // Mark as DEAD
            conn.execute(
                r#"
                UPDATE feishu_outbox SET 
                    status = ?,
                    attempts = ?,
                    next_attempt_at = NULL,
                    error_code = 'max_attempts_exceeded',
                    updated_at = ?
                WHERE outbound_id = ?
                "#,
                params![
                    OutboxStatus::Dead.as_str(),
                    new_attempts,
                    now,
                    outbound_id,
                ],
            )?;
            println!("[feishu-retry] max_attempts_exceeded outbound_id={} attempts={}", outbound_id, new_attempts);
            return Ok(false);
        }
        
        // Set to SENDING, increment attempts
        conn.execute(
            r#"
            UPDATE feishu_outbox SET 
                status = ?,
                attempts = ?,
                updated_at = ?
            WHERE outbound_id = ?
            "#,
            params![
                OutboxStatus::Sending.as_str(),
                new_attempts,
                now,
                outbound_id,
            ],
        )?;
        
        println!("[feishu-retry] sending outbound_id={} attempts={}", outbound_id, new_attempts);
        
        Ok(true)
    }

    /// Get outbox by outbound_id
    pub fn get_outbox(&self, outbound_id: &str) -> Result<Option<FeishuOutbox>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, outbound_id, job_id, event_key, channel, chat_id, reply_kind, status,
                   attempts, max_attempts, next_attempt_at, platform_message_id, reply_hash,
                   reply_preview, result_json, created_at, updated_at, sent_at, error_code, error_message
            FROM feishu_outbox
            WHERE outbound_id = ?
            "#,
        )?;
        
        let outbox = stmt.query_row(params![outbound_id], |row| {
            Ok(FeishuOutbox {
                id: row.get(0)?,
                outbound_id: row.get(1)?,
                job_id: row.get(2)?,
                event_key: row.get(3)?,
                channel: row.get(4)?,
                chat_id: row.get(5)?,
                reply_kind: row.get(6)?,
                status: OutboxStatus::from_str(&row.get::<_, String>(7)?).unwrap_or(OutboxStatus::Pending),
                attempts: row.get(8)?,
                max_attempts: row.get(9)?,
                next_attempt_at: row.get(10)?,
                platform_message_id: row.get(11)?,
                reply_hash: row.get(12)?,
                reply_preview: row.get(13)?,
                result_json: row.get(14)?,
                created_at: row.get(15)?,
                updated_at: row.get(16)?,
                sent_at: row.get(17)?,
                error_code: row.get(18)?,
                error_message: row.get(19)?,
            })
        }).optional()?;
        
        Ok(outbox)
    }

    /// Get pending outbox items for recovery
    pub fn get_recoverable_outbox(&self) -> Result<Vec<FeishuOutbox>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        
        let mut stmt = conn.prepare(
            r#"
            SELECT id, outbound_id, job_id, event_key, channel, chat_id, reply_kind, status,
                   attempts, max_attempts, next_attempt_at, platform_message_id, reply_hash,
                   reply_preview, result_json, created_at, updated_at, sent_at, error_code, error_message
            FROM feishu_outbox
            WHERE status IN ('PENDING', 'FAILED', 'SENDING')
              AND (status != 'SENDING' OR updated_at < ?)
              AND (status IN ('PENDING', 'FAILED') OR next_attempt_at IS NULL OR next_attempt_at <= ?)
            ORDER BY created_at ASC
            LIMIT 100
            "#,
        )?;
        
        let outbox_items = stmt.query_map(params![now - 60, now], |row| {
            Ok(FeishuOutbox {
                id: row.get(0)?,
                outbound_id: row.get(1)?,
                job_id: row.get(2)?,
                event_key: row.get(3)?,
                channel: row.get(4)?,
                chat_id: row.get(5)?,
                reply_kind: row.get(6)?,
                status: OutboxStatus::from_str(&row.get::<_, String>(7)?).unwrap_or(OutboxStatus::Pending),
                attempts: row.get(8)?,
                max_attempts: row.get(9)?,
                next_attempt_at: row.get(10)?,
                platform_message_id: row.get(11)?,
                reply_hash: row.get(12)?,
                reply_preview: row.get(13)?,
                result_json: row.get(14)?,
                created_at: row.get(15)?,
                updated_at: row.get(16)?,
                sent_at: row.get(17)?,
                error_code: row.get(18)?,
                error_message: row.get(19)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(outbox_items)
    }

    /// Get job by job_id
    pub fn get_job(&self, job_id: &str) -> Result<Option<FeishuJob>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        
        let job = conn.query_row(
            r#"
            SELECT id, job_id, event_key, channel, mode, slash_command, status,
                   attempts, max_attempts, next_attempt_at, locked_at, locked_by,
                   created_at, updated_at, started_at, completed_at, error_code, 
                   error_message, payload_json
            FROM feishu_jobs WHERE job_id = ?
            "#,
            params![job_id],
            |row| {
                Ok(FeishuJob {
                    id: row.get(0)?,
                    job_id: row.get(1)?,
                    event_key: row.get(2)?,
                    channel: row.get(3)?,
                    mode: row.get(4)?,
                    slash_command: row.get(5)?,
                    status: JobStatus::from_str(&row.get::<_, String>(6)?).unwrap_or(JobStatus::Pending),
                    attempts: row.get(7)?,
                    max_attempts: row.get(8)?,
                    next_attempt_at: row.get(9)?,
                    locked_at: row.get(10)?,
                    locked_by: row.get(11)?,
                    created_at: row.get(12)?,
                    updated_at: row.get(13)?,
                    started_at: row.get(14)?,
                    completed_at: row.get(15)?,
                    error_code: row.get(16)?,
                    error_message: row.get(17)?,
                    payload_json: row.get(18)?,
                })
            },
        ).optional()?;
        
        Ok(job)
    }

    /// Get the latest job for an event_key (most recent one by created_at).
    /// Used to check job status on duplicate events.
    pub fn get_job_by_event_key(&self, event_key: &str) -> Result<Option<FeishuJob>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        
        let job = conn.query_row(
            r#"
            SELECT id, job_id, event_key, channel, mode, slash_command, status,
                   attempts, max_attempts, next_attempt_at, locked_at, locked_by,
                   created_at, updated_at, started_at, completed_at, error_code, 
                   error_message, payload_json
            FROM feishu_jobs WHERE event_key = ?
            ORDER BY created_at DESC LIMIT 1
            "#,
            params![event_key],
            |row| {
                Ok(FeishuJob {
                    id: row.get(0)?,
                    job_id: row.get(1)?,
                    event_key: row.get(2)?,
                    channel: row.get(3)?,
                    mode: row.get(4)?,
                    slash_command: row.get(5)?,
                    status: JobStatus::from_str(&row.get::<_, String>(6)?).unwrap_or(JobStatus::Pending),
                    attempts: row.get(7)?,
                    max_attempts: row.get(8)?,
                    next_attempt_at: row.get(9)?,
                    locked_at: row.get(10)?,
                    locked_by: row.get(11)?,
                    created_at: row.get(12)?,
                    updated_at: row.get(13)?,
                    started_at: row.get(14)?,
                    completed_at: row.get(15)?,
                    error_code: row.get(16)?,
                    error_message: row.get(17)?,
                    payload_json: row.get(18)?,
                })
            },
        ).optional()?;
        
        Ok(job)
    }
    
    /// Mark outbound as abandoned during recovery (no reply content to resend)
    pub fn outbox_abandon(&self, outbound_id: &str, reason: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        
        conn.execute(
            r#"
            UPDATE feishu_outbox SET 
                status = ?,
                error_code = ?,
                error_message = ?,
                updated_at = ?
            WHERE outbound_id = ?
            "#,
            params![
                OutboxStatus::Abandoned.as_str(),
                "recovery_abandoned",
                reason,
                now,
                outbound_id,
            ],
        )?;
        
        println!("[feishu-outbox] abandoned outbound_id={} reason={}", outbound_id, reason);
        Ok(())
    }
    
    /// Mark job as dead during recovery
    pub fn job_abandon(&self, job_id: &str, reason: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        let now = chrono_timestamp();
        
        conn.execute(
            r#"
            UPDATE feishu_jobs SET 
                status = ?,
                error_code = ?,
                error_message = ?,
                updated_at = ?
            WHERE job_id = ?
            "#,
            params![
                JobStatus::Dead.as_str(),
                "recovery_abandoned",
                reason,
                now,
                job_id,
            ],
        )?;
        
        println!("[feishu-store] job_abandoned job_id={} reason={}", job_id, reason);
        Ok(())
    }
}

/// Input for creating a new event
#[derive(Debug, Clone)]
pub struct FeishuEventInput {
    pub event_key: String,
    pub channel: String,
    pub event_id: Option<String>,
    pub message_id: Option<String>,
    pub chat_id: Option<String>,
    pub user_id_hash: Option<String>,
    pub event_type: Option<String>,
    pub sender_type: Option<String>,
    pub message_type: Option<String>,
    pub text: Option<String>,
    pub skip_reason: Option<String>,
    pub metadata_json: Option<String>,
}

/// Input for creating a new job
#[derive(Debug, Clone)]
pub struct FeishuJobInput {
    pub job_id: String,
    pub event_key: String,
    pub channel: String,
    pub mode: String,
    pub slash_command: Option<String>,
    pub payload_json: Option<String>,
}

/// Input for creating a new outbox entry
#[derive(Debug, Clone)]
pub struct FeishuOutboxInput {
    pub outbound_id: String,
    pub job_id: Option<String>,
    pub event_key: Option<String>,
    pub channel: String,
    pub chat_id: Option<String>,
    pub reply_kind: Option<String>,
    /// Optional reply preview (max 120 chars). Full reply is NEVER stored.
    pub reply: Option<String>,
    /// Optional structured result data for retryable replies
    /// (e.g., /monitor results that can be reconstructed)
    pub result_json: Option<String>,
}

/// Get current timestamp (Unix milliseconds)
/// Get current timestamp in milliseconds
pub fn chrono_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Recursively redact secrets from JSON value
fn redact_secrets_from_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                let key_lower = key.to_lowercase();
                if SECRET_KEYS.iter().any(|s| key_lower.contains(s)) {
                    *val = serde_json::Value::String("[REDACTED]".to_string());
                } else {
                    redact_secrets_from_value(val);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                redact_secrets_from_value(item);
            }
        }
        _ => {}
    }
}

// ============================================================================
// CLI Query Methods (for omninova feishu CLI commands)
// ============================================================================

impl FeishuStore {
    /// Get a summary of store statistics for CLI status command.
    pub fn get_store_stats(&self) -> Result<FeishuStoreStats, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        
        // Get counts
        let events_total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM feishu_events", [], |row| row.get(0)
        ).unwrap_or(0);
        
        let jobs_total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM feishu_jobs", [], |row| row.get(0)
        ).unwrap_or(0);
        
        let outbox_total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM feishu_outbox", [], |row| row.get(0)
        ).unwrap_or(0);
        
        // Get jobs by status
        let mut job_status_counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        let mut stmt = conn.prepare("SELECT status, COUNT(*) FROM feishu_jobs GROUP BY status")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            if let Ok((status, count)) = row {
                job_status_counts.insert(status, count);
            }
        }
        
        // Get outbox by status
        let mut outbox_status_counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        let mut stmt = conn.prepare("SELECT status, COUNT(*) FROM feishu_outbox GROUP BY status")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            if let Ok((status, count)) = row {
                outbox_status_counts.insert(status, count);
            }
        }
        
        // Get last event time
        let last_event_at: Option<i64> = conn.query_row(
            "SELECT MAX(received_at) FROM feishu_events", [], |row| row.get(0)
        ).optional()?.flatten();
        
        // Get error count (jobs with error_code not null)
        let error_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM feishu_jobs WHERE error_code IS NOT NULL", [], |row| row.get(0)
        ).unwrap_or(0);
        
        Ok(FeishuStoreStats {
            events_total,
            jobs_total,
            outbox_total,
            job_status_counts,
            outbox_status_counts,
            last_event_at,
            error_count,
        })
    }
    
    /// Get recent events for CLI listing.
    pub fn get_recent_events(&self, limit: usize) -> Result<Vec<FeishuEvent>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        
        let mut stmt = conn.prepare(
            r#"
            SELECT id, event_key, channel, event_id, message_id, chat_id, user_id_hash,
                   event_type, sender_type, message_type, text_hash, text_preview, status,
                   skip_reason, received_at, updated_at, metadata_json
            FROM feishu_events
            ORDER BY received_at DESC
            LIMIT ?
            "#,
        )?;
        
        let events = stmt.query_map(params![limit as i64], |row| {
            Ok(FeishuEvent {
                id: row.get(0)?,
                event_key: row.get(1)?,
                channel: row.get(2)?,
                event_id: row.get(3)?,
                message_id: row.get(4)?,
                chat_id: row.get(5)?,
                user_id_hash: row.get(6)?,
                event_type: row.get(7)?,
                sender_type: row.get(8)?,
                message_type: row.get(9)?,
                text_hash: row.get(10)?,
                text_preview: row.get(11)?,
                status: EventStatus::from_str(&row.get::<_, String>(12)?).unwrap_or(EventStatus::Received),
                skip_reason: row.get(13)?,
                received_at: row.get(14)?,
                updated_at: row.get(15)?,
                metadata_json: row.get(16)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(events)
    }
    
    /// Get recent jobs for CLI listing.
    pub fn get_recent_jobs(&self, limit: usize) -> Result<Vec<FeishuJob>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        
        let mut stmt = conn.prepare(
            r#"
            SELECT id, job_id, event_key, channel, mode, slash_command, status,
                   attempts, max_attempts, next_attempt_at, locked_at, locked_by,
                   created_at, updated_at, started_at, completed_at, error_code, 
                   error_message, payload_json
            FROM feishu_jobs
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )?;
        
        let jobs = stmt.query_map(params![limit as i64], |row| {
            Ok(FeishuJob {
                id: row.get(0)?,
                job_id: row.get(1)?,
                event_key: row.get(2)?,
                channel: row.get(3)?,
                mode: row.get(4)?,
                slash_command: row.get(5)?,
                status: JobStatus::from_str(&row.get::<_, String>(6)?).unwrap_or(JobStatus::Pending),
                attempts: row.get(7)?,
                max_attempts: row.get(8)?,
                next_attempt_at: row.get(9)?,
                locked_at: row.get(10)?,
                locked_by: row.get(11)?,
                created_at: row.get(12)?,
                updated_at: row.get(13)?,
                started_at: row.get(14)?,
                completed_at: row.get(15)?,
                error_code: row.get(16)?,
                error_message: row.get(17)?,
                payload_json: row.get(18)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(jobs)
    }
    
    /// Get recent outbox items for CLI listing.
    pub fn get_recent_outbox(&self, limit: usize) -> Result<Vec<FeishuOutbox>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        
        let mut stmt = conn.prepare(
            r#"
            SELECT id, outbound_id, job_id, event_key, channel, chat_id, reply_kind, status,
                   attempts, max_attempts, next_attempt_at, platform_message_id, reply_hash,
                   reply_preview, result_json, created_at, updated_at, sent_at, error_code, error_message
            FROM feishu_outbox
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )?;
        
        let outbox_items = stmt.query_map(params![limit as i64], |row| {
            Ok(FeishuOutbox {
                id: row.get(0)?,
                outbound_id: row.get(1)?,
                job_id: row.get(2)?,
                event_key: row.get(3)?,
                channel: row.get(4)?,
                chat_id: row.get(5)?,
                reply_kind: row.get(6)?,
                status: OutboxStatus::from_str(&row.get::<_, String>(7)?).unwrap_or(OutboxStatus::Pending),
                attempts: row.get(8)?,
                max_attempts: row.get(9)?,
                next_attempt_at: row.get(10)?,
                platform_message_id: row.get(11)?,
                reply_hash: row.get(12)?,
                reply_preview: row.get(13)?,
                result_json: row.get(14)?,
                created_at: row.get(15)?,
                updated_at: row.get(16)?,
                sent_at: row.get(17)?,
                error_code: row.get(18)?,
                error_message: row.get(19)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(outbox_items)
    }
    
    /// Get event by event_key.
    pub fn get_event_by_key(&self, event_key: &str) -> Result<Option<FeishuEvent>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        
        let event = conn.query_row(
            r#"
            SELECT id, event_key, channel, event_id, message_id, chat_id, user_id_hash,
                   event_type, sender_type, message_type, text_hash, text_preview, status,
                   skip_reason, received_at, updated_at, metadata_json
            FROM feishu_events WHERE event_key = ?
            "#,
            params![event_key],
            |row| {
                Ok(FeishuEvent {
                    id: row.get(0)?,
                    event_key: row.get(1)?,
                    channel: row.get(2)?,
                    event_id: row.get(3)?,
                    message_id: row.get(4)?,
                    chat_id: row.get(5)?,
                    user_id_hash: row.get(6)?,
                    event_type: row.get(7)?,
                    sender_type: row.get(8)?,
                    message_type: row.get(9)?,
                    text_hash: row.get(10)?,
                    text_preview: row.get(11)?,
                    status: EventStatus::from_str(&row.get::<_, String>(12)?).unwrap_or(EventStatus::Received),
                    skip_reason: row.get(13)?,
                    received_at: row.get(14)?,
                    updated_at: row.get(15)?,
                    metadata_json: row.get(16)?,
                })
            },
        ).optional()?;
        
        Ok(event)
    }
    
    /// Get migration version.
    pub fn get_migration_version(&self) -> Result<i64, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::PoisonedLock)?;
        
        let version: i64 = conn.query_row(
            "SELECT MAX(version) FROM feishu_schema_version", [], |row| row.get(0)
        ).optional()?.unwrap_or(0);
        
        Ok(version)
    }
}

/// Statistics summary for CLI status command
#[derive(Debug, Clone)]
pub struct FeishuStoreStats {
    pub events_total: i64,
    pub jobs_total: i64,
    pub outbox_total: i64,
    pub job_status_counts: std::collections::HashMap<String, i64>,
    pub outbox_status_counts: std::collections::HashMap<String, i64>,
    pub last_event_at: Option<i64>,
    pub error_count: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    
    fn create_test_store() -> Arc<FeishuStore> {
        let temp_dir = std::env::temp_dir().join(format!("feishu_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        
        let store = FeishuStore::open(&temp_dir).unwrap();
        Arc::new(store)
    }
    
    #[test]
    fn test_store_open_creates_tables() {
        let store = create_test_store();
        // Check that a new event can be inserted (tables exist)
        let input = FeishuEventInput {
            event_key: "test_key".to_string(),
            channel: "feishu".to_string(),
            event_id: None,
            message_id: None,
            chat_id: None,
            user_id_hash: None,
            event_type: None,
            sender_type: None,
            message_type: None,
            text: None,
            skip_reason: None,
            metadata_json: None,
        };
        let result = store.insert_event(&input);
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_insert_event_success() {
        let store = create_test_store();
        
        let input = FeishuEventInput {
            event_key: "test_event_1".to_string(),
            channel: "feishu".to_string(),
            event_id: Some("evt_123".to_string()),
            message_id: Some("msg_456".to_string()),
            chat_id: Some("chat_789".to_string()),
            user_id_hash: Some("hash_abc".to_string()),
            event_type: Some("im.message.receive_v1".to_string()),
            sender_type: Some("user".to_string()),
            message_type: Some("text".to_string()),
            text: Some("Hello world".to_string()),
            skip_reason: None,
            metadata_json: Some(r#"{"key": "value"}"#.to_string()),
        };
        
        let event = store.insert_event(&input).unwrap();
        assert_eq!(event.event_key, "test_event_1");
        assert!(event.text_preview.is_some());
        assert!(event.text_hash.is_some());
    }
    
    #[test]
    fn test_duplicate_event_rejected() {
        let store = create_test_store();
        
        let input = FeishuEventInput {
            event_key: "duplicate_test".to_string(),
            channel: "feishu".to_string(),
            event_id: None,
            message_id: None,
            chat_id: None,
            user_id_hash: None,
            event_type: None,
            sender_type: None,
            message_type: None,
            text: None,
            skip_reason: None,
            metadata_json: None,
        };
        
        store.insert_event(&input).unwrap();
        
        let result = store.insert_event(&input);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_insert_job() {
        let store = create_test_store();
        
        // First insert event
        let event_input = FeishuEventInput {
            event_key: "job_test".to_string(),
            channel: "feishu".to_string(),
            event_id: None,
            message_id: None,
            chat_id: None,
            user_id_hash: None,
            event_type: None,
            sender_type: None,
            message_type: None,
            text: None,
            skip_reason: None,
            metadata_json: None,
        };
        store.insert_event(&event_input).unwrap();
        
        let job_input = FeishuJobInput {
            job_id: "job_123".to_string(),
            event_key: "job_test".to_string(),
            channel: "feishu".to_string(),
            mode: "tool".to_string(),
            slash_command: Some("/monitor".to_string()),
            payload_json: None,
        };
        
        let job = store.insert_job(&job_input).unwrap();
        assert_eq!(job.job_id, "job_123");
        assert_eq!(job.status, JobStatus::Pending);
    }
    
    #[test]
    fn test_job_status_transitions() {
        let store = create_test_store();
        
        // Insert event and job
        let event_input = FeishuEventInput {
            event_key: "status_test".to_string(),
            channel: "feishu".to_string(),
            event_id: None,
            message_id: None,
            chat_id: None,
            user_id_hash: None,
            event_type: None,
            sender_type: None,
            message_type: None,
            text: None,
            skip_reason: None,
            metadata_json: None,
        };
        store.insert_event(&event_input).unwrap();
        
        let job_input = FeishuJobInput {
            job_id: "status_job".to_string(),
            event_key: "status_test".to_string(),
            channel: "feishu".to_string(),
            mode: "tool".to_string(),
            slash_command: None,
            payload_json: None,
        };
        store.insert_job(&job_input).unwrap();
        
        // Start processing
        store.job_start_processing("status_job", "instance_1").unwrap();
        let job = store.get_job("status_job").unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Processing);
        assert_eq!(job.attempts, 1);
        
        // Complete
        store.job_completed("status_job").unwrap();
        let job = store.get_job("status_job").unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Completed);
    }
    
    #[test]
    fn test_outbox_insert_and_sent() {
        let store = create_test_store();
        
        let outbox_input = FeishuOutboxInput {
            outbound_id: "out_123".to_string(),
            job_id: Some("job_456".to_string()),
            event_key: Some("evt_789".to_string()),
            channel: "feishu".to_string(),
            chat_id: Some("chat_abc".to_string()),
            reply_kind: Some("reply".to_string()),
            reply: Some("Hello from Feishu!".to_string()),
            result_json: None,
        };
        
        let outbox = store.insert_outbox(&outbox_input).unwrap();
        assert_eq!(outbox.outbound_id, "out_123");
        assert!(outbox.reply_preview.is_some());
        
        store.outbox_sent("out_123", "feishu_msg_123").unwrap();
        
        // Check it's now SENT
        let outboxes = store.get_recoverable_outbox().unwrap();
        assert!(!outboxes.iter().any(|o| o.outbound_id == "out_123"));
    }
    
    #[test]
    fn test_outbox_retry_with_backoff() {
        let store = create_test_store();
        
        let outbox_input = FeishuOutboxInput {
            outbound_id: "out_fail".to_string(),
            job_id: None,
            event_key: None,
            channel: "feishu".to_string(),
            chat_id: None,
            reply_kind: None,
            reply: Some("Test reply".to_string()),
            result_json: None,
        };
        
        store.insert_outbox(&outbox_input).unwrap();
        
        // First failure
        let retryable = store.outbox_failed("out_fail", "network_error", "Connection failed").unwrap();
        assert!(retryable);
        
        // Second failure (should be dead after 3 attempts)
        store.outbox_failed("out_fail", "network_error", "Still failing").unwrap();
        let retryable = store.outbox_failed("out_fail", "network_error", "Final").unwrap();
        assert!(!retryable); // Now dead
    }
    
    #[test]
    fn test_text_preview_truncation() {
        // ASCII test
        let long_text = "A".repeat(200);
        let preview = FeishuStore::truncate_preview(&long_text, 80);
        assert_eq!(preview.len(), 80);
        assert!(preview.ends_with("..."));
    }
    
    #[test]
    fn test_chinese_text_truncation() {
        // Chinese characters (3 bytes each in UTF-8)
        let chinese = "你好世界这是一个测试".repeat(10);
        let preview = FeishuStore::truncate_preview(&chinese, 80);
        // Should NOT panic and should be at most 80 chars + "..."
        let char_count = preview.chars().count();
        assert!(char_count <= 83); // 80 chars + "..."
        assert!(preview.ends_with("...") || preview.chars().count() <= 80);
    }
    
    #[test]
    fn test_emoji_truncation() {
        // Emoji characters (4 bytes each in UTF-8)
        let emoji = "🎉🎊🎈".repeat(30);
        let preview = FeishuStore::truncate_preview(&emoji, 80);
        // Should NOT panic
        let char_count = preview.chars().count();
        assert!(char_count <= 83);
    }
    
    #[test]
    fn test_mixed_cjk_truncation() {
        // Mixed Chinese + English + Emoji
        let mixed = "你好hello🎉world世界test123emoji😀中文测试".repeat(10);
        let preview = FeishuStore::truncate_preview(&mixed, 80);
        // Should NOT panic
        let char_count = preview.chars().count();
        assert!(char_count <= 83);
        assert!(preview.ends_with("...") || preview.chars().count() <= 80);
    }
    
    #[test]
    fn test_text_preview_exact_boundary() {
        // Exactly 80 characters - should not truncate
        let text = "A".repeat(80);
        let preview = FeishuStore::truncate_preview(&text, 80);
        assert_eq!(preview.len(), 80);
        assert!(!preview.ends_with("..."));
    }
    
    #[test]
    fn test_text_preview_under_limit() {
        // Under 80 chars - should not truncate
        let text = "短文本";
        let preview = FeishuStore::truncate_preview(&text, 80);
        assert_eq!(preview, text);
    }
    
    #[test]
    fn test_reply_preview_120_chars() {
        // reply_preview should be max 120 chars
        let long = "A".repeat(200);
        let preview = FeishuStore::truncate_preview(&long, 120);
        let char_count = preview.chars().count();
        assert!(char_count <= 123); // 120 + "..."
        assert!(preview.ends_with("..."));
    }
    
    #[test]
    fn test_reply_preview_chinese() {
        // Chinese text for reply_preview
        let chinese = "这是一段很长的中文文本用于测试预览截断功能是否正常工作".repeat(5);
        let preview = FeishuStore::truncate_preview(&chinese, 120);
        // Should NOT panic
        let char_count = preview.chars().count();
        assert!(char_count <= 123);
    }
    
    #[test]
    fn test_secret_redaction() {
        let json = r#"{
            "app_secret": "my_secret_key",
            "tenant_access_token": "token123",
            "normal_key": "visible_value"
        }"#;
        
        let redacted = FeishuStore::redact_secrets(json);
        
        assert!(redacted.contains("visible_value"));
        assert!(redacted.contains("[REDACTED]"));
        assert!(!redacted.contains("my_secret_key"));
        assert!(!redacted.contains("token123"));
    }
    
    #[test]
    fn test_recovery_jobs() {
        let store = create_test_store();
        
        // Insert event
        let event_input = FeishuEventInput {
            event_key: "recovery_test".to_string(),
            channel: "feishu".to_string(),
            event_id: None,
            message_id: None,
            chat_id: None,
            user_id_hash: None,
            event_type: None,
            sender_type: None,
            message_type: None,
            text: None,
            skip_reason: None,
            metadata_json: None,
        };
        store.insert_event(&event_input).unwrap();
        
        // Insert job
        let job_input = FeishuJobInput {
            job_id: "recovery_job".to_string(),
            event_key: "recovery_test".to_string(),
            channel: "feishu".to_string(),
            mode: "tool".to_string(),
            slash_command: None,
            payload_json: None,
        };
        store.insert_job(&job_input).unwrap();
        
        let jobs = store.get_recoverable_jobs().unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].job_id, "recovery_job");
    }
    
    #[test]
    fn test_outbox_sending_transitions() {
        let store = create_test_store();
        
        let outbox_input = FeishuOutboxInput {
            outbound_id: "out_sending".to_string(),
            job_id: Some("job_123".to_string()),
            event_key: Some("evt_456".to_string()),
            channel: "feishu".to_string(),
            chat_id: Some("chat_789".to_string()),
            reply_kind: Some("final".to_string()),
            reply: Some("Test reply".to_string()),
            result_json: None,
        };
        
        let outbox = store.insert_outbox(&outbox_input).unwrap();
        assert_eq!(outbox.status, OutboxStatus::Pending);
        
        // Update to sending
        store.outbox_sending("out_sending").unwrap();
        
        // Should not appear in recoverable (only PENDING/FAILED appear)
        let recoverable = store.get_recoverable_outbox().unwrap();
        assert!(!recoverable.iter().any(|o| o.outbound_id == "out_sending"));
    }
    
    #[test]
    fn test_outbox_abandon_on_recovery() {
        let store = create_test_store();
        
        // Insert outbox without reply (privacy mode)
        let outbox_input = FeishuOutboxInput {
            outbound_id: "out_no_reply".to_string(),
            job_id: Some("job_123".to_string()),
            event_key: Some("evt_456".to_string()),
            channel: "feishu".to_string(),
            chat_id: Some("chat_789".to_string()),
            reply_kind: Some("final".to_string()),
            reply: None, // No reply stored
            result_json: None,
        };
        
        store.insert_outbox(&outbox_input).unwrap();
        
        // Abandon during recovery
        store.outbox_abandon("out_no_reply", "no_reply_content_for_privacy").unwrap();
        
        // Should be dead now
        let recoverable = store.get_recoverable_outbox().unwrap();
        assert!(!recoverable.iter().any(|o| o.outbound_id == "out_no_reply"));
    }
    
    #[test]
    fn test_job_abandon_on_recovery() {
        let store = create_test_store();
        
        // Insert event
        let event_input = FeishuEventInput {
            event_key: "abandon_test".to_string(),
            channel: "feishu".to_string(),
            event_id: None,
            message_id: None,
            chat_id: None,
            user_id_hash: None,
            event_type: None,
            sender_type: None,
            message_type: None,
            text: None,
            skip_reason: None,
            metadata_json: None,
        };
        store.insert_event(&event_input).unwrap();
        
        // Insert job without payload
        let job_input = FeishuJobInput {
            job_id: "abandon_job".to_string(),
            event_key: "abandon_test".to_string(),
            channel: "feishu".to_string(),
            mode: "tool".to_string(),
            slash_command: None,
            payload_json: None, // No payload - can't recover
        };
        store.insert_job(&job_input).unwrap();
        
        // Abandon during recovery
        store.job_abandon("abandon_job", "missing_payload").unwrap();
        
        // Should be dead now
        let jobs = store.get_recoverable_jobs().unwrap();
        assert!(!jobs.iter().any(|j| j.job_id == "abandon_job"));
    }
    
    #[test]
    fn test_job_recovery_with_valid_payload() {
        let store = create_test_store();
        
        // Insert event
        let event_input = FeishuEventInput {
            event_key: "payload_test".to_string(),
            channel: "feishu".to_string(),
            event_id: Some("evt_123".to_string()),
            message_id: Some("msg_456".to_string()),
            chat_id: Some("chat_789".to_string()),
            user_id_hash: None,
            event_type: Some("im.message.receive_v1".to_string()),
            sender_type: Some("user".to_string()),
            message_type: Some("text".to_string()),
            text: Some("hello".to_string()),
            skip_reason: None,
            metadata_json: None,
        };
        store.insert_event(&event_input).unwrap();
        
        // Insert job with valid payload
        let payload = serde_json::json!({
            "schema": "2.0",
            "header": {
                "event_id": "evt_123",
                "event_type": "im.message.receive_v1"
            },
            "event": {
                "sender": {
                    "sender_type": "user",
                    "sender_id": {
                        "user_id": "user_123"
                    }
                },
                "message": {
                    "message_id": "msg_456",
                    "chat_id": "chat_789",
                    "message_type": "text",
                    "content": "{\"text\":\"hello\"}"
                }
            }
        });
        
        let job_input = FeishuJobInput {
            job_id: "payload_job".to_string(),
            event_key: "payload_test".to_string(),
            channel: "feishu".to_string(),
            mode: "tool".to_string(),
            slash_command: Some("/monitor".to_string()),
            payload_json: Some(serde_json::to_string(&payload).unwrap()),
        };
        store.insert_job(&job_input).unwrap();
        
        // Should be recoverable
        let jobs = store.get_recoverable_jobs().unwrap();
        assert!(jobs.iter().any(|j| j.job_id == "payload_job"));
        assert!(jobs[0].payload_json.is_some());
    }
    
    // ========== Privacy-first outbox semantics tests (v0.8.7.5.1) ==========
    
    #[test]
    fn test_reply_kind_retryable_classification() {
        // Template replies are retryable
        assert!(ReplyKind::Progress.is_retryable());
        assert!(ReplyKind::Timeout.is_retryable());
        assert!(ReplyKind::Failure.is_retryable());
        assert!(ReplyKind::ChatOnlyBlocked.is_retryable());
        assert!(ReplyKind::Unsupported.is_retryable());
        // Monitor final is retryable (reconstructible from result_json)
        assert!(ReplyKind::MonitorFinal.is_retryable());
        // LLM final is NOT retryable due to privacy
        assert!(!ReplyKind::LlmFinal.is_retryable());
    }
    
    #[test]
    fn test_llm_reply_does_not_store_full_body() {
        let store = create_test_store();
        
        // Insert outbox for an LLM final reply - we store preview only
        let long_reply = "A".repeat(500);
        let outbox_input = FeishuOutboxInput {
            outbound_id: "out_llm".to_string(),
            job_id: None,
            event_key: None,
            channel: "feishu".to_string(),
            chat_id: Some("chat_x".to_string()),
            reply_kind: Some("llm_final".to_string()),
            reply: Some(long_reply.clone()),
            result_json: None,
        };
        
        let outbox = store.insert_outbox(&outbox_input).unwrap();
        
        // Preview must be truncated to MAX_REPLY_PREVIEW_LEN (120)
        assert!(outbox.reply_preview.is_some());
        assert!(outbox.reply_preview.unwrap().len() <= 120);
        
        // Hash should be present (SHA256 first 16 bytes)
        assert!(outbox.reply_hash.is_some());
    }
    
    #[test]
    fn test_outbox_abandoned_status_for_llm_final() {
        let store = create_test_store();
        
        // Insert as abandoned (audit-only)
        let outbox_input = FeishuOutboxInput {
            outbound_id: "out_audit".to_string(),
            job_id: None,
            event_key: None,
            channel: "feishu".to_string(),
            chat_id: Some("chat_x".to_string()),
            reply_kind: Some("llm_final".to_string()),
            reply: Some("Long LLM reply that should not be stored...".to_string()),
            result_json: None,
        };
        
        let outbox = store.insert_outbox_abandoned(&outbox_input, "full_reply_not_stored_for_privacy").unwrap();
        
        assert_eq!(outbox.status, OutboxStatus::Abandoned);
        assert_eq!(outbox.error_code, Some("privacy_no_full_body".to_string()));
    }
    
    #[test]
    fn test_outbox_mark_retryable() {
        let store = create_test_store();
        
        let outbox_input = FeishuOutboxInput {
            outbound_id: "out_retry".to_string(),
            job_id: None,
            event_key: None,
            channel: "feishu".to_string(),
            chat_id: Some("chat_x".to_string()),
            reply_kind: Some("timeout_reply".to_string()),
            reply: None,
            result_json: None,
        };
        store.insert_outbox(&outbox_input).unwrap();
        
        // Mark as retryable
        store.outbox_mark_retryable("out_retry").unwrap();
        
        // Should be PENDING again
        let recovered = store.get_outbox("out_retry").unwrap().unwrap();
        assert_eq!(recovered.status, OutboxStatus::Pending);
        assert_eq!(recovered.attempts, 0);
    }
    
    #[test]
    fn test_outbox_failed_privacy_no_retry() {
        let store = create_test_store();
        
        let outbox_input = FeishuOutboxInput {
            outbound_id: "out_privacy".to_string(),
            job_id: None,
            event_key: None,
            channel: "feishu".to_string(),
            chat_id: Some("chat_x".to_string()),
            reply_kind: Some("llm_final".to_string()),
            reply: None,
            result_json: None,
        };
        store.insert_outbox(&outbox_input).unwrap();
        
        // Mark as privacy-no-retry
        store.outbox_mark_failed_privacy("out_privacy", "llm_reply_not_reconstructible").unwrap();
        
        let outbox = store.get_outbox("out_privacy").unwrap().unwrap();
        assert_eq!(outbox.status, OutboxStatus::FailedPrivacyNoRetry);
        assert_eq!(outbox.error_code, Some("privacy_no_full_body".to_string()));
    }
    
    #[test]
    fn test_template_reply_reconstructible() {
        use crate::gateway::feishu_worker;
        
        // All template kinds should produce Some(reply) regardless of result_json
        let kinds = [
            ReplyKind::Progress,
            ReplyKind::Timeout,
            ReplyKind::Failure,
            ReplyKind::ChatOnlyBlocked,
            ReplyKind::Unsupported,
        ];
        
        for kind in kinds.iter() {
            let reply = feishu_worker::reconstruct_reply(*kind, None, None);
            assert!(reply.is_some(), "Template {:?} should be reconstructible", kind);
        }
    }
    
    #[test]
    fn test_monitor_final_reconstructible_from_result_json() {
        use crate::gateway::feishu_worker;
        
        let result_json = serde_json::json!({
            "duration_secs": 30,
            "changed": true,
            "start_path": "C:\\path\\to\\start.png",
            "end_path": "C:\\path\\to\\end.png"
        });
        let json_str = serde_json::to_string(&result_json).unwrap();
        
        let reply = feishu_worker::reconstruct_reply(ReplyKind::MonitorFinal, Some(&json_str), None);
        assert!(reply.is_some());
        let reply = reply.unwrap();
        assert!(reply.contains("桌面监控完成"));
        assert!(reply.contains("30"));
        assert!(reply.contains("有变化"));
    }
    
    #[test]
    fn test_monitor_final_without_result_json_cannot_reconstruct() {
        use crate::gateway::feishu_worker;
        
        // Without result_json, monitor final cannot be reconstructed
        let reply = feishu_worker::reconstruct_reply(ReplyKind::MonitorFinal, None, None);
        assert!(reply.is_none());
    }
    
    #[test]
    fn test_llm_final_cannot_be_reconstructed() {
        use crate::gateway::feishu_worker;
        
        // LLM final is never reconstructible
        let reply = feishu_worker::reconstruct_reply(ReplyKind::LlmFinal, None, None);
        assert!(reply.is_none());
    }
    
    #[test]
    fn test_recovery_classifies_template_vs_llm() {
        let store = create_test_store();
        
        // Insert a template outbox (retryable)
        let template_input = FeishuOutboxInput {
            outbound_id: "out_template".to_string(),
            job_id: None,
            event_key: None,
            channel: "feishu".to_string(),
            chat_id: Some("chat_x".to_string()),
            reply_kind: Some("timeout_reply".to_string()),
            reply: None,
            result_json: None,
        };
        store.insert_outbox(&template_input).unwrap();
        
        // Insert an LLM final outbox (audit-only)
        let llm_input = FeishuOutboxInput {
            outbound_id: "out_llm_audit".to_string(),
            job_id: None,
            event_key: None,
            channel: "feishu".to_string(),
            chat_id: Some("chat_y".to_string()),
            reply_kind: Some("llm_final".to_string()),
            reply: None,
            result_json: None,
        };
        store.insert_outbox(&llm_input).unwrap();
        
        // Get all recoverable outbox
        let recoverable = store.get_recoverable_outbox().unwrap();
        assert_eq!(recoverable.len(), 2);
        
        // Classify each
        for item in &recoverable {
            let kind = item.reply_kind.as_deref()
                .and_then(ReplyKind::from_str);
            
            match item.outbound_id.as_str() {
                "out_template" => {
                    assert_eq!(kind, Some(ReplyKind::Timeout));
                    assert!(kind.unwrap().is_retryable());
                }
                "out_llm_audit" => {
                    assert_eq!(kind, Some(ReplyKind::LlmFinal));
                    assert!(!kind.unwrap().is_retryable());
                }
                _ => panic!("Unexpected outbound_id"),
            }
        }
    }
    
    #[test]
    fn test_metadata_json_redaction_includes_authorization() {
        let json = r#"{
            "Authorization": "Bearer xyz",
            "app_secret": "secret_value",
            "tenant_access_token": "t-123",
            "normal": "visible"
        }"#;
        
        let redacted = FeishuStore::redact_secrets(json);
        
        assert!(redacted.contains("visible"));
        assert!(!redacted.contains("Bearer xyz"));
        assert!(!redacted.contains("secret_value"));
        assert!(!redacted.contains("t-123"));
    }
    
    #[test]
    fn test_status_enum_string_round_trip() {
        use std::str::FromStr;
        
        let statuses = [
            OutboxStatus::Pending,
            OutboxStatus::Sending,
            OutboxStatus::Sent,
            OutboxStatus::Failed,
            OutboxStatus::Dead,
            OutboxStatus::Skipped,
            OutboxStatus::Abandoned,
            OutboxStatus::FailedPrivacyNoRetry,
        ];
        
        for status in statuses.iter() {
            let s = status.as_str();
            let parsed = OutboxStatus::from_str(s);
            assert_eq!(parsed, Some(*status), "Round-trip failed for {}", s);
        }
    }
    
    #[test]
    fn test_outbox_abandon_uses_abandoned_status_not_dead() {
        let store = create_test_store();
        
        let outbox_input = FeishuOutboxInput {
            outbound_id: "out_test_abandon".to_string(),
            job_id: None,
            event_key: None,
            channel: "feishu".to_string(),
            chat_id: Some("chat_z".to_string()),
            reply_kind: Some("llm_final".to_string()),
            reply: None,
            result_json: None,
        };
        store.insert_outbox(&outbox_input).unwrap();
        
        // Abandon (must use ABANDONED, not DEAD)
        store.outbox_abandon("out_test_abandon", "no_reply_content_for_privacy").unwrap();
        
        let item = store.get_outbox("out_test_abandon").unwrap().unwrap();
        assert_eq!(item.status, OutboxStatus::Abandoned);
        assert_eq!(item.error_code, Some("recovery_abandoned".to_string()));
    }
    
    #[test]
    fn test_result_json_persists_for_monitor() {
        let store = create_test_store();
        
        let result_data = serde_json::json!({
            "duration_secs": 60,
            "changed": false,
            "start_path": "/path/start.png",
            "end_path": "/path/end.png"
        });
        
        let outbox_input = FeishuOutboxInput {
            outbound_id: "out_monitor".to_string(),
            job_id: None,
            event_key: None,
            channel: "feishu".to_string(),
            chat_id: Some("chat_z".to_string()),
            reply_kind: Some("monitor_final".to_string()),
            reply: None,
            result_json: Some(serde_json::to_string(&result_data).unwrap()),
        };
        
        let outbox = store.insert_outbox(&outbox_input).unwrap();
        assert!(outbox.result_json.is_some());
        
        let retrieved = store.get_outbox("out_monitor").unwrap().unwrap();
        assert!(retrieved.result_json.is_some());
    }
    
    // ========== Retry worker integration tests (v0.8.7.5.2) ==========
    
    #[test]
    fn test_outbox_begin_retry_increments_attempts() {
        let store = create_test_store();
        
        let outbox_input = FeishuOutboxInput {
            outbound_id: "out_retry_attempts".to_string(),
            job_id: None,
            event_key: None,
            channel: "feishu".to_string(),
            chat_id: Some("chat_x".to_string()),
            reply_kind: Some("timeout_reply".to_string()),
            reply: None,
            result_json: None,
        };
        store.insert_outbox(&outbox_input).unwrap();
        
        // Begin retry
        let can_retry = store.outbox_begin_retry("out_retry_attempts").unwrap();
        assert!(can_retry);
        
        // Should now be SENDING with attempts=1
        let item = store.get_outbox("out_retry_attempts").unwrap().unwrap();
        assert_eq!(item.status, OutboxStatus::Sending);
        assert_eq!(item.attempts, 1);
    }
    
    #[test]
    fn test_outbox_begin_retry_max_attempts_dead() {
        let store = create_test_store();
        
        let outbox_input = FeishuOutboxInput {
            outbound_id: "out_max_attempts".to_string(),
            job_id: None,
            event_key: None,
            channel: "feishu".to_string(),
            chat_id: Some("chat_x".to_string()),
            reply_kind: Some("timeout_reply".to_string()),
            reply: None,
            result_json: None,
        };
        store.insert_outbox(&outbox_input).unwrap();
        
        // Simulate 3 attempts (max is 3)
        for _ in 0..3 {
            let _ = store.outbox_begin_retry("out_max_attempts").unwrap();
            let _ = store.outbox_sent("out_max_attempts", "skipped"); // Reset to SENT then back to PENDING
            let _ = store.outbox_failed("out_max_attempts", "test_error", "fail");
        }
        
        // Next begin_retry should mark as DEAD
        let can_retry = store.outbox_begin_retry("out_max_attempts").unwrap();
        assert!(!can_retry);
        
        let item = store.get_outbox("out_max_attempts").unwrap().unwrap();
        assert_eq!(item.status, OutboxStatus::Dead);
    }
    
    #[test]
    fn test_outbox_begin_retry_skips_already_sent() {
        let store = create_test_store();
        
        let outbox_input = FeishuOutboxInput {
            outbound_id: "out_already_sent".to_string(),
            job_id: None,
            event_key: None,
            channel: "feishu".to_string(),
            chat_id: Some("chat_x".to_string()),
            reply_kind: Some("timeout_reply".to_string()),
            reply: None,
            result_json: None,
        };
        store.insert_outbox(&outbox_input).unwrap();
        
        // Mark as SENT with valid platform_message_id
        store.outbox_sent("out_already_sent", "feishu_msg_123").unwrap();
        
        // Begin retry should not be allowed
        let can_retry = store.outbox_begin_retry("out_already_sent").unwrap();
        assert!(!can_retry);
    }
    
    #[test]
    fn test_outbox_reconstruct_incomplete_for_missing_result_json() {
        let store = create_test_store();
        
        // monitor_final without result_json
        let outbox_input = FeishuOutboxInput {
            outbound_id: "out_monitor_no_json".to_string(),
            job_id: None,
            event_key: None,
            channel: "feishu".to_string(),
            chat_id: Some("chat_x".to_string()),
            reply_kind: Some("monitor_final".to_string()),
            reply: None,
            result_json: None,
        };
        store.insert_outbox(&outbox_input).unwrap();
        
        store.outbox_mark_reconstruct_incomplete("out_monitor_no_json", "missing_result_json").unwrap();
        
        let item = store.get_outbox("out_monitor_no_json").unwrap().unwrap();
        assert_eq!(item.status, OutboxStatus::FailedReconstructIncomplete);
        assert_eq!(item.error_code, Some("reconstruct_incomplete".to_string()));
    }
    
    #[test]
    fn test_retry_classifies_retryable_kinds() {
        use crate::gateway::feishu_worker;
        
        let retryable_kinds = [
            "progress_reply",
            "timeout_reply",
            "failure_reply",
            "chat_only_blocked_reply",
            "unsupported_reply",
            "monitor_final",
        ];
        
        for kind_str in retryable_kinds.iter() {
            let kind = ReplyKind::from_str(kind_str);
            assert!(kind.is_some(), "Kind {} should be parseable", kind_str);
            
            // Skip monitor_final because it requires result_json
            if matches!(kind, Some(ReplyKind::MonitorFinal)) {
                let result = feishu_worker::reconstruct_reply(kind.unwrap(), None, None);
                assert!(result.is_none(), "monitor_final without json should be None");
            } else {
                let result = feishu_worker::reconstruct_reply(kind.unwrap(), None, None);
                assert!(result.is_some(), "Template kind {} should be reconstructible", kind_str);
            }
        }
    }
    
    #[test]
    fn test_retry_classifies_llm_final_as_audit_only() {
        let store = create_test_store();
        
        // Insert llm_final outbox
        let outbox_input = FeishuOutboxInput {
            outbound_id: "out_llm_audit".to_string(),
            job_id: None,
            event_key: None,
            channel: "feishu".to_string(),
            chat_id: Some("chat_x".to_string()),
            reply_kind: Some("llm_final".to_string()),
            reply: None,
            result_json: None,
        };
        store.insert_outbox(&outbox_input).unwrap();
        
        // Get recoverable outbox (should include this PENDING one)
        let recoverable = store.get_recoverable_outbox().unwrap();
        let item = recoverable.iter().find(|o| o.outbound_id == "out_llm_audit").unwrap();
        
        // LLM final is NOT retryable
        let kind = item.reply_kind.as_deref()
            .and_then(ReplyKind::from_str);
        assert_eq!(kind, Some(ReplyKind::LlmFinal));
        assert!(!kind.unwrap().is_retryable());
        
        // Mark as FAILED_PRIVACY_NO_RETRY
        store.outbox_mark_failed_privacy("out_llm_audit", "llm_reply_not_reconstructible").unwrap();
        
        let item = store.get_outbox("out_llm_audit").unwrap().unwrap();
        assert_eq!(item.status, OutboxStatus::FailedPrivacyNoRetry);
    }
    
    #[test]
    fn test_retry_full_state_machine_template_reply() {
        let store = create_test_store();
        
        // Insert timeout_reply as PENDING
        let outbox_input = FeishuOutboxInput {
            outbound_id: "out_template_state".to_string(),
            job_id: None,
            event_key: None,
            channel: "feishu".to_string(),
            chat_id: Some("chat_x".to_string()),
            reply_kind: Some("timeout_reply".to_string()),
            reply: None,
            result_json: None,
        };
        store.insert_outbox(&outbox_input).unwrap();
        
        // 1. Begin retry: SENDING
        assert!(store.outbox_begin_retry("out_template_state").unwrap());
        assert_eq!(
            store.get_outbox("out_template_state").unwrap().unwrap().status,
            OutboxStatus::Sending
        );
        
        // 2. Success: SENT
        store.outbox_sent("out_template_state", "feishu_msg_456").unwrap();
        let item = store.get_outbox("out_template_state").unwrap().unwrap();
        assert_eq!(item.status, OutboxStatus::Sent);
        assert_eq!(item.platform_message_id, Some("feishu_msg_456".to_string()));
    }
    
    #[test]
    fn test_retry_failure_transitions_to_failed() {
        let store = create_test_store();
        
        let outbox_input = FeishuOutboxInput {
            outbound_id: "out_failure_transition".to_string(),
            job_id: None,
            event_key: None,
            channel: "feishu".to_string(),
            chat_id: Some("chat_x".to_string()),
            reply_kind: Some("timeout_reply".to_string()),
            reply: None,
            result_json: None,
        };
        store.insert_outbox(&outbox_input).unwrap();
        
        store.outbox_begin_retry("out_failure_transition").unwrap();
        // Simulate send failure
        let retryable = store.outbox_failed("out_failure_transition", "send_error", "Connection lost").unwrap();
        assert!(retryable);
        
        let item = store.get_outbox("out_failure_transition").unwrap().unwrap();
        assert_eq!(item.status, OutboxStatus::Failed);
        assert!(item.next_attempt_at.is_some());
    }
    
    #[test]
    fn test_retry_does_not_send_audit_only() {
        let store = create_test_store();
        
        // Insert llm_final
        let outbox_input = FeishuOutboxInput {
            outbound_id: "out_audit_no_send".to_string(),
            job_id: None,
            event_key: None,
            channel: "feishu".to_string(),
            chat_id: Some("chat_x".to_string()),
            reply_kind: Some("llm_final".to_string()),
            reply: None,
            result_json: None,
        };
        store.insert_outbox(&outbox_input).unwrap();
        
        // outbox_begin_retry should still technically work, but the retry worker
        // should classify LLM final as audit-only BEFORE calling begin_retry.
        // The classification logic is in run_retry_worker_once which is harder to unit test
        // without a full runtime. Here we test that the kind classification works.
        let item = store.get_outbox("out_audit_no_send").unwrap().unwrap();
        let kind = item.reply_kind.as_deref().and_then(ReplyKind::from_str);
        assert_eq!(kind, Some(ReplyKind::LlmFinal));
        assert!(!kind.unwrap().is_retryable());
    }
    
    #[test]
    fn test_status_enum_string_round_trip_includes_reconstruct_incomplete() {
        use std::str::FromStr;
        
        let statuses = [
            OutboxStatus::Pending,
            OutboxStatus::Sending,
            OutboxStatus::Sent,
            OutboxStatus::Failed,
            OutboxStatus::Dead,
            OutboxStatus::Skipped,
            OutboxStatus::Abandoned,
            OutboxStatus::FailedPrivacyNoRetry,
            OutboxStatus::FailedReconstructIncomplete,
        ];
        
        for status in statuses.iter() {
            let s = status.as_str();
            let parsed = OutboxStatus::from_str(s);
            assert_eq!(parsed, Some(*status), "Round-trip failed for {}", s);
        }
    }
}
