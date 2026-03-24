//! Channel Types
//!
//! Core type definitions for channels including messages, statuses,
//! capabilities, and credentials.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::ChannelKind;

// ============================================================================
// Channel Status
// ============================================================================

/// Connection status of a channel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelStatus {
    /// Not connected
    Disconnected,
    /// Attempting to connect
    Connecting,
    /// Successfully connected
    Connected,
    /// Connection error
    Error,
}

impl Default for ChannelStatus {
    fn default() -> Self {
        Self::Disconnected
    }
}

impl ChannelStatus {
    /// Check if the channel is connected
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }

    /// Check if the channel is in an error state
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error)
    }
}

// ============================================================================
// Channel Capabilities
// ============================================================================

bitflags::bitflags! {
    /// Channel capability flags
    ///
    /// Describes what features a channel supports.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct ChannelCapabilities: u32 {
        /// Supports plain text messages
        const TEXT = 0b0000_0000_0001;
        /// Supports rich text / Markdown
        const RICH_TEXT = 0b0000_0000_0010;
        /// Supports file attachments
        const FILES = 0b0000_0000_0100;
        /// Supports images
        const IMAGES = 0b0000_0000_1000;
        /// Supports message threads
        const THREADS = 0b0000_0001_0000;
        /// Supports message replies
        const REPLIES = 0b0000_0010_0000;
        /// Supports editing messages
        const EDIT = 0b0000_0100_0000;
        /// Supports deleting messages
        const DELETE = 0b0000_1000_0000;
        /// Supports @ mentions
        const MENTIONS = 0b0001_0000_0000;
        /// Supports emoji reactions
        const REACTIONS = 0b0010_0000_0000;
        /// Supports direct messages
        const DIRECT_MESSAGE = 0b0100_0000_0000;
        /// Supports channel messages
        const CHANNEL_MESSAGE = 0b1000_0000_0000;
    }
}

impl Default for ChannelCapabilities {
    fn default() -> Self {
        Self::TEXT
    }
}

impl ChannelCapabilities {
    /// Create capabilities with only text support
    pub fn text_only() -> Self {
        Self::TEXT
    }

    /// Create capabilities for a full-featured chat platform
    pub fn full_chat() -> Self {
        Self::TEXT
            | Self::RICH_TEXT
            | Self::FILES
            | Self::IMAGES
            | Self::THREADS
            | Self::REPLIES
            | Self::MENTIONS
            | Self::REACTIONS
            | Self::DIRECT_MESSAGE
            | Self::CHANNEL_MESSAGE
    }

    /// Create capabilities for email
    pub fn email() -> Self {
        Self::TEXT | Self::RICH_TEXT | Self::FILES | Self::IMAGES
    }

    /// Check if rich text is supported
    pub fn supports_rich_text(&self) -> bool {
        self.contains(Self::RICH_TEXT)
    }

    /// Check if files are supported
    pub fn supports_files(&self) -> bool {
        self.contains(Self::FILES)
    }

    /// Check if images are supported
    pub fn supports_images(&self) -> bool {
        self.contains(Self::IMAGES)
    }

    /// Check if threads are supported
    pub fn supports_threads(&self) -> bool {
        self.contains(Self::THREADS)
    }

    /// Check if mentions are supported
    pub fn supports_mentions(&self) -> bool {
        self.contains(Self::MENTIONS)
    }
}

// ============================================================================
// Credentials
// ============================================================================

/// Authentication credentials for different channel types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Credentials {
    /// Bot token authentication (Slack, Discord, Telegram)
    BotToken {
        /// The bot token
        token: String,
    },
    /// OAuth2 authentication
    OAuth2 {
        /// OAuth2 access token
        access_token: String,
        /// Optional refresh token
        refresh_token: Option<String>,
        /// Token expiration timestamp (Unix)
        expires_at: Option<i64>,
    },
    /// Username/password authentication (Email)
    UsernamePassword {
        /// Username
        username: String,
        /// Password
        password: String,
    },
    /// API key authentication
    ApiKey {
        /// API key
        key: String,
        /// Optional API secret
        secret: Option<String>,
    },
    /// Webhook URL
    Webhook {
        /// Webhook URL
        url: String,
        /// Optional secret for signature verification
        secret: Option<String>,
    },
    /// No authentication required
    None,
}

impl Default for Credentials {
    fn default() -> Self {
        Self::None
    }
}

// ============================================================================
// Message Types
// ============================================================================

/// Unique identifier for a message
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

impl MessageId {
    /// Create a new message ID
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Generate a new random message ID
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for MessageId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for MessageId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Message sender information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSender {
    /// Unique sender ID in the channel
    pub id: String,
    /// Display name
    pub name: Option<String>,
    /// Username/handle
    pub username: Option<String>,
    /// Profile picture URL
    pub avatar_url: Option<String>,
    /// Whether this is a bot
    pub is_bot: bool,
}

impl MessageSender {
    /// Create a new message sender
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: None,
            username: None,
            avatar_url: None,
            is_bot: false,
        }
    }

    /// Set the display name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the username
    pub fn with_username(mut self, username: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self
    }

    /// Mark as a bot
    pub fn as_bot(mut self) -> Self {
        self.is_bot = true;
        self
    }
}

/// Rich text format type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RichTextFormat {
    /// Markdown format
    Markdown,
    /// HTML format
    Html,
}

impl Default for RichTextFormat {
    fn default() -> Self {
        Self::Markdown
    }
}

/// Message content types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageContent {
    /// Plain text message
    Text {
        /// The text content
        text: String,
    },
    /// Rich text message (Markdown or HTML)
    RichText {
        /// The text content
        text: String,
        /// Format type
        format: RichTextFormat,
    },
    /// File attachment
    File {
        /// Filename
        filename: String,
        /// MIME type
        content_type: String,
        /// URL to download the file
        url: Option<String>,
        /// Raw file data (if available)
        data: Option<Vec<u8>>,
    },
    /// Image
    Image {
        /// Image URL
        url: String,
        /// Alt text for accessibility
        alt_text: Option<String>,
    },
    /// Composite content with multiple parts
    Composite {
        /// Content parts
        parts: Vec<MessageContent>,
    },
}

impl MessageContent {
    /// Create a text message
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// Create a markdown message
    pub fn markdown(text: impl Into<String>) -> Self {
        Self::RichText {
            text: text.into(),
            format: RichTextFormat::Markdown,
        }
    }

    /// Create an HTML message
    pub fn html(text: impl Into<String>) -> Self {
        Self::RichText {
            text: text.into(),
            format: RichTextFormat::Html,
        }
    }

    /// Get the text content if this is a text or rich text message
    pub fn text_content(&self) -> Option<&str> {
        match self {
            Self::Text { text } | Self::RichText { text, .. } => Some(text),
            _ => None,
        }
    }
}

/// Incoming message from a channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingMessage {
    /// Unique message ID
    pub id: MessageId,
    /// Channel instance ID
    pub channel_id: String,
    /// Channel type
    pub channel_kind: ChannelKind,
    /// Message sender
    pub sender: MessageSender,
    /// Message content
    pub content: MessageContent,
    /// Message timestamp (Unix)
    pub timestamp: i64,
    /// Thread ID if part of a thread
    pub thread_id: Option<String>,
    /// Parent message ID if this is a reply
    pub reply_to: Option<MessageId>,
    /// Additional metadata
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl IncomingMessage {
    /// Create a new incoming message
    pub fn new(
        channel_id: impl Into<String>,
        channel_kind: ChannelKind,
        sender: MessageSender,
        content: MessageContent,
    ) -> Self {
        Self {
            id: MessageId::generate(),
            channel_id: channel_id.into(),
            channel_kind,
            sender,
            content,
            timestamp: chrono::Utc::now().timestamp(),
            thread_id: None,
            reply_to: None,
            metadata: HashMap::new(),
        }
    }

    /// Set the thread ID
    pub fn in_thread(mut self, thread_id: impl Into<String>) -> Self {
        self.thread_id = Some(thread_id.into());
        self
    }

    /// Set the parent message this replies to
    pub fn reply_to(mut self, message_id: MessageId) -> Self {
        self.reply_to = Some(message_id);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

/// Outgoing message to a channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingMessage {
    /// Message content
    pub content: MessageContent,
    /// Target channel ID or user ID
    pub target_id: String,
    /// Thread ID to post in
    pub thread_id: Option<String>,
    /// Message to reply to
    pub reply_to: Option<MessageId>,
    /// Additional metadata
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl OutgoingMessage {
    /// Create a new outgoing message
    pub fn new(target_id: impl Into<String>, content: MessageContent) -> Self {
        Self {
            content,
            target_id: target_id.into(),
            thread_id: None,
            reply_to: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a text message
    pub fn text(target_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(target_id, MessageContent::text(text))
    }

    /// Create a markdown message
    pub fn markdown(target_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(target_id, MessageContent::markdown(text))
    }

    /// Set the thread ID
    pub fn in_thread(mut self, thread_id: impl Into<String>) -> Self {
        self.thread_id = Some(thread_id.into());
        self
    }

    /// Set the message to reply to
    pub fn reply_to(mut self, message_id: MessageId) -> Self {
        self.reply_to = Some(message_id);
        self
    }
}

// ============================================================================
// Channel Info
// ============================================================================

/// Channel information and statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInfo {
    /// Channel instance ID
    pub id: String,
    /// Channel name
    pub name: String,
    /// Channel type
    pub kind: ChannelKind,
    /// Connection status
    pub status: ChannelStatus,
    /// Channel capabilities
    pub capabilities: ChannelCapabilities,
    /// Total messages sent
    pub messages_sent: u64,
    /// Total messages received
    pub messages_received: u64,
    /// Last activity timestamp
    pub last_activity: Option<i64>,
    /// Error message if in error state
    pub error_message: Option<String>,
}

impl ChannelInfo {
    /// Create new channel info
    pub fn new(id: impl Into<String>, name: impl Into<String>, kind: ChannelKind) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            status: ChannelStatus::default(),
            capabilities: ChannelCapabilities::default(),
            messages_sent: 0,
            messages_received: 0,
            last_activity: None,
            error_message: None,
        }
    }
}

use std::fmt;

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // ChannelStatus Tests
    // ============================================================================

    #[test]
    fn test_channel_status_default() {
        let status = ChannelStatus::default();
        assert_eq!(status, ChannelStatus::Disconnected);
    }

    #[test]
    fn test_channel_status_is_connected() {
        assert!(ChannelStatus::Connected.is_connected());
        assert!(!ChannelStatus::Disconnected.is_connected());
        assert!(!ChannelStatus::Connecting.is_connected());
        assert!(!ChannelStatus::Error.is_connected());
    }

    #[test]
    fn test_channel_status_is_error() {
        assert!(ChannelStatus::Error.is_error());
        assert!(!ChannelStatus::Connected.is_error());
    }

    #[test]
    fn test_channel_status_serialize() {
        let status = ChannelStatus::Connected;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"connected\"");
    }

    #[test]
    fn test_channel_status_deserialize() {
        let status: ChannelStatus = serde_json::from_str("\"connecting\"").unwrap();
        assert_eq!(status, ChannelStatus::Connecting);
    }

    // ============================================================================
    // ChannelCapabilities Tests
    // ============================================================================

    #[test]
    fn test_capabilities_default() {
        let caps = ChannelCapabilities::default();
        assert!(caps.contains(ChannelCapabilities::TEXT));
        assert!(!caps.contains(ChannelCapabilities::RICH_TEXT));
    }

    #[test]
    fn test_capabilities_text_only() {
        let caps = ChannelCapabilities::text_only();
        assert!(caps.contains(ChannelCapabilities::TEXT));
        assert_eq!(caps.bits(), 1);
    }

    #[test]
    fn test_capabilities_full_chat() {
        let caps = ChannelCapabilities::full_chat();
        assert!(caps.contains(ChannelCapabilities::TEXT));
        assert!(caps.contains(ChannelCapabilities::RICH_TEXT));
        assert!(caps.contains(ChannelCapabilities::FILES));
        assert!(caps.contains(ChannelCapabilities::THREADS));
        assert!(caps.contains(ChannelCapabilities::MENTIONS));
    }

    #[test]
    fn test_capabilities_email() {
        let caps = ChannelCapabilities::email();
        assert!(caps.contains(ChannelCapabilities::TEXT));
        assert!(caps.contains(ChannelCapabilities::RICH_TEXT));
        assert!(caps.contains(ChannelCapabilities::FILES));
        assert!(!caps.contains(ChannelCapabilities::THREADS));
    }

    #[test]
    fn test_capabilities_helper_methods() {
        let caps = ChannelCapabilities::full_chat();
        assert!(caps.supports_rich_text());
        assert!(caps.supports_files());
        assert!(caps.supports_images());
        assert!(caps.supports_threads());
        assert!(caps.supports_mentions());

        let caps_text = ChannelCapabilities::text_only();
        assert!(!caps_text.supports_rich_text());
        assert!(!caps_text.supports_files());
    }

    #[test]
    fn test_capabilities_combine() {
        let caps = ChannelCapabilities::TEXT | ChannelCapabilities::FILES | ChannelCapabilities::IMAGES;
        assert!(caps.contains(ChannelCapabilities::TEXT));
        assert!(caps.contains(ChannelCapabilities::FILES));
        assert!(caps.contains(ChannelCapabilities::IMAGES));
        assert!(!caps.contains(ChannelCapabilities::THREADS));
    }

    // ============================================================================
    // Credentials Tests
    // ============================================================================

    #[test]
    fn test_credentials_bot_token() {
        let creds = Credentials::BotToken {
            token: "xoxb-123".to_string(),
        };
        let json = serde_json::to_string(&creds).unwrap();
        assert!(json.contains("bot_token"));
    }

    #[test]
    fn test_credentials_oauth2() {
        let creds = Credentials::OAuth2 {
            access_token: "access123".to_string(),
            refresh_token: Some("refresh456".to_string()),
            expires_at: Some(1234567890),
        };
        let json = serde_json::to_string(&creds).unwrap();
        assert!(json.contains("o_auth2"));
        assert!(json.contains("access123"));
    }

    #[test]
    fn test_credentials_webhook() {
        let creds = Credentials::Webhook {
            url: "https://example.com/hook".to_string(),
            secret: Some("secret123".to_string()),
        };
        let json = serde_json::to_string(&creds).unwrap();
        assert!(json.contains("webhook"));
    }

    // ============================================================================
    // MessageId Tests
    // ============================================================================

    #[test]
    fn test_message_id_new() {
        let id = MessageId::new("msg-123");
        assert_eq!(id.0, "msg-123");
    }

    #[test]
    fn test_message_id_generate() {
        let id = MessageId::generate();
        assert!(!id.0.is_empty());
        // UUID format: 8-4-4-4-12
        assert_eq!(id.0.len(), 36);
    }

    #[test]
    fn test_message_id_display() {
        let id = MessageId::new("test-id");
        assert_eq!(format!("{}", id), "test-id");
    }

    #[test]
    fn test_message_id_from_str() {
        let id: MessageId = "msg-456".into();
        assert_eq!(id.0, "msg-456");
    }

    // ============================================================================
    // MessageSender Tests
    // ============================================================================

    #[test]
    fn test_message_sender_new() {
        let sender = MessageSender::new("user-123");
        assert_eq!(sender.id, "user-123");
        assert!(sender.name.is_none());
        assert!(sender.username.is_none());
        assert!(!sender.is_bot);
    }

    #[test]
    fn test_message_sender_builder() {
        let sender = MessageSender::new("user-123")
            .with_name("John Doe")
            .with_username("johndoe")
            .as_bot();
        assert_eq!(sender.name, Some("John Doe".to_string()));
        assert_eq!(sender.username, Some("johndoe".to_string()));
        assert!(sender.is_bot);
    }

    // ============================================================================
    // MessageContent Tests
    // ============================================================================

    #[test]
    fn test_message_content_text() {
        let content = MessageContent::text("Hello world");
        assert_eq!(content.text_content(), Some("Hello world"));
    }

    #[test]
    fn test_message_content_markdown() {
        let content = MessageContent::markdown("# Heading\n\nSome **bold** text");
        match &content {
            MessageContent::RichText { text, format } => {
                assert!(text.contains("Heading"));
                assert_eq!(*format, RichTextFormat::Markdown);
            }
            _ => panic!("Expected RichText"),
        }
        assert_eq!(content.text_content(), Some("# Heading\n\nSome **bold** text"));
    }

    #[test]
    fn test_message_content_html() {
        let content = MessageContent::html("<p>Hello</p>");
        match &content {
            MessageContent::RichText { text, format } => {
                assert_eq!(text, "<p>Hello</p>");
                assert_eq!(*format, RichTextFormat::Html);
            }
            _ => panic!("Expected RichText"),
        }
    }

    #[test]
    fn test_message_content_image() {
        let content = MessageContent::Image {
            url: "https://example.com/img.png".to_string(),
            alt_text: Some("An image".to_string()),
        };
        assert!(content.text_content().is_none());
    }

    #[test]
    fn test_message_content_file() {
        let content = MessageContent::File {
            filename: "doc.pdf".to_string(),
            content_type: "application/pdf".to_string(),
            url: Some("https://example.com/doc.pdf".to_string()),
            data: None,
        };
        assert!(content.text_content().is_none());
    }

    // ============================================================================
    // IncomingMessage Tests
    // ============================================================================

    #[test]
    fn test_incoming_message_new() {
        let sender = MessageSender::new("user-1").with_name("Test User");
        let content = MessageContent::text("Hello");
        let msg = IncomingMessage::new("channel-1", ChannelKind::Slack, sender.clone(), content.clone());

        assert!(!msg.id.0.is_empty());
        assert_eq!(msg.channel_id, "channel-1");
        assert_eq!(msg.channel_kind, ChannelKind::Slack);
        assert_eq!(msg.sender.id, "user-1");
        assert!(msg.thread_id.is_none());
        assert!(msg.reply_to.is_none());
    }

    #[test]
    fn test_incoming_message_builder() {
        let sender = MessageSender::new("user-1");
        let content = MessageContent::text("Hello");
        let parent_id = MessageId::new("msg-parent");

        let msg = IncomingMessage::new("channel-1", ChannelKind::Discord, sender, content)
            .in_thread("thread-123")
            .reply_to(parent_id.clone())
            .with_metadata("custom", serde_json::json!("value"));

        assert_eq!(msg.thread_id, Some("thread-123".to_string()));
        assert_eq!(msg.reply_to, Some(parent_id));
        assert_eq!(msg.metadata.get("custom"), Some(&serde_json::json!("value")));
    }

    // ============================================================================
    // OutgoingMessage Tests
    // ============================================================================

    #[test]
    fn test_outgoing_message_new() {
        let content = MessageContent::text("Hello");
        let msg = OutgoingMessage::new("channel-1", content);

        assert_eq!(msg.target_id, "channel-1");
        assert!(msg.thread_id.is_none());
        assert!(msg.reply_to.is_none());
    }

    #[test]
    fn test_outgoing_message_text() {
        let msg = OutgoingMessage::text("channel-1", "Hello world");
        assert_eq!(msg.content.text_content(), Some("Hello world"));
    }

    #[test]
    fn test_outgoing_message_markdown() {
        let msg = OutgoingMessage::markdown("channel-1", "# Title");
        match msg.content {
            MessageContent::RichText { format, .. } => {
                assert_eq!(format, RichTextFormat::Markdown);
            }
            _ => panic!("Expected RichText"),
        }
    }

    #[test]
    fn test_outgoing_message_builder() {
        let parent_id = MessageId::new("msg-parent");
        let msg = OutgoingMessage::text("channel-1", "Reply")
            .in_thread("thread-123")
            .reply_to(parent_id.clone());

        assert_eq!(msg.thread_id, Some("thread-123".to_string()));
        assert_eq!(msg.reply_to, Some(parent_id));
    }

    // ============================================================================
    // ChannelInfo Tests
    // ============================================================================

    #[test]
    fn test_channel_info_new() {
        let info = ChannelInfo::new("ch-1", "My Channel", ChannelKind::Slack);
        assert_eq!(info.id, "ch-1");
        assert_eq!(info.name, "My Channel");
        assert_eq!(info.kind, ChannelKind::Slack);
        assert_eq!(info.status, ChannelStatus::Disconnected);
        assert_eq!(info.messages_sent, 0);
        assert_eq!(info.messages_received, 0);
        assert!(info.last_activity.is_none());
        assert!(info.error_message.is_none());
    }
}