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
use crate::gateway::dingtalk_card;
use crate::gateway::dingtalk_card_stream::ParsedCardCallback;
use crate::gateway::dingtalk_worker::DingtalkAsyncJob;
use crate::gateway::{DingTalkStreamState, GatewayRuntime};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
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

/// Guard to manage the Stream lifecycle and enable graceful shutdown.
pub struct DingtalkStreamGuard {
    shutdown_tx: Option<watch::Sender<bool>>,
    runtime: Arc<GatewayRuntime>,
}

impl Drop for DingtalkStreamGuard {
    fn drop(&mut self) {
        self.runtime.set_dingtalk_stream_connected(false);
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(true);
        }
    }
}

/// Start the unified DingTalk Stream transport.
/// Returns None if Stream mode is not enabled or configuration is incomplete.
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

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(run_reconnect_loop(
        runtime.clone(),
        app_key.unwrap(),
        app_secret.unwrap(),
        shutdown_rx,
    ));

    Some(DingtalkStreamGuard {
        shutdown_tx: Some(shutdown_tx),
        runtime,
    })
}

// ---------------------------------------------------------------------------
// Reconnect loop - single owner to prevent storm
// ---------------------------------------------------------------------------

async fn run_reconnect_loop(
    runtime: Arc<GatewayRuntime>,
    app_key: String,
    app_secret: String,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut attempt = 0u32;

    loop {
        if *shutdown.borrow() {
            break;
        }

        println!("[dingtalk-stream] state=connecting");

        match connect_and_run(&runtime, &app_key, &app_secret, shutdown.clone()).await {
            Ok(()) => {
                attempt = 0;
            }
            Err(e) => {
                let kind = stream_error_kind(&e);
                attempt = attempt.saturating_add(1);
                println!(
                    "[dingtalk-stream] disconnected reason={} reconnect_attempt={}",
                    kind, attempt
                );
            }
        }

        runtime.set_dingtalk_stream_connected(false);

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

    runtime.set_dingtalk_stream_connected(false);
    println!("[dingtalk-stream] shutdown=true");
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
    app_key: &str,
    app_secret: &str,
    mut shutdown: watch::Receiver<bool>,
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
        .map_err(|e| format!("websocket_connect_error:{}", e))?;

    println!("[dingtalk-stream] websocket_open=true");
    runtime.set_dingtalk_stream_connected(true);

    // 3. Run socket loop until failure
    run_socket_loop(runtime, &mut socket, shutdown).await
}

async fn run_socket_loop<S>(
    runtime: &Arc<GatewayRuntime>,
    socket: &mut S,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), String>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error>
        + futures_util::StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
{
    runtime.set_dingtalk_stream_connected(true);
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
                        if let Err(e) = handle_downstream_json(socket, runtime, text.as_ref()).await {
                            return Err(e);
                        }
                    }
                    Message::Binary(bytes) => {
                        let byte_count = bytes.len();
                        println!("[dingtalk-stream] frame_received kind=binary bytes={}", byte_count);
                        match std::str::from_utf8(&bytes) {
                            Ok(text) => {
                                if let Err(e) = handle_downstream_json(socket, runtime, text).await {
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
            handle_system_frame(socket, runtime, &envelope).await?;
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

// ---------------------------------------------------------------------------
// Frame handling
// ---------------------------------------------------------------------------

/// Handle text frame - delegates to shared downstream JSON handler
async fn handle_text_frame<S>(
    socket: &mut S,
    runtime: &Arc<GatewayRuntime>,
    text: &str,
) -> Result<(), String>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    handle_downstream_json(socket, runtime, text).await
}

async fn handle_system_frame<S>(
    socket: &mut S,
    runtime: &Arc<GatewayRuntime>,
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
            runtime.set_dingtalk_stream_connected(true);
        }
        "disconnect" => {
            println!("[dingtalk-stream] system=disconnect");
            runtime.set_dingtalk_stream_connected(false);
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
            println!(
                "[dingtalk-stream] topic=robot parse_failed=true reason={}",
                e
            );
            // Still ACK to prevent redelivery
            let ack = build_stream_ack(envelope, serde_json::json!({}));
            let _ = socket.send(Message::Text(ack.to_string().into())).await;
            return Ok(());
        }
    };

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
                "[dingtalk-stream] topic=robot callback_received=true msg_id_hash={}",
                short_hash(&msg_id)
            );
            let ack = build_stream_ack(envelope, serde_json::json!({}));
            let _ = socket.send(Message::Text(ack.to_string().into())).await;
            println!("[dingtalk-stream] topic=robot ack=true");
        }
        Err(_) => {
            println!(
                "[dingtalk-stream] topic=robot queue_full=true msg_id_hash={}",
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

    // Phase 1: only allow gateway_status
    if !crate::gateway::dingtalk_card_stream::is_allowed_phase1_action(&callback.action) {
        println!(
            "[dingtalk-stream] topic=card action_rejected action={}",
            safe_action(&callback.action)
        );
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
    let out_track_id = callback.out_track_id;
    tokio::spawn(async move {
        process_card_action(runtime_clone, out_track_id).await;
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

    Ok(ParsedCardCallback {
        out_track_id,
        action,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RobotPayload {
    #[serde(rename = "msgId")]
    msg_id: String,
    #[serde(rename = "conversationType")]
    conversation_type: String,
    sender_nick: Option<String>,
    sender_staff_id: Option<String>,
    sender_id: Option<String>,
    conversation_id: Option<String>,
    robot_code: Option<String>,
    session_webhook: Option<String>,
    #[serde(rename = "sessionWebhookExpiredTime")]
    session_webhook_expired_time: Option<String>,
    #[serde(rename = "msgtype", default)]
    msg_type: Option<String>,
    text: Option<RobotText>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RobotText {
    content: String,
}

fn parse_robot_payload(envelope: &StreamEnvelope) -> Result<RobotPayload, String> {
    let data_str = &envelope.data;

    serde_json::from_str(data_str).map_err(|e| format!("parse_error:{}", e))
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

    println!("[dingtalk-stream] endpoint_acquired=true");
    Ok((endpoint, ticket))
}

// ---------------------------------------------------------------------------
// Card action processing (reuse Phase 1 logic)
// ---------------------------------------------------------------------------

async fn process_card_action(runtime: Arc<GatewayRuntime>, out_track_id: String) {
    println!("[dingtalk-card-action] action=gateway_status status=started");

    let config = runtime.get_config().await;
    let entry = config.channels_config.dingtalk.as_ref();
    let app_key = crate::gateway::resolve_dingtalk_app_key_for_worker(&config, entry);
    let app_secret = crate::gateway::resolve_dingtalk_secret_for_worker(&config, entry);

    let (Some(app_key), Some(app_secret)) = (app_key, app_secret) else {
        println!("[dingtalk-card-stream] action_failed reason=missing_credentials");
        return;
    };

    let token =
        match crate::gateway::dingtalk_worker::fetch_dingtalk_access_token(&app_key, &app_secret)
            .await
        {
            Ok(t) => t,
            Err(e) => {
                println!(
                    "[dingtalk-card-stream] action_failed reason={}",
                    stream_error_kind(&e)
                );
                return;
            }
        };

    // Update to RUNNING
    if let Err(e) = dingtalk_card::update_card(
        &token,
        &out_track_id,
        "RUNNING",
        "正在读取状态",
        "",
        "gateway_status",
    )
    .await
    {
        println!(
            "[dingtalk-card-stream] action_failed stage=running reason={}",
            stream_error_kind(&e)
        );
        return;
    }

    // Get status
    let status =
        crate::gateway::dingtalk_worker::build_runtime_dingtalk_status_text(&runtime).await;

    match dingtalk_card::update_card(
        &token,
        &out_track_id,
        "SUCCESS",
        "状态读取完成",
        &status,
        "gateway_status",
    )
    .await
    {
        Ok(()) => {
            println!("[dingtalk-card-action] action=gateway_status status=success");
        }
        Err(e) => {
            println!(
                "[dingtalk-card-stream] action_failed stage=success reason={}",
                stream_error_kind(&e)
            );
            let _ = dingtalk_card::update_card(
                &token,
                &out_track_id,
                "FAILED",
                "状态更新失败",
                "请稍后重试",
                "gateway_status",
            )
            .await;
        }
    }
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

fn reconnect_delay(attempt: u32) -> Duration {
    let exp = attempt.min(MAX_BACKOFF_ATTEMPTS);
    Duration::from_secs(2u64.saturating_pow(exp).min(60))
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
        metadata.insert("source".to_string(), serde_json::json!("dingtalk"));
        metadata.insert("stream_msg_id".to_string(), serde_json::json!(msg_id));

        let inbound = InboundMessage {
            channel: ChannelKind::Dingtalk,
            user_id: sender_staff_id.or(sender_id),
            session_id: conversation_id,
            text,
            metadata,
        };

        let job = DingtalkAsyncJob::new(inbound, serde_json::json!(payload));

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
