//! Lightweight ownership + recovery helpers for OmniNova-created
//! agent-browser sessions. Not a daemon manager or browser pool.

use crate::tools::browser_bin::resolve_agent_browser_binary;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::time::timeout;

/// Isolates OmniNova sockets from the user's default agent-browser namespace.
pub const AGENT_BROWSER_NAMESPACE: &str = "omninova";

const CLOSE_WAIT_SECS: u64 = 5;

static OWNED_SESSIONS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

#[cfg(test)]
static OWNED_TEST_LOCK: Mutex<()> = Mutex::new(());

fn owned_sessions() -> &'static Mutex<HashSet<String>> {
    OWNED_SESSIONS.get_or_init(|| Mutex::new(HashSet::new()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserActionKind {
    ReadOnly,
    Idempotent,
    Mutating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserFailureKind {
    DaemonUnavailable,
    SessionUnavailable,
    Crashed,
    Timeout,
    CommandFailed,
}

pub(crate) fn action_kind(action: &str) -> BrowserActionKind {
    match action {
        "snapshot" | "get_text" | "get_html" | "get_url" | "get_title" | "get_value"
        | "is_visible" | "is_enabled" | "screenshot" | "find" | "read" => {
            BrowserActionKind::ReadOnly
        }
        "open" | "reload" | "wait" | "back" | "forward" | "close" => BrowserActionKind::Idempotent,
        _ => BrowserActionKind::Mutating,
    }
}

pub(crate) fn is_retryable_action(action: &str) -> bool {
    matches!(
        action,
        "open"
            | "snapshot"
            | "get_text"
            | "get_html"
            | "get_url"
            | "get_title"
            | "get_value"
            | "is_visible"
            | "is_enabled"
            | "wait"
            | "reload"
            | "read"
    )
}

pub(crate) fn classify_browser_output(output: &str) -> BrowserFailureKind {
    let lower = output.to_ascii_lowercase();
    if looks_like_daemon_unavailable(&lower) {
        return BrowserFailureKind::DaemonUnavailable;
    }
    if looks_like_crash(&lower) {
        return BrowserFailureKind::Crashed;
    }
    if looks_like_session_unavailable(&lower) {
        return BrowserFailureKind::SessionUnavailable;
    }
    BrowserFailureKind::CommandFailed
}

pub(crate) fn failure_prefix(kind: BrowserFailureKind) -> &'static str {
    match kind {
        BrowserFailureKind::DaemonUnavailable => "BrowserDaemonUnavailable",
        BrowserFailureKind::SessionUnavailable => "BrowserSessionUnavailable",
        BrowserFailureKind::Crashed => "BrowserCrashed",
        BrowserFailureKind::Timeout => "BrowserCommandTimeout",
        BrowserFailureKind::CommandFailed => "BrowserCommandFailed",
    }
}

pub(crate) fn should_recover(kind: BrowserFailureKind) -> bool {
    matches!(
        kind,
        BrowserFailureKind::DaemonUnavailable
            | BrowserFailureKind::SessionUnavailable
            | BrowserFailureKind::Crashed
    )
}

pub(crate) fn is_concurrent_start_race(output: &str) -> bool {
    output.to_ascii_lowercase().contains("started concurrently")
}

/// One recovery retry, plus a single follow-up if agent-browser races a new daemon.
pub(crate) fn auto_retry_allowed(
    action: &str,
    already_recovered: bool,
    concurrent_followup_used: bool,
    kind: BrowserFailureKind,
    output: &str,
) -> bool {
    if !is_retryable_action(action) {
        return false;
    }
    if !already_recovered && should_recover(kind) {
        return true;
    }
    already_recovered && !concurrent_followup_used && is_concurrent_start_race(output)
}

fn looks_like_daemon_unavailable(lower: &str) -> bool {
    lower.contains("os error 10060")
        || lower.contains("os error 10061")
        || lower.contains("connection refused")
        || lower.contains("connection reset")
        || lower.contains("econnrefused")
        || lower.contains("etimedout")
        || lower.contains("failed to read")
        || lower.contains("broken pipe")
        || lower.contains("daemon version mismatch")
        || lower.contains("started concurrently")
        || lower.contains("could not connect")
}

fn looks_like_crash(lower: &str) -> bool {
    lower.contains("target closed")
        || lower.contains("browser has been closed")
        || lower.contains("browser closed")
        || lower.contains("page crashed")
        || lower.contains("protocol error")
}

fn looks_like_session_unavailable(lower: &str) -> bool {
    lower.contains("session not found") || lower.contains("no active session")
}

pub fn remember_owned_browser_session(session: &str) {
    if session.is_empty() {
        return;
    }
    owned_sessions().lock().insert(session.to_string());
}

pub fn forget_owned_browser_session(session: &str) {
    owned_sessions().lock().remove(session);
}

#[cfg(test)]
pub(crate) fn owned_browser_sessions() -> Vec<String> {
    let mut ids: Vec<String> = owned_sessions().lock().iter().cloned().collect();
    ids.sort();
    ids
}

#[cfg(test)]
pub(crate) fn clear_owned_browser_sessions_for_test() {
    owned_sessions().lock().clear();
}

#[cfg(test)]
pub(crate) fn with_owned_sessions_lock<R>(f: impl FnOnce() -> R) -> R {
    let _guard = OWNED_TEST_LOCK.lock();
    clear_owned_browser_sessions_for_test();
    let out = f();
    clear_owned_browser_sessions_for_test();
    out
}

pub fn cleanup_owned_browser_sessions() {
    let sessions: Vec<String> = owned_sessions().lock().drain().collect();
    if sessions.is_empty() {
        return;
    }
    let Ok(resolved) = resolve_agent_browser_binary() else {
        return;
    };
    for session in sessions {
        close_session_blocking(&resolved.path, &session);
        let _ = clear_stale_session_sidecars(AGENT_BROWSER_NAMESPACE, &session);
    }
}

fn close_session_blocking(binary: &Path, session: &str) {
    let mut cmd = std::process::Command::new(binary);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.args([
        "--namespace",
        AGENT_BROWSER_NAMESPACE,
        "--session",
        session,
        "--json",
        "close",
    ]);
    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let Ok(mut child) = cmd.spawn() else {
        return;
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < Duration::from_secs(CLOSE_WAIT_SECS) => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
}

pub(crate) fn recover_owned_session(session: &str) -> bool {
    clear_stale_session_sidecars_in(&namespace_run_dir(AGENT_BROWSER_NAMESPACE), session)
}

/// Sidecar pid probe only. Does not spawn a daemon or clear files.
/// `None` = no pid file, `Some(true)` = process alive, `Some(false)` = stale.
pub(crate) fn probe_owned_session_pid(session: &str) -> Option<bool> {
    if !is_safe_sidecar_token(session) {
        return None;
    }
    let pid_path = namespace_run_dir(AGENT_BROWSER_NAMESPACE).join(format!("{session}.pid"));
    let text = std::fs::read_to_string(pid_path).ok()?;
    let pid = text.trim().parse::<u32>().ok()?;
    Some(pid_is_alive(pid))
}

pub(crate) fn clear_stale_session_sidecars(namespace: &str, session: &str) -> bool {
    if !is_safe_sidecar_token(namespace) {
        return false;
    }
    clear_stale_session_sidecars_in(&namespace_run_dir(namespace), session)
}

pub(crate) fn clear_stale_session_sidecars_in(dir: &Path, session: &str) -> bool {
    if !is_safe_sidecar_token(session) {
        return false;
    }
    let pid_path = dir.join(format!("{session}.pid"));
    if let Ok(text) = std::fs::read_to_string(&pid_path) {
        if let Ok(pid) = text.trim().parse::<u32>() {
            if pid_is_alive(pid) {
                return false;
            }
        }
    }
    let prefix = format!("{session}.");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return false;
    };
    let mut cleared = false;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) {
            if std::fs::remove_file(entry.path()).is_ok() {
                cleared = true;
            }
        }
    }
    cleared
}

fn is_safe_sidecar_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub(crate) fn namespace_run_dir(namespace: &str) -> PathBuf {
    let home = home::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".agent-browser")
        .join("namespaces")
        .join(namespace)
        .join("run")
}

fn pid_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        windows_pid_is_alive(pid)
    }
    #[cfg(unix)]
    {
        unix_pid_is_alive(pid)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = pid;
        false
    }
}

#[cfg(windows)]
fn windows_pid_is_alive(pid: u32) -> bool {
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
        fn GetExitCodeProcess(handle: *mut std::ffi::c_void, code: *mut u32) -> i32;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(handle, &mut code);
        CloseHandle(handle);
        ok != 0 && code == STILL_ACTIVE
    }
}

#[cfg(unix)]
fn unix_pid_is_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub(crate) fn terminate_pid(pid: u32) {
    if pid == 0 {
        return;
    }
    #[cfg(windows)]
    {
        windows_terminate(pid);
    }
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status();
    }
}

#[cfg(windows)]
fn windows_terminate(pid: u32) {
    const PROCESS_TERMINATE: u32 = 0x0001;
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
        fn TerminateProcess(handle: *mut std::ffi::c_void, exit_code: u32) -> i32;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if handle.is_null() {
            return;
        }
        TerminateProcess(handle, 1);
        CloseHandle(handle);
    }
}

#[derive(Debug)]
pub(crate) enum ChildRunError {
    Timeout { pid: Option<u32> },
    Io(std::io::Error),
}

/// Hard cap on bytes captured from one child process stream
/// (ProcessOutputLimit). Prevents an unbounded page dump from exhausting
/// memory before semantic budgeting happens.
const MAX_PROCESS_OUTPUT_BYTES: usize =
    crate::tools::browser_output::BROWSER_PROCESS_OUTPUT_LIMIT_BYTES;
const PIPE_DRAIN_GRACE_MS: u64 = 500;

async fn read_pipe_limited<R>(mut pipe: R, output: Arc<Mutex<Vec<u8>>>)
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;
    let mut chunk = [0u8; 65_536];
    loop {
        match pipe.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let mut buf = output.lock();
                let remaining = MAX_PROCESS_OUTPUT_BYTES.saturating_sub(buf.len());
                if remaining == 0 {
                    break;
                }
                let take = n.min(remaining);
                buf.extend_from_slice(&chunk[..take]);
                if buf.len() >= MAX_PROCESS_OUTPUT_BYTES {
                    break;
                }
            }
        }
    }
}

async fn finish_pipe_read(
    task: Option<tokio::task::JoinHandle<()>>,
    output: Arc<Mutex<Vec<u8>>>,
) -> Vec<u8> {
    if let Some(mut task) = task {
        if timeout(Duration::from_millis(PIPE_DRAIN_GRACE_MS), &mut task)
            .await
            .is_err()
        {
            // Some agent-browser daemon versions inherit the short-lived
            // CLI's stdout/stderr handles. The CLI has already exited and its
            // response has been read, but EOF will not arrive until the daemon
            // exits. Drop our read handle instead of hanging the tool call.
            task.abort();
            let _ = task.await;
        }
    }
    let bytes = output.lock().clone();
    bytes
}

pub(crate) async fn run_command_with_timeout(
    mut cmd: Command,
    timeout_secs: u64,
) -> Result<std::process::Output, ChildRunError> {
    cmd.kill_on_drop(true);
    let mut child = cmd.spawn().map_err(ChildRunError::Io)?;
    let pid = child.id();

    let stdout = Arc::new(Mutex::new(Vec::new()));
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let stdout_task = child.stdout.take().map(|pipe| {
        let output = Arc::clone(&stdout);
        tokio::spawn(async move { read_pipe_limited(pipe, output).await })
    });
    let stderr_task = child.stderr.take().map(|pipe| {
        let output = Arc::clone(&stderr);
        tokio::spawn(async move { read_pipe_limited(pipe, output).await })
    });

    let status = match timeout(Duration::from_secs(timeout_secs), child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => return Err(ChildRunError::Io(e)),
        Err(_) => {
            if let Some(pid) = pid {
                terminate_pid(pid);
            }
            let _ = child.start_kill();
            let _ = timeout(Duration::from_secs(3), child.wait()).await;
            if let Some(task) = stdout_task {
                task.abort();
            }
            if let Some(task) = stderr_task {
                task.abort();
            }
            return Err(ChildRunError::Timeout { pid });
        }
    };

    let stdout = finish_pipe_read(stdout_task, stdout).await;
    let stderr = finish_pipe_read(stderr_task, stderr).await;
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_actions_are_readonly_or_idempotent_open() {
        for action in [
            "open",
            "snapshot",
            "get_url",
            "get_title",
            "get_text",
            "wait",
            "reload",
            "is_visible",
        ] {
            assert!(is_retryable_action(action), "{action}");
            assert!(auto_retry_allowed(
                action,
                false,
                false,
                BrowserFailureKind::DaemonUnavailable,
                "Failed to read: os error 10060",
            ));
            assert!(
                !auto_retry_allowed(
                    action,
                    true,
                    false,
                    BrowserFailureKind::DaemonUnavailable,
                    "Failed to read: os error 10060",
                ),
                "{action} must recover at most once"
            );
        }
        for action in ["click", "fill", "type", "press", "select", "eval"] {
            assert!(!is_retryable_action(action), "{action} must not auto-retry");
            assert_eq!(action_kind(action), BrowserActionKind::Mutating);
            assert!(
                !auto_retry_allowed(
                    action,
                    false,
                    false,
                    BrowserFailureKind::DaemonUnavailable,
                    "Failed to read: os error 10060",
                ),
                "{action} must not automatic retry"
            );
        }
    }

    #[test]
    fn concurrent_daemon_race_allows_one_followup_after_recovery() {
        let output =
            "A daemon for session 'x' started concurrently with different daemon configuration";
        assert!(is_concurrent_start_race(output));
        assert!(auto_retry_allowed(
            "get_url",
            true,
            false,
            BrowserFailureKind::DaemonUnavailable,
            output,
        ));
        assert!(!auto_retry_allowed(
            "get_url",
            true,
            true,
            BrowserFailureKind::DaemonUnavailable,
            output,
        ));
        assert!(!auto_retry_allowed(
            "click",
            true,
            false,
            BrowserFailureKind::DaemonUnavailable,
            output,
        ));
    }

    #[test]
    fn connection_errors_classify_as_daemon_unavailable() {
        let kind = classify_browser_output(
            r#"{"error":"Failed to read: os error 10060","success":false}"#,
        );
        assert_eq!(kind, BrowserFailureKind::DaemonUnavailable);
        assert!(should_recover(kind));
        assert_eq!(failure_prefix(kind), "BrowserDaemonUnavailable");
    }

    #[test]
    fn concurrent_daemon_error_is_recoverable() {
        let kind = classify_browser_output(
            "A daemon for session 'x' started concurrently with different daemon configuration",
        );
        assert_eq!(kind, BrowserFailureKind::DaemonUnavailable);
        assert!(should_recover(kind));
    }

    #[test]
    fn owned_session_registry_is_isolated() {
        with_owned_sessions_lock(|| {
            remember_owned_browser_session("omninova-aaa");
            remember_owned_browser_session("omninova-bbb");
            remember_owned_browser_session("omninova-aaa");
            assert_eq!(
                owned_browser_sessions(),
                vec!["omninova-aaa".to_string(), "omninova-bbb".to_string()]
            );
            forget_owned_browser_session("omninova-aaa");
            assert_eq!(owned_browser_sessions(), vec!["omninova-bbb".to_string()]);
        });
    }

    #[test]
    fn sidecar_clear_skips_live_pid_and_other_sessions() {
        let root = std::env::temp_dir().join(format!("omninova-sidecars-{}", uuid::Uuid::new_v4()));
        let dir = root.join("namespaces").join("omninova").join("run");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("sess-a.port"), b"1").unwrap();
        std::fs::write(dir.join("sess-b.port"), b"2").unwrap();
        std::fs::write(dir.join("sess-a.pid"), b"0").unwrap();
        assert!(clear_stale_session_sidecars_in(&dir, "sess-a"));
        assert!(!dir.join("sess-a.port").exists());
        assert!(dir.join("sess-b.port").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn timeout_kills_cli_child() {
        let mut cmd = Command::new(sleep_command());
        for arg in sleep_args(30) {
            cmd.arg(arg);
        }
        cmd.stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        crate::tools::configure_background_command(&mut cmd);
        let started = Instant::now();
        let err = run_command_with_timeout(cmd, 1)
            .await
            .err()
            .expect("sleep must time out");
        assert!(
            started.elapsed() < Duration::from_secs(8),
            "took {:?}",
            started.elapsed()
        );
        match err {
            ChildRunError::Timeout { pid } => {
                if let Some(pid) = pid {
                    std::thread::sleep(Duration::from_millis(200));
                    assert!(
                        !pid_is_alive(pid),
                        "timed-out CLI child {pid} must not remain"
                    );
                }
            }
            other => panic!("expected timeout, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn pipe_drain_keeps_output_when_a_daemon_holds_the_writer_open() {
        use tokio::io::AsyncWriteExt;

        let (mut writer, reader) = tokio::io::duplex(64);
        let output = Arc::new(Mutex::new(Vec::new()));
        let reader_output = Arc::clone(&output);
        let task = tokio::spawn(async move { read_pipe_limited(reader, reader_output).await });
        let writer_task = tokio::spawn(async move {
            writer.write_all(b"cli-response").await.unwrap();
            tokio::time::sleep(Duration::from_secs(10)).await;
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        let started = Instant::now();
        let bytes = finish_pipe_read(Some(task), output).await;
        writer_task.abort();

        assert_eq!(bytes, b"cli-response");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "pipe drain waited for inherited writer: {:?}",
            started.elapsed()
        );
    }

    fn sleep_command() -> &'static str {
        #[cfg(windows)]
        {
            "powershell.exe"
        }
        #[cfg(not(windows))]
        {
            "sleep"
        }
    }

    fn sleep_args(secs: u64) -> Vec<String> {
        #[cfg(windows)]
        {
            vec![
                "-NoProfile".into(),
                "-Command".into(),
                format!("Start-Sleep -Seconds {secs}"),
            ]
        }
        #[cfg(not(windows))]
        {
            vec![secs.to_string()]
        }
    }
}
