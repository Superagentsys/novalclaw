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
    pub fn new(
        inbound: InboundMessage,
        raw_payload: serde_json::Value,
    ) -> Self {
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
}

impl Default for DingtalkWorkerState {
    fn default() -> Self {
        Self::new()
    }
}

impl DingtalkWorkerState {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<DingtalkAsyncJob>(QUEUE_CAPACITY);
        Self {
            sender: Some(sender),
            receiver: Some(receiver),
            queue_len: Arc::new(RwLock::new(0)),
        }
    }

    pub fn sender(&self) -> DingtalkJobSender {
        self.sender.clone().expect("sender already taken")
    }

    pub fn take_receiver(&mut self) -> mpsc::Receiver<DingtalkAsyncJob> {
        self.receiver.take().expect("receiver already taken")
    }

    pub async fn try_enqueue(&self, job: DingtalkAsyncJob) -> Result<(), EnqueueError> {
        let queue_len = self.queue_len.read().await;
        if *queue_len >= QUEUE_CAPACITY {
            return Err(EnqueueError::QueueFull);
        }
        drop(queue_len);

        self.sender().send(job).await.map_err(|_| EnqueueError::QueueFull)?;

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

/// Start the DingTalk worker
pub async fn start_dingtalk_worker(
    mut state: DingtalkWorkerState,
    runtime: Arc<GatewayRuntime>,
    store: Arc<DingtalkStore>,
) {
    let mut receiver = state.take_receiver();
    let queue_len = state.queue_len.clone();

    tokio::spawn(async move {
        while let Some(job) = receiver.recv().await {
            let runtime_clone = runtime.clone();
            let store_clone = store.clone();
            let queue_len_clone = queue_len.clone();

            tokio::spawn(async move {
                process_dingtalk_job(runtime_clone, store_clone, job).await;
                let mut ql = queue_len_clone.write().await;
                *ql = ql.saturating_sub(1);
            });
        }
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
    let reply = if let Some((cmd, command_reply)) = evaluate_command_for_job(&runtime, &job).await {
        println!(
            "[dingtalk-worker] command_reply command={} job_id={}",
            cmd.name(),
            job_id
        );
        command_reply
    } else {
        match route_to_agent(&runtime, &inbound).await {
            Ok(r) => r,
            Err(e) => {
                store.mark_failed(&job_id, e.to_string()).await;
                return;
            }
        }
    };

    // Send reply via sessionWebhook
    let result = send_reply_via_session_webhook(&runtime, &inbound, &reply).await;

    match result {
        Ok(_) => {
            store.update_status(&job_id, JobStatus::Completed).await;
        }
        Err(e) => {
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

    // App key and secret precedence (first non-empty wins):
    //   1. channels_config.dingtalk.extra["app_key"|"app_secret"]
    //   2. gateway.dingtalk.app_key|app_secret  (inline)
    //   3. gateway.dingtalk.app_key_env|app_secret_env  (env var name)
    //   4. OMNINOVA_DINGTALK_APP_KEY|APP_SECRET
    let app_key = crate::gateway::resolve_dingtalk_app_key_for_worker(&config, entry)
        .ok_or_else(|| "missing_app_key".to_string())?;
    let app_secret = crate::gateway::resolve_dingtalk_secret_for_worker(&config, entry)
        .ok_or_else(|| "missing_app_secret".to_string())?;

    // Extract sessionWebhook from inbound metadata
    let session_webhook = inbound
        .metadata
        .get("sessionWebhook")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "missing_session_webhook".to_string())?;

    let token = fetch_dingtalk_access_token(&app_key, &app_secret).await?;

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
        .map(String::from);

    send_dingtalk_text_message(&token, session_webhook, &conversation_id, sender_staff_id.as_deref(), reply)
        .await
        .map_err(|e| format!("session_webhook_error: {}", e))
}

/// Fetch DingTalk access token via app_key + app_secret
pub(crate) async fn fetch_dingtalk_access_token(
    app_key: &str,
    app_secret: &str,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.dingtalk.com/v1.0/oauth2/accessToken")
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "appKey": app_key,
            "appSecret": app_secret,
        }))
        .send()
        .await
        .map_err(|e| format!("network_error: {}", e))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("read_error: {}", e))?;

    if !status.is_success() {
        return Err(format!("http_error: status={} body={}", status.as_u16(), redact_for_log(&body)));
    }

    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("parse_error: {}", e))?;

    json.get("accessToken")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| {
            let err_code = json
                .get("errCode")
                .and_then(|v| v.as_i64())
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let err_msg = json
                .get("errMsg")
                .and_then(|v| v.as_str())
                .unwrap_or("no token in response");
            format!("token_error: code={} msg={}", err_code, err_msg)
        })
}

/// Send text message via DingTalk sessionWebhook API
pub(crate) async fn send_dingtalk_text_message(
    token: &str,
    session_webhook: &str,
    conversation_id: &str,
    sender_staff_id: Option<&str>,
    text: &str,
) -> Result<(), String> {
    let client = reqwest::Client::new();

    let mut body = serde_json::json!({
        "robotCode": session_webhook,
        "topLevelUnitId": conversation_id,
        "msgKey": "sampleText",
        "msgParam": serde_json::json!({
            "content": text,
        }),
    });

    if let Some(staff_id) = sender_staff_id {
        body["senderStaffId"] = serde_json::json!(staff_id);
    }

    let resp = client
        .post("https://api.dingtalk.com/v1.0/im/robot/sendFromApp")
        .header("Content-Type", "application/json")
        .header("x-acs-dingtalk-access-token", token)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("network_error: {}", e))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("read_error: {}", e))?;

    if !status.is_success() {
        return Err(format!(
            "http_error: status={} body={}",
            status.as_u16(),
            redact_for_log(&body)
        ));
    }

    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("parse_error: {}", e))?;

    if json.get("success").and_then(|v| v.as_bool()) == Some(true) {
        Ok(())
    } else {
        let err_code = json
            .get("errCode")
            .and_then(|v| v.as_i64())
            .map(|c| c.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let err_msg = json
            .get("errMsg")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        Err(format!("send_error: code={} msg={}", err_code, err_msg))
    }
}

/// Redact sensitive values from log strings
fn redact_for_log(input: &str) -> String {
    // Truncate long bodies for log safety
    let truncated: String = input.chars().take(200).collect();
    if input.len() > 200 {
        format!("{}…[truncated]", truncated)
    } else {
        truncated
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
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(key.as_bytes())
        .expect("HMAC can take key of any size");
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
    fn test_redact_for_log_truncation() {
        let long = "x".repeat(300);
        let result = redact_for_log(&long);
        assert!(result.len() < 250);
        assert!(result.contains("…[truncated]"));
    }

    #[test]
    fn test_redact_for_log_short() {
        let short = "hello world";
        let result = redact_for_log(short);
        assert_eq!(result, "hello world");
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
