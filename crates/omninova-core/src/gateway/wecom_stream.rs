//! WeCom (企业微信) WebSocket long-connection transport.
//!
//! Phase 1B.2: Fixed physical writer with socket sink.

use crate::gateway::wecom_inbound::normalize_wecom_callback;
use crate::gateway::{DedupCache, GatewayRuntime};
use futures_util::{Sink, SinkExt, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;

use super::wecom_protocol::{
    build_ping_envelope, build_stream_respond_envelope, build_subscribe_envelope,
    parse_response_envelope, WecomCallbackEnvelope, WecomCommandType, WecomEventType,
    WecomResponseEnvelope,
};

pub const WECOM_WS_URL: &str = "wss://openws.work.weixin.qq.com";
const HEARTBEAT_INTERVAL_SECS: u64 = 30;
const MAX_BACKOFF_ATTEMPTS: u32 = 6;
const BACKOFF_CAP_SECS: u64 = 30;
const OUTBOUND_CHANNEL_SIZE: usize = 16;

// Heartbeat sampling counters - log every N events to reduce noise
static HEARTBEAT_DISPATCH_SAMPLE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static HEARTBEAT_WRITE_SAMPLE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static HEARTBEAT_ACK_SAMPLE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
const HEARTBEAT_LOG_EVERY_N: u64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WecomStreamState {
    Disconnected,
    Connecting,
    Connected,
    ConnectedSubscribed,
    Stopping,
}

pub struct WecomStreamGuard {
    owner_gen: u64,
    runtime: Arc<GatewayRuntime>,
    shutdown_tx: Option<watch::Sender<bool>>,
    shutdown_complete: bool,
}

impl WecomStreamGuard {
    pub async fn shutdown(&mut self) {
        if self.shutdown_complete {
            return;
        }
        println!("[wecom-stream] shutdown_requested gen={}", self.owner_gen);
        if let Some(tx) = &self.shutdown_tx {
            let _ = tx.send(true);
        }
        let outcome = self
            .runtime
            .shutdown_wecom_stream_generation(self.owner_gen, Duration::from_secs(5))
            .await;
        println!("[wecom-stream] lifecycle_join outcome={outcome:?} gen={}", self.owner_gen);
        self.shutdown_complete = true;
    }
}

impl Drop for WecomStreamGuard {
    fn drop(&mut self) {
        if !self.shutdown_complete {
            self.runtime.signal_wecom_stream_shutdown(self.owner_gen);
        }
    }
}

#[derive(Debug, Clone)]
pub enum WecomOutboundMsg {
    Reply { req_id: String, text: String },
    Ping { req_id: String },
}

pub async fn start(runtime: Arc<GatewayRuntime>) -> Option<WecomStreamGuard> {
    let config = runtime.get_config().await;

    // Check if WeCom is enabled via channels_config.wecom OR gateway.wecom.enabled
    let is_enabled = config.gateway.wecom.enabled
        || config.channels_config.wecom.as_ref().map(|e| e.enabled).unwrap_or(false);

    if !is_enabled {
        println!("[wecom-stream] start_skipped reason=channel_disabled");
        return None;
    }

    // Get bot_id: try channels_config.extra["bot_id"] first, then gateway.wecom.bot_id, then bot_id_env
    let mut bot_id = config
        .channels_config
        .wecom
        .as_ref()
        .and_then(|e| e.extra.get("bot_id"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(String::from)
        .unwrap_or_default();

    if bot_id.is_empty() {
        bot_id = config.gateway.wecom.bot_id.clone();
        if bot_id.is_empty() {
            if let Some(ref env_name) = config.gateway.wecom.bot_id_env {
                bot_id = std::env::var(env_name).unwrap_or_default();
            }
        }
    }

    // Get secret: try channels_config.extra["secret"] first, then gateway.wecom.secret, then secret_env
    let mut secret = config
        .channels_config
        .wecom
        .as_ref()
        .and_then(|e| e.extra.get("secret"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(String::from)
        .unwrap_or_default();

    if secret.is_empty() {
        secret = config.gateway.wecom.secret.clone();
        if secret.is_empty() {
            if let Some(ref env_name) = config.gateway.wecom.secret_env {
                secret = std::env::var(env_name).unwrap_or_default();
            }
        }
    }

    if bot_id.is_empty() || secret.is_empty() {
        println!("[wecom-stream] start_skipped reason=missing_credentials");
        return None;
    }

    println!("[wecom-stream] start_initiated");

    // True single-owner acquisition: if another WeCom lifecycle is already
    // active on this runtime (e.g. a previous Gateway Stop has not finished
    // tearing down), refuse to start a second instance. A second lifecycle
    // would open a second WebSocket and cause the server to supersede the
    // first connection.
    let Some(gen) = runtime.try_acquire_wecom_stream_owner().await else {
        println!(
            "[wecom-stream] start_skipped reason=already_started gen={}",
            runtime.current_wecom_stream_generation()
        );
        return None;
    };
    println!("[wecom-stream] owner_acquired gen={}", gen);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Shared outbound channel - STABLE across reconnects
    let (shared_tx, shared_rx) = mpsc::channel::<WecomOutboundMsg>(OUTBOUND_CHANNEL_SIZE);
    let logical_id = short_hash(&format!("queue_{}", gen));
    println!("[wecom-stream] logical_queue_created gen={} logical_id={}", gen, logical_id);

    // Worker channel
    let (worker_tx, worker_rx) = mpsc::channel::<crate::gateway::wecom_worker::WecomAsyncJob>(100);

    // Spawn worker with shared outbound sender
    let worker_runtime = runtime.clone();
    let worker_logical_id = logical_id.clone();
    let worker_shared_tx = shared_tx.clone();
    tokio::spawn(async move {
        crate::gateway::wecom_worker::run_wecom_worker(
            worker_rx,
            worker_runtime,
            worker_shared_tx,
            worker_logical_id,
        ).await;
    });

    // Spawn heartbeat loop with shared outbound sender
    let heartbeat_runtime = runtime.clone();
    let hb_logical_id = logical_id.clone();
    let hb_shared_tx = shared_tx.clone();
    let hb_shutdown_rx = shutdown_rx.clone();
    tokio::spawn(async move {
        run_heartbeat_loop(heartbeat_runtime, hb_shutdown_rx, hb_shared_tx, hb_logical_id).await;
    });

    // Start reconnect loop with shared receiver. Register its JoinHandle so
    // Gateway Stop can truly join the loop before a new Start.
    let reconnect_handle = tokio::spawn(run_reconnect_loop(
        runtime.clone(),
        bot_id,
        secret,
        gen,
        logical_id,
        shutdown_rx,
        worker_tx,
        shared_rx,
    ));
    runtime
        .register_wecom_stream_loop_handle(gen, reconnect_handle)
        .await;

    println!("[wecom-stream] guard_created gen={}", gen);

    Some(WecomStreamGuard {
        owner_gen: gen,
        runtime,
        shutdown_tx: Some(shutdown_tx),
        shutdown_complete: false,
    })
}

fn reconnect_delay(attempt: u32) -> Duration {
    let base = 2u64.saturating_pow(attempt.min(MAX_BACKOFF_ATTEMPTS));
    Duration::from_secs(base.min(BACKOFF_CAP_SECS))
}

pub(crate) fn classify_error(err: &str) -> &'static str {
    if err.contains("superseded") {
        "superseded"
    } else if err.contains("auth") || err.contains("40001") || err.contains("40013") {
        "auth_failure"
    } else if err.contains("rate") || err.contains("40014") {
        "rate_limit"
    } else if err.contains("timeout") {
        "timeout"
    } else if err.contains("connection") || err.contains("network") {
        "network"
    } else if err.contains("closed") {
        "closed"
    } else {
        "protocol"
    }
}

/// Connection exit reason semantics.
///
/// `Superseded` means the WeCom server replaced this physical connection with
/// a newer one (aibot `disconnected_event`). It is NOT a network error, NOT a
/// protocol parsing error, and NOT a heartbeat timeout — it must never trigger
/// an automatic reconnect from the same reconnect loop, otherwise the
/// replacement connection gets kicked again in an infinite ping-pong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WecomConnectionExit {
    /// Transient/network-like failure: reconnect policy applies.
    Retryable,
    /// Server replaced this connection: stop, do NOT auto-reconnect.
    Superseded,
}

impl WecomConnectionExit {
    pub(crate) fn from_error(error: &str) -> Self {
        if classify_error(error) == "superseded" {
            Self::Superseded
        } else {
            Self::Retryable
        }
    }
}

// ---------------------------------------------------------------------------
// Reconnect loop
// ---------------------------------------------------------------------------

async fn run_reconnect_loop(
    runtime: Arc<GatewayRuntime>,
    bot_id: String,
    secret: String,
    gen: u64,
    logical_id: String,
    mut shutdown: watch::Receiver<bool>,
    worker_tx: mpsc::Sender<crate::gateway::wecom_worker::WecomAsyncJob>,
    mut shared_rx: mpsc::Receiver<WecomOutboundMsg>,
) {
    let mut attempt: u32 = 0;

    loop {
        if *shutdown.borrow() {
            println!("[wecom-stream] shutdown_signal gen={}", gen);
            break;
        }

        if !runtime.is_wecom_stream_generation_active(gen) {
            break;
        }

        let physical_id = short_hash(&format!("physical_{}_{}", gen, attempt));
        println!("[wecom-stream] state=connecting attempt={} gen={} physical_id={}", attempt, gen, physical_id);

        match connect_and_run(&runtime, &bot_id, &secret, gen, attempt, &logical_id, shutdown.clone(), worker_tx.clone(), &mut shared_rx).await {
            Ok(()) => attempt = 0,
            Err(e) => {
                let kind = classify_error(&e);
                match WecomConnectionExit::from_error(&e) {
                    WecomConnectionExit::Superseded => {
                        // Server replaced this connection. Do NOT attempt a
                        // reconnect from this loop: a fresh connection would
                        // itself be superseded again (infinite ping-pong).
                        // Exit and release ownership; a later Gateway
                        // Stop → Start can establish a new connection.
                        println!("[wecom-stream] disconnected reason={} gen={} superseded_no_reconnect=true", kind, gen);
                        break;
                    }
                    WecomConnectionExit::Retryable => {
                        attempt = attempt.saturating_add(1);
                        println!("[wecom-stream] disconnected reason={} attempt={} gen={}", kind, attempt, gen);
                    }
                }
            }
        }

        runtime.set_wecom_stream_connected(false);
        runtime.set_wecom_stream_subscribed(false);

        if !runtime.is_wecom_stream_generation_active(gen) {
            break;
        }

        let delay = reconnect_delay(attempt);
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            _ = tokio::time::sleep(delay) => {}
        }
    }

    runtime.set_wecom_stream_connected(false);
    runtime.set_wecom_stream_subscribed(false);
    runtime.release_wecom_stream_generation(gen);
    println!("[wecom-stream] owner_released gen={}", gen);
    println!("[wecom-stream] loop_exited gen={}", gen);
}

// ---------------------------------------------------------------------------
// Single connection lifecycle - PHYSICAL WRITER IS HERE
// ---------------------------------------------------------------------------

async fn connect_and_run(
    runtime: &Arc<GatewayRuntime>,
    bot_id: &str,
    secret: &str,
    gen: u64,
    attempt: u32,
    logical_id: &str,
    mut shutdown: watch::Receiver<bool>,
    worker_tx: mpsc::Sender<crate::gateway::wecom_worker::WecomAsyncJob>,
    shared_rx: &mut mpsc::Receiver<WecomOutboundMsg>,
) -> Result<(), String> {
    if !runtime.is_wecom_stream_generation_active(gen) {
        return Err("stale_generation".to_string());
    }

    let physical_id = short_hash(&format!("physical_{}_{}", gen, attempt));
    println!("[wecom-stream] websocket_connecting physical_id={}", physical_id);

    let (ws_stream, _) = tokio_tungstenite::connect_async(WECOM_WS_URL)
        .await
        .map_err(|e| format!("connect_error: {}", e))?;
    println!("[wecom-stream] websocket_open=true physical_id={}", physical_id);
    runtime.set_wecom_stream_connected(true);

    let (mut write, mut read) = ws_stream.split();

    // Subscribe
    let req_id = format!("sub_{}_{}", gen, std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
    let subscribe = build_subscribe_envelope(&req_id, bot_id, secret);
    let subscribe_str = serde_json::to_string(&subscribe)
        .map_err(|e| format!("serialize_error: {}", e))?;

    println!("[wecom-stream] subscribe_sent=true physical_id={}", physical_id);
    write.send(Message::Text(subscribe_str.into())).await
        .map_err(|e| format!("subscribe_send_error: {}", e))?;

    // Wait for subscribe response
    let resp = tokio::time::timeout(Duration::from_secs(10), read.next()).await
        .map_err(|_| "subscribe_timeout")?
        .ok_or("subscribe_no_response")?
        .map_err(|e| format!("subscribe_recv_error: {}", e))?;

    let resp_str = resp.into_text()
        .map_err(|e| format!("subscribe_text_error: {}", e))?;

    let resp_env = parse_response_envelope(&resp_str)
        .map_err(|e| format!("subscribe_response_error: {}", e))?;

    if resp_env.errcode != 0 {
        println!("[wecom-stream] subscribe_rejected errcode={} physical_id={}", resp_env.errcode, physical_id);
        return Err(format!("subscribe_failed errcode={}", resp_env.errcode));
    }

    println!("[wecom-stream] subscribe_ack=true physical_id={}", physical_id);
    runtime.set_wecom_stream_subscribed(true);

    // PHYSICAL WRITER: runs in the same task as read loop, with proper borrow
    let physical_id_clone = physical_id.clone();
    let physical_writer_runtime = runtime.clone();
    let physical_writer_shutdown = shutdown.clone();

    // Run physical writer and read loop concurrently
    tokio::select! {
        _ = run_physical_writer_inline(&mut write, shared_rx, &physical_writer_runtime, gen, attempt, physical_id.clone(), physical_writer_shutdown) => {
            // Physical writer exited, connection is broken
            println!("[wecom-stream] physical_writer_exit physical_id={}", physical_id_clone);
        }
        result = run_read_loop_inline(&mut read, runtime, gen, logical_id, shutdown, worker_tx, physical_id_clone.clone()) => {
            // Read loop exited
            println!("[wecom-stream] read_loop_exit physical_id={}", physical_id_clone);
            // Return error to trigger reconnect
            if let Err(e) = result {
                return Err(e);
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Physical Writer (inline, not spawned) - owns the WebSocket sink
// ---------------------------------------------------------------------------

async fn run_physical_writer_inline<W>(
    write: &mut W,
    rx: &mut mpsc::Receiver<WecomOutboundMsg>,
    runtime: &Arc<GatewayRuntime>,
    gen: u64,
    attempt: u32,
    physical_id: String,
    mut shutdown: watch::Receiver<bool>,
) where
    W: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    println!("[wecom-stream] physical_writer_started gen={} attempt={} physical_id={}", gen, attempt, physical_id);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    println!("[wecom-stream] physical_writer_exit shutdown physical_id={}", physical_id);
                    return;
                }
            }
            msg = rx.recv() => {
                match msg {
                    Some(WecomOutboundMsg::Ping { req_id }) => {
                        if !runtime.is_wecom_stream_generation_active(gen) {
                            println!("[wecom-stream] physical_writer_exit stale physical_id={}", physical_id);
                            return;
                        }
                        let ping = build_ping_envelope(&req_id);
                        let ping_str = serde_json::to_string(&ping).unwrap_or_default();
                        // Sample heartbeat write - only log every N to reduce noise
                        let write_sample = HEARTBEAT_WRITE_SAMPLE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if write_sample % HEARTBEAT_LOG_EVERY_N == 0 {
                            println!("[wecom-stream] heartbeat_write_started req_id={} physical_id={}", short_hash(&req_id), physical_id);
                        }
                        match write.send(Message::Text(ping_str.into())).await {
                            Ok(()) => {
                                let ack_sample = HEARTBEAT_ACK_SAMPLE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                if ack_sample % HEARTBEAT_LOG_EVERY_N == 0 {
                                    println!("[wecom-stream] heartbeat_write_ok req_id={} physical_id={}", short_hash(&req_id), physical_id);
                                }
                            }
                            Err(e) => {
                                println!("[wecom-stream] heartbeat_write_failed req_id={} physical_id={} error={}", short_hash(&req_id), physical_id, e);
                                return;
                            }
                        }
                    }
                    Some(WecomOutboundMsg::Reply { req_id, text }) => {
                        if !runtime.is_wecom_stream_generation_active(gen) {
                            println!("[wecom-stream] physical_writer_exit stale physical_id={}", physical_id);
                            return;
                        }
                        let env = build_stream_respond_envelope(&req_id, &text);
                        let json = serde_json::to_string(&env).unwrap_or_default();
                        println!("[wecom-stream] reply_write_started req_id={} physical_id={}", short_hash(&req_id), physical_id);
                        match write.send(Message::Text(json.into())).await {
                            Ok(()) => {
                                println!("[wecom-stream] reply_write_ok req_id={} physical_id={}", short_hash(&req_id), physical_id);
                                runtime.increment_wecom_reply_success().await;
                            }
                            Err(e) => {
                                println!("[wecom-stream] reply_write_failed req_id={} physical_id={} error={}", short_hash(&req_id), physical_id, e);
                                return;
                            }
                        }
                    }
                    None => {
                        println!("[wecom-stream] physical_writer_exit channel_closed physical_id={}", physical_id);
                        return;
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Heartbeat loop - sends to shared queue
// ---------------------------------------------------------------------------

async fn run_heartbeat_loop(
    runtime: Arc<GatewayRuntime>,
    mut shutdown: watch::Receiver<bool>,
    shared_tx: mpsc::Sender<WecomOutboundMsg>,
    logical_id: String,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECS));
    println!("[wecom-stream] heartbeat_loop_started logical_id={}", logical_id);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    println!("[wecom-stream] heartbeat_loop_exit shutdown logical_id={}", logical_id);
                    return;
                }
            }
            _ = interval.tick() => {
                let req_id = format!("ping_{}", std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());

                // Sample heartbeat logging - only log every N heartbeats to reduce noise
                let sample_count = HEARTBEAT_DISPATCH_SAMPLE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if sample_count % HEARTBEAT_LOG_EVERY_N == 0 {
                    println!("[wecom-stream] heartbeat_dispatch_requested req_id={} logical_id={}", short_hash(&req_id), logical_id);
                }
                if shared_tx.send(WecomOutboundMsg::Ping { req_id }).await.is_err() {
                    println!("[wecom-stream] heartbeat_loop_exit send_failed logical_id={}", logical_id);
                    return;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Read loop (inline)
// ---------------------------------------------------------------------------

async fn run_read_loop_inline<R>(
    read: &mut R,
    runtime: &Arc<GatewayRuntime>,
    gen: u64,
    logical_id: &str,
    mut shutdown: watch::Receiver<bool>,
    worker_tx: mpsc::Sender<crate::gateway::wecom_worker::WecomAsyncJob>,
    physical_id: String,
) -> Result<(), String>
where
    R: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let dedup = DedupCache::global();

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    return Ok(());
                }
            }
            msg = read.next() => {
                match msg {
                    Some(Ok(msg)) => {
                        // Stale-physical protection: if our generation is no
                        // longer the active owner (replaced by a restart or a
                        // superseding connection), this read loop is a ghost.
                        // Ignore everything and exit — never attribute frames
                        // to the current physical connection.
                        if !runtime.is_wecom_stream_generation_active(gen) {
                            println!("[wecom-stream] read_loop_exit stale_generation gen={} physical_id={}", gen, physical_id);
                            return Err("stale_generation".to_string());
                        }
                        if let Err(e) = handle_incoming_message(&msg, runtime, &dedup, &worker_tx, logical_id, &physical_id, gen).await {
                            let kind = classify_error(&e);
                            println!("[wecom-stream] handle_error reason={} gen={}", kind, gen);
                            // Superseded and auth/rate failures terminate this
                            // physical connection; transient protocol noise is
                            // tolerated so a single malformed frame does not
                            // tear down an otherwise healthy connection.
                            if kind == "auth_failure" || kind == "rate_limit" || kind == "superseded" {
                                return Err(e);
                            }
                        }
                    }
                    Some(Err(e)) => return Err(format!("read_error: {}", e)),
                    None => return Err("websocket_closed".to_string()),
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Message handler
// ---------------------------------------------------------------------------

async fn handle_incoming_message(
    msg: &Message,
    runtime: &Arc<GatewayRuntime>,
    dedup: &Arc<DedupCache>,
    worker_tx: &mpsc::Sender<crate::gateway::wecom_worker::WecomAsyncJob>,
    logical_id: &str,
    physical_id: &str,
    gen: u64,
) -> Result<(), String> {
    let text = msg.to_string();
    println!("[wecom-stream] frame_received len={}", text.len());

    // Parse ACK response
    if let Ok(ack) = serde_json::from_str::<WecomResponseEnvelope>(&text) {
        let req_id_short = short_hash(&ack.headers.req_id);
        let errmsg_short = if ack.errmsg.len() > 20 {
            format!("{}...", &ack.errmsg[..20])
        } else {
            ack.errmsg.clone()
        };

        let is_heartbeat = ack.headers.req_id.starts_with("ping_");

        if ack.errcode == 0 {
            if is_heartbeat {
                // Heartbeat ACK: no errmsg field (server returns empty for ping)
                println!("[wecom-stream] heartbeat_ack_ok req_id={} physical_id={}", req_id_short, physical_id);
            } else {
                println!("[wecom-stream] reply_ack_ok req_id={} errmsg={} physical_id={}", req_id_short, errmsg_short, physical_id);
            }
        } else {
            if is_heartbeat {
                println!("[wecom-stream] heartbeat_ack_error req_id={} errcode={} physical_id={}", req_id_short, ack.errcode, physical_id);
            } else {
                println!("[wecom-stream] reply_ack_error req_id={} errcode={} errmsg={} physical_id={}", req_id_short, ack.errcode, errmsg_short, physical_id);
            }
            if !is_heartbeat && (ack.errcode == 40001 || ack.errcode == 40013 || ack.errcode == 40014) {
                return Err(format!("ack_error errcode={}", ack.errcode));
            }
        }
        return Ok(());
    }

    // Parse callback envelope
    let envelope: WecomCallbackEnvelope = match serde_json::from_str(&text) {
        Ok(e) => e,
        Err(_) => {
            println!("[wecom-stream] non_callback_frame len={}", text.len());
            return Ok(());
        }
    };

    let cmd = &envelope.cmd;
    let req_id = &envelope.headers.req_id;
    let body = &envelope.body;

    println!("[wecom-stream] cmd={} req_id={}", cmd, short_hash(req_id));

    match WecomCommandType::from_cmd(cmd) {
        WecomCommandType::MessageCallback => {
            handle_message_callback(runtime, dedup, body, req_id, worker_tx, logical_id, physical_id, gen).await
        }
        WecomCommandType::EventCallback => {
            handle_event_callback(body, req_id, physical_id).await
        }
        WecomCommandType::Unknown => {
            println!("[wecom-stream] unknown_cmd={}", cmd);
            Ok(())
        }
    }
}

async fn handle_message_callback(
    runtime: &Arc<GatewayRuntime>,
    dedup: &Arc<DedupCache>,
    body: &crate::gateway::wecom_protocol::WecomCallbackBody,
    req_id: &str,
    worker_tx: &mpsc::Sender<crate::gateway::wecom_worker::WecomAsyncJob>,
    logical_id: &str,
    physical_id: &str,
    gen: u64,
) -> Result<(), String> {
    let msg_id = &body.msgid;

    if !dedup.check_and_insert(msg_id).await {
        println!("[wecom-stream] duplicate_msgid={} skipped=true", short_hash(msg_id));
        return Ok(());
    }

    let inbound = normalize_wecom_callback(body, req_id)
        .map_err(|e| format!("normalize_error: {}", e))?;

    let chat_type = body.chattype.as_deref().unwrap_or("unknown");
    println!(
        "[wecom-stream] inbound_parse_ok=true msg_id={} chat_type={} text_len={}",
        short_hash(msg_id),
        chat_type,
        inbound.text.len()
    );

    if inbound.text.is_empty() {
        return Ok(());
    }

    let job = crate::gateway::wecom_worker::WecomAsyncJob::new_with_writer(
        inbound,
        req_id.to_string(),
        logical_id.to_string(),
        chat_type.to_string(),
        gen,
    );

    if let Err(e) = worker_tx.try_send(job) {
        println!("[wecom-stream] worker_enqueue_failed error={}", e);
    }

    Ok(())
}

pub(crate) async fn handle_event_callback(
    body: &crate::gateway::wecom_protocol::WecomCallbackBody,
    req_id: &str,
    physical_id: &str,
) -> Result<(), String> {
    let event_type = body
        .event
        .as_ref()
        .and_then(|e| e.eventtype.as_deref())
        .unwrap_or("unknown");

    println!("[wecom-stream] event_type={}", event_type);

    match WecomEventType::from_eventtype(Some(event_type)) {
        WecomEventType::EnterChat => {
            println!("[wecom-stream] event=enter_chat");
        }
        WecomEventType::Disconnected => {
            // Official aibot semantics: this physical connection was replaced
            // by a newer one. Stop this connection entirely and NEVER
            // auto-reconnect from the same loop.
            println!(
                "[wecom-stream] server_superseded physical_id={}",
                physical_id
            );
            return Err("server_superseded".to_string());
        }
        _ => {
            println!("[wecom-stream] event=unhandled type={}", event_type);
        }
    }

    Ok(())
}

pub fn short_hash(s: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:08x}", h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconnect_delay() {
        assert_eq!(reconnect_delay(0).as_secs(), 1);
        assert_eq!(reconnect_delay(1).as_secs(), 2);
        assert_eq!(reconnect_delay(5).as_secs(), 30);
    }

    #[test]
    fn test_classify_error() {
        assert_eq!(classify_error("auth_failed 40001"), "auth_failure");
        assert_eq!(classify_error("rate_limit 40014"), "rate_limit");
        assert_eq!(classify_error("timeout"), "timeout");
    }

    #[test]
    fn test_short_hash() {
        let h1 = short_hash("test");
        let h2 = short_hash("test");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_outbound_msg_debug() {
        let msg = WecomOutboundMsg::Ping { req_id: "test".to_string() };
        let debug_str = format!("{:?}", msg);
        assert!(debug_str.contains("Ping"));

        let msg2 = WecomOutboundMsg::Reply { req_id: "test2".to_string(), text: "hello".to_string() };
        let debug_str2 = format!("{:?}", msg2);
        assert!(debug_str2.contains("Reply"));
    }
}
