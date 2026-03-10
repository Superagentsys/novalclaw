use crate::channels::{ChannelKind, InboundMessage};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::collections::HashMap;

pub fn verification_response(payload: &Value) -> Option<Value> {
    let challenge = payload.get("challenge").and_then(Value::as_str)?;
    Some(json!({ "challenge": challenge }))
}

pub fn inbound_from_platform_webhook(channel: ChannelKind, payload: Value) -> Result<InboundMessage> {
    let event = payload.get("event").unwrap_or(&payload);

    let text = extract_text(event).or_else(|| extract_text(&payload));
    let Some(text) = text else {
        bail!("channel webhook payload does not contain a text message")
    };

    let user_id = extract_user_id(event).or_else(|| extract_user_id(&payload));
    let session_id = extract_session_id(event).or_else(|| extract_session_id(&payload));

    let mut metadata = HashMap::new();
    metadata.insert("raw_payload".to_string(), payload.clone());

    if let Some(source) = source_name(&channel) {
        metadata.insert("source".to_string(), Value::String(source.to_string()));
    }

    for (key, value) in extract_known_metadata(event) {
        metadata.insert(key, value);
    }
    for (key, value) in extract_known_metadata(&payload) {
        metadata.entry(key).or_insert(value);
    }

    Ok(InboundMessage {
        channel,
        user_id,
        session_id,
        text,
        metadata,
    })
}

fn extract_text(value: &Value) -> Option<String> {
    first_string(value, &["text", "message", "content"])
        .or_else(|| nested_string(value, &[&["message", "text"], &["content", "text"], &["event", "text"]]))
        .or_else(|| extract_text_from_content_string(value))
}

fn extract_text_from_content_string(value: &Value) -> Option<String> {
    let raw = nested_value(value, &["message", "content"])
        .or_else(|| nested_value(value, &["content"]))
        .and_then(Value::as_str)?;

    if raw.trim().is_empty() {
        return None;
    }

    if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
        return first_string(&parsed, &["text"])
            .or_else(|| nested_string(&parsed, &[&["post", "zh_cn", "title"]]));
    }

    Some(raw.to_string())
}

fn extract_user_id(value: &Value) -> Option<String> {
    first_string(value, &["user_id", "sender_id", "from_user", "from"])
        .or_else(|| nested_string(value, &[&["sender", "id"], &["sender", "open_id"], &["sender", "union_id"], &["sender", "user_id"]]))
        .or_else(|| nested_string(value, &[&["operator", "union_id"], &["operator", "staff_id"]]))
}

fn extract_session_id(value: &Value) -> Option<String> {
    first_string(
        value,
        &[
            "session_id",
            "chat_id",
            "conversation_id",
            "open_chat_id",
            "room_id",
            "thread_id",
            "message_id",
        ],
    )
    .or_else(|| nested_string(value, &[&["message", "chat_id"], &["message", "conversation_id"], &["sender", "chat_id"]]))
}

fn extract_known_metadata(value: &Value) -> HashMap<String, Value> {
    let mut metadata = HashMap::new();
    let pairs = [
        ("tenant_key", first_value(value, &["tenant_key"])),
        ("app_id", first_value(value, &["app_id"])),
        ("open_id", first_value(value, &["open_id"])),
        ("union_id", first_value(value, &["union_id"])),
        ("chat_id", first_value(value, &["chat_id"])),
        ("conversation_id", first_value(value, &["conversation_id"])),
        ("message_id", first_value(value, &["message_id"])),
        ("event_type", first_value(value, &["event_type", "type"])),
    ];

    for (key, maybe_value) in pairs {
        if let Some(value) = maybe_value {
            metadata.insert(key.to_string(), value);
        }
    }

    metadata
}

fn source_name(channel: &ChannelKind) -> Option<&'static str> {
    match channel {
        ChannelKind::Wechat => Some("wechat"),
        ChannelKind::Feishu => Some("feishu"),
        ChannelKind::Lark => Some("lark"),
        ChannelKind::Dingtalk => Some("dingtalk"),
        _ => None,
    }
}

fn first_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(ToString::to_string)
}

fn first_value(value: &Value, keys: &[&str]) -> Option<Value> {
    keys.iter().find_map(|key| value.get(*key)).cloned()
}

fn nested_string(value: &Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter()
        .find_map(|path| nested_value(value, path).and_then(Value::as_str))
        .map(ToString::to_string)
}

fn nested_value<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::inbound_from_platform_webhook;
    use crate::channels::ChannelKind;
    use serde_json::json;

    #[test]
    fn parses_feishu_event_payload() {
        let inbound = inbound_from_platform_webhook(
            ChannelKind::Feishu,
            json!({
                "event": {
                    "sender": { "open_id": "ou_123" },
                    "message": {
                        "chat_id": "oc_456",
                        "message_id": "om_789",
                        "content": "{\"text\":\"hello from feishu\"}"
                    }
                },
                "tenant_key": "tenant-1",
                "type": "im.message.receive_v1"
            }),
        )
        .expect("feishu payload should parse");

        assert_eq!(inbound.user_id.as_deref(), Some("ou_123"));
        assert_eq!(inbound.session_id.as_deref(), Some("oc_456"));
        assert_eq!(inbound.text, "hello from feishu");
    }

    #[test]
    fn parses_normalized_wechat_payload() {
        let inbound = inbound_from_platform_webhook(
            ChannelKind::Wechat,
            json!({
                "text": "hello from wechat",
                "user_id": "wx-user",
                "conversation_id": "room-1"
            }),
        )
        .expect("wechat payload should parse");

        assert_eq!(inbound.user_id.as_deref(), Some("wx-user"));
        assert_eq!(inbound.session_id.as_deref(), Some("room-1"));
        assert_eq!(inbound.text, "hello from wechat");
    }
}
