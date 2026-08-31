//! R5 deterministic final-acceptance fixtures for Context Runtime.
//! No live Provider calls and no paid million-token traffic.

use super::checkpoint_semantics::extract_semantic_checkpoint;
use super::context::maintain_context;
use super::history::{CHECKPOINT_MARKER, TASK_MARKER};
use super::prompt::reconstruct_model_visible_messages;
use crate::config::AgentConfig;
use crate::observability::{
    with_context_telemetry, ContextLifecycleEvent, ContextLifecycleEventKind, VecContextTelemetry,
};
use crate::providers::context_budget::{ContextBudget, ContextBudgetSource};
use crate::providers::generation_limit::{resolve_generation_limit, GenerationLimitSource};
use crate::providers::{ChatMessage, ChatRequest, ChatResponse, Provider};
use crate::session::{load_messages, save_messages};
use async_trait::async_trait;
use serde_json::json;
use std::sync::{Arc, Mutex};

const R5_WINDOW: u64 = 160_000;
const R5_MODEL_MAX_OUTPUT: u64 = 32_000;

#[derive(Clone)]
struct R5Provider {
    budget: ContextBudget,
    summaries: Arc<Mutex<usize>>,
}

impl R5Provider {
    fn new() -> (Self, Arc<Mutex<usize>>) {
        let summaries = Arc::new(Mutex::new(0));
        let budget = ContextBudget::new(
            R5_WINDOW,
            Some(R5_MODEL_MAX_OUTPUT),
            ContextBudgetSource::BuiltIn,
        )
        .with_resolved_generation_limit(resolve_generation_limit(None, Some(R5_MODEL_MAX_OUTPUT)));
        (
            Self {
                budget,
                summaries: summaries.clone(),
            },
            summaries,
        )
    }
}

#[async_trait]
impl Provider for R5Provider {
    fn name(&self) -> &str {
        "r5-provider"
    }

    fn context_budget(&self) -> Option<ContextBudget> {
        Some(self.budget)
    }

    async fn chat(&self, request: ChatRequest<'_>) -> anyhow::Result<ChatResponse> {
        let is_summary = request
            .messages
            .iter()
            .any(|message| message.content.contains("你是对话摘要器"));
        assert!(
            is_summary,
            "R5 maintenance fixture must only call the summarizer"
        );
        *self.summaries.lock().unwrap() += 1;
        Ok(ChatResponse {
            text: Some("Continue from the authoritative structured checkpoint.".to_string()),
            tool_calls: Vec::new(),
            usage: None,
            reasoning_content: None,
            finish_reason: Some("stop".to_string()),
        })
    }

    async fn health_check(&self) -> bool {
        true
    }
}

fn task(goal: &str) -> ChatMessage {
    ChatMessage::system(format!("{TASK_MARKER} {goal}"))
}

fn tool(id: &str, content: &str) -> ChatMessage {
    ChatMessage::tool(json!({ "tool_call_id": id, "content": content }).to_string())
}

fn add_history_growth(messages: &mut Vec<ChatMessage>, cycle: usize) {
    for index in 0..18 {
        messages.push(ChatMessage::user(format!(
            "cycle-{cycle}-user-{index}\n{}",
            "u".repeat(3_000)
        )));
        messages.push(ChatMessage::assistant(format!(
            "cycle-{cycle}-assistant-{index}\n{}",
            "a".repeat(3_000)
        )));
    }
}

fn checkpoint(messages: &[ChatMessage]) -> &ChatMessage {
    messages
        .iter()
        .find(|message| message.role == "system" && message.content.starts_with(CHECKPOINT_MARKER))
        .expect("structured checkpoint")
}

fn checkpoint_count(messages: &[ChatMessage]) -> usize {
    messages
        .iter()
        .filter(|message| {
            message.role == "system" && message.content.starts_with(CHECKPOINT_MARKER)
        })
        .count()
}

fn lifecycle_name(kind: &ContextLifecycleEventKind) -> &'static str {
    match kind {
        ContextLifecycleEventKind::ContextPressureDetected { .. } => "pressure",
        ContextLifecycleEventKind::ContextPruningStarted { .. } => "pruning_started",
        ContextLifecycleEventKind::ContextPruningCompleted { .. } => "pruning_completed",
        ContextLifecycleEventKind::ContextCompactionStarted { .. } => "compaction_started",
        ContextLifecycleEventKind::ContextCompactionCompleted { .. } => "compaction_completed",
        ContextLifecycleEventKind::ContextCompactionFailed { .. } => "compaction_failed",
        ContextLifecycleEventKind::ContextOverflowRecoveryStarted { .. } => "overflow_started",
        ContextLifecycleEventKind::ContextOverflowRecoveryCompleted { .. } => "overflow_completed",
        ContextLifecycleEventKind::ContextOverflowRecoveryFailed { .. } => "overflow_failed",
    }
}

fn assert_truthful_terminals(events: &[ContextLifecycleEvent]) {
    for event in events {
        let terminal_names: &[&str] = match lifecycle_name(&event.kind) {
            "pruning_started" => &["pruning_completed"],
            "compaction_started" => &["compaction_completed", "compaction_failed"],
            "overflow_started" => &["overflow_completed", "overflow_failed"],
            _ => continue,
        };
        let terminals = events
            .iter()
            .filter(|candidate| {
                candidate.operation_id == event.operation_id
                    && terminal_names.contains(&lifecycle_name(&candidate.kind))
            })
            .count();
        assert_eq!(
            terminals, 1,
            "operation {} must have exactly one truthful terminal event",
            event.operation_id
        );
    }
}

async fn maintain(
    provider: &R5Provider,
    messages: Vec<ChatMessage>,
    run_id: &str,
) -> (Arc<VecContextTelemetry>, Vec<ChatMessage>) {
    let sink = Arc::new(VecContextTelemetry::new());
    let out = with_context_telemetry(
        Some("session-r5".to_string()),
        Some(run_id.to_string()),
        provider.name(),
        "deepseek-v4-flash",
        sink.clone(),
        maintain_context(provider, messages, &[], 12, true, Some(64_000)),
    )
    .await;
    (sink, out)
}

#[tokio::test]
async fn r5_real_continuation_survives_two_compactions_restart_and_reload() {
    let (provider, summary_calls) = R5Provider::new();
    let mut messages = vec![
        ChatMessage::system("OmniNova bootstrap. Keep the active task truthful."),
        task("Fix a Context Runtime issue and finish its release acceptance."),
        ChatMessage::user("Do not push."),
        ChatMessage::user("Do not remove security validation."),
        ChatMessage::user("Keep Provider calls off session-open."),
        ChatMessage::user("Only change the context subsystem in workspace E:/novalclaw."),
        ChatMessage::user("request_max_output_tokens = 64K"),
        ChatMessage::user(
            "Plan:\n1. modify crates/omninova-core/src/agent/context.rs\n2. run test suite B\n3. run desktop acceptance\n4. commit only after approval",
        ),
        ChatMessage::assistant("Maybe the root cause is a stale global token counter."),
        tool(
            "inspect-1",
            &format!(
                "FILES_ALREADY_INSPECTED = crates/omninova-core/src/agent/context.rs crates/omninova-core/src/agent/checkpoint_semantics.rs\nCURRENT_ROOT_CAUSE = active request budget was not reused by maintenance\nWORK_ALREADY_COMPLETED = modified crates/omninova-core/src/agent/context.rs\nNEXT_PENDING_ACTION = run desktop acceptance\nOn branch feature/testnew-context-security-hardening\ntest suite B failed once\n{}",
                "z".repeat(45_000)
            ),
        ),
        tool("test-latest", "test result: suite B passed: 84 passed; 0 failed"),
        ChatMessage::user("Approve `git status` this once."),
        tool("approval", "approved once: git status"),
    ];
    add_history_growth(&mut messages, 1);

    let (cycle_one_events, mut first) = maintain(&provider, messages, "run-r5-1").await;
    let first_names: Vec<_> = cycle_one_events
        .events()
        .iter()
        .map(|event| lifecycle_name(&event.kind))
        .collect();
    assert!(first_names.contains(&"pressure"));
    assert!(first_names.contains(&"pruning_completed"));
    assert!(first_names.contains(&"compaction_completed"));
    assert_truthful_terminals(&cycle_one_events.events());
    assert_eq!(checkpoint_count(&first), 1);
    let first_size = checkpoint(&first).content.chars().count();

    first.push(ChatMessage::user(
        "Continue the same task. Desktop acceptance and commit remain pending. Do not push.",
    ));
    first.push(tool(
        "inspect-2",
        "FILES_ALREADY_INSPECTED = crates/omninova-core/src/agent/context.rs crates/omninova-core/src/agent/checkpoint_semantics.rs crates/omninova-core/src/gateway/context_projection.rs\nCURRENT_ROOT_CAUSE = active request budget was not reused by maintenance\nWORK_ALREADY_COMPLETED = context maintenance fix and suite B validation\nNEXT_PENDING_ACTION = run desktop acceptance\nOn branch feature/testnew-context-security-hardening\ntest result: suite B passed: 84 passed; 0 failed",
    ));
    add_history_growth(&mut first, 2);

    let (cycle_two_events, second) = maintain(&provider, first, "run-r5-2").await;
    let second_names: Vec<_> = cycle_two_events
        .events()
        .iter()
        .map(|event| lifecycle_name(&event.kind))
        .collect();
    assert!(second_names.contains(&"pressure"));
    assert!(second_names.contains(&"compaction_completed"));
    assert_truthful_terminals(&cycle_two_events.events());
    assert_eq!(
        checkpoint_count(&second),
        1,
        "checkpoints must replace, not stack"
    );
    let second_checkpoint = checkpoint(&second);
    let second_size = second_checkpoint.content.chars().count();
    assert_eq!(
        second_checkpoint.content.matches("## Primary Goal").count(),
        1
    );
    assert!(
        second_size <= first_size.saturating_add(1_500),
        "new checkpoint must not duplicate the superseded checkpoint: {first_size} -> {second_size}"
    );

    let dir = std::env::temp_dir().join(format!("omninova-r5-{}", uuid::Uuid::new_v4()));
    save_messages(&dir, "session-a", &second)
        .await
        .expect("persist compacted session");
    let before_reload_calls = *summary_calls.lock().unwrap();
    let loaded = load_messages(&dir, "session-a")
        .await
        .expect("reload session")
        .expect("session exists");
    let reconstructed = reconstruct_model_visible_messages(&AgentConfig::default(), loaded.clone());
    assert_eq!(
        *summary_calls.lock().unwrap(),
        before_reload_calls,
        "session-open reconstruction must not call Provider"
    );
    assert_eq!(checkpoint_count(&reconstructed), 1);

    let state = extract_semantic_checkpoint(&reconstructed, "");
    assert!(state.goal.contains("Fix a Context Runtime issue"));
    assert!(state
        .constraints
        .iter()
        .any(|item| item.contains("Do not push")));
    assert!(state
        .constraints
        .iter()
        .any(|item| item.contains("security validation")));
    assert!(state
        .constraints
        .iter()
        .any(|item| item.contains("Provider calls off session-open")));
    assert!(state
        .constraints
        .iter()
        .any(|item| item.contains("workspace E:/novalclaw")));
    assert!(state
        .current
        .iter()
        .any(|item| item.contains("request_max_output_tokens = 64K")));
    assert!(state
        .current
        .iter()
        .any(|item| item.contains("feature/testnew-context-security-hardening")));
    assert!(state
        .current
        .iter()
        .any(|item| item.contains("active request budget")));
    assert!(state
        .completed
        .iter()
        .any(|item| item.contains("context maintenance fix")));
    assert!(state
        .pending
        .iter()
        .any(|item| item.contains("desktop acceptance")));
    assert!(state.pending.iter().any(|item| item.contains("commit")));
    assert!(state
        .references
        .iter()
        .any(|item| item.contains("context_projection.rs")));
    assert!(state
        .references
        .iter()
        .any(|item| item.contains("84 passed")));
    assert!(!second_checkpoint
        .content
        .contains("stale global token counter"));
    assert!(!second_checkpoint
        .content
        .contains("test suite B failed once"));
    assert!(!state.failures.iter().any(|item| item.contains("84 passed")));
    assert!(!second_checkpoint
        .content
        .contains("shell commands are approved"));
    assert!(
        second_checkpoint.content.contains("approved once")
            || second_checkpoint.content.contains("this once")
    );

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn r5_session_switching_keeps_checkpoints_and_projection_inputs_isolated() {
    let dir = std::env::temp_dir().join(format!("omninova-r5-switch-{}", uuid::Uuid::new_v4()));
    let a = vec![
        task("Session A goal"),
        super::history::build_structured_checkpoint(
            &[
                task("Session A goal"),
                ChatMessage::user("request_max_output_tokens = 64K"),
                tool("a", "On branch feature/session-a\ntest result: A passed"),
            ],
            "",
        ),
    ];
    let b = vec![
        task("Session B goal"),
        super::history::build_structured_checkpoint(
            &[
                task("Session B goal"),
                ChatMessage::user("request_max_output_tokens = 32K"),
                tool("b", "On branch feature/session-b\ntest result: B skipped"),
            ],
            "",
        ),
    ];
    save_messages(&dir, "session-a", &a).await.expect("save A");
    save_messages(&dir, "session-b", &b).await.expect("save B");

    let loaded_a = load_messages(&dir, "session-a").await.unwrap().unwrap();
    let loaded_b = load_messages(&dir, "session-b").await.unwrap().unwrap();
    let loaded_a_again = load_messages(&dir, "session-a").await.unwrap().unwrap();
    let a_text = checkpoint(&loaded_a).content.clone();
    let b_text = checkpoint(&loaded_b).content.clone();
    assert!(a_text.contains("Session A goal") && a_text.contains("feature/session-a"));
    assert!(!a_text.contains("Session B goal") && !a_text.contains("feature/session-b"));
    assert!(b_text.contains("Session B goal") && b_text.contains("feature/session-b"));
    assert!(!b_text.contains("Session A goal") && !b_text.contains("feature/session-a"));
    assert_eq!(checkpoint(&loaded_a_again).content, a_text);

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[test]
fn r5_budget_authority_matrix_matches_native_reserve_values() {
    use crate::providers::context_budget::DEFAULT_SAFETY_RESERVE_TOKENS;

    let base = ContextBudget::new(1_000_000, Some(384_000), ContextBudgetSource::BuiltIn);
    let product =
        base.with_resolved_generation_limit(resolve_generation_limit(None, Some(384_000)));
    let profile = base.with_resolved_generation_limit(
        crate::providers::generation_limit::resolve_effective_request_generation_limit(
            None,
            Some(64_000),
            None,
            Some(384_000),
        ),
    );
    let request = product.with_request_generation_override(Some(128_000));

    let cases = [
        (
            product,
            32_000,
            935_232,
            GenerationLimitSource::ProductDefault,
        ),
        (
            profile,
            64_000,
            903_232,
            GenerationLimitSource::ProfileOverride,
        ),
        (
            request,
            128_000,
            839_232,
            GenerationLimitSource::RequestOverride,
        ),
    ];
    for (budget, native_cap, max_input, source) in cases {
        assert_eq!(budget.request_output_reserve_tokens, native_cap);
        assert_eq!(budget.max_input_tokens, max_input);
        assert_eq!(budget.request_generation_limit_source, source);
        assert_eq!(
            budget.context_window_tokens
                - budget.request_output_reserve_tokens
                - DEFAULT_SAFETY_RESERVE_TOKENS,
            max_input
        );
        assert_eq!(
            budget.pressure_threshold(),
            (max_input as f64 * 0.80).floor() as u64
        );
    }
}
