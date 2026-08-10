//! DingTalk Stream client for advanced-card callbacks.
//!
//! The callback frame is acknowledged before any business work is spawned.
//! Phase 1 deliberately allows only `gateway_status`; all other menu actions
//! are rejected without invoking the Agent, tools, shell, or monitor runner.

use crate::config::schema::DingtalkTransportMode;
use crate::gateway::dingtalk_card;
use crate::gateway::GatewayRuntime;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;

const DINGTALK_STREAM_GATEWAY_URL: &str = "https://api.dingtalk.com/v1.0/gateway/connections/open";
pub const DINGTALK_CARD_CALLBACK_TOPIC: &str = "/v1.0/card/instances/callback";

pub struct DingtalkCardStreamGuard {
    shutdown: Option<watch::Sender<bool>>,
    runtime: Arc<GatewayRuntime>,
}

impl Drop for DingtalkCardStreamGuard {
    fn drop(&mut self) {
        self.runtime.set_dingtalk_stream_connected(false);
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(true);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCardCallback {
    pub out_track_id: String,
    pub action: String,
    pub user_id: Option<String>,
    pub space_id: Option<String>,
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
    tokio::spawn(run_reconnecting(
        runtime.clone(),
        app_key,
        app_secret,
        shutdown_rx,
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
) {
    let mut attempt = 0u32;
    loop {
        if *shutdown.borrow() {
            break;
        }
        match connect_once(runtime.clone(), &app_key, &app_secret, shutdown.clone()).await {
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
        runtime.set_dingtalk_stream_connected(false);
        let delay = reconnect_delay(attempt);
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() { break; }
            }
            _ = tokio::time::sleep(delay) => {}
        }
    }
    runtime.set_dingtalk_stream_connected(false);
    println!("[dingtalk-card-stream] stopped=true");
}

async fn connect_once(
    runtime: Arc<GatewayRuntime>,
    app_key: &str,
    app_secret: &str,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    let (endpoint, ticket) = request_stream_connection(app_key, app_secret).await?;
    let mut url =
        reqwest::Url::parse(&endpoint).map_err(|_| "invalid_stream_endpoint".to_string())?;
    url.query_pairs_mut().append_pair("ticket", &ticket);
    let (mut socket, _) = tokio_tungstenite::connect_async(url.as_str())
        .await
        .map_err(|_| "stream_connect_error".to_string())?;
    runtime.set_dingtalk_stream_connected(true);
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
                        handle_text_frame(&mut socket, runtime.clone(), text.as_ref()).await?;
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

    // ACK first. The callback task cannot block DingTalk's delivery loop.
    let ack = build_stream_ack(&envelope, serde_json::json!({ "response": {} }));
    socket
        .send(Message::Text(ack.to_string().into()))
        .await
        .map_err(|_| "stream_ack_error".to_string())?;
    println!("[dingtalk-card-stream] callback_ack=true");

    match parse_card_callback_envelope(&envelope) {
        Ok(callback) if is_allowed_phase1_action(&callback.action) => {
            println!("[dingtalk-card-stream] callback_received action=gateway_status");
            tokio::spawn(process_gateway_status(runtime, callback.out_track_id));
        }
        Ok(callback) => {
            println!(
                "[dingtalk-card-stream] action_rejected action={} reason=phase1_allowlist",
                safe_action(&callback.action)
            );
        }
        Err(error) => {
            println!(
                "[dingtalk-card-stream] callback_rejected reason={}",
                safe_error_kind(&error)
            );
        }
    }
    Ok(())
}

async fn process_gateway_status(runtime: Arc<GatewayRuntime>, out_track_id: String) {
    println!("[dingtalk-card-action] action=gateway_status status=started");
    let config = runtime.get_config().await;
    let entry = config.channels_config.dingtalk.as_ref();
    let app_key = crate::gateway::resolve_dingtalk_app_key_for_worker(&config, entry);
    let app_secret = crate::gateway::resolve_dingtalk_secret_for_worker(&config, entry);
    let (Some(app_key), Some(app_secret)) = (app_key, app_secret) else {
        println!(
            "[dingtalk-card-stream] action_failed action=gateway_status reason=missing_credentials"
        );
        return;
    };
    let token =
        match crate::gateway::dingtalk_worker::fetch_dingtalk_access_token(&app_key, &app_secret)
            .await
        {
            Ok(token) => token,
            Err(error) => {
                println!(
                    "[dingtalk-card-stream] action_failed action=gateway_status reason={}",
                    safe_error_kind(&error)
                );
                return;
            }
        };
    if let Err(error) = dingtalk_card::update_card(
        &token,
        &out_track_id,
        "RUNNING",
        "正在读取 Gateway 状态",
        "",
        "gateway_status",
    )
    .await
    {
        println!(
            "[dingtalk-card-stream] action_failed action=gateway_status stage=running_update reason={}",
            safe_error_kind(&error)
        );
        return;
    }

    let status =
        crate::gateway::dingtalk_worker::build_runtime_dingtalk_status_text(&runtime).await;
    match dingtalk_card::update_card(
        &token,
        &out_track_id,
        "SUCCESS",
        "Gateway 状态读取完成",
        &status,
        "gateway_status",
    )
    .await
    {
        Ok(()) => {
            println!("[dingtalk-card-action] action=gateway_status status=success");
            println!("[dingtalk-card-stream] action_completed action=gateway_status");
        }
        Err(error) => {
            println!(
                "[dingtalk-card-stream] action_failed action=gateway_status stage=success_update reason={}",
                safe_error_kind(&error)
            );
            let _ = dingtalk_card::update_card(
                &token,
                &out_track_id,
                "FAILED",
                "Gateway 状态更新失败",
                "请稍后重试",
                "gateway_status",
            )
            .await;
        }
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
    Ok(ParsedCardCallback {
        out_track_id,
        action,
        user_id,
        space_id,
    })
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

pub fn is_allowed_phase1_action(action: &str) -> bool {
    crate::gateway::agent_menu::canonical_agent_menu_action(action) == Some("gateway_status")
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
    fn phase1_allowlist_rejects_every_non_status_action() {
        assert!(is_allowed_phase1_action("gateway_status"));
        for action in [
            "monitor_30s",
            "monitor_60s",
            "recent_jobs",
            "help",
            "unknown",
        ] {
            assert!(!is_allowed_phase1_action(action));
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
}
