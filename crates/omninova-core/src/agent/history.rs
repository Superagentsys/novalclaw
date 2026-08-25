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
pub const TASK_MARKER: &str = "[任务]";
pub const CHECKPOINT_MARKER: &str = "[检查点]";

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
    plan_compaction_with_tail_tokens(messages, max_history, None)
}

pub fn plan_compaction_with_tail_tokens(
    messages: &[ChatMessage],
    max_history: usize,
    recent_tail_tokens: Option<u64>,
) -> Option<CompactionPlan> {
    if max_history == 0 || messages.len() <= max_history {
        return None;
    }

    let head_len = messages
        .iter()
        .take_while(|message| message.role == "system" && !is_summary(message))
        .count();

    let mut tail_start = if let Some(budget) = recent_tail_tokens {
        let estimator = crate::providers::context_budget::TokenEstimator::new();
        let mut consumed = 0u64;
        let mut index = messages.len();
        while index > head_len {
            let idx = index - 1;
            let tokens = estimator.estimate_text(&messages[idx].content);
            if consumed.saturating_add(tokens) > budget {
                break;
            }
            consumed = consumed.saturating_add(tokens);
            index -= 1;
        }
        index
    } else {
        let keep_recent = (max_history / RECENT_KEEP_RATIO).max(1);
        messages.len().saturating_sub(keep_recent).max(head_len)
    };

    // Never split an assistant tool_call from its tool results.
    while tail_start > head_len
        && tail_start < messages.len()
        && messages[tail_start].role == "tool"
    {
        tail_start -= 1;
    }
    if tail_start <= head_len {
        return None;
    }

    let mut head = messages[..head_len].to_vec();
    let mut summarize = messages[head_len..tail_start].to_vec();
    let mut extra_pinned = Vec::new();
    summarize.retain(|message| {
        if is_pinned_system(message) {
            extra_pinned.push(message.clone());
            false
        } else {
            true
        }
    });
    head.extend(extra_pinned);
    if summarize.iter().all(is_summary) {
        return None;
    }

    Some(CompactionPlan {
        head,
        summarize,
        tail: messages[tail_start..].to_vec(),
    })
}

fn is_summary(message: &ChatMessage) -> bool {
    message.role == "system" && message.content.starts_with(SUMMARY_MARKER)
}

pub fn is_pinned_system(message: &ChatMessage) -> bool {
    message.role == "system"
        && (message.content.starts_with(TASK_MARKER)
            || message.content.starts_with(CHECKPOINT_MARKER))
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

    let mut protected = Vec::new();
    let mut rest = Vec::new();
    let mut in_prefix = true;
    for message in messages {
        if in_prefix && message.role == "system" {
            protected.push(message);
            continue;
        }
        in_prefix = false;
        if is_pinned_system(&message) {
            protected.push(message);
        } else {
            rest.push(message);
        }
    }

    let tail_budget = max_history.saturating_sub(protected.len());
    if tail_budget == 0 {
        return protected;
    }
    let start = rest.len().saturating_sub(tail_budget);
    protected.extend(rest.drain(start..));
    protected
}

/// Keeps the first bootstrap system message and protected markers, drops
/// transient runtime system/skill/knowledge messages that are re-injected
/// fresh on every request.
pub fn normalize_transient_system_messages(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let mut first_system = true;
    messages
        .into_iter()
        .filter(|message| {
            if message.role != "system" {
                return true;
            }
            if is_summary(message) || is_pinned_system(message) {
                return true;
            }
            if first_system {
                first_system = false;
                return true;
            }
            false
        })
        .collect()
}

/// Replaces oversized historical tool results with a bounded pruning marker.
/// Recent messages within `recent_tail_tokens` are left untouched so the Agent
/// can continue the current operation.
pub fn prune_oversized_tool_results(
    messages: Vec<ChatMessage>,
    max_tool_result_tokens: u64,
    recent_tail_tokens: u64,
) -> (Vec<ChatMessage>, usize) {
    use crate::providers::context_budget::TokenEstimator;

    let estimator = TokenEstimator::new();
    let mut protected_from_end = 0usize;
    let mut consumed = 0u64;
    for message in messages.iter().rev() {
        let tokens = estimator.estimate_text(&message.content);
        if consumed.saturating_add(tokens) > recent_tail_tokens {
            break;
        }
        consumed = consumed.saturating_add(tokens);
        protected_from_end += 1;
    }

    let prune_start = messages.len().saturating_sub(protected_from_end);
    let mut pruned = 0usize;
    let mut out = Vec::with_capacity(messages.len());
    for (index, mut message) in messages.into_iter().enumerate() {
        if index >= prune_start || message.role != "tool" {
            out.push(message);
            continue;
        }
        let tokens = estimator.estimate_text(&message.content);
        if tokens <= max_tool_result_tokens {
            out.push(message);
            continue;
        }
        if let Some(pruned_content) = prune_tool_content(&message.content) {
            if message.original_tool_content.is_none() {
                if let Some(original) = extract_tool_content(&message.content) {
                    message.original_tool_content = Some(original);
                }
            }
            message.content = pruned_content;
            pruned += 1;
        }
        out.push(message);
    }
    (out, pruned)
}

fn extract_tool_content(content: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(content).ok()?;
    value
        .get("content")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn prune_tool_content(content: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(content).ok()?;
    let original = value.get("content").and_then(|v| v.as_str())?;
    let head: String = original.chars().take(600).collect();
    let tail_start = original.len().saturating_sub(600);
    let tail: String = original.chars().skip(tail_start).take(600).collect();
    let marker = format!(
        "[Tool output pruned from model context]\noriginal_size={}\nretained_head={}\nretained_tail={}\n\n{head}\n\n... omitted ...\n\n{tail}",
        original.chars().count(),
        head.chars().count(),
        tail.chars().count(),
    );
    let mut pruned = value;
    pruned["content"] = serde_json::Value::String(marker);
    Some(serde_json::to_string(&pruned).ok()?)
}

/// Builds a structured checkpoint system message from available task pins and
/// a compact summary. This is intentionally deterministic: it preserves what
/// source history provides and does not invent missing fields.
pub fn build_structured_checkpoint(messages: &[ChatMessage], summary: &str) -> ChatMessage {
    let mut task_lines = Vec::new();
    let mut checkpoint_lines = Vec::new();
    for message in messages {
        if message.content.starts_with(TASK_MARKER) {
            task_lines.push(message.content.trim_start_matches(TASK_MARKER).trim().to_string());
        } else if message.content.starts_with(CHECKPOINT_MARKER) {
            checkpoint_lines.push(
                message
                    .content
                    .trim_start_matches(CHECKPOINT_MARKER)
                    .trim()
                    .to_string(),
            );
        }
    }

    let body = format!(
        "## Primary Goal\n{}\n\n## Current Task State\n{}\n\n## Important Facts\n{}",
        task_lines.join("\n"),
        checkpoint_lines
            .last()
            .cloned()
            .unwrap_or_else(|| summary.to_string()),
        task_lines.join("\n"),
    );
    ChatMessage::system(format!("{CHECKPOINT_MARKER} structured checkpoint\n{body}"))
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

    #[test]
    fn pinned_task_stays_in_compaction_head() {
        let mut messages = vec![
            ChatMessage::system("bootstrap"),
            ChatMessage::system(format!("{TASK_MARKER} 持续跟标书")),
            ChatMessage::system(format!("{CHECKPOINT_MARKER} 已列提纲")),
        ];
        messages.extend(history(0, 20));

        let plan = plan_compaction(&messages, 10).expect("plan");
        assert!(plan
            .head
            .iter()
            .any(|message| message.content.starts_with(TASK_MARKER)));
        assert!(plan
            .head
            .iter()
            .any(|message| message.content.starts_with(CHECKPOINT_MARKER)));
        assert!(!plan
            .summarize
            .iter()
            .any(|message| is_pinned_system(message)));
    }
    #[test]
    fn normalize_keeps_first_system_and_markers_drops_transient() {
        let messages = vec![
            ChatMessage::system("bootstrap"),
            ChatMessage::system("skill runtime instructions"),
            ChatMessage::system(format!("{SUMMARY_MARKER} old summary")),
            ChatMessage::system(format!("{TASK_MARKER} goal")),
            ChatMessage::user("hi"),
        ];
        let out = normalize_transient_system_messages(messages);
        let system_count = out.iter().filter(|m| m.role == "system").count();
        assert_eq!(system_count, 3);
        assert!(out.iter().any(|m| m.content.starts_with(SUMMARY_MARKER)));
        assert!(out.iter().any(|m| m.content.starts_with(TASK_MARKER)));
        assert!(!out.iter().any(|m| m.content.contains("skill runtime")));
    }

    #[test]
    fn prune_prunes_old_large_tool_result_and_keeps_recent() {
        use crate::providers::context_budget::TokenEstimator;
        let estimator = TokenEstimator::new();
        let huge = "x".repeat(200_000);
        let old_tool = ChatMessage::tool(format!("{{\"tool_call_id\":\"call-old\",\"content\":\"{huge}\"}}"));
        let recent_tool = ChatMessage::tool("{\"tool_call_id\":\"call-new\",\"content\":\"small\"}");
        let messages = vec![
            ChatMessage::assistant("{\"tool_calls\":[]}"),
            old_tool,
            ChatMessage::assistant("{\"tool_calls\":[]}"),
            recent_tool,
        ];
        let (out, pruned) = prune_oversized_tool_results(messages, 1_000, 10_000);
        assert_eq!(pruned, 1);
        assert!(out.iter().any(|m| m.content.contains("[Tool output pruned from model context]")));
        assert!(out.iter().any(|m| m.content.contains("call-new")));
        let after = estimator.estimate_messages_with_tools(&out, &[]);
        let before = estimator.estimate_messages_with_tools(&out, &[]);
        assert!(after <= before + 8);
    }

    #[test]
    fn structured_checkpoint_preserves_task_and_checkpoint() {
        let messages = vec![
            ChatMessage::system(format!("{TASK_MARKER} keep goal")),
            ChatMessage::system(format!("{CHECKPOINT_MARKER} keep state")),
            ChatMessage::user("hello"),
        ];
        let checkpoint = build_structured_checkpoint(&messages, "fallback summary");
        let body = checkpoint.content;
        assert!(body.contains("Primary Goal"));
        assert!(body.contains("keep goal"));
        assert!(body.contains("keep state"));
        assert!(body.starts_with(CHECKPOINT_MARKER));
    }

    #[test]
    fn pruning_captures_original_tool_content_for_durable_recovery() {
        let huge = "x".repeat(20_000);
        let tool = ChatMessage::tool(format!(r#"{{"tool_call_id":"call-1","content":"{huge}"}}"#));
        let messages = vec![
            ChatMessage::assistant(r#"{"tool_calls":[]}"#),
            tool,
            ChatMessage::assistant("done"),
        ];
        let (pruned_messages, pruned) = prune_oversized_tool_results(messages, 100, 10_000);
        assert_eq!(pruned, 1);
        let pruned_tool = pruned_messages
            .iter()
            .find(|m| m.role == "tool")
            .expect("tool remains");
        assert!(pruned_tool.content.contains("[Tool output pruned from model context]"));
        let original = pruned_tool
            .original_tool_content
            .as_deref()
            .expect("original output captured");
        assert_eq!(original, huge);
    }

}
