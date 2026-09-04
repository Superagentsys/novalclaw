//! OmniNova `BrowserBackend` trait — domain operations, not a CLI wrapper.
//!
//! Implementations (B3.1C+) encode vendor protocol. This module must not
//! mention agent-browser argv, sidecar files, daemon pids, or vendor JSON.

use crate::tools::browser_types::{
    BackendAvailability, BackendCapabilities, BackendSessionHandle, BrowserAction,
    BrowserActionResult, BrowserBackendError, BrowserBackendId, BrowserErrorKind, BrowserHealth,
    BrowserObservation, BrowserSessionOpenRequest, BrowserTab, NavigateRequest, ObserveRequest,
    ScreenshotRequest, ScreenshotResult,
};
use async_trait::async_trait;

/// Planned config.browser.backend mapping.
///
/// missing / empty → agent-browser
/// legacy `"playwright"` → agent-browser
/// `"agent-browser"` → agent-browser
/// `"personal-chrome"` → personal-chrome (future)
/// unknown → Rejected (`BrowserBackendUnsupported` at B3.1C)
pub fn planned_backend_id_from_config(
    value: Option<&str>,
) -> Result<BrowserBackendId, BrowserBackendError> {
    let trimmed = value.map(str::trim).filter(|s| !s.is_empty());
    match trimmed {
        None | Some("playwright") | Some("agent-browser") => Ok(BrowserBackendId::agent_browser()),
        Some("personal-chrome") => Ok(BrowserBackendId::personal_chrome()),
        Some(other) => Err(BrowserBackendError::new(
            BrowserErrorKind::Rejected,
            BrowserBackendId::new(other),
            format!("BrowserBackendUnsupported: {other}"),
        )),
    }
}

#[async_trait]
pub trait BrowserBackend: Send + Sync {
    fn id(&self) -> BrowserBackendId;

    fn capabilities(&self) -> BackendCapabilities;

    /// Installed/configured? Not a live-session probe.
    fn availability(&self) -> BackendAvailability;

    /// Open or attach using OmniNova's logical [`BrowserSessionKey`].
    /// Same key after Runtime rebuild must be able to reuse the backend session.
    async fn open_session(
        &self,
        req: &BrowserSessionOpenRequest,
    ) -> Result<BackendSessionHandle, BrowserBackendError>;

    async fn session_health(&self, session: &BackendSessionHandle) -> BrowserHealth;

    async fn close_session(
        &self,
        session: &BackendSessionHandle,
    ) -> Result<(), BrowserBackendError>;

    async fn open(
        &self,
        session: &BackendSessionHandle,
        req: &NavigateRequest,
    ) -> Result<BrowserActionResult, BrowserBackendError>;

    async fn observe(
        &self,
        session: &BackendSessionHandle,
        req: &ObserveRequest,
    ) -> Result<BrowserObservation, BrowserBackendError>;

    async fn act(
        &self,
        session: &BackendSessionHandle,
        action: &BrowserAction,
    ) -> Result<BrowserActionResult, BrowserBackendError>;

    async fn screenshot(
        &self,
        session: &BackendSessionHandle,
        req: &ScreenshotRequest,
    ) -> Result<ScreenshotResult, BrowserBackendError>;

    async fn tabs(
        &self,
        session: &BackendSessionHandle,
    ) -> Result<Vec<BrowserTab>, BrowserBackendError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::browser_types::{
        route_v1_tool_action, v1_error_kind, BrowserAction, BrowserBackendId, BrowserErrorKind,
        BrowserHealth, BrowserObservation, BrowserObserveKind, BrowserSessionKey,
        BrowserSessionOpenRequest, BrowserSessionOptions, BrowserSnapshot, BrowserTarget,
        NavigateRequest, ObserveRequest, ScreenshotRequest, V1ToolActionRoute, V1_TOOL_ACTIONS,
    };
    use crate::tools::traits::Tool;
    use crate::tools::BrowserTool;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeBackend {
        available: bool,
        last_open_key: Mutex<Option<BrowserSessionKey>>,
    }

    impl FakeBackend {
        fn up() -> Self {
            Self {
                available: true,
                ..Self::default()
            }
        }

        fn open_req(key: &str) -> BrowserSessionOpenRequest {
            BrowserSessionOpenRequest::new(
                BrowserSessionKey::new(key).unwrap(),
                BrowserSessionOptions::default(),
            )
        }

        fn issued_token(key: &BrowserSessionKey) -> String {
            format!("opaque:{}", key.as_str())
        }
    }

    #[async_trait]
    impl BrowserBackend for FakeBackend {
        fn id(&self) -> BrowserBackendId {
            BrowserBackendId::agent_browser()
        }

        fn capabilities(&self) -> BackendCapabilities {
            BackendCapabilities {
                navigation: true,
                observation: true,
                element_actions: true,
                tabs: true,
                screenshot: true,
                eval: true,
                attach: true,
                profiles: false,
            }
        }

        fn availability(&self) -> BackendAvailability {
            if self.available {
                BackendAvailability::Available
            } else {
                BackendAvailability::Unavailable {
                    kind: BrowserErrorKind::BinaryMissing,
                    detail: "fake missing".into(),
                }
            }
        }

        async fn open_session(
            &self,
            req: &BrowserSessionOpenRequest,
        ) -> Result<BackendSessionHandle, BrowserBackendError> {
            assert!(req.options.profile.is_none());
            *self.last_open_key.lock().expect("last_open_key") = Some(req.key.clone());
            if !self.available {
                return Err(BrowserBackendError::new(
                    BrowserErrorKind::BinaryMissing,
                    self.id(),
                    "unavailable",
                ));
            }
            BackendSessionHandle::new(self.id(), Self::issued_token(&req.key)).map_err(|err| {
                BrowserBackendError::new(BrowserErrorKind::Rejected, self.id(), err.to_string())
            })
        }

        async fn session_health(&self, session: &BackendSessionHandle) -> BrowserHealth {
            if session.token().starts_with("opaque:") {
                BrowserHealth::Healthy
            } else {
                BrowserHealth::Unhealthy {
                    kind: BrowserErrorKind::SessionNotFound,
                    detail: "unknown session".into(),
                }
            }
        }

        async fn close_session(
            &self,
            _session: &BackendSessionHandle,
        ) -> Result<(), BrowserBackendError> {
            Ok(())
        }

        async fn open(
            &self,
            _session: &BackendSessionHandle,
            req: &NavigateRequest,
        ) -> Result<BrowserActionResult, BrowserBackendError> {
            Ok(BrowserActionResult {
                detail: "opened".into(),
                url: Some(req.url.clone()),
                title: Some("Example".into()),
            })
        }

        async fn observe(
            &self,
            _session: &BackendSessionHandle,
            req: &ObserveRequest,
        ) -> Result<BrowserObservation, BrowserBackendError> {
            let text = match &req.kind {
                BrowserObserveKind::Url => Some("https://example.com/".into()),
                BrowserObserveKind::Title => Some("Example".into()),
                _ => Some("hello".into()),
            };
            Ok(BrowserObservation {
                url: Some("https://example.com/".into()),
                title: Some("Example".into()),
                text,
                snapshot: Some(BrowserSnapshot {
                    text: "hello".into(),
                    elements: Vec::new(),
                }),
            })
        }

        async fn act(
            &self,
            _session: &BackendSessionHandle,
            action: &BrowserAction,
        ) -> Result<BrowserActionResult, BrowserBackendError> {
            Ok(BrowserActionResult {
                detail: action.name().into(),
                url: None,
                title: None,
            })
        }

        async fn screenshot(
            &self,
            _session: &BackendSessionHandle,
            _req: &ScreenshotRequest,
        ) -> Result<ScreenshotResult, BrowserBackendError> {
            Ok(ScreenshotResult {
                locator: "/tmp/shot.png".into(),
            })
        }

        async fn tabs(
            &self,
            _session: &BackendSessionHandle,
        ) -> Result<Vec<BrowserTab>, BrowserBackendError> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn session_key_is_not_backend_handle() {
        let key = BrowserSessionKey::new("omninova-chat").unwrap();
        let handle =
            BackendSessionHandle::new(BrowserBackendId::agent_browser(), "omninova-a82b").unwrap();
        assert_ne!(key.as_str(), handle.token());
        let _ = handle.backend();
    }

    #[test]
    fn backend_session_handle_is_opaque() {
        let handle =
            BackendSessionHandle::new(BrowserBackendId::personal_chrome(), "claim/abc:xyz")
                .unwrap();
        assert_eq!(handle.token(), "claim/abc:xyz");
        assert!(!handle.token().contains("--session"));
        assert!(BackendSessionHandle::new(BrowserBackendId::agent_browser(), "").is_err());
    }

    #[test]
    fn availability_is_not_session_health() {
        let down = FakeBackend::default();
        assert!(matches!(
            down.availability(),
            BackendAvailability::Unavailable {
                kind: BrowserErrorKind::BinaryMissing,
                ..
            }
        ));
        let up = FakeBackend::up();
        assert!(matches!(up.availability(), BackendAvailability::Available));
    }

    #[tokio::test]
    async fn session_health_uses_handle_not_availability() {
        let backend = FakeBackend::up();
        let session = backend
            .open_session(&FakeBackend::open_req("chat-1"))
            .await
            .unwrap();
        assert!(matches!(
            backend.session_health(&session).await,
            BrowserHealth::Healthy
        ));
        let other = BackendSessionHandle::new(backend.id(), "other").unwrap();
        assert!(matches!(
            backend.session_health(&other).await,
            BrowserHealth::Unhealthy {
                kind: BrowserErrorKind::SessionNotFound,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn open_session_is_deterministic_for_the_same_logical_key() {
        let backend = FakeBackend::up();
        let req = FakeBackend::open_req("chat-rebuild");
        let first = backend.open_session(&req).await.unwrap();
        let second = backend.open_session(&req).await.unwrap();
        assert_eq!(first, second);
        assert_eq!(
            backend.last_open_key.lock().unwrap().as_ref(),
            Some(&req.key)
        );
        let other = backend
            .open_session(&FakeBackend::open_req("other-chat"))
            .await
            .unwrap();
        assert_ne!(first, other);
    }

    #[tokio::test]
    async fn trait_is_object_safe_and_arc_dyn() {
        let backend: Arc<dyn BrowserBackend> = Arc::new(FakeBackend::up());
        let req = FakeBackend::open_req("arc-session");
        assert!(req.options.profile.is_none());
        let session = backend.open_session(&req).await.unwrap();
        let _ = backend
            .open(
                &session,
                &NavigateRequest {
                    url: "https://example.com/".into(),
                },
            )
            .await
            .unwrap();
        let obs = backend
            .observe(&session, &ObserveRequest::snapshot())
            .await
            .unwrap();
        assert!(obs.snapshot.unwrap().elements.is_empty());
        let clicked = backend
            .act(
                &session,
                &BrowserAction::Click {
                    target: BrowserTarget::Css("a".into()),
                },
            )
            .await
            .unwrap();
        assert_eq!(clicked.detail, "click");
        backend.close_session(&session).await.unwrap();
        assert!(backend.capabilities().eval);
        assert!(backend
            .capabilities()
            .supports_action(&BrowserAction::Eval { script: "1".into() }));
        assert!(backend
            .capabilities()
            .supports_observation(&BrowserObserveKind::Url));
        assert!(backend
            .capabilities()
            .supports_observation(&BrowserObserveKind::Text { target: None }));
    }

    #[test]
    fn backend_error_uses_typed_backend_id() {
        let err = BrowserBackendError::new(
            BrowserErrorKind::Rejected,
            BrowserBackendId::personal_chrome(),
            "no",
        );
        assert_eq!(err.backend, BrowserBackendId::personal_chrome());
        assert_eq!(err.backend.as_str(), BrowserBackendId::PERSONAL_CHROME);
        let unknown = BrowserBackendId::new("future-plugin");
        let err = BrowserBackendError::new(BrowserErrorKind::CommandFailed, unknown.clone(), "x");
        assert_eq!(err.backend, unknown);
    }

    #[test]
    fn planned_legacy_backend_config_is_agent_browser() {
        assert_eq!(
            planned_backend_id_from_config(None).unwrap(),
            BrowserBackendId::agent_browser()
        );
        assert_eq!(
            planned_backend_id_from_config(Some("")).unwrap(),
            BrowserBackendId::agent_browser()
        );
        assert_eq!(
            planned_backend_id_from_config(Some("playwright")).unwrap(),
            BrowserBackendId::agent_browser()
        );
        assert_eq!(
            planned_backend_id_from_config(Some("agent-browser")).unwrap(),
            BrowserBackendId::agent_browser()
        );
        let err = planned_backend_id_from_config(Some("unknown-backend")).unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::Rejected);
        assert!(err.detail.contains("BrowserBackendUnsupported"));
    }

    #[test]
    fn v1_tool_schema_actions_unchanged() {
        let tool = BrowserTool::new(Vec::new(), true, false, None);
        let schema = tool.parameters_schema();
        let actions = schema["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum")
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            actions,
            V1_TOOL_ACTIONS
                .iter()
                .map(|s| (*s).to_string())
                .collect::<Vec<_>>()
        );
        assert!(actions.contains(&"close".to_string()));
        assert!(actions.contains(&"get_text".to_string()));
        assert!(actions.contains(&"find".to_string()));
        assert_eq!(
            route_v1_tool_action("close"),
            Some(V1ToolActionRoute::CloseSession)
        );
        assert_eq!(
            route_v1_tool_action("get_text"),
            Some(V1ToolActionRoute::Observe)
        );
        assert_eq!(
            route_v1_tool_action("find"),
            Some(V1ToolActionRoute::Observe)
        );
        assert_eq!(route_v1_tool_action("click"), Some(V1ToolActionRoute::Act));
    }

    #[test]
    fn v1_error_mapping_still_lists_required_prefixes() {
        for prefix in [
            "BrowserBinaryMissing",
            "BrowserBinaryNotExecutable",
            "BrowserLaunchFailed",
            "BrowserDaemonUnavailable",
            "BrowserSessionUnavailable",
            "BrowserCrashed",
            "BrowserCommandTimeout",
            "BrowserUrlRejected",
            "BrowserSessionInvalid",
            "BrowserCommandFailed",
        ] {
            assert!(
                v1_error_kind(&format!("{prefix}: detail")).is_some(),
                "{prefix}"
            );
        }
    }

    #[test]
    fn v1_tool_still_executes_without_backend_trait() {
        let tool = BrowserTool::new(Vec::new(), true, false, None);
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                tool.execute(json!({
                    "action": "open",
                    "url": "https://example.com"
                }))
                .await
            })
            .unwrap();
        assert!(
            !result.success,
            "missing session must fail before binary resolve"
        );
        let error = result.error.expect("error");
        assert!(error.starts_with("BrowserSessionMissing:"), "error={error}");
        assert!(!error.to_lowercase().contains("browserbackend"));
    }
}
