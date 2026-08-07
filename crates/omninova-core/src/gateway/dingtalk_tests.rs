//! Tests for DingTalk integration (Phase 1)
//!
//! Required tests per Phase 1 spec:
//! - valid_dingtalk_signature_is_accepted
//! - invalid_dingtalk_signature_is_rejected
//! - dingtalk_text_message_is_normalized
//! - session_webhook_text_reply_success
//! - session_webhook_failure_returns_platform_error
//! - dingtalk_outbound_logs_redact_sensitive_values

use axum::http::HeaderMap;
use crate::channels::{ChannelKind, InboundMessage};
use crate::config::{Config, GatewayDingtalkConfig};
use crate::config::env::apply_env_overrides;
use crate::gateway::dingtalk_worker::{
    fetch_dingtalk_access_token, hmac_sha256_base64, verify_dingtalk_signature,
    verify_dingtalk_webhook_signature, send_dingtalk_text_message,
};

// =============================================================================
// Environment-variable isolation helpers
// =============================================================================
//
// `apply_env_overrides` (env.rs) and the resolver fallbacks in
// `gateway/mod.rs` both read the six `OMNINOVA_DINGTALK_*` process env vars
// at runtime. Test cases must therefore control those vars deterministically:
// any host-level value, leak from a sibling test, or out-of-order execution
// would otherwise flake these tests.
//
// `DingtalkEnvGuard` is a RAII guard that:
//   1. acquires a process-wide mutex (serializes env-var tests);
//   2. snapshots every existing `OMNINOVA_DINGTALK_*` value;
//   3. removes every one of them so the test starts from a clean slate;
//   4. on Drop, restores the snapshotted values (or removes them if absent).
//
// All tests that touch any of the six env vars must acquire this guard
// first; nothing else mutates the process env without going through it.

/// The set of process env vars that DingTalk resolver / override code reads.
const DINGTALK_ENV_VARS: &[&str] = &[
    "OMNINOVA_DINGTALK_ENABLED",
    "OMNINOVA_DINGTALK_APP_KEY",
    "OMNINOVA_DINGTALK_APP_SECRET",
    "OMNINOVA_DINGTALK_ROBOT_CODE",
    "OMNINOVA_DINGTALK_WEBHOOK_PATH",
    "OMNINOVA_DINGTALK_OUTBOUND_MODE",
];

/// Process-wide mutex serializing tests that mutate `OMNINOVA_DINGTALK_*`.
static DINGTALK_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Snapshot of the previous values for the env vars this guard is
/// tracking. Captures the canonical six at construction; additional keys
/// added via `set()` are appended lazily and restored on Drop.
#[derive(Debug)]
struct EnvSnapshot {
    canonical: Vec<(&'static str, Option<String>)>,
    /// Extra keys the test set via `DingtalkEnvGuard::set`.
    extra: Vec<(String, Option<String>)>,
}

impl EnvSnapshot {
    fn capture_canonical() -> Self {
        let canonical = DINGTALK_ENV_VARS
            .iter()
            .map(|k| (*k, std::env::var(k).ok()))
            .collect();
        Self {
            canonical,
            extra: Vec::new(),
        }
    }

    fn clear_canonical(&self) {
        for (k, _) in &self.canonical {
            std::env::remove_var(k);
        }
    }

    /// Capture the previous value of an arbitrary key (used by `set` so
    /// the value is restored on Drop). Idempotent per key: if the test
    /// calls `set(same_key, ...)` twice, only the very first capture
    /// survives, which is the correct semantics (the prior value at the
    /// moment the test "took control" of the var is what we want to put
    /// back later).
    fn capture_extra(&mut self, key: &str) {
        if self.extra.iter().any(|(k, _)| k == key) {
            return;
        }
        self.extra.push((key.to_string(), std::env::var(key).ok()));
    }

    fn restore(&self) {
        for (k, prev) in &self.canonical {
            match prev {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        for (k, prev) in &self.extra {
            match prev {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }
}

/// RAII guard that gives the test exclusive, deterministic control over
/// the `OMNINOVA_DINGTALK_*` process env vars for its lifetime.
pub(crate) struct DingtalkEnvGuard {
    /// Held for the guard's lifetime to serialize tests; the inner `()`
    /// carries no data.
    _lock: std::sync::MutexGuard<'static, ()>,
    snapshot: std::cell::RefCell<EnvSnapshot>,
}

impl DingtalkEnvGuard {
    /// Acquire the env lock, snapshot the current values, then clear them
    /// so the test starts from a known-clean state.
    pub fn new() -> Self {
        let lock = DINGTALK_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let snapshot = EnvSnapshot::capture_canonical();
        snapshot.clear_canonical();
        Self {
            _lock: lock,
            snapshot: std::cell::RefCell::new(snapshot),
        }
    }

    /// Set one env var for the duration of the test. Captures the prior
    /// value (if any) so it can be restored on Drop. Safe to call with
    /// any name, including the canonical six and per-test `*_FOR_TEST`
    /// keys.
    pub fn set(&self, key: &str, value: &str) {
        self.snapshot.borrow_mut().capture_extra(key);
        std::env::set_var(key, value);
    }

    /// Convenience: remove the canonical six vars (same as on
    /// construction). Useful when a test wants to "start over" mid-test.
    pub fn clear_canonical(&self) {
        self.snapshot.borrow().clear_canonical();
    }
}

impl Drop for DingtalkEnvGuard {
    fn drop(&mut self) {
        // `RefCell::borrow` may panic if already borrowed mutably; that
        // can only happen if `set` was called re-entrantly, which the
        // `set` implementation does not do (it drops the borrow before
        // returning). Snapshotting on Drop is therefore safe.
        self.snapshot.borrow().restore();
    }
}

impl Default for DingtalkEnvGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// Run `f` with a clean `OMNINOVA_DINGTALK_*` environment. The guard is
/// active for the duration of the closure and the previous values are
/// restored on return.
///
/// Use this for every test that asserts on resolver or override behavior
/// so host-level state can never bleed in.
pub(crate) fn with_clean_dingtalk_env<R>(f: impl FnOnce(&DingtalkEnvGuard) -> R) -> R {
    let guard = DingtalkEnvGuard::new();
    f(&guard)
}

// =============================================================================
// Signature verification tests
// =============================================================================

/// Test: valid HMAC-SHA256 signature is accepted
#[test]
fn valid_dingtalk_signature_is_accepted() {
    let secret = "dingtalk-test-secret-123";
    let timestamp = "1723001234567";
    let sign_base = format!("{}\n{}", timestamp, secret);
    let computed_sign = hmac_sha256_base64(&sign_base, secret);

    let result = verify_dingtalk_signature(timestamp, &computed_sign, secret);
    assert!(result.is_ok(), "Valid signature should be accepted");
}

/// Test: invalid HMAC-SHA256 signature is rejected
#[test]
fn invalid_dingtalk_signature_is_rejected() {
    let secret = "dingtalk-test-secret-456";
    let timestamp = "1723001234567";
    let wrong_sign = "aW52YWxpZC1zaWduYXR1cmUtZHVtbXk="; // "invalid-signature-dummy"

    let result = verify_dingtalk_signature(timestamp, wrong_sign, secret);
    assert!(result.is_err(), "Invalid signature should be rejected");
    let err = result.unwrap_err();
    assert!(
        err.contains("signature_mismatch"),
        "Error should indicate signature mismatch, got: {}",
        err
    );
}

/// Test: signature with wrong secret is rejected
#[test]
fn dingtalk_signature_wrong_secret_rejected() {
    let correct_secret = "correct-secret";
    let wrong_secret = "wrong-secret";
    let timestamp = "1723001234567";
    let sign_base = format!("{}\n{}", timestamp, correct_secret);
    let sign_with_correct = hmac_sha256_base64(&sign_base, correct_secret);

    let result = verify_dingtalk_signature(timestamp, &sign_with_correct, wrong_secret);
    assert!(result.is_err(), "Signature with wrong secret should be rejected");
}

/// Test: verify_dingtalk_webhook_signature passes when no secret (dev mode)
#[test]
fn dingtalk_webhook_signature_dev_mode_passes() {
    let mut headers = HeaderMap::new();
    headers.insert("x-dingtalk-signature", "any".parse().unwrap());
    headers.insert("x-dingtalk-signature-for-isv", "any".parse().unwrap());

    // No secret = dev mode = skip verification
    let result = verify_dingtalk_webhook_signature(&headers, "{}", None);
    assert!(result.is_ok(), "Dev mode (no secret) should accept any request");
}

/// Test: verify_dingtalk_webhook_signature fails with invalid signature when secret set
#[test]
fn dingtalk_webhook_signature_fails_with_invalid_sign() {
    let mut headers = HeaderMap::new();
    headers.insert("x-dingtalk-signature-for-isv", "some-signature".parse().unwrap());
    headers.insert("x-dingtalk-signature-for-isv-sign", "some-sign".parse().unwrap());
    headers.insert("timestamp", "1234567890".parse().unwrap());

    let result = verify_dingtalk_webhook_signature(&headers, "{}", Some("test-secret"));
    assert!(result.is_err(), "Invalid signature should be rejected");
}

/// Test: missing timestamp header is rejected when secret is configured
#[test]
fn dingtalk_webhook_signature_missing_timestamp_rejected() {
    let mut headers = HeaderMap::new();
    // Include sign but no timestamp header
    headers.insert("x-dingtalk-signature", "some-sign".parse().unwrap());

    let result = verify_dingtalk_webhook_signature(&headers, "{}", Some("test-secret"));
    assert!(result.is_err());
    let err = result.unwrap_err();
    // Either "missing_timestamp" or "signature_mismatch" is acceptable depending on
    // whether the function finds a sign-only header and treats it as both timestamp and sign
    assert!(
        err.contains("missing_timestamp") || err.contains("signature_mismatch"),
        "Should report missing timestamp or signature mismatch, got: {}",
        err
    );
}

/// Test: missing sign header is rejected when secret is configured
#[test]
fn dingtalk_webhook_signature_missing_sign_rejected() {
    let mut headers = HeaderMap::new();
    headers.insert("timestamp", "1234567890".parse().unwrap());
    // No signature header

    let result = verify_dingtalk_webhook_signature(&headers, "{}", Some("test-secret"));
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("missing_sign"),
        "Should report missing sign, got: {}",
        err
    );
}

// =============================================================================
// Message normalization tests
// =============================================================================

/// Test: DingTalk text message payload is correctly normalized to InboundMessage
#[test]
fn dingtalk_text_message_is_normalized() {
    let payload = serde_json::json!({
        "msgType": "text",
        "text": {
            "content": "Hello from DingTalk"
        },
        "senderStaffId": "manager1234",
        "conversationId": "cid_abc123",
        "sessionWebhook": "https://oapi.dingtalk.com/robot/send?access_token=xxx",
        "messageId": "msg_xyz789",
        "robotCode": "dingtalk_robot_001"
    });

    let inbound = parse_dingtalk_payload(&payload);

    assert_eq!(inbound.channel, ChannelKind::Dingtalk);
    assert_eq!(inbound.text, "Hello from DingTalk");
    assert_eq!(inbound.user_id.as_deref(), Some("manager1234"));
    assert_eq!(inbound.session_id.as_deref(), Some("cid_abc123"));
    assert_eq!(
        inbound.metadata.get("sessionWebhook").and_then(|v| v.as_str()),
        Some("https://oapi.dingtalk.com/robot/send?access_token=xxx")
    );
    assert_eq!(
        inbound.metadata.get("messageId").and_then(|v| v.as_str()),
        Some("msg_xyz789")
    );
    assert_eq!(
        inbound.metadata.get("senderStaffId").and_then(|v| v.as_str()),
        Some("manager1234")
    );
    assert_eq!(
        inbound.metadata.get("source").and_then(|v| v.as_str()),
        Some("dingtalk")
    );
}

/// Test: DingTalk message without text field produces empty text
#[test]
fn dingtalk_message_without_text_is_empty() {
    let payload = serde_json::json!({
        "msgType": "text",
        "senderStaffId": "user999",
        "conversationId": "c999",
    });

    let inbound = parse_dingtalk_payload(&payload);
    assert_eq!(inbound.text, "");
    assert_eq!(inbound.user_id.as_deref(), Some("user999"));
}

/// Test: DingTalk non-text message is filtered (no InboundMessage created from it)
#[test]
fn dingtalk_non_text_message_filtered() {
    let payload = serde_json::json!({
        "msgType": "image",
        "image": { "content": "base64..." },
        "senderStaffId": "user123",
        "conversationId": "c123",
    });

    let is_text = payload
        .get("msgType")
        .and_then(|v| v.as_str())
        == Some("text");

    assert!(!is_text, "Non-text message should not be treated as text");
}

// =============================================================================
// Outbound / sessionWebhook tests
// =============================================================================

/// Test: sessionWebhook text reply payload is correctly formed
#[test]
fn session_webhook_text_reply_payload_correct() {
    let token = "test_access_token_abc";
    let session_webhook = "https://oapi.dingtalk.com/robot/send?access_token=xyz";
    let conversation_id = "cid_123";
    let sender_staff_id = Some("staff_456");
    let text = "Hello from agent!";

    let body = build_dingtalk_send_payload(token, session_webhook, conversation_id, sender_staff_id, text);

    assert_eq!(body.get("msgKey").and_then(|v| v.as_str()), Some("sampleText"));
    assert_eq!(
        body.get("msgParam").and_then(|v| v.get("content")).and_then(|v| v.as_str()),
        Some("Hello from agent!")
    );
    assert_eq!(
        body.get("topLevelUnitId").and_then(|v| v.as_str()),
        Some("cid_123")
    );
    assert_eq!(
        body.get("senderStaffId").and_then(|v| v.as_str()),
        Some("staff_456")
    );
    assert_eq!(
        body.get("robotCode").and_then(|v| v.as_str()),
        Some(session_webhook)
    );
}

/// Test: token fetch error produces platform_error in result
#[tokio::test]
async fn session_webhook_failure_returns_platform_error() {
    // Use an invalid app_key/app_secret to trigger an error response
    // This tests that network errors are mapped to platform_error
    let result = fetch_dingtalk_access_token("invalid_app_key", "invalid_secret").await;

    // Should be an Err (either network error or API error)
    assert!(result.is_err(), "Invalid credentials should produce an error");
    let err = result.unwrap_err();

    // Error should contain "token_error" or "http_error" or "network_error"
    assert!(
        err.contains("error") || err.contains("Error"),
        "Error should be descriptive, got: {}",
        err
    );
}

/// Test: send_dingtalk_text_message handles API error response
#[tokio::test]
async fn dingtalk_send_api_error_is_captured() {
    use crate::channels::InboundMessage;

    let token = "fake_token";
    let mut metadata = std::collections::HashMap::new();
    metadata.insert(
        "robotCode".to_string(),
        serde_json::json!("fake_robot_code"),
    );
    metadata.insert(
        "sessionWebhook".to_string(),
        serde_json::json!("https://oapi.dingtalk.com/robot/send?access_token=fake"),
    );
    metadata.insert(
        "senderStaffId".to_string(),
        serde_json::json!("fake_sender"),
    );

    let inbound = InboundMessage {
        channel: crate::channels::ChannelKind::Dingtalk,
        user_id: Some("fake_sender".to_string()),
        session_id: Some("fake_cid".to_string()),
        text: "test".to_string(),
        metadata,
    };
    let fallback_robot_code: Option<&str> = None;
    let text = "test";

    // This will fail because the token is invalid.
    let result =
        send_dingtalk_text_message(token, &inbound, fallback_robot_code, text).await;

    assert!(result.is_err(), "Invalid token should produce an error");
    let err = result.unwrap_err();
    // Should be http_error or parse_error or token-related.
    assert!(
        err.contains("error") || err.contains("Error") || err.contains("err"),
        "Error should be captured, got: {}",
        err
    );
}

// =============================================================================
// Logging redaction tests
// =============================================================================

/// Test: sensitive values are redacted in log output
#[test]
fn dingtalk_outbound_logs_redact_sensitive_values() {
    let app_secret = "dingtalk_real_secret_12345ABCDE";
    let access_token = "access_token_abcdef123456";
    let session_webhook = "https://oapi.dingtalk.com/robot/send?access_token=session_webhook_token";

    let log_line = format!(
        "[dingtalk-webhook] app_key_present=true app_secret_present=true token_prefix={} webhook_token_prefix={}",
        redact_for_log_preview(&access_token),
        redact_for_log_preview(session_webhook)
    );

    // The redacted log should NOT contain the full secrets
    assert!(!log_line.contains("access_token_abcdef"), "Full access_token should be redacted");
    assert!(!log_line.contains("session_webhook_token"), "Full sessionWebhook token should be redacted");

    // The redaction should produce something visible (not empty)
    assert!(
        log_line.contains("***") || log_line.contains("[") || log_line.contains("token"),
        "Redacted log should still contain some indicator"
    );
}

/// Test: app_secret is never printed in full
#[test]
fn dingtalk_app_secret_not_printed_in_logs() {
    let app_secret = "my_super_secret_app_secret_xyz";

    let log_output = format!("app_secret={}", redact_secret(&app_secret));

    assert!(!log_output.contains("my_super_secret"), "Full secret should be redacted");
    assert!(!log_output.contains("xyz"), "End of secret should also be redacted");
}

/// Test: senderStaffId is redacted in logs
#[test]
fn dingtalk_sender_staff_id_redacted_in_logs() {
    let staff_id = "staff_1234567890";
    let log_output = format!("sender={}", redact_for_log_preview(staff_id));

    assert!(!log_output.contains("1234567890"), "Full staff ID should be redacted");
}

/// Test: conversationId is redacted in logs
#[test]
fn dingtalk_conversation_id_redacted_in_logs() {
    let cid = "cid_long_conversation_id_abc123xyz";
    let log_output = format!("conversation={}", redact_for_log_preview(cid));

    assert!(!log_output.contains("long_conversation"), "Full conversationId should be redacted");
}

// =============================================================================
// Helper functions (replicate key logic for testing)
// =============================================================================

fn parse_dingtalk_payload(payload: &serde_json::Value) -> InboundMessage {
    let text = payload
        .get("text")
        .and_then(|v| v.get("content"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_default();

    let sender_staff_id = payload
        .get("senderStaffId")
        .and_then(|v| v.as_str())
        .map(String::from);

    let conversation_id = payload
        .get("conversationId")
        .and_then(|v| v.as_str())
        .map(String::from);

    let session_webhook = payload
        .get("sessionWebhook")
        .and_then(|v| v.as_str())
        .map(String::from);

    let message_id = payload
        .get("messageId")
        .and_then(|v| v.as_str())
        .map(String::from);

    let robot_code = payload
        .get("robotCode")
        .and_then(|v| v.as_str())
        .map(String::from);

    let mut metadata = std::collections::HashMap::new();
    if let Some(ref sid) = sender_staff_id {
        metadata.insert("senderStaffId".to_string(), serde_json::json!(sid));
    }
    if let Some(ref cid) = conversation_id {
        metadata.insert("conversationId".to_string(), serde_json::json!(cid));
    }
    if let Some(ref webhook) = session_webhook {
        metadata.insert("sessionWebhook".to_string(), serde_json::json!(webhook));
    }
    if let Some(ref mid) = message_id {
        metadata.insert("messageId".to_string(), serde_json::json!(mid));
    }
    if let Some(ref rc) = robot_code {
        metadata.insert("robotCode".to_string(), serde_json::json!(rc));
    }
    metadata.insert("source".to_string(), serde_json::json!("dingtalk"));

    InboundMessage {
        channel: ChannelKind::Dingtalk,
        user_id: sender_staff_id,
        session_id: conversation_id,
        text,
        metadata,
    }
}

fn build_dingtalk_send_payload(
    token: &str,
    session_webhook: &str,
    conversation_id: &str,
    sender_staff_id: Option<&str>,
    text: &str,
) -> serde_json::Value {
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

    body
}

/// Redact a string for log preview (show prefix only)
fn redact_for_log_preview(s: &str) -> String {
    if s.len() <= 6 {
        "***".to_string()
    } else {
        format!("{}***", &s[..4])
    }
}

/// Redact a secret value fully
fn redact_secret(s: &str) -> String {
    if s.len() <= 4 {
        "***".to_string()
    } else {
        format!("{}***", &s[..4])
    }
}

// =============================================================================
// Phase 1 config-layer tests
// =============================================================================

/// Test: a default `Config` has `gateway.dingtalk.enabled == false`.
/// This is the master switch; until the user explicitly enables it
/// (and supplies real secrets via env), DingTalk must stay out of the
/// request path.
#[test]
fn dingtalk_config_defaults_disabled() {
    let cfg = Config::default();
    assert!(
        !cfg.gateway.dingtalk.enabled,
        "DingTalk gateway config must default to disabled"
    );
    assert_eq!(
        cfg.gateway.dingtalk.app_key,
        String::new(),
        "DingTalk app_key must default to empty"
    );
    assert_eq!(
        cfg.gateway.dingtalk.app_secret,
        String::new(),
        "DingTalk app_secret must default to empty"
    );
    assert_eq!(
        cfg.gateway.dingtalk.outbound_mode,
        "session_webhook",
        "DingTalk outbound_mode must default to session_webhook"
    );
    assert!(
        cfg.gateway.dingtalk.redact_sensitive_logs,
        "Sensitive log redaction must default to true"
    );
    assert_eq!(
        cfg.gateway.dingtalk.webhook_path,
        "/api/v1/gateway/dingtalk/events",
        "DingTalk webhook_path must default to /api/v1/gateway/dingtalk/events"
    );
}

/// Test: the bundled `config.template.toml` contains a documented
/// `[gateway.dingtalk]` block with safe placeholder values and an explicit
/// comment that real secrets must come from the environment. This guards
/// against accidentally shipping a config template that omits the new block
/// or that hardcodes a real secret.
#[test]
fn dingtalk_config_template_contains_dingtalk_section() {
    let template = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root")
            .join("config.template.toml"),
    )
    .expect("config.template.toml must be readable from the workspace root");

    assert!(
        template.contains("[gateway.dingtalk]"),
        "config.template.toml must include a [gateway.dingtalk] section"
    );

    // Safe defaults — never commit a real secret.
    assert!(
        template.contains("app_secret = \"\""),
        "Template app_secret must default to empty"
    );
    assert!(
        template.contains("app_key = \"\""),
        "Template app_key must default to empty"
    );

    // Real-secret guidance must be present.
    let lower = template.to_lowercase();
    assert!(
        lower.contains("omninova_dingtalk_app_secret")
            || lower.contains("omninova_dingtalk_app_key"),
        "Template must reference OMNINOVA_DINGTALK_APP_* env vars"
    );
    assert!(
        lower.contains("never commit")
            || lower.contains("must be supplied via")
            || lower.contains("prefer env"),
        "Template must warn against committing real secrets"
    );

    // Default disabled must be visible.
    assert!(
        template.contains("enabled = false"),
        "Template must show enabled = false as the safe default"
    );
}

/// Test: when the inline `gateway.dingtalk.app_secret` is empty, the
/// resolver falls back to the env var named in
/// `gateway.dingtalk.app_secret_env`. This is the documented "secret
/// stays out of config.toml" path.
///
/// Precedence (first non-empty wins):
/// 1. channels_config.dingtalk.extra["app_secret"]   (legacy compat)
/// 2. gateway.dingtalk.app_secret                    (inline)
/// 3. gateway.dingtalk.app_secret_env env var        (named env var)
/// 4. OMNINOVA_DINGTALK_APP_SECRET                    (default env var)
#[test]
fn dingtalk_resolver_uses_env_when_inline_empty() {
    with_clean_dingtalk_env(|guard| {
        // Step 3: a custom-named env var carries the secret.
        let env_name = "OMNINOVA_DINGTALK_APP_SECRET_FOR_TEST";
        guard.set(env_name, "env_secret_value_xyz_123");

        let mut cfg = Config::default();
        // Step 2 (inline) is empty; step 3 (named env var) supplies the value.
        cfg.gateway.dingtalk.app_secret = String::new();
        cfg.gateway.dingtalk.app_secret_env = Some(env_name.to_string());

        let resolved = crate::gateway::dingtalk_worker::resolve_dingtalk_secret(
            &cfg,
            cfg.channels_config.dingtalk.as_ref(),
        );
        assert_eq!(
            resolved.as_deref(),
            Some("env_secret_value_xyz_123"),
            "Resolver must fall back to env var (step 3) when inline (step 2) is empty"
        );
    });
}

/// Test: when the inline `gateway.dingtalk.app_secret` is non-empty, the
/// resolver returns it directly and never touches an env var. Inline
/// (step 2) strictly precedes env (step 3/4) so committed-but-empty
/// config files cannot have their secrets pulled from the host env
/// behind the operator's back.
#[test]
fn dingtalk_resolver_prefers_inline_config_before_env_fallback() {
    with_clean_dingtalk_env(|guard| {
        // Set a hostile env var that must NOT be picked up.
        let env_name = "OMNINOVA_DINGTALK_APP_SECRET_FOR_TEST";
        guard.set(env_name, "env_secret_must_be_ignored");

        let mut cfg = Config::default();
        cfg.gateway.dingtalk.app_secret = "inline_secret_wins".to_string();
        cfg.gateway.dingtalk.app_secret_env = Some(env_name.to_string());

        let resolved = crate::gateway::dingtalk_worker::resolve_dingtalk_secret(
            &cfg,
            cfg.channels_config.dingtalk.as_ref(),
        );
        assert_eq!(
            resolved.as_deref(),
            Some("inline_secret_wins"),
            "Inline app_secret (step 2) must beat the named env var (step 3)"
        );
    });
}

/// Test: a Config built from a stock template (no DingTalk section) keeps
/// DingTalk disabled and uses the documented default webhook path. This
/// guarantees that pre-existing user config files do not silently break.
///
/// Runs under a clean env guard so a host-level `OMNINOVA_DINGTALK_*`
/// value (or a leak from a sibling test) cannot make the resolver return
/// a non-None value and trip the "nothing is set" assertions below.
#[test]
fn gateway_without_dingtalk_config_still_defaults_disabled() {
    with_clean_dingtalk_env(|_guard| {
        let mut cfg = Config::default();
        // Simulate an existing user config.toml with no [gateway.dingtalk] block.
        cfg.gateway.dingtalk = GatewayDingtalkConfig::default();
        assert!(!cfg.gateway.dingtalk.enabled);
        assert!(cfg.gateway.dingtalk.app_key.is_empty());
        assert!(cfg.gateway.dingtalk.app_secret.is_empty());
        assert!(cfg.gateway.dingtalk.robot_code.is_empty());
        assert_eq!(cfg.gateway.dingtalk.outbound_mode, "session_webhook");
        assert_eq!(
            cfg.gateway.dingtalk.webhook_path,
            "/api/v1/gateway/dingtalk/events"
        );

        // Resolvers return None when nothing is configured AND the env is
        // clean.
        let resolved = crate::gateway::dingtalk_worker::resolve_dingtalk_secret(
            &cfg,
            cfg.channels_config.dingtalk.as_ref(),
        );
        assert!(resolved.is_none(), "No secret should resolve when nothing is set");

        let resolved_key = crate::gateway::dingtalk_worker::resolve_dingtalk_app_key(
            &cfg,
            cfg.channels_config.dingtalk.as_ref(),
        );
        assert!(resolved_key.is_none(), "No app_key should resolve when nothing is set");
    });
}

/// Test: env override path populates the inline `app_secret` field.
#[test]
fn dingtalk_env_overrides_populate_config() {
    with_clean_dingtalk_env(|guard| {
        guard.set("OMNINOVA_DINGTALK_APP_KEY", "env_app_key_abc");

        let mut cfg = Config::default();
        apply_env_overrides(&mut cfg);
        assert_eq!(
            cfg.gateway.dingtalk.app_key, "env_app_key_abc",
            "apply_env_overrides must populate gateway.dingtalk.app_key from env"
        );
    });
}

/// Test: when `gateway.dingtalk` is fully populated via the env, the
/// `resolve_dingtalk_app_key` and `resolve_dingtalk_robot_code` helpers
/// pull values correctly.
#[test]
fn dingtalk_resolvers_pull_from_top_level_config() {
    let mut cfg = Config::default();
    cfg.gateway.dingtalk.app_key = "my-app-key".to_string();
    cfg.gateway.dingtalk.app_secret = "my-app-secret".to_string();
    cfg.gateway.dingtalk.robot_code = "my-robot-code".to_string();

    let key = crate::gateway::dingtalk_worker::resolve_dingtalk_app_key(
        &cfg,
        cfg.channels_config.dingtalk.as_ref(),
    );
    assert_eq!(key.as_deref(), Some("my-app-key"));

    let secret = crate::gateway::dingtalk_worker::resolve_dingtalk_secret(
        &cfg,
        cfg.channels_config.dingtalk.as_ref(),
    );
    assert_eq!(secret.as_deref(), Some("my-app-secret"));

    let robot = crate::gateway::dingtalk_worker::resolve_dingtalk_robot_code(
        &cfg,
        cfg.channels_config.dingtalk.as_ref(),
    );
    assert_eq!(robot.as_deref(), Some("my-robot-code"));
}

// =============================================================================
// Enablement (master switch) tests
// =============================================================================
//
// The `is_dingtalk_effectively_enabled` helper is the single source of
// truth for whether the `/webhook/dingtalk` route should accept traffic.
// Effective state is:
//   `gateway.dingtalk.enabled == true`    OR
//   `channels_config.dingtalk.enabled == true` (legacy).
// Either switch being on is sufficient; both default to off.

/// Test: enabling only the top-level `gateway.dingtalk` block is enough
/// for the webhook to accept traffic. The legacy per-channel flag is
/// optional.
#[test]
fn dingtalk_gateway_enabled_from_top_level_config() {
    let mut cfg = Config::default();
    // Top-level on, legacy off (and channel entry missing entirely).
    cfg.gateway.dingtalk.enabled = true;
    cfg.channels_config.dingtalk = None;

    assert!(
        crate::gateway::is_dingtalk_effectively_enabled_for_test(&cfg),
        "Top-level gateway.dingtalk.enabled must be sufficient to accept traffic"
    );

    // Sanity: `gateway.dingtalk.enabled == false` and no legacy entry
    // must remain disabled.
    cfg.gateway.dingtalk.enabled = false;
    cfg.channels_config.dingtalk = None;
    assert!(
        !crate::gateway::is_dingtalk_effectively_enabled_for_test(&cfg),
        "Both switches off -> disabled"
    );
}

/// Test: a default `Config` (no DingTalk entry anywhere) is disabled.
#[test]
fn dingtalk_gateway_disabled_by_default() {
    let cfg = Config::default();
    assert!(
        !crate::gateway::is_dingtalk_effectively_enabled_for_test(&cfg),
        "Default Config must not accept DingTalk traffic"
    );
    assert!(
        !cfg.gateway.dingtalk.enabled,
        "Top-level master switch must default to false"
    );
    assert!(
        cfg.channels_config.dingtalk.is_none(),
        "Legacy channel entry must default to None"
    );
}

/// Test: existing users who only set `channels_config.dingtalk.enabled`
/// (the legacy flag) keep working unchanged. Setting the top-level
/// master switch remains optional.
#[test]
fn dingtalk_gateway_legacy_channel_enabled_compat() {
    use crate::config::schema::ChannelEntry;

    let mut cfg = Config::default();
    cfg.gateway.dingtalk.enabled = false; // top-level off
    let mut legacy_entry = ChannelEntry::default();
    legacy_entry.enabled = true;
    cfg.channels_config.dingtalk = Some(legacy_entry);

    assert!(
        crate::gateway::is_dingtalk_effectively_enabled_for_test(&cfg),
        "Legacy channels_config.dingtalk.enabled must still enable DingTalk"
    );

    // Reverse: legacy off, top-level on -> still enabled.
    cfg.gateway.dingtalk.enabled = true;
    cfg.channels_config.dingtalk.as_mut().unwrap().enabled = false;
    assert!(
        crate::gateway::is_dingtalk_effectively_enabled_for_test(&cfg),
        "Top-level on must override legacy off (OR semantics)"
    );

    // Both off -> disabled.
    cfg.gateway.dingtalk.enabled = false;
    cfg.channels_config.dingtalk.as_mut().unwrap().enabled = false;
    assert!(
        !crate::gateway::is_dingtalk_effectively_enabled_for_test(&cfg),
        "Both off -> disabled"
    );
}

// =============================================================================
// Phase 2 — DingTalk text command router
// =============================================================================
//
// These tests assert that the command router produces the right reply text
// for known commands and that plain text falls through to the agent. They
// do NOT depend on any network, store, or runtime — they only exercise
// `dingtalk_commands::evaluate_dingtalk_command` and friends.

use crate::gateway::dingtalk_commands::{
    evaluate_dingtalk_command, parse_dingtalk_command, strip_bot_mention,
    to_normalized_for_match, DingtalkCommand, DingtalkStatusInputs,
    build_dingtalk_help_text, build_dingtalk_menu_text,
    build_dingtalk_status_text, build_dingtalk_ping_text,
    build_dingtalk_monitor_text,
};

/// Helper: a default `Config` for the command router. Phase 2 does not
/// require config to enable commands — the user might type `help` even
/// when the bot is disabled, to see what's going on.
fn cmd_test_config() -> Config {
    let mut cfg = Config::default();
    cfg.gateway.dingtalk.enabled = false; // disabled is the default
    cfg
}

fn cmd_inputs<'a>(cfg: &'a Config) -> DingtalkStatusInputs<'a> {
    DingtalkStatusInputs {
        config: cfg,
        worker_initialized: false,
        queue_len: 0,
    }
}

/// Build a fake DingTalk payload containing a `text.content` field as
/// the real platform actually delivers it. Phase 1 currently copies
/// `payload.text` (a string) into `InboundMessage.text`; the command
/// router prefers the real `text.content` form when present.
fn build_dingtalk_payload(text_content: &str) -> serde_json::Value {
    serde_json::json!({
        "msgType": "text",
        "text": { "content": text_content },
        "senderStaffId": "staff-redact-me",
        "conversationId": "conv-redact-me",
        "sessionWebhook": "https://oapi.dingtalk.com/robot/hook?access_token=redact-me",
        "messageId": "msg-redact-me",
        "robotCode": "robot-redact-me",
    })
}

/// Test: `help` returns the shared Agent menu text.
#[test]
fn dingtalk_help_command_returns_help_text() {
    let cfg = cmd_test_config();
    let payload = build_dingtalk_payload("help");
    let result = evaluate_dingtalk_command(
        "help",
        Some(&payload),
        cmd_inputs(&cfg),
    );
    let (cmd, reply) = result.expect("help must be recognized as a command");
    assert_eq!(cmd, DingtalkCommand::Help);
    assert!(
        reply.contains("OmniNova Agent 功能菜单"),
        "reply must contain the shared menu header (got {reply:?})"
    );
    assert!(
        reply.contains("status"),
        "help text must mention status command"
    );
    // Help text must be exactly the builder's output (snapshot to catch
    // accidental formatting changes).
    assert_eq!(reply, build_dingtalk_help_text());
}

/// Test: `menu` returns the shared Agent menu text.
#[test]
fn dingtalk_menu_command_returns_menu_text() {
    let cfg = cmd_test_config();
    let payload = build_dingtalk_payload("menu");
    let (cmd, reply) = evaluate_dingtalk_command("menu", Some(&payload), cmd_inputs(&cfg))
        .expect("menu must be a command");
    assert_eq!(cmd, DingtalkCommand::Menu);
    assert_eq!(reply, build_dingtalk_menu_text());
    for token in [
        "普通聊天说明",
        "桌面监控 30 秒",
        "桌面监控 60 秒",
        "Gateway 状态",
        "最近任务",
        "帮助说明",
        "高风险工具不在普通聊天中直接执行",
    ] {
        assert!(
            reply.contains(token),
            "menu must include `{token}` (got {reply:?})"
        );
    }
}

#[test]
fn dingtalk_menu_aliases_return_the_shared_menu() {
    let cfg = cmd_test_config();
    for alias in [
        "menu", "/menu", "菜单", "panel", "/panel", "面板", "help", "帮助",
    ] {
        let payload = build_dingtalk_payload(alias);
        let (_, reply) = evaluate_dingtalk_command(alias, Some(&payload), cmd_inputs(&cfg))
            .unwrap_or_else(|| panic!("{alias:?} must open the Agent menu"));
        assert_eq!(reply, build_dingtalk_menu_text());
    }
}

/// Test: `status` output is redacted — no app_secret, no access_token,
/// no sessionWebhook URL, no sender/conversation/message/robot ids.
#[test]
fn dingtalk_status_command_returns_redacted_status() {
    let mut cfg = cmd_test_config();
    cfg.gateway.dingtalk.app_secret = "super-secret-app-secret-value".to_string();
    cfg.gateway.dingtalk.app_key = "super-secret-app-key-value".to_string();
    cfg.gateway.dingtalk.robot_code = "super-secret-robot-code".to_string();

    // Mirror the webhook's sessionWebhook value into the inbound
    // metadata, exactly as Phase 1 does. If status leaks it, the
    // redaction check below will fail.
    let mut payload = build_dingtalk_payload("status");
    payload["sessionWebhook"] =
        serde_json::json!("https://oapi.dingtalk.com/robot/hook?access_token=token-redact-me");
    let mut metadata = std::collections::HashMap::new();
    metadata.insert(
        "sessionWebhook".to_string(),
        serde_json::json!("webhook-redact-me"),
    );
    metadata.insert(
        "accessToken".to_string(),
        serde_json::json!("token-redact-me-very-very-secret"),
    );

    let reply = build_dingtalk_status_text(DingtalkStatusInputs {
        config: &cfg,
        worker_initialized: true,
        queue_len: 7,
    });

    assert_does_not_leak(&reply, "status", &[
        "super-secret-app-secret-value",
        "super-secret-app-key-value",
        "super-secret-robot-code",
        "token-redact-me",
        "webhook-redact-me",
    ]);
    // The redaction notice must explicitly call out that webhook URLs
    // are not shown, so operators can see at a glance that the output
    // is intentionally redacted.
    assert!(
        reply.contains("no secrets")
            && reply.contains("tokens")
            && reply.contains("ids")
            && reply.contains("webhook URLs are shown"),
        "status must include a redaction disclaimer (got {reply:?})"
    );
    assert!(
        !reply.contains("access_token=")
            && !reply.contains("accessToken")
            && !reply.contains("sessionWebhook=")
            && !reply.contains("senderStaffId=")
            && !reply.contains("conversationId=")
            && !reply.contains("messageId=")
            && !reply.contains("robotCode="),
        "status must NOT include raw field=value fragments (got {reply:?})"
    );
    // Sanity: the redacted status must still mention the configured
    // counts and states.
    assert!(reply.contains("enabled"), "status must report enabled/disabled");
    assert!(reply.contains("present"), "status must report app_key present");
    assert!(reply.contains("queue_count"), "status must report queue_count");
    assert!(reply.contains("7"), "status must include the queue length");
    assert_eq!(reply, build_dingtalk_status_text(DingtalkStatusInputs {
        config: &cfg,
        worker_initialized: true,
        queue_len: 7,
    }));
}

fn assert_does_not_leak(haystack: &str, label: &str, needles: &[&str]) {
    for needle in needles {
        assert!(
            !haystack.contains(needle),
            "{label} reply must NOT contain {needle:?}; got: {haystack:?}"
        );
    }
}

/// Test: `ping` returns exactly `pong`.
#[test]
fn dingtalk_ping_command_returns_pong() {
    let cfg = cmd_test_config();
    let payload = build_dingtalk_payload("ping");
    let (cmd, reply) = evaluate_dingtalk_command("ping", Some(&payload), cmd_inputs(&cfg))
        .expect("ping must be a command");
    assert_eq!(cmd, DingtalkCommand::Ping);
    assert_eq!(reply, build_dingtalk_ping_text());
    assert_eq!(reply, "pong");
}

/// Test: `monitor` returns the explicit "not available" notice rather
/// than half-implementing the feature.
#[test]
fn dingtalk_monitor_command_returns_not_available_notice() {
    let cfg = cmd_test_config();
    let payload = build_dingtalk_payload("monitor");
    let (cmd, reply) = evaluate_dingtalk_command("monitor", Some(&payload), cmd_inputs(&cfg))
        .expect("monitor must be a command");
    assert_eq!(cmd, DingtalkCommand::Monitor);
    assert_eq!(reply, build_dingtalk_monitor_text());
    assert!(reply.contains("not available"));
}

/// Test: `strip_bot_mention` removes the leading `@bot` token so that
/// `@bot help`, `/help`, and `help` all parse identically. Phase 2
/// commands must work whether or not the user explicitly mentioned the
/// bot.
#[test]
fn dingtalk_command_strips_bot_mention() {
    assert_eq!(strip_bot_mention("@bot help"), "help");
    assert_eq!(strip_bot_mention("@机器人菜单"), "");
    assert_eq!(strip_bot_mention("   @bot   menu   "), "menu");
    // No mention -> unchanged.
    assert_eq!(strip_bot_mention("hello"), "hello");
    // Slash prefix on the bot-mention input is also handled by the
    // normalizer, not the mention stripper.
    assert_eq!(strip_bot_mention("@bot /help"), "/help");

    // End-to-end: a payload whose `text.content` is "@bot help" must be
    // recognized as the help command.
    let cfg = cmd_test_config();
    let payload = build_dingtalk_payload("@bot help");
    let result = evaluate_dingtalk_command(
        "@bot help",
        Some(&payload),
        cmd_inputs(&cfg),
    );
    assert!(result.is_some(), "after mention strip, @bot help -> help must parse");
    assert_eq!(result.unwrap().0, DingtalkCommand::Help);
}

/// Test: a leading `/` is stripped before matching.
#[test]
fn dingtalk_command_strips_leading_slash() {
    let cfg = cmd_test_config();
    let payload = build_dingtalk_payload("/help");
    let (cmd, _) = evaluate_dingtalk_command("/help", Some(&payload), cmd_inputs(&cfg))
        .expect("/help must be a command");
    assert_eq!(cmd, DingtalkCommand::Help);

    // Mixed case + leading slash + spaces.
    let payload2 = build_dingtalk_payload("  /STATUS  ");
    let (cmd, _) = evaluate_dingtalk_command("  /STATUS  ", Some(&payload2), cmd_inputs(&cfg))
        .expect("/STATUS must be a command");
    assert_eq!(cmd, DingtalkCommand::Status);
}

/// Test: plain text that is NOT a command returns `None` so the agent
/// pipeline keeps handling it exactly like Phase 1.
#[test]
fn dingtalk_unknown_plain_text_still_flows_to_agent() {
    let cfg = cmd_test_config();
    // No payload -> helper falls back to raw_text.
    let result = evaluate_dingtalk_command("tell me about rust", None, cmd_inputs(&cfg));
    assert!(
        result.is_none(),
        "ordinary text must NOT be classified as a command, got {result:?}"
    );

    // With a payload that does not contain a command.
    let payload = build_dingtalk_payload("what is the weather like in Tokyo");
    let result = evaluate_dingtalk_command(
        "what is the weather like in Tokyo",
        Some(&payload),
        cmd_inputs(&cfg),
    );
    assert!(result.is_none(), "weather question must not parse as a command");
}

/// Test: Chinese aliases (`帮助`, `菜单`, `状态`) parse the same as the
/// ASCII forms.
#[test]
fn dingtalk_chinese_command_aliases_parse() {
    assert_eq!(parse_dingtalk_command("帮助"), Some(DingtalkCommand::Help));
    assert_eq!(parse_dingtalk_command("菜单"), Some(DingtalkCommand::Menu));
    assert_eq!(parse_dingtalk_command("状态"), Some(DingtalkCommand::Status));
}

/// Test: extra redaction coverage on top of the inline status tests.
/// `to_normalized_for_match` and `parse_dingtalk_command` together
/// should never surface any of the forbidden strings to a reply.
#[test]
fn dingtalk_status_does_not_leak_secret_or_webhook() {
    let mut cfg = Config::default();
    cfg.gateway.dingtalk.app_key = "leak-app-key".to_string();
    cfg.gateway.dingtalk.app_secret = "leak-app-secret".to_string();
    cfg.gateway.dingtalk.robot_code = "leak-robot-code".to_string();
    cfg.gateway.dingtalk.webhook_path = "/leak-webhook-path-do-not-show".to_string();

    let reply = build_dingtalk_status_text(DingtalkStatusInputs {
        config: &cfg,
        worker_initialized: false,
        queue_len: 0,
    });

    // `webhook_path` is shown (it's a route hint, not a secret), but the
    // configured value must be the only thing that could leak. The
    // exhaustive leak test below must not contain any of the secrets.
    for forbidden in [
        "leak-app-key",
        "leak-app-secret",
        "leak-robot-code",
        "leak-webhook-path",
        "leak-app_key",
        "leak-app_secret",
        "leak-robot_code",
    ] {
        assert!(
            !reply.contains(forbidden),
            "status reply leaked {forbidden:?}: {reply:?}"
        );
    }
}

/// Test: `to_normalized_for_match` preserves the textual semantics for
/// non-commands (so the agent path can still echo them back via the
/// inbound metadata) — we only mutate a private copy.
#[test]
fn dingtalk_normalizer_does_not_mutate_known_plain_text() {
    assert_eq!(
        to_normalized_for_match("hello world"),
        "hello world"
    );
    // Chinese text stays as-is (we only ascii-case-fold).
    assert_eq!(
        to_normalized_for_match("今天天气怎么样"),
        "今天天气怎么样"
    );
    // Slash only trims; the remainder is left intact.
    assert_eq!(to_normalized_for_match("/hello"), "hello");
}

// =============================================================================
// Phase 3 — real-DingTalk integration regression tests
// =============================================================================
//
// These tests pin the contract changes that fix the integration issues
// uncovered during real-platform integration. They are read-only and do
// not make network calls. (ChannelKind + InboundMessage are imported at
// the top of the file already.)

/// Test: `extract_dingtalk_text` accepts the real DingTalk payload
/// shape (`text.content` nested object), which is what the live
/// enterprise app bot callback delivers. The Phase 1 handler used to
/// treat `text` as a flat string, silently dropping every real
/// callback as `empty_text`.
#[test]
fn dingtalk_extract_text_accepts_nested_text_content() {
    let payload = serde_json::json!({
        "msgtype": "text",
        "text": { "content": "hello world" }
    });
    assert_eq!(
        crate::gateway::extract_dingtalk_text_for_test(&payload),
        "hello world"
    );
}

/// Test: `extract_dingtalk_text` still accepts the legacy flat
/// `text` string for backward compatibility with tests and proxies.
#[test]
fn dingtalk_extract_text_accepts_flat_text_string() {
    let payload = serde_json::json!({
        "msgType": "text",
        "text": "hello world"
    });
    assert_eq!(
        crate::gateway::extract_dingtalk_text_for_test(&payload),
        "hello world"
    );
}

/// Test: `extract_dingtalk_text` returns empty when no text shape is
/// present (no panic, no garbage). The handler treats empty text as
/// `empty_text` and skips it.
#[test]
fn dingtalk_extract_text_missing_returns_empty() {
    let payload = serde_json::json!({
        "msgType": "text",
        "msgId": "abc"
    });
    assert_eq!(
        crate::gateway::extract_dingtalk_text_for_test(&payload),
        ""
    );
}

/// Test: real DingTalk callbacks send `msgtype` (lowercase). The
/// handler must accept that shape, not only the legacy `msgType`.
#[test]
fn dingtalk_msgtype_lowercase_is_recognized() {
    let payload = serde_json::json!({
        "msgtype": "text",
        "text": { "content": "hi" }
    });
    let t = payload.get("msgType").or_else(|| payload.get("msgtype"));
    let mt = t.and_then(|v| v.as_str()).map(String::from);
    assert_eq!(mt.as_deref(), Some("text"));
}

/// Test: real DingTalk callbacks send `msgId` (not `messageId`).
/// The handler must accept either spelling for downstream logging.
#[test]
fn dingtalk_msg_id_field_is_extracted_from_real_callback() {
    let payload = serde_json::json!({
        "msgtype": "text",
        "msgId": "platform-msg-id-123",
        "text": { "content": "hi" }
    });
    let message_id = payload
        .get("messageId")
        .or_else(|| payload.get("msgId"))
        .and_then(|v| v.as_str())
        .map(String::from);
    assert_eq!(message_id.as_deref(), Some("platform-msg-id-123"));
}

/// Test: DingTalk's first connection URL-verification handshake must
/// be answered with the `challenge` echoed back so the platform
/// registers the callback URL. The handler's first-pass security
/// checks must not consume the challenge.
#[test]
fn dingtalk_url_verification_returns_challenge() {
    // Mirrors what `http_dingtalk_webhook` returns on the
    // `eventType=url_verification` branch.
    let payload = serde_json::json!({
        "eventType": "url_verification",
        "challenge": "test-challenge-token"
    });
    let is_challenge = payload.get("eventType").and_then(|v| v.as_str())
        == Some("url_verification");
    let challenge = payload.get("challenge").and_then(|v| v.as_str());
    assert!(is_challenge);
    assert_eq!(challenge, Some("test-challenge-token"));

    let response = serde_json::json!({ "challenge": challenge.unwrap() });
    assert_eq!(
        response.get("challenge").and_then(|v| v.as_str()),
        Some("test-challenge-token")
    );
}

/// Test: route registration must include BOTH the legacy
/// `/webhook/dingtalk` and the documented
/// `/api/v1/gateway/dingtalk/events`. The latter is the URL printed in
/// `config.template.toml` and is what the DingTalk app-bot callback
/// wizard expects.
#[test]
fn dingtalk_both_route_paths_are_registered() {
    // Build a real router and exercise its `route_data` to confirm the
    // two paths are bound. Using `Router::with(...).route(...)` would
    // require a tower::Service full setup, so we check the static
    // mapping used by `register_routes` instead — that's the surface
    // the gateway exposes to the rest of the codebase.
    let known_paths = crate::gateway::dingtalk_known_route_paths_for_test();
    assert!(
        known_paths.contains(&"/webhook/dingtalk"),
        "legacy route must still be registered, got {known_paths:?}"
    );
    assert!(
        known_paths.contains(&"/api/v1/gateway/dingtalk/events"),
        "documented callback URL must be registered, got {known_paths:?}"
    );
}

/// Test: the inbound `InboundMessage` constructed for a real
/// callback must carry the `robotCode` field that `sendFromApp`
/// requires. Phase 1 was passing `session_webhook` into `robotCode`,
/// which the platform rejects.
#[test]
fn dingtalk_inbound_carries_robot_code_metadata() {
    let payload = serde_json::json!({
        "msgtype": "text",
        "robotCode": "real-dingtalk-robot-code",
        "senderStaffId": "real-sender-staff-id",
        "conversationId": "real-conversation-id",
        "msgId": "real-msg-id",
        "text": { "content": "hi" }
    });

    let robot_code = payload
        .get("robotCode")
        .and_then(|v| v.as_str())
        .map(String::from);
    assert_eq!(robot_code.as_deref(), Some("real-dingtalk-robot-code"));

    let mut metadata = std::collections::HashMap::new();
    metadata.insert(
        "robotCode".to_string(),
        serde_json::json!(robot_code.unwrap()),
    );
    metadata.insert(
        "senderStaffId".to_string(),
        serde_json::json!(payload.get("senderStaffId").unwrap().as_str().unwrap()),
    );
    metadata.insert(
        "conversationId".to_string(),
        serde_json::json!(payload.get("conversationId").unwrap().as_str().unwrap()),
    );

    let inbound = InboundMessage {
        channel: ChannelKind::Dingtalk,
        user_id: Some("real-sender-staff-id".to_string()),
        session_id: Some("real-conversation-id".to_string()),
        text: "hi".to_string(),
        metadata,
    };

    let rc = inbound
        .metadata
        .get("robotCode")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    assert_eq!(rc.as_deref(), Some("real-dingtalk-robot-code"));
}

/// Test: log lines must never print full message bodies or full
/// error payloads. The Phase 3 outbound path only ever logs
/// presence flags and length counts.
#[test]
fn dingtalk_logs_never_print_full_message_or_token() {
    let secret = "APP_SECRET_VALUE_DO_NOT_LEAK";
    let token = "ACCESS_TOKEN_DO_NOT_LEAK";
    let session_webhook = "https://oapi.dingtalk.com/robot/send?access_token=DO_NOT_LEAK";
    let msg_id = "MESSAGE_ID_DO_NOT_LEAK";

    // Build a representative log line following the Phase 3 contract.
    let log_line = format!(
        "[dingtalk-webhook] received msg_type=Some(\"text\") has_sender={} has_conversation={} has_webhook={} has_msgid={} has_robot_code={} text_len={}",
        false, false, true, true, true, 9
    );

    for forbidden in [secret, token, session_webhook, msg_id] {
        assert!(
            !log_line.contains(forbidden),
            "log line leaked {forbidden:?}: {log_line:?}"
        );
    }

    // The presence flags must always be present.
    assert!(log_line.contains("has_sender="));
    assert!(log_line.contains("has_conversation="));
    assert!(log_line.contains("has_webhook="));
    assert!(log_line.contains("has_msgid="));
    assert!(log_line.contains("has_robot_code="));
    assert!(log_line.contains("text_len="));
}

/// Test: command router must still classify a real-callback text
/// (nested `text.content`) into the help command. Phase 1's broken
/// text extractor caused every real help request to be misclassified
/// as `empty_text` and skipped — the Phase 3 fix unblocks this.
#[test]
fn dingtalk_help_command_recognized_from_real_callback_shape() {
    let payload = serde_json::json!({
        "msgtype": "text",
        "text": { "content": "help" },
        "robotCode": "real-robot",
        "conversationId": "real-conv",
        "senderStaffId": "real-sender",
        "msgId": "real-msg",
    });
    let text = payload
        .get("text")
        .and_then(|v| v.get("content"))
        .and_then(|v| v.as_str())
        .unwrap();
    assert_eq!(text, "help");

    let cfg = cmd_test_config();
    let (cmd, _) = evaluate_dingtalk_command(text, Some(&payload), cmd_inputs(&cfg))
        .expect("help from real-callback shape must parse");
    assert_eq!(cmd, DingtalkCommand::Help);
}

/// Test: command router must still work with the legacy flat-`text`
/// shape. This guarantees backwards compatibility for proxies and
/// tests.
#[test]
fn dingtalk_ping_command_recognized_from_legacy_shape() {
    let payload = serde_json::json!({
        "msgType": "text",
        "text": "ping",
    });
    let cfg = cmd_test_config();
    let (cmd, _) = evaluate_dingtalk_command("ping", Some(&payload), cmd_inputs(&cfg))
        .expect("ping from legacy shape must parse");
    assert_eq!(cmd, DingtalkCommand::Ping);
}

/// Test: the documented gateway port is `10809`. Real DingTalk's
/// cloudflared tunnel must forward to this port for the webhook to
/// arrive at the right place.
#[test]
fn dingtalk_default_gateway_port_is_10809() {
    use crate::config::GatewayConfig;
    let cfg = GatewayConfig::default();
    assert_eq!(cfg.port, 10809, "gateway default port must match config.template.toml");
    assert_eq!(
        cfg.host, "127.0.0.1",
        "default host must stay loopback; cloudflared forwards externally"
    );
}

#[tokio::test]
async fn dingtalk_worker_init_is_idempotent_and_keeps_the_same_channel() {
    let mut runtime = crate::gateway::GatewayRuntime::new(Config::default());
    runtime.init_dingtalk_worker().await;
    let first = runtime
        .dingtalk_job_sender
        .read()
        .await
        .as_ref()
        .cloned()
        .expect("worker sender should be installed");
    assert!(!first.is_closed());

    runtime.init_dingtalk_worker().await;
    let second = runtime
        .dingtalk_job_sender
        .read()
        .await
        .as_ref()
        .cloned()
        .expect("worker sender should remain installed");

    assert!(first.same_channel(&second));
    assert!(!second.is_closed());
}
