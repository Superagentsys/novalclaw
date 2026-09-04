//! `AgentBrowserBackend`: the only module that speaks agent-browser CLI.
//!
//! Isolates `--session` / `--namespace` / `--json`, sidecar recovery, and
//! vendor JSON. BrowserRuntime and BrowserTool must not depend on those.

use crate::tools::browser_backend::{planned_backend_id_from_config, BrowserBackend};
use crate::tools::browser_bin::{
    AgentBrowserBinaryResolved, AgentBrowserResolveError, BrowserBinarySearch,
};
use crate::tools::browser_lifecycle::{
    auto_retry_allowed, classify_browser_output, failure_prefix, forget_owned_browser_session,
    is_retryable_action, probe_owned_session_pid, recover_owned_session,
    remember_owned_browser_session, run_command_with_timeout, BrowserFailureKind, ChildRunError,
    AGENT_BROWSER_NAMESPACE,
};
use crate::tools::browser_output::{cli_max_output_for_action, parse_action_outcome};
use crate::tools::browser_types::{
    BackendAvailability, BackendCapabilities, BackendSessionHandle, BrowserAction,
    BrowserActionResult, BrowserBackendError, BrowserBackendId, BrowserErrorKind, BrowserHealth,
    BrowserObservation, BrowserObserveKind, BrowserPageId, BrowserSessionKey,
    BrowserSessionOpenRequest, BrowserSessionOptions, BrowserSnapshot, BrowserTab, BrowserTarget,
    NavigateRequest, ObserveRequest, ScreenshotRequest, ScreenshotResult,
};
use crate::tools::configure_background_command;
use crate::tools::web_client::redact_secrets_in_text;
use async_trait::async_trait;
use std::io::ErrorKind;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;

const DEFAULT_TIMEOUT_SECS: u64 = 30;
pub const BROWSER_SESSION_PREFIX: &str = "omninova";
pub const BROWSER_SESSION_HASH_CHARS: usize = 20;
const BROWSER_SESSION_MAX_LEN: usize = 64;

/// Map an OmniNova logical session to a stable agent-browser CLI session name.
pub fn browser_session_id(session_id: Option<&str>) -> Result<String, String> {
    let Some(raw) = session_id else {
        return Err(browser_session_missing_error());
    };
    if raw.trim().is_empty() {
        return Err(browser_session_missing_error());
    }
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(raw.as_bytes());
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let hex = &hex[..BROWSER_SESSION_HASH_CHARS.min(hex.len())];
    Ok(format!("{BROWSER_SESSION_PREFIX}-{hex}"))
}

pub fn browser_session_missing_error() -> String {
    crate::tools::browser_types::BROWSER_SESSION_MISSING_DETAIL.to_string()
}

pub(crate) fn validate_cli_session_name(session: &str) -> Result<(), String> {
    if session.is_empty() {
        return Err(
            "BrowserSessionInvalid: session name is empty; refusing to use the agent-browser default session"
                .into(),
        );
    }
    if session.eq_ignore_ascii_case("default") {
        return Err(
            "BrowserSessionInvalid: refusing to use the agent-browser default session".into(),
        );
    }
    if session.len() > BROWSER_SESSION_MAX_LEN {
        return Err(format!(
            "BrowserSessionInvalid: session name exceeds {BROWSER_SESSION_MAX_LEN} characters"
        ));
    }
    if !session
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "BrowserSessionInvalid: session name contains characters that are not safe to pass to the agent-browser CLI"
                .into(),
        );
    }
    Ok(())
}

pub fn browser_session_prefix() -> &'static str {
    BROWSER_SESSION_PREFIX
}

pub fn browser_session_hash_chars() -> usize {
    BROWSER_SESSION_HASH_CHARS
}

pub struct AgentBrowserBackend {
    search: Option<BrowserBinarySearch>,
    defaults: BrowserSessionOptions,
}

impl AgentBrowserBackend {
    pub fn new(search: Option<BrowserBinarySearch>, defaults: BrowserSessionOptions) -> Self {
        Self { search, defaults }
    }

    fn id_value(&self) -> BrowserBackendId {
        BrowserBackendId::agent_browser()
    }

    fn resolve_binary(&self) -> Result<AgentBrowserBinaryResolved, BrowserBackendError> {
        let search = self
            .search
            .clone()
            .unwrap_or_else(BrowserBinarySearch::from_process);
        search
            .resolve()
            .map_err(|err| resolve_error(self.id_value(), err))
    }

    fn cli_session(&self, key: &BrowserSessionKey) -> Result<String, BrowserBackendError> {
        let mapped = browser_session_id(Some(key.as_str())).map_err(|detail| {
            BrowserBackendError::new(BrowserErrorKind::SessionNotFound, self.id_value(), detail)
        })?;
        validate_cli_session_name(&mapped).map_err(|detail| {
            BrowserBackendError::new(BrowserErrorKind::Rejected, self.id_value(), detail)
        })?;
        Ok(mapped)
    }

    fn handle_from_key(
        &self,
        key: &BrowserSessionKey,
    ) -> Result<BackendSessionHandle, BrowserBackendError> {
        let token = self.cli_session(key)?;
        BackendSessionHandle::new(self.id_value(), token).map_err(|err| {
            BrowserBackendError::new(BrowserErrorKind::Rejected, self.id_value(), err.to_string())
        })
    }

    fn target_arg(&self, target: &BrowserTarget) -> Result<String, BrowserBackendError> {
        match target {
            BrowserTarget::Element(reference) => {
                if reference.backend() != &self.id_value() {
                    return Err(BrowserBackendError::new(
                        BrowserErrorKind::Rejected,
                        self.id_value(),
                        format!(
                            "BrowserTargetBackendMismatch: ref backend={} expected={}",
                            reference.backend().as_str(),
                            self.id_value().as_str()
                        ),
                    ));
                }
                Ok(reference.as_str().to_string())
            }
            BrowserTarget::Css(selector) => Ok(selector.clone()),
            BrowserTarget::Role { role, name } => {
                let mut arg = role.clone();
                if let Some(name) = name {
                    arg.push(':');
                    arg.push_str(name);
                }
                Ok(arg)
            }
        }
    }

    fn optional_target_arg(
        &self,
        target: &Option<BrowserTarget>,
    ) -> Result<Option<String>, BrowserBackendError> {
        match target {
            Some(target) => Ok(Some(self.target_arg(target)?)),
            None => Ok(None),
        }
    }

    async fn run_named(
        &self,
        session: &BackendSessionHandle,
        opts: &BrowserSessionOptions,
        v1_action: &str,
        extra_args: &[String],
    ) -> Result<NamedSuccess, BrowserBackendError> {
        if session.backend() != &self.id_value() {
            return Err(BrowserBackendError::new(
                BrowserErrorKind::Rejected,
                self.id_value(),
                format!(
                    "BrowserTargetBackendMismatch: session backend={} expected={}",
                    session.backend().as_str(),
                    self.id_value().as_str()
                ),
            ));
        }
        let cli_session = session.token();
        validate_cli_session_name(cli_session).map_err(|detail| {
            BrowserBackendError::new(BrowserErrorKind::Rejected, self.id_value(), detail)
        })?;
        let retryable_action = is_retryable_action(v1_action);
        let mut recovered = false;
        let mut concurrent_followup = false;
        loop {
            match self
                .spawn_cli(cli_session, opts, v1_action, extra_args)
                .await
            {
                Ok((exit_success, stdout, stderr, binary_path)) => {
                    let outcome = parse_action_outcome(v1_action, &stdout, &stderr, exit_success);
                    if outcome.success {
                        if v1_action == "close" {
                            forget_owned_browser_session(cli_session);
                        }
                        return Ok(NamedSuccess {
                            output: outcome.output,
                            stdout,
                        });
                    }
                    let diagnostic = format!(
                        "{} {}",
                        outcome.error_text.as_deref().unwrap_or_default(),
                        stderr
                    );
                    let kind = classify_browser_output(&diagnostic);
                    if auto_retry_allowed(
                        v1_action,
                        recovered,
                        concurrent_followup,
                        kind,
                        &diagnostic,
                    ) {
                        if !recovered {
                            recover_owned_session(cli_session);
                            recovered = true;
                        } else {
                            concurrent_followup = true;
                        }
                        continue;
                    }
                    if v1_action == "close"
                        && matches!(
                            kind,
                            BrowserFailureKind::DaemonUnavailable
                                | BrowserFailureKind::SessionUnavailable
                        )
                    {
                        recover_owned_session(cli_session);
                        forget_owned_browser_session(cli_session);
                        return Ok(NamedSuccess {
                            output: outcome.output,
                            stdout,
                        });
                    }
                    let prefix = failure_prefix(kind);
                    let detail = outcome
                        .error_text
                        .unwrap_or_else(|| "command failed".to_string());
                    let summary: String =
                        redact_secrets_in_text(&detail).chars().take(1200).collect();
                    return Err(exhausted(BrowserBackendError::new(
                        failure_kind(kind),
                        self.id_value(),
                        format!(
                            "{prefix}: requested_binary={} summary={summary}",
                            binary_path.display()
                        ),
                    )));
                }
                Err(err) => {
                    let msg = err.detail.clone();
                    if retryable_action && !recovered && msg.starts_with("BrowserCommandTimeout:") {
                        if recover_owned_session(cli_session) {
                            recovered = true;
                            continue;
                        }
                    }
                    return Err(exhausted(err));
                }
            }
        }
    }

    async fn spawn_cli(
        &self,
        cli_session: &str,
        opts: &BrowserSessionOptions,
        v1_action: &str,
        extra_args: &[String],
    ) -> Result<(bool, String, String, std::path::PathBuf), BrowserBackendError> {
        let resolved = self.resolve_binary()?;
        let mut cmd = Command::new(&resolved.path);
        configure_background_command(&mut cmd);
        if !opts.headless {
            cmd.arg("--headed");
        }
        cmd.arg("--session").arg(cli_session);
        cmd.arg("--namespace").arg(AGENT_BROWSER_NAMESPACE);
        if opts.attach_only {
            cmd.arg("--attach-only");
        }
        if let Some(cdp_url) = &opts.cdp_url {
            cmd.arg("--cdp-url").arg(cdp_url);
        }
        cmd.arg("--json");
        if let Some(max_output) = cli_max_output_for_action(v1_action) {
            cmd.arg("--max-output").arg(max_output);
        }
        for arg in extra_args {
            cmd.arg(arg);
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        remember_owned_browser_session(cli_session);
        match run_command_with_timeout(cmd, DEFAULT_TIMEOUT_SECS).await {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                Ok((output.status.success(), stdout, stderr, resolved.path))
            }
            Err(ChildRunError::Timeout { .. }) => Err(BrowserBackendError::new(
                BrowserErrorKind::Timeout,
                self.id_value(),
                format!(
                    "BrowserCommandTimeout: requested_binary={} timeout_secs={DEFAULT_TIMEOUT_SECS}",
                    resolved.path.display()
                ),
            )),
            Err(ChildRunError::Io(e)) => Err(spawn_error(self.id_value(), &resolved.path, e)),
        }
    }
}

fn exhausted(mut err: BrowserBackendError) -> BrowserBackendError {
    err.retryable = false;
    err
}

struct NamedSuccess {
    output: String,
    stdout: String,
}

fn failure_kind(kind: BrowserFailureKind) -> BrowserErrorKind {
    match kind {
        BrowserFailureKind::DaemonUnavailable => BrowserErrorKind::NotConnected,
        BrowserFailureKind::SessionUnavailable => BrowserErrorKind::SessionNotFound,
        BrowserFailureKind::Crashed => BrowserErrorKind::Crashed,
        BrowserFailureKind::Timeout => BrowserErrorKind::Timeout,
        BrowserFailureKind::CommandFailed => BrowserErrorKind::CommandFailed,
    }
}

fn resolve_error(id: BrowserBackendId, err: AgentBrowserResolveError) -> BrowserBackendError {
    let kind = match &err {
        AgentBrowserResolveError::Missing(_) => BrowserErrorKind::BinaryMissing,
        AgentBrowserResolveError::NotExecutable { .. } => BrowserErrorKind::BinaryMissing,
    };
    BrowserBackendError::new(kind, id, err.to_string())
}

fn spawn_error(id: BrowserBackendId, path: &Path, e: std::io::Error) -> BrowserBackendError {
    let requested = path.to_string_lossy();
    let (kind, detail) = match e.kind() {
        ErrorKind::NotFound => (
            BrowserErrorKind::BinaryMissing,
            format!(
                "BrowserBinaryMissing: requested_binary={requested} resolution_source=launch checked_candidates={requested}"
            ),
        ),
        ErrorKind::PermissionDenied => (
            BrowserErrorKind::BinaryMissing,
            format!("BrowserBinaryNotExecutable: requested_binary={requested} detail={e}"),
        ),
        _ => (
            BrowserErrorKind::LaunchFailed,
            format!("BrowserLaunchFailed: requested_binary={requested} detail={e}"),
        ),
    };
    BrowserBackendError::new(kind, id, detail)
}

fn observation(output: String) -> BrowserObservation {
    BrowserObservation {
        url: None,
        title: None,
        text: Some(output.clone()),
        snapshot: Some(BrowserSnapshot {
            text: output,
            elements: Vec::new(),
        }),
    }
}

fn action_result(output: String) -> BrowserActionResult {
    BrowserActionResult {
        detail: output,
        url: None,
        title: None,
    }
}

/// Production factory. Legacy `"playwright"` maps to this backend.
pub fn backend_from_config(
    backend_name: &str,
    search: Option<BrowserBinarySearch>,
    defaults: BrowserSessionOptions,
) -> Result<Arc<dyn BrowserBackend>, BrowserBackendError> {
    let trimmed = backend_name.trim();
    let id = planned_backend_id_from_config(Some(trimmed))?;
    if id == BrowserBackendId::agent_browser() {
        if trimmed.eq_ignore_ascii_case("playwright") {
            tracing::info!(
                target: "browser",
                "legacy backend 'playwright' mapped to 'agent-browser'"
            );
        }
        return Ok(Arc::new(AgentBrowserBackend::new(search, defaults)));
    }
    Err(BrowserBackendError::new(
        BrowserErrorKind::Rejected,
        id,
        format!("BrowserBackendUnsupported: {trimmed}"),
    ))
}

pub fn browser_backend_enabled(configured_enabled: bool, backend_name: &str) -> bool {
    if !configured_enabled {
        return false;
    }
    match planned_backend_id_from_config(Some(backend_name)) {
        Ok(id) if id == BrowserBackendId::agent_browser() => {
            crate::tools::browser_bin::effective_browser_capability(
                true,
                crate::tools::browser_bin::agent_browser_runtime_available(),
            )
        }
        _ => false,
    }
}

#[async_trait]
impl BrowserBackend for AgentBrowserBackend {
    fn id(&self) -> BrowserBackendId {
        self.id_value()
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
        match self.resolve_binary() {
            Ok(_) => BackendAvailability::Available,
            Err(err) => BackendAvailability::Unavailable {
                kind: err.kind,
                detail: err.detail,
            },
        }
    }

    async fn open_session(
        &self,
        req: &BrowserSessionOpenRequest,
    ) -> Result<BackendSessionHandle, BrowserBackendError> {
        let _ = &req.options;
        self.handle_from_key(&req.key)
    }

    async fn session_health(&self, session: &BackendSessionHandle) -> BrowserHealth {
        if session.backend() != &self.id_value() {
            return BrowserHealth::Unhealthy {
                kind: BrowserErrorKind::Rejected,
                detail: "session handle is not from this backend".into(),
            };
        }
        match probe_owned_session_pid(session.token()) {
            Some(true) => BrowserHealth::Healthy,
            Some(false) => BrowserHealth::Unhealthy {
                kind: BrowserErrorKind::Crashed,
                detail: "sidecar pid is not alive".into(),
            },
            None => BrowserHealth::Unhealthy {
                kind: BrowserErrorKind::SessionNotFound,
                detail: "no sidecar pid for session".into(),
            },
        }
    }

    async fn close_session(
        &self,
        session: &BackendSessionHandle,
    ) -> Result<(), BrowserBackendError> {
        let result = self
            .run_named(session, &self.defaults, "close", &["close".into()])
            .await?;
        let _ = result.output;
        Ok(())
    }

    async fn open(
        &self,
        session: &BackendSessionHandle,
        req: &NavigateRequest,
    ) -> Result<BrowserActionResult, BrowserBackendError> {
        let output = self
            .run_named(
                session,
                &self.defaults,
                "open",
                &["open".into(), req.url.clone()],
            )
            .await?
            .output;
        Ok(action_result(output))
    }

    async fn observe(
        &self,
        session: &BackendSessionHandle,
        req: &ObserveRequest,
    ) -> Result<BrowserObservation, BrowserBackendError> {
        let (v1_action, args) = self.observe_args(req)?;
        let output = self
            .run_named(session, &self.defaults, v1_action, &args)
            .await?
            .output;
        Ok(observation(output))
    }

    async fn act(
        &self,
        session: &BackendSessionHandle,
        action: &BrowserAction,
    ) -> Result<BrowserActionResult, BrowserBackendError> {
        let (v1_action, args) = self.act_args(action)?;
        let output = self
            .run_named(session, &self.defaults, v1_action, &args)
            .await?
            .output;
        Ok(action_result(output))
    }

    async fn screenshot(
        &self,
        session: &BackendSessionHandle,
        _req: &ScreenshotRequest,
    ) -> Result<ScreenshotResult, BrowserBackendError> {
        let output = self
            .run_named(
                session,
                &self.defaults,
                "screenshot",
                &["screenshot".into()],
            )
            .await?
            .output;
        Ok(ScreenshotResult { locator: output })
    }

    async fn tabs(
        &self,
        session: &BackendSessionHandle,
    ) -> Result<Vec<BrowserTab>, BrowserBackendError> {
        let result = self
            .run_named(session, &self.defaults, "tabs", &["tabs".into()])
            .await?;
        Ok(parse_tabs_output(&result.stdout, &result.output))
    }
}

impl AgentBrowserBackend {
    fn observe_args(
        &self,
        req: &ObserveRequest,
    ) -> Result<(&'static str, Vec<String>), BrowserBackendError> {
        match &req.kind {
            BrowserObserveKind::Snapshot => {
                let mut args = vec!["snapshot".into()];
                if req.interactive_only {
                    args.push("-i".into());
                }
                if req.compact {
                    args.push("-c".into());
                }
                Ok(("snapshot", args))
            }
            BrowserObserveKind::Text { target } => match self.optional_target_arg(target)? {
                Some(sel) => Ok(("get_text", vec!["get".into(), "text".into(), sel])),
                None => Ok(("get_text", vec!["read".into()])),
            },
            BrowserObserveKind::Html { target } => {
                let sel = self.optional_target_arg(target)?.ok_or_else(|| {
                    BrowserBackendError::new(
                        BrowserErrorKind::Rejected,
                        self.id_value(),
                        "get_html requires a target",
                    )
                })?;
                Ok(("get_html", vec!["get".into(), "html".into(), sel]))
            }
            BrowserObserveKind::Url => Ok(("get_url", vec!["get".into(), "url".into()])),
            BrowserObserveKind::Title => Ok(("get_title", vec!["get".into(), "title".into()])),
            BrowserObserveKind::Value { target } => {
                let sel = self.target_arg(target)?;
                Ok(("get_value", vec!["get".into(), "value".into(), sel]))
            }
            BrowserObserveKind::Visibility { target } => {
                let sel = self.target_arg(target)?;
                Ok(("is_visible", vec!["is".into(), "visible".into(), sel]))
            }
            BrowserObserveKind::Enabled { target } => {
                let sel = self.target_arg(target)?;
                Ok(("is_enabled", vec!["is".into(), "enabled".into(), sel]))
            }
            BrowserObserveKind::Find { role, name, action } => {
                let find_action = action.as_deref().unwrap_or("text");
                let mut args = vec![
                    "find".into(),
                    "role".into(),
                    role.clone(),
                    find_action.into(),
                ];
                if let Some(name) = name {
                    args.push("--name".into());
                    args.push(name.clone());
                }
                Ok(("find", args))
            }
        }
    }

    fn act_args(
        &self,
        action: &BrowserAction,
    ) -> Result<(&'static str, Vec<String>), BrowserBackendError> {
        match action {
            BrowserAction::Click { target } => {
                Ok(("click", vec!["click".into(), self.target_arg(target)?]))
            }
            BrowserAction::Fill { target, value } => Ok((
                "fill",
                vec!["fill".into(), self.target_arg(target)?, value.clone()],
            )),
            BrowserAction::Type { target, text } => Ok((
                "type",
                vec!["type".into(), self.target_arg(target)?, text.clone()],
            )),
            BrowserAction::Press { key } => Ok(("press", vec!["press".into(), key.clone()])),
            BrowserAction::Scroll {
                direction,
                pixels,
                target: _,
            } => {
                let dir = match direction {
                    crate::tools::browser_types::ScrollDirection::Up => "up",
                    crate::tools::browser_types::ScrollDirection::Down => "down",
                    crate::tools::browser_types::ScrollDirection::Left => "left",
                    crate::tools::browser_types::ScrollDirection::Right => "right",
                };
                let mut args = vec!["scroll".into(), dir.into()];
                if let Some(px) = pixels {
                    args.push(px.to_string());
                }
                Ok(("scroll", args))
            }
            BrowserAction::Select { target, value } => Ok((
                "select",
                vec!["select".into(), self.target_arg(target)?, value.clone()],
            )),
            BrowserAction::Hover { target } => {
                Ok(("hover", vec!["hover".into(), self.target_arg(target)?]))
            }
            BrowserAction::Eval { script } => Ok(("eval", vec!["eval".into(), script.clone()])),
            BrowserAction::Wait {
                timeout_ms,
                text,
                target,
            } => {
                let args = if let Some(text) = text {
                    vec!["wait".into(), "--text".into(), text.clone()]
                } else if let Some(ms) = timeout_ms {
                    vec!["wait".into(), ms.to_string()]
                } else if let Some(sel) = self.optional_target_arg(target)? {
                    vec!["wait".into(), sel]
                } else {
                    vec!["wait".into(), "1000".into()]
                };
                Ok(("wait", args))
            }
            BrowserAction::Back => Ok(("back", vec!["back".into()])),
            BrowserAction::Forward => Ok(("forward", vec!["forward".into()])),
            BrowserAction::Reload => Ok(("reload", vec!["reload".into()])),
        }
    }
}

fn parse_tabs_output(stdout: &str, normalized: &str) -> Vec<BrowserTab> {
    let raw = if stdout.trim().starts_with('{') || stdout.trim().starts_with('[') {
        stdout
    } else {
        normalized
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw.trim()) else {
        return Vec::new();
    };
    let rows = value
        .get("data")
        .and_then(|data| data.get("tabs").or_else(|| data.as_array().map(|_| data)))
        .and_then(|tabs| tabs.as_array())
        .cloned()
        .or_else(|| value.as_array().cloned())
        .unwrap_or_default();
    rows.into_iter()
        .enumerate()
        .filter_map(|(idx, row)| {
            let id = row
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("tab-{idx}"));
            let page_id = BrowserPageId::new(id).ok()?;
            Some(BrowserTab {
                id: page_id,
                url: row
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                title: row
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                active: row.get("active").and_then(|v| v.as_bool()).unwrap_or(false),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::browser_types::BrowserElementRef;

    #[test]
    fn session_mapping_is_deterministic() {
        let a = browser_session_id(Some("chat/room 1")).unwrap();
        let b = browser_session_id(Some("chat/room 2")).unwrap();
        assert_ne!(a, b);
        assert!(a.starts_with("omninova-"));
        assert_eq!(a, browser_session_id(Some("chat/room 1")).unwrap());
        validate_cli_session_name(&a).unwrap();
        assert!(browser_session_id(None)
            .unwrap_err()
            .starts_with("BrowserSessionMissing:"));
    }

    #[test]
    fn open_session_is_lazy_and_stable() {
        let backend = AgentBrowserBackend::new(None, BrowserSessionOptions::default());
        let key = BrowserSessionKey::new("logical-chat").unwrap();
        let req = BrowserSessionOpenRequest::new(key, BrowserSessionOptions::default());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let first = rt.block_on(backend.open_session(&req)).unwrap();
        let second = rt.block_on(backend.open_session(&req)).unwrap();
        assert_eq!(first, second);
        assert!(first.token().starts_with("omninova-"));
    }

    #[test]
    fn backend_opaque_token_default_is_allowed() {
        let handle =
            BackendSessionHandle::new(BrowserBackendId::agent_browser(), "default").unwrap();
        assert_eq!(handle.token(), "default");
        let backend = AgentBrowserBackend::new(None, BrowserSessionOptions::default());
        let key = BrowserSessionKey::new("logical-chat").unwrap();
        let req = BrowserSessionOpenRequest::new(key.clone(), BrowserSessionOptions::default());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let handle = rt.block_on(backend.open_session(&req)).unwrap();
        let again = rt.block_on(backend.open_session(&req)).unwrap();
        assert_eq!(handle, again);
        assert_eq!(
            handle.token(),
            browser_session_id(Some(key.as_str())).unwrap()
        );
    }

    #[test]
    fn foreign_element_ref_is_rejected() {
        let backend = AgentBrowserBackend::new(None, BrowserSessionOptions::default());
        let foreign = BrowserTarget::Element(BrowserElementRef::new(
            BrowserBackendId::personal_chrome(),
            "@e2",
        ));
        let err = backend.target_arg(&foreign).unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::Rejected);
        assert!(err.detail.contains("BrowserTargetBackendMismatch"));
    }

    #[test]
    fn playwright_config_selects_agent_browser() {
        let backend =
            backend_from_config("playwright", None, BrowserSessionOptions::default()).unwrap();
        assert_eq!(backend.id(), BrowserBackendId::agent_browser());
        let err =
            match backend_from_config("personal-chrome", None, BrowserSessionOptions::default()) {
                Ok(_) => panic!("personal-chrome must not construct a backend"),
                Err(err) => err,
            };
        assert!(err.detail.contains("BrowserBackendUnsupported"));
        let err = match backend_from_config("unknown", None, BrowserSessionOptions::default()) {
            Ok(_) => panic!("unknown backend must not construct a backend"),
            Err(err) => err,
        };
        assert!(err.detail.contains("BrowserBackendUnsupported"));
    }

    #[test]
    fn parse_tabs_reads_vendor_json() {
        let stdout = r#"{"success":true,"data":{"tabs":[{"id":"1","url":"https://example.com/","title":"Example","active":true}]}}"#;
        let tabs = parse_tabs_output(stdout, "URL: https://example.com/");
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].url.as_deref(), Some("https://example.com/"));
        assert!(tabs[0].active);
    }

    #[tokio::test]
    async fn resolved_native_version_does_not_hang_under_create_no_window() {
        use crate::tools::browser_bin::BrowserBinarySearch;
        use tokio::time::{timeout, Duration};

        let Ok(resolved) = BrowserBinarySearch::from_process().resolve() else {
            eprintln!("skip: agent-browser unavailable");
            return;
        };
        let ext = resolved
            .path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        assert!(
            !ext.eq_ignore_ascii_case("cmd") && !ext.eq_ignore_ascii_case("bat"),
            "resolver must return a native binary, got {}",
            resolved.path.display()
        );

        let mut cmd = Command::new(&resolved.path);
        configure_background_command(&mut cmd);
        cmd.arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = timeout(Duration::from_secs(15), cmd.output())
            .await
            .expect("native --version must not hang under CREATE_NO_WINDOW")
            .expect("spawn native --version");
        assert!(
            output.status.success(),
            "stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(
            text.to_lowercase().contains("agent-browser") || text.contains("0."),
            "unexpected --version output: {text}"
        );
    }
}
