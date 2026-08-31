//! Session-driven context projection: reconstruct, measure, persist metadata.
//!
//! This path never calls a Provider, never prunes/compacts, and never stores
//! prompt or tool contents. The session sidecar remains the persistence
//! authority.

use super::context_telemetry::{
    build_snapshot, current_context, emit_snapshot, set_exact_tokenizer, with_context_telemetry,
    ContextUsageSnapshot, MeasurementKind, VecContextTelemetry,
};
use crate::providers::context_budget::ContextBudget;
use crate::providers::ChatMessage;
use crate::tools::ToolSpec;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Historical Provider-reported input tokens for a prior request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedContextActual {
    pub input_tokens: u64,
    pub request_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub measured_at: u64,
}

/// Last-known context projection stored with the session.
///
/// Numbers and identity only. Never prompt text, tool results, system prompt
/// contents, API keys, or request bodies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedContextProjection {
    pub snapshot: ContextUsageSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_actual: Option<PersistedContextActual>,
}

/// Whether a persisted snapshot may be shown immediately for this identity.
pub fn projection_identity_compatible(
    snapshot: &ContextUsageSnapshot,
    session_id: &str,
    provider: &str,
    model: &str,
) -> bool {
    if provider.trim().is_empty() || model.trim().is_empty() {
        return false;
    }
    if snapshot.provider != provider || snapshot.model != model {
        return false;
    }
    match snapshot.session_id.as_deref() {
        Some(id) if !id.is_empty() => id == session_id,
        _ => true,
    }
}

pub fn compatible_current_snapshot<'a>(
    persisted: &'a PersistedContextProjection,
    session_id: &str,
    provider: &str,
    model: &str,
) -> Option<&'a ContextUsageSnapshot> {
    if persisted.snapshot.measurement_kind == MeasurementKind::ProviderActual {
        return None;
    }
    if !projection_identity_compatible(&persisted.snapshot, session_id, provider, model) {
        return None;
    }
    Some(&persisted.snapshot)
}

pub fn compatible_last_actual<'a>(
    persisted: &'a PersistedContextProjection,
    session_id: &str,
    provider: &str,
    model: &str,
) -> Option<&'a PersistedContextActual> {
    let actual = persisted.last_actual.as_ref()?;
    if actual.provider != provider || actual.model != model {
        return None;
    }
    if !projection_identity_compatible(&persisted.snapshot, session_id, provider, model)
        && persisted.snapshot.session_id.as_deref() != Some(session_id)
    {
        return None;
    }
    if actual.input_tokens == 0 {
        return None;
    }
    Some(actual)
}

fn actual_from_snapshot(snapshot: &ContextUsageSnapshot) -> Option<PersistedContextActual> {
    let input_tokens = snapshot.provider_actual_input_tokens.filter(|tokens| *tokens > 0)?;
    Some(PersistedContextActual {
        input_tokens,
        request_revision: snapshot.request_revision,
        run_id: snapshot.run_id.clone(),
        provider: snapshot.provider.clone(),
        model: snapshot.model.clone(),
        measured_at: snapshot.measured_at,
    })
}

/// Fold a newly measured or observed snapshot into the persisted record.
pub fn merge_persisted_projection(
    existing: Option<PersistedContextProjection>,
    snapshot: &ContextUsageSnapshot,
) -> PersistedContextProjection {
    match snapshot.measurement_kind {
        MeasurementKind::ProviderActual => PersistedContextProjection {
            snapshot: existing
                .as_ref()
                .map(|item| item.snapshot.clone())
                .filter(|item| item.measurement_kind != MeasurementKind::ProviderActual)
                .unwrap_or_else(|| snapshot.clone()),
            last_actual: actual_from_snapshot(snapshot).or_else(|| {
                existing.and_then(|item| item.last_actual)
            }),
        },
        _ => PersistedContextProjection {
            snapshot: snapshot.clone(),
            last_actual: existing
                .and_then(|item| item.last_actual)
                .or_else(|| actual_from_snapshot(snapshot)),
        },
    }
}

/// Local Candidate measurement of a reconstructed model-visible surface.
///
/// Uses a trusted local tokenizer when configured, otherwise SafetyEstimate.
/// Does not call remote token-count APIs and does not send a Provider request.
pub async fn measure_projected_context(
    session_id: Option<String>,
    provider: &str,
    model: &str,
    exact_tokenizer: Option<String>,
    budget: Option<&ContextBudget>,
    messages: &[ChatMessage],
    tools: &[ToolSpec],
) -> ContextUsageSnapshot {
    let sink = Arc::new(VecContextTelemetry::new());
    with_context_telemetry(
        session_id,
        None,
        provider.to_string(),
        model.to_string(),
        sink.clone(),
        async {
            set_exact_tokenizer(exact_tokenizer);
            let ctx = current_context().expect("projection telemetry context");
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
            emit_snapshot(snapshot.clone());
            snapshot
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::prompt::reconstruct_model_visible_messages;
    use crate::config::AgentConfig;
    use crate::observability::MeasurementProvenance;
    use crate::providers::native_request::native_context_view_json;
    use crate::providers::{ChatMessage, ChatRequest, ChatResponse, Provider};
    use crate::tools::ToolSpec;
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn tool(name: &str, description: &str) -> ToolSpec {
        ToolSpec {
            name: name.into(),
            description: description.into(),
            parameters: json!({"type": "object", "properties": {}}),
        }
    }

    struct CountingProvider {
        chats: AtomicU64,
    }

    impl CountingProvider {
        fn new() -> Self {
            Self {
                chats: AtomicU64::new(0),
            }
        }
    }

    #[async_trait]
    impl Provider for CountingProvider {
        fn name(&self) -> &str {
            "counting"
        }

        fn model(&self) -> Option<&str> {
            Some("counting-model")
        }

        async fn chat(&self, _request: ChatRequest<'_>) -> anyhow::Result<ChatResponse> {
            self.chats.fetch_add(1, Ordering::SeqCst);
            panic!("session-open projection must not call Provider");
        }

        async fn health_check(&self) -> bool {
            true
        }
    }

    #[test]
    fn j_unsent_composer_draft_is_not_part_of_reconstruction() {
        let mut config = AgentConfig::default();
        config.system_prompt = Some("sys".into());
        let history = vec![
            ChatMessage::system("old"),
            ChatMessage::user("hello"),
            ChatMessage::assistant("hi"),
        ];
        let reconstructed = reconstruct_model_visible_messages(&config, history);
        let draft = "unsent composer draft that must not appear";
        assert!(reconstructed.iter().all(|message| !message.content.contains(draft)));
        assert_eq!(reconstructed.iter().filter(|m| m.role == "user").count(), 1);
    }

    #[test]
    fn m_system_instructions_included_exactly_once() {
        let mut config = AgentConfig::default();
        config.system_prompt = Some("CURRENT_AGENT_INSTRUCTIONS".into());
        let history = vec![
            ChatMessage::system("STALE_SYSTEM"),
            ChatMessage::system("[对话摘要] earlier"),
            ChatMessage::user("hello"),
        ];
        let reconstructed = reconstruct_model_visible_messages(&config, history);
        let system_current = reconstructed
            .iter()
            .filter(|m| m.role == "system" && m.content == "CURRENT_AGENT_INSTRUCTIONS")
            .count();
        assert_eq!(system_current, 1);
        assert!(reconstructed
            .iter()
            .any(|m| m.role == "system" && m.content.starts_with("[对话摘要]")));
        assert!(!reconstructed.iter().any(|m| m.content == "STALE_SYSTEM"));
    }

    #[test]
    fn n_reconstruction_does_not_prune_or_compact_history() {
        let mut config = AgentConfig::default();
        config.system_prompt = Some("sys".into());
        config.max_history_messages = 2;
        config.compact_context = true;
        let history = vec![
            ChatMessage::system("old"),
            ChatMessage::user("one"),
            ChatMessage::assistant("two"),
            ChatMessage::user("three"),
            ChatMessage::assistant("four"),
        ];
        let reconstructed = reconstruct_model_visible_messages(&config, history);
        assert_eq!(reconstructed.len(), 5);
        assert_eq!(reconstructed[1].content, "one");
        assert_eq!(reconstructed[4].content, "four");
    }

    #[tokio::test]
    async fn a_open_existing_session_emits_candidate_without_user_message() {
        let mut config = AgentConfig::default();
        config.system_prompt = Some("sys".into());
        let history = vec![
            ChatMessage::system("old"),
            ChatMessage::user("hello"),
            ChatMessage::assistant("world"),
        ];
        let reconstructed = reconstruct_model_visible_messages(&config, history);
        let tools = vec![tool("echo", "echo a value")];
        let snapshot = measure_projected_context(
            Some("session-a".into()),
            "mock",
            "mock-model",
            None,
            None,
            &reconstructed,
            &tools,
        )
        .await;
        assert_eq!(
            snapshot.measurement_kind,
            MeasurementKind::CandidateEstimate
        );
        assert_eq!(snapshot.session_id.as_deref(), Some("session-a"));
        assert!(snapshot.estimated_input_tokens > 0);
        assert_eq!(snapshot.run_id, None);
        assert!(!reconstructed.iter().any(|m| m.role == "user" && m.content != "hello"));
    }

    #[tokio::test]
    async fn b_session_open_measurement_performs_zero_provider_requests() {
        let provider = CountingProvider::new();
        let messages = vec![ChatMessage::user("hello")];
        let tools = vec![tool("echo", "echo")];
        let _snapshot = measure_projected_context(
            Some("session-b".into()),
            provider.name(),
            provider.model().unwrap(),
            None,
            provider.context_budget().as_ref(),
            &messages,
            &tools,
        )
        .await;
        assert_eq!(provider.chats.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn r2_k_projection_reports_request_runtime_budget_without_provider_call() {
        let provider = CountingProvider::new();
        let budget = crate::providers::context_budget::ContextBudget::new(
            1_000_000,
            Some(384_000),
            crate::providers::context_budget::ContextBudgetSource::BuiltIn,
        )
        .with_request_output_cap(Some(32_000));
        let snapshot = measure_projected_context(
            Some("session-k".into()),
            provider.name(),
            "deepseek-v4-flash",
            None,
            Some(&budget),
            &[ChatMessage::user("hello")],
            &[],
        )
        .await;
        assert_eq!(provider.chats.load(Ordering::SeqCst), 0);
        assert_eq!(snapshot.max_input_tokens, Some(1_000_000 - 32_000 - 32_768));
        assert_eq!(snapshot.request_output_reserve_tokens, Some(32_000));
        assert_eq!(snapshot.model_max_output_tokens, Some(384_000));
        assert_eq!(snapshot.safety_reserve_tokens, Some(32_768));
        assert_eq!(
            snapshot.measurement_kind,
            MeasurementKind::CandidateEstimate
        );
    }

    #[tokio::test]
    async fn r21_g_projection_uses_factory_request_limit_without_provider_call() {
        let mut config = crate::config::Config::default();
        config.model_providers.insert(
            "deepseek".into(),
            crate::config::ModelProviderConfig {
                enabled: true,
                api_key: Some("sk-test".into()),
                default_model: Some("deepseek-v4-flash".into()),
                models: vec!["deepseek-v4-flash".into()],
                request_max_output_tokens: Some(32_000),
                ..crate::config::ModelProviderConfig::default()
            },
        );
        let factory_provider = crate::providers::factory::build_provider_with_selection(
            &config,
            &crate::providers::factory::ProviderSelection {
                provider: Some("deepseek".into()),
                model: Some("deepseek-v4-flash".into()),
            },
        );
        let budget = factory_provider.context_budget();
        let counting = CountingProvider::new();
        let snapshot = measure_projected_context(
            Some("session-r21-g".into()),
            counting.name(),
            "deepseek-v4-flash",
            None,
            budget.as_ref(),
            &[ChatMessage::user("hello")],
            &[],
        )
        .await;
        assert_eq!(counting.chats.load(Ordering::SeqCst), 0);
        assert_eq!(snapshot.request_output_reserve_tokens, Some(32_000));
        assert_eq!(snapshot.max_input_tokens, Some(935_232));
        assert_eq!(snapshot.model_max_output_tokens, Some(384_000));
        assert_eq!(
            snapshot.request_generation_limit_source.as_deref(),
            Some("profile_override")
        );
    }

    #[tokio::test]
    async fn r24_j_projection_uses_product_default_without_provider_call() {
        let mut config = crate::config::Config::default();
        config.model_providers.insert(
            "deepseek".into(),
            crate::config::ModelProviderConfig {
                enabled: true,
                api_key: Some("sk-test".into()),
                default_model: Some("deepseek-v4-flash".into()),
                models: vec!["deepseek-v4-flash".into()],
                ..crate::config::ModelProviderConfig::default()
            },
        );
        let factory_provider = crate::providers::factory::build_provider_with_selection(
            &config,
            &crate::providers::factory::ProviderSelection {
                provider: Some("deepseek".into()),
                model: Some("deepseek-v4-flash".into()),
            },
        );
        let budget = factory_provider.context_budget();
        let counting = CountingProvider::new();
        let snapshot = measure_projected_context(
            Some("session-r24-j".into()),
            counting.name(),
            "deepseek-v4-flash",
            None,
            budget.as_ref(),
            &[ChatMessage::user("hello")],
            &[],
        )
        .await;
        assert_eq!(counting.chats.load(Ordering::SeqCst), 0);
        assert_eq!(snapshot.request_output_reserve_tokens, Some(32_000));
        assert_eq!(snapshot.max_input_tokens, Some(935_232));
        assert_eq!(
            snapshot.request_generation_limit_source.as_deref(),
            Some("product_default")
        );
    }

    #[tokio::test]
    async fn g_trusted_local_tokenizer_is_used_when_available() {
        let messages = vec![ChatMessage::system("sys"), ChatMessage::user("hello world")];
        let snapshot = measure_projected_context(
            Some("session-g".into()),
            "test",
            "omninova-test-exact",
            Some("test_exact".into()),
            None,
            &messages,
            &[],
        )
        .await;
        assert_eq!(
            snapshot.measurement_provenance,
            MeasurementProvenance::ExactTokenizer
        );
        assert_eq!(
            snapshot.measurement_kind,
            MeasurementKind::CandidateEstimate
        );
    }

    #[tokio::test]
    async fn h_unknown_tokenizer_falls_back_to_safety_estimate() {
        let messages = vec![ChatMessage::system("sys"), ChatMessage::user("hello world")];
        let snapshot = measure_projected_context(
            Some("session-h".into()),
            "unknown",
            "unknown-alias",
            Some("not_a_real_tokenizer".into()),
            None,
            &messages,
            &[],
        )
        .await;
        assert_eq!(
            snapshot.measurement_provenance,
            MeasurementProvenance::SafetyEstimate
        );
        assert!(!snapshot.measurement_exact);
    }

    #[test]
    fn i_provider_actual_remains_separate_historical_anchor() {
        let candidate = ContextUsageSnapshot {
            session_id: Some("session-i".into()),
            run_id: Some("run-1".into()),
            request_revision: 3,
            provider: "deepseek".into(),
            model: "deepseek-chat".into(),
            measurement_kind: MeasurementKind::CandidateEstimate,
            measurement_provenance: MeasurementProvenance::ExactTokenizer,
            measurement_exact: false,
            estimated_input_tokens: 11_300,
            provider_actual_input_tokens: None,
            context_window_tokens: Some(640_000),
            max_input_tokens: Some(583_000),
            output_reserve_tokens: None,
            model_max_output_tokens: None,
            request_output_reserve_tokens: None,
            request_generation_limit_source: None,
            safety_reserve_tokens: None,
            pressure_threshold_tokens: Some(466_400),
            budget_source: None,
            usage_ratio: None,
            breakdown: Default::default(),
            measured_at: 1,
        };
        let mut actual = candidate.clone();
        actual.measurement_kind = MeasurementKind::ProviderActual;
        actual.measurement_provenance = MeasurementProvenance::ProviderActual;
        actual.measurement_exact = true;
        actual.provider_actual_input_tokens = Some(4_777);
        actual.estimated_input_tokens = 4_777;

        let persisted = merge_persisted_projection(None, &candidate);
        let persisted = merge_persisted_projection(Some(persisted), &actual);
        assert_eq!(
            persisted.snapshot.measurement_kind,
            MeasurementKind::CandidateEstimate
        );
        assert_eq!(persisted.snapshot.estimated_input_tokens, 11_300);
        assert_eq!(persisted.last_actual.as_ref().unwrap().input_tokens, 4_777);

        let current = compatible_current_snapshot(
            &persisted,
            "session-i",
            "deepseek",
            "deepseek-chat",
        )
        .expect("candidate");
        assert_eq!(current.estimated_input_tokens, 11_300);
        assert_eq!(
            compatible_last_actual(&persisted, "session-i", "deepseek", "deepseek-chat")
                .unwrap()
                .input_tokens,
            4_777
        );
    }

    #[tokio::test]
    async fn k_hidden_original_tool_content_is_excluded() {
        let mut tool_msg = ChatMessage::tool(
            json!({"tool_call_id":"call-1","content":"VISIBLE_TOOL_RESULT"}).to_string(),
        );
        tool_msg.original_tool_content = Some("HIDDEN_ORIGINAL_TOOL_CONTENT".into());
        let messages = vec![
            ChatMessage::system("sys"),
            ChatMessage::user("run"),
            ChatMessage::assistant(r#"{"tool_calls":[{"id":"call-1","name":"echo","arguments":"{}"}]}"#),
            tool_msg,
        ];
        let view = native_context_view_json(&messages, &[]);
        assert!(view.contains("VISIBLE_TOOL_RESULT"));
        assert!(!view.contains("HIDDEN_ORIGINAL_TOOL_CONTENT"));

        let snapshot = measure_projected_context(
            Some("session-k".into()),
            "mock",
            "mock-model",
            None,
            None,
            &messages,
            &[],
        )
        .await;
        let encoded = serde_json::to_string(&snapshot).unwrap();
        assert!(!encoded.contains("HIDDEN_ORIGINAL_TOOL_CONTENT"));
        assert!(snapshot.breakdown.tool_result_tokens > 0);
    }

    #[tokio::test]
    async fn l_current_tool_schemas_are_included() {
        let messages = vec![ChatMessage::system("sys"), ChatMessage::user("hello")];
        let tools = vec![tool(
            "unique_projection_tool",
            "a distinctive tool schema for projection",
        )];
        let with_tools = measure_projected_context(
            Some("session-l".into()),
            "mock",
            "mock-model",
            None,
            None,
            &messages,
            &tools,
        )
        .await;
        let without_tools = measure_projected_context(
            Some("session-l".into()),
            "mock",
            "mock-model",
            None,
            None,
            &messages,
            &[],
        )
        .await;
        assert!(with_tools.breakdown.tool_schema_tokens > 0);
        assert!(with_tools.estimated_input_tokens > without_tools.estimated_input_tokens);
    }

    #[test]
    fn f_provider_model_mismatch_rejects_incompatible_snapshot() {
        let snapshot = ContextUsageSnapshot {
            session_id: Some("session-f".into()),
            run_id: None,
            request_revision: 1,
            provider: "deepseek".into(),
            model: "deepseek-chat".into(),
            measurement_kind: MeasurementKind::CandidateEstimate,
            measurement_provenance: MeasurementProvenance::ExactTokenizer,
            measurement_exact: false,
            estimated_input_tokens: 99,
            provider_actual_input_tokens: None,
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
            measured_at: 1,
        };
        let persisted = PersistedContextProjection {
            snapshot: snapshot.clone(),
            last_actual: None,
        };
        assert!(compatible_current_snapshot(
            &persisted,
            "session-f",
            "deepseek",
            "deepseek-chat"
        )
        .is_some());
        assert!(compatible_current_snapshot(
            &persisted,
            "session-f",
            "anthropic",
            "claude-sonnet"
        )
        .is_none());
        assert!(compatible_current_snapshot(
            &persisted,
            "session-other",
            "deepseek",
            "deepseek-chat"
        )
        .is_none());
        assert!(!projection_identity_compatible(
            &snapshot,
            "session-f",
            "deepseek",
            "deepseek-reasoner"
        ));
    }

    #[test]
    fn e_session_snapshots_are_keyed_per_session() {
        let snap_a = ContextUsageSnapshot {
            session_id: Some("session-a".into()),
            run_id: None,
            request_revision: 1,
            provider: "mock".into(),
            model: "mock-model".into(),
            measurement_kind: MeasurementKind::CandidateEstimate,
            measurement_provenance: MeasurementProvenance::SafetyEstimate,
            measurement_exact: false,
            estimated_input_tokens: 10,
            provider_actual_input_tokens: None,
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
            measured_at: 1,
        };
        let mut snap_b = snap_a.clone();
        snap_b.session_id = Some("session-b".into());
        snap_b.estimated_input_tokens = 20;
        let persisted_a = PersistedContextProjection {
            snapshot: snap_a,
            last_actual: None,
        };
        let persisted_b = PersistedContextProjection {
            snapshot: snap_b,
            last_actual: None,
        };
        assert_eq!(
            compatible_current_snapshot(&persisted_a, "session-a", "mock", "mock-model")
                .unwrap()
                .estimated_input_tokens,
            10
        );
        assert_eq!(
            compatible_current_snapshot(&persisted_b, "session-b", "mock", "mock-model")
                .unwrap()
                .estimated_input_tokens,
            20
        );
        assert!(compatible_current_snapshot(
            &persisted_a,
            "session-b",
            "mock",
            "mock-model"
        )
        .is_none());
    }
}
