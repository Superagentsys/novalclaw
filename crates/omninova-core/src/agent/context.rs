use crate::agent::history::{
    apply_compaction, build_structured_checkpoint, normalize_transient_system_messages,
    plan_compaction, plan_compaction_with_tail_tokens, prune_oversized_tool_results,
    render_for_summary, truncate_history_preserving_system, SUMMARY_MARKER,
};
use crate::providers::context_budget::{
    largest_tool_result_tokens, TokenEstimator, MAX_MAINTENANCE_PASSES, MAX_SUMMARY_SOURCE_CHARS,
    UNKNOWN_BUDGET_RECENT_TAIL_TOKENS, UNKNOWN_BUDGET_TOOL_SOFT_CAP_TOKENS,
};
use crate::providers::{ChatMessage, ChatRequest, Provider};
use crate::tools::ToolSpec;

const SUMMARIZER_PROMPT: &str = "你是对话摘要器。把下面较早的对话浓缩成要点，必须保留：\
用户的目标与偏好、已确认的事实与决定、未完成的任务与下一步、关键文件路径与命令、阻塞原因。\
省略寒暄与重复内容，不要编造未出现的信息。用中文输出，不超过 1200 字。\
不要改写或省略以 [任务] 或 [检查点] 开头的内容。";


/// Maintenance mode used by the Context Runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMaintenanceMode {
    /// Normal proactive maintenance (C2/C2.1/C2.2).
    Normal,
    /// Forced recovery after a Provider-reported context overflow (C3).
    /// Even when local pressure thresholds are not crossed, this mode runs
    /// tool-result pruning and structured compaction in a bounded way.
    ForcedOverflowRecovery,
}

/// One authoritative context-maintenance path used by both turn-boundary
/// compaction and mid-turn Provider-request preparation.
///
/// Flow:
///   1. normalize transient system/skill/knowledge messages
///   2. measure
///   3. if pressure: prune oversized historical tool results
///   4. remeasure
///   5. if still above target and compaction is enabled: bounded structured
///      compaction with a checkpoint + recent-tail preservation
///   6. return the model-visible context (C1 preflight still runs afterwards
///      inside the Provider)
pub async fn maintain_context(
    provider: &dyn Provider,
    messages: Vec<ChatMessage>,
    tool_specs: &[ToolSpec],
    max_history_messages: usize,
    compact_context: bool,
) -> Vec<ChatMessage> {
    maintain_context_with_mode(
        provider,
        messages,
        tool_specs,
        max_history_messages,
        compact_context,
        ContextMaintenanceMode::Normal,
    )
    .await
}

/// Forced context recovery used after a Provider reports `ContextWindowExceeded`.
///
/// This is intentionally more aggressive than normal proactive maintenance:
/// it always prunes oversized tool results and attempts structured compaction
/// when pruning alone is not sufficient, regardless of local pressure
/// thresholds. It still respects tool-pair safety and durability.
pub async fn force_context_recovery(
    provider: &dyn Provider,
    messages: Vec<ChatMessage>,
    tool_specs: &[ToolSpec],
    max_history_messages: usize,
) -> Vec<ChatMessage> {
    maintain_context_with_mode(
        provider,
        messages,
        tool_specs,
        max_history_messages,
        true,
        ContextMaintenanceMode::ForcedOverflowRecovery,
    )
    .await
}

async fn maintain_context_with_mode(
    provider: &dyn Provider,
    messages: Vec<ChatMessage>,
    tool_specs: &[ToolSpec],
    max_history_messages: usize,
    compact_context: bool,
    mode: ContextMaintenanceMode,
) -> Vec<ChatMessage> {
    let forced = mode == ContextMaintenanceMode::ForcedOverflowRecovery;
    let Some(budget) = provider.context_budget() else {
        let estimator = TokenEstimator::new();
        let mut messages = normalize_transient_system_messages(messages);
        let before = estimator.estimate_messages_with_tools(&messages, tool_specs);
        let largest_tool = largest_tool_result_tokens(&messages, &estimator);
        let oversized_tool_trigger = largest_tool > UNKNOWN_BUDGET_TOOL_SOFT_CAP_TOKENS;
        let message_count_trigger = messages.len() > max_history_messages.max(1);
        let should_prune = forced || oversized_tool_trigger || message_count_trigger;

        if should_prune {
            tracing::info!(
                target: "omninova_core::context",
                budget_source = "unknown",
                context_window = tracing::field::Empty,
                message_count = %messages.len(),
                estimated_input = %before,
                largest_tool_result_tokens = %largest_tool,
                unknown_budget_oversize_trigger = %oversized_tool_trigger,
                message_count_trigger = %message_count_trigger,
                forced = %forced,
                "unknown_budget_context_maintenance"
            );
            let (mut current, pruned_count) = prune_oversized_tool_results(
                messages,
                UNKNOWN_BUDGET_TOOL_SOFT_CAP_TOKENS,
                UNKNOWN_BUDGET_RECENT_TAIL_TOKENS,
            );
            let after_tool_prune = estimator.estimate_messages_with_tools(&current, tool_specs);
            if pruned_count > 0 {
                tracing::info!(
                    target: "omninova_core::context",
                    budget_source = "unknown",
                    message_count = %current.len(),
                    estimated_before = %before,
                    estimated_after = %after_tool_prune,
                    pruning_triggered = true,
                    "unknown_budget_tool_results_pruned"
                );
            }

            if !compact_context {
                return current;
            }

            if !forced && !message_count_trigger {
                return current;
            }

            messages = current;
        }

        if !compact_context {
            return messages;
        }
        let max_history = if forced {
            (messages.len() / 2).max(1)
        } else {
            max_history_messages.max(1)
        };
        let Some(plan) = plan_compaction(&messages, max_history) else {
            return messages;
        };
        let request_messages = vec![
            ChatMessage::system(SUMMARIZER_PROMPT),
            ChatMessage::user(render_for_summary(&plan.summarize)),
        ];
        let summary = match provider
            .chat(ChatRequest {
                messages: &request_messages,
                tools: None,
            })
            .await
        {
            Ok(response) => response.text.unwrap_or_default(),
            Err(error) => {
                tracing::warn!("context compaction failed, truncating instead: {error}");
                return truncate_history_preserving_system(messages, max_history);
            }
        };
        return apply_compaction(plan, &summary);
    };

    let estimator = TokenEstimator::new();
    let messages = normalize_transient_system_messages(messages);
    let before = estimator.estimate_messages_with_tools(&messages, tool_specs);
    let pressure_threshold = budget.pressure_threshold();
    if !forced && before <= pressure_threshold {
        return messages;
    }

    tracing::info!(
        target: "omninova_core::context",
        estimated_before = %before,
        pressure_threshold = %pressure_threshold,
        max_input = %budget.max_input_tokens,
        forced = %forced,
        "context_pressure_detected"
    );

    let max_tool_result_tokens = if forced {
        UNKNOWN_BUDGET_TOOL_SOFT_CAP_TOKENS
    } else {
        budget.max_input_tokens.saturating_div(20).max(1)
    };
    let recent_tail_tokens = if forced {
        UNKNOWN_BUDGET_RECENT_TAIL_TOKENS
    } else {
        budget.recent_tail_budget()
    };
    let (mut current, pruned_count) =
        prune_oversized_tool_results(messages, max_tool_result_tokens, recent_tail_tokens);
    let after_tool_prune = estimator.estimate_messages_with_tools(&current, tool_specs);
    if pruned_count > 0 {
        tracing::info!(
            target: "omninova_core::context",
            tool_results_pruned = %pruned_count,
            estimated_before = %before,
            estimated_after = %after_tool_prune,
            forced = %forced,
            "tool_results_pruned"
        );
    }

    if !compact_context {
        return current;
    }

    let should_compact = if forced {
        true
    } else {
        after_tool_prune > budget.target_after_compaction()
    };
    if should_compact {
        let mut last_estimate = after_tool_prune;
        for _pass in 0..MAX_MAINTENANCE_PASSES {
            let max_history = if forced {
                (current.len() / 2).max(1)
            } else {
                max_history_messages.max(1)
            };
            let tail_budget = if forced {
                None
            } else {
                Some(budget.recent_tail_budget())
            };
            let Some(plan) = plan_compaction_with_tail_tokens(
                &current,
                max_history,
                tail_budget,
            ) else {
                break;
            };
            if plan.summarize.is_empty() {
                break;
            }
            let render = render_for_summary(&plan.summarize);
            let bounded_render: String = if render.chars().count() > MAX_SUMMARY_SOURCE_CHARS {
                render.chars().take(MAX_SUMMARY_SOURCE_CHARS).collect()
            } else {
                render
            };
            let request_messages = vec![
                ChatMessage::system(SUMMARIZER_PROMPT),
                ChatMessage::user(bounded_render),
            ];
            let summary = match provider
                .chat(ChatRequest {
                    messages: &request_messages,
                    tools: None,
                })
                .await
            {
                Ok(response) => response.text.unwrap_or_default(),
                Err(error) => {
                    tracing::warn!("context compaction failed: {error}");
                    break;
                }
            };
            if summary.trim().is_empty() {
                break;
            }
            let checkpoint = build_structured_checkpoint(&current, &summary);
            let compacted = apply_compaction(plan, &summary)
                .into_iter()
                .map(|message| {
                    if message.role == "system" && message.content.starts_with(SUMMARY_MARKER) {
                        checkpoint.clone()
                    } else {
                        message
                    }
                })
                .collect::<Vec<_>>();
            let after_compact = estimator.estimate_messages_with_tools(&compacted, tool_specs);
            if after_compact >= last_estimate {
                tracing::warn!(
                    target: "omninova_core::context",
                    before = %last_estimate,
                    after = %after_compact,
                    "context_compaction_failed: non-shrinking"
                );
                break;
            }
            current = compacted;
            last_estimate = after_compact;
            tracing::info!(
                target: "omninova_core::context",
                estimated_before = %before,
                estimated_after = %after_compact,
                max_input = %budget.max_input_tokens,
                forced = %forced,
                "context_compaction_completed"
            );
            if after_compact <= budget.target_after_compaction() {
                break;
            }
        }
    }

    current
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::providers::context_budget::{ContextBudget, ContextBudgetSource};
    use crate::providers::{ChatMessage, ChatResponse, TokenUsage};
    use crate::security::SecurityContext;
    use crate::tools::{Tool, ToolResult};
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    struct BudgetProvider {
        budget: ContextBudget,
        requests: Arc<Mutex<Vec<Vec<ChatMessage>>>>,
        compact_summaries: Arc<Mutex<Vec<String>>>,
        tool_round: AtomicUsize,
    }

    impl BudgetProvider {
        fn new(context_window: u64) -> (Self, Arc<Mutex<Vec<Vec<ChatMessage>>>>, Arc<Mutex<Vec<String>>>) {
            let requests = Arc::new(Mutex::new(Vec::new()));
            let compact_summaries = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    budget: ContextBudget::new(context_window, Some(16_384), ContextBudgetSource::BuiltIn),
                    requests: requests.clone(),
                    compact_summaries: compact_summaries.clone(),
                    tool_round: AtomicUsize::new(0),
                },
                requests,
                compact_summaries,
            )
        }
    }

    #[async_trait]
    impl Provider for BudgetProvider {
        fn name(&self) -> &str {
            "budget"
        }

        fn context_budget(&self) -> Option<ContextBudget> {
            Some(self.budget)
        }

        async fn chat(&self, request: ChatRequest<'_>) -> anyhow::Result<ChatResponse> {
            self.requests.lock().unwrap().push(request.messages.to_vec());
            let is_summarizer = request
                .messages
                .iter()
                .any(|m| m.content.contains("你是对话摘要器"));
            if is_summarizer {
                self.compact_summaries.lock().unwrap().push("summarized context".to_string());
                return Ok(ChatResponse {
                    text: Some("summarized context".to_string()),
                    tool_calls: vec![],
                    usage: None,
                    reasoning_content: None,
                    finish_reason: Some("stop".to_string()),
                });
            }
            const NORMAL_TOOL_CALLS: usize = 8;
            let round = self.tool_round.fetch_add(1, Ordering::SeqCst);
            if round < NORMAL_TOOL_CALLS {
                return Ok(ChatResponse {
                    text: None,
                    tool_calls: vec![crate::providers::ToolCall {
                        id: format!("call-{}", round + 1),
                        name: "big_tool".to_string(),
                        arguments: "{}".to_string(),
                    }],
                    usage: Some(TokenUsage {
                        input_tokens: Some(100),
                        output_tokens: Some(10),
                    }),
                    reasoning_content: None,
                    finish_reason: Some("tool_calls".to_string()),
                });
            }
            Ok(ChatResponse {
                text: Some("final answer".to_string()),
                tool_calls: vec![],
                usage: Some(TokenUsage {
                    input_tokens: Some(100),
                    output_tokens: Some(10),
                }),
                reasoning_content: None,
                finish_reason: Some("stop".to_string()),
            })
        }

        async fn health_check(&self) -> bool {
            true
        }
    }

    struct UnknownBudgetProvider {
        compact_summaries: Arc<Mutex<Vec<String>>>,
    }

    impl UnknownBudgetProvider {
        fn new() -> (Self, Arc<Mutex<Vec<String>>>) {
            let compact_summaries = Arc::new(Mutex::new(Vec::new()));
            (Self {
                compact_summaries: compact_summaries.clone(),
            }, compact_summaries)
        }
    }

    #[async_trait]
    impl Provider for UnknownBudgetProvider {
        fn name(&self) -> &str {
            "unknown-budget"
        }

        fn context_budget(&self) -> Option<ContextBudget> {
            None
        }

        async fn chat(&self, request: ChatRequest<'_>) -> anyhow::Result<ChatResponse> {
            let is_summarizer = request
                .messages
                .iter()
                .any(|m| m.content.contains("你是对话摘要器"));
            if is_summarizer {
                self.compact_summaries.lock().unwrap().push("summary".to_string());
                return Ok(ChatResponse {
                    text: Some("summary".to_string()),
                    tool_calls: vec![],
                    usage: None,
                    reasoning_content: None,
                    finish_reason: Some("stop".to_string()),
                });
            }
            Ok(ChatResponse {
                text: Some("ok".to_string()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
                finish_reason: Some("stop".to_string()),
            })
        }

        async fn health_check(&self) -> bool {
            true
        }
    }

    struct BigTool;

    #[async_trait]
    impl Tool for BigTool {
        fn name(&self) -> &str {
            "big_tool"
        }

        fn description(&self) -> &str {
            "big tool"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            json!({ "type": "object", "properties": {} })
        }

        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            Ok(ToolResult {
                success: true,
                output: "X".repeat(5_000),
                error: None,
            })
        }
    }

    #[tokio::test]
    async fn long_run_mid_turn_compaction_preserves_continuity_and_stays_under_budget() {
        let (provider, requests, summaries) = BudgetProvider::new(60_000);
        let memory: Arc<dyn crate::memory::Memory> = Arc::new(crate::InMemoryMemory::new());
        let mut config = Config::default();
        config.autonomy.level = "autonomous".to_string();
        config.approvals.enabled = false;
        config.security.tool_policy.allowed_tools = vec!["big_tool".to_string()];
        let security = SecurityContext::from_config(&config);
        let mut config = crate::config::AgentConfig::default();
        config.compact_context = true;
        config.max_history_messages = 10;
        config.system_prompt = Some("Primary goal: complete long task. Constraint: don't stop. Branch: feature/testnew-context-security-hardening".to_string());

        let mut agent = crate::agent::Agent::new(
            Box::new(provider),
            vec![Box::new(BigTool)],
            memory,
            config,
            security,
        );
        let (reply, _events) = agent
            .process_message_with_events_streaming(
                "Run the long task and keep all task state visible.",
                Box::new(|_| {}),
                Some("run-long".to_string()),
                Some("session-long".to_string()),
                crate::agent::AgentCancellationToken::new(),
            )
            .await
            .expect("long run completes");
        assert_eq!(reply, "final answer");

        let captured = requests.lock().unwrap();
        assert!(captured.len() >= 2, "expected at least two model calls");
        let final_request = captured.last().unwrap();
        let estimator = TokenEstimator::new();
        let final_estimate = estimator.estimate_messages_with_tools(final_request, &[]);
        let budget = ContextBudget::new(200_000, Some(16_384), ContextBudgetSource::BuiltIn);
        assert!(
            final_estimate <= budget.max_input_tokens,
            "final request must stay under C1 budget"
        );
        let all_text: String = final_request
            .iter()
            .map(|m| m.content.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all_text.contains("Primary goal"));
        assert!(all_text.contains("feature/testnew-context-security-hardening"));
        assert!(all_text.contains("summarized context") || all_text.contains("[检查点]"));
        assert!(summaries.lock().unwrap().len() >= 1);
    }

    #[tokio::test]
    async fn maintain_context_prunes_before_structured_compaction() {
        let (provider, _, summaries) = BudgetProvider::new(200_000);
        let huge = "x".repeat(120_000);
        let messages = vec![
            ChatMessage::system("bootstrap"),
            ChatMessage::user("goal"),
            ChatMessage::assistant(r#"{"tool_calls":[{"id":"old","name":"big_tool","arguments":"{}"}]}"#),
            ChatMessage::tool(format!(r#"{{"tool_call_id":"old","content":"{huge}"}}"#)),
            ChatMessage::assistant("continue"),
        ];
        let out = maintain_context(
            &provider,
            messages,
            &[],
            10,
            true,
        ).await;
        let pruned = out.iter().any(|m| m.content.contains("[Tool output pruned from model context]"));
        let has_checkpoint = out.iter().any(|m| m.content.starts_with("[检查点]"));
        assert!(pruned, "large tool output should be pruned");
        if !has_checkpoint {
            assert!(summaries.lock().unwrap().is_empty(), "summarizer ran but no checkpoint was inserted");
        } else {
            assert!(summaries.lock().unwrap().len() >= 1);
        }
    }

    #[tokio::test]
    async fn unknown_budget_oversized_tool_prunes_independent_of_message_count() {
        let (provider, summaries) = UnknownBudgetProvider::new();
        let huge = "x".repeat(1_924_822);
        let mut messages = vec![ChatMessage::system("bootstrap")];
        for i in 0..16 {
            messages.push(ChatMessage::user(format!("u{i}")));
            messages.push(ChatMessage::assistant(format!("a{i}")));
        }
        messages.push(ChatMessage::assistant(r#"{"tool_calls":[{"id":"call-old","name":"big_tool","arguments":"{}"}]}"#));
        messages.push(ChatMessage::tool(format!(r#"{{"tool_call_id":"call-old","content":"{huge}"}}"#)));
        messages.push(ChatMessage::assistant("continue"));
        assert!(messages.len() <= 50);
        let estimator = TokenEstimator::new();
        let before = estimator.estimate_messages_with_tools(&messages, &[]);
        let out = maintain_context(&provider, messages, &[], 50, true).await;
        let after = estimator.estimate_messages_with_tools(&out, &[]);
        assert!(out.iter().any(|m| m.content.contains("[Tool output pruned from model context]")));
        assert!(after < before);
        assert!(summaries.lock().unwrap().is_empty(), "count fallback should not run when only oversize triggered");
    }

    #[tokio::test]
    async fn unknown_budget_small_tool_results_no_unnecessary_maintenance() {
        let (provider, summaries) = UnknownBudgetProvider::new();
        let messages = vec![
            ChatMessage::system("bootstrap"),
            ChatMessage::user("goal"),
            ChatMessage::assistant(r#"{"tool_calls":[{"id":"call-1","name":"big_tool","arguments":"{}"}]}"#),
            ChatMessage::tool(r#"{"tool_call_id":"call-1","content":"small result"}"#.to_string()),
            ChatMessage::assistant("continue"),
        ];
        let out = maintain_context(&provider, messages, &[], 50, true).await;
        assert!(!out.iter().any(|m| m.content.contains("[Tool output pruned from model context]")));
        assert!(summaries.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn unknown_budget_message_count_fallback_still_functions() {
        let (provider, summaries) = UnknownBudgetProvider::new();
        let mut messages = vec![ChatMessage::system("bootstrap")];
        for i in 0..60 {
            messages.push(ChatMessage::user(format!("u{i}")));
            messages.push(ChatMessage::assistant(format!("a{i}")));
        }
        assert!(messages.len() > 50);
        let out = maintain_context(&provider, messages, &[], 50, true).await;
        assert!(out.len() < 122, "count-based compaction should shrink history");
        assert!(summaries.lock().unwrap().len() >= 1);
    }

    #[tokio::test]
    async fn unknown_budget_pruning_is_idempotent_for_already_pruned_content() {
        let (provider, _summaries) = UnknownBudgetProvider::new();
        let huge = "x".repeat(1_000_000);
        let mut tool = ChatMessage::tool(format!(r#"{{"tool_call_id":"call-1","content":"{huge}"}}"#));
        tool.original_tool_content = Some(huge.clone());
        tool.content = r#"{"tool_call_id":"call-1","content":"[Tool output pruned from model context]\n..."}"#.to_string();
        let messages = vec![
            ChatMessage::system("bootstrap"),
            ChatMessage::assistant(r#"{"tool_calls":[{"id":"call-1","name":"big_tool","arguments":"{}"}]}"#),
            tool,
            ChatMessage::assistant("continue"),
        ];
        let before_content = messages[2].content.clone();
        let out = maintain_context(&provider, messages, &[], 50, true).await;
        let out_tool = out.iter().find(|m| m.role == "tool").expect("tool remains");
        assert_eq!(out_tool.content, before_content);
        assert_eq!(out_tool.original_tool_content.as_deref(), Some(huge.as_str()));
    }

    #[tokio::test]
    async fn forced_recovery_unknown_budget_prunes_and_compacts() {
        let (provider, summaries) = UnknownBudgetProvider::new();
        let huge = "x".repeat(200_000);
        let mut messages = vec![ChatMessage::system("bootstrap")];
        for i in 0..8 {
            messages.push(ChatMessage::user(format!("u{i}")));
            messages.push(ChatMessage::assistant(format!("a{i}")));
        }
        messages.push(ChatMessage::assistant(r#"{"tool_calls":[{"id":"call-1","name":"big_tool","arguments":"{}"}]}"#));
        messages.push(ChatMessage::tool(format!(r#"{{"tool_call_id":"call-1","content":"{huge}"}}"#)));
        messages.push(ChatMessage::assistant("continue"));
        let out = force_context_recovery(&provider, messages, &[], 50).await;
        assert!(out.iter().any(|m| m.content.contains("[Tool output pruned from model context]")));
        assert!(summaries.lock().unwrap().len() >= 1, "forced recovery should run structured compaction");
    }

    #[tokio::test]
    async fn forced_recovery_known_budget_compacts_even_when_below_pressure() {
        let (provider, _, summaries) = BudgetProvider::new(200_000);
        let mut messages = vec![ChatMessage::system("bootstrap")];
        for i in 0..8 {
            messages.push(ChatMessage::user(format!("u{i} {}", "x".repeat(200))));
            messages.push(ChatMessage::assistant(format!("a{i} {}", "y".repeat(200))));
        }
        let estimator = TokenEstimator::new();
        let before = estimator.estimate_messages_with_tools(&messages, &[]);
        let out = force_context_recovery(&provider, messages, &[], 50).await;
        let after = estimator.estimate_messages_with_tools(&out, &[]);
        assert!(summaries.lock().unwrap().len() >= 1);
        assert!(after < before);
    }
}
