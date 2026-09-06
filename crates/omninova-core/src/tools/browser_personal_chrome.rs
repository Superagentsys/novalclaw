//! Personal Chrome backend (B3.5-C).
//!
//! Attach-only control of an explicitly authorized tab in the user's existing
//! Chrome, over the B3.5-B Native Messaging transport. This module never
//! launches, kills, or debugs Chrome and never reads profile databases.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::tools::browser_backend::BrowserBackend;
use crate::tools::browser_types::{
    BackendAvailability, BackendCapabilities, BackendSessionHandle, BrowserAction,
    BrowserActionResult, BrowserBackendError, BrowserBackendId, BrowserElement, BrowserElementRef,
    BrowserErrorKind, BrowserHealth, BrowserObservation, BrowserObserveKind, BrowserPageId,
    BrowserSessionKey, BrowserSessionOpenRequest, BrowserSnapshot, BrowserTab, BrowserTarget,
    NavigateRequest, ObserveRequest, ScreenshotRequest, ScreenshotResult, ScrollDirection,
};

pub const PERSONAL_CHROME_UNAVAILABLE: &str = "PersonalChromeUnavailable";
pub const EXTENSION_DISCONNECTED: &str = "ExtensionDisconnected";
pub const PERSONAL_CHROME_NOT_AUTHORIZED: &str = "PersonalChromeNotAuthorized";
pub const TAB_UNAVAILABLE: &str = "TabUnavailable";
pub const OPERATION_UNSUPPORTED: &str = "OperationUnsupported";
pub const PROTOCOL_MISMATCH: &str = "ProtocolMismatch";

const MAX_OBSERVE_ELEMENTS: usize = 80;
const MAX_ELEMENT_TEXT: usize = 400;
const MAX_SNAPSHOT_TEXT: usize = 24_000;

/// Recovery for Personal Chrome is reattach-only. Runtime may retry
/// `open_session`; this backend must never spawn Chrome.
pub const PERSONAL_RECOVERY: &str = "REATTACH_ONLY";

/// Production `browser.backend = "personal-chrome"` stays fail-closed until B3.5-D.
pub fn personal_chrome_production_enabled() -> bool {
    false
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizedTab {
    pub window_id: i32,
    pub tab_id: i32,
    pub authorization_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonalTransportView {
    pub connected: bool,
    pub protocol_version: u32,
    pub generation: u64,
    pub connection_id: Option<String>,
}

#[derive(Debug)]
pub enum PersonalChromePortError {
    Disconnected,
    ProtocolMismatch,
    Unauthorized(String),
    TabUnavailable(String),
    Unsupported(String),
    Restricted(String),
    Stale(String),
    Command { code: String, message: String },
}

#[async_trait]
pub trait PersonalChromePort: Send + Sync {
    fn availability_hint(&self) -> bool;
    async fn transport_view(&self) -> PersonalTransportView;
    async fn call(
        &self,
        operation: &str,
        session_id: &str,
        payload: Value,
    ) -> Result<Value, PersonalChromePortError>;
}

#[derive(Clone)]
struct BoundSession {
    key: BrowserSessionKey,
    token: String,
    window_id: i32,
    tab_id: i32,
    authorization_generation: u64,
    transport_generation: u64,
    connection_id: String,
}

pub struct PersonalChromeBackend {
    port: Arc<dyn PersonalChromePort>,
    sessions: Mutex<HashMap<String, BoundSession>>,
    keys: Mutex<HashMap<String, String>>,
}

impl PersonalChromeBackend {
    pub fn new(port: Arc<dyn PersonalChromePort>) -> Self {
        Self {
            port,
            sessions: Mutex::new(HashMap::new()),
            keys: Mutex::new(HashMap::new()),
        }
    }

    pub fn from_bridge(bridge: omninova_browser_host::PersonalChromeBridge) -> Self {
        Self::new(Arc::new(BridgePersonalChromePort { bridge }))
    }

    pub async fn grant_test_authorization(
        &self,
        window_id: i32,
        tab_id: i32,
    ) -> Result<AuthorizedTab, BrowserBackendError> {
        let payload = self
            .rpc(
                "",
                "authorize_tab_test_only",
                json!({ "window_id": window_id, "tab_id": tab_id }),
            )
            .await?;
        Ok(AuthorizedTab {
            window_id: payload
                .get("window_id")
                .and_then(Value::as_i64)
                .unwrap_or(window_id as i64) as i32,
            tab_id: payload
                .get("tab_id")
                .and_then(Value::as_i64)
                .unwrap_or(tab_id as i64) as i32,
            authorization_generation: payload
                .get("authorization_generation")
                .and_then(Value::as_u64)
                .unwrap_or(1),
        })
    }

    fn id_value() -> BrowserBackendId {
        BrowserBackendId::personal_chrome()
    }

    fn fail(
        kind: BrowserErrorKind,
        prefix: &str,
        detail: impl Into<String>,
    ) -> BrowserBackendError {
        let detail = detail.into();
        let text = if detail.starts_with(prefix) {
            detail
        } else if detail.is_empty() {
            format!("{prefix}: personal chrome backend")
        } else {
            format!("{prefix}: {detail}")
        };
        let mut err = BrowserBackendError::new(kind, Self::id_value(), text);
        if matches!(
            kind,
            BrowserErrorKind::Rejected
                | BrowserErrorKind::StaleReference
                | BrowserErrorKind::CommandFailed
        ) && prefix != EXTENSION_DISCONNECTED
        {
            err.retryable = false;
        }
        err
    }

    fn reject_launch_options(
        &self,
        req: &BrowserSessionOpenRequest,
    ) -> Result<(), BrowserBackendError> {
        if req.options.profile.is_some() {
            return Err(Self::fail(
                BrowserErrorKind::Rejected,
                PERSONAL_CHROME_NOT_AUTHORIZED,
                "managed profile is incompatible with attach-only Personal Chrome",
            ));
        }
        if req.options.installed_profile.is_some() {
            return Err(Self::fail(
                BrowserErrorKind::Rejected,
                PERSONAL_CHROME_NOT_AUTHORIZED,
                "installed profile snapshot is incompatible with attach-only Personal Chrome",
            ));
        }
        if req.options.cdp_url.is_some() {
            return Err(Self::fail(
                BrowserErrorKind::Rejected,
                PERSONAL_CHROME_NOT_AUTHORIZED,
                "CDP URL is forbidden for Personal Chrome",
            ));
        }
        if req.options.headless {
            return Err(Self::fail(
                BrowserErrorKind::Rejected,
                PERSONAL_CHROME_NOT_AUTHORIZED,
                "headless mode is incompatible with the user's headed Chrome",
            ));
        }
        if !req.options.attach_only {
            return Err(Self::fail(
                BrowserErrorKind::Rejected,
                PERSONAL_CHROME_NOT_AUTHORIZED,
                "Personal Chrome is attach-only; set attach_only",
            ));
        }
        Ok(())
    }

    async fn rpc(
        &self,
        session_id: &str,
        operation: &str,
        payload: Value,
    ) -> Result<Value, BrowserBackendError> {
        match self.port.call(operation, session_id, payload).await {
            Ok(value) => Ok(value),
            Err(PersonalChromePortError::Disconnected) => Err(Self::fail(
                BrowserErrorKind::NotConnected,
                EXTENSION_DISCONNECTED,
                "native messaging transport is not connected",
            )),
            Err(PersonalChromePortError::ProtocolMismatch) => Err(Self::fail(
                BrowserErrorKind::Rejected,
                PROTOCOL_MISMATCH,
                "extension protocol version is incompatible",
            )),
            Err(PersonalChromePortError::Unauthorized(detail)) => Err(Self::fail(
                BrowserErrorKind::Rejected,
                PERSONAL_CHROME_NOT_AUTHORIZED,
                detail,
            )),
            Err(PersonalChromePortError::TabUnavailable(detail)) => Err(Self::fail(
                BrowserErrorKind::SessionNotFound,
                TAB_UNAVAILABLE,
                detail,
            )),
            Err(PersonalChromePortError::Unsupported(detail)) => Err(Self::fail(
                BrowserErrorKind::CommandFailed,
                OPERATION_UNSUPPORTED,
                detail,
            )),
            Err(PersonalChromePortError::Restricted(detail)) => Err(Self::fail(
                BrowserErrorKind::Rejected,
                OPERATION_UNSUPPORTED,
                detail,
            )),
            Err(PersonalChromePortError::Stale(detail)) => Err(Self::fail(
                BrowserErrorKind::StaleReference,
                "BrowserCommandFailed",
                detail,
            )),
            Err(PersonalChromePortError::Command { code, message }) => {
                let prefix = match code.as_str() {
                    PERSONAL_CHROME_NOT_AUTHORIZED => PERSONAL_CHROME_NOT_AUTHORIZED,
                    TAB_UNAVAILABLE => TAB_UNAVAILABLE,
                    EXTENSION_DISCONNECTED => EXTENSION_DISCONNECTED,
                    PROTOCOL_MISMATCH => PROTOCOL_MISMATCH,
                    OPERATION_UNSUPPORTED => OPERATION_UNSUPPORTED,
                    _ => OPERATION_UNSUPPORTED,
                };
                let kind = match code.as_str() {
                    EXTENSION_DISCONNECTED => BrowserErrorKind::NotConnected,
                    TAB_UNAVAILABLE => BrowserErrorKind::SessionNotFound,
                    _ => BrowserErrorKind::Rejected,
                };
                Err(Self::fail(kind, prefix, message))
            }
        }
    }

    async fn require_transport(&self) -> Result<PersonalTransportView, BrowserBackendError> {
        let view = self.port.transport_view().await;
        if !view.connected {
            return Err(Self::fail(
                BrowserErrorKind::NotConnected,
                PERSONAL_CHROME_UNAVAILABLE,
                "Personal Chrome bridge is not connected",
            ));
        }
        Ok(view)
    }

    fn lookup<'a>(
        sessions: &'a HashMap<String, BoundSession>,
        session: &BackendSessionHandle,
    ) -> Result<&'a BoundSession, BrowserBackendError> {
        if session.backend() != &Self::id_value() {
            return Err(Self::fail(
                BrowserErrorKind::Rejected,
                PERSONAL_CHROME_NOT_AUTHORIZED,
                "session handle is not from personal-chrome",
            ));
        }
        sessions.get(session.token()).ok_or_else(|| {
            Self::fail(
                BrowserErrorKind::SessionNotFound,
                TAB_UNAVAILABLE,
                "backend session is not attached",
            )
        })
    }

    fn encode_ref(generation: u64, id: &str) -> BrowserElementRef {
        BrowserElementRef::new(
            Self::id_value(),
            format!("pc:{generation}:{id}"),
        )
    }

    fn decode_ref(
        &self,
        target: &BrowserTarget,
    ) -> Result<(Option<String>, Option<String>), BrowserBackendError> {
        match target {
            BrowserTarget::Element(reference) => {
                if reference.backend() != &Self::id_value() {
                    return Err(Self::fail(
                        BrowserErrorKind::Rejected,
                        PERSONAL_CHROME_NOT_AUTHORIZED,
                        "BrowserTargetBackendMismatch",
                    ));
                }
                Ok((Some(reference.as_str().to_string()), None))
            }
            BrowserTarget::Css(selector) => Ok((None, Some(selector.clone()))),
            BrowserTarget::Role { role, name } => Ok((
                None,
                Some(format!("role:{role}:{}", name.clone().unwrap_or_default())),
            )),
        }
    }

    fn observation_from_payload(
        &self,
        payload: &Value,
        kind: &BrowserObserveKind,
    ) -> BrowserObservation {
        let url = payload
            .get("url")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let title = payload
            .get("title")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let generation = payload
            .get("snapshot_generation")
            .and_then(Value::as_u64)
            .unwrap_or(1);
        let mut elements = Vec::new();
        if let Some(list) = payload.get("elements").and_then(Value::as_array) {
            for item in list.iter().take(MAX_OBSERVE_ELEMENTS) {
                let id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("0")
                    .to_string();
                let mut name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if name.len() > MAX_ELEMENT_TEXT {
                    name.truncate(MAX_ELEMENT_TEXT);
                }
                elements.push(BrowserElement {
                    reference: Self::encode_ref(generation, &id),
                    role: item
                        .get("role")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    name: if name.is_empty() { None } else { Some(name) },
                    interactive: item
                        .get("interactive")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                });
            }
        }
        let mut snapshot_text = payload
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let truncated = snapshot_text.len() > MAX_SNAPSHOT_TEXT || elements.len() >= MAX_OBSERVE_ELEMENTS;
        if snapshot_text.len() > MAX_SNAPSHOT_TEXT {
            snapshot_text.truncate(MAX_SNAPSHOT_TEXT);
        }
        let text = match kind {
            BrowserObserveKind::Url => url.clone(),
            BrowserObserveKind::Title => title.clone(),
            BrowserObserveKind::Value { .. } => payload
                .get("value")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            BrowserObserveKind::Visibility { .. } => payload
                .get("visible")
                .map(|v| v.to_string()),
            BrowserObserveKind::Enabled { .. } => payload
                .get("enabled")
                .map(|v| v.to_string()),
            _ => {
                if snapshot_text.is_empty() {
                    None
                } else {
                    Some(snapshot_text.clone())
                }
            }
        };
        BrowserObservation {
            url,
            title,
            text,
            snapshot: Some(BrowserSnapshot {
                text: snapshot_text,
                elements,
            }),
            truncated,
        }
    }
}

#[async_trait]
impl BrowserBackend for PersonalChromeBackend {
    fn id(&self) -> BrowserBackendId {
        Self::id_value()
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            navigation: true,
            observation: true,
            element_actions: true,
            tabs: true,
            screenshot: false,
            eval: false,
            attach: true,
            profiles: false,
        }
    }

    fn availability(&self) -> BackendAvailability {
        if self.port.availability_hint() {
            BackendAvailability::Available
        } else {
            BackendAvailability::Unavailable {
                kind: BrowserErrorKind::NotConnected,
                detail: format!(
                    "{PERSONAL_CHROME_UNAVAILABLE}: Personal Chrome transport is not connected"
                ),
            }
        }
    }

    async fn open_session(
        &self,
        req: &BrowserSessionOpenRequest,
    ) -> Result<BackendSessionHandle, BrowserBackendError> {
        self.reject_launch_options(req)?;
        if let Some(token) = self.keys.lock().expect("keys").get(req.key.as_str()).cloned() {
            if self.sessions.lock().expect("sessions").contains_key(&token) {
                return BackendSessionHandle::new(Self::id_value(), token).map_err(|err| {
                    Self::fail(BrowserErrorKind::Rejected, PERSONAL_CHROME_UNAVAILABLE, err.to_string())
                });
            }
        }
        let view = self.require_transport().await?;
        let listed = self.rpc("", "tab_list_authorized", json!({})).await?;
        let tabs = listed
            .get("tabs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if tabs.is_empty() {
            return Err(Self::fail(
                BrowserErrorKind::Rejected,
                PERSONAL_CHROME_NOT_AUTHORIZED,
                "no authorized tab; backend cannot attach",
            ));
        }
        if tabs.len() != 1 {
            return Err(Self::fail(
                BrowserErrorKind::Rejected,
                PERSONAL_CHROME_NOT_AUTHORIZED,
                "exactly one authorized tab is required to attach",
            ));
        }
        let tab = &tabs[0];
        let window_id = tab.get("window_id").and_then(Value::as_i64).unwrap_or(0) as i32;
        let tab_id = tab.get("tab_id").and_then(Value::as_i64).unwrap_or(0) as i32;
        let authorization_generation = tab
            .get("authorization_generation")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let attached = self
            .rpc(
                "",
                "attach_session",
                json!({
                    "window_id": window_id,
                    "tab_id": tab_id,
                    "authorization_generation": authorization_generation,
                    "logical_key": req.key.as_str(),
                }),
            )
            .await?;
        let token = attached
            .get("session_token")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("pcs:{}", Uuid::new_v4()));
        let bound = BoundSession {
            key: req.key.clone(),
            token: token.clone(),
            window_id,
            tab_id,
            authorization_generation,
            transport_generation: view.generation,
            connection_id: view.connection_id.unwrap_or_default(),
        };
        self.sessions
            .lock()
            .expect("sessions")
            .insert(token.clone(), bound);
        self.keys
            .lock()
            .expect("keys")
            .insert(req.key.as_str().to_string(), token.clone());
        BackendSessionHandle::new(Self::id_value(), token).map_err(|err| {
            Self::fail(
                BrowserErrorKind::Rejected,
                PERSONAL_CHROME_UNAVAILABLE,
                err.to_string(),
            )
        })
    }

    async fn session_health(&self, session: &BackendSessionHandle) -> BrowserHealth {
        let bound = {
            let sessions = self.sessions.lock().expect("sessions");
            match Self::lookup(&sessions, session) {
                Ok(bound) => BoundSession {
                    key: bound.key.clone(),
                    token: bound.token.clone(),
                    window_id: bound.window_id,
                    tab_id: bound.tab_id,
                    authorization_generation: bound.authorization_generation,
                    transport_generation: bound.transport_generation,
                    connection_id: bound.connection_id.clone(),
                },
                Err(err) => {
                    return BrowserHealth::Unhealthy {
                        kind: err.kind,
                        detail: err.detail,
                    };
                }
            }
        };
        let view = self.port.transport_view().await;
        if !view.connected {
            return BrowserHealth::Unhealthy {
                kind: BrowserErrorKind::NotConnected,
                detail: format!("{EXTENSION_DISCONNECTED}: transport is down"),
            };
        }
        if view.generation != bound.transport_generation {
            return BrowserHealth::Unhealthy {
                kind: BrowserErrorKind::Rejected,
                detail: format!("{PROTOCOL_MISMATCH}: extension generation changed"),
            };
        }
        match self
            .rpc(
                &bound.token,
                "session_health",
                json!({
                    "window_id": bound.window_id,
                    "tab_id": bound.tab_id,
                    "authorization_generation": bound.authorization_generation,
                }),
            )
            .await
        {
            Ok(_) => BrowserHealth::Healthy,
            Err(err) => BrowserHealth::Unhealthy {
                kind: err.kind,
                detail: err.detail,
            },
        }
    }

    async fn close_session(
        &self,
        session: &BackendSessionHandle,
    ) -> Result<(), BrowserBackendError> {
        let bound = {
            let mut sessions = self.sessions.lock().expect("sessions");
            sessions.remove(session.token())
        };
        if let Some(bound) = bound {
            self.keys.lock().expect("keys").remove(bound.key.as_str());
            let _ = self
                .rpc(
                    &bound.token,
                    "detach_session",
                    json!({
                        "window_id": bound.window_id,
                        "tab_id": bound.tab_id,
                    }),
                )
                .await;
        }
        Ok(())
    }

    async fn open(
        &self,
        session: &BackendSessionHandle,
        req: &NavigateRequest,
    ) -> Result<BrowserActionResult, BrowserBackendError> {
        let bound = {
            let sessions = self.sessions.lock().expect("sessions");
            Self::lookup(&sessions, session)?.clone()
        };
        let payload = self
            .rpc(
                &bound.token,
                "navigate",
                json!({
                    "window_id": bound.window_id,
                    "tab_id": bound.tab_id,
                    "authorization_generation": bound.authorization_generation,
                    "url": req.url,
                }),
            )
            .await?;
        Ok(BrowserActionResult {
            detail: "opened".into(),
            url: payload
                .get("url")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .or_else(|| Some(req.url.clone())),
            title: payload
                .get("title")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            structured_output: None,
        })
    }

    async fn observe(
        &self,
        session: &BackendSessionHandle,
        req: &ObserveRequest,
    ) -> Result<BrowserObservation, BrowserBackendError> {
        let bound = {
            let sessions = self.sessions.lock().expect("sessions");
            Self::lookup(&sessions, session)?.clone()
        };
        let mut payload = json!({
            "window_id": bound.window_id,
            "tab_id": bound.tab_id,
            "authorization_generation": bound.authorization_generation,
            "kind": req.kind.v1_action_name(),
            "interactive_only": req.interactive_only,
            "compact": req.compact,
        });
        if let Some(target) = observe_target(&req.kind) {
            let (reference, selector) = self.decode_ref(target)?;
            if let Some(reference) = reference {
                payload["ref"] = json!(reference);
            }
            if let Some(selector) = selector {
                payload["selector"] = json!(selector);
            }
        }
        let result = self.rpc(&bound.token, "observe", payload).await?;
        Ok(self.observation_from_payload(&result, &req.kind))
    }

    async fn act(
        &self,
        session: &BackendSessionHandle,
        action: &BrowserAction,
    ) -> Result<BrowserActionResult, BrowserBackendError> {
        if matches!(action, BrowserAction::Eval { .. }) {
            return Err(Self::fail(
                BrowserErrorKind::CommandFailed,
                OPERATION_UNSUPPORTED,
                "eval and page_eval are unsupported in Personal Chrome v1",
            ));
        }
        let bound = {
            let sessions = self.sessions.lock().expect("sessions");
            Self::lookup(&sessions, session)?.clone()
        };
        let mut payload = json!({
            "window_id": bound.window_id,
            "tab_id": bound.tab_id,
            "authorization_generation": bound.authorization_generation,
            "action": action.name(),
        });
        match action {
            BrowserAction::Click { target }
            | BrowserAction::Hover { target }
            | BrowserAction::Fill { target, .. }
            | BrowserAction::Type { target, .. }
            | BrowserAction::Select { target, .. } => {
                let (reference, selector) = self.decode_ref(target)?;
                if let Some(reference) = reference {
                    payload["ref"] = json!(reference);
                }
                if let Some(selector) = selector {
                    payload["selector"] = json!(selector);
                }
            }
            BrowserAction::Scroll { target, direction, pixels } => {
                if let Some(target) = target {
                    let (reference, selector) = self.decode_ref(target)?;
                    if let Some(reference) = reference {
                        payload["ref"] = json!(reference);
                    }
                    if let Some(selector) = selector {
                        payload["selector"] = json!(selector);
                    }
                }
                payload["direction"] = json!(match direction {
                    ScrollDirection::Up => "up",
                    ScrollDirection::Down => "down",
                    ScrollDirection::Left => "left",
                    ScrollDirection::Right => "right",
                });
                payload["pixels"] = json!(pixels);
            }
            BrowserAction::Press { key } => {
                payload["key"] = json!(key);
            }
            BrowserAction::Wait {
                timeout_ms,
                text,
                target,
            } => {
                payload["timeout_ms"] = json!(timeout_ms);
                payload["text"] = json!(text);
                if let Some(target) = target {
                    let (reference, selector) = self.decode_ref(target)?;
                    if let Some(reference) = reference {
                        payload["ref"] = json!(reference);
                    }
                    if let Some(selector) = selector {
                        payload["selector"] = json!(selector);
                    }
                }
            }
            BrowserAction::Back | BrowserAction::Forward | BrowserAction::Reload => {}
            BrowserAction::Eval { .. } => unreachable!(),
        }
        if let BrowserAction::Fill { value, .. } = action {
            payload["value"] = json!(value);
        }
        if let BrowserAction::Type { text, .. } = action {
            payload["text"] = json!(text);
        }
        if let BrowserAction::Select { value, .. } = action {
            payload["value"] = json!(value);
        }
        let result = self.rpc(&bound.token, "act", payload).await?;
        Ok(BrowserActionResult {
            detail: action.name().into(),
            url: result
                .get("url")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            title: result
                .get("title")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            structured_output: None,
        })
    }

    async fn screenshot(
        &self,
        _session: &BackendSessionHandle,
        _req: &ScreenshotRequest,
    ) -> Result<ScreenshotResult, BrowserBackendError> {
        Err(Self::fail(
            BrowserErrorKind::CommandFailed,
            OPERATION_UNSUPPORTED,
            "screenshot requires host permission beyond the B3.5-C v1 model; captureVisibleTab is not enabled",
        ))
    }

    async fn tabs(
        &self,
        session: &BackendSessionHandle,
    ) -> Result<Vec<BrowserTab>, BrowserBackendError> {
        let bound = {
            let sessions = self.sessions.lock().expect("sessions");
            Self::lookup(&sessions, session)?.clone()
        };
        let listed = self
            .rpc(
                &bound.token,
                "tab_list_authorized",
                json!({
                    "window_id": bound.window_id,
                    "tab_id": bound.tab_id,
                    "authorization_generation": bound.authorization_generation,
                }),
            )
            .await?;
        let mut out = Vec::new();
        if let Some(tabs) = listed.get("tabs").and_then(Value::as_array) {
            for tab in tabs {
                let tab_id = tab.get("tab_id").and_then(Value::as_i64).unwrap_or(0);
                out.push(BrowserTab {
                    id: BrowserPageId::new(tab_id.to_string()).map_err(|err| {
                        Self::fail(BrowserErrorKind::Rejected, TAB_UNAVAILABLE, err.to_string())
                    })?,
                    url: tab
                        .get("url")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    title: tab
                        .get("title")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    active: tab.get("tab_id").and_then(Value::as_i64) == Some(bound.tab_id as i64),
                });
            }
        }
        Ok(out)
    }
}

fn observe_target(kind: &BrowserObserveKind) -> Option<&BrowserTarget> {
    match kind {
        BrowserObserveKind::Text { target }
        | BrowserObserveKind::Html { target } => target.as_ref(),
        BrowserObserveKind::Value { target }
        | BrowserObserveKind::Visibility { target }
        | BrowserObserveKind::Enabled { target } => Some(target),
        _ => None,
    }
}

struct BridgePersonalChromePort {
    bridge: omninova_browser_host::PersonalChromeBridge,
}

#[async_trait]
impl PersonalChromePort for BridgePersonalChromePort {
    fn availability_hint(&self) -> bool {
        true
    }

    async fn transport_view(&self) -> PersonalTransportView {
        let status = self.bridge.status().await;
        PersonalTransportView {
            connected: status.connected,
            protocol_version: status.protocol_version,
            generation: status.generation,
            connection_id: None,
        }
    }

    async fn call(
        &self,
        operation: &str,
        session_id: &str,
        payload: Value,
    ) -> Result<Value, PersonalChromePortError> {
        let response = self
            .bridge
            .request(operation, session_id, payload)
            .await
            .map_err(|err| match err {
                omninova_browser_host::BridgeError::Disconnected => {
                    PersonalChromePortError::Disconnected
                }
                omninova_browser_host::BridgeError::ProtocolMismatch { .. } => {
                    PersonalChromePortError::ProtocolMismatch
                }
                other => PersonalChromePortError::Command {
                    code: other.code().to_string(),
                    message: other.to_string(),
                },
            })?;
        if !response.ok {
            let code = response
                .error
                .as_ref()
                .map(|e| e.code.clone())
                .unwrap_or_else(|| OPERATION_UNSUPPORTED.to_string());
            let message = response
                .error
                .as_ref()
                .map(|e| e.message.clone())
                .unwrap_or_default();
            return Err(match code.as_str() {
                EXTENSION_DISCONNECTED => PersonalChromePortError::Disconnected,
                PROTOCOL_MISMATCH => PersonalChromePortError::ProtocolMismatch,
                PERSONAL_CHROME_NOT_AUTHORIZED => PersonalChromePortError::Unauthorized(message),
                TAB_UNAVAILABLE => PersonalChromePortError::TabUnavailable(message),
                OPERATION_UNSUPPORTED => PersonalChromePortError::Unsupported(message),
                _ => PersonalChromePortError::Command { code, message },
            });
        }
        Ok(response.payload.unwrap_or(Value::Null))
    }
}

#[derive(Clone)]
struct MockElement {
    id: String,
    role: String,
    name: String,
    selector: String,
    input_type: String,
    value: String,
    interactive: bool,
}

struct MockPage {
    window_id: i32,
    tab_id: i32,
    url: String,
    title: String,
    snapshot_generation: u64,
    elements: Vec<MockElement>,
    exists: bool,
}

struct MockInner {
    configured: bool,
    connected: bool,
    protocol_ok: bool,
    generation: u64,
    connection_id: String,
    auth_generation: u64,
    authorized: HashMap<i32, AuthorizedTab>,
    pages: HashMap<i32, MockPage>,
    attached: HashMap<String, i32>,
    chrome_alive: bool,
}

pub struct MockPersonalChromePort {
    inner: Mutex<MockInner>,
    launch_attempts: AtomicU32,
    last_password_logged: Mutex<Option<String>>,
}

impl MockPersonalChromePort {
    pub fn connected_fixture() -> Arc<Self> {
        let page = MockPage {
            window_id: 1,
            tab_id: 11,
            url: "https://example.test/form".into(),
            title: "Form".into(),
            snapshot_generation: 1,
            exists: true,
            elements: vec![
                MockElement {
                    id: "1".into(),
                    role: "textbox".into(),
                    name: "user".into(),
                    selector: "#user".into(),
                    input_type: "text".into(),
                    value: String::new(),
                    interactive: true,
                },
                MockElement {
                    id: "2".into(),
                    role: "textbox".into(),
                    name: "password".into(),
                    selector: "#password".into(),
                    input_type: "password".into(),
                    value: "super-secret-password".into(),
                    interactive: true,
                },
                MockElement {
                    id: "3".into(),
                    role: "button".into(),
                    name: "Go".into(),
                    selector: "#go".into(),
                    input_type: String::new(),
                    value: String::new(),
                    interactive: true,
                },
            ],
        };
        let other = MockPage {
            window_id: 1,
            tab_id: 99,
            url: "https://bank.test/inbox".into(),
            title: "Bank".into(),
            snapshot_generation: 1,
            exists: true,
            elements: vec![MockElement {
                id: "9".into(),
                role: "link".into(),
                name: "secret mail".into(),
                selector: "a".into(),
                input_type: String::new(),
                value: String::new(),
                interactive: true,
            }],
        };
        Arc::new(Self {
            inner: Mutex::new(MockInner {
                configured: true,
                connected: true,
                protocol_ok: true,
                generation: 2,
                connection_id: "conn:mock".into(),
                auth_generation: 0,
                authorized: HashMap::new(),
                pages: HashMap::from([(11, page), (99, other)]),
                attached: HashMap::new(),
                chrome_alive: true,
            }),
            launch_attempts: AtomicU32::new(0),
            last_password_logged: Mutex::new(None),
        })
    }

    pub fn disconnected_fixture() -> Arc<Self> {
        let port = Self::connected_fixture();
        {
            let mut inner = port.inner.lock().expect("mock");
            inner.connected = false;
            inner.configured = false;
        }
        port
    }

    pub fn launch_attempts(&self) -> u32 {
        self.launch_attempts.load(Ordering::SeqCst)
    }

    pub fn chrome_alive(&self) -> bool {
        self.inner.lock().expect("mock").chrome_alive
    }

    pub fn password_was_logged(&self) -> bool {
        self.last_password_logged
            .lock()
            .expect("log")
            .as_deref()
            == Some("super-secret-password")
    }

    pub fn disconnect(&self) {
        self.inner.lock().expect("mock").connected = false;
    }

    pub fn reconnect_same_generation(&self) {
        let mut inner = self.inner.lock().expect("mock");
        inner.connected = true;
    }

    pub fn bump_generation_and_drop_auth(&self) {
        let mut inner = self.inner.lock().expect("mock");
        inner.generation = inner.generation.saturating_add(1);
        inner.authorized.clear();
        inner.connected = true;
    }

    pub fn force_protocol_mismatch(&self) {
        self.inner.lock().expect("mock").protocol_ok = false;
    }

    pub fn close_authorized_tab(&self) {
        let mut inner = self.inner.lock().expect("mock");
        if let Some(page) = inner.pages.get_mut(&11) {
            page.exists = false;
        }
        inner.authorized.remove(&11);
    }

    pub fn revoke_authorization(&self) {
        self.inner.lock().expect("mock").authorized.clear();
    }

    fn dispatch_locked(
        inner: &mut MockInner,
        operation: &str,
        payload: &Value,
    ) -> Result<Value, PersonalChromePortError> {
        if !inner.protocol_ok {
            return Err(PersonalChromePortError::ProtocolMismatch);
        }
        if !inner.connected {
            return Err(PersonalChromePortError::Disconnected);
        }
        match operation {
            "authorize_tab_test_only" => {
                let tab_id = payload.get("tab_id").and_then(Value::as_i64).unwrap_or(0) as i32;
                let window_id = payload
                    .get("window_id")
                    .and_then(Value::as_i64)
                    .unwrap_or(1) as i32;
                if !inner.pages.get(&tab_id).map(|p| p.exists).unwrap_or(false) {
                    return Err(PersonalChromePortError::TabUnavailable(
                        "tab does not exist".into(),
                    ));
                }
                inner.auth_generation = inner.auth_generation.saturating_add(1);
                let grant = AuthorizedTab {
                    window_id,
                    tab_id,
                    authorization_generation: inner.auth_generation,
                };
                inner.authorized.insert(tab_id, grant.clone());
                Ok(json!({
                    "window_id": window_id,
                    "tab_id": tab_id,
                    "authorization_generation": grant.authorization_generation,
                }))
            }
            "tab_list_authorized" => {
                let tabs: Vec<Value> = inner
                    .authorized
                    .values()
                    .filter_map(|grant| {
                        let page = inner.pages.get(&grant.tab_id)?;
                        if !page.exists {
                            return None;
                        }
                        Some(json!({
                            "window_id": grant.window_id,
                            "tab_id": grant.tab_id,
                            "authorization_generation": grant.authorization_generation,
                            "url": page.url,
                            "title": page.title,
                        }))
                    })
                    .collect();
                Ok(json!({ "tabs": tabs }))
            }
            "attach_session" => {
                let tab_id = payload.get("tab_id").and_then(Value::as_i64).unwrap_or(0) as i32;
                let gen = payload
                    .get("authorization_generation")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let grant = inner.authorized.get(&tab_id).ok_or_else(|| {
                    PersonalChromePortError::Unauthorized("tab is not authorized".into())
                })?;
                if grant.authorization_generation != gen {
                    return Err(PersonalChromePortError::Unauthorized(
                        "authorization generation is no longer valid".into(),
                    ));
                }
                let token = format!("pcs:{}", Uuid::new_v4());
                inner.attached.insert(token.clone(), tab_id);
                Ok(json!({ "session_token": token, "tab_id": tab_id }))
            }
            "detach_session" => {
                inner.attached.clear();
                Ok(json!({ "detached": true, "chrome_alive": inner.chrome_alive }))
            }
            "session_health" => health_locked(inner, payload),
            "observe" => observe_locked(inner, payload),
            "act" => act_locked(inner, payload),
            "navigate" => navigate_locked(inner, payload),
            "tab_get" => {
                require_authorized(inner, payload)?;
                let tab_id = payload.get("tab_id").and_then(Value::as_i64).unwrap_or(0) as i32;
                let page = inner.pages.get(&tab_id).ok_or_else(|| {
                    PersonalChromePortError::TabUnavailable("missing tab".into())
                })?;
                Ok(json!({ "url": page.url, "title": page.title, "tab_id": tab_id }))
            }
            "screenshot" | "eval" | "cookies" | "storage_get" | "raw_eval" | "debugger" => {
                Err(PersonalChromePortError::Unsupported(format!(
                    "{operation} is not available"
                )))
            }
            _ => Err(PersonalChromePortError::Unsupported(format!(
                "unknown operation {operation}"
            ))),
        }
    }
}

fn require_authorized<'a>(
    inner: &'a MockInner,
    payload: &Value,
) -> Result<&'a AuthorizedTab, PersonalChromePortError> {
    let tab_id = payload.get("tab_id").and_then(Value::as_i64).unwrap_or(0) as i32;
    let gen = payload
        .get("authorization_generation")
        .and_then(Value::as_u64);
    let grant = inner.authorized.get(&tab_id).ok_or_else(|| {
        PersonalChromePortError::Unauthorized("tab is not authorized".into())
    })?;
    if let Some(gen) = gen {
        if grant.authorization_generation != gen {
            return Err(PersonalChromePortError::Unauthorized(
                "authorization generation is no longer valid".into(),
            ));
        }
    }
    let page = inner.pages.get(&tab_id).ok_or_else(|| {
        PersonalChromePortError::TabUnavailable("tab does not exist".into())
    })?;
    if !page.exists {
        return Err(PersonalChromePortError::TabUnavailable(
            "authorized tab was closed".into(),
        ));
    }
    if is_restricted_url(&page.url) {
        return Err(PersonalChromePortError::Restricted(
            "chrome restricted pages cannot be observed".into(),
        ));
    }
    Ok(grant)
}

fn health_locked(inner: &MockInner, payload: &Value) -> Result<Value, PersonalChromePortError> {
    require_authorized(inner, payload)?;
    Ok(json!({
        "healthy": true,
        "generation": inner.generation,
        "connection_id": inner.connection_id,
    }))
}

fn observe_locked(inner: &MockInner, payload: &Value) -> Result<Value, PersonalChromePortError> {
    require_authorized(inner, payload)?;
    let tab_id = payload.get("tab_id").and_then(Value::as_i64).unwrap_or(0) as i32;
    let page = inner.pages.get(&tab_id).unwrap();
    let kind = payload.get("kind").and_then(Value::as_str).unwrap_or("snapshot");
    if kind == "get_value" {
        if let Some(el) = find_element(page, payload) {
            let value = if el.input_type == "password" {
                String::new()
            } else {
                el.value.clone()
            };
            return Ok(json!({
                "url": page.url,
                "title": page.title,
                "value": value,
                "snapshot_generation": page.snapshot_generation,
                "elements": [],
                "text": "",
            }));
        }
    }
    let elements: Vec<Value> = page
        .elements
        .iter()
        .map(|el| {
            json!({
                "id": el.id,
                "role": el.role,
                "name": el.name,
                "interactive": el.interactive,
                "input_type": el.input_type,
            })
        })
        .collect();
    let text = page
        .elements
        .iter()
        .map(|el| {
            if el.input_type == "password" {
                format!("{} [password]", el.name)
            } else {
                el.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(json!({
        "url": page.url,
        "title": page.title,
        "text": text,
        "snapshot_generation": page.snapshot_generation,
        "elements": elements,
    }))
}

fn act_locked(inner: &mut MockInner, payload: &Value) -> Result<Value, PersonalChromePortError> {
    require_authorized(inner, payload)?;
    let tab_id = payload.get("tab_id").and_then(Value::as_i64).unwrap_or(0) as i32;
    let action = payload.get("action").and_then(Value::as_str).unwrap_or("");
    if action == "eval" {
        return Err(PersonalChromePortError::Unsupported(
            "eval is unsupported".into(),
        ));
    }
    let page = inner.pages.get_mut(&tab_id).unwrap();
    if let Some(reference) = payload.get("ref").and_then(Value::as_str) {
        let expected = format!("pc:{}:", page.snapshot_generation);
        if reference.starts_with("pc:") && !reference.starts_with(&expected) {
            return Err(PersonalChromePortError::Stale(
                "element reference is stale".into(),
            ));
        }
    }
    if matches!(action, "fill" | "type") {
        if let Some(el) = find_element_mut(page, payload) {
            let incoming = payload
                .get("value")
                .or_else(|| payload.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("");
            el.value = incoming.to_string();
        }
    }
    if action == "click" {
        if let Some(el) = find_element(page, payload) {
            if el.id == "3" {
                page.title = "Clicked".into();
            }
        }
    }
    Ok(json!({ "url": page.url, "title": page.title }))
}

fn navigate_locked(
    inner: &mut MockInner,
    payload: &Value,
) -> Result<Value, PersonalChromePortError> {
    require_authorized(inner, payload)?;
    let tab_id = payload.get("tab_id").and_then(Value::as_i64).unwrap_or(0) as i32;
    let url = payload
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if is_restricted_url(&url) {
        return Err(PersonalChromePortError::Restricted(
            "cannot navigate the authorized tab to a restricted URL".into(),
        ));
    }
    let page = inner.pages.get_mut(&tab_id).unwrap();
    page.url = url.clone();
    page.title = "Navigated".into();
    page.snapshot_generation = page.snapshot_generation.saturating_add(1);
    Ok(json!({ "url": page.url, "title": page.title, "tab_id": tab_id }))
}

fn find_element<'a>(page: &'a MockPage, payload: &Value) -> Option<&'a MockElement> {
    if let Some(reference) = payload.get("ref").and_then(Value::as_str) {
        let id = reference.rsplit(':').next()?;
        return page.elements.iter().find(|el| el.id == id);
    }
    if let Some(selector) = payload.get("selector").and_then(Value::as_str) {
        return page.elements.iter().find(|el| el.selector == selector);
    }
    page.elements.first()
}

fn find_element_mut<'a>(page: &'a mut MockPage, payload: &Value) -> Option<&'a mut MockElement> {
    if let Some(reference) = payload.get("ref").and_then(Value::as_str) {
        let id = reference.rsplit(':').next()?.to_string();
        return page.elements.iter_mut().find(|el| el.id == id);
    }
    if let Some(selector) = payload.get("selector").and_then(Value::as_str) {
        let selector = selector.to_string();
        return page.elements.iter_mut().find(|el| el.selector == selector);
    }
    page.elements.first_mut()
}

pub fn is_restricted_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("chrome://")
        || lower.starts_with("chrome-extension://")
        || lower.contains("chromewebstore.google.com")
        || lower.starts_with("edge://")
        || lower.starts_with("about:")
}

#[async_trait]
impl PersonalChromePort for MockPersonalChromePort {
    fn availability_hint(&self) -> bool {
        self.inner.lock().expect("mock").configured
    }

    async fn transport_view(&self) -> PersonalTransportView {
        let inner = self.inner.lock().expect("mock");
        PersonalTransportView {
            connected: inner.connected,
            protocol_version: 1,
            generation: inner.generation,
            connection_id: Some(inner.connection_id.clone()),
        }
    }

    async fn call(
        &self,
        operation: &str,
        _session_id: &str,
        payload: Value,
    ) -> Result<Value, PersonalChromePortError> {
        self.launch_attempts.store(0, Ordering::SeqCst);
        let mut inner = self.inner.lock().expect("mock");
        Self::dispatch_locked(&mut inner, operation, &payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::browser_control::TakeoverReason;
    use crate::tools::browser_runtime::{BrowserRuntime, BrowserRuntimePolicy};
    use crate::tools::browser_types::{BrowserSessionKey, BrowserSessionOptions};
    use crate::tools::traits::Tool;
    use crate::tools::BrowserTool;

    fn attach_opts() -> BrowserSessionOptions {
        BrowserSessionOptions {
            headless: false,
            attach_only: true,
            cdp_url: None,
            profile: None,
            installed_profile: None,
        }
    }

    fn open_req(key: &str) -> BrowserSessionOpenRequest {
        BrowserSessionOpenRequest::new(BrowserSessionKey::new(key).unwrap(), attach_opts())
    }

    async fn attached_backend() -> (PersonalChromeBackend, Arc<MockPersonalChromePort>) {
        let port = MockPersonalChromePort::connected_fixture();
        let backend = PersonalChromeBackend::new(port.clone());
        backend.grant_test_authorization(1, 11).await.unwrap();
        (backend, port)
    }

    #[test]
    fn backend_id_and_capabilities_match_v1() {
        let port = MockPersonalChromePort::disconnected_fixture();
        let backend = PersonalChromeBackend::new(port);
        assert_eq!(backend.id(), BrowserBackendId::personal_chrome());
        let caps = backend.capabilities();
        assert!(caps.attach);
        assert!(caps.navigation);
        assert!(caps.observation);
        assert!(caps.element_actions);
        assert!(caps.tabs);
        assert!(!caps.eval);
        assert!(!caps.profiles);
        assert!(!caps.screenshot);
        assert!(!caps.supports_action(&BrowserAction::eval_raw("1")));
        assert_eq!(PERSONAL_RECOVERY, "REATTACH_ONLY");
        assert!(!personal_chrome_production_enabled());
    }

    #[test]
    fn availability_disconnected_is_not_session_health() {
        let backend = PersonalChromeBackend::new(MockPersonalChromePort::disconnected_fixture());
        assert!(matches!(
            backend.availability(),
            BackendAvailability::Unavailable {
                kind: BrowserErrorKind::NotConnected,
                ..
            }
        ));
    }

    #[test]
    fn source_never_launches_or_enables_debugger() {
        let src = include_str!("browser_personal_chrome.rs");
        let impl_src = src.split("mod tests").next().expect("tests module");
        assert!(!impl_src.contains("Command::new"));
        assert!(!impl_src.contains("remote-debugging"));
        assert!(!impl_src.contains("chrome.debugger"));
        assert!(!impl_src.contains("User Data\\\\Default"));
        assert!(!impl_src.contains("SingletonLock"));
    }

    #[tokio::test]
    async fn attach_requires_authorized_tab() {
        let port = MockPersonalChromePort::connected_fixture();
        let backend = PersonalChromeBackend::new(port.clone());
        let err = backend.open_session(&open_req("chat-1")).await.unwrap_err();
        assert!(err.detail.contains(PERSONAL_CHROME_NOT_AUTHORIZED));
        assert_eq!(port.launch_attempts(), 0);
    }

    #[tokio::test]
    async fn open_session_is_attach_only_and_never_launches_chrome() {
        let (backend, port) = attached_backend().await;
        let handle = backend.open_session(&open_req("chat-1")).await.unwrap();
        assert!(handle.token().starts_with("pcs:"));
        assert_eq!(handle.backend(), &BrowserBackendId::personal_chrome());
        assert_eq!(port.launch_attempts(), 0);
        let again = backend.open_session(&open_req("chat-1")).await.unwrap();
        assert_eq!(handle, again);
    }

    #[tokio::test]
    async fn incompatible_session_options_are_rejected() {
        let (backend, _) = attached_backend().await;
        let mut opts = attach_opts();
        opts.cdp_url = Some("http://127.0.0.1:9222".into());
        let err = backend
            .open_session(&BrowserSessionOpenRequest::new(
                BrowserSessionKey::new("x").unwrap(),
                opts,
            ))
            .await
            .unwrap_err();
        assert!(err.detail.contains(PERSONAL_CHROME_NOT_AUTHORIZED));
        assert!(err.detail.contains("CDP"));
    }

    #[tokio::test]
    async fn exact_authorized_tab_binding_and_unauthorized_tab_hidden() {
        let (backend, _) = attached_backend().await;
        let session = backend.open_session(&open_req("chat-1")).await.unwrap();
        let tabs = backend.tabs(&session).await.unwrap();
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].id.as_str(), "11");
        assert_ne!(tabs[0].url.as_deref(), Some("https://bank.test/inbox"));
        let obs = backend
            .observe(&session, &ObserveRequest::snapshot())
            .await
            .unwrap();
        assert!(obs.text.as_deref().unwrap_or_default().contains("user"));
        assert!(!obs
            .text
            .as_deref()
            .unwrap_or_default()
            .contains("secret mail"));
    }

    #[tokio::test]
    async fn password_value_is_redacted() {
        let (backend, port) = attached_backend().await;
        let session = backend.open_session(&open_req("chat-1")).await.unwrap();
        let obs = backend
            .observe(&session, &ObserveRequest::snapshot())
            .await
            .unwrap();
        let blob = format!("{obs:?}");
        assert!(!blob.contains("super-secret-password"));
        let password_ref = obs
            .snapshot
            .unwrap()
            .elements
            .into_iter()
            .find(|el| el.name.as_deref() == Some("password"))
            .unwrap()
            .reference;
        let value = backend
            .observe(
                &session,
                &ObserveRequest {
                    kind: BrowserObserveKind::Value {
                        target: BrowserTarget::Element(password_ref),
                    },
                    interactive_only: false,
                    compact: false,
                },
            )
            .await
            .unwrap();
        assert_eq!(value.text.as_deref(), Some(""));
        assert!(!port.password_was_logged());
    }

    #[tokio::test]
    async fn click_fill_type_and_stale_ref() {
        let (backend, _) = attached_backend().await;
        let session = backend.open_session(&open_req("chat-1")).await.unwrap();
        let obs = backend
            .observe(&session, &ObserveRequest::snapshot())
            .await
            .unwrap();
        let user = obs
            .snapshot
            .as_ref()
            .unwrap()
            .elements
            .iter()
            .find(|el| el.name.as_deref() == Some("user"))
            .unwrap()
            .reference
            .clone();
        backend
            .act(
                &session,
                &BrowserAction::Fill {
                    target: BrowserTarget::Element(user.clone()),
                    value: "alice".into(),
                },
            )
            .await
            .unwrap();
        backend
            .act(
                &session,
                &BrowserAction::Type {
                    target: BrowserTarget::Element(user.clone()),
                    text: "alice".into(),
                },
            )
            .await
            .unwrap();
        let go = obs
            .snapshot
            .unwrap()
            .elements
            .into_iter()
            .find(|el| el.name.as_deref() == Some("Go"))
            .unwrap()
            .reference;
        let clicked = backend
            .act(
                &session,
                &BrowserAction::Click {
                    target: BrowserTarget::Element(go.clone()),
                },
            )
            .await
            .unwrap();
        assert_eq!(clicked.title.as_deref(), Some("Clicked"));
        backend
            .open(
                &session,
                &NavigateRequest {
                    url: "https://example.test/next".into(),
                },
            )
            .await
            .unwrap();
        let err = backend
            .act(
                &session,
                &BrowserAction::Click {
                    target: BrowserTarget::Element(user),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::StaleReference);
        let fresh = backend
            .observe(&session, &ObserveRequest::snapshot())
            .await
            .unwrap();
        assert_eq!(fresh.url.as_deref(), Some("https://example.test/next"));
    }

    #[tokio::test]
    async fn eval_and_screenshot_are_typed_unsupported() {
        let (backend, _) = attached_backend().await;
        let session = backend.open_session(&open_req("chat-1")).await.unwrap();
        let eval = backend
            .act(&session, &BrowserAction::eval_raw("1+1"))
            .await
            .unwrap_err();
        assert!(eval.detail.contains(OPERATION_UNSUPPORTED));
        let shot = backend
            .screenshot(&session, &ScreenshotRequest { locator: None })
            .await
            .unwrap_err();
        assert!(shot.detail.contains(OPERATION_UNSUPPORTED));
    }

    #[tokio::test]
    async fn close_session_detaches_only() {
        let (backend, port) = attached_backend().await;
        let session = backend.open_session(&open_req("chat-1")).await.unwrap();
        backend.close_session(&session).await.unwrap();
        assert!(port.chrome_alive());
        let health = backend.session_health(&session).await;
        assert!(matches!(health, BrowserHealth::Unhealthy { .. }));
    }

    #[tokio::test]
    async fn extension_disconnect_and_reconnect_reattach() {
        let (backend, port) = attached_backend().await;
        let session = backend.open_session(&open_req("chat-1")).await.unwrap();
        port.disconnect();
        let health = backend.session_health(&session).await;
        assert!(matches!(
            health,
            BrowserHealth::Unhealthy {
                kind: BrowserErrorKind::NotConnected,
                ..
            }
        ));
        port.reconnect_same_generation();
        assert!(matches!(
            backend.session_health(&session).await,
            BrowserHealth::Healthy
        ));
        port.bump_generation_and_drop_auth();
        backend.close_session(&session).await.unwrap();
        let err = backend.open_session(&open_req("chat-1")).await.unwrap_err();
        assert!(err.detail.contains(PERSONAL_CHROME_NOT_AUTHORIZED));
        assert_eq!(port.launch_attempts(), 0);
    }

    #[tokio::test]
    async fn tab_close_and_authorization_revoke_fail_typed() {
        let (backend, port) = attached_backend().await;
        let session = backend.open_session(&open_req("chat-1")).await.unwrap();
        port.close_authorized_tab();
        let err = backend
            .observe(&session, &ObserveRequest::snapshot())
            .await
            .unwrap_err();
        assert!(err.detail.contains(TAB_UNAVAILABLE) || err.detail.contains(PERSONAL_CHROME_NOT_AUTHORIZED));
        let (backend, port) = attached_backend().await;
        let session = backend.open_session(&open_req("chat-2")).await.unwrap();
        port.revoke_authorization();
        let err = backend
            .observe(&session, &ObserveRequest::snapshot())
            .await
            .unwrap_err();
        assert!(err.detail.contains(PERSONAL_CHROME_NOT_AUTHORIZED));
    }

    #[tokio::test]
    async fn human_takeover_reuses_b3_4_runtime() {
        let port = MockPersonalChromePort::connected_fixture();
        let backend = Arc::new(PersonalChromeBackend::new(port.clone()));
        backend.grant_test_authorization(1, 11).await.unwrap();
        let runtime = BrowserRuntime::new(
            backend,
            BrowserRuntimePolicy {
                allowed_domains: vec!["example.test".into()],
                ..BrowserRuntimePolicy::default()
            },
        );
        let key = BrowserSessionKey::new("takeover-pc").unwrap();
        let opts = attach_opts();
        runtime
            .observe(&key, &opts, &ObserveRequest::snapshot())
            .await
            .unwrap();
        runtime
            .request_human_takeover(&key, &opts, TakeoverReason::ExplicitUserRequest)
            .unwrap();
        let blocked = runtime
            .act(
                &key,
                &opts,
                &BrowserAction::Click {
                    target: BrowserTarget::Css("#go".into()),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(blocked.kind, BrowserErrorKind::HumanTakeoverActive);
        runtime.release_human_takeover(&key).unwrap();
        let stale = runtime
            .act(
                &key,
                &opts,
                &BrowserAction::Click {
                    target: BrowserTarget::Css("#go".into()),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(stale.kind, BrowserErrorKind::StaleAssumptions);
        runtime
            .observe(&key, &opts, &ObserveRequest::snapshot())
            .await
            .unwrap();
        runtime
            .act(
                &key,
                &opts,
                &BrowserAction::Click {
                    target: BrowserTarget::Css("#go".into()),
                },
            )
            .await
            .unwrap();
        runtime.close_session(&key, &opts).await.unwrap();
        assert!(port.chrome_alive());
    }

    #[tokio::test]
    async fn runtime_open_keeps_allowed_domain_policy() {
        let port = MockPersonalChromePort::connected_fixture();
        let backend = Arc::new(PersonalChromeBackend::new(port));
        backend.grant_test_authorization(1, 11).await.unwrap();
        let runtime = BrowserRuntime::new(
            backend,
            BrowserRuntimePolicy {
                allowed_domains: vec!["example.test".into()],
                ..BrowserRuntimePolicy::default()
            },
        );
        let key = BrowserSessionKey::new("policy-pc").unwrap();
        let err = runtime
            .open(
                &key,
                &attach_opts(),
                &NavigateRequest {
                    url: "https://evil.test/".into(),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::Rejected);
    }

    #[test]
    fn tool_schema_unchanged_and_factory_fail_closed() {
        let tool = BrowserTool::new(Vec::new(), true, false, None);
        let schema = tool.parameters_schema();
        let actions = schema["properties"]["action"]["enum"]
            .as_array()
            .unwrap()
            .len();
        assert_eq!(actions, crate::tools::browser_types::V1_TOOL_ACTIONS.len());
        let err = match crate::tools::browser_agent_backend::backend_from_config(
            "personal-chrome",
            None,
            BrowserSessionOptions::default(),
        ) {
            Ok(_) => panic!("personal-chrome production factory must stay fail-closed"),
            Err(err) => err,
        };
        assert!(err.detail.contains("BrowserBackendUnsupported"));
        assert!(err.detail.contains("B3.5-D") || err.detail.contains("authorization"));
    }

    #[test]
    fn restricted_urls_are_detected() {
        assert!(is_restricted_url("chrome://settings"));
        assert!(is_restricted_url("chrome-extension://abc/popup.html"));
        assert!(is_restricted_url("https://chromewebstore.google.com/detail/x"));
        assert!(!is_restricted_url("https://example.test/"));
    }

    #[tokio::test]
    async fn protocol_mismatch_is_typed() {
        let port = MockPersonalChromePort::connected_fixture();
        port.force_protocol_mismatch();
        let backend = PersonalChromeBackend::new(port);
        let err = backend.grant_test_authorization(1, 11).await.unwrap_err();
        assert!(err.detail.contains(PROTOCOL_MISMATCH));
    }

    #[tokio::test]
    async fn session_health_healthy_when_authorized_tab_exists() {
        let (backend, _) = attached_backend().await;
        let session = backend.open_session(&open_req("chat-health")).await.unwrap();
        assert!(matches!(
            backend.session_health(&session).await,
            BrowserHealth::Healthy
        ));
    }

    #[tokio::test]
    async fn press_scroll_hover_do_not_spawn_chrome() {
        let (backend, port) = attached_backend().await;
        let session = backend.open_session(&open_req("chat-keys")).await.unwrap();
        backend
            .act(
                &session,
                &BrowserAction::Press {
                    key: "Enter".into(),
                },
            )
            .await
            .unwrap();
        backend
            .act(
                &session,
                &BrowserAction::Scroll {
                    direction: ScrollDirection::Down,
                    pixels: Some(40),
                    target: None,
                },
            )
            .await
            .unwrap();
        backend
            .act(
                &session,
                &BrowserAction::Hover {
                    target: BrowserTarget::Css("#go".into()),
                },
            )
            .await
            .unwrap();
        assert_eq!(port.launch_attempts(), 0);
        assert!(port.chrome_alive());
    }

    #[tokio::test]
    async fn managed_profile_and_headless_are_rejected() {
        let (backend, port) = attached_backend().await;
        let mut opts = attach_opts();
        opts.headless = true;
        let err = backend
            .open_session(&BrowserSessionOpenRequest::new(
                BrowserSessionKey::new("headless-pc").unwrap(),
                opts,
            ))
            .await
            .unwrap_err();
        assert!(err.detail.contains(PERSONAL_CHROME_NOT_AUTHORIZED));
        let mut opts = attach_opts();
        opts.attach_only = false;
        let err = backend
            .open_session(&BrowserSessionOpenRequest::new(
                BrowserSessionKey::new("launch-pc").unwrap(),
                opts,
            ))
            .await
            .unwrap_err();
        assert!(err.detail.contains("attach-only"));
        assert_eq!(port.launch_attempts(), 0);
    }
}
