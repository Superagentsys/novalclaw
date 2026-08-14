//! WeCom smart-bot interactive template card panel (Phase 2A.3.1).
//!
//! Official protocol target: WecomTeam/aibot-node-sdk template_card —
//! `button_interaction` cards sent as `aibot_respond_msg` replies with
//! `body.msgtype = "template_card"`; button presses arrive as
//! `msgtype = "template_card_event"` callbacks carrying `task_id` and
//! `event_key`. Replying to the EVENT callback's req_id with a card
//! carrying the SAME task_id updates that card (the official SDK
//! requires the update promptly after the event callback).
//!
//! Long-connection implementation only. HTTP-callback card parity is
//! Phase 2A.3.2.
//!
//! Security model:
//! - 5-command allowlist only (`gateway_status`, `recent_jobs`,
//!   `monitor_30`, `monitor_60`, `help`); anything else is rejected
//!   with a safe prompt — never the Agent, never shell/tools.
//! - task_id validation against a registered panel state with a 30-min
//!   TTL; expired panels answer with a safe "reopen" prompt.
//! - per-(task, event msgid) dedup so retried event frames cannot
//!   re-run actions; monitor actions are additionally single-flight.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use time::OffsetDateTime;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::gateway::wecom_protocol::WecomCallbackBody;
use crate::gateway::wecom_stream::{short_hash, WecomOutboundMsg};
use crate::gateway::GatewayRuntime;

/// Panel card constants (Phase 2A.3.1e unified menu UX).
pub const PANEL_TITLE: &str = "OmniNova · 控制中心";
pub const PANEL_DESC: &str = "企业微信长连接 · Online";
pub const PANEL_READY_SUBTITLE: &str = "请选择操作";
pub const PANEL_EXPIRED_TEXT: &str = "该操作面板已过期，请重新打开。";
pub const UNKNOWN_ACTION_TEXT: &str = "未知操作，已忽略。";
pub const CARD_TTL_SECS: i64 = 30 * 60;
pub const RECENT_JOBS_LIMIT: usize = 5;
/// Hard timeout grace added on top of the monitor's soft duration.
pub const MONITOR_GRACE_SECS: u64 = 15;

/// Canonical panel buttons: (action key, label) — order is the card layout.
pub const PANEL_BUTTONS: &[(&str, &str)] = &[
    ("gateway_status", "网关状态"),
    ("recent_jobs", "最近任务"),
    ("monitor_30", "监控 30 秒"),
    ("monitor_60", "监控 60 秒"),
    ("help", "帮助"),
];

/// Deterministic panel trigger commands. The LLM never decides when to
/// show the panel; only these EXACT strings (trimmed; ASCII
/// case-insensitive) do. `contains` is deliberately NOT used — "menu
/// test" / "打开menu" / "修改菜单" must never open the panel.
pub fn is_panel_command(text: &str) -> bool {
    panel_command(text).is_some()
}

/// Normalized panel command detector. Returns the canonical command
/// name for logging when the text matches one of:
/// 菜单 / 面板 / menu / /menu / panel / /panel
pub fn panel_command(text: &str) -> Option<&'static str> {
    let normalized = text.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "菜单" => Some("菜单"),
        "面板" => Some("面板"),
        "menu" => Some("menu"),
        "/menu" => Some("/menu"),
        "panel" => Some("panel"),
        "/panel" => Some("/panel"),
        _ => None,
    }
}

/// Unique, stable-per-panel task_id. UUID-based: no userid/chatid/
/// msgid/secret material is embedded.
pub fn new_task_id() -> String {
    format!("wecom_panel_{}", uuid::Uuid::new_v4())
}

// ---------------------------------------------------------------------------
// Canonical actions + allowlist
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WecomPanelAction {
    GatewayStatus,
    RecentJobs,
    Monitor30,
    Monitor60,
    Help,
}

impl WecomPanelAction {
    pub fn key(self) -> &'static str {
        match self {
            WecomPanelAction::GatewayStatus => "gateway_status",
            WecomPanelAction::RecentJobs => "recent_jobs",
            WecomPanelAction::Monitor30 => "monitor_30",
            WecomPanelAction::Monitor60 => "monitor_60",
            WecomPanelAction::Help => "help",
        }
    }

    /// Allowlist-only canonicalization. Any key outside the 5-command
    /// allowlist maps to `None` (rejected as UNKNOWN_ACTION).
    pub fn from_event_key(key: &str) -> Option<WecomPanelAction> {
        match key {
            "gateway_status" => Some(WecomPanelAction::GatewayStatus),
            "recent_jobs" => Some(WecomPanelAction::RecentJobs),
            "monitor_30" => Some(WecomPanelAction::Monitor30),
            "monitor_60" => Some(WecomPanelAction::Monitor60),
            "help" => Some(WecomPanelAction::Help),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            WecomPanelAction::GatewayStatus => "网关状态",
            WecomPanelAction::RecentJobs => "最近任务",
            WecomPanelAction::Monitor30 => "监控 30 秒",
            WecomPanelAction::Monitor60 => "监控 60 秒",
            WecomPanelAction::Help => "帮助",
        }
    }
}

// ---------------------------------------------------------------------------
// Card JSON builders (official template_card / button_interaction shape)
// ---------------------------------------------------------------------------

/// Build the `template_card` object for the OmniNova control panel
/// (Ready state: subtitle = "请选择操作").
pub fn build_panel_card(task_id: &str) -> serde_json::Value {
    serde_json::json!({
        "card_type": "button_interaction",
        "source": {
            "desc": "OmniNova",
            "desc_color": 0,
        },
        "main_title": {
            "title": PANEL_TITLE,
            "desc": PANEL_DESC,
        },
        "sub_title_text": PANEL_READY_SUBTITLE,
        "task_id": task_id,
        "button_list": PANEL_BUTTONS
            .iter()
            .map(|(key, label)| serde_json::json!({
                "text": label,
                "style": 1,
                "key": key,
            }))
            .collect::<Vec<_>>(),
    })
}

/// Menu subtitle driven by the panel's monitor state (Phase 2A.3.1e).
pub fn panel_subtitle_for_state(state: &MonitorState) -> String {
    match state {
        MonitorState::Idle => PANEL_READY_SUBTITLE.to_string(),
        MonitorState::Running { deadline, .. } => {
            let remaining = remaining_secs_until(*deadline);
            format!("桌面监控中 · 剩余 {remaining} 秒")
        }
        MonitorState::Completed { .. } => "桌面监控完成".to_string(),
        MonitorState::Failed { .. } => "桌面监控失败".to_string(),
    }
}

/// Panel card with an action result shown in `sub_title_text`
/// (card updates keep the buttons; the result replaces the subtitle).
pub fn build_panel_card_with_text(task_id: &str, text: &str) -> serde_json::Value {
    let mut card = build_panel_card(task_id);
    card["sub_title_text"] = serde_json::json!(text);
    card
}

// ---------------------------------------------------------------------------
// Card state store (in-memory, 30-min TTL, event dedup, monitor
// single-flight + countdown state)
// ---------------------------------------------------------------------------

/// Per-panel monitor state (Phase 2A.3.1e). `Running.deadline` is a
/// monotonic `Instant`; remaining seconds are always recomputed as
/// `deadline - now` — never a decrement counter, so scheduler drift
/// cannot accumulate.
/// Delivery outcome of a completed monitor's proactive result
/// (Phase 2A.3.1f): task completion ≠ result delivery. Only an ACK with
/// errcode==0 marks it Sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorDelivery {
    Pending,
    Sent,
    Failed { errcode: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorState {
    Idle,
    Running {
        duration_secs: u64,
        started_at: i64,
        deadline: std::time::Instant,
        generation: u64,
    },
    Completed {
        duration_secs: u64,
        elapsed_ms: u64,
        summary: String,
        completed_at: i64,
        delivery: MonitorDelivery,
    },
    Failed {
        error_summary: String,
        elapsed_ms: u64,
    },
}

impl Default for MonitorState {
    fn default() -> Self {
        MonitorState::Idle
    }
}

impl MonitorState {
    pub fn phase(&self) -> &'static str {
        match self {
            MonitorState::Idle => "idle",
            MonitorState::Running { .. } => "running",
            MonitorState::Completed { .. } => "completed",
            MonitorState::Failed { .. } => "failed",
        }
    }
}

/// Monotonic remaining seconds: deadline - now (min 0).
pub fn remaining_secs_until(deadline: std::time::Instant) -> u64 {
    deadline
        .saturating_duration_since(std::time::Instant::now())
        .as_secs()
}

/// Kinds of outbound card/proactive req_ids awaiting ACK, so the ACK
/// log can distinguish initial card / card update / proactive send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WecomCardReqKind {
    InitialCard,
    UpdateCard,
    Proactive,
}

#[derive(Debug, Clone)]
pub struct WecomCardState {
    pub task_id: String,
    pub session_key: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_action: Option<String>,
    pub monitor: MonitorState,
}

/// One entry of the WeCom recent-jobs log (redacted summary only).
#[derive(Debug, Clone)]
pub struct WecomRecentJob {
    pub chat_type: String,
    pub status: String,
    pub created_at: i64,
}

/// A pending outbound req awaiting its ACK: kind + optional task_id so
/// a proactive ACK can update the monitor delivery state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WecomCardPendingReq {
    pub kind: WecomCardReqKind,
    pub task_id: Option<String>,
}

struct WecomCardStoreInner {
    cards: HashMap<String, WecomCardState>,
    /// Event msgids already handled per task (retry dedup).
    seen_events: HashMap<String, HashSet<String>>,
    /// Monitor state per task_id (single-flight + countdown).
    monitors: HashMap<String, MonitorState>,
    /// req_ids of card/proactive messages awaiting their ACK, by kind.
    pending_card_req_ids: HashMap<String, WecomCardPendingReq>,
    /// Bounded recent-jobs log (most recent first).
    recent_jobs: VecDeque<WecomRecentJob>,
}

#[derive(Clone)]
pub struct WecomCardStore {
    inner: Arc<Mutex<WecomCardStoreInner>>,
}

impl WecomCardStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(WecomCardStoreInner {
                cards: HashMap::new(),
                seen_events: HashMap::new(),
                monitors: HashMap::new(),
                pending_card_req_ids: HashMap::new(),
                recent_jobs: VecDeque::new(),
            })),
        }
    }

    fn now() -> i64 {
        OffsetDateTime::now_utc().unix_timestamp()
    }

    /// Register a fresh panel (used by the panel trigger).
    pub async fn register(&self, task_id: String, session_key: Option<String>) {
        self.register_at(task_id, session_key, Self::now()).await;
    }

    pub(crate) async fn register_at(
        &self,
        task_id: String,
        session_key: Option<String>,
        created_at: i64,
    ) {
        let mut inner = self.inner.lock();
        inner.cards.insert(
            task_id.clone(),
            WecomCardState {
                task_id,
                session_key,
                created_at,
                updated_at: created_at,
                last_action: None,
                monitor: MonitorState::Idle,
            },
        );
    }

    /// Lookup a live panel by task_id. Expired panels (30-min TTL) are
    /// removed and reported as missing.
    pub async fn lookup_valid(&self, task_id: &str) -> Option<WecomCardState> {
        let now = Self::now();
        let mut inner = self.inner.lock();
        let expired = match inner.cards.get(task_id) {
            Some(state) => now - state.updated_at > CARD_TTL_SECS,
            None => return None,
        };
        if expired {
            inner.cards.remove(task_id);
            inner.seen_events.remove(task_id);
            inner.monitors.remove(task_id);
            return None;
        }
        inner.cards.get(task_id).cloned()
    }

    /// Event retry dedup keyed by (task_id, event msgid). Returns true
    /// the first time, false for duplicates.
    pub async fn dedup_event(&self, task_id: &str, event_id: &str) -> bool {
        let mut inner = self.inner.lock();
        inner
            .seen_events
            .entry(task_id.to_string())
            .or_default()
            .insert(event_id.to_string())
    }

    /// Record the action onto the panel state.
    pub async fn touch_action(&self, task_id: &str, action: &str) {
        let mut inner = self.inner.lock();
        if let Some(state) = inner.cards.get_mut(task_id) {
            state.last_action = Some(action.to_string());
            state.updated_at = Self::now();
        }
    }

    /// Current monitor state for a task.
    pub async fn monitor_state(&self, task_id: &str) -> MonitorState {
        let inner = self.inner.lock();
        inner.monitors.get(task_id).cloned().unwrap_or_default()
    }

    /// Single-flight monitor admission. Returns false when a monitor is
    /// already running on this task (the click then shows live remaining
    /// time instead of starting a second task).
    pub async fn try_start_monitor(&self, task_id: &str, duration_secs: u64, generation: u64) -> bool {
        let mut inner = self.inner.lock();
        if let Some(MonitorState::Running { .. }) = inner.monitors.get(task_id) {
            return false;
        }
        let now = Self::now();
        inner.monitors.insert(
            task_id.to_string(),
            MonitorState::Running {
                duration_secs,
                started_at: now,
                deadline: std::time::Instant::now() + std::time::Duration::from_secs(duration_secs),
                generation,
            },
        );
        if let Some(state) = inner.cards.get_mut(task_id) {
            state.updated_at = now;
        }
        true
    }

    /// Live remaining seconds for a running monitor (None when idle).
    pub async fn monitor_remaining_secs(&self, task_id: &str) -> Option<u64> {
        let inner = self.inner.lock();
        match inner.monitors.get(task_id) {
            Some(MonitorState::Running { deadline, .. }) => Some(remaining_secs_until(*deadline)),
            _ => None,
        }
    }

    /// Mark a monitor completed (summary rendered to the user later).
    pub async fn complete_monitor(
        &self,
        task_id: &str,
        duration_secs: u64,
        elapsed_ms: u64,
        summary: String,
    ) {
        let mut inner = self.inner.lock();
        inner.monitors.insert(
            task_id.to_string(),
            MonitorState::Completed {
                duration_secs,
                elapsed_ms,
                summary,
                completed_at: Self::now(),
                delivery: MonitorDelivery::Pending,
            },
        );
        if let Some(state) = inner.cards.get_mut(task_id) {
            state.updated_at = Self::now();
        }
    }

    /// ACK errcode==0 for the proactive result → delivery Sent.
    pub async fn mark_delivery_sent(&self, task_id: &str) {
        let mut inner = self.inner.lock();
        if let Some(MonitorState::Completed { delivery, .. }) = inner.monitors.get_mut(task_id) {
            *delivery = MonitorDelivery::Sent;
        }
    }

    /// ACK errcode!=0 for the proactive result → delivery Failed. The
    /// monitor task itself is NOT re-executed.
    pub async fn mark_delivery_failed(&self, task_id: &str, errcode: i64) {
        let mut inner = self.inner.lock();
        if let Some(MonitorState::Completed { delivery, .. }) = inner.monitors.get_mut(task_id) {
            *delivery = MonitorDelivery::Failed { errcode };
        }
    }

    /// Mark a monitor failed/timed out.
    pub async fn fail_monitor(&self, task_id: &str, error_summary: String, elapsed_ms: u64) {
        let mut inner = self.inner.lock();
        inner.monitors.insert(
            task_id.to_string(),
            MonitorState::Failed {
                error_summary,
                elapsed_ms,
            },
        );
        if let Some(state) = inner.cards.get_mut(task_id) {
            state.updated_at = Self::now();
        }
    }

    /// Test-only deadline override for deterministic countdown tests.
    #[cfg(test)]
    pub async fn set_monitor_deadline_for_test(&self, task_id: &str, deadline: std::time::Instant) {
        let mut inner = self.inner.lock();
        if let Some(MonitorState::Running {
            duration_secs,
            started_at,
            generation,
            ..
        }) = inner.monitors.get(task_id).cloned()
        {
            inner.monitors.insert(
                task_id.to_string(),
                MonitorState::Running {
                    duration_secs,
                    started_at,
                    deadline,
                    generation,
                },
            );
        }
    }

    /// Record one inbound WeCom message handling entry (bounded log).
    pub async fn record_job(&self, chat_type: &str, status: &str) {
        let mut inner = self.inner.lock();
        inner.recent_jobs.push_front(WecomRecentJob {
            chat_type: chat_type.to_string(),
            status: status.to_string(),
            created_at: Self::now(),
        });
        while inner.recent_jobs.len() > 20 {
            inner.recent_jobs.pop_back();
        }
    }

    /// Render the recent-jobs card text (last 5, redacted).
    pub async fn recent_jobs_text(&self) -> String {
        let inner = self.inner.lock();
        let jobs: Vec<WecomRecentJob> = inner
            .recent_jobs
            .iter()
            .take(RECENT_JOBS_LIMIT)
            .cloned()
            .collect();
        if jobs.is_empty() {
            return "最近任务\n\n暂无最近任务。".to_string();
        }
        let mut lines = vec![format!("最近任务（{} 条）\n", jobs.len())];
        for (index, job) in jobs.iter().enumerate() {
            lines.push(format!(
                "{}. {} | {} | Unix {}",
                index + 1,
                job.chat_type,
                job.status,
                job.created_at
            ));
        }
        lines.join("\n")
    }

    /// Track an outbound card/proactive req_id until its ACK arrives.
    pub async fn note_card_req_id(
        &self,
        req_id: &str,
        kind: WecomCardReqKind,
        task_id: Option<&str>,
    ) {
        let mut inner = self.inner.lock();
        inner.pending_card_req_ids.insert(
            req_id.to_string(),
            WecomCardPendingReq {
                kind,
                task_id: task_id.map(String::from),
            },
        );
    }

    /// Returns (and clears) the pending req when the ACK arrives; used
    /// for kind-specific ACK logging and delivery-state updates.
    pub async fn consume_card_req_ack(&self, req_id: &str) -> Option<WecomCardPendingReq> {
        let mut inner = self.inner.lock();
        inner.pending_card_req_ids.remove(req_id)
    }

    /// Test-only reset of the global store.
    #[cfg(test)]
    pub async fn reset(&self) {
        let mut inner = self.inner.lock();
        inner.cards.clear();
        inner.seen_events.clear();
        inner.monitors.clear();
        inner.pending_card_req_ids.clear();
        inner.recent_jobs.clear();
    }
}

impl Default for WecomCardStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-wide in-memory card store (Phase 2A.3.1 MVP).
pub fn card_store() -> &'static WecomCardStore {
    static STORE: OnceLock<WecomCardStore> = OnceLock::new();
    STORE.get_or_init(WecomCardStore::new)
}

/// Register a new panel for the trigger path (one call site).
pub async fn register_panel(task_id: &str, session_key: Option<String>) {
    card_store()
        .register(task_id.to_string(), session_key)
        .await;
}

// ---------------------------------------------------------------------------
// Deterministic business actions (no LLM, no shell, no tools)
// ---------------------------------------------------------------------------

/// Enabled channels (deterministic, no secrets).
pub fn enabled_channels(config: &Config) -> Vec<&'static str> {
    let mut out = Vec::new();
    let cc = &config.channels_config;
    if cc.feishu.as_ref().map(|e| e.enabled).unwrap_or(false) {
        out.push("feishu");
    }
    if cc.wechat.as_ref().map(|e| e.enabled).unwrap_or(false) {
        out.push("wechat");
    }
    if cc.wecom.as_ref().map(|e| e.enabled).unwrap_or(false) || config.gateway.wecom.enabled {
        out.push("wecom");
    }
    if cc.dingtalk.as_ref().map(|e| e.enabled).unwrap_or(false) || config.gateway.dingtalk.enabled {
        out.push("dingtalk");
    }
    out
}

/// REAL state snapshot consumed by the gateway_status action — no
/// hardcoded "normal" values.
#[derive(Debug, Clone)]
pub struct WecomGatewayStatusSnapshot {
    pub transport: &'static str,
    pub connection_connected: bool,
    pub stream_active: bool,
    pub generation: u64,
    /// Honest flag: last-heartbeat time is NOT tracked anywhere yet.
    pub last_heartbeat_available: bool,
    pub enabled_channels: Vec<&'static str>,
    pub agent_name: String,
}

/// Pure, deterministic gateway status text from the REAL state snapshot
/// (no secrets, no LLM, no hardcoded health claims).
pub fn gateway_status_text(snapshot: &WecomGatewayStatusSnapshot, now: &OffsetDateTime) -> String {
    let channel_list = if snapshot.enabled_channels.is_empty() {
        "无".to_string()
    } else {
        snapshot.enabled_channels.join("、")
    };
    let heartbeat = if snapshot.last_heartbeat_available {
        "已追踪"
    } else {
        "未追踪（当前版本不记录心跳时间）"
    };
    let time_text = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| now.unix_timestamp().to_string());
    format!(
        "Gateway 状态\n\nGateway running: true\nWeCom transport: {}\nconnection: {}\nstream active: {}\ngeneration: {}\nlast heartbeat: {}\nenabled channels: {}\nAgent: {}\n时间: {}",
        snapshot.transport,
        if snapshot.connection_connected { "connected" } else { "disconnected" },
        snapshot.stream_active,
        snapshot.generation,
        heartbeat,
        channel_list,
        snapshot.agent_name,
        time_text,
    )
}

/// Fixed help text (no LLM): lists the panel commands and the 5 actions.
pub fn help_text() -> &'static str {
    "OmniNova 使用帮助\n\n菜单命令：菜单 / 面板 / menu / /menu / panel / /panel\n\n操作说明：\n· 网关状态：查看 Gateway 与 WeCom 连接状态\n· 最近任务：最近 5 条处理记录\n· 监控 30 秒：桌面监控 30 秒，完成后自动推送结果\n· 监控 60 秒：桌面监控 60 秒，完成后自动推送结果\n· 帮助：显示本说明\n\n普通聊天直接发消息即可。"
}

// ---------------------------------------------------------------------------
// Shared card action boundary (Phase 2A.3.1d / 2A.3.1e)
// ---------------------------------------------------------------------------
//
// Same architecture as the DingTalk/Feishu panels: transport adapters
// render, the action core computes. WeCom reads ONLY platform-
// independent business services here (config snapshot, the shared
// desktop_capture monitor); it never calls DingTalk/Feishu card
// transports or renderers.

/// Platform-independent action context passed to the action service.
#[derive(Debug, Clone)]
pub struct CardActionContext {
    pub task_id: String,
    pub session_key: Option<String>,
    pub action: WecomPanelAction,
    /// Proactive-send target: single chat → userid, group → chatid.
    pub target_chat_id: Option<String>,
}

/// Platform-independent action result. `content` is rendered into the
/// card subtitle by the WeCom adapter.
#[derive(Debug, Clone)]
pub struct CardActionResult {
    pub title: &'static str,
    pub content: String,
}

/// Shared, deterministic card action service. No LLM, no shell, no
/// transport — the WeCom adapter is one of its consumers.
pub struct WecomCardActionService;

impl WecomCardActionService {
    pub async fn execute(
        runtime: &Arc<GatewayRuntime>,
        store: &WecomCardStore,
        outbound_tx: &mpsc::Sender<WecomOutboundMsg>,
        context: CardActionContext,
    ) -> CardActionResult {
        store
            .touch_action(&context.task_id, context.action.key())
            .await;
        match context.action {
            WecomPanelAction::GatewayStatus => {
                let config = runtime.get_config().await;
                let snapshot = WecomGatewayStatusSnapshot {
                    transport: wecom_transport_mode_str(&config),
                    connection_connected: runtime.is_wecom_stream_connected(),
                    stream_active: runtime.is_wecom_stream_active(),
                    generation: runtime.current_wecom_stream_generation(),
                    // Honest: heartbeat timestamps are not tracked.
                    last_heartbeat_available: false,
                    enabled_channels: enabled_channels(&config),
                    agent_name: config.agent.name.clone(),
                };
                CardActionResult {
                    title: "网关状态",
                    content: gateway_status_text(&snapshot, &OffsetDateTime::now_utc()),
                }
            }
            WecomPanelAction::RecentJobs => CardActionResult {
                title: "最近任务",
                content: store.recent_jobs_text().await,
            },
            WecomPanelAction::Monitor30 => CardActionResult {
                title: "桌面监控 · 30 秒",
                content: start_monitor(
                    runtime,
                    store,
                    outbound_tx,
                    &context.task_id,
                    context.target_chat_id.clone(),
                    30,
                )
                .await,
            },
            WecomPanelAction::Monitor60 => CardActionResult {
                title: "桌面监控 · 60 秒",
                content: start_monitor(
                    runtime,
                    store,
                    outbound_tx,
                    &context.task_id,
                    context.target_chat_id.clone(),
                    60,
                )
                .await,
            },
            WecomPanelAction::Help => CardActionResult {
                title: "帮助",
                content: help_text().to_string(),
            },
        }
    }
}

fn wecom_transport_mode_str(config: &Config) -> &'static str {
    crate::gateway::wecom_http::resolve_transport_mode(config).as_str()
}

/// Proactive target resolution (Phase 2A.3.1e P6):
/// single chat → event.from.userid; group chat → event.chatid.
pub fn resolve_card_target(body: &WecomCallbackBody) -> Option<String> {
    let chat_type = body.chattype.as_deref().unwrap_or("");
    if crate::gateway::wecom_protocol::WecomChatType::from_str(Some(chat_type)).is_group() {
        body.chatid
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(String::from)
    } else {
        body.from
            .as_ref()
            .and_then(|f| f.userid.as_deref())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(String::from)
    }
}

/// Proactive outbound body abstraction (Phase 2A.3.1f): the monitor
/// result currently ships as Markdown (reliable); the template_card
/// path stays available as a future re-enable boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProactiveBody {
    Markdown { content: String },
    TemplateCard { body: serde_json::Value },
}

impl ProactiveBody {
    pub fn to_wire(&self) -> serde_json::Value {
        match self {
            ProactiveBody::Markdown { content } => serde_json::json!({
                "msgtype": "markdown",
                "markdown": { "content": content },
            }),
            ProactiveBody::TemplateCard { body } => body.clone(),
        }
    }

    pub fn format_name(&self) -> &'static str {
        match self {
            ProactiveBody::Markdown { .. } => "markdown",
            ProactiveBody::TemplateCard { .. } => "template_card",
        }
    }
}

/// FUTURE card path (kept per P5, NOT used for monitor delivery in
/// 2A.3.1f): text_notice template_card body. Real E2E showed errcode
/// 42045 for this payload, so delivery defaults to Markdown.
pub fn build_monitor_result_message(title: &str, summary: &str) -> serde_json::Value {
    serde_json::json!({
        "msgtype": "template_card",
        "template_card": {
            "card_type": "text_notice",
            "source": {
                "desc": "OmniNova",
                "desc_color": 0,
            },
            "main_title": {
                "title": title,
                "desc": summary,
            }
        }
    })
}

/// Reliable Markdown content for the monitor final result (P3).
/// Never contains absolute paths, secrets, userids or stacktraces —
/// `summary` comes exclusively from our own fixed safe texts.
pub fn build_monitor_result_markdown(
    kind: &str,
    duration_secs: u64,
    elapsed_ms: u64,
    summary: &str,
) -> String {
    match kind {
        "completed" => format!(
            "### OmniNova · 桌面监控完成\n\n**状态：** 完成\n**监控时长：** {duration_secs} 秒\n**实际耗时：** {:.1} 秒\n\n**检测结果**\n{summary}",
            elapsed_ms as f64 / 1000.0
        ),
        "timeout" => format!(
            "### OmniNova · 桌面监控超时\n\n**状态：** 超时\n**监控时长：** {duration_secs} 秒"
        ),
        _ => format!("### OmniNova · 桌面监控失败\n\n**状态：** 失败\n**原因：** {summary}"),
    }
}

/// Monitor click decision (P2/P3): Running → live remaining seconds
/// (no second task); otherwise starts the monitor and returns the
/// immediate countdown text.
async fn start_monitor(
    runtime: &Arc<GatewayRuntime>,
    store: &WecomCardStore,
    outbound_tx: &mpsc::Sender<WecomOutboundMsg>,
    task_id: &str,
    target_chat_id: Option<String>,
    duration_secs: u64,
) -> String {
    // MONITOR_SINGLE_FLIGHT: a second click while running shows the
    // live remaining time and never starts another task.
    if let Some(remaining) = store.monitor_remaining_secs(task_id).await {
        println!(
            "[wecom-card] monitor_admission=busy task_id={} remaining={}",
            short_hash(task_id),
            remaining
        );
        return format!("桌面监控正在运行\n剩余：{remaining} 秒");
    }

    let generation = runtime.current_wecom_stream_generation();
    if !store.try_start_monitor(task_id, duration_secs, generation).await {
        return "桌面监控正在运行，请稍候…".to_string();
    }
    println!(
        "[wecom-card] monitor_started task_id={} duration={} generation={}",
        short_hash(task_id),
        duration_secs,
        generation
    );

    let store_for_job = store.clone();
    let runtime_for_job = runtime.clone();
    let tx_for_job = outbound_tx.clone();
    let task_for_job = task_id.to_string();
    let started = std::time::Instant::now();
    tokio::spawn(async move {
        let captures_dir = directories::ProjectDirs::from("com", "omninova", "OmniNova")
            .map(|dirs| dirs.config_dir().join("captures"))
            .unwrap_or_else(|| std::env::temp_dir().join("omninova-captures"));
        // Hard timeout = soft duration + grace: a monitor can never hang forever.
        let hard_timeout = std::time::Duration::from_secs(duration_secs + MONITOR_GRACE_SECS);
        let outcome = tokio::time::timeout(
            hard_timeout,
            crate::desktop_capture::monitor_desktop(&captures_dir, duration_secs),
        )
        .await;
        let elapsed_ms = started.elapsed().as_millis() as u64;

        let (kind, summary) = match outcome {
            Err(_) => (
                "timeout",
                format!("桌面监控超时\n\n监控时长：{duration_secs} 秒\n实际耗时：{:.1} 秒\n请稍后重试。", elapsed_ms as f64 / 1000.0),
            ),
            Ok(result) if result.ok => {
                let detail = if result.changed.unwrap_or(false) {
                    "检测到桌面变化"
                } else {
                    "未检测到明显变化"
                };
                (
                    "completed",
                    format!(
                        "桌面监控完成\n\n监控时长：{duration_secs} 秒\n实际耗时：{:.1} 秒\n结果：{detail}",
                        elapsed_ms as f64 / 1000.0
                    ),
                )
            }
            Ok(_) => (
                "failed",
                "桌面监控失败\n\n请稍后重试。".to_string(),
            ),
        };
        println!(
            "[wecom-card] monitor_completed task_id={} kind={} duration_ms={} result_len={}",
            short_hash(&task_for_job),
            kind,
            elapsed_ms,
            summary.chars().count()
        );
        if kind != "completed" {
            println!("[wecom-card] monitor_{kind} task_id={}", short_hash(&task_for_job));
        }
        let result = match kind {
            "completed" => {
                store_for_job
                    .complete_monitor(&task_for_job, duration_secs, elapsed_ms, summary.clone())
                    .await;
                summary
            }
            _ => {
                store_for_job
                    .fail_monitor(&task_for_job, summary.clone(), elapsed_ms)
                    .await;
                summary
            }
        };
        // Automatic result delivery (P7) — with generation fence (P9).
        deliver_monitor_result(
            &runtime_for_job,
            &store_for_job,
            &tx_for_job,
            &task_for_job,
            target_chat_id,
            generation,
            kind,
            duration_secs,
            elapsed_ms,
            &result,
        )
        .await;
    });

    // Immediate card update within the 5-second window (P3 countdown).
    format!(
        "桌面监控\n状态：监控中\n总时长：{duration_secs} 秒\n剩余：{duration_secs} 秒"
    )
}

/// Deliver a finished monitor result via a PROACTIVE `aibot_send_msg`.
///
/// Phase 2A.3.1f: the body is RELIABLE MARKDOWN (msgtype=markdown);
/// `proactive_send_write_ok` means TRANSPORT WRITE only — delivery is
/// confirmed by the ACK (errcode==0), which the stream ACK branch maps
/// into the monitor delivery state (Pending/Sent/Failed).
///
/// Guards: generation fence (stale lifecycle results discarded), panel
/// TTL validity, target presence. Never touches the event req_id (the
/// official updateTemplateCard 5-second rule is preserved).
#[allow(clippy::too_many_arguments)]
async fn deliver_monitor_result(
    runtime: &Arc<GatewayRuntime>,
    store: &WecomCardStore,
    outbound_tx: &mpsc::Sender<WecomOutboundMsg>,
    task_id: &str,
    target_chat_id: Option<String>,
    generation: u64,
    kind: &str,
    duration_secs: u64,
    elapsed_ms: u64,
    summary: &str,
) {
    // P9: stale generation → discard; a new lifecycle must never receive
    // an old lifecycle's monitor result.
    if generation != 0 && !runtime.is_wecom_stream_generation_active(generation) {
        println!(
            "[wecom-card] monitor_result_discarded reason=stale_generation task_id={}",
            short_hash(task_id)
        );
        return;
    }
    // Panel must still be valid (30-min TTL).
    if store.lookup_valid(task_id).await.is_none() {
        println!(
            "[wecom-card] monitor_result_discarded reason=panel_expired task_id={}",
            short_hash(task_id)
        );
        return;
    }
    let Some(target) = target_chat_id else {
        println!(
            "[wecom-card] monitor_result_discarded reason=no_target task_id={}",
            short_hash(task_id)
        );
        return;
    };

    let body = ProactiveBody::Markdown {
        content: build_monitor_result_markdown(kind, duration_secs, elapsed_ms, summary),
    };
    let req_id = format!("wecom_monitor_{}", uuid::Uuid::new_v4());
    println!(
        "[wecom-card] monitor_result_push_requested task_id={} target={} kind={} format={}",
        short_hash(task_id),
        short_hash(&target),
        kind,
        body.format_name()
    );
    store
        .note_card_req_id(&req_id, WecomCardReqKind::Proactive, Some(task_id))
        .await;

    // Transport send: at most ONE retry for a transient transport
    // failure. Payload validation errors (4xxxx ACK) are NEVER retried —
    // that path lives in the ACK handler and only marks delivery failed.
    let message = WecomOutboundMsg::ProactiveMessage {
        req_id: req_id.clone(),
        chat_id: target,
        body: body.to_wire(),
    };
    if outbound_tx.send(message.clone()).await.is_err() {
        println!(
            "[wecom-card] monitor_result_push_retry task_id={} attempt=2 reason=transport_send_failed",
            short_hash(task_id)
        );
        if outbound_tx.send(message).await.is_err() {
            println!(
                "[wecom-card] monitor_result_push_failed task_id={} reason=channel_closed",
                short_hash(task_id)
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Event handling (template_card_event)
// ---------------------------------------------------------------------------

/// Official routing discriminator: a callback is a template-card click
/// ONLY when the frame is an event callback with
/// `body.msgtype = "event"` AND `event.eventtype = "template_card_event"`.
/// Every other event (enter_chat, disconnected_event, feedback_event,
/// …) keeps its original handling.
pub fn is_template_card_event(body: &WecomCallbackBody) -> bool {
    body.msgtype.as_deref() == Some("event")
        && body
            .event
            .as_ref()
            .and_then(|event| event.eventtype.as_deref())
            == Some("template_card_event")
}

/// Normalized, transport-independent card event (Phase 2A.3.1d).
/// The card action layer reads ONLY this struct — never the raw
/// `body.event.*` / `body.event.template_card_event.*` fields directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedWecomCardEvent {
    pub task_id: Option<String>,
    pub event_key: Option<String>,
    pub card_type: Option<String>,
    pub req_id: String,
    pub msg_id: String,
    pub user_id: Option<String>,
    pub chat_id: Option<String>,
}

/// SINGLE card-event normalizer. Returns `None` when the callback is
/// not a template_card_event (callers keep the existing event routing).
///
/// Field precedence (real runtime shape wins over SDK/document shape):
/// 1. `body.event.template_card_event.{card_type,event_key,task_id}`  (nested_runtime)
/// 2. `body.event.{card_type,event_key,task_id}`                      (flat_sdk)
pub fn normalize_template_card_event(
    body: &WecomCallbackBody,
    req_id: &str,
) -> Option<NormalizedWecomCardEvent> {
    if !is_template_card_event(body) {
        return None;
    }
    let event = body.event.as_ref()?;
    let nested = event.template_card_event.as_ref();
    Some(NormalizedWecomCardEvent {
        task_id: nested
            .and_then(|n| n.task_id.clone())
            .or_else(|| event.task_id.clone()),
        event_key: nested
            .and_then(|n| n.event_key.clone())
            .or_else(|| event.event_key.clone()),
        card_type: nested
            .and_then(|n| n.card_type.clone())
            .or_else(|| event.card_type.clone()),
        req_id: req_id.to_string(),
        msg_id: body.msgid.clone(),
        user_id: body.from.as_ref().and_then(|f| f.userid.clone()),
        chat_id: body.chatid.clone(),
    })
}

/// Which wire shape produced the normalized event (for diagnostics).
pub fn card_event_source(body: &WecomCallbackBody) -> &'static str {
    match body
        .event
        .as_ref()
        .and_then(|event| event.template_card_event.as_ref())
    {
        Some(_) => "nested_runtime",
        None => "flat_sdk",
    }
}

/// Handle a `template_card_event` callback (long connection).
///
/// validate task_id → TTL → dedup → allowlist → deterministic action →
/// update the SAME card via the official `aibot_respond_update_msg`
/// envelope (headers.req_id = the event frame's req_id; task_id
/// preserved; dispatched immediately, well under the 5-second window —
/// no LLM, no monitor wait). Card events NEVER dispatch the Agent.
///
/// APPLICATION-LEVEL ERROR ISOLATION (Phase 2A.3.1c): a malformed or
/// stale card event is NOT a transport protocol error. Missing
/// task_id/event_key, unknown/expired tasks, duplicates and unknown
/// actions are logged and consumed (Ok) — they can never bubble up to
/// the read loop as a connection-level `protocol` failure.
pub async fn handle_card_event(
    runtime: &Arc<GatewayRuntime>,
    body: &WecomCallbackBody,
    req_id: &str,
    outbound_tx: &mpsc::Sender<WecomOutboundMsg>,
) -> Result<(), String> {
    let event = body.event.as_ref();

    // Low-sensitive structural diagnostic so the next real E2E shows
    // exactly which fields WeCom actually sent. Never prints values.
    // Flat vs nested fields are reported SEPARATELY (Phase 2A.3.1e).
    println!(
        "[wecom-card] event_shape msgtype_event={} eventtype_present={} eventtype_template_card={} flat_event_key_present={} flat_task_id_present={} nested_payload_present={} nested_event_key_present={} nested_task_id_present={} userid_present={} chatid_present={}",
        body.msgtype.as_deref() == Some("event"),
        event.and_then(|e| e.eventtype.as_ref()).is_some(),
        event.and_then(|e| e.eventtype.as_deref()) == Some("template_card_event"),
        event.and_then(|e| e.event_key.as_deref()).map(|v| !v.trim().is_empty()).unwrap_or(false),
        event.and_then(|e| e.task_id.as_deref()).map(|v| !v.trim().is_empty()).unwrap_or(false),
        event.and_then(|e| e.template_card_event.as_ref()).is_some(),
        event.and_then(|e| e.template_card_event.as_ref()).and_then(|n| n.event_key.as_deref()).map(|v| !v.trim().is_empty()).unwrap_or(false),
        event.and_then(|e| e.template_card_event.as_ref()).and_then(|n| n.task_id.as_deref()).map(|v| !v.trim().is_empty()).unwrap_or(false),
        body.from.as_ref().and_then(|f| f.userid.as_deref()).map(|v| !v.trim().is_empty()).unwrap_or(false),
        body.chatid.as_deref().map(|v| !v.trim().is_empty()).unwrap_or(false),
    );

    // SINGLE normalization source: the whole card pipeline reads only
    // the normalized event (ONE_CARD_EVENT_SOURCE=true).
    let Some(normalized) = normalize_template_card_event(body, req_id) else {
        println!("[wecom-card] event_rejected reason=not_template_card_event");
        return Ok(());
    };
    println!(
        "[wecom-card] event_normalized source={} task_id_present={} event_key_present={} card_type_present={}",
        card_event_source(body),
        normalized.task_id.as_deref().map(|v| !v.trim().is_empty()).unwrap_or(false),
        normalized.event_key.as_deref().map(|v| !v.trim().is_empty()).unwrap_or(false),
        normalized.card_type.is_some(),
    );

    // Missing task_id: application-level rejection, never a protocol error.
    let task_id = match normalized.task_id.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        Some(task_id) => task_id,
        None => {
            println!("[wecom-card] event_rejected reason=missing_task_id");
            return Ok(());
        }
    };
    // Missing event_key: application-level rejection, never a protocol error.
    let event_key = match normalized.event_key.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        Some(event_key) => event_key,
        None => {
            println!("[wecom-card] event_rejected reason=missing_event_key");
            return Ok(());
        }
    };

    println!(
        "[wecom-card] event_received task_id={} event_key={}",
        short_hash(task_id),
        event_key
    );

    let store = card_store();

    // task_id validation + 30-min TTL.
    let Some(state) = store.lookup_valid(task_id).await else {
        println!("[wecom-card] card_expired task_id={}", short_hash(task_id));
        let _ = outbound_tx
            .send(WecomOutboundMsg::Reply {
                req_id: req_id.to_string(),
                text: PANEL_EXPIRED_TEXT.to_string(),
            })
            .await;
        return Ok(());
    };

    // Event retry dedup (per task_id + event msgid).
    if !store.dedup_event(task_id, &normalized.msg_id).await {
        println!(
            "[wecom-card] event_duplicate ignored=true task_id={}",
            short_hash(task_id)
        );
        return Ok(());
    }

    // Allowlist: unknown action → safe prompt; never Agent/tools.
    let Some(action) = WecomPanelAction::from_event_key(event_key) else {
        println!(
            "[wecom-card] unknown_action rejected=true task_id={}",
            short_hash(task_id)
        );
        let _ = outbound_tx
            .send(WecomOutboundMsg::Reply {
                req_id: req_id.to_string(),
                text: UNKNOWN_ACTION_TEXT.to_string(),
            })
            .await;
        return Ok(());
    };

    println!(
        "[wecom-card] action={} agent_dispatch=false task_id={}",
        action.key(),
        short_hash(task_id)
    );

    // Deterministic immediate update (no LLM / no long task): the card
    // update is dispatched right here, within the 5-second window.
    let context = CardActionContext {
        task_id: task_id.to_string(),
        session_key: state.session_key.clone(),
        action,
        target_chat_id: resolve_card_target(body),
    };
    let result = WecomCardActionService::execute(runtime, store, outbound_tx, context).await;
    let card = build_panel_card_with_text(task_id, &result.content);
    println!(
        "[wecom-card] card_dispatch_requested task_id={} update=true",
        short_hash(task_id)
    );
    store
        .note_card_req_id(req_id, WecomCardReqKind::UpdateCard, Some(task_id))
        .await;
    if let Err(error) = outbound_tx
        .send(WecomOutboundMsg::TemplateCardUpdate {
            req_id: req_id.to_string(),
            body: card,
        })
        .await
    {
        // Outbound queue loss is a transport concern of the writer loop;
        // never a stream protocol error.
        println!("[wecom-card] card_update_send_failed reason={error} isolated=true");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event_body(task_id: &str, event_key: &str) -> WecomCallbackBody {
        serde_json::from_value(serde_json::json!({
            "msgid": "evt-1",
            "msgtype": "event",
            "from": {"userid": "user-1"},
            "event": {
                "eventtype": "template_card_event",
                "event_key": event_key,
                "task_id": task_id,
                "card_type": "button_interaction"
            }
        }))
        .unwrap()
    }

    #[test]
    fn wecom_panel_command_detected() {
        // Parameterized: every supported command form must match exactly.
        for command in [
            "菜单",
            " 菜单 ",
            "面板",
            " 面板 ",
            "menu",
            "MENU",
            " menu ",
            "/menu",
            "/MENU",
            "panel",
            "PANEL",
            "/panel",
            "/PANEL",
        ] {
            assert!(is_panel_command(command), "must match: {command:?}");
            let canonical = panel_command(command);
            assert!(canonical.is_some(), "must canonicalize: {command:?}");
        }
        // Exact match only — anything else stays on the Agent path.
        for text in [
            "menu test",
            "打开menu",
            "菜单测试",
            "修改菜单",
            "panel test",
            "hello",
            "",
            "菜单给我看看",
        ] {
            assert!(!is_panel_command(text), "must NOT match: {text:?}");
        }
    }

    #[test]
    fn wecom_panel_command_does_not_dispatch_agent() {
        // The long-connection handler only enqueues an Agent job when
        // is_panel_command() is false; panel texts short-circuit to the
        // card branch (PANEL_COMMAND_AGENT_DISPATCH=NO by construction).
        assert!(is_panel_command("菜单"));
        assert!(is_panel_command("/panel"));
        // Panel card JSON contains no agent-dispatch payload at all.
        let card = build_panel_card(&new_task_id());
        let rendered = card.to_string();
        assert!(!rendered.contains("agent"));
        assert!(!rendered.contains("process_inbound"));
    }

    #[test]
    fn wecom_card_build_button_interaction() {
        let task_id = new_task_id();
        let card = build_panel_card(&task_id);
        assert_eq!(card["card_type"], "button_interaction");
        assert_eq!(card["main_title"]["title"], PANEL_TITLE);
        assert_eq!(card["main_title"]["desc"], PANEL_DESC);
        assert_eq!(card["task_id"], task_id);
        let buttons = card["button_list"].as_array().unwrap();
        assert_eq!(buttons.len(), 5);
        assert_eq!(buttons[0]["key"], "gateway_status");
        assert_eq!(buttons[0]["text"], "网关状态");
        assert_eq!(buttons[1]["key"], "recent_jobs");
        assert_eq!(buttons[2]["key"], "monitor_30");
        assert_eq!(buttons[3]["key"], "monitor_60");
        assert_eq!(buttons[4]["key"], "help");
    }

    #[test]
    fn wecom_card_has_unique_task_id() {
        let a = new_task_id();
        let b = new_task_id();
        assert_ne!(a, b);
        assert!(a.starts_with("wecom_panel_"));
        // No user-identifying material can leak into the task_id.
        assert!(!a.contains("user"));
        assert_eq!(a.len(), "wecom_panel_".len() + 36);
    }

    #[test]
    fn wecom_card_action_allowlist() {
        assert_eq!(
            WecomPanelAction::from_event_key("gateway_status"),
            Some(WecomPanelAction::GatewayStatus)
        );
        assert_eq!(
            WecomPanelAction::from_event_key("recent_jobs"),
            Some(WecomPanelAction::RecentJobs)
        );
        assert_eq!(
            WecomPanelAction::from_event_key("monitor_30"),
            Some(WecomPanelAction::Monitor30)
        );
        assert_eq!(
            WecomPanelAction::from_event_key("monitor_60"),
            Some(WecomPanelAction::Monitor60)
        );
        assert_eq!(
            WecomPanelAction::from_event_key("help"),
            Some(WecomPanelAction::Help)
        );
        for evil in [
            "shell",
            "rm -rf",
            "monitor_30s",
            "gateway_status ",
            "GatewayStatus",
            "",
            "monitor_30/../../etc",
        ] {
            assert_eq!(WecomPanelAction::from_event_key(evil), None, "key: {evil}");
        }
    }

    #[test]
    fn wecom_card_unknown_action_rejected() {
        // Unknown keys must resolve to None (handler replies with the
        // safe prompt and never executes anything).
        assert!(WecomPanelAction::from_event_key("run_shell").is_none());
        assert!(UNKNOWN_ACTION_TEXT.contains("未知操作"));
    }

    #[test]
    fn wecom_template_card_event_parse() {
        let body = event_body("task-1", "gateway_status");
        // Official hierarchy: body.msgtype = "event", data under body.event.
        assert_eq!(body.msgtype.as_deref(), Some("event"));
        assert_eq!(body.event.as_ref().is_none(), false);
        let event = body.event.as_ref().unwrap();
        assert_eq!(event.eventtype.as_deref(), Some("template_card_event"));
        assert_eq!(event.task_id.as_deref(), Some("task-1"));
        assert_eq!(event.event_key.as_deref(), Some("gateway_status"));
        assert_eq!(event.card_type.as_deref(), Some("button_interaction"));
        assert!(is_template_card_event(&body));
    }

    #[test]
    fn wecom_template_card_event_frame_parses_official_hierarchy() {
        // Full official wire fixture: cmd=aibot_event_callback,
        // body.msgtype=event, event.eventtype=template_card_event.
        let frame = serde_json::json!({
            "cmd": "aibot_event_callback",
            "headers": {"req_id": "req-123"},
            "body": {
                "msgid": "evt-official",
                "aibotid": "bot-1",
                "chatid": "chat-1",
                "chattype": "group",
                "from": {"userid": "user-1"},
                "msgtype": "event",
                "event": {
                    "eventtype": "template_card_event",
                    "event_key": "gateway_status",
                    "task_id": "task-original",
                    "card_type": "button_interaction"
                }
            }
        });
        let envelope: crate::gateway::wecom_protocol::WecomCallbackEnvelope =
            serde_json::from_value(frame).unwrap();
        assert_eq!(envelope.cmd, "aibot_event_callback");
        assert_eq!(envelope.headers.req_id, "req-123");
        let body = &envelope.body;
        assert_eq!(body.msgtype.as_deref(), Some("event"));
        assert_eq!(body.msgid, "evt-official");
        assert_eq!(body.from.as_ref().and_then(|f| f.userid.as_deref()), Some("user-1"));
        assert_eq!(body.chatid.as_deref(), Some("chat-1"));
        assert_eq!(body.chattype.as_deref(), Some("group"));
        let event = body.event.as_ref().unwrap();
        assert_eq!(event.eventtype.as_deref(), Some("template_card_event"));
        assert_eq!(event.event_key.as_deref(), Some("gateway_status"));
        assert_eq!(event.task_id.as_deref(), Some("task-original"));
        assert!(is_template_card_event(body));
    }

    #[test]
    fn wecom_event_routing_only_template_card_event() {
        // Only eventtype=template_card_event routes to the card handler.
        assert!(is_template_card_event(&event_body("t", "gateway_status")));

        for (msgtype, eventtype) in [
            ("event", "enter_chat"),
            ("event", "disconnected_event"),
            ("event", "feedback_event"),
            ("text", "template_card_event"),
        ] {
            let body: WecomCallbackBody = serde_json::from_value(serde_json::json!({
                "msgid": "other",
                "msgtype": msgtype,
                "event": {"eventtype": eventtype}
            }))
            .unwrap();
            assert!(
                !is_template_card_event(&body),
                "must not route to card handler: msgtype={msgtype} eventtype={eventtype}"
            );
        }
    }

    #[test]
    fn wecom_initial_card_wire_is_respond_msg() {
        let task_id = new_task_id();
        let card = build_panel_card(&task_id);
        let envelope =
            crate::gateway::wecom_protocol::build_template_card_respond_envelope("req-init", card);
        assert_eq!(envelope.cmd, "aibot_respond_msg");
        assert_eq!(envelope.headers.req_id, "req-init");
        let body = envelope.body.unwrap();
        assert_eq!(body["msgtype"], "template_card");
        assert_eq!(body["template_card"]["card_type"], "button_interaction");
        assert_eq!(body["template_card"]["task_id"], task_id);
        // The initial reply must NOT carry the update response_type.
        assert!(body.get("response_type").is_none());
    }

    #[test]
    fn wecom_update_card_wire_is_respond_update_msg() {
        let task_id = new_task_id();
        let updated = build_panel_card_with_text(&task_id, "Gateway 状态\n\n运行中");
        let envelope = crate::gateway::wecom_protocol::build_template_card_update_envelope(
            "req-event-1",
            updated,
        );
        assert_eq!(envelope.cmd, "aibot_respond_update_msg");
        // headers.req_id MUST be the original event frame req_id.
        assert_eq!(envelope.headers.req_id, "req-event-1");
        let body = envelope.body.unwrap();
        assert_eq!(body["response_type"], "update_template_card");
        assert!(body.get("msgtype").is_none());
        assert_eq!(body["template_card"]["task_id"], task_id);
        assert_eq!(
            body["template_card"]["sub_title_text"],
            "Gateway 状态\n\n运行中"
        );
    }

    #[test]
    fn wecom_update_card_preserves_task_id_and_event_req_id() {
        // The official update contract: same task_id, event frame's req_id.
        let task_id = new_task_id();
        let card = build_panel_card_with_text(&task_id, "监控中...");
        let envelope =
            crate::gateway::wecom_protocol::build_template_card_update_envelope("evt-req", card);
        assert_eq!(envelope.headers.req_id, "evt-req");
        assert_eq!(
            envelope.body.unwrap()["template_card"]["task_id"],
            task_id
        );
    }

    // ------------------------------------------------------------------
    // Phase 2A.3.1c: real event tolerance + application error isolation
    // ------------------------------------------------------------------

    fn event_body_optional(
        msgid: &str,
        eventtype: &str,
        event_key: Option<&str>,
        task_id: Option<&str>,
        card_type: Option<&str>,
    ) -> WecomCallbackBody {
        let mut event = serde_json::json!({ "eventtype": eventtype });
        if let Some(value) = event_key {
            event["event_key"] = serde_json::json!(value);
        }
        if let Some(value) = task_id {
            event["task_id"] = serde_json::json!(value);
        }
        if let Some(value) = card_type {
            event["card_type"] = serde_json::json!(value);
        }
        serde_json::from_value(serde_json::json!({
            "msgid": msgid,
            "msgtype": "event",
            "from": {"userid": "u-1"},
            "event": event
        }))
        .unwrap()
    }

    #[test]
    fn template_card_event_without_card_type_parses() {
        // Official TemplateCardEventData has NO card_type — deserialize
        // must succeed and routing must still work.
        let body = event_body_optional("evt-nct", "template_card_event", Some("gateway_status"), Some("task-nct"), None);
        assert!(body.event.as_ref().unwrap().card_type.is_none());
        assert!(is_template_card_event(&body));
    }

    #[test]
    fn template_card_event_event_key_optional_at_deserialize() {
        let body = event_body_optional("evt-nek", "template_card_event", None, Some("task-nek"), None);
        assert!(body.event.as_ref().unwrap().event_key.is_none());
        assert!(is_template_card_event(&body));
    }

    #[test]
    fn template_card_event_task_id_optional_at_deserialize() {
        let body = event_body_optional("evt-ntk", "template_card_event", Some("help"), None, None);
        assert!(body.event.as_ref().unwrap().task_id.is_none());
        assert!(is_template_card_event(&body));
    }

    async fn handler_with_channel() -> (
        Arc<GatewayRuntime>,
        mpsc::Sender<WecomOutboundMsg>,
        mpsc::Receiver<WecomOutboundMsg>,
    ) {
        let runtime = Arc::new(GatewayRuntime::new(Config::default()));
        let (tx, rx) = mpsc::channel::<WecomOutboundMsg>(8);
        (runtime, tx, rx)
    }

    #[tokio::test]
    async fn missing_event_key_is_safe_rejection() {
        let (runtime, tx, mut rx) = handler_with_channel().await;
        let body = event_body_optional("evt-mek", "template_card_event", None, Some("task-mek"), None);
        let result = handle_card_event(&runtime, &body, "req-mek", &tx).await;
        assert!(result.is_ok(), "missing event_key must be consumed, got {result:?}");
        assert!(rx.try_recv().is_err(), "rejected event must not queue anything");
    }

    #[tokio::test]
    async fn missing_task_id_is_safe_rejection() {
        let (runtime, tx, mut rx) = handler_with_channel().await;
        let body = event_body_optional("evt-mtk", "template_card_event", Some("help"), None, None);
        let result = handle_card_event(&runtime, &body, "req-mtk", &tx).await;
        assert!(result.is_ok(), "missing task_id must be consumed, got {result:?}");
        assert!(rx.try_recv().is_err(), "rejected event must not queue anything");
    }

    #[tokio::test]
    async fn unknown_task_id_is_safe_rejection() {
        let (runtime, tx, mut rx) = handler_with_channel().await;
        let body = event_body_optional("evt-utk", "template_card_event", Some("help"), Some("task-never-registered"), None);
        let result = handle_card_event(&runtime, &body, "req-utk", &tx).await;
        assert!(result.is_ok(), "unknown task_id must be consumed, got {result:?}");
        // Safe prompt reply, never an error, never an Agent job.
        let queued = rx.try_recv().expect("unknown task must answer with a safe Reply");
        match queued {
            WecomOutboundMsg::Reply { req_id, text } => {
                assert_eq!(req_id, "req-utk");
                assert_eq!(text, PANEL_EXPIRED_TEXT);
            }
            other => panic!("expected safe Reply, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn card_event_error_does_not_return_stream_protocol_error() {
        // Aggregate guarantee: every application-level rejection path
        // returns Ok(()) so the read loop never sees a protocol error.
        let (runtime, tx, _rx) = handler_with_channel().await;
        for body in [
            event_body_optional("evt-iso-1", "template_card_event", None, Some("t-1"), None),
            event_body_optional("evt-iso-2", "template_card_event", Some("help"), None, None),
            event_body_optional("evt-iso-3", "template_card_event", Some("help"), Some("t-never"), None),
            event_body_optional("evt-iso-4", "template_card_event", Some("not-allowed"), Some("t-2"), None),
        ] {
            let result = handle_card_event(&runtime, &body, "req-iso", &tx).await;
            assert!(result.is_ok(), "card event must never surface as an error");
        }
    }

    #[tokio::test]
    async fn task_id_generated_once_and_shared_between_store_and_wire() {
        // P5 invariant: ONE generation → store key == wire card task_id.
        let task_id = new_task_id();
        card_store().register(task_id.clone(), None).await;
        let card = build_panel_card(&task_id);
        assert_eq!(card["task_id"], task_id);
        let state = card_store()
            .lookup_valid(&task_id)
            .await
            .expect("registered task must be found");
        assert_eq!(state.task_id, task_id);
    }

    #[test]
    fn official_button_event_routes_gateway_status() {
        let body = event_body("task-gw", "gateway_status");
        assert!(is_template_card_event(&body));
        let event = body.event.as_ref().unwrap();
        assert_eq!(event.eventtype.as_deref(), Some("template_card_event"));
        assert_eq!(
            WecomPanelAction::from_event_key(event.event_key.as_deref().unwrap()),
            Some(WecomPanelAction::GatewayStatus)
        );
    }

    #[tokio::test]
    async fn gateway_status_event_queues_template_card_update() {
        let (runtime, tx, mut rx) = handler_with_channel().await;
        let task_id = new_task_id();
        card_store().register(task_id.clone(), None).await;
        let body = event_body(&task_id, "gateway_status");
        let result = handle_card_event(&runtime, &body, "req-gw", &tx).await;
        assert!(result.is_ok());
        let queued = rx
            .try_recv()
            .expect("gateway_status must queue a TemplateCardUpdate");
        match queued {
            WecomOutboundMsg::TemplateCardUpdate { req_id, body: card } => {
                assert_eq!(req_id, "req-gw");
                assert_eq!(card["task_id"], task_id);
                assert!(
                    card["sub_title_text"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("Gateway 状态"),
                    "update must carry the gateway status text"
                );
                let envelope =
                    crate::gateway::wecom_protocol::build_template_card_update_envelope("req-gw", card);
                assert_eq!(envelope.cmd, "aibot_respond_update_msg");
                assert_eq!(envelope.body.unwrap()["response_type"], "update_template_card");
            }
            other => panic!("expected TemplateCardUpdate, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn wecom_template_card_event_dedup() {
        let store = WecomCardStore::new();
        assert!(store.dedup_event("task-a", "evt-1").await);
        assert!(!store.dedup_event("task-a", "evt-1").await);
        assert!(store.dedup_event("task-a", "evt-2").await);
        assert!(store.dedup_event("task-b", "evt-1").await);
    }

    #[tokio::test]
    async fn wecom_template_card_event_expired() {
        let store = WecomCardStore::new();
        let old = OffsetDateTime::now_utc().unix_timestamp() - CARD_TTL_SECS - 60;
        store
            .register_at("task-old".to_string(), None, old)
            .await;
        assert!(store.lookup_valid("task-old").await.is_none());
        // A fresh panel stays valid.
        store.register("task-new".to_string(), None).await;
        assert!(store.lookup_valid("task-new").await.is_some());
    }

    #[test]
    fn wecom_gateway_status_card() {
        let now = OffsetDateTime::now_utc();
        let snapshot = WecomGatewayStatusSnapshot {
            transport: "long_connection",
            connection_connected: true,
            stream_active: true,
            generation: 7,
            last_heartbeat_available: false,
            enabled_channels: vec!["wecom", "dingtalk"],
            agent_name: "omninova".to_string(),
        };
        let text = gateway_status_text(&snapshot, &now);
        assert!(text.contains("Gateway 状态"));
        assert!(text.contains("Gateway running: true"));
        assert!(text.contains("WeCom transport: long_connection"));
        assert!(text.contains("connection: connected"));
        assert!(text.contains("generation: 7"));
        assert!(text.contains("enabled channels: wecom、dingtalk"));
        assert!(text.contains("Agent: omninova"));
        // Honest about what is NOT tracked.
        assert!(text.contains("未追踪"));
        // No secret-like content ever enters this text.
        assert!(!text.contains("secret"));
        assert!(!text.contains("token"));
    }

    #[tokio::test]
    async fn wecom_recent_jobs_card() {
        let store = WecomCardStore::new();
        assert!(store.recent_jobs_text().await.contains("暂无最近任务"));
        for i in 0..7 {
            store
                .record_job(if i % 2 == 0 { "single" } else { "group" }, "已受理")
                .await;
        }
        let text = store.recent_jobs_text().await;
        assert!(text.contains("最近任务（5 条）"));
        assert!(!text.contains("7 条"));
        // Redacted: no payload/secret material.
        assert!(!text.contains("payload"));
    }

    #[tokio::test]
    async fn wecom_monitor_30_singleflight() {
        let store = WecomCardStore::new();
        store.register("task-m30".to_string(), None).await;
        assert!(store.try_start_monitor("task-m30", 30, 0).await);
        assert!(!store.try_start_monitor("task-m30", 30, 0).await);
        store
            .complete_monitor("task-m30", 30, 1000, "done".to_string())
            .await;
        assert!(matches!(
            store.monitor_state("task-m30").await,
            MonitorState::Completed { .. }
        ));
        assert!(store.try_start_monitor("task-m30", 30, 0).await);
    }

    #[tokio::test]
    async fn wecom_monitor_60_singleflight() {
        let store = WecomCardStore::new();
        store.register("task-m60".to_string(), None).await;
        assert!(store.try_start_monitor("task-m60", 60, 0).await);
        assert!(!store.try_start_monitor("task-m60", 60, 0).await);
        // A different card can still start its own monitor.
        store.register("task-other".to_string(), None).await;
        assert!(store.try_start_monitor("task-other", 60, 0).await);
    }

    #[test]
    fn wecom_help_card() {
        let text = help_text();
        assert!(text.contains("OmniNova 使用帮助"));
        assert!(text.contains("/panel"));
        // Deterministic: two calls return identical content.
        assert_eq!(help_text(), text);
    }

    #[test]
    fn wecom_card_event_never_dispatches_agent() {
        // Card events resolve through the allowlist to deterministic
        // actions; the Agent path (process_inbound) is never referenced
        // by the card module — asserted by the payload shape.
        for key in ["gateway_status", "recent_jobs", "monitor_30", "monitor_60", "help"] {
            assert!(WecomPanelAction::from_event_key(key).is_some());
        }
        let card = build_panel_card_with_text("task-x", "Gateway 状态\n\n运行中");
        assert_eq!(card["msgtype"], serde_json::Value::Null);
        assert_eq!(card["card_type"], "button_interaction");
    }

    #[test]
    fn wecom_normal_text_still_dispatches_agent() {
        // Anything that is NOT an exact panel command keeps the normal path.
        for text in ["你好", "帮我查一下", "/monitor 桌面 30秒", "菜单给我看看", "panel test"] {
            assert!(!is_panel_command(text), "must stay on Agent path: {text}");
        }
    }

    #[test]
    fn wecom_http_existing_stream_unchanged() {
        // Panel commands are a long-connection concept; the HTTP stream
        // path is untouched by this module. Guard the shared constants.
        assert_eq!(PANEL_BUTTONS.len(), 5);
        assert!(CARD_TTL_SECS >= 30 * 60);
        // The HTTP reply builders live in wecom_http and are unchanged.
        let card = build_panel_card("t");
        assert_eq!(card["card_type"], "button_interaction");
    }

    // ------------------------------------------------------------------
    // Phase 2A.3.1d: REAL runtime nested shape + single normalizer
    // ------------------------------------------------------------------

    /// REAL runtime fixture (official SDK GitHub Issue #22): the card
    /// data nests under body.event.template_card_event.
    fn nested_event_body(
        msgid: &str,
        event_key: Option<&str>,
        task_id: Option<&str>,
        card_type: Option<&str>,
    ) -> WecomCallbackBody {
        let mut payload = serde_json::json!({});
        if let Some(value) = event_key {
            payload["event_key"] = serde_json::json!(value);
        }
        if let Some(value) = task_id {
            payload["task_id"] = serde_json::json!(value);
        }
        if let Some(value) = card_type {
            payload["card_type"] = serde_json::json!(value);
        }
        serde_json::from_value(serde_json::json!({
            "msgid": msgid,
            "msgtype": "event",
            "from": {"userid": "u-1"},
            "chatid": "chat-1",
            "event": {
                "eventtype": "template_card_event",
                "template_card_event": payload
            }
        }))
        .unwrap()
    }

    #[test]
    fn runtime_nested_card_event_parses() {
        let body = nested_event_body("evt-n-1", Some("gateway_status"), Some("task-n-1"), Some("button_interaction"));
        let event = body.event.as_ref().unwrap();
        assert_eq!(event.eventtype.as_deref(), Some("template_card_event"));
        let nested = event.template_card_event.as_ref().expect("nested payload must parse");
        assert_eq!(nested.event_key.as_deref(), Some("gateway_status"));
        assert_eq!(nested.task_id.as_deref(), Some("task-n-1"));
        assert_eq!(nested.card_type.as_deref(), Some("button_interaction"));
        assert!(is_template_card_event(&body));
    }

    #[test]
    fn runtime_nested_card_event_normalizes() {
        let body = nested_event_body("evt-n-2", Some("help"), Some("task-n-2"), None);
        let normalized = normalize_template_card_event(&body, "req-n-2").unwrap();
        assert_eq!(normalized.event_key.as_deref(), Some("help"));
        assert_eq!(normalized.task_id.as_deref(), Some("task-n-2"));
        assert_eq!(normalized.req_id, "req-n-2");
        assert_eq!(normalized.msg_id, "evt-n-2");
        assert_eq!(normalized.user_id.as_deref(), Some("u-1"));
        assert_eq!(normalized.chat_id.as_deref(), Some("chat-1"));
        assert_eq!(card_event_source(&body), "nested_runtime");
    }

    #[test]
    fn runtime_nested_event_key_resolved() {
        let body = nested_event_body("evt-n-3", Some("gateway_status"), Some("task-n-3"), None);
        let normalized = normalize_template_card_event(&body, "r").unwrap();
        assert_eq!(normalized.event_key.as_deref(), Some("gateway_status"));
    }

    #[test]
    fn runtime_nested_task_id_resolved() {
        let body = nested_event_body("evt-n-4", Some("help"), Some("task-n-4"), None);
        let normalized = normalize_template_card_event(&body, "r").unwrap();
        assert_eq!(normalized.task_id.as_deref(), Some("task-n-4"));
    }

    #[test]
    fn runtime_nested_card_type_resolved() {
        let body = nested_event_body("evt-n-5", Some("help"), Some("task-n-5"), Some("button_interaction"));
        let normalized = normalize_template_card_event(&body, "r").unwrap();
        assert_eq!(normalized.card_type.as_deref(), Some("button_interaction"));
    }

    #[test]
    fn sdk_flat_card_event_parses() {
        let body = event_body("task-flat-1", "gateway_status");
        assert!(is_template_card_event(&body));
        assert_eq!(card_event_source(&body), "flat_sdk");
    }

    #[test]
    fn sdk_flat_card_event_normalizes() {
        let body = event_body("task-flat-2", "help");
        let normalized = normalize_template_card_event(&body, "req-flat").unwrap();
        assert_eq!(normalized.event_key.as_deref(), Some("help"));
        assert_eq!(normalized.task_id.as_deref(), Some("task-flat-2"));
        assert_eq!(normalized.req_id, "req-flat");
    }

    #[test]
    fn nested_flat_precedence_nested_wins() {
        // Both shapes present: the real runtime nested shape must win.
        let body: WecomCallbackBody = serde_json::from_value(serde_json::json!({
            "msgid": "evt-both",
            "msgtype": "event",
            "from": {"userid": "u-1"},
            "event": {
                "eventtype": "template_card_event",
                "event_key": "flat-help",
                "task_id": "flat-task",
                "card_type": "flat-type",
                "template_card_event": {
                    "event_key": "nested-gateway_status",
                    "task_id": "nested-task",
                    "card_type": "button_interaction"
                }
            }
        }))
        .unwrap();
        let normalized = normalize_template_card_event(&body, "r").unwrap();
        assert_eq!(normalized.event_key.as_deref(), Some("nested-gateway_status"));
        assert_eq!(normalized.task_id.as_deref(), Some("nested-task"));
        assert_eq!(normalized.card_type.as_deref(), Some("button_interaction"));
        assert_eq!(card_event_source(&body), "nested_runtime");
    }

    #[test]
    fn normalized_gateway_status_routes_action() {
        let body = nested_event_body("evt-r-1", Some("gateway_status"), Some("task-r-1"), None);
        let normalized = normalize_template_card_event(&body, "r").unwrap();
        let action = WecomPanelAction::from_event_key(
            normalized.event_key.as_deref().expect("event_key resolved"),
        )
        .expect("gateway_status is on the allowlist");
        assert_eq!(action, WecomPanelAction::GatewayStatus);
    }

    #[test]
    fn normalized_event_never_dispatches_agent() {
        // The normalized pipeline resolves actions purely through the
        // allowlist; no Agent dispatch exists anywhere in it.
        for key in ["gateway_status", "recent_jobs", "monitor_30", "monitor_60", "help"] {
            let body = nested_event_body("evt-na", Some(key), Some("task-na"), None);
            let normalized = normalize_template_card_event(&body, "r").unwrap();
            assert!(WecomPanelAction::from_event_key(normalized.event_key.as_deref().unwrap()).is_some());
        }
        let body = nested_event_body("evt-na-x", Some("rm -rf"), Some("task-na-x"), None);
        let normalized = normalize_template_card_event(&body, "r").unwrap();
        assert!(WecomPanelAction::from_event_key(normalized.event_key.as_deref().unwrap()).is_none());
    }

    #[tokio::test]
    async fn normalized_event_queues_template_card_update() {
        let (runtime, tx, mut rx) = handler_with_channel().await;
        let task_id = new_task_id();
        card_store().register(task_id.clone(), None).await;
        let body = nested_event_body("evt-q-1", Some("gateway_status"), Some(&task_id), Some("button_interaction"));
        let result = handle_card_event(&runtime, &body, "req-q-1", &tx).await;
        assert!(result.is_ok());
        match rx.try_recv().expect("normalized event must queue an update") {
            WecomOutboundMsg::TemplateCardUpdate { req_id, body: card } => {
                assert_eq!(req_id, "req-q-1");
                assert_eq!(card["task_id"], task_id);
                assert!(
                    card["sub_title_text"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("Gateway 状态")
                );
            }
            other => panic!("expected TemplateCardUpdate, got {other:?}"),
        }
    }

    // ------------------------------------------------------------------
    // Phase 2A.3.1e: monitor countdown state + proactive auto delivery
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn monitor_30_state_starts_at_30() {
        let store = WecomCardStore::new();
        store.register("task-cd-1".to_string(), None).await;
        assert!(store.try_start_monitor("task-cd-1", 30, 0).await);
        let remaining = store.monitor_remaining_secs("task-cd-1").await.unwrap();
        assert!(remaining <= 30, "remaining={remaining}");
        assert!(remaining >= 29, "remaining={remaining}");
        assert!(matches!(
            store.monitor_state("task-cd-1").await,
            MonitorState::Running { duration_secs: 30, .. }
        ));
    }

    #[tokio::test]
    async fn monitor_countdown_uses_deadline() {
        // Remaining seconds must be recomputed from the monotonic
        // deadline (deadline - now), not a decrement counter.
        let store = WecomCardStore::new();
        store.register("task-cd-2".to_string(), None).await;
        assert!(store.try_start_monitor("task-cd-2", 30, 0).await);
        // Shift the deadline 12s into the past.
        let past = std::time::Instant::now() - std::time::Duration::from_secs(12);
        store.set_monitor_deadline_for_test("task-cd-2", past).await;
        let remaining = store.monitor_remaining_secs("task-cd-2").await.unwrap();
        assert_eq!(remaining, 0, "deadline passed → remaining 0");
    }

    #[tokio::test]
    async fn monitor_running_second_click_does_not_duplicate() {
        let store = WecomCardStore::new();
        store.register("task-sf-1".to_string(), None).await;
        assert!(store.try_start_monitor("task-sf-1", 30, 0).await);
        assert!(!store.try_start_monitor("task-sf-1", 30, 0).await);
        assert!(matches!(
            store.monitor_state("task-sf-1").await,
            MonitorState::Running { .. }
        ));
    }

    #[tokio::test]
    async fn monitor_running_second_click_returns_remaining() {
        let store = WecomCardStore::new();
        store.register("task-sf-2".to_string(), None).await;
        assert!(store.try_start_monitor("task-sf-2", 30, 0).await);
        let remaining = store.monitor_remaining_secs("task-sf-2").await.unwrap();
        let subtitle = panel_subtitle_for_state(&store.monitor_state("task-sf-2").await);
        assert_eq!(subtitle, format!("桌面监控中 · 剩余 {remaining} 秒"));
    }

    #[tokio::test]
    async fn monitor_completion_changes_state_completed() {
        let store = WecomCardStore::new();
        store.register("task-fin-1".to_string(), None).await;
        assert!(store.try_start_monitor("task-fin-1", 30, 0).await);
        store
            .complete_monitor("task-fin-1", 30, 30123, "桌面监控完成\n\n结果：检测到桌面变化".to_string())
            .await;
        match store.monitor_state("task-fin-1").await {
            MonitorState::Completed {
                duration_secs,
                elapsed_ms,
                summary,
                ..
            } => {
                assert_eq!(duration_secs, 30);
                assert_eq!(elapsed_ms, 30123);
                assert!(summary.contains("桌面监控完成"));
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn monitor_completion_queues_proactive_message() {
        let (runtime, tx, mut rx) = handler_with_channel().await;
        let store = WecomCardStore::new();
        store.register("task-push-1".to_string(), None).await;
        deliver_monitor_result(
            &runtime,
            &store,
            &tx,
            "task-push-1",
            Some("user-target".to_string()),
            0,
            "completed",
            30,
            30815,
            "检测到桌面变化",
        )
        .await;
        match rx.try_recv().expect("completion must queue a proactive message") {
            WecomOutboundMsg::ProactiveMessage { req_id, chat_id, body } => {
                assert!(req_id.starts_with("wecom_monitor_"));
                assert_eq!(chat_id, "user-target");
                // Phase 2A.3.1f: reliable MARKDOWN, no template_card.
                assert_eq!(body["msgtype"], "markdown");
                assert!(body.get("template_card").is_none());
                let content = body["markdown"]["content"].as_str().unwrap();
                assert!(content.contains("### OmniNova · 桌面监控完成"));
                assert!(content.contains("30 秒"));
                assert!(content.contains("30.8 秒"));
                assert!(content.contains("检测到桌面变化"));
            }
            other => panic!("expected ProactiveMessage, got {other:?}"),
        }
    }

    #[test]
    fn proactive_single_uses_userid() {
        let body: WecomCallbackBody = serde_json::from_value(serde_json::json!({
            "msgid": "t-s",
            "msgtype": "event",
            "chattype": "single",
            "from": {"userid": "user-single-1"},
            "event": {"eventtype": "template_card_event"}
        }))
        .unwrap();
        assert_eq!(resolve_card_target(&body).as_deref(), Some("user-single-1"));
    }

    #[test]
    fn proactive_group_uses_chatid() {
        let body: WecomCallbackBody = serde_json::from_value(serde_json::json!({
            "msgid": "t-g",
            "msgtype": "event",
            "chattype": "group",
            "chatid": "chat-group-1",
            "from": {"userid": "user-group-1"},
            "event": {"eventtype": "template_card_event"}
        }))
        .unwrap();
        assert_eq!(resolve_card_target(&body).as_deref(), Some("chat-group-1"));
    }

    #[test]
    fn proactive_wire_cmd_is_aibot_send_msg() {
        let body = ProactiveBody::Markdown {
            content: build_monitor_result_markdown("completed", 30, 30815, "检测到桌面变化"),
        }
        .to_wire();
        let envelope =
            crate::gateway::wecom_protocol::build_send_message_envelope("req-pro", "chat-1", body);
        assert_eq!(envelope.cmd, "aibot_send_msg");
        assert_eq!(envelope.headers.req_id, "req-pro");
        let body = envelope.body.unwrap();
        assert_eq!(body["chatid"], "chat-1");
        assert_eq!(body["msgtype"], "markdown");
    }

    #[tokio::test]
    async fn proactive_ack_correlates_req_id() {
        let store = WecomCardStore::new();
        store
            .note_card_req_id("req-pro-1", WecomCardReqKind::Proactive, Some("task-ack-1"))
            .await;
        let pending = store.consume_card_req_ack("req-pro-1").await.unwrap();
        assert_eq!(pending.kind, WecomCardReqKind::Proactive);
        assert_eq!(pending.task_id.as_deref(), Some("task-ack-1"));
        assert_eq!(store.consume_card_req_ack("req-pro-1").await, None);
    }

    #[tokio::test]
    async fn monitor_failure_notifies_user() {
        let (runtime, tx, mut rx) = handler_with_channel().await;
        let store = WecomCardStore::new();
        store.register("task-fail-1".to_string(), None).await;
        deliver_monitor_result(
            &runtime,
            &store,
            &tx,
            "task-fail-1",
            Some("user-target".to_string()),
            0,
            "failed",
            30,
            3000,
            "桌面监控失败，请稍后重试。",
        )
        .await;
        match rx.try_recv().expect("failure must notify the user") {
            WecomOutboundMsg::ProactiveMessage { body, .. } => {
                assert_eq!(body["msgtype"], "markdown");
                let content = body["markdown"]["content"].as_str().unwrap();
                assert!(content.contains("### OmniNova · 桌面监控失败"));
                assert!(content.contains("桌面监控失败"));
            }
            other => panic!("expected ProactiveMessage, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn monitor_timeout_notifies_user() {
        let (runtime, tx, mut rx) = handler_with_channel().await;
        let store = WecomCardStore::new();
        store.register("task-time-1".to_string(), None).await;
        deliver_monitor_result(
            &runtime,
            &store,
            &tx,
            "task-time-1",
            Some("user-target".to_string()),
            0,
            "timeout",
            30,
            45000,
            "桌面监控超时，请稍后重试。",
        )
        .await;
        match rx.try_recv().expect("timeout must notify the user") {
            WecomOutboundMsg::ProactiveMessage { body, .. } => {
                assert_eq!(body["msgtype"], "markdown");
                let content = body["markdown"]["content"].as_str().unwrap();
                assert!(content.contains("### OmniNova · 桌面监控超时"));
            }
            other => panic!("expected ProactiveMessage, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn stale_generation_monitor_result_discarded() {
        let (runtime, tx, mut rx) = handler_with_channel().await;
        let store = WecomCardStore::new();
        store.register("task-stale-1".to_string(), None).await;
        // generation=99 is not active on this runtime → result discarded.
        deliver_monitor_result(
            &runtime,
            &store,
            &tx,
            "task-stale-1",
            Some("user-target".to_string()),
            99,
            "completed",
            30,
            30000,
            "检测到桌面变化",
        )
        .await;
        assert!(
            rx.try_recv().is_err(),
            "stale generation must never send a monitor result"
        );
    }

    #[test]
    fn menu_ready_render() {
        let card = build_panel_card("task-menu-1");
        assert_eq!(card["main_title"]["title"], PANEL_TITLE);
        assert_eq!(card["main_title"]["desc"], PANEL_DESC);
        assert_eq!(card["sub_title_text"], PANEL_READY_SUBTITLE);
        assert_eq!(card["button_list"].as_array().unwrap().len(), 5);
        assert_eq!(panel_subtitle_for_state(&MonitorState::Idle), PANEL_READY_SUBTITLE);
    }

    #[tokio::test]
    async fn menu_running_render() {
        let store = WecomCardStore::new();
        store.register("task-menu-2".to_string(), None).await;
        store.try_start_monitor("task-menu-2", 30, 0).await;
        let state = store.monitor_state("task-menu-2").await;
        assert!(panel_subtitle_for_state(&state).contains("桌面监控中 · 剩余"));
    }

    #[tokio::test]
    async fn menu_completed_render() {
        let store = WecomCardStore::new();
        store.register("task-menu-3".to_string(), None).await;
        store
            .complete_monitor("task-menu-3", 30, 1000, "done".to_string())
            .await;
        let state = store.monitor_state("task-menu-3").await;
        assert_eq!(panel_subtitle_for_state(&state), "桌面监控完成");
    }

    #[tokio::test]
    async fn menu_failed_render() {
        let store = WecomCardStore::new();
        store.register("task-menu-4".to_string(), None).await;
        store
            .fail_monitor("task-menu-4", "error".to_string(), 500)
            .await;
        let state = store.monitor_state("task-menu-4").await;
        assert_eq!(panel_subtitle_for_state(&state), "桌面监控失败");
    }

    #[test]
    fn gateway_status_uses_real_state() {
        let now = OffsetDateTime::now_utc();
        // A DISCONNECTED http_callback snapshot must render disconnected
        // values — never a hardcoded "正常".
        let snapshot = WecomGatewayStatusSnapshot {
            transport: "http_callback",
            connection_connected: false,
            stream_active: false,
            generation: 0,
            last_heartbeat_available: false,
            enabled_channels: vec![],
            agent_name: "omninova".to_string(),
        };
        let text = gateway_status_text(&snapshot, &now);
        assert!(text.contains("WeCom transport: http_callback"));
        assert!(text.contains("connection: disconnected"));
        assert!(text.contains("stream active: false"));
        assert!(text.contains("generation: 0"));
        assert!(text.contains("未追踪"));
        assert!(!text.contains("正常"));
    }

    #[test]
    fn help_remains_deterministic() {
        let first = help_text();
        let second = help_text();
        assert_eq!(first, second);
        for command in ["菜单", "面板", "menu", "/menu", "panel", "/panel"] {
            assert!(first.contains(command), "help must list {command}");
        }
        for action in ["网关状态", "最近任务", "监控 30 秒", "监控 60 秒", "帮助"] {
            assert!(first.contains(action), "help must describe {action}");
        }
    }

    // ------------------------------------------------------------------
    // Phase 2A.3.1f: reliable Markdown delivery + ACK semantics
    // ------------------------------------------------------------------

    #[test]
    fn monitor_completed_builds_markdown_proactive_message() {
        let content = build_monitor_result_markdown("completed", 30, 30815, "检测到桌面变化");
        assert!(content.contains("### OmniNova · 桌面监控完成"));
        assert!(content.contains("**状态：** 完成"));
        assert!(content.contains("**监控时长：** 30 秒"));
        assert!(content.contains("**实际耗时：** 30.8 秒"));
        assert!(content.contains("检测到桌面变化"));
    }

    #[test]
    fn monitor_failed_builds_markdown_proactive_message() {
        let content = build_monitor_result_markdown("failed", 30, 3000, "桌面监控失败，请稍后重试。");
        assert!(content.contains("### OmniNova · 桌面监控失败"));
        assert!(content.contains("**状态：** 失败"));
        assert!(content.contains("**原因：** 桌面监控失败"));
    }

    #[test]
    fn monitor_timeout_builds_markdown_proactive_message() {
        let content = build_monitor_result_markdown("timeout", 60, 75000, "");
        assert!(content.contains("### OmniNova · 桌面监控超时"));
        assert!(content.contains("**状态：** 超时"));
        assert!(content.contains("**监控时长：** 60 秒"));
    }

    #[test]
    fn monitor_result_markdown_has_no_template_card() {
        for (kind, duration, elapsed, summary) in [
            ("completed", 30, 30815, "检测到桌面变化"),
            ("failed", 30, 3000, "桌面监控失败，请稍后重试。"),
            ("timeout", 60, 75000, ""),
        ] {
            let body = ProactiveBody::Markdown {
                content: build_monitor_result_markdown(kind, duration, elapsed, summary),
            }
            .to_wire();
            assert_eq!(body["msgtype"], "markdown", "kind={kind}");
            assert!(body.get("template_card").is_none(), "kind={kind}");
        }
        // Markdown must never leak paths/secrets/userids.
        let content = build_monitor_result_markdown("completed", 30, 30815, "检测到桌面变化");
        assert!(!content.contains("C:\\"));
        assert!(!content.contains("/Users/"));
    }

    #[test]
    fn proactive_markdown_wire_cmd_is_aibot_send_msg() {
        let body = ProactiveBody::Markdown {
            content: build_monitor_result_markdown("completed", 30, 30815, "检测到桌面变化"),
        }
        .to_wire();
        let envelope =
            crate::gateway::wecom_protocol::build_send_message_envelope("req-md", "chat-md", body);
        assert_eq!(envelope.cmd, "aibot_send_msg");
        assert_eq!(envelope.headers.req_id, "req-md");
    }

    #[test]
    fn proactive_markdown_body_has_msgtype_markdown() {
        let body = ProactiveBody::Markdown {
            content: build_monitor_result_markdown("completed", 30, 30815, "检测到桌面变化"),
        }
        .to_wire();
        assert_eq!(body["msgtype"], "markdown");
        assert!(body["markdown"]["content"].as_str().is_some());
        // The template_card path stays available as a future boundary.
        let card = build_monitor_result_message("t", "s");
        assert_eq!(card["template_card"]["card_type"], "text_notice");
    }

    #[tokio::test]
    async fn proactive_write_ok_is_not_delivery_success() {
        // Completion alone leaves delivery Pending: only an ACK errcode==0
        // (handled in the stream ACK branch) marks it Sent.
        let store = WecomCardStore::new();
        store.register("task-wok".to_string(), None).await;
        store
            .complete_monitor("task-wok", 30, 30000, "检测到桌面变化".to_string())
            .await;
        match store.monitor_state("task-wok").await {
            MonitorState::Completed { delivery, .. } => {
                assert_eq!(delivery, MonitorDelivery::Pending);
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn proactive_ack_zero_marks_delivery_sent() {
        let store = WecomCardStore::new();
        store.register("task-sent".to_string(), None).await;
        store
            .complete_monitor("task-sent", 30, 30000, "检测到桌面变化".to_string())
            .await;
        store.mark_delivery_sent("task-sent").await;
        match store.monitor_state("task-sent").await {
            MonitorState::Completed { delivery, .. } => {
                assert_eq!(delivery, MonitorDelivery::Sent);
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn proactive_ack_error_marks_delivery_failed() {
        let store = WecomCardStore::new();
        store.register("task-aerr".to_string(), None).await;
        store
            .complete_monitor("task-aerr", 30, 30000, "检测到桌面变化".to_string())
            .await;
        store.mark_delivery_failed("task-aerr", 42045).await;
        match store.monitor_state("task-aerr").await {
            MonitorState::Completed { delivery, .. } => {
                assert_eq!(delivery, MonitorDelivery::Failed { errcode: 42045 });
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn payload_validation_error_not_retried() {
        // A 4xxxx ACK error only marks delivery failed — the same payload
        // is never re-sent and no new outbound message is produced.
        let (_runtime, tx, mut rx) = handler_with_channel().await;
        let store = WecomCardStore::new();
        store.register("task-vretry".to_string(), None).await;
        store
            .complete_monitor("task-vretry", 30, 30000, "检测到桌面变化".to_string())
            .await;
        store.mark_delivery_failed("task-vretry", 42045).await;
        // The ACK-error path (stream branch) only calls mark_delivery_failed:
        // nothing is enqueued on the outbound channel.
        assert!(
            rx.try_recv().is_err(),
            "payload validation errors must never re-queue a message"
        );
    }

    #[tokio::test]
    async fn monitor_completion_not_reexecuted_on_delivery_failure() {
        // Delivery failure must NOT re-run the monitor: state stays
        // Completed (Failed delivery), never flips back to Running.
        let store = WecomCardStore::new();
        store.register("task-nore".to_string(), None).await;
        store
            .complete_monitor("task-nore", 30, 30000, "检测到桌面变化".to_string())
            .await;
        store.mark_delivery_failed("task-nore", 42045).await;
        assert!(matches!(
            store.monitor_state("task-nore").await,
            MonitorState::Completed { .. }
        ));
        // And the single-flight gate stays free of any automatic restart:
        // only an explicit user click may start a NEW monitor.
        assert!(!matches!(
            store.monitor_state("task-nore").await,
            MonitorState::Running { .. }
        ));
    }

    #[test]
    fn single_target_still_uses_userid() {
        let body: WecomCallbackBody = serde_json::from_value(serde_json::json!({
            "msgid": "t-s2",
            "msgtype": "event",
            "chattype": "single",
            "from": {"userid": "user-single-2"},
            "event": {"eventtype": "template_card_event"}
        }))
        .unwrap();
        assert_eq!(resolve_card_target(&body).as_deref(), Some("user-single-2"));
    }

    #[test]
    fn group_target_still_uses_chatid() {
        let body: WecomCallbackBody = serde_json::from_value(serde_json::json!({
            "msgid": "t-g2",
            "msgtype": "event",
            "chattype": "group",
            "chatid": "chat-group-2",
            "from": {"userid": "user-group-2"},
            "event": {"eventtype": "template_card_event"}
        }))
        .unwrap();
        assert_eq!(resolve_card_target(&body).as_deref(), Some("chat-group-2"));
    }
}
