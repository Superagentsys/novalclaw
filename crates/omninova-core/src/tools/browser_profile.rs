//! Managed persistent browser profile path resolution.
//!
//! Backend-neutral: this module maps a [`BrowserProfileRef`] logical identity
//! onto an OmniNova-owned directory under a trusted root. It does not mention
//! agent-browser, Chrome, CDP, or CLI flags.

use crate::tools::browser_types::BrowserProfileRef;
use sha2::{Digest, Sha256};
use std::fmt;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

/// Same hex width as `AgentBrowserBackend` session hashing.
const PROFILE_DIR_HASH_CHARS: usize = 20;
pub(crate) const PROFILE_DIR_PREFIX: &str = "profile-";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserProfileError {
    Invalid { detail: String },
    EscapedRoot,
    Missing,
    Io { detail: String },
}

impl fmt::Display for BrowserProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid { detail } => write!(f, "{detail}"),
            Self::EscapedRoot => write!(
                f,
                "BrowserProfileRejected: resolved profile path escaped the trusted profile root"
            ),
            Self::Missing => write!(
                f,
                "BrowserProfileMissing: managed profile was not found"
            ),
            Self::Io { detail } => write!(
                f,
                "BrowserLaunchFailed: failed to prepare managed browser profile directory ({detail})"
            ),
        }
    }
}

impl std::error::Error for BrowserProfileError {}

/// OmniNova-owned persistent profile directory for one logical id.
pub struct ResolvedBrowserProfile {
    pub id: BrowserProfileRef,
    pub path: PathBuf,
}

impl fmt::Debug for ResolvedBrowserProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedBrowserProfile")
            .field("id", &self.id)
            .field("path", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct BrowserProfileResolver {
    root: PathBuf,
}

impl BrowserProfileResolver {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Production root: `<omninova app root>/browser/profiles`.
    /// Reuses `resolve_config_dir` (`.omninova` / `.omninova-dev` / `OMNINOVA_CONFIG_DIR`).
    pub fn omninova_default() -> Self {
        Self::new(omninova_managed_profile_root())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn resolve(
        &self,
        profile: &BrowserProfileRef,
    ) -> Result<ResolvedBrowserProfile, BrowserProfileError> {
        let derived = derived_profile_directory_name(profile.as_str());
        if !is_safe_derived_directory_name(&derived) {
            return Err(BrowserProfileError::Invalid {
                detail: "BrowserProfileRejected: derived profile directory name is unsafe".into(),
            });
        }

        std::fs::create_dir_all(&self.root).map_err(io_error)?;
        let canonical_root = std::fs::canonicalize(&self.root).map_err(io_error)?;

        let child = canonical_root.join(&derived);
        if child
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            return Err(BrowserProfileError::EscapedRoot);
        }

        if child.exists() {
            reject_unsafe_existing_profile_dir(&child)?;
        } else {
            match std::fs::create_dir(&child) {
                Ok(()) => {}
                Err(err) if err.kind() == ErrorKind::AlreadyExists => {
                    reject_unsafe_existing_profile_dir(&child)?;
                }
                Err(err) => return Err(io_error(err)),
            }
        }

        let canonical_child = std::fs::canonicalize(&child).map_err(io_error)?;
        if !path_is_within(&canonical_root, &canonical_child) {
            return Err(BrowserProfileError::EscapedRoot);
        }
        if !canonical_child.is_absolute() {
            return Err(BrowserProfileError::Invalid {
                detail: "BrowserProfileRejected: resolved profile path must be absolute".into(),
            });
        }

        tracing::debug!(
            target: "browser",
            profile_id = profile.as_str(),
            "resolved managed browser profile"
        );

        Ok(ResolvedBrowserProfile {
            id: profile.clone(),
            path: canonical_child,
        })
    }

    /// Resolve an existing managed profile without creating directories.
    pub fn locate(
        &self,
        profile: &BrowserProfileRef,
    ) -> Result<ResolvedBrowserProfile, BrowserProfileError> {
        let derived = derived_profile_directory_name(profile.as_str());
        if !is_safe_derived_directory_name(&derived) {
            return Err(BrowserProfileError::Invalid {
                detail: "BrowserProfileRejected: derived profile directory name is unsafe".into(),
            });
        }
        if !self.root.exists() {
            return Err(BrowserProfileError::Missing);
        }
        let canonical_root = std::fs::canonicalize(&self.root).map_err(io_error)?;
        let child = canonical_root.join(&derived);
        if child
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            return Err(BrowserProfileError::EscapedRoot);
        }
        if !child.exists() {
            return Err(BrowserProfileError::Missing);
        }
        reject_unsafe_existing_profile_dir(&child)?;
        let canonical_child = std::fs::canonicalize(&child).map_err(io_error)?;
        if !path_is_within(&canonical_root, &canonical_child) {
            return Err(BrowserProfileError::EscapedRoot);
        }
        if paths_refer_to_same_location(&canonical_root, &canonical_child) {
            return Err(BrowserProfileError::Invalid {
                detail: "BrowserProfileRejected: refusing to operate on the managed profile root"
                    .into(),
            });
        }
        Ok(ResolvedBrowserProfile {
            id: profile.clone(),
            path: canonical_child,
        })
    }
}

pub fn omninova_managed_profile_root() -> PathBuf {
    crate::config::loader::resolve_config_dir()
        .join("browser")
        .join("profiles")
}

pub(crate) fn derived_profile_directory_name(logical_id: &str) -> String {
    let digest = Sha256::digest(logical_id.as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    let hex = &hex[..PROFILE_DIR_HASH_CHARS.min(hex.len())];
    format!("{PROFILE_DIR_PREFIX}{hex}")
}

pub(crate) fn path_is_within(root: &Path, candidate: &Path) -> bool {
    let root_comps: Vec<Component<'_>> = root.components().collect();
    let candidate_comps: Vec<Component<'_>> = candidate.components().collect();
    if candidate_comps.len() < root_comps.len() {
        return false;
    }
    root_comps
        .iter()
        .zip(candidate_comps.iter())
        .all(|(a, b)| components_eq(*a, *b))
}

pub(crate) fn paths_refer_to_same_location(a: &Path, b: &Path) -> bool {
    path_is_within(a, b) && path_is_within(b, a)
}

fn components_eq(a: Component<'_>, b: Component<'_>) -> bool {
    #[cfg(windows)]
    {
        match (a, b) {
            (Component::Prefix(ap), Component::Prefix(bp)) => {
                ap.as_os_str().eq_ignore_ascii_case(bp.as_os_str())
            }
            (Component::Normal(an), Component::Normal(bn)) => an.eq_ignore_ascii_case(bn),
            _ => a == b,
        }
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

fn is_safe_derived_directory_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && name != "."
        && name != ".."
        && Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(os) if os == name))
}

pub(crate) fn reject_unsafe_existing_profile_dir(
    path: &Path,
) -> Result<(), BrowserProfileError> {
    let meta = std::fs::symlink_metadata(path).map_err(io_error)?;
    if is_reparse_or_symlink(&meta) {
        return Err(BrowserProfileError::EscapedRoot);
    }
    if !meta.is_dir() {
        return Err(BrowserProfileError::Invalid {
            detail: "BrowserProfileRejected: managed profile path is not a directory".into(),
        });
    }
    Ok(())
}

pub(crate) fn is_reparse_or_symlink(meta: &std::fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        return meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagedProfileConfigError {
    InvalidIdentity,
}

pub fn parse_trusted_managed_profile(
    raw: Option<&str>,
) -> Result<Option<BrowserProfileRef>, ManagedProfileConfigError> {
    let Some(value) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    BrowserProfileRef::new(value).map(Some).map_err(|_| {
        ManagedProfileConfigError::InvalidIdentity
    })
}

fn io_error(err: std::io::Error) -> BrowserProfileError {
    BrowserProfileError::Io {
        detail: err.kind().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::browser_types::BrowserTypeError;

    fn scratch_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("omninova-b33b-{}-{}", label, uuid::Uuid::new_v4()))
    }

    #[test]
    fn default_root_is_under_omninova_app_root() {
        let config_dir = crate::config::loader::resolve_config_dir();
        let root = omninova_managed_profile_root();
        assert!(path_is_within(&config_dir, &root));
        assert_eq!(root, config_dir.join("browser").join("profiles"));
        assert!(
            !root
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains("user data\\default"),
            "must not point at installed Chrome User Data"
        );
    }

    #[test]
    fn derived_name_is_deterministic_and_not_the_logical_id() {
        let a = derived_profile_directory_name("work");
        let b = derived_profile_directory_name("work");
        let c = derived_profile_directory_name("personal-test");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.starts_with(PROFILE_DIR_PREFIX));
        assert!(!a.contains("work"));
        assert_eq!(a.len(), PROFILE_DIR_PREFIX.len() + PROFILE_DIR_HASH_CHARS);
        let default_dir = derived_profile_directory_name("default");
        assert_ne!(default_dir, "default");
        assert_ne!(default_dir, "Default");
        assert!(is_safe_derived_directory_name(&a));
    }

    #[test]
    fn path_component_prefix_is_not_string_starts_with() {
        let foo = PathBuf::from(if cfg!(windows) { r"C:\foo" } else { "/foo" });
        let foobar = PathBuf::from(if cfg!(windows) {
            r"C:\foobar"
        } else {
            "/foobar"
        });
        assert!(!path_is_within(&foo, &foobar));
        let child = foo.join("profile-ab");
        assert!(path_is_within(&foo, &child));
        assert!(!path_is_within(&child, &foo));
    }

    #[test]
    #[cfg(windows)]
    fn windows_path_compare_is_case_insensitive() {
        let root = PathBuf::from(r"C:\OmniNova\browser\profiles");
        let child = PathBuf::from(r"c:\omninova\browser\profiles\profile-ab");
        assert!(path_is_within(&root, &child));
        assert!(paths_refer_to_same_location(
            &PathBuf::from(r"C:\OmniNova"),
            &PathBuf::from(r"c:\omninova")
        ));
    }

    #[test]
    fn resolve_creates_child_inside_canonical_root() {
        let root = scratch_root("resolve");
        let resolver = BrowserProfileResolver::new(root.clone());
        let id = BrowserProfileRef::new("integration-profile").unwrap();
        let resolved = resolver.resolve(&id).unwrap();
        let canonical_root = std::fs::canonicalize(&root).unwrap();
        assert!(resolved.path.is_absolute());
        assert!(path_is_within(&canonical_root, &resolved.path));
        assert!(resolved.path.starts_with(&canonical_root) || cfg!(windows));
        assert!(resolved
            .path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(PROFILE_DIR_PREFIX));
        assert_ne!(
            resolved.path.file_name().unwrap().to_string_lossy(),
            "integration-profile"
        );
        let again = resolver.resolve(&id).unwrap();
        assert_eq!(resolved.path, again.path);
        let debug = format!("{resolved:?}");
        assert!(debug.contains("integration-profile"));
        assert!(!debug.contains(resolved.path.to_string_lossy().as_ref()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn default_logical_id_still_resolves_to_omninova_path() {
        let root = scratch_root("default-id");
        let resolver = BrowserProfileResolver::new(root.clone());
        let id = BrowserProfileRef::new("default").unwrap();
        let resolved = resolver.resolve(&id).unwrap();
        assert!(resolved.path.is_absolute());
        assert!(resolved
            .path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(PROFILE_DIR_PREFIX));
        assert_ne!(resolved.path.file_name().unwrap(), "default");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn symlink_child_is_rejected() {
        let root = scratch_root("symlink");
        std::fs::create_dir_all(&root).unwrap();
        let outside = scratch_root("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let id = BrowserProfileRef::new("account-a").unwrap();
        let child = std::fs::canonicalize(&root)
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
            let _ = std::fs::remove_dir_all(&root);
            let _ = std::fs::remove_dir_all(&outside);
            return;
        }
        let resolver = BrowserProfileResolver::new(root.clone());
        let err = resolver.resolve(&id).unwrap_err();
        assert_eq!(err, BrowserProfileError::EscapedRoot);
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn profile_ref_rejects_paths_and_separators() {
        for bad in [
            "",
            " ",
            ".",
            "..",
            "Work",
            "Default",
            "C:",
            r"C:\",
            "/",
            r"\",
            r"\\server",
            "../x",
            r"..\x",
            "a/b",
            r"a\b",
            "has space",
            "id.with.dot",
            "\n",
            "\t",
            "\u{0000}",
        ] {
            assert!(
                BrowserProfileRef::new(bad).is_err(),
                "should reject {bad:?}"
            );
        }
        assert!(matches!(
            BrowserProfileRef::new(""),
            Err(BrowserTypeError::EmptyIdentity)
        ));
        assert!(BrowserProfileRef::new("work").is_ok());
        assert!(BrowserProfileRef::new("personal-test").is_ok());
        assert!(BrowserProfileRef::new("account_a").is_ok());
        assert!(BrowserProfileRef::new("default").is_ok());
        assert!(BrowserProfileRef::new("a").is_ok());
        let long = "a".repeat(64);
        assert!(BrowserProfileRef::new(&long).is_ok());
        let too_long = "a".repeat(65);
        assert!(BrowserProfileRef::new(too_long).is_err());
    }

    #[test]
    fn locate_does_not_create_missing_profile() {
        let root = scratch_root("locate-missing");
        std::fs::create_dir_all(&root).unwrap();
        let resolver = BrowserProfileResolver::new(root.clone());
        let id = BrowserProfileRef::new("absent-profile").unwrap();
        assert_eq!(resolver.locate(&id).unwrap_err(), BrowserProfileError::Missing);
        let derived = root.join(derived_profile_directory_name(id.as_str()));
        assert!(!derived.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn trusted_config_rejects_path_like_managed_profile() {
        assert!(parse_trusted_managed_profile(None).unwrap().is_none());
        assert!(parse_trusted_managed_profile(Some("")).unwrap().is_none());
        assert_eq!(
            parse_trusted_managed_profile(Some("work"))
                .unwrap()
                .unwrap()
                .as_str(),
            "work"
        );
        assert_eq!(
            parse_trusted_managed_profile(Some(r"C:\Users\Hero\.omninova\browser\profiles")),
            Err(ManagedProfileConfigError::InvalidIdentity)
        );
        assert_eq!(
            parse_trusted_managed_profile(Some("Default")),
            Err(ManagedProfileConfigError::InvalidIdentity)
        );
    }
}
