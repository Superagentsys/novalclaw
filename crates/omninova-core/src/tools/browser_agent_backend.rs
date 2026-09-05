//! `AgentBrowserBackend`: the only module that speaks agent-browser CLI.
//!
//! Isolates `--session` / `--namespace` / `--json`, sidecar recovery, and
//! vendor JSON. BrowserRuntime and BrowserTool must not depend on those.

use crate::tools::browser_backend::{planned_backend_id_from_config, BrowserBackend};
use crate::tools::browser_bin::{
    AgentBrowserBinaryResolved, AgentBrowserResolveError, BrowserBinarySearch,
};
use crate::tools::browser_executable::{
    browser_executable_argv, BrowserExecutable, BrowserExecutableResolver,
};
use crate::tools::browser_lifecycle::{
    auto_retry_allowed, classify_browser_output, failure_prefix, forget_owned_browser_session,
    is_retryable_action, probe_owned_session_pid, recover_owned_session,
    remember_owned_browser_session, run_command_with_timeout, BrowserFailureKind, ChildRunError,
    AGENT_BROWSER_NAMESPACE,
};
use crate::tools::browser_output::{
    cli_max_output_for_action, observation_from_read, parse_action_outcome, parse_read_output,
    parse_snapshot_refs, parse_structured_eval_wire, RawSnapshotRef, StructuredEvalWire,
    BROWSER_EXTRACT_SOURCE_CHAR_LIMIT,
};
use crate::tools::browser_profile::{
    paths_refer_to_same_location, BrowserProfileError, BrowserProfileResolver,
};
use crate::tools::browser_types::{
    BackendAvailability, BackendCapabilities, BackendSessionHandle, BrowserAction,
    BrowserActionResult, BrowserBackendError, BrowserBackendId, BrowserElement, BrowserElementRef,
    BrowserErrorKind, BrowserEvalMode, BrowserHealth, BrowserObservation, BrowserObserveKind,
    BrowserPageId, BrowserProfileRef, BrowserSessionKey, BrowserSessionOpenRequest,
    BrowserSessionOptions, BrowserSnapshot, BrowserTab, BrowserTarget, NavigateRequest,
    ObserveRequest, ScreenshotRequest, ScreenshotResult, BROWSER_EXTRACT_SOURCE_TOO_LARGE_DETAIL,
    BROWSER_PROFILE_BUSY_DETAIL, BROWSER_SESSION_PROFILE_MISMATCH_DETAIL,
    BROWSER_STALE_REFERENCE_DETAIL,
};
use crate::tools::configure_background_command;
use crate::tools::web_client::redact_secrets_in_text;
use async_trait::async_trait;
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
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
    last_stdout_len: Mutex<Option<usize>>,
    profile_resolver: BrowserProfileResolver,
    executable_resolver: BrowserExecutableResolver,
    launch_by_session: Mutex<HashMap<String, BoundSessionLaunch>>,
    #[cfg(test)]
    cli_invocations: Mutex<Vec<CliInvocationRecord>>,
}

/// Per-session launch identity. Bound at `open_session` and reused by every
/// later CLI spawn so open/eval cannot diverge. Profile path is argv-only
/// and must not be logged or returned to the model.
#[derive(Clone)]
struct BoundSessionLaunch {
    profile: Option<BrowserProfileRef>,
    profile_path: Option<PathBuf>,
    cli_profile_arg: Option<OsString>,
    headless: bool,
    attach_only: bool,
    cdp_url: Option<String>,
    executable: Option<BrowserExecutable>,
}

#[cfg(test)]
#[derive(Clone, Debug)]
struct CliInvocationRecord {
    operation: String,
    cli_session: String,
    namespace: String,
    profile_present: bool,
    profile_hash: Option<String>,
    headless: bool,
    #[allow(dead_code)]
    attach_only: bool,
    #[allow(dead_code)]
    cdp_url_present: bool,
    executable_present: bool,
    executable_source: Option<&'static str>,
    launch_fingerprint: String,
}

impl AgentBrowserBackend {
    pub fn new(search: Option<BrowserBinarySearch>, defaults: BrowserSessionOptions) -> Self {
        Self::new_with_executable_path(search, defaults, None)
    }

    pub fn new_with_executable_path(
        search: Option<BrowserBinarySearch>,
        defaults: BrowserSessionOptions,
        executable_path: Option<PathBuf>,
    ) -> Self {
        let executable_resolver =
            BrowserExecutableResolver::from_process(search.clone(), executable_path);
        Self::with_resolvers(
            search,
            defaults,
            BrowserProfileResolver::omninova_default(),
            executable_resolver,
        )
    }

    pub fn with_profile_resolver(
        search: Option<BrowserBinarySearch>,
        defaults: BrowserSessionOptions,
        profile_resolver: BrowserProfileResolver,
    ) -> Self {
        let executable_resolver = BrowserExecutableResolver::from_process(search.clone(), None);
        Self::with_resolvers(search, defaults, profile_resolver, executable_resolver)
    }

    fn with_resolvers(
        search: Option<BrowserBinarySearch>,
        defaults: BrowserSessionOptions,
        profile_resolver: BrowserProfileResolver,
        executable_resolver: BrowserExecutableResolver,
    ) -> Self {
        Self {
            search,
            defaults,
            last_stdout_len: Mutex::new(None),
            profile_resolver,
            executable_resolver,
            launch_by_session: Mutex::new(HashMap::new()),
            #[cfg(test)]
            cli_invocations: Mutex::new(Vec::new()),
        }
    }

    pub fn last_stdout_len(&self) -> Option<usize> {
        self.last_stdout_len.lock().ok().and_then(|guard| *guard)
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

    fn bind_session_launch(
        &self,
        token: &str,
        options: &BrowserSessionOptions,
        profile_path: Option<PathBuf>,
    ) -> Result<(), BrowserBackendError> {
        let mut map = self
            .launch_by_session
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        if let Some(existing) = map.get(token) {
            if existing.profile != options.profile {
                let mut err = BrowserBackendError::new(
                    BrowserErrorKind::Rejected,
                    self.id_value(),
                    BROWSER_SESSION_PROFILE_MISMATCH_DETAIL,
                );
                err.retryable = false;
                return Err(err);
            }
            return Ok(());
        }
        if let Some(path) = profile_path.as_ref() {
            let stale: Vec<String> = map
                .iter()
                .filter_map(|(other, cfg)| {
                    if other == token {
                        return None;
                    }
                    let other_path = cfg.profile_path.as_ref()?;
                    if !paths_refer_to_same_location(other_path, path) {
                        return None;
                    }
                    match probe_owned_session_pid(other) {
                        Some(false) => Some(other.clone()),
                        _ => None,
                    }
                })
                .collect();
            for session in stale {
                map.remove(&session);
            }
            let busy = map.iter().any(|(other, cfg)| {
                other != token
                    && cfg
                        .profile_path
                        .as_ref()
                        .is_some_and(|other_path| paths_refer_to_same_location(other_path, path))
            });
            if busy {
                return Err(profile_busy_error(self.id_value()));
            }
        }
        let cli_profile_arg = profile_path.as_ref().map(|path| profile_path_for_cli(path));
        map.insert(
            token.to_string(),
            BoundSessionLaunch {
                profile: options.profile.clone(),
                profile_path,
                cli_profile_arg,
                headless: options.headless,
                attach_only: options.attach_only,
                cdp_url: options.cdp_url.clone(),
                executable: None,
            },
        );
        Ok(())
    }

    async fn ensure_session_executable(
        &self,
        token: &str,
        v1_action: &str,
    ) -> Result<BoundSessionLaunch, BrowserBackendError> {
        let Some(bound) = self.session_launch(token) else {
            return Err(BrowserBackendError::new(
                BrowserErrorKind::SessionNotFound,
                self.id_value(),
                "BrowserSessionUnavailable: launch configuration is not bound",
            ));
        };
        if bound.attach_only || bound.cdp_url.is_some() || v1_action == "close" {
            return Ok(bound);
        }
        if bound.executable.is_some() {
            return Ok(bound);
        }
        if !cfg!(windows) && !self.executable_resolver.has_explicit() {
            return Ok(bound);
        }
        let executable = self.executable_resolver.resolve().await.map_err(|error| {
            let mut backend_error = BrowserBackendError::new(
                BrowserErrorKind::BrowserUnavailable,
                self.id_value(),
                error.detail,
            );
            backend_error.retryable = false;
            backend_error
        })?;
        let mut map = self
            .launch_by_session
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let Some(current) = map.get_mut(token) else {
            return Err(BrowserBackendError::new(
                BrowserErrorKind::SessionNotFound,
                self.id_value(),
                "BrowserSessionUnavailable: launch configuration was released",
            ));
        };
        if current.executable.is_none() {
            current.executable = Some(executable);
        }
        Ok(current.clone())
    }

    fn session_launch(&self, token: &str) -> Option<BoundSessionLaunch> {
        self.launch_by_session
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .get(token)
            .cloned()
    }

    fn session_profile_path(&self, token: &str) -> Option<PathBuf> {
        self.session_launch(token)
            .and_then(|cfg| cfg.profile_path)
    }

    fn unbind_session_launch(&self, token: &str) {
        if let Ok(mut map) = self.launch_by_session.lock() {
            map.remove(token);
        }
    }

    fn resolve_request_profile(
        &self,
        profile: &BrowserProfileRef,
    ) -> Result<PathBuf, BrowserBackendError> {
        match self.profile_resolver.resolve(profile) {
            Ok(resolved) => Ok(resolved.path),
            Err(BrowserProfileError::Invalid { detail }) => Err(BrowserBackendError::new(
                BrowserErrorKind::Rejected,
                self.id_value(),
                detail,
            )),
            Err(BrowserProfileError::EscapedRoot) => Err(BrowserBackendError::new(
                BrowserErrorKind::Rejected,
                self.id_value(),
                BrowserProfileError::EscapedRoot.to_string(),
            )),
            Err(BrowserProfileError::Io { detail }) => Err(BrowserBackendError::new(
                BrowserErrorKind::LaunchFailed,
                self.id_value(),
                format!(
                    "BrowserLaunchFailed: failed to prepare managed browser profile directory ({detail})"
                ),
            )),
        }
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
                Ok(cli_element_arg(reference.as_str()))
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
        let bound = self
            .ensure_session_executable(cli_session, v1_action)
            .await?;
        let profile_path = bound.profile_path.clone();
        let mut recovered = false;
        let mut concurrent_followup = false;
        loop {
            match self
                .spawn_cli(
                    cli_session,
                    opts,
                    Some(&bound),
                    v1_action,
                    extra_args,
                )
                .await
            {
                Ok((exit_success, stdout, stderr, binary_path)) => {
                    if let Ok(mut len) = self.last_stdout_len.lock() {
                        *len = Some(stdout.len());
                    }
                    let outcome = parse_action_outcome(v1_action, &stdout, &stderr, exit_success);
                    if outcome.success {
                        if v1_action == "close" {
                            forget_owned_browser_session(cli_session);
                            self.unbind_session_launch(cli_session);
                        }
                        return Ok(NamedSuccess {
                            output: outcome.output,
                            stdout,
                            data: outcome.data,
                        });
                    }
                    let diagnostic = format!(
                        "{} {}",
                        outcome.error_text.as_deref().unwrap_or_default(),
                        stderr
                    );
                    if looks_like_agent_browser_stale_ref(&diagnostic) {
                        let summary: String = redact_secrets_in_text(
                            outcome.error_text.as_deref().unwrap_or("unknown ref"),
                        )
                        .chars()
                        .take(1200)
                        .collect();
                        let mut err = BrowserBackendError::new(
                            BrowserErrorKind::StaleReference,
                            self.id_value(),
                            format!(
                                "{BROWSER_STALE_REFERENCE_DETAIL}; requested_binary={} summary={summary}",
                                binary_path.display()
                            ),
                        );
                        err.retryable = false;
                        return Err(err);
                    }
                    if profile_path.is_some() && looks_like_managed_profile_busy(&diagnostic) {
                        return Err(exhausted(profile_busy_error(self.id_value())));
                    }
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
                        self.unbind_session_launch(cli_session);
                        return Ok(NamedSuccess {
                            output: outcome.output,
                            stdout,
                            data: outcome.data,
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
        bound: Option<&BoundSessionLaunch>,
        v1_action: &str,
        extra_args: &[String],
    ) -> Result<(bool, String, String, std::path::PathBuf), BrowserBackendError> {
        let resolved = self.resolve_binary()?;
        let headless = bound.map(|cfg| cfg.headless).unwrap_or(opts.headless);
        let attach_only = bound.map(|cfg| cfg.attach_only).unwrap_or(opts.attach_only);
        let cdp_url = bound
            .and_then(|cfg| cfg.cdp_url.as_deref())
            .or(opts.cdp_url.as_deref());
        let cli_profile_arg = bound.and_then(|cfg| cfg.cli_profile_arg.clone());
        let executable = bound.and_then(|cfg| cfg.executable.as_ref());
        let daemon_alive = probe_owned_session_pid(cli_session) == Some(true);
        let emit_launch_config = should_forward_local_launch_config(daemon_alive);
        let emit_profile = cli_profile_arg.is_some() && emit_launch_config;
        let emit_executable = executable.is_some() && emit_launch_config;

        let mut cmd = Command::new(&resolved.path);
        configure_background_command(&mut cmd);
        if !headless {
            cmd.arg("--headed");
        }
        cmd.arg("--session").arg(cli_session);
        cmd.arg("--namespace").arg(AGENT_BROWSER_NAMESPACE);
        if emit_profile {
            if let Some(profile_arg) = cli_profile_arg.as_ref() {
                apply_managed_profile_cli_arg(&mut cmd, profile_arg, self.id_value())?;
            }
        }
        if emit_executable {
            if let Some(executable) = executable {
                apply_browser_executable_cli_arg(
                    &mut cmd,
                    &executable.path,
                    self.id_value(),
                )?;
            }
        }
        if attach_only {
            cmd.arg("--attach-only");
        }
        if let Some(cdp_url) = cdp_url {
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
        #[cfg(test)]
        self.record_cli_invocation(
            v1_action,
            cli_session,
            emit_profile,
            cli_profile_arg.as_ref(),
            headless,
            attach_only,
            cdp_url.is_some(),
            emit_executable,
            executable,
        );
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

    #[cfg(test)]
    fn record_cli_invocation(
        &self,
        operation: &str,
        cli_session: &str,
        profile_present: bool,
        cli_profile_arg: Option<&OsString>,
        headless: bool,
        attach_only: bool,
        cdp_url_present: bool,
        executable_present: bool,
        executable: Option<&BrowserExecutable>,
    ) {
        let profile_hash = cli_profile_arg.map(|arg| hash_cli_profile_arg(arg));
        let executable_hash = executable.map(|value| hash_path(&value.path));
        let launch_fingerprint = format!(
            "session={cli_session}|ns={AGENT_BROWSER_NAMESPACE}|profile={}|headless={headless}|attach_only={attach_only}|cdp={cdp_url_present}|exe={}",
            profile_hash.as_deref().unwrap_or("none"),
            executable_hash.as_deref().unwrap_or("none")
        );
        if let Ok(mut log) = self.cli_invocations.lock() {
            log.push(CliInvocationRecord {
                operation: operation.to_string(),
                cli_session: cli_session.to_string(),
                namespace: AGENT_BROWSER_NAMESPACE.to_string(),
                profile_present,
                profile_hash,
                headless,
                attach_only,
                cdp_url_present,
                executable_present,
                executable_source: executable.map(|value| value.source.as_str()),
                launch_fingerprint,
            });
        }
    }

    #[cfg(test)]
    fn take_cli_invocations(&self) -> Vec<CliInvocationRecord> {
        self.cli_invocations
            .lock()
            .map(|mut log| std::mem::take(&mut *log))
            .unwrap_or_default()
    }
}

fn exhausted(mut err: BrowserBackendError) -> BrowserBackendError {
    err.retryable = false;
    err
}

fn profile_busy_error(backend: BrowserBackendId) -> BrowserBackendError {
    let mut err = BrowserBackendError::new(
        BrowserErrorKind::ProfileBusy,
        backend,
        BROWSER_PROFILE_BUSY_DETAIL,
    );
    err.retryable = false;
    err
}

/// Conservative 0.36.0 Chrome singleton signature. Not "any exit 21".
fn looks_like_managed_profile_busy(diagnostic: &str) -> bool {
    let lower = diagnostic.to_ascii_lowercase();
    lower.contains("chrome exited early")
        && lower.contains("exit code: 21")
        && lower.contains("devtoolsactiveport")
}

fn apply_managed_profile_cli_arg(
    cmd: &mut Command,
    profile_arg: &OsString,
    backend: BrowserBackendId,
) -> Result<(), BrowserBackendError> {
    let path = Path::new(profile_arg);
    if !path.is_absolute() {
        return Err(BrowserBackendError::new(
            BrowserErrorKind::Rejected,
            backend,
            "BrowserProfileRejected: managed profile path must be absolute",
        ));
    }
    cmd.arg("--profile");
    cmd.arg(profile_arg);
    Ok(())
}

fn apply_browser_executable_cli_arg(
    cmd: &mut Command,
    executable_path: &Path,
    backend: BrowserBackendId,
) -> Result<(), BrowserBackendError> {
    if !executable_path.is_absolute() {
        return Err(BrowserBackendError::new(
            BrowserErrorKind::Rejected,
            backend,
            "BrowserUnavailable: configured browser executable path must be absolute",
        ));
    }
    for arg in browser_executable_argv(Some(executable_path)) {
        cmd.arg(arg);
    }
    Ok(())
}

/// agent-browser 0.36.0 `launch_hash` hashes the `--profile` string. Windows
/// `canonicalize` yields `\\?\` verbatim paths; stripping that prefix keeps
/// open/eval argv bytes identical and matches Chrome `--user-data-dir`.
fn profile_path_for_cli(path: &Path) -> OsString {
    #[cfg(windows)]
    {
        let raw = path.as_os_str().to_string_lossy();
        if let Some(stripped) = raw.strip_prefix(r"\\?\") {
            if !stripped.starts_with("UNC\\") {
                let normalized = Path::new(stripped);
                if normalized.is_absolute() {
                    return OsString::from(stripped);
                }
            }
        }
    }
    path.as_os_str().to_os_string()
}

/// agent-browser 0.36.0 sends a daemon `launch` envelope whenever `--profile`
/// or `--executable-path` is present (`should_send_local_launch_config`). A
/// follow-up launch can relaunch Chrome onto about:blank. Once this session's
/// sidecar is alive, omit launch-affecting flags so later commands attach.
fn should_forward_local_launch_config(daemon_alive: bool) -> bool {
    !daemon_alive
}

#[cfg(test)]
fn hash_cli_profile_arg(arg: &OsString) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(arg.to_string_lossy().as_bytes());
    digest
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
fn hash_path(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(path.as_os_str().to_string_lossy().as_bytes());
    digest
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn managed_profile_argv(profile_path: Option<&Path>) -> Vec<std::ffi::OsString> {
    match profile_path {
        Some(path) => vec!["--profile".into(), profile_path_for_cli(path)],
        None => Vec::new(),
    }
}

struct NamedSuccess {
    output: String,
    stdout: String,
    data: Option<serde_json::Value>,
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
        truncated: false,
    }
}

fn snapshot_observation(
    backend: BrowserBackendId,
    output: String,
    stdout: &str,
) -> BrowserObservation {
    BrowserObservation {
        url: None,
        title: None,
        text: Some(output.clone()),
        snapshot: Some(BrowserSnapshot {
            text: output,
            elements: snapshot_elements_from_stdout(backend, stdout),
        }),
        truncated: false,
    }
}

/// Prefix `@` for agent-browser CLI element args. Runtime never adds or parses `@`.
/// Stored observe refs are opaque (`e8`); the V1 tool path may already pass `@e8`.
fn cli_element_arg(value: &str) -> String {
    if value.starts_with('@') {
        value.to_string()
    } else {
        format!("@{value}")
    }
}

/// agent-browser 0.36.0 `INTERACTIVE_ROLES`. Conservative metadata only:
/// `interactive == false` must not cause Runtime to refuse `BrowserAction`.
const AGENT_BROWSER_INTERACTIVE_ROLES: &[&str] = &[
    "button",
    "link",
    "textbox",
    "checkbox",
    "radio",
    "combobox",
    "listbox",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "option",
    "searchbox",
    "slider",
    "spinbutton",
    "switch",
    "tab",
    "treeitem",
    "Iframe",
];

fn is_agent_browser_interactive_role(role: &str) -> bool {
    AGENT_BROWSER_INTERACTIVE_ROLES.contains(&role)
}

fn snapshot_elements_from_stdout(backend: BrowserBackendId, stdout: &str) -> Vec<BrowserElement> {
    match parse_snapshot_refs(stdout) {
        Ok(raw) => raw
            .into_iter()
            .map(|entry| element_from_raw_ref(backend.clone(), entry))
            .collect(),
        Err(reason) => {
            tracing::debug!(
                target: "browser",
                reason,
                "structured snapshot refs unavailable; continuing with text observation"
            );
            Vec::new()
        }
    }
}

fn element_from_raw_ref(backend: BrowserBackendId, entry: RawSnapshotRef) -> BrowserElement {
    BrowserElement {
        reference: BrowserElementRef::new(backend, entry.id),
        interactive: entry
            .role
            .as_deref()
            .is_some_and(is_agent_browser_interactive_role),
        role: entry.role,
        name: entry.name,
    }
}

fn action_result(output: String) -> BrowserActionResult {
    BrowserActionResult {
        detail: output,
        url: None,
        title: None,
        structured_output: None,
    }
}

fn read_cli_args(outline: bool, filter: Option<&str>) -> Vec<String> {
    let mut args = vec!["read".into()];
    if outline {
        args.push("--outline".into());
    }
    if let Some(filter) = filter {
        args.push("--filter".into());
        args.push(filter.to_string());
    }
    args
}

/// agent-browser 0.36.0 element.rs: `Unknown ref: {id}`, role/name fallback
/// failure, and missing objectId after a stored ref. Not session loss.
fn looks_like_agent_browser_stale_ref(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("unknown ref:")
        || lower.contains("could not locate element with role=")
        || lower.contains("no objectid for ref ")
        || lower.contains("ax node has no backenddomnodeid")
}

/// Production factory. Legacy `"playwright"` maps to this backend.
pub fn backend_from_config(
    backend_name: &str,
    search: Option<BrowserBinarySearch>,
    defaults: BrowserSessionOptions,
) -> Result<Arc<dyn BrowserBackend>, BrowserBackendError> {
    backend_from_config_with_executable(backend_name, search, defaults, None)
}

pub fn backend_from_config_with_executable(
    backend_name: &str,
    search: Option<BrowserBinarySearch>,
    defaults: BrowserSessionOptions,
    executable_path: Option<PathBuf>,
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
        return Ok(Arc::new(AgentBrowserBackend::new_with_executable_path(
            search,
            defaults,
            executable_path,
        )));
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
            profiles: true,
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
        let handle = self.handle_from_key(&req.key)?;
        let profile_path = match &req.options.profile {
            Some(profile) => Some(self.resolve_request_profile(profile)?),
            None => None,
        };
        self.bind_session_launch(handle.token(), &req.options, profile_path)?;
        Ok(handle)
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
        let result = self
            .run_named(session, &self.defaults, v1_action, &args)
            .await?;
        if matches!(req.kind, BrowserObserveKind::Snapshot) {
            Ok(snapshot_observation(
                self.id_value(),
                result.output,
                &result.stdout,
            ))
        } else if matches!(req.kind, BrowserObserveKind::Read { .. }) {
            Ok(match parse_read_output(&result.stdout) {
                Ok(parsed) => observation_from_read(parsed),
                Err(reason) => {
                    tracing::debug!(
                        target: "browser",
                        reason,
                        "typed read output unavailable; using normalized text"
                    );
                    BrowserObservation {
                        url: None,
                        title: None,
                        text: Some(result.output),
                        snapshot: None,
                        truncated: false,
                    }
                }
            })
        } else {
            Ok(observation(result.output))
        }
    }

    async fn act(
        &self,
        session: &BackendSessionHandle,
        action: &BrowserAction,
    ) -> Result<BrowserActionResult, BrowserBackendError> {
        let (v1_action, args) = self.act_args(action)?;
        let result = self
            .run_named(session, &self.defaults, v1_action, &args)
            .await?;
        let structured_output = match action {
            BrowserAction::Eval {
                mode: BrowserEvalMode::StructuredJson,
                ..
            } => {
                return structured_eval_action_result(self.id_value(), result);
            }
            _ => structured_output_for_action(action, result.data.as_ref()),
        };
        Ok(BrowserActionResult {
            detail: result.output,
            url: None,
            title: None,
            structured_output,
        })
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
            BrowserObserveKind::Read { outline, filter } => {
                Ok(("read", read_cli_args(*outline, filter.as_deref())))
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
            BrowserAction::Eval { script, mode } => match mode {
                BrowserEvalMode::Raw => Ok(("eval", vec!["eval".into(), script.clone()])),
                BrowserEvalMode::StructuredJson => Ok((
                    "eval",
                    vec!["eval".into(), wrap_structured_eval_expression(script)],
                )),
            },
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

fn structured_output_for_action(
    action: &BrowserAction,
    data: Option<&serde_json::Value>,
) -> Option<serde_json::Value> {
    matches!(
        action,
        BrowserAction::Eval {
            mode: BrowserEvalMode::Raw,
            ..
        }
    )
    .then(|| data.and_then(|value| value.get("result")).cloned())
    .flatten()
}

fn structured_eval_action_result(
    backend: BrowserBackendId,
    result: NamedSuccess,
) -> Result<BrowserActionResult, BrowserBackendError> {
    let wire = result
        .data
        .as_ref()
        .and_then(|data| data.get("result"))
        .map(parse_structured_eval_wire)
        .unwrap_or(StructuredEvalWire::InvalidEnvelope);
    match wire {
        StructuredEvalWire::Value(value) => Ok(BrowserActionResult {
            detail: result.output,
            url: None,
            title: None,
            structured_output: Some(value),
        }),
        StructuredEvalWire::SourceTooLarge { observed_chars } => {
            let mut err = BrowserBackendError::new(
                BrowserErrorKind::CommandFailed,
                backend,
                match observed_chars {
                    Some(chars) => {
                        format!("{BROWSER_EXTRACT_SOURCE_TOO_LARGE_DETAIL}; observed_chars={chars}")
                    }
                    None => BROWSER_EXTRACT_SOURCE_TOO_LARGE_DETAIL.to_string(),
                },
            );
            err.retryable = false;
            Err(err)
        }
        StructuredEvalWire::NotSerializable { detail } => {
            let mut err = BrowserBackendError::new(
                BrowserErrorKind::InvalidStructuredOutput,
                backend,
                format!(
                    "BrowserStructuredOutputInvalid: structured extract result is not JSON-serializable; {detail}"
                ),
            );
            err.retryable = false;
            Err(err)
        }
        StructuredEvalWire::InvalidEnvelope => {
            let mut err = BrowserBackendError::new(
                BrowserErrorKind::InvalidStructuredOutput,
                backend,
                "BrowserStructuredOutputInvalid: structured extract envelope is missing or invalid",
            );
            err.retryable = false;
            Err(err)
        }
    }
}

/// Compatibility workaround for agent-browser 0.36.0 JSON eval path ignoring
/// `--max-output`. Expression is spliced as an expression body (not regex-
/// escaped) so quotes / backticks remain JS source. Evaluates once, then
/// `JSON.stringify`s a protocol envelope in page context. Oversized results
/// return a small `source_too_large` envelope instead of the huge value.
///
/// Future upgrades should re-smoke `eval --json --max-output`. If upstream
/// JSON mode starts honoring that flag, re-evaluate removing this wrapper.
/// Do not branch on CLI version.
pub(crate) fn wrap_structured_eval_expression(expression: &str) -> String {
    let mut out = String::new();
    out.push_str("(async () => {\n");
    out.push_str("  const SOURCE_LIMIT = ");
    out.push_str(&BROWSER_EXTRACT_SOURCE_CHAR_LIMIT.to_string());
    out.push_str(";\n");
    out.push_str(
        r#"  const fail = (error, extra) => JSON.stringify(Object.assign({
    "__omninova_extract_v": 1,
    ok: false,
    error
  }, extra || {}));
  let value;
  try {
    value = await (async () => {
      return ("#,
    );
    out.push_str(expression);
    out.push_str(
        r#");
    })();
  } catch (err) {
    return fail("not_json_serializable", { detail: String((err && err.message) || err).slice(0, 200) });
  }
  if (value === undefined || typeof value === "function" || typeof value === "symbol") {
    return fail("not_json_serializable", { detail: typeof value });
  }
  let serialized;
  try {
    const envelope = { "__omninova_extract_v": 1, ok: true };
    envelope.value = value;
    serialized = JSON.stringify(envelope);
  } catch (err) {
    return fail("not_json_serializable", { detail: String((err && err.message) || err).slice(0, 200) });
  }
  if (typeof serialized !== "string") {
    return fail("not_json_serializable", { detail: "stringify returned non-string" });
  }
  if (serialized.length > SOURCE_LIMIT) {
    return fail("source_too_large", { observed_chars: serialized.length });
  }
  return serialized;
})()"#,
    );
    out
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
    use crate::tools::browser_bin::bundled_agent_browser_relative_path;
    use crate::tools::browser_executable::{
        BrowserExecutableProbe, BrowserExecutableProbeResult, BrowserExecutableSource,
    };
    use crate::tools::browser_output::agent_browser_0_36_snapshot_stdout;
    use crate::tools::browser_output::BROWSER_EXTRACT_SOURCE_CHAR_LIMIT;
    use crate::tools::browser_profile::BrowserProfileResolver;
    use crate::tools::browser_runtime::{BrowserRuntime, BrowserRuntimePolicy};
    use crate::tools::browser_types::{
        BrowserElement, BrowserElementRef, BrowserErrorKind, BrowserExtractRequest,
        BrowserProfileRef, BrowserSessionKey, BrowserSessionOpenRequest,
        BrowserSessionOptions, BROWSER_PROFILE_BUSY_DETAIL,
        BROWSER_SESSION_PROFILE_MISMATCH_DETAIL,
    };
    use serde_json::{json, Value};

    struct PassingExecutableProbe {
        calls: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl BrowserExecutableProbe for PassingExecutableProbe {
        async fn probe(&self, _candidate: &Path) -> BrowserExecutableProbeResult {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            BrowserExecutableProbeResult {
                usable: true,
                diagnostic: "ok",
            }
        }
    }

    fn native_cli_search() -> Option<BrowserBinarySearch> {
        let resource_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/omninova-tauri/src-tauri/resources");
        let bundled = resource_root.join(bundled_agent_browser_relative_path());
        if bundled.is_file() {
            return Some(BrowserBinarySearch {
                env_path: None,
                bundled_candidates: Vec::new(),
                extra_roots: vec![resource_root],
                include_exe_relative: false,
                path_dirs: Some(Vec::new()),
            });
        }
        let resolved = BrowserBinarySearch::from_process().resolve().ok()?;
        Some(BrowserBinarySearch {
            env_path: Some(resolved.path),
            bundled_candidates: Vec::new(),
            extra_roots: Vec::new(),
            include_exe_relative: false,
            path_dirs: Some(Vec::new()),
        })
    }

    #[test]
    fn eval_maps_agent_browser_data_result_without_changing_text_output() {
        let action = BrowserAction::eval_raw("JSON.stringify({name:'Alpha'})");
        let data = json!({
            "result": "{\"name\":\"Alpha\"}",
            "origin": "about:blank",
            "lifecycle": {"reused": true}
        });
        assert_eq!(
            structured_output_for_action(&action, Some(&data)),
            Some(json!("{\"name\":\"Alpha\"}"))
        );
        assert_eq!(
            crate::tools::browser_output::normalize_action("eval", &data),
            "{\"name\":\"Alpha\"}"
        );
    }

    #[tokio::test]
    #[ignore = "requires a live agent-browser + Chromium runtime"]
    async fn real_runtime_extract_json_returns_typed_page_data() {
        let Some(search) = native_cli_search() else {
            eprintln!("skip runtime-dependent test: agent-browser unavailable");
            return;
        };
        let page_port = crate::tools::web_client::tests::spawn_test_server(|_, stream| {
            crate::tools::web_client::tests::write_response(
                stream,
                "HTTP/1.1 200 OK",
                "<html><body><div class='product'><span class='name'>Alpha</span><span class='price'>12.50</span></div><div class='product'><span class='name'>你好🚀</span><span class='price'>7</span></div></body></html>",
                &["content-type: text/html; charset=utf-8".to_string()],
            );
        });
        let backend = Arc::new(AgentBrowserBackend::new(
            Some(search),
            BrowserSessionOptions::default(),
        ));
        let runtime = BrowserRuntime::new(
            backend,
            BrowserRuntimePolicy {
                allowed_domains: vec!["127.0.0.1".into()],
                ..BrowserRuntimePolicy::default()
            },
        );
        let key = BrowserSessionKey::new(format!("b32d-{}", uuid::Uuid::new_v4())).unwrap();
        let opts = BrowserSessionOptions::default();
        runtime
            .open(
                &key,
                &opts,
                &NavigateRequest {
                    url: format!("http://127.0.0.1:{page_port}/products"),
                },
            )
            .await
            .expect("open fixture");
        let extracted = runtime
            .extract_json(
                &key,
                &opts,
                &BrowserExtractRequest {
                    expression: "Array.from(document.querySelectorAll('.product')).map(product => ({name: product.querySelector('.name').textContent, price: Number(product.querySelector('.price').textContent), active: true, note: null}))".into(),
                },
            )
            .await
            .expect("structured extract");
        let _ = runtime.close_session(&key, &opts).await;

        assert!(!extracted.truncated);
        assert_eq!(
            extracted.value,
            json!([
                {"name": "Alpha", "price": 12.5, "active": true, "note": null},
                {"name": "你好🚀", "price": 7, "active": true, "note": null}
            ])
        );
    }

    fn wire_named(result: serde_json::Value) -> NamedSuccess {
        NamedSuccess {
            output: "ok".into(),
            stdout: "{}".into(),
            data: Some(json!({ "result": result })),
        }
    }

    #[test]
    fn raw_eval_does_not_use_structured_wrapper() {
        let backend = AgentBrowserBackend::new(None, BrowserSessionOptions::default());
        let action = BrowserAction::eval_raw("1 + 1");
        let (name, args) = backend.act_args(&action).unwrap();
        assert_eq!(name, "eval");
        assert_eq!(args, vec!["eval", "1 + 1"]);
        assert!(!args.iter().any(|arg| arg.contains("__omninova_extract_v")));
        assert!(!args.iter().any(|arg| arg.contains("SOURCE_LIMIT")));
        assert!(!wrap_structured_eval_expression("1").contains("1 + 1"));
    }

    #[test]
    fn structured_eval_wraps_expression_as_source_body() {
        let backend = AgentBrowserBackend::new(None, BrowserSessionOptions::default());
        let expression =
            r#""hello 'world' `tick`" || (1 ? await Promise.resolve({ok: true}) : null)"#;
        let (_, args) = backend
            .act_args(&BrowserAction::eval_structured_json(expression))
            .unwrap();
        assert_eq!(args[0], "eval");
        let wrapped = &args[1];
        assert!(wrapped.contains("__omninova_extract_v"));
        assert!(wrapped.contains("SOURCE_LIMIT"));
        assert!(wrapped.contains(&BROWSER_EXTRACT_SOURCE_CHAR_LIMIT.to_string()));
        assert!(wrapped.contains(expression));
        assert!(wrapped.contains("return ("));
        assert_eq!(wrapped.matches("await (async () =>").count(), 1);
        assert!(wrapped.contains("JSON.stringify(envelope)"));
    }

    #[test]
    fn structured_expression_executes_exactly_once() {
        let expression = "window.__calls = (window.__calls || 0) + 1";
        let wrapped = wrap_structured_eval_expression(expression);
        assert_eq!(wrapped.matches(expression).count(), 1);
        assert_eq!(wrapped.matches("return (").count(), 1);
        assert!(!wrapped.contains(expression.repeat(2).as_str()));
    }

    #[test]
    fn promise_result_is_awaited() {
        let wrapped = wrap_structured_eval_expression("await Promise.resolve({ awaited: true })");
        assert!(wrapped.contains("await (async () =>"));
        assert!(wrapped.contains("await Promise.resolve({ awaited: true })"));
        assert_eq!(
            wrapped
                .matches("await Promise.resolve({ awaited: true })")
                .count(),
            1
        );
    }

    #[test]
    fn cyclic_value_returns_serialization_error() {
        let envelope = json!({
            "__omninova_extract_v": 1,
            "ok": false,
            "error": "not_json_serializable",
            "detail": "Converting circular structure to JSON"
        });
        let err = structured_eval_action_result(
            BrowserBackendId::agent_browser(),
            wire_named(Value::String(envelope.to_string())),
        )
        .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::InvalidStructuredOutput);
        assert!(!err.retryable);
        assert!(err.detail.contains("not JSON-serializable"));
        assert!(!err.detail.contains(&"X".repeat(50)));
    }

    #[test]
    fn bigint_returns_serialization_error() {
        let envelope = json!({
            "__omninova_extract_v": 1,
            "ok": false,
            "error": "not_json_serializable",
            "detail": "Do not know how to serialize a BigInt"
        });
        let err = structured_eval_action_result(
            BrowserBackendId::agent_browser(),
            wire_named(Value::String(envelope.to_string())),
        )
        .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::InvalidStructuredOutput);
        assert!(!err.retryable);
    }

    #[test]
    fn source_too_large_wire_is_command_failed_and_not_retryable() {
        let envelope = json!({
            "__omninova_extract_v": 1,
            "ok": false,
            "error": "source_too_large",
            "observed_chars": 5_000_000
        });
        let err = structured_eval_action_result(
            BrowserBackendId::agent_browser(),
            wire_named(Value::String(envelope.to_string())),
        )
        .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::CommandFailed);
        assert!(!err.retryable);
        assert!(err.detail.contains("source limit"));
        assert!(err.detail.contains("observed_chars=5000000"));
        assert!(!err.detail.contains(&"X".repeat(100)));
    }

    #[test]
    fn json_compatibility_policy_is_json_stringify() {
        let wrapped = wrap_structured_eval_expression("value");
        assert!(wrapped.contains("JSON.stringify(envelope)"));
        assert!(wrapped.contains("value === undefined"));
        assert!(wrapped.contains("typeof value === \"function\""));
        assert!(wrapped.contains("typeof value === \"symbol\""));
        assert!(wrapped.contains(".slice(0, 200)"));
    }

    #[tokio::test]
    #[ignore = "requires a live agent-browser + Chromium runtime"]
    async fn real_structured_eval_source_bound_rejects_five_megabyte_payload() {
        let Some(search) = native_cli_search() else {
            eprintln!("skip runtime-dependent test: agent-browser unavailable");
            return;
        };
        let page_port = crate::tools::web_client::tests::spawn_test_server(|_, stream| {
            crate::tools::web_client::tests::write_response(
                stream,
                "HTTP/1.1 200 OK",
                "<html><body><p>source-bound</p></body></html>",
                &["content-type: text/html".to_string()],
            );
        });
        let backend = Arc::new(AgentBrowserBackend::new(
            Some(search),
            BrowserSessionOptions::default(),
        ));
        let runtime = BrowserRuntime::new(
            backend.clone(),
            BrowserRuntimePolicy {
                allowed_domains: vec!["127.0.0.1".into()],
                ..BrowserRuntimePolicy::default()
            },
        );
        let key = BrowserSessionKey::new(format!("b32d1-{}", uuid::Uuid::new_v4())).unwrap();
        let opts = BrowserSessionOptions::default();
        runtime
            .open(
                &key,
                &opts,
                &NavigateRequest {
                    url: format!("http://127.0.0.1:{page_port}/blank"),
                },
            )
            .await
            .expect("open");
        const PAGE_VALUE_SIZE: usize = 5_000_000;
        let err = runtime
            .extract_json(
                &key,
                &opts,
                &BrowserExtractRequest {
                    expression: format!("({{ blob: 'X'.repeat({PAGE_VALUE_SIZE}) }})"),
                },
            )
            .await
            .expect_err("5MB extract must fail at source bound");
        let stdout_len = backend.last_stdout_len().unwrap_or(0);
        let _ = runtime.close_session(&key, &opts).await;

        assert_eq!(err.kind, BrowserErrorKind::CommandFailed);
        assert!(!err.retryable);
        assert!(err.detail.contains("source limit"));
        assert!(!err.detail.contains(&"X".repeat(1000)));
        assert!(
            stdout_len < 64 * 1024,
            "CLI_STDOUT_SIZE={stdout_len} leaked the 5MB payload"
        );
        eprintln!(
            "PAGE_VALUE_SIZE={PAGE_VALUE_SIZE} CLI_STDOUT_SIZE={stdout_len} SOURCE_LIMIT={} ERROR_RESULT_SIZE={}",
            BROWSER_EXTRACT_SOURCE_CHAR_LIMIT,
            err.detail.len()
        );
    }

    #[tokio::test]
    #[ignore = "requires a live agent-browser + Chromium runtime"]
    async fn real_structured_eval_edges_and_single_execution() {
        let Some(search) = native_cli_search() else {
            eprintln!("skip runtime-dependent test: agent-browser unavailable");
            return;
        };
        let page_port = crate::tools::web_client::tests::spawn_test_server(|_, stream| {
            crate::tools::web_client::tests::write_response(
                stream,
                "HTTP/1.1 200 OK",
                "<html><body></body></html>",
                &["content-type: text/html".to_string()],
            );
        });
        let backend = Arc::new(AgentBrowserBackend::new(
            Some(search),
            BrowserSessionOptions::default(),
        ));
        let runtime = BrowserRuntime::new(
            backend,
            BrowserRuntimePolicy {
                allowed_domains: vec!["127.0.0.1".into()],
                ..BrowserRuntimePolicy::default()
            },
        );
        let key = BrowserSessionKey::new(format!("b32d1e-{}", uuid::Uuid::new_v4())).unwrap();
        let opts = BrowserSessionOptions::default();
        runtime
            .open(
                &key,
                &opts,
                &NavigateRequest {
                    url: format!("http://127.0.0.1:{page_port}/edges"),
                },
            )
            .await
            .expect("open");

        let once = runtime
            .extract_json(
                &key,
                &opts,
                &BrowserExtractRequest {
                    expression: "(window.__omninovaExtractCalls = (window.__omninovaExtractCalls || 0) + 1, { n: window.__omninovaExtractCalls })".into(),
                },
            )
            .await
            .expect("count");
        assert_eq!(once.value, json!({"n": 1}));

        let promised = runtime
            .extract_json(
                &key,
                &opts,
                &BrowserExtractRequest {
                    expression: "await Promise.resolve({ awaited: true })".into(),
                },
            )
            .await
            .expect("promise");
        assert_eq!(promised.value, json!({"awaited": true}));

        let cyclic = runtime
            .extract_json(
                &key,
                &opts,
                &BrowserExtractRequest {
                    expression: "(() => { const o = {}; o.self = o; return o; })()".into(),
                },
            )
            .await
            .expect_err("cyclic");
        assert_eq!(cyclic.kind, BrowserErrorKind::InvalidStructuredOutput);
        assert!(!cyclic.retryable);

        let bigint = runtime
            .extract_json(
                &key,
                &opts,
                &BrowserExtractRequest {
                    expression: "({ n: 1n })".into(),
                },
            )
            .await
            .expect_err("bigint");
        assert_eq!(bigint.kind, BrowserErrorKind::InvalidStructuredOutput);

        let nan = runtime
            .extract_json(
                &key,
                &opts,
                &BrowserExtractRequest {
                    expression: "Number.NaN".into(),
                },
            )
            .await
            .expect("nan");
        assert_eq!(nan.value, Value::Null);

        let inf = runtime
            .extract_json(
                &key,
                &opts,
                &BrowserExtractRequest {
                    expression: "Infinity".into(),
                },
            )
            .await
            .expect("infinity");
        assert_eq!(inf.value, Value::Null);

        let undef = runtime
            .extract_json(
                &key,
                &opts,
                &BrowserExtractRequest {
                    expression: "undefined".into(),
                },
            )
            .await
            .expect_err("undefined");
        assert_eq!(undef.kind, BrowserErrorKind::InvalidStructuredOutput);

        let func = runtime
            .extract_json(
                &key,
                &opts,
                &BrowserExtractRequest {
                    expression: "(() => {})".into(),
                },
            )
            .await
            .expect_err("function");
        assert_eq!(func.kind, BrowserErrorKind::InvalidStructuredOutput);

        let quoted = runtime
            .extract_json(
                &key,
                &opts,
                &BrowserExtractRequest {
                    expression: r#""hello 'world' `tick`""#.into(),
                },
            )
            .await
            .expect("quotes");
        assert_eq!(quoted.value, json!("hello 'world' `tick`"));

        let unicode = runtime
            .extract_json(
                &key,
                &opts,
                &BrowserExtractRequest {
                    expression: r#""你好🚀""#.into(),
                },
            )
            .await
            .expect("unicode");
        assert_eq!(unicode.value, json!("你好🚀"));

        let _ = runtime.close_session(&key, &opts).await;
    }

    #[test]
    fn non_eval_action_never_exposes_structured_payload() {
        let data = json!({"result": "{\"hidden\":true}"});
        assert_eq!(
            structured_output_for_action(&BrowserAction::Reload, Some(&data)),
            None
        );
    }

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

    #[test]
    fn structured_refs_map_to_elements_without_merging_duplicates() {
        let elements = snapshot_elements_from_stdout(
            BrowserBackendId::agent_browser(),
            &agent_browser_0_36_snapshot_stdout(),
        );
        assert_eq!(elements.len(), 6);
        assert_eq!(elements[0].reference.as_str(), "e1");
        assert_eq!(elements[0].role.as_deref(), Some("heading"));
        assert!(!elements[0].interactive);
        assert_eq!(elements[1].name.as_deref(), Some(" Remember me"));
        assert!(elements[1].interactive);
        let submit: Vec<&BrowserElement> = elements
            .iter()
            .filter(|el| el.name.as_deref() == Some("Submit"))
            .collect();
        assert_eq!(submit.len(), 2);
        assert_eq!(submit[0].role.as_deref(), Some("button"));
        assert_eq!(submit[1].role.as_deref(), Some("button"));
        assert_ne!(submit[0].reference, submit[1].reference);
        assert_eq!(submit[0].reference.as_str(), "e8");
        assert_eq!(submit[1].reference.as_str(), "e9");
        assert!(submit.iter().all(|el| el.interactive));
        let iframe_box = elements
            .iter()
            .find(|el| el.reference.as_str() == "e13")
            .expect("iframe textbox");
        assert_eq!(iframe_box.role.as_deref(), Some("textbox"));
        assert_eq!(iframe_box.name.as_deref(), Some("Card number"));
        assert!(iframe_box.interactive);
        assert_eq!(
            iframe_box.reference.backend(),
            &BrowserBackendId::agent_browser()
        );
        let pay = elements
            .iter()
            .find(|el| el.reference.as_str() == "e14")
            .expect("iframe pay");
        assert_eq!(pay.role.as_deref(), Some("button"));
        assert_eq!(pay.name.as_deref(), Some("Pay"));
    }

    #[test]
    fn observe_element_ref_maps_to_cli_at_ref() {
        let backend = AgentBrowserBackend::new(None, BrowserSessionOptions::default());
        let elements = snapshot_elements_from_stdout(
            backend.id_value(),
            &agent_browser_0_36_snapshot_stdout(),
        );
        let e8 = elements
            .iter()
            .find(|el| el.reference.as_str() == "e8")
            .expect("e8");
        let cli = backend
            .target_arg(&BrowserTarget::Element(e8.reference.clone()))
            .unwrap();
        assert_eq!(cli, "@e8");
        let already_prefixed = backend
            .target_arg(&BrowserTarget::Element(BrowserElementRef::new(
                backend.id_value(),
                "@e8",
            )))
            .unwrap();
        assert_eq!(already_prefixed, "@e8");
    }

    #[test]
    fn malformed_refs_do_not_fail_snapshot_text() {
        let stdout = r#"{"success":true,"data":{"origin":"https://example.com/","snapshot":"- heading \"Example\" [ref=e1]","refs":["bad"]},"error":null}"#;
        let outcome = parse_action_outcome("snapshot", stdout, "", true);
        assert!(outcome.success);
        assert!(outcome.output.contains("[ref=e1]"));
        let obs = snapshot_observation(BrowserBackendId::agent_browser(), outcome.output, stdout);
        assert!(obs.snapshot.as_ref().unwrap().elements.is_empty());
        assert!(obs.text.as_ref().unwrap().contains("[ref=e1]"));
    }

    #[test]
    fn iframe_refs_are_ordinary_element_handles() {
        let elements = snapshot_elements_from_stdout(
            BrowserBackendId::agent_browser(),
            &agent_browser_0_36_snapshot_stdout(),
        );
        let card = elements
            .iter()
            .find(|el| el.reference.as_str() == "e13")
            .unwrap();
        assert_eq!(card.reference.as_str(), "e13");
        let backend = AgentBrowserBackend::new(None, BrowserSessionOptions::default());
        assert_eq!(
            backend
                .target_arg(&BrowserTarget::Element(card.reference.clone()))
                .unwrap(),
            "@e13"
        );
    }

    #[test]
    fn read_outline_maps_to_backend_argv() {
        let args = read_cli_args(true, None);
        assert_eq!(args, vec!["read", "--outline"]);
    }

    #[test]
    fn read_filter_is_passed_as_single_argv() {
        let args = read_cli_args(false, Some("security"));
        assert_eq!(args, vec!["read", "--filter", "security"]);
        assert_eq!(args[2], "security");
        let both = read_cli_args(true, Some("auth docs"));
        assert_eq!(both, vec!["read", "--outline", "--filter", "auth docs"]);
    }

    #[test]
    fn unknown_ref_maps_to_stale_reference() {
        assert!(looks_like_agent_browser_stale_ref("Unknown ref: e999"));
        assert!(looks_like_agent_browser_stale_ref(
            "Could not locate element with role=button name=Submit"
        ));
        assert!(!looks_like_agent_browser_stale_ref(
            "Element not found: #missing"
        ));
        assert!(!looks_like_agent_browser_stale_ref(
            "BrowserSessionUnavailable: session gone"
        ));
        assert!(!looks_like_agent_browser_stale_ref(
            "element went stale after navigation"
        ));
        assert!(looks_like_agent_browser_stale_ref("No objectId for ref e8"));
        assert!(looks_like_agent_browser_stale_ref(
            "AX node has no backendDOMNodeId for role=button"
        ));
        let err = BrowserBackendError::new(
            BrowserErrorKind::StaleReference,
            BrowserBackendId::agent_browser(),
            format!("{BROWSER_STALE_REFERENCE_DETAIL}; summary=Unknown ref: e999"),
        );
        assert_eq!(err.kind, BrowserErrorKind::StaleReference);
        assert_ne!(err.kind, BrowserErrorKind::SessionNotFound);
        assert!(!err.retryable);
        assert!(err.detail.starts_with("BrowserCommandFailed:"));
        assert!(err.detail.contains("snapshot again"));
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

    fn scratch_profile_backend(
        search: Option<BrowserBinarySearch>,
    ) -> (AgentBrowserBackend, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("omninova-b33b-backend-{}", uuid::Uuid::new_v4()));
        let backend = AgentBrowserBackend::with_profile_resolver(
            search,
            BrowserSessionOptions::default(),
            BrowserProfileResolver::new(root.clone()),
        );
        (backend, root)
    }

    fn scratch_executable_backend() -> (AgentBrowserBackend, PathBuf, Arc<PassingExecutableProbe>) {
        let root =
            std::env::temp_dir().join(format!("omninova-b33c-backend-{}", uuid::Uuid::new_v4()));
        let executable = root.join("runtime/chrome.exe");
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::write(&executable, b"test executable").unwrap();
        let probe = Arc::new(PassingExecutableProbe {
            calls: std::sync::atomic::AtomicUsize::new(0),
        });
        let executable_resolver = BrowserExecutableResolver::for_test(
            vec![(
                executable,
                BrowserExecutableSource::SystemChrome,
                false,
            )],
            probe.clone(),
        );
        let backend = AgentBrowserBackend::with_resolvers(
            None,
            BrowserSessionOptions::default(),
            BrowserProfileResolver::new(root.join("profiles")),
            executable_resolver,
        );
        (backend, root, probe)
    }

    fn profile_opts(id: &str) -> BrowserSessionOptions {
        BrowserSessionOptions {
            profile: Some(BrowserProfileRef::new(id).unwrap()),
            ..BrowserSessionOptions::default()
        }
    }

    #[test]
    fn agent_backend_declares_profile_capability() {
        let backend = AgentBrowserBackend::new(None, BrowserSessionOptions::default());
        assert!(backend.capabilities().profiles);
    }

    #[test]
    fn ephemeral_session_does_not_emit_profile_argv() {
        assert!(managed_profile_argv(None).is_empty());
        let backend = AgentBrowserBackend::new(None, BrowserSessionOptions::default());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let key = BrowserSessionKey::new("ephemeral-chat").unwrap();
        let handle = rt
            .block_on(backend.open_session(&BrowserSessionOpenRequest::new(
                key,
                BrowserSessionOptions::default(),
            )))
            .unwrap();
        assert!(backend.session_profile_path(handle.token()).is_none());
    }

    #[test]
    fn managed_profile_argv_is_two_absolute_entries() {
        let path = if cfg!(windows) {
            PathBuf::from(r"C:\omninova-test\browser\profiles\profile-ab12")
        } else {
            PathBuf::from("/tmp/omninova-test/browser/profiles/profile-ab12")
        };
        let args = managed_profile_argv(Some(&path));
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "--profile");
        assert_eq!(Path::new(&args[1]), path.as_path());
        assert!(Path::new(&args[1]).is_absolute());
        assert!(
            path.to_string_lossy().contains('\\') || path.to_string_lossy().contains('/'),
            "absolute profile path must contain a separator so it is not an installed-profile name"
        );
        assert_ne!(args[1], "work");
        assert_ne!(args[1], "default");
    }

    #[test]
    fn profile_path_for_cli_strips_windows_verbatim_prefix() {
        #[cfg(windows)]
        {
            let verbatim = PathBuf::from(r"\\?\C:\omninova-test\browser\profiles\profile-ab12");
            assert_eq!(
                profile_path_for_cli(&verbatim),
                OsString::from(r"C:\omninova-test\browser\profiles\profile-ab12")
            );
            let unc = PathBuf::from(r"\\?\UNC\server\share\profiles\p1");
            assert_eq!(profile_path_for_cli(&unc), unc.as_os_str());
        }
        let ordinary = if cfg!(windows) {
            PathBuf::from(r"C:\omninova-test\browser\profiles\profile-ab12")
        } else {
            PathBuf::from("/tmp/omninova-test/browser/profiles/profile-ab12")
        };
        assert_eq!(profile_path_for_cli(&ordinary), ordinary.as_os_str());
    }

    #[test]
    fn should_forward_local_launch_config_omits_when_daemon_alive() {
        assert!(should_forward_local_launch_config(false));
        assert!(!should_forward_local_launch_config(true));
    }

    #[tokio::test]
    async fn executable_is_bound_for_ephemeral_and_persistent_sessions() {
        let (backend, root, probe) = scratch_executable_backend();
        let ephemeral = backend
            .open_session(&BrowserSessionOpenRequest::new(
                BrowserSessionKey::new("b33c-ephemeral").unwrap(),
                BrowserSessionOptions::default(),
            ))
            .await
            .unwrap();
        let ephemeral_bound = backend
            .ensure_session_executable(ephemeral.token(), "open")
            .await
            .unwrap();
        assert!(ephemeral_bound.profile_path.is_none());
        assert_eq!(
            ephemeral_bound.executable.as_ref().map(|value| value.source),
            Some(BrowserExecutableSource::SystemChrome)
        );

        let persistent = backend
            .open_session(&BrowserSessionOpenRequest::new(
                BrowserSessionKey::new("b33c-persistent").unwrap(),
                profile_opts("account-c"),
            ))
            .await
            .unwrap();
        let persistent_bound = backend
            .ensure_session_executable(persistent.token(), "open")
            .await
            .unwrap();
        assert!(persistent_bound.profile_path.is_some());
        assert_eq!(
            persistent_bound.executable.as_ref().map(|value| value.source),
            Some(BrowserExecutableSource::SystemChrome)
        );
        assert_eq!(
            probe.calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "backend selection must be launch-probed once and shared across sessions"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn cdp_attach_does_not_select_or_forward_local_executable() {
        let (backend, root, probe) = scratch_executable_backend();
        let options = BrowserSessionOptions {
            attach_only: true,
            cdp_url: Some("http://127.0.0.1:9222".into()),
            ..BrowserSessionOptions::default()
        };
        let session = backend
            .open_session(&BrowserSessionOpenRequest::new(
                BrowserSessionKey::new("b33c-cdp").unwrap(),
                options,
            ))
            .await
            .unwrap();
        let bound = backend
            .ensure_session_executable(session.token(), "open")
            .await
            .unwrap();
        assert!(bound.executable.is_none());
        assert_eq!(probe.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(browser_executable_argv(bound.executable.as_ref().map(|value| value.path.as_path())).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn same_session_profile_mismatch_is_rejected() {
        let (backend, root) = scratch_profile_backend(None);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let key = BrowserSessionKey::new("mismatch-session").unwrap();
        rt.block_on(backend.open_session(&BrowserSessionOpenRequest::new(
            key.clone(),
            profile_opts("account-a"),
        )))
        .unwrap();
        let err = rt
            .block_on(backend.open_session(&BrowserSessionOpenRequest::new(
                key,
                profile_opts("account-b"),
            )))
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::Rejected);
        assert!(!err.retryable);
        assert_eq!(err.detail, BROWSER_SESSION_PROFILE_MISMATCH_DETAIL);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn same_process_profile_occupancy_is_profile_busy() {
        let (backend, root) = scratch_profile_backend(None);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(backend.open_session(&BrowserSessionOpenRequest::new(
            BrowserSessionKey::new("holder-a").unwrap(),
            profile_opts("shared-x"),
        )))
        .unwrap();
        let err = rt
            .block_on(backend.open_session(&BrowserSessionOpenRequest::new(
                BrowserSessionKey::new("challenger-b").unwrap(),
                profile_opts("shared-x"),
            )))
            .unwrap_err();
        assert_eq!(err.kind, BrowserErrorKind::ProfileBusy);
        assert!(!err.retryable);
        assert_eq!(err.detail, BROWSER_PROFILE_BUSY_DETAIL);
        assert!(!err.detail.to_ascii_lowercase().contains("kill"));
        assert!(!err.detail.to_ascii_lowercase().contains("singleton"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn same_session_same_profile_rebinding_is_stable() {
        let (backend, root) = scratch_profile_backend(None);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let key = BrowserSessionKey::new("stable-session").unwrap();
        let first = rt
            .block_on(backend.open_session(&BrowserSessionOpenRequest::new(
                key.clone(),
                profile_opts("account-a"),
            )))
            .unwrap();
        let path_first = backend.session_profile_path(first.token()).unwrap();
        let second = rt
            .block_on(backend.open_session(&BrowserSessionOpenRequest::new(
                key,
                profile_opts("account-a"),
            )))
            .unwrap();
        let path_second = backend.session_profile_path(second.token()).unwrap();
        assert_eq!(first, second);
        assert_eq!(path_first, path_second);
        assert!(path_first.is_absolute());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn distinct_sessions_keep_distinct_profile_paths() {
        let (backend, root) = scratch_profile_backend(None);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let a = rt
            .block_on(backend.open_session(&BrowserSessionOpenRequest::new(
                BrowserSessionKey::new("sess-a").unwrap(),
                profile_opts("account-a"),
            )))
            .unwrap();
        let b = rt
            .block_on(backend.open_session(&BrowserSessionOpenRequest::new(
                BrowserSessionKey::new("sess-b").unwrap(),
                profile_opts("account-b"),
            )))
            .unwrap();
        let path_a = backend.session_profile_path(a.token()).unwrap();
        let path_b = backend.session_profile_path(b.token()).unwrap();
        assert_ne!(a.token(), b.token());
        assert_ne!(path_a, path_b);
        assert!(path_a.is_absolute());
        assert!(path_b.is_absolute());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn stale_dead_pid_releases_profile_occupancy() {
        let (backend, root) = scratch_profile_backend(None);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let nonce = uuid::Uuid::new_v4();
        let handle_a = rt
            .block_on(backend.open_session(&BrowserSessionOpenRequest::new(
                BrowserSessionKey::new(format!("stale-holder-{nonce}")).unwrap(),
                profile_opts("shared-stale"),
            )))
            .unwrap();
        let pid_path = crate::tools::browser_lifecycle::namespace_run_dir(AGENT_BROWSER_NAMESPACE)
            .join(format!("{}.pid", handle_a.token()));
        if let Some(parent) = pid_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&pid_path, b"0").expect("dead pid file");
        let opened_b = rt.block_on(backend.open_session(&BrowserSessionOpenRequest::new(
            BrowserSessionKey::new(format!("stale-challenger-{nonce}")).unwrap(),
            profile_opts("shared-stale"),
        )));
        let _ = std::fs::remove_file(&pid_path);
        let handle_b = opened_b.expect("dead holder must release occupancy");
        assert_ne!(handle_a.token(), handle_b.token());
        assert!(backend.session_profile_path(handle_b.token()).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn profile_busy_classifier_is_conservative() {
        let signature = "Chrome exited early (exit code: 21) without writing DevToolsActivePort";
        assert!(looks_like_managed_profile_busy(signature));
        assert!(!looks_like_managed_profile_busy(
            "Chrome exited early (exit code: 21)"
        ));
        assert!(!looks_like_managed_profile_busy(
            "something failed with exit code: 21 and more text"
        ));
        assert!(!looks_like_managed_profile_busy(
            "DevToolsActivePort file doesn't exist"
        ));
    }

    fn persist_fixture_html(account: &str) -> String {
        format!(
            r#"<!DOCTYPE html><html><head><title>waiting</title></head><body>
<script>
window.__omninovaPersistFixture = {{ ready: false, error: null, stage: "boot", runs: 0 }};
document.title = "persist-boot";
(async () => {{
  const fixture = () => window.__omninovaPersistFixture;
  const setStage = (stage) => {{ fixture().stage = stage; }};
  try {{
    fixture().runs = (fixture().runs || 0) + 1;
    setStage("cookie");
    document.cookie = "omninova_persist={account}; path=/; max-age=31536000; SameSite=Lax";
    setStage("local-storage");
    localStorage.setItem("omninova_persist", "{account}");
    setStage("indexeddb-open");
    await new Promise((resolve, reject) => {{
      const req = indexedDB.open("omninova_persist_db", 1);
      req.onupgradeneeded = () => {{ req.result.createObjectStore("kv"); }};
      req.onerror = () => reject(req.error || new Error("indexeddb-open"));
      req.onsuccess = () => {{
        setStage("indexeddb-write");
        const db = req.result;
        const tx = db.transaction("kv", "readwrite");
        tx.objectStore("kv").put("{account}", "account");
        tx.oncomplete = () => {{ db.close(); resolve(); }};
        tx.onerror = () => reject(tx.error || new Error("indexeddb-write"));
      }};
    }});
    window.__omninovaPersistFixture = {{
      ready: true,
      error: null,
      stage: "done",
      runs: fixture().runs
    }};
    document.title = "persist-ready";
  }} catch (err) {{
    window.__omninovaPersistFixture = {{
      ready: false,
      error: String(err),
      stage: fixture().stage,
      runs: fixture().runs
    }};
    document.title = "persist-error";
  }}
}})();
</script></body></html>"#
        )
    }

    const PERSIST_READ_EXPRESSION: &str = r#"(async () => {
  const cookie = document.cookie;
  const ls = localStorage.getItem("omninova_persist");
  const idb = await new Promise((resolve, reject) => {
    const req = indexedDB.open("omninova_persist_db", 1);
    req.onerror = () => reject(req.error);
    req.onsuccess = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains("kv")) { resolve(null); return; }
      const tx = db.transaction("kv", "readonly");
      const get = tx.objectStore("kv").get("account");
      get.onsuccess = () => { db.close(); resolve(get.result ?? null); };
      get.onerror = () => reject(get.error);
    };
  });
  return { cookie, ls, idb };
})()"#;

    const PERSIST_READY_POLL_EXPRESSION: &str = r#"({
  ready: window.__omninovaPersistFixture?.ready === true,
  error: window.__omninovaPersistFixture?.error ?? null,
  stage: window.__omninovaPersistFixture?.stage ?? null,
  title: document.title,
  readyState: document.readyState,
  href: String(location.href),
  runs: window.__omninovaPersistFixture?.runs ?? null
})"#;

    fn bound_fixture_diag(value: &str) -> String {
        value.chars().take(200).collect()
    }

    fn json_diag_field(value: &serde_json::Value, key: &str) -> String {
        match value.get(key) {
            None => "missing".into(),
            Some(serde_json::Value::Null) => "null".into(),
            Some(serde_json::Value::String(text)) => text.clone(),
            Some(other) => bound_fixture_diag(&other.to_string()),
        }
    }

    async fn wait_persist_ready(
        runtime: &BrowserRuntime,
        key: &BrowserSessionKey,
        opts: &BrowserSessionOptions,
    ) {
        let deadline = std::time::Duration::from_secs(15);
        let interval = std::time::Duration::from_millis(150);
        let started = std::time::Instant::now();
        let mut last_stage = String::from("missing");
        let mut last_title = String::from("missing");
        let mut last_ready_state = String::from("missing");
        let mut last_href = String::from("missing");
        let mut last_eval_err: Option<String> = None;
        let mut polls: u32 = 0;
        let mut ok_polls: u32 = 0;
        loop {
            polls += 1;
            match runtime
                .extract_json(
                    key,
                    opts,
                    &BrowserExtractRequest {
                        expression: PERSIST_READY_POLL_EXPRESSION.into(),
                    },
                )
                .await
            {
                Ok(extracted) => {
                    ok_polls += 1;
                    last_eval_err = None;
                    let ready = extracted
                        .value
                        .get("ready")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let error = extracted
                        .value
                        .get("error")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty());
                    last_stage = json_diag_field(&extracted.value, "stage");
                    last_title = json_diag_field(&extracted.value, "title");
                    last_ready_state = json_diag_field(&extracted.value, "readyState");
                    last_href = json_diag_field(&extracted.value, "href");
                    if let Some(error) = error {
                        panic!(
                            "persist fixture failed at {}: {}; title={}; readyState={}; href={}",
                            last_stage,
                            bound_fixture_diag(error),
                            bound_fixture_diag(&last_title),
                            bound_fixture_diag(&last_ready_state),
                            bound_fixture_diag(&last_href)
                        );
                    }
                    if ready {
                        let runs = extracted.value.get("runs").and_then(|v| v.as_u64());
                        assert_eq!(
                            runs,
                            Some(1),
                            "fixture init must run once; stage={last_stage} title={last_title} href={last_href}"
                        );
                        return;
                    }
                }
                Err(err) => {
                    last_eval_err = Some(bound_fixture_diag(&err.detail));
                }
            }
            if started.elapsed() >= deadline {
                match last_eval_err {
                    Some(eval_err) => panic!(
                        "persist fixture timeout after 15s: stage={last_stage}, title={}, readyState={}, href={}, polls={polls}, ok_polls={ok_polls}, eval_error={eval_err}",
                        bound_fixture_diag(&last_title),
                        bound_fixture_diag(&last_ready_state),
                        bound_fixture_diag(&last_href)
                    ),
                    None => panic!(
                        "persist fixture timeout after 15s: stage={last_stage}, title={}, readyState={}, href={}, polls={polls}, ok_polls={ok_polls}",
                        bound_fixture_diag(&last_title),
                        bound_fixture_diag(&last_ready_state),
                        bound_fixture_diag(&last_href)
                    ),
                }
            }
            tokio::time::sleep(interval).await;
        }
    }

    fn owned_sidecar_pid(token: &str) -> Option<u32> {
        let pid_path = crate::tools::browser_lifecycle::namespace_run_dir(AGENT_BROWSER_NAMESPACE)
            .join(format!("{token}.pid"));
        std::fs::read_to_string(pid_path)
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    fn child_pids(parent: u32) -> Vec<(u32, String)> {
        #[cfg(windows)]
        {
            let output = std::process::Command::new("wmic")
                .args([
                    "process",
                    "where",
                    &format!("ParentProcessId={parent}"),
                    "get",
                    "ProcessId,Name",
                    "/FORMAT:CSV",
                ])
                .output();
            let Ok(output) = output else {
                return Vec::new();
            };
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| {
                    let cols: Vec<&str> = line.split(',').map(str::trim).collect();
                    if cols.len() < 3 {
                        return None;
                    }
                    let name = cols[1];
                    let pid = cols[2].parse::<u32>().ok()?;
                    if name.eq_ignore_ascii_case("Name") {
                        return None;
                    }
                    Some((pid, name.to_string()))
                })
                .collect()
        }
        #[cfg(not(windows))]
        {
            let _ = parent;
            Vec::new()
        }
    }

    fn chrome_pid_near(daemon_pid: Option<u32>) -> Option<u32> {
        let parent = daemon_pid?;
        child_pids(parent)
            .into_iter()
            .find(|(_, name)| {
                let lower = name.to_ascii_lowercase();
                lower.contains("chrome") || lower.contains("chromium")
            })
            .map(|(pid, _)| pid)
            .or_else(|| {
                child_pids(parent).into_iter().find_map(|(child, _)| {
                    child_pids(child)
                        .into_iter()
                        .find(|(_, name)| {
                            let lower = name.to_ascii_lowercase();
                            lower.contains("chrome") || lower.contains("chromium")
                        })
                        .map(|(pid, _)| pid)
                })
            })
    }

    fn dump_cli_invocations(records: &[CliInvocationRecord]) {
        for record in records {
            eprintln!(
                "OP={} cli_session={} namespace={} profile_present={} profile_hash={} executable_present={} executable_source={} headless={} attach_only={} cdp={} launch_fingerprint={}",
                record.operation,
                record.cli_session,
                record.namespace,
                record.profile_present,
                record.profile_hash.as_deref().unwrap_or("none"),
                record.executable_present,
                record.executable_source.unwrap_or("none"),
                record.headless,
                record.attach_only,
                record.cdp_url_present,
                record.launch_fingerprint
            );
        }
    }

    fn kill_owned_session_tree(token: &str) {
        let Some(pid) = owned_sidecar_pid(token) else {
            return;
        };
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        #[cfg(not(windows))]
        {
            let _ = std::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .status();
        }
    }

    #[tokio::test]
    #[ignore = "requires a live agent-browser + System Chrome runtime"]
    async fn real_system_chrome_ephemeral_open_snapshot_read() {
        let Some(search) = native_cli_search() else {
            eprintln!("skip runtime-dependent test: agent-browser unavailable");
            return;
        };
        let html = "<html><head><title>b33c-ephemeral</title></head><body><h1>Ephemeral browser fixture</h1><p>Local read passed.</p></body></html>";
        let page_port = crate::tools::web_client::tests::spawn_test_server(move |_, stream| {
            crate::tools::web_client::tests::write_response(
                stream,
                "HTTP/1.1 200 OK",
                html,
                &["content-type: text/html; charset=utf-8".to_string()],
            );
        });
        let backend = Arc::new(AgentBrowserBackend::new(
            Some(search),
            BrowserSessionOptions::default(),
        ));
        let runtime = BrowserRuntime::new(
            backend.clone(),
            BrowserRuntimePolicy {
                allowed_domains: vec!["127.0.0.1".into()],
                ..BrowserRuntimePolicy::default()
            },
        );
        let options = BrowserSessionOptions::default();
        let key = BrowserSessionKey::new(format!(
            "b33c-ephemeral-live-{}",
            uuid::Uuid::new_v4()
        ))
        .unwrap();
        runtime
            .open(
                &key,
                &options,
                &NavigateRequest {
                    url: format!("http://127.0.0.1:{page_port}/fixture"),
                },
            )
            .await
            .expect("open local fixture with selected System Chrome");
        let snapshot = runtime
            .observe(
                &key,
                &options,
                &ObserveRequest {
                    kind: BrowserObserveKind::Snapshot,
                    interactive_only: false,
                    compact: false,
                },
            )
            .await
            .expect("snapshot local fixture");
        let read = runtime
            .observe(
                &key,
                &options,
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
            .expect("read local fixture");
        let invocations = backend.take_cli_invocations();
        let open = invocations
            .iter()
            .find(|record| record.operation == "open")
            .expect("open invocation");
        let snapshot_invocation = invocations
            .iter()
            .find(|record| record.operation == "snapshot")
            .expect("snapshot invocation");
        assert!(!open.profile_present);
        assert!(open.executable_present);
        assert_eq!(open.executable_source, Some("system_chrome"));
        assert!(!snapshot_invocation.executable_present);
        assert!(
            snapshot
                .text
                .as_deref()
                .unwrap_or_default()
                .contains("Ephemeral browser fixture")
        );
        assert!(
            read.text
                .as_deref()
                .unwrap_or_default()
                .contains("Local read passed")
        );
        runtime.close_session(&key, &options).await.ok();
    }

    #[tokio::test]
    #[ignore = "requires a live agent-browser + Chromium runtime"]
    async fn real_managed_profile_navigation_targets_same_page() {
        let Some(search) = native_cli_search() else {
            eprintln!("skip runtime-dependent test: agent-browser unavailable");
            return;
        };
        let html = r#"<!DOCTYPE html><html><head><title>target-page</title></head><body><main><h1>Managed browser fixture</h1><p>Deterministic local read.</p></main><script>window.__targetMarker = "managed-profile-target";</script></body></html>"#;
        let page_port = crate::tools::web_client::tests::spawn_test_server(move |_, stream| {
            crate::tools::web_client::tests::write_response(
                stream,
                "HTTP/1.1 200 OK",
                html,
                &["content-type: text/html; charset=utf-8".to_string()],
            );
        });
        let (backend, root) = scratch_profile_backend(Some(search));
        let backend = Arc::new(backend);
        let runtime = BrowserRuntime::new(
            backend.clone(),
            BrowserRuntimePolicy {
                allowed_domains: vec!["127.0.0.1".into()],
                ..BrowserRuntimePolicy::default()
            },
        );
        let nonce = uuid::Uuid::new_v4();
        let opts = profile_opts(&format!("b33b-nav-{nonce}"));
        let key = BrowserSessionKey::new(format!("b33b-nav-sess-{nonce}")).unwrap();
        let url = format!("http://127.0.0.1:{page_port}/target");
        runtime
            .open(&key, &opts, &NavigateRequest { url: url.clone() })
            .await
            .expect("open target page");
        let token = browser_session_id(Some(key.as_str())).unwrap();
        let daemon_pid_open = owned_sidecar_pid(&token);
        let chrome_pid_open = chrome_pid_near(daemon_pid_open);
        eprintln!(
            "DAEMON_PID_OPEN={:?} CHROME_PID_OPEN={:?}",
            daemon_pid_open, chrome_pid_open
        );

        let observed_url = runtime
            .observe(
                &key,
                &opts,
                &ObserveRequest {
                    kind: BrowserObserveKind::Url,
                    interactive_only: false,
                    compact: false,
                },
            )
            .await
            .expect("observe url");
        let observed_title = runtime
            .observe(
                &key,
                &opts,
                &ObserveRequest {
                    kind: BrowserObserveKind::Title,
                    interactive_only: false,
                    compact: false,
                },
            )
            .await
            .expect("observe title");
        let snapshot = runtime
            .observe(
                &key,
                &opts,
                &ObserveRequest {
                    kind: BrowserObserveKind::Snapshot,
                    interactive_only: false,
                    compact: false,
                },
            )
            .await
            .expect("snapshot local fixture");
        let read = runtime
            .observe(
                &key,
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
            .expect("read local fixture");
        let extracted = runtime
            .extract_json(
                &key,
                &opts,
                &BrowserExtractRequest {
                    expression: "({ href: String(document.location.href), title: String(document.title), marker: window.__targetMarker ?? null })".into(),
                },
            )
            .await
            .expect("eval target marker");
        let daemon_pid_eval = owned_sidecar_pid(&token);
        let chrome_pid_eval = chrome_pid_near(daemon_pid_eval);
        eprintln!(
            "DAEMON_PID_EVAL={:?} CHROME_PID_EVAL={:?}",
            daemon_pid_eval, chrome_pid_eval
        );

        let invocations = backend.take_cli_invocations();
        dump_cli_invocations(&invocations);
        let open_inv = invocations
            .iter()
            .find(|row| row.operation == "open")
            .expect("open argv record");
        let eval_inv = invocations
            .iter()
            .find(|row| row.operation == "eval")
            .expect("eval argv record");
        eprintln!("OPEN_AGENT_BROWSER_SESSION={}", open_inv.cli_session);
        eprintln!("EVAL_AGENT_BROWSER_SESSION={}", eval_inv.cli_session);
        eprintln!("OPEN_NAMESPACE={}", open_inv.namespace);
        eprintln!("EVAL_NAMESPACE={}", eval_inv.namespace);
        eprintln!("OPEN_LAUNCH_FINGERPRINT={}", open_inv.launch_fingerprint);
        eprintln!("EVAL_LAUNCH_FINGERPRINT={}", eval_inv.launch_fingerprint);
        eprintln!(
            "OPEN_PROFILE_PRESENT={} EVAL_PROFILE_PRESENT={}",
            open_inv.profile_present, eval_inv.profile_present
        );

        let href = extracted.value["href"].as_str().unwrap_or("");
        let title = extracted.value["title"].as_str().unwrap_or("");
        let marker = extracted.value["marker"].as_str().unwrap_or("");
        let observed_url_text = observed_url
            .text
            .as_deref()
            .or(observed_url.url.as_deref())
            .unwrap_or("");
        let observed_title_text = observed_title
            .text
            .as_deref()
            .or(observed_title.title.as_deref())
            .unwrap_or("");
        eprintln!(
            "OBSERVE_URL={observed_url_text} OBSERVE_TITLE={observed_title_text} EVAL_HREF={href} EVAL_TITLE={title} MARKER={marker}"
        );

        let _ = runtime.close_session(&key, &opts).await;
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(open_inv.cli_session, eval_inv.cli_session);
        assert_eq!(open_inv.namespace, eval_inv.namespace);
        assert_eq!(open_inv.launch_fingerprint, eval_inv.launch_fingerprint);
        assert!(open_inv.profile_present, "first open must send --profile");
        assert!(
            open_inv.executable_present,
            "first open must send the selected --executable-path"
        );
        assert!(
            !eval_inv.profile_present,
            "follow-up eval must not re-send --profile while the session daemon is alive"
        );
        assert!(
            !eval_inv.executable_present,
            "follow-up eval must not re-send --executable-path while the session daemon is alive"
        );
        assert_eq!(open_inv.executable_source, Some("system_chrome"));
        assert_eq!(eval_inv.executable_source, Some("system_chrome"));
        assert_eq!(daemon_pid_open, daemon_pid_eval, "daemon pid changed");
        assert!(
            href.contains(&url) && !href.contains("about:blank"),
            "eval href was {href}, expected {url}"
        );
        assert!(
            observed_url_text.contains(&url),
            "observe url was {observed_url_text}"
        );
        assert_eq!(observed_title_text.trim(), "target-page");
        assert!(
            snapshot
                .text
                .as_deref()
                .unwrap_or_default()
                .contains("Managed browser fixture")
        );
        assert!(
            read.text
                .as_deref()
                .unwrap_or_default()
                .contains("Deterministic local read")
        );
        assert_eq!(title, "target-page");
        assert_eq!(marker, "managed-profile-target");
    }

    #[tokio::test]
    #[ignore = "requires a live agent-browser + Chromium runtime"]
    async fn real_managed_profile_persists_across_logical_sessions() {
        let Some(search) = native_cli_search() else {
            eprintln!("skip runtime-dependent test: agent-browser unavailable");
            return;
        };
        let html = persist_fixture_html("cookie-a");
        let page_port = crate::tools::web_client::tests::spawn_test_server(move |_, stream| {
            crate::tools::web_client::tests::write_response(
                stream,
                "HTTP/1.1 200 OK",
                &html,
                &["content-type: text/html; charset=utf-8".to_string()],
            );
        });
        let (backend, root) = scratch_profile_backend(Some(search));
        let backend = Arc::new(backend);
        let runtime = BrowserRuntime::new(
            backend,
            BrowserRuntimePolicy {
                allowed_domains: vec!["127.0.0.1".into()],
                ..BrowserRuntimePolicy::default()
            },
        );
        let profile_id = format!("b33b-p-{}", uuid::Uuid::new_v4());
        let opts = profile_opts(&profile_id);
        let url = format!("http://127.0.0.1:{page_port}/persist");
        let key_a =
            BrowserSessionKey::new(format!("b33b-sess-a-{}", uuid::Uuid::new_v4())).unwrap();
        runtime
            .open(&key_a, &opts, &NavigateRequest { url: url.clone() })
            .await
            .expect("open A");
        wait_persist_ready(&runtime, &key_a, &opts).await;
        runtime.close_session(&key_a, &opts).await.expect("close A");
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let key_b =
            BrowserSessionKey::new(format!("b33b-sess-b-{}", uuid::Uuid::new_v4())).unwrap();
        runtime
            .open(&key_b, &opts, &NavigateRequest { url })
            .await
            .expect("open B");
        let read = runtime
            .extract_json(
                &key_b,
                &opts,
                &BrowserExtractRequest {
                    expression: PERSIST_READ_EXPRESSION.into(),
                },
            )
            .await
            .expect("read B");
        let _ = runtime.close_session(&key_b, &opts).await;
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(read.value["ls"], json!("cookie-a"));
        assert_eq!(read.value["idb"], json!("cookie-a"));
        let cookie = read.value["cookie"].as_str().unwrap_or("");
        assert!(
            cookie.contains("omninova_persist=cookie-a"),
            "cookie missing after clean restart: {cookie}"
        );
    }

    #[tokio::test]
    #[ignore = "requires a live agent-browser + Chromium runtime"]
    async fn real_managed_profiles_stay_isolated() {
        let Some(search) = native_cli_search() else {
            eprintln!("skip runtime-dependent test: agent-browser unavailable");
            return;
        };
        let html_a = persist_fixture_html("account-a");
        let html_b = persist_fixture_html("account-b");
        let page_port = crate::tools::web_client::tests::spawn_test_server(move |req, stream| {
            let path = req.request_line.split(' ').nth(1).unwrap_or("/");
            let body = if path.contains("b") { &html_b } else { &html_a };
            crate::tools::web_client::tests::write_response(
                stream,
                "HTTP/1.1 200 OK",
                body,
                &["content-type: text/html; charset=utf-8".to_string()],
            );
        });
        let (backend, root) = scratch_profile_backend(Some(search));
        let runtime = BrowserRuntime::new(
            Arc::new(backend),
            BrowserRuntimePolicy {
                allowed_domains: vec!["127.0.0.1".into()],
                ..BrowserRuntimePolicy::default()
            },
        );
        let nonce = uuid::Uuid::new_v4();
        let opts_a = profile_opts(&format!("b33b-ia-{nonce}"));
        let opts_b = profile_opts(&format!("b33b-ib-{nonce}"));
        let key_a = BrowserSessionKey::new(format!("b33b-iso-a-{nonce}")).unwrap();
        let key_b = BrowserSessionKey::new(format!("b33b-iso-b-{nonce}")).unwrap();
        let url_a = format!("http://127.0.0.1:{page_port}/a");
        let url_b = format!("http://127.0.0.1:{page_port}/b");

        runtime
            .open(&key_a, &opts_a, &NavigateRequest { url: url_a.clone() })
            .await
            .expect("open A");
        wait_persist_ready(&runtime, &key_a, &opts_a).await;
        runtime
            .close_session(&key_a, &opts_a)
            .await
            .expect("close A");

        runtime
            .open(&key_b, &opts_b, &NavigateRequest { url: url_b.clone() })
            .await
            .expect("open B");
        wait_persist_ready(&runtime, &key_b, &opts_b).await;
        runtime
            .close_session(&key_b, &opts_b)
            .await
            .expect("close B");

        runtime
            .open(&key_a, &opts_a, &NavigateRequest { url: url_a })
            .await
            .expect("reopen A");
        let read_a = runtime
            .extract_json(
                &key_a,
                &opts_a,
                &BrowserExtractRequest {
                    expression: PERSIST_READ_EXPRESSION.into(),
                },
            )
            .await
            .expect("read A");
        runtime.close_session(&key_a, &opts_a).await.ok();

        runtime
            .open(&key_b, &opts_b, &NavigateRequest { url: url_b })
            .await
            .expect("reopen B");
        let read_b = runtime
            .extract_json(
                &key_b,
                &opts_b,
                &BrowserExtractRequest {
                    expression: PERSIST_READ_EXPRESSION.into(),
                },
            )
            .await
            .expect("read B");
        runtime.close_session(&key_b, &opts_b).await.ok();
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(read_a.value["ls"], json!("account-a"));
        assert_eq!(read_a.value["idb"], json!("account-a"));
        assert_eq!(read_b.value["ls"], json!("account-b"));
        assert_eq!(read_b.value["idb"], json!("account-b"));
        assert_ne!(read_a.value["ls"], read_b.value["ls"]);
    }

    #[tokio::test]
    #[ignore = "requires a live agent-browser + Chromium runtime"]
    async fn real_same_profile_concurrency_is_profile_busy() {
        let Some(search) = native_cli_search() else {
            eprintln!("skip runtime-dependent test: agent-browser unavailable");
            return;
        };
        let page_port = crate::tools::web_client::tests::spawn_test_server(|_, stream| {
            crate::tools::web_client::tests::write_response(
                stream,
                "HTTP/1.1 200 OK",
                "<html><head><title>hold</title></head><body>hold</body></html>",
                &["content-type: text/html".to_string()],
            );
        });
        let (backend, root) = scratch_profile_backend(Some(search));
        let runtime = BrowserRuntime::new(
            Arc::new(backend),
            BrowserRuntimePolicy {
                allowed_domains: vec!["127.0.0.1".into()],
                ..BrowserRuntimePolicy::default()
            },
        );
        let nonce = uuid::Uuid::new_v4();
        let opts = profile_opts(&format!("b33b-cx-{nonce}"));
        let key_a = BrowserSessionKey::new(format!("b33b-c-a-{nonce}")).unwrap();
        let key_b = BrowserSessionKey::new(format!("b33b-c-b-{nonce}")).unwrap();
        let url = format!("http://127.0.0.1:{page_port}/hold");
        runtime
            .open(&key_a, &opts, &NavigateRequest { url: url.clone() })
            .await
            .expect("open A");
        let url_a = runtime
            .observe(
                &key_a,
                &opts,
                &crate::tools::browser_types::ObserveRequest {
                    kind: crate::tools::browser_types::BrowserObserveKind::Url,
                    interactive_only: false,
                    compact: false,
                },
            )
            .await
            .expect("url A");

        let err = runtime
            .open(&key_b, &opts, &NavigateRequest { url: url.clone() })
            .await
            .expect_err("B must not steal profile");
        assert_eq!(err.kind, BrowserErrorKind::ProfileBusy);
        assert!(!err.retryable);
        assert!(err.detail.contains("already in use"));

        let url_a_again = runtime
            .observe(
                &key_a,
                &opts,
                &crate::tools::browser_types::ObserveRequest {
                    kind: crate::tools::browser_types::BrowserObserveKind::Url,
                    interactive_only: false,
                    compact: false,
                },
            )
            .await
            .expect("A still healthy");
        assert_eq!(url_a.text, url_a_again.text);

        runtime.close_session(&key_a, &opts).await.expect("close A");
        runtime
            .open(&key_b, &opts, &NavigateRequest { url })
            .await
            .expect("B after close A");
        let _ = runtime.close_session(&key_b, &opts).await;
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[ignore = "requires a live agent-browser + Chromium runtime"]
    async fn real_managed_profile_survives_owned_crash() {
        let Some(search) = native_cli_search() else {
            eprintln!("skip runtime-dependent test: agent-browser unavailable");
            return;
        };
        let html = persist_fixture_html("crash-a");
        let page_port = crate::tools::web_client::tests::spawn_test_server(move |_, stream| {
            crate::tools::web_client::tests::write_response(
                stream,
                "HTTP/1.1 200 OK",
                &html,
                &["content-type: text/html; charset=utf-8".to_string()],
            );
        });
        let (backend, root) = scratch_profile_backend(Some(search));
        let backend = Arc::new(backend);
        let runtime = BrowserRuntime::new(
            backend.clone(),
            BrowserRuntimePolicy {
                allowed_domains: vec!["127.0.0.1".into()],
                ..BrowserRuntimePolicy::default()
            },
        );
        let nonce = uuid::Uuid::new_v4();
        let opts = profile_opts(&format!("b33b-cr-{nonce}"));
        let key_a = BrowserSessionKey::new(format!("b33b-crash-a-{nonce}")).unwrap();
        let url = format!("http://127.0.0.1:{page_port}/crash");
        runtime
            .open(&key_a, &opts, &NavigateRequest { url: url.clone() })
            .await
            .expect("open A");
        wait_persist_ready(&runtime, &key_a, &opts).await;
        let token = browser_session_id(Some(key_a.as_str())).unwrap();
        kill_owned_session_tree(&token);
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let key_b = BrowserSessionKey::new(format!("b33b-crash-b-{nonce}")).unwrap();
        runtime
            .open(&key_b, &opts, &NavigateRequest { url })
            .await
            .expect("reopen after crash");
        let read = runtime
            .extract_json(
                &key_b,
                &opts,
                &BrowserExtractRequest {
                    expression: PERSIST_READ_EXPRESSION.into(),
                },
            )
            .await
            .expect("read after crash");
        let _ = runtime.close_session(&key_b, &opts).await;
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(read.value["ls"], json!("crash-a"));
        let cookie = read.value["cookie"].as_str().unwrap_or("");
        if cookie.contains("omninova_persist=crash-a") {
            eprintln!("CRASH_COOKIE=present");
        } else {
            eprintln!("CRASH_COOKIE=missing_in_crash_window cookie={cookie}");
        }
    }
}
