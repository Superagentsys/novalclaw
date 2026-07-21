use crate::channels::ChannelKind;
use serde::{Deserialize, Serialize};
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
    /// Successfully delivered to platform (for webhook response)
    Delivered,
    /// Failed to deliver to platform (for webhook response)
    DeliveryFailed,
}

/// Trait for sending outbound messages to various channels
#[async_trait::async_trait]
pub trait ChannelOutboundSender: Send + Sync {
    async fn send_text_reply(&self, target: &ReplyTarget, text: &str) -> OutboundResult;
    fn channel_kind(&self) -> ChannelKind;
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
        OutboundResult::success("mock", mock_id)
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
            extra: std::collections::HashMap::new(),
        };
        // Note: FeishuOutboundSender would need to be implemented separately
        // This test documents the expected behavior
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
            extra,
        };
        assert!(config.extra.get("app_id").is_some());
        assert!(config.extra.get("app_secret").is_some());
    }
}
