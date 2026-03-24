//! Discord Channel Adapter
//!
//! Implements the Channel trait for Discord integration, supporting:
//! - Bot Token authentication
//! - Guild (server) and channel support
//! - Gateway connection for real-time events
//! - Embeds and message formatting

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
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
// Discord API Constants
// ============================================================================

const DISCORD_API_URL: &str = "https://discord.com/api/v10";
const DISCORD_GATEWAY_URL: &str = "wss://gateway.discord.gg/?v=10&encoding=json";

// ============================================================================
// Discord Configuration
// ============================================================================

/// Discord channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    /// Discord Bot Token
    pub bot_token: String,

    /// Application ID for slash commands (optional)
    #[serde(default)]
    pub application_id: Option<String>,

    /// List of guild (server) IDs to listen to (empty = all guilds)
    #[serde(default)]
    pub enabled_guilds: Vec<String>,

    /// List of channel IDs to listen to (empty = all channels)
    #[serde(default)]
    pub enabled_channels: Vec<String>,

    /// Enable Gateway connection for real-time events
    #[serde(default = "default_gateway_enabled")]
    pub gateway_enabled: bool,
}

fn default_gateway_enabled() -> bool {
    true
}

impl DiscordConfig {
    /// Create a new Discord configuration
    pub fn new(bot_token: impl Into<String>) -> Self {
        Self {
            bot_token: bot_token.into(),
            application_id: None,
            enabled_guilds: Vec::new(),
            enabled_channels: Vec::new(),
            gateway_enabled: true,
        }
    }

    /// Set the application ID
    pub fn with_application_id(mut self, id: impl Into<String>) -> Self {
        self.application_id = Some(id.into());
        self
    }

    /// Set enabled guilds (servers)
    pub fn with_guilds(mut self, guilds: Vec<String>) -> Self {
        self.enabled_guilds = guilds;
        self
    }

    /// Set enabled channels
    pub fn with_channels(mut self, channels: Vec<String>) -> Self {
        self.enabled_channels = channels;
        self
    }

    /// Enable or disable Gateway connection
    pub fn with_gateway(mut self, enabled: bool) -> Self {
        self.gateway_enabled = enabled;
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ChannelError> {
        if self.bot_token.is_empty() {
            return Err(ChannelError::config("bot_token is required"));
        }

        // Discord bot tokens don't have a specific prefix format like Slack,
        // but they're typically base64-encoded strings
        if self.bot_token.len() < 50 {
            return Err(ChannelError::config(
                "bot_token appears to be invalid (too short)",
            ));
        }

        Ok(())
    }

    /// Check if a guild is enabled
    pub fn is_guild_enabled(&self, guild_id: &str) -> bool {
        if self.enabled_guilds.is_empty() {
            return true; // All guilds enabled
        }
        self.enabled_guilds.contains(&guild_id.to_string())
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
// Discord API Response Types
// ============================================================================

/// Discord user object
#[derive(Debug, Clone, Deserialize)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub discriminator: String,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub bot: Option<bool>,
}

/// Discord channel object (API response)
#[derive(Debug, Clone, Deserialize)]
pub struct DiscordChannelInfo {
    pub id: String,
    #[serde(rename = "type")]
    pub channel_type: i32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub guild_id: Option<String>,
}

/// Discord guild (server) object
#[derive(Debug, Clone, Deserialize)]
pub struct DiscordGuild {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
}

/// Discord message object
#[derive(Debug, Clone, Deserialize)]
pub struct DiscordMessage {
    pub id: String,
    pub channel_id: String,
    #[serde(default)]
    pub guild_id: Option<String>,
    pub author: DiscordUser,
    pub content: String,
    #[serde(default)]
    pub mentions: Vec<DiscordUser>,
    #[serde(default)]
    pub mention_everyone: bool,
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub edited_timestamp: Option<String>,
    #[serde(default)]
    pub tts: bool,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub embeds: Vec<DiscordEmbed>,
}

/// Discord embed object
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscordEmbed {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footer: Option<DiscordEmbedFooter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<DiscordEmbedImage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<DiscordEmbedThumbnail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<DiscordEmbedAuthor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<DiscordEmbedField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordEmbedFooter {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordEmbedImage {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordEmbedThumbnail {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordEmbedAuthor {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordEmbedField {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub inline: bool,
}

/// Discord message send payload
#[derive(Debug, Clone, Serialize)]
struct MessagePayload {
    content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    embeds: Vec<DiscordEmbed>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tts: Option<bool>,
}

// ============================================================================
// Discord API Client
// ============================================================================

/// Discord HTTP API client
#[derive(Debug, Clone)]
pub struct DiscordApi {
    client: reqwest::Client,
    bot_token: String,
}

impl DiscordApi {
    /// Create a new Discord API client
    pub fn new(bot_token: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            bot_token,
        }
    }

    /// Get the current bot user
    pub async fn get_current_user(&self) -> Result<DiscordUser, ChannelError> {
        let response = self
            .client
            .get(format!("{}/users/@me", DISCORD_API_URL))
            .header("Authorization", format!("Bot {}", self.bot_token))
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| ChannelError::connection_failed(format!("Get current user failed: {}", e)))?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ChannelError::auth_failed("Invalid bot token"));
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return self.handle_rate_limit(response).await;
        }

        let user: DiscordUser = response
            .json()
            .await
            .map_err(|e| ChannelError::Internal(format!("Failed to parse user response: {}", e)))?;

        Ok(user)
    }

    /// Send a message to a channel
    pub async fn post_message(
        &self,
        channel_id: &str,
        content: &str,
        embeds: Vec<DiscordEmbed>,
    ) -> Result<DiscordMessage, ChannelError> {
        let payload = MessagePayload {
            content: content.to_string(),
            embeds,
            tts: None,
        };

        let response = self
            .client
            .post(format!("{}/channels/{}/messages", DISCORD_API_URL, channel_id))
            .header("Authorization", format!("Bot {}", self.bot_token))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| ChannelError::send_failed(format!("Post message failed: {}", e)))?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ChannelError::auth_failed("Invalid bot token"));
        }

        if status == reqwest::StatusCode::FORBIDDEN {
            return Err(ChannelError::send_failed("Missing permissions to send message"));
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(ChannelError::send_failed("Channel not found"));
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return self.handle_rate_limit(response).await;
        }

        let message: DiscordMessage = response
            .json()
            .await
            .map_err(|e| ChannelError::Internal(format!("Failed to parse message response: {}", e)))?;

        Ok(message)
    }

    /// Get channel information
    pub async fn get_channel(&self, channel_id: &str) -> Result<DiscordChannelInfo, ChannelError> {
        let response = self
            .client
            .get(format!("{}/channels/{}", DISCORD_API_URL, channel_id))
            .header("Authorization", format!("Bot {}", self.bot_token))
            .send()
            .await
            .map_err(|e| ChannelError::connection_failed(format!("Get channel failed: {}", e)))?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(ChannelError::not_found(&format!("Channel {}", channel_id)));
        }

        let channel: DiscordChannelInfo = response
            .json()
            .await
            .map_err(|e| ChannelError::Internal(format!("Failed to parse channel response: {}", e)))?;

        Ok(channel)
    }

    /// Get guild (server) information
    pub async fn get_guild(&self, guild_id: &str) -> Result<DiscordGuild, ChannelError> {
        let response = self
            .client
            .get(format!("{}/guilds/{}", DISCORD_API_URL, guild_id))
            .header("Authorization", format!("Bot {}", self.bot_token))
            .send()
            .await
            .map_err(|e| ChannelError::connection_failed(format!("Get guild failed: {}", e)))?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(ChannelError::not_found(&format!("Guild {}", guild_id)));
        }

        let guild: DiscordGuild = response
            .json()
            .await
            .map_err(|e| ChannelError::Internal(format!("Failed to parse guild response: {}", e)))?;

        Ok(guild)
    }

    /// Handle rate limit response
    async fn handle_rate_limit<T>(&self, response: reqwest::Response) -> Result<T, ChannelError> {
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<f64>().ok())
            .map(|s| s as u64);

        // Try to parse the response body for more info
        let body = response.text().await.unwrap_or_default();
        debug!("Rate limited by Discord: {}", body);

        Err(ChannelError::rate_limit(retry_after))
    }
}

// ============================================================================
// Gateway Types
// ============================================================================

/// Gateway opcodes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
enum GatewayOp {
    Dispatch = 0,
    Heartbeat = 1,
    Identify = 2,
    PresenceUpdate = 3,
    VoiceStateUpdate = 4,
    Resume = 6,
    Reconnect = 7,
    RequestGuildMembers = 8,
    InvalidSession = 9,
    Hello = 10,
    HeartbeatAck = 11,
}

/// Gateway event types
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "t", content = "d")]
pub enum GatewayEvent {
    #[serde(rename = "READY")]
    Ready(ReadyEvent),
    #[serde(rename = "MESSAGE_CREATE")]
    MessageCreate(DiscordMessage),
    #[serde(rename = "MESSAGE_UPDATE")]
    MessageUpdate(DiscordMessage),
    #[serde(rename = "MESSAGE_DELETE")]
    MessageDelete(MessageDeleteEvent),
    #[serde(rename = "GUILD_CREATE")]
    GuildCreate(DiscordGuild),
    #[serde(rename = "CHANNEL_CREATE")]
    ChannelCreate(DiscordChannelInfo),
    #[serde(other)]
    Other,
}

/// Gateway message payload
#[derive(Debug, Clone, Deserialize)]
struct GatewayMessage {
    op: i32,
    #[serde(default)]
    s: Option<i32>,
    #[serde(default)]
    t: Option<String>,
    #[serde(default)]
    d: Option<serde_json::Value>,
}

/// Hello event payload
#[derive(Debug, Clone, Deserialize)]
struct HelloPayload {
    heartbeat_interval: u64,
}

/// Ready event payload
#[derive(Debug, Clone, Deserialize)]
pub struct ReadyEvent {
    pub v: i32,
    pub user: DiscordUser,
    pub guilds: Vec<UnavailableGuild>,
    pub session_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnavailableGuild {
    pub id: String,
    #[serde(default)]
    pub unavailable: bool,
}

/// Message delete event
#[derive(Debug, Clone, Deserialize)]
pub struct MessageDeleteEvent {
    pub id: String,
    pub channel_id: String,
    #[serde(default)]
    pub guild_id: Option<String>,
}

/// Identify payload
#[derive(Debug, Clone, Serialize)]
struct IdentifyPayload {
    token: String,
    properties: IdentifyProperties,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    compress: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    large_threshold: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shard: Option<[i32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    presence: Option<GatewayPresence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    intents: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
struct IdentifyProperties {
    os: String,
    browser: String,
    device: String,
}

#[derive(Debug, Clone, Serialize)]
struct GatewayPresence {
    status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    activities: Option<Vec<serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    afk: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    since: Option<i64>,
}

// Gateway intents
const GATEWAY_INTENT_GUILDS: i32 = 1 << 0;
const GATEWAY_INTENT_GUILD_MESSAGES: i32 = 1 << 9;
const GATEWAY_INTENT_MESSAGE_CONTENT: i32 = 1 << 15;

// ============================================================================
// Discord Gateway Client
// ============================================================================

/// Gateway connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GatewayState {
    Disconnected,
    Connecting,
    Connected,
}

/// Discord Gateway client for real-time events
pub struct DiscordGateway {
    bot_token: String,
    state: Arc<RwLock<GatewayState>>,
    message_handler: Option<Arc<RwLock<Box<dyn MessageHandler>>>>,
    channel_id: String,
    enabled_guilds: Vec<String>,
    enabled_channels: Vec<String>,
    bot_user_id: Option<String>,
    stop_tx: Option<mpsc::Sender<()>>,
}

impl DiscordGateway {
    /// Create a new Gateway client
    pub fn new(bot_token: String) -> Self {
        Self {
            bot_token,
            state: Arc::new(RwLock::new(GatewayState::Disconnected)),
            message_handler: None,
            channel_id: String::new(),
            enabled_guilds: Vec::new(),
            enabled_channels: Vec::new(),
            bot_user_id: None,
            stop_tx: None,
        }
    }

    /// Set the channel ID for message routing
    pub fn with_channel_id(mut self, channel_id: String) -> Self {
        self.channel_id = channel_id;
        self
    }

    /// Set enabled guilds
    pub fn with_enabled_guilds(mut self, guilds: Vec<String>) -> Self {
        self.enabled_guilds = guilds;
        self
    }

    /// Set enabled channels
    pub fn with_enabled_channels(mut self, channels: Vec<String>) -> Self {
        self.enabled_channels = channels;
        self
    }

    /// Set the bot user ID for filtering own messages
    pub fn with_bot_user_id(mut self, user_id: String) -> Self {
        self.bot_user_id = Some(user_id);
        self
    }

    /// Set the message handler
    pub fn with_message_handler(mut self, handler: Arc<RwLock<Box<dyn MessageHandler>>>) -> Self {
        self.message_handler = Some(handler);
        self
    }

    /// Set message handler after construction
    pub fn set_message_handler(&mut self, handler: Arc<RwLock<Box<dyn MessageHandler>>>) {
        self.message_handler = Some(handler);
    }

    /// Connect to the Gateway
    pub async fn connect(&mut self) -> Result<(), ChannelError> {
        let mut state = self.state.write().await;
        if *state == GatewayState::Connected {
            return Ok(());
        }

        *state = GatewayState::Connecting;
        drop(state);

        info!("Connecting to Discord Gateway...");

        // Create stop channel
        let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
        self.stop_tx = Some(stop_tx);

        // Clone necessary data for the async task
        let bot_token = self.bot_token.clone();
        let channel_id = self.channel_id.clone();
        let enabled_guilds = self.enabled_guilds.clone();
        let enabled_channels = self.enabled_channels.clone();
        let bot_user_id = self.bot_user_id.clone();
        let message_handler = self.message_handler.clone();
        let gateway_state = self.state.clone();

        // Spawn the Gateway connection task
        tokio::spawn(async move {
            match connect_async(DISCORD_GATEWAY_URL).await {
                Ok((ws_stream, _)) => {
                    info!("WebSocket connected to Discord Gateway");

                    let (mut write, mut read) = ws_stream.split();

                    // Wait for Hello
                    let heartbeat_interval = match read.next().await {
                        Some(Ok(WsMessage::Text(text))) => {
                            match serde_json::from_str::<GatewayMessage>(&text) {
                                Ok(msg) if msg.op == GatewayOp::Hello as i32 => {
                                    if let Some(d) = msg.d {
                                        match serde_json::from_value::<HelloPayload>(d) {
                                            Ok(hello) => hello.heartbeat_interval,
                                            Err(e) => {
                                                error!("Failed to parse Hello: {}", e);
                                                return;
                                            }
                                        }
                                    } else {
                                        error!("Hello payload missing data");
                                        return;
                                    }
                                }
                                _ => {
                                    error!("Expected Hello message from Gateway");
                                    return;
                                }
                            }
                        }
                        _ => {
                            error!("Failed to receive Hello from Gateway");
                            return;
                        }
                    };

                    debug!("Gateway heartbeat interval: {}ms", heartbeat_interval);

                    // Send Identify
                    let identify = IdentifyPayload {
                        token: bot_token.clone(),
                        properties: IdentifyProperties {
                            os: "linux".to_string(),
                            browser: "omninova".to_string(),
                            device: "omninova".to_string(),
                        },
                        compress: None,
                        large_threshold: None,
                        shard: None,
                        presence: Some(GatewayPresence {
                            status: "online".to_string(),
                            activities: None,
                            afk: None,
                            since: None,
                        }),
                        intents: Some(
                            GATEWAY_INTENT_GUILDS
                                | GATEWAY_INTENT_GUILD_MESSAGES
                                | GATEWAY_INTENT_MESSAGE_CONTENT,
                        ),
                    };

                    let identify_msg = serde_json::json!({
                        "op": GatewayOp::Identify as i32,
                        "d": identify
                    });

                    if let Err(e) = write.send(WsMessage::Text(identify_msg.to_string())).await {
                        error!("Failed to send Identify: {}", e);
                        return;
                    }

                    debug!("Identify sent to Gateway");

                    // Update state
                    {
                        let mut s = gateway_state.write().await;
                        *s = GatewayState::Connected;
                    }

                    info!("Discord Gateway connected successfully");

                    // Start heartbeat timer
                    let mut heartbeat_interval_timer =
                        tokio::time::interval(Duration::from_millis(heartbeat_interval));

                    // Process events
                    loop {
                        tokio::select! {
                            _ = stop_rx.recv() => {
                                info!("Gateway stop signal received");
                                break;
                            }
                            _ = heartbeat_interval_timer.tick() => {
                                let heartbeat = serde_json::json!({
                                    "op": GatewayOp::Heartbeat as i32,
                                    "d": null
                                });
                                if write.send(WsMessage::Text(heartbeat.to_string())).await.is_err() {
                                    error!("Failed to send heartbeat");
                                    break;
                                }
                                debug!("Heartbeat sent");
                            }
                            msg = read.next() => {
                                match msg {
                                    Some(Ok(WsMessage::Text(text))) => {
                                        if let Ok(gateway_msg) = serde_json::from_str::<GatewayMessage>(&text) {
                                            if gateway_msg.op == GatewayOp::HeartbeatAck as i32 {
                                                debug!("Heartbeat ACK received");
                                                continue;
                                            }

                                            if gateway_msg.op == GatewayOp::Dispatch as i32 {
                                                if let Some(t) = &gateway_msg.t {
                                                    match t.as_str() {
                                                        "MESSAGE_CREATE" | "MESSAGE_UPDATE" => {
                                                            if let Some(d) = &gateway_msg.d {
                                                                if let Ok(discord_msg) = serde_json::from_value::<DiscordMessage>(d.clone()) {
                                                                    Self::handle_message(
                                                                        discord_msg,
                                                                        &channel_id,
                                                                        &enabled_guilds,
                                                                        &enabled_channels,
                                                                        &bot_user_id,
                                                                        &message_handler,
                                                                    ).await;
                                                                }
                                                            }
                                                        }
                                                        "READY" => {
                                                            if let Some(d) = &gateway_msg.d {
                                                                if let Ok(ready) = serde_json::from_value::<ReadyEvent>(d.clone()) {
                                                                    info!("Gateway ready as {}", ready.user.username);
                                                                }
                                                            }
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Some(Ok(WsMessage::Ping(data))) => {
                                        let _ = write.send(WsMessage::Pong(data)).await;
                                    }
                                    Some(Ok(WsMessage::Close(_))) => {
                                        warn!("Gateway connection closed by server");
                                        break;
                                    }
                                    Some(Err(e)) => {
                                        error!("Gateway error: {}", e);
                                        break;
                                    }
                                    None => break,
                                    _ => {}
                                }
                            }
                        }
                    }

                    // Update state on exit
                    {
                        let mut s = gateway_state.write().await;
                        *s = GatewayState::Disconnected;
                    }
                }
                Err(e) => {
                    error!("Failed to connect to Gateway: {}", e);
                    let mut s = gateway_state.write().await;
                    *s = GatewayState::Disconnected;
                }
            }
        });

        Ok(())
    }

    /// Handle incoming Discord message
    async fn handle_message(
        msg: DiscordMessage,
        channel_id: &str,
        enabled_guilds: &[String],
        enabled_channels: &[String],
        bot_user_id: &Option<String>,
        message_handler: &Option<Arc<RwLock<Box<dyn MessageHandler>>>>,
    ) {
        // Skip bot messages (including our own)
        if msg.author.bot.unwrap_or(false) {
            return;
        }

        // Skip our own messages
        if let Some(ref bot_id) = bot_user_id {
            if &msg.author.id == bot_id {
                return;
            }
        }

        // Check guild filter
        if let Some(ref guild_id) = msg.guild_id {
            if !enabled_guilds.is_empty() && !enabled_guilds.contains(guild_id) {
                return;
            }
        }

        // Check channel filter
        if !enabled_channels.is_empty() && !enabled_channels.contains(&msg.channel_id) {
            return;
        }

        // Skip empty messages
        if msg.content.is_empty() {
            return;
        }

        // Create message sender
        let sender = MessageSender::new(&msg.author.id)
            .with_name(&msg.author.username);

        // Create message content
        let content = MessageContent::text(&msg.content);

        // Create incoming message
        let mut incoming = IncomingMessage::new(
            channel_id,
            ChannelKind::Discord,
            sender,
            content,
        );

        // Add Discord-specific metadata
        incoming = incoming
            .with_metadata("discord_channel_id", serde_json::json!(msg.channel_id))
            .with_metadata("discord_message_id", serde_json::json!(msg.id));

        if let Some(ref guild_id) = msg.guild_id {
            incoming = incoming.with_metadata("discord_guild_id", serde_json::json!(guild_id));
        }

        // Check for mentions
        let is_mentioned = !msg.mentions.is_empty() || msg.mention_everyone;
        if is_mentioned {
            incoming = incoming.with_metadata("discord_mentioned", serde_json::json!(true));
        }

        // Invoke message handler
        if let Some(ref handler) = message_handler {
            let h = handler.read().await;
            debug!("Invoking message handler for Discord message from {}", msg.author.username);
            h.handle(incoming).await;
        }
    }

    /// Disconnect from the Gateway
    pub async fn disconnect(&mut self) -> Result<(), ChannelError> {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(()).await;
        }

        let mut state = self.state.write().await;
        *state = GatewayState::Disconnected;

        info!("Discord Gateway disconnected");
        Ok(())
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        let state = self.state.read().await;
        *state == GatewayState::Connected
    }
}

// ============================================================================
// Discord Channel Implementation
// ============================================================================

/// Discord channel adapter
pub struct DiscordChannel {
    /// Channel instance ID
    id: ChannelId,

    /// Discord configuration
    config: DiscordConfig,

    /// Connection status
    status: ChannelStatus,

    /// Discord API client
    api: Option<DiscordApi>,

    /// Message handler
    message_handler: Option<Arc<RwLock<Box<dyn MessageHandler>>>>,

    /// Bot user ID (from get_current_user)
    bot_user_id: Option<String>,

    /// Gateway client (optional)
    gateway: Option<DiscordGateway>,
}

impl DiscordChannel {
    /// Create a new Discord channel instance
    pub fn new(id: impl Into<String>, config: DiscordConfig) -> Self {
        Self {
            id: id.into(),
            config,
            status: ChannelStatus::Disconnected,
            api: None,
            message_handler: None,
            bot_user_id: None,
            gateway: None,
        }
    }

    /// Get the Discord API client
    fn api(&self) -> Result<&DiscordApi, ChannelError> {
        self.api.as_ref().ok_or(ChannelError::NotConnected)
    }

    /// Convert a Discord message to an IncomingMessage
    fn convert_message(&self, discord_msg: DiscordMessage) -> Option<IncomingMessage> {
        // Skip messages from bots (including our own)
        if discord_msg.author.bot.unwrap_or(false) {
            return None;
        }

        // Skip messages without content
        if discord_msg.content.is_empty() {
            return None;
        }

        // Check channel filter
        if !self.config.is_channel_enabled(&discord_msg.channel_id) {
            return None;
        }

        // Check guild filter
        if let Some(ref guild_id) = discord_msg.guild_id {
            if !self.config.is_guild_enabled(guild_id) {
                return None;
            }
        }

        // Create message sender
        let sender = MessageSender::new(&discord_msg.author.id)
            .with_name(&discord_msg.author.username);

        // Create message content
        let content = MessageContent::text(&discord_msg.content);

        // Create incoming message
        let mut message = IncomingMessage::new(
            &self.id,
            ChannelKind::Discord,
            sender,
            content,
        );

        // Add Discord-specific metadata
        message = message
            .with_metadata("discord_channel_id", serde_json::json!(discord_msg.channel_id))
            .with_metadata("discord_message_id", serde_json::json!(discord_msg.id));

        if let Some(ref guild_id) = discord_msg.guild_id {
            message = message.with_metadata("discord_guild_id", serde_json::json!(guild_id));
        }

        // Check for mentions
        let is_mentioned = !discord_msg.mentions.is_empty() || discord_msg.mention_everyone;
        if is_mentioned {
            message = message.with_metadata("discord_mentioned", serde_json::json!(true));
        }

        Some(message)
    }
}

#[async_trait]
impl Channel for DiscordChannel {
    fn id(&self) -> &str {
        &self.id
    }

    fn channel_kind(&self) -> ChannelKind {
        ChannelKind::Discord
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
        let api = DiscordApi::new(self.config.bot_token.clone());

        // Get current bot user
        let user = api.get_current_user().await?;

        info!("Discord connected as {} ({})", user.username, user.id);

        // Store bot info
        self.bot_user_id = Some(user.id.clone());

        // Store API client
        self.api = Some(api);

        // Start Gateway if enabled
        if self.config.gateway_enabled {
            info!("Starting Discord Gateway connection");

            let mut gateway = DiscordGateway::new(self.config.bot_token.clone())
                .with_channel_id(self.id.clone())
                .with_enabled_guilds(self.config.enabled_guilds.clone())
                .with_enabled_channels(self.config.enabled_channels.clone())
                .with_bot_user_id(user.id);

            // Set message handler if available
            if let Some(ref handler) = self.message_handler {
                gateway = gateway.with_message_handler(handler.clone());
            }

            gateway.connect().await?;
            self.gateway = Some(gateway);

            info!("Discord Gateway connected successfully");
        }

        self.status = ChannelStatus::Connected;

        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), ChannelError> {
        // Disconnect Gateway if active
        if let Some(ref mut gateway) = self.gateway {
            gateway.disconnect().await?;
        }
        self.gateway = None;

        self.status = ChannelStatus::Disconnected;
        self.api = None;
        self.bot_user_id = None;

        info!("Discord channel {} disconnected", self.id);
        Ok(())
    }

    async fn send_message(&self, message: OutgoingMessage) -> Result<MessageId, ChannelError> {
        let api = self.api()?;

        if !matches!(self.status, ChannelStatus::Connected) {
            return Err(ChannelError::NotConnected);
        }

        // Extract text content and embeds
        let (text, embeds) = match &message.content {
            MessageContent::Text { text } => (text.clone(), Vec::new()),
            MessageContent::RichText { text, .. } => (text.clone(), Vec::new()),
            _ => {
                return Err(ChannelError::InvalidMessage(
                    "Only text and rich text messages are supported".to_string(),
                ));
            }
        };

        // Post message
        let response = api.post_message(&message.target_id, &text, embeds).await?;

        debug!(
            "Sent message to Discord channel {}: message_id={}",
            response.channel_id, response.id
        );

        Ok(response.id)
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
            | ChannelCapabilities::CHANNEL_MESSAGE
    }

    fn set_message_handler(&mut self, handler: Box<dyn MessageHandler>) {
        let handler_arc = Arc::new(RwLock::new(handler));
        self.message_handler = Some(handler_arc.clone());

        // Update Gateway handler if already connected
        if let Some(ref mut gateway) = self.gateway {
            gateway.set_message_handler(handler_arc);
        }
    }
}

// ============================================================================
// Discord Channel Factory
// ============================================================================

/// Factory for creating Discord channel instances
pub struct DiscordChannelFactory;

impl DiscordChannelFactory {
    /// Create a new Discord channel factory
    pub fn new() -> Self {
        Self
    }
}

impl Default for DiscordChannelFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelFactory for DiscordChannelFactory {
    fn channel_kind(&self) -> ChannelKind {
        ChannelKind::Discord
    }

    fn create(&self, config: ChannelConfig) -> Result<Box<dyn Channel>, ChannelError> {
        // Extract Discord configuration from credentials
        let discord_config = match config.credentials {
            Credentials::BotToken { ref token } => {
                let mut discord_config = DiscordConfig::new(token.clone());

                // Extract additional settings from extra field
                if !config.settings.extra.is_null() {
                    if let Ok(extra) = serde_json::from_value::<DiscordConfigExtra>(config.settings.extra.clone()) {
                        if let Some(application_id) = extra.application_id {
                            discord_config = discord_config.with_application_id(application_id);
                        }
                        if let Some(enabled_guilds) = extra.enabled_guilds {
                            discord_config = discord_config.with_guilds(enabled_guilds);
                        }
                        if let Some(enabled_channels) = extra.enabled_channels {
                            discord_config = discord_config.with_channels(enabled_channels);
                        }
                        discord_config = discord_config.with_gateway(extra.gateway_enabled.unwrap_or(true));
                    }
                }

                discord_config
            }
            Credentials::ApiKey { ref key, .. } => {
                // Treat API key as bot token for Discord
                DiscordConfig::new(key.clone())
            }
            Credentials::OAuth2 { .. } => {
                return Err(ChannelError::config(
                    "OAuth2 credentials not supported for Discord",
                ));
            }
            Credentials::UsernamePassword { .. } => {
                return Err(ChannelError::config(
                    "Username/password credentials not supported for Discord",
                ));
            }
            Credentials::Webhook { .. } => {
                return Err(ChannelError::config(
                    "Webhook credentials not supported for Discord",
                ));
            }
            Credentials::None => {
                return Err(ChannelError::config(
                    "Discord requires bot token credentials",
                ));
            }
        };

        // Create channel ID from config name
        let channel_id = if config.name.is_empty() {
            format!("discord-{}", uuid::Uuid::new_v4())
        } else {
            config.name.clone()
        };

        Ok(Box::new(DiscordChannel::new(channel_id, discord_config)))
    }
}

/// Extra configuration for Discord
#[derive(Debug, Clone, Deserialize)]
struct DiscordConfigExtra {
    #[serde(default)]
    application_id: Option<String>,
    #[serde(default)]
    enabled_guilds: Option<Vec<String>>,
    #[serde(default)]
    enabled_channels: Option<Vec<String>>,
    #[serde(default)]
    gateway_enabled: Option<bool>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discord_config_creation() {
        let config = DiscordConfig::new("test_token_123456789012345678901234567890123456789012345678");
        assert_eq!(config.bot_token, "test_token_123456789012345678901234567890123456789012345678");
        assert!(config.gateway_enabled);
        assert!(config.enabled_guilds.is_empty());
        assert!(config.enabled_channels.is_empty());
    }

    #[test]
    fn test_discord_config_validation() {
        let valid_config = DiscordConfig::new("test_token_123456789012345678901234567890123456789012345678");
        assert!(valid_config.validate().is_ok());

        let empty_config = DiscordConfig::new("");
        assert!(empty_config.validate().is_err());

        let short_config = DiscordConfig::new("short");
        assert!(short_config.validate().is_err());
    }

    #[test]
    fn test_discord_config_builder() {
        let config = DiscordConfig::new("test_token_123456789012345678901234567890123456789012345678")
            .with_application_id("123456789")
            .with_guilds(vec!["guild1".to_string()])
            .with_channels(vec!["channel1".to_string()])
            .with_gateway(false);

        assert_eq!(config.application_id, Some("123456789".to_string()));
        assert_eq!(config.enabled_guilds, vec!["guild1"]);
        assert_eq!(config.enabled_channels, vec!["channel1"]);
        assert!(!config.gateway_enabled);
    }

    #[test]
    fn test_discord_config_filters() {
        let config = DiscordConfig::new("test_token_123456789012345678901234567890123456789012345678")
            .with_guilds(vec!["guild1".to_string()])
            .with_channels(vec!["channel1".to_string()]);

        // Test guild filter
        assert!(config.is_guild_enabled("guild1"));
        assert!(!config.is_guild_enabled("guild2"));

        // Test channel filter
        assert!(config.is_channel_enabled("channel1"));
        assert!(!config.is_channel_enabled("channel2"));

        // Test empty filters (accept all)
        let empty_config = DiscordConfig::new("test_token_123456789012345678901234567890123456789012345678");
        assert!(empty_config.is_guild_enabled("any_guild"));
        assert!(empty_config.is_channel_enabled("any_channel"));
    }

    #[test]
    fn test_discord_channel_factory() {
        let factory = DiscordChannelFactory::new();
        assert_eq!(factory.channel_kind(), ChannelKind::Discord);

        let config = ChannelConfig {
            id: "test-id".to_string(),
            name: "test-discord".to_string(),
            channel_kind: ChannelKind::Discord,
            credentials: Credentials::BotToken {
                token: "test_token_123456789012345678901234567890123456789012345678".to_string(),
            },
            settings: Default::default(),
            enabled: true,
        };

        let channel = factory.create(config);
        assert!(channel.is_ok());
    }

    #[test]
    fn test_discord_channel_creation() {
        let config = DiscordConfig::new("test_token_123456789012345678901234567890123456789012345678");
        let channel = DiscordChannel::new("test-channel", config);

        assert_eq!(channel.id(), "test-channel");
        assert_eq!(channel.channel_kind(), ChannelKind::Discord);
        assert_eq!(channel.get_status(), ChannelStatus::Disconnected);
    }

    #[test]
    fn test_discord_channel_capabilities() {
        let config = DiscordConfig::new("test_token_123456789012345678901234567890123456789012345678");
        let channel = DiscordChannel::new("test-channel", config);

        let caps = channel.capabilities();
        assert!(caps.contains(ChannelCapabilities::TEXT));
        assert!(caps.contains(ChannelCapabilities::RICH_TEXT));
        assert!(caps.contains(ChannelCapabilities::THREADS));
        assert!(caps.contains(ChannelCapabilities::MENTIONS));
        assert!(caps.contains(ChannelCapabilities::REACTIONS));
        assert!(caps.contains(ChannelCapabilities::CHANNEL_MESSAGE));
    }

    #[test]
    fn test_message_conversion() {
        let config = DiscordConfig::new("test_token_123456789012345678901234567890123456789012345678");
        let channel = DiscordChannel::new("test-channel", config);

        let discord_msg = DiscordMessage {
            id: "123456789".to_string(),
            channel_id: "987654321".to_string(),
            guild_id: Some("111222333".to_string()),
            author: DiscordUser {
                id: "user123".to_string(),
                username: "TestUser".to_string(),
                discriminator: "0001".to_string(),
                avatar: None,
                bot: Some(false),
            },
            content: "Hello, world!".to_string(),
            mentions: vec![],
            mention_everyone: false,
            timestamp: Some("2024-01-01T00:00:00.000Z".to_string()),
            edited_timestamp: None,
            tts: false,
            pinned: false,
            embeds: vec![],
        };

        let incoming = channel.convert_message(discord_msg);
        assert!(incoming.is_some());

        let msg = incoming.unwrap();
        assert_eq!(msg.sender.id, "user123");
        assert_eq!(msg.sender.name, Some("TestUser".to_string()));
    }

    #[test]
    fn test_message_conversion_filters_bots() {
        let config = DiscordConfig::new("test_token_123456789012345678901234567890123456789012345678");
        let channel = DiscordChannel::new("test-channel", config);

        let bot_msg = DiscordMessage {
            id: "123456789".to_string(),
            channel_id: "987654321".to_string(),
            guild_id: None,
            author: DiscordUser {
                id: "bot123".to_string(),
                username: "TestBot".to_string(),
                discriminator: "0000".to_string(),
                avatar: None,
                bot: Some(true),
            },
            content: "Bot message".to_string(),
            mentions: vec![],
            mention_everyone: false,
            timestamp: None,
            edited_timestamp: None,
            tts: false,
            pinned: false,
            embeds: vec![],
        };

        let incoming = channel.convert_message(bot_msg);
        assert!(incoming.is_none());
    }

    #[test]
    fn test_embed_serialization() {
        let embed = DiscordEmbed {
            title: Some("Test Title".to_string()),
            description: Some("Test Description".to_string()),
            color: Some(0x3498db),
            ..Default::default()
        };

        let json = serde_json::to_string(&embed).unwrap();
        assert!(json.contains("Test Title"));
        assert!(json.contains("Test Description"));
        assert!(json.contains("3447003")); // 0x3498db in decimal
    }

    #[test]
    fn test_config_extra_deserialization() {
        let json = serde_json::json!({
            "application_id": "123456789",
            "enabled_guilds": ["guild1", "guild2"],
            "enabled_channels": ["channel1"],
            "gateway_enabled": false
        });

        let extra: DiscordConfigExtra = serde_json::from_value(json).unwrap();
        assert_eq!(extra.application_id, Some("123456789".to_string()));
        assert_eq!(extra.enabled_guilds, Some(vec!["guild1".to_string(), "guild2".to_string()]));
        assert_eq!(extra.enabled_channels, Some(vec!["channel1".to_string()]));
        assert_eq!(extra.gateway_enabled, Some(false));
    }
}