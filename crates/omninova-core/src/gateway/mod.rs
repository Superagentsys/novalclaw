pub mod pairing;
pub mod ws;
pub mod agent_menu;
pub mod feishu_worker;
pub mod feishu_store;
pub mod dingtalk_worker;
pub mod dingtalk_store;
pub mod dingtalk_commands;

#[cfg(test)]
mod dingtalk_tests;

use crate::gateway::feishu_store::FeishuStore;
use crate::gateway::dingtalk_worker::verify_dingtalk_webhook_signature;
use home;

use crate::agent::sanitize_messages_for_provider;
use crate::agent::{AgentCancellationToken, ToolExecutionEvent};
use crate::channels::adapters::outbound::{
    ChannelOutboundSender, FeishuOutboundSender, LarkOutboundSender, MockOutboundSender,
    OutboundDeliveryStatus, OutboundResult, OutboundResultSummary, ReplyTarget, TokenCache,
};
use crate::channels::adapters::platform_webhook::{
    inbound_from_platform_webhook, verification_response,
};
use crate::channels::adapters::webhook::{inbound_from_webhook, WebhookInboundPayload};
use crate::channels::{ChannelKind, InboundMessage};
use crate::config::{
    resolve_effective_workspace_dir, Config, GatewayPublicConfig, GatewayPublicMode,
};
use crate::gateway::feishu_worker::FeishuJobSender;
use crate::memory::{factory::build_memory_from_config, Memory};
use crate::providers::ChatMessage;
use crate::providers::{
    build_provider_from_config, build_provider_with_selection, ProviderSelection,
};
use crate::routing::{resolve_agent_route, RouteDecision};
use crate::security::{
    is_tool_globally_allowed, resolve_shell_allowlist, ApprovalController, EstopController,
    EstopState, PendingApproval, SecurityContext,
};
use crate::skills::{format_skills_prompt, load_skills_from_dir};
use crate::tools::{
    AgentInvoker, BrowserTool, ContentSearchTool, DelegateRequest, DelegateTool, FileEditTool,
    FileListTool, FilePatchTool, FileReadTool, FileWriteTool, GitOperationsTool, GlobSearchTool,
    HttpRequestTool, MemoryRecallTool, MemoryStoreTool, PdfReadTool, ShellTool, Tool, WebFetchTool,
    WebSearchTool,
};
use crate::util::auth::verify_webhook_signature_with_policy_options;
use crate::Agent;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;
use tokio::sync::RwLock;
use tracing::{info, warn};

static SESSION_LOCK_WAIT_EVENTS: AtomicU64 = AtomicU64::new(0);
static SESSION_LOCK_TIMEOUT_EVENTS: AtomicU64 = AtomicU64::new(0);
static OUTBOUND_TOKEN_CACHE: OnceLock<Arc<TokenCache>> = OnceLock::new();
static DEDUP_CACHE: OnceLock<Arc<DedupCache>> = OnceLock::new();
static OUTBOUND_MSG_CACHE: OnceLock<Arc<OutboundMsgCache>> = OnceLock::new();
const WORKSPACE_REQUIRED_MESSAGE: &str =
    "请先选择 Workspace，Agent 需要一个真实工作目录才能执行文件、Shell 或 Git 操作。";

/// TTL-based deduplication cache for webhook events
#[derive(Debug, Clone)]
pub(crate) struct DedupCache {
    inner: Arc<tokio::sync::RwLock<std::collections::HashMap<String, Instant>>>,
    ttl_secs: u64,
}

impl DedupCache {
    fn new(ttl_secs: u64) -> Self {
        Self {
            inner: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            ttl_secs,
        }
    }

    fn global() -> Arc<DedupCache> {
        DEDUP_CACHE
            .get_or_init(|| Arc::new(DedupCache::new(1800))) // 30 minutes default
            .clone()
    }

    async fn check_and_insert(&self, key: &str) -> bool {
        self.check_and_insert_with_ttl(key, self.ttl_secs).await
    }

    /// Insert the key with a custom TTL (seconds). Returns true if the key was
    /// not previously cached (i.e., this is a fresh event).
    async fn check_and_insert_with_ttl(&self, key: &str, ttl_secs: u64) -> bool {
        let mut cache = self.inner.write().await;
        let now = Instant::now();

        // Clean expired entries
        cache.retain(|_, &mut expiry| now < expiry);

        if cache.contains_key(key) {
            return false; // Duplicate
        }

        cache.insert(key.to_string(), now + Duration::from_secs(ttl_secs));
        true // New event
    }

    /// Clear all cached entries. Intended for unit tests.
    #[cfg(test)]
    async fn clear(&self) {
        let mut cache = self.inner.write().await;
        cache.clear();
    }
}

/// Cheap, opaque, irreversible string mixer for dedup KEY construction.
/// The output is fed into the cache key but is never logged or returned
/// to the user — it deliberately prevents the raw event_id /
/// open_message_id / chat_id from leaking through cache-trace logs.
pub(crate) fn hash_dedup_string(s: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    let h = hasher.finish();
    // 16 hex chars of entropy — collision-safe for our 30 min window.
    format!("h{:016x}", h)
}

/// TTL-based cache for tracking recently sent outbound messages
/// Used to filter out self-messages when Feishu webhooks re-deliver bot messages
#[derive(Debug, Clone)]
struct OutboundMsgCache {
    inner: Arc<tokio::sync::RwLock<std::collections::HashMap<String, Instant>>>,
    ttl_secs: u64,
}

impl OutboundMsgCache {
    fn new(ttl_secs: u64) -> Self {
        Self {
            inner: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            ttl_secs,
        }
    }

    fn global() -> Arc<OutboundMsgCache> {
        OUTBOUND_MSG_CACHE
            .get_or_init(|| Arc::new(OutboundMsgCache::new(1800))) // 30 minutes default
            .clone()
    }

    /// Record that we sent a message with this platform message_id
    async fn record_outbound(&self, channel: &str, message_id: &str) {
        let key = format!("{}:{}", channel, message_id);
        let mut cache = self.inner.write().await;
        cache.insert(key, Instant::now() + Duration::from_secs(self.ttl_secs));
    }

    /// Check if a message was sent by us recently
    async fn is_our_message(&self, channel: &str, message_id: &str) -> bool {
        let key = format!("{}:{}", channel, message_id);
        let mut cache = self.inner.write().await;
        let now = Instant::now();
        
        // Clean expired entries
        cache.retain(|_, &mut expiry| now < expiry);
        
        cache.contains_key(&key)
    }
}

/// Single-flight guard for desktop-monitor executions.
/// Prevents concurrent monitor runs within the same Feishu chat session.
///
/// Key design:
/// - Per-chat (hashed chat_id) so different users don't interfere
/// - TTL = max(monitor_duration + 15s, 45s) so a stuck/hung monitor
///   does not lock the user out forever
/// - Every lease has an opaque owner id so an expired job cannot release a newer job
/// - Guard is stored in a global cache to survive across HTTP callbacks
#[derive(Debug, Clone)]
struct MonitorFlightEntry {
    owner_id: String,
    expires_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MonitorFlightLease {
    key: String,
    owner_id: String,
    ttl_secs: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct MonitorFlightGuard {
    /// Map: feishu:<hash(chat_id)>:desktop_monitor → active lease.
    inner: Arc<tokio::sync::RwLock<std::collections::HashMap<String, MonitorFlightEntry>>>,
    /// Minimum TTL for any guard entry
    min_ttl_secs: u64,
}

fn monitor_flight_key(chat_id: &str) -> String {
    let raw_chat_id = chat_id.strip_prefix("feishu:").unwrap_or(chat_id);
    format!(
        "feishu:{}:desktop_monitor",
        hash_dedup_string(raw_chat_id)
    )
}

pub(crate) fn monitor_guard_ttl_secs(duration_secs: u64) -> u64 {
    duration_secs.saturating_add(15).max(45)
}

impl MonitorFlightGuard {
    fn new() -> Arc<MonitorFlightGuard> {
        Arc::new(MonitorFlightGuard {
            inner: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            min_ttl_secs: 45,
        })
    }

    /// Attempt to acquire the single-flight slot for `chat_id` with a caller-specified TTL.
    /// Returns an owner-scoped lease if the slot was free; `None` if busy.
    async fn try_acquire_with_ttl(
        &self,
        chat_id: &str,
        ttl_secs: u64,
    ) -> Option<MonitorFlightLease> {
        let key = monitor_flight_key(chat_id);
        let now = Instant::now();
        let ttl_secs = ttl_secs.max(self.min_ttl_secs);
        let mut cache = self.inner.write().await;
        cache.retain(|_, entry| now < entry.expires_at);
        if cache.contains_key(&key) {
            return None;
        }

        let owner_id = uuid::Uuid::new_v4().to_string();
        cache.insert(
            key.clone(),
            MonitorFlightEntry {
                owner_id: owner_id.clone(),
                expires_at: now + Duration::from_secs(ttl_secs),
            },
        );

        Some(MonitorFlightLease {
            key,
            owner_id,
            ttl_secs,
        })
    }

    /// Extend a queued lease immediately before the monitor starts.
    async fn renew(&self, lease: &MonitorFlightLease) -> bool {
        let now = Instant::now();
        let mut cache = self.inner.write().await;
        let Some(entry) = cache.get_mut(&lease.key) else {
            return false;
        };
        if entry.owner_id != lease.owner_id || now >= entry.expires_at {
            return false;
        }
        entry.expires_at = now + Duration::from_secs(lease.ttl_secs);
        true
    }

    /// Release only if the caller still owns the active lease.
    async fn release(&self, lease: &MonitorFlightLease) -> bool {
        let mut cache = self.inner.write().await;
        let is_owner = cache
            .get(&lease.key)
            .is_some_and(|entry| entry.owner_id == lease.owner_id);
        if is_owner {
            cache.remove(&lease.key);
        }
        is_owner
    }

    /// Clear all entries. Intended for unit tests.
    #[cfg(test)]
    pub(crate) async fn clear(&self) {
        let mut cache = self.inner.write().await;
        cache.clear();
    }

    #[cfg(test)]
    async fn active_count(&self) -> usize {
        let now = Instant::now();
        let mut cache = self.inner.write().await;
        cache.retain(|_, entry| now < entry.expires_at);
        cache.len()
    }
}
use std::time::Instant;
use std::time::Duration;

fn now_ts() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let t = now.as_secs();
    let h = (t / 3600) % 24;
    let m = (t / 60) % 60;
    let s = t % 60;
    let ms = now.subsec_millis();
    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms)
}

#[derive(Clone)]
pub struct GatewayRuntime {
    config: Arc<RwLock<Config>>,
    pub(crate) memory: Arc<dyn Memory>,
    cron_store: Option<crate::cron::CronStore>,
    webhook_nonces: Arc<RwLock<HashMap<String, i64>>>,
    session_store_guard: Arc<tokio::sync::Mutex<()>>,
    active_inbound: Arc<AtomicUsize>,
    active_children_by_parent: Arc<RwLock<HashMap<String, usize>>>,
    session_tree: Arc<RwLock<HashMap<String, SessionLineageMeta>>>,
    run_registry: AgentRunRegistry,
    /// Feishu async job queue sender (for background worker processing)
    feishu_job_sender: Arc<RwLock<Option<FeishuJobSender>>>,
    /// Feishu worker queue length tracker
    feishu_queue_len: Arc<RwLock<usize>>,
    /// Feishu SQLite store for event/job/outbox persistence
    feishu_store: Option<Arc<FeishuStore>>,
    /// DingTalk async job queue sender (for background worker processing)
    dingtalk_job_sender: Arc<RwLock<Option<dingtalk_worker::DingtalkJobSender>>>,
    /// DingTalk worker queue length tracker
    dingtalk_queue_len: Arc<RwLock<usize>>,
    /// DingTalk in-memory store for job tracking
    dingtalk_store: Option<Arc<dingtalk_store::DingtalkStore>>,
    /// Per-runtime webhook event deduplication state.
    dedup_cache: Arc<DedupCache>,
    /// Per-runtime, per-chat desktop-monitor single-flight state.
    monitor_flights: Arc<MonitorFlightGuard>,
}

#[derive(Clone, Debug)]
pub struct AgentRunRegistry {
    inner: Arc<RwLock<HashMap<String, ActiveAgentRun>>>,
}

#[derive(Clone, Debug)]
struct ActiveAgentRun {
    run_id: String,
    session_id: String,
    started_at: SystemTime,
    cancel_token: AgentCancellationToken,
}

impl AgentRunRegistry {
    fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn start_run(
        &self,
        run_id: String,
        session_id: String,
    ) -> anyhow::Result<AgentCancellationToken> {
        let mut runs = self.inner.write().await;
        if runs
            .values()
            .any(|run| run.session_id == session_id && !run.cancel_token.is_cancelled())
        {
            return Err(anyhow::anyhow!(
                "当前会话已有任务正在运行，请等待完成或取消。"
            ));
        }
        let token = AgentCancellationToken::new();
        runs.insert(
            run_id.clone(),
            ActiveAgentRun {
                run_id,
                session_id,
                started_at: SystemTime::now(),
                cancel_token: token.clone(),
            },
        );
        Ok(token)
    }

    async fn finish_run(&self, run_id: &str) {
        self.inner.write().await.remove(run_id);
    }

    pub async fn cancel_run(&self, run_id: &str) -> anyhow::Result<()> {
        let token = {
            let runs = self.inner.read().await;
            runs.get(run_id).map(|run| run.cancel_token.clone())
        };
        match token {
            Some(token) => {
                token.cancel();
                Ok(())
            }
            None => Err(anyhow::anyhow!("未找到正在运行的 Agent Run")),
        }
    }
}

struct ActiveRunGuard {
    registry: AgentRunRegistry,
    run_id: String,
    finished: bool,
}

impl ActiveRunGuard {
    fn new(registry: AgentRunRegistry, run_id: String) -> Self {
        Self {
            registry,
            run_id,
            finished: false,
        }
    }

    async fn finish(mut self) {
        self.registry.finish_run(&self.run_id).await;
        self.finished = true;
    }
}

impl Drop for ActiveRunGuard {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let registry = self.registry.clone();
        let run_id = self.run_id.clone();
        tokio::spawn(async move {
            registry.finish_run(&run_id).await;
        });
    }
}

impl GatewayRuntime {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            memory: Arc::new(crate::InMemoryMemory::new()),
            cron_store: None,
            webhook_nonces: Arc::new(RwLock::new(HashMap::new())),
            session_store_guard: Arc::new(tokio::sync::Mutex::new(())),
            active_inbound: Arc::new(AtomicUsize::new(0)),
            active_children_by_parent: Arc::new(RwLock::new(HashMap::new())),
            session_tree: Arc::new(RwLock::new(HashMap::new())),
            run_registry: AgentRunRegistry::new(),
            feishu_job_sender: Arc::new(RwLock::new(None)),
            feishu_queue_len: Arc::new(RwLock::new(0)),
            feishu_store: None,
            dingtalk_job_sender: Arc::new(RwLock::new(None)),
            dingtalk_queue_len: Arc::new(RwLock::new(0)),
            dingtalk_store: None,
            dedup_cache: Arc::new(DedupCache::new(1800)),
            monitor_flights: MonitorFlightGuard::new(),
        }
    }

    pub fn with_memory(config: Config, memory: Arc<dyn Memory>) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            memory,
            cron_store: None,
            webhook_nonces: Arc::new(RwLock::new(HashMap::new())),
            session_store_guard: Arc::new(tokio::sync::Mutex::new(())),
            active_inbound: Arc::new(AtomicUsize::new(0)),
            active_children_by_parent: Arc::new(RwLock::new(HashMap::new())),
            session_tree: Arc::new(RwLock::new(HashMap::new())),
            run_registry: AgentRunRegistry::new(),
            feishu_job_sender: Arc::new(RwLock::new(None)),
            feishu_queue_len: Arc::new(RwLock::new(0)),
            feishu_store: None,
            dingtalk_job_sender: Arc::new(RwLock::new(None)),
            dingtalk_queue_len: Arc::new(RwLock::new(0)),
            dingtalk_store: None,
            dedup_cache: Arc::new(DedupCache::new(1800)),
            monitor_flights: MonitorFlightGuard::new(),
        }
    }

    pub(crate) fn dedup_cache(&self) -> Arc<DedupCache> {
        self.dedup_cache.clone()
    }

    pub(crate) fn monitor_flight_guard(&self) -> Arc<MonitorFlightGuard> {
        self.monitor_flights.clone()
    }
    
    /// Initialize the Feishu async worker with the given queue
    pub async fn init_feishu_worker(&self, sender: FeishuJobSender) {
        let mut lock = self.feishu_job_sender.write().await;
        *lock = Some(sender);
    }

    pub async fn init_dingtalk_worker(&mut self) {
        // Keep initialization idempotent. Most importantly, never replace the
        // sender while the receiver from the same channel is still running.
        {
            let sender = self.dingtalk_job_sender.read().await;
            if sender.as_ref().is_some_and(|sender| !sender.is_closed()) {
                println!("[dingtalk-async-worker] already_started=true");
                return;
            }
        }

        let worker_state = dingtalk_worker::DingtalkWorkerState::with_queue_len(
            self.dingtalk_queue_len.clone(),
        );
        let store = Arc::new(dingtalk_store::DingtalkStore::new());
        self.dingtalk_store = Some(store.clone());
        let sender = worker_state.sender();

        // Check again under the write lock. A repeated init must not install a
        // sender from a new channel whose receiver will never be started.
        {
            let mut lock = self.dingtalk_job_sender.write().await;
            if lock.as_ref().is_some_and(|sender| !sender.is_closed()) {
                println!("[dingtalk-async-worker] already_started=true");
                return;
            }
            *lock = Some(sender);
        }

        let runtime = Arc::new(self.clone());
        dingtalk_worker::start_dingtalk_worker(worker_state, runtime, store).await;
    }

    /// Read-only diagnostic state. This does not start, stop, or replace the
    /// worker channel and never exposes queued payloads or platform IDs.
    pub async fn dingtalk_worker_started(&self) -> bool {
        self.dingtalk_job_sender
            .read()
            .await
            .as_ref()
            .is_some_and(|sender| !sender.is_closed())
    }
    
    /// Get approximate queue length
    pub async fn feishu_queue_len(&self) -> usize {
        *self.feishu_queue_len.read().await
    }
    
    /// Increment queue length
    pub async fn inc_feishu_queue_len(&self) {
        let mut len = self.feishu_queue_len.write().await;
        *len += 1;
    }
    
    /// Get Feishu store reference
    pub fn feishu_store(&self) -> Option<Arc<FeishuStore>> {
        self.feishu_store.clone()
    }
    
    /// Recover pending jobs and outbox items
    pub async fn recover_pending(&self) {
        if let Some(ref store) = self.feishu_store {
            // Recover jobs
            match store.get_recoverable_jobs() {
                Ok(recoverable_jobs) => {
                    let mut recovered_count = 0;
                    for job in recoverable_jobs {
                        // Try to re-enqueue the job
                        if let Some(payload_json) = &job.payload_json {
                            match serde_json::from_str::<serde_json::Value>(payload_json) {
                                Ok(payload) => {
                                    // Reconstruct FeishuAsyncJob
                                    if let Some(recovered_job) = self.try_reconstruct_job(&job, &payload) {
                                        match self.try_send_feishu_job(recovered_job).await {
                                            Ok(()) => {
                                                recovered_count += 1;
                                                println!("[feishu-recovery] job_recovered job_id={}", job.job_id);
                                            }
                                            Err(_) => {
                                                // Queue full - mark as dead
                                                let _ = store.job_abandon(&job.job_id, "queue_full_on_recovery");
                                            }
                                        }
                                    } else {
                                        // Can't reconstruct - mark as dead
                                        let _ = store.job_abandon(&job.job_id, "cannot_reconstruct_payload");
                                    }
                                }
                                Err(e) => {
                                    println!("[feishu-recovery] job_parse_error job_id={} error={}", job.job_id, e);
                                    let _ = store.job_abandon(&job.job_id, "payload_parse_error");
                                }
                            }
                        } else {
                            // No payload - can't recover
                            let _ = store.job_abandon(&job.job_id, "missing_payload");
                        }
                    }
                    if recovered_count > 0 {
                        println!("[feishu-recovery] jobs_recovered count={}", recovered_count);
                    }
                }
                Err(e) => {
                    println!("[feishu-recovery] failed to get jobs: {}", e);
                }
            }
            
            // Recover outbox - distinguish retryable vs audit-only
            match store.get_recoverable_outbox() {
                Ok(recoverable_outbox) => {
                    let mut abandoned_count = 0;
                    let mut retryable_count = 0;
                    for item in recoverable_outbox {
                        let kind = item.reply_kind.as_deref()
                            .and_then(crate::gateway::feishu_store::ReplyKind::from_str);
                        
                        // Privacy-first: only retryable kinds can be sent after restart
                        let can_retry = matches!(kind, Some(
                            crate::gateway::feishu_store::ReplyKind::Progress
                            | crate::gateway::feishu_store::ReplyKind::Timeout
                            | crate::gateway::feishu_store::ReplyKind::Failure
                            | crate::gateway::feishu_store::ReplyKind::ChatOnlyBlocked
                            | crate::gateway::feishu_store::ReplyKind::Unsupported
                            | crate::gateway::feishu_store::ReplyKind::MonitorFinal
                        ));
                        
                        if can_retry {
                            // Mark as retryable for next recovery sweep
                            // Actual retry requires reconstructing reply and re-sending
                            println!(
                                "[feishu-outbox] retryable outbound_id={} reply_kind={}",
                                item.outbound_id,
                                item.reply_kind.as_deref().unwrap_or("?")
                            );
                            let _ = store.outbox_mark_retryable(&item.outbound_id);
                            retryable_count += 1;
                        } else {
                            // Audit-only outbox (e.g., LLM replies) - cannot retry
                            println!("[feishu-outbox] abandoned reason=privacy_no_full_body outbound_id={}", item.outbound_id);
                            let _ = store.outbox_abandon(&item.outbound_id, "no_reply_content_for_privacy");
                            abandoned_count += 1;
                        }
                    }
                    if abandoned_count > 0 {
                        println!("[feishu-recovery] outbox_abandoned count={} reason=privacy_no_full_body", abandoned_count);
                    }
                    if retryable_count > 0 {
                        println!("[feishu-recovery] outbox_retryable count={}", retryable_count);
                    }
                }
                Err(e) => {
                    println!("[feishu-recovery] failed to get outbox: {}", e);
                }
            }
        }
    }
    
    /// Try to reconstruct a FeishuAsyncJob from job record and payload
    fn try_reconstruct_job(&self, job: &crate::gateway::feishu_store::FeishuJob, payload: &serde_json::Value) -> Option<crate::gateway::feishu_worker::FeishuAsyncJob> {
        use crate::channels::InboundMessage;
        use crate::channels::ChannelKind;
        
        // Extract inbound from payload
        let inbound = match inbound_from_platform_webhook(
            ChannelKind::Feishu, 
            payload.clone()
        ) {
            Ok(i) => i,
            Err(e) => {
                println!("[feishu-recovery] cannot_parse_inbound job_id={} error={}", job.job_id, e);
                return None;
            }
        };
        
        // Determine chat_only from mode
        let is_chat_only = job.mode == "chat_only";
        
        Some(crate::gateway::feishu_worker::FeishuAsyncJob::new(
            ChannelKind::Feishu,
            inbound,
            payload.clone(),
            is_chat_only,
            job.event_key.clone(),
            Some(job.job_id.clone()), // Use recovered job_id for consistency
        ))
    }
    
    /// Try to send a job to the Feishu worker queue
    pub async fn try_send_feishu_job(&self, job: crate::gateway::feishu_worker::FeishuAsyncJob) -> Result<(), crate::gateway::feishu_worker::EnqueueError> {
        let sender = self.feishu_job_sender.read().await;
        if let Some(ref s) = *sender {
            s.send(job).await.map_err(|_| crate::gateway::feishu_worker::EnqueueError::QueueFull)?;
            drop(sender);
            self.inc_feishu_queue_len().await;
            Ok(())
        } else {
            Err(crate::gateway::feishu_worker::EnqueueError::QueueFull)
        }
    }

    /// Non-blocking enqueue: uses `try_send` so it never awaits the queue.
    /// Returns `Ok(true)` if accepted, `Ok(false)` if dropped because the
    /// queue is full (channel was disconnected or capacity exhausted).
    /// This is the safe variant to call from synchronous-feeling paths like
    /// Feishu card callbacks that must ACK under 3 seconds.
    pub async fn try_send_feishu_job_nonblocking(
        &self,
        job: crate::gateway::feishu_worker::FeishuAsyncJob,
    ) -> Result<bool, crate::gateway::feishu_worker::EnqueueError> {
        let sender = self.feishu_job_sender.read().await;
        let Some(s) = sender.as_ref() else {
            return Err(crate::gateway::feishu_worker::EnqueueError::QueueFull);
        };
        match s.try_send(job) {
            Ok(()) => {
                drop(sender);
                let queue_len = self.feishu_queue_len.clone();
                tokio::spawn(async move {
                    let mut len = queue_len.write().await;
                    *len += 1;
                });
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    /// Check if the Feishu async worker is initialized
    pub async fn is_feishu_worker_initialized(&self) -> bool {
        let sender = self.feishu_job_sender.read().await;
        sender.is_some()
    }

    // -------------------------------------------------------------------------
    // DingTalk async worker methods
    // -------------------------------------------------------------------------

    pub async fn dingtalk_queue_len(&self) -> usize {
        *self.dingtalk_queue_len.read().await
    }

    pub async fn try_send_dingtalk_job(&self, job: dingtalk_worker::DingtalkAsyncJob) -> Result<(), dingtalk_worker::EnqueueError> {
        let sender = self
            .dingtalk_job_sender
            .read()
            .await
            .as_ref()
            .filter(|sender| !sender.is_closed())
            .cloned()
            .ok_or(dingtalk_worker::EnqueueError::QueueFull)?;
        {
            // Increment before send so a fast receiver cannot decrement from
            // zero and leave the approximate count permanently stale.
            let mut queue_len = self.dingtalk_queue_len.write().await;
            *queue_len += 1;
        }
        if sender.send(job).await.is_err() {
            let mut queue_len = self.dingtalk_queue_len.write().await;
            *queue_len = queue_len.saturating_sub(1);
            return Err(dingtalk_worker::EnqueueError::QueueFull);
        }
        Ok(())
    }

    pub async fn is_dingtalk_worker_initialized(&self) -> bool {
        let sender = self.dingtalk_job_sender.read().await;
        sender.as_ref().is_some_and(|sender| !sender.is_closed())
    }

    pub fn dingtalk_store(&self) -> Option<Arc<dingtalk_store::DingtalkStore>> {
        self.dingtalk_store.clone()
    }

    pub fn with_cron_store(mut self, store: crate::cron::CronStore) -> Self {
        self.cron_store = Some(store);
        self
    }

    pub async fn health(&self) -> GatewayHealth {
        let cfg = self.config.read().await.clone();
        let provider = build_provider_from_config(&cfg);
        GatewayHealth {
            ok: true,
            provider: provider.name().to_string(),
            provider_healthy: provider.health_check().await,
            memory_healthy: self.memory.health_check().await,
        }
    }

    pub async fn get_config(&self) -> Config {
        self.config.read().await.clone()
    }

    pub async fn set_config(&self, mut config: Config) -> anyhow::Result<()> {
        config.validate_or_bail()?;
        let mut lock = self.config.write().await;
        config.config_path = lock.config_path.clone();
        *lock = config;
        Ok(())
    }

    pub async fn cancel_agent_run(&self, run_id: &str) -> anyhow::Result<()> {
        self.run_registry.cancel_run(run_id).await
    }

    pub async fn refresh_memory_from_config(&mut self) -> anyhow::Result<()> {
        let cfg = self.config.read().await.clone();
        self.memory = build_memory_from_config(&cfg).await?;
        Ok(())
    }

    pub async fn chat(&self, message: &str) -> anyhow::Result<String> {
        self.ensure_not_stopped().await?;
        let cfg = self.config.read().await.clone();
        let route_agent_name = cfg.agent.name.clone();
        let provider = build_provider_from_config(&cfg);
        let agent_delegate = cfg.agents.get(&route_agent_name);
        let effective_workspace = resolve_effective_workspace_dir(
            None,
            agent_delegate.and_then(|d| d.workspace_dir.as_deref()),
            &cfg.workspace_dir,
        )
        .ok_or_else(|| anyhow::anyhow!(WORKSPACE_REQUIRED_MESSAGE))?;
        let mut tools = create_tools_for_route(
            &cfg,
            &route_agent_name,
            self.memory.clone(),
            &effective_workspace,
        );
        attach_delegate_tool(
            &cfg,
            self,
            &route_agent_name,
            None,
            &ChannelKind::Web,
            0,
            &mut tools,
        );
        let mut agent_cfg = cfg.agent.clone();
        agent_cfg.max_tool_iterations = resolve_agent_max_tool_iterations(&cfg, &route_agent_name);

        if cfg.skills.open_skills_enabled {
            let skills_dir = cfg
                .skills
                .open_skills_dir
                .as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| effective_workspace.join("skills"));
            if let Ok(skills) = load_skills_from_dir(&skills_dir) {
                let prompt = format_skills_prompt(&skills);
                if !prompt.is_empty() {
                    let current = agent_cfg.system_prompt.unwrap_or_default();
                    agent_cfg.system_prompt = Some(format!("{}\n{}", current, prompt));
                }
            }
        }

        let security = SecurityContext::from_config(&cfg);
        let mut agent = Agent::new(provider, tools, self.memory.clone(), agent_cfg, security);
        agent.process_message(message).await
    }

    /// Build a ready-to-use in-process `Agent` (provider + tools + skills +
    /// security), for interactive front-ends like the terminal UI that drive
    /// multi-turn streaming conversations and keep history in-memory.
    pub async fn build_interactive_agent(&self) -> anyhow::Result<Agent> {
        let cfg = self.config.read().await.clone();
        let route_agent_name = cfg.agent.name.clone();
        let provider = build_provider_from_config(&cfg);
        let agent_delegate = cfg.agents.get(&route_agent_name);
        let effective_workspace = resolve_effective_workspace_dir(
            None,
            agent_delegate.and_then(|d| d.workspace_dir.as_deref()),
            &cfg.workspace_dir,
        )
        .ok_or_else(|| anyhow::anyhow!(WORKSPACE_REQUIRED_MESSAGE))?;
        let mut tools = create_tools_for_route(
            &cfg,
            &route_agent_name,
            self.memory.clone(),
            &effective_workspace,
        );
        // Give the interactive agent the delegate tool so it can hand subtasks
        // to other configured agents (multi-agent), matching `chat()`.
        attach_delegate_tool(
            &cfg,
            self,
            &route_agent_name,
            None,
            &ChannelKind::Web,
            0,
            &mut tools,
        );
        let mut agent_cfg = cfg.agent.clone();
        agent_cfg.max_tool_iterations = resolve_agent_max_tool_iterations(&cfg, &route_agent_name);

        if cfg.skills.open_skills_enabled {
            let skills_dir = cfg
                .skills
                .open_skills_dir
                .as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| effective_workspace.join("skills"));
            if let Ok(skills) = load_skills_from_dir(&skills_dir) {
                let prompt = format_skills_prompt(&skills);
                if !prompt.is_empty() {
                    let current = agent_cfg.system_prompt.unwrap_or_default();
                    agent_cfg.system_prompt = Some(format!("{}\n{}", current, prompt));
                }
            }
        }

        let security = SecurityContext::from_config(&cfg);
        Ok(Agent::new(
            provider,
            tools,
            self.memory.clone(),
            agent_cfg,
            security,
        ))
    }

    pub async fn route(&self, inbound: &InboundMessage) -> RouteDecision {
        let cfg = self.config.read().await.clone();
        resolve_agent_route(&cfg, inbound)
    }

    pub async fn process_inbound(
        &self,
        inbound: &InboundMessage,
    ) -> anyhow::Result<GatewayInboundResponse> {
        let started = std::time::Instant::now();
        self.ensure_not_stopped().await?;
        let cfg = self.config.read().await.clone();
        let route = resolve_agent_route(&cfg, inbound);
        let security = SecurityContext::for_inbound(&cfg, inbound, &route);
        let channel_label = security.audit().context().channel.clone();
        crate::observability::record_inbound_request(&channel_label);
        security
            .audit_inbound_start(inbound.text.chars().count())
            .await;

        let mut steps = vec![ExecutionStep::done(
            "接收请求",
            format!(
                "channel={:?}, session={}, trace={}",
                inbound.channel,
                inbound.session_id.as_deref().unwrap_or("-"),
                security.trace_id()
            ),
        )];

        let result = (async {
        let _slot = acquire_inbound_slot(&cfg, &self.active_inbound)?;
        let _child_slot =
            acquire_subagent_guard(&cfg, inbound, &self.active_children_by_parent).await?;
        steps.push(ExecutionStep::done(
            "路由选择",
            format!(
                "Agent: {}{}{}",
                route.agent_name,
                route.provider.as_ref().map(|p| format!(", Provider: {p}")).unwrap_or_default(),
                route.model.as_ref().map(|m| format!(", Model: {m}")).unwrap_or_default()
            ),
        ));
        security
            .audit_route(&format!(
                "agent={} provider={:?} model={:?}",
                route.agent_name, route.provider, route.model
            ))
            .await;

        // Resolve effective workspace for this request.
        // Priority: session metadata > per-agent workspace > global workspace.
        let agent_delegate = cfg.agents.get(&route.agent_name);
        let session_workspace_path: Option<std::path::PathBuf> = inbound
            .metadata
            .get("workspace_dir")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from);
        let effective_workspace = resolve_effective_workspace_dir(
            session_workspace_path.as_deref(),
            agent_delegate.and_then(|d| d.workspace_dir.as_deref()),
            &cfg.workspace_dir,
        );
        let effective_workspace = match effective_workspace {
            Some(w) if !w.as_os_str().is_empty() => w,
            _ => {
                let message = WORKSPACE_REQUIRED_MESSAGE;
                steps.push(ExecutionStep::error(
                    "Workspace",
                    message,
                ));
                anyhow::bail!(message);
            }
        };
        steps.push(ExecutionStep::done(
            "Workspace",
            format!("{}", effective_workspace.display()),
        ));
        security
            .audit_route(&format!(
                "agent={} workspace={}",
                route.agent_name,
                effective_workspace.display()
            ))
            .await;
        let lineage = self
            .validate_and_resolve_session_lineage(&cfg, inbound, &route.agent_name)
            .await?;
        if let Some(max_depth) = cfg
            .agents
            .get(&route.agent_name)
            .and_then(|delegate| delegate.max_depth)
        {
            if lineage.spawn_depth > max_depth {
                anyhow::bail!(
                    "delegate agent '{}' spawn depth {} exceeds limit {}",
                    route.agent_name,
                    lineage.spawn_depth,
                    max_depth
                );
            }
        }
        let selection = ProviderSelection {
            provider: route.provider.clone(),
            model: route.model.clone(),
        };
        let provider = build_provider_with_selection(&cfg, &selection);
        let mut tools = create_tools_for_route(
            &cfg,
            &route.agent_name,
            self.memory.clone(),
            &effective_workspace,
        );
        if attach_delegate_tool(
            &cfg,
            self,
            &route.agent_name,
            inbound.session_id.as_deref(),
            &inbound.channel,
            lineage.spawn_depth,
            &mut tools,
        ) {
            steps.push(ExecutionStep::done("加载委托工具", "已启用 delegate 工具"));
        }
        steps.push(ExecutionStep::done(
            "加载工具",
            format!("可用工具数：{}", tools.len()),
        ));

        let mut agent_cfg = cfg.agent.clone();
        if let Some(delegate) = cfg.agents.get(&route.agent_name) {
            if let Some(prompt) = &delegate.system_prompt {
                agent_cfg.system_prompt = Some(prompt.clone());
            }
        }
        agent_cfg.max_tool_iterations = resolve_agent_max_tool_iterations(&cfg, &route.agent_name);

        // Always tell the agent where its workspace lives so it can answer
        // "where am I working?" and so the LLM has a single source of truth
        // for absolute paths. Path /home or /workspace style guesses should
        // never be used to answer that question.
        {
            let workspace_note = format!(
                "\n[环境信息] 当前 Workspace 目录是：{}。回答“你当前 workspace 在哪里”这类问题时，必须直接引用本路径，不要尝试通过 shell 或 file_read 探测 /workspace、/home、~ 等路径。\n[工具路径规则] 调用文件、搜索、Shell working_directory 或 Git 工具时，所有 path 必须是 workspace-relative；Workspace 根目录用 \".\"。不要把 D:\\、E:\\ 或完整 Workspace 绝对路径传给工具。写 index.html 时 path 只能是 \"index.html\"。编辑已存在文件时优先 file_read 后使用 file_patch；新建文件才使用 file_write，除非用户明确要求整文件重写。\n",
                effective_workspace.display()
            );
            let current = agent_cfg.system_prompt.unwrap_or_default();
            agent_cfg.system_prompt = Some(format!("{current}{workspace_note}"));
        }

        if cfg.skills.open_skills_enabled {
            let skills_dir = cfg.skills.open_skills_dir.as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| effective_workspace.join("skills"));
            if let Ok(skills) = load_skills_from_dir(&skills_dir) {
                let prompt = format_skills_prompt(&skills);
                if !prompt.is_empty() {
                    let current = agent_cfg.system_prompt.unwrap_or_default();
                    agent_cfg.system_prompt = Some(format!("{}\n{}", current, prompt));
                    steps.push(ExecutionStep::done("加载技能提示", "已注入 workspace skills"));
                }
            }
        }

        // ==== FEISHU CHAT-ONLY MODE ====
        // Inject chat-only system prompt when inbound is chat_only mode
        // This must be done AFTER the workspace note but BEFORE agent creation
        if let Some(chat_only_prompt) = security.chat_only_system_prompt() {
            let current = agent_cfg.system_prompt.unwrap_or_default();
            agent_cfg.system_prompt = Some(format!("{}\n\n{}", current, chat_only_prompt));
            steps.push(ExecutionStep::done("飞书聊天模式", "已注入 chat_only 限制"));

            // Detect tool intent in user message and short-circuit if needed
            if let Some(intent) = security.detect_tool_intent(&inbound.text) {
                let blocked_response = security.tool_intent_blocked_response();
                println!(
                    "[feishu-policy] short_circuit reason=tool_intent_detected intent={} text_len={}",
                    intent,
                    inbound.text.len()
                );
                // Return early with blocked response - no agent execution needed
                return Ok(GatewayInboundResponse {
                    route,
                    reply: blocked_response,
                    steps: vec![
                        ExecutionStep::done("飞书聊天模式", "已检测工具意图并拦截"),
                        ExecutionStep::done("Agent 执行", "已拦截 - 不执行工具"),
                    ],
                });
            }
        }

        let agent_security = security.clone();
        let mut agent = Agent::new(
            provider,
            tools,
            self.memory.clone(),
            agent_cfg.clone(),
            agent_security,
        );
        // Check for stateless mode - skip session history loading
        let is_stateless = inbound.metadata.get("stateless")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        
        if is_stateless {
            steps.push(ExecutionStep::done("加载会话历史", "stateless 模式跳过"));
        } else if let Some(session_id) = inbound.session_id.as_deref() {
            let _guard = self.session_store_guard.lock().await;
            match load_session_history(&cfg, &inbound.channel, session_id).await {
                Ok(history) if !history.is_empty() => {
                    let sanitized = sanitize_messages_for_provider(history);
                    steps.push(ExecutionStep::done(
                        "加载会话历史",
                        format!("历史消息数：{}", sanitized.len()),
                    ));
                    agent.import_messages(sanitized)
                }
                Ok(_) => steps.push(ExecutionStep::done("加载会话历史", "无历史消息")),
                Err(e) => {
                    steps.push(ExecutionStep::error("加载会话历史", e.to_string()));
                    warn!("failed to load session history for {}: {}", session_id, e)
                }
            }
        }

        let raw_vision_images = collect_desktop_vision_images(&cfg, inbound);
        let vision_images = if provider_supports_openai_vision(route.provider.as_deref()) {
            raw_vision_images.clone()
        } else {
            Vec::new()
        };
        if !raw_vision_images.is_empty() && vision_images.is_empty() {
            steps.push(ExecutionStep::error(
                "桌面视觉",
                "当前 Provider 不支持图像输入，请改用 OpenAI 兼容的视觉模型（如 GPT-4o、DeepSeek-VL、豆包视觉等）",
            ));
        } else if !vision_images.is_empty() {
            steps.push(ExecutionStep::done(
                "桌面视觉",
                format!("已附加 {} 张屏幕截图", vision_images.len()),
            ));
        }

        steps.push(ExecutionStep::running("Agent 执行", "调用模型；如模型请求工具，将继续执行工具循环"));
        let (reply, run_events) = if vision_images.is_empty() {
            agent.process_message_with_events(&inbound.text).await?
        } else {
            // Vision is not yet supported in process_message_with_events; degrade.
            let reply = agent.process_message(&inbound.text).await?;
            (reply, Vec::new())
        };
        let run_events: Vec<RunEvent> = run_events.into_iter().map(RunEvent::from).collect();
        steps.push(
            ExecutionStep::done("Agent 执行", "模型返回最终回复")
                .with_events(run_events)
        );
        steps.extend(extract_tool_steps(&agent.export_messages()));
        if let Some(session_id) = inbound.session_id.as_deref() {
            let _guard = self.session_store_guard.lock().await;
            let history_messages = agent
                .export_messages()
                .into_iter()
                .map(ChatMessage::strip_images_for_history)
                .collect();
            if let Err(e) = save_session_history(
                &cfg,
                &inbound.channel,
                session_id,
                history_messages,
                agent_cfg.max_history_messages,
                lineage.parent_session_key.clone(),
                lineage.parent_agent_id.clone(),
                route.agent_name.clone(),
                lineage.spawn_depth,
            )
            .await
            {
                steps.push(ExecutionStep::error("保存会话历史", e.to_string()));
                warn!("failed to save session history for {}: {}", session_id, e);
            } else {
                let count = agent.export_messages().len();
                security
                    .audit_session_persisted(session_id, count)
                    .await;
                steps.push(ExecutionStep::done("保存会话历史", session_id.to_string()));
            }
        }
        Ok(GatewayInboundResponse { route, reply, steps })
        })
        .await;

        match &result {
            Ok(resp) => {
                security
                    .audit_inbound_complete(true, &format!("reply_len={}", resp.reply.len()))
                    .await;
                crate::observability::record_inbound_duration(
                    &channel_label,
                    "ok",
                    started.elapsed().as_secs_f64(),
                );
            }
            Err(err) => {
                crate::observability::record_inbound_error("process_inbound");
                security
                    .audit_inbound_complete(false, &err.to_string())
                    .await;
                crate::observability::record_inbound_duration(
                    &channel_label,
                    "error",
                    started.elapsed().as_secs_f64(),
                );
            }
        }
        result
    }

    /// Streaming variant of [`process_inbound`]: emits [`AgentRunEvent`]s via `events_tx`
    /// in real-time so the frontend can display a live execution timeline.
    pub async fn process_inbound_streaming(
        &self,
        inbound: &InboundMessage,
        events_tx: tokio::sync::mpsc::UnboundedSender<serde_json::Value>,
    ) -> anyhow::Result<GatewayInboundResponse> {
        let started = std::time::Instant::now();
        let cfg = self.config.read().await.clone();
        let route = resolve_agent_route(&cfg, inbound);
        let security = SecurityContext::for_inbound(&cfg, inbound, &route);
        let channel_label = security.audit().context().channel.clone();
        crate::observability::record_inbound_request(&channel_label);
        security
            .audit_inbound_start(inbound.text.chars().count())
            .await;

        let mut steps = vec![ExecutionStep::done(
            "接收请求",
            format!(
                "channel={:?}, session={}, trace={}",
                inbound.channel,
                inbound.session_id.as_deref().unwrap_or("-"),
                security.trace_id()
            ),
        )];

        let _slot = acquire_inbound_slot(&cfg, &self.active_inbound)?;
        let _child_slot =
            acquire_subagent_guard(&cfg, inbound, &self.active_children_by_parent).await?;
        steps.push(ExecutionStep::done(
            "路由选择",
            format!(
                "Agent: {}{}{}",
                route.agent_name,
                route
                    .provider
                    .as_ref()
                    .map(|p| format!(", Provider: {p}"))
                    .unwrap_or_default(),
                route
                    .model
                    .as_ref()
                    .map(|m| format!(", Model: {m}"))
                    .unwrap_or_default()
            ),
        ));
        security
            .audit_route(&format!(
                "agent={} provider={:?} model={:?}",
                route.agent_name, route.provider, route.model
            ))
            .await;

        let agent_delegate = cfg.agents.get(&route.agent_name);
        let session_workspace_path: Option<std::path::PathBuf> = inbound
            .metadata
            .get("workspace_dir")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from);
        let effective_workspace = resolve_effective_workspace_dir(
            session_workspace_path.as_deref(),
            agent_delegate.and_then(|d| d.workspace_dir.as_deref()),
            &cfg.workspace_dir,
        );
        let effective_workspace = match effective_workspace {
            Some(w) if !w.as_os_str().is_empty() => w,
            _ => {
                let message = WORKSPACE_REQUIRED_MESSAGE;
                steps.push(ExecutionStep::error("Workspace", message));
                anyhow::bail!(message);
            }
        };
        steps.push(ExecutionStep::done(
            "Workspace",
            format!("{}", effective_workspace.display()),
        ));
        security
            .audit_route(&format!(
                "agent={} workspace={}",
                route.agent_name,
                effective_workspace.display()
            ))
            .await;

        let lineage = self
            .validate_and_resolve_session_lineage(&cfg, inbound, &route.agent_name)
            .await?;

        let mut tools = create_tools_for_route(
            &cfg,
            &route.agent_name,
            self.memory.clone(),
            &effective_workspace,
        );

        if attach_delegate_tool(
            &cfg,
            self,
            &route.agent_name,
            inbound.session_id.as_deref(),
            &inbound.channel,
            lineage.spawn_depth,
            &mut tools,
        ) {
            steps.push(ExecutionStep::done("加载委托工具", "已启用 delegate 工具"));
        }
        steps.push(ExecutionStep::done(
            "加载工具",
            format!("可用工具数：{}", tools.len()),
        ));

        let mut agent_cfg = cfg.agent.clone();
        if let Some(delegate) = cfg.agents.get(&route.agent_name) {
            if let Some(prompt) = &delegate.system_prompt {
                agent_cfg.system_prompt = Some(prompt.clone());
            }
        }
        agent_cfg.max_tool_iterations = resolve_agent_max_tool_iterations(&cfg, &route.agent_name);

        {
            let workspace_note = format!(
                "\n[环境信息] 当前 Workspace 目录是：{}。\n[工具路径规则] 调用文件、搜索、Shell working_directory 或 Git 工具时，所有 path 必须是 workspace-relative；Workspace 根目录用 \".\"。不要把 D:\\、E:\\ 或完整 Workspace 绝对路径传给工具。写 index.html 时 path 只能是 \"index.html\"。编辑已存在文件时优先 file_read 后使用 file_patch；新建文件才使用 file_write，除非用户明确要求整文件重写。\n",
                effective_workspace.display()
            );
            let current = agent_cfg.system_prompt.unwrap_or_default();
            agent_cfg.system_prompt = Some(format!("{current}{workspace_note}"));
        }

        if cfg.skills.open_skills_enabled {
            let skills_dir = cfg
                .skills
                .open_skills_dir
                .as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| effective_workspace.join("skills"));
            if let Ok(skills) = load_skills_from_dir(&skills_dir) {
                let prompt = format_skills_prompt(&skills);
                if !prompt.is_empty() {
                    let current = agent_cfg.system_prompt.unwrap_or_default();
                    agent_cfg.system_prompt = Some(format!("{}\n{}", current, prompt));
                    steps.push(ExecutionStep::done(
                        "加载技能提示",
                        "已注入 workspace skills",
                    ));
                }
            }
        }

        let agent_security = security.clone();
        let mut agent = Agent::new(
            build_provider_from_config(&cfg),
            tools,
            self.memory.clone(),
            agent_cfg.clone(),
            agent_security.clone(),
        );

        // Check for stateless mode - skip session history loading
        let is_stateless = inbound.metadata.get("stateless")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if is_stateless {
            steps.push(ExecutionStep::done("加载会话历史", "stateless 模式跳过"));
        } else if let Some(session_id) = inbound.session_id.as_deref() {
            let _guard = self.session_store_guard.lock().await;
            match load_session_history(&cfg, &inbound.channel, session_id).await {
                Ok(history) if !history.is_empty() => {
                    let sanitized = sanitize_messages_for_provider(history);
                    steps.push(ExecutionStep::done(
                        "加载会话历史",
                        format!("历史消息数：{}", sanitized.len()),
                    ));
                    agent.import_messages(sanitized);
                }
                Ok(_) => steps.push(ExecutionStep::done("加载会话历史", "无历史消息")),
                Err(e) => {
                    steps.push(ExecutionStep::error("加载会话历史", e.to_string()));
                    warn!("failed to load session history for {}: {}", session_id, e);
                }
            }
        }

        // Use the frontend-provided run_id if available, otherwise generate a new one.
        let run_id = metadata_str(inbound, &["run_id"])
            .map(String::from)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let session_key = inbound
            .session_id
            .clone()
            .unwrap_or_else(|| format!("{:?}:default", inbound.channel));
        let cancel_token = match self
            .run_registry
            .start_run(run_id.clone(), session_key)
            .await
        {
            Ok(token) => token,
            Err(err) => {
                if let Ok(v) = serde_json::to_value(&AgentRunEvent::run_failed {
                    run_id: run_id.clone(),
                    error: err.to_string(),
                }) {
                    let _ = events_tx.send(v);
                }
                return Err(err);
            }
        };
        let run_guard = ActiveRunGuard::new(self.run_registry.clone(), run_id.clone());

        let events_tx_inner = events_tx.clone();
        let emit_fn = move |evt: crate::agent::AgentRunEvent| {
            if let Ok(v) = serde_json::to_value(&evt) {
                let _ = events_tx_inner.send(v);
            }
        };

        // Forward the frontend's run_id and session_id through to the agent.
        let session_id = inbound.session_id.as_deref().map(String::from);
        tracing::debug!(target: "e2e", "[e2e-gateway-agent-call] timestamp={} run_id={}", now_ts(), run_id);
        let result = agent
            .process_message_with_events_streaming(
                &inbound.text,
                Box::new(emit_fn),
                Some(run_id.clone()),
                session_id,
                cancel_token.clone(),
            )
            .await;
        tracing::debug!(target: "e2e", "[e2e-gateway-agent-return] timestamp={} run_id={}", now_ts(), run_id);
        run_guard.finish().await;

        let reply_text = match &result {
            Ok((reply, _)) => {
                security
                    .audit_inbound_complete(true, &format!("reply_len={}", reply.len()))
                    .await;
                crate::observability::record_inbound_duration(
                    &channel_label,
                    "ok",
                    started.elapsed().as_secs_f64(),
                );
                reply.clone()
            }
            Err(err) => {
                crate::observability::record_inbound_error("process_inbound_streaming");
                security
                    .audit_inbound_complete(false, &err.to_string())
                    .await;
                crate::observability::record_inbound_duration(
                    &channel_label,
                    "error",
                    started.elapsed().as_secs_f64(),
                );
                return Err(anyhow::anyhow!(err.to_string()));
            }
        };

        tracing::debug!(target: "e2e", "[e2e-gateway-return] timestamp={} run_id={} reply_len={}", now_ts(), run_id, reply_text.len());
        Ok(GatewayInboundResponse {
            route,
            reply: reply_text,
            steps,
        })
    }

    /// Debug-only: directly executes a shell command and streams output as agent-run-events.
    /// Does NOT go through LLM. Used to verify runtime streaming infrastructure.
    pub async fn debug_shell_stream(
        &self,
        command: String,
        run_id: String,
        events_tx: tokio::sync::mpsc::UnboundedSender<serde_json::Value>,
    ) -> anyhow::Result<()> {
        use crate::agent::event_bus::EventBus;
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, BufReader};

        let events_tx_inner = events_tx.clone();
        let emit_fn = move |evt: crate::agent::AgentRunEvent| {
            if let Ok(v) = serde_json::to_value(&evt) {
                let _ = events_tx_inner.send(v);
            }
        };

        let (bus, drain_handle) = EventBus::new(run_id.clone(), emit_fn);

        tokio::spawn(async move {
            drain_handle.drain().await;
        });

        bus.run_started("debug-shell".to_string(), None, None);

        // Resolve cwd: prefer workspace root, fallback to current_dir.
        let cfg = self.config.read().await.clone();
        let workspace = cfg.workspace_dir.clone();
        drop(cfg);

        let cwd = if workspace.exists() {
            tracing::debug!(target: "e2e", "[e2e-debug-cwd] cwd={} exists=true", workspace.display());
            workspace
        } else {
            let fallback = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            tracing::debug!(target: "e2e", "[e2e-debug-cwd] cwd={} exists={}", fallback.display(), fallback.exists());
            if !fallback.exists() {
                bus.run_failed(format!(
                    "debug_shell_stream cwd does not exist: {}",
                    fallback.display()
                ));
                return Err(anyhow::anyhow!(
                    "cwd does not exist: {}",
                    fallback.display()
                ));
            }
            fallback
        };

        let step_id = bus.step_started("debug shell streaming".to_string(), None);
        let tool_call_id = uuid::Uuid::new_v4().to_string();
        bus.tool_started(
            tool_call_id.clone(),
            "shell".to_string(),
            format!("执行: {}", truncate_for_step(&command, 50)),
            None,
        );

        let preview_cmd = truncate_for_step(&command, 80);
        tracing::debug!(target: "e2e", "[e2e-debug-spawn] program=cmd.exe arg0=/C command={} cwd={} exists={}",
            preview_cmd, cwd.display(), cwd.exists());

        let mut child = tokio::process::Command::new("cmd.exe")
            .arg("/C")
            .arg(&command)
            .current_dir(&cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("failed to spawn: {}", e))?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let step_id_out = step_id.clone();
        let tool_call_id_out = tool_call_id.clone();
        let bus_for_stdout = bus.clone();
        let stdout_handle = tokio::spawn(async move {
            if let Some(out) = stdout {
                let mut lines = BufReader::new(out).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    bus_for_stdout.command_output(
                        step_id_out.clone(),
                        tool_call_id_out.clone(),
                        "shell".to_string(),
                        line,
                        false,
                    );
                }
            }
        });

        let step_id_err = step_id.clone();
        let tool_call_id_err = tool_call_id.clone();
        let bus_for_stderr = bus.clone();
        let stderr_handle = tokio::spawn(async move {
            if let Some(err) = stderr {
                let mut lines = BufReader::new(err).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    bus_for_stderr.command_output(
                        step_id_err.clone(),
                        tool_call_id_err.clone(),
                        "shell".to_string(),
                        line,
                        true,
                    );
                }
            }
        });

        let status = child
            .wait()
            .await
            .map_err(|e| anyhow::anyhow!("wait error: {}", e))?;
        let _ = stdout_handle.await;
        let _ = stderr_handle.await;

        let success = status.success();
        bus.tool_completed(
            step_id,
            tool_call_id,
            "shell".to_string(),
            success,
            0,
            String::new(),
            None,
        );
        let reply = if success {
            "命令执行成功".to_string()
        } else {
            "命令执行失败".to_string()
        };
        bus.run_completed(reply.clone(), reply);

        Ok(())
    }

    async fn validate_and_resolve_session_lineage(
        &self,
        cfg: &Config,
        inbound: &InboundMessage,
        route_agent_name: &str,
    ) -> anyhow::Result<SessionLineageMeta> {
        let Some(session_id) = inbound.session_id.as_deref() else {
            return Ok(SessionLineageMeta::default());
        };
        let key = session_key(&inbound.channel, session_id);
        let requested_parent_key = metadata_str(inbound, &["parent_session_id", "parentSessionId"])
            .map(|parent| session_key(&inbound.channel, parent));
        let requested_parent_agent_id =
            metadata_str(inbound, &["parent_agent_id", "parentAgentId"]).map(ToString::to_string);
        if requested_parent_agent_id.is_some() && requested_parent_key.is_none() {
            anyhow::bail!("parentAgentId requires parentSessionId");
        }
        let requested_depth = metadata_u32(inbound, &["spawn_depth", "spawnDepth"]);

        {
            let mut tree = self.session_tree.write().await;
            if let Some(existing) = tree.get_mut(&key) {
                if let Some(parent_key) = requested_parent_key.as_ref() {
                    if existing.parent_session_key.as_ref() != Some(parent_key) {
                        anyhow::bail!("session parent mismatch for '{}'", session_id);
                    }
                }
                if let Some(depth) = requested_depth {
                    if existing.spawn_depth != depth {
                        anyhow::bail!("session depth mismatch for '{}'", session_id);
                    }
                }
                if let Some(parent_agent_id) = requested_parent_agent_id.as_ref() {
                    if existing.parent_agent_id.as_deref() != Some(parent_agent_id.as_str()) {
                        anyhow::bail!("session parent agent mismatch for '{}'", session_id);
                    }
                }
                if existing.agent_name.as_deref() != Some(route_agent_name) {
                    anyhow::bail!("session agent mismatch for '{}'", session_id);
                }
                existing.updated_at = now_unix_ts();
                return Ok(existing.clone());
            }
        }

        if let Some(record) = load_session_record(cfg, &inbound.channel, session_id).await? {
            let resolved = SessionLineageMeta {
                parent_session_key: record.parent_session_key,
                parent_agent_id: record.parent_agent_id,
                agent_name: record.agent_name,
                spawn_depth: record.spawn_depth,
                updated_at: now_unix_ts(),
            };
            if let Some(parent_key) = requested_parent_key.as_ref() {
                if resolved.parent_session_key.as_ref() != Some(parent_key) {
                    anyhow::bail!("session parent mismatch for '{}'", session_id);
                }
            }
            if let Some(depth) = requested_depth {
                if resolved.spawn_depth != depth {
                    anyhow::bail!("session depth mismatch for '{}'", session_id);
                }
            }
            if let Some(parent_agent_id) = requested_parent_agent_id.as_ref() {
                if resolved.parent_agent_id.as_deref() != Some(parent_agent_id.as_str()) {
                    anyhow::bail!("session parent agent mismatch for '{}'", session_id);
                }
            }
            if resolved.agent_name.as_deref() != Some(route_agent_name) {
                anyhow::bail!("session agent mismatch for '{}'", session_id);
            }
            let mut tree = self.session_tree.write().await;
            tree.insert(key, resolved.clone());
            return Ok(resolved);
        }

        let mut resolved_parent_agent_id = requested_parent_agent_id;
        let resolved_parent_key = requested_parent_key;
        let resolved_depth = match resolved_parent_key.as_ref() {
            Some(parent_key) => {
                let parent_meta = self.resolve_parent_lineage(cfg, parent_key).await?;
                if let Some(expected_parent_agent_id) = resolved_parent_agent_id.as_ref() {
                    if parent_meta.agent_name.as_deref() != Some(expected_parent_agent_id.as_str())
                    {
                        anyhow::bail!(
                            "parentAgentId '{}' does not match parent session agent",
                            expected_parent_agent_id
                        );
                    }
                } else {
                    resolved_parent_agent_id = parent_meta.agent_name.clone();
                }
                let parent_depth = parent_meta.spawn_depth;
                let inferred = parent_depth.saturating_add(1);
                if let Some(depth) = requested_depth {
                    if depth != inferred {
                        anyhow::bail!(
                            "session depth mismatch: expected {}, got {}",
                            inferred,
                            depth
                        );
                    }
                }
                inferred
            }
            None => requested_depth.unwrap_or(0),
        };

        if let Some(max_depth) = cfg
            .agent_defaults_extended
            .subagents
            .as_ref()
            .and_then(|s| s.max_spawn_depth)
        {
            if resolved_depth > max_depth {
                anyhow::bail!(
                    "subagent spawn depth {} exceeds limit {}",
                    resolved_depth,
                    max_depth
                );
            }
        }

        let resolved = SessionLineageMeta {
            parent_session_key: resolved_parent_key,
            parent_agent_id: resolved_parent_agent_id,
            agent_name: Some(route_agent_name.to_string()),
            spawn_depth: resolved_depth,
            updated_at: now_unix_ts(),
        };
        let mut tree = self.session_tree.write().await;
        tree.insert(key, resolved.clone());
        Ok(resolved)
    }

    async fn resolve_parent_lineage(
        &self,
        cfg: &Config,
        parent_key: &str,
    ) -> anyhow::Result<SessionLineageMeta> {
        {
            let tree = self.session_tree.read().await;
            if let Some(node) = tree.get(parent_key) {
                return Ok(node.clone());
            }
        }
        if let Some(record) = load_session_record_by_key(cfg, parent_key).await? {
            return Ok(SessionLineageMeta {
                parent_session_key: record.parent_session_key,
                parent_agent_id: record.parent_agent_id,
                agent_name: record.agent_name,
                spawn_depth: record.spawn_depth,
                updated_at: record.updated_at,
            });
        }
        anyhow::bail!("parent session not found: {}", parent_key)
    }

    pub async fn estop_status(&self) -> anyhow::Result<EstopState> {
        let cfg = self.config.read().await.clone();
        EstopController::from_config(&cfg).load().await
    }

    pub async fn estop_pause(
        &self,
        level: Option<String>,
        domain: Option<String>,
        tool: Option<String>,
        reason: Option<String>,
    ) -> anyhow::Result<EstopState> {
        let cfg = self.config.read().await.clone();
        crate::observability::record_estop_event("pause");
        EstopController::from_config(&cfg)
            .pause(level, domain, tool, reason)
            .await
    }

    pub async fn estop_resume(&self) -> anyhow::Result<EstopState> {
        let cfg = self.config.read().await.clone();
        crate::observability::record_estop_event("resume");
        EstopController::from_config(&cfg).resume().await
    }

    pub async fn list_approvals(&self, pending_only: bool) -> anyhow::Result<Vec<PendingApproval>> {
        let cfg = self.config.read().await.clone();
        ApprovalController::from_workspace(&cfg.workspace_dir)
            .list(pending_only)
            .await
    }

    pub async fn approve_request(
        &self,
        id: &str,
        approved_by: Option<String>,
    ) -> anyhow::Result<PendingApproval> {
        let cfg = self.config.read().await.clone();
        crate::observability::record_approval_event("approved");
        ApprovalController::from_workspace(&cfg.workspace_dir)
            .approve(id, approved_by)
            .await
    }

    pub async fn reject_request(
        &self,
        id: &str,
        reason: Option<String>,
    ) -> anyhow::Result<PendingApproval> {
        let cfg = self.config.read().await.clone();
        crate::observability::record_approval_event("rejected");
        ApprovalController::from_workspace(&cfg.workspace_dir)
            .reject(id, reason)
            .await
    }

    pub async fn session_tree_snapshot(&self) -> anyhow::Result<GatewaySessionTreeResponse> {
        self.session_tree_snapshot_filtered(&GatewaySessionTreeQuery::default())
            .await
    }

    /// ????? UI ?????????? system/tool ??????????
    pub async fn get_session_history(
        &self,
        channel: &ChannelKind,
        session_id: &str,
    ) -> GatewaySessionHistoryResponse {
        let cfg = self.config.read().await.clone();
        let messages = load_session_history(&cfg, channel, session_id)
            .await
            .unwrap_or_default();
        let updated_at = load_session_record(&cfg, channel, session_id)
            .await
            .ok()
            .flatten()
            .map(|r| r.updated_at);
        GatewaySessionHistoryResponse {
            session_id: session_id.to_string(),
            channel: channel_label(channel),
            messages: messages_for_chat_ui(&messages),
            updated_at,
        }
    }

    /// ????????????????????????????
    /// ????????????
    pub async fn delete_session(
        &self,
        channel: &ChannelKind,
        session_id: &str,
    ) -> anyhow::Result<bool> {
        let cfg = self.config.read().await.clone();
        let key = session_key(channel, session_id);

        // ?????
        {
            let mut tree = self.session_tree.write().await;
            tree.remove(&key);
        }

        // ?????
        let path = session_store_path(&cfg);
        let mut store = load_session_store(&path).await?;
        let removed = store.sessions.remove(&key).is_some();
        if removed {
            let serialized = serde_json::to_string_pretty(&store)?;
            atomic_write_string(&path, &serialized).await?;
        }
        Ok(removed)
    }

    pub async fn session_tree_snapshot_filtered(
        &self,
        query: &GatewaySessionTreeQuery,
    ) -> anyhow::Result<GatewaySessionTreeResponse> {
        let query = normalize_session_tree_query(query);
        if let (Some(min_depth), Some(max_depth)) = (query.min_spawn_depth, query.max_spawn_depth) {
            if min_depth > max_depth {
                anyhow::bail!("min_spawn_depth cannot be greater than max_spawn_depth");
            }
        }
        let cfg = self.config.read().await.clone();
        let now = now_unix_ts();
        let path = session_store_path(&cfg);
        let mut merged: HashMap<String, GatewaySessionTreeNode> = HashMap::new();

        let persisted = load_session_store(&path).await?;
        for (session_key, record) in persisted.sessions {
            if now - record.updated_at > cfg.gateway.session_ttl_secs as i64 {
                continue;
            }
            merged.insert(
                session_key,
                GatewaySessionTreeNode {
                    session_key: None,
                    channel: None,
                    session_id: None,
                    parent_session_key: record.parent_session_key,
                    parent_agent_id: record.parent_agent_id,
                    agent_name: record.agent_name,
                    spawn_depth: record.spawn_depth,
                    updated_at: record.updated_at,
                    source: "persisted".to_string(),
                },
            );
        }

        {
            let in_memory = self.session_tree.read().await;
            for (session_key, meta) in in_memory.iter() {
                let source = if merged.contains_key(session_key) {
                    "memory+persisted"
                } else {
                    "memory"
                };
                merged.insert(
                    session_key.clone(),
                    GatewaySessionTreeNode {
                        session_key: None,
                        channel: None,
                        session_id: None,
                        parent_session_key: meta.parent_session_key.clone(),
                        parent_agent_id: meta.parent_agent_id.clone(),
                        agent_name: meta.agent_name.clone(),
                        spawn_depth: meta.spawn_depth,
                        updated_at: meta.updated_at,
                        source: source.to_string(),
                    },
                );
            }
        }

        let mut sessions = merged
            .into_iter()
            .map(|(session_key, mut node)| {
                let (channel, session_id) = split_session_key(&session_key);
                node.session_key = Some(session_key);
                node.channel = channel;
                node.session_id = session_id;
                node
            })
            .collect::<Vec<_>>();
        let total_before_filter = sessions.len();
        sessions.retain(|entry| match_session_tree_filters(entry, &query));
        let total_after_filter = sessions.len();
        let source_counts_after_filter = count_session_sources(&sessions);
        let stats_after_filter = compute_session_tree_stats(&sessions);
        sort_session_tree_entries(&mut sessions, &query);
        let offset = query.offset.unwrap_or(0);
        if offset >= sessions.len() {
            sessions.clear();
        } else if offset > 0 {
            sessions = sessions.split_off(offset);
        }
        if let Some(limit) = query.limit {
            sessions.truncate(limit);
        }
        let returned = sessions.len();
        let has_more = offset.saturating_add(returned) < total_after_filter;
        let next_offset = if has_more {
            Some(offset.saturating_add(returned))
        } else {
            None
        };
        let prev_offset = if offset > 0 {
            Some(offset.saturating_sub(query.limit.unwrap_or(offset)))
        } else {
            None
        };

        let active_children_by_parent = self.active_children_by_parent.read().await.clone();
        Ok(GatewaySessionTreeResponse {
            sessions,
            active_children_by_parent,
            total_before_filter,
            total_after_filter,
            returned,
            offset,
            limit: query.limit,
            has_more,
            next_offset,
            prev_offset,
            next_cursor: next_offset,
            prev_cursor: prev_offset,
            source_counts_after_filter,
            stats_after_filter,
        })
    }

    async fn ensure_not_stopped(&self) -> anyhow::Result<()> {
        let cfg = self.config.read().await.clone();
        let estop = EstopController::from_config(&cfg);
        if estop.is_paused().await? {
            anyhow::bail!("agent is paused by emergency stop");
        }
        Ok(())
    }

    async fn validate_webhook_replay(&self, headers: &HeaderMap) -> anyhow::Result<()> {
        let cfg = self.config.read().await.clone();
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let ts = match headers
            .get("x-omninova-timestamp")
            .and_then(|v| v.to_str().ok())
        {
            Some(raw) => raw
                .parse::<i64>()
                .map_err(|e| anyhow::anyhow!("invalid x-omninova-timestamp header: {e}"))?,
            None => {
                if cfg.gateway.webhook_require_nonce {
                    anyhow::bail!("missing x-omninova-timestamp header")
                }
                return Ok(());
            }
        };

        if (now - ts).abs() > cfg.gateway.webhook_max_skew_secs as i64 {
            anyhow::bail!("webhook timestamp is outside allowed skew window");
        }

        let nonce = match headers
            .get("x-omninova-nonce")
            .and_then(|v| v.to_str().ok())
        {
            Some(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => {
                if cfg.gateway.webhook_require_nonce {
                    anyhow::bail!("missing x-omninova-nonce header")
                }
                return Ok(());
            }
        };

        let cache_key = format!("{nonce}:{ts}");
        let mut cache = self.webhook_nonces.write().await;
        cache.retain(|_, seen_at| now - *seen_at <= cfg.gateway.webhook_nonce_ttl_secs as i64);
        if cache.contains_key(&cache_key) {
            anyhow::bail!("replayed webhook request detected");
        }
        cache.insert(cache_key, now);
        Ok(())
    }

    /// Start an HTTP gateway server with `/`, `/health`, `/chat`, `/config`.
    pub async fn serve_http(mut self) -> anyhow::Result<()> {
        let cfg = self.get_config().await;
        
        // Log config path and Feishu configuration summary
        println!(
            "[config] loaded path={}",
            cfg.config_path.display()
        );
        
        // Log Feishu channel configuration
        if let Some(ref feishu) = cfg.channels_config.feishu {
            let app_id_present = feishu.extra.get("app_id")
                .and_then(|v| v.as_str())
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            let app_secret_present = feishu.extra.get("app_secret")
                .and_then(|v| v.as_str())
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            let outbound_mode = feishu.extra.get("outbound_mode")
                .and_then(|v| v.as_str())
                .unwrap_or("not_set");
            println!(
                "[config] feishu enabled={} app_id_present={} app_secret_present={} outbound_mode={}",
                feishu.enabled, app_id_present, app_secret_present, outbound_mode
            );
            if feishu.enabled {
                let security = FeishuSecurityConfig::from_entry(Some(feishu));
                if security.insecure {
                    let reason = if security.verification_token.is_some() {
                        "dev_mode_permits_unverified_requests"
                    } else {
                        "no_verification_token"
                    };
                    println!(
                        "[feishu-security] mode=dev insecure=true reason={reason}"
                    );
                } else {
                    println!(
                        "[feishu-security] mode={} verification_token_configured={} encrypt_key_configured={}",
                        security.mode.as_str(),
                        security.verification_token.is_some(),
                        security.encrypt_key.is_some()
                    );
                }
            }
        } else {
            println!(
                "[config] feishu enabled=false app_id_present=false app_secret_present=false outbound_mode=not_configured"
            );
        }
        
        let addr: SocketAddr = format!("{}:{}", cfg.gateway.host, cfg.gateway.port)
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid gateway bind address: {e}"))?;
        
        // Initialize Feishu SQLite store
        let config_dir = cfg.config_path.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| {
                // Fallback to home dir
                home::home_dir()
                    .map(|h| h.join(".omninova"))
                    .unwrap_or_else(|| std::path::PathBuf::from(".omninova"))
            });
        let feishu_store = match FeishuStore::open(&config_dir) {
            Ok(store) => {
                let store = Arc::new(store);
                Some(store)
            }
            Err(e) => {
                println!("[feishu-store] failed to open: {}", e);
                None
            }
        };
        self.feishu_store = feishu_store;
        
        // Perform recovery of pending jobs and outbox
        self.recover_pending().await;
        
        // Initialize Feishu async worker
        use crate::gateway::feishu_worker::{spawn_worker, FeishuWorkerState};
        let mut worker_state = FeishuWorkerState::new();
        let receiver = worker_state.take_receiver();
        let queue_len = worker_state.queue_len.clone();
        self.init_feishu_worker(worker_state.sender()).await;
        let runtime = self.clone();
        let _worker_handle = spawn_worker(receiver, runtime.clone(), queue_len);
        println!("[gateway] feishu_async_worker started");

        // Initialize DingTalk async worker
        self.init_dingtalk_worker().await;

        // Run retry/recovery worker to re-send retryable outbox (template/monitor_final)
        // and to abandon LLM final outbox (cannot be sent without storing full body).
        crate::gateway::feishu_worker::run_retry_worker_once(&runtime).await;

        if self.cron_store.is_none() {
            let cron_path = cfg.workspace_dir.join("cron.json");
            match crate::cron::CronStore::open(&cron_path).await {
                Ok(store) => self.cron_store = Some(store),
                Err(e) => warn!(
                    "failed to initialize cron store at {}: {e}",
                    cron_path.display()
                ),
            }
        }

        let app = Router::new()
            .route("/", get(http_root))
            .route("/health", get(http_health))
            .route("/chat", post(http_chat))
            .route("/route", post(http_route))
            .route("/ingress", post(http_ingress))
            .route("/webhook", post(http_webhook))
            .route("/webhook/wechat", post(http_wechat_webhook))
            .route("/webhook/feishu", post(http_feishu_webhook))
            .route("/webhook/lark", post(http_lark_webhook))
            .route("/webhook/dingtalk", post(http_dingtalk_webhook))
            // Alias documented in `config.template.toml` and used by the
            // real DingTalk enterprise app bot callback wizard:
            //   `https://<public-host>/api/v1/gateway/dingtalk/events`.
            // Both routes point to the same handler so the documented
            // URL works end-to-end while the legacy `/webhook/dingtalk`
            // path keeps working for anyone already wired up.
            .route("/api/v1/gateway/dingtalk/events", post(http_dingtalk_webhook))
            .route("/webhook/feishu/card", post(http_feishu_card_callback))
            .route("/sessions/tree", get(http_sessions_tree))
            .route("/estop/status", get(http_estop_status))
            .route("/estop/pause", post(http_estop_pause))
            .route("/estop/resume", post(http_estop_resume))
            .route("/approvals", get(http_approvals_list))
            .route("/approvals/{id}/approve", post(http_approvals_approve))
            .route("/approvals/{id}/reject", post(http_approvals_reject))
            .route("/config", get(http_get_config).post(http_set_config))
            .route("/api/status", get(http_api_status))
            .route("/api/tools", get(http_api_tools))
            .route(
                "/api/memory",
                get(http_api_memory_list)
                    .post(http_api_memory_store)
                    .delete(http_api_memory_forget),
            )
            .route("/api/doctor", get(http_api_doctor))
            .route("/api/cron", get(http_api_cron_list).post(http_api_cron_add))
            // Phone Agent (iOS/Android) compatibility API
            .route("/api/health", get(http_health))
            .route("/api/inbound", post(http_ingress))
            .route("/api/webhook", post(http_phone_conversation_sync))
            .route(
                "/api/skill/phone-call-assistant/rules",
                get(http_phone_spam_rules),
            )
            .route(
                "/api/skill/phone-call-assistant/extract",
                post(http_phone_extract_ack),
            )
            .route("/metrics", get(http_metrics))
            .route("/ws/chat", get(ws::ws_chat_handler))
            .with_state(self);

        if cfg.observability.prometheus_enabled {
            let metrics_port = cfg.observability.prometheus_port.unwrap_or(9090);
            let metrics_addr: SocketAddr = format!("{}:{}", cfg.gateway.host, metrics_port)
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid prometheus bind address: {e}"))?;
            let metrics_app = Router::new().route("/metrics", get(http_metrics_standalone));
            tokio::spawn(async move {
                match tokio::net::TcpListener::bind(metrics_addr).await {
                    Ok(listener) => {
                        info!(
                            "prometheus metrics listening on http://{}/metrics",
                            metrics_addr
                        );
                        if let Err(e) = axum::serve(listener, metrics_app).await {
                            warn!("prometheus metrics server stopped: {e}");
                        }
                    }
                    Err(e) => warn!("failed to bind prometheus metrics on {}: {e}", metrics_addr),
                }
            });
        }

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;
        Ok(())
    }
}

struct InboundSlotGuard {
    active: Arc<AtomicUsize>,
}

impl Drop for InboundSlotGuard {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::Release);
    }
}

struct ChildSlotGuard {
    parent_agent_id: Option<String>,
    active_children_by_parent: Arc<RwLock<HashMap<String, usize>>>,
}

impl Drop for ChildSlotGuard {
    fn drop(&mut self) {
        let Some(parent_agent_id) = self.parent_agent_id.clone() else {
            return;
        };
        let map = Arc::clone(&self.active_children_by_parent);
        tokio::spawn(async move {
            let mut lock = map.write().await;
            if let Some(count) = lock.get_mut(&parent_agent_id) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    lock.remove(&parent_agent_id);
                }
            }
        });
    }
}

fn acquire_inbound_slot(
    cfg: &Config,
    active: &Arc<AtomicUsize>,
) -> anyhow::Result<Option<InboundSlotGuard>> {
    let limit = cfg.agent_defaults_extended.max_concurrent.or_else(|| {
        cfg.agent_defaults_extended
            .subagents
            .as_ref()
            .and_then(|s| s.max_concurrent)
    });
    let Some(limit) = limit else {
        return Ok(None);
    };
    if limit == 0 {
        return Ok(None);
    }
    let limit = limit as usize;
    loop {
        let current = active.load(Ordering::Acquire);
        if current >= limit {
            anyhow::bail!("too many concurrent inbound requests (limit={limit})");
        }
        if active
            .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return Ok(Some(InboundSlotGuard {
                active: Arc::clone(active),
            }));
        }
    }
}

async fn acquire_subagent_guard(
    cfg: &Config,
    inbound: &InboundMessage,
    active_children_by_parent: &Arc<RwLock<HashMap<String, usize>>>,
) -> anyhow::Result<Option<ChildSlotGuard>> {
    let subagents = match &cfg.agent_defaults_extended.subagents {
        Some(v) => v,
        None => return Ok(None),
    };

    if let Some(max_depth) = subagents.max_spawn_depth {
        let depth = metadata_u32(inbound, &["spawn_depth", "spawnDepth"]).unwrap_or(0);
        if depth > max_depth {
            anyhow::bail!("subagent spawn depth {} exceeds limit {}", depth, max_depth);
        }
    }

    let Some(limit) = subagents.max_children_per_agent else {
        return Ok(None);
    };
    if limit == 0 {
        return Ok(None);
    }
    let Some(parent_agent_id) =
        metadata_str(inbound, &["parent_agent_id", "parentAgentId"]).map(str::to_string)
    else {
        return Ok(None);
    };

    let mut lock = active_children_by_parent.write().await;
    let count = lock.entry(parent_agent_id.clone()).or_insert(0);
    if *count >= limit as usize {
        anyhow::bail!(
            "subagent children limit exceeded for parent '{}' (limit={})",
            parent_agent_id,
            limit
        );
    }
    *count += 1;
    drop(lock);

    Ok(Some(ChildSlotGuard {
        parent_agent_id: Some(parent_agent_id),
        active_children_by_parent: Arc::clone(active_children_by_parent),
    }))
}

fn provider_supports_openai_vision(provider: Option<&str>) -> bool {
    let Some(name) = provider.map(str::to_ascii_lowercase) else {
        return true;
    };
    !matches!(name.as_str(), "anthropic" | "gemini" | "mock")
}

fn metadata_bool(inbound: &InboundMessage, keys: &[&str]) -> bool {
    keys.iter().any(|key| {
        inbound
            .metadata
            .get(*key)
            .and_then(|value| {
                value
                    .as_bool()
                    .or_else(|| value.as_str().map(|s| s == "true" || s == "1"))
            })
            .unwrap_or(false)
    })
}

fn metadata_string_array(inbound: &InboundMessage, keys: &[&str]) -> Vec<String> {
    for key in keys {
        let Some(value) = inbound.metadata.get(*key) else {
            continue;
        };
        if let Some(items) = value.as_array() {
            let urls = items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .filter(|url| !url.is_empty())
                .collect::<Vec<_>>();
            if !urls.is_empty() {
                return urls;
            }
        }
        if let Some(text) = value.as_str() {
            if !text.is_empty() {
                return vec![text.to_string()];
            }
        }
    }
    Vec::new()
}

fn collect_desktop_vision_images(cfg: &Config, inbound: &InboundMessage) -> Vec<String> {
    let requested = metadata_bool(
        inbound,
        &[
            "desktop_vision",
            "desktopVision",
            "include_desktop_vision",
            "includeDesktopVision",
        ],
    );
    if !cfg.multimodal.desktop_vision_enabled && !requested {
        return Vec::new();
    }

    metadata_string_array(
        inbound,
        &[
            "desktop_vision_images",
            "desktopVisionImages",
            "screen_images",
            "screenImages",
        ],
    )
}

fn metadata_u32(inbound: &InboundMessage, keys: &[&str]) -> Option<u32> {
    for key in keys {
        let Some(value) = inbound.metadata.get(*key) else {
            continue;
        };
        if let Some(v) = value.as_u64() {
            return u32::try_from(v).ok();
        }
        if let Some(v) = value.as_str() {
            if let Ok(parsed) = v.parse::<u32>() {
                return Some(parsed);
            }
        }
    }
    None
}

fn metadata_str<'a>(inbound: &'a InboundMessage, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| {
        inbound
            .metadata
            .get(*key)
            .and_then(serde_json::Value::as_str)
    })
}

fn extract_tool_steps(messages: &[ChatMessage]) -> Vec<ExecutionStep> {
    let mut steps = Vec::new();
    let mut tool_names_by_id = HashMap::new();

    for message in messages {
        match message.role.as_str() {
            "assistant" => {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&message.content) else {
                    continue;
                };
                let Some(tool_calls) = value
                    .get("tool_calls")
                    .and_then(serde_json::Value::as_array)
                else {
                    continue;
                };
                for call in tool_calls {
                    let id = call
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("-");
                    let name = call
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("unknown_tool");
                    let args = call
                        .get("arguments")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("");
                    tool_names_by_id.insert(id.to_string(), name.to_string());
                    steps.push(ExecutionStep::done(
                        format!("调用工具：{name}"),
                        truncate_for_step(args, 240),
                    ));
                }
            }
            "tool" => {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&message.content) else {
                    continue;
                };
                let tool_call_id = value
                    .get("tool_call_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("-");
                let name = tool_names_by_id
                    .get(tool_call_id)
                    .map(String::as_str)
                    .unwrap_or("unknown_tool");
                let content = value
                    .get("content")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                steps.push(ExecutionStep::done(
                    format!("工具完成：{name}"),
                    truncate_for_step(content, 300),
                ));
            }
            _ => {}
        }
    }

    steps
}

fn truncate_for_step(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    let mut out = String::new();
    for (idx, ch) in trimmed.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayHealth {
    pub ok: bool,
    pub provider: String,
    pub provider_healthy: bool,
    pub memory_healthy: bool,
}

/// Normalize a user-entered public webhook URL to the configured base URL.
///
/// The persisted value never contains a Feishu endpoint path. This keeps one
/// source of truth for both the normal event and card callback URLs.
pub fn normalize_public_webhook_base_url(value: &str) -> Option<String> {
    let trimmed = value
        .trim()
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    let endpoint_len = [
        "/api/v1/gateway/dingtalk/events",
        "/webhook/feishu/card",
        "/webhook/feishu",
        "/webhook/dingtalk",
    ]
    .iter()
    .find_map(|suffix| lower.ends_with(suffix).then_some(suffix.len()))
    .unwrap_or(0);
    let without_endpoint = trimmed[..trimmed.len().saturating_sub(endpoint_len)]
        .trim_end_matches('/');

    if without_endpoint.is_empty()
        || ((without_endpoint.starts_with("http://")
            || without_endpoint.starts_with("https://"))
            && reqwest::Url::parse(without_endpoint)
                .ok()
                .is_some_and(|url| !url.username().is_empty() || url.password().is_some()))
    {
        None
    } else {
        Some(without_endpoint.to_string())
    }
}

/// Normalize a named Cloudflare Tunnel hostname without retaining credentials,
/// query parameters, fragments, or callback paths.
pub fn normalize_named_tunnel_hostname(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let candidate = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let parsed = reqwest::Url::parse(&candidate).ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return None;
    }
    parsed
        .host_str()
        .map(|hostname| hostname.trim_end_matches('.').to_ascii_lowercase())
        .filter(|hostname| !hostname.is_empty())
}

pub fn normalize_gateway_public_config(config: &mut GatewayPublicConfig) {
    config.public_webhook_base_url = config
        .public_webhook_base_url
        .as_deref()
        .and_then(normalize_public_webhook_base_url);
    config.cloudflared_path = config
        .cloudflared_path
        .take()
        .filter(|path| !path.as_os_str().is_empty());
    config.named_tunnel_name = config
        .named_tunnel_name
        .take()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty());
    config.named_tunnel_hostname = config
        .named_tunnel_hostname
        .as_deref()
        .and_then(normalize_named_tunnel_hostname);

    if matches!(config.mode, GatewayPublicMode::NamedCloudflareTunnel) {
        config.public_webhook_base_url = config
            .named_tunnel_hostname
            .as_ref()
            .map(|hostname| format!("https://{hostname}"));
    }
}

pub fn cloudflared_available(config: &Config) -> bool {
    if config
        .gateway_public
        .cloudflared_path
        .as_ref()
        .filter(|path| !path.as_os_str().is_empty())
        .is_some_and(|path| path.is_file())
    {
        return true;
    }
    let Some(path_value) = std::env::var_os("PATH") else {
        return false;
    };
    let candidates = if cfg!(windows) {
        ["cloudflared.exe", "cloudflared"]
    } else {
        ["cloudflared", "cloudflared"]
    };
    std::env::split_paths(&path_value).any(|directory| {
        candidates
            .iter()
            .any(|candidate| directory.join(candidate).is_file())
    })
}

pub fn resolve_public_webhook_base_url(config: &Config) -> Option<String> {
    let mut gateway_public = config.gateway_public.clone();
    normalize_gateway_public_config(&mut gateway_public);
    let configured = gateway_public.public_webhook_base_url;
    if configured.is_some() {
        return configured;
    }

    config
        .channels_config
        .feishu
        .as_ref()
        .and_then(|entry| entry.extra.get("public_webhook_base_url"))
        .and_then(serde_json::Value::as_str)
        .and_then(normalize_public_webhook_base_url)
}

pub fn feishu_public_callback_urls(config: &Config) -> (Option<String>, Option<String>) {
    let Some(base) = resolve_public_webhook_base_url(config) else {
        return (None, None);
    };
    (
        Some(format!("{base}/webhook/feishu")),
        Some(format!("{base}/webhook/feishu/card")),
    )
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct GatewayPublicHealthStatus {
    pub configured: bool,
    pub ok: bool,
    pub base_url: Option<String>,
    pub checked_url: Option<String>,
    pub checked_at: Option<i64>,
    pub status_code: Option<u16>,
    pub error_kind: Option<String>,
    pub error: Option<String>,
}

impl GatewayPublicHealthStatus {
    pub fn not_configured() -> Self {
        Self {
            configured: false,
            ok: false,
            base_url: None,
            checked_url: None,
            checked_at: None,
            status_code: None,
            error_kind: Some("url_not_configured".to_string()),
            error: Some("Public Base URL 未配置。".to_string()),
        }
    }

    pub fn not_checked(base_url: String) -> Self {
        Self {
            configured: true,
            ok: false,
            base_url: Some(base_url),
            checked_url: None,
            checked_at: None,
            status_code: None,
            error_kind: Some("not_checked".to_string()),
            error: Some("公网 Health 尚未检测。".to_string()),
        }
    }
}

fn classify_public_health_error(error: &reqwest::Error) -> (&'static str, String) {
    let lower = error.to_string().to_ascii_lowercase();
    if error.is_timeout() {
        ("timeout", "公网 Health 检查连接超时。".to_string())
    } else if lower.contains("dns")
        || lower.contains("name resolution")
        || lower.contains("failed to lookup")
    {
        ("dns_error", "公网域名 DNS 解析失败。".to_string())
    } else if lower.contains("certificate") || lower.contains("tls") || lower.contains("ssl") {
        ("tls_error", "公网入口 TLS 校验失败。".to_string())
    } else if error.is_connect() {
        (
            "connection_error",
            "无法连接公网入口，请确认隧道和 Gateway 已启动。".to_string(),
        )
    } else {
        ("request_error", "公网 Health 检查请求失败。".to_string())
    }
}

pub async fn check_gateway_public_health(config: &Config) -> GatewayPublicHealthStatus {
    let Some(base_url) = resolve_public_webhook_base_url(config) else {
        return GatewayPublicHealthStatus::not_configured();
    };
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
    {
        Ok(client) => client,
        Err(_) => {
            return GatewayPublicHealthStatus {
                configured: true,
                ok: false,
                base_url: Some(base_url),
                checked_url: None,
                checked_at: Some(now_unix_ts()),
                status_code: None,
                error_kind: Some("client_error".to_string()),
                error: Some("无法创建公网 Health 检查客户端。".to_string()),
            };
        }
    };

    let mut last_status = None;
    for suffix in ["/health", "/api/health"] {
        let checked_url = format!("{base_url}{suffix}");
        match client.get(&checked_url).send().await {
            Ok(response) => {
                let status = response.status().as_u16();
                last_status = Some(status);
                if status == 200 {
                    return GatewayPublicHealthStatus {
                        configured: true,
                        ok: true,
                        base_url: Some(base_url),
                        checked_url: Some(checked_url),
                        checked_at: Some(now_unix_ts()),
                        status_code: Some(status),
                        error_kind: None,
                        error: None,
                    };
                }
                if status != 404 {
                    let (kind, message) = if matches!(status, 502 | 503 | 504) {
                        (
                            "cloudflare_origin_error",
                            "公网入口可达，但 Cloudflare 无法连接本地 Gateway。".to_string(),
                        )
                    } else {
                        (
                            "http_error",
                            format!("公网 Health 检查返回 HTTP {status}。"),
                        )
                    };
                    return GatewayPublicHealthStatus {
                        configured: true,
                        ok: false,
                        base_url: Some(base_url),
                        checked_url: Some(checked_url),
                        checked_at: Some(now_unix_ts()),
                        status_code: Some(status),
                        error_kind: Some(kind.to_string()),
                        error: Some(message),
                    };
                }
            }
            Err(error) => {
                let (kind, message) = classify_public_health_error(&error);
                return GatewayPublicHealthStatus {
                    configured: true,
                    ok: false,
                    base_url: Some(base_url),
                    checked_url: Some(checked_url),
                    checked_at: Some(now_unix_ts()),
                    status_code: None,
                    error_kind: Some(kind.to_string()),
                    error: Some(message),
                };
            }
        }
    }

    GatewayPublicHealthStatus {
        configured: true,
        ok: false,
        base_url: Some(base_url.clone()),
        checked_url: Some(format!("{base_url}/api/health")),
        checked_at: Some(now_unix_ts()),
        status_code: last_status,
        error_kind: Some("health_endpoint_not_found".to_string()),
        error: Some("公网入口可达，但未找到 Health 接口。".to_string()),
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DingtalkPublicRouteProbe {
    pub configured: bool,
    pub reachable: bool,
    pub status_code: Option<u16>,
    pub result_kind: String,
    pub message: String,
}

fn classify_dingtalk_route_response(status_code: u16, body: &str) -> DingtalkPublicRouteProbe {
    let lower = body.to_ascii_lowercase();
    if lower.contains("1033") {
        return DingtalkPublicRouteProbe {
            configured: true,
            reachable: false,
            status_code: Some(status_code),
            result_kind: "tunnel_unreachable".to_string(),
            message: "公网隧道不可达或地址已过期。".to_string(),
        };
    }
    if lower.contains("missing_timestamp") || lower.contains("missing_sign") {
        return DingtalkPublicRouteProbe {
            configured: true,
            reachable: true,
            status_code: Some(status_code),
            result_kind: "missing_timestamp".to_string(),
            message: "路由可达，缺少签名头；这是手动空请求的预期结果。".to_string(),
        };
    }
    if lower.contains("signature_mismatch") || lower.contains("invalid_sign") {
        return DingtalkPublicRouteProbe {
            configured: true,
            reachable: true,
            status_code: Some(status_code),
            result_kind: "signature_mismatch".to_string(),
            message: "路由可达，签名未通过；这是无有效签名手动请求的预期结果。".to_string(),
        };
    }
    if (200..300).contains(&status_code) {
        return DingtalkPublicRouteProbe {
            configured: true,
            reachable: true,
            status_code: Some(status_code),
            result_kind: "ok".to_string(),
            message: "钉钉公网回调路由可达。".to_string(),
        };
    }
    DingtalkPublicRouteProbe {
        configured: true,
        reachable: false,
        status_code: Some(status_code),
        result_kind: "http_error".to_string(),
        message: format!("钉钉公网回调路由返回 HTTP {status_code}。"),
    }
}

/// Probe only route reachability with an empty JSON body. No platform secret,
/// signature, user message, or identifier is sent or retained.
pub async fn check_dingtalk_public_route(base_url: &str) -> DingtalkPublicRouteProbe {
    let Some(base_url) = normalize_public_webhook_base_url(base_url) else {
        return DingtalkPublicRouteProbe {
            configured: false,
            reachable: false,
            status_code: None,
            result_kind: "not_configured".to_string(),
            message: "Public Base URL 未配置。".to_string(),
        };
    };
    let callback_url = format!("{base_url}/api/v1/gateway/dingtalk/events");
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
    {
        Ok(client) => client,
        Err(_) => {
            return DingtalkPublicRouteProbe {
                configured: true,
                reachable: false,
                status_code: None,
                result_kind: "client_error".to_string(),
                message: "无法创建公网路由检测客户端。".to_string(),
            };
        }
    };

    match client.post(callback_url).json(&serde_json::json!({})).send().await {
        Ok(response) => {
            let status_code = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            classify_dingtalk_route_response(status_code, &body)
        }
        Err(error) => DingtalkPublicRouteProbe {
            configured: true,
            reachable: false,
            status_code: None,
            result_kind: if error.is_timeout() {
                "timeout"
            } else {
                "network_error"
            }
            .to_string(),
            message: "公网隧道不可达或地址已过期，请检查 cloudflared。".to_string(),
        },
    }
}

/// Productized runtime status — safe to return to UI/CLI without leaking secrets.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayRuntimeStatus {
    pub running: bool,
    pub bind_host: String,
    pub bind_port: u16,
    pub local_base_url: String,
    /// The configured public webhook base URL, if any.
    pub public_webhook_base_url: Option<String>,
    pub gateway_public_mode: String,
    pub quick_tunnel_non_production: bool,
    pub cloudflared_configured: bool,
    pub cloudflared_found: bool,
    pub named_tunnel_name_configured: bool,
    pub named_tunnel_hostname_configured: bool,
    pub named_tunnel_config_complete: bool,
    /// The auto-generated feishu normal event callback URL.
    pub feishu_webhook_url: Option<String>,
    /// The auto-generated feishu card interaction callback URL.
    pub feishu_card_callback_url: Option<String>,
    pub enabled_channels: Vec<String>,
    pub security_mode: Option<String>,
    pub outbound_mode: Option<String>,
    pub store_opened: bool,
    /// Sanitized store path (never contains secrets).
    pub store_path: Option<String>,
    pub retry_worker_enabled: bool,
    /// Human-readable timestamp of last successful start.
    pub last_started_at: Option<i64>,
    /// Human-readable last error message.
    pub last_error: Option<String>,
    pub health_ok: bool,
    pub public_health: GatewayPublicHealthStatus,
}

impl GatewayRuntimeStatus {
    /// Build a status snapshot from the current config and runtime internals.
    /// Does NOT include any secrets, tokens, or full payloads.
    pub async fn from_runtime(
        running: bool,
        runtime: &GatewayRuntime,
        last_started_at: Option<i64>,
        last_error: Option<String>,
    ) -> Self {
        let cfg = runtime.get_config().await;
        let store = runtime.feishu_store();

        let feishu_cfg = cfg.channels_config.feishu.as_ref();
        let security_mode = feishu_cfg
            .and_then(|e| e.extra.get("security_mode"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let outbound_mode = feishu_cfg
            .and_then(|e| e.extra.get("outbound_mode"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let public_webhook_base = resolve_public_webhook_base_url(&cfg);
        let named_tunnel_name_configured = cfg
            .gateway_public
            .named_tunnel_name
            .as_deref()
            .is_some_and(|name| !name.trim().is_empty());
        let named_tunnel_hostname_configured = cfg
            .gateway_public
            .named_tunnel_hostname
            .as_deref()
            .and_then(normalize_named_tunnel_hostname)
            .is_some();
        let public_health = public_webhook_base
            .clone()
            .map(GatewayPublicHealthStatus::not_checked)
            .unwrap_or_else(GatewayPublicHealthStatus::not_configured);
        let local_base_url = format!("http://{}:{}", cfg.gateway.host, cfg.gateway.port);
        let callback_base = public_webhook_base
            .as_deref()
            .unwrap_or(local_base_url.as_str());
        let feishu_webhook_url = Some(format!("{callback_base}/webhook/feishu"));
        let feishu_card_callback_url =
            Some(format!("{callback_base}/webhook/feishu/card"));

        let enabled_channels = {
            let mut list = Vec::new();
            let c = &cfg.channels_config;
            if c.feishu.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("feishu".to_string()); }
            if c.lark.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("lark".to_string()); }
            if c.slack.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("slack".to_string()); }
            if c.telegram.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("telegram".to_string()); }
            if c.discord.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("discord".to_string()); }
            if c.whatsapp.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("whatsapp".to_string()); }
            if c.msteams.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("msteams".to_string()); }
            if c.email.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("email".to_string()); }
            if c.webhook.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("webhook".to_string()); }
            if c.wechat.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("wechat".to_string()); }
            if c.dingtalk.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("dingtalk".to_string()); }
            if c.google_chat.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("google_chat".to_string()); }
            if c.matrix.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("matrix".to_string()); }
            if c.signal.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("signal".to_string()); }
            if c.irc.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("irc".to_string()); }
            if c.synology_chat.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("synology_chat".to_string()); }
            if c.mattermost.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("mattermost".to_string()); }
            if c.nextcloud_talk.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("nextcloud_talk".to_string()); }
            if c.nostr.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("nostr".to_string()); }
            if c.twitch.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("twitch".to_string()); }
            if c.webchat.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("webchat".to_string()); }
            if c.line.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("line".to_string()); }
            if c.bluebubbles.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("bluebubbles".to_string()); }
            if c.imessage.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("imessage".to_string()); }
            if c.tlon.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("tlon".to_string()); }
            if c.zalo.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("zalo".to_string()); }
            if c.zalo_personal.as_ref().map(|e| e.enabled).unwrap_or(false) { list.push("zalo_personal".to_string()); }
            list
        };

        let config_dir = cfg
            .config_path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| cfg.workspace_dir.clone());
        let expected_store_path = config_dir.join("state.sqlite");
        let store_path = Some(
            store
                .as_ref()
                .map(|s| s.db_path().to_path_buf())
                .unwrap_or(expected_store_path)
                .to_string_lossy()
                .to_string(),
        );
        // `serve_http` owns a cloned runtime. If the caller keeps the original
        // runtime (as the desktop app does), the presence of the initialized
        // SQLite file is the safe non-mutating fallback. `gateway doctor`
        // performs the authoritative openability check.
        let store_opened = running
            && (store.is_some()
                || store_path
                    .as_deref()
                    .map(std::path::Path::new)
                    .is_some_and(std::path::Path::exists));

        let health = runtime.health().await;
        let retry_worker_enabled = running
            && store_opened
            && feishu_cfg.map(|entry| entry.enabled).unwrap_or(false);

        Self {
            running,
            bind_host: cfg.gateway.host.clone(),
            bind_port: cfg.gateway.port,
            local_base_url,
            public_webhook_base_url: public_webhook_base,
            gateway_public_mode: cfg.gateway_public.mode.as_str().to_string(),
            quick_tunnel_non_production: matches!(
                cfg.gateway_public.mode,
                crate::config::schema::GatewayPublicMode::QuickTunnel
            ),
            cloudflared_configured: cfg
                .gateway_public
                .cloudflared_path
                .as_ref()
                .is_some_and(|path| !path.as_os_str().is_empty()),
            cloudflared_found: cloudflared_available(&cfg),
            named_tunnel_name_configured,
            named_tunnel_hostname_configured,
            named_tunnel_config_complete: named_tunnel_name_configured
                && named_tunnel_hostname_configured,
            feishu_webhook_url,
            feishu_card_callback_url,
            enabled_channels,
            security_mode,
            outbound_mode,
            store_opened,
            store_path,
            retry_worker_enabled,
            last_started_at,
            last_error,
            health_ok: running && health.ok,
            public_health,
        }
    }

    /// Returns true when dev mode is active without any secrets configured.
    pub fn is_insecure_dev(&self) -> bool {
        self.security_mode.as_deref() == Some("dev")
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayChatRequest {
    pub message: String,
    pub session_id: Option<String>,
    pub user_id: Option<String>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayChatResponse {
    pub reply: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayConfigUpdateResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayRouteRequest {
    pub channel: Option<ChannelKind>,
    pub text: String,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayInboundResponse {
    pub route: RouteDecision,
    pub reply: String,
    #[serde(default)]
    pub steps: Vec<ExecutionStep>,
}

/// Response structure for platform webhooks (Feishu/Lark)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlatformWebhookResponse {
    pub ok: bool,
    pub channel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_reply: Option<String>,
    #[serde(default)]
    pub outbound_delivery: OutboundDeliveryStatus,
    /// Outbound result details (without secrets)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outbound_result: Option<OutboundResultSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl PlatformWebhookResponse {
    pub fn success(
        channel: &str,
        message_id: Option<String>,
        conversation_id: Option<String>,
        reply: String,
    ) -> Self {
        Self {
            ok: true,
            channel: channel.to_string(),
            message_id,
            conversation_id,
            agent_reply: Some(reply),
            outbound_delivery: OutboundDeliveryStatus::HttpResponseOnly,
            outbound_result: None,
            error: None,
        }
    }

    pub fn success_with_outbound(
        channel: &str,
        message_id: Option<String>,
        conversation_id: Option<String>,
        reply: String,
        outbound_result: OutboundResultSummary,
    ) -> Self {
        Self {
            ok: true,
            channel: channel.to_string(),
            message_id,
            conversation_id,
            agent_reply: Some(reply),
            outbound_delivery: outbound_result.delivery.clone(),
            outbound_result: Some(outbound_result),
            error: None,
        }
    }

    pub fn error(channel: &str, error: impl Into<String>) -> Self {
        Self {
            ok: false,
            channel: channel.to_string(),
            message_id: None,
            conversation_id: None,
            agent_reply: None,
            outbound_delivery: OutboundDeliveryStatus::NotImplemented,
            outbound_result: None,
            error: Some(error.into()),
        }
    }

    pub fn challenge(channel: &str) -> Self {
        Self {
            ok: true,
            channel: channel.to_string(),
            message_id: None,
            conversation_id: None,
            agent_reply: None,
            outbound_delivery: OutboundDeliveryStatus::NotImplemented,
            outbound_result: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionStep {
    pub title: String,
    pub status: String,
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub run_events: Vec<RunEvent>,
}

/// Enriched run events produced during a tool call loop.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RunEvent {
    ToolStarted {
        tool_call_id: String,
        tool_name: String,
        summary: String,
    },
    ToolCompleted {
        tool_call_id: String,
        tool_name: String,
        success: bool,
        duration_ms: u64,
        result_summary: String,
        diff_stats: Option<RunDiffStats>,
    },
    CommandOutput {
        tool_call_id: String,
        tool_name: String,
        output: String,
        is_stderr: bool,
    },
    FileChanged {
        path: String,
        additions: i32,
        deletions: i32,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunDiffStats {
    pub additions: i32,
    pub deletions: i32,
}

impl From<ToolExecutionEvent> for RunEvent {
    fn from(evt: ToolExecutionEvent) -> Self {
        match evt {
            ToolExecutionEvent::Started {
                tool_call_id,
                tool_name,
                summary,
            } => RunEvent::ToolStarted {
                tool_call_id,
                tool_name,
                summary,
            },
            ToolExecutionEvent::Completed {
                tool_call_id,
                tool_name,
                success,
                duration_ms,
                result_summary,
                diff_stats,
            } => RunEvent::ToolCompleted {
                tool_call_id,
                tool_name,
                success,
                duration_ms,
                result_summary,
                diff_stats: diff_stats.map(|d| RunDiffStats {
                    additions: d.additions,
                    deletions: d.deletions,
                }),
            },
            ToolExecutionEvent::CommandOutput {
                tool_call_id,
                tool_name,
                output,
                is_stderr,
            } => RunEvent::CommandOutput {
                tool_call_id,
                tool_name,
                output,
                is_stderr,
            },
            ToolExecutionEvent::FileChanged {
                path,
                additions,
                deletions,
            } => RunEvent::FileChanged {
                path,
                additions,
                deletions,
            },
        }
    }
}

impl ExecutionStep {
    fn done(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            status: "done".to_string(),
            detail: Some(detail.into()),
            run_events: Vec::new(),
        }
    }

    fn running(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            status: "running".to_string(),
            detail: Some(detail.into()),
            run_events: Vec::new(),
        }
    }

    fn error(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            status: "error".to_string(),
            detail: Some(detail.into()),
            run_events: Vec::new(),
        }
    }

    fn with_events(mut self, events: Vec<RunEvent>) -> Self {
        self.run_events = events;
        self
    }
}

// Re-exports for backward compatibility with external crate users.
pub use crate::agent::AgentRunEvent;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewaySessionTreeResponse {
    pub sessions: Vec<GatewaySessionTreeNode>,
    #[serde(default)]
    pub active_children_by_parent: HashMap<String, usize>,
    #[serde(default)]
    pub total_before_filter: usize,
    #[serde(default)]
    pub total_after_filter: usize,
    #[serde(default)]
    pub returned: usize,
    #[serde(default)]
    pub offset: usize,
    pub limit: Option<usize>,
    #[serde(default)]
    pub has_more: bool,
    pub next_offset: Option<usize>,
    pub prev_offset: Option<usize>,
    pub next_cursor: Option<usize>,
    pub prev_cursor: Option<usize>,
    #[serde(default)]
    pub source_counts_after_filter: HashMap<String, usize>,
    pub stats_after_filter: GatewaySessionTreeStats,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct GatewaySessionTreeStats {
    #[serde(default)]
    pub unique_agents: usize,
    #[serde(default)]
    pub unique_parent_agents: usize,
    #[serde(default)]
    pub max_spawn_depth: u32,
    #[serde(default)]
    pub min_updated_at: i64,
    #[serde(default)]
    pub max_updated_at: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct GatewaySessionTreeQuery {
    pub session_id: Option<String>,
    pub session_key: Option<String>,
    pub parent_session_id: Option<String>,
    pub parent_session_key: Option<String>,
    pub agent_name: Option<String>,
    pub parent_agent_id: Option<String>,
    pub channel: Option<String>,
    pub source: Option<String>,
    pub min_spawn_depth: Option<u32>,
    pub max_spawn_depth: Option<u32>,
    pub contains: Option<String>,
    pub case_insensitive: Option<bool>,
    pub cursor: Option<usize>,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
}

/// ???????????????
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewaySessionHistoryResponse {
    pub session_id: String,
    pub channel: String,
    pub messages: Vec<GatewayChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewaySessionTreeNode {
    pub session_key: Option<String>,
    pub channel: Option<String>,
    pub session_id: Option<String>,
    pub parent_session_key: Option<String>,
    pub parent_agent_id: Option<String>,
    pub agent_name: Option<String>,
    pub spawn_depth: u32,
    pub updated_at: i64,
    pub source: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct GatewayEstopPauseRequest {
    pub level: Option<String>,
    pub domain: Option<String>,
    pub tool: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct GatewayApprovalActionRequest {
    pub approved_by: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct GatewayApprovalsQuery {
    pub pending_only: Option<bool>,
}

async fn http_root() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "OmniNova Gateway",
        "health": "/health",
        "chat": "/chat",
        "config": "/config",
        "channel_webhooks": {
            "wechat": "/webhook/wechat",
            "feishu": "/webhook/feishu",
            "lark": "/webhook/lark",
            "dingtalk": "/webhook/dingtalk"
        }
    }))
}

async fn http_health(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<GatewayHealth>, Json<GatewayError>> {
    Ok(Json(runtime.health().await))
}

#[derive(Debug, Clone, serde::Deserialize)]
struct PhoneConversationSyncRequest {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    session: serde_json::Value,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct PhoneExtractRequest {
    session_id: Option<String>,
}

/// iOS/Android 通话结束后上传会话 JSON（当前仅确认接收，抽取在端侧完成）。
async fn http_phone_conversation_sync(
    Json(body): Json<PhoneConversationSyncRequest>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    if body.kind != "conversation_sync" {
        return Err(Json(GatewayError {
            message: format!("unsupported phone event type: {}", body.kind),
        }));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// 网关侧暂不重复抽取；端侧 `KeyInfoExtractor` 已处理，此处返回 ack。
async fn http_phone_extract_ack(
    Json(body): Json<PhoneExtractRequest>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    Ok(Json(serde_json::json!({
        "ok": true,
        "message": "extraction handled on device",
        "session_id": body.session_id
    })))
}

fn resolve_phone_spam_rules_path(cfg: &Config) -> PathBuf {
    if let Some(dir) = cfg.skills.open_skills_dir.as_ref() {
        let candidate = PathBuf::from(dir).join("phone-call-assistant/spam_detection_rules.json");
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from("skills/phone-call-assistant/spam_detection_rules.json")
}

async fn http_phone_spam_rules(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    let cfg = runtime.get_config().await;
    let path = resolve_phone_spam_rules_path(&cfg);
    let content = tokio::fs::read_to_string(&path).await.map_err(|e| {
        Json(GatewayError {
            message: format!("phone spam rules not found at {}: {e}", path.display()),
        })
    })?;
    let value: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
        Json(GatewayError {
            message: format!("invalid phone spam rules JSON: {e}"),
        })
    })?;
    Ok(Json(value))
}

async fn http_chat(
    State(runtime): State<GatewayRuntime>,
    Json(req): Json<GatewayChatRequest>,
) -> Result<Json<GatewayChatResponse>, Json<GatewayError>> {
    let inbound = InboundMessage {
        channel: ChannelKind::Web,
        user_id: req.user_id,
        session_id: req.session_id,
        text: req.message,
        metadata: req.metadata,
    };
    match runtime.process_inbound(&inbound).await {
        Ok(resp) => Ok(Json(GatewayChatResponse { reply: resp.reply })),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_get_config(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<Config>, Json<GatewayError>> {
    Ok(Json(runtime.get_config().await))
}

async fn http_route(
    State(runtime): State<GatewayRuntime>,
    Json(req): Json<GatewayRouteRequest>,
) -> Result<Json<RouteDecision>, Json<GatewayError>> {
    let inbound = InboundMessage {
        channel: req.channel.unwrap_or(ChannelKind::Cli),
        user_id: req.user_id,
        session_id: req.session_id,
        text: req.text,
        metadata: req.metadata,
    };
    Ok(Json(runtime.route(&inbound).await))
}

async fn http_set_config(
    State(runtime): State<GatewayRuntime>,
    Json(config): Json<Config>,
) -> Result<Json<GatewayConfigUpdateResponse>, Json<GatewayError>> {
    match runtime.set_config(config).await {
        Ok(()) => Ok(Json(GatewayConfigUpdateResponse { ok: true })),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_sessions_tree(
    State(runtime): State<GatewayRuntime>,
    Query(query): Query<GatewaySessionTreeQuery>,
) -> Result<Json<GatewaySessionTreeResponse>, Json<GatewayError>> {
    match runtime.session_tree_snapshot_filtered(&query).await {
        Ok(snapshot) => Ok(Json(snapshot)),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_ingress(
    State(runtime): State<GatewayRuntime>,
    Json(req): Json<GatewayRouteRequest>,
) -> Result<Json<GatewayInboundResponse>, Json<GatewayError>> {
    let cfg = runtime.get_config().await;

    // Security: Validate channel if explicitly specified
    let channel = req.channel.unwrap_or(ChannelKind::Cli);
    if channel != ChannelKind::Cli {
        // Non-CLI channels require explicit enablement
        if !is_channel_enabled(&cfg, &channel) {
            return Err(Json(GatewayError {
                message: format!("{:?} channel is disabled", channel),
            }));
        }
    }

    let inbound = InboundMessage {
        channel,
        user_id: req.user_id,
        session_id: req.session_id,
        text: req.text,
        metadata: req.metadata,
    };
    match runtime.process_inbound(&inbound).await {
        Ok(resp) => Ok(Json(resp)),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_webhook(
    State(runtime): State<GatewayRuntime>,
    headers: HeaderMap,
    raw_body: String,
) -> Result<Json<GatewayInboundResponse>, Json<GatewayError>> {
    let cfg = runtime.get_config().await;
    if let Some(secret) = webhook_signing_secret(&cfg) {
        let allowed_algorithms = cfg
            .gateway
            .webhook_signature_algorithms
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let priority_algorithms = cfg
            .gateway
            .webhook_signature_priority
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let signature = headers
            .get("x-omninova-signature")
            .or_else(|| headers.get("x-signature"))
            .or_else(|| headers.get("x-hub-signature-256"))
            .and_then(|v| v.to_str().ok());
        let signed_payload = signed_webhook_payload(&cfg, &headers, &raw_body).map_err(|e| {
            Json(GatewayError {
                message: e.to_string(),
            })
        })?;
        let verified = verify_webhook_signature_with_policy_options(
            &signed_payload,
            signature,
            &secret,
            &allowed_algorithms,
            &priority_algorithms,
            cfg.gateway.webhook_signature_strict_priority,
        )
        .map_err(|e| {
            Json(GatewayError {
                message: e.to_string(),
            })
        })?;
        if !verified {
            return Err(Json(GatewayError {
                message: "invalid webhook signature".to_string(),
            }));
        }
    }
    runtime
        .validate_webhook_replay(&headers)
        .await
        .map_err(|e| {
            Json(GatewayError {
                message: e.to_string(),
            })
        })?;

    let payload: WebhookInboundPayload = serde_json::from_str(&raw_body).map_err(|e| {
        Json(GatewayError {
            message: format!("invalid webhook payload: {e}"),
        })
    })?;
    let inbound = inbound_from_webhook(payload);
    match runtime.process_inbound(&inbound).await {
        Ok(resp) => Ok(Json(resp)),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_wechat_webhook(
    State(runtime): State<GatewayRuntime>,
    headers: HeaderMap,
    raw_body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<GatewayError>)> {
    http_channel_webhook(runtime, headers, raw_body, ChannelKind::Wechat).await
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct FeishuWebhookError {
    error: String,
}

#[derive(Debug, Clone, Copy)]
struct FeishuTokenSource<'a> {
    value: &'a str,
    source: &'static str,
}

const FEISHU_TOKEN_SOURCES_CHECKED: [&str; 6] = [
    "top_level",
    "header",
    "event",
    "event_header",
    "x-feishu-verification-token",
    "x-lark-verification-token",
];

fn extract_feishu_verification_token<'a>(
    payload: &'a serde_json::Value,
    headers: &'a HeaderMap,
) -> Option<FeishuTokenSource<'a>> {
    let payload_sources = [
        (
            payload.get("token").and_then(serde_json::Value::as_str),
            "top_level.token",
        ),
        (
            payload
                .pointer("/header/token")
                .and_then(serde_json::Value::as_str),
            "header.token",
        ),
        (
            payload
                .pointer("/event/token")
                .and_then(serde_json::Value::as_str),
            "event.token",
        ),
        (
            payload
                .pointer("/event/header/token")
                .and_then(serde_json::Value::as_str),
            "event.header.token",
        ),
    ];
    for (value, source) in payload_sources {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            return Some(FeishuTokenSource { value, source });
        }
    }

    for (header_name, source) in [
        (
            "x-feishu-verification-token",
            "header.x-feishu-verification-token",
        ),
        (
            "x-lark-verification-token",
            "header.x-lark-verification-token",
        ),
    ] {
        if let Some(value) = headers
            .get(header_name)
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty())
        {
            return Some(FeishuTokenSource { value, source });
        }
    }

    None
}

fn safe_json_object_keys(value: Option<&serde_json::Value>) -> Vec<String> {
    let Some(object) = value.and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    let mut keys = object
        .keys()
        .take(32)
        .map(|key| {
            key.chars()
                .take(64)
                .map(|character| {
                    if character.is_ascii_alphanumeric()
                        || matches!(character, '_' | '-' | '.')
                    {
                        character
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    keys.sort();
    keys
}

fn feishu_request_content_type(headers: &HeaderMap) -> &'static str {
    match headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    {
        Some(value) if value.to_ascii_lowercase().starts_with("application/json") => {
            "application/json"
        }
        Some(_) => "other",
        None => "missing",
    }
}

fn feishu_token_missing_diagnostic_lines(
    payload: &serde_json::Value,
    headers: &HeaderMap,
) -> Vec<String> {
    let top_keys = safe_json_object_keys(Some(payload));
    let header_keys = safe_json_object_keys(payload.get("header"));
    let event_keys = safe_json_object_keys(payload.get("event"));
    let event_type_present = payload.pointer("/header/event_type").is_some();
    let event_message_present = payload.pointer("/event/message").is_some();
    let looks_like_message_event = payload
        .pointer("/header/event_type")
        .and_then(serde_json::Value::as_str)
        == Some("im.message.receive_v1")
        && event_message_present;

    vec![
        format!("[feishu-security] payload_shape top_keys={top_keys:?}"),
        format!("[feishu-security] header_shape keys={header_keys:?}"),
        format!("[feishu-security] event_shape keys={event_keys:?}"),
        format!(
            "[feishu-security] request_shape method=POST path=/webhook/feishu content_type={} type_present={} header_event_type_present={} event_message_present={} looks_like_im_message_receive_v1={}",
            feishu_request_content_type(headers),
            payload.get("type").is_some(),
            event_type_present,
            event_message_present,
            looks_like_message_event,
        ),
        format!(
            "[feishu-security] token_extract token_present=false sources_checked={:?}",
            FEISHU_TOKEN_SOURCES_CHECKED
        ),
    ]
}

fn feishu_webhook_error_code(message: &str) -> &'static str {
    match message {
        "invalid_json" => "invalid_json",
        "token_missing" => "token_missing",
        "token_mismatch" => "token_mismatch",
        "decrypt_failed" => "decrypt_failed",
        "encrypt_key_missing" => "encrypt_key_missing",
        "missing_challenge" => "missing_challenge",
        "unsupported_event" => "unsupported_event",
        "invalid_security_mode" => "invalid_security_mode",
        "encrypted_payload_required" => "encrypted_payload_required",
        "channel_disabled" => "channel_disabled",
        "signature_invalid" => "signature_invalid",
        "replay_rejected" => "replay_rejected",
        "internal_error" => "internal_error",
        _ if message.contains("invalid json") || message.contains("invalid channel webhook payload") => {
            "invalid_json"
        }
        _ if message.contains("verification_token mismatch") => "token_mismatch",
        _ if message.contains("verification_token missing from request") => "token_missing",
        _ if message.contains("encrypt_key") && message.contains("not configured") => {
            "encrypt_key_missing"
        }
        _ if message.contains("decrypt") => "decrypt_failed",
        _ if message.contains("encrypted Feishu payload required")
            || message.contains("encrypt field missing") =>
        {
            "encrypted_payload_required"
        }
        _ if message.contains("channel is disabled") => "channel_disabled",
        _ if message.contains("signature") => "signature_invalid",
        _ if message.contains("timestamp")
            || message.contains("nonce")
            || message.contains("skew") =>
        {
            "replay_rejected"
        }
        _ if message.contains("does not contain a text message") => "unsupported_event",
        _ => "internal_error",
    }
}

async fn http_feishu_webhook(
    State(runtime): State<GatewayRuntime>,
    headers: HeaderMap,
    raw_body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<FeishuWebhookError>)> {
    http_channel_webhook(runtime, headers, raw_body, ChannelKind::Feishu)
        .await
        .map_err(|(status, Json(error))| {
            (
                status,
                Json(FeishuWebhookError {
                    error: feishu_webhook_error_code(&error.message).to_string(),
                }),
            )
        })
}

/// Feishu card action callback endpoint.
/// Feishu sends card interactions as JSON with a `action.value.action` field.
/// Security: uses the same security_mode as the main Feishu webhook.
async fn http_feishu_card_callback(
    state: State<GatewayRuntime>,
    headers: HeaderMap,
    raw_body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<GatewayError>)> {
    process_feishu_card_action_callback(state, headers, raw_body, "card_endpoint").await
}

async fn process_feishu_card_action_callback(
    State(runtime): State<GatewayRuntime>,
    headers: HeaderMap,
    raw_body: String,
    source: &'static str,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<GatewayError>)> {
    use crate::gateway::feishu_worker::{
        canonical_card_action, gateway_status_reply, help_reply,
        recent_jobs_reply_text, summarize_job_for_card, RecentJobLine,
    };

    let channel_name = "feishu";
    let cfg = runtime.get_config().await;
    let ack_start = Instant::now();
    println!("[feishu-card] callback_start source={source}");

    if !is_channel_enabled(&cfg, &ChannelKind::Feishu) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(GatewayError {
                message: "Feishu channel is disabled".to_string(),
            }),
        ));
    }

    let sec_cfg = FeishuSecurityConfig::from_entry(cfg.channels_config.feishu.as_ref());

        // Get token from header or payload
        let header_token = headers
            .get("x-lark-verification-token")
            .and_then(|v| v.to_str().ok())
            .map(String::from);
        let payload_token = extract_token_from_payload(&raw_body);
        let req_token = header_token.or(payload_token);

        if matches!(sec_cfg.mode, FeishuSecurityMode::Token | FeishuSecurityMode::Encrypted) {
            match req_token {
                Some(token) => {
                    match verify_feishu_verification_token(&sec_cfg, Some(token.as_str())) {
                        Ok(true) => {
                            println!("[feishu-card-security] token_verified=true security_mode={:?}", sec_cfg.mode);
                        }
                        Ok(false) => {
                            println!("[feishu-card-security] token_mismatch security_mode={:?}", sec_cfg.mode);
                            return Err((StatusCode::FORBIDDEN, Json(GatewayError {
                                message: "verification_token mismatch".to_string(),
                            })));
                        }
                        Err(e) => {
                            println!("[feishu-card-security] token_error={} security_mode={:?}", e, sec_cfg.mode);
                            return Err((StatusCode::FORBIDDEN, Json(GatewayError {
                                message: format!("verification_token error: {}", e),
                            })));
                        }
                    }
                }
                None => {
                    println!("[feishu-card-security] token_missing security_mode={:?}", sec_cfg.mode);
                    return Err((StatusCode::FORBIDDEN, Json(GatewayError {
                        message: "verification_token missing".to_string(),
                    })));
                }
            }
        }

    if matches!(sec_cfg.mode, FeishuSecurityMode::Dev | FeishuSecurityMode::Default) {
        println!("[feishu-card-security] mode=dev insecure=true reason=no_verification");
    }

    let payload: serde_json::Value = match serde_json::from_str(&raw_body) {
        Ok(p) => p,
        Err(e) => {
            let top_keys = payload_keys_summary(&raw_body);
            println!(
                "[feishu-card] invalid_json error={} payload_top_keys={:?}",
                e, top_keys
            );
            return Err((
                StatusCode::BAD_REQUEST,
                Json(GatewayError { message: format!("invalid json: {}", e) }),
            ));
        }
    };

    // Feishu validates the dedicated card callback URL with the same
    // top-level url_verification payload used by the normal event endpoint.
    // This must return before card action extraction because the challenge
    // intentionally has no event/action object.
    if payload.get("type").and_then(serde_json::Value::as_str)
        == Some("url_verification")
    {
        println!("[feishu-card] url_verification_detected");
        let challenge = payload
            .get("challenge")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(GatewayError {
                        message: "missing_challenge".to_string(),
                    }),
                )
            })?;
        println!("[feishu-card] challenge_response_sent");
        return Ok(Json(serde_json::json!({ "challenge": challenge })));
    }

    println!(
        "[feishu-card] payload_shape top_keys={:?} event_keys={:?}",
        safe_json_object_keys(Some(&payload)),
        safe_json_object_keys(payload.get("event"))
    );

    let Some((action, action_source)) = extract_feishu_card_action(&payload) else {
        println!("[feishu-card] action_missing");
        let ack_ms = ack_start.elapsed().as_millis() as i64;
        return Ok(Json(serde_json::json!({
            "toast": { "type": "warning", "content": "未识别卡片操作，请重新打开功能菜单。" },
            "ok": false,
            "ack_ms": ack_ms
        })));
    };

    let canonical = match canonical_card_action(&action) {
        Some(c) => c,
        None => {
            println!(
                "[feishu-card] action_extract action_present=true action=unknown source={} action_chars={}",
                action_source,
                action.chars().count()
            );
            println!("[feishu-security] unknown_action_rejected");
            let ack_ms = ack_start.elapsed().as_millis() as i64;
            return Ok(Json(serde_json::json!({
                "toast": { "type": "warning", "content": "未知操作，已安全忽略。请发送 / 重新打开功能菜单。" },
                "ok": false,
                "ack_ms": ack_ms
            })));
        }
    };

    println!(
        "[feishu-card] action_extract action_present=true action={} source={}",
        canonical, action_source
    );
    println!("[feishu-card] action_resolved action={}", canonical);

    let reply_context = extract_feishu_card_reply_context(&payload);
    println!(
        "[feishu-card] reply_context chat_id_present={} chat_id_source={} open_message_id_present={} open_message_id_source={}",
        reply_context.chat_id.is_some(),
        reply_context.chat_id_source.unwrap_or("none"),
        reply_context.open_message_id.is_some(),
        reply_context.open_message_id_source.unwrap_or("none"),
    );

    // ==== Tiered deduplication ====
    // Tier 1: event_id present       → 30-minute TTL  (strong id from Feishu)
    // Tier 2: only open_message_id   → 8-second TTL   (covers double-click bursts)
    // Tier 3: neither id available   → 3-second TTL   (covers double-click bursts,
    //         and explicitly does NOT lock the button out for 30 minutes).
    //
    // The dedup KEY is internal cache storage and is never printed; the audit
    // log only carries the boolean presence flags.
    let event_id_opt = payload
        .pointer("/header/event_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());
    let open_message_id_opt = payload
        .pointer("/event/context/open_message_id")
        .or_else(|| payload.pointer("/event/open_message_id"))
        .or_else(|| payload.pointer("/event/message/message_id"))
        .or_else(|| payload.pointer("/open_message_id"))
        .or_else(|| payload.pointer("/action/value/open_message_id"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());
    let event_id_present = event_id_opt.is_some();
    let open_message_id_present = open_message_id_opt.is_some();
    let dedup_cache = runtime.dedup_cache();

    // Compute (key, ttl_secs, tier_label) for the cache. We use FxHash-style
    // of mixing the value into the string itself so we never log the raw id.
    let (dedup_key, dedup_ttl_secs) = if let Some(ref eid) = event_id_opt {
        (
            format!(
                "feishu-card:event:{}:{}",
                hash_dedup_string(eid),
                canonical
            ),
            1800u64, // 30 minutes
        )
    } else if let Some(ref mid) = open_message_id_opt {
        (
            format!(
                "feishu-card:mid:{}:{}",
                hash_dedup_string(mid),
                canonical
            ),
            8u64, // 8 seconds
        )
    } else {
        // No id available at all. Use action+chat and cap to 3 seconds so a
        // single click cannot lock the user out for 30 minutes.
        (
            format!(
                "feishu-card:chat:{}:{}",
                canonical,
                reply_context.chat_id.as_deref().unwrap_or("<no-chat>")
            ),
            3u64, // 3 seconds
        )
    };

    let is_new = dedup_cache
        .check_and_insert_with_ttl(&dedup_key, dedup_ttl_secs)
        .await;
    if !is_new {
        // Duplicate: ACK immediately and skip dispatch entirely.
        println!(
            "[feishu-card] duplicate_action ignored=true event_id_present={} open_message_id_present={} action={}",
            event_id_present, open_message_id_present, canonical
        );
        let ack_ms = ack_start.elapsed().as_millis() as i64;
        // If chat_id is present, send a detached "still working" message so the user
        // sees that their double-click was caught (does NOT re-run monitor).
        if let Some(chat_id_dup) = reply_context.chat_id.clone() {
            let runtime_dup = runtime.clone();
            let canonical_dup = canonical.to_string();
            tokio::spawn(async move {
                let inbound_dup = InboundMessage {
                    text: format!("/card-action {canonical_dup} (dedup)"),
                    channel: ChannelKind::Feishu,
                    user_id: None,
                    session_id: Some(chat_id_dup),
                    metadata: std::collections::HashMap::from([(
                        "card_action".to_string(),
                        serde_json::Value::String(format!("{canonical_dup}_duplicate")),
                    )]),
                };
                crate::gateway::feishu_worker::send_card_action_result(
                    &runtime_dup,
                    &inbound_dup,
                    "已收到重复请求，跳过。",
                    crate::gateway::feishu_store::ReplyKind::CardActionResult,
                    "feishu",
                    &format!("dedup_{}", chrono_timestamp_simple()),
                    &format!("dedup_event_{}", chrono_timestamp_simple()),
                ).await;
            });
        }
        return Ok(Json(serde_json::json!({
            "toast": { "type": "info", "content": "已收到请求，已忽略重复点击" },
            "ok": true,
            "duplicate": true,
            "ack_ms": ack_ms,
            "action": canonical
        })));
    }
    println!(
        "[feishu-card] dedupe accepted event_id_present={} open_message_id_present={} chat_id_present={} action={}",
        event_id_present,
        open_message_id_present,
        reply_context.chat_id.is_some(),
        canonical
    );

    if reply_context.chat_id.is_none() {
        println!(
            "[feishu-card] missing_chat_id action={} ack_ok=true",
            canonical
        );
        let ack_ms = ack_start.elapsed().as_millis() as i64;
        // Spawn detached: nothing to send because chat_id is missing.
        let runtime_mc = runtime.clone();
        let canonical_mc = canonical.to_string();
        tokio::spawn(async move {
            // Ack-only path: do NOT run monitor, do NOT call send_text.
            // We only record an audit line.
            println!(
                "[feishu-card] missing_chat_id followup_no_send action={}",
                canonical_mc
            );
            let _ = runtime_mc;
        });
        return Ok(Json(serde_json::json!({
            "toast": { "type": "warning", "content": "未找到会话信息，操作未执行。请重新打开功能菜单。" },
            "ok": false,
            "action": canonical,
            "ack_ms": ack_ms
        })));
    }

    let chat_id = reply_context.chat_id.clone().expect("checked above");

    let timestamp = chrono_timestamp_simple();
    let event_key = payload
        .pointer("/header/event_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(|value| format!("card_event_{value}_{canonical}"))
        .unwrap_or_else(|| format!("card_{timestamp}_{canonical}"));
    let job_id = format!("card_{timestamp}_{canonical}");
    let mut metadata = std::collections::HashMap::new();
    metadata.insert(
        "card_action".to_string(),
        serde_json::Value::String(canonical.to_string()),
    );
    if let Some(message_id) = reply_context.open_message_id.clone() {
        metadata.insert(
            "message_id".to_string(),
            serde_json::Value::String(message_id),
        );
    }
    let inbound = crate::channels::InboundMessage {
        text: match canonical {
            "monitor_30s" => "/monitor 桌面 30秒".to_string(),
            "monitor_60s" => "/monitor 桌面 60秒".to_string(),
            _ => format!("/card-action {canonical}"),
        },
        channel: ChannelKind::Feishu,
        user_id: None,
        session_id: Some(chat_id.clone()),
        metadata,
    };

    // ==== Fast-ACK strategy ====
    // For monitor_* actions, the cheap channel-send enqueue must run synchronously
    // so the runtime's worker contract is preserved, but the heavy parts
    // (outbox writes, send_text progress, completion notification) all run in a
    // detached task. For other actions (gateway_status/recent_jobs/help),
    // even the reply-text generation runs in the detached task.

    let store = runtime.feishu_store();

    if matches!(canonical, "monitor_30s" | "monitor_60s") {
        // ==== Single-flight guard: only one monitor per chat at a time ====
        // CRITICAL: the owner-scoped lease is stored IN the job so the WORKER releases it
        // when the monitor actually completes (not when the HTTP callback returns).
        // The HTTP handler exits before the monitor runs, so RAII in the handler
        // would release too early.
        let guard = runtime.monitor_flight_guard();
        let monitor_duration_secs: u64 = if canonical == "monitor_30s" { 30 } else { 60 };
        let ttl_secs = monitor_guard_ttl_secs(monitor_duration_secs);
        let chat_id_str = chat_id.as_str();
        match guard.try_acquire_with_ttl(chat_id_str, ttl_secs).await {
            Some(lease) => {
                println!(
                    "[feishu-monitor] singleflight_acquired source=card action={} ttl_secs={} chat_present=true",
                    canonical, ttl_secs
                );
                // Non-blocking enqueue with a strict wall-clock deadline.
                // `try_send` is the primary path; we also wrap in `timeout` to defend
                // against any pathological lock contention on the sender clone itself.
                // Goal: NEVER block the HTTP callback waiting for the queue.
                let monitor_job = crate::gateway::feishu_worker::FeishuAsyncJob::new(
                    ChannelKind::Feishu,
                    inbound.clone(),
                    serde_json::json!({ "card_action": canonical }),
                    false,
                    event_key.clone(),
                    Some(job_id.clone()),
                )
                .with_monitor_guard_lease(lease.clone());

                let enqueue_dispatch = tokio::time::timeout(
                    std::time::Duration::from_millis(50),
                    runtime.try_send_feishu_job_nonblocking(monitor_job),
                )
                .await;
                let (queued_flag, busy_flag) = match enqueue_dispatch {
                    Ok(Ok(true)) => {
                        println!("[feishu-card] monitor_queued action={canonical}");
                        (true, false)
                    }
                    Ok(Ok(false)) => {
                        println!("[feishu-card] monitor_busy action={canonical} reason=queue_full");
                        (false, true)
                    }
                    Ok(Err(_)) | Err(_) => {
                        println!("[feishu-card] monitor_busy action={canonical} reason=timeout_or_uninit");
                        (false, true)
                    }
                };
                if !queued_flag {
                    let released = guard.release(&lease).await;
                    println!(
                        "[feishu-monitor] singleflight_released source=enqueue action={} success=false released={}",
                        canonical, released
                    );
                }

                // Detached task: outbox INSERT and any post-send bookkeeping. Must NOT block
                // the HTTP callback. Only runs when the job was actually accepted.
                if queued_flag {
                    let runtime_bg = runtime.clone();
                    let inbound_bg = inbound.clone();
                    let chat_id_bg = chat_id.clone();
                    let job_id_bg = job_id.clone();
                    let event_key_bg = event_key.clone();
                    let store_bg = store.clone();
                    tokio::spawn(async move {
                        if let Some(s) = store_bg {
                            let outbound_id = format!("{}_{}", job_id_bg, chrono_timestamp_simple());
                            let outbox_input = crate::gateway::feishu_store::FeishuOutboxInput {
                                outbound_id,
                                job_id: Some(job_id_bg.clone()),
                                event_key: Some(event_key_bg.clone()),
                                channel: "feishu".to_string(),
                                chat_id: Some(chat_id_bg),
                                reply_kind: Some("card_action_result".to_string()),
                                reply: None,
                                result_json: None,
                            };
                            let _ = s.insert_outbox(&outbox_input);
                        }
                        let _ = (runtime_bg, inbound_bg);
                    });
                }

                // ==== Fast ACK return (monitor path) ====
                let ack_ms = ack_start.elapsed().as_millis() as i64;
                println!(
                    "[feishu-card] callback_ack ack_ms={ack_ms} action={canonical} queued={queued_flag} busy={busy_flag}"
                );
                let toast_content = if queued_flag {
                    "已收到监控任务，正在执行。"
                } else {
                    "监控任务队列繁忙，请稍后再试。"
                };
                return Ok(Json(serde_json::json!({
                    "toast": { "type": "info", "content": toast_content },
                    "ok": true,
                    "action": canonical,
                    "queued": queued_flag,
                    "busy": busy_flag,
                    "ack_ms": ack_ms
                })));
            }
            None => {
                // Single-flight: a monitor is already running in this chat.
                println!(
                    "[feishu-monitor] singleflight_busy source=card action={} chat_present=true",
                    canonical
                );
                // Send a detached "already busy" message so the user doesn't
                // see an error. The HTTP ACK is still fast (no runtime hit).
                let runtime_busy = runtime.clone();
                let chat_id_busy = chat_id.clone();
                let canonical_busy = canonical.to_string();
                tokio::spawn(async move {
                    let inbound_busy = InboundMessage {
                        text: format!("/card-action {canonical_busy} (busy)"),
                        channel: ChannelKind::Feishu,
                        user_id: None,
                        session_id: Some(chat_id_busy.clone()),
                        metadata: std::collections::HashMap::from([(
                            "card_action".to_string(),
                            serde_json::Value::String(format!("{canonical_busy}_busy")),
                        )]),
                    };
                    crate::gateway::feishu_worker::send_card_action_result(
                        &runtime_busy,
                        &inbound_busy,
                        "已有桌面监控任务正在执行，请等待当前任务完成后再试。",
                        crate::gateway::feishu_store::ReplyKind::CardActionResult,
                        "feishu",
                        &format!("busy_{}", chrono_timestamp_simple()),
                        &format!("busy_event_{}", chrono_timestamp_simple()),
                    ).await;
                });
                let ack_ms = ack_start.elapsed().as_millis() as i64;
                println!(
                    "[feishu-card] callback_ack ack_ms={ack_ms} action={canonical} queued=false busy=true reason=singleflight"
                );
                return Ok(Json(serde_json::json!({
                    "toast": { "type": "info", "content": "已有桌面监控任务正在执行，请稍后再试" },
                    "ok": true,
                    "action": canonical,
                    "queued": false,
                    "busy": true,
                    "ack_ms": ack_ms
                })));
            }
        }
    }

    // Non-monitor actions: gateway_status / recent_jobs / help / unknown.
    // The reply_text itself is built inside the detached task so the HTTP
    // callback is not slowed down by SQLite reads or large status strings.
    {
        let runtime_bg = runtime.clone();
        let inbound_bg = inbound.clone();
        let canonical_bg = canonical.to_string();
        let job_id_bg = job_id.clone();
        let event_key_bg = event_key.clone();
        tokio::spawn(async move {
            let store = runtime_bg.feishu_store();
            let cfg_bg = runtime_bg.get_config().await;
            let sec_cfg_bg = FeishuSecurityConfig::from_entry(
                cfg_bg.channels_config.feishu.as_ref(),
            );

            // Build the status / jobs text inside the task.
            let reply_text: String = match canonical_bg.as_str() {
                "gateway_status" => {
                    let security_mode_str = sec_cfg_bg.mode.as_str();
                    let verification_token_configured =
                        sec_cfg_bg.verification_token.is_some();
                    let encrypt_key_configured = sec_cfg_bg.encrypt_key.is_some();
                    let outbound_mode = cfg_bg
                        .channels_config
                        .feishu
                        .as_ref()
                        .and_then(|e| e.extra.get("outbound_mode"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("disabled");
                    let store_path_exists = store.is_some();
                    let store_path = store
                        .as_ref()
                        .map(|s| s.db_path().to_string_lossy().to_string())
                        .unwrap_or_else(|| "(no store)".to_string());
                    let (pending_jobs, pending_outbox) = store
                        .as_ref()
                        .and_then(|s| s.get_store_stats().ok())
                        .map(|stats| (stats.jobs_total, stats.outbox_total))
                        .unwrap_or((0, 0));
                    gateway_status_reply(
                        Some(security_mode_str),
                        verification_token_configured,
                        encrypt_key_configured,
                        Some(outbound_mode),
                        store_path_exists,
                        &store_path,
                        pending_jobs,
                        pending_outbox,
                    )
                }
                "recent_jobs" => match store.as_ref() {
                    Some(s) => match s.get_recent_jobs(5) {
                        Ok(jobs) => {
                            let lines: Vec<RecentJobLine> = jobs
                                .into_iter()
                                .map(|j| {
                                    summarize_job_for_card(
                                        &j.job_id,
                                        &j.mode,
                                        &j.status.as_str(),
                                        j.attempts as i64,
                                        j.error_code.as_ref().map(|s| s.as_str()),
                                        j.created_at,
                                        j.completed_at,
                                    )
                                })
                                .collect();
                            recent_jobs_reply_text(&lines)
                        }
                        Err(_) => "最近任务暂不可用：store unavailable".to_string(),
                    },
                    None => "最近任务暂不可用：store unavailable".to_string(),
                },
                "help" => help_reply(),
                _ => "未知操作，已忽略。请发送 / 打开功能菜单。".to_string(),
            };

            let reply_kind = match canonical_bg.as_str() {
                "gateway_status" => crate::gateway::feishu_store::ReplyKind::GatewayStatusReply,
                "recent_jobs" => crate::gateway::feishu_store::ReplyKind::RecentJobsReply,
                _ => crate::gateway::feishu_store::ReplyKind::CardActionResult,
            };
            crate::gateway::feishu_worker::send_card_action_result(
                &runtime_bg,
                &inbound_bg,
                &reply_text,
                reply_kind,
                "feishu",
                &job_id_bg,
                &event_key_bg,
            )
            .await;
        });
    }

    // ==== Fast ACK return (non-monitor path) ====
    let ack_ms = ack_start.elapsed().as_millis() as i64;
    println!(
        "[feishu-card] callback_ack ack_ms={ack_ms} action={canonical} queued=true busy=false"
    );
    Ok(Json(serde_json::json!({
        "toast": { "type": "info", "content": "已收到请求，正在处理" },
        "ok": true,
        "action": canonical,
        "queued": true,
        "busy": false,
        "ack_ms": ack_ms
    })))
}

fn payload_keys_summary(raw: &str) -> Vec<String> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(o) = v.as_object() {
            return o.keys().map(|k| k.as_str().to_string()).collect();
        }
    }
    Vec::new()
}

fn extract_token_from_payload(raw: &str) -> Option<String> {
    let v = serde_json::from_str::<serde_json::Value>(raw).ok()?;
    let s = v
        .pointer("/payload/verification_token")
        .or_else(|| v.pointer("/verification_token"))
        .or_else(|| v.pointer("/token"))
        .or_else(|| v.pointer("/header/token"))
        .or_else(|| v.pointer("/event/token"))
        .or_else(|| v.pointer("/event/header/token"))
        .and_then(|val| val.as_str())
        .filter(|s| !s.is_empty())?;
    Some(s.to_string())
}

fn is_feishu_card_action_event(payload: &serde_json::Value) -> bool {
    payload
        .pointer("/header/event_type")
        .and_then(serde_json::Value::as_str)
        == Some("card.action.trigger")
        || payload
            .pointer("/event/type")
            .and_then(serde_json::Value::as_str)
            == Some("card.action.trigger")
        || payload.get("type").and_then(serde_json::Value::as_str)
            == Some("card.action.trigger")
}

fn extract_feishu_card_action(payload: &serde_json::Value) -> Option<(String, &'static str)> {
    [
        ("/event/action/value/action", "event.action.value.action"),
        ("/event/action/value/key", "event.action.value.key"),
        ("/event/action/value", "event.action.value"),
        ("/event/action/tag", "event.action.tag"),
        ("/action/value/action", "action.value.action"),
        ("/action/value/key", "action.value.key"),
        ("/action/value", "action.value"),
    ]
    .into_iter()
    .find_map(|(path, source)| {
        payload
            .pointer(path)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(|value| (value.trim().to_string(), source))
    })
}

#[derive(Debug, Default, PartialEq, Eq)]
struct FeishuCardReplyContext {
    chat_id: Option<String>,
    chat_id_source: Option<&'static str>,
    open_message_id: Option<String>,
    open_message_id_source: Option<&'static str>,
}

fn first_feishu_card_string(
    payload: &serde_json::Value,
    candidates: &[(&'static str, &'static str)],
) -> Option<(String, &'static str)> {
    candidates.iter().find_map(|(path, source)| {
        payload
            .pointer(path)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(|value| (value.trim().to_string(), *source))
    })
}

fn extract_feishu_card_reply_context(payload: &serde_json::Value) -> FeishuCardReplyContext {
    let chat_id = first_feishu_card_string(
        payload,
        &[
            ("/event/context/open_chat_id", "event.context.open_chat_id"),
            ("/event/open_chat_id", "event.open_chat_id"),
            ("/event/chat_id", "event.chat_id"),
            ("/event/context/chat_id", "event.context.chat_id"),
            ("/event/message/chat_id", "event.message.chat_id"),
            ("/event/action/value/open_chat_id", "event.action.value.open_chat_id"),
            ("/event/action/value/chat_id", "event.action.value.chat_id"),
            ("/open_chat_id", "open_chat_id"),
            ("/chat_id", "chat_id"),
            ("/action/value/open_chat_id", "action.value.open_chat_id"),
            ("/action/value/chat_id", "action.value.chat_id"),
        ],
    );
    let open_message_id = first_feishu_card_string(
        payload,
        &[
            (
                "/event/context/open_message_id",
                "event.context.open_message_id",
            ),
            ("/event/open_message_id", "event.open_message_id"),
            ("/event/message/message_id", "event.message.message_id"),
            (
                "/event/action/value/open_message_id",
                "event.action.value.open_message_id",
            ),
            ("/open_message_id", "open_message_id"),
            ("/action/value/open_message_id", "action.value.open_message_id"),
        ],
    );
    FeishuCardReplyContext {
        chat_id: chat_id.as_ref().map(|(value, _)| value.clone()),
        chat_id_source: chat_id.map(|(_, source)| source),
        open_message_id: open_message_id
            .as_ref()
            .map(|(value, _)| value.clone()),
        open_message_id_source: open_message_id.map(|(_, source)| source),
    }
}

fn chrono_timestamp_simple() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}




async fn http_lark_webhook(
    State(runtime): State<GatewayRuntime>,
    headers: HeaderMap,
    raw_body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<GatewayError>)> {
    http_channel_webhook(runtime, headers, raw_body, ChannelKind::Lark).await
}

async fn http_dingtalk_webhook(
    State(runtime): State<GatewayRuntime>,
    headers: HeaderMap,
    raw_body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<GatewayError>)> {
    let cfg = runtime.get_config().await;

    // ==== DingTalk-specific security checks ====
    // 1. Enabled check.
    //
    // Effective state = `gateway.dingtalk.enabled` (preferred new master
    // switch) OR legacy `channels_config.dingtalk.enabled` (kept for
    // backwards compatibility). When both are unset (the default), the
    // route returns `channel_disabled` and no further work is done.
    if !is_dingtalk_effectively_enabled(&cfg) {
        return Ok(Json(serde_json::json!({
            "ok": true,
            "accepted": false,
            "processing": "skipped",
            "reason": "channel_disabled"
        })));
    }

    // 2. Parse JSON payload early for signature verification
    let payload: serde_json::Value = match serde_json::from_str(&raw_body) {
        Ok(p) => p,
        Err(e) => {
            println!("[dingtalk-webhook] rejected reason=invalid_json parse_error={}", e);
            return Err((
                StatusCode::BAD_REQUEST,
                Json(GatewayError {
                    message: "invalid_json".to_string(),
                }),
            ));
        }
    };

    // 3. Signature verification (DingTalk uses HMAC-SHA256 with timestamp+app_secret)
    let dingtalk_entry = cfg.channels_config.dingtalk.as_ref();
    let app_secret = resolve_dingtalk_secret(&cfg, dingtalk_entry);
    if app_secret.is_some() {
        match verify_dingtalk_webhook_signature(&headers, &raw_body, app_secret.as_deref()) {
            Ok(()) => {
                println!("[dingtalk-security] signature_verified");
            }
            Err(reason) => {
                println!("[dingtalk-security] rejected reason={}", reason);
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(GatewayError {
                        message: reason,
                    }),
                ));
            }
        }
    } else {
        // Dev mode - no app_secret configured
        if std::sync::atomic::AtomicU64::new(0)
            .fetch_add(0, std::sync::atomic::Ordering::Relaxed)
            == 0
        {}
        // Low-frequency dev mode warning (every 100th request)
        use std::sync::atomic::{AtomicU64, Ordering};
        static DEV_WARNING_COUNTER: AtomicU64 = AtomicU64::new(0);
        let count = DEV_WARNING_COUNTER.fetch_add(1, Ordering::Relaxed);
        if count % 100 == 0 {
            println!("[dingtalk-security] insecure_webhook_allowed mode=dev");
        }
    }

    // 4. Extract key fields for filtering.
    //
    // Real DingTalk enterprise app bot v1 callbacks send:
    //   - `msgtype` (lowercase) for the message-type discriminator
    //   - `text.content` (nested) for the user-visible text
    //   - `msgId` (camelCase) for the platform message id
    //
    // Some DingTalk client SDKs and proxies still emit `msgType`,
    // `text` as a flat string, or `messageId`. We accept both for
    // forward compatibility without changing the Phase 1 internal model.
    let msg_type = payload
        .get("msgType")
        .or_else(|| payload.get("msgtype"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let is_text_message = msg_type.as_deref() == Some("text");

    // URL verification challenge (real DingTalk sends this once at
    // first connection). Phase 1 silently dropped these; Phase 3 must
    // echo `challenge` back so the platform registers our URL.
    if payload.get("eventType").and_then(|v| v.as_str())
        == Some("url_verification")
    {
        if let Some(challenge) = payload.get("challenge").and_then(|v| v.as_str()) {
            println!(
                "[dingtalk-webhook] url_verification_ok challenge_len={}",
                challenge.len()
            );
            return Ok(Json(serde_json::json!({ "challenge": challenge })));
        }
        println!("[dingtalk-webhook] url_verification_missing_challenge");
    }

    // 5. Extract sender info (DingTalk corpid/userid)
    let sender_staff_id = payload
        .get("senderStaffId")
        .and_then(|v| v.as_str())
        .map(String::from);
    let sender_nick = payload
        .get("senderNick")
        .and_then(|v| v.as_str())
        .map(String::from);
    let conversation_id = payload
        .get("conversationId")
        .and_then(|v| v.as_str())
        .map(String::from);
    let session_webhook = payload
        .get("sessionWebhook")
        .and_then(|v| v.as_str())
        .map(String::from);
    let message_id = payload
        .get("messageId")
        .or_else(|| payload.get("msgId"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let robot_code = payload
        .get("robotCode")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Log inbound (presence flags and length only — never the values).
    println!(
        "[dingtalk-webhook] received msg_type={:?} has_sender={} has_conversation={} has_webhook={} has_msgid={} has_robot_code={} text_len={}",
        msg_type,
        sender_staff_id.as_ref().map(|_| true).unwrap_or(false),
        conversation_id.as_ref().map(|_| true).unwrap_or(false),
        session_webhook.as_ref().map(|_| true).unwrap_or(false),
        message_id.as_ref().map(|_| true).unwrap_or(false),
        robot_code.as_ref().map(|_| true).unwrap_or(false),
        extract_dingtalk_text_len(&payload)
    );

    // 6. Filter: only handle text messages (Phase 1)
    if !is_text_message {
        println!("[dingtalk-webhook] skip_unsupported_message_type msg_type={:?}", msg_type);
        return Ok(Json(serde_json::json!({
            "ok": true,
            "accepted": true,
            "processing": "skipped",
            "reason": "unsupported_message_type"
        })));
    }

    // 7. Extract text content. Accept both nested `text.content` (real
    // DingTalk) and flat `text` (some proxies / SDKs). `text.content`
    // wins when both are present.
    let text = extract_dingtalk_text(&payload);
    if text.trim().is_empty() {
        println!("[dingtalk-webhook] skip_empty_text");
        return Ok(Json(serde_json::json!({
            "ok": true,
            "accepted": true,
            "processing": "skipped",
            "reason": "empty_text"
        })));
    }

    // 8. Build InboundMessage with DingTalk-specific metadata
    let mut metadata = std::collections::HashMap::new();
    if let Some(ref sid) = sender_staff_id {
        metadata.insert("senderStaffId".to_string(), serde_json::json!(sid));
    }
    if let Some(ref nick) = sender_nick {
        metadata.insert("senderNick".to_string(), serde_json::json!(nick));
    }
    if let Some(ref cid) = conversation_id {
        metadata.insert("conversationId".to_string(), serde_json::json!(cid));
    }
    if let Some(ref webhook) = session_webhook {
        metadata.insert("sessionWebhook".to_string(), serde_json::json!(webhook));
    }
    if let Some(ref mid) = message_id {
        metadata.insert("messageId".to_string(), serde_json::json!(mid));
    }
    if let Some(ref rc) = robot_code {
        metadata.insert("robotCode".to_string(), serde_json::json!(rc));
    }
    metadata.insert("source".to_string(), serde_json::json!("dingtalk"));
    metadata.insert("raw_payload".to_string(), payload.clone());

    let inbound = InboundMessage {
        channel: ChannelKind::Dingtalk,
        user_id: sender_staff_id,
        session_id: conversation_id,
        text,
        metadata,
    };

    // 9. Check if DingTalk worker is initialized
    let is_async_available = runtime.is_dingtalk_worker_initialized().await;

    if is_async_available {
        // Queue job for background processing
        let text_len = inbound.text.chars().count();
        let queue_len_before = runtime.dingtalk_queue_len().await;
        println!(
            "[dingtalk-webhook] enqueue_attempt=true text_len={} queue_len={}",
            text_len,
            queue_len_before
        );

        let job = dingtalk_worker::DingtalkAsyncJob::new(inbound, payload);
        let job_id = job.job_id.clone();

        match runtime.try_send_dingtalk_job(job).await {
            Ok(()) => {
                let queue_len = runtime.dingtalk_queue_len().await;
                println!(
                    "[dingtalk-webhook] enqueue_success=true queue_len={}",
                    queue_len
                );
                println!(
                    "[dingtalk-webhook] ack_queued ack_ms={} queue_len={}",
                    0,
                    queue_len
                );
                return Ok(Json(serde_json::json!({
                    "ok": true,
                    "accepted": true,
                    "processing": "queued",
                    "job_id": job_id
                })));
            }
            Err(_) => {
                let queue_len = runtime.dingtalk_queue_len().await;
                println!(
                    "[dingtalk-webhook] enqueue_attempt=true text_len={} queue_len={} result=failed",
                    text_len,
                    queue_len
                );
                println!("[dingtalk-worker] queue_full capacity=100");
                return Ok(Json(serde_json::json!({
                    "ok": false,
                    "accepted": false,
                    "processing": "queue_full",
                    "retryable": true
                })));
            }
        }
    }

    // 10. Fallback: sync processing via Runtime + outbound
    println!("[dingtalk-webhook] sync_mode reason=worker_not_initialized");

    // Phase 2: short-circuit commands (help/menu/status/ping/monitor)
    // even in the sync fallback path. Non-command text continues into
    // the agent exactly like Phase 1.
    let sync_command_inputs = crate::gateway::dingtalk_commands::DingtalkStatusInputs {
        config: &cfg,
        worker_initialized: false,
        queue_len: runtime.dingtalk_queue_len().await,
    };
    let command_decision = crate::gateway::dingtalk_commands::evaluate_dingtalk_command(
        &inbound.text,
        Some(&payload),
        sync_command_inputs,
    );
    if let Some((cmd, command_reply)) = command_decision {
        println!(
            "[dingtalk-webhook] command_reply command={}",
            cmd.name()
        );
        if command_reply.trim().is_empty() {
            return Ok(Json(serde_json::json!({
                "ok": true,
                "accepted": true,
                "processing": "completed",
                "reply": ""
            })));
        }
        let result = deliver_dingtalk_reply(&cfg, &inbound, &command_reply).await;
        match result {
            Ok(_) => {
                println!(
                    "[dingtalk-webhook] outbound_ok reply_len={}",
                    command_reply.len()
                );
                return Ok(Json(serde_json::json!({
                    "ok": true,
                    "accepted": true,
                    "processing": "completed",
                    "reply": ""
                })));
            }
            Err(e) => {
                println!("[dingtalk-webhook] outbound_failed error={}", e);
                return Ok(Json(serde_json::json!({
                    "ok": true,
                    "accepted": true,
                    "processing": "completed_with_error",
                    "reply": "",
                    "platform_error": e
                })));
            }
        }
    }

    let response = match runtime.process_inbound(&inbound).await {
        Ok(r) => r,
        Err(e) => {
            println!("[dingtalk-webhook] runtime_failed error={}", e);
            return Ok(Json(serde_json::json!({
                "ok": false,
                "error": "agent_runtime_failed",
                "message": e.to_string()
            })));
        }
    };

    if response.reply.trim().is_empty() {
        return Ok(Json(serde_json::json!({
            "ok": true,
            "accepted": true,
            "processing": "completed",
            "reply": ""
        })));
    }

    // Deliver reply via sessionWebhook
    let result = deliver_dingtalk_reply(&cfg, &inbound, &response.reply).await;
    match result {
        Ok(_) => {
            println!("[dingtalk-webhook] outbound_ok reply_len={}", response.reply.len());
            Ok(Json(serde_json::json!({
                "ok": true,
                "accepted": true,
                "processing": "completed",
                "reply": ""
            })))
        }
        Err(e) => {
            println!("[dingtalk-webhook] outbound_failed error={}", e);
            Ok(Json(serde_json::json!({
                "ok": true,
                "accepted": true,
                "processing": "completed_with_error",
                "reply": "",
                "platform_error": e
            })))
        }
    }
}

async fn http_channel_webhook(
    runtime: GatewayRuntime,
    headers: HeaderMap,
    raw_body: String,
    channel: ChannelKind,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<GatewayError>)> {
    let channel_name = format!("{:?}", channel).to_lowercase();
    let cfg = runtime.get_config().await;

    // Security: Reject requests for disabled channels
    if !is_channel_enabled(&cfg, &channel) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(GatewayError {
                message: format!("{:?} channel is disabled", channel),
            }),
        ));
    }
    
    // === Feishu-specific security checks ===
    let mut processed_body = raw_body.clone();
    
    if channel == ChannelKind::Feishu {
        let entry = cfg.channels_config.feishu.as_ref();
        let sec_cfg = FeishuSecurityConfig::from_entry(entry.as_deref());
        if matches!(sec_cfg.mode, FeishuSecurityMode::Invalid) {
            println!("[feishu-security] rejected reason=invalid_security_mode");
            return Err((
                StatusCode::BAD_REQUEST,
                Json(GatewayError {
                    message: "invalid_security_mode".to_string(),
                }),
            ));
        }
        // Log security mode
        if !sec_cfg.insecure {
            println!(
                "[feishu-security] mode={} verification_token_configured={}",
                sec_cfg.mode.as_str(),
                sec_cfg.verification_token.is_some()
            );
        }
        
        // Parse payload (may need decryption)
        let payload_for_check: serde_json::Value = match serde_json::from_str(&raw_body) {
            Ok(p) => p,
            Err(_) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(GatewayError {
                        message: "invalid_json".to_string(),
                    }),
                ))
            }
        };
        
        let encrypted_payload = is_encrypted_payload(&payload_for_check);
        if matches!(sec_cfg.mode, FeishuSecurityMode::Encrypted) && !encrypted_payload {
            println!("[feishu-security] rejected reason=encrypted_payload_required");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(GatewayError {
                    message: "encrypted_payload_required".to_string(),
                }),
            ));
        }

        // Encrypted mode must decrypt before token verification or any queue
        // interaction. Dev/token modes intentionally do not attempt decryption.
        if encrypted_payload && matches!(sec_cfg.mode, FeishuSecurityMode::Encrypted) {
            println!("[feishu-security] encrypted_payload_detected");
            
            let encrypt_key = sec_cfg.encrypt_key.as_deref()
                .ok_or_else(|| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(GatewayError {
                            message: "encrypt_key_missing".to_string(),
                        }),
                    )
                })?;
            
            let encrypted_content = payload_for_check.get("encrypt")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(GatewayError {
                            message: "encrypted_payload_required".to_string(),
                        }),
                    )
                })?;
            
            match decrypt_feishu_payload(encrypted_content, encrypt_key) {
                Ok(decrypted) => {
                    println!("[feishu-security] decrypt_ok");
                    processed_body = decrypted;
                }
                Err(e) => {
                    let _ = e;
                    println!("[feishu-security] decrypt_failed");
                    return Err((
                        StatusCode::FORBIDDEN,
                        Json(GatewayError {
                            message: "decrypt_failed".to_string(),
                        }),
                    ));
                }
            }
        }

        let secured_payload: serde_json::Value =
            serde_json::from_str(&processed_body).map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(GatewayError {
                        message: "invalid_json".to_string(),
                    }),
                )
            })?;

        // Verify verification_token in token/encrypted mode
        if matches!(sec_cfg.mode, FeishuSecurityMode::Token | FeishuSecurityMode::Encrypted) {
            let request_token = extract_feishu_verification_token(&secured_payload, &headers);
            if let Some(source) = request_token {
                println!(
                    "[feishu-security] token_extract token_present=true source={}",
                    source.source
                );
            } else {
                for line in feishu_token_missing_diagnostic_lines(&secured_payload, &headers) {
                    println!("{line}");
                }
            }

            match verify_feishu_verification_token(
                &sec_cfg,
                request_token.map(|source| source.value),
            ) {
                Ok(true) => {
                    println!("[feishu-security] token_verified=true");
                }
                Ok(false) => {
                    println!("[feishu-security] token_mismatch");
                    println!("[feishu-security] rejected reason=token_mismatch");
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        Json(GatewayError {
                            message: "token_mismatch".to_string(),
                        }),
                    ));
                }
                Err(msg) => {
                    let (status, reason) = if msg == "verification_token missing from request" {
                        println!("[feishu-security] token_missing");
                        (StatusCode::UNAUTHORIZED, "token_missing")
                    } else {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "internal_error",
                        )
                    };
                    println!("[feishu-security] rejected reason={reason}");
                    return Err((
                        status,
                        Json(GatewayError {
                            message: reason.to_string(),
                        }),
                    ));
                }
            }
        }
        
        // Dev mode: low frequency warning (every 100th request)
        if matches!(sec_cfg.mode, FeishuSecurityMode::Dev | FeishuSecurityMode::Default) {
            use std::sync::atomic::{AtomicU64, Ordering};
            static DEV_WARNING_COUNTER: AtomicU64 = AtomicU64::new(0);
            let count = DEV_WARNING_COUNTER.fetch_add(1, Ordering::Relaxed);
            if count % 100 == 0 {
                println!("[feishu-security] insecure_webhook_allowed mode=dev");
            }
        }

        if secured_payload.get("type").and_then(serde_json::Value::as_str)
            == Some("url_verification")
        {
            println!("[feishu-security] url_verification_detected");
            let challenge = secured_payload
                .get("challenge")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(GatewayError {
                            message: "missing_challenge".to_string(),
                        }),
                    )
                })?;
            println!("[feishu-security] url_verification_ok");
            println!("[feishu-webhook] challenge_response_sent");
            return Ok(Json(serde_json::json!({ "challenge": challenge })));
        }

        // The SQLite event_key uniqueness constraint remains the replay
        // barrier. This warning makes unusually delayed deliveries auditable
        // without rejecting legitimate Feishu retries.
        if let Some(created_at) = secured_payload
            .pointer("/header/create_time")
            .cloned()
            .and_then(|value| match value {
                serde_json::Value::String(value) => value.parse::<i64>().ok(),
                serde_json::Value::Number(value) => value.as_i64(),
                _ => None,
            })
        {
            if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                let age_seconds = now.as_secs().saturating_sub(created_at.max(0) as u64);
                if age_seconds > 600 {
                    println!("[feishu-security] stale_event_warning age_seconds={age_seconds}");
                }
            }
        }
    }
    
    if let Some(secret) = channel_webhook_signing_secret(&cfg, &channel) {
        let allowed_algorithms = cfg
            .gateway
            .webhook_signature_algorithms
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let priority_algorithms = cfg
            .gateway
            .webhook_signature_priority
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let signature = headers
            .get("x-omninova-signature")
            .or_else(|| headers.get("x-signature"))
            .or_else(|| headers.get("x-hub-signature-256"))
            .and_then(|v| v.to_str().ok());
        let signed_payload = signed_webhook_payload(&cfg, &headers, &raw_body).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(GatewayError {
                    message: e.to_string(),
                }),
            )
        })?;
        let verified = verify_webhook_signature_with_policy_options(
            &signed_payload,
            signature,
            &secret,
            &allowed_algorithms,
            &priority_algorithms,
            cfg.gateway.webhook_signature_strict_priority,
        )
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(GatewayError {
                    message: e.to_string(),
                }),
            )
        })?;
        if !verified {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(GatewayError {
                    message: "invalid webhook signature".to_string(),
                }),
            ));
        }
    }

    runtime
        .validate_webhook_replay(&headers)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(GatewayError {
                    message: e.to_string(),
                }),
            )
        })?;

    let payload: serde_json::Value = match serde_json::from_str(&processed_body) {
        Ok(p) => p,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(GatewayError {
                    message: format!("invalid channel webhook payload: {e}"),
                }),
            ))
        }
    };

    // Feishu's unified callback URL delivers card.action.trigger to the main
    // webhook. Route it before the ordinary message_type filter; the dedicated
    // /webhook/feishu/card endpoint remains supported by the same handler.
    if channel == ChannelKind::Feishu && is_feishu_card_action_event(&payload) {
        println!(
            "[feishu-webhook] route_card_action source=main_webhook event_type=card.action.trigger"
        );
        return process_feishu_card_action_callback(
            State(runtime),
            headers,
            processed_body,
            "main_webhook",
        )
        .await;
    }

    // Feishu challenges have already returned after Feishu security checks and
    // before replay/dedupe. Other platform adapters retain their existing
    // challenge handling here.
    if channel != ChannelKind::Feishu {
        if let Some(challenge) = verification_response(&payload) {
            println!("[{}-webhook] challenge_response_sent", channel_name);
            return Ok(Json(challenge));
        }
    }

    // Extract key fields for filtering and logging
    let header_event_id = payload.get("header")
        .and_then(|h| h.get("event_id"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let header_event_type = payload.get("header")
        .and_then(|h| h.get("event_type"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let top_level_event_id = payload.get("event_id").and_then(|v| v.as_str()).map(String::from);
    let event_id = header_event_id.as_ref().or(top_level_event_id.as_ref()).cloned();
    
    // Extract sender_type (for bot/self-message filtering)
    let sender_type = payload.get("event")
        .and_then(|e| e.get("sender"))
        .and_then(|s| s.get("sender_type"))
        .and_then(|v| v.as_str())
        .map(String::from);
    
    // Extract message_type (for filtering non-text messages)
    let message_type = payload.get("event")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.get("message_type"))
        .and_then(|v| v.as_str())
        .map(String::from);
    
    // Extract message_id for deduplication
    let message_id = payload.get("event")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.get("message_id"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // Helper to extract text preview
    fn extract_text_for_log(payload: &serde_json::Value) -> String {
        // Try event.message.content (Feishu v2)
        if let Some(event) = payload.get("event") {
            if let Some(msg) = event.get("message") {
                if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(content) {
                        if let Some(t) = parsed.get("text").and_then(|v| v.as_str()) {
                            return truncate_chars_for_log(t, 100);
                        }
                    }
                }
            }
        }
        // Try direct text field
        if let Some(t) = payload.get("text").and_then(|v| v.as_str()) {
            return truncate_chars_for_log(t, 100);
        }
        "(no text found)".to_string()
    }

    let text_preview = extract_text_for_log(&payload);
    let payload_clone = payload.clone(); // Keep original payload for async job
    let effective_event_type = header_event_type.clone()
        .or_else(|| payload.get("event").and_then(|e| e.get("type")).and_then(|v| v.as_str()).map(String::from))
        .or_else(|| payload.get("type").and_then(|v| v.as_str()).map(String::from))
        .unwrap_or_else(|| "unknown".to_string());

    println!(
        "[{}-webhook] received event_type={} event_id_present={} header_event_id_present={} message_id_present={} chat_id_present={} sender_type={:?} message_type={:?} text_len={}",
        channel_name, 
        effective_event_type,
        event_id.is_some(),
        header_event_id.is_some(),
        message_id.is_some(),
        payload.get("event").and_then(|e| e.get("message")).and_then(|m| m.get("chat_id")).is_some(),
        sender_type,
        message_type,
        text_preview.len()
    );

    // ==== FILTERING: sender_type ====
    match sender_type.as_deref() {
        Some("app") | Some("bot") => {
            println!(
                "[{}-webhook] skip_self_message reason=sender_type_{}",
                channel_name,
                sender_type.as_ref().unwrap()
            );
            return Ok(Json(serde_json::json!({
                "ok": true,
                "accepted": true,
                "processing": "skipped",
                "reason": format!("sender_type_{}", sender_type.as_ref().unwrap())
            })));
        }
        Some(t) if t != "user" => {
            println!(
                "[{}-webhook] sender_type_unknown type={}",
                channel_name,
                t
            );
            // For unknown sender types, skip to be safe and avoid loops
            return Ok(Json(serde_json::json!({
                "ok": true,
                "accepted": true,
                "processing": "skipped",
                "reason": "sender_type_unknown"
            })));
        }
        _ => {}
    }

    // ==== FILTERING: message_type ====
    if message_type.as_deref() != Some("text") {
        println!(
            "[{}-webhook] skip_unsupported_message_type message_type={:?}",
            channel_name,
            message_type
        );
        return Ok(Json(serde_json::json!({
            "ok": true,
            "accepted": true,
            "processing": "skipped",
            "reason": "unsupported_message_type"
        })));
    }

    // ==== DEDUPLICATION ====
    let dedup_cache = runtime.dedup_cache();
    let dedup_key = event_id.as_ref()
        .or(message_id.as_ref())
        .map(|k| format!("{}:{}", channel_name, k));
    
    if let Some(key) = &dedup_key {
        let is_new = dedup_cache.check_and_insert(key).await;
        if !is_new {
            println!("[{}-dedupe] duplicate key_type={}", channel_name, 
                if event_id.is_some() { "event_id" } else { "message_id" });
            return Ok(Json(serde_json::json!({
                "ok": true,
                "accepted": true,
                "duplicate": true,
                "processing": "skipped"
            })));
        }
        println!("[{}-dedupe] accepted key_type={}", channel_name,
            if event_id.is_some() { "event_id" } else { "message_id" });
    }

    // ==== FILTERING: outbound message ID (self-message detection) ====
    // Check if this message was sent by us (bot) recently
    if let Some(incoming_msg_id) = message_id.as_ref() {
        let channel_str = channel_name.clone();
        if OutboundMsgCache::global().is_our_message(&channel_str, incoming_msg_id).await {
            println!(
                "[{}-webhook] skip_self_message reason=outbound_message_id_match message_id_present=true",
                channel_name
            );
            return Ok(Json(serde_json::json!({
                "ok": true,
                "accepted": true,
                "processing": "skipped",
                "reason": "outbound_message_id_match"
            })));
        }
    }

    // Parse payload into inbound message
    let inbound_channel = channel.clone();
    let mut inbound = match inbound_from_platform_webhook(channel, payload) {
        Ok(i) => i,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(GatewayError {
                    message: e.to_string(),
                }),
            ))
        }
    };

    // ==== FEISHU SESSION ISOLATION ====
    // For Feishu, use stateless mode to prevent session pollution from desktop tasks
    if inbound_channel == ChannelKind::Feishu {
        // Mark this inbound as stateless - prevents loading session history
        inbound.metadata.insert("stateless".to_string(), serde_json::Value::Bool(true));
        
        // Prefix session_id with channel to ensure isolation
        if let Some(ref chat_id) = inbound.session_id {
            let prefixed = format!("feishu:{chat_id}");
            inbound.session_id = Some(prefixed.clone());
            println!(
                "[{}-webhook] session_mode stateless=true session_id_prefix=feishu session_id_present=true",
                channel_name
            );
        }
        
        // ==== FEISHU ROUTING MODE ====
        // Determine if this is a chat-only message or tool command
        let text_trimmed = inbound.text.trim();
        
        // Supported slash commands for tool mode
        const SLASH_COMMANDS: &[&str] = &["/run", "/tool", "/monitor", "/file", "/workspace", "/agent"];
        
        let is_slash_command = SLASH_COMMANDS.iter().any(|cmd| text_trimmed.starts_with(cmd));
        
        if is_slash_command {
            // Extract command name
            let command = SLASH_COMMANDS.iter()
                .find(|cmd| text_trimmed.starts_with(*cmd))
                .map(|s| s.to_string())
                .unwrap_or_default();
            
            inbound.metadata.insert("feishu_mode".to_string(), serde_json::Value::String("tool".to_string()));
            inbound.metadata.insert("chat_only".to_string(), serde_json::Value::Bool(false));
            inbound.metadata.insert("slash_command".to_string(), serde_json::Value::String(command.clone()));
            
            println!(
                "[{}-router] mode=tool reason=slash_command command={} text_len={}",
                channel_name, command, inbound.text.len()
            );
        } else {
            // Default to chat-only mode
            inbound.metadata.insert("feishu_mode".to_string(), serde_json::Value::String("chat_only".to_string()));
            inbound.metadata.insert("chat_only".to_string(), serde_json::Value::Bool(true));
            
            println!(
                "[{}-router] mode=chat_only reason=default_text text_len={}",
                channel_name, inbound.text.len()
            );
        }
    }

    // Diagnostic: log inbound ready
    let user_id_present = inbound.user_id.is_some();
    let metadata_has_message_id = inbound.metadata.get("message_id").is_some()
        || inbound.metadata.get("message_message_id").is_some();
    let metadata_has_chat_id = inbound.metadata.get("chat_id").is_some()
        || inbound.metadata.get("message_chat_id").is_some()
        || inbound.metadata.get("conversation_id").is_some();
    println!(
        "[{}-webhook] inbound_ready user_id_present={} session_id_present={} metadata_has_message_id={} metadata_has_chat_id={} text_len={}",
        channel_name, user_id_present, inbound.session_id.is_some(), metadata_has_message_id, metadata_has_chat_id, inbound.text.len()
    );

    // ==== FEISHU ASYNC PROCESSING ====
    // For Feishu, check if async worker is available (tests may not have it initialized)
    if inbound_channel == ChannelKind::Feishu {
        let is_async_available = runtime.is_feishu_worker_initialized().await;
        
        if is_async_available {
            let is_chat_only = inbound.metadata.get("chat_only")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            
            // Build event_key for dedupe
            let event_key = event_id.as_ref()
                .or(message_id.as_ref())
                .map(|k| format!("{}:{}", channel_name, k))
                .unwrap_or_else(|| {
                    let session_hash = inbound
                        .session_id
                        .as_deref()
                        .map(hash_dedup_string)
                        .unwrap_or_else(|| "unknown".to_string());
                    format!(
                        "{}:{}:{}",
                        channel_name,
                        session_hash,
                        crate::gateway::feishu_store::FeishuStore::hash_text(&inbound.text)
                    )
                });
            
            // Extract chat_id
            let chat_id = inbound.session_id.clone()
                .or_else(|| inbound.metadata.get("chat_id").and_then(|v| v.as_str()).map(String::from))
                .or_else(|| inbound.metadata.get("message_chat_id").and_then(|v| v.as_str()).map(String::from));
            
            // Build metadata JSON (redacted) - serialize immediately to avoid borrow conflict
            let metadata_json = {
                let mut meta = serde_json::Map::new();
                if let Some(ref msg_id) = message_id {
                    meta.insert("message_id".to_string(), serde_json::Value::String(msg_id.clone()));
                }
                if let Some(ref evt_id) = event_id {
                    meta.insert("event_id".to_string(), serde_json::Value::String(evt_id.clone()));
                }
                if let Some(ref sender) = sender_type {
                    meta.insert("sender_type".to_string(), serde_json::Value::String(sender.clone()));
                }
                let feishu_security = FeishuSecurityConfig::from_entry(cfg.channels_config.feishu.as_ref());
                meta.insert(
                    "security_mode".to_string(),
                    serde_json::Value::String(feishu_security.mode.as_str().to_string()),
                );
                meta.insert(
                    "token_verified".to_string(),
                    serde_json::Value::Bool(matches!(
                        feishu_security.mode,
                        FeishuSecurityMode::Token | FeishuSecurityMode::Encrypted
                    )),
                );
                meta.insert("encrypted".to_string(), serde_json::Value::Bool(is_encrypted_payload(&payload_clone)));
                serde_json::Value::Object(meta)
            };
            let metadata_json_str = serde_json::to_string(&metadata_json).ok();
            
            // Generate job_id upfront so it can be used in both store and worker
            let job_id = uuid::Uuid::new_v4().to_string();
            
            // Persist event to SQLite (before enqueuing)
            if let Some(ref store) = runtime.feishu_store() {
                let event_input = crate::gateway::feishu_store::FeishuEventInput {
                    event_key: event_key.clone(),
                    channel: channel_name.clone(),
                    event_id: event_id.clone(),
                    message_id: message_id.clone(),
                    chat_id: chat_id.clone(),
                    user_id_hash: inbound.user_id.as_ref().map(|uid| 
                        crate::gateway::feishu_store::FeishuStore::hash_text(uid)),
                    event_type: Some(effective_event_type.to_string()),
                    sender_type: sender_type.clone(),
                    message_type: message_type.clone(),
                    text: Some(inbound.text.clone()),
                    skip_reason: None,
                    metadata_json: metadata_json_str,
                };
                
                let event_key_clone = event_key.clone();
                
                match store.insert_event(&event_input) {
                    Ok(_event) => {
                        // Event inserted successfully - now create job with SAME job_id
                        let job_input = crate::gateway::feishu_store::FeishuJobInput {
                            job_id: job_id.clone(),
                            event_key: event_key_clone.clone(),
                            channel: channel_name.clone(),
                            mode: if is_chat_only { "chat_only".to_string() } else { "tool".to_string() },
                            slash_command: inbound.metadata.get("slash_command")
                                .and_then(|v| v.as_str())
                                .map(String::from),
                            // Privacy boundary: the in-memory worker receives the
                            // event, but SQLite never retains the full inbound
                            // payload or user message body. Recovery will mark an
                            // interrupted in-flight job as abandoned instead of
                            // replaying private content after restart.
                            payload_json: None,
                        };
                        
                        if let Err(e) = store.insert_job(&job_input) {
                            println!("[{}-webhook] failed to persist job: {}", channel_name, e);
                        }
                    }
                    Err(crate::gateway::feishu_store::StoreError::DuplicateEvent(_)) => {
                        // Duplicate event - check job status
                        println!("[{}-store] duplicate event_key={}", channel_name, event_key);
                        
                        // Try to get job status to see if it's still in progress
                        if let Ok(Some(existing_job)) = store.get_job_by_event_key(&event_key) {
                            let status = existing_job.status.as_str();
                            println!("[{}-store] duplicate job status job_id={} status={}", channel_name, existing_job.job_id, status);
                            
                            // If job is still pending/processing, return duplicate skipped
                            if matches!(existing_job.status, 
                                crate::gateway::feishu_store::JobStatus::Pending
                                | crate::gateway::feishu_store::JobStatus::Queued
                                | crate::gateway::feishu_store::JobStatus::Processing
                                | crate::gateway::feishu_store::JobStatus::Failed
                            ) {
                                return Ok(Json(serde_json::json!({
                                    "ok": true,
                                    "accepted": true,
                                    "duplicate": true,
                                    "processing": "in_progress",
                                    "job_id": existing_job.job_id,
                                    "status": status
                                })));
                            }
                        }
                        
                        // Completed or unknown - return duplicate skipped
                        return Ok(Json(serde_json::json!({
                            "ok": true,
                            "accepted": true,
                            "duplicate": true,
                            "processing": "skipped"
                        })));
                    }
                    Err(crate::gateway::feishu_store::StoreError::PoisonedLock) => {
                        println!("[{}-webhook] store poisoned, continuing without persistence", channel_name);
                    }
                    Err(e) => {
                        println!("[{}-webhook] failed to persist event: {}", channel_name, e);
                        // Continue anyway - try to process
                    }
                }
            }
            
            // Slash /monitor shares the exact same per-chat guard as card monitor actions.
            // Acquire before queueing so a busy request never reaches direct_monitor.
            let is_slash_monitor = inbound
                .metadata
                .get("slash_command")
                .and_then(serde_json::Value::as_str)
                == Some("/monitor");
            let monitor_guard_lease = if is_slash_monitor {
                if let Some(chat_identity) = inbound.session_id.as_deref() {
                    let duration_secs =
                        crate::gateway::feishu_worker::parse_monitor_duration_from_text(
                            &inbound.text,
                        );
                    let ttl_secs = monitor_guard_ttl_secs(duration_secs);
                    match runtime
                        .monitor_flight_guard()
                        .try_acquire_with_ttl(chat_identity, ttl_secs)
                        .await
                    {
                        Some(lease) => {
                            println!(
                                "[feishu-monitor] singleflight_acquired source=slash command=/monitor chat_present=true ttl_secs={}",
                                ttl_secs
                            );
                            Some(lease)
                        }
                        None => {
                            println!(
                                "[feishu-monitor] singleflight_busy source=slash command=/monitor chat_present=true"
                            );
                            if let Some(ref store) = runtime.feishu_store() {
                                let _ = store.job_completed(&job_id);
                                let _ = store.update_event_status(
                                    &event_key,
                                    crate::gateway::feishu_store::EventStatus::Processed,
                                );
                            }
                            let runtime_busy = runtime.clone();
                            let inbound_busy = inbound.clone();
                            let job_id_busy = job_id.clone();
                            let event_key_busy = event_key.clone();
                            let channel_name_busy = channel_name.clone();
                            tokio::spawn(async move {
                                crate::gateway::feishu_worker::send_card_action_result(
                                    &runtime_busy,
                                    &inbound_busy,
                                    "已有桌面监控任务正在执行，请等待当前任务完成后再试。",
                                    crate::gateway::feishu_store::ReplyKind::CardActionResult,
                                    &channel_name_busy,
                                    &job_id_busy,
                                    &event_key_busy,
                                )
                                .await;
                            });
                            return Ok(Json(serde_json::json!({
                                "ok": true,
                                "accepted": true,
                                "processing": "skipped",
                                "busy": true
                            })));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Create job with SAME job_id that was used in store (or generate new one if store unavailable)
            let mut job = crate::gateway::feishu_worker::FeishuAsyncJob::new(
                inbound_channel,
                inbound,
                payload_clone,
                is_chat_only,
                event_key.clone(),
                Some(job_id.clone()),
            );
            if let Some(lease) = monitor_guard_lease.clone() {
                job = job.with_monitor_guard_lease(lease);
            }

            match runtime.try_send_feishu_job(job).await {
                Ok(()) => {
                    // Update job status to QUEUED if store is available
                    if let Some(ref store) = runtime.feishu_store() {
                        let _ = store.update_event_status(&event_key, 
                            crate::gateway::feishu_store::EventStatus::Queued);
                    }
                    
                    let queue_len = runtime.feishu_queue_len().await;
                    println!(
                        "[{}-webhook] ack_queued ack_ms={} queue_len={}",
                        channel_name,
                        0,
                        queue_len
                    );
                    return Ok(Json(serde_json::json!({
                        "ok": true,
                        "accepted": true,
                        "processing": "queued"
                    })));
                }
                Err(_) => {
                    if let Some(ref lease) = monitor_guard_lease {
                        let released = runtime.monitor_flight_guard().release(lease).await;
                        println!(
                            "[feishu-monitor] singleflight_released source=enqueue command=/monitor success=false released={}",
                            released
                        );
                    }
                    println!(
                        "[{}-worker] queue_full capacity=100",
                        channel_name
                    );
                    return Ok(Json(serde_json::json!({
                        "ok": false,
                        "accepted": false,
                        "processing": "queue_full",
                        "retryable": true
                    })));
                }
            }
        } else {
            println!("[{}-webhook] sync_mode reason=worker_not_initialized", channel_name);
        }
    }

    // Process in Runtime (synchronous to ensure proper ordering) - for non-Feishu channels
    println!("[{}-webhook] runtime_start", channel_name);
    let response = match runtime.process_inbound(&inbound).await {
        Ok(response) => {
            println!(
                "[{}-webhook] runtime_done reply_len={} reply_empty={}",
                channel_name,
                response.reply.len(),
                response.reply.trim().is_empty()
            );
            response
        }
        Err(e) => {
            println!("[{}-webhook] runtime_failed error={}", channel_name, e);
            return Ok(Json(serde_json::json!({
                "ok": false,
                "error": "agent_runtime_failed",
                "message": e.to_string()
            })));
        }
    };

    if response.reply.trim().is_empty() {
        println!("[{}-webhook] skipped_empty_reply", channel_name);
        return Ok(Json(serde_json::json!({
            "ok": true,
            "accepted": true,
            "processing": "completed",
            "reply": ""
        })));
    }

    // Deliver the reply via outbound sender
    println!("[{}-webhook] outbound_start reply_len={}", channel_name, response.reply.len());
    let message_id_for_response = inbound.metadata.get("message_id")
        .or_else(|| inbound.metadata.get("message_message_id"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let session_id_for_response = inbound.session_id.clone()
        .or_else(|| inbound.metadata.get("chat_id").and_then(|v| v.as_str()).map(String::from))
        .or_else(|| inbound.metadata.get("message_chat_id").and_then(|v| v.as_str()).map(String::from));

    let webhook_response = match deliver_platform_reply(&cfg, &inbound, &response.reply).await {
        Some(outbound_result) => PlatformWebhookResponse::success_with_outbound(
            &channel_name,
            message_id_for_response,
            session_id_for_response,
            response.reply,
            outbound_result.to_summary(),
        ),
        None => PlatformWebhookResponse::success(
            &channel_name,
            message_id_for_response,
            session_id_for_response,
            response.reply,
        ),
    };
    Ok(platform_webhook_response_json(webhook_response))
}

fn truncate_chars_for_log(value: &str, max_chars: usize) -> String {
    let mut preview = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        preview.push_str("…");
    }
    preview
}

fn platform_webhook_response_json(response: PlatformWebhookResponse) -> Json<serde_json::Value> {
    Json(serde_json::to_value(response).unwrap_or_else(|_| {
        serde_json::json!({
            "ok": false,
            "error": "webhook_response_serialization_failed"
        })
    }))
}

fn outbound_token_cache() -> Arc<TokenCache> {
    OUTBOUND_TOKEN_CACHE
        .get_or_init(|| Arc::new(TokenCache::new()))
        .clone()
}

fn channel_entry_for_outbound<'a>(
    config: &'a Config,
    channel: &ChannelKind,
) -> Option<&'a crate::config::schema::ChannelEntry> {
    match channel {
        ChannelKind::Feishu => config.channels_config.feishu.as_ref(),
        ChannelKind::Lark => config.channels_config.lark.as_ref(),
        _ => None,
    }
}

fn config_string(entry: &crate::config::schema::ChannelEntry, key: &str) -> Option<String> {
    entry
        .extra
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            entry
                .extra
                .get(&format!("{key}_env"))
                .and_then(serde_json::Value::as_str)
                .and_then(|env_key| std::env::var(env_key).ok())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

fn reply_target_from_inbound(inbound: &InboundMessage) -> Option<ReplyTarget> {
    // Try multiple keys for chat_id/conversation_id
    let chat_id = ["conversation_id", "chat_id", "message_chat_id", "message_conversation_id"]
        .iter()
        .find_map(|key| {
            inbound
                .metadata
                .get(*key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        })
        .or_else(|| {
            inbound
                .session_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        })?;

    let message_id = inbound
        .metadata
        .get("message_id")
        .or_else(|| inbound.metadata.get("message_message_id"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let user_id = inbound.user_id.clone().or_else(|| {
        inbound
            .metadata
            .get("open_id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    });
    Some(ReplyTarget {
        channel: inbound.channel.clone(),
        chat_id,
        message_id,
        user_id,
    })
}

async fn send_with_sender(
    sender: &dyn ChannelOutboundSender,
    target: &ReplyTarget,
    reply: &str,
) -> OutboundResult {
    sender.send_text_reply(target, reply).await
}

/// Send an interactive card through the configured channel sender.
/// Returns `Ok(OutboundResult)` on completion, `Err(reason)` if the
/// channel cannot send (e.g. unsupported channel).
pub async fn deliver_interactive_card(
    config: &Config,
    inbound: &InboundMessage,
    card: &serde_json::Value,
) -> Result<OutboundResult, String> {
    use crate::channels::adapters::outbound::{
        FeishuOutboundSender, LarkOutboundSender, MockOutboundSender,
    };
    let channel_name_for_log = format!("{:?}", inbound.channel).to_lowercase();
    let entry = match channel_entry_for_outbound(config, &inbound.channel) {
        Some(e) => e,
        None => return Err(format!("no channel entry for {}", channel_name_for_log)),
    };
    let provider = match inbound.channel {
        ChannelKind::Feishu => "feishu",
        ChannelKind::Lark => "lark",
        _ => return Err("unsupported_channel".to_string()),
    };
    let target = match reply_target_from_inbound(inbound) {
        Some(t) => t,
        None => return Err("missing_reply_target".to_string()),
    };
    let outbound_mode = config_string(entry, "outbound_mode")
        .unwrap_or_else(|| "disabled".to_string())
        .to_ascii_lowercase();
    if outbound_mode == "disabled" {
        return Err("outbound_disabled".to_string());
    }
    match outbound_mode.as_str() {
        "mock" => {
            let sender = MockOutboundSender::new();
            Ok(sender.send_interactive_card(&target, card).await)
        }
        "real" => {
            let _app_id = match config_string(entry, "app_id") {
                Some(v) => v,
                None => return Err("missing_app_id".to_string()),
            };
            let _app_secret = match config_string(entry, "app_secret") {
                Some(v) => v,
                None => return Err("missing_app_secret".to_string()),
            };
            let token_cache = outbound_token_cache();
            match inbound.channel {
                ChannelKind::Feishu => {
                    let sender = FeishuOutboundSender::new(_app_id, _app_secret, token_cache);
                    Ok(sender.send_interactive_card(&target, card).await)
                }
                ChannelKind::Lark => {
                    let sender = LarkOutboundSender::new(_app_id, _app_secret, token_cache);
                    Ok(sender.send_interactive_card(&target, card).await)
                }
                _ => Err("unsupported_channel".to_string()),
            }
        }
        _ => Err(format!("outbound_mode_unsupported: {}", outbound_mode)),
    }
}

async fn deliver_platform_reply(
    config: &Config,
    inbound: &InboundMessage,
    reply: &str,
) -> Option<OutboundResult> {
    let entry = channel_entry_for_outbound(config, &inbound.channel)?;
    
    // Diagnostic: log outbound config
    let channel_name_for_log = format!("{:?}", inbound.channel).to_lowercase();
    let outbound_mode = config_string(entry, "outbound_mode")
        .unwrap_or_else(|| "disabled".to_string())
        .to_ascii_lowercase();
    let app_id_present = config_string(entry, "app_id").is_some();
    let app_secret_present = config_string(entry, "app_secret").is_some();
    println!(
        "[{}-webhook] outbound_config outbound_mode={} app_id_present={} app_secret_present={}",
        channel_name_for_log, outbound_mode, app_id_present, app_secret_present
    );

    if outbound_mode == "disabled" {
        println!("[{}-webhook] outbound_skip reason=disabled", channel_name_for_log);
        return None;
    }

    let provider = match inbound.channel {
        ChannelKind::Feishu => "feishu",
        ChannelKind::Lark => "lark",
        _ => {
            println!("[{}-webhook] outbound_skip reason=unsupported_channel", channel_name_for_log);
            return None;
        }
    };
    
    if reply.trim().is_empty() {
        println!("[{}-webhook] outbound_skip reason=empty_reply", channel_name_for_log);
        return Some(OutboundResult::skipped_empty_reply(provider));
    }
    
    // Diagnostic: log reply target
    let target = reply_target_from_inbound(inbound);
    println!(
        "[{}-webhook] outbound_target chat_id_present={} message_id_present={} user_id_present={}",
        channel_name_for_log,
        target.as_ref().map(|t| !t.chat_id.is_empty()).unwrap_or(false),
        target.as_ref().and_then(|t| t.message_id.as_ref()).map(|s| !s.is_empty()).unwrap_or(false),
        target.as_ref().and_then(|t| t.user_id.as_ref()).map(|s| !s.is_empty()).unwrap_or(false)
    );
    
    let Some(target) = target else {
        println!("[{}-webhook] outbound_skip reason=missing_reply_target", channel_name_for_log);
        return Some(OutboundResult::failed(
            provider,
            "missing_reply_target",
            "reply target was missing",
        ));
    };

    match outbound_mode.as_str() {
        "mock" => {
            println!("[{}-outbound] selected_sender sender=mock", channel_name_for_log);
            let sender = MockOutboundSender::new();
            Some(send_with_sender(&sender, &target, reply).await)
        }
        "real" => {
            let Some(_app_id) = config_string(entry, "app_id") else {
                println!("[{}-outbound] outbound_skip reason=missing_app_id", channel_name_for_log);
                return Some(OutboundResult::not_configured(
                    provider,
                    "app_id was missing",
                ));
            };
            let Some(_app_secret) = config_string(entry, "app_secret") else {
                println!("[{}-outbound] outbound_skip reason=missing_app_secret", channel_name_for_log);
                return Some(OutboundResult::not_configured(
                    provider,
                    "app_secret was missing",
                ));
            };
            println!("[{}-outbound] selected_sender provider={}", channel_name_for_log, provider);
            let token_cache = outbound_token_cache();
            let result = match inbound.channel {
                ChannelKind::Feishu => {
                    let sender = FeishuOutboundSender::new(_app_id, _app_secret, token_cache);
                    send_with_sender(&sender, &target, reply).await
                }
                ChannelKind::Lark => {
                    let sender = LarkOutboundSender::new(_app_id, _app_secret, token_cache);
                    send_with_sender(&sender, &target, reply).await
                }
                _ => OutboundResult::failed(
                    provider,
                    "not_implemented",
                    "sender was not implemented",
                ),
            };
            println!(
                "[{}-outbound] send_result ok={} delivery={:?}",
                channel_name_for_log, result.ok, result.delivery
            );
            
            // Record outbound message_id for self-message filtering
            if result.ok {
                if let Some(msg_id) = result.platform_message_id.as_ref() {
                    let channel_str = format!("{:?}", inbound.channel).to_lowercase();
                    OutboundMsgCache::global()
                        .record_outbound(&channel_str, msg_id)
                        .await;
                    println!(
                        "[{}-outbound] recorded_outbound_message_id=true ttl_secs=1800",
                        channel_name_for_log
                    );
                }
            }
            
            Some(result)
        }
        _ => {
            println!("[{}-webhook] outbound_skip reason=invalid_mode", channel_name_for_log);
            Some(OutboundResult::not_configured(
                provider,
                &format!("outbound_mode '{}' must be disabled, mock, or real", outbound_mode),
            ))
        }
    }
}

/// Resolve DingTalk app_secret from channel config (extra or env var) OR
/// the new top-level `gateway.dingtalk` config block.
///
/// Order (first non-empty wins):
/// 1. Channel entry's `app_secret` field in `extra`
/// 2. `gateway.dingtalk.app_secret` (inline)
/// 3. `gateway.dingtalk.app_secret_env` env var
/// 4. `OMNINOVA_DINGTALK_APP_SECRET` env var directly
fn resolve_dingtalk_secret(
    config: &Config,
    entry: Option<&crate::config::schema::ChannelEntry>,
) -> Option<String> {
    if let Some(entry) = entry {
        if let Some(v) = entry
            .extra
            .get("app_secret")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(v.to_string());
        }
    }

    let cfg = &config.gateway.dingtalk;
    if !cfg.app_secret.trim().is_empty() {
        return Some(cfg.app_secret.trim().to_string());
    }

    if let Some(ref env_name) = cfg.app_secret_env {
        if let Ok(v) = std::env::var(env_name) {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    if let Ok(v) = std::env::var("OMNINOVA_DINGTALK_APP_SECRET") {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    None
}

/// Deliver reply via DingTalk sessionWebhook (Phase 1 outbound)
async fn deliver_dingtalk_reply(
    config: &Config,
    inbound: &InboundMessage,
    reply: &str,
) -> Result<(), String> {
    let entry = config.channels_config.dingtalk.as_ref();

    // Outbound mode priority:
    //   1. `gateway.dingtalk.outbound_mode`  (new top-level, default `session_webhook`)
    //   2. legacy `channels_config.dingtalk.extra["outbound_mode"]`
    // Treat absent / empty as `"disabled"` so the legacy "did not set a mode"
    // behavior keeps working.
    let outbound_mode = {
        let top = config.gateway.dingtalk.outbound_mode.trim();
        if !top.is_empty() {
            top.to_string()
        } else {
            entry
                .and_then(|e| e.extra.get("outbound_mode"))
                .and_then(|v| v.as_str())
                .unwrap_or("disabled")
                .to_string()
        }
    };

    if outbound_mode == "disabled" {
        println!("[dingtalk-webhook] outbound_skip reason=disabled");
        return Ok(());
    }

    if outbound_mode == "mock" {
        println!("[dingtalk-webhook] outbound_selected sender=mock");
        return Ok(());
    }

    // real mode: use sessionWebhook
    let session_webhook_present = inbound
        .metadata
        .get("sessionWebhook")
        .and_then(|v| v.as_str())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    if !session_webhook_present {
        println!(
            "[dingtalk-webhook] session_webhook_missing fallback=robot_code_required \
             inbound_session_id_present={}",
            inbound.session_id.is_some()
        );
    }

    let app_key = resolve_dingtalk_app_key(config, entry)
        .ok_or_else(|| "missing_app_key".to_string())?;
    let app_secret = resolve_dingtalk_secret(config, entry)
        .ok_or_else(|| "missing_app_secret".to_string())?;

    let token = dingtalk_worker::fetch_dingtalk_access_token(&app_key, &app_secret)
        .await
        .map_err(|e| format!("token_fetch_error: {}", e))?;

    // Fallback robot_code comes from the gateway top-level config so
    // the send path can build a valid `sendFromApp` body even when the
    // inbound payload does not carry a `robotCode` field.
    let fallback_robot_code = resolve_dingtalk_robot_code(config, entry);

    dingtalk_worker::send_dingtalk_text_message(
        &token,
        inbound,
        fallback_robot_code.as_deref(),
        reply,
    )
    .await
}

/// Resolve DingTalk app_key from channel config (extra) OR the top-level
/// `gateway.dingtalk` config. Order: channel extra, gateway.dingtalk.app_key,
/// gateway.dingtalk.app_key_env, `OMNINOVA_DINGTALK_APP_KEY` env var.
fn resolve_dingtalk_app_key(
    config: &Config,
    entry: Option<&crate::config::schema::ChannelEntry>,
) -> Option<String> {
    if let Some(entry) = entry {
        if let Some(v) = entry
            .extra
            .get("app_key")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(v.to_string());
        }
    }

    let cfg = &config.gateway.dingtalk;
    if !cfg.app_key.trim().is_empty() {
        return Some(cfg.app_key.trim().to_string());
    }

    if let Some(ref env_name) = cfg.app_key_env {
        if let Ok(v) = std::env::var(env_name) {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    if let Ok(v) = std::env::var("OMNINOVA_DINGTALK_APP_KEY") {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    None
}

/// Resolve DingTalk robot_code with the same precedence as app_key.
fn resolve_dingtalk_robot_code(
    config: &Config,
    entry: Option<&crate::config::schema::ChannelEntry>,
) -> Option<String> {
    if let Some(entry) = entry {
        if let Some(v) = entry
            .extra
            .get("robot_code")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(v.to_string());
        }
    }

    let cfg = &config.gateway.dingtalk;
    if !cfg.robot_code.trim().is_empty() {
        return Some(cfg.robot_code.trim().to_string());
    }

    if let Some(ref env_name) = cfg.robot_code_env {
        if let Ok(v) = std::env::var(env_name) {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    if let Ok(v) = std::env::var("OMNINOVA_DINGTALK_ROBOT_CODE") {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    None
}

/// Returns true if the top-level `gateway.dingtalk` block is enabled.
/// Per-channel `channels_config.dingtalk.enabled` continues to be honored
/// by `is_channel_enabled`; this helper is for master-switch checks.
fn is_dingtalk_gateway_enabled(config: &Config) -> bool {
    config.gateway.dingtalk.enabled
}

/// Effective DingTalk enablement: top-level master switch OR legacy
/// channel entry. Either being on is sufficient to accept inbound
/// webhooks; the per-channel flag exists for backwards compatibility with
/// existing config.toml files that only set `channels.dingtalk.enabled`.
/// Both default to disabled, so a Config with neither set yields false.
fn is_dingtalk_effectively_enabled(config: &Config) -> bool {
    if is_dingtalk_gateway_enabled(config) {
        return true;
    }
    if is_channel_enabled(config, &ChannelKind::Dingtalk) {
        return true;
    }
    false
}

/// Sanitized DingTalk configuration snapshot used by desktop diagnostics.
/// It deliberately exposes only presence booleans and non-secret mode/path
/// labels while reusing the same resolvers as the live outbound path.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DingtalkDiagnosticConfigState {
    pub enabled: bool,
    pub app_key_present: bool,
    pub app_secret_present: bool,
    pub robot_code_present: bool,
    pub webhook_path: String,
    pub outbound_mode: String,
}

pub fn dingtalk_diagnostic_config_state(config: &Config) -> DingtalkDiagnosticConfigState {
    let entry = config.channels_config.dingtalk.as_ref();
    let top_level_mode = config.gateway.dingtalk.outbound_mode.trim();
    let outbound_mode = if top_level_mode.is_empty() {
        entry
            .and_then(|entry| entry.extra.get("outbound_mode"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("disabled")
            .to_string()
    } else {
        top_level_mode.to_string()
    };

    DingtalkDiagnosticConfigState {
        enabled: is_dingtalk_effectively_enabled(config),
        app_key_present: resolve_dingtalk_app_key(config, entry).is_some(),
        app_secret_present: resolve_dingtalk_secret(config, entry).is_some(),
        robot_code_present: resolve_dingtalk_robot_code(config, entry).is_some(),
        webhook_path: "/api/v1/gateway/dingtalk/events".to_string(),
        outbound_mode,
    }
}

// ---------------------------------------------------------------------------
// DingTalk payload parsing helpers (Phase 3 — real-platform compatibility)
// ---------------------------------------------------------------------------
//
// Real DingTalk enterprise app bot v1 callbacks deliver the user-visible
// text under `text.content`, not as a flat `text` string. The same body
// shape, however, also travels through some proxies / SDKs as a flat
// `text` field. Both shapes are accepted here so the same handler can
// process real production callbacks AND the synthetic payloads used in
// tests.

/// Extract the user-visible text from a DingTalk callback payload,
/// preferring the nested `text.content` shape and falling back to a
/// flat `text` string when only that one is present.
fn extract_dingtalk_text(payload: &serde_json::Value) -> String {
    if let Some(content) = payload
        .get("text")
        .and_then(|v| v.get("content"))
        .and_then(|v| v.as_str())
    {
        return content.to_string();
    }
    if let Some(s) = payload.get("text").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    String::new()
}

/// Length of the user-visible text without copying it out — used in
/// log lines that must not contain the message body itself.
fn extract_dingtalk_text_len(payload: &serde_json::Value) -> usize {
    extract_dingtalk_text(payload).chars().count()
}

// ---------------------------------------------------------------------------
// Worker-callable resolvers (used by `dingtalk_worker` at runtime, not only in tests)
// ---------------------------------------------------------------------------

pub(crate) fn resolve_dingtalk_secret_for_worker(
    config: &Config,
    entry: Option<&crate::config::schema::ChannelEntry>,
) -> Option<String> {
    resolve_dingtalk_secret(config, entry)
}

pub(crate) fn resolve_dingtalk_app_key_for_worker(
    config: &Config,
    entry: Option<&crate::config::schema::ChannelEntry>,
) -> Option<String> {
    resolve_dingtalk_app_key(config, entry)
}

pub(crate) fn resolve_dingtalk_robot_code_for_worker(
    config: &Config,
    entry: Option<&crate::config::schema::ChannelEntry>,
) -> Option<String> {
    resolve_dingtalk_robot_code(config, entry)
}

// ---------------------------------------------------------------------------
// Test-only re-exports (used by `dingtalk_worker` wrappers in unit tests)
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) fn resolve_dingtalk_secret_for_test(
    config: &Config,
    entry: Option<&crate::config::schema::ChannelEntry>,
) -> Option<String> {
    resolve_dingtalk_secret(config, entry)
}

#[cfg(test)]
pub(crate) fn resolve_dingtalk_app_key_for_test(
    config: &Config,
    entry: Option<&crate::config::schema::ChannelEntry>,
) -> Option<String> {
    resolve_dingtalk_app_key(config, entry)
}

#[cfg(test)]
pub(crate) fn resolve_dingtalk_robot_code_for_test(
    config: &Config,
    entry: Option<&crate::config::schema::ChannelEntry>,
) -> Option<String> {
    resolve_dingtalk_robot_code(config, entry)
}

#[cfg(test)]
pub(crate) fn extract_dingtalk_text_for_test(payload: &serde_json::Value) -> String {
    extract_dingtalk_text(payload)
}

/// List of HTTP paths the DingTalk webhook handler is registered at.
/// Exposed for the Phase 3 integration regression tests so we can
/// confirm both the documented callback URL and the legacy alias are
/// bound to the same handler without spinning up a full router.
#[cfg(test)]
pub(crate) fn dingtalk_known_route_paths_for_test() -> &'static [&'static str] {
    &[
        "/webhook/dingtalk",
        "/api/v1/gateway/dingtalk/events",
    ]
}

#[cfg(test)]
pub(crate) fn is_dingtalk_effectively_enabled_for_test(config: &Config) -> bool {
    is_dingtalk_effectively_enabled(config)
}

fn signed_webhook_payload(
    config: &Config,
    headers: &HeaderMap,
    raw_body: &str,
) -> anyhow::Result<String> {
    if !config.gateway.webhook_signing_include_timestamp {
        return Ok(raw_body.to_string());
    }
    let timestamp = headers
        .get("x-omninova-timestamp")
        .or_else(|| headers.get("x-timestamp"))
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty());
    match timestamp {
        Some(ts) => Ok(format!("{ts}.{raw_body}")),
        None => {
            if config.gateway.webhook_signing_require_timestamp {
                anyhow::bail!("missing timestamp header for webhook signature payload")
            }
            Ok(raw_body.to_string())
        }
    }
}

fn webhook_signing_secret(config: &Config) -> Option<String> {
    let webhook = config.channels_config.webhook.as_ref()?;
    if let Some(secret) = webhook
        .extra
        .get("signing_secret")
        .and_then(serde_json::Value::as_str)
    {
        return Some(secret.to_string());
    }
    if let Some(env_key) = webhook
        .extra
        .get("signing_secret_env")
        .and_then(serde_json::Value::as_str)
    {
        return std::env::var(env_key).ok().filter(|v| !v.trim().is_empty());
    }
    None
}

fn channel_webhook_signing_secret(config: &Config, channel: &ChannelKind) -> Option<String> {
    let entry = match channel {
        ChannelKind::Wechat => config.channels_config.wechat.as_ref(),
        ChannelKind::Feishu => config.channels_config.feishu.as_ref(),
        ChannelKind::Lark => config.channels_config.lark.as_ref(),
        ChannelKind::Dingtalk => config.channels_config.dingtalk.as_ref(),
        _ => None,
    };

    channel_entry_signing_secret(entry).or_else(|| webhook_signing_secret(config))
}

/// Check if a channel is enabled in the configuration
fn is_channel_enabled(config: &Config, channel: &ChannelKind) -> bool {
    let entry = match channel {
        ChannelKind::Wechat => config.channels_config.wechat.as_ref(),
        ChannelKind::Feishu => config.channels_config.feishu.as_ref(),
        ChannelKind::Lark => config.channels_config.lark.as_ref(),
        ChannelKind::Dingtalk => config.channels_config.dingtalk.as_ref(),
        ChannelKind::Telegram => config.channels_config.telegram.as_ref(),
        ChannelKind::Discord => config.channels_config.discord.as_ref(),
        ChannelKind::Slack => config.channels_config.slack.as_ref(),
        ChannelKind::Whatsapp => config.channels_config.whatsapp.as_ref(),
        ChannelKind::Matrix => config.channels_config.matrix.as_ref(),
        ChannelKind::Irc => config.channels_config.irc.as_ref(),
        ChannelKind::Email => config.channels_config.email.as_ref(),
        ChannelKind::Msteams => config.channels_config.msteams.as_ref(),
        _ => None,
    };
    entry.map(|e| e.enabled).unwrap_or(false)
}

fn channel_entry_signing_secret(
    entry: Option<&crate::config::schema::ChannelEntry>,
) -> Option<String> {
    let entry = entry?;
    if let Some(secret) = entry
        .extra
        .get("signing_secret")
        .and_then(serde_json::Value::as_str)
    {
        return Some(secret.to_string());
    }
    if let Some(env_key) = entry
        .extra
        .get("signing_secret_env")
        .and_then(serde_json::Value::as_str)
    {
        return std::env::var(env_key).ok().filter(|v| !v.trim().is_empty());
    }
    None
}

/// Security mode for Feishu webhook
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeishuSecurityMode {
    /// Dev mode: no verification, but log warnings
    Dev,
    /// Token mode: verify verification_token
    Token,
    /// Encrypted mode: decrypt encrypted payload
    Encrypted,
    /// Default: dev mode for backwards compatibility
    Default,
    /// Explicit but unsupported security mode
    Invalid,
}

impl FeishuSecurityMode {
    pub fn from_str(s: Option<&str>) -> Self {
        match s.map(|s| s.to_lowercase()).as_deref() {
            Some("dev") => FeishuSecurityMode::Dev,
            Some("token") => FeishuSecurityMode::Token,
            Some("encrypted") => FeishuSecurityMode::Encrypted,
            None => FeishuSecurityMode::Default,
            Some(_) => FeishuSecurityMode::Invalid,
        }
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            FeishuSecurityMode::Dev => "dev",
            FeishuSecurityMode::Token => "token",
            FeishuSecurityMode::Encrypted => "encrypted",
            FeishuSecurityMode::Default => "dev",
            FeishuSecurityMode::Invalid => "invalid",
        }
    }
}

/// Security configuration for Feishu channel
#[derive(Debug, Clone)]
pub struct FeishuSecurityConfig {
    pub mode: FeishuSecurityMode,
    pub verification_token: Option<String>,
    pub encrypt_key: Option<String>,
    pub insecure: bool,
}

impl FeishuSecurityConfig {
    fn is_configured_secret(value: &str) -> bool {
        !value.trim().is_empty() && value != "***SET***"
    }

    /// Get verification token from channel entry (config or env var)
    pub fn get_verification_token(entry: Option<&crate::config::schema::ChannelEntry>) -> Option<String> {
        let entry = entry?;
        
        // Direct value
        if let Some(ref token) = entry.verification_token {
            if Self::is_configured_secret(token) {
                return Some(token.clone());
            }
        }
        
        // Env var reference. Setup transports these references in `extra` so
        // saving unrelated fields cannot expose or discard the secret.
        let env_key = entry
            .verification_token_env
            .as_deref()
            .or_else(|| entry.extra.get("verification_token_env").and_then(serde_json::Value::as_str));
        if let Some(env_key) = env_key {
            return std::env::var(env_key)
                .ok()
                .filter(|value| Self::is_configured_secret(value));
        }
        
        // Legacy: check extra for verification_token
        if let Some(token) = entry.extra.get("verification_token")
            .and_then(serde_json::Value::as_str)
        {
            if Self::is_configured_secret(token) {
                return Some(token.to_string());
            }
        }
        
        None
    }
    
    /// Get encryption key from channel entry (config or env var)
    pub fn get_encrypt_key(entry: Option<&crate::config::schema::ChannelEntry>) -> Option<String> {
        let entry = entry?;
        
        // Direct value
        if let Some(ref key) = entry.encrypt_key {
            if Self::is_configured_secret(key) {
                return Some(key.clone());
            }
        }
        
        // See verification_token_env above for why the `extra` fallback is
        // required for the Tauri setup transport.
        let env_key = entry
            .encrypt_key_env
            .as_deref()
            .or_else(|| entry.extra.get("encrypt_key_env").and_then(serde_json::Value::as_str));
        if let Some(env_key) = env_key {
            return std::env::var(env_key)
                .ok()
                .filter(|value| Self::is_configured_secret(value));
        }
        
        // Legacy: check extra for encrypt_key
        if let Some(key) = entry.extra.get("encrypt_key")
            .and_then(serde_json::Value::as_str)
        {
            if Self::is_configured_secret(key) {
                return Some(key.to_string());
            }
        }
        
        None
    }
    
    /// Build security config from channel entry
    pub fn from_entry(entry: Option<&crate::config::schema::ChannelEntry>) -> Self {
        let security_mode_str = entry
            .and_then(|e| e.security_mode.as_deref().or_else(|| {
                e.extra
                    .get("security_mode")
                    .and_then(serde_json::Value::as_str)
            }));
        let security_mode = FeishuSecurityMode::from_str(security_mode_str);
        
        let verification_token = Self::get_verification_token(entry);
        let encrypt_key = Self::get_encrypt_key(entry);
        
        // dev is deliberately permissive for local integration work, but must
        // always be reported as insecure even if a token happens to be stored.
        let insecure = matches!(security_mode, FeishuSecurityMode::Dev | FeishuSecurityMode::Default);
        
        Self {
            mode: security_mode,
            verification_token,
            encrypt_key,
            insecure,
        }
    }
}

/// Sanitized Feishu configuration snapshot used by desktop diagnostics.
/// Secret-bearing values are reduced to presence booleans before the value
/// leaves core, so the renderer never receives credentials.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct FeishuDiagnosticConfigState {
    pub enabled: bool,
    pub app_id_present: bool,
    pub app_secret_present: bool,
    pub verification_token_present: bool,
    pub encrypt_key_present: bool,
    pub security_mode: String,
    pub outbound_mode: String,
}

pub fn feishu_diagnostic_config_state(config: &Config) -> FeishuDiagnosticConfigState {
    let entry = config.channels_config.feishu.as_ref();
    let security = FeishuSecurityConfig::from_entry(entry);
    let configured_extra = |key: &str| {
        entry
            .and_then(|entry| entry.extra.get(key))
            .and_then(serde_json::Value::as_str)
            .is_some_and(FeishuSecurityConfig::is_configured_secret)
    };

    FeishuDiagnosticConfigState {
        enabled: entry.is_some_and(|entry| entry.enabled),
        app_id_present: configured_extra("app_id"),
        app_secret_present: configured_extra("app_secret"),
        verification_token_present: security.verification_token.is_some(),
        encrypt_key_present: security.encrypt_key.is_some(),
        security_mode: security.mode.as_str().to_string(),
        outbound_mode: entry
            .and_then(|entry| entry.extra.get("outbound_mode"))
            .and_then(serde_json::Value::as_str)
            .filter(|mode| !mode.trim().is_empty())
            .unwrap_or("disabled")
            .to_string(),
    }
}

/// Verify the verification token from the request
pub fn verify_feishu_verification_token(
    security_config: &FeishuSecurityConfig,
    request_token: Option<&str>,
) -> Result<bool, String> {
    match security_config.mode {
        FeishuSecurityMode::Dev | FeishuSecurityMode::Default => {
            Ok(true)
        }
        FeishuSecurityMode::Token | FeishuSecurityMode::Encrypted => {
            let expected_token = security_config.verification_token.as_deref()
                .ok_or_else(|| "verification_token not configured".to_string())?;

            match request_token {
                Some(token) if token == expected_token => Ok(true),
                Some(_) => Ok(false), // Token mismatch
                None => Err("verification_token missing from request".to_string()),
            }
        }
        FeishuSecurityMode::Invalid => Err("invalid security mode".to_string()),
    }
}

/// Check if payload is encrypted (Feishu encrypt field)
pub fn is_encrypted_payload(payload: &serde_json::Value) -> bool {
    // Feishu encrypted event has "encrypt" field with the encrypted content
    payload.get("encrypt").and_then(|v| v.as_str()).is_some()
}

/// Decrypt an encrypted Feishu payload using AES-256-CBC
/// 
/// Feishu uses AES-256-CBC with:
/// - Key: first 32 bytes of SHA-256 of the encrypt_key
/// - IV: first 16 bytes of the decoded encrypted event payload
/// 
/// Returns the decrypted JSON string, or error message.
pub fn decrypt_feishu_payload(
    encrypted_base64: &str,
    encrypt_key: &str,
) -> Result<String, String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use sha2::{Sha256, Digest};
    
    // Decode base64
    let encrypted_bytes = BASE64.decode(encrypted_base64)
        .map_err(|e| format!("base64 decode failed: {}", e))?;
    
    if encrypted_bytes.len() <= 16 || encrypted_bytes.len() % 16 != 0 {
        return Err("encrypted payload length invalid".to_string());
    }
    
    // Extract IV (first 16 bytes) and ciphertext
    let iv = &encrypted_bytes[..16];
    let ciphertext = &encrypted_bytes[16..];
    
    // Derive key: SHA-256 of encrypt_key
    let mut hasher = Sha256::new();
    hasher.update(encrypt_key.as_bytes());
    let key_hash = hasher.finalize();
    let key: [u8; 32] = key_hash[..32].try_into().unwrap();
    
    // Decrypt using AES-256-CBC
    let result = decrypt_aes256_cbc(ciphertext, &key, iv)
        .map_err(|e| format!("decrypt failed: {}", e))?;
    
    // Remove PKCS7 padding
    let padding_len = result[result.len() - 1] as usize;
    if padding_len == 0 || padding_len > 16 {
        return Err("invalid padding".to_string());
    }
    for i in 0..padding_len {
        if result[result.len() - 1 - i] != padding_len as u8 {
            return Err("invalid padding".to_string());
        }
    }
    
    let plaintext = &result[..result.len() - padding_len];
    String::from_utf8(plaintext.to_vec())
        .map_err(|e| format!("utf8 decode failed: {}", e))
}

/// Decrypt AES-256-CBC using raw AES-256 operations
/// Feishu uses AES-256-CBC with PKCS7 padding
fn decrypt_aes256_cbc(ciphertext: &[u8], key: &[u8; 32], iv: &[u8]) -> Result<Vec<u8>, String> {
    use aes::Aes256;
    use aes::cipher::{Block, BlockDecrypt, KeyInit};
    
    let cipher = Aes256::new_from_slice(key)
        .map_err(|e| format!("cipher init failed: {:?}", e))?;
    
    let block_size = 16;
    let mut plaintext = Vec::with_capacity(ciphertext.len());
    let mut previous = iv.to_vec();
    
    for chunk in ciphertext.chunks(block_size) {
        // Create a mutable copy of the chunk
        let mut block_bytes = [0u8; 16];
        block_bytes.copy_from_slice(chunk);
        let mut block = Block::<Aes256>::from(block_bytes);
        
        // Decrypt block
        cipher.decrypt_block(&mut block);
        
        // XOR with previous ciphertext block (or IV for first block)
        let mut xored = [0u8; 16];
        for i in 0..16 {
            xored[i] = block[i] ^ previous[i];
        }
        plaintext.extend_from_slice(&xored);
        
        previous.copy_from_slice(chunk);
    }
    
    Ok(plaintext)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct SessionStoreFile {
    #[serde(default)]
    sessions: HashMap<String, SessionRecord>,
}

fn session_store_path(config: &Config) -> PathBuf {
    config.workspace_dir.join(".omninova-sessions.json")
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct SessionRecord {
    #[serde(default)]
    messages: Vec<ChatMessage>,
    #[serde(default)]
    parent_session_key: Option<String>,
    #[serde(default)]
    parent_agent_id: Option<String>,
    #[serde(default)]
    agent_name: Option<String>,
    #[serde(default)]
    spawn_depth: u32,
    updated_at: i64,
}

#[derive(Debug, Clone, Default)]
struct SessionLineageMeta {
    parent_session_key: Option<String>,
    parent_agent_id: Option<String>,
    agent_name: Option<String>,
    spawn_depth: u32,
    updated_at: i64,
}

fn session_key(channel: &ChannelKind, session_id: &str) -> String {
    format!("{:?}:{session_id}", channel).to_lowercase()
}

fn split_session_key(key: &str) -> (Option<String>, Option<String>) {
    let Some((channel, session_id)) = key.split_once(':') else {
        return (None, Some(key.to_string()));
    };
    (Some(channel.to_string()), Some(session_id.to_string()))
}

fn match_session_tree_filters(
    entry: &GatewaySessionTreeNode,
    query: &GatewaySessionTreeQuery,
) -> bool {
    let case_insensitive = query.case_insensitive.unwrap_or(true);
    let cmp = |left: Option<&str>, right: Option<&str>| -> bool {
        match (left, right) {
            (Some(l), Some(r)) if case_insensitive => l.eq_ignore_ascii_case(r),
            (Some(l), Some(r)) => l == r,
            _ => false,
        }
    };
    if let Some(session_id) = query.session_id.as_deref() {
        if !cmp(entry.session_id.as_deref(), Some(session_id)) {
            return false;
        }
    }
    if let Some(session_key) = query.session_key.as_deref() {
        if !cmp(entry.session_key.as_deref(), Some(session_key)) {
            return false;
        }
    }
    if let Some(parent_session_key) = query.parent_session_key.as_deref() {
        if !cmp(
            entry.parent_session_key.as_deref(),
            Some(parent_session_key),
        ) {
            return false;
        }
    }
    if let Some(parent_session_id) = query.parent_session_id.as_deref() {
        let parent_session_id_actual = entry
            .parent_session_key
            .as_deref()
            .and_then(|key| split_session_key(key).1);
        if !cmp(parent_session_id_actual.as_deref(), Some(parent_session_id)) {
            return false;
        }
    }
    if let Some(agent_name) = query.agent_name.as_deref() {
        if !cmp(entry.agent_name.as_deref(), Some(agent_name)) {
            return false;
        }
    }
    if let Some(parent_agent_id) = query.parent_agent_id.as_deref() {
        if !cmp(entry.parent_agent_id.as_deref(), Some(parent_agent_id)) {
            return false;
        }
    }
    if let Some(channel) = query.channel.as_deref() {
        if !cmp(entry.channel.as_deref(), Some(channel)) {
            return false;
        }
    }
    if let Some(source) = query.source.as_deref() {
        if !cmp(Some(entry.source.as_str()), Some(source)) {
            return false;
        }
    }
    if let Some(min_depth) = query.min_spawn_depth {
        if entry.spawn_depth < min_depth {
            return false;
        }
    }
    if let Some(max_depth) = query.max_spawn_depth {
        if entry.spawn_depth > max_depth {
            return false;
        }
    }
    if let Some(contains) = query.contains.as_deref() {
        let hay = format!(
            "{}|{}|{}|{}",
            entry.session_key.clone().unwrap_or_default(),
            entry.session_id.clone().unwrap_or_default(),
            entry.agent_name.clone().unwrap_or_default(),
            entry.parent_session_key.clone().unwrap_or_default()
        );
        let contains_match = if case_insensitive {
            hay.to_lowercase().contains(&contains.to_lowercase())
        } else {
            hay.contains(contains)
        };
        if !contains_match {
            return false;
        }
    }
    true
}

fn sort_session_tree_entries(
    entries: &mut [GatewaySessionTreeNode],
    query: &GatewaySessionTreeQuery,
) {
    let sort_by = query.sort_by.as_deref().unwrap_or("updated_at");
    let asc = query
        .sort_order
        .as_deref()
        .map(|v| v == "asc")
        .unwrap_or(false);
    entries.sort_by(|a, b| {
        let ord = match sort_by {
            "spawn_depth" => a.spawn_depth.cmp(&b.spawn_depth),
            "session_id" => a.session_id.cmp(&b.session_id),
            "agent_name" => a.agent_name.cmp(&b.agent_name),
            _ => a.updated_at.cmp(&b.updated_at),
        };
        if asc {
            ord
        } else {
            ord.reverse()
        }
    });
}

fn normalize_session_tree_query(query: &GatewaySessionTreeQuery) -> GatewaySessionTreeQuery {
    let mut normalized = query.clone();
    normalized.session_id = normalized.session_id.map(|v| v.trim().to_string());
    normalized.session_key = normalized.session_key.map(|v| v.trim().to_string());
    normalized.parent_session_id = normalized.parent_session_id.map(|v| v.trim().to_string());
    normalized.parent_session_key = normalized.parent_session_key.map(|v| v.trim().to_string());
    normalized.agent_name = normalized.agent_name.map(|v| v.trim().to_string());
    normalized.parent_agent_id = normalized.parent_agent_id.map(|v| v.trim().to_string());
    normalized.channel = normalized.channel.map(|v| v.trim().to_string());
    normalized.source = normalized.source.map(|v| v.trim().to_string());
    normalized.contains = normalized.contains.map(|v| v.trim().to_string());
    normalized.sort_by = normalized
        .sort_by
        .map(|v| v.trim().to_lowercase())
        .filter(|v| {
            matches!(
                v.as_str(),
                "updated_at" | "spawn_depth" | "session_id" | "agent_name"
            )
        });
    normalized.sort_order = normalized
        .sort_order
        .map(|v| v.trim().to_lowercase())
        .filter(|v| matches!(v.as_str(), "asc" | "desc"));
    if normalized.offset.is_none() {
        normalized.offset = normalized.cursor;
    }
    normalized
}

fn count_session_sources(entries: &[GatewaySessionTreeNode]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for entry in entries {
        *counts.entry(entry.source.clone()).or_insert(0) += 1;
    }
    counts
}

fn compute_session_tree_stats(entries: &[GatewaySessionTreeNode]) -> GatewaySessionTreeStats {
    if entries.is_empty() {
        return GatewaySessionTreeStats::default();
    }
    let mut unique_agents = HashSet::new();
    let mut unique_parent_agents = HashSet::new();
    let mut max_spawn_depth = 0u32;
    let mut min_updated_at = i64::MAX;
    let mut max_updated_at = i64::MIN;

    for entry in entries {
        if let Some(agent_name) = entry.agent_name.as_deref() {
            unique_agents.insert(agent_name.to_string());
        }
        if let Some(parent_agent_id) = entry.parent_agent_id.as_deref() {
            unique_parent_agents.insert(parent_agent_id.to_string());
        }
        max_spawn_depth = max_spawn_depth.max(entry.spawn_depth);
        min_updated_at = min_updated_at.min(entry.updated_at);
        max_updated_at = max_updated_at.max(entry.updated_at);
    }

    GatewaySessionTreeStats {
        unique_agents: unique_agents.len(),
        unique_parent_agents: unique_parent_agents.len(),
        max_spawn_depth,
        min_updated_at,
        max_updated_at,
    }
}

fn now_unix_ts() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

fn channel_label(channel: &ChannelKind) -> String {
    match channel {
        ChannelKind::Web => "web".to_string(),
        ChannelKind::WebChat => "webchat".to_string(),
        ChannelKind::Cli => "cli".to_string(),
        other => format!("{:?}", other).to_lowercase(),
    }
}

/// ??????? `ChatMessage` ?? UI ?????
fn messages_for_chat_ui(messages: &[ChatMessage]) -> Vec<GatewayChatMessage> {
    let mut out = Vec::new();
    for msg in messages {
        match msg.role.as_str() {
            "user" => {
                let text = msg.content.trim();
                if !text.is_empty() {
                    out.push(GatewayChatMessage {
                        role: "user".into(),
                        content: msg.content.clone(),
                        agent: None,
                    });
                }
            }
            "assistant" => {
                if let Some(text) = assistant_text_for_ui(&msg.content) {
                    out.push(GatewayChatMessage {
                        role: "assistant".into(),
                        content: text,
                        agent: None,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

fn assistant_text_for_ui(content: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(content) {
        let has_tool_calls = value
            .get("tool_calls")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|a| !a.is_empty());
        if let Some(text) = value
            .get("content")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            return Some(text.to_string());
        }
        if has_tool_calls {
            return None;
        }
    }
    let trimmed = content.trim();
    if trimmed.is_empty() || trimmed.starts_with('{') {
        return None;
    }
    Some(trimmed.to_string())
}

async fn load_session_history(
    config: &Config,
    channel: &ChannelKind,
    session_id: &str,
) -> anyhow::Result<Vec<ChatMessage>> {
    let path = session_store_path(config);
    let store = load_session_store(&path).await?;
    let key = session_key(channel, session_id);
    let Some(record) = store.sessions.get(&key) else {
        return Ok(Vec::new());
    };
    let age = now_unix_ts() - record.updated_at;
    if age > config.gateway.session_ttl_secs as i64 {
        return Ok(Vec::new());
    }
    Ok(record.messages.clone())
}

async fn save_session_history(
    config: &Config,
    channel: &ChannelKind,
    session_id: &str,
    mut messages: Vec<ChatMessage>,
    max_history_messages: usize,
    parent_session_key: Option<String>,
    parent_agent_id: Option<String>,
    agent_name: String,
    spawn_depth: u32,
) -> anyhow::Result<()> {
    if max_history_messages > 0 && messages.len() > max_history_messages {
        let start = messages.len() - max_history_messages;
        messages = messages.split_off(start);
    }
    messages = sanitize_messages_for_provider(messages);

    let path = session_store_path(config);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut store = load_session_store(&path).await?;
    let now = now_unix_ts();
    store
        .sessions
        .retain(|_, record| now - record.updated_at <= config.gateway.session_ttl_secs as i64);

    let key = session_key(channel, session_id);
    store.sessions.insert(
        key,
        SessionRecord {
            messages,
            parent_session_key,
            parent_agent_id,
            agent_name: Some(agent_name),
            spawn_depth,
            updated_at: now,
        },
    );

    if store.sessions.len() > config.gateway.max_sessions {
        let mut entries: Vec<(String, SessionRecord)> = store.sessions.into_iter().collect();
        entries.sort_by(|a, b| b.1.updated_at.cmp(&a.1.updated_at));
        entries.truncate(config.gateway.max_sessions);
        store.sessions = entries.into_iter().collect();
    }

    let serialized = serde_json::to_string_pretty(&store)?;
    atomic_write_string(&path, &serialized).await?;
    Ok(())
}

async fn load_session_record(
    config: &Config,
    channel: &ChannelKind,
    session_id: &str,
) -> anyhow::Result<Option<SessionRecord>> {
    let key = session_key(channel, session_id);
    load_session_record_by_key(config, &key).await
}

async fn load_session_record_by_key(
    config: &Config,
    key: &str,
) -> anyhow::Result<Option<SessionRecord>> {
    let path = session_store_path(config);
    let store = load_session_store(&path).await?;
    Ok(store.sessions.get(key).cloned())
}

async fn load_session_store(path: &PathBuf) -> anyhow::Result<SessionStoreFile> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let _guard = acquire_lockfile_guard(path, 5_000, 60_000).await?;
    if !path.exists() {
        return Ok(SessionStoreFile::default());
    }
    let raw = tokio::fs::read_to_string(path).await.unwrap_or_default();
    match serde_json::from_str::<SessionStoreFile>(&raw) {
        Ok(v) => Ok(v),
        Err(e) => {
            let corrupt_path = path.with_extension(format!("corrupt.{}.json", now_unix_ts()));
            let _ = tokio::fs::rename(path, &corrupt_path).await;
            warn!(
                "session store corrupted (moved to {}): {}",
                corrupt_path.display(),
                e
            );
            Ok(SessionStoreFile::default())
        }
    }
}

async fn atomic_write_string(path: &PathBuf, content: &str) -> anyhow::Result<()> {
    let _guard = acquire_lockfile_guard(path, 5_000, 60_000).await?;
    let tmp = path.with_extension(format!("tmp.{}", now_unix_ts()));
    tokio::fs::write(&tmp, content).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

struct LockfileGuard {
    path: PathBuf,
}

impl Drop for LockfileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

async fn acquire_lockfile_guard(
    target: &PathBuf,
    timeout_ms: u64,
    stale_lock_ms: u64,
) -> anyhow::Result<LockfileGuard> {
    let lock_path = resolve_session_lock_path(target);
    let wait_started = std::time::Instant::now();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let mut retries: u32 = 0;

    loop {
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&lock_path)
        {
            Ok(_) => {
                let waited_ms = wait_started.elapsed().as_millis() as u64;
                if waited_ms >= 50 {
                    let events = SESSION_LOCK_WAIT_EVENTS.fetch_add(1, Ordering::Relaxed) + 1;
                    warn!(
                        "session lock contention: target={}, waited_ms={}, retries={}, total_events={}",
                        target.display(),
                        waited_ms,
                        retries,
                        events
                    );
                }
                return Ok(LockfileGuard { path: lock_path });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                retries = retries.saturating_add(1);
                if let Ok(meta) = std::fs::metadata(&lock_path) {
                    if let Ok(modified) = meta.modified() {
                        if let Ok(elapsed) = modified.elapsed() {
                            if elapsed > std::time::Duration::from_millis(stale_lock_ms) {
                                let _ = std::fs::remove_file(&lock_path);
                            }
                        }
                    }
                }
                if std::time::Instant::now() >= deadline {
                    let timeout_events =
                        SESSION_LOCK_TIMEOUT_EVENTS.fetch_add(1, Ordering::Relaxed) + 1;
                    warn!(
                        "session lock timeout: target={}, retries={}, total_timeouts={}",
                        target.display(),
                        retries,
                        timeout_events
                    );
                    anyhow::bail!("timed out waiting for session store lock");
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            Err(e) => return Err(anyhow::anyhow!("failed to acquire lock: {e}")),
        }
    }
}

fn resolve_session_lock_path(target: &PathBuf) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    target.hash(&mut hasher);
    let hash = hasher.finish();
    let lock_name = format!("session_{hash:016x}.lock");

    let candidates = [
        std::env::var("OMNINOVA_LOCK_DIR").ok().map(PathBuf::from),
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".omninova").join("locks")),
        Some(std::env::temp_dir().join("omninova-locks")),
    ];

    for candidate in candidates.into_iter().flatten() {
        if std::fs::create_dir_all(&candidate).is_ok() {
            return candidate.join(&lock_name);
        }
    }

    target.with_extension("lock")
}

async fn http_estop_status(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<EstopState>, Json<GatewayError>> {
    match runtime.estop_status().await {
        Ok(state) => Ok(Json(state)),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_estop_pause(
    State(runtime): State<GatewayRuntime>,
    Json(req): Json<GatewayEstopPauseRequest>,
) -> Result<Json<EstopState>, Json<GatewayError>> {
    match runtime
        .estop_pause(req.level, req.domain, req.tool, req.reason)
        .await
    {
        Ok(state) => Ok(Json(state)),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_estop_resume(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<EstopState>, Json<GatewayError>> {
    match runtime.estop_resume().await {
        Ok(state) => Ok(Json(state)),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_approvals_list(
    State(runtime): State<GatewayRuntime>,
    Query(query): Query<GatewayApprovalsQuery>,
) -> Result<Json<Vec<PendingApproval>>, Json<GatewayError>> {
    match runtime
        .list_approvals(query.pending_only.unwrap_or(true))
        .await
    {
        Ok(items) => Ok(Json(items)),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_approvals_approve(
    State(runtime): State<GatewayRuntime>,
    Path(id): Path<String>,
    Json(req): Json<GatewayApprovalActionRequest>,
) -> Result<Json<PendingApproval>, Json<GatewayError>> {
    match runtime.approve_request(&id, req.approved_by).await {
        Ok(item) => Ok(Json(item)),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_approvals_reject(
    State(runtime): State<GatewayRuntime>,
    Path(id): Path<String>,
    Json(req): Json<GatewayApprovalActionRequest>,
) -> Result<Json<PendingApproval>, Json<GatewayError>> {
    match runtime.reject_request(&id, req.reason).await {
        Ok(item) => Ok(Json(item)),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_api_status(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    let health = runtime.health().await;
    let cfg = runtime.get_config().await;
    let tools = create_default_tools(&cfg);
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    Ok(Json(serde_json::json!({
        "gateway": {
            "ok": health.ok,
            "provider": health.provider,
            "provider_healthy": health.provider_healthy,
            "memory_healthy": health.memory_healthy,
        },
        "config": {
            "default_provider": cfg.default_provider,
            "default_model": cfg.default_model,
            "gateway_host": cfg.gateway.host,
            "gateway_port": cfg.gateway.port,
            "agent_name": cfg.agent.name,
        },
        "tools": tool_names,
        "agents": cfg.agents.keys().collect::<Vec<_>>(),
    })))
}

async fn http_api_tools(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    let cfg = runtime.get_config().await;
    let tools = create_default_tools(&cfg);
    let specs: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name(),
                "description": t.description(),
                "parameters": t.parameters_schema(),
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "tools": specs })))
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ApiMemoryStoreRequest {
    key: String,
    content: String,
    category: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ApiMemoryForgetRequest {
    key: String,
}

async fn http_api_memory_list(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    let entries = runtime.memory.list(None, None).await.map_err(|e| {
        Json(GatewayError {
            message: e.to_string(),
        })
    })?;
    let items: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "key": e.key,
                "content": e.content,
                "category": format!("{:?}", e.category),
                "timestamp": e.timestamp,
            })
        })
        .collect();
    Ok(Json(
        serde_json::json!({ "entries": items, "count": items.len() }),
    ))
}

async fn http_api_memory_store(
    State(runtime): State<GatewayRuntime>,
    Json(req): Json<ApiMemoryStoreRequest>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    use crate::memory::MemoryCategory;
    let category = match req.category.as_deref() {
        Some("daily") => MemoryCategory::Daily,
        Some("conversation") => MemoryCategory::Conversation,
        _ => MemoryCategory::Core,
    };
    runtime
        .memory
        .store(&req.key, &req.content, category, None)
        .await
        .map_err(|e| {
            Json(GatewayError {
                message: e.to_string(),
            })
        })?;
    Ok(Json(serde_json::json!({ "ok": true, "key": req.key })))
}

async fn http_api_memory_forget(
    State(runtime): State<GatewayRuntime>,
    Json(req): Json<ApiMemoryForgetRequest>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    let removed = runtime.memory.forget(&req.key).await.map_err(|e| {
        Json(GatewayError {
            message: e.to_string(),
        })
    })?;
    Ok(Json(
        serde_json::json!({ "ok": true, "key": req.key, "removed": removed }),
    ))
}

async fn http_api_doctor(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    let health = runtime.health().await;
    let cfg = runtime.get_config().await;
    let estop = runtime.estop_status().await.ok();
    let session_tree = runtime.session_tree_snapshot().await.ok();
    let memory_count = runtime.memory.count().await.unwrap_or(0);

    let mut checks = Vec::new();
    checks.push(serde_json::json!({
        "check": "provider_health",
        "ok": health.provider_healthy,
        "detail": health.provider,
    }));
    checks.push(serde_json::json!({
        "check": "memory_health",
        "ok": health.memory_healthy,
        "detail": format!("{memory_count} entries"),
    }));
    checks.push(serde_json::json!({
        "check": "estop",
        "ok": estop.as_ref().map(|s| !s.paused).unwrap_or(true),
        "detail": estop.map(|s| if s.paused { "PAUSED" } else { "active" }.to_string()),
    }));
    checks.push(serde_json::json!({
        "check": "sessions",
        "ok": true,
        "detail": format!("{} active sessions", session_tree.map(|t| t.total_before_filter).unwrap_or(0)),
    }));
    checks.push(serde_json::json!({
        "check": "config",
        "ok": cfg.validate().is_ok(),
        "detail": format!("provider={}, model={}", cfg.default_provider.as_deref().unwrap_or("-"), cfg.default_model.as_deref().unwrap_or("-")),
    }));

    let all_ok = checks.iter().all(|c| c["ok"].as_bool().unwrap_or(false));
    Ok(Json(serde_json::json!({
        "ok": all_ok,
        "checks": checks,
        "penetration_assessment": crate::security::penetration_playbook::build_playbook_payload(),
    })))
}

async fn http_api_cron_list(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    let Some(store) = &runtime.cron_store else {
        return Ok(Json(
            serde_json::json!({ "jobs": [], "note": "cron store not initialized" }),
        ));
    };
    let jobs = store.list();
    let items: Vec<serde_json::Value> = jobs
        .iter()
        .map(|j| {
            serde_json::json!({
                "id": j.id,
                "name": j.name,
                "schedule": j.schedule,
                "command": j.command,
                "enabled": j.enabled,
                "last_run": j.last_run,
                "last_status": j.last_status,
                "next_run": j.next_run,
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "jobs": items })))
}

#[derive(Debug, serde::Deserialize)]
struct ApiCronAddRequest {
    name: String,
    schedule: String,
    command: String,
}

async fn http_api_cron_add(
    State(runtime): State<GatewayRuntime>,
    Json(req): Json<ApiCronAddRequest>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    let Some(store) = &runtime.cron_store else {
        return Err(Json(GatewayError {
            message: "cron store not initialized".to_string(),
        }));
    };
    let job = crate::cron::CronJob {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name,
        schedule: req.schedule,
        command: req.command,
        enabled: true,
        last_run: None,
        last_status: None,
        next_run: None,
        created_at: time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
    };
    let id = job.id.clone();
    store.add(job).await.map_err(|e| {
        Json(GatewayError {
            message: e.to_string(),
        })
    })?;
    Ok(Json(serde_json::json!({ "ok": true, "id": id })))
}

async fn http_metrics(
    State(runtime): State<GatewayRuntime>,
) -> Result<(StatusCode, String), StatusCode> {
    let cfg = runtime.get_config().await;
    if !cfg.observability.prometheus_enabled {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok((StatusCode::OK, crate::observability::encode_metrics()))
}

async fn http_metrics_standalone() -> (StatusCode, String) {
    (StatusCode::OK, crate::observability::encode_metrics())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayError {
    pub message: String,
}

pub fn create_default_tools(config: &Config) -> Vec<Box<dyn Tool>> {
    create_workspace_tools(&config.workspace_dir, config)
}

/// Build all workspace-scoped tools with the given effective workspace root.
pub fn create_workspace_tools(
    effective_workspace: &PathBuf,
    config: &Config,
) -> Vec<Box<dyn Tool>> {
    let workspace = effective_workspace.clone();
    let shell_allowlist = resolve_shell_allowlist(config);
    vec![
        Box::new(FileReadTool::new(workspace.clone())),
        Box::new(FileWriteTool::new(workspace.clone())),
        Box::new(FileEditTool::new(workspace.clone())),
        Box::new(FilePatchTool::new(workspace.clone())),
        Box::new(FileListTool::new(workspace.clone())),
        Box::new(GlobSearchTool::new(workspace.clone())),
        Box::new(ContentSearchTool::new(workspace.clone())),
        Box::new(GitOperationsTool::new(workspace.clone())),
        Box::new(ShellTool::new(
            workspace.clone(),
            shell_allowlist,
            Some(30),
            config.clone(),
        )),
        Box::new(PdfReadTool::new(workspace)),
    ]
}

pub fn create_all_tools(config: &Config, memory: Arc<dyn Memory>) -> Vec<Box<dyn Tool>> {
    let mut tools = create_default_tools(config);

    if config.http_request.enabled {
        tools.push(Box::new(HttpRequestTool::new(
            config.http_request.allowed_domains.clone(),
        )));
    }

    if config.web_fetch.enabled {
        tools.push(Box::new(WebFetchTool::new(
            config.web_fetch.allowed_domains.clone(),
        )));
    }

    if config.web_search.enabled {
        if let Some(key) = &config.web_search.brave_api_key {
            tools.push(Box::new(WebSearchTool::new(key.clone())));
        }
    }

    if config.browser.enabled {
        tools.push(Box::new(BrowserTool::new(
            config.browser.allowed_domains.clone(),
            config.browser.native_headless,
            config.browser.attach_only,
            config.browser.cdp_url.clone(),
        )));
    }

    tools.push(Box::new(MemoryStoreTool::new(memory.clone())));
    tools.push(Box::new(MemoryRecallTool::new(memory)));

    tools
        .into_iter()
        .filter(|tool| is_tool_globally_allowed(config, tool.name()))
        .collect()
}

#[async_trait::async_trait]
impl AgentInvoker for GatewayRuntime {
    /// Run a delegated subtask on another configured agent. The child request
    /// goes through the full `process_inbound` pipeline, so routing, security,
    /// audit, lineage tracking and concurrency limits all apply unchanged.
    async fn invoke_agent(&self, request: DelegateRequest) -> anyhow::Result<String> {
        let cfg = self.config.read().await.clone();
        if !cfg.agents.contains_key(&request.agent) {
            anyhow::bail!("delegate target '{}' is not configured", request.agent);
        }

        let child_session_id = format!("subagent-{}", uuid::Uuid::new_v4());
        let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();
        metadata.insert("agent".into(), serde_json::json!(request.agent));
        metadata.insert("spawn_depth".into(), serde_json::json!(request.child_depth));
        // Parent lineage requires a registered parent session; sessionless
        // parents still propagate depth so spawn limits hold.
        if let Some(parent_session_id) = &request.parent_session_id {
            metadata.insert(
                "parent_session_id".into(),
                serde_json::json!(parent_session_id),
            );
            metadata.insert(
                "parent_agent_id".into(),
                serde_json::json!(request.parent_agent),
            );
        }

        let text = match &request.context {
            Some(context) => format!(
                "{}\n\n[Context from delegating agent '{}']\n{}",
                request.task, request.parent_agent, context
            ),
            None => request.task.clone(),
        };
        let inbound = InboundMessage {
            channel: request.channel.clone(),
            user_id: None,
            session_id: Some(child_session_id),
            text,
            metadata,
        };

        let timeout_secs = cfg
            .agent_defaults_extended
            .subagents
            .as_ref()
            .and_then(|s| s.run_timeout_seconds)
            .filter(|secs| *secs > 0);
        let response = match timeout_secs {
            Some(secs) => tokio::time::timeout(
                std::time::Duration::from_secs(secs as u64),
                self.process_inbound(&inbound),
            )
            .await
            .map_err(|_| {
                anyhow::anyhow!("subagent '{}' timed out after {}s", request.agent, secs)
            })??,
            None => self.process_inbound(&inbound).await?,
        };
        Ok(response.reply)
    }
}

/// Attach the `delegate` tool when the current agent is allowed to spawn
/// subagents. Returns whether the tool was attached.
fn attach_delegate_tool(
    cfg: &Config,
    runtime: &GatewayRuntime,
    route_agent_name: &str,
    parent_session_id: Option<&str>,
    channel: &ChannelKind,
    parent_depth: u32,
    tools: &mut Vec<Box<dyn Tool>>,
) -> bool {
    let mut targets: Vec<String> = cfg
        .agents
        .keys()
        .filter(|name| name.as_str() != route_agent_name)
        .cloned()
        .collect();
    if targets.is_empty() {
        return false;
    }
    if !is_tool_globally_allowed(cfg, "delegate") {
        return false;
    }
    // Delegate agents with an explicit tool allowlist must opt in.
    if let Some(delegate_cfg) = cfg.agents.get(route_agent_name) {
        if !delegate_cfg.allowed_tools.is_empty()
            && !delegate_cfg
                .allowed_tools
                .iter()
                .any(|t| t.eq_ignore_ascii_case("delegate"))
        {
            return false;
        }
    }
    let child_depth = parent_depth.saturating_add(1);
    if let Some(max_depth) = cfg
        .agent_defaults_extended
        .subagents
        .as_ref()
        .and_then(|s| s.max_spawn_depth)
    {
        if child_depth > max_depth {
            return false;
        }
    }
    targets.sort();
    tools.push(Box::new(DelegateTool::new(
        Arc::new(runtime.clone()),
        targets,
        route_agent_name.to_string(),
        parent_session_id.map(ToString::to_string),
        channel.clone(),
        child_depth,
    )));
    true
}

fn create_tools_for_route(
    config: &Config,
    route_agent_name: &str,
    _memory: Arc<dyn Memory>,
    effective_workspace: &PathBuf,
) -> Vec<Box<dyn Tool>> {
    let tools = create_workspace_tools(effective_workspace, config);
    let Some(delegate) = config.agents.get(route_agent_name) else {
        return tools;
    };
    if delegate.allowed_tools.is_empty() {
        return tools;
    }
    let allowed: HashSet<&str> = delegate.allowed_tools.iter().map(String::as_str).collect();
    tools
        .into_iter()
        .filter(|tool| allowed.contains(tool.name()))
        .collect()
}

fn resolve_agent_max_tool_iterations(config: &Config, route_agent_name: &str) -> usize {
    config
        .agents
        .get(route_agent_name)
        .and_then(|delegate| delegate.max_iterations)
        .unwrap_or(config.agent.max_tool_iterations)
}

#[cfg(test)]
mod tests {
    use super::{
        acquire_inbound_slot, acquire_subagent_guard, attach_delegate_tool, create_tools_for_route,
        check_dingtalk_public_route, check_gateway_public_health,
        classify_dingtalk_route_response,
        dingtalk_diagnostic_config_state, feishu_diagnostic_config_state,
        feishu_public_callback_urls,
        normalize_gateway_public_config, normalize_named_tunnel_hostname,
        normalize_public_webhook_base_url, resolve_agent_max_tool_iterations,
        resolve_public_webhook_base_url, split_session_key, DedupCache, GatewayRuntime,
        GatewayRuntimeStatus, GatewaySessionTreeQuery, MonitorFlightGuard,
        SessionLineageMeta,
    };
    use crate::channels::{ChannelKind, InboundMessage};
    use crate::config::{
        ChannelEntry, Config, DelegateAgentConfig, GatewayPublicConfig, GatewayPublicMode,
    };
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use serde_json::json;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[test]
    fn delegate_allowed_tools_filter_default_toolset() {
        let mut config = Config::default();
        config.agents.insert(
            "researcher".to_string(),
            DelegateAgentConfig {
                allowed_tools: vec!["file_read".to_string(), "shell".to_string()],
                ..DelegateAgentConfig::default()
            },
        );
        let effective_ws = PathBuf::from("/fake/workspace");

        let memory: Arc<dyn crate::memory::Memory> = Arc::new(crate::InMemoryMemory::new());
        let tools = create_tools_for_route(&config, "researcher", memory, &effective_ws);
        let names = tools.iter().map(|tool| tool.name()).collect::<Vec<_>>();
        assert_eq!(names, vec!["file_read", "shell"]);
    }

    #[test]
    fn attach_delegate_tool_requires_other_agents() {
        let mut config = Config::default();
        let runtime = GatewayRuntime::new(config.clone());
        let mut tools: Vec<Box<dyn crate::tools::Tool>> = Vec::new();

        // No configured agents -> not attached.
        assert!(!attach_delegate_tool(
            &config,
            &runtime,
            "omninova",
            None,
            &ChannelKind::Cli,
            0,
            &mut tools
        ));

        // One other agent -> attached.
        config
            .agents
            .insert("researcher".to_string(), DelegateAgentConfig::default());
        assert!(attach_delegate_tool(
            &config,
            &runtime,
            "omninova",
            None,
            &ChannelKind::Cli,
            0,
            &mut tools
        ));
        assert_eq!(tools.last().map(|t| t.name()), Some("delegate"));

        // The only target equals the current agent -> not attached.
        let mut tools2: Vec<Box<dyn crate::tools::Tool>> = Vec::new();
        assert!(!attach_delegate_tool(
            &config,
            &runtime,
            "researcher",
            None,
            &ChannelKind::Cli,
            0,
            &mut tools2
        ));
    }

    #[test]
    fn attach_delegate_tool_respects_depth_and_allowlist() {
        let mut config = Config::default();
        config
            .agents
            .insert("researcher".to_string(), DelegateAgentConfig::default());
        config.agents.insert(
            "writer".to_string(),
            DelegateAgentConfig {
                allowed_tools: vec!["file_read".to_string()],
                ..DelegateAgentConfig::default()
            },
        );
        config.agent_defaults_extended.subagents = Some(crate::config::schema::SubagentsConfig {
            max_spawn_depth: Some(1),
            ..crate::config::schema::SubagentsConfig::default()
        });
        let runtime = GatewayRuntime::new(config.clone());

        // Depth at limit -> child depth would exceed -> not attached.
        let mut tools: Vec<Box<dyn crate::tools::Tool>> = Vec::new();
        assert!(!attach_delegate_tool(
            &config,
            &runtime,
            "omninova",
            None,
            &ChannelKind::Cli,
            1,
            &mut tools
        ));

        // Within depth -> attached.
        assert!(attach_delegate_tool(
            &config,
            &runtime,
            "omninova",
            None,
            &ChannelKind::Cli,
            0,
            &mut tools
        ));

        // Agent with allowlist not containing "delegate" -> not attached.
        let mut tools2: Vec<Box<dyn crate::tools::Tool>> = Vec::new();
        assert!(!attach_delegate_tool(
            &config,
            &runtime,
            "writer",
            None,
            &ChannelKind::Cli,
            0,
            &mut tools2
        ));
    }

    #[test]
    fn delegate_max_iterations_overrides_agent_default() {
        let mut config = Config::default();
        config.agent.max_tool_iterations = 20;
        config.agents.insert(
            "researcher".to_string(),
            DelegateAgentConfig {
                max_iterations: Some(4),
                ..DelegateAgentConfig::default()
            },
        );

        assert_eq!(resolve_agent_max_tool_iterations(&config, "researcher"), 4);
        assert_eq!(resolve_agent_max_tool_iterations(&config, "omninova"), 20);
    }

    #[test]
    fn acquire_inbound_slot_enforces_limit() {
        let mut config = Config::default();
        config.agent_defaults_extended.max_concurrent = Some(1);
        let active = Arc::new(AtomicUsize::new(0));

        let first = acquire_inbound_slot(&config, &active).expect("first slot should succeed");
        assert!(first.is_some());

        let second = acquire_inbound_slot(&config, &active);
        assert!(second.is_err());

        drop(first);
        let third = acquire_inbound_slot(&config, &active).expect("slot should be released");
        assert!(third.is_some());
    }

    #[test]
    fn acquire_inbound_slot_uses_subagent_limit_fallback() {
        let mut config = Config::default();
        config.agent_defaults_extended.max_concurrent = None;
        config.agent_defaults_extended.subagents = Some(crate::config::schema::SubagentsConfig {
            max_concurrent: Some(1),
            ..crate::config::schema::SubagentsConfig::default()
        });
        let active = Arc::new(AtomicUsize::new(0));
        let first = acquire_inbound_slot(&config, &active).expect("first slot should succeed");
        assert!(first.is_some());
        let second = acquire_inbound_slot(&config, &active);
        assert!(second.is_err());
    }

    #[tokio::test]
    async fn subagent_guard_rejects_depth_over_limit() {
        let mut config = Config::default();
        config.agent_defaults_extended.subagents = Some(crate::config::schema::SubagentsConfig {
            max_spawn_depth: Some(2),
            ..crate::config::schema::SubagentsConfig::default()
        });
        let mut metadata = HashMap::new();
        metadata.insert("spawnDepth".to_string(), json!(3));
        let inbound = InboundMessage {
            channel: ChannelKind::Cli,
            text: "spawn".to_string(),
            metadata,
            ..InboundMessage::default()
        };
        let map = Arc::new(RwLock::new(HashMap::new()));
        let result = acquire_subagent_guard(&config, &inbound, &map).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn subagent_guard_enforces_children_per_parent() {
        let mut config = Config::default();
        config.agent_defaults_extended.subagents = Some(crate::config::schema::SubagentsConfig {
            max_children_per_agent: Some(1),
            ..crate::config::schema::SubagentsConfig::default()
        });
        let mut metadata = HashMap::new();
        metadata.insert("parentAgentId".to_string(), json!("main"));
        let inbound = InboundMessage {
            channel: ChannelKind::Cli,
            text: "spawn".to_string(),
            metadata,
            ..InboundMessage::default()
        };
        let map = Arc::new(RwLock::new(HashMap::new()));

        let first = acquire_subagent_guard(&config, &inbound, &map)
            .await
            .expect("first child should pass");
        assert!(first.is_some());

        let second = acquire_subagent_guard(&config, &inbound, &map).await;
        assert!(second.is_err());
    }

    fn temp_workspace() -> PathBuf {
        std::env::temp_dir().join(format!("omninova-test-{}", uuid::Uuid::new_v4()))
    }

    #[tokio::test]
    async fn session_lineage_registers_root_session() {
        let mut config = Config::default();
        config.workspace_dir = temp_workspace();
        let runtime = GatewayRuntime::new(config.clone());
        let inbound = InboundMessage {
            channel: ChannelKind::Cli,
            session_id: Some("root-1".to_string()),
            text: "root".to_string(),
            ..InboundMessage::default()
        };
        let meta = runtime
            .validate_and_resolve_session_lineage(&config, &inbound, "omninova")
            .await
            .expect("root session should register");
        assert_eq!(meta.spawn_depth, 0);
        assert!(meta.parent_session_key.is_none());
    }

    #[tokio::test]
    async fn session_lineage_validates_parent_child_depth() {
        let mut config = Config::default();
        config.workspace_dir = temp_workspace();
        let runtime = GatewayRuntime::new(config.clone());

        let root = InboundMessage {
            channel: ChannelKind::Cli,
            session_id: Some("parent".to_string()),
            text: "root".to_string(),
            ..InboundMessage::default()
        };
        runtime
            .validate_and_resolve_session_lineage(&config, &root, "omninova")
            .await
            .expect("root session should register");

        let mut child_meta = HashMap::new();
        child_meta.insert("parentSessionId".to_string(), json!("parent"));
        child_meta.insert("spawnDepth".to_string(), json!(1));
        let child = InboundMessage {
            channel: ChannelKind::Cli,
            session_id: Some("child".to_string()),
            text: "child".to_string(),
            metadata: child_meta,
            ..InboundMessage::default()
        };
        runtime
            .validate_and_resolve_session_lineage(&config, &child, "delegate")
            .await
            .expect("child depth should match parent");

        let mut bad_meta = HashMap::new();
        bad_meta.insert("parentSessionId".to_string(), json!("parent"));
        bad_meta.insert("spawnDepth".to_string(), json!(3));
        let bad_child = InboundMessage {
            channel: ChannelKind::Cli,
            session_id: Some("child-bad".to_string()),
            text: "child".to_string(),
            metadata: bad_meta,
            ..InboundMessage::default()
        };
        let result = runtime
            .validate_and_resolve_session_lineage(&config, &bad_child, "delegate")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn session_lineage_validates_parent_agent_binding() {
        let mut config = Config::default();
        config.workspace_dir = temp_workspace();
        let runtime = GatewayRuntime::new(config.clone());

        let root = InboundMessage {
            channel: ChannelKind::Cli,
            session_id: Some("parent-agent".to_string()),
            text: "root".to_string(),
            ..InboundMessage::default()
        };
        runtime
            .validate_and_resolve_session_lineage(&config, &root, "omninova")
            .await
            .expect("root session should register");

        let mut child_meta = HashMap::new();
        child_meta.insert("parentSessionId".to_string(), json!("parent-agent"));
        child_meta.insert("parentAgentId".to_string(), json!("wrong-agent"));
        child_meta.insert("spawnDepth".to_string(), json!(1));
        let child = InboundMessage {
            channel: ChannelKind::Cli,
            session_id: Some("child-agent-check".to_string()),
            text: "child".to_string(),
            metadata: child_meta,
            ..InboundMessage::default()
        };
        let result = runtime
            .validate_and_resolve_session_lineage(&config, &child, "delegate")
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn split_session_key_parses_channel_and_session() {
        let (channel, session_id) = split_session_key("cli:abc-123");
        assert_eq!(channel.as_deref(), Some("cli"));
        assert_eq!(session_id.as_deref(), Some("abc-123"));
    }

    #[tokio::test]
    async fn session_tree_snapshot_exposes_in_memory_nodes() {
        let mut config = Config::default();
        config.workspace_dir = temp_workspace();
        let runtime = GatewayRuntime::new(config.clone());
        {
            let mut lock = runtime.session_tree.write().await;
            lock.insert(
                "cli:debug-session".to_string(),
                SessionLineageMeta {
                    parent_session_key: Some("cli:parent".to_string()),
                    parent_agent_id: Some("omninova".to_string()),
                    agent_name: Some("delegate".to_string()),
                    spawn_depth: 1,
                    updated_at: super::now_unix_ts(),
                },
            );
        }
        let snapshot = runtime
            .session_tree_snapshot()
            .await
            .expect("snapshot should load");
        assert_eq!(snapshot.total_before_filter, 1);
        assert_eq!(snapshot.total_after_filter, 1);
        assert_eq!(snapshot.returned, 1);
        assert!(!snapshot.has_more);
        assert_eq!(snapshot.next_offset, None);
        assert_eq!(
            snapshot.source_counts_after_filter.get("memory"),
            Some(&1usize)
        );
        assert_eq!(snapshot.stats_after_filter.unique_agents, 1);
        assert_eq!(snapshot.stats_after_filter.unique_parent_agents, 1);
        assert_eq!(snapshot.stats_after_filter.max_spawn_depth, 1);
        assert!(snapshot
            .sessions
            .iter()
            .any(
                |entry| entry.session_key.as_deref() == Some("cli:debug-session")
                    && entry.parent_agent_id.as_deref() == Some("omninova")
            ));
    }

    #[tokio::test]
    async fn session_tree_snapshot_supports_query_filters() {
        let mut config = Config::default();
        config.workspace_dir = temp_workspace();
        let runtime = GatewayRuntime::new(config.clone());
        {
            let mut lock = runtime.session_tree.write().await;
            lock.insert(
                "cli:keep-me".to_string(),
                SessionLineageMeta {
                    parent_session_key: None,
                    parent_agent_id: Some("omninova".to_string()),
                    agent_name: Some("delegate-a".to_string()),
                    spawn_depth: 0,
                    updated_at: super::now_unix_ts(),
                },
            );
            lock.insert(
                "cli:drop-me".to_string(),
                SessionLineageMeta {
                    parent_session_key: None,
                    parent_agent_id: Some("omninova".to_string()),
                    agent_name: Some("delegate-b".to_string()),
                    spawn_depth: 0,
                    updated_at: super::now_unix_ts(),
                },
            );
        }

        let filtered = runtime
            .session_tree_snapshot_filtered(&GatewaySessionTreeQuery {
                session_id: Some("keep-me".to_string()),
                agent_name: Some("delegate-a".to_string()),
                channel: Some("cli".to_string()),
                source: Some("memory".to_string()),
                limit: Some(1),
                ..GatewaySessionTreeQuery::default()
            })
            .await
            .expect("filtered snapshot should load");

        assert_eq!(filtered.sessions.len(), 1);
        assert_eq!(filtered.total_before_filter, 2);
        assert_eq!(filtered.total_after_filter, 1);
        assert_eq!(filtered.returned, 1);
        assert!(!filtered.has_more);
        assert_eq!(filtered.next_offset, None);
        assert_eq!(
            filtered.source_counts_after_filter.get("memory"),
            Some(&1usize)
        );
        assert_eq!(
            filtered.sessions[0].session_key.as_deref(),
            Some("cli:keep-me")
        );
    }

    #[tokio::test]
    async fn session_tree_snapshot_supports_parent_and_depth_filters() {
        let mut config = Config::default();
        config.workspace_dir = temp_workspace();
        let runtime = GatewayRuntime::new(config.clone());
        {
            let mut lock = runtime.session_tree.write().await;
            lock.insert(
                "cli:parent-x".to_string(),
                SessionLineageMeta {
                    parent_session_key: None,
                    parent_agent_id: None,
                    agent_name: Some("OmniNova".to_string()),
                    spawn_depth: 0,
                    updated_at: super::now_unix_ts(),
                },
            );
            lock.insert(
                "cli:child-x-1".to_string(),
                SessionLineageMeta {
                    parent_session_key: Some("cli:parent-x".to_string()),
                    parent_agent_id: Some("OmniNova".to_string()),
                    agent_name: Some("Delegate-X".to_string()),
                    spawn_depth: 1,
                    updated_at: super::now_unix_ts(),
                },
            );
            lock.insert(
                "cli:child-x-2".to_string(),
                SessionLineageMeta {
                    parent_session_key: Some("cli:parent-x".to_string()),
                    parent_agent_id: Some("OmniNova".to_string()),
                    agent_name: Some("Delegate-Y".to_string()),
                    spawn_depth: 2,
                    updated_at: super::now_unix_ts(),
                },
            );
        }

        let filtered = runtime
            .session_tree_snapshot_filtered(&GatewaySessionTreeQuery {
                parent_session_id: Some("PARENT-X".to_string()),
                parent_agent_id: Some("omninova".to_string()),
                min_spawn_depth: Some(1),
                max_spawn_depth: Some(1),
                source: Some("MEMORY".to_string()),
                case_insensitive: Some(true),
                ..GatewaySessionTreeQuery::default()
            })
            .await
            .expect("filtered snapshot should load");

        assert_eq!(filtered.sessions.len(), 1);
        assert_eq!(
            filtered.sessions[0].session_key.as_deref(),
            Some("cli:child-x-1")
        );
    }

    #[tokio::test]
    async fn session_tree_snapshot_supports_sort_and_offset() {
        let mut config = Config::default();
        config.workspace_dir = temp_workspace();
        let runtime = GatewayRuntime::new(config.clone());
        {
            let mut lock = runtime.session_tree.write().await;
            lock.insert(
                "cli:s1".to_string(),
                SessionLineageMeta {
                    parent_session_key: None,
                    parent_agent_id: None,
                    agent_name: Some("B-Agent".to_string()),
                    spawn_depth: 2,
                    updated_at: super::now_unix_ts(),
                },
            );
            lock.insert(
                "cli:s2".to_string(),
                SessionLineageMeta {
                    parent_session_key: None,
                    parent_agent_id: None,
                    agent_name: Some("A-Agent".to_string()),
                    spawn_depth: 1,
                    updated_at: super::now_unix_ts(),
                },
            );
            lock.insert(
                "cli:s3".to_string(),
                SessionLineageMeta {
                    parent_session_key: None,
                    parent_agent_id: None,
                    agent_name: Some("C-Agent".to_string()),
                    spawn_depth: 3,
                    updated_at: super::now_unix_ts(),
                },
            );
        }

        let filtered = runtime
            .session_tree_snapshot_filtered(&GatewaySessionTreeQuery {
                sort_by: Some("spawn_depth".to_string()),
                sort_order: Some("asc".to_string()),
                offset: Some(1),
                limit: Some(1),
                ..GatewaySessionTreeQuery::default()
            })
            .await
            .expect("filtered snapshot should load");

        assert_eq!(filtered.total_before_filter, 3);
        assert_eq!(filtered.total_after_filter, 3);
        assert_eq!(filtered.offset, 1);
        assert_eq!(filtered.limit, Some(1));
        assert_eq!(filtered.returned, 1);
        assert!(filtered.has_more);
        assert_eq!(filtered.next_offset, Some(2));
        assert_eq!(filtered.prev_offset, Some(0));
        assert_eq!(filtered.next_cursor, Some(2));
        assert_eq!(filtered.prev_cursor, Some(0));
        assert_eq!(
            filtered.source_counts_after_filter.get("memory"),
            Some(&3usize)
        );
        assert_eq!(filtered.stats_after_filter.unique_agents, 3);
        assert_eq!(filtered.stats_after_filter.unique_parent_agents, 0);
        assert_eq!(filtered.stats_after_filter.max_spawn_depth, 3);
        assert_eq!(filtered.sessions[0].spawn_depth, 2);
        assert_eq!(filtered.sessions[0].session_key.as_deref(), Some("cli:s1"));
    }

    #[tokio::test]
    async fn session_tree_snapshot_supports_cursor_as_offset_alias() {
        let mut config = Config::default();
        config.workspace_dir = temp_workspace();
        let runtime = GatewayRuntime::new(config.clone());
        {
            let mut lock = runtime.session_tree.write().await;
            lock.insert(
                "cli:c1".to_string(),
                SessionLineageMeta {
                    parent_session_key: None,
                    parent_agent_id: None,
                    agent_name: Some("A".to_string()),
                    spawn_depth: 1,
                    updated_at: super::now_unix_ts(),
                },
            );
            lock.insert(
                "cli:c2".to_string(),
                SessionLineageMeta {
                    parent_session_key: None,
                    parent_agent_id: None,
                    agent_name: Some("B".to_string()),
                    spawn_depth: 2,
                    updated_at: super::now_unix_ts(),
                },
            );
        }

        let filtered = runtime
            .session_tree_snapshot_filtered(&GatewaySessionTreeQuery {
                sort_by: Some("spawn_depth".to_string()),
                sort_order: Some("asc".to_string()),
                cursor: Some(1),
                limit: Some(1),
                ..GatewaySessionTreeQuery::default()
            })
            .await
            .expect("cursor paging should work");

        assert_eq!(filtered.offset, 1);
        assert_eq!(filtered.sessions.len(), 1);
        assert_eq!(filtered.sessions[0].session_key.as_deref(), Some("cli:c2"));
    }

    // =============================================================================
    // Channel enabled security tests
    // =============================================================================

    fn make_config_with_channel(channel: ChannelKind, enabled: bool) -> Config {
        let mut config = Config::default();
        let entry = crate::config::schema::ChannelEntry {
            enabled,
            token: None,
            token_env: None,
            security_mode: None,
            verification_token: None,
            verification_token_env: None,
            encrypt_key: None,
            encrypt_key_env: None,
            extra: HashMap::new(),
        };
        match channel {
            ChannelKind::Feishu => config.channels_config.feishu = Some(entry),
            ChannelKind::Lark => config.channels_config.lark = Some(entry),
            ChannelKind::Telegram => config.channels_config.telegram = Some(entry),
            _ => {}
        }
        config
    }

    fn feishu_security_config(
        mode: &str,
        verification_token: Option<&str>,
        encrypt_key: Option<&str>,
    ) -> Config {
        let mut config = outbound_test_config(ChannelKind::Feishu, "mock", false);
        let entry = config
            .channels_config
            .feishu
            .as_mut()
            .expect("feishu config should exist");
        entry.security_mode = Some(mode.to_string());
        entry.verification_token = verification_token.map(ToString::to_string);
        entry.encrypt_key = encrypt_key.map(ToString::to_string);
        config
    }

    fn encrypt_feishu_payload_for_test(plaintext: &str, encrypt_key: &str) -> String {
        use aes::cipher::{Block, BlockEncrypt, KeyInit};
        use aes::Aes256;
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
        use sha2::{Digest, Sha256};

        let key_hash = Sha256::digest(encrypt_key.as_bytes());
        let cipher = Aes256::new_from_slice(&key_hash).expect("test AES key");
        let iv = [0x5Au8; 16];
        let padding = 16 - (plaintext.len() % 16);
        let mut bytes = plaintext.as_bytes().to_vec();
        bytes.extend(std::iter::repeat_n(padding as u8, padding));

        let mut previous = iv;
        let mut ciphertext = Vec::with_capacity(bytes.len());
        for chunk in bytes.chunks_exact(16) {
            let mut block_bytes = [0u8; 16];
            for (index, value) in chunk.iter().enumerate() {
                block_bytes[index] = *value ^ previous[index];
            }
            let mut block = Block::<Aes256>::from(block_bytes);
            cipher.encrypt_block(&mut block);
            previous.copy_from_slice(&block);
            ciphertext.extend_from_slice(&block);
        }
        let mut combined = iv.to_vec();
        combined.extend_from_slice(&ciphertext);
        BASE64.encode(combined)
    }

    async fn call_feishu_http(
        runtime: GatewayRuntime,
        body: String,
    ) -> (StatusCode, String, serde_json::Value) {
        let response = super::http_feishu_webhook(
            axum::extract::State(runtime),
            HeaderMap::new(),
            body,
        )
        .await
        .into_response();
        let status = response.status();
        let content_type = response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("Feishu response body");
        let value = serde_json::from_slice(&bytes).expect("Feishu response must be JSON");
        (status, content_type, value)
    }

    async fn call_feishu_card_http(
        runtime: GatewayRuntime,
        body: String,
    ) -> (StatusCode, serde_json::Value) {
        let response = super::http_feishu_card_callback(
            axum::extract::State(runtime),
            HeaderMap::new(),
            body,
        )
        .await
        .into_response();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("Feishu card response body");
        let value = serde_json::from_slice(&bytes).expect("Feishu card response must be JSON");
        (status, value)
    }

    #[tokio::test]
    async fn feishu_card_url_verification_returns_challenge() {
        let challenge = "card-verification-challenge";
        let body = json!({
            "type": "url_verification",
            "token": "card-verification-token",
            "challenge": challenge
        })
        .to_string();

        let (status, response) = call_feishu_card_http(
            GatewayRuntime::new(feishu_security_config("dev", None, None)),
            body,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            response.get("challenge").and_then(serde_json::Value::as_str),
            Some(challenge)
        );
        assert!(response.get("toast").is_none());
    }

    #[tokio::test]
    async fn feishu_card_challenge_does_not_require_action() {
        let token = "configured-card-token";
        let body = json!({
            "type": "url_verification",
            "token": token,
            "challenge": "challenge-without-action"
        })
        .to_string();

        let (status, response) = call_feishu_card_http(
            GatewayRuntime::new(feishu_security_config("token", Some(token), None)),
            body,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            response.get("challenge").and_then(serde_json::Value::as_str),
            Some("challenge-without-action")
        );
        assert!(response.get("ok").is_none());
        assert!(response.get("toast").is_none());
    }

    #[test]
    fn feishu_security_dev_mode_is_explicitly_insecure() {
        let config = feishu_security_config("dev", None, None);
        let security = super::FeishuSecurityConfig::from_entry(config.channels_config.feishu.as_ref());
        assert!(security.insecure);
        assert_eq!(security.mode.as_str(), "dev");
    }

    #[test]
    fn feishu_token_extractor_supports_payload_and_header_locations() {
        let headers = HeaderMap::new();
        let cases = [
            (
                json!({ "token": "top-token" }),
                "top-token",
                "top_level.token",
            ),
            (
                json!({ "header": { "token": "header-token" } }),
                "header-token",
                "header.token",
            ),
            (
                json!({ "event": { "token": "event-token" } }),
                "event-token",
                "event.token",
            ),
            (
                json!({ "event": { "header": { "token": "event-header-token" } } }),
                "event-header-token",
                "event.header.token",
            ),
        ];

        for (payload, expected_value, expected_source) in cases {
            let extracted = super::extract_feishu_verification_token(&payload, &headers)
                .expect("token should be extracted");
            assert_eq!(extracted.value, expected_value);
            assert_eq!(extracted.source, expected_source);
        }

        let mut feishu_header = HeaderMap::new();
        feishu_header.insert(
            "x-feishu-verification-token",
            "http-header-token".parse().expect("header value"),
        );
        let empty_payload = json!({});
        let extracted =
            super::extract_feishu_verification_token(&empty_payload, &feishu_header)
                .expect("HTTP token header should be extracted");
        assert_eq!(extracted.value, "http-header-token");
        assert_eq!(
            extracted.source,
            "header.x-feishu-verification-token"
        );
    }

    #[test]
    fn feishu_card_action_extractor_supports_unified_and_legacy_shapes() {
        let cases = [
            (
                json!({"event":{"action":{"value":{"action":"monitor_30s"}}}}),
                "monitor_30s",
                "event.action.value.action",
            ),
            (
                json!({"event":{"action":{"value":{"key":"gateway_status"}}}}),
                "gateway_status",
                "event.action.value.key",
            ),
            (
                json!({"event":{"action":{"value":"recent_jobs"}}}),
                "recent_jobs",
                "event.action.value",
            ),
            (
                json!({"action":{"value":{"action":"help"}}}),
                "help",
                "action.value.action",
            ),
        ];
        for (payload, expected_action, expected_source) in cases {
            let (action, source) =
                super::extract_feishu_card_action(&payload).expect("card action");
            assert_eq!(action, expected_action);
            assert_eq!(source, expected_source);
        }
    }

    #[test]
    fn feishu_card_context_extracts_real_unified_callback_paths() {
        let payload = json!({
            "event": {
                "context": {
                    "open_chat_id": "oc_private_value",
                    "open_message_id": "om_private_value"
                },
                "operator": { "open_id": "ou_must_not_be_used_as_chat_id" }
            }
        });
        let context = super::extract_feishu_card_reply_context(&payload);
        assert_eq!(context.chat_id.as_deref(), Some("oc_private_value"));
        assert_eq!(
            context.chat_id_source,
            Some("event.context.open_chat_id")
        );
        assert_eq!(
            context.open_message_id.as_deref(),
            Some("om_private_value")
        );
        assert_eq!(
            context.open_message_id_source,
            Some("event.context.open_message_id")
        );
    }

    fn unified_card_payload(token: Option<&str>, action: &str, with_chat: bool) -> String {
        let mut event = json!({
            "action": { "value": { "action": action } },
            "operator": { "open_id": "ou_private" }
        });
        if with_chat {
            event["context"] = json!({
                "open_chat_id": "oc_private",
                "open_message_id": "om_private"
            });
        }
        json!({
            "schema": "2.0",
            "header": {
                "event_id": format!("evt_{}", uuid::Uuid::new_v4()),
                "event_type": "card.action.trigger",
                "token": token
            },
            "event": event
        })
        .to_string()
    }

    #[test]
    fn monitor_singleflight_key_hashes_chat_id_and_normalizes_feishu_prefix() {
        let raw = "oc_monitor_private_value";
        let raw_key = super::monitor_flight_key(raw);
        let prefixed_key = super::monitor_flight_key(&format!("feishu:{raw}"));
        assert_eq!(raw_key, prefixed_key);
        assert!(raw_key.starts_with("feishu:h"));
        assert!(raw_key.ends_with(":desktop_monitor"));
        assert!(!raw_key.contains(raw));
    }

    #[test]
    fn monitor_singleflight_ttl_includes_safety_margin() {
        assert_eq!(super::monitor_guard_ttl_secs(30), 45);
        assert_eq!(super::monitor_guard_ttl_secs(60), 75);
        assert!(super::monitor_guard_ttl_secs(30) >= 40);
        assert!(super::monitor_guard_ttl_secs(60) >= 70);
    }

    #[tokio::test]
    async fn monitor_singleflight_acquire_is_atomic() {
        let guard = MonitorFlightGuard::new();
        let (first, second) = tokio::join!(
            guard.try_acquire_with_ttl("oc_atomic", 45),
            guard.try_acquire_with_ttl("feishu:oc_atomic", 45)
        );
        assert_eq!(
            usize::from(first.is_some()) + usize::from(second.is_some()),
            1,
            "exactly one concurrent acquire may succeed"
        );
    }

    #[tokio::test]
    async fn monitor_singleflight_release_is_owner_scoped() {
        let guard = MonitorFlightGuard::new();
        let old_lease = guard
            .try_acquire_with_ttl("oc_owner", 45)
            .await
            .expect("first lease");
        guard.clear().await;
        let new_lease = guard
            .try_acquire_with_ttl("oc_owner", 45)
            .await
            .expect("replacement lease");

        assert!(!guard.release(&old_lease).await);
        assert!(
            guard
                .try_acquire_with_ttl("oc_owner", 45)
                .await
                .is_none(),
            "stale lease must not release the current owner"
        );
        assert!(guard.release(&new_lease).await);
        assert!(
            guard
                .try_acquire_with_ttl("oc_owner", 45)
                .await
                .is_some(),
            "release must allow a new monitor"
        );
    }

    #[tokio::test]
    async fn monitor_singleflight_expired_lease_allows_reacquire() {
        let guard = MonitorFlightGuard::new();
        let expired = guard
            .try_acquire_with_ttl("oc_expired", 45)
            .await
            .expect("initial lease");
        {
            let mut entries = guard.inner.write().await;
            let expired_at =
                std::time::Instant::now() - std::time::Duration::from_secs(1);
            entries
                .get_mut(&expired.key)
                .expect("active entry")
                .expires_at = expired_at;
        }
        let replacement = guard
            .try_acquire_with_ttl("oc_expired", 45)
            .await
            .expect("expired lease must not block a replacement");

        assert!(!guard.release(&expired).await);
        assert!(guard.release(&replacement).await);
    }

    #[tokio::test]
    async fn rejected_card_security_never_acquires_monitor_guard() {
        let runtime = GatewayRuntime::new(feishu_security_config(
            "token",
            Some("expected-verification-token"),
            None,
        ));

        let (missing_status, _, _) = call_feishu_http(
            runtime.clone(),
            unified_card_payload(None, "monitor_30s", true),
        )
        .await;
        assert_ne!(missing_status, StatusCode::OK);
        assert_eq!(runtime.monitor_flight_guard().active_count().await, 0);

        let (mismatch_status, _, _) = call_feishu_http(
            runtime.clone(),
            unified_card_payload(Some("wrong-token"), "monitor_30s", true),
        )
        .await;
        assert_ne!(mismatch_status, StatusCode::OK);
        assert_eq!(runtime.monitor_flight_guard().active_count().await, 0);
    }

    #[tokio::test]
    async fn unified_feishu_webhook_routes_card_action_before_message_filter() {
        let (status, _, response) = call_feishu_http(
            GatewayRuntime::new(feishu_security_config("dev", None, None)),
            unified_card_payload(None, "gateway_status", true),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            response.get("action").and_then(serde_json::Value::as_str),
            Some("gateway_status")
        );
        assert_eq!(
            response.get("ok").and_then(serde_json::Value::as_bool),
            Some(true)
        );
    }

    #[tokio::test]
    async fn dedicated_feishu_card_endpoint_remains_available() {
        let (status, response) = call_feishu_card_http(
            GatewayRuntime::new(feishu_security_config("dev", None, None)),
            unified_card_payload(None, "help", true),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            response.get("action").and_then(serde_json::Value::as_str),
            Some("help")
        );
    }

    #[tokio::test]
    async fn unified_card_action_enforces_token_before_dispatch() {
        let config = || {
            feishu_security_config("token", Some("test-verification-token"), None)
        };
        let (ok_status, _, ok_response) = call_feishu_http(
            GatewayRuntime::new(config()),
            unified_card_payload(
                Some("test-verification-token"),
                "gateway_status",
                true,
            ),
        )
        .await;
        assert_eq!(ok_status, StatusCode::OK);
        assert_eq!(
            ok_response.get("action").and_then(serde_json::Value::as_str),
            Some("gateway_status")
        );

        let (missing_status, _, missing_response) = call_feishu_http(
            GatewayRuntime::new(config()),
            unified_card_payload(None, "gateway_status", true),
        )
        .await;
        assert_eq!(missing_status, StatusCode::UNAUTHORIZED);
        assert_eq!(
            missing_response
                .get("error")
                .and_then(serde_json::Value::as_str),
            Some("token_missing")
        );

        let (bad_status, _, bad_response) = call_feishu_http(
            GatewayRuntime::new(config()),
            unified_card_payload(Some("wrong-token"), "gateway_status", true),
        )
        .await;
        assert_eq!(bad_status, StatusCode::UNAUTHORIZED);
        assert_eq!(
            bad_response
                .get("error")
                .and_then(serde_json::Value::as_str),
            Some("token_mismatch")
        );
    }

    #[tokio::test]
    async fn unified_card_monitor_queues_direct_monitor_with_context_chat_id() {
        let runtime = GatewayRuntime::new(feishu_security_config("dev", None, None));
        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<crate::gateway::feishu_worker::FeishuAsyncJob>(2);
        runtime.init_feishu_worker(sender).await;

        let (status, _, response) = call_feishu_http(
            runtime,
            unified_card_payload(None, "monitor_30s", true),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            response.get("action").and_then(serde_json::Value::as_str),
            Some("monitor_30s")
        );
        let job = receiver.try_recv().expect("monitor job should be queued");
        assert_eq!(job.inbound.session_id.as_deref(), Some("oc_private"));
        assert_eq!(job.inbound.text, "/monitor 桌面 30秒");
        assert_eq!(
            job.inbound
                .metadata
                .get("message_id")
                .and_then(serde_json::Value::as_str),
            Some("om_private")
        );
    }

    #[tokio::test]
    async fn unified_card_missing_chat_id_acks_without_queue_or_panic() {
        let runtime = GatewayRuntime::new(feishu_security_config("dev", None, None));
        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<crate::gateway::feishu_worker::FeishuAsyncJob>(2);
        runtime.init_feishu_worker(sender).await;
        let (status, _, response) = call_feishu_http(
            runtime,
            unified_card_payload(None, "monitor_30s", false),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            response.get("ok").and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert!(receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn unified_card_unknown_action_is_safely_acked_without_runtime() {
        let (status, _, response) = call_feishu_http(
            GatewayRuntime::new(feishu_security_config("dev", None, None)),
            unified_card_payload(None, "shell_anything", true),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            response.get("ok").and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert!(response.get("action").is_none());
    }

    /// v0.9.1.2: callback response must include ack_ms for observability.
    #[tokio::test]
    async fn unified_card_callback_includes_ack_ms_metric() {
        DedupCache::global().clear().await;
        let (status, _, response) = call_feishu_http(
            GatewayRuntime::new(feishu_security_config("dev", None, None)),
            unified_card_payload(None, "gateway_status", true),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let ack_ms = response.get("ack_ms").and_then(serde_json::Value::as_i64);
        assert!(ack_ms.is_some(), "ack_ms must be present in card callback response");
        // Should be a non-negative measurement
        assert!(ack_ms.unwrap_or(-1) >= 0);
        assert_eq!(
            response.get("action").and_then(serde_json::Value::as_str),
            Some("gateway_status")
        );
    }

    /// v0.9.1.2: unknown action still returns HTTP 200 and does not crash.
    /// It also records ack_ms and keeps `ok=false`/`action` undefined to
    /// match pre-existing semantic for unknown actions.
    #[tokio::test]
    async fn unified_card_unknown_action_includes_ack_ms_and_no_panic() {
        DedupCache::global().clear().await;
        let (status, _, response) = call_feishu_http(
            GatewayRuntime::new(feishu_security_config("dev", None, None)),
            unified_card_payload(None, "delete_database", true),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            response.get("ok").and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert!(response.get("ack_ms").is_some());
        assert!(response.get("action").is_none());
    }

    /// v0.9.1.2: same event_id arriving twice is deduped at the ACK layer —
    /// the second click does NOT enqueue a fresh monitor job.
    #[tokio::test]
    async fn unified_card_duplicate_event_id_is_deduped_without_requeue() {
        DedupCache::global().clear().await;
        let runtime = GatewayRuntime::new(feishu_security_config("dev", None, None));
        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<crate::gateway::feishu_worker::FeishuAsyncJob>(4);
        runtime.init_feishu_worker(sender).await;

        // Build two identical payloads (same event_id) so dedup will trigger on the 2nd.
        let body = format!(
            r#"{{"schema":"2.0","header":{{"event_id":"evt_FIXED_DEDUP","event_type":"card.action.trigger","token":null}},"event":{{"action":{{"value":{{"action":"monitor_30s"}}}},"context":{{"open_chat_id":"oc_dedup","open_message_id":"om_dedup"}}}}}}"#
        );

        let (status1, _, response1) = call_feishu_http(runtime.clone(), body.clone()).await;
        assert_eq!(status1, StatusCode::OK);
        assert_eq!(
            response1.get("ok").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            response1.get("duplicate").and_then(serde_json::Value::as_bool),
            None
        );

        let (status2, _, response2) = call_feishu_http(runtime, body).await;
        assert_eq!(status2, StatusCode::OK);
        assert_eq!(
            response2.get("ok").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            response2.get("duplicate").and_then(serde_json::Value::as_bool),
            Some(true)
        );

        // Only ONE monitor job should have been enqueued.
        let first = receiver.try_recv().expect("first monitor job should be queued");
        assert_eq!(first.inbound.text, "/monitor 桌面 30秒");
        // Second try_recv() should yield nothing.
        assert!(receiver.try_recv().is_err(), "duplicate click must not enqueue a second monitor job");
    }

    /// v0.9.1.2: monitor callback must not block on outbox / SQLite writes.
    /// With no store attached the response still returns 200 immediately.
    #[tokio::test]
    async fn unified_card_monitor_callback_acks_immediately_without_store() {
        DedupCache::global().clear().await;
        let runtime = GatewayRuntime::new(feishu_security_config("dev", None, None));
        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<crate::gateway::feishu_worker::FeishuAsyncJob>(2);
        runtime.init_feishu_worker(sender).await;

        let (status, _, response) = call_feishu_http(
            runtime,
            unified_card_payload(None, "monitor_30s", true),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            response.get("ok").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        let ack_ms = response
            .get("ack_ms")
            .and_then(serde_json::Value::as_i64)
            .expect("ack_ms present");
        // Ack must be fast (well under any realistic SQLite roundtrip).
        // We only assert the metric is recorded.
        let _ = (ack_ms, receiver.try_recv().expect("monitor job queued synchronously"));
    }

    /// v0.9.1.2: missing chat_id still returns HTTP 200 with ack_ms recorded
    /// and does NOT panic / enqueue anything.
    #[tokio::test]
    async fn unified_card_missing_chat_id_records_ack_ms() {
        DedupCache::global().clear().await;
        let runtime = GatewayRuntime::new(feishu_security_config("dev", None, None));
        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<crate::gateway::feishu_worker::FeishuAsyncJob>(2);
        runtime.init_feishu_worker(sender).await;

        let (status, _, response) = call_feishu_http(
            runtime,
            unified_card_payload(None, "monitor_30s", false),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            response.get("ok").and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert!(response.get("ack_ms").is_some(), "ack_ms must be recorded");
        assert!(receiver.try_recv().is_err(), "no job must be enqueued when chat_id is missing");
    }

    /// v0.9.1.2.1: monitor callback with full worker queue must STILL ACK
    /// quickly with busy=true and NOT execute the monitor. To verify, the
    /// queue is filled to capacity, then a monitor click is sent, then we
    /// check the receiver after the response has returned.
    #[tokio::test]
    async fn unified_card_monitor_does_not_block_when_queue_is_full() {
        DedupCache::global().clear().await;
        let runtime = GatewayRuntime::new(feishu_security_config("dev", None, None));
        // Pre-fill the queue with 100 dummy jobs so the next monitor_30s
        // cannot enqueue. (QUEUE_CAPACITY = 100.)
        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<crate::gateway::feishu_worker::FeishuAsyncJob>(100);
        runtime.init_feishu_worker(sender).await;

        for i in 0..100 {
            // Distinct event_keys so the dummy jobs do not collide on dedup.
            let dummy = crate::gateway::feishu_worker::FeishuAsyncJob::new(
                ChannelKind::Feishu,
                InboundMessage {
                    text: format!("dummy {i}"),
                    channel: ChannelKind::Feishu,
                    user_id: None,
                    session_id: Some(format!("oc_dummy_{i}")),
                    metadata: HashMap::new(),
                },
                json!({ "i": i }),
                false,
                format!("dummy_event_{i}"),
                Some(format!("dummy_job_{i}")),
            );
            runtime
                .try_send_feishu_job_nonblocking(dummy)
                .await
                .expect("enqueue helper");
        }

        // Now send the card callback. The monitor queue is full; the
        // callback must still ACK in <50ms with busy=true.
        let body = format!(
            r#"{{"schema":"2.0","header":{{"event_id":"evt_QUEUE_FULL","event_type":"card.action.trigger","token":null}},"event":{{"action":{{"value":{{"action":"monitor_30s"}}}},"context":{{"open_chat_id":"oc_fullqueue","open_message_id":"om_fullqueue"}}}}}}"#
        );
        let (status, _, response) = call_feishu_http(runtime.clone(), body).await;

        assert_eq!(status, StatusCode::OK, "callback must ACK under queue-full");
        assert_eq!(
            response.get("ok").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            response.get("busy").and_then(serde_json::Value::as_bool),
            Some(true),
            "busy must be true when queue is full"
        );
        assert_eq!(
            response.get("queued").and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert!(response.get("ack_ms").is_some(), "ack_ms must always be recorded");

        // The freshly pushed "monitor_30s" must NOT have made it into the
        // queue — drain the dummy jobs and confirm.
        for _ in 0..100 {
            let _ = receiver.try_recv().expect("drained dummy");
        }
        assert!(
            receiver.try_recv().is_err(),
            "no extra monitor job must have been queued under busy=true"
        );

        // Queue rejection must release the lease immediately rather than waiting for TTL.
        let retry_body = format!(
            r#"{{"schema":"2.0","header":{{"event_id":"evt_QUEUE_RETRY","event_type":"card.action.trigger","token":null}},"event":{{"action":{{"value":{{"action":"monitor_30s"}}}},"context":{{"open_chat_id":"oc_fullqueue","open_message_id":"om_fullqueue_retry"}}}}}}"#
        );
        let (retry_status, _, retry_response) =
            call_feishu_http(runtime, retry_body).await;
        assert_eq!(retry_status, StatusCode::OK);
        assert_eq!(
            retry_response
                .get("queued")
                .and_then(serde_json::Value::as_bool),
            Some(true),
            "queue failure must not leave the chat locked"
        );
        let _ = receiver
            .try_recv()
            .expect("monitor may enqueue after queue capacity is restored");
    }

    /// v0.9.1.2.1: tier 2 — when only open_message_id is present (no
    /// event_id), a second click within 8 seconds should be deduped.
    /// After ~9 seconds, a fresh click should be allowed again.
    #[tokio::test]
    async fn unified_card_open_message_id_dedup_uses_short_ttl() {
        DedupCache::global().clear().await;
        let runtime = GatewayRuntime::new(feishu_security_config("dev", None, None));
        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<crate::gateway::feishu_worker::FeishuAsyncJob>(4);
        runtime.init_feishu_worker(sender).await;

        // No header.event_id; only open_message_id is present.
        let body = format!(
            r#"{{"schema":"2.0","header":{{"event_type":"card.action.trigger","token":null}},"event":{{"action":{{"value":{{"action":"gateway_status"}}}},"context":{{"open_chat_id":"oc_short","open_message_id":"om_short_1"}}}}}}"#
        );

        let (s1, _, r1) = call_feishu_http(runtime.clone(), body.clone()).await;
        assert_eq!(s1, StatusCode::OK);
        assert_eq!(
            r1.get("ok").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert!(r1.get("duplicate").is_none());

        // Second click with the SAME open_message_id should dedup.
        let (s2, _, r2) = call_feishu_http(runtime.clone(), body.clone()).await;
        assert_eq!(s2, StatusCode::OK);
        assert_eq!(
            r2.get("duplicate").and_then(serde_json::Value::as_bool),
            Some(true),
            "second click with same open_message_id within 8s must dedup"
        );

        // Wait > tier-2 TTL (8 seconds) plus a margin.
        tokio::time::sleep(std::time::Duration::from_millis(9_000)).await;

        // Third click after the TTL should be allowed.
        let (s3, _, r3) = call_feishu_http(runtime, body).await;
        assert_eq!(s3, StatusCode::OK);
        assert!(
            r3.get("duplicate").is_none(),
            "third click after tier-2 TTL must NOT be treated as duplicate"
        );
        // Allow receiver drain quietly (none of these should have enqueued jobs
        // because action=gateway_status, not monitor_*).
        let _ = receiver.try_recv();
    }

    /// v0.9.1.2.1: tier 3 — when neither event_id nor open_message_id is
    /// present, dedup must use a SHORT (3-second) TTL keyed only by
    /// action+chat. This protects against 30-minute lockouts even when
    /// payloads have no identifiers at all.
    #[tokio::test]
    async fn unified_card_no_id_uses_short_ttl_not_30_minutes() {
        DedupCache::global().clear().await;
        let runtime = GatewayRuntime::new(feishu_security_config("dev", None, None));
        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<crate::gateway::feishu_worker::FeishuAsyncJob>(8);
        runtime.init_feishu_worker(sender).await;

        // No event_id, no open_message_id anywhere in the payload.
        let body = format!(
            r#"{{"schema":"2.0","header":{{"event_type":"card.action.trigger","token":null}},"event":{{"action":{{"value":{{"action":"monitor_30s"}}}},"context":{{"open_chat_id":"oc_notier3"}}}}}}"#
        );

        let (s1, _, r1) = call_feishu_http(runtime.clone(), body.clone()).await;
        assert_eq!(s1, StatusCode::OK);
        assert_eq!(
            r1.get("queued").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        let first_job = receiver.try_recv().expect("first monitor job enqueued");
        let first_lease = first_job
            .monitor_guard_lease
            .as_ref()
            .expect("first monitor job carries guard lease")
            .clone();

        // Sleep 4 seconds — longer than tier-3 TTL (3s).
        tokio::time::sleep(std::time::Duration::from_millis(4_000)).await;

        // The dedup window has expired, but the original 45-second monitor
        // single-flight remains active. The callback is therefore recognized
        // as a fresh request and rejected as busy rather than as a duplicate.
        let (s2, _, r2) = call_feishu_http(runtime.clone(), body.clone()).await;
        assert_eq!(s2, StatusCode::OK);
        assert!(
            r2.get("duplicate").is_none(),
            "tier-3 dedup must NOT lock out for 30 minutes when no id is present"
        );
        assert_eq!(
            r2.get("queued").and_then(serde_json::Value::as_bool),
            Some(false),
            "an active monitor must prevent a second job from being queued"
        );
        assert_eq!(
            r2.get("busy").and_then(serde_json::Value::as_bool),
            Some(true),
            "fresh callback must be classified as single-flight busy"
        );
        assert!(receiver.try_recv().is_err(), "busy callback must not enqueue");

        // Once the first run completes and releases its owner-scoped lease,
        // the same no-id action is allowed again after the busy callback's
        // independent three-second dedup window expires.
        assert!(
            runtime
                .monitor_flight_guard()
                .release(&first_lease)
                .await,
            "first monitor lease must release"
        );
        tokio::time::sleep(std::time::Duration::from_millis(4_000)).await;
        let (s3, _, r3) = call_feishu_http(runtime, body).await;
        assert_eq!(s3, StatusCode::OK);
        assert_eq!(
            r3.get("queued").and_then(serde_json::Value::as_bool),
            Some(true),
            "monitor must enqueue again after the active lease is released"
        );
        assert!(receiver.try_recv().is_ok(), "replacement monitor job enqueued");
    }

    /// v0.9.1.2.2: single-flight — first monitor_30s card click acquires the guard.
    #[tokio::test]
    async fn unified_card_monitor_first_click_acquires_singleflight() {
        DedupCache::global().clear().await;
        let runtime = GatewayRuntime::new(feishu_security_config("dev", None, None));
        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<crate::gateway::feishu_worker::FeishuAsyncJob>(8);
        runtime.init_feishu_worker(sender).await;

        let body = format!(
            r#"{{"schema":"2.0","header":{{"event_id":"evt_SF1","event_type":"card.action.trigger","token":null}},"event":{{"action":{{"value":{{"action":"monitor_30s"}}}},"context":{{"open_chat_id":"oc_sf1","open_message_id":"om_sf1"}}}}}}"#
        );
        let (status, _, response) = call_feishu_http(runtime.clone(), body).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            response.get("ok").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            response.get("queued").and_then(serde_json::Value::as_bool),
            Some(true),
            "first click must acquire guard and enqueue"
        );
        let job = receiver.try_recv().expect("first monitor job enqueued");
        assert!(
            job.monitor_guard_lease.is_some(),
            "job must carry monitor guard lease"
        );
        let lease = job.monitor_guard_lease.as_ref().expect("monitor lease");
        assert!(
            runtime.monitor_flight_guard().renew(lease).await,
            "worker must be able to renew the lease without a second acquire"
        );
        assert!(
            runtime
                .monitor_flight_guard()
                .try_acquire_with_ttl(
                    job.inbound.session_id.as_deref().expect("chat session"),
                    45,
                )
                .await
                .is_none(),
            "the same chat must remain single-flight while the lease is active"
        );
        assert_eq!(
            job.inbound.text,
            "/monitor 桌面 30秒"
        );
    }

    /// v0.9.1.2.2: single-flight — second monitor_30s while first is running
    /// returns busy=true, queued=false, no second job enqueued.
    #[tokio::test]
    async fn unified_card_monitor_second_click_returns_busy() {
        DedupCache::global().clear().await;
        let runtime = GatewayRuntime::new(feishu_security_config("dev", None, None));
        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<crate::gateway::feishu_worker::FeishuAsyncJob>(8);
        runtime.init_feishu_worker(sender).await;

        // Use DIFFERENT event_ids so the second click bypasses the dedup cache
        // and reaches the single-flight guard. The single-flight guard is what
        // should return busy=true.
        let body1 = format!(
            r#"{{"schema":"2.0","header":{{"event_id":"evt_SF2a","event_type":"card.action.trigger","token":null}},"event":{{"action":{{"value":{{"action":"monitor_30s"}}}},"context":{{"open_chat_id":"oc_sf2","open_message_id":"om_sf2a"}}}}}}"#
        );
        let body2 = format!(
            r#"{{"schema":"2.0","header":{{"event_id":"evt_SF2b","event_type":"card.action.trigger","token":null}},"event":{{"action":{{"value":{{"action":"monitor_30s"}}}},"context":{{"open_chat_id":"oc_sf2","open_message_id":"om_sf2b"}}}}}}"#
        );

        // First click: acquires guard
        let (s1, _, r1) = call_feishu_http(runtime.clone(), body1).await;
        assert_eq!(s1, StatusCode::OK);
        assert_eq!(
            r1.get("queued").and_then(serde_json::Value::as_bool),
            Some(true)
        );

        // Second click: guard is held — returns busy
        let (s2, _, r2) = call_feishu_http(runtime.clone(), body2).await;
        assert_eq!(s2, StatusCode::OK);
        assert_eq!(
            r2.get("ok").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            r2.get("busy").and_then(serde_json::Value::as_bool),
            Some(true),
            "second click must return busy=true"
        );
        assert_eq!(
            r2.get("queued").and_then(serde_json::Value::as_bool),
            Some(false),
            "second click must not enqueue a new job"
        );
        assert!(r2.get("ack_ms").is_some(), "ack_ms must be recorded");

        // Only one job should be in the queue
        let _first = receiver.try_recv().expect("first job");
        assert!(
            receiver.try_recv().is_err(),
            "no second monitor job must be enqueued while guard is held"
        );
    }

    /// v0.9.1.2.2: after guard TTL expires, a new click can re-acquire.
    /// TTL = 30+15 = 45 seconds. To keep the test fast, clear this runtime's
    /// in-memory guard instead of actually sleeping.
    #[tokio::test]
    async fn unified_card_monitor_reacquires_after_guard_release() {
        DedupCache::global().clear().await;
        let runtime = GatewayRuntime::new(feishu_security_config("dev", None, None));
        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<crate::gateway::feishu_worker::FeishuAsyncJob>(8);
        runtime.init_feishu_worker(sender).await;

        // Use unique event_ids so each click bypasses the dedup cache.
        let body1 = format!(
            r#"{{"schema":"2.0","header":{{"event_id":"evt_SF3a","event_type":"card.action.trigger","token":null}},"event":{{"action":{{"value":{{"action":"monitor_30s"}}}},"context":{{"open_chat_id":"oc_sf3","open_message_id":"om_sf3a"}}}}}}"#
        );
        let body2 = format!(
            r#"{{"schema":"2.0","header":{{"event_id":"evt_SF3b","event_type":"card.action.trigger","token":null}},"event":{{"action":{{"value":{{"action":"monitor_30s"}}}},"context":{{"open_chat_id":"oc_sf3","open_message_id":"om_sf3b"}}}}}}"#
        );
        let body3 = format!(
            r#"{{"schema":"2.0","header":{{"event_id":"evt_SF3c","event_type":"card.action.trigger","token":null}},"event":{{"action":{{"value":{{"action":"monitor_30s"}}}},"context":{{"open_chat_id":"oc_sf3","open_message_id":"om_sf3c"}}}}}}"#
        );

        // First click
        let (s1, _, r1) = call_feishu_http(runtime.clone(), body1).await;
        assert_eq!(s1, StatusCode::OK);
        assert_eq!(r1.get("queued").and_then(serde_json::Value::as_bool), Some(true));
        let _ = receiver.try_recv().expect("first job");

        // Second click blocked by guard (different event_id bypasses dedup)
        let (s2, _, r2) = call_feishu_http(runtime.clone(), body2).await;
        assert_eq!(s2, StatusCode::OK);
        assert_eq!(r2.get("busy").and_then(serde_json::Value::as_bool), Some(true));

        // Simulate guard expiry: clear the flight guard
        runtime.monitor_flight_guard().clear().await;

        // Third click after expiry: must succeed
        let (s3, _, r3) = call_feishu_http(runtime, body3).await;
        assert_eq!(s3, StatusCode::OK);
        assert_eq!(
            r3.get("busy").and_then(serde_json::Value::as_bool),
            Some(false),
            "third click after guard release must succeed"
        );
        assert_eq!(r3.get("queued").and_then(serde_json::Value::as_bool), Some(true));
        let _ = receiver.try_recv().expect("second job after guard release");
    }

    /// v0.9.1.2.2: ack_ms must still be recorded on busy response.
    #[tokio::test]
    async fn unified_card_monitor_busy_response_has_ack_ms() {
        DedupCache::global().clear().await;
        let runtime = GatewayRuntime::new(feishu_security_config("dev", None, None));
        let (sender, _) =
            tokio::sync::mpsc::channel::<crate::gateway::feishu_worker::FeishuAsyncJob>(8);
        runtime.init_feishu_worker(sender).await;

        // Use unique event_ids so the second click reaches the single-flight guard.
        let body1 = format!(
            r#"{{"schema":"2.0","header":{{"event_id":"evt_ACKBUSY1","event_type":"card.action.trigger","token":null}},"event":{{"action":{{"value":{{"action":"monitor_30s"}}}},"context":{{"open_chat_id":"oc_ackbusy","open_message_id":"om_ackbusy1"}}}}}}"#
        );
        let body2 = format!(
            r#"{{"schema":"2.0","header":{{"event_id":"evt_ACKBUSY2","event_type":"card.action.trigger","token":null}},"event":{{"action":{{"value":{{"action":"monitor_30s"}}}},"context":{{"open_chat_id":"oc_ackbusy","open_message_id":"om_ackbusy2"}}}}}}"#
        );

        let (s1, _, _) = call_feishu_http(runtime.clone(), body1).await;
        assert_eq!(s1, StatusCode::OK);

        let (_, _, r2) = call_feishu_http(runtime, body2).await;
        assert_eq!(r2.get("busy").and_then(serde_json::Value::as_bool), Some(true));
        let ack_ms = r2.get("ack_ms").and_then(serde_json::Value::as_i64);
        assert!(ack_ms.is_some(), "ack_ms must be recorded on busy response");
        assert!(ack_ms.unwrap_or(-1) >= 0, "ack_ms must be non-negative");
    }

    /// v0.9.1.2.2: slash /monitor during card monitor returns busy.
    /// First: card monitor_30s acquires guard. Then: slash /monitor in same chat
    /// should be blocked by the guard and return a busy reply (not run another monitor).
    #[tokio::test]
    async fn unified_slash_monitor_blocked_by_card_monitor() {
        DedupCache::global().clear().await;
        let runtime = GatewayRuntime::new(feishu_security_config("dev", None, None));
        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<crate::gateway::feishu_worker::FeishuAsyncJob>(8);
        runtime.init_feishu_worker(sender).await;

        // Step 1: card monitor acquires the guard
        let card_body = format!(
            r#"{{"schema":"2.0","header":{{"event_id":"evt_SFslashC","event_type":"card.action.trigger","token":null}},"event":{{"action":{{"value":{{"action":"monitor_30s"}}}},"context":{{"open_chat_id":"oc_sfslash","open_message_id":"om_sfslash"}}}}}}"#
        );
        let (s1, _, r1) = call_feishu_http(runtime.clone(), card_body).await;
        assert_eq!(s1, StatusCode::OK);
        assert_eq!(r1.get("queued").and_then(serde_json::Value::as_bool), Some(true));
        let _ = receiver.try_recv().expect("card monitor job enqueued");

        // Step 2: slash /monitor with the same chat_id — should be blocked by guard.
        // Slash /monitor comes in as an im.message.receive_v1 with text starting with /monitor.
        // Must include message_type="text" to pass the message_type filter.
        let slash_body = format!(
            r#"{{"schema":"2.0","header":{{"event_id":"evt_SFslashS","event_type":"im.message.receive_v1","token":null}},"event":{{"message":{{"message_id":"om_sfslash_s","chat_id":"oc_sfslash","message_type":"text","content":"{{\"text\":\"/monitor 桌面 30秒\"}}"}},"sender":{{"sender_type":"user","open_id":"ou_test","id":"test_user"}}}}}}"#
        );
        let (s2, _, r2) = call_feishu_http(runtime.clone(), slash_body).await;
        assert_eq!(s2, StatusCode::OK);
        assert_eq!(
            r2.get("busy").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        // The slash should still ACK 200, but no new monitor job should be enqueued.
        // Only one job should be in the queue (the card monitor).
        assert!(
            receiver.try_recv().is_err(),
            "slash /monitor must not enqueue a second job when card monitor guard is held"
        );
    }

    /// v0.9.1.2.2: after card monitor guard releases, slash /monitor can run normally.
    #[tokio::test]
    async fn unified_slash_monitor_runs_after_card_guard_released() {
        DedupCache::global().clear().await;
        let runtime = GatewayRuntime::new(feishu_security_config("dev", None, None));
        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<crate::gateway::feishu_worker::FeishuAsyncJob>(8);
        runtime.init_feishu_worker(sender).await;

        // Card monitor acquires guard
        let card_body = format!(
            r#"{{"schema":"2.0","header":{{"event_id":"evt_SFreleaseC","event_type":"card.action.trigger","token":null}},"event":{{"action":{{"value":{{"action":"monitor_30s"}}}},"context":{{"open_chat_id":"oc_sfrelease","open_message_id":"om_sfrelease"}}}}}}"#
        );
        let (s1, _, r1) = call_feishu_http(runtime.clone(), card_body).await;
        assert_eq!(s1, StatusCode::OK);
        assert_eq!(r1.get("queued").and_then(serde_json::Value::as_bool), Some(true));
        let _ = receiver.try_recv().expect("card monitor job enqueued");

        // Slash blocked by guard — must include message_type="text".
        let slash_body = format!(
            r#"{{"schema":"2.0","header":{{"event_id":"evt_SFreleaseS","event_type":"im.message.receive_v1","token":null}},"event":{{"message":{{"message_id":"om_sfrelease_s","chat_id":"oc_sfrelease","message_type":"text","content":"{{\"text\":\"/monitor 桌面 30秒\"}}"}},"sender":{{"sender_type":"user","open_id":"ou_test","id":"test_user"}}}}}}"#
        );
        let (s2, _, busy_response) =
            call_feishu_http(runtime.clone(), slash_body).await;
        assert_eq!(s2, StatusCode::OK);
        assert_eq!(
            busy_response
                .get("busy")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert!(receiver.try_recv().is_err(), "slash blocked by guard");

        // Simulate guard expiry
        runtime.monitor_flight_guard().clear().await;

        // Now slash should succeed
        let retry_slash_body = format!(
            r#"{{"schema":"2.0","header":{{"event_id":"evt_SFreleaseS_retry","event_type":"im.message.receive_v1","token":null}},"event":{{"message":{{"message_id":"om_sfrelease_s_retry","chat_id":"oc_sfrelease","message_type":"text","content":"{{\"text\":\"/monitor 桌面 30秒\"}}"}},"sender":{{"sender_type":"user","open_id":"ou_test","id":"test_user"}}}}}}"#
        );
        let (s3, _, r3) = call_feishu_http(runtime, retry_slash_body).await;
        assert_eq!(s3, StatusCode::OK);
        // After guard expiry, slash /monitor enqueues a new job
        let job = receiver.try_recv().expect("slash monitor job after guard release");
        assert_eq!(
            job.inbound.text.trim(),
            "/monitor 桌面 30秒"
        );
        assert!(
            job.monitor_guard_lease.is_some(),
            "slash monitor must carry the shared guard lease"
        );
        assert_eq!(r3.get("accepted").and_then(serde_json::Value::as_bool), Some(true));
    }

    #[test]
    fn feishu_token_missing_diagnostics_only_include_shape_not_values() {
        let secret_content = "message-body-must-not-appear";
        let unrelated_secret = "unrelated-secret-must-not-appear";
        let payload = json!({
            "schema": "2.0",
            "header": {
                "event_id": "event-id-must-not-appear",
                "event_type": "im.message.receive_v1",
                "create_time": "timestamp-must-not-appear",
                "verification_token": unrelated_secret
            },
            "event": {
                "message": {
                    "content": secret_content
                },
                "sender": {
                    "sender_id": "sender-id-must-not-appear"
                }
            }
        });
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/json; charset=utf-8"
                .parse()
                .expect("content type"),
        );

        let diagnostics =
            super::feishu_token_missing_diagnostic_lines(&payload, &headers).join("\n");
        assert!(diagnostics.contains(
            "payload_shape top_keys=[\"event\", \"header\", \"schema\"]"
        ));
        assert!(diagnostics.contains(
            "header_shape keys=[\"create_time\", \"event_id\", \"event_type\", \"verification_token\"]"
        ));
        assert!(diagnostics.contains("event_shape keys=[\"message\", \"sender\"]"));
        assert!(diagnostics.contains("content_type=application/json"));
        assert!(diagnostics.contains("looks_like_im_message_receive_v1=true"));
        assert!(diagnostics.contains("token_present=false"));
        for forbidden in [
            secret_content,
            unrelated_secret,
            "event-id-must-not-appear",
            "timestamp-must-not-appear",
            "sender-id-must-not-appear",
        ] {
            assert!(!diagnostics.contains(forbidden));
        }
    }

    #[tokio::test]
    async fn feishu_dev_url_verification_returns_json_challenge() {
        let runtime = GatewayRuntime::new(feishu_security_config("dev", None, None));
        let (status, content_type, response) = call_feishu_http(
            runtime,
            json!({
                "type": "url_verification",
                "token": "ignored-in-dev",
                "challenge": "dev-challenge"
            })
            .to_string(),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(content_type.starts_with("application/json"));
        assert_eq!(
            response.get("challenge").and_then(|value| value.as_str()),
            Some("dev-challenge")
        );
    }

    #[tokio::test]
    async fn feishu_token_url_verification_returns_challenge_or_json_error() {
        let config = || {
            feishu_security_config("token", Some("test-verification-token"), None)
        };

        let (ok_status, ok_content_type, ok_response) = call_feishu_http(
            GatewayRuntime::new(config()),
            json!({
                "type": "url_verification",
                "token": "test-verification-token",
                "challenge": "token-challenge"
            })
            .to_string(),
        )
        .await;
        assert_eq!(ok_status, StatusCode::OK);
        assert!(ok_content_type.starts_with("application/json"));
        assert_eq!(
            ok_response
                .get("challenge")
                .and_then(|value| value.as_str()),
            Some("token-challenge")
        );

        let (missing_status, missing_content_type, missing_response) = call_feishu_http(
            GatewayRuntime::new(config()),
            json!({
                "type": "url_verification",
                "challenge": "missing-token"
            })
            .to_string(),
        )
        .await;
        assert_eq!(missing_status, StatusCode::UNAUTHORIZED);
        assert!(missing_content_type.starts_with("application/json"));
        assert_eq!(
            missing_response.get("error").and_then(|value| value.as_str()),
            Some("token_missing")
        );

        let (bad_status, bad_content_type, bad_response) = call_feishu_http(
            GatewayRuntime::new(config()),
            json!({
                "type": "url_verification",
                "token": "wrong-token",
                "challenge": "bad-token"
            })
            .to_string(),
        )
        .await;
        assert_eq!(bad_status, StatusCode::UNAUTHORIZED);
        assert!(bad_content_type.starts_with("application/json"));
        assert_eq!(
            bad_response.get("error").and_then(|value| value.as_str()),
            Some("token_mismatch")
        );
    }

    #[tokio::test]
    async fn feishu_url_verification_rejects_missing_challenge_and_invalid_json_as_json() {
        let (missing_status, missing_content_type, missing_response) = call_feishu_http(
            GatewayRuntime::new(feishu_security_config("dev", None, None)),
            json!({ "type": "url_verification" }).to_string(),
        )
        .await;
        assert_eq!(missing_status, StatusCode::BAD_REQUEST);
        assert!(missing_content_type.starts_with("application/json"));
        assert_eq!(
            missing_response.get("error").and_then(|value| value.as_str()),
            Some("missing_challenge")
        );

        let (invalid_status, invalid_content_type, invalid_response) = call_feishu_http(
            GatewayRuntime::new(feishu_security_config("dev", None, None)),
            "{not-json".to_string(),
        )
        .await;
        assert_eq!(invalid_status, StatusCode::BAD_REQUEST);
        assert!(invalid_content_type.starts_with("application/json"));
        assert_eq!(
            invalid_response.get("error").and_then(|value| value.as_str()),
            Some("invalid_json")
        );
    }

    #[tokio::test]
    async fn feishu_encrypted_url_verification_returns_challenge_or_json_error() {
        let encrypt_key = "test-encrypt-key";
        let plaintext = json!({
            "type": "url_verification",
            "challenge": "encrypted-json-challenge",
            "token": "test-verification-token"
        })
        .to_string();
        let encrypted = encrypt_feishu_payload_for_test(&plaintext, encrypt_key);
        let (ok_status, ok_content_type, ok_response) = call_feishu_http(
            GatewayRuntime::new(feishu_security_config(
                "encrypted",
                Some("test-verification-token"),
                Some(encrypt_key),
            )),
            json!({ "encrypt": encrypted }).to_string(),
        )
        .await;
        assert_eq!(ok_status, StatusCode::OK);
        assert!(ok_content_type.starts_with("application/json"));
        assert_eq!(
            ok_response
                .get("challenge")
                .and_then(|value| value.as_str()),
            Some("encrypted-json-challenge")
        );

        let (bad_status, bad_content_type, bad_response) = call_feishu_http(
            GatewayRuntime::new(feishu_security_config(
                "encrypted",
                Some("test-verification-token"),
                Some("wrong-key"),
            )),
            json!({ "encrypt": "AAAAAAAAAAAAAAAAAAAAAA==" }).to_string(),
        )
        .await;
        assert_eq!(bad_status, StatusCode::FORBIDDEN);
        assert!(bad_content_type.starts_with("application/json"));
        assert_eq!(
            bad_response.get("error").and_then(|value| value.as_str()),
            Some("decrypt_failed")
        );

        let (missing_key_status, missing_key_content_type, missing_key_response) =
            call_feishu_http(
                GatewayRuntime::new(feishu_security_config(
                    "encrypted",
                    Some("test-verification-token"),
                    None,
                )),
                json!({ "encrypt": "AAAAAAAAAAAAAAAAAAAAAA==" }).to_string(),
            )
            .await;
        assert_eq!(missing_key_status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(missing_key_content_type.starts_with("application/json"));
        assert_eq!(
            missing_key_response
                .get("error")
                .and_then(|value| value.as_str()),
            Some("encrypt_key_missing")
        );
    }

    #[tokio::test]
    async fn feishu_invalid_security_mode_returns_json_error() {
        let (status, content_type, response) = call_feishu_http(
            GatewayRuntime::new(feishu_security_config("unsupported-mode", None, None)),
            json!({
                "type": "url_verification",
                "challenge": "must-not-pass"
            })
            .to_string(),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(content_type.starts_with("application/json"));
        assert_eq!(
            response.get("error").and_then(|value| value.as_str()),
            Some("invalid_security_mode")
        );
    }

    #[tokio::test]
    async fn feishu_rejected_token_and_url_verification_do_not_enter_store_or_worker() {
        let test_dir = std::env::temp_dir().join(format!(
            "omninova_feishu_verification_{}",
            uuid::Uuid::new_v4()
        ));
        let store = Arc::new(
            crate::gateway::feishu_store::FeishuStore::open(&test_dir)
                .expect("test Feishu store"),
        );
        let mut runtime = GatewayRuntime::new(feishu_security_config(
            "token",
            Some("test-verification-token"),
            None,
        ));
        runtime.feishu_store = Some(store.clone());
        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<crate::gateway::feishu_worker::FeishuAsyncJob>(4);
        runtime.init_feishu_worker(sender).await;

        let event_id = format!("evt_{}", uuid::Uuid::new_v4());
        let message_id = format!("om_{}", uuid::Uuid::new_v4());
        let normal_payload = |token: Option<&str>| {
            json!({
                "header": {
                    "event_id": event_id,
                    "event_type": "im.message.receive_v1",
                    "token": token
                },
                "event": {
                    "sender": {
                        "sender_type": "user",
                        "sender_id": { "open_id": "ou_test" }
                    },
                    "message": {
                        "message_id": message_id,
                        "chat_id": "oc_test",
                        "message_type": "text",
                        "content": "{\"text\":\"hello\"}"
                    }
                }
            })
            .to_string()
        };

        let (missing_status, _, missing_response) =
            call_feishu_http(runtime.clone(), normal_payload(None)).await;
        assert_eq!(missing_status, StatusCode::UNAUTHORIZED);
        assert_eq!(
            missing_response.get("error").and_then(|value| value.as_str()),
            Some("token_missing")
        );

        let (bad_status, _, bad_response) =
            call_feishu_http(runtime.clone(), normal_payload(Some("wrong-token"))).await;
        assert_eq!(bad_status, StatusCode::UNAUTHORIZED);
        assert_eq!(
            bad_response.get("error").and_then(|value| value.as_str()),
            Some("token_mismatch")
        );
        let empty_stats = store.get_store_stats().expect("empty store stats");
        assert_eq!(empty_stats.events_total, 0);
        assert_eq!(empty_stats.jobs_total, 0);
        assert!(matches!(
            receiver.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));

        let (message_status, _, message_response) =
            call_feishu_http(
                runtime.clone(),
                normal_payload(Some("test-verification-token")),
            )
            .await;
        assert_eq!(message_status, StatusCode::OK);
        assert_eq!(
            message_response.get("accepted").and_then(|value| value.as_bool()),
            Some(true)
        );
        assert!(receiver.try_recv().is_ok());
        let message_stats = store.get_store_stats().expect("message store stats");
        assert_eq!(message_stats.events_total, 1);
        assert_eq!(message_stats.jobs_total, 1);

        let (challenge_status, _, challenge_response) = call_feishu_http(
            runtime.clone(),
            json!({
                "type": "url_verification",
                "token": "test-verification-token",
                "challenge": "store-bypass-challenge"
            })
            .to_string(),
        )
        .await;
        assert_eq!(challenge_status, StatusCode::OK);
        assert_eq!(
            challenge_response
                .get("challenge")
                .and_then(|value| value.as_str()),
            Some("store-bypass-challenge")
        );
        let challenge_stats = store.get_store_stats().expect("challenge store stats");
        assert_eq!(challenge_stats.events_total, 1);
        assert_eq!(challenge_stats.jobs_total, 1);
        assert!(matches!(
            receiver.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));

        drop(runtime);
        drop(store);
        std::fs::remove_dir_all(&test_dir).expect("remove test Feishu store");
    }

    #[tokio::test]
    async fn feishu_token_mode_accepts_correct_token_and_rejects_bad_or_missing_tokens() {
        let body = r#"{"token":"test-verification-token","header":{"event_id":"evt_security_token_ok"},"event":{"sender":{"sender_type":"user","sender_id":{"open_id":"ou_test"}},"message":{"message_id":"om_security_token_ok","chat_id":"oc_test","message_type":"text","content":"{\"text\":\"hello\"}"}}}"#.to_string();
        let runtime = GatewayRuntime::new(feishu_security_config("token", Some("test-verification-token"), None));
        let ok = super::http_channel_webhook(runtime, HeaderMap::new(), body.clone(), ChannelKind::Feishu)
            .await
            .expect("correct token should pass")
            .0;
        assert_eq!(ok.get("ok").and_then(|v| v.as_bool()), Some(true));

        let missing = super::http_channel_webhook(
            GatewayRuntime::new(feishu_security_config("token", Some("test-verification-token"), None)),
            HeaderMap::new(),
            body.replacen("\"token\":\"test-verification-token\",", "", 1),
            ChannelKind::Feishu,
        )
        .await;
        assert!(matches!(missing, Err((StatusCode::UNAUTHORIZED, _))));

        let bad = super::http_channel_webhook(
            GatewayRuntime::new(feishu_security_config("token", Some("test-verification-token"), None)),
            HeaderMap::new(),
            body.replacen("test-verification-token", "wrong-token", 1),
            ChannelKind::Feishu,
        )
        .await;
        assert!(matches!(bad, Err((StatusCode::UNAUTHORIZED, _))));
    }

    #[tokio::test]
    async fn feishu_encrypted_mode_decrypts_before_token_and_challenge_handling() {
        let encrypt_key = "test-encrypt-key";
        let plaintext = r#"{"type":"url_verification","challenge":"encrypted-challenge","token":"test-verification-token"}"#;
        let encrypted = encrypt_feishu_payload_for_test(plaintext, encrypt_key);
        let body = serde_json::json!({ "encrypt": encrypted }).to_string();
        let runtime = GatewayRuntime::new(feishu_security_config(
            "encrypted",
            Some("test-verification-token"),
            Some(encrypt_key),
        ));
        let response = super::http_channel_webhook(runtime, HeaderMap::new(), body, ChannelKind::Feishu)
            .await
            .expect("encrypted payload should pass")
            .0;
        assert_eq!(response.get("challenge").and_then(|v| v.as_str()), Some("encrypted-challenge"));
    }

    #[tokio::test]
    async fn feishu_encrypted_mode_rejects_missing_or_invalid_encrypt_key_before_worker() {
        let body = serde_json::json!({ "encrypt": "AAAAAAAAAAAAAAAAAAAAAA==" }).to_string();
        let missing_key = super::http_channel_webhook(
            GatewayRuntime::new(feishu_security_config("encrypted", Some("token"), None)),
            HeaderMap::new(),
            body.clone(),
            ChannelKind::Feishu,
        )
        .await;
        assert!(missing_key.is_err());

        let invalid_key = super::http_channel_webhook(
            GatewayRuntime::new(feishu_security_config("encrypted", Some("token"), Some("wrong-key"))),
            HeaderMap::new(),
            body,
            ChannelKind::Feishu,
        )
        .await;
        assert!(invalid_key.is_err());
    }

    #[test]
    fn is_channel_enabled_returns_true_for_enabled_channel() {
        let config = make_config_with_channel(ChannelKind::Feishu, true);
        assert!(super::is_channel_enabled(&config, &ChannelKind::Feishu));
    }

    #[test]
    fn is_channel_enabled_returns_false_for_disabled_channel() {
        let config = make_config_with_channel(ChannelKind::Feishu, false);
        assert!(!super::is_channel_enabled(&config, &ChannelKind::Feishu));
    }

    #[test]
    fn is_channel_enabled_returns_false_for_unknown_channel() {
        let config = Config::default();
        assert!(!super::is_channel_enabled(&config, &ChannelKind::Feishu));
    }

    #[test]
    fn is_channel_enabled_works_for_lark() {
        let config = make_config_with_channel(ChannelKind::Lark, true);
        assert!(super::is_channel_enabled(&config, &ChannelKind::Lark));

        let config_disabled = make_config_with_channel(ChannelKind::Lark, false);
        assert!(!super::is_channel_enabled(
            &config_disabled,
            &ChannelKind::Lark
        ));
    }

    #[test]
    fn is_channel_enabled_works_for_telegram() {
        let config = make_config_with_channel(ChannelKind::Telegram, true);
        assert!(super::is_channel_enabled(&config, &ChannelKind::Telegram));
    }

    // =============================================================================
    // Platform webhook response tests
    // =============================================================================

    #[test]
    fn platform_webhook_response_success() {
        let response = super::PlatformWebhookResponse::success(
            "feishu",
            Some("msg_123".to_string()),
            Some("chat_456".to_string()),
            "Hello from agent".to_string(),
        );

        assert!(response.ok);
        assert_eq!(response.channel, "feishu");
        assert_eq!(response.message_id, Some("msg_123".to_string()));
        assert_eq!(response.conversation_id, Some("chat_456".to_string()));
        assert_eq!(response.agent_reply, Some("Hello from agent".to_string()));
        assert!(matches!(
            response.outbound_delivery,
            super::OutboundDeliveryStatus::HttpResponseOnly
        ));
        assert!(response.error.is_none());
    }

    #[test]
    fn platform_webhook_response_error() {
        let response = super::PlatformWebhookResponse::error("feishu", "agent runtime failed");

        assert!(!response.ok);
        assert_eq!(response.channel, "feishu");
        assert!(response.agent_reply.is_none());
        assert!(matches!(
            response.outbound_delivery,
            super::OutboundDeliveryStatus::NotImplemented
        ));
        assert_eq!(response.error, Some("agent runtime failed".to_string()));
    }

    #[test]
    fn outbound_delivery_status_default() {
        let status = super::OutboundDeliveryStatus::default();
        assert!(matches!(
            status,
            super::OutboundDeliveryStatus::NotImplemented
        ));
    }

    #[test]
    fn platform_webhook_response_success_with_outbound() {
        let summary = super::OutboundResultSummary {
            ok: true,
            provider: "feishu".to_string(),
            delivery: super::OutboundDeliveryStatus::Sent,
            platform_message_id: Some("om_reply_123".to_string()),
            error_code: None,
            message: None,
        };
        let response = super::PlatformWebhookResponse::success_with_outbound(
            "feishu",
            Some("msg_123".to_string()),
            Some("chat_456".to_string()),
            "Agent reply".to_string(),
            summary,
        );

        assert!(response.ok);
        assert_eq!(response.channel, "feishu");
        assert!(response.agent_reply.is_some());
        assert!(matches!(
            response.outbound_delivery,
            super::OutboundDeliveryStatus::Sent
        ));
        assert!(response.outbound_result.is_some());
        assert_eq!(
            response.outbound_result.as_ref().unwrap().provider,
            "feishu"
        );
    }

    fn outbound_test_config(channel: ChannelKind, mode: &str, with_credentials: bool) -> Config {
        let mut config = make_config_with_channel(channel.clone(), true);
        config.default_provider = Some("mock".to_string());
        config.workspace_dir = temp_workspace();
        let entry = match channel {
            ChannelKind::Feishu => config.channels_config.feishu.as_mut().unwrap(),
            ChannelKind::Lark => config.channels_config.lark.as_mut().unwrap(),
            _ => unreachable!("test only configures Feishu/Lark"),
        };
        entry.extra.insert("outbound_mode".to_string(), json!(mode));
        if with_credentials {
            entry.extra.insert("app_id".to_string(), json!("cli_test"));
            entry
                .extra
                .insert("app_secret".to_string(), json!("fake_secret"));
        }
        config
    }

    fn feishu_inbound(chat_id: Option<&str>) -> InboundMessage {
        let mut metadata = HashMap::new();
        metadata.insert("message_id".to_string(), json!("om_test"));
        if let Some(chat_id) = chat_id {
            metadata.insert("chat_id".to_string(), json!(chat_id));
        }
        InboundMessage {
            channel: ChannelKind::Feishu,
            user_id: Some("ou_test".to_string()),
            session_id: chat_id.map(ToString::to_string),
            text: "hello".to_string(),
            metadata,
        }
    }

    #[tokio::test]
    async fn feishu_webhook_runtime_success_uses_mock_sender() {
        let runtime = GatewayRuntime::new(outbound_test_config(ChannelKind::Feishu, "mock", false));
        // Add message_type: "text" so message is not filtered as unsupported type
        let body = r#"{"event":{"sender":{"sender_type":"user","sender_id":{"open_id":"ou_test"}},"message":{"message_id":"om_test","chat_id":"oc_test","message_type":"text","content":"{\"text\":\"hello\"}"}}}"#.to_string();
        let response =
            super::http_channel_webhook(runtime, HeaderMap::new(), body, ChannelKind::Feishu)
                .await
                .expect("mock webhook should succeed")
                .0;
        assert_eq!(response.get("ok").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            response.get("outbound_delivery").and_then(|v| v.as_str()),
            Some("mock_sent")
        );
        assert_eq!(
            response
                .pointer("/outbound_result/provider")
                .and_then(|v| v.as_str()),
            Some("mock")
        );
    }

    #[tokio::test]
    async fn outbound_empty_reply_is_skipped() {
        let result = super::deliver_platform_reply(
            &outbound_test_config(ChannelKind::Feishu, "mock", false),
            &feishu_inbound(Some("oc_test")),
            "   ",
        )
        .await
        .expect("Feishu outbound mode should produce a result");
        assert_eq!(
            result.delivery,
            super::OutboundDeliveryStatus::SkippedEmptyReply
        );
    }

    #[tokio::test]
    async fn outbound_missing_reply_target_fails_without_sending() {
        let result = super::deliver_platform_reply(
            &outbound_test_config(ChannelKind::Feishu, "mock", false),
            &feishu_inbound(None),
            "reply",
        )
        .await
        .expect("Feishu outbound mode should produce a result");
        assert_eq!(result.delivery, super::OutboundDeliveryStatus::Failed);
        assert_eq!(result.error_code.as_deref(), Some("missing_reply_target"));
    }

    #[tokio::test]
    async fn outbound_real_mode_requires_app_credentials() {
        let result = super::deliver_platform_reply(
            &outbound_test_config(ChannelKind::Lark, "real", false),
            &InboundMessage {
                channel: ChannelKind::Lark,
                ..feishu_inbound(Some("oc_test"))
            },
            "reply",
        )
        .await
        .expect("real outbound mode should return a configuration result");
        assert_eq!(
            result.delivery,
            super::OutboundDeliveryStatus::NotConfigured
        );
        assert_eq!(result.error_code.as_deref(), Some("not_configured"));
    }

    #[test]
    fn outbound_failure_keeps_runtime_success_and_redacts_secrets() {
        let failure = crate::channels::adapters::outbound::OutboundResult::failed(
            "feishu",
            "token_fetch_failed",
            "token request failed (HTTP 401)",
        );
        let response = super::PlatformWebhookResponse::success_with_outbound(
            "feishu",
            Some("om_test".to_string()),
            Some("oc_test".to_string()),
            "agent reply".to_string(),
            failure.to_summary(),
        );
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value.get("ok").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            value.get("outbound_delivery").and_then(|v| v.as_str()),
            Some("failed")
        );
        assert_eq!(
            value
                .pointer("/outbound_result/error_code")
                .and_then(|v| v.as_str()),
            Some("token_fetch_failed")
        );
        assert!(!value.to_string().contains("fake_secret"));
        assert!(!value.to_string().contains("access_token"));
    }

    #[tokio::test]
    async fn challenge_bypasses_runtime_and_sender() {
        let runtime = GatewayRuntime::new(outbound_test_config(ChannelKind::Feishu, "mock", false));
        let response = super::http_channel_webhook(
            runtime,
            HeaderMap::new(),
            r#"{"type":"url_verification","challenge":"test_challenge"}"#.to_string(),
            ChannelKind::Feishu,
        )
        .await
        .expect("challenge should return directly")
        .0;
        assert_eq!(
            response.get("challenge").and_then(|v| v.as_str()),
            Some("test_challenge")
        );
        assert!(response.get("agent_reply").is_none());
    }

    #[tokio::test]
    async fn disabled_channel_is_rejected_before_runtime_and_sender() {
        let runtime = GatewayRuntime::new(make_config_with_channel(ChannelKind::Feishu, false));
        let result = super::http_channel_webhook(
            runtime,
            HeaderMap::new(),
            r#"{"event":{"message":{"content":"{\"text\":\"hello\"}"}}}"#.to_string(),
            ChannelKind::Feishu,
        )
        .await;
        assert!(matches!(result, Err((StatusCode::FORBIDDEN, _))));
    }

    #[tokio::test]
    async fn disabled_outbound_mode_preserves_http_response_only() {
        let result = super::deliver_platform_reply(
            &outbound_test_config(ChannelKind::Feishu, "disabled", false),
            &feishu_inbound(Some("oc_test")),
            "reply",
        )
        .await;
        assert!(result.is_none());
    }

    // =============================================================================
    // Feishu self-message filtering tests
    // =============================================================================

    #[tokio::test]
    async fn feishu_sender_type_user_enters_runtime() {
        let runtime = GatewayRuntime::new(outbound_test_config(ChannelKind::Feishu, "mock", false));
        // User message with sender_type=user
        let body = r#"{"header":{"event_id":"evt_user_001"},"event":{"sender":{"sender_type":"user","sender_id":{"open_id":"ou_real_user"}},"message":{"message_id":"om_user_001","chat_id":"oc_chat","message_type":"text","content":"{\"text\":\"user hello\"}"}}}"#.to_string();
        let response = super::http_channel_webhook(
            runtime,
            HeaderMap::new(),
            body,
            ChannelKind::Feishu,
        )
        .await
        .expect("user message should succeed");
        assert_eq!(response.0.get("ok").and_then(|v| v.as_bool()), Some(true));
        // Should have runtime result
        assert!(response.0.get("agent_reply").is_some() || response.0.get("outbound_delivery").is_some());
    }

    #[tokio::test]
    async fn feishu_sender_type_app_skips_runtime() {
        let runtime = GatewayRuntime::new(outbound_test_config(ChannelKind::Feishu, "mock", false));
        // Bot's own message with sender_type=app
        let body = r#"{"header":{"event_id":"evt_app_001"},"event":{"sender":{"sender_type":"app","sender_id":{"open_id":"ou_app_xxx"}},"message":{"message_id":"om_app_001","chat_id":"oc_chat","message_type":"text","content":"{\"text\":\"app hello\"}"}}}"#.to_string();
        let response = super::http_channel_webhook(
            runtime,
            HeaderMap::new(),
            body,
            ChannelKind::Feishu,
        )
        .await
        .expect("app message should return success");
        assert_eq!(response.0.get("ok").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(response.0.get("processing").and_then(|v| v.as_str()), Some("skipped"));
        assert!(response.0.get("reason").and_then(|v| v.as_str()).unwrap_or("").contains("app"));
    }

    #[tokio::test]
    async fn feishu_sender_type_bot_skips_runtime() {
        let runtime = GatewayRuntime::new(outbound_test_config(ChannelKind::Feishu, "mock", false));
        // Bot's own message with sender_type=bot
        let body = r#"{"header":{"event_id":"evt_bot_001"},"event":{"sender":{"sender_type":"bot","sender_id":{"open_id":"ou_bot_xxx"}},"message":{"message_id":"om_bot_001","chat_id":"oc_chat","message_type":"text","content":"{\"text\":\"bot hello\"}"}}}"#.to_string();
        let response = super::http_channel_webhook(
            runtime,
            HeaderMap::new(),
            body,
            ChannelKind::Feishu,
        )
        .await
        .expect("bot message should return success");
        assert_eq!(response.0.get("ok").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(response.0.get("processing").and_then(|v| v.as_str()), Some("skipped"));
        assert!(response.0.get("reason").and_then(|v| v.as_str()).unwrap_or("").contains("bot"));
    }

    #[tokio::test]
    async fn feishu_unknown_sender_type_skips_runtime() {
        let runtime = GatewayRuntime::new(outbound_test_config(ChannelKind::Feishu, "mock", false));
        // Unknown sender_type
        let body = r#"{"header":{"event_id":"evt_unknown_001"},"event":{"sender":{"sender_type":"unknown_type","sender_id":{"open_id":"ou_unknown"}},"message":{"message_id":"om_unknown_001","chat_id":"oc_chat","message_type":"text","content":"{\"text\":\"unknown hello\"}"}}}"#.to_string();
        let response = super::http_channel_webhook(
            runtime,
            HeaderMap::new(),
            body,
            ChannelKind::Feishu,
        )
        .await
        .expect("unknown sender message should return success");
        assert_eq!(response.0.get("ok").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(response.0.get("processing").and_then(|v| v.as_str()), Some("skipped"));
    }

    #[tokio::test]
    async fn feishu_non_text_message_skips_runtime() {
        let runtime = GatewayRuntime::new(outbound_test_config(ChannelKind::Feishu, "mock", false));
        // Image message (not text)
        let body = r#"{"header":{"event_id":"evt_img_001"},"event":{"sender":{"sender_type":"user","sender_id":{"open_id":"ou_user"}},"message":{"message_id":"om_img_001","chat_id":"oc_chat","message_type":"image","content":"{\"image_key\":\"img_xxx\"}"}}}"#.to_string();
        let response = super::http_channel_webhook(
            runtime,
            HeaderMap::new(),
            body,
            ChannelKind::Feishu,
        )
        .await
        .expect("image message should return success");
        assert_eq!(response.0.get("ok").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(response.0.get("reason").and_then(|v| v.as_str()), Some("unsupported_message_type"));
    }

    // =============================================================================
    // Feishu session isolation tests
    // =============================================================================

    #[tokio::test]
    async fn feishu_session_is_stateless() {
        let runtime = GatewayRuntime::new(outbound_test_config(ChannelKind::Feishu, "mock", false));
        let body = r#"{"header":{"event_id":"evt_stateless_001"},"event":{"sender":{"sender_type":"user","sender_id":{"open_id":"ou_user"}},"message":{"message_id":"om_stateless_001","chat_id":"oc_123","message_type":"text","content":"{\"text\":\"stateless test\"}"}}}"#.to_string();
        let response = super::http_channel_webhook(
            runtime,
            HeaderMap::new(),
            body,
            ChannelKind::Feishu,
        )
        .await
        .expect("stateless message should succeed");
        // Verify success response
        assert_eq!(response.0.get("ok").and_then(|v| v.as_bool()), Some(true));
        // Note: session_id is prefixed with "feishu:" internally but not exposed in response JSON
    }

    // =============================================================================
    // Feishu deduplication tests
    // =============================================================================

    #[tokio::test]
    async fn feishu_duplicate_event_id_is_rejected() {
        let runtime = GatewayRuntime::new(outbound_test_config(ChannelKind::Feishu, "mock", false));
        let body = r#"{"header":{"event_id":"evt_dup_001"},"event":{"sender":{"sender_type":"user","sender_id":{"open_id":"ou_user"}},"message":{"message_id":"om_dup_001","chat_id":"oc_chat","message_type":"text","content":"{\"text\":\"duplicate test\"}"}}}"#.to_string();
        
        // First request should succeed (not duplicate)
        let response1 = super::http_channel_webhook(
            runtime.clone(),
            HeaderMap::new(),
            body.clone(),
            ChannelKind::Feishu,
        )
        .await
        .expect("first request should succeed");
        // First request should NOT have duplicate=true
        assert_ne!(response1.0.get("duplicate").and_then(|v| v.as_bool()), Some(true));
        
        // Second request with same event_id should be duplicate
        let response2 = super::http_channel_webhook(
            runtime,
            HeaderMap::new(),
            body,
            ChannelKind::Feishu,
        )
        .await
        .expect("duplicate request should return success");
        assert_eq!(response2.0.get("duplicate").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(response2.0.get("processing").and_then(|v| v.as_str()), Some("skipped"));
    }

    // =============================================================================
    // Feishu chat-only mode tests
    // =============================================================================

    #[tokio::test]
    async fn feishu_normal_text_sets_chat_only() {
        let runtime = GatewayRuntime::new(outbound_test_config(ChannelKind::Feishu, "mock", false));
        // Normal text without slash command
        let body = r#"{"header":{"event_id":"evt_chat_001"},"event":{"sender":{"sender_type":"user","sender_id":{"open_id":"ou_user"}},"message":{"message_id":"om_chat_001","chat_id":"oc_123","message_type":"text","content":"{\"text\":\"hello world\"}"}}}"#.to_string();
        let response = super::http_channel_webhook(
            runtime,
            HeaderMap::new(),
            body,
            ChannelKind::Feishu,
        )
        .await
        .expect("chat message should succeed");
        assert_eq!(response.0.get("ok").and_then(|v| v.as_bool()), Some(true));
    }

    #[tokio::test]
    async fn feishu_run_command_sets_tool_mode() {
        let runtime = GatewayRuntime::new(outbound_test_config(ChannelKind::Feishu, "mock", false));
        // /run command
        let body = r#"{"header":{"event_id":"evt_run_001"},"event":{"sender":{"sender_type":"user","sender_id":{"open_id":"ou_user"}},"message":{"message_id":"om_run_001","chat_id":"oc_123","message_type":"text","content":"{\"text\":\"/run ls -la\"}"}}}"#.to_string();
        let response = super::http_channel_webhook(
            runtime,
            HeaderMap::new(),
            body,
            ChannelKind::Feishu,
        )
        .await
        .expect("/run command should succeed");
        assert_eq!(response.0.get("ok").and_then(|v| v.as_bool()), Some(true));
    }

    #[tokio::test]
    async fn feishu_monitor_command_sets_tool_mode() {
        let runtime = GatewayRuntime::new(outbound_test_config(ChannelKind::Feishu, "mock", false));
        // /monitor command
        let body = r#"{"header":{"event_id":"evt_mon_001"},"event":{"sender":{"sender_type":"user","sender_id":{"open_id":"ou_user"}},"message":{"message_id":"om_mon_001","chat_id":"oc_123","message_type":"text","content":"{\"text\":\"/monitor 桌面 1 分钟\"}"}}}"#.to_string();
        let response = super::http_channel_webhook(
            runtime,
            HeaderMap::new(),
            body,
            ChannelKind::Feishu,
        )
        .await
        .expect("/monitor command should succeed");
        assert_eq!(response.0.get("ok").and_then(|v| v.as_bool()), Some(true));
    }

    // =============================================================================
    // Security context chat-only tests
    // =============================================================================

    #[test]
    fn chat_only_mode_blocks_shell_tool() {
        use crate::security::context::SecurityContext;
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("chat_only".to_string(), serde_json::Value::Bool(true));
        
        let mut ctx = SecurityContext::from_config(&crate::config::Config::default());
        ctx.set_inbound_metadata(metadata);
        
        assert!(ctx.is_chat_only());
        assert!(ctx.is_tool_blocked_by_chat_only("shell"));
        assert!(ctx.is_tool_blocked_by_chat_only("bash"));
        assert!(ctx.is_tool_blocked_by_chat_only("file_write"));
        assert!(ctx.is_tool_blocked_by_chat_only("file_read"));
    }

    #[test]
    fn non_chat_only_mode_allows_tools() {
        use crate::security::context::SecurityContext;
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("chat_only".to_string(), serde_json::Value::Bool(false));
        
        let mut ctx = SecurityContext::from_config(&crate::config::Config::default());
        ctx.set_inbound_metadata(metadata);
        
        assert!(!ctx.is_chat_only());
        assert!(!ctx.is_tool_blocked_by_chat_only("shell"));
        assert!(!ctx.is_tool_blocked_by_chat_only("file_write"));
    }

    #[test]
    fn default_security_context_allows_tools() {
        use crate::security::context::SecurityContext;
        let ctx = SecurityContext::from_config(&crate::config::Config::default());

        // No chat_only metadata means tools are not blocked
        assert!(!ctx.is_chat_only());
        assert!(!ctx.is_tool_blocked_by_chat_only("shell"));
        assert!(!ctx.is_tool_blocked_by_chat_only("file_write"));
    }

    // =============================================================================
    // Feishu config preservation tests
    // =============================================================================

    #[test]
    fn feishu_extra_fields_are_preserved_in_metadata() {
        // Test that feishu extra fields (app_id, app_secret, outbound_mode) are extracted
        use crate::channels::adapters::platform_webhook::inbound_from_platform_webhook;
        use crate::channels::ChannelKind;
        use serde_json::json;

        let inbound = inbound_from_platform_webhook(
            ChannelKind::Feishu,
            json!({
                "header": {
                    "event_id": "evt_test_001"
                },
                "event": {
                    "sender": {
                        "sender_type": "user",
                        "sender_id": { "open_id": "ou_user" }
                    },
                    "message": {
                        "chat_id": "oc_chat",
                        "message_type": "text",
                        "content": "{\"text\":\"test\"}"
                    }
                }
            }),
        )
        .expect("should parse");

        // Metadata should contain raw_payload
        assert!(inbound.metadata.contains_key("raw_payload"));
        
        // session_id should be extracted from chat_id
        assert_eq!(inbound.session_id.as_deref(), Some("oc_chat"));
        
        // user_id should be extracted from sender
        assert_eq!(inbound.user_id.as_deref(), Some("ou_user"));
    }

    #[test]
    fn feishu_chat_only_mode_is_set_by_default() {
        use crate::channels::adapters::platform_webhook::inbound_from_platform_webhook;
        use crate::channels::ChannelKind;
        use serde_json::json;

        let inbound = inbound_from_platform_webhook(
            ChannelKind::Feishu,
            json!({
                "event": {
                    "sender": {
                        "sender_type": "user",
                        "sender_id": { "open_id": "ou_user" }
                    },
                    "message": {
                        "chat_id": "oc_chat",
                        "message_type": "text",
                        "content": "{\"text\":\"hello\"}"
                    }
                }
            }),
        )
        .expect("should parse");

        // Metadata should be preserved
        assert!(!inbound.metadata.is_empty());
    }

    // =============================================================================
    // Feishu chat_only policy tests
    // =============================================================================

    #[test]
    fn chat_only_system_prompt_is_returned_when_enabled() {
        use crate::security::context::SecurityContext;

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("chat_only".to_string(), serde_json::Value::Bool(true));

        let mut ctx = SecurityContext::from_config(&crate::config::Config::default());
        ctx.set_inbound_metadata(metadata);

        assert!(ctx.is_chat_only());
        let prompt = ctx.chat_only_system_prompt();
        assert!(prompt.is_some());
        let prompt = prompt.unwrap();
        // Should mention the restrictions
        assert!(prompt.contains("飞书"));
        assert!(prompt.contains("工具"));
        assert!(prompt.contains("/file"));
        assert!(prompt.contains("/run"));
        assert!(prompt.contains("/monitor"));
    }

    #[test]
    fn chat_only_system_prompt_not_returned_when_disabled() {
        use crate::security::context::SecurityContext;

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("chat_only".to_string(), serde_json::Value::Bool(false));

        let mut ctx = SecurityContext::from_config(&crate::config::Config::default());
        ctx.set_inbound_metadata(metadata);

        assert!(!ctx.is_chat_only());
        assert!(ctx.chat_only_system_prompt().is_none());
    }

    #[test]
    fn tool_intent_detected_for_file_delete() {
        use crate::security::context::SecurityContext;

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("chat_only".to_string(), serde_json::Value::Bool(true));

        let mut ctx = SecurityContext::from_config(&crate::config::Config::default());
        ctx.set_inbound_metadata(metadata);

        // Should detect file delete intent
        let intent = ctx.detect_tool_intent("删除 D:\\123\\a.txt");
        assert!(intent.is_some());
        assert_eq!(intent.unwrap(), "file_delete");
    }

    #[test]
    fn tool_intent_detected_for_shell_exec() {
        use crate::security::context::SecurityContext;

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("chat_only".to_string(), serde_json::Value::Bool(true));

        let mut ctx = SecurityContext::from_config(&crate::config::Config::default());
        ctx.set_inbound_metadata(metadata);

        // Should detect shell exec intent
        let intent = ctx.detect_tool_intent("执行 git commit");
        assert!(intent.is_some());
        assert_eq!(intent.unwrap(), "git_operation");
    }

    #[test]
    fn tool_intent_not_detected_for_normal_chat() {
        use crate::security::context::SecurityContext;

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("chat_only".to_string(), serde_json::Value::Bool(true));

        let mut ctx = SecurityContext::from_config(&crate::config::Config::default());
        ctx.set_inbound_metadata(metadata);

        // Should NOT detect tool intent for normal chat
        let intent = ctx.detect_tool_intent("你好，请帮我解释一下这段代码");
        assert!(intent.is_none());
    }

    #[test]
    fn tool_intent_not_detected_when_chat_only_disabled() {
        use crate::security::context::SecurityContext;

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("chat_only".to_string(), serde_json::Value::Bool(false));

        let mut ctx = SecurityContext::from_config(&crate::config::Config::default());
        ctx.set_inbound_metadata(metadata);

        // Should NOT detect tool intent when chat_only is disabled
        let intent = ctx.detect_tool_intent("删除 D:\\123\\a.txt");
        assert!(intent.is_none());
    }

    #[test]
    fn blocked_response_contains_guidance() {
        use crate::security::context::SecurityContext;

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("chat_only".to_string(), serde_json::Value::Bool(true));

        let mut ctx = SecurityContext::from_config(&crate::config::Config::default());
        ctx.set_inbound_metadata(metadata);

        let response = ctx.tool_intent_blocked_response();
        // Should contain guidance to use slash commands
        assert!(response.contains("/file"));
        assert!(response.contains("高风险操作"));
        assert!(response.contains("确认"));
    }

    #[test]
    fn chat_only_mode_blocks_git_tools() {
        use crate::security::context::SecurityContext;

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("chat_only".to_string(), serde_json::Value::Bool(true));

        let mut ctx = SecurityContext::from_config(&crate::config::Config::default());
        ctx.set_inbound_metadata(metadata);

        // Git tools should be blocked
        assert!(ctx.is_tool_blocked_by_chat_only("git"));
        assert!(ctx.is_tool_blocked_by_chat_only("git_clone"));
        assert!(ctx.is_tool_blocked_by_chat_only("git_commit"));
        assert!(ctx.is_tool_blocked_by_chat_only("git_push"));
        assert!(ctx.is_tool_blocked_by_chat_only("git_pull"));
    }

    #[test]
    fn tool_intent_patterns_coverage() {
        use crate::security::context::SecurityContext;

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("chat_only".to_string(), serde_json::Value::Bool(true));

        let mut ctx = SecurityContext::from_config(&crate::config::Config::default());
        ctx.set_inbound_metadata(metadata);

        // Test various tool intent patterns
        let test_cases = vec![
            ("删除文件", "file_delete"),
            ("新建文件 index.html", "file_write"),
            ("查看 D 盘", "path_access"),
            ("执行命令 ls", "shell_exec"),
            ("git push", "git_operation"),
            ("监控桌面", "desktop_monitor"),
            ("打开浏览器", "browser_automation"),
        ];

        for (text, expected_intent) in test_cases {
            let intent = ctx.detect_tool_intent(text);
            assert!(intent.is_some(), "Should detect intent for: {}", text);
            assert_eq!(intent.unwrap(), expected_intent, "Wrong intent for: {}", text);
        }
    }

    // ========== v0.8.9.1: card callback tests ==========
    use super::{
        extract_token_from_payload, payload_keys_summary, verify_feishu_verification_token,
        chrono_timestamp_simple,
    };
    use crate::gateway::feishu_worker;

    #[test]
    fn test_payload_keys_summary_parses_valid_json() {
        let raw = r#"{"event":{"action":{"value":{"action":"monitor_30s"}}},"header":{"token":"abc"}}"#;
        let keys = payload_keys_summary(raw);
        assert!(keys.contains(&"event".to_string()));
        assert!(keys.contains(&"header".to_string()));
    }

    #[test]
    fn test_payload_keys_summary_returns_empty_for_invalid_json() {
        let raw = "not json at all";
        let keys = payload_keys_summary(raw);
        assert!(keys.is_empty());
    }

    #[test]
    fn test_extract_token_from_payload_finds_token() {
        let raw = r#"{"verification_token":"my_secret_token"}"#;
        let token = extract_token_from_payload(raw);
        assert_eq!(token, Some("my_secret_token".to_string()));
    }

    #[test]
    fn test_extract_token_from_payload_finds_nested_token() {
        let raw = r#"{"payload":{"verification_token":"nested_token"}}"#;
        let token = extract_token_from_payload(raw);
        assert_eq!(token, Some("nested_token".to_string()));
    }

    #[test]
    fn test_extract_token_from_payload_returns_none_when_missing() {
        let raw = r#"{"event":{"action":{"value":{"action":"monitor_30s"}}}}"#;
        let token = extract_token_from_payload(raw);
        assert!(token.is_none());
    }

    #[test]
    fn test_verify_feishu_verification_token_dev_mode_always_true() {
        use super::{FeishuSecurityConfig, FeishuSecurityMode};
        let cfg = FeishuSecurityConfig {
            mode: FeishuSecurityMode::Dev,
            verification_token: None,
            encrypt_key: None,
            insecure: true,
        };
        let result = verify_feishu_verification_token(&cfg, Some("any_token"));
        assert_eq!(result, Ok(true));
    }

    #[test]
    fn test_verify_feishu_verification_token_token_mode_valid() {
        use super::{FeishuSecurityConfig, FeishuSecurityMode};
        let cfg = FeishuSecurityConfig {
            mode: FeishuSecurityMode::Token,
            verification_token: Some("expected_token".to_string()),
            encrypt_key: None,
            insecure: false,
        };
        let result = verify_feishu_verification_token(&cfg, Some("expected_token"));
        assert_eq!(result, Ok(true));
    }

    #[test]
    fn test_verify_feishu_verification_token_token_mode_mismatch() {
        use super::{FeishuSecurityConfig, FeishuSecurityMode};
        let cfg = FeishuSecurityConfig {
            mode: FeishuSecurityMode::Token,
            verification_token: Some("expected_token".to_string()),
            encrypt_key: None,
            insecure: false,
        };
        let result = verify_feishu_verification_token(&cfg, Some("wrong_token"));
        assert_eq!(result, Ok(false));
    }

    #[test]
    fn test_verify_feishu_verification_token_token_mode_missing() {
        use super::{FeishuSecurityConfig, FeishuSecurityMode};
        let cfg = FeishuSecurityConfig {
            mode: FeishuSecurityMode::Token,
            verification_token: Some("expected_token".to_string()),
            encrypt_key: None,
            insecure: false,
        };
        let result = verify_feishu_verification_token(&cfg, None);
        assert_eq!(result.is_err(), true);
    }

    #[test]
    fn test_gateway_status_reply_text_includes_all_fields() {
        let s = feishu_worker::gateway_status_reply(
            Some("token"),
            true,
            false,
            Some("real"),
            true,
            "C:\\Users\\Hero\\.omninova\\state.sqlite",
            5,
            2,
        );
        assert!(s.contains("Gateway 状态"));
        assert!(s.contains("security_mode"));
        assert!(s.contains("outbound_mode"));
        assert!(s.contains("store status"));
        assert!(s.contains("pending jobs"));
        assert!(s.contains("pending outbox"));
        // Must NOT contain raw secrets
        assert!(!s.contains("app_secret"));
    }

    #[test]
    fn test_chrono_timestamp_simple_returns_reasonable_value() {
        let ts = chrono_timestamp_simple();
        // Should be in milliseconds, so should be > 1 trillion
        assert!(ts > 1_000_000_000_000);
        // Should not be absurdly large
        assert!(ts < 2_000_000_000_000);
    }

    #[test]
    fn public_webhook_base_url_normalizes_event_and_card_paths() {
        assert_eq!(
            normalize_public_webhook_base_url(
                " https://example.test/webhook/feishu/card/ "
            ),
            Some("https://example.test".to_string())
        );
        assert_eq!(
            normalize_public_webhook_base_url("https://example.test/webhook/feishu"),
            Some("https://example.test".to_string())
        );
        assert_eq!(
            normalize_public_webhook_base_url(
                "https://example.test/api/v1/gateway/dingtalk/events/"
            ),
            Some("https://example.test".to_string())
        );
        assert_eq!(
            normalize_public_webhook_base_url("https://example.test/webhook/dingtalk"),
            Some("https://example.test".to_string())
        );
        assert_eq!(normalize_public_webhook_base_url("   "), None);
    }

    #[test]
    fn dingtalk_diagnostic_config_state_contains_presence_only() {
        let mut config = Config::default();
        config.gateway.dingtalk.enabled = true;
        config.gateway.dingtalk.app_key = "diagnostic-app-key".to_string();
        config.gateway.dingtalk.app_secret = "diagnostic-app-secret".to_string();
        config.gateway.dingtalk.robot_code = "diagnostic-robot-code".to_string();

        let state = dingtalk_diagnostic_config_state(&config);
        assert!(state.enabled);
        assert!(state.app_key_present);
        assert!(state.app_secret_present);
        assert!(state.robot_code_present);
        assert_eq!(state.webhook_path, "/api/v1/gateway/dingtalk/events");

        let rendered = serde_json::to_string(&state).expect("serialize diagnostics");
        assert!(!rendered.contains("diagnostic-app-key"));
        assert!(!rendered.contains("diagnostic-app-secret"));
        assert!(!rendered.contains("diagnostic-robot-code"));
    }

    #[test]
    fn feishu_diagnostic_config_state_contains_presence_only() {
        let mut config = Config::default();
        config.channels_config.feishu = Some(ChannelEntry {
            enabled: true,
            security_mode: Some("encrypted".to_string()),
            verification_token: Some("diagnostic-verification-token".to_string()),
            encrypt_key: Some("diagnostic-encrypt-key".to_string()),
            extra: HashMap::from([
                ("app_id".to_string(), json!("diagnostic-app-id")),
                ("app_secret".to_string(), json!("diagnostic-app-secret")),
                ("outbound_mode".to_string(), json!("real")),
            ]),
            ..Default::default()
        });

        let state = feishu_diagnostic_config_state(&config);
        assert!(state.enabled);
        assert!(state.app_id_present);
        assert!(state.app_secret_present);
        assert!(state.verification_token_present);
        assert!(state.encrypt_key_present);
        assert_eq!(state.security_mode, "encrypted");
        assert_eq!(state.outbound_mode, "real");

        let rendered = serde_json::to_string(&state).expect("serialize diagnostics");
        for secret in [
            "diagnostic-app-id",
            "diagnostic-app-secret",
            "diagnostic-verification-token",
            "diagnostic-encrypt-key",
        ] {
            assert!(!rendered.contains(secret));
        }
    }

    #[test]
    fn dingtalk_route_probe_classifies_expected_unsigned_responses() {
        let missing = classify_dingtalk_route_response(401, r#"{"message":"missing_timestamp"}"#);
        assert!(missing.reachable);
        assert_eq!(missing.result_kind, "missing_timestamp");

        let mismatch = classify_dingtalk_route_response(403, r#"{"message":"signature_mismatch"}"#);
        assert!(mismatch.reachable);
        assert_eq!(mismatch.result_kind, "signature_mismatch");

        let tunnel = classify_dingtalk_route_response(530, "Cloudflare error 1033");
        assert!(!tunnel.reachable);
        assert_eq!(tunnel.result_kind, "tunnel_unreachable");
    }

    #[tokio::test]
    async fn dingtalk_route_probe_reports_unconfigured_without_network_access() {
        let result = check_dingtalk_public_route("   ").await;
        assert!(!result.configured);
        assert!(!result.reachable);
        assert_eq!(result.status_code, None);
        assert_eq!(result.result_kind, "not_configured");
    }

    #[test]
    fn external_public_url_generates_both_feishu_callbacks() {
        let mut config = Config::default();
        config.gateway_public = GatewayPublicConfig {
            mode: GatewayPublicMode::ExternalPublicUrl,
            public_webhook_base_url: Some(
                "https://gateway.example.test/webhook/feishu/card/".to_string(),
            ),
            ..GatewayPublicConfig::default()
        };

        assert_eq!(
            resolve_public_webhook_base_url(&config).as_deref(),
            Some("https://gateway.example.test")
        );
        let (event_url, card_url) = feishu_public_callback_urls(&config);
        assert_eq!(
            event_url.as_deref(),
            Some("https://gateway.example.test/webhook/feishu")
        );
        assert_eq!(
            card_url.as_deref(),
            Some("https://gateway.example.test/webhook/feishu/card")
        );
    }

    #[test]
    fn feishu_callback_urls_handle_trailing_slash_and_unconfigured_base() {
        let empty = Config::default();
        assert_eq!(feishu_public_callback_urls(&empty), (None, None));

        let mut configured = Config::default();
        configured.gateway_public = GatewayPublicConfig {
            mode: GatewayPublicMode::ExternalPublicUrl,
            public_webhook_base_url: Some("https://gateway.example.test/".to_string()),
            ..GatewayPublicConfig::default()
        };
        let (event_url, card_url) = feishu_public_callback_urls(&configured);
        assert_eq!(
            event_url.as_deref(),
            Some("https://gateway.example.test/webhook/feishu")
        );
        assert_eq!(
            card_url.as_deref(),
            Some("https://gateway.example.test/webhook/feishu/card")
        );
    }

    #[test]
    fn named_tunnel_hostname_is_used_when_public_base_is_empty() {
        let mut config = Config::default();
        config.gateway_public = GatewayPublicConfig {
            mode: GatewayPublicMode::NamedCloudflareTunnel,
            named_tunnel_hostname: Some("gateway.example.test".to_string()),
            ..GatewayPublicConfig::default()
        };

        assert_eq!(
            resolve_public_webhook_base_url(&config).as_deref(),
            Some("https://gateway.example.test")
        );
    }

    #[test]
    fn named_tunnel_hostname_normalizes_callback_urls_and_sensitive_suffixes() {
        assert_eq!(
            normalize_named_tunnel_hostname(
                "https://Gateway.Example.Test/webhook/feishu/card?signature=hidden"
            )
            .as_deref(),
            Some("gateway.example.test")
        );
        assert_eq!(
            normalize_named_tunnel_hostname(
                "gateway.example.test/webhook/feishu#callback"
            )
            .as_deref(),
            Some("gateway.example.test")
        );
        assert_eq!(
            normalize_public_webhook_base_url(
                "https://gateway.example.test/webhook/feishu?signature=hidden"
            )
            .as_deref(),
            Some("https://gateway.example.test")
        );
        assert_eq!(
            normalize_public_webhook_base_url(
                "https://user:password@gateway.example.test/webhook/feishu"
            ),
            None
        );
    }

    #[test]
    fn named_tunnel_normalization_persists_generated_public_base() {
        let mut public = GatewayPublicConfig {
            mode: GatewayPublicMode::NamedCloudflareTunnel,
            public_webhook_base_url: Some("https://stale.trycloudflare.com".to_string()),
            named_tunnel_name: Some("  omninova-gateway  ".to_string()),
            named_tunnel_hostname: Some(
                "https://Fixed.Example.Test/webhook/feishu/card".to_string(),
            ),
            ..GatewayPublicConfig::default()
        };

        normalize_gateway_public_config(&mut public);

        assert_eq!(public.named_tunnel_name.as_deref(), Some("omninova-gateway"));
        assert_eq!(
            public.named_tunnel_hostname.as_deref(),
            Some("fixed.example.test")
        );
        assert_eq!(
            public.public_webhook_base_url.as_deref(),
            Some("https://fixed.example.test")
        );
    }

    #[test]
    fn non_named_public_modes_keep_their_configured_base() {
        for mode in [
            GatewayPublicMode::QuickTunnel,
            GatewayPublicMode::ExternalPublicUrl,
        ] {
            let mut public = GatewayPublicConfig {
                mode,
                public_webhook_base_url: Some(
                    "https://public.example.test/webhook/feishu/card".to_string(),
                ),
                named_tunnel_hostname: Some("ignored.example.test".to_string()),
                ..GatewayPublicConfig::default()
            };
            normalize_gateway_public_config(&mut public);
            assert_eq!(
                public.public_webhook_base_url.as_deref(),
                Some("https://public.example.test")
            );
        }
    }

    #[tokio::test]
    async fn public_health_without_url_returns_structured_not_configured_status() {
        let status = check_gateway_public_health(&Config::default()).await;
        assert!(!status.configured);
        assert!(!status.ok);
        assert_eq!(status.error_kind.as_deref(), Some("url_not_configured"));
    }

    #[tokio::test]
    async fn public_health_connection_failure_is_structured() {
        // Use a reserved RFC-5737 / TEST-NET IP + port 1 to make sure the OS
        // can't accidentally resolve a real service. 192.0.2.0/24 (TEST-NET-1)
        // and 198.51.100.0/24 (TEST-NET-2) are guaranteed unreachable for
        // documentation purposes per RFC 6761.
        let mut config = Config::default();
        config.gateway_public.public_webhook_base_url =
            Some("http://192.0.2.1:1".to_string());
        let status = check_gateway_public_health(&config).await;

        assert!(status.configured);
        assert!(!status.ok);
        // The OS-platform connection-error classifier is intentionally broad:
        // any of these "connectivity family" kinds is acceptable, since
        // reqwest + rustls + Windows can label the same event as
        // connection_error, request_error, timeout, dns_error, or tls_error
        // depending on the host resolver, the IPv4/IPv6 stack state, and
        // registry-level TLS settings. The structural invariant we DO check
        // is that the failure is classified into a recognized category,
        // not silently dropped into `None`.
        let kind = status.error_kind.as_deref().unwrap_or_default();
        assert!(
            matches!(
                kind,
                "connection_error"
                    | "request_error"
                    | "timeout"
                    | "dns_error"
                    | "tls_error"
            ),
            "unexpected error_kind from unreachable host: {kind}"
        );
        assert!(!status.error.as_deref().unwrap_or_default().is_empty());
    }

    /// Run twice to confirm this test is stable across runs.
    #[tokio::test]
    async fn public_health_connection_failure_is_structured_twice() {
        // Same logic as the sibling test, but exercises the classifier twice
        // back-to-back so a flaky OS error classification cannot pass.
        let mut config = Config::default();
        config.gateway_public.public_webhook_base_url =
            Some("http://198.51.100.1:1".to_string());
        let status1 = check_gateway_public_health(&config).await;
        let status2 = check_gateway_public_health(&config).await;
        assert!(!status1.ok);
        assert!(!status2.ok);
        let kind1 = status1.error_kind.as_deref().unwrap_or_default();
        let kind2 = status2.error_kind.as_deref().unwrap_or_default();
        for kind in [kind1, kind2] {
            assert!(
                matches!(
                    kind,
                    "connection_error"
                        | "request_error"
                        | "timeout"
                        | "dns_error"
                        | "tls_error"
                ),
                "unexpected error_kind from unreachable host: {kind}"
            );
        }
    }

    #[tokio::test]
    async fn quick_tunnel_status_is_marked_non_production() {
        let mut config = Config::default();
        config.gateway_public.mode = GatewayPublicMode::QuickTunnel;
        config.gateway_public.cloudflared_path =
            Some(PathBuf::from("C:\\tools\\cloudflared.exe"));
        let runtime = GatewayRuntime::new(config);
        let status = GatewayRuntimeStatus::from_runtime(false, &runtime, None, None).await;

        assert_eq!(status.gateway_public_mode, "quick_tunnel");
        assert!(status.quick_tunnel_non_production);
        assert!(status.cloudflared_configured);
    }

    #[tokio::test]
    async fn gateway_runtime_status_is_stopped_and_never_serializes_secrets() {
        let mut config = make_config_with_channel(ChannelKind::Feishu, true);
        let feishu = config.channels_config.feishu.as_mut().unwrap();
        feishu.extra.insert(
            "security_mode".to_string(),
            json!("token"),
        );
        feishu.extra.insert(
            "verification_token".to_string(),
            json!("verification-token-must-not-leak"),
        );
        feishu.extra.insert(
            "app_secret".to_string(),
            json!("app-secret-must-not-leak"),
        );
        feishu.extra.insert(
            "public_webhook_base_url".to_string(),
            json!("https://example.test/webhook/feishu/card"),
        );
        let runtime = GatewayRuntime::new(config);
        let status =
            GatewayRuntimeStatus::from_runtime(false, &runtime, None, None).await;
        let serialized = serde_json::to_string(&status).unwrap();

        assert!(!status.running);
        assert!(!status.health_ok);
        assert!(!status.store_opened);
        assert_eq!(
            status.public_webhook_base_url.as_deref(),
            Some("https://example.test")
        );
        assert_eq!(
            status.feishu_card_callback_url.as_deref(),
            Some("https://example.test/webhook/feishu/card")
        );
        assert!(!serialized.contains("verification-token-must-not-leak"));
        assert!(!serialized.contains("app-secret-must-not-leak"));
    }
}
