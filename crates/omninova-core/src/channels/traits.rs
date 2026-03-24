//! Channel Trait Definitions
//!
//! Core traits for channel adapters that enable multi-platform communication.
//! All channel adapters (Slack, Discord, Email, etc.) must implement the
//! [`Channel`] trait.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::behavior::ChannelBehaviorConfig;
use super::error::ChannelError;
use super::types::{
    ChannelCapabilities, ChannelInfo, ChannelStatus, IncomingMessage,
    OutgoingMessage,
};
use super::ChannelKind;

/// Channel identifier type alias.
pub type ChannelId = String;

/// Message identifier type alias.
pub type MessageId = String;

/// Handler for incoming messages from a channel.
///
/// Implement this trait to handle messages received from channels.
/// The handler is called asynchronously when a message arrives.
///
/// # Example
///
/// ```rust
/// use omninova_core::channels::{MessageHandler, IncomingMessage};
/// use async_trait::async_trait;
///
/// struct MyHandler;
///
/// #[async_trait]
/// impl MessageHandler for MyHandler {
///     async fn handle(&self, message: IncomingMessage) {
///         println!("Received message: {:?}", message);
///     }
/// }
/// ```
#[async_trait]
pub trait MessageHandler: Send + Sync {
    /// Handle an incoming message.
    async fn handle(&self, message: IncomingMessage);
}

/// Factory trait for creating channel instances.
///
/// Implement this trait to provide a factory for creating channel adapters.
/// Factories are registered with [`ChannelRegistry`] to enable dynamic
/// channel creation based on configuration.
///
/// # Example
///
/// ```rust
/// use omninova_core::channels::{ChannelFactory, ChannelConfig, Channel, ChannelError, ChannelKind};
///
/// struct SlackFactory;
///
/// impl ChannelFactory for SlackFactory {
///     fn channel_kind(&self) -> ChannelKind {
///         ChannelKind::Slack
///     }
///
///     fn create(&self, config: ChannelConfig) -> Result<Box<dyn Channel>, ChannelError> {
///         // Create and return a Slack channel instance
///         todo!()
///     }
/// }
/// ```
pub trait ChannelFactory: Send + Sync {
    /// Get the channel kind this factory creates.
    fn channel_kind(&self) -> ChannelKind;

    /// Create a new channel instance with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid or the channel
    /// cannot be created.
    fn create(&self, config: ChannelConfig) -> Result<Box<dyn Channel>, ChannelError>;
}

/// Core channel trait — implement for any communication platform adapter.
///
/// This trait defines the interface for all channel adapters, enabling
/// unified communication across different platforms like Slack, Discord,
/// Email, Telegram, and more.
///
/// # Lifecycle
///
/// 1. Create a channel instance via [`ChannelFactory::create`]
/// 2. Call [`connect`] to establish connection
/// 3. Use [`send_message`] and handle incoming messages
/// 4. Call [`disconnect`] when done
///
/// # Thread Safety
///
/// All channel implementations must be `Send + Sync` to support
/// concurrent access from multiple threads.
///
/// # Example
///
/// ```rust
/// use omninova_core::channels::{Channel, ChannelConfig, ChannelStatus, ChannelKind, MessageHandler, ChannelCapabilities};
/// use omninova_core::channels::{IncomingMessage, OutgoingMessage, MessageId, ChannelError};
/// use async_trait::async_trait;
///
/// struct MyChannel {
///     id: String,
///     status: ChannelStatus,
/// }
///
/// #[async_trait]
/// impl Channel for MyChannel {
///     fn id(&self) -> &str {
///         &self.id
///     }
///
///     fn channel_kind(&self) -> ChannelKind {
///         ChannelKind::Webhook
///     }
///
///     async fn connect(&mut self) -> Result<(), ChannelError> {
///         self.status = ChannelStatus::Connected;
///         Ok(())
///     }
///
///     async fn disconnect(&mut self) -> Result<(), ChannelError> {
///         self.status = ChannelStatus::Disconnected;
///         Ok(())
///     }
///
///     async fn send_message(
///         &self,
///         _message: OutgoingMessage,
///     ) -> Result<MessageId, ChannelError> {
///         Ok("msg_123".to_string())
///     }
///
///     fn get_status(&self) -> ChannelStatus {
///         self.status.clone()
///     }
///
///     fn capabilities(&self) -> ChannelCapabilities {
///         ChannelCapabilities::TEXT | ChannelCapabilities::RICH_TEXT
///     }
///
///     fn set_message_handler(&mut self, _handler: Box<dyn MessageHandler>) {
///         // No-op for this example
///     }
/// }
/// ```
#[async_trait]
pub trait Channel: Send + Sync {
    /// Get the unique identifier for this channel instance.
    fn id(&self) -> &str;

    /// Get the channel kind (Slack, Discord, Email, etc.).
    fn channel_kind(&self) -> ChannelKind;

    /// Connect to the channel service.
    ///
    /// This method should establish the connection to the external service
    /// and prepare the channel for sending and receiving messages.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Connection fails (network issues, service unavailable)
    /// - Authentication fails (invalid credentials)
    /// - Configuration is invalid
    ///
    /// # Idempotency
    ///
    /// Implementations should handle being called multiple times gracefully.
    /// If already connected, return `Ok(())`.
    async fn connect(&mut self) -> Result<(), ChannelError>;

    /// Disconnect from the channel service.
    ///
    /// This method should cleanly close the connection and release
    /// any resources held by the channel.
    ///
    /// # Errors
    ///
    /// Returns an error if the disconnection fails unexpectedly.
    ///
    /// # Idempotency
    ///
    /// Implementations should handle being called when already disconnected.
    /// If already disconnected, return `Ok(())`.
    async fn disconnect(&mut self) -> Result<(), ChannelError>;

    /// Send a message through this channel.
    ///
    /// # Arguments
    ///
    /// * `message` - The message to send
    ///
    /// # Returns
    ///
    /// The message ID assigned by the channel service.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Channel is not connected
    /// - Message is invalid or too large
    /// - Rate limit is exceeded
    /// - Network error occurs
    async fn send_message(&self, message: OutgoingMessage) -> Result<MessageId, ChannelError>;

    /// Get the current connection status.
    fn get_status(&self) -> ChannelStatus;

    /// Get the capabilities supported by this channel.
    ///
    /// Capabilities describe what features the channel supports,
    /// such as rich text, file attachments, threads, reactions, etc.
    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities::TEXT
    }

    /// Set the message handler for incoming messages.
    ///
    /// The handler will be called whenever a message is received
    /// from this channel.
    fn set_message_handler(&mut self, handler: Box<dyn MessageHandler>);

    /// Get detailed channel information including status and statistics.
    ///
    /// Provides a snapshot of the channel's current state.
    fn get_info(&self) -> ChannelInfo {
        let mut info = ChannelInfo::new(self.id(), "", self.channel_kind());
        info.status = self.get_status();
        info.capabilities = self.capabilities();
        info
    }

    /// Check if the channel supports a specific capability.
    fn has_capability(&self, capability: ChannelCapabilities) -> bool {
        self.capabilities().contains(capability)
    }

    /// Check if the channel is currently connected.
    fn is_connected(&self) -> bool {
        matches!(self.get_status(), ChannelStatus::Connected)
    }

    /// Callback invoked when behavior configuration changes at runtime.
    ///
    /// Implementations can override this to react to behavior changes
    /// without requiring a channel restart.
    ///
    /// # Arguments
    ///
    /// * `config` - The new behavior configuration
    ///
    /// # Default Implementation
    ///
    /// The default implementation does nothing. Override this method
    /// to implement custom behavior change handling.
    fn on_behavior_changed(&mut self, _config: &ChannelBehaviorConfig) {
        // Default: no-op
    }
}

// ============================================================================
// Channel Configuration
// ============================================================================

/// Configuration for creating a channel instance.
///
/// This struct contains all the information needed to create and
/// configure a channel adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    /// Unique identifier for this channel instance.
    pub id: String,

    /// Human-readable display name.
    pub name: String,

    /// Kind of channel (Slack, Discord, etc.).
    pub channel_kind: ChannelKind,

    /// Authentication credentials.
    pub credentials: super::types::Credentials,

    /// Channel-specific settings.
    #[serde(default)]
    pub settings: ChannelSettings,

    /// Whether this channel is enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl ChannelConfig {
    /// Create a new channel configuration.
    pub fn new(id: impl Into<String>, channel_kind: ChannelKind, credentials: super::types::Credentials) -> Self {
        Self {
            id: id.into(),
            name: String::new(),
            channel_kind,
            credentials,
            settings: ChannelSettings::default(),
            enabled: true,
        }
    }

    /// Set the display name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set channel-specific settings.
    pub fn with_settings(mut self, settings: ChannelSettings) -> Self {
        self.settings = settings;
        self
    }

    /// Set whether the channel is enabled.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

// ============================================================================
// Channel Settings
// ============================================================================

/// Channel-specific configuration settings.
///
/// These settings control channel behavior and can be customized
/// per channel instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelSettings {
    /// Maximum message length (0 = no limit).
    #[serde(default)]
    pub max_message_length: usize,

    /// Enable message threading support.
    #[serde(default)]
    pub enable_threads: bool,

    /// Enable file attachment support.
    #[serde(default)]
    pub enable_files: bool,

    /// Enable image support.
    #[serde(default)]
    pub enable_images: bool,

    /// Custom rate limit (messages per minute, 0 = no limit).
    #[serde(default)]
    pub rate_limit: u32,

    /// Retry attempts for failed messages.
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: u32,

    /// Timeout for operations in seconds.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Channel behavior configuration.
    #[serde(default)]
    pub behavior: ChannelBehaviorConfig,

    /// Additional channel-specific settings as JSON.
    #[serde(default)]
    pub extra: serde_json::Value,
}

impl Default for ChannelSettings {
    fn default() -> Self {
        Self {
            max_message_length: 0,
            enable_threads: false,
            enable_files: false,
            enable_images: false,
            rate_limit: 0,
            retry_attempts: default_retry_attempts(),
            timeout_secs: default_timeout(),
            behavior: ChannelBehaviorConfig::default(),
            extra: serde_json::Value::Null,
        }
    }
}

fn default_retry_attempts() -> u32 {
    3
}

fn default_timeout() -> u64 {
    30
}

impl ChannelSettings {
    /// Create default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum message length.
    pub fn with_max_message_length(mut self, length: usize) -> Self {
        self.max_message_length = length;
        self
    }

    /// Enable threading support.
    pub fn with_threads(mut self, enable: bool) -> Self {
        self.enable_threads = enable;
        self
    }

    /// Enable file support.
    pub fn with_files(mut self, enable: bool) -> Self {
        self.enable_files = enable;
        self
    }

    /// Enable image support.
    pub fn with_images(mut self, enable: bool) -> Self {
        self.enable_images = enable;
        self
    }

    /// Set rate limit.
    pub fn with_rate_limit(mut self, limit: u32) -> Self {
        self.rate_limit = limit;
        self
    }

    /// Set timeout in seconds.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Set channel behavior configuration.
    pub fn with_behavior(mut self, behavior: ChannelBehaviorConfig) -> Self {
        self.behavior = behavior;
        self
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::Credentials;

    // ============================================================================
    // ChannelConfig Tests
    // ============================================================================

    #[test]
    fn test_channel_config_new() {
        let config =
            ChannelConfig::new("slack-123", ChannelKind::Slack, Credentials::BotToken { token: "token".to_string() });
        assert_eq!(config.id, "slack-123");
        assert_eq!(config.channel_kind, ChannelKind::Slack);
        assert!(config.enabled);
        assert!(config.name.is_empty());
    }

    #[test]
    fn test_channel_config_builder() {
        let config = ChannelConfig::new("discord-456", ChannelKind::Discord, Credentials::BotToken { token: "token".to_string() })
            .with_name("My Discord Server")
            .with_enabled(false);

        assert_eq!(config.id, "discord-456");
        assert_eq!(config.name, "My Discord Server");
        assert!(!config.enabled);
    }

    #[test]
    fn test_channel_config_serialize_deserialize() {
        let config = ChannelConfig::new("test-123", ChannelKind::Slack, Credentials::BotToken { token: "token".to_string() })
            .with_name("Test Channel");

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ChannelConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "test-123");
        assert_eq!(deserialized.name, "Test Channel");
        assert_eq!(deserialized.channel_kind, ChannelKind::Slack);
    }

    // ============================================================================
    // ChannelSettings Tests
    // ============================================================================

    #[test]
    fn test_channel_settings_default() {
        let settings = ChannelSettings::new();
        assert_eq!(settings.max_message_length, 0);
        assert!(!settings.enable_threads);
        assert!(!settings.enable_files);
        assert!(!settings.enable_images);
        assert_eq!(settings.rate_limit, 0);
        assert_eq!(settings.retry_attempts, 3);
        assert_eq!(settings.timeout_secs, 30);
    }

    #[test]
    fn test_channel_settings_builder() {
        let settings = ChannelSettings::new()
            .with_max_message_length(4000)
            .with_threads(true)
            .with_files(true)
            .with_images(true)
            .with_rate_limit(10)
            .with_timeout(60);

        assert_eq!(settings.max_message_length, 4000);
        assert!(settings.enable_threads);
        assert!(settings.enable_files);
        assert!(settings.enable_images);
        assert_eq!(settings.rate_limit, 10);
        assert_eq!(settings.timeout_secs, 60);
    }

    // ============================================================================
    // Mock Channel for Testing
    // ============================================================================

    struct MockChannel {
        id: String,
        status: ChannelStatus,
        capabilities: ChannelCapabilities,
        message_handler: Option<Box<dyn MessageHandler>>,
    }

    impl MockChannel {
        fn new(id: impl Into<String>) -> Self {
            Self {
                id: id.into(),
                status: ChannelStatus::Disconnected,
                capabilities: ChannelCapabilities::TEXT | ChannelCapabilities::RICH_TEXT,
                message_handler: None,
            }
        }

        fn with_capabilities(mut self, caps: ChannelCapabilities) -> Self {
            self.capabilities = caps;
            self
        }
    }

    #[async_trait]
    impl Channel for MockChannel {
        fn id(&self) -> &str {
            &self.id
        }

        fn channel_kind(&self) -> ChannelKind {
            ChannelKind::Webhook
        }

        async fn connect(&mut self) -> Result<(), ChannelError> {
            if matches!(self.status, ChannelStatus::Connected) {
                return Ok(());
            }
            self.status = ChannelStatus::Connecting;
            // Simulate connection
            self.status = ChannelStatus::Connected;
            Ok(())
        }

        async fn disconnect(&mut self) -> Result<(), ChannelError> {
            self.status = ChannelStatus::Disconnected;
            Ok(())
        }

        async fn send_message(
            &self,
            _message: OutgoingMessage,
        ) -> Result<MessageId, ChannelError> {
            if !self.is_connected() {
                return Err(ChannelError::NotConnected);
            }
            Ok(format!("msg_{}", uuid::Uuid::new_v4()))
        }

        fn get_status(&self) -> ChannelStatus {
            self.status.clone()
        }

        fn capabilities(&self) -> ChannelCapabilities {
            self.capabilities
        }

        fn set_message_handler(&mut self, handler: Box<dyn MessageHandler>) {
            self.message_handler = Some(handler);
        }
    }

    #[tokio::test]
    async fn test_mock_channel_connect() {
        let mut channel = MockChannel::new("test-1");
        assert_eq!(channel.get_status(), ChannelStatus::Disconnected);

        channel.connect().await.unwrap();
        assert_eq!(channel.get_status(), ChannelStatus::Connected);
        assert!(channel.is_connected());
    }

    #[tokio::test]
    async fn test_mock_channel_disconnect() {
        let mut channel = MockChannel::new("test-1");
        channel.connect().await.unwrap();

        channel.disconnect().await.unwrap();
        assert_eq!(channel.get_status(), ChannelStatus::Disconnected);
        assert!(!channel.is_connected());
    }

    #[tokio::test]
    async fn test_mock_channel_send_not_connected() {
        let channel = MockChannel::new("test-1");
        let message = OutgoingMessage::text("test-1", "Hello");

        let result = channel.send_message(message).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ChannelError::NotConnected));
    }

    #[tokio::test]
    async fn test_mock_channel_send_success() {
        let mut channel = MockChannel::new("test-1");
        channel.connect().await.unwrap();

        let message = OutgoingMessage::text("test-1", "Hello");

        let result = channel.send_message(message).await;
        assert!(result.is_ok());
        let msg_id = result.unwrap();
        assert!(msg_id.starts_with("msg_"));
    }

    #[test]
    fn test_mock_channel_capabilities() {
        let channel = MockChannel::new("test-1")
            .with_capabilities(ChannelCapabilities::TEXT | ChannelCapabilities::FILES);

        assert!(channel.has_capability(ChannelCapabilities::TEXT));
        assert!(channel.has_capability(ChannelCapabilities::FILES));
        assert!(!channel.has_capability(ChannelCapabilities::THREADS));
    }

    #[test]
    fn test_mock_channel_get_info() {
        let channel = MockChannel::new("test-1");
        let info = channel.get_info();

        assert_eq!(info.id, "test-1");
        assert_eq!(info.kind, ChannelKind::Webhook);
        assert_eq!(info.status, ChannelStatus::Disconnected);
    }

    // ============================================================================
    // ChannelFactory Tests
    // ============================================================================

    struct MockFactory;

    impl ChannelFactory for MockFactory {
        fn channel_kind(&self) -> ChannelKind {
            ChannelKind::Webhook
        }

        fn create(&self, config: ChannelConfig) -> Result<Box<dyn Channel>, ChannelError> {
            Ok(Box::new(MockChannel::new(config.id)))
        }
    }

    #[test]
    fn test_factory_channel_kind() {
        let factory = MockFactory;
        assert_eq!(factory.channel_kind(), ChannelKind::Webhook);
    }

    #[test]
    fn test_factory_create() {
        let factory = MockFactory;
        let config = ChannelConfig::new("test-123", ChannelKind::Webhook, Credentials::None);
        let channel = factory.create(config).unwrap();

        assert_eq!(channel.id(), "test-123");
        assert_eq!(channel.channel_kind(), ChannelKind::Webhook);
    }
}