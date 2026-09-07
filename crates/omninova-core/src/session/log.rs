use super::types::{SessionEvent, SessionEventKind};
use super::{is_summary_content};
use crate::providers::ChatMessage;
use serde_json::Value;

fn now_unix_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn events_from_messages(messages: &[ChatMessage]) -> Vec<SessionEvent> {
    let ts = now_unix_ts();
    messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            let seq = (index as u64).saturating_add(1);
            let kind = match message.role.as_str() {
                "system" if is_summary_content(&message.content) => {
                    let summary = message
                        .content
                        .strip_prefix(crate::agent::history::SUMMARY_MARKER)
                        .unwrap_or(&message.content)
                        .trim()
                        .to_string();
                    SessionEventKind::Compact {
                        summary,
                        hidden_through_seq: seq.saturating_sub(1),
                    }
                }
                "user" => SessionEventKind::User {
                    content: message.content.clone(),
                },
                "assistant" => SessionEventKind::Assistant {
                    content: message.content.clone(),
                },
                "tool" => {
                    let (tool_call_id, content) = split_tool_payload(&message.content);
                    SessionEventKind::ToolResult {
                        tool_call_id,
                        content,
                        interrupted: false,
                        original_content: message.original_tool_content.clone(),
                    }
                }
                _ => SessionEventKind::System {
                    content: message.content.clone(),
                },
            };
            SessionEvent { seq, ts, kind }
        })
        .collect()
}

pub fn derive_messages(events: &[SessionEvent]) -> Vec<ChatMessage> {
    let hidden_through = events
        .iter()
        .filter_map(|event| match &event.kind {
            SessionEventKind::Compact {
                hidden_through_seq, ..
            } => Some(*hidden_through_seq),
            _ => None,
        })
        .max()
        .unwrap_or(0);

    let mut messages = Vec::with_capacity(events.len());
    for event in events {
        let pinned_or_compact = matches!(
            event.kind,
            SessionEventKind::Compact { .. } | SessionEventKind::System { .. }
        );
        if event.seq <= hidden_through && !pinned_or_compact {
            continue;
        }
        if let Some(message) = event_to_message(event) {
            messages.push(message);
        }
    }
    messages
}

pub fn repair_unclosed_tools(events: &mut Vec<SessionEvent>) -> bool {
    let Some(last_assistant) = events.iter().rev().find_map(|event| match &event.kind {
        SessionEventKind::Assistant { content } => Some((event.seq, content.clone())),
        _ => None,
    }) else {
        return false;
    };

    let expected = assistant_tool_call_ids(&last_assistant.1);
    if expected.is_empty() {
        return false;
    }

    let mut answered = std::collections::HashSet::new();
    for event in events.iter() {
        if event.seq <= last_assistant.0 {
            continue;
        }
        if let SessionEventKind::ToolResult { tool_call_id, .. } = &event.kind {
            answered.insert(tool_call_id.clone());
        }
    }

    let missing: Vec<String> = expected
        .into_iter()
        .filter(|id| !answered.contains(id))
        .collect();
    if missing.is_empty() {
        return false;
    }

    let ts = now_unix_ts();
    let mut next_seq = events.last().map(|event| event.seq).unwrap_or(0);
    for tool_call_id in missing {
        next_seq += 1;
        events.push(SessionEvent {
            seq: next_seq,
            ts,
            kind: SessionEventKind::ToolResult {
                content: serde_json::json!({
                    "status": "interrupted",
                    "reason": "session closed before the tool result was recorded",
                })
                .to_string(),
                tool_call_id,
                interrupted: true,
                original_content: None,
            },
        });
    }
    events.push(SessionEvent {
        seq: next_seq + 1,
        ts,
        kind: SessionEventKind::Interrupt {
            reason: "unclosed tool calls repaired on load".to_string(),
        },
    });
    true
}

fn event_to_message(event: &SessionEvent) -> Option<ChatMessage> {
    match &event.kind {
        SessionEventKind::System { content } => Some(ChatMessage::system(content)),
        SessionEventKind::User { content } => Some(ChatMessage::user(content)),
        SessionEventKind::Assistant { content } => Some(ChatMessage::assistant(content)),
        SessionEventKind::ToolResult {
            tool_call_id,
            content,
            interrupted,
            original_content,
        } => {
            let payload = serde_json::json!({
                "tool_call_id": tool_call_id,
                "content": content,
                "interrupted": interrupted,
            });
            let mut message = ChatMessage::tool(payload.to_string());
            message.original_tool_content = original_content.clone();
            Some(message)
        }
        SessionEventKind::Compact { summary, .. } => Some(ChatMessage::system(format!(
            "{} {}",
            crate::agent::history::SUMMARY_MARKER,
            summary
        ))),
        SessionEventKind::Interrupt { .. } => None,
    }
}

fn split_tool_payload(raw: &str) -> (String, String) {
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        let id = value
            .get("tool_call_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let content = value
            .get("content")
            .map(|item| match item {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            })
            .unwrap_or_else(|| raw.to_string());
        return (id, content);
    }
    (String::new(), raw.to_string())
}

fn assistant_tool_call_ids(content: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<Value>(content) else {
        return Vec::new();
    };
    value
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|calls| {
            calls
                .iter()
                .filter_map(|call| {
                    call.get("id")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_roles() {
        let messages = vec![
            ChatMessage::system("bootstrap"),
            ChatMessage::user("hello"),
            ChatMessage::assistant("hi"),
        ];
        let events = events_from_messages(&messages);
        let restored = derive_messages(&events);
        assert_eq!(restored.len(), 3);
        assert_eq!(restored[1].content, "hello");
    }

    #[test]
    fn compact_event_becomes_summary_system_message() {
        let messages = vec![
            ChatMessage::system("bootstrap"),
            ChatMessage::system(format!(
                "{} older turns",
                crate::agent::history::SUMMARY_MARKER
            )),
            ChatMessage::user("next"),
        ];
        let events = events_from_messages(&messages);
        assert!(matches!(
            events[1].kind,
            SessionEventKind::Compact { .. }
        ));
        let restored = derive_messages(&events);
        assert!(restored[1]
            .content
            .starts_with(crate::agent::history::SUMMARY_MARKER));
    }

    #[test]
    fn repair_appends_synthetic_tool_results() {
        let assistant = serde_json::json!({
            "content": null,
            "tool_calls": [{ "id": "call_1", "name": "shell", "arguments": "{}" }]
        })
        .to_string();
        let mut events = events_from_messages(&[ChatMessage::assistant(assistant)]);
        assert!(repair_unclosed_tools(&mut events));
        assert!(matches!(
            events[1].kind,
            SessionEventKind::ToolResult {
                interrupted: true,
                ..
            }
        ));
    }

    #[test]
    fn tool_result_original_content_round_trips() {
        let messages = vec![
            ChatMessage::system("bootstrap"),
            ChatMessage::user("hello"),
            ChatMessage::assistant(r#"{"tool_calls":[{"id":"call-1","name":"x","arguments":"{}"}]}"#),
            {
                let mut tool = ChatMessage::tool(r#"{"tool_call_id":"call-1","content":"pruned"}"#.to_string());
                tool.original_tool_content = Some("FULL_ORIGINAL_OUTPUT".to_string());
                tool
            },
        ];
        let events = events_from_messages(&messages);
        let restored = derive_messages(&events);
        let restored_tool = restored
            .iter()
            .find(|m| m.role == "tool")
            .expect("tool message restored");
        assert_eq!(restored_tool.original_tool_content.as_deref(), Some("FULL_ORIGINAL_OUTPUT"));
    }
}
