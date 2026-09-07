use std::path::{Path, PathBuf};

use native_messaging::{install, remove, verify_installed, Scope};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::constants::{
    allowed_origin, native_host_binary, native_host_name, INSTALL_BROWSERS, INSTALL_DESCRIPTION,
    ENV_HOST_EXE,
};
use crate::error::BridgeError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallOutcome {
    pub host_name: String,
    pub installed: bool,
}

pub fn chrome_allowed_origins() -> Vec<String> {
    vec![allowed_origin()]
}

pub fn validate_host_exe_path(exe_path: &Path) -> Result<(), BridgeError> {
    if !exe_path.is_absolute() {
        return Err(BridgeError::HostPathNotAbsolute {
            path: exe_path.to_path_buf(),
        });
    }
    if exe_path.to_str().is_none() {
        return Err(BridgeError::HostPathNotUtf8 {
            path: exe_path.to_path_buf(),
        });
    }
    Ok(())
}

pub fn install_host(exe_path: &Path, host_name: &str) -> Result<InstallOutcome, BridgeError> {
    validate_host_exe_path(exe_path)?;
    install(
        host_name,
        INSTALL_DESCRIPTION,
        exe_path,
        &chrome_allowed_origins(),
        &[],
        INSTALL_BROWSERS,
        Scope::User,
    )
    .map_err(|err| BridgeError::Install(err.to_string()))?;
    Ok(InstallOutcome {
        host_name: host_name.to_string(),
        installed: true,
    })
}

pub fn verify_host(host_name: &str) -> Result<bool, BridgeError> {
    verify_installed(host_name, Some(INSTALL_BROWSERS), Scope::User)
        .map_err(|err| BridgeError::Install(err.to_string()))
}

pub fn remove_host(host_name: &str) -> Result<InstallOutcome, BridgeError> {
    remove(host_name, INSTALL_BROWSERS, Scope::User)
        .map_err(|err| BridgeError::Install(err.to_string()))?;
    Ok(InstallOutcome {
        host_name: host_name.to_string(),
        installed: false,
    })
}

pub fn install_product_host(exe_path: &Path) -> Result<InstallOutcome, BridgeError> {
    install_host(exe_path, &native_host_name())
}

pub fn verify_product_host() -> Result<bool, BridgeError> {
    verify_host(&native_host_name())
}

pub fn remove_product_host() -> Result<InstallOutcome, BridgeError> {
    remove_host(&native_host_name())
}

/// Resolve the host executable for Desktop installation.
///
/// Packaging plan (B3.5-B, not a production installer):
/// - expected resource path: sidecar next to the OmniNova executable, or
///   `src-tauri/resources/omninova-browser-host.exe`
/// - installed host path: the absolute path written into the Native Messaging manifest
/// - manifest path: `%LOCALAPPDATA%/NativeMessagingHosts/<host>.json` plus HKCU
///   `Software\Google\Chrome\NativeMessagingHosts\<host>`
/// - update behavior: startup/install verification must refresh the manifest if
///   the application directory moves after a Tauri update
pub fn resolve_host_executable() -> Result<PathBuf, BridgeError> {
    if let Ok(explicit) = std::env::var(ENV_HOST_EXE) {
        let path = PathBuf::from(explicit);
        validate_host_exe_path(&path)?;
        if path.is_file() {
            return Ok(path);
        }
        return Err(BridgeError::HostBinaryNotFound);
    }
    let exe_name = if cfg!(windows) {
        format!("{}.exe", native_host_binary())
    } else {
        native_host_binary()
    };
    if let Ok(current) = std::env::current_exe() {
        if let Some(dir) = current.parent() {
            let sibling = dir.join(&exe_name);
            if sibling.is_file() {
                let canonical = sibling.canonicalize().unwrap_or(sibling);
                validate_host_exe_path(&canonical)?;
                return Ok(canonical);
            }
            let resource = dir.join("resources").join(&exe_name);
            if resource.is_file() {
                let canonical = resource.canonicalize().unwrap_or(resource);
                validate_host_exe_path(&canonical)?;
                return Ok(canonical);
            }
        }
    }
    Err(BridgeError::HostBinaryNotFound)
}

pub fn manifest_allows_only_expected_origin(manifest_json: &str) -> Result<(), BridgeError> {
    let value: Value = serde_json::from_str(manifest_json).map_err(BridgeError::from_json)?;
    let origins = value
        .get("allowed_origins")
        .and_then(|v| v.as_array())
        .ok_or_else(|| BridgeError::Install("manifest missing allowed_origins".into()))?;
    if origins.len() != 1 {
        return Err(BridgeError::Install(
            "manifest must pin exactly one allowed origin".into(),
        ));
    }
    let origin = origins[0]
        .as_str()
        .ok_or_else(|| BridgeError::Install("allowed_origins entry is not a string".into()))?;
    if origin != allowed_origin() {
        return Err(BridgeError::OriginRejected);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_host_path_is_rejected() {
        let err = validate_host_exe_path(Path::new("omninova-browser-host.exe")).unwrap_err();
        assert!(matches!(err, BridgeError::HostPathNotAbsolute { .. }));
    }

    #[test]
    fn manifest_origin_pin_rejects_extra_origins() {
        let json = r#"{"allowed_origins":["chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"]}"#;
        assert!(manifest_allows_only_expected_origin(json).is_err());
        let good = format!(
            r#"{{"allowed_origins":["{}"]}}"#,
            allowed_origin()
        );
        assert!(manifest_allows_only_expected_origin(&good).is_ok());
    }
}
