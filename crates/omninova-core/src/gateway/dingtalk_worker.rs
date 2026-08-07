//! DingTalk async worker for background processing of webhook events.
//!
//! Phase 1: DingTalk app bot text message handling with sessionWebhook outbound.

use crate::channels::ChannelKind;
use crate::channels::InboundMessage;
use crate::gateway::dingtalk_store::{DingtalkStore, JobStatus};
use crate::gateway::GatewayRuntime;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::RwLock;

/// Queue capacity - max number of pending jobs
const QUEUE_CAPACITY: usize = 100;

/// Runtime execution timeout
const RUNTIME_TIMEOUT_SECS: u64 = 120;

/// Outbound send timeout
const OUTBOUND_TIMEOUT_SECS: u64 = 20;

// ---------------------------------------------------------------------------
// Job types
// ---------------------------------------------------------------------------

/// DingTalk async job - represents a webhook event to be processed in background
#[derive(Debug, Clone)]
pub struct DingtalkAsyncJob {
    pub channel: ChannelKind,
    pub inbound: InboundMessage,
    pub raw_payload: serde_json::Value,
    pub created_at: u64,
    pub job_id: String,
}

impl DingtalkAsyncJob {
    pub fn new(inbound: InboundMessage, raw_payload: serde_json::Value) -> Self {
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let job_id = format!(
            "dt_job_{}_{}",
            created_at,
            &uuid::Uuid::new_v4().to_string()[..8]
        );

        Self {
            channel: ChannelKind::Dingtalk,
            inbound,
            raw_payload,
            created_at,
            job_id,
        }
    }
}

/// Job queue sender type
pub type DingtalkJobSender = mpsc::Sender<DingtalkAsyncJob>;

/// Shared state for the DingTalk worker
pub struct DingtalkWorkerState {
    sender: Option<mpsc::Sender<DingtalkAsyncJob>>,
    receiver: Option<mpsc::Receiver<DingtalkAsyncJob>>,
    pub queue_len: Arc<RwLock<usize>>,
    initialized: bool,
}

impl Default for DingtalkWorkerState {
    fn default() -> Self {
        Self::new()
    }
}

impl DingtalkWorkerState {
    pub fn new() -> Self {
        Self::with_queue_len(Arc::new(RwLock::new(0)))
    }

    pub fn with_queue_len(queue_len: Arc<RwLock<usize>>) -> Self {
        let (sender, receiver) = mpsc::channel::<DingtalkAsyncJob>(QUEUE_CAPACITY);
        Self {
            sender: Some(sender),
            receiver: Some(receiver),
            queue_len,
            initialized: false,
        }
    }

    /// Check if the worker has been initialized (receiver taken)
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub fn sender(&self) -> DingtalkJobSender {
        self.sender.clone().expect("sender already taken")
    }

    /// Take the receiver for starting the worker loop.
    /// Returns Some(receiver) if not yet taken, None if already initialized.
    /// This prevents panic on repeated initialization.
    pub fn try_take_receiver(&mut self) -> Option<mpsc::Receiver<DingtalkAsyncJob>> {
        if self.initialized {
            return None;
        }
        self.initialized = true;
        self.receiver.take()
    }

    /// Take the receiver for starting the worker loop.
    /// Panics if already taken. Use try_take_receiver() for safe idempotent access.
    pub fn take_receiver(&mut self) -> mpsc::Receiver<DingtalkAsyncJob> {
        self.try_take_receiver().expect("receiver already taken")
    }

    pub async fn try_enqueue(&self, job: DingtalkAsyncJob) -> Result<(), EnqueueError> {
        let queue_len = self.queue_len.read().await;
        if *queue_len >= QUEUE_CAPACITY {
            return Err(EnqueueError::QueueFull);
        }
        drop(queue_len);

        self.sender()
            .send(job)
            .await
            .map_err(|_| EnqueueError::QueueFull)?;

        let mut queue_len = self.queue_len.write().await;
        *queue_len += 1;

        Ok(())
    }
}

#[derive(Debug)]
pub enum EnqueueError {
    QueueFull,
}

// ---------------------------------------------------------------------------
// Worker
// ---------------------------------------------------------------------------

/// Start the DingTalk worker.
/// Returns immediately if the worker is already running (idempotent).
pub async fn start_dingtalk_worker(
    mut state: DingtalkWorkerState,
    runtime: Arc<GatewayRuntime>,
    store: Arc<DingtalkStore>,
) {
    let receiver = match state.try_take_receiver() {
        Some(rx) => rx,
        None => {
            println!("[dingtalk-async-worker] already_started=true");
            return;
        }
    };
    let queue_len = state.queue_len.clone();

    println!("[dingtalk-async-worker] started=true");

    tokio::spawn(async move {
        let mut receiver = receiver;
        println!("[dingtalk-worker] loop_started=true");

        while let Some(job) = receiver.recv().await {
            let text_len = job.inbound.text.chars().count();
            let has_conversation = job
                .inbound
                .session_id
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty());
            let has_webhook = job
                .inbound
                .metadata
                .get("sessionWebhook")
                .map(|v| !v.as_str().unwrap_or("").trim().is_empty())
                .unwrap_or(false);
            let has_robot_code = job
                .inbound
                .metadata
                .get("robotCode")
                .map(|v| !v.as_str().unwrap_or("").trim().is_empty())
                .unwrap_or(false);

            println!(
                "[dingtalk-worker] job_received=true text_len={} has_conversation={} has_webhook={} has_robot_code={}",
                text_len, has_conversation, has_webhook, has_robot_code
            );

            let runtime_clone = runtime.clone();
            let store_clone = store.clone();
            let queue_len_clone = queue_len.clone();

            tokio::spawn(async move {
                process_dingtalk_job(runtime_clone, store_clone, job).await;
                let mut ql = queue_len_clone.write().await;
                *ql = ql.saturating_sub(1);
            });
        }

        println!("[dingtalk-worker] loop_stopped=true reason=receiver_closed");
    });
}

async fn process_dingtalk_job(
    runtime: Arc<GatewayRuntime>,
    store: Arc<DingtalkStore>,
    job: DingtalkAsyncJob,
) {
    let job_id = job.job_id.clone();
    let inbound = job.inbound.clone();

    // Update store: processing
    store.update_status(&job_id, JobStatus::Processing).await;

    // Phase 2: short-circuit commands (help/menu/status/ping/monitor).
    // Non-command text falls through to the agent exactly like Phase 1.
    let command_result = evaluate_command_for_job(&runtime, &job).await;
    let reply = if let Some((cmd, command_reply)) = command_result {
        println!("[dingtalk-command] matched=true command={}", cmd.name());
        command_reply
    } else {
        println!("[dingtalk-command] matched=false command=none");
        let reply = match route_to_agent(&runtime, &inbound).await {
            Ok(r) => r,
            Err(e) => {
                println!("[dingtalk-worker] job_failed=true reason=agent_error");
                store.mark_failed(&job_id, e.to_string()).await;
                return;
            }
        };
        reply
    };

    // Send reply via sessionWebhook
    let result = send_reply_via_session_webhook(&runtime, &inbound, &reply).await;

    match result {
        Ok(_) => {
            println!("[dingtalk-worker] job_completed=true");
            store.update_status(&job_id, JobStatus::Completed).await;
        }
        Err(e) => {
            println!(
                "[dingtalk-worker] job_failed=true reason={}",
                safe_error_kind(&e)
            );
            store.mark_failed(&job_id, e.to_string()).await;
        }
    }
}

/// Try to interpret the inbound text as a DingTalk command. Returns
/// `Some((command, reply))` when the message is a recognized command
/// (the agent must NOT be invoked), or `None` when the message is
/// ordinary text that must continue into the agent pipeline.
///
/// Errors reading the live config or queue length are non-fatal: when
/// the helper cannot render `/status`, the worker falls back to the
/// agent like any other non-command message.
async fn evaluate_command_for_job(
    runtime: &Arc<GatewayRuntime>,
    job: &DingtalkAsyncJob,
) -> Option<(crate::gateway::dingtalk_commands::DingtalkCommand, String)> {
    let config = runtime.get_config().await;
    let queue_len = runtime.dingtalk_queue_len().await;
    let worker_initialized = runtime.is_dingtalk_worker_initialized().await;

    let inputs = crate::gateway::dingtalk_commands::DingtalkStatusInputs {
        config: &config,
        worker_initialized,
        queue_len,
    };

    crate::gateway::dingtalk_commands::evaluate_dingtalk_command(
        &job.inbound.text,
        Some(&job.raw_payload),
        inputs,
    )
}

async fn route_to_agent(
    runtime: &Arc<GatewayRuntime>,
    inbound: &InboundMessage,
) -> Result<String, String> {
    let response = runtime
        .process_inbound(inbound)
        .await
        .map_err(|e| format!("runtime_error: {}", e))?;
    Ok(response.reply)
}

async fn send_reply_via_session_webhook(
    runtime: &Arc<GatewayRuntime>,
    inbound: &InboundMessage,
    reply: &str,
) -> Result<(), String> {
    if reply.trim().is_empty() {
        return Ok(());
    }

    let config = runtime.get_config().await;
    let entry = config.channels_config.dingtalk.as_ref();

    let outbound_mode = {
        let configured = config.gateway.dingtalk.outbound_mode.trim();
        if !configured.is_empty() {
            configured
        } else {
            entry
                .and_then(|entry| entry.extra.get("outbound_mode"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("disabled")
        }
    };

    if outbound_mode == "disabled" {
        println!("[dingtalk-outbound] sending=false mode=disabled");
        return Ok(());
    }
    if outbound_mode == "mock" {
        println!("[dingtalk-outbound] sending=false mode=mock");
        return Ok(());
    }

    let session_webhook = inbound
        .metadata
        .get("sessionWebhook")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let session_webhook_present = session_webhook.is_some();
    let robot_code_present = inbound
        .metadata
        .get("robotCode")
        .and_then(|v| v.as_str())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    let conversation_id_present = inbound.session_id.is_some();

    if let Some(webhook) = session_webhook {
        println!(
            "[dingtalk-outbound] sending=true mode=session_webhook robot_code_present={} conversation_id_present={} webhook_present=true",
            robot_code_present, conversation_id_present
        );
        match send_dingtalk_session_webhook(webhook, reply).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                println!(
                    "[dingtalk-outbound] fallback=true from=session_webhook to=send_from_app reason={}",
                    safe_error_kind(&error)
                );
            }
        }
    } else {
        println!(
            "[dingtalk-outbound] session_webhook_missing fallback=robot_code_required \
             inbound_session_id_present={}",
            inbound.session_id.is_some()
        );
    }

    println!(
        "[dingtalk-outbound] sending=true mode=send_from_app robot_code_present={} conversation_id_present={} webhook_present={}",
        robot_code_present, conversation_id_present, session_webhook_present
    );

    // App key and secret are only needed for the sendFromApp fallback.
    let app_key = crate::gateway::resolve_dingtalk_app_key_for_worker(&config, entry)
        .ok_or_else(|| "missing_app_key".to_string())?;
    let app_secret = crate::gateway::resolve_dingtalk_secret_for_worker(&config, entry)
        .ok_or_else(|| "missing_app_secret".to_string())?;

    let token = match fetch_dingtalk_access_token(&app_key, &app_secret).await {
        Ok(token) => {
            println!("[dingtalk-outbound] access_token_present=true");
            token
        }
        Err(error) => {
            println!("[dingtalk-outbound] access_token_present=false");
            return Err(error);
        }
    };

    let fallback_robot_code = crate::gateway::resolve_dingtalk_robot_code_for_worker(
        &config,
        config.channels_config.dingtalk.as_ref(),
    );

    send_dingtalk_text_message(&token, inbound, fallback_robot_code.as_deref(), reply)
        .await
        .map_err(|error| format!("send_from_app_error:{error}"))
}

async fn send_dingtalk_session_webhook(webhook: &str, text: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(webhook).map_err(|_| "invalid_session_webhook".to_string())?;
    if url.scheme() != "https" || url.host_str() != Some("oapi.dingtalk.com") {
        return Err("invalid_session_webhook_host".to_string());
    }

    let client = dingtalk_http_client()?;
    let response = client
        .post(url)
        .header("Content-Type", "application/json; charset=utf-8")
        .json(&build_session_webhook_payload(text))
        .send()
        .await
        .map_err(|_| {
            println!(
                "[dingtalk-outbound] response status=0 err_code=network_error err_msg_len=0 body_len=0"
            );
            "network_error".to_string()
        })?;

    handle_dingtalk_send_response(response).await
}

/// Fetch DingTalk access token via app_key + app_secret
pub(crate) async fn fetch_dingtalk_access_token(
    app_key: &str,
    app_secret: &str,
) -> Result<String, String> {
    let client = dingtalk_http_client()?;
    let resp = client
        .post("https://api.dingtalk.com/v1.0/oauth2/accessToken")
        .header("Content-Type", "application/json; charset=utf-8")
        .json(&serde_json::json!({
            "appKey": app_key,
            "appSecret": app_secret,
        }))
        .send()
        .await
        .map_err(|_| {
            println!(
                "[dingtalk-outbound] response status=0 err_code=token_network_error err_msg_len=0 body_len=0"
            );
            "token_network_error".to_string()
        })?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|_| "token_read_error".to_string())?;

    if !status.is_success() {
        let summary = summarize_dingtalk_response(status.as_u16(), &body);
        println!(
            "[dingtalk-outbound] response status={} err_code={} err_msg_len={} body_len={}",
            summary.status, summary.err_code, summary.err_msg_len, summary.body_len
        );
        return Err(format!(
            "token_http_error:status={} code={} msg_len={} body_len={}",
            summary.status, summary.err_code, summary.err_msg_len, summary.body_len
        ));
    }

    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|_| "token_parse_error".to_string())?;

    if let Some(token) = json.get("accessToken").and_then(|value| value.as_str()) {
        return Ok(token.to_string());
    }

    let summary = summarize_dingtalk_json(status.as_u16(), body.len(), &json);
    println!(
        "[dingtalk-outbound] response status={} err_code={} err_msg_len={} body_len={}",
        summary.status, summary.err_code, summary.err_msg_len, summary.body_len
    );
    Err(format!(
        "token_error:code={} msg_len={} body_len={}",
        summary.err_code, summary.err_msg_len, summary.body_len
    ))
}

/// Send text message via DingTalk `sendFromApp` API.
///
/// Field mapping (DingTalk enterprise app bot, in-house bot):
/// - `robotCode`     — from `inbound.metadata["robotCode"]` (DingTalk
///                     assigns this to every inbound message). Falls
///                     back to the configured `gateway.dingtalk.robot_code`
///                     when the metadata is missing (older proxies).
/// - `conversationId` — from `inbound.session_id` (the platform
///                     `conversationId`).
/// - `senderStaffId`  — from `inbound.metadata["senderStaffId"]`.
///
/// The signature is intentionally narrow: callers must supply the
/// InboundMessage so we never have to derive these fields from a
/// possibly-leaked session webhook URL.
pub(crate) async fn send_dingtalk_text_message(
    token: &str,
    inbound: &InboundMessage,
    fallback_robot_code: Option<&str>,
    text: &str,
) -> Result<(), String> {
    let client = dingtalk_http_client()?;

    let robot_code = inbound
        .metadata
        .get("robotCode")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| fallback_robot_code.map(str::trim).filter(|s| !s.is_empty()))
        .ok_or_else(|| "missing_robot_code".to_string())?;

    let conversation_id = inbound
        .session_id
        .clone()
        .or_else(|| {
            inbound
                .metadata
                .get("conversationId")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .ok_or_else(|| "missing_conversation_id".to_string())?;

    let sender_staff_id = inbound
        .metadata
        .get("senderStaffId")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);

    // DingTalk expects `msgParam` as a JSON-encoded string, not an
    // object. Serialize the inner content object as a String.
    let body = build_send_from_app_payload(
        robot_code,
        &conversation_id,
        sender_staff_id.as_deref(),
        text,
    );

    let resp = client
        .post("https://api.dingtalk.com/v1.0/im/robot/sendFromApp")
        .header("Content-Type", "application/json")
        .header("x-acs-dingtalk-access-token", token)
        .json(&body)
        .send()
        .await
        .map_err(|_| {
            println!(
                "[dingtalk-outbound] response status=0 err_code=network_error err_msg_len=0 body_len=0"
            );
            "network_error".to_string()
        })?;

    handle_dingtalk_send_response(resp).await
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DingtalkResponseSummary {
    status: u16,
    err_code: String,
    err_msg_len: usize,
    body_len: usize,
    log_id_present: bool,
    success: bool,
}

fn build_session_webhook_payload(text: &str) -> serde_json::Value {
    serde_json::json!({
        "msgtype": "text",
        "text": { "content": text },
    })
}

fn build_send_from_app_payload(
    robot_code: &str,
    conversation_id: &str,
    sender_staff_id: Option<&str>,
    text: &str,
) -> serde_json::Value {
    // DingTalk requires msgParam to be a JSON string, not a nested object.
    let msg_param = serde_json::json!({ "content": text }).to_string();
    let mut body = serde_json::json!({
        "robotCode": robot_code,
        "msgKey": "sampleText",
        "msgParam": msg_param,
        "conversationId": conversation_id,
    });
    if let Some(sender_staff_id) = sender_staff_id {
        body["senderStaffId"] = serde_json::json!(sender_staff_id);
    }
    body
}

fn dingtalk_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(OUTBOUND_TIMEOUT_SECS))
        .build()
        .map_err(|_| "http_client_error".to_string())
}

fn json_string_or_number(value: Option<&serde_json::Value>) -> Option<String> {
    value.and_then(|value| {
        value
            .as_str()
            .map(ToString::to_string)
            .or_else(|| value.as_i64().map(|number| number.to_string()))
            .or_else(|| value.as_u64().map(|number| number.to_string()))
    })
}

fn summarize_dingtalk_json(
    status: u16,
    body_len: usize,
    json: &serde_json::Value,
) -> DingtalkResponseSummary {
    let err_code = json_string_or_number(
        json.get("errCode")
            .or_else(|| json.get("errcode"))
            .or_else(|| json.get("code")),
    )
    .unwrap_or_else(|| "0".to_string());
    let err_msg_len = json
        .get("errMsg")
        .or_else(|| json.get("errmsg"))
        .or_else(|| json.get("message"))
        .and_then(serde_json::Value::as_str)
        .map(|value| value.chars().count())
        .unwrap_or(0);
    let log_id_present = json
        .get("logId")
        .or_else(|| json.get("logid"))
        .or_else(|| json.get("requestId"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    let explicit_success = json.get("success").and_then(serde_json::Value::as_bool);
    let success = (200..300).contains(&status)
        && match explicit_success {
            Some(value) => value,
            None => err_code == "0" || err_code.eq_ignore_ascii_case("ok"),
        };

    DingtalkResponseSummary {
        status,
        err_code,
        err_msg_len,
        body_len,
        log_id_present,
        success,
    }
}

fn summarize_dingtalk_response(status: u16, body: &str) -> DingtalkResponseSummary {
    if body.trim().is_empty() {
        return DingtalkResponseSummary {
            status,
            err_code: if (200..300).contains(&status) {
                "0"
            } else {
                "unknown"
            }
            .to_string(),
            err_msg_len: 0,
            body_len: body.len(),
            log_id_present: false,
            success: (200..300).contains(&status),
        };
    }

    match serde_json::from_str::<serde_json::Value>(body) {
        Ok(json) => summarize_dingtalk_json(status, body.len(), &json),
        Err(_) => DingtalkResponseSummary {
            status,
            err_code: "invalid_json".to_string(),
            err_msg_len: 0,
            body_len: body.len(),
            log_id_present: false,
            success: false,
        },
    }
}

async fn handle_dingtalk_send_response(response: reqwest::Response) -> Result<(), String> {
    let status = response.status().as_u16();
    let body = match response.text().await {
        Ok(body) => body,
        Err(_) => {
            println!(
                "[dingtalk-outbound] response status={} err_code=read_error err_msg_len=0 body_len=0",
                status
            );
            return Err("read_error".to_string());
        }
    };
    let summary = summarize_dingtalk_response(status, &body);
    println!(
        "[dingtalk-outbound] response status={} err_code={} err_msg_len={} body_len={}",
        summary.status, summary.err_code, summary.err_msg_len, summary.body_len
    );

    if summary.success {
        Ok(())
    } else {
        Err(format!(
            "send_error:status={} code={} msg_len={} log_id_present={}",
            summary.status, summary.err_code, summary.err_msg_len, summary.log_id_present
        ))
    }
}

fn safe_error_kind(error: &str) -> &'static str {
    if error.starts_with("invalid_session_webhook") {
        "invalid_session_webhook"
    } else if error.contains("missing_app_key") {
        "missing_app_key"
    } else if error.contains("missing_app_secret") {
        "missing_app_secret"
    } else if error.contains("missing_robot_code") {
        "missing_robot_code"
    } else if error.contains("missing_conversation_id") {
        "missing_conversation_id"
    } else if error.contains("token_") {
        "access_token_error"
    } else if error.contains("network_error") {
        "network_error"
    } else if error.contains("send_error") {
        "platform_error"
    } else if error.contains("read_error") || error.contains("parse_error") {
        "invalid_platform_response"
    } else {
        "outbound_error"
    }
}

// ---------------------------------------------------------------------------
// Signature verification (for use in gateway/mod.rs)
// ---------------------------------------------------------------------------

/// Verify DingTalk webhook signature.
/// Returns Ok(()) if valid, Err(reason) if invalid.
pub(crate) fn verify_dingtalk_signature(
    timestamp: &str,
    sign: &str,
    secret: &str,
) -> Result<(), String> {
    let sign_base = format!("{}\n{}", timestamp, secret);
    let computed = hmac_sha256_base64(&sign_base, secret);
    if computed == sign {
        Ok(())
    } else {
        Err("signature_mismatch".to_string())
    }
}

pub(crate) fn hmac_sha256_base64(data: &str, key: &str) -> String {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let mut mac =
        HmacSha256::new_from_slice(key.as_bytes()).expect("HMAC can take key of any size");
    mac.update(data.as_bytes());
    let result = mac.finalize();
    let bytes = result.into_bytes();

    BASE64.encode(&bytes)
}

/// Extract and verify DingTalk signature from headers/body.
/// Returns Ok(()) if no secret configured (dev mode) or signature valid.
/// Returns Err(reason) if secret configured but signature invalid.
pub(crate) fn verify_dingtalk_webhook_signature(
    headers: &axum::http::HeaderMap,
    raw_body: &str,
    app_secret: Option<&str>,
) -> Result<(), String> {
    let Some(secret) = app_secret else {
        // No secret configured - dev mode, skip verification
        return Ok(());
    };

    // Try dedicated timestamp header first; fall back to shared x-dingtalk-signature
    let timestamp = headers
        .get("timestamp")
        .or_else(|| headers.get("x-dingtalk-signature-for-isv"))
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| "missing_timestamp".to_string())?;

    // Try dedicated sign header first; fall back to shared x-dingtalk-signature
    let sign = headers
        .get("x-dingtalk-signature-for-isv-sign")
        .or_else(|| headers.get("sign"))
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| "missing_sign".to_string())?;

    verify_dingtalk_signature(timestamp, sign, secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_valid_signature() {
        // Known test vectors for HMAC-SHA256 + Base64
        let secret = "test-secret";
        let timestamp = "1234567890";
        let sign_base = format!("{}\n{}", timestamp, secret);

        // Compute expected signature
        let expected = hmac_sha256_base64(&sign_base, secret);

        let result = verify_dingtalk_signature(timestamp, &expected, secret);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_invalid_signature() {
        let secret = "test-secret";
        let timestamp = "1234567890";
        let result = verify_dingtalk_signature(timestamp, "invalid-signature", secret);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("signature_mismatch"));
    }

    #[test]
    fn test_hmac_base64_output_length() {
        // HMAC-SHA256 produces 32 bytes = 44 chars with padding
        let result = hmac_sha256_base64("test-data", "test-key");
        assert!(result.len() >= 40);
    }

    #[test]
    fn test_session_webhook_payload_is_text_message() {
        let body = build_session_webhook_payload("pong");
        assert_eq!(body["msgtype"], "text");
        assert_eq!(body["text"]["content"], "pong");
    }

    #[test]
    fn test_send_from_app_payload_uses_json_string_msg_param() {
        let body = build_send_from_app_payload("robot", "conversation", Some("sender"), "pong");
        assert_eq!(body["msgKey"], "sampleText");
        let msg_param = body["msgParam"]
            .as_str()
            .expect("msgParam must be a string");
        let decoded: serde_json::Value = serde_json::from_str(msg_param).unwrap();
        assert_eq!(decoded["content"], "pong");
    }

    #[test]
    fn test_dingtalk_response_summary_accepts_both_api_success_shapes() {
        let session = summarize_dingtalk_response(200, r#"{"errcode":0,"errmsg":"ok"}"#);
        assert!(session.success);
        assert_eq!(session.err_code, "0");

        let send_from_app = summarize_dingtalk_response(200, r#"{"success":true}"#);
        assert!(send_from_app.success);
    }

    #[test]
    fn test_dingtalk_response_summary_is_structured_and_redacted() {
        let secret = "must-not-appear-in-log-summary";
        let body = format!(
            r#"{{"errCode":"Forbidden","errMsg":"{}","logId":"present"}}"#,
            secret
        );
        let summary = summarize_dingtalk_response(403, &body);
        assert!(!summary.success);
        assert_eq!(summary.status, 403);
        assert_eq!(summary.err_code, "Forbidden");
        assert_eq!(summary.err_msg_len, secret.chars().count());
        assert!(summary.log_id_present);
        assert!(!format!("{summary:?}").contains(secret));
    }

    #[tokio::test]
    async fn test_dingtalk_async_job_creation() {
        let job = DingtalkAsyncJob::new(
            InboundMessage {
                channel: ChannelKind::Dingtalk,
                user_id: Some("user123".to_string()),
                session_id: Some("sess456".to_string()),
                text: "hello".to_string(),
                metadata: Default::default(),
            },
            serde_json::json!({}),
        );

        assert_eq!(job.channel, ChannelKind::Dingtalk);
        assert_eq!(job.inbound.text, "hello");
        assert!(job.job_id.starts_with("dt_job_"));
    }

    #[test]
    fn test_dingtalk_worker_state_try_take_receiver_idempotent() {
        // First take should succeed
        let mut state1 = DingtalkWorkerState::new();
        assert!(!state1.is_initialized());
        let receiver1 = state1.try_take_receiver();
        assert!(receiver1.is_some());
        assert!(state1.is_initialized());

        // Second take should return None (idempotent, no panic)
        let receiver2 = state1.try_take_receiver();
        assert!(receiver2.is_none());
    }

    #[test]
    fn test_dingtalk_worker_state_take_receiver_panics_on_second_call() {
        // take_receiver should panic on second call (same as before, but explicit test)
        let mut state = DingtalkWorkerState::new();
        let _ = state.take_receiver();
        // After first take, try_take_receiver returns None, which means the state is "done"
        // The old take_receiver behavior (expect) would panic - we verify that
        // try_take_receiver is the safe alternative
        let result = state.try_take_receiver();
        assert!(
            result.is_none(),
            "try_take_receiver should return None after initialization"
        );
    }

    #[test]
    fn test_dingtalk_worker_state_new_is_not_initialized() {
        let state = DingtalkWorkerState::new();
        assert!(!state.is_initialized());
    }

    #[test]
    fn test_dingtalk_worker_state_sender_cloned_but_not_taken() {
        // Creating multiple states should each have their own sender
        let state1 = DingtalkWorkerState::new();
        let state2 = DingtalkWorkerState::new();

        // Each state has its own sender, cloning is fine
        let sender1 = state1.sender();
        let sender2 = state2.sender();
        assert!(!sender1.is_closed());
        assert!(!sender2.is_closed());
    }

    #[tokio::test]
    async fn test_dingtalk_worker_sender_and_receiver_share_one_channel() {
        let mut state = DingtalkWorkerState::new();
        let sender = state.sender();
        let mut receiver = state
            .try_take_receiver()
            .expect("receiver should be available once");
        let job = DingtalkAsyncJob::new(
            InboundMessage {
                channel: ChannelKind::Dingtalk,
                user_id: None,
                session_id: Some("conversation".to_string()),
                text: "ping".to_string(),
                metadata: Default::default(),
            },
            serde_json::json!({}),
        );

        sender.send(job).await.unwrap();
        let received = tokio::time::timeout(std::time::Duration::from_secs(1), receiver.recv())
            .await
            .expect("receiver timed out")
            .expect("channel closed");
        assert_eq!(received.inbound.text, "ping");
    }

    #[test]
    fn test_dingtalk_worker_can_share_runtime_queue_counter() {
        let queue_len = Arc::new(RwLock::new(0));
        let state = DingtalkWorkerState::with_queue_len(queue_len.clone());
        assert!(Arc::ptr_eq(&state.queue_len, &queue_len));
    }
}

// =============================================================================
// Config resolvers (re-exports for tests)
// =============================================================================
//
// The actual resolution logic lives in `crate::gateway` so it can access the
// full `Config` and `ChannelEntry`. These wrappers exist so test modules
// (notably `dingtalk_tests`) can call the resolvers through the
// `dingtalk_worker` module path without depending on private items in
// `gateway::mod`.

#[cfg(test)]
pub fn resolve_dingtalk_secret(
    config: &crate::config::Config,
    entry: Option<&crate::config::schema::ChannelEntry>,
) -> Option<String> {
    crate::gateway::resolve_dingtalk_secret_for_test(config, entry)
}

#[cfg(test)]
pub fn resolve_dingtalk_app_key(
    config: &crate::config::Config,
    entry: Option<&crate::config::schema::ChannelEntry>,
) -> Option<String> {
    crate::gateway::resolve_dingtalk_app_key_for_test(config, entry)
}

#[cfg(test)]
pub fn resolve_dingtalk_robot_code(
    config: &crate::config::Config,
    entry: Option<&crate::config::schema::ChannelEntry>,
) -> Option<String> {
    crate::gateway::resolve_dingtalk_robot_code_for_test(config, entry)
}
