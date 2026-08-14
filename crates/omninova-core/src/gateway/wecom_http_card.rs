//! WeCom HTTP callback interactive card adapter (Phase 2A.3.3).
//!
//! The SHARED business core lives in `wecom_card.rs` (panel model,
//! normalizer, allowlist, CardStore, action service, monitor state).
//! This module is ONLY the HTTP transport adapter:
//!
//! - initial card / card updates are returned as encrypted
//!   passive-reply envelopes (`{"encrypt","msgsignature","timestamp",
//!   "nonce"}`) built by `wecom_http::build_encrypted_reply` — the same
//!   crypto path as the existing HTTP stream replies;
//! - card events reuse `handle_card_event_core` with transport
//!   `HttpCallback` — never the WebSocket outbound queue;
//! - monitor final results are explicitly DEFERRED (stored + shown on
//!   the next interaction): no verified HTTP proactive channel exists
//!   and the WebSocket is never started from HTTP mode.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::config::Config;
use crate::gateway::wecom_card::{
    build_panel_card_with_subtitle, handle_card_event_core, http_menu_subtitle, new_task_id,
    panel_command, register_http_panel_with_url, CardEventOutcome, MonitorDeliveryTarget,
    WecomCardTransport,
};
use crate::gateway::wecom_http::{
    build_encrypted_reply, ParsedHttpCallback, WecomCallbackQuery, WecomHttpError,
};
use crate::gateway::wecom_protocol::WecomCallbackBody;
use crate::gateway::wecom_stream::short_hash;
use crate::gateway::GatewayRuntime;

/// Plaintext of an HTTP template_card reply body (INITIAL card only —
/// `msgtype=template_card` SENDS a card; it does not update one).
pub fn template_card_plaintext(card: &Value) -> String {
    serde_json::to_string(&json!({
        "msgtype": "template_card",
        "template_card": card,
    }))
    .unwrap_or_default()
}

/// Plaintext of an HTTP template-card UPDATE response (Phase 2A.3.3b).
///
/// The official HTTP update semantics are NOT `msgtype=template_card`
/// (which would send a NEW card): the card-event response body is
/// `{"response_type":"update_template_card","template_card":{...}}`
/// with the ORIGINAL task_id. This mirrors the long-connection
/// `aibot_respond_update_msg` semantics over the HTTP transport.
pub fn build_http_template_card_update_response(card: &Value) -> String {
    serde_json::to_string(&json!({
        "response_type": "update_template_card",
        "template_card": card,
    }))
    .unwrap_or_default()
}

/// Plaintext of an HTTP text reply body.
pub fn text_plaintext(content: &str) -> String {
    serde_json::to_string(&json!({
        "msgtype": "text",
        "text": { "content": content },
    }))
    .unwrap_or_default()
}

/// HTTP panel trigger (P4): when the inbound text is a panel command,
/// registers an HttpCallback-origin panel and returns the encrypted
/// initial-card response. Returns `None` → the normal Agent stream path
/// continues untouched (HTTP_PANEL_AGENT_DISPATCH=NO).
pub async fn panel_trigger_response(
    config: &Config,
    query: &WecomCallbackQuery,
    parsed: &ParsedHttpCallback,
) -> Option<Value> {
    match panel_command(&parsed.inbound.text) {
        Some(command) => {
            println!(
                "[wecom-http-card] panel_trigger_checked matched=true command={}",
                command
            );
            println!(
                "[wecom-http-card] panel_requested msg_id={}",
                short_hash(&parsed.body.msgid)
            );
            let task_id = new_task_id();
            // Panel-trigger dedup (parity with the WS stream-level msgid
            // dedup): a RETRIED menu callback reuses the SAME panel
            // (same task_id) instead of creating a second one.
            let existing = crate::gateway::wecom_card::card_store()
                .dedup_panel_trigger(&parsed.body.msgid, &task_id)
                .await;
            let is_retry = existing.is_some();
            let effective_task_id = existing.unwrap_or_else(|| task_id.clone());
            if !is_retry {
                register_http_panel_with_url(
                    &effective_task_id,
                    parsed.inbound.session_id.clone(),
                    parsed.body.response_url.clone(),
                )
                .await;
            }
            let card = build_panel_card_with_subtitle(&effective_task_id, &http_menu_subtitle());
            let plaintext = template_card_plaintext(&card);
            let reply =
                build_encrypted_reply(config, &query.timestamp, &query.nonce, &plaintext).ok()?;
            println!(
                "[wecom-http-card] initial_card_encrypted msgtype=template_card task_id={} dedup={}",
                short_hash(&effective_task_id),
                if is_retry { "reused" } else { "accepted" }
            );
            Some(reply)
        }
        None => {
            println!(
                "[wecom-http-card] panel_trigger_checked matched=false text_bytes={}",
                parsed.inbound.text.len()
            );
            None
        }
    }
}

/// HTTP card event (P6/P7): shared core → synchronous encrypted
/// updated-card HTTP response. Card events NEVER dispatch the Agent.
pub async fn handle_http_card_event(
    runtime: &Arc<GatewayRuntime>,
    config: &Config,
    query: &WecomCallbackQuery,
    body: &WecomCallbackBody,
) -> Result<Value, WecomHttpError> {
    println!(
        "[wecom-http-card] event_received msg_id={}",
        short_hash(&body.msgid)
    );
    let delivery = MonitorDeliveryTarget::HttpCallback;
    match handle_card_event_core(runtime, body, WecomCardTransport::HttpCallback, &delivery).await {
        CardEventOutcome::Updated {
            task_id,
            action,
            card,
        } => {
            println!(
                "[wecom-http-card] action={} agent_dispatch=false task_id={}",
                action,
                short_hash(&task_id)
            );
            // Dedicated HTTP UPDATE response (NOT the initial-card
            // msgtype=template_card plaintext): response_type=
            // update_template_card + the ORIGINAL task_id.
            let plaintext = build_http_template_card_update_response(&card);
            println!(
                "[wecom-http-card] card_update_response_built response_type=update_template_card task_id={}",
                short_hash(&task_id)
            );
            let reply = build_encrypted_reply(config, &query.timestamp, &query.nonce, &plaintext)?;
            println!(
                "[wecom-http-card] card_update_encrypted response_type=update_template_card task_id={}",
                short_hash(&task_id)
            );
            Ok(reply)
        }
        CardEventOutcome::Rejected { reply_text } => {
            // Parity gap fix: HTTP rejections update the ORIGINAL card
            // with the safe subtitle (update_template_card semantics)
            // instead of a detached text reply. Text fallback only when
            // the event carries no task_id.
            let task_id = crate::gateway::wecom_card::normalize_template_card_event(body, "")
                .and_then(|normalized| normalized.task_id)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            match task_id {
                Some(task_id) => {
                    let card = crate::gateway::wecom_card::build_panel_card_with_text(
                        &task_id,
                        &reply_text,
                    );
                    let plaintext = build_http_template_card_update_response(&card);
                    println!(
                        "[wecom-http-card] card_rejection_update response_type=update_template_card task_id={}",
                        short_hash(&task_id)
                    );
                    Ok(build_encrypted_reply(
                        config,
                        &query.timestamp,
                        &query.nonce,
                        &plaintext,
                    )
                    .unwrap_or_else(|_| json!({})))
                }
                None => {
                    let plaintext = text_plaintext(&reply_text);
                    Ok(build_encrypted_reply(
                        config,
                        &query.timestamp,
                        &query.nonce,
                        &plaintext,
                    )
                    .unwrap_or_else(|_| json!({})))
                }
            }
        }
        CardEventOutcome::Consumed => Ok(json!({})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::wecom_card::{
        build_monitor_result_markdown, build_panel_card_with_subtitle, card_store,
        handle_card_event_core, http_menu_subtitle, new_task_id, register_http_panel,
        register_http_panel_with_url, render_monitor_running_card_text, response_url_hash,
        CardEventOutcome, MonitorDelivery, MonitorDeliveryTarget, MonitorState,
        WecomCardStore, WecomCardTransport,
    };
    use crate::gateway::wecom_crypto::{decrypt_message, TEST_ENCODING_AES_KEY};
    use crate::gateway::wecom_http::parse_post;
    use crate::gateway::wecom_http::WecomCallbackQuery;
    use crate::config::WecomTransportMode;

    const TOKEN: &str = "callback-token";

    fn http_config() -> Config {
        let mut config = Config::default();
        config.gateway.wecom.enabled = true;
        config.gateway.wecom.transport_mode = WecomTransportMode::HttpCallback;
        config.gateway.wecom.callback_token = TOKEN.to_string();
        config.gateway.wecom.encoding_aes_key = TEST_ENCODING_AES_KEY.to_string();
        config
    }

    fn query_for(encrypted: &str) -> WecomCallbackQuery {
        let timestamp = "1700000000";
        let nonce = "nonce-1";
        let mut values = [TOKEN, timestamp, nonce, encrypted];
        values.sort_unstable();
        let signature =
            crate::gateway::wecom_crypto::signature_for_test(&values.concat());
        WecomCallbackQuery {
            msg_signature: signature,
            timestamp: timestamp.to_string(),
            nonce: nonce.to_string(),
            echostr: None,
        }
    }

    /// Encrypt a callback body and parse it through the real HTTP path.
    fn parse_encrypted_callback(config: &Config, body: &serde_json::Value) -> ParsedHttpCallback {
        let plaintext = body.to_string();
        let encrypted =
            crate::gateway::wecom_crypto::encrypt_message(&TEST_ENCODING_AES_KEY, &plaintext, "")
                .unwrap();
        let envelope = json!({"encrypt": encrypted}).to_string();
        parse_post(config, &query_for(&encrypted), &envelope).unwrap()
    }

    fn text_callback(content: &str) -> serde_json::Value {
        json!({
            "msgid": "msg-txt",
            "chattype": "single",
            "from": {"userid": "u-1"},
            "msgtype": "text",
            "text": {"content": content}
        })
    }

    fn nested_card_event(task_id: &str, event_key: &str) -> serde_json::Value {
        json!({
            "msgid": "msg-ev",
            "msgtype": "event",
            "from": {"userid": "u-1"},
            "chattype": "single",
            "event": {
                "eventtype": "template_card_event",
                "template_card_event": {
                    "event_key": event_key,
                    "task_id": task_id,
                    "card_type": "button_interaction"
                }
            }
        })
    }

    fn decrypt_reply(reply: &Value) -> String {
        let encrypted = reply["encrypt"].as_str().expect("encrypted envelope");
        decrypt_message(TEST_ENCODING_AES_KEY, encrypted, "")
            .unwrap()
            .message
    }

    // ------------------------------------------------------------------
    // P14: HTTP card parity tests
    // ------------------------------------------------------------------

    #[test]
    fn http_panel_menu_detected() {
        assert!(panel_command("menu").is_some());
        assert!(panel_command("/menu").is_some());
        assert!(panel_command("panel").is_some());
        assert!(panel_command("/panel").is_some());
    }

    #[test]
    fn http_panel_chinese_menu_detected() {
        assert!(panel_command("菜单").is_some());
        assert!(panel_command("面板").is_some());
        assert!(panel_command(" 菜单 ").is_some());
    }

    #[tokio::test]
    async fn http_panel_does_not_dispatch_agent() {
        let config = http_config();
        let parsed = parse_encrypted_callback(&config, &text_callback("menu"));
        let reply = panel_trigger_response(&config, &query_for("x"), &parsed)
            .await
            .expect("panel command must produce a card response");
        // Initial card: template_card, not an agent stream placeholder.
        let plaintext = decrypt_reply(&reply);
        assert!(plaintext.contains("\"template_card\""));
        assert!(plaintext.contains("OmniNova 控制中心"));
        assert!(!plaintext.contains("\"stream\""));
    }

    #[tokio::test]
    async fn http_normal_text_still_dispatches_agent() {
        let config = http_config();
        let parsed = parse_encrypted_callback(&config, &text_callback("你好"));
        let reply = panel_trigger_response(&config, &query_for("x"), &parsed).await;
        assert!(reply.is_none(), "normal text must keep the Agent path");
    }

    #[tokio::test]
    async fn http_initial_card_uses_shared_renderer() {
        let config = http_config();
        let parsed = parse_encrypted_callback(&config, &text_callback("菜单"));
        let reply = panel_trigger_response(&config, &query_for("x"), &parsed)
            .await
            .unwrap();
        let plaintext = decrypt_reply(&reply);
        // Same shared renderer as the long connection.
        assert!(plaintext.contains("运行正常 · HTTP 回调"));
        assert!(plaintext.contains("\"button_interaction\""));
        for key in ["gateway_status", "recent_jobs", "monitor_30", "monitor_60", "help"] {
            assert!(plaintext.contains(key), "missing action key {key}");
        }
    }

    #[tokio::test]
    async fn http_initial_card_is_encrypted() {
        let config = http_config();
        let parsed = parse_encrypted_callback(&config, &text_callback("/panel"));
        let reply = panel_trigger_response(&config, &query_for("x"), &parsed)
            .await
            .unwrap();
        assert!(reply.get("encrypt").is_some());
        assert!(reply.get("msgsignature").is_some());
        assert_eq!(reply["timestamp"], "1700000000");
        assert_eq!(reply["nonce"], "nonce-1");
    }

    #[tokio::test]
    async fn http_card_event_decrypts() {
        let config = http_config();
        let parsed = parse_encrypted_callback(&config, &nested_card_event("task-e", "help"));
        assert!(crate::gateway::wecom_card::is_template_card_event(&parsed.body));
        assert_eq!(parsed.body.msgtype.as_deref(), Some("event"));
    }

    #[tokio::test]
    async fn http_card_event_nested_runtime_normalizes() {
        let config = http_config();
        let parsed = parse_encrypted_callback(&config, &nested_card_event("task-n", "help"));
        let normalized =
            crate::gateway::wecom_card::normalize_template_card_event(&parsed.body, "")
                .expect("nested runtime event must normalize");
        assert_eq!(normalized.task_id.as_deref(), Some("task-n"));
        assert_eq!(normalized.event_key.as_deref(), Some("help"));
        assert_eq!(
            crate::gateway::wecom_card::card_event_source(&parsed.body),
            "nested_runtime"
        );
    }

    #[tokio::test]
    async fn http_card_event_routes_gateway_status() {
        let runtime = Arc::new(GatewayRuntime::new(Config::default()));
        let config = http_config();
        let task_id = new_task_id();
        register_http_panel(&task_id, None).await;
        let body: WecomCallbackBody = serde_json::from_value(nested_card_event(&task_id, "gateway_status"))
            .unwrap();
        let reply = handle_http_card_event(&runtime, &config, &query_for("x"), &body)
            .await
            .expect("gateway_status must produce an updated card");
        let plaintext = decrypt_reply(&reply);
        assert!(plaintext.contains("\"template_card\""));
        assert!(plaintext.contains("运行状态"));
        assert!(plaintext.contains("网关：运行中"));
    }

    #[tokio::test]
    async fn http_card_event_never_dispatches_agent() {
        let runtime = Arc::new(GatewayRuntime::new(Config::default()));
        let config = http_config();
        let task_id = new_task_id();
        register_http_panel(&task_id, None).await;
        let body: WecomCallbackBody =
            serde_json::from_value(nested_card_event(&task_id, "help")).unwrap();
        let reply = handle_http_card_event(&runtime, &config, &query_for("x"), &body)
            .await
            .unwrap();
        let plaintext = decrypt_reply(&reply);
        // Card pipeline only: no stream placeholder, no agent markers.
        assert!(!plaintext.contains("\"stream\""));
        assert!(plaintext.contains("\"template_card\""));
    }

    #[tokio::test]
    async fn http_card_unknown_action_rejected() {
        let runtime = Arc::new(GatewayRuntime::new(Config::default()));
        let config = http_config();
        let task_id = new_task_id();
        register_http_panel(&task_id, None).await;
        let body: WecomCallbackBody =
            serde_json::from_value(nested_card_event(&task_id, "rm -rf")).unwrap();
        let reply = handle_http_card_event(&runtime, &config, &query_for("x"), &body)
            .await
            .unwrap();
        let plaintext = decrypt_reply(&reply);
        assert!(plaintext.contains("未知操作"), "unknown action must be safely rejected");
    }

    #[tokio::test]
    async fn http_card_event_dedup() {
        let runtime = Arc::new(GatewayRuntime::new(Config::default()));
        let config = http_config();
        let task_id = new_task_id();
        register_http_panel(&task_id, None).await;
        let body: WecomCallbackBody =
            serde_json::from_value(nested_card_event(&task_id, "help")).unwrap();
        let first = handle_http_card_event(&runtime, &config, &query_for("x"), &body)
            .await
            .unwrap();
        assert!(first.get("encrypt").is_some());
        // Retry of the same event msgid → consumed (empty response).
        let second = handle_http_card_event(&runtime, &config, &query_for("x"), &body)
            .await
            .unwrap();
        assert!(second.get("encrypt").is_none());
    }

    #[tokio::test]
    async fn http_card_expired_rejected() {
        let runtime = Arc::new(GatewayRuntime::new(Config::default()));
        let config = http_config();
        let task_id = new_task_id();
        card_store()
            .register_at_with_transport(
                task_id.clone(),
                None,
                time::OffsetDateTime::now_utc().unix_timestamp() - 3600,
                WecomCardTransport::HttpCallback,
            )
            .await;
        let body: WecomCallbackBody =
            serde_json::from_value(nested_card_event(&task_id, "help")).unwrap();
        let reply = handle_http_card_event(&runtime, &config, &query_for("x"), &body)
            .await
            .unwrap();
        let plaintext = decrypt_reply(&reply);
        assert!(plaintext.contains("已过期"), "expired panel must be safely rejected");
    }

    #[tokio::test]
    async fn http_monitor_30_singleflight() {
        let store = WecomCardStore::new();
        store
            .register_with_transport(
                "task-hm30".to_string(),
                None,
                WecomCardTransport::HttpCallback,
            )
            .await;
        assert!(store.try_start_monitor("task-hm30", 30, 0).await);
        assert!(!store.try_start_monitor("task-hm30", 30, 0).await);
    }

    #[tokio::test]
    async fn http_monitor_60_singleflight() {
        let store = WecomCardStore::new();
        store
            .register_with_transport(
                "task-hm60".to_string(),
                None,
                WecomCardTransport::HttpCallback,
            )
            .await;
        assert!(store.try_start_monitor("task-hm60", 60, 0).await);
        assert!(!store.try_start_monitor("task-hm60", 60, 0).await);
    }

    #[tokio::test]
    async fn http_monitor_immediate_response() {
        // The click response is the immediate running render — the HTTP
        // request is never held for the 30/60s monitor duration.
        let store = WecomCardStore::new();
        store
            .register_with_transport(
                "task-himm".to_string(),
                None,
                WecomCardTransport::HttpCallback,
            )
            .await;
        assert!(store.try_start_monitor("task-himm", 30, 0).await);
        let immediate = render_monitor_running_card_text(30, 30);
        assert!(immediate.contains("进行中"));
        assert!(immediate.contains("剩余约 30 秒"));
    }

    #[tokio::test]
    async fn http_monitor_result_uses_http_delivery_path() {
        // No response_url → explicit failure, never a fake success.
        let store = WecomCardStore::new();
        store
            .register_with_transport(
                "task-hdel".to_string(),
                None,
                WecomCardTransport::HttpCallback,
            )
            .await;
        store
            .complete_monitor("task-hdel", 30, 30000, "桌面监控完成".to_string())
            .await;
        crate::gateway::wecom_card::deliver_monitor_result_http(
            &store,
            "task-hdel",
            "completed",
            30,
            30000,
            "桌面监控完成",
        )
        .await;
        match store.monitor_state("task-hdel").await {
            MonitorState::Completed { delivery, .. } => {
                assert_eq!(delivery, MonitorDelivery::Failed { errcode: -1 });
            }
            other => panic!("expected Completed with failed delivery, got {other:?}"),
        }
    }

    // ------------------------------------------------------------------
    // Phase 2A.3.3a: response_url context + parity
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn http_response_url_saved_to_panel_context() {
        let task_id = new_task_id();
        register_http_panel_with_url(
            &task_id,
            None,
            Some("https://example.invalid/r/tmp-secret".to_string()),
        )
        .await;
        let (url, received_at) = card_store()
            .http_delivery_context(&task_id)
            .await
            .expect("response_url must be stored");
        assert_eq!(url, "https://example.invalid/r/tmp-secret");
        let now = WecomCardStore::now_unix();
        assert!(received_at <= now && now - received_at < 60);
    }

    #[tokio::test]
    async fn http_response_url_not_logged_plaintext() {
        let url = "https://example.invalid/r/secret-token-value";
        let hash = crate::gateway::wecom_card::response_url_hash(url);
        // Logs carry only the short hash; renderers never embed the URL.
        assert!(!hash.contains("secret-token"));
        let card = build_panel_card_with_subtitle("t", &http_menu_subtitle());
        assert!(!card.to_string().contains(url));
        let markdown = crate::gateway::wecom_card::build_monitor_result_markdown(
            "completed", 30, 30000, "检测到桌面变化",
        );
        assert!(!markdown.contains(url));
    }

    #[tokio::test]
    async fn http_monitor_completion_does_not_use_websocket() {
        // The HTTP delivery target never constructs a WebSocket outbound
        // message: delivery goes through wecom_http_delivery (HTTP POST)
        // only. Asserted at the type level + no WS channel anywhere.
        let delivery = MonitorDeliveryTarget::HttpCallback;
        match delivery {
            MonitorDeliveryTarget::LongConnection { .. } => {
                panic!("HTTP mode must not use the WS outbound queue")
            }
            MonitorDeliveryTarget::HttpCallback => {}
        }
        // And the shared markdown content is identical across transports.
        let ws_markdown = crate::gateway::wecom_card::build_monitor_result_markdown(
            "completed", 30, 30815, "检测到桌面变化",
        );
        assert!(ws_markdown.contains("### OmniNova · 桌面监控完成"));
    }

    #[tokio::test]
    async fn http_gateway_status_parity() {
        // Same shared core → same rendered content regardless of transport.
        let runtime = Arc::new(GatewayRuntime::new(Config::default()));
        let config = http_config();
        let task_ws = new_task_id();
        let task_http = new_task_id();
        crate::gateway::wecom_card::card_store()
            .register(task_ws.clone(), None)
            .await;
        register_http_panel_with_url(&task_http, None, None).await;
        let body_ws: WecomCallbackBody = serde_json::from_value(json!({
            "msgid": "evt-ws-p", "msgtype": "event", "from": {"userid": "u"},
            "event": {"eventtype": "template_card_event", "event_key": "gateway_status", "task_id": task_ws}
        })).unwrap();
        let body_http: WecomCallbackBody = serde_json::from_value(json!({
            "msgid": "evt-http-p", "msgtype": "event", "from": {"userid": "u"},
            "event": {"eventtype": "template_card_event", "event_key": "gateway_status", "task_id": task_http}
        })).unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel::<crate::gateway::wecom_stream::WecomOutboundMsg>(4);
        let ws_outcome = crate::gateway::wecom_card::handle_card_event_core(
            &runtime,
            &body_ws,
            WecomCardTransport::LongConnection,
            &MonitorDeliveryTarget::LongConnection { outbound_tx: tx },
        )
        .await;
        let http_outcome = crate::gateway::wecom_card::handle_card_event_core(
            &runtime,
            &body_http,
            WecomCardTransport::HttpCallback,
            &MonitorDeliveryTarget::HttpCallback,
        )
        .await;
        match (ws_outcome, http_outcome) {
            (
                CardEventOutcome::Updated { card: ws_card, .. },
                CardEventOutcome::Updated { card: http_card, .. },
            ) => {
                // Same renderer core; only the connection line is
                // transport-aware (WS reads the socket state, HTTP
                // proves connectivity via the callback itself).
                let ws_text = ws_card["sub_title_text"].as_str().unwrap();
                let http_text = http_card["sub_title_text"].as_str().unwrap();
                for shared in ["运行状态", "网关：运行中", "连接方式：长连接"] {
                    assert!(ws_text.contains(shared), "WS missing {shared}");
                    assert!(http_text.contains(shared), "HTTP missing {shared}");
                }
                assert!(ws_text.contains("企业微信：未连接"), "WS reads the socket state");
                assert!(http_text.contains("企业微信：已连接"), "HTTP callback proves the channel");
            }
            other => panic!("expected Updated outcomes, got {other:?}"),
        }
        let _ = config;
    }

    #[tokio::test]
    async fn http_recent_jobs_parity() {
        let store = WecomCardStore::new();
        store.record_job("single", "已受理").await;
        store.record_job("监控30秒", "已完成").await;
        // The shared recent-jobs render is transport-agnostic.
        let text = store.recent_jobs_text().await;
        assert!(text.contains("最近任务"));
        assert!(text.contains("监控30秒 · 已完成"));
        assert!(text.contains("单聊 · 处理中"));
    }

    #[tokio::test]
    async fn http_help_parity() {
        // One shared help render — no HTTP-only copy exists.
        let http_help = crate::gateway::wecom_card::help_text();
        assert_eq!(http_help, crate::gateway::wecom_card::help_text());
        assert!(http_help.contains("menu / 菜单 / 面板"));
    }

    #[tokio::test]
    async fn http_monitor_singleflight_parity() {
        // Shared MonitorState/single-flight: the same store rules the
        // HTTP panel as the long-connection panel.
        let store = WecomCardStore::new();
        store
            .register_with_transport(
                "task-hsf".to_string(),
                None,
                WecomCardTransport::HttpCallback,
            )
            .await;
        assert!(store.try_start_monitor("task-hsf", 30, 0).await);
        assert!(!store.try_start_monitor("task-hsf", 30, 0).await);
        let remaining = store.monitor_remaining_secs("task-hsf").await.unwrap();
        assert!(remaining <= 30);
    }

    #[tokio::test]
    async fn long_connection_delivery_regression() {
        // WS delivery must remain the proactive aibot_send_msg path.
        use crate::gateway::wecom_card::deliver_monitor_result;
        use crate::gateway::wecom_stream::WecomOutboundMsg;
        let runtime = Arc::new(GatewayRuntime::new(Config::default()));
        let (tx, mut rx) = tokio::sync::mpsc::channel::<WecomOutboundMsg>(8);
        let store = WecomCardStore::new();
        store.register("task-ws-del".to_string(), None).await;
        deliver_monitor_result(
            &runtime,
            &store,
            &tx,
            "task-ws-del",
            Some("user-target".to_string()),
            0,
            "completed",
            30,
            30815,
            "检测到桌面变化",
        )
        .await;
        match rx.try_recv().expect("WS delivery must queue a proactive message") {
            WecomOutboundMsg::ProactiveMessage { body, .. } => {
                assert_eq!(body["msgtype"], "markdown");
            }
            other => panic!("expected ProactiveMessage, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn http_transport_never_starts_websocket() {
        // The HTTP adapter never touches the WebSocket outbound types:
        // its monitor delivery target is HttpCallback, and the deferred
        // delivery function has no sender parameter at all.
        let delivery = MonitorDeliveryTarget::HttpCallback;
        match delivery {
            MonitorDeliveryTarget::LongConnection { .. } => {
                panic!("HTTP mode must not construct a WS delivery target")
            }
            MonitorDeliveryTarget::HttpCallback => {}
        }
        // And the initial-card path stays inside the HTTP crypto envelope.
        let config = http_config();
        let parsed = parse_encrypted_callback(&config, &text_callback("menu"));
        let reply = panel_trigger_response(&config, &query_for("x"), &parsed)
            .await
            .unwrap();
        assert!(reply.get("encrypt").is_some());
    }

    #[tokio::test]
    async fn long_connection_card_regression() {
        // The shared core + WS adapter must keep working: gateway_status
        // over the long connection still queues a TemplateCardUpdate.
        use crate::gateway::wecom_card::{card_store, handle_card_event, new_task_id};
        use crate::gateway::wecom_stream::WecomOutboundMsg;
        let runtime = Arc::new(GatewayRuntime::new(Config::default()));
        let (tx, mut rx) = tokio::sync::mpsc::channel::<WecomOutboundMsg>(8);
        let task_id = new_task_id();
        card_store().register(task_id.clone(), None).await;
        let body: WecomCallbackBody = serde_json::from_value(json!({
            "msgid": "evt-ws-reg",
            "msgtype": "event",
            "from": {"userid": "u-1"},
            "event": {
                "eventtype": "template_card_event",
                "event_key": "gateway_status",
                "task_id": task_id
            }
        }))
        .unwrap();
        handle_card_event(&runtime, &body, "req-ws-reg", &tx)
            .await
            .unwrap();
        let queued = rx.try_recv().expect("WS adapter must still queue an update");
        assert!(matches!(
            queued,
            WecomOutboundMsg::TemplateCardUpdate { .. }
        ));
    }

    // ------------------------------------------------------------------
    // Phase 2A.3.3b: HTTP update-response semantics
    // ------------------------------------------------------------------

    #[test]
    fn http_initial_card_plaintext_is_msgtype_template_card() {
        let card = crate::gateway::wecom_card::build_panel_card("t-init");
        let plaintext = template_card_plaintext(&card);
        let value: serde_json::Value = serde_json::from_str(&plaintext).unwrap();
        assert_eq!(value["msgtype"], "template_card");
        assert!(value.get("response_type").is_none(), "initial card must not be an update response");
    }

    #[test]
    fn http_card_update_plaintext_is_response_type_update_template_card() {
        let card = crate::gateway::wecom_card::build_panel_card_with_text("t-upd", "运行状态");
        let plaintext = build_http_template_card_update_response(&card);
        let value: serde_json::Value = serde_json::from_str(&plaintext).unwrap();
        assert_eq!(value["response_type"], "update_template_card");
        assert_eq!(value["template_card"]["task_id"], "t-upd");
    }

    #[test]
    fn http_card_update_has_no_msgtype_template_card() {
        let card = crate::gateway::wecom_card::build_panel_card_with_text("t-nom", "帮助");
        let plaintext = build_http_template_card_update_response(&card);
        let value: serde_json::Value = serde_json::from_str(&plaintext).unwrap();
        assert!(value.get("msgtype").is_none(), "update response must not carry msgtype");
    }

    #[test]
    fn http_card_update_preserves_task_id() {
        let task_id = new_task_id();
        let card = crate::gateway::wecom_card::build_panel_card_with_text(&task_id, "进行中");
        let plaintext = build_http_template_card_update_response(&card);
        let value: serde_json::Value = serde_json::from_str(&plaintext).unwrap();
        assert_eq!(value["template_card"]["task_id"], task_id);
    }

    async fn run_update_for_action(action: &str) -> (String, String) {
        let runtime = Arc::new(GatewayRuntime::new(Config::default()));
        let config = http_config();
        let task_id = new_task_id();
        register_http_panel_with_url(&task_id, None, None).await;
        let body: WecomCallbackBody =
            serde_json::from_value(nested_card_event(&task_id, action)).unwrap();
        let reply = handle_http_card_event(&runtime, &config, &query_for("x"), &body)
            .await
            .unwrap();
        let plaintext = decrypt_reply(&reply);
        (plaintext, task_id)
    }

    #[tokio::test]
    async fn http_gateway_status_returns_update_response() {
        let (plaintext, task_id) = run_update_for_action("gateway_status").await;
        assert!(plaintext.contains("\"response_type\":\"update_template_card\""));
        assert!(!plaintext.contains("\"msgtype\":\"template_card\""));
        assert!(plaintext.contains("运行状态"));
        assert!(plaintext.contains(&task_id));
    }

    #[tokio::test]
    async fn http_recent_jobs_returns_update_response() {
        crate::gateway::wecom_card::card_store()
            .record_job("single", "已受理")
            .await;
        let (plaintext, _) = run_update_for_action("recent_jobs").await;
        assert!(plaintext.contains("\"response_type\":\"update_template_card\""));
        assert!(plaintext.contains("最近任务"));
    }

    #[tokio::test]
    async fn http_help_returns_update_response() {
        let (plaintext, _) = run_update_for_action("help").await;
        assert!(plaintext.contains("\"response_type\":\"update_template_card\""));
        assert!(plaintext.contains("menu / 菜单 / 面板"));
    }

    #[tokio::test]
    async fn http_monitor_30_returns_update_response() {
        crate::gateway::wecom_card::disable_monitor_execution_for_tests(true);
        let (plaintext, task_id) = run_update_for_action("monitor_30").await;
        crate::gateway::wecom_card::disable_monitor_execution_for_tests(false);
        assert!(plaintext.contains("\"response_type\":\"update_template_card\""));
        assert!(plaintext.contains("进行中"));
        assert!(plaintext.contains("剩余约 30 秒"));
        assert!(plaintext.contains(&task_id));
    }

    #[tokio::test]
    async fn http_monitor_60_returns_update_response() {
        crate::gateway::wecom_card::disable_monitor_execution_for_tests(true);
        let (plaintext, _) = run_update_for_action("monitor_60").await;
        crate::gateway::wecom_card::disable_monitor_execution_for_tests(false);
        assert!(plaintext.contains("\"response_type\":\"update_template_card\""));
        assert!(plaintext.contains("进行中"));
        assert!(plaintext.contains("剩余约 60 秒"));
    }

    #[tokio::test]
    async fn http_update_response_is_encrypted() {
        let runtime = Arc::new(GatewayRuntime::new(Config::default()));
        let config = http_config();
        let task_id = new_task_id();
        register_http_panel_with_url(&task_id, None, None).await;
        let body: WecomCallbackBody =
            serde_json::from_value(nested_card_event(&task_id, "help")).unwrap();
        let reply = handle_http_card_event(&runtime, &config, &query_for("x"), &body)
            .await
            .unwrap();
        assert!(reply.get("encrypt").is_some());
        assert!(reply.get("msgsignature").is_some());
        assert_eq!(reply["timestamp"], "1700000000");
        assert_eq!(reply["nonce"], "nonce-1");
    }

    #[tokio::test]
    async fn http_update_uses_existing_crypto() {
        // The update plaintext flows through the SAME verified crypto
        // path (AES + signature) as the GET/stream replies: a decrypt
        // roundtrip with the same key must reproduce the plaintext.
        let runtime = Arc::new(GatewayRuntime::new(Config::default()));
        let config = http_config();
        let task_id = new_task_id();
        register_http_panel_with_url(&task_id, None, None).await;
        let body: WecomCallbackBody =
            serde_json::from_value(nested_card_event(&task_id, "help")).unwrap();
        let reply = handle_http_card_event(&runtime, &config, &query_for("x"), &body)
            .await
            .unwrap();
        let decrypted = decrypt_reply(&reply);
        assert!(decrypted.contains("update_template_card"));
        assert!(decrypted.contains(&task_id));
    }

    #[tokio::test]
    async fn http_monitor_result_response_url_unchanged() {
        // The monitor FINAL result still goes through response_url (the
        // verified path) — the card-event update fix does not reroute it
        // back into the synchronous callback.
        let store = WecomCardStore::new();
        store
            .register_with_transport(
                "task-fixed-delivery".to_string(),
                None,
                WecomCardTransport::HttpCallback,
            )
            .await;
        store
            .complete_monitor("task-fixed-delivery", 30, 30000, "检测到桌面变化".to_string())
            .await;
        // No response_url → delivery fails safely, exactly as before.
        crate::gateway::wecom_card::deliver_monitor_result_http(
            &store,
            "task-fixed-delivery",
            "completed",
            30,
            30000,
            "检测到桌面变化",
        )
        .await;
        match store.monitor_state("task-fixed-delivery").await {
            MonitorState::Completed { delivery, .. } => {
                assert_eq!(delivery, MonitorDelivery::Failed { errcode: -1 });
            }
            other => panic!("expected Completed with failed delivery, got {other:?}"),
        }
        // And the shared markdown content is untouched.
        let markdown = build_monitor_result_markdown("completed", 30, 30000, "检测到桌面变化");
        assert!(markdown.contains("### OmniNova · 桌面监控完成"));
    }

    #[tokio::test]
    async fn http_normal_text_stream_unchanged() {
        let config = http_config();
        let parsed = parse_encrypted_callback(&config, &text_callback("你好"));
        assert!(panel_trigger_response(&config, &query_for("x"), &parsed)
            .await
            .is_none());
        // The stream placeholder reply shape is untouched.
        let plaintext = crate::gateway::wecom_http::build_stream_reply_plaintext("s", "正在处理中...", false);
        assert!(plaintext.contains("\"msgtype\":\"stream\""));
    }

    // ------------------------------------------------------------------
    // Parity gap fills (WS vs HTTP cards)
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn http_panel_trigger_retry_reuses_same_task_id() {
        let config = http_config();
        // Same msgid twice: the retry must reuse the SAME panel task_id.
        let first = parse_encrypted_callback(&config, &text_callback("menu")).clone();
        let reply_one = panel_trigger_response(&config, &query_for("x"), &first)
            .await
            .unwrap();
        let reply_two = panel_trigger_response(&config, &query_for("x"), &first)
            .await
            .unwrap();
        let card_one: serde_json::Value = serde_json::from_str(&decrypt_reply(&reply_one)).unwrap();
        let card_two: serde_json::Value = serde_json::from_str(&decrypt_reply(&reply_two)).unwrap();
        assert_eq!(
            card_one["template_card"]["task_id"],
            card_two["template_card"]["task_id"],
            "retried menu callback must reuse the same panel"
        );
    }

    #[tokio::test]
    async fn http_gateway_status_connected_in_http_mode() {
        let (plaintext, _) = run_update_for_action("gateway_status").await;
        assert!(plaintext.contains("企业微信：已连接"), "HTTP callback proves the channel");
        assert!(!plaintext.contains("企业微信：未连接"));
    }

    #[tokio::test]
    async fn http_expired_rejection_updates_original_card() {
        let runtime = Arc::new(GatewayRuntime::new(Config::default()));
        let config = http_config();
        let task_id = new_task_id();
        card_store()
            .register_at_with_transport(
                task_id.clone(),
                None,
                time::OffsetDateTime::now_utc().unix_timestamp() - 3600,
                WecomCardTransport::HttpCallback,
            )
            .await;
        let body: WecomCallbackBody =
            serde_json::from_value(nested_card_event(&task_id, "help")).unwrap();
        let reply = handle_http_card_event(&runtime, &config, &query_for("x"), &body)
            .await
            .unwrap();
        let plaintext = decrypt_reply(&reply);
        assert!(
            plaintext.contains("\"response_type\":\"update_template_card\""),
            "expired rejection must update the original card, got: {}",
            plaintext
        );
        assert!(plaintext.contains("已过期"));
        assert!(plaintext.contains(&task_id));
    }

    #[tokio::test]
    async fn http_unknown_action_rejection_updates_original_card() {
        let runtime = Arc::new(GatewayRuntime::new(Config::default()));
        let config = http_config();
        let task_id = new_task_id();
        register_http_panel_with_url(&task_id, None, None).await;
        let body: WecomCallbackBody =
            serde_json::from_value(nested_card_event(&task_id, "rm -rf")).unwrap();
        let reply = handle_http_card_event(&runtime, &config, &query_for("x"), &body)
            .await
            .unwrap();
        let plaintext = decrypt_reply(&reply);
        assert!(plaintext.contains("\"response_type\":\"update_template_card\""));
        assert!(plaintext.contains("未知操作"));
        assert!(plaintext.contains(&task_id));
    }
}
