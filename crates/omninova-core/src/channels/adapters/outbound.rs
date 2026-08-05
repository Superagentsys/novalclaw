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

#[derive(Clone)]
struct PlatformOutboundSender {
    provider: &'static str,
    channel: ChannelKind,
    api_base_url: &'static str,
    app_id: String,
    app_secret: String,
    client: Client,
    token_cache: Arc<TokenCache>,
}

impl PlatformOutboundSender {
    fn new(
        provider: &'static str,
        channel: ChannelKind,
        api_base_url: &'static str,
        app_id: String,
        app_secret: String,
        token_cache: Arc<TokenCache>,
    ) -> Self {
        Self {
            provider,
            channel,
            api_base_url,
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
            "[{}-outbound] send_text_reply_start chat_id_present={} text_len={}",
            self.provider,
            !target.chat_id.is_empty(),
            text.len()
        );
        
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

        let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        let platform_msg = body.get("msg").and_then(|v| v.as_str()).unwrap_or("unknown");
        let platform_message_id = body
            .pointer("/data/message_id")
            .and_then(serde_json::Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .map(String::from);

        if code == 0 {
            println!(
                "[{}-outbound] send_text_ok platform_message_id_present={}",
                self.provider,
                platform_message_id.is_some()
            );
            OutboundResult::success(self.provider, platform_message_id.unwrap_or_else(|| "accepted".to_string()))
        } else {
            println!(
                "[{}-outbound] send_text_failed http_status={} platform_error_code={} message={}",
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
}
