//! Channel Module
//!
//! This module provides the channel abstraction layer for multi-platform
//! communication. It defines the core traits and types for connecting
//! to various messaging platforms like Slack, Discord, Email, etc.
//!
//! # Architecture
//!
//! - [`traits`] - Core `Channel` trait and `ChannelFactory` trait
//! - [`types`] - Message types, status, capabilities, and other data structures
//! - [`error`] - Error types for channel operations
//! - [`event`] - Channel lifecycle events
//! - [`manager`] - Central manager for coordinating all channel connections
//! - [`adapters`] - Platform-specific channel implementations
//! - [`behavior`] - Channel behavior configuration (style, triggers, delays, hours)
//!
//! # Legacy Types
//!
//! The module also exports legacy types (`ChannelKind`, `InboundMessage`, `OutboundMessage`)
//! for backward compatibility with existing code.

pub mod adapters;
pub mod behavior;
pub mod error;
pub mod event;
pub mod manager;
pub mod traits;
pub mod types;

// Re-export commonly used types for convenience
pub use error::ChannelError;
pub use event::{AgentId, ChannelEvent, ReconnectPolicy};
pub use manager::ChannelManager;
pub use traits::{Channel, ChannelConfig, ChannelFactory, ChannelId, MessageHandler, MessageId};
pub use types::{
    ChannelCapabilities, ChannelInfo, ChannelStatus, IncomingMessage,
    MessageContent, MessageSender, OutgoingMessage,
};

use std::collections::HashMap;

// ============================================================================
// Legacy Types (for backward compatibility)
// ============================================================================

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    Cli,
    Web,
    WebChat,
    Telegram,
    Discord,
    Slack,
    Whatsapp,
    GoogleChat,
    Signal,
    BlueBubbles,
    Imessage,
    Irc,
    Msteams,
    Matrix,
    Feishu,
    Line,
    Mattermost,
    NextcloudTalk,
    Nostr,
    SynologyChat,
    Tlon,
    Twitch,
    Wechat,
    Zalo,
    ZaloPersonal,
    Lark,
    Dingtalk,
    Email,
    Webhook,
    Other(String),
}

impl Default for ChannelKind {
    fn default() -> Self {
        Self::Cli
    }
}

impl std::fmt::Display for ChannelKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cli => write!(f, "Cli"),
            Self::Web => write!(f, "Web"),
            Self::WebChat => write!(f, "WebChat"),
            Self::Telegram => write!(f, "Telegram"),
            Self::Discord => write!(f, "Discord"),
            Self::Slack => write!(f, "Slack"),
            Self::Whatsapp => write!(f, "WhatsApp"),
            Self::GoogleChat => write!(f, "GoogleChat"),
            Self::Signal => write!(f, "Signal"),
            Self::BlueBubbles => write!(f, "BlueBubbles"),
            Self::Imessage => write!(f, "iMessage"),
            Self::Irc => write!(f, "IRC"),
            Self::Msteams => write!(f, "MSTeams"),
            Self::Matrix => write!(f, "Matrix"),
            Self::Feishu => write!(f, "Feishu"),
            Self::Line => write!(f, "Line"),
            Self::Mattermost => write!(f, "Mattermost"),
            Self::NextcloudTalk => write!(f, "NextcloudTalk"),
            Self::Nostr => write!(f, "Nostr"),
            Self::SynologyChat => write!(f, "SynologyChat"),
            Self::Tlon => write!(f, "Tlon"),
            Self::Twitch => write!(f, "Twitch"),
            Self::Wechat => write!(f, "WeChat"),
            Self::Zalo => write!(f, "Zalo"),
            Self::ZaloPersonal => write!(f, "ZaloPersonal"),
            Self::Lark => write!(f, "Lark"),
            Self::Dingtalk => write!(f, "DingTalk"),
            Self::Email => write!(f, "Email"),
            Self::Webhook => write!(f, "Webhook"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct InboundMessage {
    #[serde(default)]
    pub channel: ChannelKind,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub text: String,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct OutboundMessage {
    #[serde(default)]
    pub channel: ChannelKind,
    pub session_id: Option<String>,
    pub text: String,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}
