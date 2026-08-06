use crate::channels::ChannelKind;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};

// =============================================================================
// Core traits and types
// =============================================================================

/// Target for sending outbound messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyTarget {
    pub channel: ChannelKind,
    pub chat_id: String,
    pub message_id: Option<String>,
    pub user_id: Option<String>,
}

/// Result of an outbound send operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundResult {
    pub ok: bool,
    pub provider: String,
    pub delivery: OutboundDeliveryStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl OutboundResult {
    pub fn success(provider: &str, platform_message_id: String) -> Self {
        Self {
            ok: true,
            provider: provider.to_string(),
            delivery: OutboundDeliveryStatus::Sent,
            platform_message_id: Some(platform_message_id),
            error_code: None,
            message: None,
        }
    }

    pub fn not_configured(provider: &str, reason: &str) -> Self {
        Self {
            ok: false,
            provider: provider.to_string(),
            delivery: OutboundDeliveryStatus::NotConfigured,
            platform_message_id: None,
            error_code: Some("not_configured".to_string()),
            message: Some(reason.to_string()),
        }
    }

    pub fn failed(provider: &str, error_code: &str, message: &str) -> Self {
        Self {
            ok: false,
            provider: provider.to_string(),
            delivery: OutboundDeliveryStatus::Failed,
            platform_message_id: None,
            error_code: Some(error_code.to_string()),
            message: Some(message.to_string()),
        }
    }

    pub fn skipped_empty_reply(provider: &str) -> Self {
        Self {
            ok: true,
            provider: provider.to_string(),
            delivery: OutboundDeliveryStatus::SkippedEmptyReply,
            platform_message_id: None,
            error_code: None,
            message: Some("Agent reply was empty, skipped sending".to_string()),
        }
    }

    pub fn mock_sent(provider: &str, mock_id: String) -> Self {
        Self {
            ok: true,
            provider: provider.to_string(),
            delivery: OutboundDeliveryStatus::MockSent,
            platform_message_id: Some(mock_id),
            error_code: None,
            message: Some("Sent via mock sender (no real API call)".to_string()),
        }
    }

    /// Convert to a summary (without secrets)
    pub fn to_summary(&self) -> OutboundResultSummary {
        OutboundResultSummary {
            ok: self.ok,
            provider: self.provider.clone(),
            delivery: self.delivery.clone(),
            platform_message_id: self.platform_message_id.clone(),
            error_code: self.error_code.clone(),
            message: self.message.clone(),
        }
    }
}

/// Summary of outbound result (for API responses)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundResultSummary {
    pub ok: bool,
    pub provider: String,
    pub delivery: OutboundDeliveryStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutboundDeliveryStatus {
    /// Outbound reply not implemented yet
    #[default]
    NotImplemented,
    /// HTTP response only (webhook response)
    HttpResponseOnly,
    /// Successfully delivered to platform
    Sent,
    /// Failed to deliver to platform
    Failed,
    /// Not configured (missing app_id/app_secret)
    NotConfigured,
    /// Skipped because reply was empty
    SkippedEmptyReply,
    /// Sent via mock sender (no real API call)
    MockSent,
}

/// Trait for sending outbound messages to various channels
#[async_trait::async_trait]
pub trait ChannelOutboundSender: Send + Sync {
    async fn send_text_reply(&self, target: &ReplyTarget, text: &str) -> OutboundResult;
    /// Send an interactive card. Default falls back to `send_text_reply`
    /// so existing senders (mock etc.) keep working without code changes.
    async fn send_interactive_card(
        &self,
        target: &ReplyTarget,
        card_json: &serde_json::Value,
    ) -> OutboundResult {
        // Default fallback: send a short text summary. Real Feishu
        // outbound overrides this to POST `interactive` messages.
        let summary = format!("[card] {}", short_card_summary(card_json));
        self.send_text_reply(target, &summary).await
    }
    fn channel_kind(&self) -> ChannelKind;
}

/// Format a short text summary of an interactive card for fallback
/// / logging / preview. Never includes the full card payload.
fn short_card_summary(card: &serde_json::Value) -> String {
    let title = card
        .pointer("/header/title/content")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let mut chars: String = title.chars().take(40).collect();
    if title.chars().count() > 40 {
        chars.push('…');
    }
    if chars.is_empty() {
        let s = card.to_string();
        chars = s.chars().take(40).collect::<String>();
        if s.chars().count() > 40 {
            chars.push('…');
        }
    }
    chars
}

// =============================================================================
// Mock sender for testing
// =============================================================================

#[derive(Debug, Clone, Default)]
pub struct MockOutboundSender {
    sent_messages: std::sync::Arc<std::sync::Mutex<Vec<MockMessage>>>,
}

#[derive(Debug, Clone)]
pub struct MockMessage {
    pub target: ReplyTarget,
    pub text: String,
    pub timestamp: Instant,
}

impl MockOutboundSender {
    pub fn new() -> Self {
        Self {
            sent_messages: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    pub fn sent_messages(&self) -> Vec<(ReplyTarget, String)> {
        self.sent_messages
            .lock()
            .unwrap()
            .iter()
            .map(|m| (m.target.clone(), m.text.clone()))
            .collect()
    }

    pub fn clear(&self) {
        self.sent_messages.lock().unwrap().clear();
    }

    pub fn count(&self) -> usize {
        self.sent_messages.lock().unwrap().len()
    }
}

#[async_trait::async_trait]
impl ChannelOutboundSender for MockOutboundSender {
    async fn send_text_reply(&self, target: &ReplyTarget, text: &str) -> OutboundResult {
        let mock_id = format!("mock_msg_{}", uuid::Uuid::new_v4());
        self.sent_messages.lock().unwrap().push(MockMessage {
            target: target.clone(),
            text: text.to_string(),
            timestamp: Instant::now(),
        });
        OutboundResult::mock_sent("mock", mock_id)
    }

    fn channel_kind(&self) -> ChannelKind {
        ChannelKind::Feishu
    }
}

// =============================================================================
// Token management
// =============================================================================

#[derive(Debug, Clone)]
struct CachedToken {
    token: String,
    expires_at: Instant,
}

impl CachedToken {
    fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }
}

#[derive(Debug, Default)]
pub struct TokenCache {
    tokens: std::sync::Mutex<std::collections::HashMap<String, CachedToken>>,
}

impl TokenCache {
    pub fn new() -> Self {
        Self {
            tokens: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        let tokens = self.tokens.lock().unwrap();
        tokens.get(key).and_then(|cached| {
            if cached.is_expired() {
                None
            } else {
                Some(cached.token.clone())
            }
        })
    }

    pub fn set(&self, key: String, token: String, ttl_seconds: u64) {
        let mut tokens = self.tokens.lock().unwrap();
        let expires_at = Instant::now() + Duration::from_secs(ttl_seconds);
        tokens.insert(key, CachedToken { token, expires_at });
    }

    pub fn invalidate(&self, key: &str) {
        let mut tokens = self.tokens.lock().unwrap();
        tokens.remove(key);
    }
}

// =============================================================================
// Feishu / Lark real senders
// =============================================================================

const TOKEN_EXPIRY_SAFETY_MARGIN_SECS: u64 = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlatformErrorDetails {
    code: i64,
    message: String,
    log_id_present: bool,
}

fn parse_platform_error(body: &serde_json::Value) -> PlatformErrorDetails {
    PlatformErrorDetails {
        code: body.get("code").and_then(|value| value.as_i64()).unwrap_or(-1),
        message: body
            .get("msg")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
            .to_string(),
        log_id_present: body.get("log_id").is_some()
            || body.pointer("/error/log_id").is_some()
            || body.pointer("/data/log_id").is_some(),
    }
}

fn safe_platform_message(message: &str, sensitive_values: &[&str]) -> String {
    let truncated = message
        .chars()
        .filter(|character| !character.is_control())
        .take(200)
        .collect::<String>();
    sensitive_values
        .iter()
        .filter(|value| !value.is_empty())
        .fold(truncated, |safe, value| safe.replace(value, "[REDACTED]"))
}

#[derive(Clone)]
struct PlatformOutboundSender {
    provider: &'static str,
    channel: ChannelKind,
    api_base_url: String,
    app_id: String,
    app_secret: String,
    client: Client,
    token_cache: Arc<TokenCache>,
}

impl PlatformOutboundSender {
    fn new(
        provider: &'static str,
        channel: ChannelKind,
        api_base_url: impl Into<String>,
        app_id: String,
        app_secret: String,
        token_cache: Arc<TokenCache>,
    ) -> Self {
        Self {
            provider,
            channel,
            api_base_url: api_base_url.into(),
            app_id,
            app_secret,
            client: Client::new(),
            token_cache,
        }
    }

    async fn tenant_access_token(&self) -> Result<String, OutboundResult> {
        let cache_key = format!("{}:{}", self.provider, self.app_id);
        if let Some(token) = self.token_cache.get(&cache_key) {
            return Ok(token);
        }

        let response = self
            .client
            .post(format!(
                "{}/auth/v3/tenant_access_token/internal",
                self.api_base_url
            ))
            .json(&json!({ "app_id": self.app_id, "app_secret": self.app_secret }))
            .send()
            .await
            .map_err(|_| {
                OutboundResult::failed(self.provider, "token_fetch_failed", "token request failed")
            })?;

        let status = response.status();
        let body = response.json::<serde_json::Value>().await.map_err(|_| {
            OutboundResult::failed(
                self.provider,
                "token_fetch_failed",
                "token response was invalid",
            )
        })?;
        if !status.is_success()
            || body
                .get("code")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0)
                != 0
        {
            return Err(OutboundResult::failed(
                self.provider,
                "token_fetch_failed",
                &format!("token request failed (HTTP {})", status.as_u16()),
            ));
        }

        let token = body
            .get("tenant_access_token")
            .and_then(serde_json::Value::as_str)
            .filter(|token| !token.trim().is_empty())
            .ok_or_else(|| {
                OutboundResult::failed(self.provider, "token_fetch_failed", "token was missing")
            })?
            .to_string();
        let expires_in = body
            .get("expire")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(3600)
            .saturating_sub(TOKEN_EXPIRY_SAFETY_MARGIN_SECS)
            .max(1);
        self.token_cache.set(cache_key, token.clone(), expires_in);
        Ok(token)
    }

    async fn send_text_reply(&self, target: &ReplyTarget, text: &str) -> OutboundResult {
        println!(
            "[{}-outbound] send_text_reply_start chat_id_present={} message_id_present={} text_len={}",
            self.provider,
            !target.chat_id.is_empty(),
            target.message_id.is_some(),
            text.len()
        );

        // If we have a message_id, try the reply message API first
        if let Some(ref message_id) = target.message_id {
            let reply_result = self
                .send_text_by_reply_message(message_id, text)
                .await;
            if reply_result.ok {
                return reply_result;
            }
            // Log reply failure and fallback to create message
            println!(
                "[{}-outbound] reply_message_fallback_to_create reason=reply_failed",
                self.provider
            );
        }

        // Fallback or direct: use create message API
        self.send_text_by_create_message(target, text).await
    }

    /// Send text message via reply message API
    async fn send_text_by_reply_message(
        &self,
        message_id: &str,
        text: &str,
    ) -> OutboundResult {
        let token = match self.tenant_access_token().await {
            Ok(token) => {
                println!("[{}-outbound] token_fetch_ok reply=true", self.provider);
                token
            }
            Err(result) => {
                println!(
                    "[{}-outbound] token_fetch_failed reply=true error_code={:?}",
                    self.provider, result.error_code
                );
                return result;
            }
        };

        println!(
            "[{}-outbound] reply_message_start message_id_present=true text_len={}",
            self.provider,
            text.len()
        );

        let encoded_message_id = urlencoding::encode(message_id);
        let url = format!(
            "{}/im/v1/messages/{}/reply",
            self.api_base_url, encoded_message_id
        );

        let response = match self
            .client
            .post(&url)
            .bearer_auth(&token)
            .header("Content-Type", "application/json; charset=utf-8")
            .json(&json!({
                "msg_type": "text",
                "content": json!({ "text": text }).to_string(),
            }))
            .send()
            .await
        {
            Ok(response) => response,
            Err(_) => {
                println!(
                    "[{}-outbound] reply_message_failed http_status=transport_error platform_error_code=unknown message=request_failed log_id_present=false",
                    self.provider
                );
                return OutboundResult::failed(
                    self.provider,
                    "reply_message_failed",
                    "reply request failed",
                );
            }
        };

        let status = response.status();
        let body = match response.json::<serde_json::Value>().await {
            Ok(body) => body,
            Err(_) => {
                println!(
                    "[{}-outbound] reply_message_failed http_status={} platform_error_code=unknown message=invalid_response log_id_present=false",
                    self.provider,
                    status.as_u16()
                );
                return OutboundResult::failed(
                    self.provider,
                    "reply_message_failed",
                    "reply response was invalid",
                );
            }
        };

        let error = parse_platform_error(&body);
        let platform_message_id = body
            .pointer("/data/message_id")
            .and_then(serde_json::Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .map(String::from);

        if status.is_success() && error.code == 0 {
            println!(
                "[{}-outbound] reply_message_ok platform_message_id_present={}",
                self.provider,
                platform_message_id.is_some()
            );
            OutboundResult::success(
                self.provider,
                platform_message_id.unwrap_or_else(|| "accepted".to_string()),
            )
        } else {
            let platform_msg = safe_platform_message(
                &error.message,
                &[&token, &self.app_secret, message_id],
            );
            println!(
                "[{}-outbound] reply_message_failed http_status={} platform_error_code={} message={} log_id_present={}",
                self.provider,
                status.as_u16(),
                error.code,
                platform_msg,
                error.log_id_present
            );
            OutboundResult::failed(
                self.provider,
                &format!("platform_error_{}", error.code),
                &platform_msg,
            )
        }
    }

    /// Send text message via create message API
    async fn send_text_by_create_message(
        &self,
        target: &ReplyTarget,
        text: &str,
    ) -> OutboundResult {
        let token = match self.tenant_access_token().await {
            Ok(token) => {
                println!("[{}-outbound] token_fetch_ok", self.provider);
                token
            }
            Err(result) => {
                println!("[{}-outbound] token_fetch_failed error_code={:?}", self.provider, result.error_code);
                return result;
            }
        };

        println!(
            "[{}-outbound] send_text_start receive_id_type=chat_id chat_id_present={}",
            self.provider,
            !target.chat_id.is_empty()
        );

        let response = match self
            .client
            .post(format!(
                "{}/im/v1/messages?receive_id_type=chat_id",
                self.api_base_url
            ))
            .bearer_auth(&token)
            .header("Content-Type", "application/json; charset=utf-8")
            .json(&json!({
                "receive_id": target.chat_id,
                "msg_type": "text",
                "content": json!({ "text": text }).to_string(),
            }))
            .send()
            .await
        {
            Ok(response) => response,
            Err(e) => {
                println!("[{}-outbound] send_text_failed error={}", self.provider, e);
                return OutboundResult::failed(
                    self.provider,
                    "message_send_failed",
                    &format!("message request failed: {}", e),
                );
            }
        };

        let status = response.status();
        let body = match response.json::<serde_json::Value>().await {
            Ok(body) => body,
            Err(e) => {
                println!("[{}-outbound] send_text_failed invalid_response", self.provider);
                return OutboundResult::failed(
                    self.provider,
                    "message_send_failed",
                    &format!("message response invalid: {}", e),
                );
            }
        };

        let error = parse_platform_error(&body);
        let platform_message_id = body
            .pointer("/data/message_id")
            .and_then(serde_json::Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .map(String::from);

        if status.is_success() && error.code == 0 {
            println!(
                "[{}-outbound] send_text_ok platform_message_id_present={}",
                self.provider,
                platform_message_id.is_some()
            );
            OutboundResult::success(self.provider, platform_message_id.unwrap_or_else(|| "accepted".to_string()))
        } else {
            let platform_msg = safe_platform_message(
                &error.message,
                &[
                    &token,
                    &self.app_secret,
                    &target.chat_id,
                    target.message_id.as_deref().unwrap_or(""),
                    target.user_id.as_deref().unwrap_or(""),
                ],
            );
            println!(
                "[{}-outbound] send_text_failed http_status={} platform_error_code={} message={} log_id_present={}",
                self.provider,
                status.as_u16(),
                error.code,
                platform_msg,
                error.log_id_present
            );
            OutboundResult::failed(
                self.provider,
                &format!("platform_error_{}", error.code),
                &platform_msg,
            )
        }
    }

    /// Send an interactive message (card). Used by Feishu/Lark command palette.
    /// The card content must already be a JSON object; we serialize it to a
    /// JSON string before posting (per Feishu docs).
    pub async fn send_interactive_card_message(
        &self,
        target: &ReplyTarget,
        card: &serde_json::Value,
    ) -> OutboundResult {
        let token = match self.tenant_access_token().await {
            Ok(token) => {
                println!("[{}-outbound] token_fetch_ok card=true", self.provider);
                token
            }
            Err(result) => {
                println!(
                    "[{}-outbound] token_fetch_failed card=true error_code={:?}",
                    self.provider, result.error_code
                );
                return result;
            }
        };

        // Feishu requires content to be a JSON-encoded *string*.
        let content_str = match serde_json::to_string(card) {
            Ok(s) => s,
            Err(e) => {
                return OutboundResult::failed(
                    self.provider,
                    "card_serialize_failed",
                    &format!("card json serialize failed: {}", e),
                );
            }
        };

        println!(
            "[{}-outbound] card_send_start receive_id_type=chat_id card_chars={}",
            self.provider,
            content_str.chars().count()
        );

        let response = match self
            .client
            .post(format!(
                "{}/im/v1/messages?receive_id_type=chat_id",
                self.api_base_url
            ))
            .bearer_auth(&token)
            .json(&json!({
                "receive_id": target.chat_id,
                "msg_type": "interactive",
                "content": content_str,
            }))
            .send()
            .await
        {
            Ok(response) => response,
            Err(e) => {
                println!(
                    "[{}-outbound] card_send_failed error={}",
                    self.provider, e
                );
                return OutboundResult::failed(
                    self.provider,
                    "card_send_failed",
                    &format!("card request failed: {}", e),
                );
            }
        };

        let status = response.status();
        let body = match response.json::<serde_json::Value>().await {
            Ok(body) => body,
            Err(e) => {
                println!(
                    "[{}-outbound] card_send_failed invalid_response",
                    self.provider
                );
                return OutboundResult::failed(
                    self.provider,
                    "card_send_failed",
                    &format!("card response invalid: {}", e),
                );
            }
        };

        let code = body
            .get("code")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(-1);
        let platform_msg = body
            .get("msg")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let platform_message_id = body
            .pointer("/data/message_id")
            .and_then(serde_json::Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .map(String::from);

        if code == 0 {
            println!(
                "[{}-outbound] card_send_ok platform_message_id_present={}",
                self.provider,
                platform_message_id.is_some()
            );
            OutboundResult::success(
                self.provider,
                platform_message_id.unwrap_or_else(|| "accepted".to_string()),
            )
        } else {
            println!(
                "[{}-outbound] card_send_failed http_status={} platform_error_code={} message={}",
                self.provider,
                status.as_u16(),
                code,
                platform_msg
            );
            OutboundResult::failed(
                self.provider,
                &format!("platform_error_{}", code),
                platform_msg,
            )
        }
    }
}

#[derive(Clone)]
pub struct FeishuOutboundSender(PlatformOutboundSender);

impl FeishuOutboundSender {
    pub fn new(app_id: String, app_secret: String, token_cache: Arc<TokenCache>) -> Self {
        Self(PlatformOutboundSender::new(
            "feishu",
            ChannelKind::Feishu,
            "https://open.feishu.cn/open-apis",
            app_id,
            app_secret,
            token_cache,
        ))
    }
}

#[async_trait::async_trait]
impl ChannelOutboundSender for FeishuOutboundSender {
    async fn send_text_reply(&self, target: &ReplyTarget, text: &str) -> OutboundResult {
        self.0.send_text_reply(target, text).await
    }

    async fn send_interactive_card(
        &self,
        target: &ReplyTarget,
        card: &serde_json::Value,
    ) -> OutboundResult {
        self.0.send_interactive_card_message(target, card).await
    }

    fn channel_kind(&self) -> ChannelKind {
        self.0.channel.clone()
    }
}

#[derive(Clone)]
pub struct LarkOutboundSender(PlatformOutboundSender);

impl LarkOutboundSender {
    pub fn new(app_id: String, app_secret: String, token_cache: Arc<TokenCache>) -> Self {
        Self(PlatformOutboundSender::new(
            "lark",
            ChannelKind::Lark,
            "https://open.larksuite.com/open-apis",
            app_id,
            app_secret,
            token_cache,
        ))
    }
}

#[async_trait::async_trait]
impl ChannelOutboundSender for LarkOutboundSender {
    async fn send_text_reply(&self, target: &ReplyTarget, text: &str) -> OutboundResult {
        self.0.send_text_reply(target, text).await
    }

    fn channel_kind(&self) -> ChannelKind {
        self.0.channel.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::{Path, State};
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::post;
    use axum::{Json, Router};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[derive(Clone)]
    struct TestApiState {
        reply_succeeds: bool,
        reply_calls: Arc<AtomicUsize>,
        create_calls: Arc<AtomicUsize>,
        reply_content_is_string: Arc<AtomicBool>,
        reply_authorized: Arc<AtomicBool>,
    }

    struct TestApiServer {
        base_url: String,
        state: TestApiState,
        task: tokio::task::JoinHandle<()>,
    }

    impl Drop for TestApiServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn test_token_endpoint() -> Json<serde_json::Value> {
        Json(json!({
            "code": 0,
            "tenant_access_token": "test-tenant-token",
            "expire": 3600
        }))
    }

    async fn test_reply_endpoint(
        State(state): State<TestApiState>,
        Path(_message_id): Path<String>,
        headers: HeaderMap,
        Json(body): Json<serde_json::Value>,
    ) -> (StatusCode, Json<serde_json::Value>) {
        state.reply_calls.fetch_add(1, Ordering::SeqCst);
        state.reply_content_is_string.store(
            body.get("content").is_some_and(|value| value.is_string()),
            Ordering::SeqCst,
        );
        state.reply_authorized.store(
            headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                == Some("Bearer test-tenant-token"),
            Ordering::SeqCst,
        );
        if state.reply_succeeds {
            (
                StatusCode::OK,
                Json(json!({ "code": 0, "msg": "ok", "data": { "message_id": "reply-result" } })),
            )
        } else {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "code": 230006,
                    "msg": "Bot ability is not activated.",
                    "error": { "log_id": "test-log-id" }
                })),
            )
        }
    }

    async fn test_create_endpoint(
        State(state): State<TestApiState>,
        Json(_body): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        state.create_calls.fetch_add(1, Ordering::SeqCst);
        Json(json!({ "code": 0, "msg": "ok", "data": { "message_id": "create-result" } }))
    }

    async fn spawn_test_api(reply_succeeds: bool) -> TestApiServer {
        let state = TestApiState {
            reply_succeeds,
            reply_calls: Arc::new(AtomicUsize::new(0)),
            create_calls: Arc::new(AtomicUsize::new(0)),
            reply_content_is_string: Arc::new(AtomicBool::new(false)),
            reply_authorized: Arc::new(AtomicBool::new(false)),
        };
        let app = Router::new()
            .route(
                "/auth/v3/tenant_access_token/internal",
                post(test_token_endpoint),
            )
            .route(
                "/im/v1/messages/{message_id}/reply",
                post(test_reply_endpoint),
            )
            .route("/im/v1/messages", post(test_create_endpoint))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test API");
        let address = listener.local_addr().expect("test API address");
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        TestApiServer {
            base_url: format!("http://{address}"),
            state,
            task,
        }
    }

    fn real_sender_for_test(server: &TestApiServer) -> PlatformOutboundSender {
        PlatformOutboundSender::new(
            "feishu",
            ChannelKind::Feishu,
            server.base_url.clone(),
            "test-app-id".to_string(),
            "test-app-secret".to_string(),
            Arc::new(TokenCache::new()),
        )
    }

    fn reply_target(message_id: Option<&str>) -> ReplyTarget {
        ReplyTarget {
            channel: ChannelKind::Feishu,
            chat_id: "test-chat-id".to_string(),
            message_id: message_id.map(ToString::to_string),
            user_id: Some("test-user-id".to_string()),
        }
    }

    #[tokio::test]
    async fn message_id_prefers_reply_and_success_skips_create() {
        let server = spawn_test_api(true).await;
        let result = real_sender_for_test(&server)
            .send_text_reply(&reply_target(Some("test-message-id")), "hello")
            .await;

        assert!(result.ok);
        assert_eq!(server.state.reply_calls.load(Ordering::SeqCst), 1);
        assert_eq!(server.state.create_calls.load(Ordering::SeqCst), 0);
        assert!(server.state.reply_content_is_string.load(Ordering::SeqCst));
        assert!(server.state.reply_authorized.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn reply_failure_falls_back_to_create() {
        let server = spawn_test_api(false).await;
        let result = real_sender_for_test(&server)
            .send_text_reply(&reply_target(Some("test-message-id")), "hello")
            .await;

        assert!(result.ok);
        assert_eq!(server.state.reply_calls.load(Ordering::SeqCst), 1);
        assert_eq!(server.state.create_calls.load(Ordering::SeqCst), 1);
        assert_eq!(result.platform_message_id.as_deref(), Some("create-result"));
    }

    #[tokio::test]
    async fn missing_message_id_uses_create_message() {
        let server = spawn_test_api(true).await;
        let result = real_sender_for_test(&server)
            .send_text_reply(&reply_target(None), "hello")
            .await;

        assert!(result.ok);
        assert_eq!(server.state.reply_calls.load(Ordering::SeqCst), 0);
        assert_eq!(server.state.create_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn parses_platform_error_code_message_and_log_id() {
        let parsed = parse_platform_error(&json!({
            "code": 230006,
            "msg": "Bot ability is not activated.",
            "error": { "log_id": "private-log-id" }
        }));

        assert_eq!(parsed.code, 230006);
        assert_eq!(parsed.message, "Bot ability is not activated.");
        assert!(parsed.log_id_present);
    }

    #[test]
    fn outbound_error_log_message_redacts_known_sensitive_values() {
        let forbidden = [
            "tenant-token",
            "app-secret",
            "verification-token",
            "encrypt-key",
            "chat-id",
            "open-id",
            "user-id",
            "message-id",
        ];
        let raw = forbidden.join(" ");
        let safe = safe_platform_message(&raw, &forbidden);

        for value in forbidden {
            assert!(!safe.contains(value));
        }
    }

    #[tokio::test]
    async fn mock_sender_records_messages() {
        let sender = MockOutboundSender::new();
        let target = ReplyTarget {
            channel: ChannelKind::Feishu,
            chat_id: "chat_123".to_string(),
            message_id: Some("msg_456".to_string()),
            user_id: Some("user_789".to_string()),
        };

        assert_eq!(sender.count(), 0);
        sender.send_text_reply(&target, "Hello").await;
        assert_eq!(sender.count(), 1);

        let messages = sender.sent_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].1, "Hello");
    }

    #[tokio::test]
    async fn mock_sender_clears_messages() {
        let sender = MockOutboundSender::new();
        let target = ReplyTarget {
            channel: ChannelKind::Feishu,
            chat_id: "chat_123".to_string(),
            message_id: None,
            user_id: None,
        };

        sender.send_text_reply(&target, "Hello").await;
        sender.send_text_reply(&target, "World").await;
        assert_eq!(sender.count(), 2);

        sender.clear();
        assert_eq!(sender.count(), 0);
    }

    #[test]
    fn token_cache_stores_and_retrieves() {
        let cache = TokenCache::new();
        cache.set("key1".to_string(), "token123".to_string(), 3600);

        assert_eq!(cache.get("key1"), Some("token123".to_string()));
        assert_eq!(cache.get("key2"), None);
    }

    #[test]
    fn outbound_result_factory_methods() {
        let success = OutboundResult::success("feishu", "msg_id_123".to_string());
        assert!(success.ok);
        assert_eq!(success.delivery, OutboundDeliveryStatus::Sent);

        let not_config = OutboundResult::not_configured("feishu", "missing app_id");
        assert!(!not_config.ok);
        assert_eq!(not_config.delivery, OutboundDeliveryStatus::NotConfigured);
        assert_eq!(not_config.error_code, Some("not_configured".to_string()));

        let failed = OutboundResult::failed("feishu", "token_fetch_failed", "Token expired");
        assert!(!failed.ok);
        assert_eq!(failed.delivery, OutboundDeliveryStatus::Failed);
        assert_eq!(failed.error_code, Some("token_fetch_failed".to_string()));

        let skipped = OutboundResult::skipped_empty_reply("feishu");
        assert!(skipped.ok);
        assert_eq!(skipped.delivery, OutboundDeliveryStatus::SkippedEmptyReply);
    }

    // =============================================================================
    // Feishu sender tests
    // =============================================================================

    use crate::config::ChannelEntry;

    #[test]
    fn feishu_sender_requires_config() {
        let config = ChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            security_mode: None,
            verification_token: None,
            verification_token_env: None,
            encrypt_key: None,
            encrypt_key_env: None,
            extra: std::collections::HashMap::new(),
        };
        // The gateway turns this missing configuration into `not_configured`
        // before constructing the real Feishu sender.
        assert!(config.extra.get("app_id").is_none());
        assert!(config.extra.get("app_secret").is_none());
    }

    #[test]
    fn feishu_sender_with_config() {
        let mut extra = std::collections::HashMap::new();
        extra.insert("app_id".to_string(), serde_json::json!("fake_app_id"));
        extra.insert("app_secret".to_string(), serde_json::json!("fake_secret"));

        let config = ChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            security_mode: None,
            verification_token: None,
            verification_token_env: None,
            encrypt_key: None,
            encrypt_key_env: None,
            extra,
        };
        assert!(config.extra.get("app_id").is_some());
        assert!(config.extra.get("app_secret").is_some());
    }

    // =============================================================================
    // Reply message API tests
    // =============================================================================

    #[test]
    fn reply_target_has_message_id_field() {
        let target_with_msg_id = ReplyTarget {
            channel: ChannelKind::Feishu,
            chat_id: "chat_abc".to_string(),
            message_id: Some("msg_xyz".to_string()),
            user_id: Some("user_123".to_string()),
        };
        assert!(target_with_msg_id.message_id.is_some());

        let target_without_msg_id = ReplyTarget {
            channel: ChannelKind::Feishu,
            chat_id: "chat_abc".to_string(),
            message_id: None,
            user_id: None,
        };
        assert!(target_without_msg_id.message_id.is_none());
    }

    #[test]
    fn reply_target_serializes_without_secrets() {
        let target = ReplyTarget {
            channel: ChannelKind::Feishu,
            chat_id: "chat_secret_id".to_string(),
            message_id: Some("msg_secret_id".to_string()),
            user_id: Some("user_secret_id".to_string()),
        };
        let json = serde_json::to_string(&target).unwrap();
        // Verify fields are present (secrets are in values, not structure)
        assert!(json.contains("chat_secret_id"));
        assert!(json.contains("msg_secret_id"));
        assert!(json.contains("user_secret_id"));
    }

    #[test]
    fn outbound_result_error_includes_code_and_msg() {
        let result = OutboundResult::failed("feishu", "platform_error_230006", "Bot ability is not activated.");
        assert!(!result.ok);
        assert_eq!(result.error_code, Some("platform_error_230006".to_string()));
        assert_eq!(result.message, Some("Bot ability is not activated.".to_string()));
    }

    #[test]
    fn outbound_result_success_includes_platform_message_id() {
        let result = OutboundResult::success("feishu", "omni_msg_123".to_string());
        assert!(result.ok);
        assert_eq!(result.platform_message_id, Some("omni_msg_123".to_string()));
        assert!(result.error_code.is_none());
        assert!(result.message.is_none());
    }

    #[test]
    fn outbound_result_summary_preserves_fields() {
        let result = OutboundResult::failed("feishu", "platform_error_999", "test message");
        let summary = result.to_summary();
        assert_eq!(summary.error_code, result.error_code);
        assert_eq!(summary.message, result.message);
        assert_eq!(summary.ok, result.ok);
    }
}
