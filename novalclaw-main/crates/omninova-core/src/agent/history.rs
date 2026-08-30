use crate::providers::ChatMessage;
use serde_json::Value;

/// 从 assistant 消息 JSON 中解析 tool_calls 数量（OpenAI 兼容格式）。
fn assistant_tool_call_count(message: &ChatMessage) -> usize {
    if message.role != "assistant" {
        return 0;
    }
    let Ok(value) = serde_json::from_str::<Value>(&message.content) else {
        return 0;
    };
    value
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0)
}

/// tool 消息是否包含有效的 tool_call_id。
fn tool_message_has_call_id(message: &ChatMessage) -> bool {
    if message.role != "tool" {
        return false;
    }
    let Ok(value) = serde_json::from_str::<Value>(&message.content) else {
        return false;
    };
    value
        .get("tool_call_id")
        .and_then(Value::as_str)
        .is_some_and(|id| !id.is_empty())
}

/// 移除孤立的 tool 消息、不完整的 tool 轮次，避免 OpenAI API 400。
pub fn sanitize_messages_for_provider(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let mut out = Vec::with_capacity(messages.len());

    let mut i = 0;
    while i < messages.len() {
        let msg = &messages[i];

        if msg.role == "tool" {
            // 前一条必须是带 tool_calls 的 assistant
            i += 1;
            continue;
        }

        if msg.role == "assistant" {
            let call_count = assistant_tool_call_count(msg);
            if call_count > 0 {
                let mut j = i + 1;
                let mut valid_tools = 0usize;
                while j < messages.len() && messages[j].role == "tool" {
                    if tool_message_has_call_id(&messages[j]) {
                        valid_tools += 1;
                    }
                    j += 1;
                }

                if valid_tools > 0 && valid_tools == call_count {
                    for k in i..j {
                        out.push(messages[k].clone());
                    }
                    i = j;
                    continue;
                }

                // 不完整或孤立的 tool 轮次：跳过 assistant 与后续 tool
                i += 1;
                while i < messages.len() && messages[i].role == "tool" {
                    i += 1;
                }
                continue;
            }
        }

        out.push(msg.clone());
        i += 1;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_orphan_tool_messages() {
        let messages = vec![
            ChatMessage::user("hi"),
            ChatMessage::tool(
                r#"{"tool_call_id":"call_1","content":"result"}"#.to_string(),
            ),
        ];
        let sanitized = sanitize_messages_for_provider(messages);
        assert_eq!(sanitized.len(), 1);
        assert_eq!(sanitized[0].role, "user");
    }

    #[test]
    fn keeps_complete_tool_turn() {
        let assistant = serde_json::json!({
            "content": null,
            "tool_calls": [{
                "id": "call_1",
                "name": "test_tool",
                "arguments": "{}"
            }]
        })
        .to_string();
        let messages = vec![
            ChatMessage::user("run tool"),
            ChatMessage::assistant(assistant),
            ChatMessage::tool(
                r#"{"tool_call_id":"call_1","content":"ok"}"#.to_string(),
            ),
            ChatMessage::assistant("done"),
        ];
        let sanitized = sanitize_messages_for_provider(messages);
        assert_eq!(sanitized.len(), 4);
    }
}
