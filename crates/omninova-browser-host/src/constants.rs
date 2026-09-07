use serde::Deserialize;

const SHARED_JSON: &str = include_str!("../shared/constants.json");

#[derive(Debug, Deserialize)]
struct SharedConstants {
    native_host_name: String,
    native_host_binary: String,
    protocol_version: u32,
    application_max_message_bytes: usize,
    dev_extension_id: String,
    app_data_folder: String,
    bridge_subdir: String,
}

fn shared() -> SharedConstants {
    serde_json::from_str(SHARED_JSON).expect("shared/constants.json must parse")
}

/// Chromium Native Messaging host name. Lowercase, stable, shared with the extension.
pub fn native_host_name() -> String {
    shared().native_host_name
}

pub fn native_host_binary() -> String {
    shared().native_host_binary
}

pub fn protocol_version() -> u32 {
    shared().protocol_version
}

/// Application-level inbound payload budget. Matches Chrome host→browser (1 MiB).
pub fn application_max_message_bytes() -> usize {
    shared().application_max_message_bytes
}

pub fn dev_extension_id() -> String {
    shared().dev_extension_id
}

pub fn allowed_origin() -> String {
    format!("chrome-extension://{}/", dev_extension_id())
}

pub fn app_data_folder() -> String {
    shared().app_data_folder
}

pub fn bridge_subdir() -> String {
    shared().bridge_subdir
}

pub const INSTALL_DESCRIPTION: &str = "OmniNova Personal Chrome native messaging host";
pub const INSTALL_BROWSERS: &[&str] = &["chrome", "chrome_for_testing"];

pub const SECRET_FILE_NAME: &str = "ipc.secret";
pub const ENDPOINT_FILE_NAME: &str = "endpoint.json";

pub const ENV_BRIDGE_DIR: &str = "OMNINOVA_BROWSER_BRIDGE_DIR";
pub const ENV_HOST_EXE: &str = "OMNINOVA_BROWSER_HOST_EXE";

/// Transport-level capabilities advertised during hello_ack. No page automation.
pub const TRANSPORT_CAPABILITIES: &[&str] = &[
    "hello",
    "ping",
    "capabilities",
    "attach_transport",
    "detach_transport",
    "reconnect",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_constants_are_locked() {
        assert_eq!(native_host_name(), "com.omninova.browser_host");
        assert_eq!(native_host_binary(), "omninova-browser-host");
        assert_eq!(protocol_version(), 1);
        assert_eq!(application_max_message_bytes(), 1_048_576);
        assert_eq!(dev_extension_id(), "caooogobppgihkdpcjibhoinkfobenhe");
        assert_eq!(
            allowed_origin(),
            "chrome-extension://caooogobppgihkdpcjibhoinkfobenhe/"
        );
        let extension_copy = include_str!("../../../extensions/omninova-personal-chrome/src/constants.json");
        assert_eq!(SHARED_JSON, extension_copy);
        assert!(
            !SHARED_JSON.contains("BEGIN") && !SHARED_JSON.contains("PRIVATE"),
            "must not commit private signing material"
        );
    }
}
