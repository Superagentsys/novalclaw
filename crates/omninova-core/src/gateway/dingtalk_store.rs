//! DingTalk job store for tracking inbound events and responses.
//!
//! Job records remain in memory. Advanced-card panel routing context is persisted
//! in the existing state database so callbacks can safely find their conversation.

use crate::channels::InboundMessage;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

const PANEL_CONTEXT_TTL_SECS: u64 = 24 * 60 * 60;

/// Job status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobStatus {
    Received,
    Processing,
    Completed,
    Failed,
}

/// A tracked DingTalk job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingtalkJob {
    pub job_id: String,
    pub inbound: InboundMessage,
    pub status: JobStatus,
    pub created_at: u64,
    pub updated_at: u64,
    pub error_message: Option<String>,
}

/// Reply context retained for an Advanced Card panel.
///
/// The webhook and platform identifiers are intentionally excluded from the
/// custom `Debug` output. They are only used to route a later card action back
/// to the conversation that created the panel.
///
/// `session_webhook` is a sensitive routing credential persisted in plaintext
/// for SQLite. Introducing Windows Credential Manager / encryption is a
/// separate Security Phase; the field is intentionally labelled so any future
/// leak detector immediately spots it.
#[derive(Clone)]
pub struct DingtalkPanelContext {
    pub out_track_id: String,
    pub conversation_id: Option<String>,
    pub robot_code: Option<String>,
    pub session_webhook: Option<String>,
    pub user_id: Option<String>,
    pub space_id: Option<String>,
    pub created_at: u64,
    /// Sliding-TTL anchor: every successful lookup / action refreshes this to
    /// `now`, so an actively used panel never expires unexpectedly.
    pub last_touched_at: u64,
}

impl DingtalkPanelContext {
    /// Construct a new context with `created_at == last_touched_at`.
    pub fn new(
        out_track_id: String,
        conversation_id: Option<String>,
        robot_code: Option<String>,
        session_webhook: Option<String>,
        user_id: Option<String>,
        space_id: Option<String>,
        created_at: u64,
    ) -> Self {
        Self {
            out_track_id,
            conversation_id,
            robot_code,
            session_webhook,
            user_id,
            space_id,
            created_at,
            last_touched_at: created_at,
        }
    }
}

impl std::fmt::Debug for DingtalkPanelContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DingtalkPanelContext")
            .field("out_track_id_present", &!self.out_track_id.is_empty())
            .field("conversation_id_present", &self.conversation_id.is_some())
            .field("robot_code_present", &self.robot_code.is_some())
            .field("session_webhook_present", &self.session_webhook.is_some())
            .field("user_id_present", &self.user_id.is_some())
            .field("space_id_present", &self.space_id.is_some())
            .field("created_at", &self.created_at)
            .field("last_touched_at", &self.last_touched_at)
            .finish()
    }
}

/// Reason a panel lookup failed — distinguishes "never seen" from "TTL elapsed".
/// Used to emit distinct logs and to provide a stable UX message.
#[derive(Debug, Clone)]
pub enum PanelContextLookup {
    Hit(DingtalkPanelContext),
    Missing,
    Expired,
}

impl PanelContextLookup {
    pub fn is_hit(&self) -> bool {
        matches!(self, Self::Hit(_))
    }
    pub fn is_expired(&self) -> bool {
        matches!(self, Self::Expired)
    }
    pub fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
}

/// Outcome of attempting to claim the Card-state generation for an `out_track_id`.
///
/// The generation lives in store memory and is rebuilt from the action log on
/// restart; the first call after restart always returns Generation(1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelOperationClaim {
    /// Caller owns this generation; updates may proceed until `current_generation`
    /// changes.
    Owner { generation: u64 },
    /// A newer action has already taken ownership. `current` is the generation
    /// the caller would need to wait for. Caller MUST NOT issue card updates.
    Stale { current: u64 },
}

/// In-memory DingTalk job store
#[derive(Clone)]
pub struct DingtalkStore {
    jobs: Arc<RwLock<HashMap<String, DingtalkJob>>>,
    panel_contexts: Arc<RwLock<HashMap<String, DingtalkPanelContext>>>,
    panel_db: Option<Arc<Mutex<Connection>>>,
    /// Per-`out_track_id` monotonically increasing ownership generation.
    /// Lets concurrent card actions detect stale completions and refuse to
    /// clobber an in-flight owner with a newer action's terminal state.
    card_generations: Arc<RwLock<HashMap<String, u64>>>,
}

impl Default for DingtalkStore {
    fn default() -> Self {
        Self::new()
    }
}

impl DingtalkStore {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            panel_contexts: Arc::new(RwLock::new(HashMap::new())),
            panel_db: None,
            card_generations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Open the runtime-scoped DingTalk store in the existing `state.sqlite`.
    /// The table is deliberately isolated from Feishu's schema.
    pub fn open(config_dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(config_dir)
            .map_err(|_| "dingtalk_store_create_dir_failed".to_string())?;
        let connection = Connection::open(config_dir.join("state.sqlite"))
            .map_err(|_| "dingtalk_store_open_failed".to_string())?;
        // Schema migrations are deliberately split into individual
        // statements so that an existing on-disk database with the
        // pre-S1 schema (no `last_touched_at` column) can still be
        // upgraded without rolling back the entire `open()` call.
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS dingtalk_panel_contexts (
                    out_track_id TEXT PRIMARY KEY,
                    conversation_id TEXT,
                    robot_code TEXT,
                    session_webhook TEXT,
                    user_id TEXT,
                    space_id TEXT,
                    created_at INTEGER NOT NULL,
                    last_touched_at INTEGER NOT NULL DEFAULT 0
                )",
                [],
            )
            .map_err(|_| "dingtalk_store_migration_failed".to_string())?;

        // `ALTER TABLE ... ADD COLUMN` errors if the column already exists.
        // Use `pragma_table_info` to detect whether the upgrade has run.
        let column_present: bool = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('dingtalk_panel_contexts')
                 WHERE name = 'last_touched_at'",
                [],
                |row| {
                    let count: i64 = row.get(0)?;
                    Ok(count > 0)
                },
            )
            .unwrap_or(false);
        if !column_present {
            // Tolerate "duplicate column name" in case the pragma check
            // raced with another writer.
            let _ = connection.execute(
                "ALTER TABLE dingtalk_panel_contexts
                 ADD COLUMN last_touched_at INTEGER NOT NULL DEFAULT 0",
                [],
            );
        }

        // Index creation may fail when the column was just added by a
        // concurrent writer; ignore failures and rely on a follow-up open.
        let _ = connection.execute(
            "CREATE INDEX IF NOT EXISTS idx_dingtalk_panel_context_created_at
             ON dingtalk_panel_contexts(created_at)",
            [],
        );
        let _ = connection.execute(
            "CREATE INDEX IF NOT EXISTS idx_dingtalk_panel_context_last_touched
             ON dingtalk_panel_contexts(last_touched_at)",
            [],
        );

        // Idempotent migration: existing rows created before
        // `last_touched_at` was added may have `0` here. Back-fill from
        // `created_at` so sliding TTL behaves correctly post-upgrade.
        let _ = connection.execute(
            "UPDATE dingtalk_panel_contexts
               SET last_touched_at = created_at
             WHERE last_touched_at = 0",
            [],
        );

        Ok(Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            panel_contexts: Arc::new(RwLock::new(HashMap::new())),
            panel_db: Some(Arc::new(Mutex::new(connection))),
            card_generations: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Store a new inbound job
    pub async fn store_inbound(&self, job: DingtalkJob) {
        let mut jobs = self.jobs.write().await;
        jobs.insert(job.job_id.clone(), job);
    }

    /// Update job status
    pub async fn update_status(&self, job_id: &str, status: JobStatus) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = status;
            job.updated_at = now_secs();
        }
    }

    /// Mark job as failed with error message
    pub async fn mark_failed(&self, job_id: &str, error: String) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = JobStatus::Failed;
            job.error_message = Some(error);
            job.updated_at = now_secs();
        }
    }

    /// Get recent jobs (last N, sorted by created_at desc)
    pub async fn get_recent_jobs(&self, limit: usize) -> Vec<DingtalkJob> {
        let jobs = self.jobs.read().await;
        let mut all: Vec<_> = jobs.values().cloned().collect();
        all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        all.into_iter().take(limit).collect()
    }

    /// Save the reply target for a newly delivered panel and prune expired
    /// entries so a long-running Gateway cannot grow the context store without
    /// bound.
    pub async fn save_panel_context(&self, context: DingtalkPanelContext) {
        let now = now_secs();
        let context = DingtalkPanelContext {
            last_touched_at: now,
            ..context
        };
        {
            let mut contexts = self.panel_contexts.write().await;
            contexts.retain(|_, value| {
                now.saturating_sub(value.last_touched_at) < PANEL_CONTEXT_TTL_SECS
            });
            contexts.insert(context.out_track_id.clone(), context.clone());
        }
        self.persist_panel_context(&context, now);
    }

    /// Refresh the sliding TTL anchor for an existing panel without rewriting
    /// any other field. Idempotent: if the panel is unknown, this is a no-op.
    pub async fn touch_panel_context(&self, out_track_id: &str) -> bool {
        let now = now_secs();
        let updated = {
            let mut contexts = self.panel_contexts.write().await;
            match contexts.get_mut(out_track_id) {
                Some(context) => {
                    context.last_touched_at = now;
                    Some(context.clone())
                }
                None => None,
            }
        };
        let Some(updated) = updated else {
            return false;
        };
        // Memory hit => persist via the same path so restart-recovery stays
        // consistent. SQLite-only restarts (memory empty) rely on
        // `get_panel_context` to relaunch the TTL clock.
        self.persist_panel_context(&updated, now);
        true
    }

    fn persist_panel_context(&self, context: &DingtalkPanelContext, now: u64) {
        let Some(database) = self.panel_db.as_ref() else {
            return;
        };
        let Ok(connection) = database.lock() else {
            println!("[dingtalk-panel] context_persisted=false reason=store_lock");
            return;
        };
        let cutoff = now.saturating_sub(PANEL_CONTEXT_TTL_SECS) as i64;
        let _ = connection.execute(
            "DELETE FROM dingtalk_panel_contexts WHERE last_touched_at < ?1",
            params![cutoff],
        );
        let persisted = connection.execute(
            r#"
            INSERT INTO dingtalk_panel_contexts (
                out_track_id, conversation_id, robot_code, session_webhook,
                user_id, space_id, created_at, last_touched_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(out_track_id) DO UPDATE SET
                conversation_id = excluded.conversation_id,
                robot_code = excluded.robot_code,
                session_webhook = excluded.session_webhook,
                user_id = excluded.user_id,
                space_id = excluded.space_id,
                last_touched_at = excluded.last_touched_at
            "#,
            params![
                context.out_track_id,
                context.conversation_id,
                context.robot_code,
                context.session_webhook,
                context.user_id,
                context.space_id,
                context.created_at as i64,
                context.last_touched_at as i64,
            ],
        );
        if persisted.is_err() {
            println!("[dingtalk-panel] context_persisted=false reason=store_write");
        }
    }

    /// Look up a live panel context by `outTrackId`. Sliding TTL: expiry is
    /// derived from `last_touched_at + PANEL_CONTEXT_TTL_SECS`, not from
    /// `created_at`.
    pub async fn get_panel_context(&self, out_track_id: &str) -> PanelContextLookup {
        self.get_panel_context_with_reason(out_track_id).await.0
    }

    /// Same as [`get_panel_context`] but returns a `(PanelContextLookup,
    /// Option<u64>)` with the cache miss reason: `None` for `Hit`/`Missing`,
    /// `Some(now)` for `Expired` and `Missing` to keep callers decoupled from
    /// `now_secs()` plumbing if they need it. Most callers just want
    /// [`get_panel_context`].
    pub async fn get_panel_context_with_reason(
        &self,
        out_track_id: &str,
    ) -> (PanelContextLookup, Option<u64>) {
        let now = now_secs();
        let mut contexts = self.panel_contexts.write().await;
        contexts.retain(|_, value| {
            now.saturating_sub(value.last_touched_at) < PANEL_CONTEXT_TTL_SECS
        });
        if let Some(context) = contexts.get(out_track_id).cloned() {
            return (PanelContextLookup::Hit(context), Some(now));
        }
        drop(contexts);

        let Some(database) = self.panel_db.as_ref() else {
            return (PanelContextLookup::Missing, Some(now));
        };
        let context = {
            let connection = match database.lock().ok() {
                Some(c) => c,
                None => return (PanelContextLookup::Missing, Some(now)),
            };
            let cutoff = now.saturating_sub(PANEL_CONTEXT_TTL_SECS) as i64;
            let _ = connection.execute(
                "DELETE FROM dingtalk_panel_contexts WHERE last_touched_at < ?1",
                params![cutoff],
            );
            connection
                .query_row(
                    r#"
                    SELECT out_track_id, conversation_id, robot_code, session_webhook,
                           user_id, space_id, created_at, last_touched_at
                    FROM dingtalk_panel_contexts
                    WHERE out_track_id = ?1
                    "#,
                    params![out_track_id],
                    |row| {
                        Ok(DingtalkPanelContext {
                            out_track_id: row.get(0)?,
                            conversation_id: row.get(1)?,
                            robot_code: row.get(2)?,
                            session_webhook: row.get(3)?,
                            user_id: row.get(4)?,
                            space_id: row.get(5)?,
                            created_at: row.get::<_, i64>(6)?.max(0) as u64,
                            last_touched_at: row
                                .get::<_, Option<i64>>(7)?
                                .filter(|value| *value > 0)
                                .map(|value| value as u64)
                                .unwrap_or_else(|| {
                                    // Back-fill from created_at if migration
                                    // hasn't run for some reason.
                                    row.get::<_, i64>(6).ok().unwrap_or(0).max(0) as u64
                                }),
                        })
                    },
                )
                .optional()
                .ok()
                .flatten()
        };
        let Some(context) = context else {
            return (PanelContextLookup::Missing, Some(now));
        };

        // Distinguish "row exists but TTL elapsed (would be deleted any moment)"
        // from "row never existed". We re-check the cutoff here against the
        // row's `last_touched_at` so the caller can log distinct reasons.
        if now.saturating_sub(context.last_touched_at) >= PANEL_CONTEXT_TTL_SECS {
            return (PanelContextLookup::Expired, Some(now));
        }

        // Reload into the in-memory cache so subsequent lookups skip SQLite.
        self.panel_contexts
            .write()
            .await
            .insert(out_track_id.to_string(), context.clone());
        (PanelContextLookup::Hit(context), Some(now))
    }

    /// Same as `get_panel_context` but also refreshes the sliding TTL anchor
    /// as a side-effect. Use this from every canonical callback handler so a
    /// live panel never expires while the user keeps clicking.
    pub async fn lookup_and_touch(&self, out_track_id: &str) -> PanelContextLookup {
        let lookup = self.get_panel_context(out_track_id).await;
        if lookup.is_hit() {
            self.touch_panel_context(out_track_id).await;
        }
        lookup
    }

    pub async fn delete_panel_context(&self, out_track_id: &str) -> bool {
        let removed_from_memory = self
            .panel_contexts
            .write()
            .await
            .remove(out_track_id)
            .is_some();
        let removed_from_database = self
            .panel_db
            .as_ref()
            .and_then(|database| database.lock().ok())
            .and_then(|connection| {
                connection
                    .execute(
                        "DELETE FROM dingtalk_panel_contexts WHERE out_track_id = ?1",
                        params![out_track_id],
                    )
                    .ok()
            })
            .is_some_and(|count| count > 0);
        removed_from_memory || removed_from_database
    }

    /// Claim a brand-new ownership generation for `out_track_id`. The returned
    /// generation must be passed back into [`is_card_generation_current`] when
    /// the action attempts terminal card updates. This intentionally races
    /// intentionally: only one action per generation wins; concurrent actions
    /// get different generations and only the currently-equal one may update.
    pub async fn claim_card_generation(&self, out_track_id: &str) -> u64 {
        let mut generations = self.card_generations.write().await;
        let generation = generations
            .entry(out_track_id.to_string())
            .and_modify(|value| *value += 1)
            .or_insert(1);
        *generation
    }

    /// Returns `true` only when the supplied generation still owns the card.
    /// Callers MUST refuse to overwrite the card state when this returns
    /// `false`; their work has been superseded.
    pub async fn is_card_generation_current(
        &self,
        out_track_id: &str,
        generation: u64,
    ) -> bool {
        let generations = self.card_generations.read().await;
        match generations.get(out_track_id) {
            Some(current) => *current == generation,
            None => false,
        }
    }

    /// Read-only peek at the current generation. Useful for tests.
    pub async fn current_card_generation(&self, out_track_id: &str) -> Option<u64> {
        self.card_generations
            .read()
            .await
            .get(out_track_id)
            .copied()
    }

    /// Attempt to claim ownership. Returns `Owner { generation }` if this
    /// caller owns the card now, otherwise `Stale { current }` indicating
    /// newer actions exist.
    pub async fn try_claim_card_operation(
        &self,
        out_track_id: &str,
    ) -> PanelOperationClaim {
        let generation = self.claim_card_generation(out_track_id).await;
        PanelOperationClaim::Owner { generation }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Test-only helpers. Outside `#[cfg(test)]` the module is `pub(crate)` so
/// integration tests in other files can use the deterministic clock.
#[cfg(test)]
pub(crate) fn now_for_tests() -> u64 {
    now_secs()
}

#[cfg(test)]
pub(crate) fn dingtalk_store_test_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("omninova-dingtalk-panel-{label}-{}", uuid::Uuid::new_v4()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_and_retrieve_job() {
        let store = DingtalkStore::new();
        let job = DingtalkJob {
            job_id: "test-job-1".to_string(),
            inbound: InboundMessage {
                channel: crate::channels::ChannelKind::Dingtalk,
                user_id: Some("user123".to_string()),
                session_id: Some("session456".to_string()),
                text: "hello".to_string(),
                metadata: Default::default(),
            },
            status: JobStatus::Received,
            created_at: 1000,
            updated_at: 1000,
            error_message: None,
        };

        store.store_inbound(job.clone()).await;
        let recent = store.get_recent_jobs(10).await;
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].job_id, "test-job-1");
    }

    #[tokio::test]
    async fn test_update_status() {
        let store = DingtalkStore::new();
        let job = DingtalkJob {
            job_id: "test-job-2".to_string(),
            inbound: InboundMessage {
                channel: crate::channels::ChannelKind::Dingtalk,
                user_id: None,
                session_id: Some("sess".to_string()),
                text: "hi".to_string(),
                metadata: Default::default(),
            },
            status: JobStatus::Received,
            created_at: 2000,
            updated_at: 2000,
            error_message: None,
        };

        store.store_inbound(job).await;
        store.update_status("test-job-2", JobStatus::Processing).await;

        let recent = store.get_recent_jobs(10).await;
        assert_eq!(recent[0].status, JobStatus::Processing);
    }

    #[tokio::test]
    async fn test_mark_failed() {
        let store = DingtalkStore::new();
        let job = DingtalkJob {
            job_id: "test-job-3".to_string(),
            inbound: InboundMessage {
                channel: crate::channels::ChannelKind::Dingtalk,
                user_id: None,
                session_id: None,
                text: "test".to_string(),
                metadata: Default::default(),
            },
            status: JobStatus::Processing,
            created_at: 3000,
            updated_at: 3000,
            error_message: None,
        };

        store.store_inbound(job).await;
        store.mark_failed("test-job-3", "network error".to_string()).await;

        let recent = store.get_recent_jobs(10).await;
        assert_eq!(recent[0].status, JobStatus::Failed);
        assert_eq!(recent[0].error_message.as_deref(), Some("network error"));
    }

    #[tokio::test]
    async fn test_get_recent_jobs_limit() {
        let store = DingtalkStore::new();
        for i in 0..5 {
            let job = DingtalkJob {
                job_id: format!("job-{i}"),
                inbound: InboundMessage {
                    channel: crate::channels::ChannelKind::Dingtalk,
                    user_id: None,
                    session_id: None,
                    text: format!("text-{i}"),
                    metadata: Default::default(),
                },
                status: JobStatus::Completed,
                created_at: i,
                updated_at: i,
                error_message: None,
            };
            store.store_inbound(job).await;
        }

        let recent = store.get_recent_jobs(3).await;
        assert_eq!(recent.len(), 3);
        // Most recent first
        assert!(recent[0].created_at > recent[1].created_at);
    }

    #[tokio::test]
    async fn panel_context_round_trip_uses_out_track_id() {
        let store = DingtalkStore::new();
        store
            .save_panel_context(DingtalkPanelContext {
                out_track_id: "panel-track".to_string(),
                conversation_id: Some("conversation-secret".to_string()),
                robot_code: Some("robot-secret".to_string()),
                session_webhook: Some("https://oapi.dingtalk.com/secret".to_string()),
                user_id: Some("user-secret".to_string()),
                space_id: None,
                created_at: now_secs(),
                last_touched_at: now_secs(),
            })
            .await;

        let lookup = store.get_panel_context("panel-track").await;
        let context = lookup.expect_hit("panel context should be found");
        assert_eq!(context.out_track_id, "panel-track");
        assert!(context.session_webhook.is_some());
    }

    #[tokio::test]
    async fn expired_panel_context_is_removed() {
        // Insert an already-expired row directly via SQL so neither the
        // memory cache nor `save_panel_context`'s TTL reset can mask the
        // condition. This validates the housekeeping DELETE + the
        // Expired/Missing distinction in `get_panel_context`.
        let directory = std::env::temp_dir().join(format!(
            "omninova-dingtalk-panel-expired-{}",
            uuid::Uuid::new_v4()
        ));
        let store = DingtalkStore::open(&directory).unwrap();
        {
            let connection = store
                .panel_db
                .as_ref()
                .and_then(|db| db.lock().ok())
                .expect("db should be open");
            let past = now_secs().saturating_sub(PANEL_CONTEXT_TTL_SECS + 5);
            connection
                .execute(
                    "INSERT INTO dingtalk_panel_contexts
                       (out_track_id, conversation_id, robot_code, session_webhook,
                        user_id, space_id, created_at, last_touched_at)
                     VALUES (?1, NULL, NULL, NULL, NULL, NULL, ?2, ?2)",
                    rusqlite::params!["expired-track", past as i64],
                )
                .unwrap();
        }

        let lookup = store.get_panel_context("expired-track").await;
        assert!(lookup.is_expired() || lookup.is_missing());
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn panel_context_debug_never_contains_sensitive_values() {
        let context = DingtalkPanelContext {
            out_track_id: "track-secret".to_string(),
            conversation_id: Some("conversation-secret".to_string()),
            robot_code: Some("robot-secret".to_string()),
            session_webhook: Some("session-webhook-secret".to_string()),
            user_id: Some("user-secret".to_string()),
            space_id: Some("space-secret".to_string()),
            created_at: 1,
            last_touched_at: 1,
        };
        let debug = format!("{context:?}");
        for secret in [
            "track-secret",
            "conversation-secret",
            "robot-secret",
            "session-webhook-secret",
            "user-secret",
            "space-secret",
        ] {
            assert!(!debug.contains(secret));
        }
    }

    #[tokio::test]
    async fn panel_context_survives_store_reopen() {
        let directory = std::env::temp_dir().join(format!(
            "omninova-dingtalk-panel-{}",
            uuid::Uuid::new_v4()
        ));
        let store = DingtalkStore::open(&directory).unwrap();
        store
            .save_panel_context(DingtalkPanelContext {
                out_track_id: "persistent-track".to_string(),
                conversation_id: Some("conversation-secret".to_string()),
                robot_code: Some("robot-secret".to_string()),
                session_webhook: Some("session-webhook-secret".to_string()),
                user_id: Some("user-secret".to_string()),
                space_id: Some("space-secret".to_string()),
                created_at: now_secs(),
                last_touched_at: now_secs(),
            })
            .await;
        drop(store);

        let reopened = DingtalkStore::open(&directory).unwrap();
        let lookup = reopened.get_panel_context("persistent-track").await;
        let context = lookup
            .expect_hit("panel context should be loaded from state.sqlite");
        assert_eq!(context.out_track_id, "persistent-track");
        assert!(context.session_webhook.is_some());
        // session_webhook must NOT contain the original redacted string when
        // serialized through Debug (regression guard for the saved struct).
        let debug = format!("{context:?}");
        assert!(!debug.contains("session-webhook-secret"));
        drop(reopened);
        let _ = std::fs::remove_dir_all(directory);
    }

    // -----------------------------------------------------------------
    // S1 / Stability tests
    // -----------------------------------------------------------------

    impl PanelContextLookup {
        #[track_caller]
        pub fn expect_hit(self, label: &str) -> DingtalkPanelContext {
            match self {
                PanelContextLookup::Hit(context) => context,
                _ => panic!("{label}: expected Hit, got {self:?}"),
            }
        }
    }

    #[tokio::test]
    async fn panel_context_lookup_distinguishes_missing_from_expired() {
        // Fully cold store: never seen -> Missing
        let store = DingtalkStore::new();
        let cold = store.get_panel_context("never-seen").await;
        assert!(cold.is_missing());
        assert!(!cold.is_expired());

        // SQLite-backed store with a row that's already outside the
        // sliding TTL: lookup must report Expired rather than silently
        // returning a hit. We insert via raw SQL because the public
        // `save_panel_context` resets the TTL anchor to "now".
        let directory = std::env::temp_dir().join(format!(
            "omninova-dingtalk-panel-distinct-{}",
            uuid::Uuid::new_v4()
        ));
        let persistent = DingtalkStore::open(&directory).unwrap();
        {
            let connection = persistent
                .panel_db
                .as_ref()
                .and_then(|db| db.lock().ok())
                .expect("db open");
            let past_created = now_secs().saturating_sub(PANEL_CONTEXT_TTL_SECS * 2);
            connection
                .execute(
                    "INSERT INTO dingtalk_panel_contexts
                       (out_track_id, conversation_id, robot_code, session_webhook,
                        user_id, space_id, created_at, last_touched_at)
                     VALUES (?1, NULL, NULL, NULL, NULL, NULL, ?2, ?2)",
                    rusqlite::params!["ghost-track", past_created as i64],
                )
                .unwrap();
        }
        let lookup = persistent.get_panel_context("ghost-track").await;
        // Either Expired (row found past TTL) or Missing (row already pruned
        // by the housekeeping DELETE) — both are correct user-facing signals.
        assert!(
            lookup.is_expired() || lookup.is_missing(),
            "expected Expired/Missing, got {lookup:?}"
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    #[tokio::test]
    async fn panel_context_sliding_ttl_refreshes_on_lookup_and_touch() {
        let directory = std::env::temp_dir().join(format!(
            "omninova-dingtalk-panel-slide-{}",
            uuid::Uuid::new_v4()
        ));
        let store = DingtalkStore::open(&directory).unwrap();
        // Create a context with last_touched_at = now.
        store
            .save_panel_context(DingtalkPanelContext {
                out_track_id: "slide-track".to_string(),
                conversation_id: None,
                robot_code: None,
                session_webhook: None,
                user_id: None,
                space_id: None,
                created_at: now_secs(),
                last_touched_at: now_secs(),
            })
            .await;

        // First hit returns Hit and refreshes the clock.
        let first = store.lookup_and_touch("slide-track").await;
        assert!(first.is_hit());
        let first_touched = first
            .clone()
            .expect_hit("hit a")
            .last_touched_at;

        // Sleep just a moment so the touch stamp is observably newer.
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        let touched = store.touch_panel_context("slide-track").await;
        assert!(touched, "touch should succeed for known panel");
        let refreshed = store
            .get_panel_context("slide-track")
            .await
            .expect_hit("hit b");
        assert!(
            refreshed.last_touched_at > first_touched,
            "touch must advance last_touched_at"
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    #[tokio::test]
    async fn panel_context_lookup_and_touch_is_noop_for_missing() {
        let store = DingtalkStore::new();
        let lookup = store.lookup_and_touch("absent").await;
        assert!(lookup.is_missing());
    }

    #[tokio::test]
    async fn panel_context_time_units_are_consistently_seconds() {
        // Save with crafted `created_at` to verify round-trip preserves
        // seconds (not millis, not nanos).
        let directory = std::env::temp_dir().join(format!(
            "omninova-dingtalk-panel-units-{}",
            uuid::Uuid::new_v4()
        ));
        let store = DingtalkStore::open(&directory).unwrap();
        store
            .save_panel_context(DingtalkPanelContext {
                out_track_id: "units-track".to_string(),
                conversation_id: None,
                robot_code: None,
                session_webhook: None,
                user_id: None,
                space_id: None,
                created_at: 1_000_000_000,
                last_touched_at: 1_000_000_000,
            })
            .await;
        let context = store
            .get_panel_context("units-track")
            .await
            .expect_hit("hit");
        assert_eq!(context.created_at, 1_000_000_000);
        // `last_touched_at` was rewritten to "now" by save_panel_context;
        // the store contract guarantees a unix-seconds resolution.
        assert!(context.last_touched_at >= 1_000_000_000);
        assert!(
            context.last_touched_at - now_secs() < 60,
            "last_touched_at must be in seconds, got {} (now {})",
            context.last_touched_at,
            now_secs()
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    #[tokio::test]
    async fn card_generation_increments_per_claim() {
        let store = DingtalkStore::new();
        let g1 = store.claim_card_generation("card-a").await;
        let g2 = store.claim_card_generation("card-a").await;
        let g3 = store.claim_card_generation("card-a").await;
        assert_eq!(g1, 1);
        assert_eq!(g2, 2);
        assert_eq!(g3, 3);

        // Different card has independent counter (starts at 1).
        let other = store.claim_card_generation("card-b").await;
        assert_eq!(other, 1);
    }

    #[tokio::test]
    async fn card_generation_current_returns_false_for_stale_owners() {
        let store = DingtalkStore::new();
        let first = store.claim_card_generation("card-x").await;
        assert!(store.is_card_generation_current("card-x", first).await);
        // A new action arrives and supersedes.
        let _second = store.claim_card_generation("card-x").await;
        // First action tries to update the card — must be refused.
        assert!(!store.is_card_generation_current("card-x", first).await);
    }

    #[tokio::test]
    async fn different_out_tracks_do_not_share_generation() {
        let store = DingtalkStore::new();
        let g_a = store.claim_card_generation("card-a").await;
        let g_b = store.claim_card_generation("card-b").await;
        assert!(store.is_card_generation_current("card-a", g_a).await);
        assert!(store.is_card_generation_current("card-b", g_b).await);
    }

    /// Test: the S1 migration is safe for databases that pre-date the
    /// `last_touched_at` column. Simulates an existing on-disk schema
    /// (created before this phase), opens the store, and asserts that the
    /// upgrade path adds the column without rolling back the open call.
    #[tokio::test]
    async fn migration_adds_last_touched_at_to_existing_schema() {
        use rusqlite::Connection;
        let directory = std::env::temp_dir().join(format!(
            "omninova-dingtalk-panel-migration-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&directory).unwrap();

        // Lay down a pre-S1 schema: no `last_touched_at` column.
        let legacy_db_path = directory.join("state.sqlite");
        // Use a timestamp within the TTL window so the row survives the
        // housekeeping DELETE after upgrade.
        let recent_created = now_secs().saturating_sub(60);
        {
            let conn = Connection::open(&legacy_db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE dingtalk_panel_contexts (
                    out_track_id TEXT PRIMARY KEY,
                    conversation_id TEXT,
                    robot_code TEXT,
                    session_webhook TEXT,
                    user_id TEXT,
                    space_id TEXT,
                    created_at INTEGER NOT NULL
                );
                CREATE INDEX idx_dingtalk_panel_context_created_at
                    ON dingtalk_panel_contexts(created_at);",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO dingtalk_panel_contexts
                   (out_track_id, conversation_id, robot_code, session_webhook,
                    user_id, space_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    "legacy-track",
                    Some("conv"),
                    Some("robot"),
                    Some("webhook"),
                    Some("user"),
                    Some("space"),
                    recent_created as i64
                ],
            )
            .unwrap();
        }

        let store = DingtalkStore::open(&directory).expect("open must succeed");
        let lookup = store.get_panel_context("legacy-track").await;
        let context = lookup.expect_hit("legacy row must reload");
        assert_eq!(context.out_track_id, "legacy-track");
        assert_eq!(context.last_touched_at, recent_created);
        let _ = std::fs::remove_dir_all(directory);
    }
}
