use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::constants::{
    app_data_folder, bridge_subdir, ENDPOINT_FILE_NAME, ENV_BRIDGE_DIR, SECRET_FILE_NAME,
};
use crate::error::BridgeError;

/// IPC shared secret. Never logged, Displayed, or sent to the frontend.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn from_raw(value: String) -> Self {
        Secret(value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn constant_time_eq(&self, other: &str) -> bool {
        constant_time_eq(self.0.as_bytes(), other.as_bytes())
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([redacted])")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointFile {
    pub transport: String,
    pub path: String,
    pub generation: u64,
    pub connection_nonce: String,
}

pub fn default_bridge_dir() -> PathBuf {
    if let Ok(dir) = std::env::var(ENV_BRIDGE_DIR) {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let appdata = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("HOME"))
        .unwrap_or_else(|| ".".into());
    PathBuf::from(appdata)
        .join(app_data_folder())
        .join(bridge_subdir())
}

pub fn generate_secret() -> Result<Secret, BridgeError> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|err| {
        BridgeError::Install(format!("failed to generate IPC secret: {err}"))
    })?;
    Ok(Secret(hex_encode(&bytes)))
}

pub fn load_or_create_secret(dir: &Path) -> Result<Secret, BridgeError> {
    fs::create_dir_all(dir)?;
    let path = dir.join(SECRET_FILE_NAME);
    if path.exists() {
        return load_secret(dir);
    }
    let secret = generate_secret()?;
    atomic_write_secret(&path, secret.as_str())?;
    Ok(secret)
}

pub fn load_secret(dir: &Path) -> Result<Secret, BridgeError> {
    let path = dir.join(SECRET_FILE_NAME);
    let raw = fs::read_to_string(&path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            BridgeError::MissingSecret
        } else {
            BridgeError::Io(err)
        }
    })?;
    let secret = raw.trim().to_string();
    if secret.len() < 32 {
        return Err(BridgeError::MissingSecret);
    }
    Ok(Secret(secret))
}

pub fn rotate_secret(dir: &Path) -> Result<Secret, BridgeError> {
    fs::create_dir_all(dir)?;
    let secret = generate_secret()?;
    atomic_write_secret(&dir.join(SECRET_FILE_NAME), secret.as_str())?;
    Ok(secret)
}

pub fn write_endpoint(dir: &Path, endpoint: &EndpointFile) -> Result<(), BridgeError> {
    fs::create_dir_all(dir)?;
    let path = dir.join(ENDPOINT_FILE_NAME);
    let json = serde_json::to_vec_pretty(endpoint).map_err(BridgeError::from_json)?;
    atomic_write(&path, &json)?;
    Ok(())
}

pub fn load_endpoint(dir: &Path) -> Result<EndpointFile, BridgeError> {
    let path = dir.join(ENDPOINT_FILE_NAME);
    let raw = fs::read_to_string(path)?;
    serde_json::from_str(&raw).map_err(BridgeError::from_json)
}

fn atomic_write_secret(path: &Path, secret: &str) -> Result<(), BridgeError> {
    atomic_write(path, secret.as_bytes())?;
    restrict_to_current_user(path)?;
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), BridgeError> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(windows)]
fn restrict_to_current_user(path: &Path) -> Result<(), BridgeError> {
    windows_acl::restrict_file_to_current_user(path)
}

#[cfg(not(windows))]
fn restrict_to_current_user(path: &Path) -> Result<(), BridgeError> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(windows)]
mod windows_acl {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr;

    use windows_sys::Win32::Foundation::{
        CloseHandle, LocalFree, BOOL, HANDLE, INVALID_HANDLE_VALUE, ERROR_SUCCESS,
    };
    use windows_sys::Win32::Security::{
        GetSecurityDescriptorDacl, GetTokenInformation, TokenUser, PSECURITY_DESCRIPTOR,
        TOKEN_QUERY, TOKEN_USER, ACL, DACL_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        SetNamedSecurityInfoW, SE_FILE_OBJECT, SDDL_REVISION_1,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    use crate::error::BridgeError;

    pub fn restrict_file_to_current_user(path: &Path) -> Result<(), BridgeError> {
        unsafe { restrict_file_to_current_user_inner(path) }
    }

    unsafe fn restrict_file_to_current_user_inner(path: &Path) -> Result<(), BridgeError> {
        let mut token: HANDLE = INVALID_HANDLE_VALUE;
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(acl_error("OpenProcessToken failed"));
        }
        let mut needed: u32 = 0;
        GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut needed);
        let mut buf = vec![0u8; needed as usize];
        if GetTokenInformation(
            token,
            TokenUser,
            buf.as_mut_ptr() as *mut _,
            needed,
            &mut needed,
        ) == 0
        {
            CloseHandle(token);
            return Err(acl_error("GetTokenInformation failed"));
        }
        CloseHandle(token);
        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut sid_str: *mut u16 = ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut sid_str) == 0 {
            return Err(acl_error("ConvertSidToStringSidW failed"));
        }
        let sid = utf16_ptr_to_string(sid_str);
        LocalFree(sid_str as *mut _);

        let sddl = format!("D:P(A;;FA;;;{})", sid);
        let sddl_wide: Vec<u16> = OsStr::new(&sddl)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut sd: PSECURITY_DESCRIPTOR = ptr::null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl_wide.as_ptr(),
            SDDL_REVISION_1,
            &mut sd,
            ptr::null_mut(),
        ) == 0
        {
            return Err(acl_error(
                "ConvertStringSecurityDescriptorToSecurityDescriptorW failed",
            ));
        }
        let mut present: BOOL = 0;
        let mut defaulted: BOOL = 0;
        let mut dacl: *mut ACL = ptr::null_mut();
        if GetSecurityDescriptorDacl(sd, &mut present, &mut dacl, &mut defaulted) == 0 {
            LocalFree(sd);
            return Err(acl_error("GetSecurityDescriptorDacl failed"));
        }
        let wide_path: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let status = SetNamedSecurityInfoW(
            wide_path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            dacl,
            ptr::null_mut(),
        );
        LocalFree(sd);
        if status != ERROR_SUCCESS {
            return Err(acl_error("SetNamedSecurityInfoW failed"));
        }
        Ok(())
    }

    fn utf16_ptr_to_string(ptr: *mut u16) -> String {
        let mut len = 0usize;
        unsafe {
            while *ptr.add(len) != 0 {
                len += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
        }
    }

    fn acl_error(message: &str) -> BridgeError {
        BridgeError::Install(message.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_debug_and_display_are_redacted() {
        let secret = Secret::from_raw("super-secret-value-do-not-leak".into());
        let debug = format!("{secret:?}");
        let display = format!("{secret}");
        let error = BridgeError::AuthenticationFailed.to_string();
        assert!(!debug.contains("super-secret"));
        assert!(!display.contains("super-secret"));
        assert!(!error.contains("super-secret"));
        assert!(debug.contains("redacted"));
    }

    #[test]
    fn constant_time_compare_rejects_wrong_secret() {
        let secret = Secret::from_raw("abc".repeat(16));
        assert!(secret.constant_time_eq(&"abc".repeat(16)));
        assert!(!secret.constant_time_eq("wrong"));
    }

    #[test]
    fn endpoint_file_does_not_embed_secret() {
        let endpoint = EndpointFile {
            transport: "named_pipe".into(),
            path: r"\\.\pipe\omninova-browser-host-test".into(),
            generation: 3,
            connection_nonce: "abcd".into(),
        };
        let json = serde_json::to_string(&endpoint).unwrap();
        assert!(!json.contains("secret"));
        assert!(!format!("{endpoint:?}").contains("ipc.secret"));
    }
}
