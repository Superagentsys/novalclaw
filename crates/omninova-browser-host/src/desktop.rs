use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot, Mutex, Notify};
use uuid::Uuid;

use crate::constants::{
    application_max_message_bytes, dev_extension_id, native_host_name, protocol_version,
};
use crate::error::BridgeError;
use crate::framing::{read_frame, write_frame};
use crate::health::{TransportHealth, TransportStatus};
use crate::install::{
    install_product_host, remove_product_host, resolve_host_executable, verify_product_host,
};
use crate::ipc::{verify_auth, AuthOk, AuthRequest};
use crate::protocol::{
    dispatch, parse_request, TransportRequest, TransportResponse, TransportSession,
};
use crate::secret::{
    load_endpoint, load_or_create_secret, rotate_secret, write_endpoint, EndpointFile, Secret,
};

struct LiveState {
    health: TransportHealth,
    generation: u64,
    connection_id: Option<String>,
    connection_nonce: String,
    endpoint_path: String,
    saw_connection: bool,
}

struct OutboundCall {
    raw: String,
    request_id: String,
    reply: oneshot::Sender<Result<String, BridgeError>>,
}

struct Inner {
    dir: PathBuf,
    secret: Mutex<Secret>,
    live: Mutex<LiveState>,
    wake: Notify,
    outbound: Mutex<Option<mpsc::Sender<OutboundCall>>>,
}

#[derive(Clone)]
pub struct PersonalChromeBridge {
    inner: Arc<Inner>,
}

impl PersonalChromeBridge {
    pub fn spawn(dir: PathBuf) -> Result<Self, BridgeError> {
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| BridgeError::RuntimeUnavailable)?;
        Self::spawn_on(dir, runtime)
    }

    /// Starts the bridge listener on an explicit runtime.
    ///
    /// Desktop setup callbacks are synchronous and therefore may not have an
    /// entered Tokio context even when the application already owns a runtime.
    pub fn spawn_on(
        dir: PathBuf,
        runtime: tokio::runtime::Handle,
    ) -> Result<Self, BridgeError> {
        std::fs::create_dir_all(&dir)?;
        let secret = load_or_create_secret(&dir)?;
        let previous_generation = load_endpoint(&dir).map(|e| e.generation).unwrap_or(0);
        let generation = previous_generation.saturating_add(1);
        let connection_nonce = random_hex(16)?;
        let install_token = load_or_create_install_token(&dir)?;
        let endpoint_path = ipc_path(&dir, &install_token, generation);
        let endpoint = EndpointFile {
            transport: ipc_transport_name().into(),
            path: endpoint_path.clone(),
            generation,
            connection_nonce: connection_nonce.clone(),
        };
        write_endpoint(&dir, &endpoint)?;
        let inner = Arc::new(Inner {
            dir: dir.clone(),
            secret: Mutex::new(secret),
            live: Mutex::new(LiveState {
                health: TransportHealth::HostInstalled,
                generation,
                connection_id: None,
                connection_nonce,
                endpoint_path,
                saw_connection: false,
            }),
            wake: Notify::new(),
            outbound: Mutex::new(None),
        });
        let bridge = Self { inner };
        let worker = bridge.clone();
        runtime.spawn(async move {
            if let Err(err) = worker.accept_loop().await {
                tracing::warn!(code = err.code(), "personal chrome IPC listener stopped");
            }
        });
        Ok(bridge)
    }

    pub async fn rotate_ipc_secret(&self) -> Result<(), BridgeError> {
        let secret = rotate_secret(&self.inner.dir)?;
        *self.inner.secret.lock().await = secret;
        self.bump_generation().await?;
        Ok(())
    }

    pub async fn status(&self) -> TransportStatus {
        let installed = verify_product_host().unwrap_or(false);
        let live = self.inner.live.lock().await;
        let health = if !installed {
            TransportHealth::NotInstalled
        } else {
            match live.health {
                TransportHealth::NotInstalled => TransportHealth::HostInstalled,
                other => other,
            }
        };
        TransportStatus {
            state: health.as_status_string().to_string(),
            protocol_version: protocol_version(),
            host_name: native_host_name(),
            extension_id: dev_extension_id(),
            generation: live.generation,
            connected: matches!(health, TransportHealth::Connected),
        }
    }

    pub async fn install_bridge(&self) -> Result<TransportStatus, BridgeError> {
        let exe = resolve_host_executable()?;
        install_product_host(&exe)?;
        {
            let mut live = self.inner.live.lock().await;
            if !matches!(
                live.health,
                TransportHealth::Connected | TransportHealth::Connecting
            ) {
                live.health = TransportHealth::HostInstalled;
            }
        }
        Ok(self.status().await)
    }

    pub async fn verify_bridge(&self) -> Result<TransportStatus, BridgeError> {
        Ok(self.status().await)
    }

    /// Send a backend operation to the connected extension. The Native Host
    /// remains a thin forwarder; this is Desktop → extension over the live IPC.
    pub async fn request(
        &self,
        operation: &str,
        session_id: &str,
        payload: Value,
    ) -> Result<TransportResponse, BridgeError> {
        let request_id = format!("dreq:{}", Uuid::new_v4());
        let req = TransportRequest {
            protocol_version: protocol_version(),
            request_id: request_id.clone(),
            session_id: session_id.to_string(),
            operation: operation.to_string(),
            payload,
        };
        let encoded = serde_json::to_string(&req).map_err(BridgeError::from_json)?;
        if encoded.len() > application_max_message_bytes() {
            return Err(BridgeError::PayloadTooLarge {
                len: encoded.len(),
                max: application_max_message_bytes(),
            });
        }
        let tx = self
            .inner
            .outbound
            .lock()
            .await
            .clone()
            .ok_or(BridgeError::Disconnected)?;
        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(OutboundCall {
            raw: encoded,
            request_id,
            reply: reply_tx,
        })
        .await
        .map_err(|_| BridgeError::Disconnected)?;
        let raw = tokio::time::timeout(Duration::from_secs(20), reply_rx)
            .await
            .map_err(|_| BridgeError::Disconnected)?
            .map_err(|_| BridgeError::Disconnected)??;
        serde_json::from_str(&raw).map_err(BridgeError::from_json)
    }

    pub async fn remove_bridge(&self) -> Result<TransportStatus, BridgeError> {
        remove_product_host()?;
        let mut live = self.inner.live.lock().await;
        live.health = TransportHealth::NotInstalled;
        live.connection_id = None;
        drop(live);
        Ok(self.status().await)
    }

    async fn bump_generation(&self) -> Result<(), BridgeError> {
        let mut live = self.inner.live.lock().await;
        live.generation = live.generation.saturating_add(1);
        live.connection_nonce = random_hex(16)?;
        live.connection_id = None;
        live.health = TransportHealth::HostInstalled;
        let install_token = load_or_create_install_token(&self.inner.dir)?;
        live.endpoint_path = ipc_path(&self.inner.dir, &install_token, live.generation);
        let endpoint = EndpointFile {
            transport: ipc_transport_name().into(),
            path: live.endpoint_path.clone(),
            generation: live.generation,
            connection_nonce: live.connection_nonce.clone(),
        };
        drop(live);
        write_endpoint(&self.inner.dir, &endpoint)?;
        self.inner.wake.notify_waiters();
        Ok(())
    }

    async fn accept_loop(&self) -> Result<(), BridgeError> {
        loop {
            let (path, first) = {
                let live = self.inner.live.lock().await;
                (live.endpoint_path.clone(), !live.saw_connection)
            };
            match accept_once(&path, first, &self.inner.wake).await {
                Ok(None) => continue,
                Ok(Some(stream)) => {
                    {
                        let mut live = self.inner.live.lock().await;
                        live.health = TransportHealth::Connecting;
                        live.saw_connection = true;
                    }
                    if let Err(err) = self.handle_stream(stream).await {
                        tracing::debug!(code = err.code(), "personal chrome IPC session ended");
                        let mut live = self.inner.live.lock().await;
                        live.health = match err {
                            BridgeError::ProtocolMismatch { .. } => {
                                TransportHealth::ProtocolMismatch
                            }
                            BridgeError::AuthenticationFailed | BridgeError::MissingSecret => {
                                TransportHealth::AuthenticationFailed
                            }
                            BridgeError::StaleGeneration { .. } => {
                                TransportHealth::AuthenticationFailed
                            }
                            _ => TransportHealth::ExtensionDisconnected,
                        };
                        live.connection_id = None;
                    }
                }
                Err(err) => {
                    tracing::debug!(code = err.code(), "waiting for native host IPC");
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
        }
    }

    async fn handle_stream<S>(&self, mut stream: S) -> Result<(), BridgeError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let auth_raw = tokio::time::timeout(Duration::from_secs(5), read_frame(&mut stream))
            .await
            .map_err(|_| BridgeError::AuthenticationFailed)?
            .map_err(|_| BridgeError::AuthenticationFailed)?
            .ok_or(BridgeError::Disconnected)?;
        if auth_raw.len() > application_max_message_bytes() {
            return Err(BridgeError::PayloadTooLarge {
                len: auth_raw.len(),
                max: application_max_message_bytes(),
            });
        }
        let auth: AuthRequest = serde_json::from_str(&auth_raw).map_err(|_| {
            BridgeError::MalformedFrame {
                detail: "invalid auth envelope".into(),
            }
        })?;
        let (secret, generation, nonce) = {
            let secret = self.inner.secret.lock().await.clone();
            let live = self.inner.live.lock().await;
            (secret, live.generation, live.connection_nonce.clone())
        };
        if let Err(err) = verify_auth(&auth, &secret, generation, &nonce, protocol_version()) {
            let _ = write_frame(
                &mut stream,
                &serde_json::to_string(&crate::protocol::TransportResponse::err("auth", &err))
                    .unwrap_or_else(|_| r#"{"ok":false,"error":{"code":"AuthenticationFailed","message":"AuthenticationFailed"}}"#.into()),
            )
            .await;
            return Err(err);
        }
        let connection_id = format!("conn:{}", Uuid::new_v4());
        let ack = AuthOk {
            ok: true,
            connection_id: connection_id.clone(),
            generation,
            protocol_version: protocol_version(),
        };
        write_frame(&mut stream, &serde_json::to_string(&ack).map_err(BridgeError::from_json)?)
            .await?;
        {
            let mut live = self.inner.live.lock().await;
            live.connection_id = Some(connection_id.clone());
            live.health = TransportHealth::Connecting;
        }
        let mut session = TransportSession {
            connection_id,
            generation,
            transport_session_id: None,
            hello_completed: false,
        };
        let (out_tx, mut out_rx) = mpsc::channel::<OutboundCall>(32);
        *self.inner.outbound.lock().await = Some(out_tx);
        let mut pending: HashMap<String, oneshot::Sender<Result<String, BridgeError>>> =
            HashMap::new();
        let result = self
            .session_mux(&mut stream, &mut session, &mut out_rx, &mut pending)
            .await;
        *self.inner.outbound.lock().await = None;
        for (_, reply) in pending {
            let _ = reply.send(Err(BridgeError::Disconnected));
        }
        result
    }

    async fn session_mux<S>(
        &self,
        stream: &mut S,
        session: &mut TransportSession,
        out_rx: &mut mpsc::Receiver<OutboundCall>,
        pending: &mut HashMap<String, oneshot::Sender<Result<String, BridgeError>>>,
    ) -> Result<(), BridgeError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        loop {
            tokio::select! {
                outbound = out_rx.recv() => {
                    let Some(call) = outbound else {
                        return Err(BridgeError::Disconnected);
                    };
                    pending.insert(call.request_id, call.reply);
                    write_frame(stream, &call.raw).await?;
                }
                incoming = read_frame(stream) => {
                    let raw = match incoming? {
                        Some(raw) => raw,
                        None => return Err(BridgeError::Disconnected),
                    };
                    if incoming_is_response(&raw) {
                        if let Ok(resp) = serde_json::from_str::<TransportResponse>(&raw) {
                            if let Some(reply) = pending.remove(&resp.request_id) {
                                let _ = reply.send(Ok(raw));
                            }
                        }
                        continue;
                    }
                    let response = match parse_request(&raw) {
                        Ok(req) => {
                            let response = dispatch(session, &req);
                            if !response.ok {
                                if response
                                    .error
                                    .as_ref()
                                    .map(|e| e.code == "ProtocolMismatch")
                                    .unwrap_or(false)
                                {
                                    let mut live = self.inner.live.lock().await;
                                    live.health = TransportHealth::ProtocolMismatch;
                                }
                            } else if req.operation == crate::protocol::OP_HELLO {
                                let mut live = self.inner.live.lock().await;
                                live.health = TransportHealth::Connected;
                            }
                            response
                        }
                        Err(err) => TransportResponse::err("", &err),
                    };
                    let encoded = serde_json::to_string(&response).map_err(BridgeError::from_json)?;
                    write_frame(stream, &encoded).await?;
                    if response
                        .error
                        .as_ref()
                        .map(|e| e.code == "ProtocolMismatch")
                        .unwrap_or(false)
                    {
                        return Err(BridgeError::ProtocolMismatch {
                            requested: 0,
                            expected: protocol_version(),
                        });
                    }
                }
            }
        }
    }
}

fn incoming_is_response(raw: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return false;
    };
    value.get("ok").is_some() && value.get("operation").is_none()
}

fn random_hex(nbytes: usize) -> Result<String, BridgeError> {
    let mut bytes = vec![0u8; nbytes];
    getrandom::getrandom(&mut bytes)
        .map_err(|err| BridgeError::Install(format!("rng failed: {err}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn load_or_create_install_token(dir: &Path) -> Result<String, BridgeError> {
    let path = dir.join("install.id");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    let token = random_hex(4)?;
    std::fs::write(path, &token)?;
    Ok(token)
}

fn ipc_transport_name() -> &'static str {
    if cfg!(windows) {
        "named_pipe"
    } else {
        "unix_socket"
    }
}

fn ipc_path(dir: &Path, install_token: &str, generation: u64) -> String {
    #[cfg(windows)]
    {
        let _ = dir;
        crate::ipc::named_pipe::pipe_name(install_token, generation)
    }
    #[cfg(not(windows))]
    {
        dir.join(format!("ipc-{install_token}-{generation}.sock"))
            .to_string_lossy()
            .into_owned()
    }
}

#[cfg(windows)]
async fn accept_once(
    path: &str,
    first: bool,
    wake: &tokio::sync::Notify,
) -> Result<Option<tokio::net::windows::named_pipe::NamedPipeServer>, BridgeError> {
    let server = match crate::ipc::named_pipe::create_server(path, first) {
        Ok(server) => server,
        Err(_) if first => crate::ipc::named_pipe::create_server(path, false)?,
        Err(err) => return Err(err.into()),
    };
    tokio::select! {
        result = server.connect() => {
            result?;
            Ok(Some(server))
        }
        _ = wake.notified() => Ok(None),
    }
}

#[cfg(not(windows))]
async fn accept_once(
    path: &str,
    _first: bool,
    wake: &tokio::sync::Notify,
) -> Result<Option<tokio::net::UnixStream>, BridgeError> {
    let listener = crate::ipc::unix_socket::bind(Path::new(path)).await?;
    tokio::select! {
        result = listener.accept() => {
            let (stream, _) = result?;
            Ok(Some(stream))
        }
        _ = wake.notified() => Ok(None),
    }
}

/// In-memory protocol helper used by tests that must not open Chrome.
pub fn handle_json_for_test(raw: &str, session: &mut TransportSession) -> Value {
    match parse_request(raw) {
        Ok(req) => serde_json::to_value(dispatch(session, &req)).unwrap_or(Value::Null),
        Err(err) => serde_json::to_value(crate::protocol::TransportResponse::err("", &err))
            .unwrap_or(Value::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::Secret;

    #[test]
    fn spawn_without_runtime_returns_typed_error_instead_of_panicking() {
        let tmp = tempfile::tempdir().unwrap();
        let result = PersonalChromeBridge::spawn(tmp.path().join("bridge"));
        assert!(matches!(result, Err(BridgeError::RuntimeUnavailable)));
    }

    #[test]
    fn spawn_on_accepts_an_explicit_runtime_outside_entered_context() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let result = PersonalChromeBridge::spawn_on(
            tmp.path().join("bridge"),
            runtime.handle().clone(),
        );

        assert!(result.is_ok());
    }

    #[test]
    fn auth_failure_errors_do_not_include_secret() {
        let secret = Secret::from_raw("real-secret-value-real-secret".into());
        let req = AuthRequest {
            protocol_version: 1,
            secret: "real-secret-value-real-secret".into(),
            generation: 9,
            connection_nonce: "n1".into(),
            host_pid: 1,
        };
        let err = verify_auth(&req, &secret, 10, "n2", 1).unwrap_err();
        assert!(!format!("{err:?}").contains("real-secret-value"));
        assert!(!err.to_string().contains("real-secret-value"));
    }
}
