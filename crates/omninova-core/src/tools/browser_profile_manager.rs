//! Minimal ownership-aware manager for OmniNova managed persistent profiles.
//!
//! This is not a generic profile platform. Installed Chrome profiles stay
//! dynamically discovered and read-only (B3.3D). agent-browser remains the
//! execution substrate for persistent `--profile <path>` launch and named
//! installed-profile snapshots.

use crate::tools::browser_installed_profile::{
    chrome_user_data_dirs, parse_trusted_installed_profile, InstalledProfileConfigError,
};
use crate::tools::browser_profile::{
    derived_profile_directory_name, is_reparse_or_symlink, parse_trusted_managed_profile,
    path_is_within, paths_refer_to_same_location, reject_unsafe_existing_profile_dir,
    BrowserProfileError, BrowserProfileResolver, ManagedProfileConfigError, PROFILE_DIR_PREFIX,
    ResolvedBrowserProfile,
};
use crate::tools::browser_types::{BrowserProfileRef, InstalledBrowserProfileRef};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const PROFILE_MARKER_NAME: &str = "profile.marker";
pub const PROFILE_MARKER_KIND: &str = "managed";
pub const TRASH_MARKER_NAME: &str = "trash.marker";
pub const TRASH_MARKER_KIND: &str = "managed-trash";
pub const TRASH_DIR_PREFIX: &str = ".trash-";
const TRASH_GRACE: Duration = Duration::from_secs(15 * 60);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedProfileMarker {
    pub kind: String,
    pub id: String,
    pub created_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManagedBrowserProfileState {
    Available,
    Active,
    Busy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedBrowserProfile {
    pub id: BrowserProfileRef,
    pub created_at: String,
    pub state: ManagedBrowserProfileState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserProfileManagerError {
    Invalid { detail: String },
    EscapedRoot,
    Missing,
    Unowned,
    CorruptMarker,
    MarkerMismatch,
    Active,
    Busy,
    Io { detail: String },
}

impl std::fmt::Display for BrowserProfileManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid { detail } => write!(f, "{detail}"),
            Self::EscapedRoot => write!(
                f,
                "BrowserProfileRejected: resolved profile path escaped the trusted profile root"
            ),
            Self::Missing => write!(f, "BrowserProfileMissing: managed profile was not found"),
            Self::Unowned => write!(
                f,
                "BrowserProfileUnowned: managed profile has no valid ownership marker"
            ),
            Self::CorruptMarker => write!(
                f,
                "BrowserProfileCorruptMarker: managed profile ownership marker is malformed"
            ),
            Self::MarkerMismatch => write!(
                f,
                "BrowserProfileMarkerMismatch: ownership marker id does not match the requested profile"
            ),
            Self::Active => write!(
                f,
                "BrowserProfileActive: profile is in use by a live browser session"
            ),
            Self::Busy => write!(
                f,
                "BrowserProfileBusy: profile is already in use; close the browser session holding this profile before retrying"
            ),
            Self::Io { detail } => write!(
                f,
                "BrowserProfileCleanupFailed: failed to update managed profile ({detail})"
            ),
        }
    }
}

impl std::error::Error for BrowserProfileManagerError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserProfileConfigError {
    InvalidManaged,
    InvalidInstalled,
    ConflictingModes,
}

pub fn parse_browser_session_profile_config(
    profile: Option<&str>,
    installed_profile: Option<&str>,
) -> Result<(Option<BrowserProfileRef>, Option<InstalledBrowserProfileRef>), BrowserProfileConfigError>
{
    let profile = parse_trusted_managed_profile(profile).map_err(|err| match err {
        ManagedProfileConfigError::InvalidIdentity => BrowserProfileConfigError::InvalidManaged,
    })?;
    let installed = parse_trusted_installed_profile(installed_profile).map_err(|err| match err {
        InstalledProfileConfigError::InvalidIdentity => {
            BrowserProfileConfigError::InvalidInstalled
        }
    })?;
    if profile.is_some() && installed.is_some() {
        return Err(BrowserProfileConfigError::ConflictingModes);
    }
    Ok((profile, installed))
}

#[derive(Clone, Debug)]
pub struct BrowserProfileManager {
    resolver: BrowserProfileResolver,
}

impl BrowserProfileManager {
    pub fn new(resolver: BrowserProfileResolver) -> Self {
        Self { resolver }
    }

    pub fn omninova_default() -> Self {
        Self::new(BrowserProfileResolver::omninova_default())
    }

    pub fn resolver(&self) -> &BrowserProfileResolver {
        &self.resolver
    }

    pub fn ensure_managed_profile(
        &self,
        id: &BrowserProfileRef,
    ) -> Result<ManagedBrowserProfile, BrowserProfileManagerError> {
        let resolved = self.resolver.resolve(id).map_err(map_profile_error)?;
        self.ensure_marker(&resolved)?;
        self.descriptor(&resolved, ManagedBrowserProfileState::Available)
    }

    pub fn claim_if_unmarked(
        &self,
        resolved: &ResolvedBrowserProfile,
    ) -> Result<bool, BrowserProfileManagerError> {
        self.assert_managed_target(&resolved.path)?;
        match read_profile_marker(&resolved.path) {
            Ok(None) => {
                write_profile_marker(&resolved.path, resolved.id.as_str())?;
                Ok(true)
            }
            Ok(Some(marker)) if marker.id == resolved.id.as_str() => Ok(false),
            Ok(Some(_)) | Err(BrowserProfileManagerError::CorruptMarker) => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub fn list_managed_profiles(
        &self,
        occupancy: impl Fn(&BrowserProfileRef) -> ManagedBrowserProfileState,
    ) -> Vec<ManagedBrowserProfile> {
        let Ok(canonical_root) = fs::canonicalize(self.resolver.root()) else {
            return Vec::new();
        };
        let Ok(entries) = fs::read_dir(&canonical_root) else {
            return Vec::new();
        };
        let mut listed = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name.starts_with(TRASH_DIR_PREFIX) || !name.starts_with(PROFILE_DIR_PREFIX) {
                continue;
            }
            if !path.is_dir() {
                continue;
            }
            if reject_unsafe_existing_profile_dir(&path).is_err() {
                continue;
            }
            let Ok(canonical) = fs::canonicalize(&path) else {
                continue;
            };
            if !path_is_within(&canonical_root, &canonical) {
                continue;
            }
            let Ok(Some(marker)) = read_profile_marker(&canonical) else {
                continue;
            };
            let Ok(id) = BrowserProfileRef::new(&marker.id) else {
                continue;
            };
            if derived_profile_directory_name(id.as_str()) != name {
                continue;
            }
            listed.push(ManagedBrowserProfile {
                id: id.clone(),
                created_at: marker.created_at,
                state: occupancy(&id),
            });
        }
        listed.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        listed
    }

    pub fn get_managed_profile(
        &self,
        id: &BrowserProfileRef,
        occupancy: impl Fn(&BrowserProfileRef) -> ManagedBrowserProfileState,
    ) -> Result<ManagedBrowserProfile, BrowserProfileManagerError> {
        let resolved = self.resolver.locate(id).map_err(map_profile_error)?;
        let marker = require_matching_marker(&resolved)?;
        Ok(ManagedBrowserProfile {
            id: resolved.id,
            created_at: marker.created_at,
            state: occupancy(id),
        })
    }

    pub fn delete_managed_profile(
        &self,
        id: &BrowserProfileRef,
        occupancy: impl Fn(&BrowserProfileRef) -> ManagedBrowserProfileState,
    ) -> Result<PathBuf, BrowserProfileManagerError> {
        let resolved = self.resolver.locate(id).map_err(map_profile_error)?;
        self.assert_managed_target(&resolved.path)?;
        let _ = require_matching_marker(&resolved)?;
        match occupancy(id) {
            ManagedBrowserProfileState::Available => {}
            ManagedBrowserProfileState::Active => {
                return Err(BrowserProfileManagerError::Active);
            }
            ManagedBrowserProfileState::Busy => return Err(BrowserProfileManagerError::Busy),
        }
        let canonical_root = fs::canonicalize(self.resolver.root()).map_err(io_err)?;
        if paths_refer_to_same_location(&canonical_root, &resolved.path) {
            return Err(BrowserProfileManagerError::Invalid {
                detail: "BrowserProfileRejected: refusing to operate on the managed profile root"
                    .into(),
            });
        }
        let derived = derived_profile_directory_name(id.as_str());
        let trash = canonical_root.join(format!(
            "{TRASH_DIR_PREFIX}{derived}-{}",
            uuid::Uuid::new_v4()
        ));
        if !path_is_within(&canonical_root, &trash) {
            return Err(BrowserProfileManagerError::EscapedRoot);
        }
        rename_within_root(&resolved.path, &trash)?;
        let _ = write_trash_marker(&trash, id.as_str());
        Ok(trash)
    }

    pub fn cleanup_profile_trash(&self) -> usize {
        self.cleanup_profile_trash_after(TRASH_GRACE)
    }

    pub fn cleanup_profile_trash_after(&self, grace: Duration) -> usize {
        let Ok(canonical_root) = fs::canonicalize(self.resolver.root()) else {
            return 0;
        };
        let Ok(entries) = fs::read_dir(&canonical_root) else {
            return 0;
        };
        let mut removed = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if !is_safe_trash_target(&canonical_root, &path) {
                continue;
            }
            if !trash_is_owned(&path) {
                continue;
            }
            if trash_within_grace(&path, grace) {
                continue;
            }
            match fs::remove_dir_all(&path) {
                Ok(()) => removed += 1,
                Err(err) if err.kind() == ErrorKind::NotFound => removed += 1,
                Err(_) => {}
            }
        }
        removed
    }

    fn ensure_marker(
        &self,
        resolved: &ResolvedBrowserProfile,
    ) -> Result<(), BrowserProfileManagerError> {
        self.assert_managed_target(&resolved.path)?;
        match read_profile_marker(&resolved.path) {
            Ok(None) => write_profile_marker(&resolved.path, resolved.id.as_str()),
            Ok(Some(marker)) if marker.id == resolved.id.as_str() => Ok(()),
            Ok(Some(_)) => Err(BrowserProfileManagerError::MarkerMismatch),
            Err(err) => Err(err),
        }
    }

    fn descriptor(
        &self,
        resolved: &ResolvedBrowserProfile,
        state: ManagedBrowserProfileState,
    ) -> Result<ManagedBrowserProfile, BrowserProfileManagerError> {
        let marker = require_matching_marker(resolved)?;
        Ok(ManagedBrowserProfile {
            id: resolved.id.clone(),
            created_at: marker.created_at,
            state,
        })
    }

    fn assert_managed_target(&self, path: &Path) -> Result<(), BrowserProfileManagerError> {
        let canonical_root = fs::canonicalize(self.resolver.root()).map_err(io_err)?;
        let canonical = fs::canonicalize(path).map_err(io_err)?;
        if !path_is_within(&canonical_root, &canonical) {
            return Err(BrowserProfileManagerError::EscapedRoot);
        }
        if paths_refer_to_same_location(&canonical_root, &canonical) {
            return Err(BrowserProfileManagerError::Invalid {
                detail: "BrowserProfileRejected: refusing to operate on the managed profile root"
                    .into(),
            });
        }
        reject_unsafe_existing_profile_dir(&canonical).map_err(map_profile_error)?;
        for chrome_root in chrome_user_data_dirs() {
            if paths_refer_to_same_location(&canonical, &chrome_root)
                || path_is_within(&chrome_root, &canonical)
            {
                return Err(BrowserProfileManagerError::EscapedRoot);
            }
        }
        Ok(())
    }
}

fn require_matching_marker(
    resolved: &ResolvedBrowserProfile,
) -> Result<ManagedProfileMarker, BrowserProfileManagerError> {
    match read_profile_marker(&resolved.path)? {
        None => Err(BrowserProfileManagerError::Unowned),
        Some(marker) if marker.id == resolved.id.as_str() => Ok(marker),
        Some(_) => Err(BrowserProfileManagerError::MarkerMismatch),
    }
}

fn read_profile_marker(profile_dir: &Path) -> Result<Option<ManagedProfileMarker>, BrowserProfileManagerError> {
    let path = profile_dir.join(PROFILE_MARKER_NAME);
    match fs::read_to_string(&path) {
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(io_err(err)),
        Ok(text) => parse_profile_marker(&text).map(Some),
    }
}

fn parse_profile_marker(text: &str) -> Result<ManagedProfileMarker, BrowserProfileManagerError> {
    let kind = marker_field(text, "kind").ok_or(BrowserProfileManagerError::CorruptMarker)?;
    let id = marker_field(text, "id").ok_or(BrowserProfileManagerError::CorruptMarker)?;
    let created_at =
        marker_field(text, "created_at").ok_or(BrowserProfileManagerError::CorruptMarker)?;
    if kind != PROFILE_MARKER_KIND {
        return Err(BrowserProfileManagerError::CorruptMarker);
    }
    if BrowserProfileRef::new(&id).is_err() {
        return Err(BrowserProfileManagerError::CorruptMarker);
    }
    if text.to_ascii_lowercase().contains("cookie=")
        || text.contains('\\')
        || text.contains("User Data")
    {
        return Err(BrowserProfileManagerError::CorruptMarker);
    }
    Ok(ManagedProfileMarker {
        kind,
        id,
        created_at,
    })
}

fn marker_field(text: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    text.lines().find_map(|line| {
        line.strip_prefix(&prefix)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn write_profile_marker(profile_dir: &Path, id: &str) -> Result<(), BrowserProfileManagerError> {
    let created_at = unix_now();
    let body = format!("kind={PROFILE_MARKER_KIND}\nid={id}\ncreated_at={created_at}\n");
    atomic_write(&profile_dir.join(PROFILE_MARKER_NAME), body.as_bytes())
}

fn write_trash_marker(trash_dir: &Path, id: &str) -> Result<(), BrowserProfileManagerError> {
    let deleted_at = unix_now();
    let body = format!("kind={TRASH_MARKER_KIND}\nid={id}\ndeleted_at={deleted_at}\n");
    atomic_write(&trash_dir.join(TRASH_MARKER_NAME), body.as_bytes())
}

fn rename_within_root(live: &Path, trash: &Path) -> Result<(), BrowserProfileManagerError> {
    let mut last = None;
    for attempt in 0..20 {
        match fs::rename(live, trash) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last = Some(err);
                if attempt + 1 < 20 {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }
    Err(io_err(last.expect("rename retried")))
}

fn atomic_write(path: &Path, body: &[u8]) -> Result<(), BrowserProfileManagerError> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, body).map_err(io_err)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = fs::remove_file(&tmp);
            Err(io_err(err))
        }
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn is_safe_trash_target(root: &Path, path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if !name.starts_with(TRASH_DIR_PREFIX) || name.contains("..") {
        return false;
    }
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if is_reparse_or_symlink(&meta) || !meta.is_dir() {
        return false;
    }
    let Ok(canonical) = fs::canonicalize(path) else {
        return false;
    };
    path_is_within(root, &canonical) && !paths_refer_to_same_location(root, &canonical)
}

fn trash_is_owned(path: &Path) -> bool {
    matches!(read_profile_marker(path), Ok(Some(_)))
}

fn trash_within_grace(path: &Path, grace: Duration) -> bool {
    let deleted_at = fs::read_to_string(path.join(TRASH_MARKER_NAME))
        .ok()
        .and_then(|text| marker_field(&text, "deleted_at"))
        .and_then(|value| value.parse::<u64>().ok())
        .and_then(|secs| UNIX_EPOCH.checked_add(Duration::from_secs(secs)));
    let stamp = deleted_at.or_else(|| {
        fs::metadata(path)
            .ok()
            .and_then(|meta| meta.modified().or_else(|_| meta.created()).ok())
    });
    let Some(stamp) = stamp else {
        return false;
    };
    match SystemTime::now().duration_since(stamp) {
        Ok(age) => age < grace,
        Err(_) => true,
    }
}

fn map_profile_error(err: BrowserProfileError) -> BrowserProfileManagerError {
    match err {
        BrowserProfileError::Invalid { detail } => BrowserProfileManagerError::Invalid { detail },
        BrowserProfileError::EscapedRoot => BrowserProfileManagerError::EscapedRoot,
        BrowserProfileError::Missing => BrowserProfileManagerError::Missing,
        BrowserProfileError::Io { detail } => BrowserProfileManagerError::Io { detail },
    }
}

fn io_err(err: std::io::Error) -> BrowserProfileManagerError {
    BrowserProfileManagerError::Io {
        detail: err.kind().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("omninova-b33e-{label}-{}", uuid::Uuid::new_v4()))
    }

    fn manager() -> (BrowserProfileManager, PathBuf) {
        let root = scratch("root");
        (
            BrowserProfileManager::new(BrowserProfileResolver::new(root.clone())),
            root,
        )
    }

    fn available(_: &BrowserProfileRef) -> ManagedBrowserProfileState {
        ManagedBrowserProfileState::Available
    }

    #[test]
    fn ensure_creates_directory_and_marker() {
        let (mgr, root) = manager();
        let id = BrowserProfileRef::new("b33e-test").unwrap();
        let created = mgr.ensure_managed_profile(&id).unwrap();
        assert_eq!(created.id.as_str(), "b33e-test");
        assert_eq!(created.state, ManagedBrowserProfileState::Available);
        let resolved = mgr.resolver().locate(&id).unwrap();
        let marker = read_profile_marker(&resolved.path).unwrap().unwrap();
        assert_eq!(marker.kind, PROFILE_MARKER_KIND);
        assert_eq!(marker.id, "b33e-test");
        assert!(!marker.created_at.is_empty());
        let text = fs::read_to_string(resolved.path.join(PROFILE_MARKER_NAME)).unwrap();
        assert!(!text.contains('\\'));
        assert!(!text.to_ascii_lowercase().contains("cookie"));
        assert!(!text.contains("User Data"));
        let listed = mgr.list_managed_profiles(available);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id.as_str(), "b33e-test");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn marker_roundtrip_and_idempotent_ensure() {
        let (mgr, root) = manager();
        let id = BrowserProfileRef::new("work").unwrap();
        let first = mgr.ensure_managed_profile(&id).unwrap();
        let second = mgr.ensure_managed_profile(&id).unwrap();
        assert_eq!(first.created_at, second.created_at);
        let resolved = mgr.resolver().locate(&id).unwrap();
        assert!(resolved.path.join("Cookies").exists() == false);
        fs::write(resolved.path.join("Cookies"), b"synthetic-state").unwrap();
        let third = mgr.ensure_managed_profile(&id).unwrap();
        assert_eq!(
            fs::read(resolved.path.join("Cookies")).unwrap(),
            b"synthetic-state"
        );
        assert_eq!(third.created_at, first.created_at);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn malformed_and_mismatched_markers_are_rejected() {
        let (mgr, root) = manager();
        let id = BrowserProfileRef::new("account-a").unwrap();
        let resolved = mgr.resolver().resolve(&id).unwrap();
        fs::write(resolved.path.join(PROFILE_MARKER_NAME), "kind=managed\n").unwrap();
        assert_eq!(
            mgr.get_managed_profile(&id, available).unwrap_err(),
            BrowserProfileManagerError::CorruptMarker
        );
        fs::write(
            resolved.path.join(PROFILE_MARKER_NAME),
            "kind=managed\nid=account-b\ncreated_at=1\n",
        )
        .unwrap();
        assert_eq!(
            mgr.delete_managed_profile(&id, available).unwrap_err(),
            BrowserProfileManagerError::MarkerMismatch
        );
        assert!(resolved.path.exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn legacy_unmarked_profile_can_be_claimed_but_not_deleted() {
        let (mgr, root) = manager();
        let id = BrowserProfileRef::new("legacy").unwrap();
        let resolved = mgr.resolver().resolve(&id).unwrap();
        fs::write(resolved.path.join("Cookies"), b"keep-me").unwrap();
        assert_eq!(
            mgr.delete_managed_profile(&id, available).unwrap_err(),
            BrowserProfileManagerError::Unowned
        );
        assert!(mgr.list_managed_profiles(available).is_empty());
        assert!(mgr.claim_if_unmarked(&resolved).unwrap());
        assert_eq!(
            fs::read(resolved.path.join("Cookies")).unwrap(),
            b"keep-me"
        );
        let got = mgr.get_managed_profile(&id, available).unwrap();
        assert_eq!(got.id.as_str(), "legacy");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn arbitrary_directory_is_not_claimed_or_listed() {
        let (mgr, root) = manager();
        fs::create_dir_all(&root).unwrap();
        let stray = root.join("not-a-profile");
        fs::create_dir_all(&stray).unwrap();
        fs::write(
            stray.join(PROFILE_MARKER_NAME),
            "kind=managed\nid=work\ncreated_at=1\n",
        )
        .unwrap();
        assert!(mgr.list_managed_profiles(available).is_empty());
        let work = BrowserProfileRef::new("work").unwrap();
        assert_eq!(
            mgr.get_managed_profile(&work, available).unwrap_err(),
            BrowserProfileManagerError::Missing
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn config_parse_is_fail_closed() {
        assert!(parse_browser_session_profile_config(None, None)
            .unwrap()
            .0
            .is_none());
        assert_eq!(
            parse_browser_session_profile_config(Some("work"), None)
                .unwrap()
                .0
                .unwrap()
                .as_str(),
            "work"
        );
        assert_eq!(
            parse_browser_session_profile_config(Some("Default"), None),
            Err(BrowserProfileConfigError::InvalidManaged)
        );
        assert_eq!(
            parse_browser_session_profile_config(
                Some(r"C:\Users\Hero\AppData\Local\Google\Chrome\User Data\Default"),
                None
            ),
            Err(BrowserProfileConfigError::InvalidManaged)
        );
        assert_eq!(
            parse_browser_session_profile_config(Some("work"), Some("Default")),
            Err(BrowserProfileConfigError::ConflictingModes)
        );
        let config: crate::config::BrowserConfig = toml::from_str(
            r#"enabled = true
backend = "agent-browser"
profile = "work"
"#,
        )
        .unwrap();
        assert_eq!(config.profile.as_deref(), Some("work"));
    }

    #[test]
    fn list_excludes_malformed_and_installed_identities() {
        let (mgr, root) = manager();
        let work = mgr
            .ensure_managed_profile(&BrowserProfileRef::new("work").unwrap())
            .unwrap();
        fs::create_dir_all(root.join("Default")).unwrap();
        fs::write(
            root.join("Default").join(PROFILE_MARKER_NAME),
            "kind=managed\nid=default\ncreated_at=1\n",
        )
        .unwrap();
        let listed = mgr.list_managed_profiles(|_| ManagedBrowserProfileState::Busy);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id.as_str(), work.id.as_str());
        assert_eq!(listed[0].state, ManagedBrowserProfileState::Busy);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn delete_moves_inactive_owned_profile_to_trash() {
        let (mgr, root) = manager();
        let id = BrowserProfileRef::new("closable").unwrap();
        mgr.ensure_managed_profile(&id).unwrap();
        let live = mgr.resolver().locate(&id).unwrap().path;
        let trash = mgr.delete_managed_profile(&id, available).unwrap();
        assert!(!live.exists());
        assert!(trash.exists());
        assert!(
            trash
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(TRASH_DIR_PREFIX)
        );
        assert!(read_profile_marker(&trash).unwrap().is_some());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn delete_rejects_active_and_busy() {
        let (mgr, root) = manager();
        let id = BrowserProfileRef::new("held").unwrap();
        mgr.ensure_managed_profile(&id).unwrap();
        assert_eq!(
            mgr.delete_managed_profile(&id, |_| ManagedBrowserProfileState::Active)
                .unwrap_err(),
            BrowserProfileManagerError::Active
        );
        assert_eq!(
            mgr.delete_managed_profile(&id, |_| ManagedBrowserProfileState::Busy)
                .unwrap_err(),
            BrowserProfileManagerError::Busy
        );
        assert!(mgr.resolver().locate(&id).unwrap().path.exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn delete_cannot_target_root() {
        let (mgr, root) = manager();
        fs::create_dir_all(&root).unwrap();
        assert!(mgr.assert_managed_target(mgr.resolver().root()).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn trash_gc_removes_stale_owned_and_preserves_recent_and_unowned() {
        let (mgr, root) = manager();
        let id = BrowserProfileRef::new("gc-me").unwrap();
        mgr.ensure_managed_profile(&id).unwrap();
        let trash = mgr.delete_managed_profile(&id, available).unwrap();
        let unowned = root.join(format!("{TRASH_DIR_PREFIX}unrelated"));
        fs::create_dir_all(&unowned).unwrap();
        let recent_kept = mgr.cleanup_profile_trash();
        assert_eq!(recent_kept, 0);
        assert!(trash.exists());
        assert!(unowned.exists());
        let removed = mgr.cleanup_profile_trash_after(Duration::from_secs(0));
        assert!(removed >= 1);
        assert!(!trash.exists());
        assert!(unowned.exists());
        let removed_again = mgr.cleanup_profile_trash_after(Duration::from_secs(0));
        assert_eq!(removed_again, 0);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn symlink_profile_is_rejected() {
        let (mgr, root) = manager();
        fs::create_dir_all(&root).unwrap();
        let outside = scratch("outside");
        fs::create_dir_all(&outside).unwrap();
        let id = BrowserProfileRef::new("account-a").unwrap();
        let child = fs::canonicalize(&root)
            .unwrap()
            .join(derived_profile_directory_name(id.as_str()));
        let linked = {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&outside, &child).is_ok()
            }
            #[cfg(windows)]
            {
                std::os::windows::fs::symlink_dir(&outside, &child).is_ok()
            }
            #[cfg(not(any(unix, windows)))]
            {
                false
            }
        };
        if !linked {
            let _ = fs::remove_dir_all(&root);
            let _ = fs::remove_dir_all(&outside);
            return;
        }
        assert!(mgr.get_managed_profile(&id, available).is_err());
        assert!(mgr.delete_managed_profile(&id, available).is_err());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
    }
}
