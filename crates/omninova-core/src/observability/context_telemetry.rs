use crate::providers::context_budget::{ContextBudget, TokenEstimator};
use crate::providers::native_request::native_context_view_json;
use crate::providers::token_counter::{measure_display_tokens, DisplayMeasurement};
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

/// Provenance of the display total. Safety estimates must never be labelled exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementProvenance {
    #[default]
    SafetyEstimate,
    ExactTokenizer,
    ProviderCountApi,
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
    #[serde(default)]
    pub measurement_provenance: MeasurementProvenance,
    /// Explicit precision truth. Provenance identifies the source only and
    /// must never be interpreted as automatic proof of exactness.
    #[serde(default)]
    pub measurement_exact: bool,
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
/// `TokenEstimator::estimate_request(request_body)` (the C1 final envelope)
/// and categories are measured from that Provider-native JSON. Overhead is the
/// remainder after native semantic categories.
///
/// Candidate snapshots (`request_body` absent) classify the same native
/// message/tool objects via `native_context_view_json`, while the total stays
/// `TokenEstimator::estimate_messages_with_tools`.
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
    let model = model.into();
    let provider = provider.into();
    let configured = current_context().and_then(|ctx| ctx.exact_tokenizer.lock().ok()?.clone());
    let (safety_total, mut breakdown) = if let Some(body) = request_body {
        (
            estimator.estimate_request(body),
            classify_native_request_json(body, &estimator),
        )
    } else {
        let view = native_context_view_json(messages, tools);
        (
            estimator.estimate_messages_with_tools(messages, tools),
            classify_native_request_json(&view, &estimator),
        )
    };
    finalize_breakdown(&mut breakdown, safety_total);
    let display = measure_display_tokens(
        &model,
        configured.as_deref(),
        messages,
        tools,
        request_body,
    );
    let (estimated_input_tokens, measurement_provenance, measurement_exact) = match display {
        DisplayMeasurement::Exact {
            tokens,
            production_validated,
        } => (tokens, MeasurementProvenance::ExactTokenizer, production_validated),
        DisplayMeasurement::SafetyEstimate(tokens) => {
            (tokens, MeasurementProvenance::SafetyEstimate, false)
        }
    };

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
        provider,
        model,
        measurement_kind,
        measurement_provenance,
        measurement_exact,
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

fn classify_native_request_json(body: &str, estimator: &TokenEstimator) -> ContextUsageBreakdown {
    let mut breakdown = ContextUsageBreakdown::default();
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return breakdown;
    };

    if let Some(messages) = value.get("messages").and_then(serde_json::Value::as_array) {
        for message in messages {
            let serialized = serde_json::to_string(message).unwrap_or_default();
            let tokens = estimator.estimate_text(&serialized);
            match message.get("role").and_then(serde_json::Value::as_str) {
                Some("system") => {
                    breakdown.system_tokens = breakdown.system_tokens.saturating_add(tokens);
                }
                Some("tool") => {
                    breakdown.tool_result_tokens =
                        breakdown.tool_result_tokens.saturating_add(tokens);
                }
                _ => {
                    breakdown.conversation_tokens =
                        breakdown.conversation_tokens.saturating_add(tokens);
                }
            }
        }
    }

    if let Some(tools) = value.get("tools").and_then(serde_json::Value::as_array) {
        for tool in tools {
            let serialized = serde_json::to_string(tool).unwrap_or_default();
            breakdown.tool_schema_tokens = breakdown
                .tool_schema_tokens
                .saturating_add(estimator.estimate_text(&serialized));
        }
    }

    breakdown
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
    pub fn with_provider_count_api(mut self, tokens: u64, exact: bool) -> Self {
        self.estimated_input_tokens = tokens;
        self.measurement_kind = MeasurementKind::FinalRequestEstimate;
        self.measurement_provenance = MeasurementProvenance::ProviderCountApi;
        self.measurement_exact = exact;
        self
    }

    pub fn with_provider_actual(mut self, actual_input_tokens: u64) -> Self {
        self.provider_actual_input_tokens = Some(actual_input_tokens);
        self.measurement_kind = MeasurementKind::ProviderActual;
        self.measurement_provenance = MeasurementProvenance::ProviderActual;
        self.measurement_exact = true;
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
    exact_tokenizer: Mutex<Option<String>>,
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
            exact_tokenizer: Mutex::new(None),
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

    /// Rejects late snapshots for an older request revision. This is checked
    /// at the emission boundary, so async ProviderCountApi results cannot
    /// overwrite a newer request's current measurement even when they arrive
    /// out of order.
    pub fn is_stale_revision(&self, snapshot: &ContextUsageSnapshot) -> bool {
        if snapshot.session_id.is_some()
            && self.session_id.is_some()
            && snapshot.session_id != self.session_id
        {
            return false;
        }
        if snapshot.run_id.is_some()
            && self.run_id.is_some()
            && snapshot.run_id != self.run_id
        {
            return false;
        }
        if snapshot.provider != self.provider || snapshot.model != self.model {
            return false;
        }
        let latest = self.request_revision.load(Ordering::SeqCst);
        snapshot.request_revision < latest
    }
}

pub fn set_exact_tokenizer(name: Option<String>) {
    let Some(ctx) = current_context() else {
        return;
    };
    *ctx.exact_tokenizer.lock().unwrap() = name.filter(|value| !value.trim().is_empty());
}

/// Spawns a display-only telemetry future under the same ContextTelemetryContext.
/// Used by provider-native count calls so they never block the normal model
/// request and still participate in revision-based stale-result suppression.
pub fn spawn_context_usage_task<F>(ctx: Arc<ContextTelemetryContext>, fut: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        let _ = CURRENT_CONTEXT.scope(ctx, fut).await;
    });
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
        if ctx.is_stale_revision(&snapshot) {
            return;
        }
        crate::observability::token_parity::observe_token_parity(&snapshot);
        ctx.sink.emit_usage(snapshot);
        return;
    }
    if let Some(sink) = global_sink_guard().lock().unwrap().as_ref() {
        crate::observability::token_parity::observe_token_parity(&snapshot);
        sink.emit_usage(snapshot);
    }
}

/// Emits a production CandidateEstimate for the current model-visible Agent
/// context. This is not the final Provider envelope.
pub fn emit_candidate_usage(
    messages: &[ChatMessage],
    tools: &[ToolSpec],
    budget: Option<&ContextBudget>,
) {
    let Some(ctx) = current_context() else {
        return;
    };
    if !ctx.snapshots_enabled() {
        return;
    }
    if ctx.provider.trim().is_empty() || ctx.model.trim().is_empty() {
        return;
    }
    if messages.is_empty() && tools.is_empty() {
        return;
    }
    let identity = ctx.allocate_request_identity();
    let snapshot = build_snapshot(
        identity.session_id.clone(),
        identity.run_id.clone(),
        identity.request_revision,
        identity.provider.clone(),
        identity.model.clone(),
        MeasurementKind::CandidateEstimate,
        messages,
        tools,
        budget,
        None,
    );
    // A candidate is a display-only context projection. If a later revision
    // has already started (e.g. a model request allocated the final identity),
    // this candidate is stale and must not overwrite it.
    if ctx.is_stale_revision(&snapshot) {
        return;
    }
    emit_snapshot(snapshot);
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

    fn final_native_body(messages: &[ChatMessage], tools: &[ToolSpec]) -> String {
        let request = crate::providers::native_request::NativeChatRequest {
            model: "gpt-4o".into(),
            messages: crate::providers::native_request::convert_messages(messages),
            temperature: 0.2,
            max_tokens: Some(1024),
            tools: crate::providers::native_request::convert_tools(if tools.is_empty() {
                None
            } else {
                Some(tools)
            }),
            tool_choice: if tools.is_empty() {
                None
            } else {
                Some("auto".into())
            },
            stream: Some(true),
        };
        serde_json::to_string(&request).unwrap()
    }

    fn overhead_ratio(snapshot: &ContextUsageSnapshot) -> f64 {
        snapshot.breakdown.request_overhead_tokens as f64
            / snapshot.estimated_input_tokens.max(1) as f64
    }

    #[test]
    fn a_final_native_request_total_matches_estimator() {
        let messages = sample_messages();
        let tools = sample_tools();
        let body = final_native_body(&messages, &tools);
        let snapshot = build_snapshot(
            Some("session-1".into()),
            Some("run-1".into()),
            3,
            "openai",
            "gpt-4o",
            MeasurementKind::FinalRequestEstimate,
            &messages,
            &tools,
            None,
            Some(&body),
        );
        assert_eq!(
            snapshot.estimated_input_tokens,
            TokenEstimator::new().estimate_request(&body)
        );
        assert_eq!(snapshot.measurement_kind, MeasurementKind::FinalRequestEstimate);
    }

    #[test]
    fn b_native_breakdown_sums_exactly_to_final_estimate() {
        let messages = sample_messages();
        let tools = sample_tools();
        let body = final_native_body(&messages, &tools);
        let snapshot = build_snapshot(
            None,
            None,
            1,
            "openai",
            "gpt-4o",
            MeasurementKind::FinalRequestEstimate,
            &messages,
            &tools,
            None,
            Some(&body),
        );
        assert_eq!(snapshot.breakdown.total(), snapshot.estimated_input_tokens);
        eprintln!(
            "mixed overhead_ratio={:.4} system={} conversation={} tool_schema={} tool_result={} overhead={}",
            overhead_ratio(&snapshot),
            snapshot.breakdown.system_tokens,
            snapshot.breakdown.conversation_tokens,
            snapshot.breakdown.tool_schema_tokens,
            snapshot.breakdown.tool_result_tokens,
            snapshot.breakdown.request_overhead_tokens
        );
    }

    #[test]
    fn c_large_system_prompt_is_classified_as_system_not_overhead() {
        let messages = vec![ChatMessage::system("S".repeat(20_000))];
        let body = final_native_body(&messages, &[]);
        let snapshot = build_snapshot(
            None,
            None,
            1,
            "openai",
            "gpt-4o",
            MeasurementKind::FinalRequestEstimate,
            &messages,
            &[],
            None,
            Some(&body),
        );
        eprintln!("large_system overhead_ratio={:.4}", overhead_ratio(&snapshot));
        assert!(
            snapshot.breakdown.system_tokens * 5 > snapshot.estimated_input_tokens * 4,
            "system content leaked into overhead: {:?}",
            snapshot.breakdown
        );
        assert!(overhead_ratio(&snapshot) < 0.20);
        assert_eq!(snapshot.breakdown.total(), snapshot.estimated_input_tokens);
    }

    #[test]
    fn d_large_conversation_is_classified_as_conversation_not_overhead() {
        let messages = vec![
            ChatMessage::user("U".repeat(12_000)),
            ChatMessage::assistant("A".repeat(12_000)),
        ];
        let body = final_native_body(&messages, &[]);
        let snapshot = build_snapshot(
            None,
            None,
            1,
            "openai",
            "gpt-4o",
            MeasurementKind::FinalRequestEstimate,
            &messages,
            &[],
            None,
            Some(&body),
        );
        eprintln!("large_conversation overhead_ratio={:.4}", overhead_ratio(&snapshot));
        assert!(
            snapshot.breakdown.conversation_tokens * 5 > snapshot.estimated_input_tokens * 4,
            "conversation leaked into overhead: {:?}",
            snapshot.breakdown
        );
        assert!(overhead_ratio(&snapshot) < 0.20);
    }

    #[test]
    fn e_many_tool_schemas_are_classified_as_tool_schema() {
        let tools: Vec<ToolSpec> = (0..24)
            .map(|i| ToolSpec {
                name: format!("tool_{i}"),
                description: format!("description-{i}-{}", "d".repeat(400)),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": { "path": { "type": "string" } }
                }),
            })
            .collect();
        let messages = vec![ChatMessage::user("hi")];
        let body = final_native_body(&messages, &tools);
        let snapshot = build_snapshot(
            None,
            None,
            1,
            "openai",
            "gpt-4o",
            MeasurementKind::FinalRequestEstimate,
            &messages,
            &tools,
            None,
            Some(&body),
        );
        eprintln!("many_tools overhead_ratio={:.4}", overhead_ratio(&snapshot));
        assert!(
            snapshot.breakdown.tool_schema_tokens > snapshot.breakdown.conversation_tokens,
            "tool schemas leaked into overhead: {:?}",
            snapshot.breakdown
        );
        assert!(
            snapshot.breakdown.tool_schema_tokens * 2 > snapshot.estimated_input_tokens,
            "tool schemas not dominant: {:?}",
            snapshot.breakdown
        );
    }

    #[test]
    fn f_hidden_original_tool_content_is_not_counted() {
        let visible = "visible-result".repeat(20);
        let mut tool = ChatMessage::tool(format!(
            r#"{{"tool_call_id":"call-1","content":"{visible}"}}"#
        ));
        tool.original_tool_content = Some("H".repeat(1_900_000));
        let messages = vec![
            ChatMessage::system("sys"),
            ChatMessage::assistant(
                r#"{"tool_calls":[{"id":"call-1","name":"read","arguments":"{}"}]}"#,
            ),
            tool,
        ];
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
        );
        assert!(
            snapshot.estimated_input_tokens < 20_000,
            "hidden original_tool_content was counted: {}",
            snapshot.estimated_input_tokens
        );
        assert!(!serde_json::to_string(&snapshot).unwrap().contains("HHHH"));
    }

    #[test]
    fn g_visible_pruned_tool_result_is_counted() {
        let visible = "V".repeat(1_200);
        let messages = vec![ChatMessage::tool(format!(
            r#"{{"tool_call_id":"call-1","content":"{visible}"}}"#
        ))];
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
        );
        assert!(
            snapshot.breakdown.tool_result_tokens > 1_000,
            "visible tool result missing: {:?}",
            snapshot.breakdown
        );
        eprintln!(
            "pruned_tool_result overhead_ratio={:.4}",
            overhead_ratio(&snapshot)
        );
    }

    #[test]
    fn l_candidate_then_final_keeps_kind_and_revision_order() {
        let messages = sample_messages();
        let tools = sample_tools();
        let candidate = build_snapshot(
            Some("session-1".into()),
            Some("run-1".into()),
            4,
            "openai",
            "gpt-4o",
            MeasurementKind::CandidateEstimate,
            &messages,
            &tools,
            None,
            None,
        );
        let body = final_native_body(&messages, &tools);
        let final_snap = build_snapshot(
            Some("session-1".into()),
            Some("run-1".into()),
            5,
            "openai",
            "gpt-4o",
            MeasurementKind::CandidateEstimate,
            &messages,
            &tools,
            None,
            Some(&body),
        );
        assert_eq!(candidate.measurement_kind, MeasurementKind::CandidateEstimate);
        assert_eq!(
            final_snap.measurement_kind,
            MeasurementKind::FinalRequestEstimate
        );
        assert!(final_snap.request_revision > candidate.request_revision);
        assert_eq!(
            candidate.estimated_input_tokens,
            TokenEstimator::new().estimate_messages_with_tools(&messages, &tools)
        );
        assert_eq!(
            final_snap.estimated_input_tokens,
            TokenEstimator::new().estimate_request(&body)
        );
    }

    #[test]
    fn simple_chat_overhead_is_reported() {
        let messages = vec![ChatMessage::user("hello"), ChatMessage::assistant("hi")];
        let body = final_native_body(&messages, &[]);
        let snapshot = build_snapshot(
            None,
            None,
            1,
            "openai",
            "gpt-4o",
            MeasurementKind::FinalRequestEstimate,
            &messages,
            &[],
            None,
            Some(&body),
        );
        eprintln!(
            "simple_chat overhead_ratio={:.4} breakdown={:?}",
            overhead_ratio(&snapshot),
            snapshot.breakdown
        );
        assert_eq!(snapshot.breakdown.total(), snapshot.estimated_input_tokens);
        assert!(snapshot.breakdown.conversation_tokens > 0);
    }

    #[test]
    fn mixed_realistic_agent_request_overhead_is_reported() {
        let tools = sample_tools();
        let messages = vec![
            ChatMessage::system("You are OmniNova. ".to_string() + &"rule ".repeat(400)),
            ChatMessage::user("please inspect the repo ".to_string() + &"ctx ".repeat(200)),
            ChatMessage::assistant(
                r#"{"content":"calling","tool_calls":[{"id":"c1","name":"test","arguments":"{}"}]}"#,
            ),
            ChatMessage::tool(r#"{"tool_call_id":"c1","content":"ok result"}"#),
        ];
        let body = final_native_body(&messages, &tools);
        let snapshot = build_snapshot(
            None,
            None,
            1,
            "openai",
            "gpt-4o",
            MeasurementKind::FinalRequestEstimate,
            &messages,
            &tools,
            None,
            Some(&body),
        );
        eprintln!(
            "mixed overhead_ratio={:.4} mixed={:?}",
            overhead_ratio(&snapshot),
            snapshot.breakdown
        );
        assert_eq!(snapshot.breakdown.total(), snapshot.estimated_input_tokens);
        assert!(snapshot.breakdown.system_tokens > snapshot.breakdown.request_overhead_tokens);
        assert!(snapshot.breakdown.conversation_tokens > 0);
        assert!(snapshot.breakdown.tool_schema_tokens > 0);
        assert!(snapshot.breakdown.tool_result_tokens > 0);
    }

    #[test]
    fn c_exact_tokenizer_is_used_only_for_trusted_model() {
        let messages = sample_messages();
        let tools = sample_tools();
        let exact = build_snapshot(
            None,
            None,
            1,
            "openai",
            "omninova-test-exact",
            MeasurementKind::CandidateEstimate,
            &messages,
            &tools,
            None,
            None,
        );
        let unknown = build_snapshot(
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
        assert_eq!(
            exact.measurement_provenance,
            MeasurementProvenance::ExactTokenizer
        );
        assert_eq!(
            unknown.measurement_provenance,
            MeasurementProvenance::SafetyEstimate
        );
        assert_eq!(
            unknown.estimated_input_tokens,
            TokenEstimator::new().estimate_messages_with_tools(&messages, &tools)
        );
        assert_ne!(exact.estimated_input_tokens, unknown.estimated_input_tokens);
        assert_eq!(unknown.breakdown.total(), unknown.estimated_input_tokens);
        assert_ne!(exact.breakdown.total(), exact.estimated_input_tokens);
    }

    #[test]
    fn d_unknown_tokenizer_keeps_safety_estimate_total() {
        let messages = sample_messages();
        let snapshot = build_snapshot(
            None,
            None,
            1,
            "openai",
            "mystery-model",
            MeasurementKind::FinalRequestEstimate,
            &messages,
            &[],
            None,
            Some(&final_native_body(&messages, &[])),
        );
        assert_eq!(
            snapshot.measurement_provenance,
            MeasurementProvenance::SafetyEstimate
        );
        assert_eq!(
            snapshot.estimated_input_tokens,
            TokenEstimator::new().estimate_request(&final_native_body(&messages, &[]))
        );
    }

    #[test]
    fn f_provider_actual_total_does_not_rewrite_estimated_breakdown() {
        let messages = sample_messages();
        let tools = sample_tools();
        let body = final_native_body(&messages, &tools);
        let estimate = build_snapshot(
            Some("session-1".into()),
            Some("run-1".into()),
            9,
            "openai",
            "gpt-4o",
            MeasurementKind::FinalRequestEstimate,
            &messages,
            &tools,
            None,
            Some(&body),
        );
        let actual = estimate.clone().with_provider_actual(4_777);
        assert_eq!(actual.measurement_kind, MeasurementKind::ProviderActual);
        assert_eq!(
            actual.measurement_provenance,
            MeasurementProvenance::ProviderActual
        );
        assert_eq!(actual.provider_actual_input_tokens, Some(4_777));
        assert_eq!(actual.breakdown, estimate.breakdown);
        assert_ne!(actual.breakdown.total(), 4_777);
        assert_eq!(actual.request_revision, 9);
    }

    #[tokio::test]
    async fn n_deepseek_local_count_stays_separate_from_provider_actual() {
        let sink: SharedSink = Arc::new(VecContextTelemetry::new());
        with_context_telemetry(
            Some("session-ds".into()),
            Some("run-ds".into()),
            "deepseek",
            "deepseek-v4-flash",
            sink,
            async {
                set_exact_tokenizer(Some("deepseek_v4_flash_0731".into()));
                let messages = vec![ChatMessage::user("hello")];
                let snapshot = build_snapshot(
                    Some("session-ds".into()),
                    Some("run-ds".into()),
                    3,
                    "deepseek",
                    "deepseek-v4-flash",
                    MeasurementKind::FinalRequestEstimate,
                    &messages,
                    &[],
                    None,
                    None,
                );
                assert_eq!(
                    snapshot.measurement_provenance,
                    MeasurementProvenance::ExactTokenizer
                );
                assert!(!snapshot.measurement_exact);
                assert_ne!(snapshot.breakdown.total(), snapshot.estimated_input_tokens);
                let actual = snapshot.clone().with_provider_actual(4_001);
                assert_eq!(actual.measurement_kind, MeasurementKind::ProviderActual);
                assert_eq!(
                    actual.measurement_provenance,
                    MeasurementProvenance::ProviderActual
                );
                assert_eq!(actual.provider_actual_input_tokens, Some(4_001));
                assert_eq!(actual.breakdown, snapshot.breakdown);
                assert_ne!(actual.estimated_input_tokens, 4_001);
            },
        )
        .await;
    }

    #[test]
    fn provider_count_api_marks_exact_without_touching_breakdown() {
        let messages = sample_messages();
        let tools = sample_tools();
        let body = final_native_body(&messages, &tools);
        let estimate = build_snapshot(
            Some("session-1".into()),
            Some("run-1".into()),
            9,
            "openai",
            "gpt-4o",
            MeasurementKind::FinalRequestEstimate,
            &messages,
            &tools,
            None,
            Some(&body),
        );
        let exact = estimate.clone().with_provider_count_api(4_812, true);
        assert_eq!(exact.measurement_kind, MeasurementKind::FinalRequestEstimate);
        assert_eq!(
            exact.measurement_provenance,
            MeasurementProvenance::ProviderCountApi
        );
        assert!(exact.measurement_exact);
        assert_eq!(exact.estimated_input_tokens, 4_812);
        assert_eq!(exact.provider_actual_input_tokens, None);
        assert_eq!(exact.breakdown, estimate.breakdown);
        assert_ne!(exact.breakdown.total(), 4_812);
    }

    #[test]
    fn provider_count_api_source_does_not_imply_exactness() {
        let messages = sample_messages();
        let estimate = build_snapshot(
            Some("session-1".into()),
            Some("run-1".into()),
            9,
            "anthropic",
            "claude-sonnet-4-5",
            MeasurementKind::FinalRequestEstimate,
            &messages,
            &[],
            None,
            None,
        );
        let inexact = estimate.with_provider_count_api(4_812, false);
        assert_eq!(
            inexact.measurement_provenance,
            MeasurementProvenance::ProviderCountApi
        );
        assert!(!inexact.measurement_exact);
    }

    #[tokio::test]
    async fn stale_provider_count_snapshot_is_not_emitted_through_telemetry_path() {
        let sink = Arc::new(VecContextTelemetry::new());
        let ctx = Arc::new(ContextTelemetryContext::new(
            Some("session-stale".into()),
            Some("run-stale".into()),
            "anthropic",
            "claude-sonnet-4-5",
            sink.clone(),
        ));
        // Use with_context_telemetry to install the context; inside it emit a
        // revision 10 late result after revision 11 has already been allocated.
        crate::observability::with_context_telemetry(
            ctx.session_id.clone(),
            ctx.run_id.clone(),
            ctx.provider.clone(),
            ctx.model.clone(),
            sink.clone(),
            async {
                let ctx = current_context().expect("context installed");
                // Allocate revision 10 (the old request) and revision 11 (newer).
                let old = ctx.allocate_request_identity();
                assert_eq!(old.request_revision, 1);
                let new = ctx.allocate_request_identity();
                assert_eq!(new.request_revision, 2);
                let messages = sample_messages();
                let body = final_native_body(&messages, &[]);
                let old_snapshot = build_snapshot(
                    old.session_id.clone(),
                    old.run_id.clone(),
                    old.request_revision,
                    old.provider.clone(),
                    old.model.clone(),
                    MeasurementKind::FinalRequestEstimate,
                    &messages,
                    &[],
                    None,
                    Some(&body),
                )
                .with_provider_count_api(100, true);
                emit_snapshot(old_snapshot);
                let new_snapshot = build_snapshot(
                    new.session_id.clone(),
                    new.run_id.clone(),
                    new.request_revision,
                    new.provider.clone(),
                    new.model.clone(),
                    MeasurementKind::FinalRequestEstimate,
                    &messages,
                    &[],
                    None,
                    Some(&body),
                )
                .with_provider_count_api(200, true);
                emit_snapshot(new_snapshot);
            },
        )
        .await;
        let snapshots = sink.snapshots();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].request_revision, 2);
        assert_eq!(snapshots[0].estimated_input_tokens, 200);
    }

    #[tokio::test]
    async fn spawn_context_usage_task_preserves_context_identity_and_revision_after_await() {
        let sink = Arc::new(VecContextTelemetry::new());
        let ctx = Arc::new(ContextTelemetryContext::new(
            Some("session-async".into()),
            Some("run-async".into()),
            "anthropic",
            "claude-sonnet-4-5",
            sink,
        ));
        let identity = ctx.allocate_request_identity();
        let expected = identity.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();

        spawn_context_usage_task(ctx, async move {
            tokio::task::yield_now().await;
            let current = current_context().expect("CURRENT_CONTEXT is installed in spawned task");
            let observed = ContextRequestIdentity {
                session_id: current.session_id.clone(),
                run_id: current.run_id.clone(),
                request_revision: current.request_revision.load(Ordering::SeqCst),
                provider: current.provider.clone(),
                model: current.model.clone(),
            };
            tx.send(observed).unwrap();
        });

        let observed = rx.await.expect("spawned telemetry task completed");
        assert_eq!(observed, expected);
    }
}
