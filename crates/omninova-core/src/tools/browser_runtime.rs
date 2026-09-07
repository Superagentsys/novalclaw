//! Backend-neutral Browser Runtime orchestration.
//!
//! Production path: `BrowserTool` → `BrowserRuntime` → `BrowserBackend`.

use crate::tools::browser_backend::BrowserBackend;
use crate::tools::browser_control::{
    AgentOpClass, BrowserControlError, BrowserControlRegistry, BrowserControlState, TakeoverReason,
};
use crate::tools::browser_output::{
    BROWSER_OP_CHAR_LIMIT, BROWSER_SNAPSHOT_CHAR_LIMIT, BROWSER_TEXT_CHAR_LIMIT,
};
use crate::tools::browser_types::{
    BackendAvailability, BackendSessionHandle, BrowserAction, BrowserActionResult,
    BrowserBackendError, BrowserBackendId, BrowserElement, BrowserErrorKind, BrowserExtractRequest,
    BrowserExtractResult, BrowserHealth, BrowserObservation, BrowserObserveKind, BrowserSessionKey,
    BrowserSessionOpenRequest, BrowserSessionOptions, BrowserSnapshot, BrowserTab, NavigateRequest,
    ObserveRequest, ScreenshotRequest, ScreenshotResult, BROWSER_HUMAN_TAKEOVER_ACTIVE_DETAIL,
    BROWSER_STALE_ASSUMPTIONS_DETAIL, BROWSER_STALE_REFERENCE_DETAIL,
    BROWSER_TAKEOVER_LOST_DETAIL, BROWSER_TAKEOVER_REJECTED_DETAIL,
    BROWSER_TAKEOVER_UNSUPPORTED_HEADLESS_DETAIL,
};
use crate::tools::text_bound::{bound_head, truncate_head_chars};
use crate::tools::web_client::{host_matches_allowlist, redact_secrets_in_text};
use regex::Regex;
use serde_json::{Map, Value};
use std::sync::{Arc, OnceLock};
use url::Url;

/// Independent of snapshot text (24k). Caps malicious pages that emit huge
/// structured element maps. Does not change model-visible snapshot text.
pub const BROWSER_MAX_STRUCTURED_ELEMENTS: usize = 512;
pub const BROWSER_MAX_ELEMENT_ROLE_CHARS: usize = 64;
pub const BROWSER_MAX_ELEMENT_NAME_CHARS: usize = 512;

/// Backend-neutral structured-extract limits. The final budget intentionally
/// reuses the existing eval/operation model-output budget.
pub const BROWSER_MAX_JSON_DEPTH: usize = 16;
pub const BROWSER_MAX_JSON_ARRAY_ITEMS: usize = 128;
pub const BROWSER_MAX_JSON_OBJECT_FIELDS: usize = 128;
pub const BROWSER_MAX_JSON_STRING_CHARS: usize = 2_000;
pub const BROWSER_MAX_JSON_KEY_CHARS: usize = 128;
pub const BROWSER_MAX_SERIALIZED_JSON_CHARS: usize = BROWSER_OP_CHAR_LIMIT;

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
    pub max_structured_elements: usize,
    pub max_element_role_chars: usize,
    pub max_element_name_chars: usize,
}

impl Default for BrowserRuntimePolicy {
    fn default() -> Self {
        Self {
            allowed_domains: Vec::new(),
            snapshot_char_limit: BROWSER_SNAPSHOT_CHAR_LIMIT,
            text_char_limit: BROWSER_TEXT_CHAR_LIMIT,
            operation_char_limit: BROWSER_OP_CHAR_LIMIT,
            max_recovery_attempts: 1,
            max_structured_elements: BROWSER_MAX_STRUCTURED_ELEMENTS,
            max_element_role_chars: BROWSER_MAX_ELEMENT_ROLE_CHARS,
            max_element_name_chars: BROWSER_MAX_ELEMENT_NAME_CHARS,
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
    control: Arc<BrowserControlRegistry>,
}

/// Minimal, backend-neutral control surface retained by an active Agent run.
///
/// The handle intentionally exposes neither backend/session handles nor
/// executable/profile details. It points at the exact runtime used by the
/// model-visible BrowserTool, so BrowserControlRegistry remains authoritative.
#[derive(Clone)]
pub struct BrowserTakeoverHandle {
    runtime: Arc<BrowserRuntime>,
    session_key: BrowserSessionKey,
    session_opts: BrowserSessionOptions,
}

impl std::fmt::Debug for BrowserTakeoverHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserTakeoverHandle")
            .field("session_id_present", &true)
            .field("headless", &self.session_opts.headless)
            .finish_non_exhaustive()
    }
}

impl BrowserTakeoverHandle {
    pub fn new(
        runtime: Arc<BrowserRuntime>,
        session_key: BrowserSessionKey,
        session_opts: BrowserSessionOptions,
    ) -> Self {
        Self {
            runtime,
            session_key,
            session_opts,
        }
    }

    pub fn session_id(&self) -> &str {
        self.session_key.as_str()
    }

    pub fn headless(&self) -> bool {
        self.session_opts.headless
    }

    pub fn request(
        &self,
        reason: TakeoverReason,
    ) -> Result<BrowserControlState, BrowserBackendError> {
        self.runtime
            .request_human_takeover(&self.session_key, &self.session_opts, reason)
    }

    pub fn state(&self) -> BrowserControlState {
        self.runtime.get_takeover_state(&self.session_key)
    }

    pub fn release(&self) -> Result<BrowserControlState, BrowserBackendError> {
        self.runtime.release_human_takeover(&self.session_key)
    }

    pub fn cancel(&self) -> Result<BrowserControlState, BrowserBackendError> {
        self.runtime.cancel_human_takeover(&self.session_key)
    }

    #[cfg(test)]
    pub(crate) fn shares_runtime_with(&self, runtime: &Arc<BrowserRuntime>) -> bool {
        Arc::ptr_eq(&self.runtime, runtime)
    }

    #[cfg(test)]
    pub(crate) fn note_browser_lost_for_test(&self) {
        self.runtime.control.note_browser_lost(&self.session_key);
    }

    #[cfg(test)]
    pub(crate) fn force_timeout_for_test(
        &self,
    ) -> Result<BrowserControlState, BrowserBackendError> {
        self.runtime.force_human_takeover_timeout(&self.session_key)
    }
}

impl BrowserRuntime {
    pub fn new(backend: Arc<dyn BrowserBackend>, policy: BrowserRuntimePolicy) -> Self {
        Self {
            backend,
            policy,
            control: Arc::new(BrowserControlRegistry::new()),
        }
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
        let health = match self.ensure_session(key, opts).await {
            Ok(handle) => {
                self.control.remember_headless(key, opts.headless);
                self.backend.session_health(&handle).await
            }
            Err(err) => BrowserHealth::Unhealthy {
                kind: err.kind,
                detail: err.detail,
            },
        };
        if self.control.blocks_crash_recovery(key) {
            if let BrowserHealth::Unhealthy { kind, .. } = &health {
                if is_browser_lost_kind(*kind) {
                    self.control.note_browser_lost(key);
                }
            }
        }
        health
    }

    pub fn request_human_takeover(
        &self,
        key: &BrowserSessionKey,
        opts: &BrowserSessionOptions,
        reason: TakeoverReason,
    ) -> Result<BrowserControlState, BrowserBackendError> {
        self.control
            .request_human_takeover(key, opts.headless, reason)
            .map_err(|err| self.map_control_error(err))
    }

    pub fn get_takeover_state(&self, key: &BrowserSessionKey) -> BrowserControlState {
        self.control.get(key)
    }

    pub fn release_human_takeover(
        &self,
        key: &BrowserSessionKey,
    ) -> Result<BrowserControlState, BrowserBackendError> {
        self.control
            .release_human_takeover(key)
            .map_err(|err| self.map_control_error(err))
    }

    pub fn cancel_human_takeover(
        &self,
        key: &BrowserSessionKey,
    ) -> Result<BrowserControlState, BrowserBackendError> {
        self.control
            .cancel_human_takeover(key)
            .map_err(|err| self.map_control_error(err))
    }

    #[cfg(test)]
    pub fn force_human_takeover_timeout(
        &self,
        key: &BrowserSessionKey,
    ) -> Result<BrowserControlState, BrowserBackendError> {
        self.control
            .force_timeout(key)
            .map_err(|err| self.map_control_error(err))
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

    /// Execute a page-level expression through StructuredJson eval and decode
    /// the backend-unwrapped JSON value. The backend serializes in page
    /// context; Runtime only applies semantic JSON budgets.
    ///
    /// Like ordinary Eval, this operation may mutate page state and is never
    /// blindly replayed.
    pub async fn extract_json(
        &self,
        key: &BrowserSessionKey,
        opts: &BrowserSessionOptions,
        req: &BrowserExtractRequest,
    ) -> Result<BrowserExtractResult, BrowserBackendError> {
        let result = self
            .act(
                key,
                opts,
                &BrowserAction::eval_structured_json(req.expression.clone()),
            )
            .await?;
        let value = result.structured_output.ok_or_else(|| {
            self.invalid_structured_output("structured extract payload is missing")
        })?;
        Ok(bound_structured_json(value))
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
        let handle = self.backend.open_session(&req).await?;
        self.control.remember_headless(key, opts.headless);
        Ok(handle)
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
        if matches!(op, RuntimeOp::CloseSession) {
            let result = self.dispatch_with_recovery(&op, key, opts, &mut call).await;
            self.control.remove(key);
            return result;
        }

        let class = agent_op_class(&op);
        let permit = self
            .control
            .begin_agent_operation(key, class)
            .map_err(|err| self.map_control_error(err))?;

        let result = self.dispatch_with_recovery(&op, key, opts, &mut call).await;
        match &result {
            Ok(_) if matches!(op, RuntimeOp::Observe(_)) => {
                self.control.complete_resync(key);
            }
            Err(err)
                if self.control.blocks_crash_recovery(key) && is_browser_lost_kind(err.kind) =>
            {
                self.control.note_browser_lost(key);
            }
            _ => {}
        }
        drop(permit);
        result
    }

    async fn dispatch_with_recovery<T, F, Fut>(
        &self,
        op: &RuntimeOp,
        key: &BrowserSessionKey,
        opts: &BrowserSessionOptions,
        call: &mut F,
    ) -> Result<T, BrowserBackendError>
    where
        F: FnMut(BackendSessionHandle) -> Fut,
        Fut: std::future::Future<Output = Result<T, BrowserBackendError>>,
    {
        let mut recovered = 0u8;
        loop {
            let handle = match self.ensure_session(key, opts).await {
                Ok(handle) => handle,
                Err(err) if self.can_recover(op, key, recovered, &err) => {
                    recovered = recovered.saturating_add(1);
                    continue;
                }
                Err(err) => {
                    if self.control.blocks_crash_recovery(key) && is_browser_lost_kind(err.kind) {
                        self.control.note_browser_lost(key);
                    }
                    return Err(err);
                }
            };
            match call(handle).await {
                Ok(value) => return Ok(value),
                Err(err) if self.can_recover(op, key, recovered, &err) => {
                    recovered = recovered.saturating_add(1);
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
    }

    fn can_recover(
        &self,
        op: &RuntimeOp,
        key: &BrowserSessionKey,
        recovered: u8,
        err: &BrowserBackendError,
    ) -> bool {
        !self.control.blocks_crash_recovery(key)
            && err.retryable
            && recovered < self.policy.max_recovery_attempts
            && operation_is_retryable(op)
            && runtime_should_recover(err.kind)
    }

    fn map_control_error(&self, err: BrowserControlError) -> BrowserBackendError {
        let (kind, detail) = match err {
            BrowserControlError::HumanTakeoverActive { .. } => (
                BrowserErrorKind::HumanTakeoverActive,
                BROWSER_HUMAN_TAKEOVER_ACTIVE_DETAIL.to_string(),
            ),
            BrowserControlError::UnsupportedHeadless => (
                BrowserErrorKind::TakeoverUnsupportedHeadless,
                BROWSER_TAKEOVER_UNSUPPORTED_HEADLESS_DETAIL.to_string(),
            ),
            BrowserControlError::StaleAssumptions => (
                BrowserErrorKind::StaleAssumptions,
                BROWSER_STALE_ASSUMPTIONS_DETAIL.to_string(),
            ),
            BrowserControlError::BrowserLost { .. } => (
                BrowserErrorKind::TakeoverBrowserLost,
                BROWSER_TAKEOVER_LOST_DETAIL.to_string(),
            ),
            BrowserControlError::Rejected { .. } => (
                BrowserErrorKind::TakeoverRejected,
                BROWSER_TAKEOVER_REJECTED_DETAIL.to_string(),
            ),
        };
        BrowserBackendError::new(kind, self.backend.id(), detail)
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

    fn invalid_structured_output(&self, reason: &str) -> BrowserBackendError {
        let mut error = BrowserBackendError::new(
            BrowserErrorKind::InvalidStructuredOutput,
            self.backend.id(),
            format!("BrowserStructuredOutputInvalid: {reason}"),
        );
        error.retryable = false;
        error
    }

    fn budget_action_result(&self, result: BrowserActionResult) -> BrowserActionResult {
        BrowserActionResult {
            detail: bound_head(
                &redact_secrets_in_text(&result.detail),
                self.policy.operation_char_limit,
            ),
            url: result.url.map(|u| redact_secrets_in_text(&u)),
            title: result.title.map(|t| redact_secrets_in_text(&t)),
            structured_output: result.structured_output,
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
            | BrowserObserveKind::Html { .. }
            | BrowserObserveKind::Read { .. } => obs.text.map(|t| redact_secrets_in_text(&t)),
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
            elements: self.budget_snapshot_elements(snap.elements),
        });
        BrowserObservation {
            url,
            title,
            text,
            snapshot,
            truncated: obs.truncated,
        }
    }

    fn budget_snapshot_elements(&self, elements: Vec<BrowserElement>) -> Vec<BrowserElement> {
        elements
            .into_iter()
            .take(self.policy.max_structured_elements)
            .map(|element| BrowserElement {
                reference: element.reference,
                role: element.role.map(|role| {
                    self.bound_untrusted_element_field(&role, self.policy.max_element_role_chars)
                }),
                name: element.name.map(|name| {
                    self.bound_untrusted_element_field(&name, self.policy.max_element_name_chars)
                }),
                interactive: element.interactive,
            })
            .collect()
    }

    fn bound_untrusted_element_field(&self, text: &str, max_chars: usize) -> String {
        let sanitized = sanitize_untrusted_web_field(text);
        truncate_head_chars(&sanitized, max_chars).0
    }
}

fn bound_structured_json(value: Value) -> BrowserExtractResult {
    let mut truncated = false;
    let sanitized = sanitize_structured_value(value, 0, &mut truncated);
    let bounded =
        fit_structured_value(sanitized, BROWSER_MAX_SERIALIZED_JSON_CHARS, &mut truncated);
    BrowserExtractResult {
        value: bounded,
        truncated,
    }
}

fn sanitize_structured_value(value: Value, depth: usize, truncated: &mut bool) -> Value {
    match value {
        Value::String(text) => {
            let redacted = redact_credential_assignments(&redact_secrets_in_text(&text));
            let (bounded, was_truncated, _) =
                truncate_head_chars(&redacted, BROWSER_MAX_JSON_STRING_CHARS);
            *truncated |= was_truncated;
            Value::String(bounded)
        }
        Value::Array(values) => {
            if depth >= BROWSER_MAX_JSON_DEPTH {
                *truncated = true;
                return Value::Null;
            }
            if values.len() > BROWSER_MAX_JSON_ARRAY_ITEMS {
                *truncated = true;
            }
            Value::Array(
                values
                    .into_iter()
                    .take(BROWSER_MAX_JSON_ARRAY_ITEMS)
                    .map(|value| sanitize_structured_value(value, depth + 1, truncated))
                    .collect(),
            )
        }
        Value::Object(fields) => {
            if depth >= BROWSER_MAX_JSON_DEPTH {
                *truncated = true;
                return Value::Null;
            }
            if fields.len() > BROWSER_MAX_JSON_OBJECT_FIELDS {
                *truncated = true;
            }
            let mut bounded = Map::new();
            for (key, value) in fields.into_iter().take(BROWSER_MAX_JSON_OBJECT_FIELDS) {
                let (key, key_truncated, _) = truncate_head_chars(&key, BROWSER_MAX_JSON_KEY_CHARS);
                *truncated |= key_truncated;
                // Never overwrite when two untrusted keys share a truncated
                // prefix. Dropping the later field is explicit truncation.
                if bounded.contains_key(&key) {
                    *truncated = true;
                    continue;
                }
                bounded.insert(key, sanitize_structured_value(value, depth + 1, truncated));
            }
            Value::Object(bounded)
        }
        scalar => scalar,
    }
}

fn serialized_chars(value: &Value) -> usize {
    serde_json::to_string(value)
        .map(|text| text.chars().count())
        .unwrap_or(usize::MAX)
}

/// Fit a sanitized tree to a final serialized-char limit while preserving
/// valid JSON. Containers retain the longest deterministic prefix that fits.
fn fit_structured_value(value: Value, limit: usize, truncated: &mut bool) -> Value {
    if serialized_chars(&value) <= limit {
        return value;
    }
    *truncated = true;
    match value {
        Value::String(text) => fit_json_string(text, limit),
        Value::Array(values) => {
            let mut bounded = Vec::new();
            for value in values {
                let current_len = serialized_chars(&Value::Array(bounded.clone()));
                let separator = usize::from(!bounded.is_empty());
                let available = limit.saturating_sub(current_len + separator);
                if available == 0 {
                    break;
                }
                let fitted = fit_structured_value(value, available, truncated);
                bounded.push(fitted);
                if serialized_chars(&Value::Array(bounded.clone())) > limit {
                    bounded.pop();
                    break;
                }
            }
            Value::Array(bounded)
        }
        Value::Object(fields) => {
            let mut bounded = Map::new();
            for (key, value) in fields {
                let current_len = serialized_chars(&Value::Object(bounded.clone()));
                let separator = usize::from(!bounded.is_empty());
                let key_len = serde_json::to_string(&key)
                    .map(|text| text.chars().count())
                    .unwrap_or(usize::MAX);
                let available = limit.saturating_sub(current_len + separator + key_len + 1);
                if available == 0 {
                    break;
                }
                let fitted = fit_structured_value(value, available, truncated);
                bounded.insert(key.clone(), fitted);
                if serialized_chars(&Value::Object(bounded.clone())) > limit {
                    bounded.remove(&key);
                    break;
                }
            }
            Value::Object(bounded)
        }
        scalar => {
            if serialized_chars(&scalar) <= limit {
                scalar
            } else {
                Value::Null
            }
        }
    }
}

fn fit_json_string(text: String, limit: usize) -> Value {
    let chars: Vec<char> = text.chars().collect();
    let mut low = 0usize;
    let mut high = chars.len();
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect();
        if serialized_chars(&Value::String(candidate)) <= limit {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    Value::String(chars[..low].iter().collect())
}

/// `role` / `name` are page-controlled. Redact credential-like spans and URL
/// userinfo. Never applied to element reference values.
fn sanitize_untrusted_web_field(text: &str) -> String {
    redact_credential_assignments(&redact_secrets_in_text(text))
}

fn redact_credential_assignments(text: &str) -> String {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\b(api[_-]?key|password|passwd|authorization|secret|token)\s*[:=]\s*\S+")
            .expect("credential assignment pattern")
    });
    pattern.replace_all(text, "$1=***").into_owned()
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
    if err.kind == BrowserErrorKind::StaleReference {
        if err.detail.starts_with("Browser") {
            return err.detail.clone();
        }
        if err.detail.is_empty() || err.detail == "injected" {
            return BROWSER_STALE_REFERENCE_DETAIL.to_string();
        }
        return format!("{BROWSER_STALE_REFERENCE_DETAIL} ({})", err.detail);
    }
    if err.detail.starts_with("Browser") {
        return err.detail.clone();
    }
    let prefix = match err.kind {
        BrowserErrorKind::BinaryMissing => "BrowserBinaryMissing",
        BrowserErrorKind::BrowserUnavailable => "BrowserUnavailable",
        BrowserErrorKind::LaunchFailed => "BrowserLaunchFailed",
        BrowserErrorKind::NotConnected => "BrowserDaemonUnavailable",
        BrowserErrorKind::SessionNotFound => "BrowserSessionUnavailable",
        BrowserErrorKind::Crashed => "BrowserCrashed",
        BrowserErrorKind::Timeout => "BrowserCommandTimeout",
        BrowserErrorKind::Rejected => "BrowserUrlRejected",
        BrowserErrorKind::CommandFailed => "BrowserCommandFailed",
        BrowserErrorKind::InvalidStructuredOutput => "BrowserStructuredOutputInvalid",
        BrowserErrorKind::StaleReference => "BrowserCommandFailed",
        BrowserErrorKind::ProfileBusy => "BrowserProfileBusy",
        BrowserErrorKind::InstalledProfileSnapshotIncomplete => {
            "BrowserInstalledProfileSnapshotIncomplete"
        }
        BrowserErrorKind::HumanTakeoverActive => "BrowserHumanTakeoverActive",
        BrowserErrorKind::TakeoverUnsupportedHeadless => "BrowserTakeoverUnsupportedHeadless",
        BrowserErrorKind::StaleAssumptions => "BrowserStaleAssumptions",
        BrowserErrorKind::TakeoverBrowserLost => "BrowserTakeoverLost",
        BrowserErrorKind::TakeoverRejected => "BrowserTakeoverRejected",
    };
    format!("{prefix}: {}", err.detail)
}

fn agent_op_class(op: &RuntimeOp) -> AgentOpClass {
    match op {
        RuntimeOp::Observe(_) => AgentOpClass::Observe,
        RuntimeOp::Open | RuntimeOp::Act(_) | RuntimeOp::Screenshot | RuntimeOp::Tabs => {
            AgentOpClass::Mutate
        }
        RuntimeOp::CloseSession => AgentOpClass::Mutate,
    }
}

fn is_browser_lost_kind(kind: BrowserErrorKind) -> bool {
    matches!(
        kind,
        BrowserErrorKind::Crashed
            | BrowserErrorKind::NotConnected
            | BrowserErrorKind::SessionNotFound
            | BrowserErrorKind::TakeoverBrowserLost
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::browser_control::{
        BrowserControlOwner, BrowserTakeoverPhase, TakeoverReason,
    };
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
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::sync::Notify;

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
        structured_output: Option<Value>,
        tabs: Vec<BrowserTab>,
        last_handle_tokens: HashMap<String, String>,
    }

    struct FakeBackend {
        id: BrowserBackendId,
        state: Mutex<FakeState>,
        observe_hold: AtomicBool,
        observe_started: Notify,
        observe_release: Notify,
        act_hold: AtomicBool,
        act_started: Notify,
        act_release: Notify,
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
                        truncated: false,
                    },
                    structured_output: None,
                    tabs: Vec::new(),
                    last_handle_tokens: HashMap::new(),
                }),
                observe_hold: AtomicBool::new(false),
                observe_started: Notify::new(),
                observe_release: Notify::new(),
                act_hold: AtomicBool::new(false),
                act_started: Notify::new(),
                act_release: Notify::new(),
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

        fn set_structured_output(&self, value: Option<Value>) {
            self.state.lock().expect("state").structured_output = value;
        }

        fn enable_observe_hold(&self) {
            self.observe_hold.store(true, Ordering::SeqCst);
        }

        fn release_observe_hold(&self) {
            self.observe_hold.store(false, Ordering::SeqCst);
            self.observe_release.notify_waiters();
        }

        fn enable_act_hold(&self) {
            self.act_hold.store(true, Ordering::SeqCst);
        }

        fn release_act_hold(&self) {
            self.act_hold.store(false, Ordering::SeqCst);
            self.act_release.notify_waiters();
        }

        async fn wait_if_held(&self, hold: &AtomicBool, started: &Notify, release: &Notify) {
            if hold.load(Ordering::SeqCst) {
                started.notify_waiters();
                release.notified().await;
            }
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
                structured_output: None,
            })
        }

        async fn observe(
            &self,
            _session: &BackendSessionHandle,
            req: &ObserveRequest,
        ) -> Result<BrowserObservation, BrowserBackendError> {
            self.wait_if_held(&self.observe_hold, &self.observe_started, &self.observe_release)
                .await;
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
            self.wait_if_held(&self.act_hold, &self.act_started, &self.act_release)
                .await;
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
                structured_output: self.state.lock().expect("state").structured_output.clone(),
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

    fn headed_opts() -> BrowserSessionOptions {
        BrowserSessionOptions {
            headless: false,
            ..BrowserSessionOptions::default()
        }
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
            truncated: false,
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
            truncated: false,
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
    async fn structured_element_fields_are_redacted_bounded_and_refs_untouched() {
        let backend = Arc::new(FakeBackend::new());
        let backend_id = BrowserBackendId::new("fake");
        let long_name = "你好🌍".repeat(40);
        backend.set_observation(BrowserObservation {
            url: Some("https://example.com/".into()),
            title: Some("t".into()),
            text: Some("snapshot-text-must-survive".into()),
            snapshot: Some(BrowserSnapshot {
                text: "snapshot-text-must-survive".into(),
                elements: vec![
                    BrowserElement {
                        reference: BrowserElementRef::new(backend_id.clone(), "e123"),
                        role: Some("button".into()),
                        name: Some("api_key=SECRET_TOKEN_VALUE".into()),
                        interactive: false,
                    },
                    BrowserElement {
                        reference: BrowserElementRef::new(backend_id.clone(), "e124"),
                        role: Some("heading".into()),
                        name: Some(long_name),
                        interactive: false,
                    },
                    BrowserElement {
                        reference: BrowserElementRef::new(backend_id, "e125"),
                        role: Some("link".into()),
                        name: Some("Keep me".into()),
                        interactive: true,
                    },
                ],
            }),
            truncated: false,
        });
        let mut policy = BrowserRuntimePolicy::default();
        policy.max_structured_elements = 2;
        policy.max_element_name_chars = 12;
        policy.max_element_role_chars = 16;
        let snapshot_text_before = "snapshot-text-must-survive".to_string();
        let rt = BrowserRuntime::new(backend.clone(), policy);
        let obs = rt
            .observe(&key("s"), &opts(), &ObserveRequest::snapshot())
            .await
            .unwrap();
        assert_eq!(obs.text.as_deref(), Some(snapshot_text_before.as_str()));
        let snap = obs.snapshot.unwrap();
        assert_eq!(snap.text, "snapshot-text-must-survive");
        assert_eq!(snap.elements.len(), 2);
        assert_eq!(snap.elements[0].reference.as_str(), "e123");
        assert_eq!(snap.elements[1].reference.as_str(), "e124");
        let secret_name = snap.elements[0].name.as_deref().unwrap();
        assert!(!secret_name.contains("SECRET_TOKEN_VALUE"));
        assert!(secret_name.contains("***"));
        let unicode_name = snap.elements[1].name.as_deref().unwrap();
        assert!(unicode_name.is_char_boundary(unicode_name.len()));
        assert_eq!(unicode_name.chars().count(), 12);
        assert!(!unicode_name.contains("Keep me"));
        let clicked = rt
            .act(
                &key("s"),
                &opts(),
                &BrowserAction::Click {
                    target: BrowserTarget::Element(snap.elements[0].reference.clone()),
                },
            )
            .await
            .unwrap();
        assert_eq!(clicked.detail, "click");
        assert_eq!(backend.call_count(CallKind::Act), 1);
    }

    #[tokio::test]
    async fn read_content_is_redacted_and_bounded() {
        let backend = Arc::new(FakeBackend::new());
        backend.set_observation(BrowserObservation {
            url: Some("https://user:SECRET@example.com/doc".into()),
            title: None,
            text: Some(
                "--- BEGIN WEB CONTENT ---\napi_key=SECRET_TOKEN_VALUE\n--- END WEB CONTENT ---"
                    .into(),
            ),
            snapshot: None,
            truncated: true,
        });
        let rt = runtime(backend);
        let obs = rt
            .observe(&key("s"), &opts(), &ObserveRequest::read(false, None))
            .await
            .unwrap();
        assert!(obs.snapshot.is_none());
        assert!(obs.truncated);
        assert!(!obs.url.as_deref().unwrap_or("").contains("SECRET"));
        assert!(!obs.text.as_deref().unwrap_or("").contains("user:SECRET"));
        assert!(obs.text.as_deref().unwrap().contains("BEGIN WEB CONTENT"));
    }

    #[tokio::test]
    async fn stale_reference_is_not_session_not_found() {
        let backend = Arc::new(FakeBackend::new());
        backend.fail_next(CallKind::Act, BrowserErrorKind::StaleReference, 1);
        let rt = runtime(backend);
        let err = rt
            .act(&key("s"), &opts(), &BrowserAction::Click { target: css() })
            .await
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::StaleReference);
        assert_ne!(err.kind, BrowserErrorKind::SessionNotFound);
    }

    #[tokio::test]
    async fn stale_reference_is_not_retryable() {
        let backend = Arc::new(FakeBackend::new());
        backend.fail_next(CallKind::Act, BrowserErrorKind::StaleReference, 3);
        let rt = runtime(backend.clone());
        let err = rt
            .act(&key("s"), &opts(), &BrowserAction::Click { target: css() })
            .await
            .unwrap_err();
        assert!(!err.retryable);
        assert!(!runtime_should_recover(BrowserErrorKind::StaleReference));
        assert_eq!(backend.call_count(CallKind::Act), 1);
    }

    #[tokio::test]
    async fn runtime_does_not_retry_stale_mutating_action() {
        let backend = Arc::new(FakeBackend::new());
        backend.fail_next(CallKind::Act, BrowserErrorKind::StaleReference, 3);
        let rt = runtime(backend.clone());
        let err = rt
            .act(
                &key("s"),
                &opts(),
                &BrowserAction::Fill {
                    target: css(),
                    value: "x".into(),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::StaleReference);
        assert!(!err.retryable);
        assert_eq!(backend.call_count(CallKind::Act), 1);
        assert_eq!(backend.call_count(CallKind::Observe), 0);
    }

    #[tokio::test]
    async fn runtime_does_not_retry_stale_reference_with_old_handle() {
        let backend = Arc::new(FakeBackend::new());
        let old_ref = BrowserElementRef::new(BrowserBackendId::new("fake"), "e8");
        backend.set_observation(BrowserObservation {
            url: Some("https://example.com/".into()),
            title: None,
            text: Some("snap".into()),
            snapshot: Some(BrowserSnapshot {
                text: "snap".into(),
                elements: vec![BrowserElement {
                    reference: old_ref.clone(),
                    role: Some("button".into()),
                    name: Some("DeleteMe".into()),
                    interactive: true,
                }],
            }),
            truncated: false,
        });
        let rt = runtime(backend.clone());
        let obs = rt
            .observe(&key("s"), &opts(), &ObserveRequest::snapshot())
            .await
            .unwrap();
        let handle = obs.snapshot.unwrap().elements[0].reference.clone();
        backend.fail_next(CallKind::Act, BrowserErrorKind::StaleReference, 3);
        let err = rt
            .act(
                &key("s"),
                &opts(),
                &BrowserAction::Click {
                    target: BrowserTarget::Element(handle),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::StaleReference);
        assert!(!err.retryable);
        assert_eq!(backend.call_count(CallKind::Act), 1);
        assert_eq!(backend.call_count(CallKind::Observe), 1);
        backend.fail_next(CallKind::Observe, BrowserErrorKind::StaleReference, 3);
        let text_err = rt
            .observe(
                &key("s"),
                &opts(),
                &ObserveRequest {
                    kind: BrowserObserveKind::Text {
                        target: Some(BrowserTarget::Element(old_ref)),
                    },
                    interactive_only: false,
                    compact: false,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(text_err.kind, BrowserErrorKind::StaleReference);
        assert!(!text_err.retryable);
        assert_eq!(backend.call_count(CallKind::Observe), 2);
    }

    #[tokio::test]
    async fn stale_error_guides_reobserve() {
        let backend = Arc::new(FakeBackend::new());
        backend.fail_next(CallKind::Act, BrowserErrorKind::StaleReference, 1);
        let rt = runtime(backend);
        let err = rt
            .act(&key("s"), &opts(), &BrowserAction::Click { target: css() })
            .await
            .unwrap_err();
        let presented = present_backend_error(&err);
        assert!(presented.starts_with("BrowserCommandFailed:"));
        assert!(presented.contains("snapshot again"));
        assert!(!presented.starts_with("BrowserStaleReference:"));
    }

    #[tokio::test]
    async fn backend_session_remains_healthy_when_ref_is_stale() {
        let backend = Arc::new(FakeBackend::new());
        backend.fail_next(CallKind::Act, BrowserErrorKind::StaleReference, 1);
        let rt = runtime(backend);
        let err = rt
            .act(&key("s"), &opts(), &BrowserAction::Click { target: css() })
            .await
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::StaleReference);
        let health = rt.session_health(&key("s"), &opts()).await;
        assert!(matches!(health, BrowserHealth::Healthy));
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
            .act(&key("s"), &opts(), &BrowserAction::eval_raw("1"))
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
        assert!(actions.iter().any(|a| a == "read"));
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
            BrowserObserveKind::Read {
                outline: false,
                filter: None,
            },
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
            BrowserAction::eval_raw("1"),
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
        assert!(!runtime_should_recover(BrowserErrorKind::StaleReference));
        assert!(!runtime_should_recover(BrowserErrorKind::ProfileBusy));
        assert!(!runtime_should_recover(
            BrowserErrorKind::InstalledProfileSnapshotIncomplete
        ));
        assert!(!runtime_should_recover(BrowserErrorKind::HumanTakeoverActive));
        assert!(!runtime_should_recover(
            BrowserErrorKind::TakeoverUnsupportedHeadless
        ));
        assert!(!runtime_should_recover(BrowserErrorKind::StaleAssumptions));
        assert!(!runtime_should_recover(BrowserErrorKind::TakeoverBrowserLost));
        assert!(!runtime_should_recover(BrowserErrorKind::TakeoverRejected));
        assert!(runtime_should_recover(BrowserErrorKind::NotConnected));
    }

    #[test]
    fn error_presentation_uses_stable_prefixes() {
        let id = BrowserBackendId::new("fake");
        let cases = [
            (BrowserErrorKind::BinaryMissing, "BrowserBinaryMissing"),
            (BrowserErrorKind::BrowserUnavailable, "BrowserUnavailable"),
            (BrowserErrorKind::NotConnected, "BrowserDaemonUnavailable"),
            (BrowserErrorKind::Timeout, "BrowserCommandTimeout"),
            (BrowserErrorKind::Rejected, "BrowserUrlRejected"),
            (
                BrowserErrorKind::InvalidStructuredOutput,
                "BrowserStructuredOutputInvalid",
            ),
            (BrowserErrorKind::StaleReference, "BrowserCommandFailed"),
            (BrowserErrorKind::ProfileBusy, "BrowserProfileBusy"),
            (
                BrowserErrorKind::InstalledProfileSnapshotIncomplete,
                "BrowserInstalledProfileSnapshotIncomplete",
            ),
            (BrowserErrorKind::HumanTakeoverActive, "BrowserHumanTakeoverActive"),
            (
                BrowserErrorKind::TakeoverUnsupportedHeadless,
                "BrowserTakeoverUnsupportedHeadless",
            ),
            (BrowserErrorKind::StaleAssumptions, "BrowserStaleAssumptions"),
            (BrowserErrorKind::TakeoverBrowserLost, "BrowserTakeoverLost"),
            (BrowserErrorKind::TakeoverRejected, "BrowserTakeoverRejected"),
        ];
        for (kind, prefix) in cases {
            let err = BrowserBackendError::new(kind, id.clone(), "detail");
            assert!(present_backend_error(&err).starts_with(prefix), "{kind:?}");
        }
        let stale = BrowserBackendError::new(BrowserErrorKind::StaleReference, id, "injected");
        let presented = present_backend_error(&stale);
        assert_eq!(presented, BROWSER_STALE_REFERENCE_DETAIL);
        assert!(presented.contains("snapshot again"));
    }

    #[tokio::test]
    async fn extract_json_parses_object_array_and_json_scalars_once() {
        let backend = Arc::new(FakeBackend::new());
        let rt = runtime(backend.clone());
        let key = BrowserSessionKey::new("extract-session").unwrap();
        let opts = BrowserSessionOptions::default();

        for (value, expected) in [
            (
                json!({"name":"Alpha","price":12.5}),
                json!({"name": "Alpha", "price": 12.5}),
            ),
            (
                json!([{"name":"A"},{"name":"B"}]),
                json!([{"name": "A"}, {"name": "B"}]),
            ),
            (json!("text"), json!("text")),
            (json!(42), json!(42)),
            (json!(true), json!(true)),
            (Value::Null, Value::Null),
        ] {
            backend.set_structured_output(Some(value));
            let result = rt
                .extract_json(
                    &key,
                    &opts,
                    &BrowserExtractRequest {
                        expression: "window.__fixture".into(),
                    },
                )
                .await
                .unwrap();
            assert_eq!(result.value, expected);
            assert!(!result.truncated);
        }
    }

    #[tokio::test]
    async fn extract_json_does_not_reparse_json_strings_or_require_stringify() {
        let backend = Arc::new(FakeBackend::new());
        let rt = runtime(backend.clone());
        let key = BrowserSessionKey::new("invalid-extract").unwrap();
        let opts = BrowserSessionOptions::default();

        backend.set_structured_output(None);
        let err = rt
            .extract_json(
                &key,
                &opts,
                &BrowserExtractRequest {
                    expression: "undefined".into(),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::InvalidStructuredOutput);
        assert!(!err.retryable);

        backend.set_structured_output(Some(json!("not json")));
        let text = rt
            .extract_json(
                &key,
                &opts,
                &BrowserExtractRequest {
                    expression: "'not json'".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(text.value, json!("not json"));

        backend.set_structured_output(Some(json!(r#"{"nested":true}"#)));
        let once = rt
            .extract_json(
                &key,
                &opts,
                &BrowserExtractRequest {
                    expression: "'{\"nested\":true}'".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(once.value, json!(r#"{"nested":true}"#));
    }

    #[tokio::test]
    async fn extract_json_recursively_redacts_strings_without_retyping_values() {
        let backend = Arc::new(FakeBackend::new());
        backend.set_structured_output(Some(json!({
            "token": "api_key=SUPERSECRET",
            "nested": {"password": "password=hunter2"},
            "number": 12.5,
            "enabled": true,
            "nothing": null
        })));
        let result = runtime(backend)
            .extract_json(
                &BrowserSessionKey::new("redact-extract").unwrap(),
                &BrowserSessionOptions::default(),
                &BrowserExtractRequest {
                    expression: "window.__fixture".into(),
                },
            )
            .await
            .unwrap();
        let rendered = serde_json::to_string(&result.value).unwrap();
        assert!(!rendered.contains("SUPERSECRET"));
        assert!(!rendered.contains("hunter2"));
        assert_eq!(result.value["number"], json!(12.5));
        assert_eq!(result.value["enabled"], json!(true));
        assert_eq!(result.value["nothing"], Value::Null);
        assert!(result.value.get("token").is_some());
        assert!(result.value["nested"].get("password").is_some());
    }

    #[tokio::test]
    async fn extract_json_bounds_depth_breadth_unicode_and_total_size_as_valid_json() {
        let mut deep = json!("leaf");
        for _ in 0..(BROWSER_MAX_JSON_DEPTH + 8) {
            deep = json!({"child": deep});
        }
        let huge_array: Vec<Value> = (0..(BROWSER_MAX_JSON_ARRAY_ITEMS + 40))
            .map(|index| json!({"index": index, "label": "商品🚀".repeat(40)}))
            .collect();
        let mut huge_object = Map::new();
        for index in 0..(BROWSER_MAX_JSON_OBJECT_FIELDS + 40) {
            huge_object.insert(format!("field-{index}"), json!(index));
        }
        let fixture = json!({
            "deep": deep,
            "array": huge_array,
            "object": huge_object,
            "unicode": "你好🚀".repeat(BROWSER_MAX_JSON_STRING_CHARS)
        });
        let backend = Arc::new(FakeBackend::new());
        backend.set_structured_output(Some(fixture));
        let result = runtime(backend)
            .extract_json(
                &BrowserSessionKey::new("bounded-extract").unwrap(),
                &BrowserSessionOptions::default(),
                &BrowserExtractRequest {
                    expression: "window.__fixture".into(),
                },
            )
            .await
            .unwrap();
        assert!(result.truncated);
        let encoded = serde_json::to_string(&result.value).unwrap();
        assert!(encoded.chars().count() <= BROWSER_MAX_SERIALIZED_JSON_CHARS);
        assert!(serde_json::from_str::<Value>(&encoded).is_ok());
        assert!(std::str::from_utf8(encoded.as_bytes()).is_ok());
    }

    #[test]
    fn structured_limits_bound_array_object_and_key_collisions_without_overwrite() {
        let array = Value::Array(
            (0..(BROWSER_MAX_JSON_ARRAY_ITEMS + 10))
                .map(|index| json!(index))
                .collect(),
        );
        let array = bound_structured_json(array);
        assert!(array.truncated);
        assert_eq!(
            array.value.as_array().unwrap().len(),
            BROWSER_MAX_JSON_ARRAY_ITEMS
        );

        let mut fields = Map::new();
        for index in 0..(BROWSER_MAX_JSON_OBJECT_FIELDS + 10) {
            fields.insert(format!("k{index}"), json!(index));
        }
        let object = bound_structured_json(Value::Object(fields));
        assert!(object.truncated);
        assert_eq!(
            object.value.as_object().unwrap().len(),
            BROWSER_MAX_JSON_OBJECT_FIELDS
        );

        let prefix = "x".repeat(BROWSER_MAX_JSON_KEY_CHARS);
        let mut colliding_fields = Map::new();
        colliding_fields.insert(format!("{prefix}A"), json!(1));
        colliding_fields.insert(format!("{prefix}B"), json!(2));
        let collision = bound_structured_json(Value::Object(colliding_fields));
        assert!(collision.truncated);
        assert_eq!(collision.value.as_object().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn extract_json_is_mutating_and_never_blind_retried() {
        let backend = Arc::new(FakeBackend::new());
        backend.fail_next(CallKind::Act, BrowserErrorKind::NotConnected, 3);
        let err = runtime(backend.clone())
            .extract_json(
                &BrowserSessionKey::new("extract-no-retry").unwrap(),
                &BrowserSessionOptions::default(),
                &BrowserExtractRequest {
                    expression: "{sideEffect:true}".into(),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::NotConnected);
        assert_eq!(backend.call_count(CallKind::Act), 1);
        assert_eq!(
            retry_class_for_action(&BrowserAction::eval_structured_json("{sideEffect:true}")),
            BrowserRetryClass::Mutating
        );
        assert_eq!(
            retry_class_for_action(&BrowserAction::eval_raw("1")),
            BrowserRetryClass::Mutating
        );
    }

    #[tokio::test]
    async fn source_too_large_is_not_runtime_truncation_and_is_not_retried() {
        let backend = Arc::new(FakeBackend::new());
        backend.fail_next(CallKind::Act, BrowserErrorKind::CommandFailed, 3);
        let err = runtime(backend.clone())
            .extract_json(
                &BrowserSessionKey::new("extract-too-large").unwrap(),
                &BrowserSessionOptions::default(),
                &BrowserExtractRequest {
                    expression: "'X'.repeat(5000000)".into(),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::CommandFailed);
        assert_ne!(err.kind, BrowserErrorKind::InvalidStructuredOutput);
        assert_eq!(backend.call_count(CallKind::Act), 1);
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

    #[tokio::test]
    async fn headed_takeover_blocks_all_agent_ops_until_observe_resume() {
        let backend = Arc::new(FakeBackend::new());
        let rt = runtime(backend.clone());
        let k = key("takeover-a");
        let opts = headed_opts();
        rt.open(
            &k,
            &opts,
            &NavigateRequest {
                url: "https://example.com/".into(),
            },
        )
        .await
        .unwrap();

        let granted = rt
            .request_human_takeover(&k, &opts, TakeoverReason::ExplicitUserRequest)
            .unwrap();
        assert_eq!(granted.phase, BrowserTakeoverPhase::HumanControlled);
        assert_eq!(granted.owner, BrowserControlOwner::Human);

        let open_err = rt
            .open(
                &k,
                &opts,
                &NavigateRequest {
                    url: "https://example.com/next".into(),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(open_err.kind, BrowserErrorKind::HumanTakeoverActive);
        assert!(!open_err.retryable);

        let observe_err = rt
            .observe(&k, &opts, &ObserveRequest::snapshot())
            .await
            .unwrap_err();
        assert_eq!(observe_err.kind, BrowserErrorKind::HumanTakeoverActive);

        let read_err = rt
            .observe(
                &k,
                &opts,
                &ObserveRequest {
                    kind: BrowserObserveKind::Read {
                        outline: false,
                        filter: None,
                    },
                    interactive_only: false,
                    compact: false,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(read_err.kind, BrowserErrorKind::HumanTakeoverActive);

        let shot_err = rt
            .screenshot(&k, &opts, &ScreenshotRequest { locator: None })
            .await
            .unwrap_err();
        assert_eq!(shot_err.kind, BrowserErrorKind::HumanTakeoverActive);

        let tabs_err = rt.tabs(&k, &opts).await.unwrap_err();
        assert_eq!(tabs_err.kind, BrowserErrorKind::HumanTakeoverActive);

        let act_err = rt
            .act(&k, &opts, &BrowserAction::Click { target: css() })
            .await
            .unwrap_err();
        assert_eq!(act_err.kind, BrowserErrorKind::HumanTakeoverActive);

        let eval_err = rt
            .act(&k, &opts, &BrowserAction::eval_raw("1+1"))
            .await
            .unwrap_err();
        assert_eq!(eval_err.kind, BrowserErrorKind::HumanTakeoverActive);

        let health = rt.session_health(&k, &opts).await;
        assert_eq!(health, BrowserHealth::Healthy);
        assert_eq!(
            rt.get_takeover_state(&k).phase,
            BrowserTakeoverPhase::HumanControlled
        );

        let released = rt.release_human_takeover(&k).unwrap();
        assert_eq!(released.phase, BrowserTakeoverPhase::Resynchronizing);
        assert_eq!(released.generation, 1);

        let mutate_err = rt
            .act(&k, &opts, &BrowserAction::Click { target: css() })
            .await
            .unwrap_err();
        assert_eq!(mutate_err.kind, BrowserErrorKind::StaleAssumptions);
        assert!(mutate_err.detail.contains("Observe the current browser state"));

        rt.observe(&k, &opts, &ObserveRequest::snapshot())
            .await
            .unwrap();
        assert_eq!(
            rt.get_takeover_state(&k).phase,
            BrowserTakeoverPhase::AgentControlled
        );
        rt.act(&k, &opts, &BrowserAction::Click { target: css() })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn takeover_waits_for_in_flight_observe_then_blocks_new_ops() {
        let backend = Arc::new(FakeBackend::new());
        let rt = Arc::new(runtime(backend.clone()));
        let k = key("takeover-inflight-read");
        let opts = headed_opts();
        backend.enable_observe_hold();
        let started = backend.observe_started.notified();
        let task_rt = Arc::clone(&rt);
        let task_k = k.clone();
        let task_opts = opts.clone();
        let inflight = tokio::spawn(async move {
            task_rt
                .observe(&task_k, &task_opts, &ObserveRequest::snapshot())
                .await
        });
        tokio::time::timeout(Duration::from_secs(2), started)
            .await
            .expect("in-flight observe should start");

        let pending = rt
            .request_human_takeover(&k, &opts, TakeoverReason::Captcha)
            .unwrap();
        assert_eq!(pending.phase, BrowserTakeoverPhase::TakeoverRequested);
        assert_eq!(pending.in_flight, 1);

        let blocked = rt
            .observe(&k, &opts, &ObserveRequest::snapshot())
            .await
            .unwrap_err();
        assert_eq!(blocked.kind, BrowserErrorKind::HumanTakeoverActive);

        backend.release_observe_hold();
        inflight.await.unwrap().unwrap();
        assert_eq!(
            rt.get_takeover_state(&k).phase,
            BrowserTakeoverPhase::HumanControlled
        );
        assert_eq!(rt.get_takeover_state(&k).in_flight, 0);
    }

    #[tokio::test]
    async fn takeover_waits_for_in_flight_mutation_and_error_decrements() {
        let backend = Arc::new(FakeBackend::new());
        let rt = Arc::new(runtime(backend.clone()));
        let k = key("takeover-inflight-act");
        let opts = headed_opts();
        backend.enable_act_hold();
        let started = backend.act_started.notified();
        let task_rt = Arc::clone(&rt);
        let task_k = k.clone();
        let task_opts = opts.clone();
        let inflight = tokio::spawn(async move {
            task_rt
                .act(
                    &task_k,
                    &task_opts,
                    &BrowserAction::Click { target: css() },
                )
                .await
        });
        tokio::time::timeout(Duration::from_secs(2), started)
            .await
            .expect("in-flight act should start");
        rt.request_human_takeover(&k, &opts, TakeoverReason::Mfa)
            .unwrap();
        backend.release_act_hold();
        inflight.await.unwrap().unwrap();
        assert_eq!(
            rt.get_takeover_state(&k).phase,
            BrowserTakeoverPhase::HumanControlled
        );

        let timeout_backend = Arc::new(FakeBackend::new());
        let timeout_rt = Arc::new(runtime(timeout_backend.clone()));
        let timeout_key = key("takeover-timeout-inflight");
        timeout_backend.enable_observe_hold();
        timeout_backend.fail_next(CallKind::Observe, BrowserErrorKind::Timeout, 1);
        let timeout_started = timeout_backend.observe_started.notified();
        let timeout_task_rt = Arc::clone(&timeout_rt);
        let timeout_task_key = timeout_key.clone();
        let timeout_opts = headed_opts();
        let timeout_join = tokio::spawn(async move {
            timeout_task_rt
                .observe(
                    &timeout_task_key,
                    &timeout_opts,
                    &ObserveRequest::snapshot(),
                )
                .await
        });
        tokio::time::timeout(Duration::from_secs(2), timeout_started)
            .await
            .unwrap();
        timeout_rt
            .request_human_takeover(&timeout_key, &headed_opts(), TakeoverReason::Unknown)
            .unwrap();
        timeout_backend.release_observe_hold();
        let timeout_err = timeout_join.await.unwrap().unwrap_err();
        assert_eq!(timeout_err.kind, BrowserErrorKind::Timeout);
        assert_eq!(timeout_rt.get_takeover_state(&timeout_key).in_flight, 0);
        assert_eq!(
            timeout_rt.get_takeover_state(&timeout_key).phase,
            BrowserTakeoverPhase::HumanControlled
        );
    }

    #[tokio::test]
    async fn crash_during_takeover_requested_is_browser_lost_not_recovered() {
        let backend = Arc::new(FakeBackend::new());
        let rt = Arc::new(runtime(backend.clone()));
        let k = key("takeover-lost");
        let opts = headed_opts();
        backend.enable_observe_hold();
        backend.fail_next(CallKind::Observe, BrowserErrorKind::Crashed, 1);
        let started = backend.observe_started.notified();
        let task_rt = Arc::clone(&rt);
        let task_k = k.clone();
        let task_opts = opts.clone();
        let inflight = tokio::spawn(async move {
            task_rt
                .observe(&task_k, &task_opts, &ObserveRequest::snapshot())
                .await
        });
        tokio::time::timeout(Duration::from_secs(2), started)
            .await
            .unwrap();
        rt.request_human_takeover(&k, &opts, TakeoverReason::Unknown)
            .unwrap();
        backend.release_observe_hold();
        let err = inflight.await.unwrap().unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::Crashed);
        assert_eq!(
            rt.get_takeover_state(&k).phase,
            BrowserTakeoverPhase::BrowserLost
        );
        assert_eq!(backend.call_count(CallKind::Observe), 1);

        let lost = rt
            .act(&k, &opts, &BrowserAction::Click { target: css() })
            .await
            .unwrap_err();
        assert_eq!(lost.kind, BrowserErrorKind::TakeoverBrowserLost);
        assert!(!lost.retryable);
        let release_err = rt.release_human_takeover(&k).unwrap_err();
        assert_eq!(release_err.kind, BrowserErrorKind::TakeoverBrowserLost);
        assert_eq!(
            rt.get_takeover_state(&k).phase,
            BrowserTakeoverPhase::AgentControlled
        );
    }

    #[tokio::test]
    async fn health_probe_during_human_control_can_surface_browser_lost() {
        let backend = Arc::new(FakeBackend::new());
        let rt = runtime(backend.clone());
        let k = key("takeover-health-lost");
        let opts = headed_opts();
        rt.open(
            &k,
            &opts,
            &NavigateRequest {
                url: "https://example.com/".into(),
            },
        )
        .await
        .unwrap();
        rt.request_human_takeover(&k, &opts, TakeoverReason::Unknown)
            .unwrap();
        backend.mark_unhealthy(k.as_str());
        let health = rt.session_health(&k, &opts).await;
        assert!(matches!(health, BrowserHealth::Unhealthy { .. }));
        assert_eq!(
            rt.get_takeover_state(&k).phase,
            BrowserTakeoverPhase::BrowserLost
        );
    }

    #[tokio::test]
    async fn headless_takeover_is_typed_failure_without_relaunch() {
        let backend = Arc::new(FakeBackend::new());
        let rt = runtime(backend.clone());
        let k = key("takeover-headless");
        rt.open(
            &k,
            &opts(),
            &NavigateRequest {
                url: "https://example.com/".into(),
            },
        )
        .await
        .unwrap();
        let opens = backend.call_count(CallKind::OpenSession);
        let err = rt
            .request_human_takeover(&k, &opts(), TakeoverReason::ExplicitUserRequest)
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::TakeoverUnsupportedHeadless);
        assert!(!err.retryable);
        assert_eq!(backend.call_count(CallKind::OpenSession), opens);
        assert_eq!(
            rt.get_takeover_state(&k).phase,
            BrowserTakeoverPhase::AgentControlled
        );
        rt.observe(&k, &opts(), &ObserveRequest::snapshot())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn recorded_headed_session_accepts_takeover_even_if_request_opts_are_headless() {
        let backend = Arc::new(FakeBackend::new());
        let rt = runtime(backend);
        let k = key("takeover-recorded-headed");
        rt.open(
            &k,
            &headed_opts(),
            &NavigateRequest {
                url: "https://example.com/".into(),
            },
        )
        .await
        .unwrap();
        let state = rt
            .request_human_takeover(&k, &opts(), TakeoverReason::ExplicitUserRequest)
            .unwrap();
        assert_eq!(state.phase, BrowserTakeoverPhase::HumanControlled);
    }

    #[tokio::test]
    async fn timeout_does_not_return_agent_ownership() {
        let backend = Arc::new(FakeBackend::new());
        let rt = runtime(backend);
        let k = key("takeover-timeout");
        let opts = headed_opts();
        rt.request_human_takeover(&k, &opts, TakeoverReason::Mfa)
            .unwrap();
        let timed = rt.force_human_takeover_timeout(&k).unwrap();
        assert_eq!(timed.phase, BrowserTakeoverPhase::TimedOut);
        let blocked = rt
            .observe(&k, &opts, &ObserveRequest::snapshot())
            .await
            .unwrap_err();
        assert_eq!(blocked.kind, BrowserErrorKind::HumanTakeoverActive);
        let released = rt.release_human_takeover(&k).unwrap();
        assert_eq!(released.phase, BrowserTakeoverPhase::Resynchronizing);
        assert_eq!(released.generation, 1);
    }

    #[tokio::test]
    async fn cancel_semantics_depend_on_whether_human_was_granted() {
        let backend = Arc::new(FakeBackend::new());
        let rt = Arc::new(runtime(backend.clone()));
        let k = key("takeover-cancel-pending");
        let opts = headed_opts();
        backend.enable_observe_hold();
        let started = backend.observe_started.notified();
        let task_rt = Arc::clone(&rt);
        let task_k = k.clone();
        let task_opts = opts.clone();
        let inflight = tokio::spawn(async move {
            task_rt
                .observe(&task_k, &task_opts, &ObserveRequest::snapshot())
                .await
        });
        tokio::time::timeout(Duration::from_secs(2), started)
            .await
            .unwrap();
        rt.request_human_takeover(&k, &opts, TakeoverReason::Unknown)
            .unwrap();
        let cancelled = rt.cancel_human_takeover(&k).unwrap();
        assert_eq!(cancelled.phase, BrowserTakeoverPhase::AgentControlled);
        assert_eq!(cancelled.generation, 0);
        backend.release_observe_hold();
        inflight.await.unwrap().unwrap();

        let granted = rt
            .request_human_takeover(&k, &opts, TakeoverReason::Unknown)
            .unwrap();
        assert_eq!(granted.phase, BrowserTakeoverPhase::HumanControlled);
        let after_human = rt.cancel_human_takeover(&k).unwrap();
        assert_eq!(after_human.phase, BrowserTakeoverPhase::Resynchronizing);
        assert_eq!(after_human.generation, 1);
    }

    #[tokio::test]
    async fn close_during_human_controlled_clears_registry() {
        let backend = Arc::new(FakeBackend::new());
        let rt = runtime(backend.clone());
        let k = key("takeover-close");
        let opts = headed_opts();
        rt.open(
            &k,
            &opts,
            &NavigateRequest {
                url: "https://example.com/".into(),
            },
        )
        .await
        .unwrap();
        rt.request_human_takeover(&k, &opts, TakeoverReason::Unknown)
            .unwrap();
        rt.close_session(&k, &opts).await.unwrap();
        assert_eq!(
            rt.get_takeover_state(&k).phase,
            BrowserTakeoverPhase::AgentControlled
        );
        assert_eq!(rt.get_takeover_state(&k).generation, 0);
        rt.close_session(&k, &opts).await.unwrap();
        assert_eq!(backend.call_count(CallKind::Close), 2);
    }

    #[tokio::test]
    async fn multiple_takeover_cycles_increment_generation_on_same_session() {
        let backend = Arc::new(FakeBackend::new());
        let rt = runtime(backend);
        let k = key("takeover-cycles");
        let opts = headed_opts();
        for expected in 1..=3 {
            rt.request_human_takeover(&k, &opts, TakeoverReason::ExplicitUserRequest)
                .unwrap();
            let released = rt.release_human_takeover(&k).unwrap();
            assert_eq!(released.generation, expected);
            rt.observe(&k, &opts, &ObserveRequest::snapshot())
                .await
                .unwrap();
            assert_eq!(
                rt.get_takeover_state(&k).phase,
                BrowserTakeoverPhase::AgentControlled
            );
        }
    }

    #[test]
    fn tool_schema_does_not_gain_takeover_actions() {
        let tool = BrowserTool::new(Vec::new(), true, false, None);
        let actions = tool.parameters_schema()["properties"]["action"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(actions.len(), 25);
        assert!(!actions.iter().any(|a| a.contains("takeover")));
        assert!(!actions.iter().any(|a| a.contains("human")));
        assert_eq!(
            actions,
            V1_TOOL_ACTIONS
                .iter()
                .map(|s| (*s).to_string())
                .collect::<Vec<_>>()
        );
    }
}
