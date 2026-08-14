//! WeCom inbound normalization.
//!
//! Converts WeCom protocol callbacks to the OmniNova `InboundMessage` contract.

use crate::channels::{ChannelKind, InboundMessage};
use crate::gateway::wecom_protocol::{
    WecomCallbackBody, WecomChatType, WecomReplyContext,
};

/// Normalize a WeCom callback to `InboundMessage`.
///
/// Returns `Ok(InboundMessage)` for valid text messages,
/// `Ok(Default)` for non-text callbacks (handled separately by the stream).
pub fn normalize_wecom_callback(
    body: &WecomCallbackBody,
    req_id: &str,
) -> Result<InboundMessage, String> {
    // Only handle text messages for now
    let msgtype = body.msgtype.as_deref().unwrap_or("");

    if msgtype == "text" {
        let text = body
            .text
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_default();

        let user_id = body.from.as_ref().and_then(|f| f.userid.clone());

        // Session identity with namespace prefix:
        // - Group: wecom:group:<chatid> (unique per group conversation)
        // - Single: wecom:single:<user_id> (per-user session)
        let chat_type = WecomChatType::from_str(body.chattype.as_deref());
        let session_id = if chat_type.is_group() {
            let chat_id = body.chatid.as_deref().unwrap_or("unknown");
            format!("wecom:group:{}", chat_id)
        } else {
            let user = user_id.as_deref().unwrap_or("unknown");
            format!("wecom:single:{}", user)
        };

        // Build metadata
        let mut metadata = std::collections::HashMap::new();

        // WeCom-specific fields (msgid is required, others are optional)
        metadata.insert("wecom_msgid".to_string(), serde_json::json!(&body.msgid));
        if let Some(ref id) = body.aibotid {
            metadata.insert("wecom_aibotid".to_string(), serde_json::json!(id));
        }
        if let Some(ref id) = body.chatid {
            metadata.insert("wecom_chatid".to_string(), serde_json::json!(id));
        }
        if let Some(ref ct) = body.chattype {
            metadata.insert("wecom_chattype".to_string(), serde_json::json!(ct));
        }
        if let Some(ref uid) = user_id {
            metadata.insert("wecom_userid".to_string(), serde_json::json!(uid));
        }
        if let Some(ref uid) = body.from.as_ref().and_then(|f| f.userid.clone()) {
            metadata.insert("wecom_from_userid".to_string(), serde_json::json!(uid));
        }

        // Reply context preserved for outbound
        let reply_ctx = WecomReplyContext::from_callback(body, req_id);
        metadata.insert(
            "wecom_req_id".to_string(),
            serde_json::json!(reply_ctx.req_id),
        );
        metadata.insert(
            "wecom_chat_type".to_string(),
            serde_json::json!(if reply_ctx.chat_type.is_group() {
                "group"
            } else {
                "single"
            }),
        );

        Ok(InboundMessage {
            channel: ChannelKind::Wecom,
            user_id,
            session_id: Some(session_id),
            text,
            metadata,
        })
    } else {
        // Non-text messages: return empty message with channel kind
        // (These are logged but not processed in Phase 1A)
        Ok(InboundMessage {
            channel: ChannelKind::Wecom,
            user_id: body.from.as_ref().and_then(|f| f.userid.clone()),
            session_id: None,
            text: String::new(),
            metadata: {
                let mut m = std::collections::HashMap::new();
                m.insert("wecom_msgtype".to_string(), serde_json::json!(msgtype));
                m.insert("wecom_msgid".to_string(), serde_json::json!(&body.msgid));
                m.insert("wecom_req_id".to_string(), serde_json::json!(req_id));
                m
            },
        })
    }
}

/// Get the WeCom reply context from an InboundMessage.
pub fn get_wecom_reply_context(inbound: &InboundMessage) -> Option<WecomReplyContext> {
    let req_id = inbound
        .metadata
        .get("wecom_req_id")?
        .as_str()?
        .to_string();

    let chat_id = inbound
        .metadata
        .get("wecom_chatid")
        .and_then(|v| v.as_str())
        .map(String::from);

    let chat_type_str = inbound
        .metadata
        .get("wecom_chat_type")
        .and_then(|v| v.as_str())
        .unwrap_or("single");

    let chat_type = if chat_type_str == "group" {
        WecomChatType::Group
    } else {
        WecomChatType::Single
    };

    let user_id = inbound.user_id.clone();
    let bot_id = inbound
        .metadata
        .get("wecom_aibotid")
        .and_then(|v| v.as_str())
        .map(String::from);

    Some(WecomReplyContext {
        req_id,
        chat_id,
        chat_type,
        user_id,
        bot_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_group_text_message() {
        let body = serde_json::from_str::<WecomCallbackBody>(
            r#"{
                "msgid": "msg123",
                "aibotid": "bot456",
                "chatid": "chat789",
                "chattype": "group",
                "from": {"userid": "user123"},
                "msgtype": "text",
                "text": {"content": "Hello group"}
            }"#
        ).unwrap();

        let inbound = normalize_wecom_callback(&body, "req000").unwrap();

        assert_eq!(inbound.channel, ChannelKind::Wecom);
        assert_eq!(inbound.text, "Hello group");
        assert_eq!(inbound.session_id.as_deref(), Some("wecom:group:chat789"));
        assert_eq!(inbound.user_id.as_deref(), Some("user123"));
        assert_eq!(inbound.metadata.get("wecom_msgid").and_then(|v| v.as_str()), Some("msg123"));
        assert_eq!(inbound.metadata.get("wecom_chattype").and_then(|v| v.as_str()), Some("group"));
        assert!(inbound.metadata.get("wecom_req_id").is_some());
    }

    #[test]
    fn test_normalize_single_text_message() {
        let body = serde_json::from_str::<WecomCallbackBody>(
            r#"{
                "msgid": "msg456",
                "aibotid": "bot789",
                "chattype": "single",
                "from": {"userid": "user456"},
                "msgtype": "text",
                "text": {"content": "Hello single"}
            }"#
        ).unwrap();

        let inbound = normalize_wecom_callback(&body, "req111").unwrap();

        assert_eq!(inbound.channel, ChannelKind::Wecom);
        assert_eq!(inbound.text, "Hello single");
        // Single chat: session_id = wecom:single:<user_id>
        assert_eq!(inbound.session_id.as_deref(), Some("wecom:single:user456"));
        assert!(inbound.metadata.get("wecom_chatid").is_none()); // single chats don't have chatid
    }

    #[test]
    fn test_normalize_non_text_message() {
        let body = serde_json::from_str::<WecomCallbackBody>(
            r#"{
                "msgid": "msg789",
                "aibotid": "bot000",
                "msgtype": "image",
                "from": {"userid": "user789"}
            }"#
        ).unwrap();

        let inbound = normalize_wecom_callback(&body, "req222").unwrap();

        assert_eq!(inbound.channel, ChannelKind::Wecom);
        assert!(inbound.text.is_empty()); // non-text has empty text
        assert_eq!(inbound.metadata.get("wecom_msgtype").and_then(|v| v.as_str()), Some("image"));
    }

    #[test]
    fn test_session_identity_stable() {
        // Same chat should produce same session_id
        let body1 = serde_json::from_str::<WecomCallbackBody>(
            r#"{
                "msgid": "msg1",
                "chatid": "chat123",
                "chattype": "group",
                "from": {"userid": "user1"},
                "msgtype": "text",
                "text": {"content": "First"}
            }"#
        ).unwrap();

        let body2 = serde_json::from_str::<WecomCallbackBody>(
            r#"{
                "msgid": "msg2",
                "chatid": "chat123",
                "chattype": "group",
                "from": {"userid": "user2"},
                "msgtype": "text",
                "text": {"content": "Second"}
            }"#
        ).unwrap();

        let inbound1 = normalize_wecom_callback(&body1, "req1").unwrap();
        let inbound2 = normalize_wecom_callback(&body2, "req2").unwrap();

        assert_eq!(inbound1.session_id, inbound2.session_id);
    }

    #[test]
    fn test_reply_context_preserved() {
        let body = serde_json::from_str::<WecomCallbackBody>(
            r#"{
                "msgid": "msg000",
                "aibotid": "bot111",
                "chatid": "chat222",
                "chattype": "group",
                "from": {"userid": "user333"},
                "msgtype": "text",
                "text": {"content": "Test"}
            }"#
        ).unwrap();

        let inbound = normalize_wecom_callback(&body, "req_preserve").unwrap();
        let ctx = get_wecom_reply_context(&inbound).unwrap();

        assert_eq!(ctx.req_id, "req_preserve");
        assert_eq!(ctx.chat_id.as_deref(), Some("chat222"));
        assert!(ctx.chat_type.is_group());
        assert_eq!(ctx.user_id.as_deref(), Some("user333"));
        assert_eq!(ctx.bot_id.as_deref(), Some("bot111"));
    }
}
