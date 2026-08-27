//! Session-open context projection: restore last-known snapshot, reconstruct
//! locally, measure without a Provider request, and persist metadata only.

use super::assembly::AgentAssemblyRequest;
use super::{
    config_with_subagents, load_session_history, load_session_record, now_unix_ts,
    session_key, session_store_path, atomic_write_string, load_session_store, ExecutionStep,
    GatewayRuntime,
};
use crate::agent::sanitize_messages_for_provider;
use crate::channels::{ChannelKind, InboundMessage};
use crate::config::Config;
use crate::observability::{
    compatible_current_snapshot, compatible_last_actual, merge_persisted_projection,
    ContextUsageSnapshot, PersistedContextProjection,
};
use crate::routing::resolve_agent_route;
use crate::security::SecurityContext;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GatewayContextProjectionRequest {
    pub session_id: Option<String>,
    pub channel: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GatewayContextProjectionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restored: Option<PersistedContextProjection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<ContextUsageSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_actual: Option<crate::observability::PersistedContextActual>,
    pub unavailable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl GatewayContextProjectionResponse {
    fn failed(
        restored: Option<PersistedContextProjection>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            restored,
            current: None,
            last_actual: None,
            unavailable: true,
            error: Some(error.into()),
        }
    }
}

pub(super) async fn persist_context_projection(
    config: &Config,
    channel: &ChannelKind,
    session_id: &str,
    projection: PersistedContextProjection,
) -> anyhow::Result<()> {
    let key = session_key(channel, session_id);
    let path = session_store_path(config);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut loaded = load_session_store(&path).await?;
    let record = loaded.store.sessions.entry(key).or_default();
    record.context_projection = Some(projection);
    if record.updated_at == 0 {
        record.updated_at = now_unix_ts();
    }
    let serialized = serde_json::to_string_pretty(&loaded.store)?;
    atomic_write_string(&path, &serialized).await?;
    Ok(())
}

fn restored_for_identity(
    persisted: Option<&PersistedContextProjection>,
    session_id: &str,
    provider: &str,
    model: &str,
) -> Option<PersistedContextProjection> {
    let persisted = persisted?;
    let current = compatible_current_snapshot(persisted, session_id, provider, model).cloned()?;
    let last_actual = compatible_last_actual(persisted, session_id, provider, model).cloned();
    Some(PersistedContextProjection {
        snapshot: current,
        last_actual,
    })
}

impl GatewayRuntime {
    pub async fn project_session_context(
        &self,
        channel: &ChannelKind,
        session_id: &str,
        provider: Option<String>,
        model: Option<String>,
    ) -> GatewayContextProjectionResponse {
        let cfg = config_with_subagents(self.config.read().await.clone());
        let record = load_session_record(&cfg, channel, session_id)
            .await
            .ok()
            .flatten();
        let persisted = record.as_ref().and_then(|item| item.context_projection.clone());
        match self
            .project_session_context_inner(
                &cfg,
                channel,
                session_id,
                provider.as_deref(),
                model.as_deref(),
                persisted.as_ref(),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => {
                let restored = persisted.and_then(|item| {
                    restored_for_identity(
                        Some(&item),
                        session_id,
                        provider.as_deref().unwrap_or(""),
                        model.as_deref().unwrap_or(""),
                    )
                });
                GatewayContextProjectionResponse::failed(restored, error.to_string())
            }
        }
    }

    async fn project_session_context_inner(
        &self,
        cfg: &Config,
        channel: &ChannelKind,
        session_id: &str,
        provider: Option<&str>,
        model: Option<&str>,
        persisted: Option<&PersistedContextProjection>,
    ) -> anyhow::Result<GatewayContextProjectionResponse> {
        let mut metadata = HashMap::new();
        if let Some(provider) = provider.filter(|value| !value.is_empty() && *value != "auto") {
            metadata.insert(
                "preferred_provider".into(),
                serde_json::Value::String(provider.to_string()),
            );
        }
        if let Some(model) = model.filter(|value| !value.is_empty()) {
            metadata.insert(
                "preferred_model".into(),
                serde_json::Value::String(model.to_string()),
            );
        }
        let inbound = InboundMessage {
            channel: channel.clone(),
            session_id: Some(session_id.to_string()),
            text: String::new(),
            metadata,
            user_id: None,
        };
        let route = resolve_agent_route(cfg, &inbound);
        let resolved_provider = route
            .provider
            .clone()
            .or_else(|| provider.map(str::to_string))
            .or_else(|| cfg.default_provider.clone())
            .unwrap_or_default();
        let resolved_model = route
            .model
            .clone()
            .or_else(|| model.map(str::to_string))
            .or_else(|| cfg.default_model.clone())
            .unwrap_or_default();
        let restored = restored_for_identity(
            persisted,
            session_id,
            &resolved_provider,
            &resolved_model,
        );

        let security = SecurityContext::for_inbound(cfg, &inbound, &route);
        let workspace = if cfg.workspace_dir.as_os_str().is_empty() {
            std::path::PathBuf::from(".")
        } else {
            cfg.workspace_dir.clone()
        };
        let assembly_request = AgentAssemblyRequest {
            route: &route,
            channel,
            session_id: Some(session_id),
            workspace: &workspace,
            spawn_depth: 0,
            skill_invocations: &[],
            security: &security,
        };
        let mut steps: Vec<ExecutionStep> = Vec::new();
        let mut agent = self
            .assemble_agent(cfg, &assembly_request, &mut steps)
            .await?;

        let loaded = load_session_history(cfg, channel, session_id).await?;
        let history_len_before = loaded.messages.len();
        let sanitized = sanitize_messages_for_provider(loaded.messages);
        agent.reconstruct_for_projection(sanitized);

        let current = agent
            .project_open_context(Some(session_id.to_string()))
            .await;

        let loaded_after = load_session_history(cfg, channel, session_id).await?;
        if loaded_after.messages.len() != history_len_before {
            anyhow::bail!("session-open projection must not mutate session history");
        }

        let last_actual = compatible_last_actual(
            &PersistedContextProjection {
                snapshot: current.clone(),
                last_actual: persisted.and_then(|item| {
                    compatible_last_actual(item, session_id, &resolved_provider, &resolved_model)
                        .cloned()
                }),
            },
            session_id,
            &resolved_provider,
            &resolved_model,
        )
        .cloned()
        .or_else(|| {
            persisted.and_then(|item| {
                compatible_last_actual(item, session_id, &resolved_provider, &resolved_model)
                    .cloned()
            })
        });

        let merged = merge_persisted_projection(
            Some(PersistedContextProjection {
                snapshot: current.clone(),
                last_actual: last_actual.clone(),
            }),
            &current,
        );
        {
            let _guard = self.session_store_guard.lock().await;
            persist_context_projection(cfg, channel, session_id, merged).await?;
        }

        Ok(GatewayContextProjectionResponse {
            restored,
            current: Some(current),
            last_actual,
            unavailable: false,
            error: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{load_session_history, save_session_history};
    use crate::config::{Config, ProviderConfig};
    use crate::observability::{
        MeasurementKind, MeasurementProvenance, PersistedContextActual,
    };
    use crate::providers::ChatMessage;

    fn mock_config() -> (Config, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "omninova-context-projection-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).expect("temp workspace");
        let mut cfg = Config::default();
        cfg.workspace_dir = dir.clone();
        cfg.default_provider = Some("mock".into());
        cfg.default_model = Some("mock-model".into());
        cfg.agent.system_prompt = Some("CURRENT_AGENT_INSTRUCTIONS".into());
        cfg.providers = vec![ProviderConfig {
            id: "mock".into(),
            name: "mock".into(),
            provider_type: "mock".into(),
            api_key_env: None,
            base_url: None,
            models: vec!["mock-model".into()],
            enabled: true,
        }];
        (cfg, dir)
    }

    fn sample_snapshot(session_id: &str, tokens: u64) -> ContextUsageSnapshot {
        ContextUsageSnapshot {
            session_id: Some(session_id.into()),
            run_id: Some("run-1".into()),
            request_revision: 4,
            provider: "mock".into(),
            model: "mock-model".into(),
            measurement_kind: MeasurementKind::CandidateEstimate,
            measurement_provenance: MeasurementProvenance::SafetyEstimate,
            measurement_exact: false,
            estimated_input_tokens: tokens,
            provider_actual_input_tokens: None,
            context_window_tokens: Some(640_000),
            max_input_tokens: Some(583_000),
            output_reserve_tokens: Some(16_384),
            safety_reserve_tokens: Some(32_768),
            pressure_threshold_tokens: Some(466_400),
            budget_source: Some("test".into()),
            usage_ratio: None,
            breakdown: Default::default(),
            measured_at: 1,
        }
    }

    async fn seed_history(cfg: &Config, session_id: &str, messages: Vec<ChatMessage>) {
        save_session_history(
            cfg,
            &ChannelKind::Web,
            session_id,
            messages,
            50,
            None,
            None,
            "omninova".into(),
            0,
            None,
        )
        .await
        .expect("save history");
    }

    #[tokio::test]
    async fn a_open_existing_session_returns_candidate_without_new_message() {
        let (cfg, dir) = mock_config();
        let runtime = GatewayRuntime::new(cfg.clone());
        seed_history(
            &cfg,
            "sess-a",
            vec![
                ChatMessage::system("old"),
                ChatMessage::user("hello"),
                ChatMessage::assistant("hi there"),
            ],
        )
        .await;

        let response = runtime
            .project_session_context(
                &ChannelKind::Web,
                "sess-a",
                Some("mock".into()),
                Some("mock-model".into()),
            )
            .await;
        let current = response.current.expect("candidate");
        assert!(!response.unavailable);
        assert_eq!(current.measurement_kind, MeasurementKind::CandidateEstimate);
        assert!(current.estimated_input_tokens > 0);
        assert_eq!(current.run_id, None);
        let history = load_session_history(&cfg, &ChannelKind::Web, "sess-a")
            .await
            .expect("history");
        assert_eq!(
            history
                .messages
                .iter()
                .filter(|m| m.role == "user")
                .count(),
            1
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn c_persisted_snapshot_survives_simulated_restart() {
        let (cfg, dir) = mock_config();
        let snapshot = sample_snapshot("sess-c", 11_300);
        let persisted = PersistedContextProjection {
            snapshot: snapshot.clone(),
            last_actual: Some(PersistedContextActual {
                input_tokens: 4_777,
                request_revision: 4,
                run_id: Some("run-1".into()),
                provider: "mock".into(),
                model: "mock-model".into(),
                measured_at: 1,
            }),
        };
        persist_context_projection(&cfg, &ChannelKind::Web, "sess-c", persisted)
            .await
            .expect("persist");

        drop(cfg.clone());
        let runtime_b = GatewayRuntime::new(cfg.clone());
        let history = runtime_b
            .get_session_history(&ChannelKind::Web, "sess-c")
            .await;
        let restored = history.context_projection.expect("restored");
        assert_eq!(restored.snapshot.estimated_input_tokens, 11_300);
        assert_eq!(restored.last_actual.unwrap().input_tokens, 4_777);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn e_session_a_and_b_snapshots_never_cross_contaminate() {
        let (cfg, dir) = mock_config();
        persist_context_projection(
            &cfg,
            &ChannelKind::Web,
            "sess-a",
            PersistedContextProjection {
                snapshot: sample_snapshot("sess-a", 10),
                last_actual: None,
            },
        )
        .await
        .unwrap();
        persist_context_projection(
            &cfg,
            &ChannelKind::Web,
            "sess-b",
            PersistedContextProjection {
                snapshot: sample_snapshot("sess-b", 20),
                last_actual: None,
            },
        )
        .await
        .unwrap();
        let runtime = GatewayRuntime::new(cfg.clone());
        let a = runtime
            .get_session_history(&ChannelKind::Web, "sess-a")
            .await
            .context_projection
            .unwrap();
        let b = runtime
            .get_session_history(&ChannelKind::Web, "sess-b")
            .await
            .context_projection
            .unwrap();
        assert_eq!(a.snapshot.estimated_input_tokens, 10);
        assert_eq!(b.snapshot.estimated_input_tokens, 20);
        assert_eq!(a.snapshot.session_id.as_deref(), Some("sess-a"));
        assert_eq!(b.snapshot.session_id.as_deref(), Some("sess-b"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn f_incompatible_snapshot_is_not_restored_as_current() {
        let (cfg, dir) = mock_config();
        let mut snapshot = sample_snapshot("sess-f", 99);
        snapshot.provider = "deepseek".into();
        snapshot.model = "deepseek-chat".into();
        persist_context_projection(
            &cfg,
            &ChannelKind::Web,
            "sess-f",
            PersistedContextProjection {
                snapshot,
                last_actual: None,
            },
        )
        .await
        .unwrap();
        seed_history(
            &cfg,
            "sess-f",
            vec![ChatMessage::user("hello")],
        )
        .await;
        let runtime = GatewayRuntime::new(cfg.clone());
        let response = runtime
            .project_session_context(
                &ChannelKind::Web,
                "sess-f",
                Some("mock".into()),
                Some("mock-model".into()),
            )
            .await;
        assert!(response.restored.is_none());
        let current = response.current.expect("recalculated");
        assert_eq!(current.provider, "mock");
        assert_eq!(current.model, "mock-model");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn n_session_open_does_not_mutate_or_compact_history() {
        let (mut cfg, dir) = mock_config();
        cfg.agent.compact_context = true;
        cfg.agent.max_history_messages = 2;
        let messages: Vec<ChatMessage> = (0..8)
            .map(|i| {
                if i % 2 == 0 {
                    ChatMessage::user(format!("user-{i}"))
                } else {
                    ChatMessage::assistant(format!("asst-{i}"))
                }
            })
            .collect();
        seed_history(&cfg, "sess-n", messages.clone()).await;
        let runtime = GatewayRuntime::new(cfg.clone());
        let _ = runtime
            .project_session_context(
                &ChannelKind::Web,
                "sess-n",
                Some("mock".into()),
                Some("mock-model".into()),
            )
            .await;
        let loaded = load_session_history(&cfg, &ChannelKind::Web, "sess-n")
            .await
            .unwrap();
        assert_eq!(loaded.messages.len(), messages.len());
        assert_eq!(loaded.messages[0].content, "user-0");
        assert_eq!(loaded.messages[7].content, "asst-7");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn o_reconstruction_failure_sets_unavailable() {
        let response = GatewayContextProjectionResponse::failed(None, "boom");
        assert!(response.unavailable);
        assert!(response.current.is_none());
        assert_eq!(response.error.as_deref(), Some("boom"));
    }

    #[test]
    fn persist_payload_contains_metadata_only() {
        let snapshot = sample_snapshot("sess-safe", 12);
        let encoded = serde_json::to_string(&snapshot).unwrap();
        assert!(!encoded.contains("hello"));
        assert!(!encoded.contains("system prompt"));
        assert!(encoded.contains("estimated_input_tokens"));
    }
}
