//! R3 Flash-budget context lifecycle E2E matrix.
//!
//! Synthetic fixtures + real TokenEstimator / ContextBudget / maintain_context.
//! No live Provider load test and no million-token paid traffic.

use super::context::{force_context_recovery, maintain_context};
use super::history::{CHECKPOINT_MARKER, SUMMARY_MARKER};
use crate::config::ModelProviderConfig;
use crate::observability::{
    build_snapshot, with_context_telemetry, ContextLifecycleEventKind, MeasurementKind,
    VecContextTelemetry,
};
use crate::providers::context_budget::{
    ContextBudget, ContextBudgetSource, TokenEstimator, CONTEXT_BUDGET_EXCEEDED_MARKER,
    PRESSURE_THRESHOLD_RATIO, TARGET_AFTER_COMPACTION_RATIO, UNKNOWN_BUDGET_TOOL_SOFT_CAP_TOKENS,
};
use crate::providers::generation_limit::{
    resolve_effective_request_generation_limit, resolve_generation_limit, GenerationLimitSource,
    PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS,
};
use crate::providers::native_request::{convert_messages, convert_tools, NativeChatRequest};
use crate::providers::{ChatMessage, ChatRequest, ChatResponse, Provider, TokenUsage};
use crate::tools::ToolSpec;
use async_trait::async_trait;
use serde_json::json;
use std::sync::{Arc, Mutex};

const FLASH_WINDOW: u64 = 1_000_000;
const FLASH_MODEL_MAX_OUTPUT: u64 = 384_000;
const SAFETY: u64 = 32_768;
const CROSSOVER_TOKENS: u64 = 735_000;

fn flash_base() -> ContextBudget {
    ContextBudget::new(
        FLASH_WINDOW,
        Some(FLASH_MODEL_MAX_OUTPUT),
        ContextBudgetSource::BuiltIn,
    )
}

fn flash_product_32k() -> ContextBudget {
    flash_base().with_resolved_generation_limit(resolve_generation_limit(
        None,
        Some(FLASH_MODEL_MAX_OUTPUT),
    ))
}

fn flash_profile_64k() -> ContextBudget {
    let profile = ModelProviderConfig {
        request_max_output_tokens: Some(64_000),
        ..ModelProviderConfig::default()
    };
    flash_base().with_resolved_generation_limit(resolve_generation_limit(
        Some(&profile),
        Some(FLASH_MODEL_MAX_OUTPUT),
    ))
}

fn flash_request_128k() -> ContextBudget {
    flash_product_32k().with_request_generation_override(Some(128_000))
}

fn expected_max_input(reserve: u64) -> u64 {
    FLASH_WINDOW - reserve - SAFETY
}

fn ratio_floor(value: u64, ratio: f64) -> u64 {
    (value as f64 * ratio).floor() as u64
}

#[derive(Clone)]
struct FlashProvider {
    budget: ContextBudget,
    summaries: Arc<Mutex<Vec<String>>>,
}

impl FlashProvider {
    fn new(budget: ContextBudget) -> (Self, Arc<Mutex<Vec<String>>>) {
        let summaries = Arc::new(Mutex::new(Vec::new()));
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
impl Provider for FlashProvider {
    fn name(&self) -> &str {
        "r3-flash"
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
            self.summaries
                .lock()
                .unwrap()
                .push("r3 structured summary".to_string());
            return Ok(ChatResponse {
                text: Some("r3 structured summary".to_string()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
                finish_reason: Some("stop".to_string()),
            });
        }
        Ok(ChatResponse {
            text: Some("final".to_string()),
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
    summaries: Arc<Mutex<Vec<String>>>,
}

impl UnknownBudgetProvider {
    fn new() -> (Self, Arc<Mutex<Vec<String>>>) {
        let summaries = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                summaries: summaries.clone(),
            },
            summaries,
        )
    }
}

#[async_trait]
impl Provider for UnknownBudgetProvider {
    fn name(&self) -> &str {
        "r3-unknown"
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
            self.summaries
                .lock()
                .unwrap()
                .push("unknown summary".to_string());
            return Ok(ChatResponse {
                text: Some("unknown summary".to_string()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
                finish_reason: Some("stop".to_string()),
            });
        }
        Ok(ChatResponse {
            text: Some("final".to_string()),
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

fn estimate(messages: &[ChatMessage]) -> u64 {
    TokenEstimator::new().estimate_messages_with_tools(messages, &[])
}

fn pad_user_to_tokens(target: u64) -> Vec<ChatMessage> {
    let estimator = TokenEstimator::new();
    let system = ChatMessage::system(
        "OmniNova bootstrap. Primary goal: bounded context. Constraint: stay truthful.",
    );
    let overhead = estimator.estimate_messages_with_tools(
        &[system.clone(), ChatMessage::user("turn\n")],
        &[],
    );
    let need = target.saturating_sub(overhead).max(8);
    let n = need.saturating_sub(4).saturating_mul(4) / 5;
    let mut messages = vec![
        system.clone(),
        ChatMessage::user(format!("turn\n{}", "x".repeat(n as usize))),
    ];
    let mut guard = 0u32;
    loop {
        let est = estimator.estimate_messages_with_tools(&messages, &[]);
        if est >= target {
            break;
        }
        let deficit = (target - est) as usize;
        let grow = (deficit * 4 / 5).max(1);
        messages[1].content.push_str(&"x".repeat(grow));
        guard += 1;
        assert!(guard < 32, "could not size fixture to {target}, est={est}");
    }
    messages
}

fn many_turns(turns: usize, chars_each: usize) -> Vec<ChatMessage> {
    let mut messages = vec![ChatMessage::system(
        "OmniNova bootstrap. Primary goal: bounded context. Constraint: stay truthful.",
    )];
    for i in 0..turns {
        messages.push(ChatMessage::user(format!(
            "turn-{i} goal continue\n{}",
            "u".repeat(chars_each)
        )));
        messages.push(ChatMessage::assistant(format!(
            "ack-{i} keeping task state\n{}",
            "a".repeat(chars_each)
        )));
    }
    messages
}

fn tool_json(id: &str, content: &str) -> String {
    json!({ "tool_call_id": id, "content": content }).to_string()
}

fn native_body(messages: &[ChatMessage], tools: &[ToolSpec], max_tokens: u32) -> String {
    serde_json::to_string(&NativeChatRequest {
        model: "deepseek-v4-flash".into(),
        messages: convert_messages(messages),
        temperature: 0.0,
        max_tokens: Some(max_tokens),
        tools: convert_tools(Some(tools)),
        tool_choice: tools.first().map(|_| "auto".to_string()),
        stream: None,
    })
    .expect("serialize native request")
}

fn sample_tool() -> ToolSpec {
    ToolSpec {
        name: "unique_r3_tool".into(),
        description: "r3 tool schema must appear once".into(),
        parameters: json!({ "type": "object", "properties": {} }),
    }
}

fn system_copies(messages: &[ChatMessage], needle: &str) -> usize {
    messages
        .iter()
        .filter(|m| m.role == "system" && m.content.contains(needle))
        .count()
}

fn checkpoint_count(messages: &[ChatMessage]) -> usize {
    messages
        .iter()
        .filter(|m| m.role == "system" && m.content.starts_with(CHECKPOINT_MARKER))
        .count()
}

fn pruned(messages: &[ChatMessage]) -> bool {
    messages
        .iter()
        .any(|m| m.content.contains("[Tool output pruned from model context]"))
}

async fn with_events<F, Fut, T>(f: F) -> (Arc<VecContextTelemetry>, T)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let sink = Arc::new(VecContextTelemetry::new());
    let out = with_context_telemetry(
        Some("session-r3".into()),
        Some("run-r3".into()),
        "r3-flash",
        "deepseek-v4-flash",
        sink.clone(),
        f(),
    )
    .await;
    (sink, out)
}

fn kind_name(kind: &ContextLifecycleEventKind) -> &'static str {
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

#[test]
fn r3_budgets_product_profile_and_request_override() {
    assert_eq!(
        PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS, 32_000,
        "R3 must not change the product default"
    );
    assert_eq!(PRESSURE_THRESHOLD_RATIO, 0.80);
    assert_eq!(TARGET_AFTER_COMPACTION_RATIO, 0.55);

    let product = flash_product_32k();
    assert_eq!(product.max_input_tokens, expected_max_input(32_000));
    assert_eq!(product.max_input_tokens, 935_232);
    assert_eq!(
        product.pressure_threshold(),
        ratio_floor(935_232, PRESSURE_THRESHOLD_RATIO)
    );
    assert_eq!(product.pressure_threshold(), 748_185);
    assert_eq!(
        product.target_after_compaction(),
        ratio_floor(935_232, TARGET_AFTER_COMPACTION_RATIO)
    );
    assert_eq!(
        product.request_generation_limit_source,
        GenerationLimitSource::ProductDefault
    );

    let profile = flash_profile_64k();
    assert_eq!(profile.max_input_tokens, expected_max_input(64_000));
    assert_eq!(profile.max_input_tokens, 903_232);
    assert_eq!(profile.pressure_threshold(), 722_585);
    assert_eq!(
        profile.request_generation_limit_source,
        GenerationLimitSource::ProfileOverride
    );

    let request = flash_request_128k();
    assert_eq!(request.max_input_tokens, expected_max_input(128_000));
    assert_eq!(request.max_input_tokens, 839_232);
    assert_eq!(request.pressure_threshold(), 671_385);
    assert_eq!(
        request.request_generation_limit_source,
        GenerationLimitSource::RequestOverride
    );

    assert!(
        product.pressure_threshold() > CROSSOVER_TOKENS
            && profile.pressure_threshold() < CROSSOVER_TOKENS
            && request.pressure_threshold() < CROSSOVER_TOKENS,
        "crossover fixture must sit between 32K and 64K/128K pressure"
    );
}

#[tokio::test]
async fn r3_a_b_below_and_just_below_pressure_no_maintenance() {
    let budget = flash_product_32k();
    let (provider, summaries) = FlashProvider::new(budget);
    let fifty = pad_user_to_tokens((budget.max_input_tokens as f64 * 0.50).floor() as u64);
    let just_below = pad_user_to_tokens(budget.pressure_threshold().saturating_sub(8));
    assert!(estimate(&fifty) < budget.pressure_threshold());
    assert!(estimate(&just_below) <= budget.pressure_threshold());
    assert!(estimate(&fifty) as f64 / budget.max_input_tokens as f64 > 0.49);

    let out_50 = maintain_context(&provider, fifty.clone(), &[], 80, true, None).await;
    let out_79 = maintain_context(&provider, just_below.clone(), &[], 80, true, None).await;
    assert_eq!(out_50.len(), fifty.len());
    assert_eq!(out_79.len(), just_below.len());
    assert!(!pruned(&out_50) && !pruned(&out_79));
    assert!(summaries.lock().unwrap().is_empty());
    assert_eq!(checkpoint_count(&out_50), 0);
    assert_eq!(checkpoint_count(&out_79), 0);
}

#[tokio::test]
async fn r3_c_just_above_pressure_maintenance_begins() {
    let budget = flash_product_32k();
    let (provider, _summaries) = FlashProvider::new(budget);
    let just_above = pad_user_to_tokens(budget.pressure_threshold().saturating_add(32));
    assert!(estimate(&just_above) > budget.pressure_threshold());
    assert!((estimate(&just_above) as f64 / budget.max_input_tokens as f64) > 0.80);

    let (sink, out) = with_events(|| async {
        maintain_context(&provider, just_above, &[], 80, true, None).await
    })
    .await;
    let names: Vec<_> = sink.events().iter().map(|e| kind_name(&e.kind)).collect();
    assert!(
        names.contains(&"pressure"),
        "just above threshold must start maintenance: {names:?}"
    );
    assert!(names.contains(&"pruning_started"));
    assert!(names.contains(&"pruning_completed"));
    let _ = out;
}

#[tokio::test]
async fn r3_c_d_same_fixture_crosses_pressure_only_under_64k() {
    let product = flash_product_32k();
    let profile = flash_profile_64k();
    let fixture = pad_user_to_tokens(CROSSOVER_TOKENS);
    let usage = estimate(&fixture);
    assert!(
        usage < product.pressure_threshold(),
        "crossover {usage} must stay below 32K pressure {}",
        product.pressure_threshold()
    );
    assert!(
        usage > profile.pressure_threshold(),
        "crossover {usage} must exceed 64K pressure {}",
        profile.pressure_threshold()
    );

    let (p32, s32) = FlashProvider::new(product);
    let (p64, _s64) = FlashProvider::new(profile);
    let (sink32, out32) = with_events(|| async {
        maintain_context(&p32, fixture.clone(), &[], 80, true, None).await
    })
    .await;
    let (sink64, out64) = with_events(|| async {
        maintain_context(&p64, fixture.clone(), &[], 80, true, None).await
    })
    .await;

    let names32: Vec<_> = sink32.events().iter().map(|e| kind_name(&e.kind)).collect();
    let names64: Vec<_> = sink64.events().iter().map(|e| kind_name(&e.kind)).collect();
    assert!(
        !names32.contains(&"pressure"),
        "32K policy must not maintain the crossover fixture: {names32:?}"
    );
    assert!(s32.lock().unwrap().is_empty());
    assert_eq!(out32.len(), fixture.len());
    assert!(
        names64.contains(&"pressure"),
        "64K policy must maintain the same fixture: {names64:?}"
    );
    let _ = out64;
}

#[tokio::test]
async fn r3_e_request_override_128k_then_unrelated_returns_to_profile() {
    let product = flash_product_32k();
    let fixture = pad_user_to_tokens(CROSSOVER_TOKENS);
    let usage = estimate(&fixture);
    assert!(usage < product.pressure_threshold());
    assert!(usage > flash_request_128k().pressure_threshold());

    let (provider, summaries) = FlashProvider::new(product);
    let (sink_override, _) = with_events(|| async {
        maintain_context(
            &provider,
            fixture.clone(),
            &[],
            80,
            true,
            Some(128_000),
        )
        .await
    })
    .await;
    let names_override: Vec<_> = sink_override
        .events()
        .iter()
        .map(|e| kind_name(&e.kind))
        .collect();
    assert!(
        names_override.contains(&"pressure"),
        "128K request override must use the smaller input budget: {names_override:?}"
    );

    let (sink_next, out_next) = with_events(|| async {
        maintain_context(&provider, fixture.clone(), &[], 80, true, None).await
    })
    .await;
    let names_next: Vec<_> = sink_next.events().iter().map(|e| kind_name(&e.kind)).collect();
    assert!(
        !names_next.contains(&"pressure"),
        "next unrelated request must return to the 32K profile/product budget: {names_next:?}"
    );
    assert_eq!(out_next.len(), fixture.len());
    let _ = summaries;
}

#[tokio::test]
async fn r3_f_pruning_preserves_original_and_is_not_compaction() {
    let budget = flash_product_32k();
    let (provider, summaries) = FlashProvider::new(budget);
    let hidden = format!(
        "{}HIDDEN_MIDDLE_SENTINEL_R3{}",
        "H".repeat(2_000),
        "T".repeat(80_000)
    );
    let recent = pad_user_to_tokens(budget.pressure_threshold().saturating_add(4_000));
    let historical = vec![
        ChatMessage::system(
            "OmniNova bootstrap. Primary goal: bounded context. Constraint: stay truthful.",
        ),
        ChatMessage::assistant(
            r#"{"tool_calls":[{"id":"call-old","name":"unique_r3_tool","arguments":"{}"}]}"#,
        ),
        ChatMessage::tool(tool_json("call-old", &hidden)),
        recent
            .into_iter()
            .find(|m| m.role == "user")
            .expect("padded user"),
    ];
    assert!(estimate(&historical) > budget.pressure_threshold());

    let (sink, out) = with_events(|| async {
        maintain_context(&provider, historical, &[], 80, true, None).await
    })
    .await;
    assert!(pruned(&out), "eligible historical tool result must prune");
    let tool = out
        .iter()
        .find(|m| m.role == "tool")
        .expect("tool message remains");
    assert!(tool.content.contains("[Tool output pruned from model context]"));
    assert_eq!(
        tool.original_tool_content.as_deref(),
        Some(hidden.as_str()),
        "original_tool_content must remain for durability"
    );
    let body = native_body(&out, &[], 32_000);
    assert!(
        !body.contains("HIDDEN_MIDDLE_SENTINEL_R3"),
        "omitted original tool middle must never be Provider-visible"
    );
    assert!(body.contains("[Tool output pruned from model context]"));

    let names: Vec<_> = sink.events().iter().map(|e| kind_name(&e.kind)).collect();
    assert!(names.contains(&"pruning_started") && names.contains(&"pruning_completed"));
    let prune_ops: Vec<_> = sink
        .events()
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                ContextLifecycleEventKind::ContextPruningStarted { .. }
                    | ContextLifecycleEventKind::ContextPruningCompleted { .. }
            )
        })
        .map(|e| e.operation_id.clone())
        .collect();
    assert_eq!(prune_ops.len(), 2);
    assert_eq!(prune_ops[0], prune_ops[1]);
    if names.contains(&"compaction_started") {
        assert!(
            !summaries.lock().unwrap().is_empty() || names.contains(&"compaction_failed"),
            "compaction must be a distinct truthful lifecycle, not a prune relabel"
        );
    }
}

#[tokio::test]
async fn r3_g_m_n_structured_compaction_no_duplication_multi_cycle_bounded() {
    let budget = flash_product_32k();
    let (provider, summaries) = FlashProvider::new(budget);
    let tools = vec![sample_tool()];
    let mut messages = many_turns(36, 9_000);
    assert!(
        estimate(&messages) > budget.pressure_threshold(),
        "long-run fixture must start above pressure: {}",
        estimate(&messages)
    );

    let mut sizes = Vec::new();
    let mut checkpoint_counts = Vec::new();
    for cycle in 0..3 {
        let provider_c = provider.clone();
        let tools_c = tools.clone();
        let current = messages.clone();
        let (sink, out) = with_events(move || async move {
            maintain_context(&provider_c, current, &tools_c, 12, true, None).await
        })
        .await;
        let names: Vec<_> = sink.events().iter().map(|e| kind_name(&e.kind)).collect();
        assert!(
            names.contains(&"pressure"),
            "cycle {cycle} should see pressure: {names:?}"
        );
        if names.contains(&"compaction_started") {
            assert!(
                names.contains(&"compaction_completed") || names.contains(&"compaction_failed"),
                "compaction must complete or fail: {names:?}"
            );
            let compact_ops: Vec<_> = sink
                .events()
                .iter()
                .filter(|e| {
                    matches!(
                        e.kind,
                        ContextLifecycleEventKind::ContextCompactionStarted { .. }
                            | ContextLifecycleEventKind::ContextCompactionCompleted { .. }
                            | ContextLifecycleEventKind::ContextCompactionFailed { .. }
                    )
                })
                .map(|e| e.operation_id.clone())
                .collect();
            assert!(compact_ops.len() >= 2);
            assert_eq!(compact_ops[0], compact_ops[1]);
        }
        assert_eq!(
            system_copies(&out, "OmniNova bootstrap. Primary goal"),
            1,
            "system instructions must appear once after cycle {cycle}"
        );
        let cps = checkpoint_count(&out);
        assert!(
            cps <= 1,
            "checkpoint must not stack across cycle {cycle}: {cps}"
        );
        assert!(
            !out.iter()
                .any(|m| m.role == "system" && m.content.starts_with(SUMMARY_MARKER)),
            "structured compaction should replace the summary marker with a checkpoint"
        );
        let body = native_body(&out, &tools, 32_000);
        assert_eq!(
            body.matches("r3 tool schema must appear once").count(),
            1,
            "tool schema must appear once"
        );
        sizes.push(estimate(&out));
        checkpoint_counts.push(cps);

        messages = out;
        let mut extra = 0;
        while estimate(&messages) <= budget.pressure_threshold() {
            messages.push(ChatMessage::user(format!(
                "cycle-{cycle}-more-{extra}\n{}",
                "n".repeat(9_000)
            )));
            messages.push(ChatMessage::assistant(format!(
                "cycle-{cycle}-ack-{extra}\n{}",
                "n".repeat(9_000)
            )));
            extra += 1;
            assert!(
                extra < 80,
                "could not recross pressure after cycle {cycle}, size={}",
                estimate(&messages)
            );
        }
    }

    assert!(
        summaries.lock().unwrap().len() >= 1,
        "structured compaction must call the summarizer"
    );
    let max_after = *sizes.iter().max().unwrap();
    assert!(
        max_after < FLASH_WINDOW,
        "context must remain bounded, got {max_after}"
    );
    assert!(
        checkpoint_counts.iter().all(|c| *c <= 1),
        "no monotonic checkpoint inflation: {checkpoint_counts:?}"
    );
}

#[tokio::test]
async fn r3_h_c1_allows_under_and_blocks_over_after_c2() {
    let budget = flash_product_32k();
    let (provider, _) = FlashProvider::new(budget);
    let estimator = TokenEstimator::new();

    let under = pad_user_to_tokens((budget.max_input_tokens as f64 * 0.50).floor() as u64);
    let under_after = maintain_context(&provider, under, &[], 80, true, None).await;
    let under_body = native_body(&under_after, &[], 32_000);
    let under_est = estimator.estimate_request(&under_body);
    assert!(
        under_est <= budget.max_input_tokens,
        "post-C2 under-budget request must pass C1: {under_est} vs {}",
        budget.max_input_tokens
    );

    let over = pad_user_to_tokens(budget.max_input_tokens.saturating_add(8_192));
    assert!(estimate(&over) > budget.max_input_tokens);
    let over_after = maintain_context(&provider, over, &[], 80, true, None).await;
    let over_body = native_body(&over_after, &[], 32_000);
    let over_est = estimator.estimate_request(&over_body);
    assert!(
        over_est > budget.max_input_tokens,
        "few-message oversized context cannot compact by count, C1 must still see overflow: {over_est}"
    );
    let c1_error = format!(
        "{}: estimated_input_tokens={over_est}, max_input_tokens={}",
        CONTEXT_BUDGET_EXCEEDED_MARKER, budget.max_input_tokens
    );
    assert!(c1_error.contains(CONTEXT_BUDGET_EXCEEDED_MARKER));
}

#[test]
fn r3_h_e_c1_request_override_uses_smaller_input_budget() {
    let estimator = TokenEstimator::new();
    let product = flash_product_32k();
    let override_budget = flash_request_128k();
    let messages = pad_user_to_tokens(override_budget.max_input_tokens.saturating_add(16_384));
    let body_32 = native_body(&messages, &[], 32_000);
    let body_128 = native_body(&messages, &[], 128_000);
    let est_32 = estimator.estimate_request(&body_32);
    let est_128 = estimator.estimate_request(&body_128);
    assert!(est_32 > override_budget.max_input_tokens);
    assert!(est_32 <= product.max_input_tokens);
    assert!(est_128 > override_budget.max_input_tokens);
    assert!(est_128 <= product.max_input_tokens);
}

#[tokio::test]
async fn r3_i_forced_recovery_rebuilds_and_does_not_loop() {
    let budget = flash_product_32k();
    let (provider, summaries) = FlashProvider::new(budget);
    let messages = many_turns(40, 4_000);
    let before = estimate(&messages);
    let out = force_context_recovery(&provider, messages, &[], 12, None).await;
    let after = estimate(&out);
    assert!(after < before, "forced recovery must shrink: {after} vs {before}");
    assert!(summaries.lock().unwrap().len() <= 2, "maintenance passes stay bounded");
    assert_eq!(
        system_copies(&out, "OmniNova bootstrap. Primary goal"),
        1
    );
}

#[tokio::test]
async fn r3_k_unknown_budget_has_no_fake_window_and_soft_cap_works() {
    let (provider, _summaries) = UnknownBudgetProvider::new();
    let snapshot = build_snapshot(
        Some("session-r3".into()),
        Some("run-r3".into()),
        1,
        "r3-unknown",
        "unknown-model",
        MeasurementKind::CandidateEstimate,
        &[ChatMessage::user("hello")],
        &[],
        None,
        None,
    );
    assert_eq!(snapshot.context_window_tokens, None);
    assert_eq!(snapshot.max_input_tokens, None);
    assert_eq!(snapshot.usage_ratio, None);
    assert_eq!(snapshot.pressure_threshold_tokens, None);
    assert!(snapshot.budget_source.is_none());

    let huge = "x".repeat(80_000);
    let mut messages = vec![ChatMessage::system("bootstrap")];
    for i in 0..12 {
        messages.push(ChatMessage::user(format!("u{i}")));
        messages.push(ChatMessage::assistant(format!("a{i}")));
    }
    messages.push(ChatMessage::assistant(
        r#"{"tool_calls":[{"id":"call-old","name":"unique_r3_tool","arguments":"{}"}]}"#,
    ));
    messages.push(ChatMessage::tool(tool_json("call-old", &huge)));
    messages.push(ChatMessage::assistant("continue"));
    let largest = crate::providers::context_budget::largest_tool_result_tokens(
        &messages,
        &TokenEstimator::new(),
    );
    assert!(largest > UNKNOWN_BUDGET_TOOL_SOFT_CAP_TOKENS);

    let (sink, out) = with_events(|| async {
        maintain_context(&provider, messages, &[], 50, true, None).await
    })
    .await;
    assert!(pruned(&out));
    let names: Vec<_> = sink.events().iter().map(|e| kind_name(&e.kind)).collect();
    assert!(names.contains(&"pressure"));
    let pressure = sink
        .events()
        .iter()
        .find_map(|e| match &e.kind {
            ContextLifecycleEventKind::ContextPressureDetected {
                context_window_tokens,
                pressure_threshold_tokens,
                ..
            } => Some((*context_window_tokens, *pressure_threshold_tokens)),
            _ => None,
        })
        .expect("pressure event");
    assert_eq!(pressure.0, None, "unknown budget must not invent a window");
    assert_eq!(pressure.1, None, "unknown budget must not invent a pressure percent");
}

#[tokio::test]
async fn r3_l_original_tool_content_excluded_from_native_request() {
    let mut tool = ChatMessage::tool(tool_json("call-1", "VISIBLE_TOOL_RESULT"));
    tool.original_tool_content = Some("HIDDEN_ORIGINAL_TOOL_CONTENT_R3".into());
    let messages = vec![
        ChatMessage::system("OmniNova bootstrap. Primary goal: bounded context."),
        ChatMessage::user("run"),
        ChatMessage::assistant(
            r#"{"tool_calls":[{"id":"call-1","name":"unique_r3_tool","arguments":"{}"}]}"#,
        ),
        tool,
    ];
    let body = native_body(&messages, &[sample_tool()], 32_000);
    assert!(body.contains("VISIBLE_TOOL_RESULT"));
    assert!(!body.contains("HIDDEN_ORIGINAL_TOOL_CONTENT_R3"));
    assert_eq!(body.matches("r3 tool schema must appear once").count(), 1);
    assert_eq!(
        body.matches("OmniNova bootstrap. Primary goal: bounded context.").count(),
        1
    );
}

#[test]
fn r3_o_p_session_projection_uses_product_policy_without_inventing_override() {
    let budget = flash_product_32k();
    let messages = vec![
        ChatMessage::system("OmniNova bootstrap. Primary goal: bounded context."),
        ChatMessage::user("restored turn"),
    ];
    let snapshot = build_snapshot(
        Some("session-r3".into()),
        None,
        1,
        "deepseek",
        "deepseek-v4-flash",
        MeasurementKind::CandidateEstimate,
        &messages,
        &[],
        Some(&budget),
        None,
    );
    assert_eq!(snapshot.max_input_tokens, Some(935_232));
    assert_eq!(snapshot.pressure_threshold_tokens, Some(748_185));
    assert_eq!(snapshot.request_output_reserve_tokens, Some(32_000));
    assert_eq!(
        snapshot.request_generation_limit_source.as_deref(),
        Some("product_default")
    );
    assert_eq!(snapshot.measurement_kind, MeasurementKind::CandidateEstimate);
    assert!(snapshot.run_id.is_none());
}

#[test]
fn r3_resolver_override_is_request_scoped_not_profile() {
    let profile = ModelProviderConfig {
        request_max_output_tokens: Some(32_000),
        ..ModelProviderConfig::default()
    };
    let during = resolve_effective_request_generation_limit(
        Some(128_000),
        profile.request_max_output_tokens,
        Some(PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS),
        Some(FLASH_MODEL_MAX_OUTPUT),
    );
    let after = resolve_generation_limit(Some(&profile), Some(FLASH_MODEL_MAX_OUTPUT));
    assert_eq!(during.effective_tokens, Some(128_000));
    assert_eq!(during.source, GenerationLimitSource::RequestOverride);
    assert_eq!(after.effective_tokens, Some(32_000));
    assert_eq!(after.source, GenerationLimitSource::ProfileOverride);
}
