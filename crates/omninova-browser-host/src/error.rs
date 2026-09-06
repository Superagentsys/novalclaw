use std::path::PathBuf;

use thiserror::Error;

/// Typed transport failures. Messages must never include the IPC secret.
#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("ProtocolMismatch: native messaging protocol version is incompatible")]
    ProtocolMismatch { requested: u32, expected: u32 },

    #[error("UnknownOperation: '{operation}' is not a transport operation")]
    UnknownOperation { operation: String },

    #[error("MalformedFrame: native messaging frame is invalid")]
    MalformedFrame { detail: String },

    #[error("PayloadTooLarge: {len} bytes exceeds application limit {max}")]
    PayloadTooLarge { len: usize, max: usize },

    #[error("OriginRejected: native host origin is not the OmniNova extension")]
    OriginRejected,

    #[error("OriginMissing: Chrome did not provide a native host origin argument")]
    OriginMissing,

    #[error("AuthenticationFailed: native host IPC authentication failed")]
    AuthenticationFailed,

    #[error("MissingSecret: IPC secret is not available")]
    MissingSecret,

    #[error("StaleGeneration: native host generation {got} does not match desktop {expected}")]
    StaleGeneration { got: u64, expected: u64 },

    #[error("HostPathNotAbsolute: native host executable path must be absolute")]
    HostPathNotAbsolute { path: PathBuf },

    #[error("HostPathNotUtf8: native host executable path is not valid Unicode/UTF-8")]
    HostPathNotUtf8 { path: PathBuf },

    #[error("HostBinaryNotFound: native host executable was not found")]
    HostBinaryNotFound,

    #[error("NotInstalled: OmniNova native messaging host is not installed")]
    NotInstalled,

    #[error("Disconnected: transport peer closed the connection")]
    Disconnected,

    #[error("RuntimeUnavailable: Personal Chrome transport requires a Tokio runtime")]
    RuntimeUnavailable,

    #[error("Io: {0}")]
    Io(#[from] std::io::Error),

    #[error("Json: {0}")]
    Json(String),

    #[error("Install: {0}")]
    Install(String),
}

impl BridgeError {
    pub fn code(&self) -> &'static str {
        match self {
            BridgeError::ProtocolMismatch { .. } => "ProtocolMismatch",
            BridgeError::UnknownOperation { .. } => "UnknownOperation",
            BridgeError::MalformedFrame { .. } => "MalformedFrame",
            BridgeError::PayloadTooLarge { .. } => "PayloadTooLarge",
            BridgeError::OriginRejected => "OriginRejected",
            BridgeError::OriginMissing => "OriginMissing",
            BridgeError::AuthenticationFailed => "AuthenticationFailed",
            BridgeError::MissingSecret => "MissingSecret",
            BridgeError::StaleGeneration { .. } => "StaleGeneration",
            BridgeError::HostPathNotAbsolute { .. } => "HostPathNotAbsolute",
            BridgeError::HostPathNotUtf8 { .. } => "HostPathNotUtf8",
            BridgeError::HostBinaryNotFound => "HostBinaryNotFound",
            BridgeError::NotInstalled => "NotInstalled",
            BridgeError::Disconnected => "Disconnected",
            BridgeError::RuntimeUnavailable => "RuntimeUnavailable",
            BridgeError::Io(_) => "Io",
            BridgeError::Json(_) => "Json",
            BridgeError::Install(_) => "Install",
        }
    }

    pub fn from_json(err: serde_json::Error) -> Self {
        BridgeError::Json(err.to_string())
    }
}

impl From<native_messaging::host::NmError> for BridgeError {
    fn from(err: native_messaging::host::NmError) -> Self {
        match err {
            native_messaging::host::NmError::IncomingTooLarge { len, max } => {
                BridgeError::PayloadTooLarge { len, max }
            }
            native_messaging::host::NmError::OutgoingTooLarge { len, max } => {
                BridgeError::PayloadTooLarge { len, max }
            }
            native_messaging::host::NmError::Disconnected => BridgeError::Disconnected,
            native_messaging::host::NmError::IncomingNotUtf8(_) => BridgeError::MalformedFrame {
                detail: "incoming frame is not UTF-8".into(),
            },
            native_messaging::host::NmError::Io(e) => BridgeError::Io(e),
            other => BridgeError::MalformedFrame {
                detail: other.to_string(),
            },
        }
    }
}
