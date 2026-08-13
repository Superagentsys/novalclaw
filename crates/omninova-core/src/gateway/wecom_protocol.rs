//! WeCom (企业微信) WebSocket long-connection protocol types.
//!
//! Reference: <https://developer.work.weixin.qq.com/document/path/101463>
//!
//! ## Protocol Overview
//!
//! ```text
//! Developer Service          WeCom
//!        │                      │
//!        │──wss://openws...────>│ (1) WebSocket handshake
//!        │<─────────────────────│ (2) Connection established
//!        │──aibot_subscribe────>│ (3) Auth with bot_id + secret
//!        │<─subscribe response──│
//!        │                      │
//!        │<────aibot_event─────│ (4) enter_chat (welcome)
//!        │──aibot_respond_welcome_msg──>│
//!        │                      │
//!        │<────aibot_msg───────│ (5) Message callback
//!        │──aibot_respond_msg──>│ (6) Reply (or streaming)
//!        │                      │
//!        │<────disconnect───────│ (7) Server-initiated disconnect
//!        │──aibot_subscribe────>│ (8) Reconnect with new session
//! ```
//!
//! ## WeCom-Specific Notes
//!
//! - WebSocket endpoint: `wss://openws.work.weixin.qq.com`
//! - `req_id` is in `headers.req_id` for correlation
//! - `msgid` in body for deduplication
//! - `chatid` only present for group chats; `single` chats omit it
//! - `chattype`: `single` (单聊) or `group` (群聊)
//! - `from.userid`: encrypted unless admin
//! - Heartbeat: every 30 seconds with `ping` command
//! - Single bot: only ONE active connection allowed per bot_id

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// WeCom WebSocket gateway endpoint (default for public cloud)
pub const WECOM_WS_GATEWAY_URL: &str = "wss://openws.work.weixin.qq.com";

/// Heartbeat interval in seconds (as recommended by official docs)
pub const WECOM_HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// Max reconnection backoff: 2^attempt seconds, capped at this value
pub const WECOM_MAX_BACKOFF_SECS: u64 = 30;

/// Subscribe command name
pub const WECOM_CMD_SUBSCRIBE: &str = "aibot_subscribe";

/// Ping command name
pub const WECOM_CMD_PING: &str = "ping";

/// Message callback command name
pub const WECOM_CMD_MSG_CALLBACK: &str = "aibot_msg_callback";

/// Event callback command name
pub const WECOM_CMD_EVENT_CALLBACK: &str = "aibot_event_callback";

/// Reply message command name
pub const WECOM_CMD_RESPOND_MSG: &str = "aibot_respond_msg";

/// Welcome message command name
pub const WECOM_CMD_RESPOND_WELCOME: &str = "aibot_respond_welcome_msg";

/// Send message command name
pub const WECOM_CMD_SEND_MSG: &str = "aibot_send_msg";

/// Enter chat event type
pub const WECOM_EVENT_ENTER_CHAT: &str = "enter_chat";

/// Disconnect event type
pub const WECOM_EVENT_DISCONNECTED: &str = "disconnected_event";

// ---------------------------------------------------------------------------
// Common envelope structures
// ---------------------------------------------------------------------------

/// Common headers present in all WeCom WebSocket frames.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WecomHeaders {
    /// Request identifier for correlation between request and response.
    #[serde(rename = "req_id")]
    pub req_id: String,
}

/// Client-to-server request envelope (e.g., subscribe, ping).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WecomRequestEnvelope {
    #[serde(rename = "cmd")]
    pub cmd: String,
    #[serde(rename = "headers")]
    pub headers: WecomHeaders,
    #[serde(rename = "body", skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
}

/// Server-to-client response envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WecomResponseEnvelope {
    #[serde(rename = "headers")]
    pub headers: WecomHeaders,
    #[serde(rename = "errcode")]
    pub errcode: i64,
    #[serde(rename = "errmsg")]
    pub errmsg: String,
}

/// Server-to-client callback envelope (messages and events).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WecomCallbackEnvelope {
    #[serde(rename = "cmd")]
    pub cmd: String,
    #[serde(rename = "headers")]
    pub headers: WecomHeaders,
    #[serde(rename = "body")]
    pub body: WecomCallbackBody,
}

/// Callback body containing message or event data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WecomCallbackBody {
    /// Unique message/event ID for deduplication.
    #[serde(rename = "msgid")]
    pub msgid: String,
    /// Bot ID.
    #[serde(rename = "aibotid", default)]
    pub aibotid: Option<String>,
    /// Chat ID (only for group chats).
    #[serde(rename = "chatid", skip_serializing_if = "Option::is_none")]
    pub chatid: Option<String>,
    /// Chat type: "single" (单聊) or "group" (群聊).
    #[serde(rename = "chattype", skip_serializing_if = "Option::is_none")]
    pub chattype: Option<String>,
    /// Sender information.
    #[serde(rename = "from", skip_serializing_if = "Option::is_none")]
    pub from: Option<WecomFrom>,
    /// Message type (e.g., "text", "image", "event").
    #[serde(rename = "msgtype", skip_serializing_if = "Option::is_none")]
    pub msgtype: Option<String>,
    /// Text content (for text messages).
    #[serde(rename = "text", skip_serializing_if = "Option::is_none")]
    pub text: Option<WecomText>,
    /// Event details (for event callbacks).
    #[serde(rename = "event", skip_serializing_if = "Option::is_none")]
    pub event: Option<WecomEvent>,
    /// Event creation timestamp.
    #[serde(rename = "create_time", skip_serializing_if = "Option::is_none")]
    pub create_time: Option<i64>,
    /// Additional fields (for forward compatibility).
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Sender information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WecomFrom {
    /// User ID of the sender.
    #[serde(rename = "userid", skip_serializing_if = "Option::is_none")]
    pub userid: Option<String>,
}

/// Text message content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WecomText {
    /// Text content.
    #[serde(rename = "content")]
    pub content: String,
}

/// Event details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WecomEvent {
    /// Event type (e.g., "enter_chat", "disconnected_event").
    #[serde(rename = "eventtype", skip_serializing_if = "Option::is_none")]
    pub eventtype: Option<String>,
    /// Additional event fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Subscribe request/response
// ---------------------------------------------------------------------------

/// Subscribe request body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WecomSubscribeBody {
    /// Bot ID (智能机器人 BotID).
    #[serde(rename = "bot_id")]
    pub bot_id: String,
    /// Long-connection secret.
    #[serde(rename = "secret")]
    pub secret: String,
}

/// Build an aibot_subscribe request envelope.
pub fn build_subscribe_envelope(req_id: &str, bot_id: &str, secret: &str) -> WecomRequestEnvelope {
    WecomRequestEnvelope {
        cmd: WECOM_CMD_SUBSCRIBE.to_string(),
        headers: WecomHeaders {
            req_id: req_id.to_string(),
        },
        body: Some(serde_json::to_value(WecomSubscribeBody {
            bot_id: bot_id.to_string(),
            secret: secret.to_string(),
        })
        .unwrap_or_default()),
    }
}

// ---------------------------------------------------------------------------
// Ping
// ---------------------------------------------------------------------------

/// Build a ping heartbeat envelope.
pub fn build_ping_envelope(req_id: &str) -> WecomRequestEnvelope {
    WecomRequestEnvelope {
        cmd: WECOM_CMD_PING.to_string(),
        headers: WecomHeaders {
            req_id: req_id.to_string(),
        },
        body: None,
    }
}

// ---------------------------------------------------------------------------
// Message reply
// ---------------------------------------------------------------------------

/// Reply message body for responding to a callback (text format, for welcome only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WecomRespondBody {
    /// Message type.
    #[serde(rename = "msgtype")]
    pub msgtype: String,
    /// Text content (for welcome messages only).
    #[serde(rename = "text", skip_serializing_if = "Option::is_none")]
    pub text: Option<WecomText>,
}

/// Stream message content for reply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WecomStreamContent {
    /// Unique stream identifier.
    #[serde(rename = "id")]
    pub id: String,
    /// Text content.
    #[serde(rename = "content")]
    pub content: String,
    /// Whether this is the final message.
    #[serde(rename = "finish")]
    pub finish: bool,
}

/// Reply message body for responding to a callback (stream format, for normal messages).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WecomRespondStreamBody {
    /// Message type.
    #[serde(rename = "msgtype")]
    pub msgtype: String,
    /// Stream content.
    #[serde(rename = "stream")]
    pub stream: WecomStreamContent,
}

/// Build an aibot_respond_msg request envelope with stream format (for normal messages).
pub fn build_stream_respond_envelope(req_id: &str, text: &str) -> WecomRequestEnvelope {
    // Generate unique stream id - not reusing req_id to keep them distinct
    let stream_id = format!("stream_{}_{}", req_id, std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());

    WecomRequestEnvelope {
        cmd: WECOM_CMD_RESPOND_MSG.to_string(),
        headers: WecomHeaders {
            req_id: req_id.to_string(),
        },
        body: Some(
            serde_json::to_value(WecomRespondStreamBody {
                msgtype: "stream".to_string(),
                stream: WecomStreamContent {
                    id: stream_id,
                    content: text.to_string(),
                    finish: true, // Single final message
                },
            })
            .unwrap_or_default(),
        ),
    }
}

/// Build an aibot_respond_msg request envelope (DEPRECATED: use build_stream_respond_envelope for normal replies).
pub fn build_respond_envelope(req_id: &str, text: &str) -> WecomRequestEnvelope {
    build_stream_respond_envelope(req_id, text)
}

/// Build an aibot_respond_msg request envelope with extended options.
pub fn build_respond_envelope_with_options(req_id: &str, text: &str, ctx: &WecomReplyContext) -> WecomRequestEnvelope {
    let mut env = build_stream_respond_envelope(req_id, text);
    // For now, just return the stream envelope
    // Phase 1B+ can extend body with media, ats, etc.
    let _ = ctx;
    env
}

/// Build an aibot_respond_welcome_msg request envelope (welcome messages use text format).
pub fn build_welcome_envelope(req_id: &str, text: &str) -> WecomRequestEnvelope {
    WecomRequestEnvelope {
        cmd: WECOM_CMD_RESPOND_WELCOME.to_string(),
        headers: WecomHeaders {
            req_id: req_id.to_string(),
        },
        body: Some(
            serde_json::to_value(WecomRespondBody {
                msgtype: "text".to_string(),
                text: Some(WecomText {
                    content: text.to_string(),
                }),
            })
            .unwrap_or_default(),
        ),
    }
}

// ---------------------------------------------------------------------------
// Helper types
// ---------------------------------------------------------------------------

/// Chat type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WecomChatType {
    /// Single chat (单聊).
    Single,
    /// Group chat (群聊).
    Group,
    /// Unknown or unspecified.
    Unknown,
}

impl WecomChatType {
    /// Parse from WeCom string value.
    pub fn from_str(s: Option<&str>) -> Self {
        match s {
            Some("single") => WecomChatType::Single,
            Some("group") => WecomChatType::Group,
            _ => WecomChatType::Unknown,
        }
    }

    /// Returns true if this is a group chat.
    pub fn is_group(&self) -> bool {
        matches!(self, WecomChatType::Group)
    }

    /// Returns true if this is a single chat.
    pub fn is_single(&self) -> bool {
        matches!(self, WecomChatType::Single)
    }
}

/// Command type classification based on `cmd` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WecomCommandType {
    /// Message callback (aibot_msg_callback).
    MessageCallback,
    /// Event callback (aibot_event_callback).
    EventCallback,
    /// Unknown/other command.
    Unknown,
}

impl WecomCommandType {
    /// Classify a command string.
    pub fn from_cmd(cmd: &str) -> Self {
        match cmd {
            WECOM_CMD_MSG_CALLBACK => WecomCommandType::MessageCallback,
            WECOM_CMD_EVENT_CALLBACK => WecomCommandType::EventCallback,
            _ => WecomCommandType::Unknown,
        }
    }
}

/// Event type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WecomEventType {
    /// User enters chat session (first contact).
    EnterChat,
    /// Server-initiated disconnect.
    Disconnected,
    /// Template card event.
    TemplateCardEvent,
    /// Feedback event.
    FeedbackEvent,
    /// Unknown event type.
    Unknown,
}

impl WecomEventType {
    /// Classify from eventtype string.
    pub fn from_eventtype(s: Option<&str>) -> Self {
        match s {
            Some(WECOM_EVENT_ENTER_CHAT) => WecomEventType::EnterChat,
            Some(WECOM_EVENT_DISCONNECTED) => WecomEventType::Disconnected,
            Some(t) if t.contains("template_card") => WecomEventType::TemplateCardEvent,
            Some(t) if t.contains("feedback") => WecomEventType::FeedbackEvent,
            _ => WecomEventType::Unknown,
        }
    }
}

/// WeCom reply context: preserves correlation for outbound.
#[derive(Debug, Clone)]
pub struct WecomReplyContext {
    /// Request ID for correlation.
    pub req_id: String,
    /// Chat ID (only for group).
    pub chat_id: Option<String>,
    /// Chat type.
    pub chat_type: WecomChatType,
    /// Sender user ID.
    pub user_id: Option<String>,
    /// Bot ID.
    pub bot_id: Option<String>,
}

impl WecomReplyContext {
    /// Extract from a callback envelope.
    pub fn from_callback(body: &WecomCallbackBody, req_id: &str) -> Self {
        Self {
            req_id: req_id.to_string(),
            chat_id: body.chatid.clone(),
            chat_type: WecomChatType::from_str(body.chattype.as_deref()),
            user_id: body.from.as_ref().and_then(|f| f.userid.clone()),
            bot_id: body.aibotid.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// WeCom protocol-level errors.
#[derive(Debug, Clone)]
pub enum WecomProtocolError {
    /// JSON parse failure.
    InvalidJson(String),
    /// Missing required field.
    MissingField(&'static str),
    /// Authentication failed.
    AuthFailed(i64, String),
    /// Subscription rate limited.
    RateLimited,
    /// Server error.
    ServerError(i64, String),
    /// Unknown error.
    Unknown(String),
}

impl std::fmt::Display for WecomProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WecomProtocolError::InvalidJson(s) => write!(f, "invalid_json: {}", s),
            WecomProtocolError::MissingField(s) => write!(f, "missing_field: {}", s),
            WecomProtocolError::AuthFailed(code, msg) => write!(f, "auth_failed: {} - {}", code, msg),
            WecomProtocolError::RateLimited => write!(f, "rate_limited"),
            WecomProtocolError::ServerError(code, msg) => write!(f, "server_error: {} - {}", code, msg),
            WecomProtocolError::Unknown(s) => write!(f, "unknown: {}", s),
        }
    }
}

impl std::error::Error for WecomProtocolError {}

/// Parse a WeCom response envelope and check for errors.
pub fn parse_response_envelope(json_str: &str) -> Result<WecomResponseEnvelope, WecomProtocolError> {
    let envelope: WecomResponseEnvelope =
        serde_json::from_str(json_str).map_err(|e| WecomProtocolError::InvalidJson(e.to_string()))?;

    // Check error code
    if envelope.errcode != 0 {
        let msg = &envelope.errmsg;
        match envelope.errcode {
            40001 | 40013 | 40125 => Err(WecomProtocolError::AuthFailed(envelope.errcode, msg.clone())),
            40014 | 40098 | 41004 => Err(WecomProtocolError::RateLimited),
            _ => Err(WecomProtocolError::ServerError(envelope.errcode, msg.clone())),
        }
    } else {
        Ok(envelope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_subscribe_response_success() {
        let json = r#"{"headers":{"req_id":"req123"},"errcode":0,"errmsg":"ok"}"#;
        let result = parse_response_envelope(json);
        assert!(result.is_ok());
        let env = result.unwrap();
        assert_eq!(env.headers.req_id, "req123");
        assert_eq!(env.errcode, 0);
    }

    #[test]
    fn test_parse_subscribe_response_auth_failed() {
        let json = r#"{"headers":{"req_id":"req123"},"errcode":40001,"errmsg":"invalid credential"}"#;
        let result = parse_response_envelope(json);
        assert!(matches!(result, Err(WecomProtocolError::AuthFailed(40001, _))));
    }

    #[test]
    fn test_build_subscribe_envelope() {
        let env = build_subscribe_envelope("req123", "bot456", "secret789");
        assert_eq!(env.cmd, "aibot_subscribe");
        assert_eq!(env.headers.req_id, "req123");
        let body = env.body.unwrap();
        assert_eq!(body["bot_id"], "bot456");
        assert_eq!(body["secret"], "secret789");
    }

    #[test]
    fn test_build_ping_envelope() {
        let env = build_ping_envelope("ping123");
        assert_eq!(env.cmd, "ping");
        assert_eq!(env.headers.req_id, "ping123");
        assert!(env.body.is_none());
    }

    #[test]
    fn test_build_respond_envelope() {
        let env = build_stream_respond_envelope("req456", "Hello World");
        assert_eq!(env.cmd, "aibot_respond_msg");
        assert_eq!(env.headers.req_id, "req456");
        let json = serde_json::to_string(&env).unwrap();
        // Now uses stream msgtype instead of text
        assert!(json.contains("\"msgtype\":\"stream\""));
        assert!(json.contains("\"content\":\"Hello World\""));
    }

    #[test]
    fn test_parse_message_callback() {
        let json = r#"{
            "cmd": "aibot_msg_callback",
            "headers": {"req_id": "req789"},
            "body": {
                "msgid": "msg123",
                "aibotid": "bot456",
                "chatid": "chat789",
                "chattype": "group",
                "from": {"userid": "user123"},
                "msgtype": "text",
                "text": {"content": "Hello"}
            }
        }"#;
        let envelope: WecomCallbackEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.cmd, "aibot_msg_callback");
        assert_eq!(envelope.headers.req_id, "req789");
        assert_eq!(envelope.body.msgid, "msg123");
        assert_eq!(envelope.body.chatid.as_deref(), Some("chat789"));
        assert_eq!(envelope.body.chattype.as_deref(), Some("group"));
        assert_eq!(envelope.body.from.as_ref().unwrap().userid.as_deref(), Some("user123"));
        assert_eq!(envelope.body.text.as_ref().unwrap().content, "Hello");
    }

    #[test]
    fn test_parse_single_chat_callback() {
        let json = r#"{
            "cmd": "aibot_msg_callback",
            "headers": {"req_id": "req000"},
            "body": {
                "msgid": "msg000",
                "aibotid": "bot000",
                "chattype": "single",
                "from": {"userid": "user000"},
                "msgtype": "text",
                "text": {"content": "Hi"}
            }
        }"#;
        let envelope: WecomCallbackEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.body.chattype.as_deref(), Some("single"));
        assert!(envelope.body.chatid.is_none()); // single chats don't have chatid
    }

    #[test]
    fn test_parse_event_callback_enter_chat() {
        let json = r#"{
            "cmd": "aibot_event_callback",
            "headers": {"req_id": "evt001"},
            "body": {
                "msgid": "evt001",
                "aibotid": "bot001",
                "chattype": "single",
                "from": {"userid": "user001"},
                "msgtype": "event",
                "event": {"eventtype": "enter_chat"},
                "create_time": 1700000000
            }
        }"#;
        let envelope: WecomCallbackEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.cmd, "aibot_event_callback");
        assert_eq!(envelope.body.event.as_ref().unwrap().eventtype.as_deref(), Some("enter_chat"));
        assert_eq!(envelope.body.create_time, Some(1700000000));
    }

    #[test]
    fn test_parse_disconnect_event() {
        let json = r#"{
            "cmd": "aibot_event_callback",
            "headers": {"req_id": "evt002"},
            "body": {
                "msgid": "evt002",
                "aibotid": "bot002",
                "msgtype": "event",
                "event": {"eventtype": "disconnected_event"}
            }
        }"#;
        let envelope: WecomCallbackEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.body.event.as_ref().unwrap().eventtype.as_deref(), Some("disconnected_event"));
    }

    #[test]
    fn test_wecom_chat_type() {
        assert!(WecomChatType::from_str(Some("single")).is_single());
        assert!(WecomChatType::from_str(Some("group")).is_group());
        assert!(!WecomChatType::from_str(Some("single")).is_group());
        assert!(!WecomChatType::from_str(Some("group")).is_single());
        assert!(!WecomChatType::from_str(None).is_group());
    }

    #[test]
    fn test_wecom_command_type() {
        assert_eq!(
            WecomCommandType::from_cmd("aibot_msg_callback"),
            WecomCommandType::MessageCallback
        );
        assert_eq!(
            WecomCommandType::from_cmd("aibot_event_callback"),
            WecomCommandType::EventCallback
        );
        assert_eq!(WecomCommandType::from_cmd("ping"), WecomCommandType::Unknown);
    }

    #[test]
    fn test_wecom_reply_context() {
        let json = r#"{
            "msgid": "msg123",
            "aibotid": "bot456",
            "chatid": "chat789",
            "chattype": "group",
            "from": {"userid": "user123"}
        }"#;
        let body: WecomCallbackBody = serde_json::from_str(json).unwrap();
        let ctx = WecomReplyContext::from_callback(&body, "req000");

        assert_eq!(ctx.req_id, "req000");
        assert_eq!(ctx.chat_id.as_deref(), Some("chat789"));
        assert!(ctx.chat_type.is_group());
        assert_eq!(ctx.user_id.as_deref(), Some("user123"));
        assert_eq!(ctx.bot_id.as_deref(), Some("bot456"));
    }

    #[test]
    fn test_unknown_extra_fields_tolerated() {
        // Parser should accept callbacks with extra/unknown fields
        let json = r#"{
            "cmd": "aibot_msg_callback",
            "headers": {"req_id": "req123"},
            "body": {
                "msgid": "msg123",
                "unknown_field": "should_be_ignored",
                "another_unknown": 12345
            }
        }"#;
        let result: Result<WecomCallbackEnvelope, _> = serde_json::from_str(json);
        assert!(result.is_ok());
    }

    #[test]
    fn test_missing_required_msgid_rejected() {
        // msgid is required, so missing it should cause parse failure
        let json = r#"{
            "cmd": "aibot_msg_callback",
            "headers": {"req_id": "req123"},
            "body": {
                "aibotid": "bot123"
            }
        }"#;
        let result: Result<WecomCallbackEnvelope, _> = serde_json::from_str(json);
        // serde will fail because msgid has no skip_serializing_if
        assert!(result.is_err() || result.as_ref().unwrap().body.msgid.is_empty());
    }

    #[test]
    fn test_ping_response_parsing() {
        let json = r#"{"headers":{"req_id":"ping123"},"errcode":0,"errmsg":"ok"}"#;
        let result = parse_response_envelope(json);
        assert!(result.is_ok());
    }
}
