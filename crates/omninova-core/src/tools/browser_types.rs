//! OmniNova Browser Runtime V2 domain types.
//!
//! These types belong to OmniNova. They must not leak agent-browser CLI
//! argv, `--session` / `--namespace`, `@eN` vendor semantics, sidecar paths,
//! or raw vendor JSON. Vendor encoding lives in a backend implementation
//! (B3.1C), not here.
//!
//! B3.1A.1 is a contract closeout. BrowserTool still executes the V1 path.

use std::fmt;

/// Logical OmniNova Browser session identity (Chat / Agent → Runtime).
///
/// Distinct from [`BackendSessionHandle`]: this is OmniNova's stable
/// session, not a vendor connection token. Construction is controlled
/// (non-empty only). It does not require `omninova-`, hex charset,
/// CLI-safe names, or `@eN`. Vendor naming stays in AgentBrowserBackend.
/// Hashing (`omninova-<sha256>`) stays in V1 `browser_session_id` until B3.1B.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BrowserSessionKey(String);

impl BrowserSessionKey {
    pub fn new(value: impl Into<String>) -> Result<Self, BrowserTypeError> {
        let value = value.into();
        if value.is_empty() {
            return Err(BrowserTypeError::EmptyIdentity);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Opaque backend-issued session/connection token.
///
/// Runtime must not parse `token`. Vendor formats (hashed CLI names,
/// extension claim ids) stay inside the issuing backend.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BackendSessionHandle {
    backend: BrowserBackendId,
    token: String,
}

impl BackendSessionHandle {
    pub fn new(
        backend: BrowserBackendId,
        token: impl Into<String>,
    ) -> Result<Self, BrowserTypeError> {
        let token = token.into();
        if token.is_empty() {
            return Err(BrowserTypeError::EmptyIdentity);
        }
        Ok(Self { backend, token })
    }

    pub fn backend(&self) -> &BrowserBackendId {
        &self.backend
    }

    pub fn token(&self) -> &str {
        &self.token
    }
}

/// Opaque page/tab identifier. Not assumed to be a CDP `targetId`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BrowserPageId(String);

impl BrowserPageId {
    pub fn new(value: impl Into<String>) -> Result<Self, BrowserTypeError> {
        let value = value.into();
        if value.is_empty() {
            return Err(BrowserTypeError::EmptyIdentity);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserTab {
    pub id: BrowserPageId,
    pub url: Option<String>,
    pub title: Option<String>,
    pub active: bool,
}

/// Identifies a browser backend implementation. Newtype (not a closed enum)
/// so future plugin backends can register without a crate-wide enum change.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BrowserBackendId(String);

impl BrowserBackendId {
    pub const AGENT_BROWSER: &'static str = "agent-browser";
    pub const PERSONAL_CHROME: &'static str = "personal-chrome";

    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn agent_browser() -> Self {
        Self::new(Self::AGENT_BROWSER)
    }

    pub fn personal_chrome() -> Self {
        Self::new(Self::PERSONAL_CHROME)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Opaque element reference issued by a backend. The `value` string is not
/// required to look like `@eN`; that encoding is agent-browser-specific.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BrowserElementRef {
    backend: BrowserBackendId,
    value: String,
}

impl BrowserElementRef {
    pub fn new(backend: BrowserBackendId, value: impl Into<String>) -> Self {
        Self {
            backend,
            value: value.into(),
        }
    }

    pub fn backend(&self) -> &BrowserBackendId {
        &self.backend
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserTarget {
    Element(BrowserElementRef),
    Css(String),
    Role { role: String, name: Option<String> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

/// In-session mutations / waits. Read-only page inspection is
/// [`BrowserObserveKind`] via `BrowserBackend::observe`. Session close,
/// navigation `open`, and screenshot are backend methods — not variants
/// here — so there is a single close semantic (`close_session`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserAction {
    Click {
        target: BrowserTarget,
    },
    Fill {
        target: BrowserTarget,
        value: String,
    },
    Type {
        target: BrowserTarget,
        text: String,
    },
    Press {
        key: String,
    },
    Scroll {
        direction: ScrollDirection,
        pixels: Option<u32>,
        target: Option<BrowserTarget>,
    },
    Select {
        target: BrowserTarget,
        value: String,
    },
    Hover {
        target: BrowserTarget,
    },
    Eval {
        script: String,
    },
    Wait {
        timeout_ms: Option<u64>,
        text: Option<String>,
        target: Option<BrowserTarget>,
    },
    Back,
    Forward,
    Reload,
}

impl BrowserAction {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Click { .. } => "click",
            Self::Fill { .. } => "fill",
            Self::Type { .. } => "type",
            Self::Press { .. } => "press",
            Self::Scroll { .. } => "scroll",
            Self::Select { .. } => "select",
            Self::Hover { .. } => "hover",
            Self::Eval { .. } => "eval",
            Self::Wait { .. } => "wait",
            Self::Back => "back",
            Self::Forward => "forward",
            Self::Reload => "reload",
        }
    }
}

/// Read-only observation. Distinct from [`BrowserAction`] (Stagehand-style
/// observe → then act). V1 tool `action=get_text` etc. stay on the schema;
/// a future adapter maps them here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserObserveKind {
    Snapshot,
    Text { target: Option<BrowserTarget> },
    Html { target: Option<BrowserTarget> },
    Url,
    Title,
    Value { target: BrowserTarget },
    Visibility { target: BrowserTarget },
    Enabled { target: BrowserTarget },
    Find { role: String, name: Option<String> },
}

impl BrowserObserveKind {
    /// V1 tool `action` name this observation corresponds to.
    pub fn v1_action_name(&self) -> &'static str {
        match self {
            Self::Snapshot => "snapshot",
            Self::Text { .. } => "get_text",
            Self::Html { .. } => "get_html",
            Self::Url => "get_url",
            Self::Title => "get_title",
            Self::Value { .. } => "get_value",
            Self::Visibility { .. } => "is_visible",
            Self::Enabled { .. } => "is_enabled",
            Self::Find { .. } => "find",
        }
    }
}

/// How a V1 tool `action=` string maps onto the V2 backend surface.
/// Adapter mapping is B3.1B; this classification is the contract only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum V1ToolActionRoute {
    Navigate,
    Observe,
    Screenshot,
    CloseSession,
    Act,
}

/// V1 BrowserTool `action` enum, frozen for the B3.1A contract tests.
pub const V1_TOOL_ACTIONS: &[&str] = &[
    "open",
    "snapshot",
    "click",
    "fill",
    "type",
    "screenshot",
    "get_text",
    "get_html",
    "get_url",
    "get_title",
    "get_value",
    "wait",
    "scroll",
    "select",
    "press",
    "hover",
    "eval",
    "back",
    "forward",
    "reload",
    "close",
    "is_visible",
    "is_enabled",
    "find",
];

pub fn route_v1_tool_action(action: &str) -> Option<V1ToolActionRoute> {
    match action {
        "open" => Some(V1ToolActionRoute::Navigate),
        "snapshot" | "get_text" | "get_html" | "get_url" | "get_title" | "get_value"
        | "is_visible" | "is_enabled" | "find" => Some(V1ToolActionRoute::Observe),
        "screenshot" => Some(V1ToolActionRoute::Screenshot),
        "close" => Some(V1ToolActionRoute::CloseSession),
        "click" | "fill" | "type" | "press" | "scroll" | "select" | "hover" | "eval" | "wait"
        | "back" | "forward" | "reload" => Some(V1ToolActionRoute::Act),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserObservation {
    pub url: Option<String>,
    pub title: Option<String>,
    pub text: Option<String>,
    pub snapshot: Option<BrowserSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserSnapshot {
    pub text: String,
    pub elements: Vec<BrowserElement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserElement {
    pub reference: BrowserElementRef,
    pub role: Option<String>,
    pub name: Option<String>,
    pub interactive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserActionResult {
    pub detail: String,
    pub url: Option<String>,
    pub title: Option<String>,
}

/// Backend-independent screenshot locator. Not a multimodal image payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenshotResult {
    pub locator: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigateRequest {
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObserveRequest {
    pub kind: BrowserObserveKind,
    pub interactive_only: bool,
    pub compact: bool,
}

impl ObserveRequest {
    pub fn snapshot() -> Self {
        Self {
            kind: BrowserObserveKind::Snapshot,
            interactive_only: false,
            compact: false,
        }
    }
}

/// Logical session identity plus launch/attach options.
///
/// `BrowserBackend::open_session` must receive the key so a rebuilt Runtime
/// can attach/reuse the same backend logical session. Do not rely on an
/// in-memory map of a previous BrowserTool instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserSessionOpenRequest {
    pub key: BrowserSessionKey,
    pub options: BrowserSessionOptions,
}

impl BrowserSessionOpenRequest {
    pub fn new(key: BrowserSessionKey, options: BrowserSessionOptions) -> Self {
        Self { key, options }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ScreenshotRequest {
    pub locator: Option<String>,
}

/// Opaque future profile slot. B3.1A always uses `None` on session options.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BrowserProfileRef(String);

impl BrowserProfileRef {
    pub fn new(value: impl Into<String>) -> Result<Self, BrowserTypeError> {
        let value = value.into();
        if value.is_empty() {
            return Err(BrowserTypeError::EmptyIdentity);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserSessionOptions {
    pub headless: bool,
    pub attach_only: bool,
    pub cdp_url: Option<String>,
    pub profile: Option<BrowserProfileRef>,
}

impl Default for BrowserSessionOptions {
    fn default() -> Self {
        Self {
            headless: true,
            attach_only: false,
            cdp_url: None,
            profile: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserErrorKind {
    BinaryMissing,
    LaunchFailed,
    NotConnected,
    SessionNotFound,
    Crashed,
    Timeout,
    Rejected,
    CommandFailed,
}

impl BrowserErrorKind {
    pub fn default_retryable(self) -> bool {
        matches!(
            self,
            Self::NotConnected | Self::Crashed | Self::Timeout | Self::CommandFailed
        )
    }
}

/// Contract for mapping current V1 error prefixes onto V2 kinds.
/// Does not rewrite V1 strings; V1 formatting stays in `browser.rs`.
pub fn v1_error_kind(message: &str) -> Option<BrowserErrorKind> {
    let head = message.split([':', ' ']).next().unwrap_or(message);
    match head {
        "BrowserBinaryMissing" | "BrowserBinaryNotExecutable" => {
            Some(BrowserErrorKind::BinaryMissing)
        }
        "BrowserLaunchFailed" => Some(BrowserErrorKind::LaunchFailed),
        "BrowserDaemonUnavailable" => Some(BrowserErrorKind::NotConnected),
        "BrowserSessionUnavailable" | "BrowserSessionInvalid" | "BrowserSessionMissing" => {
            Some(BrowserErrorKind::SessionNotFound)
        }
        "BrowserCrashed" => Some(BrowserErrorKind::Crashed),
        "BrowserCommandTimeout" => Some(BrowserErrorKind::Timeout),
        "BrowserUrlRejected" => Some(BrowserErrorKind::Rejected),
        "BrowserCommandFailed" | "InvalidBrowserOutput" => Some(BrowserErrorKind::CommandFailed),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserBackendError {
    pub kind: BrowserErrorKind,
    pub backend: BrowserBackendId,
    pub detail: String,
    pub retryable: bool,
}

impl BrowserBackendError {
    pub fn new(
        kind: BrowserErrorKind,
        backend: BrowserBackendId,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            retryable: kind.default_retryable(),
            kind,
            backend,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for BrowserBackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} backend={} retryable={}: {}",
            self.kind,
            self.backend.as_str(),
            self.retryable,
            self.detail
        )
    }
}

impl std::error::Error for BrowserBackendError {}

/// Whether a backend is installed/configured (registry capability).
/// Distinct from [`BrowserHealth`] (a live session).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendAvailability {
    Available,
    Unavailable {
        kind: BrowserErrorKind,
        detail: String,
    },
}

/// Health of an already-opened backend session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserHealth {
    Healthy,
    Unhealthy {
        kind: BrowserErrorKind,
        detail: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct BackendCapabilities {
    pub navigation: bool,
    pub observation: bool,
    pub element_actions: bool,
    pub tabs: bool,
    pub screenshot: bool,
    pub eval: bool,
    pub attach: bool,
    pub profiles: bool,
}

impl BackendCapabilities {
    pub fn supports_action(&self, action: &BrowserAction) -> bool {
        match action {
            BrowserAction::Eval { .. } => self.eval,
            BrowserAction::Back | BrowserAction::Forward | BrowserAction::Reload => self.navigation,
            _ => self.element_actions,
        }
    }

    pub fn supports_observation(&self, _kind: &BrowserObserveKind) -> bool {
        self.observation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserTypeError {
    EmptyIdentity,
}

impl fmt::Display for BrowserTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyIdentity => write!(f, "identity value must be non-empty"),
        }
    }
}

impl std::error::Error for BrowserTypeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_key_and_backend_handle_are_distinct_constructors() {
        let key = BrowserSessionKey::new("chat-1").unwrap();
        let handle =
            BackendSessionHandle::new(BrowserBackendId::agent_browser(), "omninova-a82b").unwrap();
        assert_eq!(key.as_str(), "chat-1");
        assert_ne!(key.as_str(), handle.token());
        assert!(BrowserSessionKey::new("").is_err());
        assert!(BackendSessionHandle::new(BrowserBackendId::agent_browser(), "").is_err());
    }

    #[test]
    fn element_ref_does_not_require_en_prefix() {
        let by_css_handle =
            BrowserElementRef::new(BrowserBackendId::personal_chrome(), "ext:node:按钮-日本語");
        assert!(!by_css_handle.as_str().starts_with("@e"));
        let vendor_looking = BrowserElementRef::new(BrowserBackendId::agent_browser(), "@e12");
        assert_eq!(vendor_looking.as_str(), "@e12");
        assert_ne!(by_css_handle, vendor_looking);
    }

    fn sample_css() -> BrowserTarget {
        BrowserTarget::Css("#a".into())
    }

    #[test]
    fn browser_action_is_mutations_only() {
        let names: Vec<&str> = [
            BrowserAction::Click {
                target: sample_css(),
            },
            BrowserAction::Fill {
                target: sample_css(),
                value: "x".into(),
            },
            BrowserAction::Type {
                target: sample_css(),
                text: "x".into(),
            },
            BrowserAction::Press {
                key: "Enter".into(),
            },
            BrowserAction::Scroll {
                direction: ScrollDirection::Down,
                pixels: Some(300),
                target: None,
            },
            BrowserAction::Select {
                target: sample_css(),
                value: "1".into(),
            },
            BrowserAction::Hover {
                target: sample_css(),
            },
            BrowserAction::Eval {
                script: "1+1".into(),
            },
            BrowserAction::Wait {
                timeout_ms: Some(1000),
                text: None,
                target: None,
            },
            BrowserAction::Back,
            BrowserAction::Forward,
            BrowserAction::Reload,
        ]
        .iter()
        .map(BrowserAction::name)
        .collect();

        for forbidden in [
            "get_text",
            "get_html",
            "get_value",
            "get_url",
            "get_title",
            "is_visible",
            "is_enabled",
            "find",
            "close",
            "open",
            "snapshot",
            "screenshot",
        ] {
            assert!(!names.contains(&forbidden), "{forbidden}");
        }

        for action in V1_TOOL_ACTIONS {
            let route = route_v1_tool_action(action).expect(action);
            match route {
                V1ToolActionRoute::Act => assert!(names.contains(action), "{action}"),
                V1ToolActionRoute::CloseSession => assert_eq!(*action, "close"),
                V1ToolActionRoute::Navigate => assert_eq!(*action, "open"),
                V1ToolActionRoute::Observe => {}
                V1ToolActionRoute::Screenshot => assert_eq!(*action, "screenshot"),
            }
        }
    }

    #[test]
    fn observe_kind_covers_v1_read_actions() {
        let kinds = [
            BrowserObserveKind::Snapshot,
            BrowserObserveKind::Text { target: None },
            BrowserObserveKind::Html { target: None },
            BrowserObserveKind::Url,
            BrowserObserveKind::Title,
            BrowserObserveKind::Value {
                target: sample_css(),
            },
            BrowserObserveKind::Visibility {
                target: sample_css(),
            },
            BrowserObserveKind::Enabled {
                target: sample_css(),
            },
            BrowserObserveKind::Find {
                role: "button".into(),
                name: Some("Go".into()),
            },
        ];
        let names: Vec<&str> = kinds
            .iter()
            .map(BrowserObserveKind::v1_action_name)
            .collect();
        for expected in [
            "snapshot",
            "get_text",
            "get_html",
            "get_url",
            "get_title",
            "get_value",
            "is_visible",
            "is_enabled",
            "find",
        ] {
            assert!(names.contains(&expected), "{expected}");
        }
        for action in V1_TOOL_ACTIONS {
            if route_v1_tool_action(action) == Some(V1ToolActionRoute::Observe) {
                assert!(names.contains(action), "{action}");
            }
        }
    }

    #[test]
    fn session_open_request_carries_logical_key() {
        let key = BrowserSessionKey::new("chat-rebuild-safe").unwrap();
        let req = BrowserSessionOpenRequest::new(key.clone(), BrowserSessionOptions::default());
        assert_eq!(req.key, key);
        assert!(req.options.profile.is_none());
    }

    #[test]
    fn session_key_does_not_require_vendor_session_rules() {
        let key = BrowserSessionKey::new("chat:slot/会话 1").unwrap();
        assert!(!key.as_str().starts_with("omninova-"));
        assert!(key.as_str().contains(' '));
        assert!(key.as_str().contains('/'));
    }

    #[test]
    fn snapshot_keeps_same_name_distinct_refs() {
        let backend = BrowserBackendId::agent_browser();
        let snapshot = BrowserSnapshot {
            text: "Submit Submit".into(),
            elements: vec![
                BrowserElement {
                    reference: BrowserElementRef::new(backend.clone(), "one"),
                    role: Some("button".into()),
                    name: Some("Submit".into()),
                    interactive: true,
                },
                BrowserElement {
                    reference: BrowserElementRef::new(backend, "two"),
                    role: Some("button".into()),
                    name: Some("Submit".into()),
                    interactive: true,
                },
            ],
        };
        assert_ne!(
            snapshot.elements[0].reference,
            snapshot.elements[1].reference
        );
        assert_eq!(snapshot.elements[0].name, snapshot.elements[1].name);
    }

    #[test]
    fn unicode_targets_are_type_safe() {
        let target = BrowserTarget::Role {
            role: "button".into(),
            name: Some("保存 ✓".into()),
        };
        let BrowserTarget::Role { name, .. } = target else {
            panic!("role target");
        };
        assert_eq!(name.as_deref(), Some("保存 ✓"));
        let key = BrowserSessionKey::new("会话-🔑").unwrap();
        assert!(key.as_str().contains('🔑'));
    }

    #[test]
    fn session_options_default_profile_none() {
        let opts = BrowserSessionOptions::default();
        assert!(opts.profile.is_none());
        assert!(opts.headless);
        assert!(!opts.attach_only);
        assert!(opts.cdp_url.is_none());
    }

    #[test]
    fn capabilities_split_action_and_observation() {
        let caps = BackendCapabilities {
            navigation: true,
            observation: true,
            element_actions: true,
            eval: true,
            ..BackendCapabilities::default()
        };
        assert!(caps.supports_action(&BrowserAction::Click {
            target: sample_css(),
        }));
        assert!(caps.supports_action(&BrowserAction::Eval { script: "1".into() }));
        assert!(caps.supports_observation(&BrowserObserveKind::Url));
        assert!(caps.supports_observation(&BrowserObserveKind::Snapshot));
        let observe_only = BackendCapabilities {
            observation: true,
            ..BackendCapabilities::default()
        };
        assert!(!observe_only.supports_action(&BrowserAction::Click {
            target: sample_css(),
        }));
        assert!(observe_only.supports_observation(&BrowserObserveKind::Title));
    }

    #[test]
    fn v1_error_prefix_mapping_contract() {
        let cases = [
            ("BrowserBinaryMissing: x", BrowserErrorKind::BinaryMissing),
            (
                "BrowserBinaryNotExecutable: x",
                BrowserErrorKind::BinaryMissing,
            ),
            ("BrowserLaunchFailed: x", BrowserErrorKind::LaunchFailed),
            (
                "BrowserDaemonUnavailable: x",
                BrowserErrorKind::NotConnected,
            ),
            (
                "BrowserSessionUnavailable: x",
                BrowserErrorKind::SessionNotFound,
            ),
            (
                "BrowserSessionInvalid: x",
                BrowserErrorKind::SessionNotFound,
            ),
            (
                "BrowserSessionMissing: x",
                BrowserErrorKind::SessionNotFound,
            ),
            ("BrowserCrashed: x", BrowserErrorKind::Crashed),
            ("BrowserCommandTimeout: x", BrowserErrorKind::Timeout),
            ("BrowserUrlRejected: x", BrowserErrorKind::Rejected),
            ("BrowserCommandFailed: x", BrowserErrorKind::CommandFailed),
        ];
        for (msg, kind) in cases {
            assert_eq!(v1_error_kind(msg), Some(kind), "{msg}");
        }
        assert!(!BrowserErrorKind::BinaryMissing.default_retryable());
        assert!(!BrowserErrorKind::Rejected.default_retryable());
        assert!(BrowserErrorKind::Timeout.default_retryable());
        assert!(BrowserErrorKind::Crashed.default_retryable());
    }
}
