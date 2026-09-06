use serde::{Deserialize, Serialize};

use crate::error::BridgeError;

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthRequest {
    pub protocol_version: u32,
    pub secret: String,
    pub generation: u64,
    pub connection_nonce: String,
    pub host_pid: u32,
}

impl std::fmt::Debug for AuthRequestRedacted<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthRequest")
            .field("protocol_version", &self.0.protocol_version)
            .field("secret", &"[redacted]")
            .field("generation", &self.0.generation)
            .field("connection_nonce", &self.0.connection_nonce)
            .field("host_pid", &self.0.host_pid)
            .finish()
    }
}

pub struct AuthRequestRedacted<'a>(pub &'a AuthRequest);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthOk {
    pub ok: bool,
    pub connection_id: String,
    pub generation: u64,
    pub protocol_version: u32,
}

pub fn verify_auth(
    req: &AuthRequest,
    expected_secret: &crate::secret::Secret,
    expected_generation: u64,
    expected_nonce: &str,
    expected_protocol: u32,
) -> Result<(), BridgeError> {
    if req.protocol_version != expected_protocol {
        return Err(BridgeError::ProtocolMismatch {
            requested: req.protocol_version,
            expected: expected_protocol,
        });
    }
    if req.secret.is_empty() {
        return Err(BridgeError::MissingSecret);
    }
    if !expected_secret.constant_time_eq(&req.secret) {
        return Err(BridgeError::AuthenticationFailed);
    }
    if req.generation != expected_generation || req.connection_nonce != expected_nonce {
        return Err(BridgeError::StaleGeneration {
            got: req.generation,
            expected: expected_generation,
        });
    }
    Ok(())
}

#[cfg(windows)]
pub mod named_pipe {
    use tokio::net::windows::named_pipe::{
        ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
    };

    pub fn pipe_name(install_token: &str, generation: u64) -> String {
        format!(r"\\.\pipe\omninova-browser-host-{install_token}-{generation}")
    }

    pub fn create_server(name: &str, first: bool) -> std::io::Result<NamedPipeServer> {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create(name)
    }

    pub fn open_client(name: &str) -> std::io::Result<NamedPipeClient> {
        ClientOptions::new().open(name)
    }
}

#[cfg(not(windows))]
pub mod unix_socket {
    use std::path::Path;

    use tokio::net::{UnixListener, UnixStream};

    pub async fn bind(path: &Path) -> std::io::Result<UnixListener> {
        let _ = std::fs::remove_file(path);
        UnixListener::bind(path)
    }

    pub async fn connect(path: &Path) -> std::io::Result<UnixStream> {
        UnixStream::connect(path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::Secret;

    #[test]
    fn auth_debug_redacts_secret() {
        let req = AuthRequest {
            protocol_version: 1,
            secret: "leaked-secret-value".into(),
            generation: 1,
            connection_nonce: "n".into(),
            host_pid: 1,
        };
        let rendered = format!("{:?}", AuthRequestRedacted(&req));
        assert!(!rendered.contains("leaked-secret-value"));
        assert!(rendered.contains("redacted"));
    }

    #[test]
    fn missing_and_wrong_secret_are_typed() {
        let secret = Secret::from_raw("correct-secret-correct-secret-ok".into());
        let missing = AuthRequest {
            protocol_version: 1,
            secret: String::new(),
            generation: 1,
            connection_nonce: "nonce".into(),
            host_pid: 9,
        };
        assert!(matches!(
            verify_auth(&missing, &secret, 1, "nonce", 1),
            Err(BridgeError::MissingSecret)
        ));
        let wrong = AuthRequest {
            secret: "nope".into(),
            ..missing
        };
        let err = verify_auth(&wrong, &secret, 1, "nonce", 1).unwrap_err();
        assert!(matches!(err, BridgeError::AuthenticationFailed));
        assert!(!err.to_string().contains("nope"));
        assert!(!err.to_string().contains("correct-secret"));
    }

    #[test]
    fn stale_generation_is_typed() {
        let secret = Secret::from_raw("correct-secret-correct-secret-ok".into());
        let req = AuthRequest {
            protocol_version: 1,
            secret: "correct-secret-correct-secret-ok".into(),
            generation: 1,
            connection_nonce: "old".into(),
            host_pid: 9,
        };
        let err = verify_auth(&req, &secret, 2, "new", 1).unwrap_err();
        assert!(matches!(err, BridgeError::StaleGeneration { got: 1, expected: 2 }));
    }
}
