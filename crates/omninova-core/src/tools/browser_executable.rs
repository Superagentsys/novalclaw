//! Deterministic, launch-probed browser executable selection for Managed Browser.
//!
//! Foreign `~/.agent-browser/browsers` state is intentionally not searched.
//! Automatic candidates fall through after a failed probe; an explicit trusted
//! configuration pin fails closed.

use crate::tools::browser_bin::{BrowserBinarySearch, BrowserBinarySearch as AgentBrowserSearch};
use crate::tools::browser_lifecycle::{
    clear_stale_session_sidecars, run_command_with_timeout, ChildRunError,
};
use crate::tools::configure_background_command;
use async_trait::async_trait;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::OnceCell;

pub(crate) const BROWSER_EXECUTABLE_PROBE_TIMEOUT_SECS: u64 = 8;
const PROBE_NAMESPACE_PREFIX: &str = "omninova-browser-probe";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BrowserExecutableSource {
    Configured,
    OmniNovaManaged,
    SystemChrome,
    SystemBrave,
}

impl BrowserExecutableSource {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Configured => "configured",
            Self::OmniNovaManaged => "omninova_managed",
            Self::SystemChrome => "system_chrome",
            Self::SystemBrave => "system_brave",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BrowserExecutable {
    pub(crate) path: PathBuf,
    pub(crate) source: BrowserExecutableSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BrowserExecutableProbeResult {
    pub(crate) usable: bool,
    pub(crate) diagnostic: &'static str,
}

impl BrowserExecutableProbeResult {
    fn usable() -> Self {
        Self {
            usable: true,
            diagnostic: "ok",
        }
    }

    #[cfg(test)]
    fn failed(diagnostic: &'static str) -> Self {
        Self {
            usable: false,
            diagnostic,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BrowserExecutableResolveError {
    pub(crate) detail: String,
}

impl std::fmt::Display for BrowserExecutableResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

#[async_trait]
pub(crate) trait BrowserExecutableProbe: Send + Sync {
    async fn probe(&self, candidate: &Path) -> BrowserExecutableProbeResult;
}

struct AgentBrowserExecutableProbe {
    search: Option<AgentBrowserSearch>,
}

impl AgentBrowserExecutableProbe {
    fn new(search: Option<AgentBrowserSearch>) -> Self {
        Self { search }
    }

    fn resolve_agent_browser(&self) -> Result<PathBuf, ()> {
        let search = self
            .search
            .clone()
            .unwrap_or_else(BrowserBinarySearch::from_process);
        search.resolve().map(|resolved| resolved.path).map_err(|_| ())
    }
}

#[async_trait]
impl BrowserExecutableProbe for AgentBrowserExecutableProbe {
    async fn probe(&self, candidate: &Path) -> BrowserExecutableProbeResult {
        let Ok(agent_browser) = self.resolve_agent_browser() else {
            return BrowserExecutableProbeResult {
                usable: false,
                diagnostic: "agent_browser_unavailable",
            };
        };
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let session = format!("probe-{}", &nonce[..16]);
        let namespace = format!("{PROBE_NAMESPACE_PREFIX}-{}", std::process::id());
        let profile = std::env::temp_dir().join(format!("omninova-browser-probe-{nonce}"));
        if std::fs::create_dir_all(&profile).is_err() {
            return BrowserExecutableProbeResult {
                usable: false,
                diagnostic: "temp_profile_error",
            };
        }

        let mut command = Command::new(&agent_browser);
        configure_background_command(&mut command);
        command
            .arg("--session")
            .arg(&session)
            .arg("--namespace")
            .arg(&namespace)
            .arg("--executable-path")
            .arg(candidate)
            .arg("--profile")
            .arg(&profile)
            .arg("--json")
            .arg("open")
            .arg("about:blank")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let result = match run_command_with_timeout(command, BROWSER_EXECUTABLE_PROBE_TIMEOUT_SECS)
            .await
        {
            Ok(output) if output.status.success() => {
                let parsed = serde_json::from_slice::<serde_json::Value>(&output.stdout).ok();
                if parsed
                    .as_ref()
                    .and_then(|value| value.get("success"))
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                {
                    BrowserExecutableProbeResult::usable()
                } else {
                    BrowserExecutableProbeResult {
                        usable: false,
                        diagnostic: "invalid_response",
                    }
                }
            }
            Ok(_) => BrowserExecutableProbeResult {
                usable: false,
                diagnostic: "nonzero_exit",
            },
            Err(ChildRunError::Timeout { .. }) => BrowserExecutableProbeResult {
                usable: false,
                diagnostic: "timeout",
            },
            Err(ChildRunError::Io(_)) => BrowserExecutableProbeResult {
                usable: false,
                diagnostic: "spawn_error",
            },
        };

        let mut close = Command::new(&agent_browser);
        configure_background_command(&mut close);
        close
            .arg("--session")
            .arg(&session)
            .arg("--namespace")
            .arg(&namespace)
            .arg("--json")
            .arg("close")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let _ = run_command_with_timeout(close, BROWSER_EXECUTABLE_PROBE_TIMEOUT_SECS).await;
        clear_stale_session_sidecars(&namespace, &session);
        let _ = std::fs::remove_dir_all(&profile);
        result
    }
}

#[derive(Clone, Debug)]
struct BrowserExecutableCandidate {
    path: PathBuf,
    source: BrowserExecutableSource,
    explicit: bool,
}

#[derive(Clone)]
pub(crate) struct BrowserExecutableResolver {
    candidates: Arc<Vec<BrowserExecutableCandidate>>,
    probe: Arc<dyn BrowserExecutableProbe>,
    selected: Arc<OnceCell<Result<BrowserExecutable, BrowserExecutableResolveError>>>,
    has_explicit: bool,
}

impl BrowserExecutableResolver {
    pub(crate) fn from_process(
        agent_browser_search: Option<BrowserBinarySearch>,
        configured: Option<PathBuf>,
    ) -> Self {
        let candidates = process_candidates(configured);
        let has_explicit = candidates.first().is_some_and(|candidate| candidate.explicit);
        Self {
            candidates: Arc::new(candidates),
            probe: Arc::new(AgentBrowserExecutableProbe::new(agent_browser_search)),
            selected: Arc::new(OnceCell::new()),
            has_explicit,
        }
    }

    pub(crate) fn has_explicit(&self) -> bool {
        self.has_explicit
    }

    pub(crate) async fn resolve(
        &self,
    ) -> Result<BrowserExecutable, BrowserExecutableResolveError> {
        self.selected
            .get_or_init(|| async { resolve_candidates(&self.candidates, self.probe.as_ref()).await })
            .await
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        candidates: Vec<(PathBuf, BrowserExecutableSource, bool)>,
        probe: Arc<dyn BrowserExecutableProbe>,
    ) -> Self {
        let candidates = candidates
            .into_iter()
            .map(|(path, source, explicit)| BrowserExecutableCandidate {
                path,
                source,
                explicit,
            })
            .collect::<Vec<_>>();
        let has_explicit = candidates.first().is_some_and(|candidate| candidate.explicit);
        Self {
            candidates: Arc::new(candidates),
            probe,
            selected: Arc::new(OnceCell::new()),
            has_explicit,
        }
    }
}

async fn resolve_candidates(
    candidates: &[BrowserExecutableCandidate],
    probe: &dyn BrowserExecutableProbe,
) -> Result<BrowserExecutable, BrowserExecutableResolveError> {
    let mut diagnostics = Vec::new();
    for candidate in candidates {
        if !candidate.path.is_absolute() || !candidate.path.is_file() {
            let diagnostic = candidate_diagnostic(candidate, "missing");
            if candidate.explicit {
                return Err(unavailable_error(vec![diagnostic], true));
            }
            diagnostics.push(diagnostic);
            continue;
        }
        let result = probe.probe(&candidate.path).await;
        if result.usable {
            tracing::info!(
                target: "browser",
                source = candidate.source.as_str(),
                basename = candidate_basename(candidate),
                "selected launch-probed browser executable"
            );
            return Ok(BrowserExecutable {
                path: candidate.path.clone(),
                source: candidate.source,
            });
        }
        let diagnostic = candidate_diagnostic(candidate, result.diagnostic);
        tracing::warn!(
            target: "browser",
            source = candidate.source.as_str(),
            basename = candidate_basename(candidate),
            failure = result.diagnostic,
            "browser executable probe failed"
        );
        if candidate.explicit {
            return Err(unavailable_error(vec![diagnostic], true));
        }
        diagnostics.push(diagnostic);
    }
    Err(unavailable_error(diagnostics, false))
}

fn candidate_diagnostic(candidate: &BrowserExecutableCandidate, failure: &str) -> String {
    format!(
        "source={} basename={} failure={}",
        candidate.source.as_str(),
        candidate_basename(candidate),
        failure.chars().take(80).collect::<String>()
    )
}

fn candidate_basename(candidate: &BrowserExecutableCandidate) -> String {
    candidate
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("browser")
        .chars()
        .take(80)
        .collect()
}

fn unavailable_error(
    diagnostics: Vec<String>,
    configured: bool,
) -> BrowserExecutableResolveError {
    let candidates = if diagnostics.is_empty() {
        "none".to_string()
    } else {
        diagnostics.into_iter().take(8).collect::<Vec<_>>().join("; ")
    };
    BrowserExecutableResolveError {
        detail: format!(
            "BrowserUnavailable: no usable Chrome-compatible browser was found for Managed Browser; configured_pin={configured} candidates={candidates}"
        ),
    }
}

fn process_candidates(configured: Option<PathBuf>) -> Vec<BrowserExecutableCandidate> {
    let mut candidates = Vec::new();
    if let Some(path) = configured {
        candidates.push(BrowserExecutableCandidate {
            path,
            source: BrowserExecutableSource::Configured,
            explicit: true,
        });
        return candidates;
    }

    candidates.extend(omninova_managed_candidates());
    #[cfg(windows)]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            push_unique_candidate(
                &mut candidates,
                local.join(r"Google\Chrome\Application\chrome.exe"),
                BrowserExecutableSource::SystemChrome,
            );
        }
        if let Some(program_files) = std::env::var_os("PROGRAMFILES").map(PathBuf::from) {
            push_unique_candidate(
                &mut candidates,
                program_files.join(r"Google\Chrome\Application\chrome.exe"),
                BrowserExecutableSource::SystemChrome,
            );
        }
        if let Some(program_files_x86) = std::env::var_os("PROGRAMFILES(X86)").map(PathBuf::from) {
            push_unique_candidate(
                &mut candidates,
                program_files_x86.join(r"Google\Chrome\Application\chrome.exe"),
                BrowserExecutableSource::SystemChrome,
            );
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            push_unique_candidate(
                &mut candidates,
                local.join(r"BraveSoftware\Brave-Browser\Application\brave.exe"),
                BrowserExecutableSource::SystemBrave,
            );
        }
    }
    candidates
}

fn omninova_managed_candidates() -> Vec<BrowserExecutableCandidate> {
    let mut candidates = Vec::new();
    if let Ok(executable) = std::env::current_exe() {
        if let Some(root) = executable.parent() {
            for path in [
                root.join(r"resources\browser\windows\chrome.exe"),
                root.join(r"browser\windows\chrome.exe"),
            ] {
                push_unique_candidate(
                    &mut candidates,
                    path,
                    BrowserExecutableSource::OmniNovaManaged,
                );
            }
        }
    }
    candidates
}

fn push_unique_candidate(
    candidates: &mut Vec<BrowserExecutableCandidate>,
    path: PathBuf,
    source: BrowserExecutableSource,
) {
    if !candidates.iter().any(|candidate| candidate.path == path) {
        candidates.push(BrowserExecutableCandidate {
            path,
            source,
            explicit: false,
        });
    }
}

pub(crate) fn browser_executable_argv(path: Option<&Path>) -> Vec<OsString> {
    match path {
        Some(path) => vec!["--executable-path".into(), path.as_os_str().to_os_string()],
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use std::collections::HashMap;

    struct FakeProbe {
        outcomes: HashMap<PathBuf, BrowserExecutableProbeResult>,
        calls: Mutex<Vec<PathBuf>>,
    }

    #[async_trait]
    impl BrowserExecutableProbe for FakeProbe {
        async fn probe(&self, candidate: &Path) -> BrowserExecutableProbeResult {
            self.calls.lock().push(candidate.to_path_buf());
            self.outcomes
                .get(candidate)
                .cloned()
                .unwrap_or_else(|| BrowserExecutableProbeResult::failed("unexpected"))
        }
    }

    fn test_file(root: &Path, name: &str) -> PathBuf {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"test executable").unwrap();
        path
    }

    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "omninova-browser-executable-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[tokio::test]
    async fn unusable_first_candidate_falls_back_to_working_browser() {
        let root = temp_root();
        let first = test_file(&root, "managed/chrome.exe");
        let second = test_file(&root, "system/chrome.exe");
        let probe = Arc::new(FakeProbe {
            outcomes: HashMap::from([
                (first.clone(), BrowserExecutableProbeResult::failed("launch_failed")),
                (second.clone(), BrowserExecutableProbeResult::usable()),
            ]),
            calls: Mutex::new(Vec::new()),
        });
        let resolver = BrowserExecutableResolver::for_test(
            vec![
                (first.clone(), BrowserExecutableSource::OmniNovaManaged, false),
                (second.clone(), BrowserExecutableSource::SystemChrome, false),
            ],
            probe.clone(),
        );
        let selected = resolver.resolve().await.unwrap();
        assert_eq!(selected.path, second);
        assert_eq!(selected.source, BrowserExecutableSource::SystemChrome);
        assert_eq!(probe.calls.lock().as_slice(), &[first, selected.path]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn configured_candidate_is_first_and_fails_closed() {
        let root = temp_root();
        let configured = test_file(&root, "configured/chrome.exe");
        let fallback = test_file(&root, "system/chrome.exe");
        let probe = Arc::new(FakeProbe {
            outcomes: HashMap::from([
                (
                    configured.clone(),
                    BrowserExecutableProbeResult::failed("launch_failed"),
                ),
                (fallback.clone(), BrowserExecutableProbeResult::usable()),
            ]),
            calls: Mutex::new(Vec::new()),
        });
        let resolver = BrowserExecutableResolver::for_test(
            vec![
                (configured.clone(), BrowserExecutableSource::Configured, true),
                (fallback, BrowserExecutableSource::SystemChrome, false),
            ],
            probe.clone(),
        );
        let error = resolver.resolve().await.unwrap_err();
        assert!(error.detail.starts_with("BrowserUnavailable:"));
        assert!(error.detail.contains("configured_pin=true"));
        assert!(!error.detail.contains(&root.to_string_lossy().to_string()));
        assert_eq!(probe.calls.lock().as_slice(), &[configured]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn configured_candidate_passes_and_selection_is_cached() {
        let root = temp_root();
        let configured = test_file(&root, "configured/chrome.exe");
        let probe = Arc::new(FakeProbe {
            outcomes: HashMap::from([(
                configured.clone(),
                BrowserExecutableProbeResult::usable(),
            )]),
            calls: Mutex::new(Vec::new()),
        });
        let resolver = BrowserExecutableResolver::for_test(
            vec![(configured.clone(), BrowserExecutableSource::Configured, true)],
            probe.clone(),
        );
        assert_eq!(resolver.resolve().await.unwrap().path, configured);
        assert_eq!(resolver.resolve().await.unwrap().source, BrowserExecutableSource::Configured);
        assert_eq!(probe.calls.lock().len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn automatic_candidate_priority_is_managed_then_chrome_then_brave() {
        let root = temp_root();
        let managed = test_file(&root, "managed/chrome.exe");
        let chrome = test_file(&root, "chrome/chrome.exe");
        let brave = test_file(&root, "brave/brave.exe");
        let probe = Arc::new(FakeProbe {
            outcomes: HashMap::from([
                (managed.clone(), BrowserExecutableProbeResult::usable()),
                (chrome.clone(), BrowserExecutableProbeResult::usable()),
                (brave.clone(), BrowserExecutableProbeResult::usable()),
            ]),
            calls: Mutex::new(Vec::new()),
        });
        let resolver = BrowserExecutableResolver::for_test(
            vec![
                (managed.clone(), BrowserExecutableSource::OmniNovaManaged, false),
                (chrome, BrowserExecutableSource::SystemChrome, false),
                (brave, BrowserExecutableSource::SystemBrave, false),
            ],
            probe.clone(),
        );
        let selected = resolver.resolve().await.unwrap();
        assert_eq!(selected.path, managed);
        assert_eq!(selected.source, BrowserExecutableSource::OmniNovaManaged);
        assert_eq!(probe.calls.lock().len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn no_existing_candidate_returns_browser_unavailable() {
        let root = temp_root();
        let missing = root.join("missing/chrome.exe");
        let probe = Arc::new(FakeProbe {
            outcomes: HashMap::new(),
            calls: Mutex::new(Vec::new()),
        });
        let resolver = BrowserExecutableResolver::for_test(
            vec![(missing, BrowserExecutableSource::SystemChrome, false)],
            probe.clone(),
        );
        let error = resolver.resolve().await.unwrap_err();
        assert!(error.detail.starts_with("BrowserUnavailable:"));
        assert!(probe.calls.lock().is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn executable_argv_is_two_absolute_entries() {
        let path = if cfg!(windows) {
            PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe")
        } else {
            PathBuf::from("/opt/google/chrome/chrome")
        };
        let argv = browser_executable_argv(Some(&path));
        assert_eq!(argv.len(), 2);
        assert_eq!(argv[0], "--executable-path");
        assert_eq!(Path::new(&argv[1]), path);
        assert!(Path::new(&argv[1]).is_absolute());
    }

    #[test]
    fn configured_path_is_the_only_candidate_and_foreign_cache_is_never_discovered() {
        let configured = if cfg!(windows) {
            PathBuf::from(r"C:\trusted\chrome.exe")
        } else {
            PathBuf::from("/trusted/chrome")
        };
        let configured_candidates = process_candidates(Some(configured.clone()));
        assert_eq!(configured_candidates.len(), 1);
        assert_eq!(configured_candidates[0].path, configured);
        assert_eq!(
            configured_candidates[0].source,
            BrowserExecutableSource::Configured
        );
        assert!(configured_candidates[0].explicit);

        let automatic = process_candidates(None);
        assert!(automatic.iter().all(|candidate| {
            !candidate
                .path
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains(".agent-browser")
        }));
        let first_brave = automatic
            .iter()
            .position(|candidate| candidate.source == BrowserExecutableSource::SystemBrave);
        let last_chrome = automatic
            .iter()
            .rposition(|candidate| candidate.source == BrowserExecutableSource::SystemChrome);
        if let (Some(first_brave), Some(last_chrome)) = (first_brave, last_chrome) {
            assert!(last_chrome < first_brave);
        }
    }

    #[test]
    fn trusted_browser_config_accepts_executable_path_without_tool_schema_changes() {
        let config: crate::config::BrowserConfig = toml::from_str(
            r#"enabled = true
backend = "agent-browser"
executable_path = "C:/trusted/Chrome/chrome.exe"
"#,
        )
        .unwrap();
        assert_eq!(
            config.executable_path,
            Some(PathBuf::from("C:/trusted/Chrome/chrome.exe"))
        );
    }
}
