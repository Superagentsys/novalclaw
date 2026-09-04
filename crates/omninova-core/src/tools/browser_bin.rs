//! Single resolution path for the `agent-browser` executable.
//!
//! Order:
//! 1. `OMNINOVA_AGENT_BROWSER_BIN` if it is a native executable (Windows
//!    `.cmd`/`.bat` shims are unwrapped to the underlying exe)
//! 2. Bundled resource candidates (registered roots + next to current exe)
//! 3. PATH native executable (`agent-browser.exe` / `agent-browser`)
//! 4. PATH npm/script shim → underlying native executable
//! 5. Script shims are never spawned directly on Windows (`CREATE_NO_WINDOW`
//!    cannot drive `cmd` wrappers)
//! 6. `BrowserBinaryMissing` / `BrowserBinaryNotExecutable`
//!
//! An explicit env value that is not a usable native binary fails immediately.
//! It does not fall through to bundled or PATH, and it is never written back
//! as if it were available.

use parking_lot::Mutex;
use std::path::{Path, PathBuf};

/// Process env var that may pin the binary. Never set this to a missing path.
pub const AGENT_BROWSER_BIN_ENV: &str = "OMNINOVA_AGENT_BROWSER_BIN";

const MAX_LOGGED_CANDIDATES: usize = 24;

static EXTRA_SEARCH_ROOTS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Register extra roots (Tauri `resource_dir`, etc.). Not a binary path.
pub fn set_agent_browser_search_roots(roots: Vec<PathBuf>) {
    *EXTRA_SEARCH_ROOTS.lock() = roots;
}

pub fn agent_browser_search_roots() -> Vec<PathBuf> {
    EXTRA_SEARCH_ROOTS.lock().clone()
}

/// Platform-relative location inside a resource root or next to the exe.
pub fn bundled_agent_browser_relative_path() -> &'static str {
    match std::env::consts::OS {
        "macos" => "agent-browser/macos/agent-browser",
        "linux" => "agent-browser/linux/agent-browser",
        "windows" => "agent-browser/windows/agent-browser.exe",
        _ => "agent-browser/agent-browser",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentBrowserBinarySource {
    Env,
    Bundled,
    Path,
}

impl AgentBrowserBinarySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::Bundled => "bundled",
            Self::Path => "path",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentBrowserBinaryResolved {
    pub path: PathBuf,
    pub source: AgentBrowserBinarySource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentBrowserResolveError {
    Missing(AgentBrowserBinaryMissing),
    NotExecutable {
        requested_binary: String,
        resolution_source: &'static str,
        detail: String,
        checked_candidates: Vec<String>,
    },
}

impl AgentBrowserResolveError {
    pub fn resolution_source(&self) -> &'static str {
        match self {
            Self::Missing(missing) => missing.resolution_source,
            Self::NotExecutable {
                resolution_source, ..
            } => resolution_source,
        }
    }
}

impl std::fmt::Display for AgentBrowserResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(missing) => write!(f, "{missing}"),
            Self::NotExecutable {
                requested_binary,
                resolution_source,
                detail,
                checked_candidates,
            } => write!(
                f,
                "BrowserBinaryNotExecutable: requested_binary={requested_binary} resolution_source={resolution_source} detail={detail} checked_candidates={}",
                if checked_candidates.is_empty() {
                    "-".to_string()
                } else {
                    checked_candidates.join("; ")
                }
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentBrowserBinaryMissing {
    pub requested_binary: Option<String>,
    pub resolution_source: &'static str,
    pub checked_candidates: Vec<String>,
}

impl std::fmt::Display for AgentBrowserBinaryMissing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BrowserBinaryMissing: requested_binary={} resolution_source={} checked_candidates={}",
            self.requested_binary.as_deref().unwrap_or("-"),
            self.resolution_source,
            if self.checked_candidates.is_empty() {
                "-".to_string()
            } else {
                self.checked_candidates.join("; ")
            }
        )
    }
}

/// Inputs for the pure resolver. Production uses [`BrowserBinarySearch::from_process`].
#[derive(Debug, Clone, Default)]
pub struct BrowserBinarySearch {
    /// When `Some`, this value is treated as `OMNINOVA_AGENT_BROWSER_BIN`.
    pub env_path: Option<PathBuf>,
    /// Explicit bundled file candidates (already joined).
    pub bundled_candidates: Vec<PathBuf>,
    /// Resource roots; each is expanded with the platform-relative layout.
    pub extra_roots: Vec<PathBuf>,
    /// Also search next to `current_exe()` (`./` and `./resources/`).
    pub include_exe_relative: bool,
    /// `None` = use process PATH. `Some` = only these directories (empty = no PATH).
    pub path_dirs: Option<Vec<PathBuf>>,
}

impl BrowserBinarySearch {
    pub fn from_process() -> Self {
        Self {
            env_path: std::env::var_os(AGENT_BROWSER_BIN_ENV).map(PathBuf::from),
            bundled_candidates: Vec::new(),
            extra_roots: agent_browser_search_roots(),
            include_exe_relative: true,
            path_dirs: None,
        }
    }

    pub fn resolve(&self) -> Result<AgentBrowserBinaryResolved, AgentBrowserResolveError> {
        resolve_agent_browser_binary_with(self)
    }
}

pub fn resolve_agent_browser_binary() -> Result<AgentBrowserBinaryResolved, AgentBrowserResolveError>
{
    BrowserBinarySearch::from_process().resolve()
}

pub fn agent_browser_runtime_available() -> bool {
    resolve_agent_browser_binary().is_ok()
}

/// Persist desktop `browser.enabled` so it cannot claim a missing runtime.
/// Returns whether the flag changed.
pub fn sync_browser_enabled_with_runtime(configured_enabled: &mut bool, available: bool) -> bool {
    if *configured_enabled == available {
        return false;
    }
    *configured_enabled = available;
    true
}

pub fn effective_browser_capability(configured_enabled: bool, runtime_available: bool) -> bool {
    configured_enabled && runtime_available
}

pub fn resolve_agent_browser_binary_with(
    search: &BrowserBinarySearch,
) -> Result<AgentBrowserBinaryResolved, AgentBrowserResolveError> {
    let mut checked = Vec::new();
    let mut unusable_shim: Option<PathBuf> = None;

    if let Some(env_path) = &search.env_path {
        push_checked(&mut checked, env_path);
        match resolve_native_spawn_path(env_path, &mut checked) {
            NativeSpawn::Native(path) => {
                return Ok(AgentBrowserBinaryResolved {
                    path,
                    source: AgentBrowserBinarySource::Env,
                });
            }
            NativeSpawn::UnusableShim(shim) => {
                return Err(not_executable_shim(
                    shim,
                    AgentBrowserBinarySource::Env.as_str(),
                    checked,
                ));
            }
            NativeSpawn::Missing => {
                return Err(AgentBrowserResolveError::Missing(
                    AgentBrowserBinaryMissing {
                        requested_binary: Some(display_path(env_path)),
                        resolution_source: AgentBrowserBinarySource::Env.as_str(),
                        checked_candidates: take_checked(checked),
                    },
                ));
            }
        }
    }

    for candidate in expand_bundled_candidates(search) {
        push_checked(&mut checked, &candidate);
        match resolve_native_spawn_path(&candidate, &mut checked) {
            NativeSpawn::Native(path) => {
                return Ok(AgentBrowserBinaryResolved {
                    path,
                    source: AgentBrowserBinarySource::Bundled,
                });
            }
            NativeSpawn::UnusableShim(shim) => {
                unusable_shim.get_or_insert(shim);
            }
            NativeSpawn::Missing => {}
        }
    }

    let path_names = path_binary_names();
    let dirs = match &search.path_dirs {
        Some(dirs) => dirs.clone(),
        None => system_path_dirs(),
    };
    for dir in dirs {
        for name in path_names {
            let candidate = dir.join(name);
            push_checked(&mut checked, &candidate);
            match resolve_native_spawn_path(&candidate, &mut checked) {
                NativeSpawn::Native(path) => {
                    return Ok(AgentBrowserBinaryResolved {
                        path,
                        source: AgentBrowserBinarySource::Path,
                    });
                }
                NativeSpawn::UnusableShim(shim) => {
                    unusable_shim.get_or_insert(shim);
                }
                NativeSpawn::Missing => {}
            }
        }
    }

    if let Some(shim) = unusable_shim {
        return Err(not_executable_shim(shim, "path", checked));
    }

    Err(AgentBrowserResolveError::Missing(
        AgentBrowserBinaryMissing {
            requested_binary: Some(path_names[0].to_string()),
            resolution_source: "none",
            checked_candidates: take_checked(checked),
        },
    ))
}

enum NativeSpawn {
    Native(PathBuf),
    UnusableShim(PathBuf),
    Missing,
}

fn not_executable_shim(
    shim: PathBuf,
    resolution_source: &'static str,
    checked: Vec<String>,
) -> AgentBrowserResolveError {
    AgentBrowserResolveError::NotExecutable {
        requested_binary: display_path(&shim),
        resolution_source,
        detail: "windows_script_shim_without_native_executable".to_string(),
        checked_candidates: take_checked(checked),
    }
}

fn resolve_native_spawn_path(path: &Path, checked: &mut Vec<String>) -> NativeSpawn {
    let Some(found) = usable_binary_path(path) else {
        return NativeSpawn::Missing;
    };
    if is_windows_script_shim(&found) {
        if let Some(native) = unwrap_windows_shim_to_native(&found) {
            push_checked(checked, &native);
            return NativeSpawn::Native(native);
        }
        return NativeSpawn::UnusableShim(found);
    }
    NativeSpawn::Native(found)
}

fn is_windows_script_shim(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some(ext) if ext.eq_ignore_ascii_case("cmd")
            || ext.eq_ignore_ascii_case("bat")
            || ext.eq_ignore_ascii_case("ps1")
    )
}

fn unwrap_windows_shim_to_native(shim: &Path) -> Option<PathBuf> {
    #[cfg(not(windows))]
    {
        let _ = shim;
        None
    }
    #[cfg(windows)]
    {
        if let Some(from_script) = native_exe_from_cmd_script(shim) {
            if is_native_windows_exe(&from_script) {
                return Some(from_script);
            }
        }
        let dir = shim.parent()?;
        for candidate in npm_native_exe_candidates(dir) {
            if is_native_windows_exe(&candidate) {
                return Some(candidate);
            }
        }
        None
    }
}

#[cfg(windows)]
fn is_native_windows_exe(path: &Path) -> bool {
    usable_binary_path(path).is_some()
        && path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
}

#[cfg(windows)]
fn npm_native_exe_candidates(shim_dir: &Path) -> Vec<PathBuf> {
    let bin = shim_dir
        .join("node_modules")
        .join("agent-browser")
        .join("bin");
    vec![
        bin.join("agent-browser-win32-x64.exe"),
        bin.join("agent-browser-win32-arm64.exe"),
        bin.join("agent-browser.exe"),
    ]
}

#[cfg(windows)]
fn native_exe_from_cmd_script(shim: &Path) -> Option<PathBuf> {
    let dir = shim.parent()?;
    let text = std::fs::read_to_string(shim).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with('@')
            || line.starts_with("REM")
            || line.starts_with("rem")
            || line.starts_with("::")
        {
            continue;
        }
        let spec = quoted_cmd_path(line)?;
        let expanded = if let Some(rest) = spec.strip_prefix("%~dp0") {
            dir.join(rest)
        } else {
            PathBuf::from(spec)
        };
        if is_native_windows_exe(&expanded) {
            return Some(expanded);
        }
    }
    None
}

#[cfg(windows)]
fn quoted_cmd_path(line: &str) -> Option<&str> {
    let start = line.find('"')?;
    let rest = &line[start + 1..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn expand_bundled_candidates(search: &BrowserBinarySearch) -> Vec<PathBuf> {
    let rel = bundled_agent_browser_relative_path();
    let mut out = search.bundled_candidates.clone();
    let mut roots = search.extra_roots.clone();
    if search.include_exe_relative {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                roots.push(dir.to_path_buf());
            }
        }
    }
    for root in roots {
        out.push(root.join(rel));
        out.push(root.join("resources").join(rel));
    }
    let mut seen = Vec::new();
    out.retain(|path| {
        if seen.iter().any(|existing: &PathBuf| existing == path) {
            false
        } else {
            seen.push(path.clone());
            true
        }
    });
    out
}

fn path_binary_names() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        &[
            "agent-browser.exe",
            "agent-browser.cmd",
            "agent-browser.bat",
            "agent-browser",
        ]
    }
    #[cfg(not(windows))]
    {
        &["agent-browser"]
    }
}

fn system_path_dirs() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|value| {
            std::env::split_paths(&value)
                .filter(|dir| !dir.as_os_str().is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn usable_binary_path(path: &Path) -> Option<PathBuf> {
    if is_usable_binary(path) {
        return Some(path.to_path_buf());
    }
    #[cfg(windows)]
    {
        let verbatim = windows_verbatim_path(path)?;
        if verbatim != path && is_usable_binary(&verbatim) {
            return Some(verbatim);
        }
    }
    None
}

fn is_usable_binary(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return path
            .metadata()
            .map(|meta| meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(windows)]
fn windows_verbatim_path(path: &Path) -> Option<PathBuf> {
    let raw = path.to_str()?;
    if raw.starts_with(r"\\?\") {
        return Some(path.to_path_buf());
    }
    if raw.starts_with(r"\\") {
        return Some(PathBuf::from(format!(
            r"\\?\UNC\{}",
            raw.trim_start_matches('\\')
        )));
    }
    if raw.len() >= 2 && raw.as_bytes()[1] == b':' {
        return Some(PathBuf::from(format!(r"\\?\{raw}")));
    }
    None
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn push_checked(checked: &mut Vec<String>, path: &Path) {
    let rendered = display_path(path);
    if !checked.iter().any(|existing| existing == &rendered) {
        checked.push(rendered);
    }
}

fn take_checked(mut checked: Vec<String>) -> Vec<String> {
    checked.truncate(MAX_LOGGED_CANDIDATES);
    checked
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("omninova-browser-bin-{label}-{nanos}"));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_fake_binary(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, b"fake-agent-browser").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms).unwrap();
        }
    }

    fn isolated(root: &Path) -> BrowserBinarySearch {
        BrowserBinarySearch {
            env_path: None,
            bundled_candidates: Vec::new(),
            extra_roots: Vec::new(),
            include_exe_relative: false,
            path_dirs: Some(vec![root.join("empty-path")]),
        }
    }

    fn platform_name() -> &'static str {
        if cfg!(windows) {
            "agent-browser.exe"
        } else {
            "agent-browser"
        }
    }

    #[test]
    fn explicit_env_valid() {
        let root = test_root("env-ok");
        let bin = root.join("dir with spaces").join(platform_name());
        write_fake_binary(&bin);
        let mut search = isolated(&root);
        search.env_path = Some(bin.clone());
        let resolved = search.resolve().unwrap();
        assert_eq!(resolved.source, AgentBrowserBinarySource::Env);
        assert_eq!(resolved.path, bin);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn explicit_env_invalid_does_not_fall_through() {
        let root = test_root("env-bad");
        let bundled = root
            .join("bundle")
            .join(bundled_agent_browser_relative_path());
        write_fake_binary(&bundled);
        let missing = root.join("does-not-exist").join(platform_name());
        let mut search = isolated(&root);
        search.env_path = Some(missing.clone());
        search.extra_roots = vec![root.join("bundle")];
        let err = search.resolve().unwrap_err();
        assert_eq!(err.resolution_source(), "env");
        assert!(err.to_string().starts_with("BrowserBinaryMissing"));
        assert!(err.to_string().contains("does-not-exist"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prepared_tauri_resource_resolves_without_path_or_env() {
        let resource_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/omninova-tauri/src-tauri/resources");
        let expected = resource_root.join(bundled_agent_browser_relative_path());
        if !expected.is_file() {
            eprintln!(
                "skip: prepared browser runtime missing at {}",
                expected.display()
            );
            return;
        }
        let search = BrowserBinarySearch {
            env_path: None,
            bundled_candidates: Vec::new(),
            extra_roots: vec![resource_root],
            include_exe_relative: false,
            path_dirs: Some(Vec::new()),
        };
        let resolved = search
            .resolve()
            .expect("prepared resource must resolve as bundled");
        assert_eq!(resolved.source, AgentBrowserBinarySource::Bundled);
        assert_eq!(resolved.path, expected);
    }

    #[test]
    fn bundled_candidate_valid() {
        let root = test_root("bundled");
        let file = root
            .join("resources")
            .join(bundled_agent_browser_relative_path());
        write_fake_binary(&file);
        let mut search = isolated(&root);
        search.extra_roots = vec![root.clone()];
        let resolved = search.resolve().unwrap();
        assert_eq!(resolved.source, AgentBrowserBinarySource::Bundled);
        assert_eq!(resolved.path, file);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn path_fallback() {
        let root = test_root("path");
        let path_dir = root.join("bin");
        write_fake_binary(&path_dir.join(platform_name()));
        let mut search = isolated(&root);
        search.path_dirs = Some(vec![path_dir.clone()]);
        let resolved = search.resolve().unwrap();
        assert_eq!(resolved.source, AgentBrowserBinarySource::Path);
        assert_eq!(resolved.path, path_dir.join(platform_name()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn all_missing() {
        let root = test_root("missing");
        let err = isolated(&root).resolve().unwrap_err();
        assert_eq!(err.resolution_source(), "none");
        assert!(err.to_string().contains("BrowserBinaryMissing"));
        match err {
            AgentBrowserResolveError::Missing(missing) => {
                assert!(!missing.checked_candidates.is_empty());
            }
            other => panic!("expected missing, got {other}"),
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn path_containing_spaces() {
        let root = test_root("spaces");
        let dir = root.join("Program Files").join("OmniNova");
        let bin = dir.join(platform_name());
        write_fake_binary(&bin);
        let mut search = isolated(&root);
        search.env_path = Some(bin.clone());
        assert_eq!(search.resolve().unwrap().path, bin);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unicode_path() {
        let root = test_root("unicode");
        let dir = root.join("测试目录");
        let bin = dir.join(platform_name());
        write_fake_binary(&bin);
        let mut search = isolated(&root);
        search.env_path = Some(bin.clone());
        assert_eq!(search.resolve().unwrap().path, bin);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn long_path() {
        let root = test_root("long");
        let mut dir = root.clone();
        for i in 0..10 {
            dir.push(format!("segment-{i:02}-xxxxxxxxxxxxxxxxxxxx"));
        }
        let bin = dir.join(platform_name());
        write_fake_binary(&bin);
        let mut search = isolated(&root);
        search.env_path = Some(bin.clone());
        let resolved = search
            .resolve()
            .unwrap_or_else(|err| panic!("long path must resolve: {err}"));
        assert!(resolved.path.is_file() || usable_binary_path(&resolved.path).is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_path_is_not_available() {
        let root = test_root("gone");
        let mut search = isolated(&root);
        search.env_path = Some(root.join("nope").join(platform_name()));
        assert!(search.resolve().is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn windows_verbatim_path() {
        let root = test_root("verbatim");
        let bin = root.join(platform_name());
        write_fake_binary(&bin);
        let canonical = fs::canonicalize(&bin).unwrap();
        assert!(
            canonical.to_string_lossy().starts_with(r"\\?\"),
            "{}",
            canonical.display()
        );
        let mut search = isolated(&root);
        search.env_path = Some(canonical.clone());
        let resolved = search.resolve().unwrap();
        assert_eq!(resolved.source, AgentBrowserBinarySource::Env);
        assert!(resolved.path.is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sync_enabled_mirrors_availability() {
        let mut enabled = true;
        assert!(sync_browser_enabled_with_runtime(&mut enabled, false));
        assert!(!enabled);
        assert!(sync_browser_enabled_with_runtime(&mut enabled, true));
        assert!(enabled);
        assert!(!sync_browser_enabled_with_runtime(&mut enabled, true));
        assert!(effective_browser_capability(true, true));
        assert!(!effective_browser_capability(true, false));
        assert!(!effective_browser_capability(false, true));
    }

    #[cfg(windows)]
    fn write_npm_shim(shim_dir: &Path, native_name: &str) -> (PathBuf, PathBuf) {
        let native = shim_dir
            .join("node_modules")
            .join("agent-browser")
            .join("bin")
            .join(native_name);
        write_fake_binary(&native);
        let shim = shim_dir.join("agent-browser.cmd");
        fs::write(
            &shim,
            format!("@ECHO off\r\n\"%~dp0node_modules\\agent-browser\\bin\\{native_name}\" %*\r\n"),
        )
        .unwrap();
        (shim, native)
    }

    #[cfg(windows)]
    #[test]
    fn npm_cmd_shim_unwraps_to_native_exe() {
        let root = test_root("npm-shim");
        let path_dir = root.join("npm");
        let (_shim, native) = write_npm_shim(&path_dir, "agent-browser-win32-x64.exe");
        let mut search = isolated(&root);
        search.path_dirs = Some(vec![path_dir]);
        let resolved = search.resolve().unwrap();
        assert_eq!(resolved.source, AgentBrowserBinarySource::Path);
        assert_eq!(resolved.path, native);
        assert!(resolved
            .path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe")));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn env_cmd_shim_unwraps_to_native_exe_and_does_not_fall_through() {
        let root = test_root("env-shim");
        let npm = root.join("npm");
        let (shim, native) = write_npm_shim(&npm, "agent-browser-win32-x64.exe");
        let bundled = root
            .join("bundle")
            .join(bundled_agent_browser_relative_path());
        write_fake_binary(&bundled);
        let mut search = isolated(&root);
        search.env_path = Some(shim);
        search.extra_roots = vec![root.join("bundle")];
        let resolved = search.resolve().unwrap();
        assert_eq!(resolved.source, AgentBrowserBinarySource::Env);
        assert_eq!(resolved.path, native);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn cmd_shim_without_native_is_not_executable() {
        let root = test_root("shim-only");
        let path_dir = root.join("npm");
        fs::create_dir_all(&path_dir).unwrap();
        let shim = path_dir.join("agent-browser.cmd");
        fs::write(&shim, "@ECHO off\r\necho missing-native\r\n").unwrap();
        let mut search = isolated(&root);
        search.path_dirs = Some(vec![path_dir]);
        let err = search.resolve().unwrap_err();
        assert!(
            err.to_string().starts_with("BrowserBinaryNotExecutable:"),
            "{err}"
        );
        assert!(err
            .to_string()
            .contains("windows_script_shim_without_native_executable"));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn process_path_does_not_select_cmd_when_native_can_be_unwrapped() {
        let Ok(resolved) = BrowserBinarySearch::from_process().resolve() else {
            return;
        };
        let ext = resolved
            .path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        assert!(
            !ext.eq_ignore_ascii_case("cmd") && !ext.eq_ignore_ascii_case("bat"),
            "resolver must not spawn a Windows script shim: {}",
            resolved.path.display()
        );
    }
}
