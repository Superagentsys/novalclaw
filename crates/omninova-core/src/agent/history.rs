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

/// Prefix identifying a compaction summary, so repeated compactions fold the
/// previous summary into the new one instead of stacking summaries forever.
pub const SUMMARY_MARKER: &str = "[对话摘要]";

/// Fraction of the history budget kept verbatim as the most recent turns.
/// The rest is eligible for summarization.
const RECENT_KEEP_RATIO: usize = 2;

/// Which messages to summarize and which to keep verbatim.
#[derive(Debug)]
pub struct CompactionPlan {
    /// Leading system messages (bootstrap prompt) — always kept as-is.
    pub head: Vec<ChatMessage>,
    /// Older turns, plus any previous summary, to be condensed.
    pub summarize: Vec<ChatMessage>,
    /// Most recent turns, kept verbatim.
    pub tail: Vec<ChatMessage>,
}

/// Decide how to compact `messages` for a `max_history` budget.
///
/// Returns `None` when history still fits, or when there is nothing worth
/// summarizing (which prevents a pointless LLM call).
pub fn plan_compaction(messages: &[ChatMessage], max_history: usize) -> Option<CompactionPlan> {
    if max_history == 0 || messages.len() <= max_history {
        return None;
    }

    let head_len = messages
        .iter()
        .take_while(|message| message.role == "system" && !is_summary(message))
        .count();
    let keep_recent = (max_history / RECENT_KEEP_RATIO).max(1);

    // Everything between the bootstrap prompt and the recent window is
    // condensed; a previous summary sits in that range and gets folded in.
    let tail_start = messages.len().saturating_sub(keep_recent).max(head_len);
    if tail_start <= head_len {
        return None;
    }

    let summarize = messages[head_len..tail_start].to_vec();
    if summarize.iter().all(is_summary) {
        return None;
    }

    Some(CompactionPlan {
        head: messages[..head_len].to_vec(),
        summarize,
        tail: messages[tail_start..].to_vec(),
    })
}

fn is_summary(message: &ChatMessage) -> bool {
    message.role == "system" && message.content.starts_with(SUMMARY_MARKER)
}

/// Render the messages being dropped into a transcript for the summarizer.
pub fn render_for_summary(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .map(|message| format!("{}: {}", message.role, message.content))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Reassemble a compacted history around `summary`.
pub fn apply_compaction(plan: CompactionPlan, summary: &str) -> Vec<ChatMessage> {
    let mut out = plan.head;
    let summary = summary.trim();
    if !summary.is_empty() {
        out.push(ChatMessage::system(format!("{SUMMARY_MARKER} {summary}")));
    }
    out.extend(plan.tail);
    sanitize_messages_for_provider(out)
}

/// Truncate history to `max_history` while preserving the leading system
/// messages, so the bootstrap prompt and any summary are never dropped.
pub fn truncate_history_preserving_system(
    messages: Vec<ChatMessage>,
    max_history: usize,
) -> Vec<ChatMessage> {
    if max_history == 0 || messages.len() <= max_history {
        return messages;
    }

    let head_len = messages
        .iter()
        .take_while(|message| message.role == "system")
        .count()
        .min(max_history);
    let tail_budget = max_history - head_len;

    let mut out = messages[..head_len].to_vec();
    if tail_budget > 0 {
        let start = messages.len().saturating_sub(tail_budget).max(head_len);
        out.extend_from_slice(&messages[start..]);
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

    fn history(system: usize, turns: usize) -> Vec<ChatMessage> {
        let mut messages = Vec::new();
        for i in 0..system {
            messages.push(ChatMessage::system(format!("bootstrap {i}")));
        }
        for i in 0..turns {
            messages.push(ChatMessage::user(format!("u{i}")));
            messages.push(ChatMessage::assistant(format!("a{i}")));
        }
        messages
    }

    #[test]
    fn no_compaction_when_history_fits() {
        assert!(plan_compaction(&history(1, 3), 50).is_none());
    }

    #[test]
    fn compaction_keeps_bootstrap_and_recent_turns() {
        let messages = history(2, 20);
        let plan = plan_compaction(&messages, 10).expect("plan");

        assert_eq!(plan.head.len(), 2);
        assert!(plan.head.iter().all(|m| m.role == "system"));
        assert_eq!(plan.tail.len(), 5);
        assert_eq!(plan.tail.last().unwrap().content, "a19");
        assert_eq!(plan.head.len() + plan.summarize.len() + plan.tail.len(), 42);
    }

    #[test]
    fn previous_summary_is_folded_into_the_next_one() {
        let mut messages = vec![
            ChatMessage::system("bootstrap"),
            ChatMessage::system(format!("{SUMMARY_MARKER} earlier context")),
        ];
        messages.extend(history(0, 20));

        let plan = plan_compaction(&messages, 10).expect("plan");

        assert_eq!(plan.head.len(), 1);
        assert!(plan.summarize[0].content.starts_with(SUMMARY_MARKER));
    }

    #[test]
    fn apply_compaction_inserts_single_summary_after_head() {
        let messages = history(1, 20);
        let plan = plan_compaction(&messages, 10).expect("plan");

        let compacted = apply_compaction(plan, "用户在配置记忆后端");

        assert_eq!(compacted[0].content, "bootstrap 0");
        assert_eq!(
            compacted[1].content,
            format!("{SUMMARY_MARKER} 用户在配置记忆后端")
        );
        assert_eq!(compacted.len(), 7);
    }

    #[test]
    fn empty_summary_does_not_add_a_message() {
        let messages = history(1, 20);
        let plan = plan_compaction(&messages, 10).expect("plan");

        let compacted = apply_compaction(plan, "   ");

        assert!(!compacted.iter().any(|m| m.content.contains(SUMMARY_MARKER)));
    }

    #[test]
    fn truncation_preserves_system_messages() {
        let mut messages = vec![
            ChatMessage::system("bootstrap"),
            ChatMessage::system(format!("{SUMMARY_MARKER} earlier")),
        ];
        messages.extend(history(0, 10));

        let truncated = truncate_history_preserving_system(messages, 6);

        assert_eq!(truncated.len(), 6);
        assert_eq!(truncated[0].content, "bootstrap");
        assert!(truncated[1].content.starts_with(SUMMARY_MARKER));
        assert_eq!(truncated.last().unwrap().content, "a9");
    }

    #[test]
    fn truncation_is_noop_when_within_budget() {
        let messages = history(1, 2);
        let truncated = truncate_history_preserving_system(messages.clone(), 50);
        assert_eq!(truncated.len(), messages.len());
    }
}
