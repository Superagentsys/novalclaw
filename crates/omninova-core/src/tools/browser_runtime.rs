//! Backend-neutral Browser Runtime orchestration.
//!
//! Production path: `BrowserTool` → `BrowserRuntime` → `BrowserBackend`.

use crate::tools::browser_backend::BrowserBackend;
use crate::tools::browser_output::{
    BROWSER_OP_CHAR_LIMIT, BROWSER_SNAPSHOT_CHAR_LIMIT, BROWSER_TEXT_CHAR_LIMIT,
};
use crate::tools::browser_types::{
    BackendAvailability, BackendSessionHandle, BrowserAction, BrowserActionResult,
    BrowserBackendError, BrowserBackendId, BrowserErrorKind, BrowserHealth, BrowserObservation,
    BrowserObserveKind, BrowserSessionKey, BrowserSessionOpenRequest, BrowserSessionOptions,
    BrowserSnapshot, BrowserTab, NavigateRequest, ObserveRequest, ScreenshotRequest,
    ScreenshotResult,
};
use crate::tools::text_bound::bound_head;
use crate::tools::web_client::{host_matches_allowlist, redact_secrets_in_text};
use std::sync::Arc;
use url::Url;

/// Backend-independent runtime limits and allowlist. Vendor launch flags are
/// not stored here; they travel on [`BrowserSessionOptions`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserRuntimePolicy {
    pub allowed_domains: Vec<String>,
    pub snapshot_char_limit: usize,
    pub text_char_limit: usize,
    pub operation_char_limit: usize,
    /// V1 recover-once: at most one automatic recovery per operation.
    pub max_recovery_attempts: u8,
}

impl Default for BrowserRuntimePolicy {
    fn default() -> Self {
        Self {
            allowed_domains: Vec::new(),
            snapshot_char_limit: BROWSER_SNAPSHOT_CHAR_LIMIT,
            text_char_limit: BROWSER_TEXT_CHAR_LIMIT,
            operation_char_limit: BROWSER_OP_CHAR_LIMIT,
            max_recovery_attempts: 1,
        }
    }
}

/// Retry class matching V1 `BrowserActionKind` semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserRetryClass {
    SafeRead,
    Idempotent,
    Mutating,
}

#[derive(Clone, Debug)]
enum RuntimeOp {
    Open,
    Observe(BrowserObserveKind),
    Act(BrowserAction),
    Screenshot,
    Tabs,
    CloseSession,
}

/// Orchestrates session ensure, security, capabilities, retry, and budgets.
pub struct BrowserRuntime {
    backend: Arc<dyn BrowserBackend>,
    policy: BrowserRuntimePolicy,
}

impl BrowserRuntime {
    pub fn new(backend: Arc<dyn BrowserBackend>, policy: BrowserRuntimePolicy) -> Self {
        Self { backend, policy }
    }

    pub fn backend_id(&self) -> BrowserBackendId {
        self.backend.id()
    }

    pub fn policy(&self) -> &BrowserRuntimePolicy {
        &self.policy
    }

    pub fn availability(&self) -> BackendAvailability {
        self.backend.availability()
    }

    pub async fn session_health(
        &self,
        key: &BrowserSessionKey,
        opts: &BrowserSessionOptions,
    ) -> BrowserHealth {
        match self.ensure_session(key, opts).await {
            Ok(handle) => self.backend.session_health(&handle).await,
            Err(err) => BrowserHealth::Unhealthy {
                kind: err.kind,
                detail: err.detail,
            },
        }
    }

    pub async fn open(
        &self,
        key: &BrowserSessionKey,
        opts: &BrowserSessionOptions,
        req: &NavigateRequest,
    ) -> Result<BrowserActionResult, BrowserBackendError> {
        if !self.backend.capabilities().navigation {
            return Err(self.capability_error("navigation"));
        }
        let parsed = self.validate_open_url(&req.url)?;
        let normalized = NavigateRequest {
            url: parsed.to_string(),
        };
        self.with_recovery(RuntimeOp::Open, key, opts, |handle| {
            let backend = Arc::clone(&self.backend);
            let req = normalized.clone();
            async move { backend.open(&handle, &req).await }
        })
        .await
        .map(|result| self.budget_action_result(result))
    }

    pub async fn observe(
        &self,
        key: &BrowserSessionKey,
        opts: &BrowserSessionOptions,
        req: &ObserveRequest,
    ) -> Result<BrowserObservation, BrowserBackendError> {
        if !self.backend.capabilities().supports_observation(&req.kind) {
            return Err(self.capability_error("observation"));
        }
        let kind = req.kind.clone();
        let req = req.clone();
        self.with_recovery(RuntimeOp::Observe(kind.clone()), key, opts, |handle| {
            let backend = Arc::clone(&self.backend);
            let req = req.clone();
            async move { backend.observe(&handle, &req).await }
        })
        .await
        .map(|obs| self.budget_observation(obs, &kind))
    }

    pub async fn act(
        &self,
        key: &BrowserSessionKey,
        opts: &BrowserSessionOptions,
        action: &BrowserAction,
    ) -> Result<BrowserActionResult, BrowserBackendError> {
        if !self.backend.capabilities().supports_action(action) {
            return Err(self.capability_error("element_actions"));
        }
        let action = action.clone();
        self.with_recovery(RuntimeOp::Act(action.clone()), key, opts, |handle| {
            let backend = Arc::clone(&self.backend);
            let action = action.clone();
            async move { backend.act(&handle, &action).await }
        })
        .await
        .map(|result| self.budget_action_result(result))
    }

    pub async fn screenshot(
        &self,
        key: &BrowserSessionKey,
        opts: &BrowserSessionOptions,
        req: &ScreenshotRequest,
    ) -> Result<ScreenshotResult, BrowserBackendError> {
        if !self.backend.capabilities().screenshot {
            return Err(self.capability_error("screenshot"));
        }
        let req = req.clone();
        self.with_recovery(RuntimeOp::Screenshot, key, opts, |handle| {
            let backend = Arc::clone(&self.backend);
            let req = req.clone();
            async move { backend.screenshot(&handle, &req).await }
        })
        .await
        .map(|result| ScreenshotResult {
            locator: redact_secrets_in_text(&result.locator),
        })
    }

    pub async fn tabs(
        &self,
        key: &BrowserSessionKey,
        opts: &BrowserSessionOptions,
    ) -> Result<Vec<BrowserTab>, BrowserBackendError> {
        if !self.backend.capabilities().tabs {
            return Err(self.capability_error("tabs"));
        }
        self.with_recovery(RuntimeOp::Tabs, key, opts, |handle| {
            let backend = Arc::clone(&self.backend);
            async move { backend.tabs(&handle).await }
        })
        .await
        .map(|tabs| {
            tabs.into_iter()
                .map(|tab| BrowserTab {
                    id: tab.id,
                    url: tab.url.map(|u| redact_secrets_in_text(&u)),
                    title: tab.title.map(|t| redact_secrets_in_text(&t)),
                    active: tab.active,
                })
                .collect()
        })
    }

    /// Close/detach the backend session for `key`. Does not assume a process kill.
    pub async fn close_session(
        &self,
        key: &BrowserSessionKey,
        opts: &BrowserSessionOptions,
    ) -> Result<(), BrowserBackendError> {
        self.with_recovery(RuntimeOp::CloseSession, key, opts, |handle| {
            let backend = Arc::clone(&self.backend);
            async move { backend.close_session(&handle).await }
        })
        .await
    }

    async fn ensure_session(
        &self,
        key: &BrowserSessionKey,
        opts: &BrowserSessionOptions,
    ) -> Result<BackendSessionHandle, BrowserBackendError> {
        self.reject_reserved_logical_session(key)?;
        if let BackendAvailability::Unavailable { kind, detail } = self.backend.availability() {
            return Err(BrowserBackendError::new(kind, self.backend.id(), detail));
        }
        let req = BrowserSessionOpenRequest::new(key.clone(), opts.clone());
        self.backend.open_session(&req).await
    }

    fn reject_reserved_logical_session(
        &self,
        key: &BrowserSessionKey,
    ) -> Result<(), BrowserBackendError> {
        let Some((kind, detail)) = key.omninova_policy_error() else {
            return Ok(());
        };
        let mut err = BrowserBackendError::new(kind, self.backend.id(), detail.to_string());
        err.retryable = false;
        Err(err)
    }

    async fn with_recovery<T, F, Fut>(
        &self,
        op: RuntimeOp,
        key: &BrowserSessionKey,
        opts: &BrowserSessionOptions,
        mut call: F,
    ) -> Result<T, BrowserBackendError>
    where
        F: FnMut(BackendSessionHandle) -> Fut,
        Fut: std::future::Future<Output = Result<T, BrowserBackendError>>,
    {
        let mut recovered = 0u8;
        loop {
            let handle = match self.ensure_session(key, opts).await {
                Ok(handle) => handle,
                Err(err) if self.can_recover(&op, recovered, &err) => {
                    recovered = recovered.saturating_add(1);
                    continue;
                }
                Err(err) => return Err(err),
            };
            match call(handle).await {
                Ok(value) => return Ok(value),
                Err(err) if self.can_recover(&op, recovered, &err) => {
                    recovered = recovered.saturating_add(1);
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
    }

    fn can_recover(&self, op: &RuntimeOp, recovered: u8, err: &BrowserBackendError) -> bool {
        err.retryable
            && recovered < self.policy.max_recovery_attempts
            && operation_is_retryable(op)
            && runtime_should_recover(err.kind)
    }

    fn validate_open_url(&self, raw: &str) -> Result<Url, BrowserBackendError> {
        let parsed = parse_browser_open_url(raw).map_err(|detail| {
            BrowserBackendError::new(BrowserErrorKind::Rejected, self.backend.id(), detail)
        })?;
        if !browser_host_allowed(&parsed, &self.policy.allowed_domains) {
            return Err(BrowserBackendError::new(
                BrowserErrorKind::Rejected,
                self.backend.id(),
                format!(
                    "BrowserUrlRejected: Domain not in allowed list: {}",
                    redact_secrets_in_text(parsed.as_str())
                ),
            ));
        }
        Ok(parsed)
    }

    fn capability_error(&self, capability: &str) -> BrowserBackendError {
        BrowserBackendError::new(
            BrowserErrorKind::Rejected,
            self.backend.id(),
            format!("BrowserCapabilityRejected: {capability} is not supported"),
        )
    }

    fn budget_action_result(&self, result: BrowserActionResult) -> BrowserActionResult {
        BrowserActionResult {
            detail: bound_head(
                &redact_secrets_in_text(&result.detail),
                self.policy.operation_char_limit,
            ),
            url: result.url.map(|u| redact_secrets_in_text(&u)),
            title: result.title.map(|t| redact_secrets_in_text(&t)),
        }
    }

    fn budget_observation(
        &self,
        obs: BrowserObservation,
        kind: &BrowserObserveKind,
    ) -> BrowserObservation {
        let url = obs.url.map(|u| redact_secrets_in_text(&u));
        let title = obs.title.map(|t| redact_secrets_in_text(&t));
        // Snapshot/text/html model envelopes are already bounded by the
        // backend's vendor normalizer (V1 `parse_action_outcome`). Re-applying
        // the inner content limit would clip headers and truncation markers.
        let text = match kind {
            BrowserObserveKind::Snapshot
            | BrowserObserveKind::Text { .. }
            | BrowserObserveKind::Html { .. } => obs.text.map(|t| redact_secrets_in_text(&t)),
            _ => obs.text.map(|t| {
                bound_head(
                    &redact_secrets_in_text(&t),
                    self.policy.operation_char_limit,
                )
            }),
        };
        let snapshot = obs.snapshot.map(|snap| BrowserSnapshot {
            text: bound_head(
                &redact_secrets_in_text(&snap.text),
                self.policy.snapshot_char_limit,
            ),
            elements: snap.elements,
        });
        BrowserObservation {
            url,
            title,
            text,
            snapshot,
        }
    }
}

/// Shared with V1 `BrowserTool`: http(s) only; bare hosts become https.
pub fn parse_browser_open_url(raw: &str) -> Result<Url, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("BrowserUrlRejected: URL is empty".into());
    }
    let parsed = match Url::parse(trimmed) {
        Ok(url) => url,
        Err(_) => Url::parse(&format!("https://{trimmed}"))
            .map_err(|e| format!("BrowserUrlRejected: invalid URL ({e})"))?,
    };
    match parsed.scheme() {
        "http" | "https" => {
            if parsed.host_str().is_none() {
                return Err("BrowserUrlRejected: URL has no host".into());
            }
            Ok(parsed)
        }
        other => Err(format!(
            "BrowserUrlRejected: scheme '{other}' is not allowed; only http/https"
        )),
    }
}

pub fn browser_host_allowed(url: &Url, allowed_domains: &[String]) -> bool {
    url.host_str()
        .is_some_and(|host| host_matches_allowlist(host, allowed_domains))
}

pub fn retry_class_for_action(action: &BrowserAction) -> BrowserRetryClass {
    match action {
        BrowserAction::Wait { .. }
        | BrowserAction::Reload
        | BrowserAction::Back
        | BrowserAction::Forward => BrowserRetryClass::Idempotent,
        _ => BrowserRetryClass::Mutating,
    }
}

pub fn retry_class_for_observe(_kind: &BrowserObserveKind) -> BrowserRetryClass {
    BrowserRetryClass::SafeRead
}

fn operation_is_retryable(op: &RuntimeOp) -> bool {
    match op {
        RuntimeOp::Open => true,
        RuntimeOp::Observe(BrowserObserveKind::Find { .. }) => false,
        RuntimeOp::Observe(_) => true,
        RuntimeOp::Act(BrowserAction::Wait { .. } | BrowserAction::Reload) => true,
        RuntimeOp::Act(_) => false,
        RuntimeOp::Screenshot | RuntimeOp::Tabs | RuntimeOp::CloseSession => false,
    }
}

fn runtime_should_recover(kind: BrowserErrorKind) -> bool {
    matches!(
        kind,
        BrowserErrorKind::NotConnected
            | BrowserErrorKind::SessionNotFound
            | BrowserErrorKind::Crashed
    )
}

/// Future model-facing prefix mapping. V1 production output is unchanged.
pub fn present_backend_error(err: &BrowserBackendError) -> String {
    if err.detail.starts_with("Browser") {
        return err.detail.clone();
    }
    let prefix = match err.kind {
        BrowserErrorKind::BinaryMissing => "BrowserBinaryMissing",
        BrowserErrorKind::LaunchFailed => "BrowserLaunchFailed",
        BrowserErrorKind::NotConnected => "BrowserDaemonUnavailable",
        BrowserErrorKind::SessionNotFound => "BrowserSessionUnavailable",
        BrowserErrorKind::Crashed => "BrowserCrashed",
        BrowserErrorKind::Timeout => "BrowserCommandTimeout",
        BrowserErrorKind::Rejected => "BrowserUrlRejected",
        BrowserErrorKind::CommandFailed => "BrowserCommandFailed",
    };
    format!("{prefix}: {}", err.detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::browser_lifecycle::is_retryable_action;
    use crate::tools::browser_types::{
        BackendCapabilities, BrowserBackendId, BrowserElement, BrowserElementRef, BrowserPageId,
        BrowserTarget, V1ToolActionRoute, V1_TOOL_ACTIONS,
    };
    use crate::tools::traits::Tool;
    use crate::tools::BrowserTool;
    use async_trait::async_trait;
    use serde_json::json;
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum CallKind {
        OpenSession,
        Open,
        Observe,
        Act,
        Screenshot,
        Tabs,
        Close,
        Health,
    }

    struct FakeState {
        available: bool,
        capabilities: BackendCapabilities,
        calls: Vec<CallKind>,
        open_keys: Vec<BrowserSessionKey>,
        observe_kinds: Vec<String>,
        act_names: Vec<String>,
        closed: Vec<String>,
        fail_on: Option<CallKind>,
        fail_kind: BrowserErrorKind,
        fail_remaining: u32,
        unhealthy_keys: HashSet<String>,
        observation: BrowserObservation,
        tabs: Vec<BrowserTab>,
        last_handle_tokens: HashMap<String, String>,
    }

    struct FakeBackend {
        id: BrowserBackendId,
        state: Mutex<FakeState>,
    }

    impl FakeBackend {
        fn new() -> Self {
            Self {
                id: BrowserBackendId::new("fake"),
                state: Mutex::new(FakeState {
                    available: true,
                    capabilities: BackendCapabilities {
                        navigation: true,
                        observation: true,
                        element_actions: true,
                        tabs: true,
                        screenshot: true,
                        eval: true,
                        attach: true,
                        profiles: false,
                    },
                    calls: Vec::new(),
                    open_keys: Vec::new(),
                    observe_kinds: Vec::new(),
                    act_names: Vec::new(),
                    closed: Vec::new(),
                    fail_on: None,
                    fail_kind: BrowserErrorKind::NotConnected,
                    fail_remaining: 0,
                    unhealthy_keys: HashSet::new(),
                    observation: BrowserObservation {
                        url: Some("https://example.com/".into()),
                        title: Some("Example".into()),
                        text: Some("hello".into()),
                        snapshot: Some(BrowserSnapshot {
                            text: "hello".into(),
                            elements: Vec::new(),
                        }),
                    },
                    tabs: Vec::new(),
                    last_handle_tokens: HashMap::new(),
                }),
            }
        }

        fn token_for(key: &BrowserSessionKey) -> String {
            format!("opaque:{}", key.as_str())
        }

        fn calls(&self) -> Vec<CallKind> {
            self.state.lock().expect("state").calls.clone()
        }

        fn call_count(&self, kind: CallKind) -> usize {
            self.calls().into_iter().filter(|c| *c == kind).count()
        }

        fn open_keys(&self) -> Vec<String> {
            self.state
                .lock()
                .expect("state")
                .open_keys
                .iter()
                .map(|k| k.as_str().to_string())
                .collect()
        }

        fn set_available(&self, available: bool) {
            self.state.lock().expect("state").available = available;
        }

        fn set_capabilities(&self, capabilities: BackendCapabilities) {
            self.state.lock().expect("state").capabilities = capabilities;
        }

        fn fail_next(&self, on: CallKind, kind: BrowserErrorKind, times: u32) {
            let mut state = self.state.lock().expect("state");
            state.fail_on = Some(on);
            state.fail_kind = kind;
            state.fail_remaining = times;
        }

        fn mark_unhealthy(&self, key: &str) {
            self.state
                .lock()
                .expect("state")
                .unhealthy_keys
                .insert(key.to_string());
        }

        fn set_observation(&self, observation: BrowserObservation) {
            self.state.lock().expect("state").observation = observation;
        }

        fn maybe_fail(&self, kind: CallKind) -> Result<(), BrowserBackendError> {
            let mut state = self.state.lock().expect("state");
            state.calls.push(kind);
            if state.fail_on == Some(kind) && state.fail_remaining > 0 {
                state.fail_remaining -= 1;
                let err = BrowserBackendError::new(state.fail_kind, self.id.clone(), "injected");
                return Err(err);
            }
            Ok(())
        }
    }

    #[async_trait]
    impl BrowserBackend for FakeBackend {
        fn id(&self) -> BrowserBackendId {
            self.id.clone()
        }

        fn capabilities(&self) -> BackendCapabilities {
            self.state.lock().expect("state").capabilities
        }

        fn availability(&self) -> BackendAvailability {
            if self.state.lock().expect("state").available {
                BackendAvailability::Available
            } else {
                BackendAvailability::Unavailable {
                    kind: BrowserErrorKind::BinaryMissing,
                    detail: "fake backend missing".into(),
                }
            }
        }

        async fn open_session(
            &self,
            req: &BrowserSessionOpenRequest,
        ) -> Result<BackendSessionHandle, BrowserBackendError> {
            self.maybe_fail(CallKind::OpenSession)?;
            let token = Self::token_for(&req.key);
            let mut state = self.state.lock().expect("state");
            state.open_keys.push(req.key.clone());
            state
                .last_handle_tokens
                .insert(req.key.as_str().to_string(), token.clone());
            BackendSessionHandle::new(self.id.clone(), token).map_err(|err| {
                BrowserBackendError::new(
                    BrowserErrorKind::Rejected,
                    self.id.clone(),
                    err.to_string(),
                )
            })
        }

        async fn session_health(&self, session: &BackendSessionHandle) -> BrowserHealth {
            let _ = self.maybe_fail(CallKind::Health);
            let state = self.state.lock().expect("state");
            let unhealthy = state.unhealthy_keys.iter().any(|key| {
                state
                    .last_handle_tokens
                    .get(key)
                    .map(|token| token == session.token())
                    .unwrap_or(false)
            });
            if unhealthy {
                BrowserHealth::Unhealthy {
                    kind: BrowserErrorKind::NotConnected,
                    detail: "session disconnected".into(),
                }
            } else {
                BrowserHealth::Healthy
            }
        }

        async fn close_session(
            &self,
            session: &BackendSessionHandle,
        ) -> Result<(), BrowserBackendError> {
            self.maybe_fail(CallKind::Close)?;
            self.state
                .lock()
                .expect("state")
                .closed
                .push(session.token().to_string());
            Ok(())
        }

        async fn open(
            &self,
            _session: &BackendSessionHandle,
            req: &NavigateRequest,
        ) -> Result<BrowserActionResult, BrowserBackendError> {
            self.maybe_fail(CallKind::Open)?;
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
            self.maybe_fail(CallKind::Observe)?;
            let mut state = self.state.lock().expect("state");
            state.observe_kinds.push(req.kind.v1_action_name().into());
            Ok(state.observation.clone())
        }

        async fn act(
            &self,
            _session: &BackendSessionHandle,
            action: &BrowserAction,
        ) -> Result<BrowserActionResult, BrowserBackendError> {
            self.maybe_fail(CallKind::Act)?;
            self.state
                .lock()
                .expect("state")
                .act_names
                .push(action.name().into());
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
            self.maybe_fail(CallKind::Screenshot)?;
            Ok(ScreenshotResult {
                locator: "/tmp/shot.png".into(),
            })
        }

        async fn tabs(
            &self,
            _session: &BackendSessionHandle,
        ) -> Result<Vec<BrowserTab>, BrowserBackendError> {
            self.maybe_fail(CallKind::Tabs)?;
            Ok(self.state.lock().expect("state").tabs.clone())
        }
    }

    fn runtime(backend: Arc<FakeBackend>) -> BrowserRuntime {
        BrowserRuntime::new(backend, BrowserRuntimePolicy::default())
    }

    fn key(id: &str) -> BrowserSessionKey {
        BrowserSessionKey::new(id).unwrap()
    }

    fn opts() -> BrowserSessionOptions {
        BrowserSessionOptions::default()
    }

    fn css() -> BrowserTarget {
        BrowserTarget::Css("#a".into())
    }

    #[tokio::test]
    async fn same_key_reuses_backend_session_handle() {
        let backend = Arc::new(FakeBackend::new());
        let rt = runtime(backend.clone());
        let k = key("chat-1");
        rt.open(
            &k,
            &opts(),
            &NavigateRequest {
                url: "https://example.com/".into(),
            },
        )
        .await
        .unwrap();
        rt.observe(&k, &opts(), &ObserveRequest::snapshot())
            .await
            .unwrap();
        let keys = backend.open_keys();
        assert!(keys.iter().all(|s| s == "chat-1"));
        let tokens: HashSet<_> = backend
            .state
            .lock()
            .unwrap()
            .last_handle_tokens
            .values()
            .cloned()
            .collect();
        assert_eq!(tokens.len(), 1);
    }

    #[tokio::test]
    async fn reserved_default_logical_session_is_rejected_before_backend() {
        let backend = Arc::new(FakeBackend::new());
        let rt = runtime(backend.clone());
        let err = rt
            .open(
                &key("default"),
                &opts(),
                &NavigateRequest {
                    url: "https://example.com/".into(),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::Rejected);
        assert!(!err.retryable);
        assert!(err.detail.starts_with("BrowserSessionInvalid:"));
        assert_eq!(backend.call_count(CallKind::OpenSession), 0);
        assert_eq!(backend.call_count(CallKind::Open), 0);

        let err = rt
            .open(
                &key("Default"),
                &opts(),
                &NavigateRequest {
                    url: "https://example.com/".into(),
                },
            )
            .await
            .unwrap_err();
        assert!(err.detail.starts_with("BrowserSessionInvalid:"));
        assert_eq!(backend.call_count(CallKind::OpenSession), 0);
    }

    #[tokio::test]
    async fn rebuilt_runtime_passes_same_logical_key() {
        let backend = Arc::new(FakeBackend::new());
        let k = key("rebuild-chat");
        let req = NavigateRequest {
            url: "https://example.com/".into(),
        };
        runtime(backend.clone())
            .open(&k, &opts(), &req)
            .await
            .unwrap();
        runtime(backend.clone())
            .open(&k, &opts(), &req)
            .await
            .unwrap();
        assert_eq!(backend.open_keys(), vec!["rebuild-chat", "rebuild-chat"]);
    }

    #[tokio::test]
    async fn runtime_does_not_require_vendor_token_shape() {
        let backend = Arc::new(FakeBackend::new());
        let rt = runtime(backend);
        let k = key("chat:slot/会话 1");
        rt.observe(&k, &opts(), &ObserveRequest::snapshot())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn unsafe_scheme_rejected_before_backend_open() {
        let backend = Arc::new(FakeBackend::new());
        let rt = runtime(backend.clone());
        let err = rt
            .open(
                &key("s"),
                &opts(),
                &NavigateRequest {
                    url: "file:///etc/passwd".into(),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::Rejected);
        assert!(present_backend_error(&err).starts_with("BrowserUrlRejected:"));
        assert_eq!(backend.call_count(CallKind::Open), 0);
        assert_eq!(backend.call_count(CallKind::OpenSession), 0);
    }

    #[tokio::test]
    async fn domain_blocked_before_backend_open() {
        let backend = Arc::new(FakeBackend::new());
        let mut policy = BrowserRuntimePolicy::default();
        policy.allowed_domains = vec!["allowed.example".into()];
        let rt = BrowserRuntime::new(backend.clone(), policy);
        let err = rt
            .open(
                &key("s"),
                &opts(),
                &NavigateRequest {
                    url: "https://evil.example/".into(),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::Rejected);
        assert!(err.detail.contains("allowed"));
        assert_eq!(backend.call_count(CallKind::Open), 0);
        assert_eq!(backend.call_count(CallKind::OpenSession), 0);
    }

    #[tokio::test]
    async fn credentials_are_redacted() {
        let backend = Arc::new(FakeBackend::new());
        backend.set_observation(BrowserObservation {
            url: Some("https://user:secret@example.com/path".into()),
            title: Some("t".into()),
            text: Some("see https://user:secret@example.com/x".into()),
            snapshot: None,
        });
        let mut policy = BrowserRuntimePolicy::default();
        policy.allowed_domains = vec!["example.com".into()];
        let rt = BrowserRuntime::new(backend.clone(), policy);
        let blocked = rt
            .open(
                &key("s"),
                &opts(),
                &NavigateRequest {
                    url: "https://user:secret@blocked.example/".into(),
                },
            )
            .await
            .unwrap_err();
        assert!(!blocked.detail.contains("secret"));
        assert!(blocked.detail.contains("***"));
        let obs = rt
            .observe(&key("s"), &opts(), &ObserveRequest::snapshot())
            .await
            .unwrap();
        assert!(!obs.url.unwrap().contains("secret"));
        assert!(!obs.text.unwrap().contains("secret"));
    }

    #[tokio::test]
    async fn observe_uses_backend_observe_not_act() {
        let backend = Arc::new(FakeBackend::new());
        let rt = runtime(backend.clone());
        rt.observe(
            &key("s"),
            &opts(),
            &ObserveRequest {
                kind: BrowserObserveKind::Url,
                interactive_only: false,
                compact: false,
            },
        )
        .await
        .unwrap();
        assert!(backend.call_count(CallKind::Observe) >= 1);
        assert_eq!(backend.call_count(CallKind::Act), 0);
        assert_eq!(
            backend.state.lock().unwrap().observe_kinds,
            vec!["get_url".to_string()]
        );
    }

    #[tokio::test]
    async fn act_uses_backend_act_not_observe() {
        let backend = Arc::new(FakeBackend::new());
        let rt = runtime(backend.clone());
        rt.act(&key("s"), &opts(), &BrowserAction::Click { target: css() })
            .await
            .unwrap();
        assert!(backend.call_count(CallKind::Act) >= 1);
        assert_eq!(backend.call_count(CallKind::Observe), 0);
        assert_eq!(backend.state.lock().unwrap().act_names, vec!["click"]);
    }

    #[tokio::test]
    async fn mutating_action_never_blind_retries() {
        let backend = Arc::new(FakeBackend::new());
        backend.fail_next(CallKind::Act, BrowserErrorKind::NotConnected, 3);
        let rt = runtime(backend.clone());
        let err = rt
            .act(&key("s"), &opts(), &BrowserAction::Click { target: css() })
            .await
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::NotConnected);
        assert_eq!(backend.call_count(CallKind::Act), 1);
    }

    #[tokio::test]
    async fn safe_operation_retries_at_most_once() {
        let backend = Arc::new(FakeBackend::new());
        backend.fail_next(CallKind::Observe, BrowserErrorKind::NotConnected, 1);
        let rt = runtime(backend.clone());
        rt.observe(
            &key("s"),
            &opts(),
            &ObserveRequest {
                kind: BrowserObserveKind::Url,
                interactive_only: false,
                compact: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(backend.call_count(CallKind::Observe), 2);
        backend.fail_next(CallKind::Observe, BrowserErrorKind::NotConnected, 2);
        let err = rt
            .observe(
                &key("s"),
                &opts(),
                &ObserveRequest {
                    kind: BrowserObserveKind::Url,
                    interactive_only: false,
                    compact: false,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::NotConnected);
        assert_eq!(backend.call_count(CallKind::Observe), 4);
    }

    #[tokio::test]
    async fn binary_missing_is_not_retried() {
        let backend = Arc::new(FakeBackend::new());
        backend.set_available(false);
        let rt = runtime(backend.clone());
        let err = rt
            .observe(&key("s"), &opts(), &ObserveRequest::snapshot())
            .await
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::BinaryMissing);
        assert_eq!(backend.call_count(CallKind::OpenSession), 0);
        assert_eq!(backend.call_count(CallKind::Observe), 0);
        let err2 = rt
            .observe(&key("s"), &opts(), &ObserveRequest::snapshot())
            .await
            .unwrap_err();
        assert_eq!(err2.kind, BrowserErrorKind::BinaryMissing);
        assert_eq!(backend.call_count(CallKind::OpenSession), 0);
    }

    #[tokio::test]
    async fn not_connected_follows_recovery_policy() {
        let backend = Arc::new(FakeBackend::new());
        backend.fail_next(CallKind::Observe, BrowserErrorKind::NotConnected, 1);
        let rt = runtime(backend.clone());
        let obs = rt
            .observe(&key("s"), &opts(), &ObserveRequest::snapshot())
            .await
            .unwrap();
        assert!(obs.snapshot.is_some());
        assert_eq!(backend.call_count(CallKind::Observe), 2);
    }

    #[tokio::test]
    async fn snapshot_budget_is_utf8_safe_and_keeps_distinct_refs() {
        let backend = Arc::new(FakeBackend::new());
        let long = "你好🌍".repeat(80);
        let backend_id = BrowserBackendId::new("fake");
        backend.set_observation(BrowserObservation {
            url: Some("https://example.com/".into()),
            title: Some("t".into()),
            text: Some(long.clone()),
            snapshot: Some(BrowserSnapshot {
                text: long,
                elements: vec![
                    BrowserElement {
                        reference: BrowserElementRef::new(backend_id.clone(), "one"),
                        role: Some("button".into()),
                        name: Some("Submit".into()),
                        interactive: true,
                    },
                    BrowserElement {
                        reference: BrowserElementRef::new(backend_id, "two"),
                        role: Some("button".into()),
                        name: Some("Submit".into()),
                        interactive: true,
                    },
                ],
            }),
        });
        let mut policy = BrowserRuntimePolicy::default();
        policy.snapshot_char_limit = 40;
        let rt = BrowserRuntime::new(backend, policy);
        let obs = rt
            .observe(&key("s"), &opts(), &ObserveRequest::snapshot())
            .await
            .unwrap();
        let snap = obs.snapshot.unwrap();
        assert!(snap.text.is_char_boundary(snap.text.len()));
        assert!(snap.text.contains("truncated") || snap.text.chars().count() <= 80);
        assert_eq!(snap.elements.len(), 2);
        assert_eq!(snap.elements[0].name, snap.elements[1].name);
        assert_ne!(snap.elements[0].reference, snap.elements[1].reference);
    }

    #[tokio::test]
    async fn capability_rejection_happens_before_backend_call() {
        let backend = Arc::new(FakeBackend::new());
        backend.set_capabilities(BackendCapabilities::default());
        let rt = runtime(backend.clone());
        let err = rt
            .observe(&key("s"), &opts(), &ObserveRequest::snapshot())
            .await
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::Rejected);
        assert!(present_backend_error(&err).contains("BrowserCapabilityRejected"));
        assert_eq!(backend.call_count(CallKind::Observe), 0);
        assert_eq!(backend.call_count(CallKind::OpenSession), 0);
        let err = rt
            .act(
                &key("s"),
                &opts(),
                &BrowserAction::Eval { script: "1".into() },
            )
            .await
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::Rejected);
        assert_eq!(backend.call_count(CallKind::Act), 0);
    }

    #[tokio::test]
    async fn availability_is_independent_of_session_health() {
        let backend = Arc::new(FakeBackend::new());
        backend.mark_unhealthy("chat-health");
        let rt = runtime(backend);
        assert!(matches!(rt.availability(), BackendAvailability::Available));
        let health = rt.session_health(&key("chat-health"), &opts()).await;
        assert!(matches!(
            health,
            BrowserHealth::Unhealthy {
                kind: BrowserErrorKind::NotConnected,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn close_delegates_without_process_kill_assumption() {
        let backend = Arc::new(FakeBackend::new());
        let rt = runtime(backend.clone());
        rt.close_session(&key("chat-close"), &opts()).await.unwrap();
        assert_eq!(backend.call_count(CallKind::Close), 1);
        assert_eq!(backend.state.lock().unwrap().closed.len(), 1);
        assert!(!backend
            .calls()
            .iter()
            .any(|c| format!("{c:?}").contains("Kill")));
    }

    #[tokio::test]
    async fn tabs_api_exists_without_tool_schema_change() {
        let backend = Arc::new(FakeBackend::new());
        {
            let mut state = backend.state.lock().unwrap();
            state.tabs = vec![BrowserTab {
                id: BrowserPageId::new("p1").unwrap(),
                url: Some("https://example.com/".into()),
                title: Some("Example".into()),
                active: true,
            }];
        }
        let rt = runtime(backend);
        let tabs = rt.tabs(&key("s"), &opts()).await.unwrap();
        assert_eq!(tabs.len(), 1);
        let tool = BrowserTool::new(Vec::new(), true, false, None);
        let actions = tool.parameters_schema()["properties"]["action"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| a == "tabs"));
        assert_eq!(
            actions,
            V1_TOOL_ACTIONS
                .iter()
                .map(|s| (*s).to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn retry_mapping_matches_v1_retryable_actions() {
        assert!(operation_is_retryable(&RuntimeOp::Open));
        assert!(is_retryable_action("open"));
        for kind in [
            BrowserObserveKind::Snapshot,
            BrowserObserveKind::Text { target: None },
            BrowserObserveKind::Html { target: None },
            BrowserObserveKind::Url,
            BrowserObserveKind::Title,
            BrowserObserveKind::Value { target: css() },
            BrowserObserveKind::Visibility { target: css() },
            BrowserObserveKind::Enabled { target: css() },
        ] {
            assert_eq!(
                operation_is_retryable(&RuntimeOp::Observe(kind.clone())),
                is_retryable_action(kind.v1_action_name()),
                "{}",
                kind.v1_action_name()
            );
            assert_eq!(retry_class_for_observe(&kind), BrowserRetryClass::SafeRead);
        }
        assert!(!operation_is_retryable(&RuntimeOp::Observe(
            BrowserObserveKind::Find {
                role: "button".into(),
                name: None,
                action: None,
            }
        )));
        assert!(!is_retryable_action("find"));
        assert!(!is_retryable_action("screenshot"));
        assert!(!operation_is_retryable(&RuntimeOp::Screenshot));
        for action in [
            BrowserAction::Click { target: css() },
            BrowserAction::Fill {
                target: css(),
                value: "x".into(),
            },
            BrowserAction::Type {
                target: css(),
                text: "x".into(),
            },
            BrowserAction::Press {
                key: "Enter".into(),
            },
            BrowserAction::Select {
                target: css(),
                value: "1".into(),
            },
            BrowserAction::Eval { script: "1".into() },
        ] {
            assert_eq!(retry_class_for_action(&action), BrowserRetryClass::Mutating);
            assert!(!operation_is_retryable(&RuntimeOp::Act(action.clone())));
            assert!(!is_retryable_action(action.name()));
        }
        assert!(operation_is_retryable(&RuntimeOp::Act(
            BrowserAction::Wait {
                timeout_ms: None,
                text: None,
                target: None,
            }
        )));
        assert!(operation_is_retryable(&RuntimeOp::Act(
            BrowserAction::Reload
        )));
        assert!(!operation_is_retryable(&RuntimeOp::Act(
            BrowserAction::Back
        )));
        assert!(!operation_is_retryable(&RuntimeOp::CloseSession));
        assert!(!runtime_should_recover(BrowserErrorKind::BinaryMissing));
        assert!(!runtime_should_recover(BrowserErrorKind::Timeout));
        assert!(runtime_should_recover(BrowserErrorKind::NotConnected));
    }

    #[test]
    fn error_presentation_uses_stable_prefixes() {
        let id = BrowserBackendId::new("fake");
        let cases = [
            (BrowserErrorKind::BinaryMissing, "BrowserBinaryMissing"),
            (BrowserErrorKind::NotConnected, "BrowserDaemonUnavailable"),
            (BrowserErrorKind::Timeout, "BrowserCommandTimeout"),
            (BrowserErrorKind::Rejected, "BrowserUrlRejected"),
        ];
        for (kind, prefix) in cases {
            let err = BrowserBackendError::new(kind, id.clone(), "detail");
            assert!(present_backend_error(&err).starts_with(prefix), "{kind:?}");
        }
    }

    #[test]
    fn v1_tool_still_does_not_call_runtime() {
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
        assert!(!result.success);
        let error = result.error.expect("error");
        assert!(error.starts_with("BrowserSessionMissing:"));
        assert!(!error.to_lowercase().contains("browserruntime"));
    }

    #[test]
    fn observe_and_act_v1_routes_stay_split() {
        assert_eq!(
            crate::tools::browser_types::route_v1_tool_action("get_url"),
            Some(V1ToolActionRoute::Observe)
        );
        assert_eq!(
            crate::tools::browser_types::route_v1_tool_action("snapshot"),
            Some(V1ToolActionRoute::Observe)
        );
        assert_eq!(
            crate::tools::browser_types::route_v1_tool_action("click"),
            Some(V1ToolActionRoute::Act)
        );
        for action in V1_TOOL_ACTIONS {
            crate::tools::browser_types::route_v1_tool_action(action).expect(action);
        }
    }
}
