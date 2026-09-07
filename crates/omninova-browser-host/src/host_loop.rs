use std::io::{self, Write};
use std::time::Duration;

use crate::constants::{application_max_message_bytes, protocol_version};
use crate::error::BridgeError;
use crate::framing::{encode_raw_json, read_frame, write_frame};
use crate::ipc::AuthRequest;
use crate::secret::{default_bridge_dir, load_endpoint, load_secret};

/// Thin pipe: Chrome Native Messaging stdin/stdout ↔ authenticated Desktop IPC.
/// stdout is reserved for Native Messaging frames.
pub async fn run_native_host() -> Result<(), BridgeError> {
    let dir = default_bridge_dir();
    let mut reader = native_messaging::host::spawn_reader(application_max_message_bytes());
    loop {
        let endpoint = match load_endpoint(&dir) {
            Ok(endpoint) => endpoint,
            Err(_) => {
                tokio::select! {
                    msg = reader.recv() => {
                        match msg {
                            None | Some(Err(native_messaging::host::NmError::Disconnected)) => {
                                return Ok(());
                            }
                            Some(_) => continue,
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(250)) => continue,
                }
            }
        };
        let secret = match load_secret(&dir) {
            Ok(secret) => secret,
            Err(err) => {
                tracing::warn!(code = err.code(), "native host missing IPC secret");
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
        };
        match connect_desktop(&endpoint.path).await {
            Ok(mut stream) => {
                let auth = AuthRequest {
                    protocol_version: protocol_version(),
                    secret: secret.as_str().to_string(),
                    generation: endpoint.generation,
                    connection_nonce: endpoint.connection_nonce.clone(),
                    host_pid: std::process::id(),
                };
                let encoded = serde_json::to_string(&auth).map_err(BridgeError::from_json)?;
                write_frame(&mut stream, &encoded).await?;
                match read_frame(&mut stream).await? {
                    Some(ack) => {
                        if ack.contains("\"ok\":false") {
                            tracing::warn!("desktop rejected native host authentication");
                            return Err(BridgeError::AuthenticationFailed);
                        }
                    }
                    None => return Err(BridgeError::Disconnected),
                }
                if let Err(err) = forward_session(&mut stream, &mut reader).await {
                    if matches!(err, BridgeError::Disconnected) {
                        return Ok(());
                    }
                    tracing::warn!(code = err.code(), "native host forward session ended");
                }
            }
            Err(_) => {
                tokio::select! {
                    msg = reader.recv() => {
                        match msg {
                            None | Some(Err(native_messaging::host::NmError::Disconnected)) => {
                                return Ok(());
                            }
                            Some(_) => {}
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(250)) => {}
                }
            }
        }
    }
}

async fn forward_session<S>(
    stream: &mut S,
    reader: &mut tokio::sync::mpsc::Receiver<Result<String, native_messaging::host::NmError>>,
) -> Result<(), BridgeError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    loop {
        tokio::select! {
            inbound = reader.recv() => {
                match inbound {
                    None | Some(Err(native_messaging::host::NmError::Disconnected)) => {
                        return Err(BridgeError::Disconnected);
                    }
                    Some(Err(err)) => return Err(err.into()),
                    Some(Ok(json)) => {
                        write_frame(stream, &json).await?;
                    }
                }
            }
            outbound = read_frame(stream) => {
                match outbound? {
                    Some(json) => write_stdout_frame(&json)?,
                    None => return Err(BridgeError::Disconnected),
                }
            }
        }
    }
}

fn write_stdout_frame(json: &str) -> Result<(), BridgeError> {
    let frame = encode_raw_json(json)?;
    let mut stdout = io::stdout();
    stdout.write_all(&frame)?;
    stdout.flush()?;
    Ok(())
}

#[cfg(windows)]
async fn connect_desktop(
    path: &str,
) -> Result<tokio::net::windows::named_pipe::NamedPipeClient, BridgeError> {
    Ok(crate::ipc::named_pipe::open_client(path)?)
}

#[cfg(not(windows))]
async fn connect_desktop(path: &str) -> Result<tokio::net::UnixStream, BridgeError> {
    Ok(crate::ipc::unix_socket::connect(std::path::Path::new(path)).await?)
}
