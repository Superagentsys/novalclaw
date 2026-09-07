//! Installed Chrome profile snapshot helpers.
//!
//! Trusted directory identities (`Default`, `Profile 1`) are resolved to
//! installed Chrome user-data metadata. The source profile is never launched
//! against and never deleted. agent-browser named-profile copy is the
//! execution substrate; OmniNova only validates and owns the temporary
//! snapshot.
//!
//! Chrome App-Bound Encryption can make copied login state unusable when the
//! selected executable does not match the installation that created the
//! source profile. This module does not decrypt credentials or bypass that
//! protection; it only checks snapshot structure (presence/size). Login reuse
//! across Chrome/CfT identities is therefore not guaranteed.

use crate::tools::browser_types::InstalledBrowserProfileRef;
use serde_json::Value;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const SNAPSHOT_DIR_PREFIX: &str = "agent-browser-profile-";
pub const SNAPSHOT_MARKER_NAME: &str = ".omninova-owned-snapshot";
pub const SNAPSHOT_KIND: &str = "installed-snapshot";

const EMPTY_COOKIE_REPLACEMENT_BYTES: u64 = 4096;
const MATERIAL_SOURCE_COOKIE_BYTES: u64 = 8192;
const UNMARKED_SNAPSHOT_GRACE: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredInstalledProfile {
    pub directory: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstalledProfileResolveError {
    NotFound,
    Ambiguous,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotIntegrityError {
    MissingCookies,
    EmptyCookiesReplacement,
    MissingProfileDirectory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookiesArtifactStatus {
    Missing,
    Unreadable,
    Present { len: u64 },
}

impl SnapshotIntegrityError {
    pub fn class_name(&self) -> &'static str {
        match self {
            Self::MissingCookies => "missing_cookies",
            Self::EmptyCookiesReplacement => "empty_cookies_replacement",
            Self::MissingProfileDirectory => "missing_profile_directory",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct BrowserInstalledProfileResolver {
    user_data_dir: Option<PathBuf>,
}

impl BrowserInstalledProfileResolver {
    pub fn discover() -> Self {
        Self { user_data_dir: None }
    }

    pub fn with_user_data_dir(user_data_dir: PathBuf) -> Self {
        Self {
            user_data_dir: Some(user_data_dir),
        }
    }

    pub fn user_data_dir(&self) -> Option<PathBuf> {
        self.user_data_dir
            .clone()
            .or_else(find_chrome_user_data_dir)
    }

    pub fn list(&self) -> Vec<DiscoveredInstalledProfile> {
        let Some(root) = self.user_data_dir() else {
            return Vec::new();
        };
        list_chrome_profiles(&root)
    }

    pub fn resolve(
        &self,
        requested: &InstalledBrowserProfileRef,
    ) -> Result<String, InstalledProfileResolveError> {
        let profiles = self.list();
        if profiles.is_empty() {
            return Err(InstalledProfileResolveError::Unavailable);
        }
        resolve_installed_profile(&profiles, requested.as_str())
    }

    pub fn source_cookies_status(&self, directory: &str) -> CookiesArtifactStatus {
        let Some(root) = self.user_data_dir() else {
            return CookiesArtifactStatus::Missing;
        };
        cookies_artifact_status(&root.join(directory))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstalledProfileConfigError {
    InvalidIdentity,
}

pub fn parse_trusted_installed_profile(
    raw: Option<&str>,
) -> Result<Option<InstalledBrowserProfileRef>, InstalledProfileConfigError> {
    let Some(value) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    InstalledBrowserProfileRef::new(value)
        .map(Some)
        .map_err(|_| InstalledProfileConfigError::InvalidIdentity)
}

pub fn chrome_user_data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let base = PathBuf::from(local);
            for name in [
                r"Google\Chrome\User Data",
                r"Google\Chrome SxS\User Data",
                r"Chromium\User Data",
                r"BraveSoftware\Brave-Browser\User Data",
            ] {
                dirs.push(base.join(name));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = home::home_dir() {
            let base = home.join("Library/Application Support");
            for name in [
                "Google/Chrome",
                "Google/Chrome Canary",
                "Chromium",
                "BraveSoftware/Brave-Browser",
            ] {
                dirs.push(base.join(name));
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(home) = home::home_dir() {
            let config = home.join(".config");
            for name in [
                "google-chrome",
                "google-chrome-unstable",
                "chromium",
                "BraveSoftware/Brave-Browser",
            ] {
                dirs.push(config.join(name));
            }
        }
    }
    dirs
}

pub fn find_chrome_user_data_dir() -> Option<PathBuf> {
    chrome_user_data_dirs()
        .into_iter()
        .find(|dir| dir.join("Local State").is_file())
}

pub fn list_chrome_profiles(user_data_dir: &Path) -> Vec<DiscoveredInstalledProfile> {
    let content = match fs::read_to_string(user_data_dir.join("Local State")) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };
    let json: Value = match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };
    let Some(info_cache) = json
        .get("profile")
        .and_then(|profile| profile.get("info_cache"))
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };
    let mut profiles: Vec<DiscoveredInstalledProfile> = info_cache
        .iter()
        .map(|(directory, info)| DiscoveredInstalledProfile {
            directory: directory.clone(),
            display_name: info
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(directory)
                .to_string(),
        })
        .collect();
    profiles.sort_by(|a, b| a.directory.cmp(&b.directory));
    profiles
}

pub fn resolve_installed_profile(
    profiles: &[DiscoveredInstalledProfile],
    input: &str,
) -> Result<String, InstalledProfileResolveError> {
    if let Some(profile) = profiles.iter().find(|profile| profile.directory == input) {
        return Ok(profile.directory.clone());
    }
    let input_lower = input.to_ascii_lowercase();
    let display_matches: Vec<&DiscoveredInstalledProfile> = profiles
        .iter()
        .filter(|profile| profile.display_name.to_ascii_lowercase() == input_lower)
        .collect();
    match display_matches.len() {
        1 => return Ok(display_matches[0].directory.clone()),
        n if n > 1 => return Err(InstalledProfileResolveError::Ambiguous),
        _ => {}
    }
    if let Some(profile) = profiles
        .iter()
        .find(|profile| profile.directory.to_ascii_lowercase() == input_lower)
    {
        return Ok(profile.directory.clone());
    }
    Err(InstalledProfileResolveError::NotFound)
}

pub fn omninova_snapshot_root() -> PathBuf {
    std::env::temp_dir().join("omninova-browser-snapshots")
}

pub fn cookies_artifact_path(profile_dir: &Path) -> Option<PathBuf> {
    let network = profile_dir.join("Network").join("Cookies");
    if network.is_file() {
        return Some(network);
    }
    let legacy = profile_dir.join("Cookies");
    if legacy.is_file() {
        return Some(legacy);
    }
    None
}

pub fn cookies_artifact_status(profile_dir: &Path) -> CookiesArtifactStatus {
    let Some(path) = cookies_artifact_path(profile_dir) else {
        return CookiesArtifactStatus::Missing;
    };
    match fs::metadata(path) {
        Ok(meta) => CookiesArtifactStatus::Present { len: meta.len() },
        Err(_) => CookiesArtifactStatus::Unreadable,
    }
}

pub fn validate_installed_snapshot(
    snapshot_user_data: &Path,
    directory: &str,
    source: CookiesArtifactStatus,
) -> Result<(), SnapshotIntegrityError> {
    let profile_dir = snapshot_user_data.join(directory);
    if !profile_dir.is_dir() {
        return Err(SnapshotIntegrityError::MissingProfileDirectory);
    }
    let snapshot = cookies_artifact_status(&profile_dir);
    let snapshot_len = match snapshot {
        CookiesArtifactStatus::Present { len } => len,
        CookiesArtifactStatus::Missing | CookiesArtifactStatus::Unreadable => {
            return Err(SnapshotIntegrityError::MissingCookies);
        }
    };
    let tiny_snapshot = snapshot_len <= EMPTY_COOKIE_REPLACEMENT_BYTES;
    let source_looks_material = match source {
        CookiesArtifactStatus::Present { len } => len >= MATERIAL_SOURCE_COOKIE_BYTES,
        CookiesArtifactStatus::Unreadable => true,
        CookiesArtifactStatus::Missing => false,
    };
    if tiny_snapshot && source_looks_material {
        return Err(SnapshotIntegrityError::EmptyCookiesReplacement);
    }
    Ok(())
}

pub fn write_snapshot_marker(
    snapshot_user_data: &Path,
    session: &str,
    directory: &str,
) -> std::io::Result<()> {
    let body = format!("kind={SNAPSHOT_KIND}\nsession={session}\ndirectory={directory}\n");
    fs::write(snapshot_user_data.join(SNAPSHOT_MARKER_NAME), body)
}

pub fn snapshot_marker_session(snapshot_user_data: &Path) -> Option<String> {
    let text = fs::read_to_string(snapshot_user_data.join(SNAPSHOT_MARKER_NAME)).ok()?;
    text.lines().find_map(|line| {
        line.strip_prefix("session=")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

pub fn is_omninova_snapshot_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name.starts_with(SNAPSHOT_DIR_PREFIX)
        && path.is_dir()
        && path_is_within(&omninova_snapshot_root(), path)
}

pub fn locate_latest_snapshot_dir(
    root: &Path,
    directory: &str,
    not_before: SystemTime,
) -> Option<PathBuf> {
    let entries = fs::read_dir(root).ok()?;
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with(SNAPSHOT_DIR_PREFIX) || !path.is_dir() {
            continue;
        }
        if !path.join(directory).is_dir() {
            continue;
        }
        let created = entry
            .metadata()
            .ok()
            .and_then(|meta| meta.created().or_else(|_| meta.modified()).ok())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if created + Duration::from_secs(2) < not_before {
            continue;
        }
        match &newest {
            Some((time, _)) if created <= *time => {}
            _ => newest = Some((created, path)),
        }
    }
    newest.map(|(_, path)| path)
}

pub fn cleanup_owned_snapshots(
    root: &Path,
    active: &[PathBuf],
    session_alive: impl Fn(&str) -> bool,
) -> usize {
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_safe_snapshot_delete_target(&path) {
            continue;
        }
        if active.iter().any(|live| paths_eq(live, &path)) {
            continue;
        }
        if let Some(session) = snapshot_marker_session(&path) {
            if session_alive(&session) {
                continue;
            }
        } else if snapshot_is_within_unmarked_grace(&path) {
            continue;
        }
        if remove_snapshot_dir(&path) {
            removed += 1;
        }
    }
    removed
}

pub fn remove_snapshot_dir(path: &Path) -> bool {
    if !is_safe_snapshot_delete_target(path) {
        return false;
    }
    match fs::remove_dir_all(path) {
        Ok(()) => true,
        Err(err) if err.kind() == ErrorKind::NotFound => true,
        Err(_) => false,
    }
}

pub fn is_safe_snapshot_delete_target(path: &Path) -> bool {
    if !is_omninova_snapshot_dir(path) {
        return false;
    }
    for chrome_root in chrome_user_data_dirs() {
        if paths_eq(path, &chrome_root) || path_is_within(&chrome_root, path) {
            return false;
        }
    }
    true
}

fn snapshot_is_within_unmarked_grace(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    let stamp = meta.created().or_else(|_| meta.modified()).ok();
    let Some(stamp) = stamp else {
        return false;
    };
    match SystemTime::now().duration_since(stamp) {
        Ok(age) => age < UNMARKED_SNAPSHOT_GRACE,
        Err(_) => true,
    }
}

fn path_is_within(root: &Path, candidate: &Path) -> bool {
    let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let candidate = fs::canonicalize(candidate).unwrap_or_else(|_| candidate.to_path_buf());
    candidate.starts_with(root)
}

fn paths_eq(a: &Path, b: &Path) -> bool {
    a == b
        || a.canonicalize()
            .ok()
            .zip(b.canonicalize().ok())
            .is_some_and(|(left, right)| left == right)
}

pub fn installed_named_profile_argv(directory: &str) -> Vec<std::ffi::OsString> {
    vec!["--profile".into(), directory.into()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::browser_types::{BrowserProfileRef, InstalledBrowserProfileRef};

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "omninova-b33d-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_local_state(root: &Path, entries: &[(&str, &str)]) {
        let cache: serde_json::Map<String, Value> = entries
            .iter()
            .map(|(dir, name)| {
                (
                    (*dir).to_string(),
                    serde_json::json!({ "name": name }),
                )
            })
            .collect();
        let json = serde_json::json!({ "profile": { "info_cache": cache } });
        fs::write(root.join("Local State"), json.to_string()).unwrap();
    }

    #[test]
    fn managed_profile_ref_semantics_unchanged() {
        assert!(BrowserProfileRef::new("default").is_ok());
        assert!(BrowserProfileRef::new("Default").is_err());
        assert!(BrowserProfileRef::new(r"C:\Users\Hero\AppData").is_err());
    }

    #[test]
    fn installed_profile_ref_rejects_path_like_input() {
        assert!(InstalledBrowserProfileRef::new("Default").is_ok());
        assert!(InstalledBrowserProfileRef::new("Profile 1").is_ok());
        assert!(InstalledBrowserProfileRef::new(r"C:\Users\Hero\AppData\Local\Google\Chrome\User Data\Default").is_err());
        assert!(InstalledBrowserProfileRef::new(r"User Data\Default").is_err());
        assert!(InstalledBrowserProfileRef::new("../Default").is_err());
        assert!(InstalledBrowserProfileRef::new("~/Default").is_err());
        assert!(InstalledBrowserProfileRef::new("").is_err());
    }

    #[test]
    fn installed_profile_identity_is_directory_name() {
        let profiles = vec![
            DiscoveredInstalledProfile {
                directory: "Default".into(),
                display_name: "Person 1".into(),
            },
            DiscoveredInstalledProfile {
                directory: "Profile 1".into(),
                display_name: "Work".into(),
            },
        ];
        assert_eq!(
            resolve_installed_profile(&profiles, "Default").unwrap(),
            "Default"
        );
        assert_eq!(
            resolve_installed_profile(&profiles, "Profile 1").unwrap(),
            "Profile 1"
        );
        assert_eq!(
            resolve_installed_profile(&profiles, "Work").unwrap(),
            "Profile 1"
        );
    }

    #[test]
    fn ambiguous_display_name_is_rejected() {
        let profiles = vec![
            DiscoveredInstalledProfile {
                directory: "Default".into(),
                display_name: "Work".into(),
            },
            DiscoveredInstalledProfile {
                directory: "Profile 1".into(),
                display_name: "Work".into(),
            },
        ];
        assert_eq!(
            resolve_installed_profile(&profiles, "Work"),
            Err(InstalledProfileResolveError::Ambiguous)
        );
        assert_eq!(
            resolve_installed_profile(&profiles, "Default").unwrap(),
            "Default"
        );
    }

    #[test]
    fn named_profile_argv_is_not_a_filesystem_path() {
        let args = installed_named_profile_argv("Default");
        assert_eq!(args[0], "--profile");
        assert_eq!(args[1], "Default");
        let value = args[1].to_string_lossy();
        assert!(!value.contains('\\'));
        assert!(!value.contains('/'));
        assert!(!value.contains("User Data"));
    }

    #[test]
    fn valid_snapshot_is_accepted() {
        let root = scratch("valid");
        let profile = root.join("Default").join("Network");
        fs::create_dir_all(&profile).unwrap();
        fs::write(profile.join("Cookies"), vec![0u8; 20_000]).unwrap();
        validate_installed_snapshot(
            &root,
            "Default",
            CookiesArtifactStatus::Present { len: 20_000 },
        )
        .unwrap();
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_cookies_are_rejected() {
        let root = scratch("missing");
        fs::create_dir_all(root.join("Default")).unwrap();
        let err = validate_installed_snapshot(
            &root,
            "Default",
            CookiesArtifactStatus::Present { len: 20_000 },
        )
        .unwrap_err();
        assert_eq!(err, SnapshotIntegrityError::MissingCookies);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_replacement_cookies_are_rejected() {
        let root = scratch("empty");
        let profile = root.join("Default").join("Network");
        fs::create_dir_all(&profile).unwrap();
        fs::write(profile.join("Cookies"), vec![0u8; 512]).unwrap();
        let err = validate_installed_snapshot(
            &root,
            "Default",
            CookiesArtifactStatus::Present { len: 64_000 },
        )
        .unwrap_err();
        assert_eq!(err, SnapshotIntegrityError::EmptyCookiesReplacement);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unreadable_source_with_tiny_snapshot_is_rejected() {
        let root = scratch("unreadable");
        let profile = root.join("Default").join("Network");
        fs::create_dir_all(&profile).unwrap();
        fs::write(profile.join("Cookies"), vec![0u8; 512]).unwrap();
        let err = validate_installed_snapshot(
            &root,
            "Default",
            CookiesArtifactStatus::Unreadable,
        )
        .unwrap_err();
        assert_eq!(err, SnapshotIntegrityError::EmptyCookiesReplacement);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn integrity_gate_does_not_read_cookie_bytes() {
        let root = scratch("no-read");
        let profile = root.join("Default").join("Network");
        fs::create_dir_all(&profile).unwrap();
        fs::write(
            profile.join("Cookies"),
            b"synthetic-cookie-payload-not-a-secret",
        )
        .unwrap();
        let err = validate_installed_snapshot(
            &root,
            "Default",
            CookiesArtifactStatus::Present { len: 64_000 },
        )
        .unwrap_err();
        assert_eq!(err, SnapshotIntegrityError::EmptyCookiesReplacement);
        assert_eq!(err.class_name(), "empty_cookies_replacement");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn trusted_config_rejects_path_like_installed_profile() {
        assert!(parse_trusted_installed_profile(None).unwrap().is_none());
        assert!(parse_trusted_installed_profile(Some("")).unwrap().is_none());
        assert_eq!(
            parse_trusted_installed_profile(Some("Default"))
                .unwrap()
                .unwrap()
                .as_str(),
            "Default"
        );
        assert_eq!(
            parse_trusted_installed_profile(Some(r"C:\Users\Hero\AppData\Local\Google\Chrome\User Data\Default")),
            Err(InstalledProfileConfigError::InvalidIdentity)
        );
    }

    #[test]
    fn trusted_browser_config_accepts_installed_profile_without_tool_schema() {
        let config: crate::config::BrowserConfig = toml::from_str(
            r#"enabled = true
backend = "agent-browser"
installed_profile = "Default"
"#,
        )
        .unwrap();
        assert_eq!(config.installed_profile.as_deref(), Some("Default"));
    }

    #[test]
    fn resolver_reads_directory_identity_from_local_state() {
        let root = scratch("discover");
        write_local_state(&root, &[("Default", "Person 1"), ("Profile 1", "Work")]);
        fs::create_dir_all(root.join("Default")).unwrap();
        let resolver = BrowserInstalledProfileResolver::with_user_data_dir(root.clone());
        let listed = resolver.list();
        assert_eq!(listed[0].directory, "Default");
        assert_eq!(
            resolver
                .resolve(&InstalledBrowserProfileRef::new("Work").unwrap())
                .unwrap(),
            "Profile 1"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cleanup_removes_stale_owned_snapshot_and_is_idempotent() {
        let root = omninova_snapshot_root().join(format!("b33d-clean-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let stale = root.join(format!("{SNAPSHOT_DIR_PREFIX}{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&stale).unwrap();
        write_snapshot_marker(&stale, "omninova-dead-session", "Default").unwrap();
        let active = root.join(format!("{SNAPSHOT_DIR_PREFIX}{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&active).unwrap();
        write_snapshot_marker(&active, "omninova-live-session", "Default").unwrap();
        let removed = cleanup_owned_snapshots(&root, &[active.clone()], |session| {
            session == "omninova-live-session"
        });
        assert!(removed >= 1);
        assert!(!stale.exists());
        assert!(active.exists());
        let removed_again = cleanup_owned_snapshots(&root, &[active.clone()], |session| {
            session == "omninova-live-session"
        });
        assert_eq!(removed_again, 0);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cleanup_refuses_source_and_unrelated_directories() {
        let chrome = scratch("chrome-user-data");
        assert!(!is_safe_snapshot_delete_target(&chrome));
        assert!(!remove_snapshot_dir(&chrome));
        assert!(chrome.exists());
        let _ = fs::remove_dir_all(&chrome);
    }

    #[test]
    fn cleanup_does_not_delete_managed_persistent_profiles() {
        let managed = scratch("managed-persistent");
        fs::create_dir_all(managed.join("profile-ab12")).unwrap();
        assert!(!is_safe_snapshot_delete_target(&managed));
        assert!(!is_safe_snapshot_delete_target(&managed.join("profile-ab12")));
        assert!(managed.join("profile-ab12").exists());
        let _ = fs::remove_dir_all(&managed);
    }

    #[test]
    fn cleanup_preserves_young_unmarked_snapshot_dirs() {
        let root = omninova_snapshot_root().join(format!("b33d-grace-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let young = root.join(format!("{SNAPSHOT_DIR_PREFIX}{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&young).unwrap();
        let removed = cleanup_owned_snapshots(&root, &[], |_| false);
        assert_eq!(removed, 0);
        assert!(young.exists(), "unmarked in-flight snapshots must not be deleted");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn live_discovery_lists_default_when_chrome_user_data_exists() {
        let Some(root) = find_chrome_user_data_dir() else {
            return;
        };
        let listed = list_chrome_profiles(&root);
        assert!(
            listed.iter().any(|profile| profile.directory == "Default"),
            "installed Chrome user data should include a Default directory identity"
        );
        let _ = listed.iter().any(|profile| profile.directory == "Profile 1");
    }
}
