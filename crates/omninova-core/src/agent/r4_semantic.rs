//! R4 semantic compaction contract.
//!
//! Deterministic fixtures around structured checkpoints. No live LLM and no
//! paid Provider load.

use super::checkpoint_semantics::{
    extract_semantic_checkpoint, render_semantic_checkpoint, SemanticCheckpoint,
};
use super::context::maintain_context;
use super::history::{
    build_structured_checkpoint, retain_latest_checkpoint, CHECKPOINT_MARKER, TASK_MARKER,
};
use super::prompt::reconstruct_model_visible_messages;
use crate::config::AgentConfig;
use crate::observability::{
    build_snapshot, with_context_telemetry, ContextLifecycleEventKind, MeasurementKind,
    VecContextTelemetry,
};
use crate::providers::context_budget::{ContextBudget, ContextBudgetSource};
use crate::providers::native_request::{convert_messages, NativeChatRequest};
use crate::providers::{ChatMessage, ChatRequest, ChatResponse, Provider};
use crate::session::{derive_messages, events_from_messages};
use async_trait::async_trait;
use serde_json::json;
use std::sync::{Arc, Mutex};

fn task(goal: &str) -> ChatMessage {
    ChatMessage::system(format!("{TASK_MARKER} {goal}"))
}

fn user(content: &str) -> ChatMessage {
    ChatMessage::user(content)
}

fn assistant(content: &str) -> ChatMessage {
    ChatMessage::assistant(content)
}

fn tool(id: &str, content: &str) -> ChatMessage {
    ChatMessage::tool(json!({ "tool_call_id": id, "content": content }).to_string())
}

fn section<'a>(body: &'a str, heading: &str) -> &'a str {
    let start = body.find(heading).unwrap_or(0);
    let from = start + heading.len();
    let rest = &body[from..];
    let next = rest.find("\n## ").or_else(|| rest.find("\nCOMPLETED:"));
    match next {
        Some(idx) => &rest[..idx],
        None => rest,
    }
}

fn checkpoint_text(messages: &[ChatMessage], summary: &str) -> String {
    build_structured_checkpoint(messages, summary).content
}

fn semantic(messages: &[ChatMessage], summary: &str) -> SemanticCheckpoint {
    extract_semantic_checkpoint(messages, summary)
}

struct ScriptedSummarizer {
    budget: ContextBudget,
    summary: String,
    calls: Arc<Mutex<Vec<String>>>,
}

impl ScriptedSummarizer {
    fn new(summary: &str) -> (Self, Arc<Mutex<Vec<String>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                budget: ContextBudget::new(
                    50_000,
                    Some(16_384),
                    ContextBudgetSource::BuiltIn,
                ),
                summary: summary.to_string(),
                calls: calls.clone(),
            },
            calls,
        )
    }
}

#[async_trait]
impl Provider for ScriptedSummarizer {
    fn name(&self) -> &str {
        "r4-sum"
    }

    fn context_budget(&self) -> Option<ContextBudget> {
        Some(self.budget)
    }

    async fn chat(&self, request: ChatRequest<'_>) -> anyhow::Result<ChatResponse> {
        let is_summarizer = request
            .messages
            .iter()
            .any(|m| m.content.contains("你是对话摘要器"));
        if is_summarizer {
            self.calls.lock().unwrap().push(self.summary.clone());
            return Ok(ChatResponse {
                text: Some(self.summary.clone()),
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

fn pressure_history(extra: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let mut messages = vec![
        ChatMessage::system("OmniNova bootstrap. Primary goal: bounded context."),
        task("Harden context compaction semantics without changing budget ratios."),
    ];
    messages.extend(extra);
    for i in 0..20 {
        messages.push(user(&format!("pad-{i} {}", "x".repeat(400))));
        messages.push(assistant(&format!("ack-{i} {}", "y".repeat(400))));
    }
    messages
}

#[test]
fn r4_a_b_user_goal_and_constraints_retained() {
    let messages = vec![
        task("Harden compaction semantics on feature/testnew-context-security-hardening"),
        user("Do not push."),
        user("Do not modify DingTalk."),
        user("Preserve security check."),
        user("Only change the context subsystem."),
        user("No provider call on session open."),
        assistant("I will follow those constraints."),
    ];
    let body = checkpoint_text(&messages, "notes");
    assert!(body.contains("Harden compaction semantics"));
    let facts = section(&body, "## Important Facts");
    assert!(facts.contains("Do not push."));
    assert!(facts.contains("Do not modify DingTalk."));
    assert!(facts.contains("Preserve security check."));
    assert!(facts.contains("Only change the context subsystem."));
    assert!(facts.contains("No provider call on session open."));
    let goal = section(&body, "## Primary Goal");
    assert!(!goal.contains("Do not push."), "constraints must not replace the goal");
}

#[test]
fn r4_c_completed_vs_pending_not_collapsed() {
    let messages = vec![
        task("Implement foo then test and commit"),
        user("Plan:\n1. modify src/foo.rs\n2. test B\n3. commit C"),
        assistant(r#"{"tool_calls":[{"id":"t1","name":"apply_patch","arguments":"{}"}]}"#),
        tool("t1", "modified src/foo.rs"),
        assistant("done"),
    ];
    let state = semantic(&messages, "the whole plan is complete");
    assert!(
        state.completed.iter().any(|item| item.contains("src/foo.rs")),
        "completed={:?}",
        state.completed
    );
    assert!(
        state.pending.iter().any(|item| item.to_ascii_lowercase().contains("test")),
        "pending={:?}",
        state.pending
    );
    assert!(
        state.pending.iter().any(|item| item.to_ascii_lowercase().contains("commit")),
        "pending={:?}",
        state.pending
    );
    assert!(
        !state.completed.iter().any(|item| item.to_ascii_lowercase().contains("commit")),
        "commit must not be marked completed"
    );
    let body = render_semantic_checkpoint(&state);
    assert!(!body.to_ascii_lowercase().contains("the whole plan is complete"));
}

#[test]
fn r4_d_test_status_not_compressed_to_pass() {
    let skipped = vec![
        task("Validate workspace tests"),
        tool("t1", "workspace tests skipped"),
        assistant("all tests passed"),
    ];
    let blocked = vec![
        task("Live parity"),
        tool("t2", "live parity blocked because credential unavailable"),
        assistant("live parity passed"),
    ];
    let skipped_body = checkpoint_text(&skipped, "all tests passed");
    let blocked_body = checkpoint_text(&blocked, "live parity passed");
    assert!(
        skipped_body.to_ascii_lowercase().contains("skipped"),
        "{skipped_body}"
    );
    assert!(!skipped_body.to_ascii_lowercase().contains("all tests passed"));
    assert!(
        blocked_body.to_ascii_lowercase().contains("blocked"),
        "{blocked_body}"
    );
    assert!(!blocked_body.to_ascii_lowercase().contains("live parity passed"));
}

#[test]
fn r4_e_g_tool_facts_and_identifiers_preserved() {
    let messages = vec![
        task("Continue context runtime on the current branch"),
        user("Keep request_max_output_tokens distinct from max_output_tokens."),
        user("Model is deepseek-v4-flash."),
        assistant(r#"{"tool_calls":[{"id":"git","name":"shell","arguments":"{}"}]}"#),
        tool(
            "git",
            "On branch feature/context-runtime-v2\n modified src/foo.rs\ntest result: 1368 passed, 1 ignored",
        ),
    ];
    let body = checkpoint_text(&messages, "max_output_tokens is 32K");
    assert!(body.contains("feature/context-runtime-v2"));
    assert!(body.contains("src/foo.rs"));
    assert!(body.contains("1368 passed"));
    assert!(body.contains("1 ignored"));
    assert!(body.contains("request_max_output_tokens"));
    assert!(body.contains("deepseek-v4-flash"));
    assert!(
        !body.contains("max_output_tokens is 32K")
            || body.contains("request_max_output_tokens"),
        "must not rewrite request_max_output_tokens into max_output_tokens"
    );
}

#[test]
fn r4_f_unsupported_assistant_claim_not_promoted() {
    let messages = vec![
        task("Finish the patch"),
        assistant("done"),
        assistant("everything is done"),
    ];
    let state = semantic(&messages, "ROOT_CAUSE = Provider timeout\nall tests passed");
    assert!(
        state.completed.is_empty(),
        "unverified completion promoted: {:?}",
        state.completed
    );
    let body = render_semantic_checkpoint(&state);
    assert!(!body.contains("ROOT_CAUSE = Provider timeout"));
    assert!(!body.to_ascii_lowercase().contains("all tests passed"));
}

#[test]
fn r4_adversarial_hypothesis_loses_to_tool_evidence() {
    let messages = vec![
        task("Diagnose provider errors"),
        assistant("可能是 Provider timeout 导致。"),
        tool("t1", "provider returned HTTP 401 invalid credential"),
    ];
    let body = checkpoint_text(&messages, "ROOT_CAUSE = Provider timeout");
    assert!(!body.contains("ROOT_CAUSE = Provider timeout"));
    assert!(body.contains("401") || body.contains("credential"));
}

#[test]
fn r4_h_superseded_state_replaced() {
    let messages = vec![
        task("Tune generation limits"),
        user("request_max_output_tokens = 32K"),
        user("request_max_output_tokens = 64K"),
        tool("t1", "On branch feature/old"),
        tool("t2", "On branch feature/context-runtime-v2"),
        tool("t3", "tests failed: 2 failed"),
        tool("t4", "test result: 1368 passed, 1 ignored"),
    ];
    let body = checkpoint_text(&messages, "still 32K on feature/old");
    assert!(body.contains("request_max_output_tokens = 64K"));
    assert!(!body.contains("request_max_output_tokens = 32K"));
    assert!(body.contains("feature/context-runtime-v2"));
    assert!(!body.contains("feature/old"));
    assert!(body.contains("1368 passed"));
    assert!(!body.contains("2 failed"));
}

#[test]
fn r4_i_one_time_approval_not_generalized() {
    let messages = vec![
        task("Run a single diagnostic command"),
        user("Approve `git status` this once."),
        assistant("shell commands are approved"),
        tool("t1", "approved once: git status"),
    ];
    let body = checkpoint_text(&messages, "shell commands are approved");
    assert!(
        body.contains("this once") || body.contains("approved once"),
        "{body}"
    );
    assert!(!body.contains("shell commands are approved"));
}

#[test]
fn r4_obsolete_constraint_can_disappear() {
    let messages = vec![
        task("Context only"),
        user("Do not modify DingTalk."),
        user("DingTalk restriction is lifted."),
    ];
    let state = semantic(&messages, "");
    assert!(
        !state.constraints.iter().any(|c| c.contains("Do not modify DingTalk")),
        "obsolete constraint retained: {:?}",
        state.constraints
    );
}

#[test]
fn r4_e_dirty_git_keeps_commit_pending() {
    let messages = vec![
        task("Patch then commit"),
        user("Plan:\n1. modify src/foo.rs\n2. commit C"),
        tool("t1", "modified src/foo.rs"),
        tool("t2", "Changes not staged for commit\nmodified: src/foo.rs"),
    ];
    let state = semantic(&messages, "committed successfully");
    assert!(state.completed.iter().any(|item| item.contains("src/foo.rs")));
    assert!(
        !state.completed.iter().any(|item| looks_committed(item)),
        "completed={:?}",
        state.completed
    );
    assert!(state.pending.iter().any(|item| item.to_ascii_lowercase().contains("commit")));
}

fn looks_committed(item: &str) -> bool {
    let lower = item.to_ascii_lowercase();
    lower.contains("commit") && !lower.contains("pending")
}

#[test]
fn r4_k_l_p_multi_cycle_stable_and_bounded() {
    let mut messages = vec![
        task("Keep semantic state across compaction cycles"),
        user("Do not push."),
        user("request_max_output_tokens = 32K"),
        tool("t1", "On branch feature/context-runtime-v2"),
    ];
    let mut sizes = Vec::new();
    for cycle in 0..4 {
        if cycle == 2 {
            messages.push(user("request_max_output_tokens = 64K"));
        }
        messages.push(user(&format!("cycle-{cycle} progress")));
        let next = build_structured_checkpoint(&messages, "keep going");
        assert!(next.content.starts_with(CHECKPOINT_MARKER));
        let nested = next.content.matches("## Primary Goal").count();
        assert_eq!(nested, 1, "checkpoint prose must not nest, cycle {cycle}");
        sizes.push(next.content.chars().count());
        messages.retain(|m| !(m.role == "system" && m.content.starts_with(CHECKPOINT_MARKER)));
        messages.insert(1, next);
        let folded = retain_latest_checkpoint(messages.clone());
        assert_eq!(
            folded
                .iter()
                .filter(|m| m.role == "system" && m.content.starts_with(CHECKPOINT_MARKER))
                .count(),
            1
        );
        messages = folded;
    }
    let latest = messages
        .iter()
        .find(|m| m.content.starts_with(CHECKPOINT_MARKER))
        .unwrap();
    assert!(latest.content.contains("Do not push."));
    assert!(latest.content.contains("request_max_output_tokens = 64K"));
    assert!(!latest.content.contains("request_max_output_tokens = 32K"));
    assert!(latest.content.contains("feature/context-runtime-v2"));
    let max = *sizes.iter().max().unwrap();
    assert!(max < 4_000, "checkpoint size must stay bounded: {sizes:?}");
    assert!(
        sizes.last().copied().unwrap_or(0) < 4_000,
        "no progressive dump of history into the checkpoint"
    );
}

#[test]
fn r4_m_original_tool_content_excluded_from_checkpoint_and_native() {
    let mut hidden = tool("t1", "VISIBLE_BRANCH On branch feature/context-runtime-v2");
    hidden.original_tool_content = Some("HIDDEN_ORIGINAL_TOOL_CONTENT_R4".into());
    let messages = vec![
        task("Preserve hidden tool originals"),
        hidden,
    ];
    let checkpoint = build_structured_checkpoint(&messages, "");
    assert!(checkpoint.content.contains("feature/context-runtime-v2"));
    assert!(!checkpoint.content.contains("HIDDEN_ORIGINAL_TOOL_CONTENT_R4"));
    let native = serde_json::to_string(&NativeChatRequest {
        model: "deepseek-v4-flash".into(),
        messages: convert_messages(&[checkpoint]),
        temperature: 0.0,
        max_tokens: Some(32_000),
        tools: None,
        tool_choice: None,
        stream: None,
    })
    .unwrap();
    assert!(!native.contains("HIDDEN_ORIGINAL_TOOL_CONTENT_R4"));
}

#[test]
fn r4_n_o_session_restore_keeps_latest_semantic_state() {
    let checkpoint = build_structured_checkpoint(
        &[
            task("Continue after restart"),
            user("Do not push."),
            user("request_max_output_tokens = 64K"),
            tool("t1", "On branch feature/context-runtime-v2"),
        ],
        "",
    );
    let history = vec![
        ChatMessage::system("OmniNova bootstrap. Primary goal: bounded context."),
        checkpoint.clone(),
        user("restored turn"),
    ];
    let events = events_from_messages(&history);
    let restored = derive_messages(&events);
    let reconstructed = reconstruct_model_visible_messages(&AgentConfig::default(), restored);
    let restored_cp = reconstructed
        .iter()
        .find(|m| m.content.starts_with(CHECKPOINT_MARKER))
        .expect("checkpoint survives restore");
    assert!(restored_cp.content.contains("Do not push."));
    assert!(restored_cp.content.contains("request_max_output_tokens = 64K"));
    assert!(restored_cp.content.contains("feature/context-runtime-v2"));
    let snapshot = build_snapshot(
        Some("session-r4".into()),
        None,
        1,
        "deepseek",
        "deepseek-v4-flash",
        MeasurementKind::CandidateEstimate,
        &reconstructed,
        &[],
        None,
        None,
    );
    assert_eq!(snapshot.measurement_kind, MeasurementKind::CandidateEstimate);
    assert!(snapshot.run_id.is_none());
    assert!(
        reconstructed.iter().any(|m| m.content.contains("Continue after restart")),
        "first post-restore continuation still sees the latest checkpoint"
    );
}

#[tokio::test]
async fn r4_j_empty_summary_does_not_install_partial_checkpoint() {
    let (provider, calls) = ScriptedSummarizer::new("   ");
    let messages = pressure_history(vec![
        user("Do not push."),
        tool("t1", "On branch feature/context-runtime-v2"),
    ]);
    let sink = Arc::new(VecContextTelemetry::new());
    let out = with_context_telemetry(
        Some("session-r4".into()),
        Some("run-r4".into()),
        "r4-sum",
        "deepseek-v4-flash",
        sink.clone(),
        maintain_context(&provider, messages.clone(), &[], 10, true, None),
    )
    .await;
    assert!(
        !out.iter()
            .any(|m| m.role == "system" && m.content.starts_with(CHECKPOINT_MARKER)),
        "empty summary must not install a checkpoint"
    );
    assert!(calls.lock().unwrap().len() >= 1);
    let names: Vec<_> = sink
        .events()
        .iter()
        .map(|e| match e.kind {
            ContextLifecycleEventKind::ContextCompactionStarted { .. } => "started",
            ContextLifecycleEventKind::ContextCompactionCompleted { .. } => "completed",
            ContextLifecycleEventKind::ContextCompactionFailed { .. } => "failed",
            _ => "other",
        })
        .collect();
    assert!(names.contains(&"started"));
    assert!(names.contains(&"failed"));
    assert!(!names.contains(&"completed"));
}

#[tokio::test]
async fn r4_summarizer_hallucination_is_not_installed_as_fact() {
    let (provider, _) = ScriptedSummarizer::new(
        "ROOT_CAUSE = Provider timeout\nall tests passed\nshell commands are approved",
    );
    let messages = pressure_history(vec![
        user("Do not push."),
        assistant("可能是 Provider timeout 导致。"),
        tool("t1", "workspace tests skipped\nOn branch feature/context-runtime-v2"),
    ]);
    let out = maintain_context(&provider, messages, &[], 10, true, None).await;
    let checkpoint = out
        .iter()
        .find(|m| m.content.starts_with(CHECKPOINT_MARKER))
        .expect("successful compact still writes a checkpoint");
    assert!(checkpoint.content.contains("Do not push."));
    assert!(checkpoint.content.to_ascii_lowercase().contains("skipped"));
    assert!(!checkpoint.content.contains("ROOT_CAUSE = Provider timeout"));
    assert!(!checkpoint.content.to_ascii_lowercase().contains("all tests passed"));
    assert!(!checkpoint.content.contains("shell commands are approved"));
}

#[test]
fn r4_important_facts_are_not_a_goal_duplicate() {
    let body = checkpoint_text(
        &[
            task("Unique goal text for R4"),
            user("Do not push."),
        ],
        "",
    );
    let facts = section(&body, "## Important Facts");
    assert!(!facts.contains("Unique goal text for R4"));
    assert!(facts.contains("Do not push."));
}

#[test]
fn r4_r_product_budget_constants_unchanged() {
    use crate::providers::context_budget::{PRESSURE_THRESHOLD_RATIO, TARGET_AFTER_COMPACTION_RATIO};
    use crate::providers::generation_limit::PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS;
    assert_eq!(PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS, 32_000);
    assert_eq!(PRESSURE_THRESHOLD_RATIO, 0.80);
    assert_eq!(TARGET_AFTER_COMPACTION_RATIO, 0.55);
}
