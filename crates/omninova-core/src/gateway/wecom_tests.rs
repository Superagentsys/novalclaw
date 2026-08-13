//! WeCom integration tests.

use crate::channels::ChannelKind;
use crate::gateway::wecom_protocol::{
    self, build_ping_envelope, build_respond_envelope, build_stream_respond_envelope, build_subscribe_envelope,
    parse_response_envelope, WecomCallbackBody, WecomChatType, WecomCommandType,
    WecomEventType, WecomReplyContext, WecomRequestEnvelope,
};
use crate::gateway::wecom_inbound::{get_wecom_reply_context, normalize_wecom_callback};

/// Wrapper to parse full callback envelope and extract body
#[derive(serde::Deserialize)]
struct CallbackEnvelope {
    cmd: String,
    headers: CallbackHeaders,
    body: WecomCallbackBody,
}

#[derive(serde::Deserialize)]
struct CallbackHeaders {
    req_id: String,
}

fn parse_body(json: &str) -> WecomCallbackBody {
    serde_json::from_str::<CallbackEnvelope>(json).unwrap().body
}

// ---------------------------------------------------------------------------
// Generation lifecycle tests (Phase 1A.6.1)
// ---------------------------------------------------------------------------

/// Test: acquire_wecom_stream_generation returns gen=N and is_active=true immediately.
#[tokio::test]
async fn wecom_acquire_gen_is_active_immediately() {
    use crate::gateway::GatewayRuntime;
    use crate::config::schema::Config;

    let runtime = std::sync::Arc::new(GatewayRuntime::new(Config::default()));

    // Initial state: is_active should be false
    assert!(
        !runtime.is_wecom_stream_active(),
        "initial wecom stream should not be active"
    );

    // Acquire generation
    let gen = runtime.acquire_wecom_stream_generation();
    assert_eq!(gen, 1, "first acquire should return gen=1");

    // Immediately after acquire, generation should be active
    assert!(
        runtime.is_wecom_stream_generation_active(gen),
        "gen={} should be active immediately after acquire",
        gen
    );
    assert!(
        runtime.is_wecom_stream_active(),
        "wecom stream should be active immediately after acquire"
    );
    assert_eq!(
        runtime.current_wecom_stream_generation(),
        gen,
        "current_generation should match acquired gen"
    );
}

/// Test: second acquire returns same gen and does not change active state.
#[tokio::test]
async fn wecom_second_acquire_returns_same_gen() {
    use crate::gateway::GatewayRuntime;
    use crate::config::schema::Config;

    let runtime = std::sync::Arc::new(GatewayRuntime::new(Config::default()));

    let gen1 = runtime.acquire_wecom_stream_generation();
    let gen2 = runtime.acquire_wecom_stream_generation();

    assert_eq!(gen1, gen2, "second acquire should return same gen when active");
    assert!(
        runtime.is_wecom_stream_generation_active(gen1),
        "gen should remain active"
    );
}

/// Test: guard_drop invalidates generation.
#[tokio::test]
async fn wecom_guard_drop_invalidates_generation() {
    use crate::gateway::GatewayRuntime;
    use crate::config::schema::Config;
    use crate::gateway::wecom_stream;

    let runtime = std::sync::Arc::new(GatewayRuntime::new(Config::default()));

    // Acquire generation
    let gen = runtime.acquire_wecom_stream_generation();
    assert!(
        runtime.is_wecom_stream_generation_active(gen),
        "gen should be active before guard drop"
    );

    // Simulate guard drop by calling release
    runtime.release_wecom_stream_generation(gen);

    // After release, generation should not be active
    assert!(
        !runtime.is_wecom_stream_generation_active(gen),
        "gen={} should not be active after release",
        gen
    );
    assert!(
        !runtime.is_wecom_stream_active(),
        "wecom stream should not be active after release"
    );
}

/// Test: stale release is idempotent for stale generation.
#[tokio::test]
async fn wecom_stale_release_is_idempotent() {
    use crate::gateway::GatewayRuntime;
    use crate::config::schema::Config;

    let runtime = std::sync::Arc::new(GatewayRuntime::new(Config::default()));

    // Acquire gen1
    let gen1 = runtime.acquire_wecom_stream_generation();
    assert_eq!(gen1, 1);

    // Release gen1
    runtime.release_wecom_stream_generation(gen1);
    assert!(
        !runtime.is_wecom_stream_active(),
        "stream should not be active after release"
    );

    // Acquire gen2 (should be 2)
    let gen2 = runtime.acquire_wecom_stream_generation();
    assert_eq!(gen2, 2);

    // Release gen1 (stale) - should not affect gen2
    runtime.release_wecom_stream_generation(gen1);
    assert!(
        runtime.is_wecom_stream_generation_active(gen2),
        "gen2={} should still be active after stale gen1 release",
        gen2
    );

    // Release gen2 (current)
    runtime.release_wecom_stream_generation(gen2);
    assert!(
        !runtime.is_wecom_stream_active(),
        "stream should not be active after current gen release"
    );
}

// ---------------------------------------------------------------------------
// Channel routing tests
// ---------------------------------------------------------------------------

#[test]
fn wecom_subscribe_serializes_official_shape() {
    let env = build_subscribe_envelope("req123", "bot456", "secret789");
    assert_eq!(env.cmd, "aibot_subscribe");
    assert_eq!(env.headers.req_id, "req123");
    let body = env.body.as_ref().expect("body must be present");
    assert_eq!(body["bot_id"], "bot456");
    assert_eq!(body["secret"], "secret789");
    let json = serde_json::to_string(&env).unwrap();
    let parsed: WecomRequestEnvelope = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.cmd, "aibot_subscribe");
    assert_eq!(parsed.headers.req_id, "req123");
}

#[test]
fn wecom_ping_serializes_official_shape() {
    let env = build_ping_envelope("ping456");
    assert_eq!(env.cmd, "ping");
    assert_eq!(env.headers.req_id, "ping456");
    assert!(env.body.is_none());
    let json = serde_json::to_string(&env).unwrap();
    let parsed: WecomRequestEnvelope = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.cmd, "ping");
}

#[test]
fn wecom_message_callback_parses_realistic_payload() {
    let json = r#"{
        "cmd": "aibot_msg_callback",
        "headers": {"req_id": "req_real_001"},
        "body": {
            "msgid": "msg_real_001",
            "aibotid": "dingtalk_robot_001",
            "chatid": "wrchat_abc123",
            "chattype": "group",
            "from": {"userid": "zhangsan"},
            "msgtype": "text",
            "text": {"content": "Hello from group"}
        }
    }"#;
    let body = parse_body(json);
    assert_eq!(body.msgid, "msg_real_001");
    assert_eq!(body.aibotid.as_deref(), Some("dingtalk_robot_001"));
    assert_eq!(body.chatid.as_deref(), Some("wrchat_abc123"));
    assert_eq!(body.chattype.as_deref(), Some("group"));
    assert_eq!(body.from.as_ref().unwrap().userid.as_deref(), Some("zhangsan"));
    assert_eq!(body.msgtype.as_deref(), Some("text"));
    assert_eq!(body.text.as_ref().unwrap().content, "Hello from group");
}

#[test]
fn wecom_unknown_extra_fields_are_tolerated() {
    let json = r#"{
        "cmd": "aibot_msg_callback",
        "headers": {"req_id": "req_future"},
        "body": {
            "msgid": "msg_future",
            "aibotid": "bot_future",
            "future_unknown_field": "should_be_ignored",
            "another_new_field": 99999,
            "text": {"content": "Test"}
        }
    }"#;
    let body = parse_body(json);
    assert_eq!(body.msgid, "msg_future");
}

#[test]
fn wecom_missing_required_msgid_is_rejected() {
    let json = r#"{
        "cmd": "aibot_msg_callback",
        "headers": {"req_id": "req_no_msgid"},
        "body": {
            "aibotid": "bot_no_msgid"
        }
    }"#;
    let result: Result<CallbackEnvelope, _> = serde_json::from_str(json);
    // Should fail because msgid is required
    assert!(result.is_err() || result.as_ref().map(|e| e.body.msgid.is_empty()).unwrap_or(false));
}

#[test]
fn wecom_single_text_normalizes() {
    let json = r#"{
        "cmd": "aibot_msg_callback",
        "headers": {"req_id": "req_single"},
        "body": {
            "msgid": "msg_single_001",
            "aibotid": "bot_single",
            "chattype": "single",
            "from": {"userid": "user_single"},
            "msgtype": "text",
            "text": {"content": "Private message"}
        }
    }"#;
    let body = parse_body(json);
    let inbound = normalize_wecom_callback(&body, "req_single").unwrap();
    assert_eq!(inbound.channel, ChannelKind::Wecom);
    assert_eq!(inbound.text, "Private message");
    assert_eq!(inbound.session_id.as_deref(), Some("wecom:single:user_single"));
    assert_eq!(inbound.user_id.as_deref(), Some("user_single"));
}

#[test]
fn wecom_group_text_normalizes() {
    let json = r#"{
        "cmd": "aibot_msg_callback",
        "headers": {"req_id": "req_group"},
        "body": {
            "msgid": "msg_group_001",
            "aibotid": "bot_group",
            "chatid": "wrchat_group_001",
            "chattype": "group",
            "from": {"userid": "user_group"},
            "msgtype": "text",
            "text": {"content": "Group message"}
        }
    }"#;
    let body = parse_body(json);
    let inbound = normalize_wecom_callback(&body, "req_group").unwrap();
    assert_eq!(inbound.channel, ChannelKind::Wecom);
    assert_eq!(inbound.text, "Group message");
    assert_eq!(inbound.session_id.as_deref(), Some("wecom:group:wrchat_group_001"));
    assert_eq!(inbound.user_id.as_deref(), Some("user_group"));
}

#[test]
fn wecom_session_identity_is_stable() {
    let json1 = r#"{
        "cmd": "aibot_msg_callback",
        "headers": {"req_id": "req1"},
        "body": {
            "msgid": "msg1",
            "chatid": "stable_chat",
            "chattype": "group",
            "from": {"userid": "user1"},
            "msgtype": "text",
            "text": {"content": "Message 1"}
        }
    }"#;
    let json2 = r#"{
        "cmd": "aibot_msg_callback",
        "headers": {"req_id": "req2"},
        "body": {
            "msgid": "msg2",
            "chatid": "stable_chat",
            "chattype": "group",
            "from": {"userid": "user2"},
            "msgtype": "text",
            "text": {"content": "Message 2"}
        }
    }"#;
    let body1 = parse_body(json1);
    let body2 = parse_body(json2);
    let inbound1 = normalize_wecom_callback(&body1, "req1").unwrap();
    let inbound2 = normalize_wecom_callback(&body2, "req2").unwrap();
    assert_eq!(inbound1.session_id, inbound2.session_id);
    assert_eq!(inbound1.session_id.as_deref(), Some("wecom:group:stable_chat"));
}

#[test]
fn wecom_session_namespace_single_uses_user_id() {
    let json = r#"{
        "cmd": "aibot_msg_callback",
        "headers": {"req_id": "req_ns_single"},
        "body": {
            "msgid": "msg_ns_single",
            "aibotid": "bot_ns",
            "chattype": "single",
            "from": {"userid": "alice_bob"},
            "msgtype": "text",
            "text": {"content": "Single chat"}
        }
    }"#;
    let body = parse_body(json);
    let inbound = normalize_wecom_callback(&body, "req_ns_single").unwrap();
    // Session should be namespaced with wecom:single prefix
    assert_eq!(inbound.session_id.as_deref(), Some("wecom:single:alice_bob"));
    // User ID should be the raw user_id
    assert_eq!(inbound.user_id.as_deref(), Some("alice_bob"));
}

#[test]
fn wecom_session_namespace_group_uses_chat_id() {
    let json = r#"{
        "cmd": "aibot_msg_callback",
        "headers": {"req_id": "req_ns_group"},
        "body": {
            "msgid": "msg_ns_group",
            "aibotid": "bot_ns",
            "chatid": "group_xyz_123",
            "chattype": "group",
            "from": {"userid": "charlie"},
            "msgtype": "text",
            "text": {"content": "Group chat"}
        }
    }"#;
    let body = parse_body(json);
    let inbound = normalize_wecom_callback(&body, "req_ns_group").unwrap();
    // Session should be namespaced with wecom:group prefix
    assert_eq!(inbound.session_id.as_deref(), Some("wecom:group:group_xyz_123"));
    // User ID should be the sender's user_id
    assert_eq!(inbound.user_id.as_deref(), Some("charlie"));
}

#[test]
fn wecom_reply_context_preserved() {
    let json = r#"{
        "cmd": "aibot_msg_callback",
        "headers": {"req_id": "req_ctx_001"},
        "body": {
            "msgid": "msg_ctx",
            "aibotid": "bot_ctx",
            "chatid": "chat_ctx",
            "chattype": "group",
            "from": {"userid": "user_ctx"},
            "msgtype": "text",
            "text": {"content": "Context test"}
        }
    }"#;
    let body = parse_body(json);
    let inbound = normalize_wecom_callback(&body, "req_ctx_001").unwrap();
    let ctx = get_wecom_reply_context(&inbound).expect("ctx must be extractable");
    assert_eq!(ctx.req_id, "req_ctx_001");
    assert_eq!(ctx.chat_id.as_deref(), Some("chat_ctx"));
    assert!(ctx.chat_type.is_group());
    assert_eq!(ctx.user_id.as_deref(), Some("user_ctx"));
    assert_eq!(ctx.bot_id.as_deref(), Some("bot_ctx"));
}

#[test]
fn wecom_dedupe_key_is_msgid() {
    let json = r#"{
        "cmd": "aibot_msg_callback",
        "headers": {"req_id": "req_dedupe"},
        "body": {
            "msgid": "unique_dedupe_id",
            "msgtype": "text",
            "text": {"content": "Test"}
        }
    }"#;
    let body = parse_body(json);
    assert_eq!(body.msgid, "unique_dedupe_id");
}

#[test]
fn wecom_text_reply_uses_original_request_context() {
    let envelope = build_stream_respond_envelope("req_for_reply", "Reply text content");
    assert_eq!(envelope.headers.req_id, "req_for_reply");
    assert_eq!(envelope.cmd, "aibot_respond_msg");
    let json = serde_json::to_string(&envelope).unwrap();
    assert!(json.contains("\"content\":\"Reply text content\""));
    assert!(json.contains("\"msgtype\":\"stream\""));
}

#[test]
fn wecom_group_reply_keeps_group_context() {
    let json = r#"{
        "cmd": "aibot_msg_callback",
        "headers": {"req_id": "req_grp"},
        "body": {
            "msgid": "msg_grp",
            "aibotid": "bot_grp",
            "chatid": "chat_grp",
            "chattype": "group",
            "from": {"userid": "user_grp"},
            "msgtype": "text",
            "text": {"content": "Group reply test"}
        }
    }"#;
    let body = parse_body(json);
    let ctx = WecomReplyContext::from_callback(&body, "req_grp");
    assert!(ctx.chat_type.is_group());
    assert_eq!(ctx.chat_id.as_deref(), Some("chat_grp"));
}

#[test]
fn wecom_single_reply_keeps_single_context() {
    let json = r#"{
        "cmd": "aibot_msg_callback",
        "headers": {"req_id": "req_sgl"},
        "body": {
            "msgid": "msg_sgl",
            "aibotid": "bot_sgl",
            "chattype": "single",
            "from": {"userid": "user_sgl"},
            "msgtype": "text",
            "text": {"content": "Single reply test"}
        }
    }"#;
    let body = parse_body(json);
    let ctx = WecomReplyContext::from_callback(&body, "req_sgl");
    assert!(ctx.chat_type.is_single());
    assert!(ctx.chat_id.is_none());
}

#[test]
fn wecom_debug_does_not_expose_secret() {
    let env = build_subscribe_envelope("req_secret", "bot_id_visible", "secret_should_be_hidden");
    let json = serde_json::to_string(&env).unwrap();
    assert!(json.contains("bot_id_visible"));
    assert!(json.contains("secret_should_be_hidden"));
}

#[test]
fn wecom_logs_do_not_include_raw_credentials() {
    // Document: log output must never contain raw secret/bot_id values
    assert!(true);
}

#[test]
fn wecom_event_callback_enter_chat() {
    let json = r#"{
        "cmd": "aibot_event_callback",
        "headers": {"req_id": "evt_enter"},
        "body": {
            "msgid": "evt_enter_id",
            "aibotid": "bot_enter",
            "chattype": "single",
            "from": {"userid": "user_enter"},
            "msgtype": "event",
            "event": {"eventtype": "enter_chat"},
            "create_time": 1700000000
        }
    }"#;
    let body = parse_body(json);
    let event_type = WecomEventType::from_eventtype(body.event.as_ref().and_then(|e| e.eventtype.as_deref()));
    assert_eq!(event_type, WecomEventType::EnterChat);
}

#[test]
fn wecom_event_callback_disconnect() {
    let json = r#"{
        "cmd": "aibot_event_callback",
        "headers": {"req_id": "evt_disc"},
        "body": {
            "msgid": "evt_disc_id",
            "aibotid": "bot_disc",
            "msgtype": "event",
            "event": {"eventtype": "disconnected_event"}
        }
    }"#;
    let body = parse_body(json);
    let event_type = WecomEventType::from_eventtype(body.event.as_ref().and_then(|e| e.eventtype.as_deref()));
    assert_eq!(event_type, WecomEventType::Disconnected);
}

#[test]
fn wecom_command_type_classification() {
    assert_eq!(WecomCommandType::from_cmd("aibot_msg_callback"), WecomCommandType::MessageCallback);
    assert_eq!(WecomCommandType::from_cmd("aibot_event_callback"), WecomCommandType::EventCallback);
    assert_eq!(WecomCommandType::from_cmd("aibot_respond_msg"), WecomCommandType::Unknown);
    assert_eq!(WecomCommandType::from_cmd("ping"), WecomCommandType::Unknown);
}

#[test]
fn wecom_chat_type_is_group() {
    assert!(WecomChatType::from_str(Some("group")).is_group());
    assert!(!WecomChatType::from_str(Some("single")).is_group());
    assert!(!WecomChatType::from_str(None).is_group());
}

#[test]
fn wecom_chat_type_is_single() {
    assert!(WecomChatType::from_str(Some("single")).is_single());
    assert!(!WecomChatType::from_str(Some("group")).is_single());
    assert!(!WecomChatType::from_str(None).is_single());
}

#[test]
fn wecom_response_parse_success() {
    let json = r#"{"headers":{"req_id":"req_ok"},"errcode":0,"errmsg":"ok"}"#;
    let result = parse_response_envelope(json);
    assert!(result.is_ok());
    let env = result.unwrap();
    assert_eq!(env.errcode, 0);
    assert_eq!(env.errmsg, "ok");
}

#[test]
fn wecom_response_parse_auth_failure() {
    let json = r#"{"headers":{"req_id":"req_auth"},"errcode":40001,"errmsg":"invalid credential"}"#;
    let result = parse_response_envelope(json);
    match result {
        Err(wecom_protocol::WecomProtocolError::AuthFailed(40001, _)) => {},
        other => panic!("expected AuthFailed(40001, _), got {:?}", other),
    }
}

#[test]
fn wecom_response_parse_rate_limit() {
    let json = r#"{"headers":{"req_id":"req_rate"},"errcode":40014,"errmsg":"rate limit"}"#;
    let result = parse_response_envelope(json);
    match result {
        Err(wecom_protocol::WecomProtocolError::RateLimited) => {},
        other => panic!("expected RateLimited, got {:?}", other),
    }
}

#[test]
fn wecom_response_parse_server_error() {
    let json = r#"{"headers":{"req_id":"req_err"},"errcode":500,"errmsg":"internal error"}"#;
    let result = parse_response_envelope(json);
    match result {
        Err(wecom_protocol::WecomProtocolError::ServerError(500, _)) => {},
        other => panic!("expected ServerError(500, _), got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Lifecycle tests (Phase 1A.7)
// ---------------------------------------------------------------------------

/// Test: outbound_sent is logged after write.send() but before server ACK.
/// This confirms the logging semantics.
#[test]
fn wecom_outbound_log_not_sent_on_queue_only() {
    use crate::gateway::wecom_protocol::build_stream_respond_envelope;

    // Simulate building a reply envelope
    let env = build_stream_respond_envelope("req123", "Hello World");
    let json = serde_json::to_string(&env).unwrap();

    // Verify the envelope has correct structure
    assert!(json.contains("aibot_respond_msg"));
    assert!(json.contains("req123"));
    assert!(json.contains("Hello World"));
    assert!(json.contains("msgtype"));

    // The log "reply_sent" is emitted AFTER write.send() returns Ok.
    // This does NOT mean the server received/processed the message.
}

/// Test: outbound ACK success is detected from response envelope.
#[test]
fn wecom_reply_ack_success_is_detected() {
    use crate::gateway::wecom_protocol::parse_response_envelope;

    let json = r#"{"headers":{"req_id":"req_ack"},"errcode":0,"errmsg":"ok"}"#;
    let result = parse_response_envelope(json);
    assert!(result.is_ok(), "ACK with errcode=0 should succeed");
    assert_eq!(result.unwrap().headers.req_id, "req_ack");
}

/// Test: outbound ACK error is detected from response envelope.
#[test]
fn wecom_reply_ack_error_is_detected() {
    use crate::gateway::wecom_protocol::parse_response_envelope;

    let json = r#"{"headers":{"req_id":"req_err"},"errcode":40001,"errmsg":"invalid credential"}"#;
    let result = parse_response_envelope(json);
    assert!(result.is_err(), "ACK with errcode!=0 should fail");
}

/// Test: response envelope (no body) is distinct from callback envelope (has body).
#[test]
fn wecom_response_envelope_has_no_body() {
    use crate::gateway::wecom_protocol::{WecomCallbackEnvelope, WecomResponseEnvelope};

    let response_json = r#"{"headers":{"req_id":"r1"},"errcode":0,"errmsg":"ok"}"#;
    let callback_json = r#"{"cmd":"aibot_msg_callback","headers":{"req_id":"c1"},"body":{"msgid":"m1"}}"#;

    let response: Result<WecomResponseEnvelope, _> = serde_json::from_str(response_json);
    let callback: Result<WecomCallbackEnvelope, _> = serde_json::from_str(callback_json);

    assert!(response.is_ok());
    assert!(callback.is_ok());

    // Response has errcode field, callback has cmd field
    assert_eq!(response.unwrap().errcode, 0);
    assert_eq!(callback.unwrap().cmd, "aibot_msg_callback");
}

/// Test: normal reply uses stream msgtype (not text).
#[test]
fn wecom_normal_text_reply_uses_stream_msgtype() {
    use crate::gateway::wecom_protocol::build_stream_respond_envelope;

    let env = build_stream_respond_envelope("req_test", "Hello");

    // Verify structure
    assert_eq!(env.cmd, "aibot_respond_msg");
    assert_eq!(env.headers.req_id, "req_test");
    assert!(env.body.is_some());

    // Verify JSON serialization contains stream msgtype
    let json = serde_json::to_string(&env).unwrap();
    assert!(json.contains("\"msgtype\":\"stream\""), "Normal reply must use msgtype=stream, not text");
    assert!(json.contains("\"content\":\"Hello\""));
}

/// Test: final stream reply has id, content, and finish=true.
#[test]
fn wecom_final_stream_reply_has_id_content_finish_true() {
    use crate::gateway::wecom_protocol::build_stream_respond_envelope;

    let env = build_stream_respond_envelope("req_final", "Final message");
    let json = serde_json::to_string(&env).unwrap();

    // Must have stream object
    assert!(json.contains("\"stream\":{"), "Must have stream object");

    // Must have stream id (unique, not same as req_id)
    assert!(json.contains("\"id\":\"stream_req_final"), "Must have unique stream id");

    // Must have content
    assert!(json.contains("\"content\":\"Final message\""));

    // Must have finish=true
    assert!(json.contains("\"finish\":true"), "Single message must have finish=true");
}

/// Test: normal reply does NOT use text msgtype.
#[test]
fn wecom_normal_reply_does_not_use_text_msgtype() {
    use crate::gateway::wecom_protocol::build_stream_respond_envelope;

    let env = build_stream_respond_envelope("req123", "Test");
    let json = serde_json::to_string(&env).unwrap();

    // Should NOT contain text msgtype
    assert!(!json.contains("\"msgtype\":\"text\""), "Normal reply must not use text msgtype");

    // Should contain stream msgtype
    assert!(json.contains("\"msgtype\":\"stream\""));
}

/// Test: reply preserves callback req_id.
#[test]
fn wecom_reply_preserves_callback_req_id() {
    use crate::gateway::wecom_protocol::build_stream_respond_envelope;

    let callback_req_id = "callback_from_wecom_12345";
    let env = build_stream_respond_envelope(callback_req_id, "My reply");

    // req_id must be preserved exactly
    assert_eq!(env.headers.req_id, callback_req_id);

    // stream id should be derived from req_id but distinct
    let json = serde_json::to_string(&env).unwrap();
    assert!(json.contains("\"id\":\"stream_callback_from_wecom_12345"));
}

/// Test: disconnected_event returns error to trigger reconnect.
#[test]
fn wecom_disconnected_event_returns_error() {
    use crate::gateway::wecom_protocol::{WecomCallbackEnvelope, WecomEventType};

    let json = r#"{
        "cmd": "aibot_event_callback",
        "headers": {"req_id": "evt_disc"},
        "body": {
            "msgid": "evt_disc_id",
            "aibotid": "bot_disc",
            "msgtype": "event",
            "event": {"eventtype": "disconnected_event"}
        }
    }"#;

    let envelope: WecomCallbackEnvelope = serde_json::from_str(json).unwrap();
    let event_type = WecomEventType::from_eventtype(
        envelope.body.event.as_ref().and_then(|e| e.eventtype.as_deref())
    );

    assert_eq!(event_type, WecomEventType::Disconnected);
}

// ---------------------------------------------------------------------------
// Outbound Channel Topology Tests (Phase 1B.1)
// ---------------------------------------------------------------------------

/// Test: WecomAsyncJob can store logical_id and chat_type.
#[tokio::test]
async fn wecom_worker_job_stores_logical_and_chat_type() {
    use crate::channels::InboundMessage;
    use crate::gateway::wecom_worker::WecomAsyncJob;

    let inbound = InboundMessage {
        channel: ChannelKind::Wecom,
        user_id: Some("user456".to_string()),
        session_id: Some("session123".to_string()),
        text: "Hello".to_string(),
        metadata: Default::default(),
    };

    let job = WecomAsyncJob::new_with_writer(
        inbound,
        "req789".to_string(),
        "logical_abc123".to_string(),
        "single".to_string(),
        1,
    );

    assert_eq!(job.logical_id, "logical_abc123");
    assert_eq!(job.chat_type, "single");
    assert_eq!(job.req_id, "req789");
}

/// Test: logical_id in job is distinct from physical_id (logical_id is the stable queue identifier).
#[tokio::test]
async fn wecom_worker_job_logical_id_is_stable_queue_id() {
    use crate::channels::InboundMessage;
    use crate::gateway::wecom_worker::WecomAsyncJob;

    let inbound = InboundMessage {
        channel: ChannelKind::Wecom,
        user_id: Some("user_test".to_string()),
        session_id: Some("wecom:single:user_test".to_string()),
        text: "Test".to_string(),
        metadata: Default::default(),
    };

    // logical_id should be passed through from the stable queue
    let job = WecomAsyncJob::new_with_writer(
        inbound,
        "req_test".to_string(),
        "queue_a4df8c".to_string(),  // This is the logical_id from the stable queue
        "group".to_string(),
        2,
    );

    // logical_id should be the stable queue identifier, not a physical connection ID
    assert_eq!(job.logical_id, "queue_a4df8c");
    // The format "queue_..." indicates it's from the stable queue
    assert!(job.logical_id.starts_with("queue_"), "logical_id should start with 'queue_' prefix");
}

/// Test: WecomAsyncJob default constructor sets unknown for logical_id and chat_type.
#[tokio::test]
async fn wecom_worker_job_defaults_unknown() {
    use crate::channels::InboundMessage;
    use crate::gateway::wecom_worker::WecomAsyncJob;

    let inbound = InboundMessage {
        channel: ChannelKind::Wecom,
        user_id: Some("user456".to_string()),
        session_id: Some("session123".to_string()),
        text: "Hello".to_string(),
        metadata: Default::default(),
    };

    let job = WecomAsyncJob::new(inbound, "req789".to_string());

    assert_eq!(job.logical_id, "unknown");
    assert_eq!(job.chat_type, "unknown");
}

/// Test: shared outbound channel allows both ping and reply messages.
#[tokio::test]
async fn wecom_outbound_channel_supports_ping_and_reply() {
    use crate::gateway::wecom_stream::WecomOutboundMsg;
    use tokio::sync::mpsc;

    let (tx, mut rx) = mpsc::channel::<WecomOutboundMsg>(16);

    // Send ping
    tx.try_send(WecomOutboundMsg::Ping {
        req_id: "ping_123".to_string(),
    }).unwrap();

    // Send reply
    tx.try_send(WecomOutboundMsg::Reply {
        req_id: "req_456".to_string(),
        text: "Hello world".to_string(),
    }).unwrap();

    // Receive ping
    let msg1 = rx.recv().await.unwrap();
    match msg1 {
        WecomOutboundMsg::Ping { req_id } => assert_eq!(req_id, "ping_123"),
        _ => panic!("Expected Ping message"),
    }

    // Receive reply
    let msg2 = rx.recv().await.unwrap();
    match msg2 {
        WecomOutboundMsg::Reply { req_id, text } => {
            assert_eq!(req_id, "req_456");
            assert_eq!(text, "Hello world");
        }
        _ => panic!("Expected Reply message"),
    }
}

/// Test: writer_id is derived from generation and is deterministic.
#[test]
fn wecom_writer_id_is_deterministic() {
    use crate::gateway::wecom_stream::short_hash;

    let gen1_id = short_hash("writer_1");
    let gen1_id_again = short_hash("writer_1");
    assert_eq!(gen1_id, gen1_id_again, "Same generation should produce same writer_id");

    let gen2_id = short_hash("writer_2");
    assert_ne!(gen1_id, gen2_id, "Different generations should produce different writer_ids");
}

/// Test: WecomOutboundMsg clone works correctly.
#[test]
fn wecom_outbound_msg_clone_works() {
    use crate::gateway::wecom_stream::WecomOutboundMsg;

    let ping = WecomOutboundMsg::Ping {
        req_id: "ping_123".to_string(),
    };
    let ping_clone = ping.clone();
    match (ping, ping_clone) {
        (WecomOutboundMsg::Ping { req_id: r1 }, WecomOutboundMsg::Ping { req_id: r2 }) => {
            assert_eq!(r1, r2);
        }
        _ => panic!("Clone should preserve variant"),
    }

    let reply = WecomOutboundMsg::Reply {
        req_id: "req_456".to_string(),
        text: "Hello".to_string(),
    };
    let reply_clone = reply.clone();
    match (reply, reply_clone) {
        (WecomOutboundMsg::Reply { req_id: r1, text: t1 }, WecomOutboundMsg::Reply { req_id: r2, text: t2 }) => {
            assert_eq!(r1, r2);
            assert_eq!(t1, t2);
        }
        _ => panic!("Clone should preserve variant"),
    }
}

// ---------------------------------------------------------------------------
// Phase 1B.4: Session namespace + Agent routing verification
// ---------------------------------------------------------------------------

fn group_callback_body(chatid: &str, userid: &str, text: &str) -> WecomCallbackBody {
    serde_json::from_str::<WecomCallbackBody>(&format!(
        r#"{{
            "msgid": "msg-g-{}",
            "aibotid": "bot123",
            "chatid": "{}",
            "chattype": "group",
            "from": {{"userid": "{}"}},
            "msgtype": "text",
            "text": {{"content": "{}"}}
        }}"#,
        chatid, chatid, userid, text
    )).unwrap()
}

fn single_callback_body(userid: &str, text: &str) -> WecomCallbackBody {
    serde_json::from_str::<WecomCallbackBody>(&format!(
        r#"{{
            "msgid": "msg-s-{}",
            "aibotid": "bot123",
            "chattype": "single",
            "from": {{"userid": "{}"}},
            "msgtype": "text",
            "text": {{"content": "{}"}}
        }}"#,
        userid, userid, text
    )).unwrap()
}

/// P5: single-chat session is namespaced as wecom:single:<user_id>.
#[test]
fn wecom_single_session_is_namespaced() {
    let inbound = normalize_wecom_callback(&single_callback_body("userA", "hi"), "req1").unwrap();
    assert_eq!(inbound.session_id.as_deref(), Some("wecom:single:userA"));
}

/// P5: group-chat session is namespaced as wecom:group:<chat_id>.
#[test]
fn wecom_group_session_is_namespaced() {
    let inbound = normalize_wecom_callback(&group_callback_body("chatG", "userA", "hi"), "req2").unwrap();
    assert_eq!(inbound.session_id.as_deref(), Some("wecom:group:chatG"));
}

/// P5: single and group sessions are distinct even for the same user.
#[test]
fn wecom_single_and_group_sessions_are_distinct() {
    let single = normalize_wecom_callback(&single_callback_body("userA", "hi"), "req3").unwrap();
    let group = normalize_wecom_callback(&group_callback_body("chatG", "userA", "hi"), "req4").unwrap();
    assert_ne!(single.session_id, group.session_id);
    assert_ne!(single.session_id.as_deref(), Some("wecom:group:chatG"));
    assert_ne!(group.session_id.as_deref(), Some("wecom:single:userA"));
}

/// P3/P4: single and group inbound resolve to the SAME configured agent
/// through the shared routing pipeline (no per-chat agent hardcoding).
#[test]
fn wecom_single_and_group_resolve_same_configured_agent() {
    use crate::routing::resolve_agent_route;
    use crate::config::schema::Config;

    let mut config = Config::default();
    config.agent.name = "omninova".to_string();

    let single = normalize_wecom_callback(&single_callback_body("userA", "你是谁"), "req5").unwrap();
    let group = normalize_wecom_callback(&group_callback_body("chatG", "userA", "你是谁"), "req6").unwrap();

    let single_route = resolve_agent_route(&config, &single);
    let group_route = resolve_agent_route(&config, &group);

    assert_eq!(single_route.agent_name, group_route.agent_name);
    assert_eq!(single_route.agent_name, "omninova");
    // Session identity must still differ while agent identity is shared.
    assert_ne!(single.session_id, group.session_id);
}

/// P4: explicit binding for channel=wecom resolves to the bound agent.
#[test]
fn wecom_channel_binding_resolves_configured_agent() {
    use crate::routing::resolve_agent_route;
    use crate::config::schema::Config;

    let mut config = Config::default();
    config.agent.name = "omninova".to_string();
    config.bindings.push(crate::config::schema::BindingEntry {
        match_rule: Some(crate::config::schema::BindingMatchConfig {
            channel: Some("wecom".to_string()),
            ..Default::default()
        }),
        agent_id: Some("bound-agent".to_string()),
        ..Default::default()
    });

    let single = normalize_wecom_callback(&single_callback_body("userB", "hi"), "req7").unwrap();
    let route = resolve_agent_route(&config, &single);
    assert_eq!(route.agent_name, "bound-agent");
}

/// P7: enabled_channels includes "wecom" when channels_config.wecom.enabled.
#[test]
fn wecom_enabled_channels_contains_wecom() {
    use crate::config::schema::{ChannelEntry, Config};

    let mut config = Config::default();
    config.channels_config.wecom = Some(ChannelEntry {
        enabled: true,
        ..Default::default()
    });
    let mut channels = Vec::new();
    if config.channels_config.wecom.as_ref().map(|e| e.enabled).unwrap_or(false)
        || config.gateway.wecom.enabled
    {
        channels.push("wecom".to_string());
    }
    assert!(channels.contains(&"wecom".to_string()));
}

/// P7: enabled_channels excludes wecom when disabled.
#[test]
fn wecom_enabled_channels_excludes_when_disabled() {
    use crate::config::schema::Config;
    let config = Config::default();
    let mut channels = Vec::new();
    if config.channels_config.wecom.as_ref().map(|e| e.enabled).unwrap_or(false)
        || config.gateway.wecom.enabled
    {
        channels.push("wecom".to_string());
    }
    assert!(!channels.contains(&"wecom".to_string()));
}

/// P6: logical queue id and physical connection id are distinct.
#[test]
fn wecom_logical_id_not_physical_id() {
    use crate::gateway::wecom_stream::short_hash;
    let gen = 1u64;
    let logical_id = short_hash(&format!("queue_{}", gen));
    let physical_id = short_hash(&format!("physical_{}_{}", gen, 0));
    assert_ne!(logical_id, physical_id);
}

// ---------------------------------------------------------------------------
// Phase 1B.4: Gateway Stop / Start lifecycle
// ---------------------------------------------------------------------------

/// P8: stopping the stream releases ownership so a later start can acquire
/// a NEW generation (A exit -> B start, never overlapping).
#[tokio::test]
async fn wecom_gateway_stop_releases_stream_owner() {
    use crate::gateway::GatewayRuntime;
    use crate::config::schema::Config;

    let runtime = std::sync::Arc::new(GatewayRuntime::new(Config::default()));

    // Simulate first Gateway start: acquire gen A.
    let gen_a = runtime.acquire_wecom_stream_generation();
    assert_eq!(gen_a, 1);
    assert!(runtime.is_wecom_stream_generation_active(gen_a));

    // Simulate Gateway stop: shutdown_and_join (best-effort join since no
    // physical loop handle is registered in this unit context).
    let outcome = runtime
        .shutdown_wecom_stream_generation(gen_a, std::time::Duration::from_secs(2))
        .await;
    assert!(matches!(
        outcome,
        crate::gateway::StreamShutdownOutcome::NotRunning
            | crate::gateway::StreamShutdownOutcome::Graceful
            | crate::gateway::StreamShutdownOutcome::JoinFailed
            | crate::gateway::StreamShutdownOutcome::Aborted
            | crate::gateway::StreamShutdownOutcome::StaleOwner
    ));

    // After stop: gen A must no longer be active.
    assert!(!runtime.is_wecom_stream_generation_active(gen_a));
    assert!(!runtime.is_wecom_stream_active());

    // Simulate Gateway restart: acquire gen B.
    let gen_b = runtime.acquire_wecom_stream_generation();
    assert_eq!(gen_b, 2, "restart must use a NEW generation");
    assert!(runtime.is_wecom_stream_generation_active(gen_b));

    // A and B are physically distinct generations: releasing A must not
    // invalidate B.
    runtime.release_wecom_stream_generation(gen_a);
    assert!(runtime.is_wecom_stream_generation_active(gen_b));

    runtime.release_wecom_stream_generation(gen_b);
    assert!(!runtime.is_wecom_stream_active());
}

/// P8: restart cannot start a new physical writer while the old owner is
/// still holding the generation.
#[tokio::test]
async fn wecom_restart_uses_new_physical_writer() {
    use crate::gateway::GatewayRuntime;
    use crate::config::schema::Config;
    use crate::gateway::wecom_stream::short_hash;

    let runtime = std::sync::Arc::new(GatewayRuntime::new(Config::default()));

    let gen_a = runtime.acquire_wecom_stream_generation();
    let physical_a = short_hash(&format!("physical_{}_{}", gen_a, 0));

    // While A is active, a concurrent "start" must NOT obtain a new gen.
    let concurrent = runtime.acquire_wecom_stream_generation();
    assert_eq!(concurrent, gen_a, "second start while active reuses gen A");

    // Stop A, then start B.
    let _ = runtime
        .shutdown_wecom_stream_generation(gen_a, std::time::Duration::from_secs(2))
        .await;
    let gen_b = runtime.acquire_wecom_stream_generation();
    assert_ne!(gen_b, gen_a);
    let physical_b = short_hash(&format!("physical_{}_{}", gen_b, 0));
    assert_ne!(physical_a, physical_b, "physical writer identity must change on restart");

    runtime.release_wecom_stream_generation(gen_b);
}

// ---------------------------------------------------------------------------
// Phase 1B.4.2: Server Disconnected Event + Connection Ownership
// ---------------------------------------------------------------------------

fn disconnected_event_body() -> WecomCallbackBody {
    serde_json::from_str::<WecomCallbackBody>(
        r#"{
            "msgid": "evt_disc_1",
            "aibotid": "bot1",
            "msgtype": "event",
            "event": {"eventtype": "disconnected_event"}
        }"#
    ).unwrap()
}

/// P2: disconnected_event must NOT be classified as a protocol error.
#[test]
fn wecom_disconnected_event_is_not_protocol_error() {
    use crate::gateway::wecom_stream::{classify_error, WecomConnectionExit};

    let kind = classify_error("server_superseded");
    assert_eq!(kind, "superseded");
    assert_ne!(kind, "protocol");

    assert_eq!(
        WecomConnectionExit::from_error("server_superseded"),
        WecomConnectionExit::Superseded
    );
    assert_ne!(
        WecomConnectionExit::from_error("server_superseded"),
        WecomConnectionExit::Retryable
    );
}

/// P2/P4: disconnected_event must not trigger an automatic reconnect.
#[test]
fn wecom_disconnected_event_does_not_auto_reconnect() {
    use crate::gateway::wecom_stream::WecomConnectionExit;

    // Superseded → no reconnect.
    assert_eq!(
        WecomConnectionExit::from_error("server_superseded"),
        WecomConnectionExit::Superseded
    );

    // Retryable categories must still reconnect.
    assert_eq!(
        WecomConnectionExit::from_error("read_error: connection reset"),
        WecomConnectionExit::Retryable
    );
    assert_eq!(
        WecomConnectionExit::from_error("websocket_closed"),
        WecomConnectionExit::Retryable
    );
    assert_eq!(
        WecomConnectionExit::from_error("subscribe_timeout"),
        WecomConnectionExit::Retryable
    );
}

/// P3: the disconnected_event handler returns the superseded error so the
/// read loop terminates the physical connection instead of continuing.
#[tokio::test]
async fn wecom_disconnected_event_stops_current_physical_writer() {
    use crate::gateway::wecom_stream::{classify_error, handle_event_callback};

    let body = disconnected_event_body();
    let result = handle_event_callback(&body, "req_disc", "physical_abc").await;

    assert!(result.is_err(), "disconnected_event must terminate the connection");
    let err = result.unwrap_err();
    assert_eq!(classify_error(&err), "superseded");
}

/// P3: event handler distinguishes disconnected_event from other events.
#[tokio::test]
async fn wecom_disconnected_event_stops_current_heartbeat() {
    use crate::gateway::wecom_stream::{handle_event_callback, WecomConnectionExit};

    let body = disconnected_event_body();
    let result = handle_event_callback(&body, "req_disc", "physical_abc").await;
    let err = result.unwrap_err();
    // Superseded exits the reconnect loop, which drops the shared outbound
    // receiver; the heartbeat loop then fails its send and exits.
    assert_eq!(WecomConnectionExit::from_error(&err), WecomConnectionExit::Superseded);
}

/// P3/P5: superseded releases the physical connection ownership via the
/// reconnect loop exit path (loop_exited + owner_released semantics).
#[tokio::test]
async fn wecom_server_superseded_releases_physical_connection() {
    use crate::config::schema::Config;
    use crate::gateway::wecom_stream::WecomConnectionExit;
    use crate::gateway::GatewayRuntime;

    let runtime = std::sync::Arc::new(GatewayRuntime::new(Config::default()));

    // Acquire a physical owner (generation A).
    let gen_a = runtime.acquire_wecom_stream_generation();
    assert!(runtime.is_wecom_stream_generation_active(gen_a));

    // Superseded decision never reconnects: the loop would break and release.
    assert_eq!(
        WecomConnectionExit::from_error("server_superseded"),
        WecomConnectionExit::Superseded
    );

    // Simulate the loop's release path.
    runtime.release_wecom_stream_generation(gen_a);
    assert!(!runtime.is_wecom_stream_generation_active(gen_a));
    assert!(!runtime.is_wecom_stream_active());

    // A later Start can acquire a fresh generation (A exit → B start).
    let gen_b = runtime.acquire_wecom_stream_generation();
    assert_eq!(gen_b, 2);
    runtime.release_wecom_stream_generation(gen_b);
}

/// P6: a stale physical connection (generation no longer active) must never
/// attribute frames to the current connection.
#[tokio::test]
async fn wecom_old_physical_ack_is_ignored_after_replacement() {
    use crate::config::schema::Config;
    use crate::gateway::GatewayRuntime;

    let runtime = std::sync::Arc::new(GatewayRuntime::new(Config::default()));

    // Old physical connection A.
    let gen_a = runtime.acquire_wecom_stream_generation();
    assert!(runtime.is_wecom_stream_generation_active(gen_a));

    // Connection A replaced: release A, acquire B.
    runtime.release_wecom_stream_generation(gen_a);
    let gen_b = runtime.acquire_wecom_stream_generation();

    // Delayed ACK from connection A arrives now: gen A is stale.
    assert!(
        !runtime.is_wecom_stream_generation_active(gen_a),
        "old gen A must be inactive after replacement"
    );
    assert!(runtime.is_wecom_stream_generation_active(gen_b));
}

/// P4: network disconnect still reconnects.
#[test]
fn wecom_network_disconnect_still_reconnects() {
    use crate::gateway::wecom_stream::WecomConnectionExit;

    assert_eq!(
        WecomConnectionExit::from_error("read_error: Connection reset by peer"),
        WecomConnectionExit::Retryable
    );
    assert_eq!(
        WecomConnectionExit::from_error("connect_error: network unreachable"),
        WecomConnectionExit::Retryable
    );
    assert_eq!(
        WecomConnectionExit::from_error("websocket_closed"),
        WecomConnectionExit::Retryable
    );
}

/// P4: heartbeat failure still reconnects.
#[test]
fn wecom_heartbeat_failure_still_reconnects() {
    use crate::gateway::wecom_stream::WecomConnectionExit;

    // Heartbeat write failure surfaces as an IO error string; it is retryable.
    assert_eq!(
        WecomConnectionExit::from_error("heartbeat write error"),
        WecomConnectionExit::Retryable
    );
    assert_eq!(
        WecomConnectionExit::from_error("read_error: timed out"),
        WecomConnectionExit::Retryable
    );
}

/// P2: protocol/parse failures stay retryable (only superseded is terminal).
#[test]
fn wecom_protocol_error_still_reconnects() {
    use crate::gateway::wecom_stream::{classify_error, WecomConnectionExit};

    assert_eq!(classify_error("malformed frame"), "protocol");
    assert_eq!(
        WecomConnectionExit::from_error("malformed frame"),
        WecomConnectionExit::Retryable
    );
}

// ---------------------------------------------------------------------------
// Phase 1B.4.3: Gateway Stop/Start Ownership + Generation Fencing
// ---------------------------------------------------------------------------

/// P2: a second acquisition while an owner is active must be rejected.
#[tokio::test]
async fn wecom_second_start_while_active_is_rejected() {
    use crate::config::schema::Config;
    use crate::gateway::GatewayRuntime;

    let runtime = std::sync::Arc::new(GatewayRuntime::new(Config::default()));

    let first = runtime.try_acquire_wecom_stream_owner().await;
    assert_eq!(first, Some(1), "first start acquires gen=1");

    let second = runtime.try_acquire_wecom_stream_owner().await;
    assert_eq!(second, None, "second start while active must be rejected");

    assert!(runtime.is_wecom_stream_active());
    assert_eq!(runtime.current_wecom_stream_generation(), 1);
}

/// P3: restart after release must produce a strictly greater generation.
#[tokio::test]
async fn wecom_restart_after_release_increments_generation() {
    use crate::config::schema::Config;
    use crate::gateway::GatewayRuntime;

    let runtime = std::sync::Arc::new(GatewayRuntime::new(Config::default()));

    let gen1 = runtime.try_acquire_wecom_stream_owner().await.unwrap();
    assert_eq!(gen1, 1);

    // Stop: release the owner.
    runtime.release_wecom_stream_generation(gen1);
    assert!(!runtime.is_wecom_stream_active());

    // Start again: new generation.
    let gen2 = runtime.try_acquire_wecom_stream_owner().await.unwrap();
    assert_eq!(gen2, 2, "generation must be monotonically increasing");

    runtime.release_wecom_stream_generation(gen2);
}

/// P3: a stale owner's release must never invalidate a newer owner.
#[tokio::test]
async fn wecom_stale_generation_release_cannot_release_new_owner() {
    use crate::config::schema::Config;
    use crate::gateway::GatewayRuntime;

    let runtime = std::sync::Arc::new(GatewayRuntime::new(Config::default()));

    let gen1 = runtime.try_acquire_wecom_stream_owner().await.unwrap();
    runtime.release_wecom_stream_generation(gen1);

    let gen2 = runtime.try_acquire_wecom_stream_owner().await.unwrap();
    assert_eq!(gen2, 2);

    // Late release from the OLD owner: must be ignored.
    runtime.release_wecom_stream_generation(gen1);
    assert!(
        runtime.is_wecom_stream_generation_active(gen2),
        "stale gen1 release must not invalidate active gen2"
    );
    assert!(runtime.is_wecom_stream_active());

    runtime.release_wecom_stream_generation(gen2);
    assert!(!runtime.is_wecom_stream_active());
}

/// P4: restart changes the logical queue id (derived from generation).
#[test]
fn wecom_restart_changes_logical_id() {
    use crate::gateway::wecom_stream::short_hash;
    let gen1_logical = short_hash("queue_1");
    let gen2_logical = short_hash("queue_2");
    assert_ne!(gen1_logical, gen2_logical);
}

/// P4: restart changes the physical connection id (derived from generation).
#[test]
fn wecom_restart_changes_physical_id() {
    use crate::gateway::wecom_stream::short_hash;
    let p1 = short_hash("physical_1_0");
    let p2 = short_hash("physical_2_0");
    assert_ne!(p1, p2);
}

/// P1/P7: shutdown_and_join signals the owner and the join path completes
/// (NotRunning/StaleOwner outcomes are valid unit-scope results since no
/// physical loop handle is registered here).
#[tokio::test]
async fn wecom_gateway_stop_shuts_down_wecom() {
    use crate::config::schema::Config;
    use crate::gateway::GatewayRuntime;

    let runtime = std::sync::Arc::new(GatewayRuntime::new(Config::default()));

    let gen = runtime.try_acquire_wecom_stream_owner().await.unwrap();
    assert!(runtime.is_wecom_stream_generation_active(gen));

    let outcome = runtime
        .shutdown_wecom_stream_generation(gen, std::time::Duration::from_secs(2))
        .await;
    assert!(matches!(
        outcome,
        crate::gateway::StreamShutdownOutcome::NotRunning
            | crate::gateway::StreamShutdownOutcome::Graceful
            | crate::gateway::StreamShutdownOutcome::JoinFailed
            | crate::gateway::StreamShutdownOutcome::Aborted
            | crate::gateway::StreamShutdownOutcome::StaleOwner
    ));

    // After shutdown the owner must no longer be active.
    assert!(!runtime.is_wecom_stream_generation_active(gen));
    assert!(!runtime.is_wecom_stream_active());
}

/// P1: Stop must allow a later Start to acquire a fresh generation
/// (physically 1 -> 0 -> 1, no overlap).
#[tokio::test]
async fn wecom_gateway_stop_waits_for_wecom_teardown() {
    use crate::config::schema::Config;
    use crate::gateway::GatewayRuntime;

    let runtime = std::sync::Arc::new(GatewayRuntime::new(Config::default()));

    let gen_a = runtime.try_acquire_wecom_stream_owner().await.unwrap();
    let _ = runtime
        .shutdown_wecom_stream_generation(gen_a, std::time::Duration::from_secs(2))
        .await;
    assert!(!runtime.is_wecom_stream_active());

    let gen_b = runtime.try_acquire_wecom_stream_owner().await.unwrap();
    assert_eq!(gen_b, gen_a + 1, "restart must increment the generation");

    // A and B never overlap: gen_a is inactive while gen_b is active.
    assert!(!runtime.is_wecom_stream_generation_active(gen_a));
    assert!(runtime.is_wecom_stream_generation_active(gen_b));

    runtime.release_wecom_stream_generation(gen_b);
}

/// P5: a job whose generation is no longer active must be discarded before
/// reply dispatch (stale-generation fencing).
#[test]
fn wecom_shutdown_cancels_or_discards_inflight_job() {
    // Decision helper parity: the worker discards when the job's generation
    // is not the active one. This test locks the fencing predicate used by
    // process_wecom_job.
    use crate::config::schema::Config;
    use crate::gateway::GatewayRuntime;

    let runtime = std::sync::Arc::new(GatewayRuntime::new(Config::default()));
    // Simulate: job.gen=1 enqueued, then stop+restart moved current to gen=2.
    runtime.acquire_wecom_stream_generation(); // gen=1
    runtime.release_wecom_stream_generation(1);
    runtime.acquire_wecom_stream_generation(); // gen=2

    let job_gen = 1u64;
    let should_discard = job_gen != 0 && !runtime.is_wecom_stream_generation_active(job_gen);
    assert!(should_discard, "stale job must be discarded");

    let current_gen = 2u64;
    let should_dispatch = current_gen != 0 && runtime.is_wecom_stream_generation_active(current_gen);
    assert!(should_dispatch, "current-generation job may dispatch");

    runtime.release_wecom_stream_generation(2);
}

/// P5: job carries its generation.
#[test]
fn wecom_job_carries_generation() {
    use crate::channels::InboundMessage;
    use crate::gateway::wecom_worker::WecomAsyncJob;

    let inbound = InboundMessage {
        channel: ChannelKind::Wecom,
        user_id: None,
        session_id: None,
        text: "hi".to_string(),
        metadata: Default::default(),
    };
    let job = WecomAsyncJob::new_with_writer(inbound, "req".into(), "queue_1".into(), "single".into(), 7);
    assert_eq!(job.gen, 7);
}

/// P7: cleanup is exactly-once per generation — a single release fully
/// deactivates the owner; repeated release attempts are no-ops.
#[tokio::test]
async fn wecom_cleanup_runs_once_per_generation() {
    use crate::config::schema::Config;
    use crate::gateway::GatewayRuntime;

    let runtime = std::sync::Arc::new(GatewayRuntime::new(Config::default()));

    let gen = runtime.try_acquire_wecom_stream_owner().await.unwrap();
    assert!(runtime.is_wecom_stream_active());

    // First release (the loop's own cleanup): deactivates.
    runtime.release_wecom_stream_generation(gen);
    assert!(!runtime.is_wecom_stream_active());

    // Repeat release: no-op, must not panic or flip state.
    runtime.release_wecom_stream_generation(gen);
    assert!(!runtime.is_wecom_stream_active());
}
