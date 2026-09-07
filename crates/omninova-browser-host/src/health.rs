use serde::{Deserialize, Serialize};

/// Transport health only. This is not BrowserControlState.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportHealth {
    NotInstalled,
    HostInstalled,
    ExtensionDisconnected,
    Connecting,
    Connected,
    ProtocolMismatch,
    AuthenticationFailed,
}

impl TransportHealth {
    pub fn as_status_string(self) -> &'static str {
        match self {
            TransportHealth::NotInstalled => "not_installed",
            TransportHealth::HostInstalled => "installed",
            TransportHealth::ExtensionDisconnected => "extension_not_connected",
            TransportHealth::Connecting => "connecting",
            TransportHealth::Connected => "connected",
            TransportHealth::ProtocolMismatch => "protocol_mismatch",
            TransportHealth::AuthenticationFailed => "authentication_failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportStatus {
    pub state: String,
    pub protocol_version: u32,
    pub host_name: String,
    pub extension_id: String,
    pub generation: u64,
    pub connected: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_is_not_browser_control_state() {
        let names = [
            TransportHealth::NotInstalled.as_status_string(),
            TransportHealth::HostInstalled.as_status_string(),
            TransportHealth::ExtensionDisconnected.as_status_string(),
            TransportHealth::Connecting.as_status_string(),
            TransportHealth::Connected.as_status_string(),
            TransportHealth::ProtocolMismatch.as_status_string(),
            TransportHealth::AuthenticationFailed.as_status_string(),
        ];
        for name in names {
            assert!(!name.contains("browser_control"));
            assert!(!name.contains("human_controlled"));
        }
    }
}
