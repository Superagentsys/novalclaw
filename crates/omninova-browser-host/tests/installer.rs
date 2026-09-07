use std::fs;
use std::path::PathBuf;

use omninova_browser_host::constants::allowed_origin;
use omninova_browser_host::install::{
    chrome_allowed_origins, install_host, manifest_allows_only_expected_origin, remove_host,
    validate_host_exe_path, verify_host,
};
use omninova_browser_host::BridgeError;

#[test]
fn install_path_must_be_absolute() {
    let err = validate_host_exe_path(std::path::Path::new("relative-host.exe")).unwrap_err();
    assert!(matches!(err, BridgeError::HostPathNotAbsolute { .. }));
}

#[test]
fn allowed_origins_pin_only_omninova_extension() {
    let origins = chrome_allowed_origins();
    assert_eq!(origins.len(), 1);
    assert_eq!(origins[0], allowed_origin());
    let manifest = serde_json::json!({ "allowed_origins": origins }).to_string();
    manifest_allows_only_expected_origin(&manifest).unwrap();
}

fn test_host_name() -> String {
    format!(
        "com.omninova.browser_host_t{}",
        std::process::id()
    )
}

#[cfg(windows)]
fn unicode_dummy_host() -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let unicode_dir = root.path().join("用户张真");
    fs::create_dir_all(&unicode_dir).unwrap();
    let exe = unicode_dir.join("omninova-browser-host-test.exe");
    fs::write(&exe, b"dummy").unwrap();
    (root, exe)
}

/// Isolated user-scope registry install. Never uses the product host name.
#[cfg(windows)]
#[test]
fn windows_user_install_verify_remove_isolated_host() {
    let host = format!("{}_{}", test_host_name(), "iso");
    let (_tmp, exe) = unicode_dummy_host();
    assert!(exe.is_absolute());
    assert!(exe.to_str().is_some(), "Unicode path must be valid UTF-8");
    install_host(&exe, &host).expect("install test host");
    assert!(verify_host(&host).expect("verify test host"));

    let manifest_dir = PathBuf::from(std::env::var("LOCALAPPDATA").unwrap()).join("NativeMessagingHosts");
    let manifest_path = manifest_dir.join(format!("{host}.json"));
    let manifest = fs::read_to_string(&manifest_path).expect("read installed manifest");
    assert!(manifest.contains(&exe.to_str().unwrap().replace('\\', "\\\\")) || manifest.contains(exe.to_str().unwrap()));
    assert!(manifest.contains("用户张真") || manifest.contains("\\u7528"));
    manifest_allows_only_expected_origin(&manifest).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    let path = parsed["path"].as_str().unwrap();
    assert!(PathBuf::from(path).is_absolute());
    assert!(path.contains("张真") || path.contains("用户"));

    remove_host(&host).expect("remove test host");
    assert!(!verify_host(&host).expect("verify removed"));
    assert!(
        !manifest_path.exists(),
        "uninstall must remove OmniNova test host manifest"
    );
}

#[cfg(windows)]
#[test]
fn uninstall_does_not_use_product_host_name_in_isolated_tests() {
    assert_ne!(test_host_name(), omninova_browser_host::native_host_name());
}
