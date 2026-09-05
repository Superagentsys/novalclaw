//! OmniNova Browser Runtime V2 domain types.
//!
//! These types belong to OmniNova. They must not leak agent-browser CLI
//! argv, `--session` / `--namespace`, `@eN` vendor semantics, sidecar paths,
//! or raw vendor JSON. Vendor encoding lives in a backend implementation
//! (B3.1C), not here.
//!
//! B3.1A.1 is a contract closeout. Production execute goes through BrowserRuntime.

use std::fmt;

use serde_json::Value;

/// Logical OmniNova Browser session identity (Chat / Agent → Runtime).
///
/// Distinct from [`BackendSessionHandle`]: this is OmniNova's stable
/// session, not a vendor connection token. Construction is controlled
/// (non-empty only). It does not require `omninova-`, hex charset,
/// CLI-safe names, or `@eN`. Vendor naming stays in AgentBrowserBackend.
/// Hashing (`omninova-<sha256>`) lives in `AgentBrowserBackend`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BrowserSessionKey(String);

/// V1 model-facing missing-session text. Shared so Tool and Runtime stay in lockstep.
pub const BROWSER_SESSION_MISSING_DETAIL: &str = "BrowserSessionMissing: OmniNova session id is required; refusing to use a default or shared agent-browser session. Do not retry this call without a valid session.";

pub const BROWSER_SESSION_RESERVED_DEFAULT_DETAIL: &str =
    "BrowserSessionInvalid: refusing to use the agent-browser default session";

/// V1-compatible model-facing stale-handle guidance. Internal kind is
/// [`BrowserErrorKind::StaleReference`]; the prefix stays `BrowserCommandFailed`.
pub const BROWSER_STALE_REFERENCE_DETAIL: &str =
    "BrowserCommandFailed: element reference is stale or unavailable; run snapshot again before retrying";

/// Model-facing source-bound extract failure. Internal kind stays
/// [`BrowserErrorKind::CommandFailed`]; not retryable.
pub const BROWSER_EXTRACT_SOURCE_TOO_LARGE_DETAIL: &str =
    "BrowserCommandFailed: structured extract result exceeds source limit; narrow the extraction expression";

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

    /// OmniNova logical session policy (V1 Tool parity). Does not constrain
    /// [`BackendSessionHandle`] tokens.
    pub fn omninova_policy_error(&self) -> Option<(BrowserErrorKind, &'static str)> {
        let trimmed = self.0.trim();
        if trimmed.is_empty() {
            return Some((
                BrowserErrorKind::SessionNotFound,
                BROWSER_SESSION_MISSING_DETAIL,
            ));
        }
        if trimmed.eq_ignore_ascii_case("default") {
            return Some((
                BrowserErrorKind::Rejected,
                BROWSER_SESSION_RESERVED_DEFAULT_DETAIL,
            ));
        }
        None
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

/// Opaque element reference issued by a backend.
///
/// Observation-scoped / page-state-scoped: the handle may become stale after
/// navigation or DOM mutation. Re-observe before reuse when stale. This is not
/// a permanent DOM id, and Runtime must not truncate or rewrite `value`.
/// Vendor encoding of `value` is backend-specific.
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
        mode: BrowserEvalMode,
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

/// Distinguishes V1 `action=eval` from internal structured extract.
///
/// Tool schema does not expose this field. [`Self::Raw`] preserves current
/// model-visible eval text. [`Self::StructuredJson`] is a backend-neutral
/// extract mode; vendor serialization lives in the backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserEvalMode {
    Raw,
    StructuredJson,
}

impl Default for BrowserEvalMode {
    fn default() -> Self {
        Self::Raw
    }
}

impl BrowserAction {
    pub fn eval_raw(script: impl Into<String>) -> Self {
        Self::Eval {
            script: script.into(),
            mode: BrowserEvalMode::Raw,
        }
    }

    pub fn eval_structured_json(script: impl Into<String>) -> Self {
        Self::Eval {
            script: script.into(),
            mode: BrowserEvalMode::StructuredJson,
        }
    }

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
    Text {
        target: Option<BrowserTarget>,
    },
    Html {
        target: Option<BrowserTarget>,
    },
    Url,
    Title,
    Value {
        target: BrowserTarget,
    },
    Visibility {
        target: BrowserTarget,
    },
    Enabled {
        target: BrowserTarget,
    },
    Find {
        role: String,
        name: Option<String>,
        /// V1 `value` for `find` (click/text/etc). Defaults to `"text"`.
        action: Option<String>,
    },
    /// Document-reading observation of the current page. Distinct from
    /// [`Self::Text`] (page/target text). Tool `action=read` maps here.
    Read {
        outline: bool,
        filter: Option<String>,
    },
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
            Self::Read { .. } => "read",
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

/// V1 BrowserTool `action` enum. B3.2F added `read` (25 actions).
pub const V1_TOOL_ACTIONS: &[&str] = &[
    "open",
    "snapshot",
    "read",
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
        "snapshot" | "read" | "get_text" | "get_html" | "get_url" | "get_title" | "get_value"
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
    /// Backend-neutral: source or runtime truncated untrusted content.
    /// Does not change V1 model-visible snapshot/get_text envelopes.
    pub truncated: bool,
}

/// Page snapshot. `text` is the model-visible envelope. `elements` is an
/// internal Runtime domain capability (not a tool-schema field).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserSnapshot {
    pub text: String,
    pub elements: Vec<BrowserElement>,
}

/// Structured snapshot element. `role` and `name` are untrusted web content
/// and must not be treated as model instructions. `interactive` is conservative
/// backend metadata: `false` does not mean the element cannot be targeted, and
/// Runtime must not refuse [`BrowserAction`] based on this flag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserElement {
    pub reference: BrowserElementRef,
    pub role: Option<String>,
    pub name: Option<String>,
    pub interactive: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct BrowserActionResult {
    pub detail: String,
    pub url: Option<String>,
    pub title: Option<String>,
    /// Backend-neutral internal result payload. It is never appended to
    /// `detail` automatically and therefore is not model-visible.
    pub structured_output: Option<Value>,
}

impl fmt::Debug for BrowserActionResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BrowserActionResult")
            .field("detail", &self.detail)
            .field("url", &self.url)
            .field("title", &self.title)
            .field(
                "structured_output_present",
                &self.structured_output.is_some(),
            )
            .finish()
    }
}

/// Deterministic structured extraction from the current browser page.
///
/// `expression` is a JavaScript expression that evaluates to a
/// JSON-compatible value (object, array, string, number, bool, or null).
/// Callers must not wrap it in `JSON.stringify`; the backend serializes.
/// Same mutating-risk class as [`BrowserAction::Eval`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserExtractRequest {
    pub expression: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserExtractResult {
    pub value: Value,
    pub truncated: bool,
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

    pub fn read(outline: bool, filter: Option<String>) -> Self {
        Self {
            kind: BrowserObserveKind::Read { outline, filter },
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
    /// Structured extract did not yield a JSON-compatible payload.
    InvalidStructuredOutput,
    /// Observation-scoped element handle is no longer valid. Session may
    /// still be healthy. Not retryable; caller must re-observe.
    StaleReference,
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
        let reserved = BrowserSessionKey::new("default").unwrap();
        let (kind, detail) = reserved
            .omninova_policy_error()
            .expect("default is reserved");
        assert_eq!(kind, BrowserErrorKind::Rejected);
        assert!(detail.starts_with("BrowserSessionInvalid:"));
        let handle =
            BackendSessionHandle::new(BrowserBackendId::agent_browser(), "default").unwrap();
        assert_eq!(handle.token(), "default");
        assert!(BrowserSessionKey::new("chat-1")
            .unwrap()
            .omninova_policy_error()
            .is_none());
    }

    #[test]
    fn action_result_debug_does_not_expose_internal_structured_output() {
        let result = BrowserActionResult {
            detail: "eval complete".into(),
            url: None,
            title: None,
            structured_output: Some(serde_json::json!({"secret": "DO_NOT_LOG"})),
        };
        let debug = format!("{result:?}");
        assert!(debug.contains("structured_output_present: true"));
        assert!(!debug.contains("DO_NOT_LOG"));
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
            BrowserAction::eval_raw("1+1"),
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
                action: None,
            },
            BrowserObserveKind::Read {
                outline: false,
                filter: None,
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
            "read",
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
        assert!(caps.supports_action(&BrowserAction::eval_raw("1")));
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
        assert!(!BrowserErrorKind::InvalidStructuredOutput.default_retryable());
        assert!(!BrowserErrorKind::StaleReference.default_retryable());
        assert!(BrowserErrorKind::Timeout.default_retryable());
        assert!(BrowserErrorKind::Crashed.default_retryable());
        assert_ne!(
            BrowserErrorKind::StaleReference,
            BrowserErrorKind::SessionNotFound
        );
    }

    #[test]
    fn eval_mode_does_not_change_action_name() {
        assert_eq!(BrowserAction::eval_raw("1+1").name(), "eval");
        assert_eq!(BrowserAction::eval_structured_json("{a:1}").name(), "eval");
        assert_eq!(
            BrowserAction::eval_raw("1"),
            BrowserAction::Eval {
                script: "1".into(),
                mode: BrowserEvalMode::Raw,
            }
        );
        assert_eq!(
            BrowserAction::eval_structured_json("1"),
            BrowserAction::Eval {
                script: "1".into(),
                mode: BrowserEvalMode::StructuredJson,
            }
        );
    }

    #[test]
    fn read_is_a_v1_observe_action() {
        assert!(V1_TOOL_ACTIONS.contains(&"read"));
        assert_eq!(V1_TOOL_ACTIONS.len(), 25);
        assert_eq!(
            BrowserObserveKind::Read {
                outline: false,
                filter: None,
            }
            .v1_action_name(),
            "read"
        );
        assert_eq!(
            route_v1_tool_action("read"),
            Some(V1ToolActionRoute::Observe)
        );
        assert_eq!(
            route_v1_tool_action("get_text"),
            Some(V1ToolActionRoute::Observe)
        );
        assert_eq!(
            BrowserObserveKind::Text { target: None }.v1_action_name(),
            "get_text"
        );
    }
}
