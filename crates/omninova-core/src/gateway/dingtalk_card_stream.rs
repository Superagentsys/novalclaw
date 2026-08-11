//! DingTalk Stream client for advanced-card callbacks.
//!
//! The callback frame is acknowledged before any business work is spawned.
//! Card actions are processed asynchronously with dedupe and single-flight guards.

use crate::config::schema::DingtalkTransportMode;
use crate::gateway::agent_menu::canonical_agent_menu_action;
use crate::gateway::dingtalk_card;
use crate::gateway::GatewayRuntime;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::sync::RwLock;
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;

const DINGTALK_STREAM_GATEWAY_URL: &str = "https://api.dingtalk.com/v1.0/gateway/connections/open";
pub const DINGTALK_CARD_CALLBACK_TOPIC: &str = "/v1.0/card/instances/callback";

/// In-memory dedupe cache for card callbacks to prevent duplicate action execution.
/// Uses HashSet with max size to prevent unbounded memory growth.
pub struct CallbackDedupeCache {
    #[cfg(test)]
    pub entries: RwLock<HashMap<String, Instant>>,
    #[cfg(not(test))]
    entries: RwLock<HashMap<String, Instant>>,
    max_size: usize,
    ttl: Duration,
}

impl CallbackDedupeCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: RwLock::new(HashMap::with_capacity(max_size)),
            max_size,
            ttl: Duration::from_secs(5 * 60),
        }
    }

    /// Try to insert a callback key. Returns false if already present (duplicate).
    pub fn try_insert(&self, key: &str) -> bool {
        let mut entries = self.entries.write().unwrap();
        let now = Instant::now();
        entries.retain(|_, inserted_at| now.duration_since(*inserted_at) < self.ttl);
        if entries.contains_key(key) {
            return false;
        }
        if entries.len() >= self.max_size {
            if let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, inserted_at)| **inserted_at)
                .map(|(key, _)| key.clone())
            {
                entries.remove(&oldest);
            }
        }
        entries.insert(key.to_string(), now);
        true
    }

    /// Remove a callback key (for testing)
    #[cfg(test)]
    pub fn remove(&self, key: &str) {
        self.entries.write().unwrap().remove(key);
    }
}

impl Default for CallbackDedupeCache {
    fn default() -> Self {
        Self::new(1000)
    }
}

pub struct DingtalkCardStreamGuard {
    shutdown: Option<watch::Sender<bool>>,
    runtime: Arc<GatewayRuntime>,
}

impl Drop for DingtalkCardStreamGuard {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(true);
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ParsedCardCallback {
    pub out_track_id: String,
    pub action: String,
    pub callback_id: Option<String>,
    pub user_id: Option<String>,
    pub space_id: Option<String>,
}

impl std::fmt::Debug for ParsedCardCallback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ParsedCardCallback")
            .field("out_track_id_present", &!self.out_track_id.is_empty())
            .field("action", &safe_action(&self.action))
            .field("callback_id_present", &self.callback_id.is_some())
            .field("user_id_present", &self.user_id.is_some())
            .field("space_id_present", &self.space_id.is_some())
            .finish()
    }
}

pub async fn start(runtime: Arc<GatewayRuntime>) -> Option<DingtalkCardStreamGuard> {
    let config = runtime.get_config().await;
    let entry = config.channels_config.dingtalk.as_ref();
    let enabled =
        config.gateway.dingtalk.enabled || entry.map(|entry| entry.enabled).unwrap_or(false);
    if !enabled {
        println!("[dingtalk-card-stream] start_skipped reason=channel_disabled");
        return None;
    }
    let transport_mode = crate::gateway::resolve_dingtalk_transport_mode_for_worker(&config, entry);
    if !should_start_card_stream(enabled, transport_mode) {
        println!("[dingtalk-card-stream] start_skipped reason=unsupported_transport mode=http");
        return None;
    }
    let app_key = crate::gateway::resolve_dingtalk_app_key_for_worker(&config, entry);
    let app_secret = crate::gateway::resolve_dingtalk_secret_for_worker(&config, entry);
    let template_id = crate::gateway::resolve_dingtalk_card_template_for_worker(&config, entry);
    println!(
        "[dingtalk-card-stream] config app_key_present={} app_secret_present={} template_configured={}",
        app_key.is_some(),
        app_secret.is_some(),
        template_id.is_some()
    );
    let (Some(app_key), Some(app_secret)) = (app_key, app_secret) else {
        println!("[dingtalk-card-stream] start_skipped reason=incomplete_configuration");
        return None;
    };

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let dedupe_cache = Arc::new(CallbackDedupeCache::default());
    tokio::spawn(run_reconnecting(
        runtime.clone(),
        app_key,
        app_secret,
        shutdown_rx,
        dedupe_cache,
    ));
    Some(DingtalkCardStreamGuard {
        shutdown: Some(shutdown_tx),
        runtime,
    })
}

pub fn should_start_card_stream(
    channel_enabled: bool,
    transport_mode: DingtalkTransportMode,
) -> bool {
    channel_enabled && transport_mode == DingtalkTransportMode::Stream
}

async fn run_reconnecting(
    runtime: Arc<GatewayRuntime>,
    app_key: String,
    app_secret: String,
    mut shutdown: watch::Receiver<bool>,
    dedupe_cache: Arc<CallbackDedupeCache>,
) {
    let mut attempt = 0u32;
    loop {
        if *shutdown.borrow() {
            break;
        }
        match connect_once(runtime.clone(), &app_key, &app_secret, shutdown.clone(), dedupe_cache.clone()).await {
            Ok(()) => attempt = 0,
            Err(error) => {
                attempt = attempt.saturating_add(1);
                println!(
                    "[dingtalk-card-stream] disconnected reason={} reconnect_attempt={}",
                    safe_error_kind(&error),
                    attempt
                );
            }
        }
        let delay = reconnect_delay(attempt);
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() { break; }
            }
            _ = tokio::time::sleep(delay) => {}
        }
    }
    println!("[dingtalk-card-stream] stopped=true");
}

async fn connect_once(
    runtime: Arc<GatewayRuntime>,
    app_key: &str,
    app_secret: &str,
    mut shutdown: watch::Receiver<bool>,
    dedupe_cache: Arc<CallbackDedupeCache>,
) -> Result<(), String> {
    let (endpoint, ticket) = request_stream_connection(app_key, app_secret).await?;
    let mut url =
        reqwest::Url::parse(&endpoint).map_err(|_| "invalid_stream_endpoint".to_string())?;
    url.query_pairs_mut().append_pair("ticket", &ticket);
    let (mut socket, _) = tokio_tungstenite::connect_async(url.as_str())
        .await
        .map_err(|_| "stream_connect_error".to_string())?;
    println!("[dingtalk-card-stream] connected=true topic=card_callback");

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    // Close the socket gracefully by dropping
                    return Ok(());
                }
            }
            frame = socket.next() => {
                let Some(frame) = frame else { return Err("stream_closed".to_string()); };
                let frame = frame.map_err(|_| "stream_read_error".to_string())?;
                match frame {
                    Message::Text(text) => {
                        handle_text_frame(&mut socket, runtime.clone(), text.as_ref(), dedupe_cache.clone()).await?;
                    }
                    Message::Binary(bytes) => {
                        if let Ok(text) = std::str::from_utf8(&bytes) {
                            handle_text_frame(&mut socket, runtime.clone(), text, dedupe_cache.clone()).await?;
                        }
                    }
                    Message::Ping(payload) => {
                        socket.send(Message::Pong(payload)).await.map_err(|_| "stream_write_error".to_string())?;
                    }
                    Message::Close(_) => return Err("stream_closed".to_string()),
                    _ => {}
                }
            }
        }
    }
}

async fn request_stream_connection(
    app_key: &str,
    app_secret: &str,
) -> Result<(String, String), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "stream_http_client_error".to_string())?;
    let response = client
        .post(DINGTALK_STREAM_GATEWAY_URL)
        .json(&serde_json::json!({
            "clientId": app_key,
            "clientSecret": app_secret,
            "ua": "omninova-claw-rust/0.1",
            "subscriptions": [{
                "type": "CALLBACK",
                "topic": DINGTALK_CARD_CALLBACK_TOPIC
            }]
        }))
        .send()
        .await
        .map_err(|_| "stream_gateway_network_error".to_string())?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|_| "stream_gateway_read_error".to_string())?;
    let payload: Value =
        serde_json::from_str(&body).map_err(|_| "stream_gateway_invalid_json".to_string())?;
    if !(200..300).contains(&status) {
        let code = payload
            .get("code")
            .map(safe_json_scalar)
            .unwrap_or_else(|| "unknown".to_string());
        println!(
            "[dingtalk-card-stream] gateway_failed http_status={} platform_code={} body_len={}",
            status,
            code,
            body.len()
        );
        return Err(format!("stream_gateway_http_error:{status}:{code}"));
    }
    let data = payload.get("data").unwrap_or(&payload);
    let endpoint = data
        .get("endpoint")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "stream_endpoint_missing".to_string())?;
    let ticket = data
        .get("ticket")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "stream_ticket_missing".to_string())?;
    Ok((endpoint.to_string(), ticket.to_string()))
}

async fn handle_text_frame<S>(
    socket: &mut S,
    runtime: Arc<GatewayRuntime>,
    text: &str,
    dedupe_cache: Arc<CallbackDedupeCache>,
) -> Result<(), String>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let envelope: Value =
        serde_json::from_str(text).map_err(|_| "invalid_stream_frame".to_string())?;
    let frame_type = envelope
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let topic = envelope
        .get("headers")
        .and_then(|headers| headers.get("topic"))
        .and_then(Value::as_str)
        .unwrap_or_default();

    if frame_type == "SYSTEM" {
        let ack = build_stream_ack(&envelope, serde_json::json!({}));
        socket
            .send(Message::Text(ack.to_string().into()))
            .await
            .map_err(|_| "stream_ack_error".to_string())?;
        return Ok(());
    }
    if frame_type != "CALLBACK" || topic != DINGTALK_CARD_CALLBACK_TOPIC {
        return Ok(());
    }

    // Parse callback to extract action and identity
    let callback = match parse_card_callback_envelope(&envelope) {
        Ok(cb) => cb,
        Err(error) => {
            println!(
                "[dingtalk-card] callback_parse_failed reason={}",
                safe_error_kind(&error)
            );
            // Still ACK to prevent DingTalk retry
            let ack = build_stream_ack(&envelope, serde_json::json!({ "response": {} }));
            let _ = socket.send(Message::Text(ack.to_string().into())).await;
            return Ok(());
        }
    };

    // Canonicalize action
    let canonical_action = match canonical_agent_menu_action(&callback.action) {
        Some(action) => action,
        None => {
            println!(
                "[dingtalk-card] action_rejected action={} reason=not_allowed",
                safe_action(&callback.action)
            );
            let ack = build_stream_ack(&envelope, serde_json::json!({ "response": {} }));
            let _ = socket.send(Message::Text(ack.to_string().into())).await;
            return Ok(());
        }
    };

    // Dedupe a delivery retry, not every future click on the same card.
    // DingTalk's Stream `messageId` identifies this callback delivery.
    let envelope_callback_id = envelope
        .get("headers")
        .and_then(|headers| {
            headers
                .get("messageId")
                .or_else(|| headers.get("time"))
        })
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());
    let dedupe_key = callback_dedupe_key(
        callback.callback_id.as_deref().or(envelope_callback_id),
        &callback.out_track_id,
        canonical_action,
        text,
    );
    if !dedupe_cache.try_insert(&dedupe_key) {
        println!(
            "[dingtalk-card] callback_duplicated dedupe_key_hash={}",
            opaque_short_hash(&dedupe_key)
        );
        let ack = build_stream_ack(&envelope, serde_json::json!({ "response": {} }));
        let _ = socket.send(Message::Text(ack.to_string().into())).await;
        return Ok(());
    }

    // ACK immediately - before long-running work
    let ack = build_stream_ack(&envelope, serde_json::json!({ "response": {} }));
    match socket.send(Message::Text(ack.to_string().into())).await {
        Ok(()) => {
            println!("[dingtalk-card] ack_ok=true");
        }
        Err(e) => {
            println!("[dingtalk-card] ack_failed reason=websocket_write");
            return Err(format!("stream_ack_error:{}", e));
        }
    }

    // Dispatch action
    println!(
        "[dingtalk-card] callback_received action={} accepted=true",
        canonical_action
    );

    tokio::spawn(process_panel_action(runtime, callback));

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PanelActionResult {
    pub action: &'static str,
    pub card_summary: String,
    pub message_body: Option<String>,
    pub success: bool,
    pub busy: bool,
}

impl PanelActionResult {
    pub(crate) fn success(
        action: &'static str,
        card_summary: impl Into<String>,
        body: String,
    ) -> Self {
        Self {
            action,
            card_summary: card_summary.into(),
            message_body: Some(body),
            success: true,
            busy: false,
        }
    }

    pub(crate) fn failed(action: &'static str, card_summary: impl Into<String>, body: String) -> Self {
        Self {
            action,
            card_summary: card_summary.into(),
            message_body: Some(body),
            success: false,
            busy: false,
        }
    }

    pub(crate) fn busy(action: &'static str) -> Self {
        Self {
            action,
            card_summary: "已有桌面监控任务正在运行".to_string(),
            message_body: None,
            success: false,
            busy: true,
        }
    }
}

/// RAII guard that releases the DingtalkMonitorGuard when dropped.
/// The guard is stored inside the runtime's monitor guard; this struct just
/// holds the out_track_id so Drop can call release.
pub(crate) struct DingtalkMonitorLease {
    guard: Arc<crate::gateway::DingtalkMonitorGuard>,
    out_track_id: String,
    owner_id: String,
}

impl Drop for DingtalkMonitorLease {
    fn drop(&mut self) {
        // We cannot use async Drop, so release synchronously.
        // SAFETY: the guard uses tokio::RwLock which is not Sync, but Drop is sync
        // and we're just taking the Arc. The release will happen on a best-effort
        // basis. For production correctness, use a spawn + forget pattern.
        let guard = self.guard.clone();
        let out_track_id = self.out_track_id.clone();
        let owner_id = self.owner_id.clone();
        tokio::spawn(async move {
            let _ = guard.release(&out_track_id, &owner_id).await;
        });
    }
}

pub(crate) async fn process_panel_action(
    runtime: Arc<GatewayRuntime>,
    callback: ParsedCardCallback,
) {
    let Some(action) = canonical_agent_menu_action(&callback.action) else {
        println!("[dingtalk-panel] action_rejected=true");
        return;
    };
    println!("[dingtalk-panel] action={action}");

    // -------------------------------------------------------------------------
    // Admission-first for monitor actions: prevent concurrent monitor executions.
    // This MUST happen before claiming the card generation.
    // -------------------------------------------------------------------------
    let monitor_lease: Option<DingtalkMonitorLease> =
        if matches!(action, "monitor_30s" | "monitor_60s") {
            let guard = runtime.dingtalk_monitor_guard();
            match guard.try_acquire(&callback.out_track_id).await {
                Some(owner_id) => {
                    println!("[dingtalk-panel] monitor_admission=acquired action={action}");
                    Some(DingtalkMonitorLease {
                        guard,
                        out_track_id: callback.out_track_id.clone(),
                        owner_id,
                    })
                }
                None => {
                    println!(
                        "[dingtalk-panel] monitor_admission=busy action={action}"
                    );
                    // BUSY without claiming generation — does not affect the
                    // running monitor's generation.
                    return;
                }
            }
        } else {
            None
        };

    let config = runtime.get_config().await;
    let entry = config.channels_config.dingtalk.as_ref();
    let app_key = crate::gateway::resolve_dingtalk_app_key_for_worker(&config, entry);
    let app_secret = crate::gateway::resolve_dingtalk_secret_for_worker(&config, entry);
    let (Some(app_key), Some(app_secret)) = (app_key, app_secret) else {
        println!("[dingtalk-panel] action_failed=true reason=missing_credentials");
        return;
    };
    let token = match crate::gateway::dingtalk_worker::fetch_dingtalk_access_token(
        &app_key,
        &app_secret,
    )
    .await
    {
        Ok(token) => token,
        Err(error) => {
            println!(
                "[dingtalk-panel] action_failed=true reason={}",
                safe_error_kind(&error)
            );
            return;
        }
    };

    // Claim ownership of the card's UI. Concurrent callbacks for the same
    // `out_track_id` will get distinct generations, so only the latest action
    // can complete the final READY update.
    let store = runtime.dingtalk_store();
    let Some(store) = store else {
        println!("[dingtalk-panel] action_failed=true reason=no_store");
        return;
    };
    let generation = store.claim_card_generation(&callback.out_track_id).await;
    let track_hash = opaque_short_hash(&callback.out_track_id);
    println!(
        "[dingtalk-panel] operation_started generation={generation} track_hash={track_hash} action={action}"
    );

    // Lookup + slide TTL atomically: a still-live panel will receive a
    // refresh so the next action is not penalised.
    let lookup = store
        .lookup_and_touch(&callback.out_track_id)
        .await;
    let lookup_log = match lookup {
        crate::gateway::dingtalk_store::PanelContextLookup::Hit(_) => "hit",
        crate::gateway::dingtalk_store::PanelContextLookup::Missing => "missing",
        crate::gateway::dingtalk_store::PanelContextLookup::Expired => "expired",
    };
    println!("[dingtalk-panel] context_lookup={lookup_log}");
    let context = match lookup {
        crate::gateway::dingtalk_store::PanelContextLookup::Hit(context) => context,
        _ => {
            // Card update must respect ownership: if we lost (or never had) a
            // generation, skip the update entirely.
            if store
                .is_card_generation_current(&callback.out_track_id, generation)
                .await
            {
                let _ = dingtalk_card::update_card(
                    &token,
                    &callback.out_track_id,
                    "READY",
                    "面板已失效",
                    "面板已失效，请重新发送 menu 打开新的控制面板。",
                    panel_action_label(action),
                )
                .await;
                println!(
                    "[dingtalk-panel] operation_completed generation={generation} reason=context_lost"
                );
            } else {
                println!(
                    "[dingtalk-panel] card_update_skipped reason=stale_generation generation={generation}"
                );
            }
            return;
        }
    };

    let running_summary = panel_running_summary(action);
    // RUNNING update is the *first* UI update; only emit it if we still
    // own the card — if a fresher action already took over, skip.
    if !store
        .is_card_generation_current(&callback.out_track_id, generation)
        .await
    {
        println!(
            "[dingtalk-panel] card_update_skipped state=RUNNING reason=stale_generation generation={generation}"
        );
        return;
    }
    if let Err(error) = dingtalk_card::update_card(
        &token,
        &callback.out_track_id,
        "RUNNING",
        running_summary,
        running_summary,
        panel_action_label(action),
    )
    .await
    {
        println!(
            "[dingtalk-panel] card_running_failed=true reason={}",
            safe_error_kind(&error)
        );
        return;
    }

    let result = match action {
        "gateway_status" => execute_gateway_status(&runtime, &context).await,
        "recent_jobs" => execute_recent_jobs(&runtime).await,
        "monitor_30s" => execute_monitor(&runtime, &context, 30).await,
        "monitor_60s" => execute_monitor(&runtime, &context, 60).await,
        "help" => execute_help(),
        _ => PanelActionResult::failed(action, "操作失败", "不支持的面板操作。".to_string()),
    };

    // BUSY on a monitor means a different monitor guard already owns the
    // desktop capture. We refuse to bump the generation here — the
    // successful monitor's later success transition still owns the card.
    if result.busy {
        if store
            .is_card_generation_current(&callback.out_track_id, generation)
            .await
        {
            let _ = dingtalk_card::update_card(
                &token,
                &callback.out_track_id,
                "BUSY",
                "正在执行其他任务",
                &result.card_summary,
                panel_action_label(result.action),
            )
            .await;
        } else {
            println!(
                "[dingtalk-panel] card_update_skipped state=BUSY reason=stale_generation generation={generation}"
            );
        }
        println!(
            "[dingtalk-panel] operation_completed generation={generation} state=busy"
        );
        return;
    }

    // Send detailed message. Failure here does NOT rerun the business
    // handler later — only the final card update can be retried.
    let message_sent = if let Some(message) = result.message_body.as_deref() {
        println!("[dingtalk-panel] detailed_reply_send=true");
        match crate::gateway::dingtalk_worker::send_dingtalk_panel_reply(
            &runtime,
            &context,
            message,
        )
        .await
        {
            Ok(()) => {
                println!("[dingtalk-panel] detailed_reply_ok=true");
                true
            }
            Err(error) => {
                println!(
                    "[dingtalk-panel] detailed_reply_ok=false reason={}",
                    safe_error_kind(&error)
                );
                false
            }
        }
    } else {
        true
    };

    let card_summary = card_summary_after_delivery(&result, message_sent);
    let status_text = if result.success {
        "Gateway 与 DingTalk 已连接"
    } else {
        "上次操作失败"
    };
    // Terminal READY update must respect ownership: a fresher action that
    // arrived while we were doing async work gets priority.
    if !store
        .is_card_generation_current(&callback.out_track_id, generation)
        .await
    {
        println!(
            "[dingtalk-panel] card_update_skipped state=READY reason=stale_generation generation={generation}"
        );
        return;
    }
    match dingtalk_card::update_card(
        &token,
        &callback.out_track_id,
        "READY",
        status_text,
        &card_summary,
        panel_action_label(result.action),
    )
    .await
    {
        Ok(()) => println!("[dingtalk-panel] card_restored=true"),
        Err(error) => println!(
            "[dingtalk-panel] card_restored=false reason={}",
            safe_error_kind(&error)
        ),
    }
    println!(
        "[dingtalk-panel] operation_completed generation={generation} state=ready"
    );
}

fn card_summary_after_delivery(result: &PanelActionResult, message_sent: bool) -> String {
    if result.success && !message_sent {
        "操作已完成，但结果消息发送失败".to_string()
    } else {
        result.card_summary.clone()
    }
}

fn panel_running_summary(action: &str) -> &'static str {
    match action {
        "gateway_status" => "正在读取 Gateway 状态...",
        "recent_jobs" => "正在读取最近任务...",
        "monitor_30s" => "正在监控桌面 · 30 秒",
        "monitor_60s" => "正在监控桌面 · 60 秒",
        "help" => "正在准备帮助说明...",
        _ => "正在执行操作...",
    }
}

fn panel_action_label(action: &str) -> &'static str {
    let panel = crate::gateway::agent_menu::build_agent_menu_panel();
    panel
        .primary_actions
        .iter()
        .chain(panel.secondary_actions)
        .find(|item| item.action == action)
        .map(|item| item.label)
        .unwrap_or("未知操作")
}

async fn execute_gateway_status(
    runtime: &GatewayRuntime,
    context: &crate::gateway::dingtalk_store::DingtalkPanelContext,
) -> PanelActionResult {
    let worker_ready = runtime.dingtalk_worker_started().await;
    let queue_len = runtime.dingtalk_queue_len().await;
    let stream_ready = runtime.is_dingtalk_stream_registered();
    let config = runtime.get_config().await;
    let entry = config.channels_config.dingtalk.as_ref();
    let app_ready = crate::gateway::resolve_dingtalk_app_key_for_worker(&config, entry).is_some()
        && crate::gateway::resolve_dingtalk_secret_for_worker(&config, entry).is_some();
    let reply_target_ready = context.session_webhook.is_some()
        || (context.conversation_id.is_some() && context.robot_code.is_some() && app_ready);
    let body = format!(
        "Gateway 状态\n\nGateway：运行正常\nDingTalk Stream：{}\n消息 Worker：{}\n待处理任务：{}\n消息发送：{}",
        if stream_ready { "已连接" } else { "未连接" },
        if worker_ready { "就绪" } else { "未就绪" },
        queue_len,
        if reply_target_ready { "正常" } else { "未就绪" }
    );
    PanelActionResult::success("gateway_status", "Gateway 状态读取完成", body)
}

async fn execute_recent_jobs(runtime: &GatewayRuntime) -> PanelActionResult {
    let Some(store) = runtime.dingtalk_store() else {
        return PanelActionResult::failed(
            "recent_jobs",
            "最近任务读取失败",
            "最近任务\n\n当前暂无最近任务。".to_string(),
        );
    };
    let jobs = store.get_recent_jobs(5).await;
    let body = format_recent_jobs_message(&jobs);
    PanelActionResult::success("recent_jobs", "最近任务已发送", body)
}

pub(crate) fn format_recent_jobs_message(
    jobs: &[crate::gateway::dingtalk_store::DingtalkJob],
) -> String {
    if jobs.is_empty() {
        return "最近任务\n\n当前暂无最近任务。".to_string();
    }
    let mut lines = vec!["最近任务".to_string(), String::new()];
    for (index, job) in jobs.iter().take(5).enumerate() {
        let command = crate::gateway::dingtalk_commands::to_normalized_for_match(&job.inbound.text);
        let label = match crate::gateway::dingtalk_commands::parse_dingtalk_command(&command) {
            Some(crate::gateway::dingtalk_commands::DingtalkCommand::Status) => "Gateway 状态",
            Some(crate::gateway::dingtalk_commands::DingtalkCommand::Monitor) => "桌面监控",
            Some(crate::gateway::dingtalk_commands::DingtalkCommand::Help)
            | Some(crate::gateway::dingtalk_commands::DingtalkCommand::Menu) => "帮助菜单",
            Some(crate::gateway::dingtalk_commands::DingtalkCommand::Ping) => "连接检查",
            None => "DingTalk 任务",
        };
        let status = match job.status {
            crate::gateway::dingtalk_store::JobStatus::Received => "等待中",
            crate::gateway::dingtalk_store::JobStatus::Processing => "执行中",
            crate::gateway::dingtalk_store::JobStatus::Completed => "成功",
            crate::gateway::dingtalk_store::JobStatus::Failed => "失败",
        };
        let timestamp = format!("Unix {}", job.created_at);
        lines.push(format!("{}. {}", index + 1, label));
        lines.push(format!("   状态：{status}"));
        lines.push(format!("   时间：{timestamp}"));
        lines.push(String::new());
    }
    lines.join("\n").trim_end().to_string()
}

fn execute_help() -> PanelActionResult {
    let body = "OmniNova Agent 面板\n\nGateway 状态\n查看 Gateway 与 DingTalk 运行状态。\n\n最近任务\n查看最近执行的任务。\n\n监控 30 秒\n持续监控桌面变化 30 秒。\n\n监控 60 秒\n持续监控桌面变化 60 秒。\n\n帮助说明\n查看面板使用说明。\n\n高风险工具不在普通聊天中直接执行。";
    PanelActionResult::success("help", "帮助说明已发送", body.to_string())
}

async fn execute_monitor(
    runtime: &GatewayRuntime,
    context: &crate::gateway::dingtalk_store::DingtalkPanelContext,
    duration_secs: u64,
) -> PanelActionResult {
    let action = if duration_secs == 30 {
        "monitor_30s"
    } else {
        "monitor_60s"
    };
    println!("[dingtalk-panel] action={action} state=running");

    // Note: the primary admission check (DingtalkMonitorGuard) is done at the
    // top of process_panel_action before this function is called.
    // The DingtalkMonitorGuard lease (if acquired) is held until process_panel_action returns.
    // This function receives a mutable reference but does NOT release the lease early.

    let captures_dir = directories::ProjectDirs::from("com", "omninova", "OmniNova")
        .map(|dirs| dirs.config_dir().join("captures"))
        .unwrap_or_else(|| std::env::temp_dir().join("omninova-captures"));
    let result = crate::desktop_capture::monitor_desktop(&captures_dir, duration_secs).await;
    println!("[dingtalk-panel] action={action} state=completed");

    panel_monitor_result(action, duration_secs, &result)
}

fn panel_monitor_result(
    action: &'static str,
    duration_secs: u64,
    result: &crate::desktop_capture::MonitorResult,
) -> PanelActionResult {
    if result.ok {
        let changed = result.changed.unwrap_or(false);
        let detail = if changed {
            "检测到桌面变化"
        } else {
            "未检测到明显变化"
        };
        PanelActionResult::success(
            action,
            format!("桌面监控完成 · {duration_secs} 秒"),
            format!(
                "桌面监控完成\n\n监控时长：{duration_secs} 秒\n结果：{detail}"
            ),
        )
    } else {
        let reason = result
            .error_code
            .as_deref()
            .map(safe_error_kind)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "capture_failed".to_string());
        PanelActionResult::failed(
            action,
            "桌面监控失败",
            format!("桌面监控失败\n\n错误：{reason}"),
        )
    }
}

pub fn parse_card_callback_envelope(envelope: &Value) -> Result<ParsedCardCallback, String> {
    let data = envelope
        .get("data")
        .ok_or_else(|| "missing_callback_data".to_string())?;
    let request: Value = match data {
        Value::String(data) => {
            serde_json::from_str(data).map_err(|_| "invalid_callback_data_json".to_string())?
        }
        Value::Object(_) => data.clone(),
        _ => return Err("invalid_callback_data_type".to_string()),
    };
    let out_track_id = request
        .get("outTrackId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing_out_track_id".to_string())?
        .to_string();
    let user_id = request
        .get("userId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let space_id = request
        .get("spaceId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let content = request
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing_content".to_string())?;
    let content: Value =
        serde_json::from_str(content).map_err(|_| "invalid_content_json".to_string())?;
    let params = content
        .get("cardPrivateData")
        .and_then(|value| value.get("params"))
        .ok_or_else(|| "missing_card_private_params".to_string())?;
    let action = params
        .get("action")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing_action".to_string())?
        .to_string();
    let callback_id = ["callbackId", "messageId", "eventId"]
        .iter()
        .find_map(|key| request.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    Ok(ParsedCardCallback {
        out_track_id,
        action,
        callback_id,
        user_id,
        space_id,
    })
}

pub fn callback_dedupe_key(
    callback_id: Option<&str>,
    out_track_id: &str,
    action: &str,
    callback_fingerprint: &str,
) -> String {
    match callback_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(callback_id) => format!("callback:{callback_id}:{action}"),
        None => format!(
            "fallback:{}:{}:{}",
            opaque_short_hash(out_track_id),
            action,
            opaque_short_hash(callback_fingerprint)
        ),
    }
}

pub fn build_stream_ack(envelope: &Value, data: Value) -> Value {
    let message_id = envelope
        .get("headers")
        .and_then(|headers| headers.get("messageId"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    serde_json::json!({
        "code": 200,
        "headers": {
            "contentType": "application/json",
            "messageId": message_id
        },
        "message": "OK",
        "data": data.to_string()
    })
}

pub fn is_allowed_action(action: &str) -> bool {
    canonical_agent_menu_action(action).is_some()
}

fn reconnect_delay(attempt: u32) -> Duration {
    Duration::from_secs(2u64.saturating_pow(attempt.min(4)).min(30))
}

fn safe_action(action: &str) -> String {
    action
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
        .take(48)
        .collect()
}

fn safe_json_scalar(value: &Value) -> String {
    safe_action(match value {
        Value::String(value) => value,
        _ => "unknown",
    })
}

fn safe_error_kind(error: &str) -> String {
    error
        .split([':', '='])
        .next()
        .unwrap_or("unknown")
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
        .take(64)
        .collect()
}

/// Public wrapper for opaque_short_hash to be used in tests
pub fn public_opaque_short_hash(value: &str) -> String {
    opaque_short_hash(value)
}

fn opaque_short_hash(value: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(value.as_bytes());
    hex::encode(&digest[..6])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_stream_worker_starts_only_for_stream_transport() {
        assert!(!should_start_card_stream(true, DingtalkTransportMode::Http));
        assert!(should_start_card_stream(
            true,
            DingtalkTransportMode::Stream
        ));
        assert!(!should_start_card_stream(
            false,
            DingtalkTransportMode::Stream
        ));
    }

    fn callback_envelope(content: Value) -> Value {
        serde_json::json!({
            "type": "CALLBACK",
            "headers": {
                "messageId": "message-secret",
                "topic": DINGTALK_CARD_CALLBACK_TOPIC
            },
            "data": serde_json::json!({
                "outTrackId": "track-secret",
                "userId": "user-secret",
                "spaceId": "space-secret",
                "content": content.to_string()
            }).to_string()
        })
    }

    #[test]
    fn parses_gateway_status_from_string_content() {
        let parsed = parse_card_callback_envelope(&callback_envelope(serde_json::json!({
            "cardPrivateData": { "params": { "action": "gateway_status" } }
        })))
        .unwrap();
        assert_eq!(parsed.action, "gateway_status");
        assert_eq!(parsed.out_track_id, "track-secret");
        assert_eq!(parsed.user_id.as_deref(), Some("user-secret"));
        assert_eq!(parsed.space_id.as_deref(), Some("space-secret"));
    }

    #[test]
    fn missing_or_invalid_callback_fields_are_rejected() {
        let cases = [
            serde_json::json!({}),
            serde_json::json!({ "data": 1 }),
            serde_json::json!({ "data": "not-json" }),
            serde_json::json!({ "data": { "outTrackId": "track", "content": "{}" } }),
        ];
        for value in cases {
            assert!(parse_card_callback_envelope(&value).is_err());
        }
    }

    #[test]
    fn malformed_content_and_params_never_panic() {
        let invalid_content = serde_json::json!({
            "data": serde_json::json!({
                "outTrackId": "track",
                "content": "not-json"
            }).to_string()
        });
        assert_eq!(
            parse_card_callback_envelope(&invalid_content).unwrap_err(),
            "invalid_content_json"
        );

        let params_not_object = callback_envelope(serde_json::json!({
            "cardPrivateData": { "params": "not-an-object" }
        }));
        assert_eq!(
            parse_card_callback_envelope(&params_not_object).unwrap_err(),
            "missing_action"
        );
    }

    #[test]
    fn allowlist_accepts_all_canonical_actions() {
        // All canonical actions should be allowed
        for action in [
            "gateway_status",
            "monitor_30s",
            "monitor_60s",
            "recent_jobs",
            "help",
        ] {
            assert!(is_allowed_action(action), "action {} should be allowed", action);
        }
    }

    #[test]
    fn allowlist_rejects_unknown_actions() {
        for action in [
            "file_delete",
            "exec",
            "rm_rf",
            "evil",
            "",
        ] {
            assert!(!is_allowed_action(action), "action {} should be rejected", action);
        }
    }

    #[test]
    fn ack_correlates_message_without_exposing_it_as_business_data() {
        let envelope = callback_envelope(serde_json::json!({}));
        let ack = build_stream_ack(&envelope, serde_json::json!({ "response": {} }));
        assert_eq!(ack["code"], 200);
        assert_eq!(ack["headers"]["messageId"], "message-secret");
        assert!(ack["data"].as_str().unwrap().contains("response"));
    }

    #[test]
    fn reconnect_backoff_is_bounded() {
        assert_eq!(reconnect_delay(0), Duration::from_secs(1));
        assert_eq!(reconnect_delay(1), Duration::from_secs(2));
        assert_eq!(reconnect_delay(99), Duration::from_secs(16));
    }

    #[test]
    fn dedupe_cache_allows_first_insert_sync() {
        let cache = CallbackDedupeCache::new(100);
        let key = "test-track:gateway_status";
        assert!(cache.try_insert(key));
    }

    #[test]
    fn dedupe_cache_rejects_duplicate_sync() {
        let cache = CallbackDedupeCache::new(100);
        let key = "test-track:gateway_status";
        assert!(cache.try_insert(key));
        assert!(!cache.try_insert(key));
    }

    #[test]
    fn dedupe_cache_different_keys_allowed_sync() {
        let cache = CallbackDedupeCache::new(100);
        assert!(cache.try_insert("track1:gateway_status"));
        assert!(cache.try_insert("track2:gateway_status"));
        assert!(cache.try_insert("track1:recent_jobs"));
        assert!(cache.try_insert("track3:monitor_30s"));
    }

    #[test]
    fn callback_id_dedupes_retry_but_allows_a_later_click() {
        let first = callback_dedupe_key(
            Some("callback-1"),
            "same-track",
            "gateway_status",
            "ignored",
        );
        let retry = callback_dedupe_key(
            Some("callback-1"),
            "same-track",
            "gateway_status",
            "ignored-again",
        );
        let later_click = callback_dedupe_key(
            Some("callback-2"),
            "same-track",
            "gateway_status",
            "ignored",
        );
        assert_eq!(first, retry);
        assert_ne!(first, later_click);
    }

    #[test]
    fn opaque_short_hash_produces_short_deterministic_output() {
        let hash1 = opaque_short_hash("secret-outtrack:gateway_status");
        let hash2 = opaque_short_hash("secret-outtrack:gateway_status");
        let hash3 = opaque_short_hash("different:action");
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 12);
        assert!(!hash1.contains("secret"));
    }

    #[test]
    fn all_canonical_actions_are_allowed() {
        let allowed = ["gateway_status", "monitor_30s", "monitor_60s", "recent_jobs", "help"];
        for action in allowed {
            assert!(is_allowed_action(action), "action {} should be allowed", action);
        }
    }

    #[test]
    fn gateway_status_result_separates_card_summary_from_message() {
        let result = PanelActionResult::success(
            "gateway_status",
            "Gateway 状态读取完成",
            "Gateway 状态\n\nGateway：运行正常".to_string(),
        );
        assert_eq!(result.card_summary, "Gateway 状态读取完成");
        assert!(result.message_body.as_deref().unwrap().contains("Gateway：运行正常"));
        assert!(!result.card_summary.contains("运行正常"));
    }

    #[test]
    fn recent_jobs_empty_message_is_explicit() {
        assert_eq!(
            format_recent_jobs_message(&[]),
            "最近任务\n\n当前暂无最近任务。"
        );
    }

    #[test]
    fn recent_jobs_message_is_limited_and_redacted() {
        let jobs = (0..6)
            .map(|index| crate::gateway::dingtalk_store::DingtalkJob {
                job_id: format!("secret-job-{index}"),
                inbound: crate::channels::InboundMessage {
                    channel: crate::channels::ChannelKind::Dingtalk,
                    user_id: Some("secret-user".to_string()),
                    session_id: Some("secret-conversation".to_string()),
                    text: "status".to_string(),
                    metadata: Default::default(),
                },
                status: crate::gateway::dingtalk_store::JobStatus::Completed,
                created_at: index,
                updated_at: index,
                error_message: None,
            })
            .collect::<Vec<_>>();
        let message = format_recent_jobs_message(&jobs);
        assert_eq!(message.matches("Gateway 状态").count(), 5);
        for secret in ["secret-job", "secret-user", "secret-conversation"] {
            assert!(!message.contains(secret));
        }
    }

    #[test]
    fn help_result_is_detailed_but_card_summary_is_short() {
        let result = execute_help();
        assert_eq!(result.card_summary, "帮助说明已发送");
        let body = result.message_body.unwrap();
        assert!(body.contains("监控 30 秒"));
        assert!(body.contains("监控 60 秒"));
        assert!(body.contains("Gateway 状态"));
        assert!(body.contains("最近任务"));
    }

    #[test]
    fn monitor_result_keeps_details_out_of_card() {
        let monitor = crate::desktop_capture::MonitorResult::success_no_detection(
            30,
            30_000,
            crate::desktop_capture::CaptureResult::success(
                "start.png".to_string(),
                1,
                1,
                1,
                "start".to_string(),
            ),
            crate::desktop_capture::CaptureResult::success(
                "end.png".to_string(),
                1,
                1,
                1,
                "end".to_string(),
            ),
        );
        let result = panel_monitor_result("monitor_30s", 30, &monitor);
        assert_eq!(result.card_summary, "桌面监控完成 · 30 秒");
        assert!(result.message_body.as_deref().unwrap().contains("监控时长：30 秒"));
        assert!(!result.card_summary.contains("监控时长"));
    }

    #[test]
    fn busy_monitor_never_has_a_detailed_message() {
        let result = PanelActionResult::busy("monitor_60s");
        assert!(result.busy);
        assert!(result.message_body.is_none());
    }

    #[test]
    fn completed_business_with_send_failure_is_not_marked_business_failed() {
        let result = PanelActionResult::success(
            "help",
            "帮助说明已发送",
            "detail".to_string(),
        );
        assert!(result.success);
        assert_eq!(
            card_summary_after_delivery(&result, false),
            "操作已完成，但结果消息发送失败"
        );
    }
}
