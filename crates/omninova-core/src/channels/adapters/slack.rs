//! Slack Channel Adapter
//!
//! Implements the Channel trait for Slack integration, supporting:
//! - Bot Token authentication
//! - Channel and direct message support
//! - Message threads
//! - Socket Mode for real-time events

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use tracing::{debug, error, info, warn};

use crate::channels::error::ChannelError;
use crate::channels::traits::{Channel, ChannelConfig, ChannelFactory, ChannelId, MessageHandler, MessageId};
use crate::channels::types::{
    ChannelCapabilities, ChannelStatus, Credentials,
    IncomingMessage, MessageContent, MessageSender, OutgoingMessage,
};
use crate::channels::ChannelKind;

// ============================================================================
// Slack API Constants
// ============================================================================

const SLACK_API_URL: &str = "https://slack.com/api";
const SLACK_API_AUTH_TEST: &str = "/auth.test";
const SLACK_API_POST_MESSAGE: &str = "/chat.postMessage";
const SLACK_API_CONVERSATIONS_LIST: &str = "/conversations.list";
const SLACK_API_USERS_INFO: &str = "/users.info";
const SLACK_API_APPS_CONNECTIONS_OPEN: &str = "/apps.connections.open";

// ============================================================================
// Slack Configuration
// ============================================================================

/// Slack channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    /// Slack Bot Token (xoxb-)
    pub bot_token: String,

    /// App-level Token for Socket Mode (xapp-)
    #[serde(default)]
    pub app_token: Option<String>,

    /// Signing secret for webhook verification
    #[serde(default)]
    pub signing_secret: Option<String>,

    /// List of channel IDs to listen to (empty = all channels)
    #[serde(default)]
    pub enabled_channels: Vec<String>,

    /// Use Socket Mode instead of webhooks
    #[serde(default)]
    pub socket_mode: bool,
}

impl SlackConfig {
    /// Create a new Slack configuration
    pub fn new(bot_token: impl Into<String>) -> Self {
        Self {
            bot_token: bot_token.into(),
            app_token: None,
            signing_secret: None,
            enabled_channels: Vec::new(),
            socket_mode: true, // Default to Socket Mode
        }
    }

    /// Set the app token for Socket Mode
    pub fn with_app_token(mut self, token: impl Into<String>) -> Self {
        self.app_token = Some(token.into());
        self
    }

    /// Set the signing secret
    pub fn with_signing_secret(mut self, secret: impl Into<String>) -> Self {
        self.signing_secret = Some(secret.into());
        self
    }

    /// Set enabled channels
    pub fn with_channels(mut self, channels: Vec<String>) -> Self {
        self.enabled_channels = channels;
        self
    }

    /// Enable or disable Socket Mode
    pub fn with_socket_mode(mut self, enabled: bool) -> Self {
        self.socket_mode = enabled;
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ChannelError> {
        if self.bot_token.is_empty() {
            return Err(ChannelError::config("bot_token is required"));
        }

        if !self.bot_token.starts_with("xoxb-") {
            return Err(ChannelError::config(
                "bot_token must start with 'xoxb-'",
            ));
        }

        if self.socket_mode {
            if let Some(ref app_token) = self.app_token {
                if !app_token.starts_with("xapp-") {
                    return Err(ChannelError::config(
                        "app_token must start with 'xapp-' for Socket Mode",
                    ));
                }
            }
        }

        Ok(())
    }

    /// Check if a channel is enabled
    pub fn is_channel_enabled(&self, channel_id: &str) -> bool {
        if self.enabled_channels.is_empty() {
            return true; // All channels enabled
        }
        self.enabled_channels.contains(&channel_id.to_string())
    }
}

// ============================================================================
// Slack API Response Types
// ============================================================================

/// Generic Slack API response wrapper
#[derive(Debug, Clone, Deserialize)]
struct SlackApiResponse<T> {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(flatten)]
    data: T,
}

/// Auth test response
#[derive(Debug, Clone, Deserialize)]
struct AuthTestResponse {
    user_id: String,
    team_id: String,
    user: String,
    team: String,
    #[serde(default)]
    bot_id: Option<String>,
}

/// Post message response
#[derive(Debug, Clone, Deserialize)]
struct PostMessageResponse {
    ts: String,
    channel: String,
    #[serde(default)]
    message: Option<SlackMessage>,
}

/// Conversations list response
#[derive(Debug, Clone, Deserialize)]
struct ConversationsListResponse {
    channels: Vec<SlackChannelInfo>,
    #[serde(default)]
    response_metadata: Option<ResponseMetadata>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ResponseMetadata {
    #[serde(default)]
    next_cursor: Option<String>,
}

/// Slack channel info
#[derive(Debug, Clone, Deserialize)]
struct SlackChannelInfo {
    id: String,
    name: String,
    #[serde(default)]
    is_channel: bool,
    #[serde(default)]
    is_private: bool,
    #[serde(default)]
    is_member: bool,
}

/// Users info response
#[derive(Debug, Clone, Deserialize)]
struct UsersInfoResponse {
    user: SlackUserInfo,
}

/// Slack user info
#[derive(Debug, Clone, Deserialize)]
struct SlackUserInfo {
    id: String,
    name: String,
    #[serde(default)]
    real_name: Option<String>,
    #[serde(default)]
    profile: Option<SlackUserProfile>,
    #[serde(default)]
    is_bot: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct SlackUserProfile {
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    image_48: Option<String>,
}

/// Slack message event
#[derive(Debug, Clone, Deserialize)]
pub struct SlackMessage {
    #[serde(default)]
    text: String,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    ts: Option<String>,
    #[serde(default)]
    thread_ts: Option<String>,
    #[serde(rename = "type")]
    #[serde(default)]
    message_type: Option<String>,
    #[serde(default)]
    channel_type: Option<String>,
    #[serde(default)]
    subtype: Option<String>,
    #[serde(default)]
    bot_id: Option<String>,
    #[serde(default)]
    blocks: Vec<serde_json::Value>,
}

// ============================================================================
// Slack API Client
// ============================================================================

/// Slack HTTP API client
#[derive(Debug, Clone)]
pub struct SlackApi {
    client: reqwest::Client,
    bot_token: String,
}

impl SlackApi {
    /// Create a new Slack API client
    pub fn new(bot_token: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            bot_token,
        }
    }

    /// Test authentication
    pub async fn auth_test(&self) -> Result<AuthTestResponse, ChannelError> {
        let response = self
            .client
            .post(format!("{}{}", SLACK_API_URL, SLACK_API_AUTH_TEST))
            .header("Authorization", format!("Bearer {}", self.bot_token))
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| ChannelError::connection_failed(format!("auth.test failed: {}", e)))?;

        let api_response: SlackApiResponse<AuthTestResponse> = response
            .json()
            .await
            .map_err(|e| ChannelError::Internal(format!("Failed to parse auth.test response: {}", e)))?;

        if !api_response.ok {
            return Err(ChannelError::auth_failed(
                api_response.error.unwrap_or_else(|| "Unknown auth error".to_string()),
            ));
        }

        Ok(api_response.data)
    }

    /// Post a message to a channel
    pub async fn post_message(
        &self,
        channel: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<PostMessageResponse, ChannelError> {
        let mut body = HashMap::new();
        body.insert("channel", channel.to_string());
        body.insert("text", text.to_string());

        if let Some(ts) = thread_ts {
            body.insert("thread_ts", ts.to_string());
        }

        let response = self
            .client
            .post(format!("{}{}", SLACK_API_URL, SLACK_API_POST_MESSAGE))
            .header("Authorization", format!("Bearer {}", self.bot_token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ChannelError::send_failed(format!("chat.postMessage failed: {}", e)))?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            // Rate limited
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            return Err(ChannelError::rate_limit(retry_after));
        }

        let api_response: SlackApiResponse<PostMessageResponse> = response
            .json()
            .await
            .map_err(|e| ChannelError::Internal(format!("Failed to parse post message response: {}", e)))?;

        if !api_response.ok {
            let error = api_response.error.unwrap_or_else(|| "Unknown post message error".to_string());
            return Err(ChannelError::send_failed(error));
        }

        Ok(api_response.data)
    }

    /// Get list of channels
    pub async fn conversations_list(&self) -> Result<Vec<SlackChannelInfo>, ChannelError> {
        let mut channels = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut url = format!("{}{}?types=public_channel,private_channel", SLACK_API_URL, SLACK_API_CONVERSATIONS_LIST);

            if let Some(ref c) = cursor {
                url.push_str(&format!("&cursor={}", c));
            }

            let response = self
                .client
                .get(&url)
                .header("Authorization", format!("Bearer {}", self.bot_token))
                .send()
                .await
                .map_err(|e| ChannelError::connection_failed(format!("conversations.list failed: {}", e)))?;

            let api_response: SlackApiResponse<ConversationsListResponse> = response
                .json()
                .await
                .map_err(|e| ChannelError::Internal(format!("Failed to parse conversations.list response: {}", e)))?;

            if !api_response.ok {
                return Err(ChannelError::Internal(
                    api_response.error.unwrap_or_else(|| "Unknown conversations.list error".to_string()),
                ));
            }

            channels.extend(api_response.data.channels);
            cursor = api_response.data.response_metadata.and_then(|m| m.next_cursor);

            if cursor.is_none() {
                break;
            }
        }

        Ok(channels)
    }

    /// Get user info
    pub async fn users_info(&self, user_id: &str) -> Result<SlackUserInfo, ChannelError> {
        let url = format!("{}{}?user={}", SLACK_API_URL, SLACK_API_USERS_INFO, user_id);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.bot_token))
            .send()
            .await
            .map_err(|e| ChannelError::connection_failed(format!("users.info failed: {}", e)))?;

        let api_response: SlackApiResponse<UsersInfoResponse> = response
            .json()
            .await
            .map_err(|e| ChannelError::Internal(format!("Failed to parse users.info response: {}", e)))?;

        if !api_response.ok {
            return Err(ChannelError::Internal(
                api_response.error.unwrap_or_else(|| "Unknown users.info error".to_string()),
            ));
        }

        Ok(api_response.data.user)
    }

    /// Get WebSocket URL for Socket Mode
    pub async fn get_socket_url(&self, app_token: &str) -> Result<String, ChannelError> {
        let response = self
            .client
            .post(format!("{}{}", SLACK_API_URL, SLACK_API_APPS_CONNECTIONS_OPEN))
            .header("Authorization", format!("Bearer {}", app_token))
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| ChannelError::connection_failed(format!("apps.connections.open failed: {}", e)))?;

        #[derive(Debug, Clone, Deserialize)]
        struct ConnectionsOpenResponse {
            url: String,
        }

        let api_response: SlackApiResponse<ConnectionsOpenResponse> = response
            .json()
            .await
            .map_err(|e| ChannelError::Internal(format!("Failed to parse apps.connections.open response: {}", e)))?;

        if !api_response.ok {
            return Err(ChannelError::connection_failed(
                api_response.error.unwrap_or_else(|| "Unknown apps.connections.open error".to_string()),
            ));
        }

        Ok(api_response.data.url)
    }
}

// ============================================================================
// Socket Mode Types
// ============================================================================

/// Socket Mode envelope types
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum SocketModeEnvelope {
    /// Hello message on connection
    #[serde(rename = "hello")]
    Hello {},

    /// Events API payload
    #[serde(rename = "events_api")]
    EventsApi {
        #[serde(default)]
        envelope_id: String,
        #[serde(default)]
        payload: serde_json::Value,
        #[serde(default)]
        accepts_response_payload: Option<bool>,
    },

    /// Slash command
    #[serde(rename = "slash_commands")]
    SlashCommands {
        #[serde(default)]
        envelope_id: String,
        #[serde(default)]
        payload: serde_json::Value,
    },

    /// Interactive payload
    #[serde(rename = "interactive")]
    Interactive {
        #[serde(default)]
        envelope_id: String,
        #[serde(default)]
        payload: serde_json::Value,
    },

    /// Disconnect message
    #[serde(rename = "disconnect")]
    Disconnect {
        #[serde(default)]
        reason: Option<String>,
    },
}

/// Events API event payload
#[derive(Debug, Clone, Deserialize)]
pub struct EventPayload {
    #[serde(default)]
    pub event: Option<serde_json::Value>,
    #[serde(default)]
    pub event_id: Option<String>,
    #[serde(default)]
    pub event_time: Option<i64>,
}

// ============================================================================
// Socket Mode Client
// ============================================================================

/// Socket Mode connection state
#[derive(Debug, Clone, PartialEq)]
pub enum SocketModeState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

/// Socket Mode client for real-time Slack events
pub struct SlackSocketMode {
    /// App-level token
    app_token: String,

    /// Bot token for API calls
    bot_token: String,

    /// Connection state
    state: Arc<RwLock<SocketModeState>>,

    /// Message handler for incoming messages
    message_handler: Option<Arc<RwLock<Box<dyn MessageHandler>>>>,

    /// Channel ID for message routing
    channel_id: String,

    /// Enabled channels filter
    enabled_channels: Vec<String>,

    /// Stop signal sender
    stop_tx: Option<mpsc::Sender<()>>,
}

impl SlackSocketMode {
    /// Create a new Socket Mode client
    pub fn new(app_token: String, bot_token: String) -> Self {
        Self {
            app_token,
            bot_token,
            state: Arc::new(RwLock::new(SocketModeState::Disconnected)),
            message_handler: None,
            channel_id: String::new(),
            enabled_channels: Vec::new(),
            stop_tx: None,
        }
    }

    /// Set the message handler
    pub fn with_message_handler(mut self, handler: Arc<RwLock<Box<dyn MessageHandler>>>) -> Self {
        self.message_handler = Some(handler);
        self
    }

    /// Set the channel ID
    pub fn with_channel_id(mut self, channel_id: String) -> Self {
        self.channel_id = channel_id;
        self
    }

    /// Set enabled channels filter
    pub fn with_enabled_channels(mut self, channels: Vec<String>) -> Self {
        self.enabled_channels = channels;
        self
    }

    /// Check if a channel is enabled
    fn is_channel_enabled(&self, channel_id: &str) -> bool {
        if self.enabled_channels.is_empty() {
            return true;
        }
        self.enabled_channels.contains(&channel_id.to_string())
    }

    /// Connect to Slack Socket Mode
    pub async fn connect(&mut self) -> Result<(), ChannelError> {
        let api = SlackApi::new(self.bot_token.clone());
        let ws_url = api.get_socket_url(&self.app_token).await?;

        info!("Connecting to Slack Socket Mode: {}", ws_url);

        *self.state.write().await = SocketModeState::Connecting;

        // Connect WebSocket
        let (ws_stream, _) = connect_async(&ws_url)
            .await
            .map_err(|e| ChannelError::connection_failed(format!("WebSocket connection failed: {}", e)))?;

        info!("WebSocket connected to Slack Socket Mode");

        let (write, read) = ws_stream.split();
        let write = Arc::new(tokio::sync::Mutex::new(write));

        // Create stop channel
        let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);

        self.stop_tx = Some(stop_tx);

        *self.state.write().await = SocketModeState::Connected;

        // Clone necessary data for the spawned task
        let state = self.state.clone();
        let write_clone = write.clone();
        let message_handler = self.message_handler.clone();
        let channel_id = self.channel_id.clone();
        let enabled_channels = self.enabled_channels.clone();
        let bot_token = self.bot_token.clone();

        // Spawn message reader task
        tokio::spawn(async move {
            Self::read_messages(
                read,
                write_clone,
                state,
                message_handler,
                channel_id,
                enabled_channels,
                bot_token,
                &mut stop_rx,
            ).await;
        });

        Ok(())
    }

    /// Disconnect from Socket Mode
    pub async fn disconnect(&mut self) -> Result<(), ChannelError> {
        *self.state.write().await = SocketModeState::Disconnected;

        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(()).await;
        }

        info!("Disconnected from Slack Socket Mode");
        Ok(())
    }

    /// Get the current state
    pub async fn state(&self) -> SocketModeState {
        self.state.read().await.clone()
    }

    /// Update the message handler
    pub fn set_message_handler(&mut self, handler: Arc<RwLock<Box<dyn MessageHandler>>>) {
        self.message_handler = Some(handler);
    }

    /// Read messages from WebSocket
    async fn read_messages(
        mut read: futures_util::stream::SplitStream<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
        >,
        write: Arc<tokio::sync::Mutex<
            futures_util::stream::SplitSink<
                tokio_tungstenite::WebSocketStream<
                    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                >,
                WsMessage,
            >,
        >>,
        state: Arc<RwLock<SocketModeState>>,
        message_handler: Option<Arc<RwLock<Box<dyn MessageHandler>>>>,
        channel_id: String,
        enabled_channels: Vec<String>,
        bot_token: String,
        stop_rx: &mut mpsc::Receiver<()>,
    ) {
        debug!("Starting Socket Mode message reader");

        // Helper function to check if channel is enabled
        let is_channel_enabled = |ch: &str| -> bool {
            if enabled_channels.is_empty() {
                return true;
            }
            enabled_channels.contains(&ch.to_string())
        };

        loop {
            // Check for stop signal
            if stop_rx.try_recv().is_ok() {
                debug!("Received stop signal, exiting message reader");
                break;
            }

            // Check state
            if *state.read().await == SocketModeState::Disconnected {
                break;
            }

            // Try to receive a message with timeout
            let msg = tokio::select! {
                msg = read.next() => msg,
                _ = tokio::time::sleep(Duration::from_secs(30)) => {
                    // Send keepalive ping
                    debug!("Sending keepalive ping");
                    continue;
                }
            };

            let Some(msg) = msg else {
                warn!("WebSocket stream ended");
                break;
            };

            match msg {
                Ok(WsMessage::Text(text)) => {
                    debug!("Received WebSocket message: {}", text);

                    // Parse the envelope
                    if let Ok(envelope) = serde_json::from_str::<SocketModeEnvelope>(&text) {
                        match envelope {
                            SocketModeEnvelope::Hello {} => {
                                info!("Received hello from Slack Socket Mode");
                            }
                            SocketModeEnvelope::EventsApi { envelope_id, payload, .. } => {
                                debug!("Received events_api message: envelope_id={}", envelope_id);

                                // Acknowledge the message
                                let ack = serde_json::json!({
                                    "envelope_id": envelope_id
                                });
                                let ack_msg = WsMessage::Text(ack.to_string());
                                {
                                    let mut writer = write.lock().await;
                                    let _ = writer.send(ack_msg).await;
                                }

                                // Process the event
                                if let Some(event) = payload.get("event") {
                                    // Parse as Slack message
                                    if let Ok(slack_msg) = serde_json::from_value::<SlackMessage>(event.clone()) {
                                        let ch_id = slack_msg.channel.clone().unwrap_or_default();

                                        // Check channel filter
                                        if !is_channel_enabled(&ch_id) {
                                            debug!("Skipping message from disabled channel: {}", ch_id);
                                            continue;
                                        }

                                        // Skip bot messages
                                        if slack_msg.bot_id.is_some() || slack_msg.subtype.is_some() {
                                            debug!("Skipping bot/subtype message");
                                            continue;
                                        }

                                        // Skip empty messages
                                        if slack_msg.text.is_empty() {
                                            continue;
                                        }

                                        // Convert to IncomingMessage
                                        let user_id = slack_msg.user.clone().unwrap_or_default();
                                        let sender = MessageSender::new(&user_id);
                                        let content = MessageContent::text(&slack_msg.text);

                                        let mut message = IncomingMessage::new(
                                            &channel_id,
                                            ChannelKind::Slack,
                                            sender,
                                            content,
                                        );

                                        // Set thread ID if present
                                        if let Some(ref thread_ts) = slack_msg.thread_ts {
                                            message = message.in_thread(thread_ts);
                                        }

                                        // Set timestamp
                                        if let Some(ref ts) = slack_msg.ts {
                                            if let Ok(ts_f64) = ts.parse::<f64>() {
                                                message.timestamp = ts_f64 as i64;
                                            }
                                        }

                                        // Add metadata
                                        message = message
                                            .with_metadata("slack_channel", serde_json::json!(&ch_id))
                                            .with_metadata("slack_ts", serde_json::json!(&slack_msg.ts));

                                        if let Some(ref channel_type) = slack_msg.channel_type {
                                            message = message.with_metadata("slack_channel_type", serde_json::json!(channel_type));
                                        }

                                        // Invoke message handler
                                        if let Some(ref handler) = message_handler {
                                            let h = handler.write().await;
                                            debug!("Invoking message handler for message from {}", ch_id);
                                            h.handle(message).await;
                                        }
                                    }
                                }
                            }
                            SocketModeEnvelope::Disconnect { reason } => {
                                warn!("Received disconnect from Slack: {:?}", reason);
                                *state.write().await = SocketModeState::Disconnected;
                                break;
                            }
                            _ => {
                                debug!("Received other envelope type: {:?}", envelope);
                            }
                        }
                    }
                }
                Ok(WsMessage::Ping(data)) => {
                    debug!("Received ping, sending pong");
                    {
                        let mut writer = write.lock().await;
                        let _ = writer.send(WsMessage::Pong(data)).await;
                    }
                }
                Ok(WsMessage::Pong(_)) => {
                    debug!("Received pong");
                }
                Ok(WsMessage::Close(_)) => {
                    warn!("WebSocket closed by server");
                    *state.write().await = SocketModeState::Disconnected;
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    *state.write().await = SocketModeState::Disconnected;
                    break;
                }
                _ => {}
            }
        }

        debug!("Socket Mode message reader stopped");
    }

    /// Send a keep-alive ping
    pub async fn ping(&self) -> Result<(), ChannelError> {
        // Ping is handled automatically by tungstenite
        Ok(())
    }
}

// ============================================================================
// Slack Channel Implementation
// ============================================================================

/// Slack channel adapter
pub struct SlackChannel {
    /// Channel instance ID
    id: ChannelId,

    /// Slack configuration
    config: SlackConfig,

    /// Connection status
    status: ChannelStatus,

    /// Slack API client
    api: Option<SlackApi>,

    /// Message handler
    message_handler: Option<Arc<RwLock<Box<dyn MessageHandler>>>>,

    /// Bot user ID (from auth.test)
    bot_user_id: Option<String>,

    /// Bot ID (from auth.test)
    bot_id: Option<String>,

    /// Socket Mode client (optional)
    socket_mode: Option<SlackSocketMode>,
}

impl SlackChannel {
    /// Create a new Slack channel instance
    pub fn new(id: impl Into<String>, config: SlackConfig) -> Self {
        Self {
            id: id.into(),
            config,
            status: ChannelStatus::Disconnected,
            api: None,
            message_handler: None,
            bot_user_id: None,
            bot_id: None,
            socket_mode: None,
        }
    }

    /// Get the Slack API client
    fn api(&self) -> Result<&SlackApi, ChannelError> {
        self.api.as_ref().ok_or(ChannelError::NotConnected)
    }

    /// Convert a Slack message to an IncomingMessage
    fn convert_message(&self, slack_msg: SlackMessage, channel_id: &str) -> Option<IncomingMessage> {
        // Skip messages from bots (including our own)
        if slack_msg.bot_id.is_some() || slack_msg.subtype.is_some() {
            return None;
        }

        // Skip messages without text
        if slack_msg.text.is_empty() {
            return None;
        }

        // Check if channel is enabled
        if !self.config.is_channel_enabled(channel_id) {
            return None;
        }

        // Create message sender
        let user_id = slack_msg.user.clone().unwrap_or_default();
        let sender = MessageSender::new(&user_id);

        // Create message content
        let content = if slack_msg.blocks.is_empty() {
            MessageContent::text(&slack_msg.text)
        } else {
            // For now, just use text - blocks parsing could be added later
            MessageContent::text(&slack_msg.text)
        };

        // Create incoming message
        let mut message = IncomingMessage::new(
            &self.id,
            ChannelKind::Slack,
            sender,
            content,
        );

        // Set thread ID if present
        if let Some(ref thread_ts) = slack_msg.thread_ts {
            message = message.in_thread(thread_ts);
        }

        // Set timestamp
        if let Some(ref ts) = slack_msg.ts {
            if let Ok(ts_f64) = ts.parse::<f64>() {
                message.timestamp = ts_f64 as i64;
            }
        }

        // Add Slack-specific metadata
        message = message
            .with_metadata("slack_channel", serde_json::json!(channel_id))
            .with_metadata("slack_ts", serde_json::json!(slack_msg.ts));

        if let Some(ref channel_type) = slack_msg.channel_type {
            message = message.with_metadata("slack_channel_type", serde_json::json!(channel_type));
        }

        Some(message)
    }
}

#[async_trait]
impl Channel for SlackChannel {
    fn id(&self) -> &str {
        &self.id
    }

    fn channel_kind(&self) -> ChannelKind {
        ChannelKind::Slack
    }

    async fn connect(&mut self) -> Result<(), ChannelError> {
        // Validate configuration
        self.config.validate()?;

        // Check current status
        if matches!(self.status, ChannelStatus::Connected) {
            return Ok(());
        }

        self.status = ChannelStatus::Connecting;

        // Create API client
        let api = SlackApi::new(self.config.bot_token.clone());

        // Test authentication
        let auth_response = api.auth_test().await?;

        info!(
            "Slack connected as {} on team {}",
            auth_response.user, auth_response.team
        );

        // Store bot info
        self.bot_user_id = Some(auth_response.user_id);
        self.bot_id = auth_response.bot_id;

        // Store API client
        self.api = Some(api);

        // Start Socket Mode if app_token is provided and socket_mode is enabled
        if self.config.socket_mode {
            if let Some(ref app_token) = self.config.app_token {
                info!("Starting Slack Socket Mode connection");

                let mut socket_mode = SlackSocketMode::new(
                    app_token.clone(),
                    self.config.bot_token.clone(),
                )
                .with_channel_id(self.id.clone())
                .with_enabled_channels(self.config.enabled_channels.clone());

                // Set message handler if available
                if let Some(ref handler) = self.message_handler {
                    socket_mode = socket_mode.with_message_handler(handler.clone());
                }

                socket_mode.connect().await?;
                self.socket_mode = Some(socket_mode);

                info!("Slack Socket Mode connected successfully");
            } else {
                warn!("Socket Mode enabled but no app_token provided. Real-time events will not be received.");
            }
        }

        self.status = ChannelStatus::Connected;

        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), ChannelError> {
        // Disconnect Socket Mode if active
        if let Some(ref mut socket_mode) = self.socket_mode {
            socket_mode.disconnect().await?;
        }
        self.socket_mode = None;

        self.status = ChannelStatus::Disconnected;
        self.api = None;
        self.bot_user_id = None;
        self.bot_id = None;

        info!("Slack channel {} disconnected", self.id);
        Ok(())
    }

    async fn send_message(&self, message: OutgoingMessage) -> Result<MessageId, ChannelError> {
        let api = self.api()?;

        if !matches!(self.status, ChannelStatus::Connected) {
            return Err(ChannelError::NotConnected);
        }

        // Extract text content
        let text = match &message.content {
            MessageContent::Text { text } => text.clone(),
            MessageContent::RichText { text, .. } => text.clone(),
            _ => {
                return Err(ChannelError::InvalidMessage(
                    "Only text and rich text messages are supported".to_string(),
                ));
            }
        };

        // Post message
        let response = api
            .post_message(&message.target_id, &text, message.thread_id.as_deref())
            .await?;

        debug!(
            "Sent message to Slack channel {}: ts={}",
            response.channel, response.ts
        );

        Ok(response.ts)
    }

    fn get_status(&self) -> ChannelStatus {
        self.status.clone()
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities::TEXT
            | ChannelCapabilities::RICH_TEXT
            | ChannelCapabilities::THREADS
            | ChannelCapabilities::MENTIONS
            | ChannelCapabilities::REACTIONS
            | ChannelCapabilities::FILES
            | ChannelCapabilities::IMAGES
            | ChannelCapabilities::CHANNEL_MESSAGE
            | ChannelCapabilities::DIRECT_MESSAGE
    }

    fn set_message_handler(&mut self, handler: Box<dyn MessageHandler>) {
        let handler_arc = Arc::new(RwLock::new(handler));
        self.message_handler = Some(handler_arc.clone());

        // Update Socket Mode handler if already connected
        if let Some(ref mut socket_mode) = self.socket_mode {
            socket_mode.set_message_handler(handler_arc);
        }
    }
}

// ============================================================================
// Slack Channel Factory
// ============================================================================

/// Factory for creating Slack channel instances
pub struct SlackChannelFactory;

impl SlackChannelFactory {
    /// Create a new Slack channel factory
    pub fn new() -> Self {
        Self
    }
}

impl Default for SlackChannelFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelFactory for SlackChannelFactory {
    fn channel_kind(&self) -> ChannelKind {
        ChannelKind::Slack
    }

    fn create(&self, config: ChannelConfig) -> Result<Box<dyn Channel>, ChannelError> {
        // Extract Slack configuration from credentials
        let slack_config = match config.credentials {
            Credentials::BotToken { ref token } => {
                let mut slack_config = SlackConfig::new(token.clone());

                // Extract additional settings from extra field
                if !config.settings.extra.is_null() {
                    if let Ok(extra) = serde_json::from_value::<SlackConfigExtra>(config.settings.extra.clone()) {
                        if let Some(app_token) = extra.app_token {
                            slack_config = slack_config.with_app_token(app_token);
                        }
                        if let Some(enabled_channels) = extra.enabled_channels {
                            slack_config = slack_config.with_channels(enabled_channels);
                        }
                        slack_config = slack_config.with_socket_mode(extra.socket_mode.unwrap_or(true));
                    }
                }

                slack_config
            }
            Credentials::OAuth2 { ref access_token, .. } => {
                SlackConfig::new(access_token)
            }
            _ => {
                return Err(ChannelError::config(
                    "Slack channel requires BotToken or OAuth2 credentials",
                ));
            }
        };

        let channel = SlackChannel::new(config.id, slack_config);
        Ok(Box::new(channel))
    }
}

/// Extra configuration for Slack from settings.extra
#[derive(Debug, Clone, Deserialize)]
struct SlackConfigExtra {
    #[serde(default)]
    app_token: Option<String>,
    #[serde(default)]
    enabled_channels: Option<Vec<String>>,
    #[serde(default)]
    socket_mode: Option<bool>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // SlackConfig Tests
    // ============================================================================

    #[test]
    fn test_slack_config_new() {
        let config = SlackConfig::new("xoxb-test-token");
        assert_eq!(config.bot_token, "xoxb-test-token");
        assert!(config.app_token.is_none());
        assert!(config.enabled_channels.is_empty());
        assert!(config.socket_mode);
    }

    #[test]
    fn test_slack_config_builder() {
        let config = SlackConfig::new("xoxb-test-token")
            .with_app_token("xapp-test-token")
            .with_channels(vec!["C123".to_string(), "C456".to_string()])
            .with_socket_mode(false);

        assert_eq!(config.app_token, Some("xapp-test-token".to_string()));
        assert_eq!(config.enabled_channels, vec!["C123", "C456"]);
        assert!(!config.socket_mode);
    }

    #[test]
    fn test_slack_config_validate_empty_token() {
        let config = SlackConfig::new("");
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_slack_config_validate_invalid_token() {
        let config = SlackConfig::new("invalid-token");
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_slack_config_validate_valid() {
        let config = SlackConfig::new("xoxb-valid-token");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_slack_config_validate_app_token() {
        let config = SlackConfig::new("xoxb-valid-token")
            .with_socket_mode(true)
            .with_app_token("invalid-app-token");

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_slack_config_is_channel_enabled() {
        let config = SlackConfig::new("xoxb-test-token")
            .with_channels(vec!["C123".to_string()]);

        assert!(config.is_channel_enabled("C123"));
        assert!(!config.is_channel_enabled("C456"));
    }

    #[test]
    fn test_slack_config_all_channels_enabled() {
        let config = SlackConfig::new("xoxb-test-token");

        assert!(config.is_channel_enabled("C123"));
        assert!(config.is_channel_enabled("C456"));
    }

    // ============================================================================
    // SlackChannel Tests
    // ============================================================================

    #[test]
    fn test_slack_channel_new() {
        let config = SlackConfig::new("xoxb-test-token");
        let channel = SlackChannel::new("slack-1", config);

        assert_eq!(channel.id(), "slack-1");
        assert_eq!(channel.channel_kind(), ChannelKind::Slack);
        assert_eq!(channel.get_status(), ChannelStatus::Disconnected);
    }

    #[test]
    fn test_slack_channel_capabilities() {
        let config = SlackConfig::new("xoxb-test-token");
        let channel = SlackChannel::new("slack-1", config);

        let caps = channel.capabilities();
        assert!(caps.contains(ChannelCapabilities::TEXT));
        assert!(caps.contains(ChannelCapabilities::RICH_TEXT));
        assert!(caps.contains(ChannelCapabilities::THREADS));
        assert!(caps.contains(ChannelCapabilities::MENTIONS));
        assert!(caps.contains(ChannelCapabilities::FILES));
    }

    #[test]
    fn test_slack_channel_convert_message() {
        let config = SlackConfig::new("xoxb-test-token");
        let channel = SlackChannel::new("slack-1", config);

        let slack_msg = SlackMessage {
            text: "Hello bot!".to_string(),
            user: Some("U123".to_string()),
            channel: Some("C123".to_string()),
            ts: Some("1234567890.123456".to_string()),
            thread_ts: None,
            message_type: Some("message".to_string()),
            channel_type: Some("channel".to_string()),
            subtype: None,
            bot_id: None,
            blocks: vec![],
        };

        let incoming = channel.convert_message(slack_msg, "C123");
        assert!(incoming.is_some());

        let msg = incoming.unwrap();
        assert_eq!(msg.sender.id, "U123");
        assert_eq!(msg.content.text_content(), Some("Hello bot!"));
        assert!(msg.thread_id.is_none());
    }

    #[test]
    fn test_slack_channel_convert_message_with_thread() {
        let config = SlackConfig::new("xoxb-test-token");
        let channel = SlackChannel::new("slack-1", config);

        let slack_msg = SlackMessage {
            text: "Reply".to_string(),
            user: Some("U123".to_string()),
            channel: Some("C123".to_string()),
            ts: Some("1234567891.123456".to_string()),
            thread_ts: Some("1234567890.000000".to_string()),
            message_type: Some("message".to_string()),
            channel_type: Some("channel".to_string()),
            subtype: None,
            bot_id: None,
            blocks: vec![],
        };

        let incoming = channel.convert_message(slack_msg, "C123");
        assert!(incoming.is_some());

        let msg = incoming.unwrap();
        assert_eq!(msg.thread_id, Some("1234567890.000000".to_string()));
    }

    #[test]
    fn test_slack_channel_convert_message_skip_bot() {
        let config = SlackConfig::new("xoxb-test-token");
        let channel = SlackChannel::new("slack-1", config);

        let slack_msg = SlackMessage {
            text: "Bot message".to_string(),
            user: None,
            channel: Some("C123".to_string()),
            ts: Some("1234567890.123456".to_string()),
            thread_ts: None,
            message_type: Some("message".to_string()),
            channel_type: Some("channel".to_string()),
            subtype: None,
            bot_id: Some("B123".to_string()), // Bot message
            blocks: vec![],
        };

        let incoming = channel.convert_message(slack_msg, "C123");
        assert!(incoming.is_none()); // Bot messages should be skipped
    }

    #[test]
    fn test_slack_channel_convert_message_channel_filter() {
        let config = SlackConfig::new("xoxb-test-token")
            .with_channels(vec!["C123".to_string()]);
        let channel = SlackChannel::new("slack-1", config);

        let slack_msg = SlackMessage {
            text: "Hello".to_string(),
            user: Some("U123".to_string()),
            channel: Some("C456".to_string()), // Not in enabled channels
            ts: Some("1234567890.123456".to_string()),
            thread_ts: None,
            message_type: Some("message".to_string()),
            channel_type: Some("channel".to_string()),
            subtype: None,
            bot_id: None,
            blocks: vec![],
        };

        let incoming = channel.convert_message(slack_msg, "C456");
        assert!(incoming.is_none()); // Channel not enabled
    }

    // ============================================================================
    // SlackChannelFactory Tests
    // ============================================================================

    #[test]
    fn test_slack_channel_factory_kind() {
        let factory = SlackChannelFactory::new();
        assert_eq!(factory.channel_kind(), ChannelKind::Slack);
    }

    #[test]
    fn test_slack_channel_factory_create_with_bot_token() {
        let factory = SlackChannelFactory::new();
        let config = ChannelConfig::new(
            "slack-1",
            ChannelKind::Slack,
            Credentials::BotToken {
                token: "xoxb-test-token".to_string(),
            },
        );

        let channel = factory.create(config);
        assert!(channel.is_ok());

        let channel = channel.unwrap();
        assert_eq!(channel.id(), "slack-1");
        assert_eq!(channel.channel_kind(), ChannelKind::Slack);
    }

    #[test]
    fn test_slack_channel_factory_create_with_oauth2() {
        let factory = SlackChannelFactory::new();
        let config = ChannelConfig::new(
            "slack-2",
            ChannelKind::Slack,
            Credentials::OAuth2 {
                access_token: "xoxb-test-token".to_string(),
                refresh_token: None,
                expires_at: None,
            },
        );

        let channel = factory.create(config);
        assert!(channel.is_ok());

        let channel = channel.unwrap();
        assert_eq!(channel.id(), "slack-2");
    }

    #[test]
    fn test_slack_channel_factory_create_invalid_credentials() {
        let factory = SlackChannelFactory::new();
        let config = ChannelConfig::new(
            "slack-3",
            ChannelKind::Slack,
            Credentials::None,
        );

        let result = factory.create(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_slack_channel_factory_create_with_extra_config() {
        let factory = SlackChannelFactory::new();

        let extra = serde_json::json!({
            "app_token": "xapp-test-token",
            "enabled_channels": ["C123", "C456"],
            "socket_mode": false
        });

        let config = ChannelConfig::new(
            "slack-4",
            ChannelKind::Slack,
            Credentials::BotToken {
                token: "xoxb-test-token".to_string(),
            },
        )
        .with_settings(crate::channels::traits::ChannelSettings {
            extra,
            ..Default::default()
        });

        let channel = factory.create(config);
        assert!(channel.is_ok());
    }

    // ============================================================================
    // Slack Message Tests
    // ============================================================================

    #[test]
    fn test_slack_message_deserialize() {
        let json = r#"{
            "text": "Hello world",
            "user": "U123",
            "channel": "C123",
            "ts": "1234567890.123456",
            "thread_ts": "1234567890.000000",
            "type": "message",
            "channel_type": "channel"
        }"#;

        let msg: SlackMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.text, "Hello world");
        assert_eq!(msg.user, Some("U123".to_string()));
        assert_eq!(msg.channel, Some("C123".to_string()));
        assert_eq!(msg.thread_ts, Some("1234567890.000000".to_string()));
    }
}