use crate::providers::context_budget::{ContextBudget, TokenEstimator};
use crate::providers::ChatMessage;
use crate::tools::ToolSpec;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// How a context measurement was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementKind {
    /// C2 maintenance-friendly projection (`estimate_messages_with_tools`).
    /// Never the user-visible authoritative total.
    CandidateEstimate,
    /// C1 final Provider-native envelope (`estimate_request(request_body)`).
    FinalRequestEstimate,
    /// Provider-reported `usage.input_tokens` for the same request identity.
    ProviderActual,
}

/// Additive breakdown of the estimated model-visible context.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextUsageBreakdown {
    pub system_tokens: u64,
    pub conversation_tokens: u64,
    pub tool_schema_tokens: u64,
    pub tool_result_tokens: u64,
    pub request_overhead_tokens: u64,
}

impl ContextUsageBreakdown {
    pub fn total(&self) -> u64 {
        self.system_tokens
            .saturating_add(self.conversation_tokens)
            .saturating_add(self.tool_schema_tokens)
            .saturating_add(self.tool_result_tokens)
            .saturating_add(self.request_overhead_tokens)
    }
}

/// Authoritative, safe context usage snapshot.
///
/// This intentionally contains no prompt/tool contents. It is the data shape
/// that the Context UI / Task Inspector should consume.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextUsageSnapshot {
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub request_revision: u64,
    pub provider: String,
    pub model: String,
    pub measurement_kind: MeasurementKind,
    pub estimated_input_tokens: u64,
    pub provider_actual_input_tokens: Option<u64>,
    pub context_window_tokens: Option<u64>,
    pub max_input_tokens: Option<u64>,
    pub output_reserve_tokens: Option<u64>,
    pub safety_reserve_tokens: Option<u64>,
    pub pressure_threshold_tokens: Option<u64>,
    pub budget_source: Option<String>,
    pub usage_ratio: Option<f64>,
    pub breakdown: ContextUsageBreakdown,
    pub measured_at: u64,
}

/// Builds a snapshot from the same local estimator authority as C1.
///
/// When `request_body` is supplied, `estimated_input_tokens` is exactly
/// `TokenEstimator::estimate_request(request_body)` (the C1 final envelope
/// accounting) and `request_overhead_tokens` absorbs the difference between
/// category subtotals and that final envelope estimate.
pub fn build_snapshot(
    session_id: Option<String>,
    run_id: Option<String>,
    request_revision: u64,
    provider: impl Into<String>,
    model: impl Into<String>,
    measurement_kind: MeasurementKind,
    messages: &[ChatMessage],
    tools: &[ToolSpec],
    budget: Option<&ContextBudget>,
    request_body: Option<&str>,
) -> ContextUsageSnapshot {
    let estimator = TokenEstimator::new();
    let mut breakdown = ContextUsageBreakdown::default();

    for message in messages {
        let tokens = estimator.estimate_text(&message.content);
        match message.role.as_str() {
            "system" => breakdown.system_tokens = breakdown.system_tokens.saturating_add(tokens),
            "tool" => breakdown.tool_result_tokens = breakdown.tool_result_tokens.saturating_add(tokens),
            _ => breakdown.conversation_tokens = breakdown.conversation_tokens.saturating_add(tokens),
        }
        if let Some(images) = &message.images {
            let image_tokens = (images.len() as u64).saturating_mul(1_024);
            breakdown.conversation_tokens = breakdown.conversation_tokens.saturating_add(image_tokens);
        }
    }

    for tool in tools {
        let spec = serde_json::to_string(tool).unwrap_or_default();
        breakdown.tool_schema_tokens = breakdown
            .tool_schema_tokens
            .saturating_add(estimator.estimate_text(&spec));
    }

    let category_total = breakdown.total();
    let estimated_input_tokens = if let Some(body) = request_body {
        estimator.estimate_request(body)
    } else {
        // Same local estimator as `TokenEstimator::estimate_messages_with_tools`
        // (the +8 envelope constant). C1 still overrides this when a serialized
        // Provider body is supplied.
        category_total.saturating_add(8)
    };
    finalize_breakdown(&mut breakdown, estimated_input_tokens);

    let context_window_tokens = budget.map(|b| b.context_window_tokens);
    let max_input_tokens = budget.map(|b| b.max_input_tokens);
    let output_reserve_tokens = budget.map(|b| b.output_reserve_tokens);
    let safety_reserve_tokens = budget.map(|b| b.safety_reserve_tokens);
    let pressure_threshold_tokens = budget.map(|b| b.pressure_threshold());
    let budget_source = budget.map(|b| b.source.as_str().to_string());
    let usage_ratio = context_window_tokens.and_then(|window| {
        if window == 0 {
            None
        } else {
            Some(estimated_input_tokens as f64 / window as f64)
        }
    });

    let measurement_kind = match (measurement_kind, request_body.is_some()) {
        (MeasurementKind::ProviderActual, _) => MeasurementKind::ProviderActual,
        (_, true) => MeasurementKind::FinalRequestEstimate,
        (_, false) => MeasurementKind::CandidateEstimate,
    };

    ContextUsageSnapshot {
        session_id,
        run_id,
        request_revision,
        provider: provider.into(),
        model: model.into(),
        measurement_kind,
        estimated_input_tokens,
        provider_actual_input_tokens: None,
        context_window_tokens,
        max_input_tokens,
        output_reserve_tokens,
        safety_reserve_tokens,
        pressure_threshold_tokens,
        budget_source,
        usage_ratio,
        breakdown,
        measured_at: now_ms(),
    }
}

fn finalize_breakdown(breakdown: &mut ContextUsageBreakdown, estimated: u64) {
    let without_overhead = breakdown
        .system_tokens
        .saturating_add(breakdown.conversation_tokens)
        .saturating_add(breakdown.tool_schema_tokens)
        .saturating_add(breakdown.tool_result_tokens);
    if without_overhead <= estimated {
        breakdown.request_overhead_tokens = estimated - without_overhead;
        return;
    }
    breakdown.request_overhead_tokens = 0;
    let mut excess = without_overhead - estimated;
    let mut reduce = |field: &mut u64| {
        if excess == 0 {
            return;
        }
        let take = (*field).min(excess);
        *field -= take;
        excess -= take;
    };
    reduce(&mut breakdown.conversation_tokens);
    reduce(&mut breakdown.tool_result_tokens);
    reduce(&mut breakdown.system_tokens);
    reduce(&mut breakdown.tool_schema_tokens);
}

impl ContextUsageSnapshot {
    pub fn with_provider_actual(mut self, actual_input_tokens: u64) -> Self {
        self.provider_actual_input_tokens = Some(actual_input_tokens);
        self.measurement_kind = MeasurementKind::ProviderActual;
        self
    }
}

/// Identity of one Provider request. Actual usage must be attached to this
/// value, never to a shared "latest preflight" slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextRequestIdentity {
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub request_revision: u64,
    pub provider: String,
    pub model: String,
}

/// Lifecycle mode for context telemetry events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextTelemetryMode {
    Proactive,
    UnknownBudgetOversize,
    ForcedOverflowRecovery,
}

/// Typed Context Runtime lifecycle events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContextLifecycleEventKind {
    ContextPressureDetected {
        mode: ContextTelemetryMode,
        estimated_before: u64,
        context_window_tokens: Option<u64>,
        pressure_threshold_tokens: Option<u64>,
        budget_source: Option<String>,
    },
    ContextPruningStarted {
        mode: ContextTelemetryMode,
        estimated_before: u64,
    },
    ContextPruningCompleted {
        mode: ContextTelemetryMode,
        estimated_before: u64,
        estimated_after: u64,
        pruned_tool_result_count: usize,
    },
    ContextCompactionStarted {
        mode: ContextTelemetryMode,
        estimated_before: u64,
    },
    ContextCompactionCompleted {
        mode: ContextTelemetryMode,
        estimated_before: u64,
        estimated_after: u64,
        checkpoint_created: bool,
    },
    ContextCompactionFailed {
        mode: ContextTelemetryMode,
        estimated_before: u64,
        reason: String,
    },
    ContextOverflowRecoveryStarted {
        mode: ContextTelemetryMode,
        provider_reported_window: Option<u64>,
        estimated_before: u64,
    },
    ContextOverflowRecoveryCompleted {
        mode: ContextTelemetryMode,
        estimated_after: u64,
    },
    ContextOverflowRecoveryFailed {
        mode: ContextTelemetryMode,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextLifecycleEvent {
    pub operation_id: String,
    pub run_id: Option<String>,
    pub session_id: Option<String>,
    pub mode: ContextTelemetryMode,
    pub kind: ContextLifecycleEventKind,
    pub timestamp: u64,
}

impl ContextLifecycleEvent {
    pub fn new(
        operation_id: impl Into<String>,
        run_id: Option<String>,
        session_id: Option<String>,
        mode: ContextTelemetryMode,
        kind: ContextLifecycleEventKind,
    ) -> Self {
        Self {
            operation_id: operation_id.into(),
            run_id,
            session_id,
            mode,
            kind,
            timestamp: now_ms(),
        }
    }
}

pub trait ContextTelemetrySink: Send + Sync {
    fn emit_usage(&self, snapshot: ContextUsageSnapshot);
    fn emit_lifecycle(&self, event: ContextLifecycleEvent);
}

#[derive(Debug, Clone, PartialEq)]
pub enum TelemetryRecord {
    Snapshot(ContextUsageSnapshot),
    Event(ContextLifecycleEvent),
}

#[derive(Default)]
pub struct VecContextTelemetry {
    records: Mutex<Vec<TelemetryRecord>>,
}

impl VecContextTelemetry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn records(&self) -> Vec<TelemetryRecord> {
        self.records.lock().unwrap().clone()
    }

    pub fn events(&self) -> Vec<ContextLifecycleEvent> {
        self.records()
            .into_iter()
            .filter_map(|record| match record {
                TelemetryRecord::Event(event) => Some(event),
                _ => None,
            })
            .collect()
    }

    pub fn snapshots(&self) -> Vec<ContextUsageSnapshot> {
        self.records()
            .into_iter()
            .filter_map(|record| match record {
                TelemetryRecord::Snapshot(snapshot) => Some(snapshot),
                _ => None,
            })
            .collect()
    }
}

impl ContextTelemetrySink for VecContextTelemetry {
    fn emit_usage(&self, snapshot: ContextUsageSnapshot) {
        self.records.lock().unwrap().push(TelemetryRecord::Snapshot(snapshot));
    }

    fn emit_lifecycle(&self, event: ContextLifecycleEvent) {
        self.records.lock().unwrap().push(TelemetryRecord::Event(event));
    }
}

pub type SharedSink = Arc<dyn ContextTelemetrySink + Send + Sync>;

/// Per-run observability identity and sink. Bound to a Tokio task so C1/C2/C3
/// can observe without changing Provider/maintenance control flow.
pub struct ContextTelemetryContext {
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub provider: String,
    pub model: String,
    request_revision: AtomicU64,
    snapshots_enabled: AtomicBool,
    sink: SharedSink,
}

impl ContextTelemetryContext {
    pub fn new(
        session_id: Option<String>,
        run_id: Option<String>,
        provider: impl Into<String>,
        model: impl Into<String>,
        sink: SharedSink,
    ) -> Self {
        Self {
            session_id,
            run_id,
            provider: provider.into(),
            model: model.into(),
            request_revision: AtomicU64::new(0),
            snapshots_enabled: AtomicBool::new(true),
            sink,
        }
    }

    pub fn next_revision(&self) -> u64 {
        self.request_revision.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn allocate_request_identity(&self) -> ContextRequestIdentity {
        ContextRequestIdentity {
            session_id: self.session_id.clone(),
            run_id: self.run_id.clone(),
            request_revision: self.next_revision(),
            provider: self.provider.clone(),
            model: self.model.clone(),
        }
    }

    pub fn snapshots_enabled(&self) -> bool {
        self.snapshots_enabled.load(Ordering::SeqCst)
    }
}

tokio::task_local! {
    static CURRENT_CONTEXT: Arc<ContextTelemetryContext>;
}

pub fn current_context() -> Option<Arc<ContextTelemetryContext>> {
    CURRENT_CONTEXT.try_with(|ctx| ctx.clone()).ok()
}

pub async fn with_context_telemetry<Fut, T>(
    session_id: Option<String>,
    run_id: Option<String>,
    provider: impl Into<String>,
    model: impl Into<String>,
    sink: SharedSink,
    fut: Fut,
) -> T
where
    Fut: Future<Output = T>,
{
    let ctx = Arc::new(ContextTelemetryContext::new(
        session_id,
        run_id,
        provider,
        model,
        sink,
    ));
    CURRENT_CONTEXT.scope(ctx, fut).await
}

pub struct SnapshotPauseGuard {
    ctx: Option<Arc<ContextTelemetryContext>>,
    previous: bool,
}

impl Drop for SnapshotPauseGuard {
    fn drop(&mut self) {
        if let Some(ctx) = &self.ctx {
            ctx.snapshots_enabled.store(self.previous, Ordering::SeqCst);
        }
    }
}

/// Suppress user-visible usage snapshots (e.g. while the compaction summarizer
/// runs). Lifecycle events still emit.
pub fn pause_usage_snapshots() -> SnapshotPauseGuard {
    let ctx = current_context();
    let previous = ctx
        .as_ref()
        .map(|c| c.snapshots_enabled.swap(false, Ordering::SeqCst))
        .unwrap_or(false);
    SnapshotPauseGuard { ctx, previous }
}

pub fn emit_lifecycle(
    operation_id: impl Into<String>,
    mode: ContextTelemetryMode,
    kind: ContextLifecycleEventKind,
) {
    let (run_id, session_id) = current_context()
        .map(|ctx| (ctx.run_id.clone(), ctx.session_id.clone()))
        .unwrap_or((None, None));
    emit_event(ContextLifecycleEvent::new(
        operation_id,
        run_id,
        session_id,
        mode,
        kind,
    ));
}

static GLOBAL_SINK: OnceLock<Mutex<Option<SharedSink>>> = OnceLock::new();

fn global_sink_guard() -> &'static Mutex<Option<SharedSink>> {
    GLOBAL_SINK.get_or_init(|| Mutex::new(None))
}

pub fn set_global_context_telemetry(sink: SharedSink) {
    *global_sink_guard().lock().unwrap() = Some(sink);
}

pub fn clear_global_context_telemetry() {
    *global_sink_guard().lock().unwrap() = None;
}

pub fn emit_snapshot(snapshot: ContextUsageSnapshot) {
    if let Some(ctx) = current_context() {
        ctx.sink.emit_usage(snapshot);
        return;
    }
    if let Some(sink) = global_sink_guard().lock().unwrap().as_ref() {
        sink.emit_usage(snapshot);
    }
}

pub fn emit_event(event: ContextLifecycleEvent) {
    if let Some(ctx) = current_context() {
        ctx.sink.emit_lifecycle(event);
        return;
    }
    if let Some(sink) = global_sink_guard().lock().unwrap().as_ref() {
        sink.emit_lifecycle(event);
    }
}

pub fn new_operation_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::context_budget::{ContextBudget, ContextBudgetSource, TokenEstimator};

    fn sample_messages() -> Vec<ChatMessage> {
        vec![
            ChatMessage::system("bootstrap"),
            ChatMessage::user("hello"),
            ChatMessage::assistant("hi"),
            ChatMessage::tool(r#"{"tool_call_id":"call-1","content":"result"}"#.to_string()),
        ]
    }

    fn sample_tools() -> Vec<ToolSpec> {
        vec![ToolSpec {
            name: "test".to_string(),
            description: "test tool".to_string(),
            parameters: serde_json::json!({"type":"object","properties":{}}),
        }]
    }

    #[test]
    fn breakdown_sums_exactly_to_local_estimated_total() {
        let messages = sample_messages();
        let tools = sample_tools();
        let snapshot = build_snapshot(
            None,
            None,
            1,
            "openai",
            "gpt-4o",
            MeasurementKind::CandidateEstimate,
            &messages,
            &tools,
            None,
            None,
        );
        assert_eq!(snapshot.breakdown.total(), snapshot.estimated_input_tokens);
        assert_eq!(
            snapshot.estimated_input_tokens,
            TokenEstimator::new().estimate_messages_with_tools(&messages, &tools)
        );
        assert_eq!(snapshot.measurement_kind, MeasurementKind::CandidateEstimate);
    }

    #[test]
    fn breakdown_sums_to_c1_body_estimate_when_body_supplied() {
        let messages = sample_messages();
        let tools = sample_tools();
        let body = "x".repeat(1000);
        let snapshot = build_snapshot(
            None,
            None,
            1,
            "openai",
            "gpt-4o",
            MeasurementKind::CandidateEstimate,
            &messages,
            &tools,
            None,
            Some(&body),
        );
        assert_eq!(snapshot.breakdown.total(), snapshot.estimated_input_tokens);
        assert_eq!(
            snapshot.estimated_input_tokens,
            TokenEstimator::new().estimate_request(&body)
        );
        assert_eq!(snapshot.measurement_kind, MeasurementKind::FinalRequestEstimate);
    }

    #[test]
    fn unknown_budget_has_no_window_or_ratio() {
        let messages = sample_messages();
        let snapshot = build_snapshot(
            None,
            None,
            1,
            "openai",
            "unknown-model",
            MeasurementKind::CandidateEstimate,
            &messages,
            &[],
            None,
            None,
        );
        assert_eq!(snapshot.context_window_tokens, None);
        assert_eq!(snapshot.usage_ratio, None);
        assert_eq!(snapshot.max_input_tokens, None);
        assert_eq!(snapshot.pressure_threshold_tokens, None);
        assert!(snapshot.budget_source.is_none());
    }

    #[test]
    fn known_budget_zero_window_has_no_usage_ratio() {
        let messages = sample_messages();
        let budget = ContextBudget::new(0, None, ContextBudgetSource::ExplicitConfig);
        let snapshot = build_snapshot(
            None,
            None,
            1,
            "openai",
            "tiny",
            MeasurementKind::CandidateEstimate,
            &messages,
            &[],
            Some(&budget),
            None,
        );
        assert_eq!(snapshot.context_window_tokens, Some(0));
        assert_eq!(snapshot.usage_ratio, None);
        assert_eq!(snapshot.max_input_tokens, Some(0));
    }

    #[test]
    fn known_budget_derives_values() {
        let messages = sample_messages();
        let budget = ContextBudget::new(
            1_000_000,
            Some(16_384),
            ContextBudgetSource::ExplicitConfig,
        );
        let snapshot = build_snapshot(
            None,
            None,
            1,
            "openai",
            "gpt-4o",
            MeasurementKind::CandidateEstimate,
            &messages,
            &[],
            Some(&budget),
            None,
        );
        assert_eq!(snapshot.context_window_tokens, Some(1_000_000));
        assert_eq!(snapshot.max_input_tokens, Some(950_848));
        assert_eq!(snapshot.output_reserve_tokens, Some(16_384));
        assert_eq!(snapshot.safety_reserve_tokens, Some(32_768));
        assert!(snapshot.usage_ratio.is_some());
    }

    #[test]
    fn provider_actual_is_captured_separately() {
        let messages = sample_messages();
        let snapshot = build_snapshot(
            None,
            None,
            1,
            "openai",
            "gpt-4o",
            MeasurementKind::CandidateEstimate,
            &messages,
            &[],
            None,
            None,
        )
        .with_provider_actual(1234);
        assert_eq!(snapshot.provider_actual_input_tokens, Some(1234));
        assert_eq!(snapshot.measurement_kind, MeasurementKind::ProviderActual);
        assert_ne!(snapshot.estimated_input_tokens, 1234);
    }

    #[test]
    fn stale_snapshot_can_be_distinguished_by_revision_and_model() {
        let messages = sample_messages();
        let old = build_snapshot(
            Some("session-1".to_string()),
            None,
            1,
            "openai",
            "gpt-4o",
            MeasurementKind::CandidateEstimate,
            &messages,
            &[],
            None,
            None,
        );
        let new = build_snapshot(
            Some("session-1".to_string()),
            None,
            2,
            "openai",
            "gpt-4o",
            MeasurementKind::CandidateEstimate,
            &messages,
            &[],
            None,
            None,
        );
        assert_ne!(old.request_revision, new.request_revision);
        assert!(new.request_revision > old.request_revision);

        let other_session = build_snapshot(
            Some("session-2".to_string()),
            None,
            1,
            "openai",
            "gpt-4o",
            MeasurementKind::CandidateEstimate,
            &messages,
            &[],
            None,
            None,
        );
        let other_model = build_snapshot(
            Some("session-1".to_string()),
            None,
            1,
            "openai",
            "other-model",
            MeasurementKind::CandidateEstimate,
            &messages,
            &[],
            None,
            None,
        );
        assert_ne!(old.session_id, other_session.session_id);
        assert_ne!(old.model, other_model.model);
        let same_identity = old.session_id == new.session_id
            && old.provider == new.provider
            && old.model == new.model;
        assert!(same_identity);
        assert!(old.request_revision < new.request_revision);
    }

    #[test]
    fn events_do_not_contain_prompt_or_tool_contents() {
        let event = ContextLifecycleEvent::new(
            "op-1",
            None,
            Some("session-1".to_string()),
            ContextTelemetryMode::Proactive,
            ContextLifecycleEventKind::ContextCompactionStarted {
                mode: ContextTelemetryMode::Proactive,
                estimated_before: 100,
            },
        );
        let json = serde_json::to_string(&event).unwrap();
        assert!(!json.contains("bootstrap"));
        assert!(!json.contains("tool_call_id"));
        assert!(!json.contains("hello"));
        assert!(!json.contains("Authorization"));
        let snapshot = build_snapshot(
            Some("session-1".to_string()),
            Some("run-1".to_string()),
            1,
            "openai",
            "gpt-4o",
            MeasurementKind::CandidateEstimate,
            &sample_messages(),
            &sample_tools(),
            None,
            None,
        );
        let snapshot_json = serde_json::to_string(&snapshot).unwrap();
        assert!(!snapshot_json.contains("bootstrap"));
        assert!(!snapshot_json.contains("hello"));
        assert!(!snapshot_json.contains("call-1"));
        assert!(!snapshot_json.contains("test tool"));
        assert!(!snapshot_json.contains("tool_call_id"));
    }

    #[test]
    fn overflow_detection_requires_semantic_evidence() {
        let err = anyhow::anyhow!("OpenAI API error (400): {}",
            r#"{"error":{"code":"context_length_exceeded"}}"#);
        assert!(crate::providers::context_budget::context_window_exceeded_info(&err).is_some());

        let generic = anyhow::anyhow!("OpenAI API error (400): {}",
            r#"{"error":{"message":"bad request"}}"#);
        assert!(crate::providers::context_budget::context_window_exceeded_info(&generic).is_none());
    }

    #[test]
    fn telemetry_sink_records_events_and_snapshots() {
        let sink = Arc::new(VecContextTelemetry::new());
        set_global_context_telemetry(sink.clone());
        emit_event(ContextLifecycleEvent::new(
            "op-1",
            None,
            None,
            ContextTelemetryMode::Proactive,
            ContextLifecycleEventKind::ContextPressureDetected {
                mode: ContextTelemetryMode::Proactive,
                estimated_before: 10,
                context_window_tokens: Some(100),
                pressure_threshold_tokens: Some(80),
                budget_source: Some("builtin".to_string()),
            },
        ));
        let messages = sample_messages();
        emit_snapshot(build_snapshot(
            None,
            None,
            1,
            "openai",
            "gpt-4o",
            MeasurementKind::CandidateEstimate,
            &messages,
            &[],
            None,
            None,
        ));
        assert_eq!(sink.events().len(), 1);
        assert_eq!(sink.snapshots().len(), 1);
        clear_global_context_telemetry();
    }

    #[test]
    fn out_of_order_provider_actual_stays_bound_to_request_identity() {
        let ctx = ContextTelemetryContext::new(
            Some("session-a".to_string()),
            Some("run-a".to_string()),
            "openai",
            "gpt-4o",
            Arc::new(VecContextTelemetry::new()),
        );
        let identity_a = ctx.allocate_request_identity();
        let identity_b = ctx.allocate_request_identity();
        assert_eq!(identity_a.request_revision, 1);
        assert_eq!(identity_b.request_revision, 2);
        assert_eq!(identity_a.session_id.as_deref(), Some("session-a"));
        assert_eq!(identity_b.model, "gpt-4o");

        let messages = sample_messages();
        let body_a = "request-a-envelope";
        let body_b = "request-b-envelope-longer";
        let snap_a = build_snapshot(
            identity_a.session_id.clone(),
            identity_a.run_id.clone(),
            identity_a.request_revision,
            identity_a.provider.clone(),
            identity_a.model.clone(),
            MeasurementKind::FinalRequestEstimate,
            &messages,
            &[],
            None,
            Some(body_a),
        )
        .with_provider_actual(111);
        let snap_b = build_snapshot(
            identity_b.session_id.clone(),
            identity_b.run_id.clone(),
            identity_b.request_revision,
            identity_b.provider.clone(),
            identity_b.model.clone(),
            MeasurementKind::FinalRequestEstimate,
            &messages,
            &[],
            None,
            Some(body_b),
        )
        .with_provider_actual(222);

        assert_eq!(snap_a.request_revision, 1);
        assert_eq!(snap_b.request_revision, 2);
        assert_eq!(snap_a.provider_actual_input_tokens, Some(111));
        assert_eq!(snap_b.provider_actual_input_tokens, Some(222));
        assert_eq!(snap_a.measurement_kind, MeasurementKind::ProviderActual);
        assert_eq!(snap_b.measurement_kind, MeasurementKind::ProviderActual);
        assert_ne!(snap_a.estimated_input_tokens, 111);
        assert_ne!(snap_b.estimated_input_tokens, 222);
        assert_eq!(snap_a.session_id, identity_a.session_id);
        assert_eq!(snap_b.run_id, identity_b.run_id);
    }
}
