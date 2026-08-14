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

/// Panel card constants.
pub const PANEL_TITLE: &str = "OmniNova 控制面板";
pub const PANEL_DESC: &str = "请选择操作";
pub const PANEL_EXPIRED_TEXT: &str = "该操作面板已过期，请重新打开。";
pub const UNKNOWN_ACTION_TEXT: &str = "未知操作，已忽略。";
pub const CARD_TTL_SECS: i64 = 30 * 60;
pub const RECENT_JOBS_LIMIT: usize = 5;

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

/// Build the `template_card` object for the OmniNova control panel.
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

/// Panel card with an action result shown in `sub_title_text`
/// (card updates keep the buttons; the result replaces the subtitle).
pub fn build_panel_card_with_text(task_id: &str, text: &str) -> serde_json::Value {
    let mut card = build_panel_card(task_id);
    card["sub_title_text"] = serde_json::json!(text);
    card
}

// ---------------------------------------------------------------------------
// Card state store (in-memory, 30-min TTL, event dedup, monitor single-flight)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WecomCardStatus {
    Active,
    MonitorRunning,
}

#[derive(Debug, Clone)]
pub struct WecomCardState {
    pub task_id: String,
    pub session_key: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_action: Option<String>,
    pub status: WecomCardStatus,
}

/// One entry of the WeCom recent-jobs log (redacted summary only).
#[derive(Debug, Clone)]
pub struct WecomRecentJob {
    pub chat_type: String,
    pub status: String,
    pub created_at: i64,
}

struct WecomCardStoreInner {
    cards: HashMap<String, WecomCardState>,
    /// Event msgids already handled per task (retry dedup).
    seen_events: HashMap<String, HashSet<String>>,
    /// Single-flight monitor guard per task_id.
    monitor_running: HashSet<String>,
    /// Last completed monitor result per task (shown on the next interaction).
    monitor_results: HashMap<String, String>,
    /// req_ids of card replies awaiting their ACK (for card_ack_ok logging).
    pending_card_req_ids: HashSet<String>,
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
                monitor_running: HashSet::new(),
                monitor_results: HashMap::new(),
                pending_card_req_ids: HashSet::new(),
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
                status: WecomCardStatus::Active,
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
            inner.monitor_running.remove(task_id);
            inner.monitor_results.remove(task_id);
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

    /// Single-flight admission for monitor actions on a card.
    pub async fn try_start_monitor(&self, task_id: &str) -> bool {
        let mut inner = self.inner.lock();
        if inner.monitor_running.contains(task_id) {
            return false;
        }
        inner.monitor_running.insert(task_id.to_string());
        if let Some(state) = inner.cards.get_mut(task_id) {
            state.status = WecomCardStatus::MonitorRunning;
            state.updated_at = Self::now();
        }
        true
    }

    /// Finish a monitor: store the result text for the next interaction.
    pub async fn finish_monitor(&self, task_id: &str, result: String) {
        let mut inner = self.inner.lock();
        inner.monitor_running.remove(task_id);
        inner
            .monitor_results
            .insert(task_id.to_string(), result);
        if let Some(state) = inner.cards.get_mut(task_id) {
            state.status = WecomCardStatus::Active;
            state.updated_at = Self::now();
        }
    }

    /// Take (and clear) the last completed monitor result.
    pub async fn take_monitor_result(&self, task_id: &str) -> Option<String> {
        let mut inner = self.inner.lock();
        inner.monitor_results.remove(task_id)
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

    /// Track a card reply req_id until its ACK arrives.
    pub async fn note_card_reply_req_id(&self, req_id: &str) {
        let mut inner = self.inner.lock();
        inner.pending_card_req_ids.insert(req_id.to_string());
    }

    /// Returns true (and clears the entry) when the ACK belongs to a
    /// card reply; used to emit `[wecom-card] card_ack_ok`.
    pub async fn consume_card_reply_ack(&self, req_id: &str) -> bool {
        let mut inner = self.inner.lock();
        inner.pending_card_req_ids.remove(req_id)
    }

    /// Test-only reset of the global store.
    #[cfg(test)]
    pub async fn reset(&self) {
        let mut inner = self.inner.lock();
        inner.cards.clear();
        inner.seen_events.clear();
        inner.monitor_running.clear();
        inner.monitor_results.clear();
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

/// Pure, deterministic gateway status text (no secrets, no LLM).
pub fn gateway_status_text(
    enabled_channels: &[&str],
    agent_name: &str,
    now: &OffsetDateTime,
) -> String {
    let channel_list = if enabled_channels.is_empty() {
        "无".to_string()
    } else {
        enabled_channels.join("、")
    };
    let time_text = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| now.unix_timestamp().to_string());
    format!(
        "Gateway 状态\n\n运行中\n启用渠道：{}\nAgent：{}\n时间：{}",
        channel_list, agent_name, time_text
    )
}

/// Fixed help text (no LLM).
pub fn help_text() -> &'static str {
    "OmniNova 使用帮助\n\n普通聊天：直接发消息，如“你好”。\n工具任务：使用 /monitor 桌面 30秒 / /monitor 桌面 60秒。\n控制面板：发送 菜单 / 面板 / /panel。\n安全说明：高风险工具默认不在聊天中直接执行。"
}

// ---------------------------------------------------------------------------
// Shared card action boundary (Phase 2A.3.1d)
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
        context: CardActionContext,
    ) -> CardActionResult {
        store
            .touch_action(&context.task_id, context.action.key())
            .await;
        match context.action {
            WecomPanelAction::GatewayStatus => {
                let config = runtime.get_config().await;
                let channels = enabled_channels(&config);
                let channels_ref: Vec<&str> = channels.iter().copied().collect();
                CardActionResult {
                    title: "网关状态",
                    content: gateway_status_text(
                        &channels_ref,
                        &config.agent.name,
                        &OffsetDateTime::now_utc(),
                    ),
                }
            }
            WecomPanelAction::RecentJobs => CardActionResult {
                title: "最近任务",
                content: store.recent_jobs_text().await,
            },
            WecomPanelAction::Monitor30 => CardActionResult {
                title: "桌面监控 · 30 秒",
                content: start_monitor(store, &context.task_id, 30).await,
            },
            WecomPanelAction::Monitor60 => CardActionResult {
                title: "桌面监控 · 60 秒",
                content: start_monitor(store, &context.task_id, 60).await,
            },
            WecomPanelAction::Help => CardActionResult {
                title: "帮助",
                content: help_text().to_string(),
            },
        }
    }
}

/// Monitor action: single-flight admission, immediate "监控中..." card
/// update, background desktop monitor (shared business service), result
/// stored for the next interaction. Phase 2A.3.1 does not proactively
/// push the final result (no unsolicited frame in the long-connection
/// MVP); it is shown on the next card interaction.
async fn start_monitor(store: &WecomCardStore, task_id: &str, duration_secs: u64) -> String {
    if !store.try_start_monitor(task_id).await {
        println!(
            "[wecom-card] monitor_admission=busy task_id={}",
            short_hash(task_id)
        );
        return "桌面监控进行中，请稍候…".to_string();
    }
    println!(
        "[wecom-card] monitor_admission=acquired task_id={} duration={}",
        short_hash(task_id),
        duration_secs
    );

    let store_for_job = store.clone();
    let task_for_job = task_id.to_string();
    tokio::spawn(async move {
        let captures_dir = directories::ProjectDirs::from("com", "omninova", "OmniNova")
            .map(|dirs| dirs.config_dir().join("captures"))
            .unwrap_or_else(|| std::env::temp_dir().join("omninova-captures"));
        let result = crate::desktop_capture::monitor_desktop(&captures_dir, duration_secs).await;
        let text = if result.ok {
            let detail = if result.changed.unwrap_or(false) {
                "检测到桌面变化"
            } else {
                "未检测到明显变化"
            };
            format!(
                "桌面监控完成\n\n监控时长：{duration_secs} 秒\n结果：{detail}"
            )
        } else {
            "桌面监控失败，请稍后重试。".to_string()
        };
        store_for_job.finish_monitor(&task_for_job, text).await;
        println!(
            "[wecom-card] monitor_completed task_id={}",
            short_hash(&task_for_job)
        );
    });

    let previous = store.take_monitor_result(task_id).await;
    match previous {
        Some(previous) => format!("上次结果：{previous}\n\n监控中...（{duration_secs} 秒桌面监控已启动）"),
        None => format!("监控中...（{duration_secs} 秒桌面监控已启动）"),
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
    println!(
        "[wecom-card] event_shape msgtype_event={} eventtype_present={} eventtype_template_card={} event_key_present={} task_id_present={} card_type_present={} userid_present={} chatid_present={}",
        body.msgtype.as_deref() == Some("event"),
        event.and_then(|e| e.eventtype.as_ref()).is_some(),
        event.and_then(|e| e.eventtype.as_deref()) == Some("template_card_event"),
        event.and_then(|e| e.event_key.as_deref()).map(|v| !v.trim().is_empty()).unwrap_or(false),
        event.and_then(|e| e.task_id.as_deref()).map(|v| !v.trim().is_empty()).unwrap_or(false),
        event.and_then(|e| e.card_type.as_deref()).is_some(),
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
    };
    let result = WecomCardActionService::execute(runtime, store, context).await;
    let card = build_panel_card_with_text(task_id, &result.content);
    println!(
        "[wecom-card] card_dispatch_requested task_id={} update=true",
        short_hash(task_id)
    );
    store.note_card_reply_req_id(req_id).await;
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
        let text = gateway_status_text(&["wecom", "dingtalk"], "omninova", &now);
        assert!(text.contains("Gateway 状态"));
        assert!(text.contains("启用渠道：wecom、dingtalk"));
        assert!(text.contains("Agent：omninova"));
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
        assert!(store.try_start_monitor("task-m30").await);
        assert!(!store.try_start_monitor("task-m30").await);
        store.finish_monitor("task-m30", "done".to_string()).await;
        assert!(store.try_start_monitor("task-m30").await);
    }

    #[tokio::test]
    async fn wecom_monitor_60_singleflight() {
        let store = WecomCardStore::new();
        store.register("task-m60".to_string(), None).await;
        assert!(store.try_start_monitor("task-m60").await);
        assert!(!store.try_start_monitor("task-m60").await);
        // A different card can still start its own monitor.
        store.register("task-other".to_string(), None).await;
        assert!(store.try_start_monitor("task-other").await);
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
}
