//! Diagnostic-only DeepSeek local tokenizer vs ProviderActual comparison.
//!
//! This module never affects C1/C2, budgets, pruning, compaction, retries,
//! ProviderActual display totals, or request success. Records contain only
//! numbers and identity metadata — never prompt or tool contents.

use super::context_telemetry::{
    ContextUsageSnapshot, MeasurementKind, MeasurementProvenance,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};

pub const TOKEN_PARITY_HISTORY_LIMIT: usize = 20;

/// Same-revision local ExactTokenizer vs ProviderActual comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenParityRecord {
    pub provider: String,
    pub model: String,
    pub request_revision: u64,
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub local_tokens: u64,
    pub provider_actual_tokens: u64,
    pub delta: i64,
    pub abs_error: u64,
    pub relative_error_percent: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ParityIdentity {
    session_id: Option<String>,
    run_id: Option<String>,
    provider: String,
    model: String,
    request_revision: u64,
}

impl ParityIdentity {
    fn from_snapshot(snapshot: &ContextUsageSnapshot) -> Self {
        Self {
            session_id: snapshot.session_id.clone(),
            run_id: snapshot.run_id.clone(),
            provider: snapshot.provider.clone(),
            model: snapshot.model.clone(),
            request_revision: snapshot.request_revision,
        }
    }
}

#[derive(Debug, Default, Clone)]
struct PendingSides {
    local_tokens: Option<u64>,
    actual_tokens: Option<u64>,
}

#[derive(Debug)]
pub struct TokenParityCollector {
    pending: HashMap<ParityIdentity, PendingSides>,
    pending_order: VecDeque<ParityIdentity>,
    history: VecDeque<TokenParityRecord>,
}

impl Default for TokenParityCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenParityCollector {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            pending_order: VecDeque::new(),
            history: VecDeque::new(),
        }
    }

    pub fn history(&self) -> Vec<TokenParityRecord> {
        self.history.iter().cloned().collect()
    }

    /// Observes one usage snapshot. Returns a completed parity record when
    /// both ExactTokenizer local and ProviderActual sides exist for the same
    /// identity. SafetyEstimate is never treated as local exact.
    pub fn observe(&mut self, snapshot: &ContextUsageSnapshot) -> Option<TokenParityRecord> {
        let identity = ParityIdentity::from_snapshot(snapshot);
        let mut sides = self
            .pending
            .get(&identity)
            .cloned()
            .unwrap_or_default();

        match snapshot.measurement_kind {
            MeasurementKind::FinalRequestEstimate
                if snapshot.measurement_provenance == MeasurementProvenance::ExactTokenizer =>
            {
                sides.local_tokens = Some(snapshot.estimated_input_tokens);
            }
            MeasurementKind::ProviderActual => {
                let Some(actual) = snapshot.provider_actual_input_tokens else {
                    return None;
                };
                if actual == 0 {
                    return None;
                }
                sides.actual_tokens = Some(actual);
            }
            _ => return None,
        }

        let is_new = !self.pending.contains_key(&identity);
        self.pending.insert(identity.clone(), sides.clone());
        if is_new {
            self.pending_order.push_back(identity.clone());
            self.evict_pending();
        }

        let (Some(local_tokens), Some(provider_actual_tokens)) =
            (sides.local_tokens, sides.actual_tokens)
        else {
            return None;
        };

        let delta = local_tokens as i64 - provider_actual_tokens as i64;
        let abs_error = delta.unsigned_abs();
        let relative_error_percent =
            (abs_error as f64 / provider_actual_tokens as f64) * 100.0;
        let record = TokenParityRecord {
            provider: identity.provider.clone(),
            model: identity.model.clone(),
            request_revision: identity.request_revision,
            session_id: identity.session_id.clone(),
            run_id: identity.run_id.clone(),
            local_tokens,
            provider_actual_tokens,
            delta,
            abs_error,
            relative_error_percent,
        };
        self.push_history(record.clone());
        Some(record)
    }

    fn evict_pending(&mut self) {
        while self.pending_order.len() > TOKEN_PARITY_HISTORY_LIMIT {
            if let Some(old) = self.pending_order.pop_front() {
                self.pending.remove(&old);
            }
        }
    }

    fn push_history(&mut self, record: TokenParityRecord) {
        if let Some(existing) = self.history.iter_mut().find(|item| {
            item.session_id == record.session_id
                && item.run_id == record.run_id
                && item.provider == record.provider
                && item.model == record.model
                && item.request_revision == record.request_revision
        }) {
            *existing = record;
            return;
        }
        self.history.push_back(record);
        while self.history.len() > TOKEN_PARITY_HISTORY_LIMIT {
            self.history.pop_front();
        }
    }
}

fn global_collector() -> &'static Mutex<TokenParityCollector> {
    static COLLECTOR: OnceLock<Mutex<TokenParityCollector>> = OnceLock::new();
    COLLECTOR.get_or_init(|| Mutex::new(TokenParityCollector::new()))
}

/// Process-wide diagnostic observer used by `emit_snapshot`.
pub fn observe_token_parity(snapshot: &ContextUsageSnapshot) {
    let Some(record) = global_collector()
        .lock()
        .ok()
        .and_then(|mut collector| collector.observe(snapshot))
    else {
        return;
    };
    tracing::info!(
        target: "omninova_core::observability::token_parity",
        provider = %record.provider,
        model = %record.model,
        request_revision = record.request_revision,
        local_tokens = record.local_tokens,
        provider_actual_tokens = record.provider_actual_tokens,
        delta = record.delta,
        abs_error = record.abs_error,
        relative_error_percent = record.relative_error_percent,
        "token_parity"
    );
}

pub fn token_parity_history() -> Vec<TokenParityRecord> {
    global_collector()
        .lock()
        .map(|collector| collector.history())
        .unwrap_or_default()
}

#[cfg(test)]
pub fn reset_token_parity_for_tests() {
    if let Ok(mut collector) = global_collector().lock() {
        *collector = TokenParityCollector::new();
    }
}

#[cfg(test)]
fn exact_local(
    session: &str,
    run: &str,
    provider: &str,
    model: &str,
    revision: u64,
    tokens: u64,
) -> ContextUsageSnapshot {
    snapshot(
        session,
        run,
        provider,
        model,
        revision,
        MeasurementKind::FinalRequestEstimate,
        MeasurementProvenance::ExactTokenizer,
        tokens,
        None,
    )
}

#[cfg(test)]
fn actual(
    session: &str,
    run: &str,
    provider: &str,
    model: &str,
    revision: u64,
    tokens: u64,
) -> ContextUsageSnapshot {
    snapshot(
        session,
        run,
        provider,
        model,
        revision,
        MeasurementKind::ProviderActual,
        MeasurementProvenance::ProviderActual,
        tokens,
        Some(tokens),
    )
}

#[cfg(test)]
fn safety(
    session: &str,
    run: &str,
    provider: &str,
    model: &str,
    revision: u64,
    tokens: u64,
) -> ContextUsageSnapshot {
    snapshot(
        session,
        run,
        provider,
        model,
        revision,
        MeasurementKind::FinalRequestEstimate,
        MeasurementProvenance::SafetyEstimate,
        tokens,
        None,
    )
}

#[cfg(test)]
fn snapshot(
    session: &str,
    run: &str,
    provider: &str,
    model: &str,
    revision: u64,
    kind: MeasurementKind,
    provenance: MeasurementProvenance,
    estimated: u64,
    actual_tokens: Option<u64>,
) -> ContextUsageSnapshot {
    ContextUsageSnapshot {
        session_id: Some(session.into()),
        run_id: Some(run.into()),
        request_revision: revision,
        provider: provider.into(),
        model: model.into(),
        measurement_kind: kind,
        measurement_provenance: provenance,
        measurement_exact: false,
        estimated_input_tokens: estimated,
        provider_actual_input_tokens: actual_tokens,
        context_window_tokens: None,
        max_input_tokens: None,
        output_reserve_tokens: None,
        model_max_output_tokens: None,
        request_output_reserve_tokens: None,
        request_generation_limit_source: None,
        safety_reserve_tokens: None,
        pressure_threshold_tokens: None,
        budget_source: None,
        usage_ratio: None,
        breakdown: Default::default(),
        measured_at: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_same_revision_local_and_actual_creates_parity_record() {
        let mut collector = TokenParityCollector::new();
        assert!(collector
            .observe(&exact_local(
                "s",
                "r",
                "deepseek",
                "deepseek-v4-flash",
                3,
                4_545,
            ))
            .is_none());
        let record = collector
            .observe(&actual(
                "s",
                "r",
                "deepseek",
                "deepseek-v4-flash",
                3,
                4_561,
            ))
            .expect("paired");
        assert_eq!(record.local_tokens, 4_545);
        assert_eq!(record.provider_actual_tokens, 4_561);
        assert_eq!(record.delta, -16);
        assert_eq!(record.abs_error, 16);
        assert!((record.relative_error_percent - 16.0 * 100.0 / 4_561.0).abs() < 1e-9);
        assert_eq!(collector.history().len(), 1);
    }

    #[test]
    fn b_different_revisions_are_never_compared() {
        let mut collector = TokenParityCollector::new();
        collector.observe(&exact_local(
            "s",
            "r",
            "deepseek",
            "deepseek-v4-flash",
            3,
            100,
        ));
        assert!(collector
            .observe(&actual(
                "s",
                "r",
                "deepseek",
                "deepseek-v4-flash",
                4,
                110,
            ))
            .is_none());
        assert!(collector.history().is_empty());
    }

    #[test]
    fn c_different_provider_or_model_are_never_compared() {
        let mut collector = TokenParityCollector::new();
        collector.observe(&exact_local(
            "s",
            "r",
            "deepseek",
            "deepseek-v4-flash",
            3,
            100,
        ));
        assert!(collector
            .observe(&actual("s", "r", "openai", "deepseek-v4-flash", 3, 110))
            .is_none());
        assert!(collector
            .observe(&actual("s", "r", "deepseek", "deepseek-v4-pro", 3, 110))
            .is_none());
        assert!(collector.history().is_empty());
    }

    #[test]
    fn d_actual_arriving_before_local_remains_safe() {
        let mut collector = TokenParityCollector::new();
        assert!(collector
            .observe(&actual(
                "s",
                "r",
                "deepseek",
                "deepseek-v4-flash",
                3,
                4_561,
            ))
            .is_none());
        assert!(collector.history().is_empty());
        let record = collector
            .observe(&exact_local(
                "s",
                "r",
                "deepseek",
                "deepseek-v4-flash",
                3,
                4_545,
            ))
            .expect("paired after local");
        assert_eq!(record.local_tokens, 4_545);
        assert_eq!(record.provider_actual_tokens, 4_561);
    }

    #[test]
    fn e_local_arriving_before_actual_works() {
        let mut collector = TokenParityCollector::new();
        collector.observe(&exact_local(
            "s",
            "r",
            "deepseek",
            "deepseek-v4-flash",
            8,
            200,
        ));
        let record = collector
            .observe(&actual(
                "s",
                "r",
                "deepseek",
                "deepseek-v4-flash",
                8,
                210,
            ))
            .expect("paired");
        assert_eq!(record.delta, -10);
    }

    #[test]
    fn h_safety_estimate_is_not_compared_as_exact_tokenizer_parity() {
        let mut collector = TokenParityCollector::new();
        collector.observe(&safety(
            "s",
            "r",
            "deepseek",
            "deepseek-v4-flash",
            3,
            4_545,
        ));
        assert!(collector
            .observe(&actual(
                "s",
                "r",
                "deepseek",
                "deepseek-v4-flash",
                3,
                4_561,
            ))
            .is_none());
        assert!(collector.history().is_empty());
    }

    #[test]
    fn i_parity_record_has_no_prompt_or_tool_contents() {
        let mut collector = TokenParityCollector::new();
        collector.observe(&exact_local(
            "s",
            "r",
            "deepseek",
            "deepseek-v4-flash",
            1,
            10,
        ));
        let record = collector
            .observe(&actual(
                "s",
                "r",
                "deepseek",
                "deepseek-v4-flash",
                1,
                12,
            ))
            .unwrap();
        let json = serde_json::to_string(&record).unwrap();
        for forbidden in [
            "content",
            "messages",
            "tools",
            "prompt",
            "hello",
            "tool_call",
            "arguments",
        ] {
            assert!(
                !json.contains(forbidden),
                "diagnostic JSON must not contain {forbidden}: {json}"
            );
        }
        assert!(json.contains("local_tokens"));
        assert!(json.contains("provider_actual_tokens"));
    }

    #[test]
    fn candidate_estimate_is_not_treated_as_pre_send_local() {
        let mut collector = TokenParityCollector::new();
        let mut candidate = exact_local("s", "r", "deepseek", "deepseek-v4-flash", 3, 99);
        candidate.measurement_kind = MeasurementKind::CandidateEstimate;
        collector.observe(&candidate);
        assert!(collector
            .observe(&actual(
                "s",
                "r",
                "deepseek",
                "deepseek-v4-flash",
                3,
                100,
            ))
            .is_none());
    }

    #[test]
    fn g_observing_parity_does_not_emit_lifecycle_or_mutate_snapshot() {
        let mut collector = TokenParityCollector::new();
        let local = exact_local("s", "r", "deepseek", "deepseek-v4-flash", 2, 50);
        let before = local.clone();
        collector.observe(&local);
        assert_eq!(local, before);
        let act = actual("s", "r", "deepseek", "deepseek-v4-flash", 2, 55);
        collector.observe(&act);
        assert_eq!(act.measurement_kind, MeasurementKind::ProviderActual);
        assert_eq!(act.measurement_provenance, MeasurementProvenance::ProviderActual);
        assert!(!act.measurement_exact);
    }

    #[test]
    fn emit_snapshot_records_parity_for_matching_identity() {
        reset_token_parity_for_tests();
        let local = exact_local(
            "v12c-emit",
            "run-emit",
            "deepseek",
            "deepseek-v4-flash",
            11,
            4_545,
        );
        let act = actual(
            "v12c-emit",
            "run-emit",
            "deepseek",
            "deepseek-v4-flash",
            11,
            4_561,
        );
        crate::observability::set_global_context_telemetry(std::sync::Arc::new(
            crate::observability::VecContextTelemetry::new(),
        ));
        crate::observability::emit_snapshot(local);
        crate::observability::emit_snapshot(act);
        crate::observability::clear_global_context_telemetry();
        let history = token_parity_history();
        let record = history
            .iter()
            .find(|item| item.session_id.as_deref() == Some("v12c-emit"))
            .expect("emit_snapshot should collect parity");
        assert_eq!(record.local_tokens, 4_545);
        assert_eq!(record.provider_actual_tokens, 4_561);
        reset_token_parity_for_tests();
    }
}
