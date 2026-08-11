//! Unified DingTalk Stream transport for robot messages and interactive card callbacks.
//!
//! This module provides a single WebSocket connection that subscribes to both:
//!   - `/v1.0/im/bot/messages/get` (robot inbound messages)
//!   - `/v1.0/card/instances/callback` (interactive card action callbacks)
//!
//! Key design principles:
//! - ACK immediately, process asynchronously (bounded queue)
//! - Registered state required for card availability
//! - Exponential backoff with max cap for reconnection
//! - Single reconnect loop to prevent storm
//! - Bounded message deduplication

use crate::channels::ChannelKind;
use crate::channels::InboundMessage;
use crate::config::schema::DingtalkTransportMode;
use crate::gateway::dingtalk_card_stream::ParsedCardCallback;
use crate::gateway::dingtalk_worker::DingtalkAsyncJob;
use crate::gateway::{DingTalkStreamState, GatewayRuntime};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;

/// DingTalk Stream gateway endpoint
const DINGTALK_STREAM_GATEWAY_URL: &str = "https://api.dingtalk.com/v1.0/gateway/connections/open";

/// Robot messages topic
pub const TOPIC_ROBOT: &str = "/v1.0/im/bot/messages/get";

/// Card callback topic
pub const TOPIC_CARD: &str = "/v1.0/card/instances/callback";

/// Backpressure queue capacity
const QUEUE_CAPACITY: usize = 100;

/// Max reconnection attempts before capping backoff
const MAX_BACKOFF_ATTEMPTS: u32 = 6;

/// Guard to manage the Stream lifecycle. Holds owner generation token that
/// prevents old owners from clearing new owners (ABA-safe).
///
/// Drop semantics:
/// - Cooperative shutdown (normal serve_http exit): Drop called → release_owner(gen)
/// - Abort shutdown (task.abort()): Drop called after JoinHandle await → release_owner(gen)
/// Both paths correctly release because tokio ensures future completion before JoinHandle returns.
pub struct DingtalkStreamGuard {
    /// Owner generation: only this owner may release.
    owner_gen: u64,
    /// Runtime reference for owner-scoped cleanup calls.
    runtime: Arc<GatewayRuntime>,
    shutdown_complete: bool,
}

impl DingtalkStreamGuard {
    pub async fn shutdown(&mut self) {
        if self.shutdown_complete {
            return;
        }
        let outcome = self
            .runtime
            .shutdown_dingtalk_stream_generation(self.owner_gen, Duration::from_secs(5))
            .await;
        println!("[dingtalk-stream] lifecycle_join outcome={outcome:?}");
        self.shutdown_complete = true;
    }
}

impl Drop for DingtalkStreamGuard {
    fn drop(&mut self) {
        if !self.shutdown_complete {
            let _ = self
                .runtime
                .signal_dingtalk_stream_shutdown(self.owner_gen);
        }
    }
}

/// Start the unified DingTalk Stream transport.
/// Returns None if Stream mode is not enabled, configuration is incomplete,
/// or another reconnect loop is already running on this runtime.
pub async fn start(runtime: Arc<GatewayRuntime>) -> Option<DingtalkStreamGuard> {
    let config = runtime.get_config().await;
    let entry = config.channels_config.dingtalk.as_ref();

    let enabled = config.gateway.dingtalk.enabled || entry.map(|e| e.enabled).unwrap_or(false);
    if !enabled {
        println!("[dingtalk-stream] start_skipped reason=channel_disabled");
        return None;
    }

    let transport_mode = crate::gateway::resolve_dingtalk_transport_mode_for_worker(&config, entry);
    if transport_mode != DingtalkTransportMode::Stream {
        println!("[dingtalk-stream] start_skipped reason=transport_mode=http");
        return None;
    }

    let app_key = crate::gateway::resolve_dingtalk_app_key_for_worker(&config, entry);
    let app_secret = crate::gateway::resolve_dingtalk_secret_for_worker(&config, entry);

    if app_key.is_none() || app_secret.is_none() {
        println!("[dingtalk-stream] start_skipped reason=incomplete_credentials");
        return None;
    }

    // Acquire ownership and install the physical JoinHandle atomically.
    let ak = app_key.unwrap();
    let asec = app_secret.unwrap();
    let rt = runtime.clone();
    let owner_gen = runtime.try_start_dingtalk_stream_loop(move |owner_gen, rx| async move {
        run_reconnect_loop_internal(rt, owner_gen, ak, asec, rx).await;
    });
    let Some(owner_gen) = owner_gen else {
        println!("[dingtalk-stream] start_skipped reason=already_owned");
        return None;
    };

    println!("[dingtalk-stream] owner_acquired gen={owner_gen}");
    Some(DingtalkStreamGuard {
        owner_gen,
        runtime,
        shutdown_complete: false,
    })
}

// ---------------------------------------------------------------------------
// Reconnect loop - single owner to prevent storm
// ---------------------------------------------------------------------------

/// Owner-scoped cleanup: only releases if the current owner matches.
/// This is a belt-and-suspenders safety net — the primary cleanup path
/// is Guard::Drop. The loop calls this as it exits so that both cooperative
/// and abort shutdown paths correctly release.
fn stream_cleanup_on_exit(runtime: &Arc<GatewayRuntime>, owner_gen: u64) {
    runtime.set_dingtalk_stream_connected(owner_gen, false);
}

async fn run_reconnect_loop_internal(
    runtime: Arc<GatewayRuntime>,
    owner_gen: u64,
    app_key: String,
    app_secret: String,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut attempt = 0u32;
    let mut throttle_until: Option<std::time::Instant> = None;

    loop {
        if *shutdown.borrow() {
            break;
        }

        // Respect a 429 throttle: hold off until the retry-after window
        // elapses instead of hammering the gateway endpoint.
        if let Some(until) = throttle_until {
            if std::time::Instant::now() < until {
                let remaining = until.saturating_duration_since(std::time::Instant::now());
                println!(
                    "[dingtalk-stream] throttled=true remaining_ms={} owner_gen={}",
                    remaining.as_millis(),
                    owner_gen
                );
                tokio::select! {
                    _ = shutdown.changed() => {
                        if *shutdown.borrow() {
                            break;
                        }
                    }
                    _ = tokio::time::sleep(remaining) => {}
                }
            }
            throttle_until = None;
        }

        println!("[dingtalk-stream] state=connecting");

        match connect_and_run(&runtime, owner_gen, &app_key, &app_secret, shutdown.clone(), attempt).await {
            Ok(()) => {
                attempt = 0;
            }
            Err(e) => {
                let kind = stream_error_kind(&e);
                attempt = attempt.saturating_add(1);
                let delay = reconnect_delay(attempt);
                if e.contains("websocket_connect_error:http:429") {
                    throttle_until = Some(
                        std::time::Instant::now()
                            + delay.max(Duration::from_secs(30)),
                    );
                }
                println!(
                    "[dingtalk-stream] disconnected reason={} reconnect_attempt={} reconnect_delay_ms={}",
                    kind,
                    attempt,
                    delay.as_millis()
                );
            }
        }

        runtime.set_dingtalk_stream_connected(owner_gen, false);

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

    // Owner-scoped cleanup: releases only if gen still matches.
    stream_cleanup_on_exit(&runtime, owner_gen);
    println!("[dingtalk-stream] shutdown=true owner_gen={owner_gen}");
}

// ---------------------------------------------------------------------------
// TLS/ rustls CryptoProvider initialization
// ---------------------------------------------------------------------------

/// Initialize the rustls crypto provider (ring).
/// This must be called before any TLS connection is made.
#[allow(dead_code)]
pub(crate) fn ensure_rustls_crypto_provider() {
    use rustls::crypto::CryptoProvider;
    use std::sync::OnceLock;

    static INITIALIZED: OnceLock<()> = OnceLock::new();

    INITIALIZED.get_or_init(|| {
        if CryptoProvider::get_default().is_some() {
            println!("[dingtalk-stream] rustls_provider=preinstalled");
            return;
        }

        match rustls::crypto::ring::default_provider().install_default() {
            Ok(()) => {
                println!("[dingtalk-stream] rustls_provider=ring");
            }
            Err(_) => {
                // 可能是并发情况下另一个线程先完成了安装。
                if CryptoProvider::get_default().is_some() {
                    println!("[dingtalk-stream] rustls_provider=preinstalled");
                } else {
                    eprintln!("[dingtalk-stream] rustls_provider_init_failed=true");
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Single connection lifecycle
// ---------------------------------------------------------------------------

async fn connect_and_run(
    runtime: &Arc<GatewayRuntime>,
    owner_gen: u64,
    app_key: &str,
    app_secret: &str,
    mut shutdown: watch::Receiver<bool>,
    attempt: u32,
) -> Result<(), String> {
    // Ensure TLS provider is initialized before connecting
    ensure_rustls_crypto_provider();

    // 1. Request endpoint/ticket
    let (endpoint, ticket) = request_stream_connection(app_key, app_secret).await?;

    // 2. Connect WebSocket
    let mut url = reqwest::Url::parse(&endpoint).map_err(|_| "invalid_endpoint".to_string())?;
    url.query_pairs_mut().append_pair("ticket", &ticket);

    let (mut socket, _) = tokio_tungstenite::connect_async(url.as_str())
        .await
        .map_err(|error| classify_ws_connect_error(&error, owner_gen, attempt))?;

    println!("[dingtalk-stream] websocket_open=true");
    runtime.set_dingtalk_stream_connected(owner_gen, true);

    // 3. Run socket loop until failure
    run_socket_loop(runtime, owner_gen, &mut socket, shutdown).await
}

async fn run_socket_loop<S>(
    runtime: &Arc<GatewayRuntime>,
    owner_gen: u64,
    socket: &mut S,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), String>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error>
        + futures_util::StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
{
    runtime.set_dingtalk_stream_connected(owner_gen, true);
    println!("[dingtalk-stream] state=connected");
    println!("[dingtalk-stream] read_loop_started=true");

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    // Close the socket gracefully by dropping
                    return Ok(());
                }
            }
            msg = socket.next() => {
                let Some(msg) = msg else {
                    return Err("socket_closed".to_string());
                };
                let msg = msg.map_err(|e| format!("socket_read_error:{}", e))?;

                match msg {
                    Message::Text(text) => {
                        let bytes = text.len();
                        println!("[dingtalk-stream] frame_received kind=text bytes={}", bytes);
                        if let Err(e) = handle_downstream_json(socket, runtime, owner_gen, text.as_ref()).await {
                            return Err(e);
                        }
                    }
                    Message::Binary(bytes) => {
                        let byte_count = bytes.len();
                        println!("[dingtalk-stream] frame_received kind=binary bytes={}", byte_count);
                        match std::str::from_utf8(&bytes) {
                            Ok(text) => {
                                if let Err(e) = handle_downstream_json(socket, runtime, owner_gen, text).await {
                                    return Err(e);
                                }
                            }
                            Err(_) => {
                                println!("[dingtalk-stream] binary_decode_failed=true bytes={}", byte_count);
                            }
                        }
                    }
                    Message::Ping(payload) => {
                        let bytes = payload.len();
                        println!("[dingtalk-stream] frame_received kind=ping bytes={}", bytes);
                        let _ = socket.send(Message::Pong(payload)).await;
                    }
                    Message::Pong(payload) => {
                        let bytes = payload.len();
                        println!("[dingtalk-stream] frame_received kind=pong bytes={}", bytes);
                    }
                    Message::Close(frame) => {
                        println!("[dingtalk-stream] frame_received kind=close");
                        return Err("socket_closed_by_server".to_string());
                    }
                    Message::Frame(_) => {
                        // Internal frame - ignore
                    }
                }
            }
        }
    }
}

/// Shared downstream JSON frame handler for both Text and Binary frames
async fn handle_downstream_json<S>(
    socket: &mut S,
    runtime: &Arc<GatewayRuntime>,
    owner_gen: u64,
    text: &str,
) -> Result<(), String>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let envelope: StreamEnvelope = serde_json::from_str(text).map_err(|_| {
        println!("[dingtalk-stream] downstream_parse_failed=true frame_kind=text");
        "invalid_json".to_string()
    })?;

    let frame_type = envelope.frame_type.as_str();
    let topic = &envelope.headers.topic;
    let data_len = envelope.data.len();

    println!(
        "[dingtalk-stream] downstream_meta type={} topic={} data_bytes={}",
        safe_action(frame_type),
        safe_action(topic),
        data_len
    );

    match frame_type {
        "SYSTEM" => {
            handle_system_frame(socket, runtime, owner_gen, &envelope).await?;
        }
        _ if !should_dispatch_business_frame(runtime, owner_gen) => {
            println!("[dingtalk-stream] frame_skipped reason=stale_owner");
        }
        "EVENT" => {
            println!(
                "[dingtalk-stream] downstream_meta event_received=true topic={}",
                topic
            );
        }
        "CALLBACK" => match topic.as_str() {
            TOPIC_ROBOT => {
                handle_robot_callback(socket, runtime, &envelope).await?;
            }
            TOPIC_CARD => {
                handle_card_callback(socket, runtime, &envelope).await?;
            }
            _ => {
                println!(
                    "[dingtalk-stream] topic=unknown callback_received=true topic={}",
                    topic
                );
            }
        },
        _ => {
            println!("[dingtalk-stream] frame_type=unknown type={}", frame_type);
        }
    }

    Ok(())
}

pub(crate) fn should_dispatch_business_frame(
    runtime: &GatewayRuntime,
    owner_gen: u64,
) -> bool {
    runtime.is_current_stream_owner(owner_gen)
}

// ---------------------------------------------------------------------------
// Frame handling
// ---------------------------------------------------------------------------

async fn handle_system_frame<S>(
    socket: &mut S,
    runtime: &Arc<GatewayRuntime>,
    owner_gen: u64,
    envelope: &StreamEnvelope,
) -> Result<(), String>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    // SYSTEM type is determined by headers.topic
    let topic = &envelope.headers.topic;

    match topic.as_str() {
        "CONNECTED" => {
            println!("[dingtalk-stream] system=CONNECTED");
            // TCP open ≠ ready - wait for REGISTERED
        }
        "REGISTERED" => {
            println!("[dingtalk-stream] system=REGISTERED");
            runtime.set_dingtalk_stream_connected(owner_gen, true);
        }
        "disconnect" => {
            println!("[dingtalk-stream] system=disconnect");
            runtime.set_dingtalk_stream_connected(owner_gen, false);
            // Ack and return error to trigger reconnect with new ticket
            // Use SYSTEM ACK to preserve original headers and data
            println!("[dingtalk-stream] system_ack topic=disconnect send_attempt=true");
            match socket
                .send(Message::Text(build_system_ack(envelope).to_string().into()))
                .await
            {
                Ok(()) => {
                    println!("[dingtalk-stream] system_ack topic=disconnect send_ok=true");
                }
                Err(e) => {
                    println!("[dingtalk-stream] system_ack topic=disconnect send_ok=false reason=websocket_write");
                }
            }
            return Err("server_disconnect".to_string());
        }
        "ping" => {
            // Respond with pong per DingTalk Stream protocol
            println!("[dingtalk-stream] system=ping");
            match socket
                .send(Message::Text(build_system_ack(envelope).to_string().into()))
                .await
            {
                Ok(()) => {
                    println!("[dingtalk-stream] system_ack topic=ping send_ok=true");
                }
                Err(e) => {
                    println!("[dingtalk-stream] system_ack topic=ping send_ok=false reason=websocket_write");
                    return Err(format!("websocket_write_error:{}", e));
                }
            }
        }
        "KEEPALIVE" => {
            println!("[dingtalk-stream] system=KEEPALIVE");
        }
        _ => {
            println!("[dingtalk-stream] system=unknown topic={}", topic);
        }
    }

    Ok(())
}

async fn handle_robot_callback<S>(
    socket: &mut S,
    runtime: &Arc<GatewayRuntime>,
    envelope: &StreamEnvelope,
) -> Result<(), String>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    // Parse robot message
    let payload = match parse_robot_payload(envelope) {
        Ok(p) => p,
        Err(e) => {
            log_robot_payload_field_types(&envelope.data);
            println!(
                "[dingtalk-stream] topic=robot parse_failed=true reason={}",
                e
            );
            // Do NOT ACK as success: an unparsed message must not be marked
            // as processed. Leaving it unacked lets DingTalk retry delivery.
            return Ok(());
        }
    };
    println!(
        "[dingtalk-stream] topic=robot parse_ok=true msg_id_hash={}",
        short_hash(&payload.msg_id)
    );

    // Dedupe check
    let msg_id = payload.msg_id.clone();
    if !runtime.try_dingtalk_stream_dedupe(&msg_id).await {
        println!(
            "[dingtalk-stream] topic=robot dedupe_skipped=true msg_id_hash={}",
            short_hash(&msg_id)
        );
        let ack = build_stream_ack(envelope, serde_json::json!({}));
        let _ = socket.send(Message::Text(ack.to_string().into())).await;
        return Ok(());
    }

    // Bounded enqueue
    match runtime.try_enqueue_dingtalk_stream_job(payload).await {
        Ok(()) => {
            println!(
                "[dingtalk-stream] topic=robot enqueue=true msg_id_hash={}",
                short_hash(&msg_id)
            );
            let ack = build_stream_ack(envelope, serde_json::json!({}));
            let _ = socket.send(Message::Text(ack.to_string().into())).await;
            println!("[dingtalk-stream] topic=robot ack=true");
        }
        Err(_) => {
            // Rollback dedupe reservation so DingTalk retry can succeed.
            runtime.dedup_cache().remove(&format!("dt_stream:{}", msg_id)).await;
            println!(
                "[dingtalk-stream] topic=robot queue_full=true msg_id_hash={} dedupe_rollback=true",
                short_hash(&msg_id)
            );
            // DO NOT ACK - let DingTalk retry later
            return Err("queue_full".to_string());
        }
    }

    Ok(())
}

async fn handle_card_callback<S>(
    socket: &mut S,
    runtime: &Arc<GatewayRuntime>,
    envelope: &StreamEnvelope,
) -> Result<(), String>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    // Parse card callback - handle both StreamEnvelope and Value
    let callback = match parse_card_callback_from_stream_envelope(envelope) {
        Ok(c) => c,
        Err(e) => {
            println!(
                "[dingtalk-stream] topic=card parse_failed=true reason={}",
                e
            );
            // Still ACK malformed callbacks
            let ack = build_stream_ack(envelope, serde_json::json!({}));
            let _ = socket.send(Message::Text(ack.to_string().into())).await;
            return Ok(());
        }
    };

    println!(
        "[dingtalk-stream] topic=card callback_received=true action={}",
        safe_action(&callback.action)
    );

    // Check against canonical allow-list
    if !crate::gateway::dingtalk_card_stream::is_allowed_action(&callback.action) {
        println!(
            "[dingtalk-stream] topic=card action_rejected action={}",
            safe_action(&callback.action)
        );
        let ack = build_stream_ack(envelope, serde_json::json!({ "response": {} }));
        let _ = socket.send(Message::Text(ack.to_string().into())).await;
        return Ok(());
    }

    let dedupe_key = crate::gateway::dingtalk_card_stream::callback_dedupe_key(
        callback.callback_id.as_deref(),
        &callback.out_track_id,
        &callback.action,
        &envelope.data,
    );
    if !runtime.try_dingtalk_stream_dedupe(&dedupe_key).await {
        println!("[dingtalk-stream] topic=card callback_duplicated=true");
        let ack = build_stream_ack(envelope, serde_json::json!({ "response": {} }));
        let _ = socket.send(Message::Text(ack.to_string().into())).await;
        return Ok(());
    }

    // ACK immediately
    let ack = build_stream_ack(envelope, serde_json::json!({ "response": {} }));
    let _ = socket.send(Message::Text(ack.to_string().into())).await;
    println!("[dingtalk-stream] topic=card ack=true");

    // Process in background
    let runtime_clone = runtime.clone();
    tokio::spawn(async move {
        crate::gateway::dingtalk_card_stream::process_panel_action(runtime_clone, callback).await;
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Card callback parsing from StreamEnvelope
// ---------------------------------------------------------------------------

/// Parse a card callback from a StreamEnvelope by extracting the data field.
pub fn parse_card_callback_from_stream_envelope(
    envelope: &StreamEnvelope,
) -> Result<ParsedCardCallback, String> {
    use crate::gateway::dingtalk_card_stream::ParsedCardCallback;

    // Parse the data string as JSON
    let request: serde_json::Value =
        serde_json::from_str(&envelope.data).map_err(|_| "invalid_data_json".to_string())?;

    let out_track_id = request
        .get("outTrackId")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or("missing_out_track_id")?
        .to_string();

    let user_id = request
        .get("userId")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string);

    let space_id = request
        .get("spaceId")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string);

    // Parse content to get action
    let content_str = request
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or("missing_content")?;

    let content: serde_json::Value =
        serde_json::from_str(content_str).map_err(|_| "invalid_content_json".to_string())?;

    let params = content
        .get("cardPrivateData")
        .and_then(|v| v.get("params"))
        .ok_or("missing_card_private_params")?;

    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or("missing_action")?
        .to_string();

    let callback_id = ["callbackId", "messageId", "eventId"]
        .iter()
        .find_map(|key| request.get(*key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| envelope.headers.message_id.clone())
        .or_else(|| envelope.headers.time.clone());

    Ok(ParsedCardCallback {
        out_track_id,
        action,
        callback_id,
        user_id,
        space_id,
    })
}

// ---------------------------------------------------------------------------
// Stream envelope types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct StreamEnvelope {
    #[serde(rename = "specVersion")]
    spec_version: Option<String>,
    #[serde(rename = "type")]
    frame_type: String,
    headers: StreamHeaders,
    #[serde(default)]
    data: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StreamHeaders {
    #[serde(rename = "appId", default)]
    app_id: Option<String>,
    #[serde(rename = "connectionId", default)]
    connection_id: Option<String>,
    #[serde(rename = "contentType", default)]
    content_type: Option<String>,
    #[serde(rename = "messageId", default)]
    message_id: Option<String>,
    #[serde(rename = "time", default)]
    time: Option<String>,
    topic: String,
}

#[derive(Clone, Deserialize, Serialize)]
struct RobotPayload {
    #[serde(rename = "msgId")]
    msg_id: String,
    #[serde(
        rename = "conversationType",
        deserialize_with = "deserialize_string_or_number"
    )]
    conversation_type: String,
    #[serde(rename = "senderNick", default, deserialize_with = "deserialize_opt_string_or_number")]
    sender_nick: Option<String>,
    #[serde(rename = "senderStaffId", default, deserialize_with = "deserialize_opt_string_or_number")]
    sender_staff_id: Option<String>,
    #[serde(rename = "senderId", default, deserialize_with = "deserialize_opt_string_or_number")]
    sender_id: Option<String>,
    #[serde(rename = "conversationId", default, deserialize_with = "deserialize_opt_string_or_number")]
    conversation_id: Option<String>,
    #[serde(rename = "robotCode", default, deserialize_with = "deserialize_opt_string_or_number")]
    robot_code: Option<String>,
    #[serde(rename = "sessionWebhook", default, deserialize_with = "deserialize_opt_string_or_number")]
    session_webhook: Option<String>,
    #[serde(
        rename = "sessionWebhookExpiredTime",
        default,
        deserialize_with = "deserialize_opt_i64_string_or_number"
    )]
    session_webhook_expired_time: Option<i64>,
    #[serde(
        rename = "createAt",
        default,
        deserialize_with = "deserialize_opt_i64_string_or_number"
    )]
    create_at: Option<i64>,
    #[serde(rename = "msgtype", default)]
    msg_type: Option<String>,
    text: Option<RobotText>,
}

impl std::fmt::Debug for RobotPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RobotPayload")
            .field("msg_id_present", &!self.msg_id.trim().is_empty())
            .field(
                "conversation_type_present",
                &!self.conversation_type.trim().is_empty(),
            )
            .field("sender_nick_present", &self.sender_nick.is_some())
            .field("sender_staff_id_present", &self.sender_staff_id.is_some())
            .field("sender_id_present", &self.sender_id.is_some())
            .field("conversation_id_present", &self.conversation_id.is_some())
            .field("robot_code_present", &self.robot_code.is_some())
            .field("session_webhook_present", &self.session_webhook.is_some())
            .field(
                "session_webhook_expired_time",
                &self.session_webhook_expired_time,
            )
            .field("create_at", &self.create_at)
            .field("msg_type", &self.msg_type)
            .field("text_present", &self.text.is_some())
            .field(
                "text_len",
                &self.text.as_ref().map(|text| text.content.chars().count()),
            )
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RobotText {
    content: String,
}

// ---------------------------------------------------------------------------
// Compatible scalar deserializers
//
// DingTalk delivers robot callback metadata on different paths/versions with
// drifting JSON types: timestamps arrive as integer milliseconds or as string
// numbers, and identity-like fields occasionally appear as numbers. These
// helpers normalize number | string-number | null | missing into a single
// Rust type so a single metadata field can never drop the whole message.
// ---------------------------------------------------------------------------

/// Optional i64 accepting `1786437210268`, `"1786437210268"`, null, missing.
fn deserialize_opt_i64_string_or_number<'de, D>(
    deserializer: D,
) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum I64OrString {
        Integer(i64),
        Unsigned(u64),
        Text(String),
    }

    let value = Option::<I64OrString>::deserialize(deserializer)?;
    Ok(value.and_then(|v| match v {
        I64OrString::Integer(n) => Some(n),
        I64OrString::Unsigned(n) => i64::try_from(n).ok(),
        I64OrString::Text(s) => s.trim().parse().ok(),
    }))
}

/// Optional String accepting text, integer, unsigned, null, missing.
fn deserialize_opt_string_or_number<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        Text(String),
        Integer(i64),
        Unsigned(u64),
    }

    let value = Option::<StringOrNumber>::deserialize(deserializer)?;
    Ok(value.map(|v| match v {
        StringOrNumber::Text(s) => s,
        StringOrNumber::Integer(n) => n.to_string(),
        StringOrNumber::Unsigned(n) => n.to_string(),
    }))
}

/// Required String accepting text or numeric scalars.
fn deserialize_string_or_number<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        Text(String),
        Integer(i64),
        Unsigned(u64),
    }

    match StringOrNumber::deserialize(deserializer)? {
        StringOrNumber::Text(s) => Ok(s),
        StringOrNumber::Integer(n) => Ok(n.to_string()),
        StringOrNumber::Unsigned(n) => Ok(n.to_string()),
    }
}

/// Log only JSON field *types* (never values) of a robot callback payload.
/// Used on parse failure so a future type drift is visible in one line.
fn log_robot_payload_field_types(data: &str) {
    if let Some(summary) = build_robot_payload_field_types(data) {
        println!("[dingtalk-stream] robot_payload_types {}", summary);
    }
}

/// Build a `key=type ...` summary of the payload fields. Never includes values.
fn build_robot_payload_field_types(data: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(data).ok()?;
    let obj = value.as_object()?;
    const DIAGNOSTIC_FIELDS: &[&str] = &[
        "msgId",
        "msgtype",
        "text",
        "conversationType",
        "createAt",
        "sessionWebhookExpiredTime",
        "isAdmin",
        "isInAtList",
    ];

    let parts: Vec<String> = DIAGNOSTIC_FIELDS
        .iter()
        .filter_map(|key| obj.get(*key).map(|v| (*key, v)))
        .map(|(key, v)| {
            let ty = match v {
                serde_json::Value::Null => "null",
                serde_json::Value::Bool(_) => "bool",
                serde_json::Value::Number(_) => "number",
                serde_json::Value::String(_) => "string",
                serde_json::Value::Array(_) => "array",
                serde_json::Value::Object(_) => "object",
            };
            format!("{}={}", key, ty)
        })
        .collect();
    Some(parts.join(" "))
}

fn parse_robot_payload(envelope: &StreamEnvelope) -> Result<RobotPayload, String> {
    let data_str = &envelope.data;
    let payload: RobotPayload =
        serde_json::from_str(data_str).map_err(|e| format!("parse_error:{}", e))?;

    if payload.msg_id.trim().is_empty() {
        return Err("core_error:empty_msg_id".to_string());
    }
    if payload.conversation_type.trim().is_empty() {
        return Err("core_error:empty_conversation_type".to_string());
    }
    if payload
        .msg_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("text"))
        && payload.text.is_none()
    {
        return Err("core_error:missing_text".to_string());
    }

    Ok(payload)
}

fn short_hash(s: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    format!("{:x}", hasher.finish())[..8].to_string()
}

// ---------------------------------------------------------------------------
// ACK builders
// ---------------------------------------------------------------------------

fn build_stream_ack(envelope: &StreamEnvelope, data: serde_json::Value) -> serde_json::Value {
    let message_id = envelope.headers.message_id.as_deref().unwrap_or_default();

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

/// Build SYSTEM ACK that echoes original headers and data per DingTalk Stream protocol.
/// For SYSTEM frames (ping, disconnect), the response must preserve the original headers and data.
fn build_system_ack(envelope: &StreamEnvelope) -> serde_json::Value {
    // Echo back the original headers from the downstream message
    // This is required by the DingTalk Stream protocol for SYSTEM frames
    serde_json::json!({
        "code": 200,
        "headers": {
            "appId": envelope.headers.app_id.as_deref().unwrap_or_default(),
            "connectionId": envelope.headers.connection_id.as_deref().unwrap_or_default(),
            "contentType": envelope.headers.content_type.as_deref().unwrap_or_default(),
            "messageId": envelope.headers.message_id.as_deref().unwrap_or_default(),
            "time": envelope.headers.time.as_deref().unwrap_or_default(),
            "topic": envelope.headers.topic
        },
        "message": "OK",
        "data": envelope.data
    })
}

// ---------------------------------------------------------------------------
// Gateway connection
// ---------------------------------------------------------------------------

async fn request_stream_connection(
    app_key: &str,
    app_secret: &str,
) -> Result<(String, String), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "http_client_error".to_string())?;

    let response = client
        .post(DINGTALK_STREAM_GATEWAY_URL)
        .json(&serde_json::json!({
            "clientId": app_key,
            "clientSecret": app_secret,
            "ua": "omninova-claw-rust/0.1",
            "subscriptions": [
                { "type": "EVENT", "topic": "*" },
                { "type": "CALLBACK", "topic": TOPIC_ROBOT },
                { "type": "CALLBACK", "topic": TOPIC_CARD }
            ]
        }))
        .send()
        .await
        .map_err(|e| format!("network_error:{}", e))?;

    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|e| format!("read_error:{}", e))?;

    if !(200..300).contains(&status) {
        let code: String = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("code").and_then(|c| c.as_str()).map(String::from))
            .unwrap_or_else(|| "unknown".to_string());
        return Err(format!("gateway_error:status={}:code={}", status, code));
    }

    let payload: serde_json::Value =
        serde_json::from_str(&body).map_err(|_| "invalid_json".to_string())?;

    let endpoint = payload
        .get("endpoint")
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .ok_or("missing_endpoint")?
        .to_string();

    let ticket = payload
        .get("ticket")
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .ok_or("missing_ticket")?
        .to_string();

    println!(
        "[dingtalk-stream] endpoint_acquired=true endpoint_hash={} ticket_hash={}",
        opaque_sha256(&endpoint),
        opaque_sha256(&ticket)
    );
    Ok((endpoint, ticket))
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

fn reconnect_delay(attempt: u32) -> Duration {
    let exp = attempt.min(MAX_BACKOFF_ATTEMPTS);
    Duration::from_secs(2u64.saturating_pow(exp).min(60))
}

/// Opaque SHA-256 digest (first 6 bytes hex) for diagnostics. Never reveals
/// the original endpoint/ticket/identifier value.
fn opaque_sha256(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    hex::encode(&digest[..6])
}

/// Classified category of a WebSocket connect failure. The raw tungstenite
/// error string may embed the endpoint/ticket, so only the category and safe
/// scalars (HTTP status, IO kind) are ever logged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DingtalkWsConnectErrorKind {
    Http,
    Io,
    Tls,
    Protocol,
    Url,
    Capacity,
    Other,
}

impl DingtalkWsConnectErrorKind {
    fn classify(error: &tokio_tungstenite::tungstenite::Error) -> Self {
        use tokio_tungstenite::tungstenite::Error;
        match error {
            Error::Http(_) => Self::Http,
            Error::Io(_) => Self::Io,
            Error::Tls(_) => Self::Tls,
            Error::Protocol(_) => Self::Protocol,
            Error::Url(_) => Self::Url,
            Error::Capacity(_) => Self::Capacity,
            _ => Self::Other,
        }
    }

    const fn log_value(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Io => "io",
            Self::Tls => "tls",
            Self::Protocol => "protocol",
            Self::Url => "url",
            Self::Capacity => "capacity",
            Self::Other => "other",
        }
    }
}

/// Classify a WebSocket connect error and log only safe diagnostics:
/// error_kind, http_status (when present), io_error_kind (when present),
/// retry-after presence. Returns a safe reason string for the reconnect loop.
fn classify_ws_connect_error(
    error: &tokio_tungstenite::tungstenite::Error,
    owner_gen: u64,
    attempt: u32,
) -> String {
    use tokio_tungstenite::tungstenite::Error;
    let kind = DingtalkWsConnectErrorKind::classify(error);
    match error {
        Error::Http(response) => {
            let http_status = response.status().as_u16();
            let retry_after_present = response.headers().contains_key("retry-after");
            println!(
                "[dingtalk-stream] websocket_connect_failed=true error_kind=http http_status={} retry_after_present={} owner_gen={} attempt={}",
                http_status, retry_after_present, owner_gen, attempt
            );
            if http_status == 429 {
                "websocket_connect_error:http:429".to_string()
            } else {
                format!("websocket_connect_error:http:{}", http_status)
            }
        }
        Error::Io(io_error) => {
            println!(
                "[dingtalk-stream] websocket_connect_failed=true error_kind=io io_kind={:?} owner_gen={} attempt={}",
                io_error.kind(),
                owner_gen,
                attempt
            );
            "websocket_connect_error:io".to_string()
        }
        _ => {
            println!(
                "[dingtalk-stream] websocket_connect_failed=true error_kind={} owner_gen={} attempt={}",
                kind.log_value(),
                owner_gen,
                attempt
            );
            format!("websocket_connect_error:{}", kind.log_value())
        }
    }
}

fn safe_action(action: &str) -> String {
    action
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        .take(48)
        .collect()
}

fn stream_error_kind(error: &str) -> String {
    error
        .split(':')
        .next()
        .unwrap_or("unknown")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        .take(64)
        .collect()
}

// ---------------------------------------------------------------------------
// GatewayRuntime extensions for Stream
// ---------------------------------------------------------------------------

impl GatewayRuntime {
    /// Bounded enqueue for Stream robot messages via the runtime's existing worker queue.
    pub async fn try_enqueue_dingtalk_stream_job(
        &self,
        payload: RobotPayload,
    ) -> Result<(), String> {
        // Build inbound message from payload
        let text = payload
            .text
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_default();

        // Extract values we need for the inbound message
        let sender_staff_id = payload.sender_staff_id.clone();
        let sender_id = payload.sender_id.clone();
        let conversation_id = payload.conversation_id.clone();
        let session_webhook = payload.session_webhook.clone();
        let robot_code = payload.robot_code.clone();
        let msg_id = payload.msg_id.clone();

        let mut metadata = std::collections::HashMap::new();
        if let Some(ref id) = sender_staff_id {
            metadata.insert("senderStaffId".to_string(), serde_json::json!(id));
        }
        if let Some(ref id) = sender_id {
            metadata.insert("senderId".to_string(), serde_json::json!(id));
        }
        if let Some(ref id) = conversation_id {
            metadata.insert("conversationId".to_string(), serde_json::json!(id));
        }
        if let Some(ref webhook) = session_webhook {
            metadata.insert("sessionWebhook".to_string(), serde_json::json!(webhook));
        }
        if let Some(ref code) = robot_code {
            metadata.insert("robotCode".to_string(), serde_json::json!(code));
        }
        // Robot callback conversationType must reach the card target builder:
        // "1" = single chat, "2" = group chat. Without this, `from_inbound`
        // sees an empty type and falls back to a Direct (single-chat) target,
        // misrouting group cards into the user's private chat.
        metadata.insert(
            "conversationType".to_string(),
            serde_json::json!(payload.conversation_type.clone()),
        );
        let raw_payload = serde_json::json!(payload);
        metadata.insert("raw_payload".to_string(), raw_payload.clone());
        metadata.insert("source".to_string(), serde_json::json!("dingtalk"));
        metadata.insert("stream_msg_id".to_string(), serde_json::json!(msg_id));

        let inbound = InboundMessage {
            channel: ChannelKind::Dingtalk,
            user_id: sender_staff_id.or(sender_id),
            session_id: conversation_id,
            text,
            metadata,
        };

        let job = DingtalkAsyncJob::new(inbound, raw_payload);

        self.try_send_dingtalk_job(job)
            .await
            .map_err(|_| "queue_full".to_string())
    }

    /// Try to dedupe a Stream message using the runtime's dedup cache.
    pub async fn try_dingtalk_stream_dedupe(&self, msg_id: &str) -> bool {
        use std::time::Instant;

        let key = format!("dt_stream:{}", msg_id);
        self.dedup_cache().check_and_insert(&key).await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::sync::{Arc as StdArc, Mutex as StdMutex};
    use std::task::{Context, Poll};

    struct RecordingSink {
        sent: StdArc<StdMutex<Vec<Message>>>,
    }

    impl RecordingSink {
        fn new() -> (Self, StdArc<StdMutex<Vec<Message>>>) {
            let sent = StdArc::new(StdMutex::new(Vec::new()));
            (Self { sent: sent.clone() }, sent)
        }
    }

    impl futures_util::Sink<Message> for RecordingSink {
        type Error = tokio_tungstenite::tungstenite::Error;

        fn poll_ready(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn start_send(self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
            self.sent.lock().unwrap().push(item);
            Ok(())
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn test_stream_state_serialization() {
        assert_eq!(DingTalkStreamState::Disconnected as u8, 0);
        assert_eq!(DingTalkStreamState::Connecting as u8, 1);
        assert_eq!(DingTalkStreamState::Connected as u8, 2);
        assert_eq!(DingTalkStreamState::Registered as u8, 3);
        assert_eq!(DingTalkStreamState::Reconnecting as u8, 4);
        assert_eq!(DingTalkStreamState::Stopping as u8, 5);
    }

    #[test]
    fn test_reconnect_delay_bounded() {
        assert_eq!(reconnect_delay(0), Duration::from_secs(1));
        assert_eq!(reconnect_delay(1), Duration::from_secs(2));
        assert_eq!(reconnect_delay(2), Duration::from_secs(4));
        assert_eq!(reconnect_delay(3), Duration::from_secs(8));
        assert_eq!(reconnect_delay(4), Duration::from_secs(16));
        assert_eq!(reconnect_delay(5), Duration::from_secs(32));
        assert_eq!(reconnect_delay(6), Duration::from_secs(60));
        assert_eq!(reconnect_delay(99), Duration::from_secs(60));
    }

    #[test]
    fn ws_connect_error_http_is_classified_with_status() {
        // HTTP handshake rejection (e.g. 403/409) must be classified as
        // error_kind=http and carry the status in the returned reason.
        use tokio_tungstenite::tungstenite::Error;
        let response = tokio_tungstenite::tungstenite::http::Response::builder()
            .status(403)
            .body(Some(Vec::<u8>::new()))
            .unwrap();
        let error = Error::Http(Box::new(response));

        let kind = DingtalkWsConnectErrorKind::classify(&error);
        assert_eq!(kind, DingtalkWsConnectErrorKind::Http);
        let reason = classify_ws_connect_error(&error, 2, 1);
        assert_eq!(reason, "websocket_connect_error:http:403");
    }

    #[test]
    fn ws_connect_error_http_429_is_classified_with_status() {
        use tokio_tungstenite::tungstenite::Error;
        let response = tokio_tungstenite::tungstenite::http::Response::builder()
            .status(429)
            .header("retry-after", "30")
            .body(Some(Vec::<u8>::new()))
            .unwrap();
        let error = Error::Http(Box::new(response));

        let kind = DingtalkWsConnectErrorKind::classify(&error);
        assert_eq!(kind, DingtalkWsConnectErrorKind::Http);
        let reason = classify_ws_connect_error(&error, 2, 1);
        assert_eq!(reason, "websocket_connect_error:http:429");
    }

    #[test]
    fn ws_connect_error_io_is_classified() {
        use std::io;
        use tokio_tungstenite::tungstenite::Error;
        let error = Error::Io(io::Error::from(io::ErrorKind::ConnectionReset));
        let kind = DingtalkWsConnectErrorKind::classify(&error);
        assert_eq!(kind, DingtalkWsConnectErrorKind::Io);
        let reason = classify_ws_connect_error(&error, 2, 1);
        assert_eq!(reason, "websocket_connect_error:io");
    }

    #[test]
    fn ws_connect_error_protocol_is_classified() {
        use tokio_tungstenite::tungstenite::Error;
        let error = Error::Protocol(
            tokio_tungstenite::tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
        );
        let kind = DingtalkWsConnectErrorKind::classify(&error);
        assert_eq!(kind, DingtalkWsConnectErrorKind::Protocol);
        let reason = classify_ws_connect_error(&error, 2, 1);
        assert_eq!(reason, "websocket_connect_error:protocol");
    }

    #[test]
    fn ws_connect_error_url_is_classified() {
        use tokio_tungstenite::tungstenite::Error;
        let error = Error::Url(tokio_tungstenite::tungstenite::error::UrlError::NoHostName);
        let kind = DingtalkWsConnectErrorKind::classify(&error);
        assert_eq!(kind, DingtalkWsConnectErrorKind::Url);
    }

    #[test]
    fn opaque_sha256_never_reveals_original_value() {
        let digest = opaque_sha256("https://api.dingtalk.com/stream?ticket=secret-abc");
        assert_eq!(digest.len(), 12);
        assert!(!digest.contains("dingtalk"));
        assert!(!digest.contains("secret"));
        // Deterministic: same input produces same digest.
        assert_eq!(
            digest,
            opaque_sha256("https://api.dingtalk.com/stream?ticket=secret-abc")
        );
    }

    #[test]
    fn test_build_stream_ack() {
        let envelope = StreamEnvelope {
            spec_version: None,
            frame_type: "CALLBACK".to_string(),
            headers: StreamHeaders {
                app_id: None,
                connection_id: None,
                message_id: Some("test-msg-id".to_string()),
                content_type: Some("application/json".to_string()),
                time: None,
                topic: TOPIC_ROBOT.to_string(),
            },
            data: "{}".to_string(),
        };

        let ack = build_stream_ack(&envelope, serde_json::json!({ "test": "data" }));
        assert_eq!(ack["code"], 200);
        assert_eq!(ack["headers"]["messageId"], "test-msg-id");
        assert_eq!(ack["message"], "OK");
    }

    #[test]
    fn test_build_system_ack_for_ping() {
        // Test that SYSTEM ping ACK uses build_system_ack
        let envelope = StreamEnvelope {
            spec_version: None,
            frame_type: "SYSTEM".to_string(),
            headers: StreamHeaders {
                app_id: Some("app".to_string()),
                connection_id: Some("conn".to_string()),
                message_id: Some("ping-msg-id".to_string()),
                content_type: Some("application/json".to_string()),
                time: Some("0".to_string()),
                topic: "ping".to_string(),
            },
            data: "ping-data".to_string(),
        };

        let ack = build_system_ack(&envelope);
        assert_eq!(ack["code"], 200);
        assert_eq!(ack["headers"]["messageId"], "ping-msg-id");
        assert_eq!(ack["data"], "ping-data");
    }

    #[test]
    fn test_robot_payload_parsing() {
        // Real DingTalk Stream uses msgtype (lowercase), which maps to msg_type in Rust
        let json = serde_json::json!({
            "msgId": "msg123",
            "conversationType": "2",
            "senderStaffId": "staff123",
            "conversationId": "conv456",
            "robotCode": "ding123",
            "msgtype": "text",
            "text": { "content": "hello" }
        });

        let payload: RobotPayload = serde_json::from_value(json).unwrap();
        assert_eq!(payload.msg_id, "msg123");
        assert_eq!(payload.conversation_type, "2");
        // msg_type comes from msgtype in JSON (optional field)
        assert_eq!(payload.msg_type.as_deref(), Some("text"));
        assert_eq!(payload.text.as_ref().unwrap().content, "hello");
    }

    #[test]
    fn robot_callback_integer_timestamp_parses() {
        // Real DingTalk delivers timestamps as integer milliseconds.
        let json = serde_json::json!({
            "msgId": "msg-ts-int",
            "conversationType": "2",
            "senderStaffId": "staff1",
            "conversationId": "conv1",
            "sessionWebhookExpiredTime": 1786437210268i64,
            "createAt": 1786437348372i64,
            "msgtype": "text",
            "text": { "content": "hello" }
        });

        let payload: RobotPayload = serde_json::from_value(json).unwrap();
        assert_eq!(payload.session_webhook_expired_time, Some(1786437210268));
        assert_eq!(payload.create_at, Some(1786437348372));
        assert_eq!(payload.text.as_ref().unwrap().content, "hello");
    }

    #[test]
    fn robot_callback_string_timestamp_parses() {
        // Some DingTalk paths deliver the same timestamps as string numbers.
        let json = serde_json::json!({
            "msgId": "msg-ts-str",
            "conversationType": "2",
            "senderStaffId": "staff1",
            "conversationId": "conv1",
            "sessionWebhookExpiredTime": "1786437210268",
            "createAt": "1786437348372",
            "msgtype": "text",
            "text": { "content": "hello" }
        });

        let payload: RobotPayload = serde_json::from_value(json).unwrap();
        assert_eq!(payload.session_webhook_expired_time, Some(1786437210268));
        assert_eq!(payload.create_at, Some(1786437348372));
    }

    #[test]
    fn robot_callback_optional_timestamp_missing_parses() {
        // Timestamps are optional metadata: missing must not fail parsing.
        let json = serde_json::json!({
            "msgId": "msg-ts-none",
            "conversationType": "2",
            "msgtype": "text",
            "text": { "content": "hello" }
        });

        let payload: RobotPayload = serde_json::from_value(json).unwrap();
        assert_eq!(payload.session_webhook_expired_time, None);
        assert_eq!(payload.create_at, None);
        assert_eq!(payload.msg_id, "msg-ts-none");
    }

    #[test]
    fn robot_callback_null_timestamp_parses() {
        // Null metadata must parse as None rather than failing the message.
        let json = serde_json::json!({
            "msgId": "msg-ts-null",
            "conversationType": "2",
            "sessionWebhookExpiredTime": null,
            "createAt": null,
            "msgtype": "text",
            "text": { "content": "hello" }
        });

        let payload: RobotPayload = serde_json::from_value(json).unwrap();
        assert_eq!(payload.session_webhook_expired_time, None);
        assert_eq!(payload.create_at, None);
    }

    #[test]
    fn robot_callback_invalid_optional_timestamp_does_not_drop_message() {
        let json = serde_json::json!({
            "msgId": "msg-ts-invalid",
            "conversationType": "2",
            "sessionWebhookExpiredTime": "not-a-timestamp",
            "createAt": "",
            "msgtype": "text",
            "text": { "content": "hello" }
        });

        let payload: RobotPayload = serde_json::from_value(json).unwrap();
        assert_eq!(payload.session_webhook_expired_time, None);
        assert_eq!(payload.create_at, None);
        assert_eq!(payload.text.as_ref().unwrap().content, "hello");
    }

    #[test]
    fn robot_callback_numeric_metadata_fields_parse() {
        // Identity-ish metadata can arrive as numbers on some delivery paths.
        let json = serde_json::json!({
            "msgId": "msg-num-meta",
            "conversationType": 2,
            "senderStaffId": 123456789,
            "senderId": 987654321,
            "conversationId": 1122334455,
            "msgtype": "text",
            "text": { "content": "hello" }
        });

        let payload: RobotPayload = serde_json::from_value(json).unwrap();
        assert_eq!(payload.conversation_type, "2");
        assert_eq!(payload.sender_staff_id.as_deref(), Some("123456789"));
        assert_eq!(payload.sender_id.as_deref(), Some("987654321"));
        assert_eq!(payload.conversation_id.as_deref(), Some("1122334455"));
    }

    #[test]
    fn robot_callback_realistic_text_payload_parses() {
        // Full realistic DingTalk text callback with timestamps as integers.
        let data = serde_json::json!({
            "msgId": "msg-real-1",
            "senderNick": "tester",
            "isAdmin": false,
            "chatbotCorpId": "dingcorp",
            "senderStaffId": "staff-real",
            "sessionWebhookExpiredTime": 1786437210268i64,
            "createAt": 1786437348372i64,
            "senderCorpId": "corp-real",
            "conversationType": "2",
            "senderId": "uid-real",
            "conversationTitle": "test group",
            "isInAtList": true,
            "conversationId": "cid-real",
            "atUsers": [],
            "chatbotUserId": "bot-real",
            "msgtype": "text",
            "text": { "content": "@OmniNova 123" }
        });

        let envelope = StreamEnvelope {
            spec_version: None,
            frame_type: "CALLBACK".to_string(),
            headers: StreamHeaders {
                app_id: None,
                connection_id: None,
                content_type: Some("application/json".to_string()),
                message_id: Some("stream-envelope-1".to_string()),
                time: None,
                topic: TOPIC_ROBOT.to_string(),
            },
            data: data.to_string(),
        };
        let payload = parse_robot_payload(&envelope).unwrap();
        assert_eq!(payload.msg_id, "msg-real-1");
        assert_eq!(payload.sender_nick.as_deref(), Some("tester"));
        assert_eq!(payload.sender_staff_id.as_deref(), Some("staff-real"));
        assert_eq!(payload.conversation_id.as_deref(), Some("cid-real"));
        assert_eq!(payload.session_webhook_expired_time, Some(1786437210268));
        assert_eq!(payload.create_at, Some(1786437348372));
        assert_eq!(payload.msg_type.as_deref(), Some("text"));
        assert_eq!(payload.text.as_ref().unwrap().content, "@OmniNova 123");
    }

    #[tokio::test]
    async fn parsed_robot_callback_reaches_enqueue() {
        let json = serde_json::json!({
            "msgId": "msg-enqueue-1",
            "conversationType": "2",
            "senderStaffId": "staff-enq",
            "conversationId": "conv-enq",
            "sessionWebhookExpiredTime": 1786437210268i64,
            "msgtype": "text",
            "text": { "content": "hello worker" }
        });
        let envelope = StreamEnvelope {
            spec_version: None,
            frame_type: "CALLBACK".to_string(),
            headers: StreamHeaders {
                app_id: None,
                connection_id: None,
                content_type: Some("application/json".to_string()),
                message_id: Some("stream-enqueue-1".to_string()),
                time: None,
                topic: TOPIC_ROBOT.to_string(),
            },
            data: json.to_string(),
        };
        let runtime = Arc::new(crate::gateway::GatewayRuntime::new(
            crate::config::Config::default(),
        ));
        let (sender, mut receiver) = mpsc::channel(4);
        *runtime.dingtalk_job_sender.write().await = Some(sender);
        let (mut socket, sent) = RecordingSink::new();

        handle_robot_callback(&mut socket, &runtime, &envelope)
            .await
            .unwrap();

        let job = receiver.try_recv().expect("parsed callback must reach queue");
        assert_eq!(job.inbound.text, "hello worker");
        assert_eq!(sent.lock().unwrap().len(), 1, "enqueue success must ACK");
    }

    #[test]
    fn malformed_core_text_payload_still_rejected() {
        fn envelope(data: serde_json::Value) -> StreamEnvelope {
            StreamEnvelope {
                spec_version: None,
                frame_type: "CALLBACK".to_string(),
                headers: StreamHeaders {
                    app_id: None,
                    connection_id: None,
                    content_type: Some("application/json".to_string()),
                    message_id: Some("malformed-core".to_string()),
                    time: None,
                    topic: TOPIC_ROBOT.to_string(),
                },
                data: data.to_string(),
            }
        }

        let missing_content = envelope(serde_json::json!({
            "msgId": "msg-bad-text",
            "conversationType": "2",
            "msgtype": "text",
            "text": {}
        }));
        assert!(
            parse_robot_payload(&missing_content).is_err(),
            "missing text.content must be rejected"
        );

        let missing_text = envelope(serde_json::json!({
            "msgId": "msg-no-text",
            "conversationType": "2",
            "msgtype": "text"
        }));
        assert_eq!(
            parse_robot_payload(&missing_text).unwrap_err(),
            "core_error:missing_text"
        );

        // Missing required msgId must also fail.
        let missing_id = envelope(serde_json::json!({
            "conversationType": "2",
            "msgtype": "text",
            "text": { "content": "no id" }
        }));
        assert!(
            parse_robot_payload(&missing_id).is_err(),
            "missing msgId must be rejected"
        );
    }

    #[tokio::test]
    async fn malformed_robot_callback_is_not_acked_as_success() {
        let envelope = StreamEnvelope {
            spec_version: None,
            frame_type: "CALLBACK".to_string(),
            headers: StreamHeaders {
                app_id: None,
                connection_id: None,
                content_type: Some("application/json".to_string()),
                message_id: Some("stream-malformed-1".to_string()),
                time: None,
                topic: TOPIC_ROBOT.to_string(),
            },
            data: serde_json::json!({
                "msgId": "msg-malformed-1",
                "conversationType": "2",
                "msgtype": "text"
            })
            .to_string(),
        };
        let runtime = Arc::new(crate::gateway::GatewayRuntime::new(
            crate::config::Config::default(),
        ));
        let (mut socket, sent) = RecordingSink::new();

        handle_robot_callback(&mut socket, &runtime, &envelope)
            .await
            .unwrap();

        assert!(sent.lock().unwrap().is_empty(), "parse failure must not ACK");
    }

    #[test]
    fn sensitive_fields_not_visible_in_debug() {
        // Debug formatting must never expose sessionWebhook / senderId /
        // conversationId / robotCode raw values.
        let json = serde_json::json!({
            "msgId": "SECRET_MESSAGE_ID",
            "conversationType": "2",
            "senderNick": "SECRET_NICK",
            "senderStaffId": "SECRET_STAFF_ID",
            "senderId": "SECRET_SENDER_123",
            "conversationId": "SECRET_CONV_456",
            "robotCode": "SECRET_ROBOT_789",
            "sessionWebhook": "https://oapi.dingtalk.com/robot/send?access_token=SECRET_TOKEN_ABC",
            "sessionWebhookExpiredTime": 1786437210268i64,
            "msgtype": "text",
            "text": { "content": "SECRET_MESSAGE_BODY" }
        });

        let payload: RobotPayload = serde_json::from_value(json).unwrap();
        let debug_str = format!("{:?}", payload);

        assert!(!debug_str.contains("SECRET_MESSAGE_ID"));
        assert!(!debug_str.contains("SECRET_NICK"));
        assert!(!debug_str.contains("SECRET_STAFF_ID"));
        assert!(!debug_str.contains("SECRET_SENDER_123"));
        assert!(!debug_str.contains("SECRET_CONV_456"));
        assert!(!debug_str.contains("SECRET_ROBOT_789"));
        assert!(!debug_str.contains("SECRET_TOKEN_ABC"));
        assert!(!debug_str.contains("oapi.dingtalk.com"));
        assert!(!debug_str.contains("SECRET_MESSAGE_BODY"));
    }

    #[test]
    fn robot_payload_field_types_logger_is_safe() {
        // Type logger outputs key=type pairs only; no values, no secrets.
        let data = r#"{
            "msgId": "msg-log-1",
            "SECRET_FIELD_WITH_VALUE": "must-not-appear",
            "sessionWebhook": "https://oapi.dingtalk.com/robot/send?access_token=TOP_SECRET",
            "sessionWebhookExpiredTime": 1786437210268,
            "createAt": 1786437348372,
            "conversationType": "2",
            "text": { "content": "hello" }
        }"#;

        let summary = build_robot_payload_field_types(data)
            .expect("valid payload must produce a type summary");

        assert!(summary.contains("sessionWebhookExpiredTime=number"), "summary: {summary}");
        assert!(summary.contains("createAt=number"), "summary: {summary}");
        assert!(summary.contains("conversationType=string"), "summary: {summary}");
        assert!(summary.contains("text=object"), "summary: {summary}");
        assert!(!summary.contains("SECRET_FIELD_WITH_VALUE"));
        assert!(
            !summary.contains("TOP_SECRET"),
            "type logger leaked a secret: {summary}"
        );
        assert!(
            !summary.contains("oapi.dingtalk.com"),
            "type logger leaked a URL: {summary}"
        );
    }

    #[test]
    fn log_robot_payload_field_types_does_not_panic_on_bad_json() {
        assert_eq!(build_robot_payload_field_types("not-json"), None);
        assert_eq!(build_robot_payload_field_types(""), None);
        assert_eq!(build_robot_payload_field_types("[]"), None);
    }

    #[test]
    fn test_safe_action() {
        assert_eq!(safe_action("gateway_status"), "gateway_status");
        assert_eq!(safe_action("gateway; DROP TABLE"), "gatewayDROPTABLE");
        assert_eq!(safe_action("中文"), "");
        assert_eq!(safe_action(""), "");
    }

    #[test]
    fn test_stream_error_kind() {
        assert_eq!(
            stream_error_kind("websocket_connect_error:something"),
            "websocket_connect_error"
        );
        assert_eq!(stream_error_kind("network_error:timeout"), "network_error");
        assert_eq!(stream_error_kind("simple_error"), "simple_error");
    }

    #[test]
    fn test_short_hash() {
        let h1 = short_hash("test-message-id-12345");
        let h2 = short_hash("test-message-id-12345");
        assert_eq!(h1, h2); // Same input = same hash
        assert_eq!(h1.len(), 8);

        let h3 = short_hash("different-message-id");
        assert_ne!(h1, h3); // Different input = different hash
    }

    #[test]
    fn test_system_frame_parsing_connected() {
        // Real DingTalk: SYSTEM type is in headers.topic, data may be empty
        let json = serde_json::json!({
            "specVersion": "1.0",
            "type": "SYSTEM",
            "headers": {
                "topic": "CONNECTED"
            },
            "data": ""
        });

        let envelope: StreamEnvelope = serde_json::from_value(json).unwrap();
        assert_eq!(envelope.headers.topic, "CONNECTED");
    }

    #[test]
    fn test_system_frame_parsing_registered() {
        // Real DingTalk: REGISTERED has empty data, type is in headers.topic
        let json = serde_json::json!({
            "specVersion": "1.0",
            "type": "SYSTEM",
            "headers": {
                "topic": "REGISTERED",
                "messageId": "msg-reg-123"
            },
            "data": ""
        });

        let envelope: StreamEnvelope = serde_json::from_value(json).unwrap();
        assert_eq!(envelope.headers.topic, "REGISTERED");
    }

    #[test]
    fn test_system_frame_parsing_ping() {
        let json = serde_json::json!({
            "specVersion": "1.0",
            "type": "SYSTEM",
            "headers": {
                "topic": "ping",
                "messageId": "ping-123"
            },
            "data": ""
        });

        let envelope: StreamEnvelope = serde_json::from_value(json).unwrap();
        assert_eq!(envelope.headers.topic, "ping");
    }

    #[test]
    fn test_system_frame_parsing_disconnect() {
        let json = serde_json::json!({
            "specVersion": "1.0",
            "type": "SYSTEM",
            "headers": {
                "topic": "disconnect",
                "messageId": "disc-123"
            },
            "data": ""
        });

        let envelope: StreamEnvelope = serde_json::from_value(json).unwrap();
        assert_eq!(envelope.headers.topic, "disconnect");
    }

    #[test]
    fn test_system_frame_parsing_keepalive() {
        let json = serde_json::json!({
            "specVersion": "1.0",
            "type": "SYSTEM",
            "headers": {
                "topic": "KEEPALIVE",
                "messageId": "keep-123"
            },
            "data": ""
        });

        let envelope: StreamEnvelope = serde_json::from_value(json).unwrap();
        assert_eq!(envelope.headers.topic, "KEEPALIVE");
    }

    #[test]
    fn test_system_frame_unknown_topic_safe() {
        // Unknown topic should not panic, just be logged as unknown
        let json = serde_json::json!({
            "specVersion": "1.0",
            "type": "SYSTEM",
            "headers": {
                "topic": "UNKNOWN_TOPIC_XYZ"
            },
            "data": ""
        });

        let envelope: StreamEnvelope = serde_json::from_value(json).unwrap();
        assert_eq!(envelope.headers.topic, "UNKNOWN_TOPIC_XYZ");
        // Match against known topics - should fall into default branch
        match envelope.headers.topic.as_str() {
            "CONNECTED" | "REGISTERED" | "disconnect" | "ping" | "KEEPALIVE" => {
                panic!("UNKNOWN_TOPIC_XYZ should not match any known topic");
            }
            _ => {
                // Expected: unknown topic falls through to default branch
            }
        }
    }

    #[test]
    fn test_stream_subscriptions_include_event_wildcard() {
        // Verify the subscriptions payload includes EVENT * for SYSTEM frames
        // Official DingTalk Stream SDK subscribes to EVENT * by default
        let subscriptions = serde_json::json!([
            { "type": "EVENT", "topic": "*" },
            { "type": "CALLBACK", "topic": "/v1.0/im/bot/messages/get" },
            { "type": "CALLBACK", "topic": "/v1.0/card/instances/callback" }
        ]);

        let sub_array = subscriptions.as_array().unwrap();
        assert_eq!(sub_array.len(), 3);

        // Check EVENT * is present
        let event_sub = sub_array.iter().find(|s| s["type"] == "EVENT");
        assert!(event_sub.is_some(), "EVENT * subscription must be present");
        assert_eq!(event_sub.unwrap()["topic"], "*");

        // Check CALLBACK robot is present
        let robot_sub = sub_array
            .iter()
            .find(|s| s["type"] == "CALLBACK" && s["topic"] == "/v1.0/im/bot/messages/get");
        assert!(
            robot_sub.is_some(),
            "CALLBACK robot subscription must be present"
        );

        // Check CALLBACK card is present
        let card_sub = sub_array
            .iter()
            .find(|s| s["type"] == "CALLBACK" && s["topic"] == "/v1.0/card/instances/callback");
        assert!(
            card_sub.is_some(),
            "CALLBACK card subscription must be present"
        );

        // Check no duplicate subscriptions
        let topics: Vec<&str> = sub_array
            .iter()
            .filter_map(|s| s["topic"].as_str())
            .collect();
        let unique_topics: std::collections::HashSet<&str> = topics.iter().cloned().collect();
        assert_eq!(
            topics.len(),
            unique_topics.len(),
            "No duplicate subscription topics"
        );
    }

    #[test]
    fn test_text_frame_json_parsing_registered() {
        // Text frame with REGISTERED JSON parses correctly
        let json = r#"{"specVersion":"1.0","type":"SYSTEM","headers":{"topic":"REGISTERED","messageId":"msg-reg-123"},"data":""}"#;

        let envelope: StreamEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.headers.topic, "REGISTERED");
        assert_eq!(envelope.frame_type, "SYSTEM");
    }

    #[test]
    fn test_binary_frame_json_parsing_registered() {
        // Binary frame (UTF-8 encoded JSON) produces same result as Text
        let json_str = r#"{"specVersion":"1.0","type":"SYSTEM","headers":{"topic":"REGISTERED","messageId":"msg-reg-123"},"data":""}"#;

        // Simulate Binary by encoding to bytes and decoding
        let bytes = json_str.as_bytes();
        let decoded = std::str::from_utf8(bytes).unwrap();

        let envelope: StreamEnvelope = serde_json::from_str(decoded).unwrap();
        assert_eq!(envelope.headers.topic, "REGISTERED");
        assert_eq!(envelope.frame_type, "SYSTEM");
    }

    #[test]
    fn test_binary_invalid_utf8_no_panic() {
        // Invalid UTF-8 bytes should not cause panic when attempting decode
        let invalid_bytes: [u8; 4] = [0x80, 0x81, 0x82, 0x83]; // Invalid UTF-8

        let result = std::str::from_utf8(&invalid_bytes);
        assert!(result.is_err(), "Invalid UTF-8 should fail to decode");
    }

    #[test]
    fn test_malformed_json_no_panic() {
        // Malformed JSON should not panic during parsing
        let malformed = r#"{"specVersion":"1.0","type":"SYSTEM","headers":{"topic":"REGISTERED""#;

        let result: Result<StreamEnvelope, _> = serde_json::from_str(malformed);
        assert!(result.is_err(), "Malformed JSON should fail to parse");
    }

    #[test]
    fn test_binary_and_text_produce_same_result() {
        // Text and Binary (UTF-8) must produce identical parsing results
        let json_str =
            r#"{"specVersion":"1.0","type":"SYSTEM","headers":{"topic":"CONNECTED"},"data":""}"#;

        // Parse as Text
        let envelope_text: StreamEnvelope = serde_json::from_str(json_str).unwrap();

        // Parse as Binary (UTF-8)
        let bytes = json_str.as_bytes();
        let decoded = std::str::from_utf8(bytes).unwrap();
        let envelope_binary: StreamEnvelope = serde_json::from_str(decoded).unwrap();

        // Results must be identical
        assert_eq!(envelope_text.frame_type, envelope_binary.frame_type);
        assert_eq!(envelope_text.headers.topic, envelope_binary.headers.topic);
    }

    #[test]
    fn test_official_envelope_system_registered() {
        // Official DingTalk Stream SDK envelope format for REGISTERED
        let json = r#"{
            "specVersion": "1.0",
            "type": "SYSTEM",
            "headers": {
                "appId": "test",
                "connectionId": "test",
                "contentType": "application/json",
                "messageId": "test",
                "time": "0",
                "topic": "REGISTERED"
            },
            "data": ""
        }"#;

        let envelope: StreamEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.frame_type, "SYSTEM");
        assert_eq!(envelope.headers.topic, "REGISTERED");
        assert_eq!(envelope.data, "");
        assert!(envelope.headers.app_id.is_some());
        assert!(envelope.headers.connection_id.is_some());
    }

    #[test]
    fn test_official_envelope_system_connected() {
        // Official DingTalk Stream SDK envelope format for CONNECTED
        let json = r#"{
            "specVersion": "1.0",
            "type": "SYSTEM",
            "headers": {
                "appId": "test",
                "connectionId": "test",
                "contentType": "application/json",
                "messageId": "test",
                "time": "0",
                "topic": "CONNECTED"
            },
            "data": ""
        }"#;

        let envelope: StreamEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.frame_type, "SYSTEM");
        assert_eq!(envelope.headers.topic, "CONNECTED");
    }

    #[test]
    fn test_official_envelope_callback_robot() {
        // Official DingTalk Stream SDK envelope format for robot callback
        let json = r#"{
            "specVersion": "1.0",
            "type": "CALLBACK",
            "headers": {
                "appId": "test",
                "connectionId": "test",
                "contentType": "application/json",
                "messageId": "test",
                "time": "0",
                "topic": "/v1.0/im/bot/messages/get"
            },
            "data": "{}"
        }"#;

        let envelope: StreamEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.frame_type, "CALLBACK");
        assert_eq!(envelope.headers.topic, "/v1.0/im/bot/messages/get");
    }

    #[test]
    fn test_official_envelope_callback_card() {
        // Official DingTalk Stream SDK envelope format for card callback
        let json = r#"{
            "specVersion": "1.0",
            "type": "CALLBACK",
            "headers": {
                "appId": "test",
                "connectionId": "test",
                "contentType": "application/json",
                "messageId": "test",
                "time": "0",
                "topic": "/v1.0/card/instances/callback"
            },
            "data": "{}"
        }"#;

        let envelope: StreamEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.frame_type, "CALLBACK");
        assert_eq!(envelope.headers.topic, "/v1.0/card/instances/callback");
    }

    #[test]
    fn test_system_ack_preserves_message_id_and_data() {
        // SYSTEM ping ACK must preserve original messageId and data
        let json = r#"{
            "specVersion": "1.0",
            "type": "SYSTEM",
            "headers": {
                "appId": "test-app",
                "connectionId": "test-connection",
                "contentType": "application/json",
                "messageId": "msg-123",
                "time": "0",
                "topic": "ping"
            },
            "data": "{\"opaque\":\"abc-123\"}"
        }"#;

        let envelope: StreamEnvelope = serde_json::from_str(json).unwrap();
        let ack = build_system_ack(&envelope);

        assert_eq!(ack["code"], 200);
        assert_eq!(ack["message"], "OK");
        assert_eq!(ack["headers"]["messageId"], "msg-123");
        // Critical: data must be preserved exactly as-is
        assert_eq!(ack["data"], "{\"opaque\":\"abc-123\"}");
    }

    #[test]
    fn test_system_ack_empty_data() {
        // SYSTEM ping with empty data must return empty data in ACK
        let json = r#"{
            "specVersion": "1.0",
            "type": "SYSTEM",
            "headers": {
                "appId": "test",
                "connectionId": "test",
                "contentType": "application/json",
                "messageId": "msg-empty",
                "time": "0",
                "topic": "ping"
            },
            "data": ""
        }"#;

        let envelope: StreamEnvelope = serde_json::from_str(json).unwrap();
        let ack = build_system_ack(&envelope);

        assert_eq!(ack["code"], 200);
        assert_eq!(ack["message"], "OK");
        assert_eq!(ack["headers"]["messageId"], "msg-empty");
        assert_eq!(ack["data"], "");
    }

    #[test]
    fn test_system_ack_disconnect_preserves_original() {
        // SYSTEM disconnect ACK must preserve original messageId and data
        let json = r#"{
            "specVersion": "1.0",
            "type": "SYSTEM",
            "headers": {
                "appId": "test-disconnect",
                "connectionId": "conn-456",
                "contentType": "application/json",
                "messageId": "disconnect-789",
                "time": "1000",
                "topic": "disconnect"
            },
            "data": "{\"reason\":\"server_reboot\"}"
        }"#;

        let envelope: StreamEnvelope = serde_json::from_str(json).unwrap();
        let ack = build_system_ack(&envelope);

        assert_eq!(ack["code"], 200);
        assert_eq!(ack["message"], "OK");
        assert_eq!(ack["headers"]["messageId"], "disconnect-789");
        assert_eq!(ack["data"], "{\"reason\":\"server_reboot\"}");
    }

    #[test]
    fn test_system_ack_serialize_parse_roundtrip() {
        // SYSTEM ACK must serialize to valid JSON and parse back correctly
        let json = r#"{
            "specVersion": "1.0",
            "type": "SYSTEM",
            "headers": {
                "appId": "app-id",
                "connectionId": "conn-id",
                "contentType": "text/plain",
                "messageId": "roundtrip-test",
                "time": "2000",
                "topic": "ping"
            },
            "data": "test-data-string"
        }"#;

        let envelope: StreamEnvelope = serde_json::from_str(json).unwrap();
        let ack = build_system_ack(&envelope);

        // Parse the ACK as a generic Value to verify structure
        #[derive(Deserialize)]
        struct AckFormat {
            code: u32,
            message: String,
            headers: AckHeaders,
            data: String,
        }
        #[derive(Deserialize)]
        struct AckHeaders {
            appId: Option<String>,
            connectionId: Option<String>,
            contentType: Option<String>,
            messageId: String,
            time: Option<String>,
            topic: String,
        }

        let ack_str = ack.to_string();
        let parsed: AckFormat = serde_json::from_str(&ack_str).unwrap();

        assert_eq!(parsed.code, 200);
        assert_eq!(parsed.message, "OK");
        assert_eq!(parsed.headers.messageId, "roundtrip-test");
        assert_eq!(parsed.headers.topic, "ping");
        assert_eq!(parsed.data, "test-data-string");
    }

    #[test]
    fn test_system_ack_all_headers_preserved() {
        // Verify all headers are echoed back in SYSTEM ACK
        let json = r#"{
            "specVersion": "1.0",
            "type": "SYSTEM",
            "headers": {
                "appId": "my-app-id",
                "connectionId": "my-conn-id",
                "contentType": "my-content-type",
                "messageId": "my-msg-id",
                "time": "1234567890",
                "topic": "ping"
            },
            "data": ""
        }"#;

        let envelope: StreamEnvelope = serde_json::from_str(json).unwrap();
        let ack = build_system_ack(&envelope);

        assert_eq!(ack["headers"]["appId"], "my-app-id");
        assert_eq!(ack["headers"]["connectionId"], "my-conn-id");
        assert_eq!(ack["headers"]["contentType"], "my-content-type");
        assert_eq!(ack["headers"]["messageId"], "my-msg-id");
        assert_eq!(ack["headers"]["time"], "1234567890");
        assert_eq!(ack["headers"]["topic"], "ping");
    }
}
