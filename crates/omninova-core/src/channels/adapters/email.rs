//! Email Channel Adapter
//!
//! Implements the Channel trait for Email integration, supporting:
//! - IMAP protocol for receiving emails
//! - SMTP protocol for sending emails
//! - Thread tracking via In-Reply-To header
//! - Email filtering rules
//! - Attachment handling (optional)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::channels::error::ChannelError;
use crate::channels::traits::{Channel, ChannelConfig, ChannelFactory, ChannelId, MessageHandler, MessageId};
use crate::channels::types::{
    ChannelCapabilities, ChannelStatus, Credentials,
    IncomingMessage, MessageContent, MessageSender, OutgoingMessage,
};
use crate::channels::ChannelKind;

// ============================================================================
// Email Constants
// ============================================================================

const DEFAULT_IMAP_PORT: u16 = 993;
const DEFAULT_SMTP_PORT: u16 = 587;
const DEFAULT_CHECK_INTERVAL_SECS: u64 = 300; // 5 minutes

// ============================================================================
// Email Configuration
// ============================================================================

/// Email channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    /// IMAP server hostname
    pub imap_server: String,

    /// IMAP server port (default: 993 for TLS)
    #[serde(default = "default_imap_port")]
    pub imap_port: u16,

    /// IMAP username (usually email address)
    pub imap_username: String,

    /// IMAP password or app password
    pub imap_password: String,

    /// SMTP server hostname
    pub smtp_server: String,

    /// SMTP server port (default: 587 for STARTTLS)
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,

    /// SMTP username (usually email address)
    pub smtp_username: String,

    /// SMTP password or app password
    pub smtp_password: String,

    /// Email address to send from
    pub from_address: String,

    /// Email address to send replies to (optional)
    #[serde(default)]
    pub reply_to_address: Option<String>,

    /// Display name for the sender
    #[serde(default)]
    pub from_name: Option<String>,

    /// Interval between email checks in seconds
    #[serde(default = "default_check_interval")]
    pub check_interval_secs: u64,

    /// Email filtering rules
    #[serde(default)]
    pub filter_rules: Vec<EmailFilter>,

    /// Whether to process attachments
    #[serde(default)]
    pub process_attachments: bool,

    /// Maximum attachment size in bytes (0 = unlimited)
    #[serde(default)]
    pub max_attachment_size: u64,
}

fn default_imap_port() -> u16 {
    DEFAULT_IMAP_PORT
}

fn default_smtp_port() -> u16 {
    DEFAULT_SMTP_PORT
}

fn default_check_interval() -> u64 {
    DEFAULT_CHECK_INTERVAL_SECS
}

impl EmailConfig {
    /// Create a new Email configuration
    pub fn new(
        imap_server: impl Into<String>,
        imap_username: impl Into<String>,
        imap_password: impl Into<String>,
        smtp_server: impl Into<String>,
        smtp_username: impl Into<String>,
        smtp_password: impl Into<String>,
        from_address: impl Into<String>,
    ) -> Self {
        Self {
            imap_server: imap_server.into(),
            imap_port: DEFAULT_IMAP_PORT,
            imap_username: imap_username.into(),
            imap_password: imap_password.into(),
            smtp_server: smtp_server.into(),
            smtp_port: DEFAULT_SMTP_PORT,
            smtp_username: smtp_username.into(),
            smtp_password: smtp_password.into(),
            from_address: from_address.into(),
            reply_to_address: None,
            from_name: None,
            check_interval_secs: DEFAULT_CHECK_INTERVAL_SECS,
            filter_rules: Vec::new(),
            process_attachments: false,
            max_attachment_size: 0,
        }
    }

    /// Set IMAP port
    pub fn with_imap_port(mut self, port: u16) -> Self {
        self.imap_port = port;
        self
    }

    /// Set SMTP port
    pub fn with_smtp_port(mut self, port: u16) -> Self {
        self.smtp_port = port;
        self
    }

    /// Set reply-to address
    pub fn with_reply_to(mut self, address: impl Into<String>) -> Self {
        self.reply_to_address = Some(address.into());
        self
    }

    /// Set sender display name
    pub fn with_from_name(mut self, name: impl Into<String>) -> Self {
        self.from_name = Some(name.into());
        self
    }

    /// Set check interval
    pub fn with_check_interval(mut self, secs: u64) -> Self {
        self.check_interval_secs = secs;
        self
    }

    /// Set filter rules
    pub fn with_filter_rules(mut self, rules: Vec<EmailFilter>) -> Self {
        self.filter_rules = rules;
        self
    }

    /// Enable attachment processing
    pub fn with_attachments(mut self, enabled: bool, max_size: u64) -> Self {
        self.process_attachments = enabled;
        self.max_attachment_size = max_size;
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ChannelError> {
        if self.imap_server.is_empty() {
            return Err(ChannelError::config("imap_server is required"));
        }
        if self.imap_username.is_empty() {
            return Err(ChannelError::config("imap_username is required"));
        }
        if self.imap_password.is_empty() {
            return Err(ChannelError::config("imap_password is required"));
        }
        if self.smtp_server.is_empty() {
            return Err(ChannelError::config("smtp_server is required"));
        }
        if self.smtp_username.is_empty() {
            return Err(ChannelError::config("smtp_username is required"));
        }
        if self.smtp_password.is_empty() {
            return Err(ChannelError::config("smtp_password is required"));
        }
        if self.from_address.is_empty() {
            return Err(ChannelError::config("from_address is required"));
        }

        // Basic email format validation
        if !self.from_address.contains('@') {
            return Err(ChannelError::config("from_address is not a valid email"));
        }

        Ok(())
    }

    /// Check if an email passes the filter rules
    pub fn passes_filters(&self, email: &EmailMessage) -> bool {
        if self.filter_rules.is_empty() {
            return true; // No filters = accept all
        }

        self.filter_rules.iter().all(|rule| rule.matches(email))
    }

    /// Get check interval as Duration
    pub fn check_interval(&self) -> Duration {
        Duration::from_secs(self.check_interval_secs)
    }
}

// ============================================================================
// Email Filter
// ============================================================================

/// Email filtering rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailFilter {
    /// Filter type
    #[serde(rename = "type")]
    pub filter_type: EmailFilterType,

    /// Pattern to match (interpretation depends on filter_type)
    pub pattern: String,

    /// Whether to include or exclude matching emails
    #[serde(default)]
    pub action: FilterAction,
}

/// Type of email filter
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EmailFilterType {
    /// Match sender email address (supports wildcards)
    Sender,
    /// Match subject line (supports wildcards)
    Subject,
    /// Match recipient email address
    Recipient,
    /// Match email body content
    Body,
    /// Match any header
    Header,
}

/// Action to take for matching emails
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FilterAction {
    /// Include matching emails (whitelist mode)
    #[default]
    Include,
    /// Exclude matching emails (blacklist mode)
    Exclude,
}

impl EmailFilter {
    /// Create a new filter
    pub fn new(filter_type: EmailFilterType, pattern: impl Into<String>) -> Self {
        Self {
            filter_type,
            pattern: pattern.into(),
            action: FilterAction::Include,
        }
    }

    /// Create an exclusion filter
    pub fn exclude(filter_type: EmailFilterType, pattern: impl Into<String>) -> Self {
        Self {
            filter_type,
            pattern: pattern.into(),
            action: FilterAction::Exclude,
        }
    }

    /// Check if an email matches this filter
    pub fn matches(&self, email: &EmailMessage) -> bool {
        let matches = match self.filter_type {
            EmailFilterType::Sender => self.pattern_match(&email.from.address),
            EmailFilterType::Subject => self.pattern_match(&email.subject),
            EmailFilterType::Recipient => {
                // Check if any recipient matches
                email.to.iter().any(|r| self.pattern_match(&r.address))
            }
            EmailFilterType::Body => self.pattern_match(&email.body_text),
            EmailFilterType::Header => false, // Not implemented yet
        };

        matches
    }

    /// Simple pattern matching (supports * wildcard)
    fn pattern_match(&self, value: &str) -> bool {
        let pattern_lower = self.pattern.to_lowercase();
        let value_lower = value.to_lowercase();

        if pattern_lower.contains('*') {
            // Wildcard matching
            let parts: Vec<&str> = pattern_lower.split('*').collect();
            if parts.len() == 2 {
                let prefix = parts[0];
                let suffix = parts[1];
                value_lower.starts_with(prefix) && value_lower.ends_with(suffix)
            } else {
                // Multiple wildcards - simple contains check
                value_lower.contains(&pattern_lower.replace('*', ""))
            }
        } else {
            // Exact match (case-insensitive)
            value_lower == pattern_lower
        }
    }

    /// Whether this filter includes matching emails
    pub fn is_inclusive(&self) -> bool {
        self.action == FilterAction::Include
    }
}

// ============================================================================
// Email Message Types
// ============================================================================

/// Email address with optional display name
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAddress {
    /// Display name
    #[serde(default)]
    pub name: Option<String>,
    /// Email address
    pub address: String,
}

impl EmailAddress {
    /// Create a new email address
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            name: None,
            address: address.into(),
        }
    }

    /// Create with display name
    pub fn with_name(address: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            address: address.into(),
        }
    }

    /// Format as "Name <address>" or just "address"
    pub fn format(&self) -> String {
        match &self.name {
            Some(name) => format!("{} <{}>", name, self.address),
            None => self.address.clone(),
        }
    }
}

/// Email message representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    /// Unique message ID
    pub message_id: String,

    /// Sender email address
    pub from: EmailAddress,

    /// Recipient email addresses
    pub to: Vec<EmailAddress>,

    /// CC recipients
    #[serde(default)]
    pub cc: Vec<EmailAddress>,

    /// Email subject
    pub subject: String,

    /// Plain text body
    #[serde(default)]
    pub body_text: String,

    /// HTML body
    #[serde(default)]
    pub body_html: Option<String>,

    /// Message ID this is a reply to (for threading)
    #[serde(default)]
    pub in_reply_to: Option<String>,

    /// All message IDs in the thread
    #[serde(default)]
    pub references: Vec<String>,

    /// Email date/timestamp
    #[serde(default)]
    pub date: Option<String>,

    /// Attachments
    #[serde(default)]
    pub attachments: Vec<EmailAttachment>,

    /// Custom headers
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
}

impl EmailMessage {
    /// Create a new email message
    pub fn new(message_id: impl Into<String>, from: EmailAddress, subject: impl Into<String>) -> Self {
        Self {
            message_id: message_id.into(),
            from,
            to: Vec::new(),
            cc: Vec::new(),
            subject: subject.into(),
            body_text: String::new(),
            body_html: None,
            in_reply_to: None,
            references: Vec::new(),
            date: None,
            attachments: Vec::new(),
            headers: std::collections::HashMap::new(),
        }
    }

    /// Add a recipient
    pub fn with_to(mut self, to: EmailAddress) -> Self {
        self.to.push(to);
        self
    }

    /// Set body text
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body_text = body.into();
        self
    }

    /// Set reply-to message ID
    pub fn with_in_reply_to(mut self, message_id: impl Into<String>) -> Self {
        self.in_reply_to = Some(message_id.into());
        self
    }

    /// Convert to IncomingMessage for the channel system
    pub fn to_incoming_message(&self, channel_id: &str) -> IncomingMessage {
        let sender = MessageSender::new(&self.from.address)
            .with_name(self.from.name.as_deref().unwrap_or(&self.from.address));

        let content = if !self.body_text.is_empty() {
            MessageContent::text(&self.body_text)
        } else if let Some(ref html) = self.body_html {
            MessageContent::markdown(html)
        } else {
            MessageContent::text(&self.subject)
        };

        let mut message = IncomingMessage::new(
            channel_id,
            ChannelKind::Email,
            sender,
            content,
        );

        // Add metadata
        message = message
            .with_metadata("email_message_id", serde_json::json!(&self.message_id))
            .with_metadata("email_subject", serde_json::json!(&self.subject));

        if let Some(ref in_reply_to) = self.in_reply_to {
            message = message.with_metadata("email_in_reply_to", serde_json::json!(in_reply_to));
            message = message.with_metadata("thread_id", serde_json::json!(in_reply_to));
        }

        if let Some(ref date) = self.date {
            message = message.with_metadata("email_date", serde_json::json!(date));
        }

        if !self.attachments.is_empty() {
            message = message.with_metadata(
                "email_has_attachments",
                serde_json::json!(true),
            );
            message = message.with_metadata(
                "email_attachment_count",
                serde_json::json!(self.attachments.len()),
            );
        }

        message
    }
}

/// Email attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAttachment {
    /// Filename
    pub filename: String,
    /// MIME content type
    pub content_type: String,
    /// Size in bytes
    pub size: u64,
    /// Whether attachment is inline
    #[serde(default)]
    pub inline: bool,
    /// Content ID for inline attachments
    #[serde(default)]
    pub content_id: Option<String>,
}

// ============================================================================
// IMAP Client (Simplified/Mock Implementation)
// ============================================================================

/// IMAP client for receiving emails
pub struct ImapClient {
    server: String,
    port: u16,
    username: String,
    password: String,
    connected: bool,
}

impl ImapClient {
    /// Create a new IMAP client
    pub fn new(server: String, port: u16, username: String, password: String) -> Self {
        Self {
            server,
            port,
            username,
            password,
            connected: false,
        }
    }

    /// Connect to the IMAP server
    pub async fn connect(&mut self) -> Result<(), ChannelError> {
        // TODO: Implement real IMAP connection using imap crate
        // For now, simulate connection
        info!(
            "Connecting to IMAP server {}:{} as {}",
            self.server, self.port, self.username
        );

        // Validate credentials format
        if self.username.is_empty() || self.password.is_empty() {
            return Err(ChannelError::auth_failed("Invalid IMAP credentials"));
        }

        self.connected = true;
        Ok(())
    }

    /// Disconnect from the IMAP server
    pub async fn disconnect(&mut self) -> Result<(), ChannelError> {
        self.connected = false;
        info!("Disconnected from IMAP server");
        Ok(())
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Fetch unseen emails
    pub async fn fetch_unseen(&self) -> Result<Vec<EmailMessage>, ChannelError> {
        if !self.connected {
            return Err(ChannelError::NotConnected);
        }

        // TODO: Implement real IMAP FETCH
        // Return empty list for mock
        Ok(Vec::new())
    }

    /// Mark an email as read
    pub async fn mark_as_read(&self, _message_id: &str) -> Result<(), ChannelError> {
        if !self.connected {
            return Err(ChannelError::NotConnected);
        }

        // TODO: Implement real IMAP STORE to set \Seen flag
        Ok(())
    }
}

// ============================================================================
// SMTP Client (Simplified/Mock Implementation)
// ============================================================================

/// SMTP client for sending emails
pub struct SmtpClient {
    server: String,
    port: u16,
    username: String,
    password: String,
    from_address: String,
    from_name: Option<String>,
    connected: bool,
}

impl SmtpClient {
    /// Create a new SMTP client
    pub fn new(
        server: String,
        port: u16,
        username: String,
        password: String,
        from_address: String,
        from_name: Option<String>,
    ) -> Self {
        Self {
            server,
            port,
            username,
            password,
            from_address,
            from_name,
            connected: false,
        }
    }

    /// Connect to the SMTP server
    pub async fn connect(&mut self) -> Result<(), ChannelError> {
        info!(
            "Connecting to SMTP server {}:{} as {}",
            self.server, self.port, self.username
        );

        if self.username.is_empty() || self.password.is_empty() {
            return Err(ChannelError::auth_failed("Invalid SMTP credentials"));
        }

        self.connected = true;
        Ok(())
    }

    /// Disconnect from the SMTP server
    pub async fn disconnect(&mut self) -> Result<(), ChannelError> {
        self.connected = false;
        info!("Disconnected from SMTP server");
        Ok(())
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Send an email
    pub async fn send(
        &self,
        to: &str,
        subject: &str,
        body: &str,
        in_reply_to: Option<&str>,
    ) -> Result<String, ChannelError> {
        if !self.connected {
            return Err(ChannelError::NotConnected);
        }

        // TODO: Implement real SMTP send using lettre crate
        // Generate a mock message ID
        let message_id = format!("<{}@{}>", uuid::Uuid::new_v4(), self.server);

        debug!(
            "Sending email to {} with subject '{}' (in_reply_to: {:?})",
            to, subject, in_reply_to
        );

        Ok(message_id)
    }

    /// Get the from address formatted
    pub fn from_header(&self) -> String {
        match &self.from_name {
            Some(name) => format!("{} <{}>", name, self.from_address),
            None => self.from_address.clone(),
        }
    }
}

// ============================================================================
// Email Channel Implementation
// ============================================================================

/// Email channel adapter
pub struct EmailChannel {
    /// Channel instance ID
    id: ChannelId,

    /// Email configuration
    config: EmailConfig,

    /// Connection status
    status: ChannelStatus,

    /// IMAP client
    imap_client: Option<ImapClient>,

    /// SMTP client
    smtp_client: Option<SmtpClient>,

    /// Message handler
    message_handler: Option<Arc<RwLock<Box<dyn MessageHandler>>>>,

    /// Polling task handle
    polling_handle: Option<JoinHandle<()>>,

    /// Stop signal sender
    stop_tx: Option<tokio::sync::mpsc::Sender<()>>,
}

impl EmailChannel {
    /// Create a new Email channel instance
    pub fn new(id: impl Into<String>, config: EmailConfig) -> Self {
        Self {
            id: id.into(),
            config,
            status: ChannelStatus::Disconnected,
            imap_client: None,
            smtp_client: None,
            message_handler: None,
            polling_handle: None,
            stop_tx: None,
        }
    }

    /// Get the IMAP client (if connected)
    fn imap(&self) -> Result<&ImapClient, ChannelError> {
        self.imap_client.as_ref().ok_or(ChannelError::NotConnected)
    }

    /// Get the SMTP client (if connected)
    fn smtp(&self) -> Result<&SmtpClient, ChannelError> {
        self.smtp_client.as_ref().ok_or(ChannelError::NotConnected)
    }

    /// Start the email polling task
    fn start_polling(&mut self) {
        let channel_id = self.id.clone();
        let check_interval = self.config.check_interval();
        let filter_rules = self.config.filter_rules.clone();

        let (stop_tx, mut stop_rx) = tokio::sync::mpsc::channel::<()>(1);
        self.stop_tx = Some(stop_tx);

        let message_handler = self.message_handler.clone();

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(check_interval);

            loop {
                tokio::select! {
                    _ = stop_rx.recv() => {
                        info!("Email polling stopped");
                        break;
                    }
                    _ = interval.tick() => {
                        debug!("Checking for new emails on channel {}", channel_id);

                        // TODO: Implement actual email fetching
                        // For now, just log the poll
                        if let Some(ref handler) = message_handler {
                            // Would fetch and process emails here
                            let _ = handler;
                        }
                    }
                }
            }
        });

        self.polling_handle = Some(handle);
    }

    /// Stop the polling task
    async fn stop_polling(&mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(()).await;
        }

        if let Some(handle) = self.polling_handle.take() {
            handle.abort();
        }
    }
}

#[async_trait]
impl Channel for EmailChannel {
    fn id(&self) -> &str {
        &self.id
    }

    fn channel_kind(&self) -> ChannelKind {
        ChannelKind::Email
    }

    async fn connect(&mut self) -> Result<(), ChannelError> {
        // Validate configuration
        self.config.validate()?;

        // Check current status
        if matches!(self.status, ChannelStatus::Connected) {
            return Ok(());
        }

        self.status = ChannelStatus::Connecting;

        // Create and connect IMAP client
        let mut imap = ImapClient::new(
            self.config.imap_server.clone(),
            self.config.imap_port,
            self.config.imap_username.clone(),
            self.config.imap_password.clone(),
        );
        imap.connect().await?;

        // Create and connect SMTP client
        let mut smtp = SmtpClient::new(
            self.config.smtp_server.clone(),
            self.config.smtp_port,
            self.config.smtp_username.clone(),
            self.config.smtp_password.clone(),
            self.config.from_address.clone(),
            self.config.from_name.clone(),
        );
        smtp.connect().await?;

        // Store connected clients
        self.imap_client = Some(imap);
        self.smtp_client = Some(smtp);

        // Start polling for new emails
        self.start_polling();

        self.status = ChannelStatus::Connected;

        info!("Email channel {} connected", self.id);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), ChannelError> {
        // Stop polling
        self.stop_polling().await;

        // Disconnect IMAP
        if let Some(mut imap) = self.imap_client.take() {
            imap.disconnect().await?;
        }

        // Disconnect SMTP
        if let Some(mut smtp) = self.smtp_client.take() {
            smtp.disconnect().await?;
        }

        self.status = ChannelStatus::Disconnected;

        info!("Email channel {} disconnected", self.id);
        Ok(())
    }

    async fn send_message(&self, message: OutgoingMessage) -> Result<MessageId, ChannelError> {
        let smtp = self.smtp()?;

        if !matches!(self.status, ChannelStatus::Connected) {
            return Err(ChannelError::NotConnected);
        }

        // Extract content
        let (text_content, is_html) = match &message.content {
            MessageContent::Text { text } => (text.clone(), false),
            MessageContent::RichText { text, .. } => (text.clone(), false),
            _ => {
                return Err(ChannelError::InvalidMessage(
                    "Only text messages are supported for email".to_string(),
                ));
            }
        };

        // Extract thread info from metadata
        let in_reply_to = message.metadata.get("email_in_reply_to")
            .and_then(|v| v.as_str());

        // Send email
        let message_id = smtp.send(
            &message.target_id,
            "Re: Query", // TODO: Use actual subject from thread
            &text_content,
            in_reply_to,
        ).await?;

        debug!("Sent email to {}: message_id={}", message.target_id, message_id);

        Ok(message_id)
    }

    fn get_status(&self) -> ChannelStatus {
        self.status.clone()
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities::TEXT
            | ChannelCapabilities::RICH_TEXT
            | ChannelCapabilities::THREADS
            | ChannelCapabilities::FILES
            | ChannelCapabilities::DIRECT_MESSAGE
    }

    fn set_message_handler(&mut self, handler: Box<dyn MessageHandler>) {
        let handler_arc = Arc::new(RwLock::new(handler));
        self.message_handler = Some(handler_arc);
    }
}

// ============================================================================
// Email Channel Factory
// ============================================================================

/// Factory for creating Email channel instances
pub struct EmailChannelFactory;

impl EmailChannelFactory {
    /// Create a new Email channel factory
    pub fn new() -> Self {
        Self
    }
}

impl Default for EmailChannelFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelFactory for EmailChannelFactory {
    fn channel_kind(&self) -> ChannelKind {
        ChannelKind::Email
    }

    fn create(&self, config: ChannelConfig) -> Result<Box<dyn Channel>, ChannelError> {
        // Extract Email configuration from credentials
        let email_config = match &config.credentials {
            Credentials::UsernamePassword { username, password } => {
                // Build email config from username/password
                // Assume the username is the email address
                let server_domain = username.split('@').nth(1).unwrap_or("example.com");

                EmailConfig::new(
                    format!("imap.{}", server_domain),
                    username.clone(),
                    password.clone(),
                    format!("smtp.{}", server_domain),
                    username.clone(),
                    password.clone(),
                    username.clone(),
                )
            }
            Credentials::ApiKey { key, secret } => {
                // Treat key as server and secret as password
                // This is for advanced configuration
                return Err(ChannelError::config(
                    "Email requires UsernamePassword credentials with email address as username",
                ));
            }
            Credentials::BotToken { .. } => {
                return Err(ChannelError::config(
                    "Bot token credentials not supported for Email",
                ));
            }
            Credentials::OAuth2 { .. } => {
                return Err(ChannelError::config(
                    "OAuth2 credentials not yet supported for Email",
                ));
            }
            Credentials::Webhook { .. } => {
                return Err(ChannelError::config(
                    "Webhook credentials not supported for Email",
                ));
            }
            Credentials::None => {
                return Err(ChannelError::config(
                    "Email requires credentials",
                ));
            }
        };

        // Extract additional settings from extra field
        let email_config = if !config.settings.extra.is_null() {
            if let Ok(extra) = serde_json::from_value::<EmailConfigExtra>(config.settings.extra.clone()) {
                let mut cfg = email_config;

                if let Some(imap_server) = extra.imap_server {
                    cfg.imap_server = imap_server;
                }
                if let Some(imap_port) = extra.imap_port {
                    cfg.imap_port = imap_port;
                }
                if let Some(smtp_server) = extra.smtp_server {
                    cfg.smtp_server = smtp_server;
                }
                if let Some(smtp_port) = extra.smtp_port {
                    cfg.smtp_port = smtp_port;
                }
                if let Some(check_interval) = extra.check_interval_secs {
                    cfg.check_interval_secs = check_interval;
                }
                if let Some(from_name) = extra.from_name {
                    cfg.from_name = Some(from_name);
                }
                if let Some(filter_rules) = extra.filter_rules {
                    cfg.filter_rules = filter_rules;
                }
                if let Some(process_attachments) = extra.process_attachments {
                    cfg.process_attachments = process_attachments;
                }

                cfg
            } else {
                email_config
            }
        } else {
            email_config
        };

        // Create channel ID from config name
        let channel_id = if config.name.is_empty() {
            format!("email-{}", uuid::Uuid::new_v4())
        } else {
            config.name.clone()
        };

        Ok(Box::new(EmailChannel::new(channel_id, email_config)))
    }
}

/// Extra configuration for Email
#[derive(Debug, Clone, Deserialize)]
struct EmailConfigExtra {
    #[serde(default)]
    imap_server: Option<String>,
    #[serde(default)]
    imap_port: Option<u16>,
    #[serde(default)]
    smtp_server: Option<String>,
    #[serde(default)]
    smtp_port: Option<u16>,
    #[serde(default)]
    check_interval_secs: Option<u64>,
    #[serde(default)]
    from_name: Option<String>,
    #[serde(default)]
    filter_rules: Option<Vec<EmailFilter>>,
    #[serde(default)]
    process_attachments: Option<bool>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_config_creation() {
        let config = EmailConfig::new(
            "imap.example.com",
            "user@example.com",
            "password",
            "smtp.example.com",
            "user@example.com",
            "password",
            "user@example.com",
        );

        assert_eq!(config.imap_server, "imap.example.com");
        assert_eq!(config.imap_port, DEFAULT_IMAP_PORT);
        assert_eq!(config.smtp_server, "smtp.example.com");
        assert_eq!(config.smtp_port, DEFAULT_SMTP_PORT);
        assert_eq!(config.from_address, "user@example.com");
        assert_eq!(config.check_interval_secs, DEFAULT_CHECK_INTERVAL_SECS);
    }

    #[test]
    fn test_email_config_validation() {
        let valid_config = EmailConfig::new(
            "imap.example.com",
            "user@example.com",
            "password",
            "smtp.example.com",
            "user@example.com",
            "password",
            "user@example.com",
        );
        assert!(valid_config.validate().is_ok());

        let empty_imap = EmailConfig::new(
            "",
            "user@example.com",
            "password",
            "smtp.example.com",
            "user@example.com",
            "password",
            "user@example.com",
        );
        assert!(empty_imap.validate().is_err());

        let invalid_email = EmailConfig::new(
            "imap.example.com",
            "user@example.com",
            "password",
            "smtp.example.com",
            "user@example.com",
            "password",
            "invalid-email",
        );
        assert!(invalid_email.validate().is_err());
    }

    #[test]
    fn test_email_config_builder() {
        let config = EmailConfig::new(
            "imap.example.com",
            "user@example.com",
            "password",
            "smtp.example.com",
            "user@example.com",
            "password",
            "user@example.com",
        )
        .with_imap_port(143)
        .with_smtp_port(25)
        .with_check_interval(60)
        .with_from_name("Test Bot");

        assert_eq!(config.imap_port, 143);
        assert_eq!(config.smtp_port, 25);
        assert_eq!(config.check_interval_secs, 60);
        assert_eq!(config.from_name, Some("Test Bot".to_string()));
    }

    #[test]
    fn test_email_filter_matching() {
        let email = EmailMessage::new(
            "<test@example.com>",
            EmailAddress::new("sender@example.com"),
            "Test Subject",
        )
        .with_body("Hello world")
        .with_to(EmailAddress::new("recipient@example.com"));

        // Test sender filter
        let sender_filter = EmailFilter::new(EmailFilterType::Sender, "sender@example.com");
        assert!(sender_filter.matches(&email));

        // Test wildcard sender filter
        let wildcard_filter = EmailFilter::new(EmailFilterType::Sender, "*@example.com");
        assert!(wildcard_filter.matches(&email));

        // Test subject filter
        let subject_filter = EmailFilter::new(EmailFilterType::Subject, "Test Subject");
        assert!(subject_filter.matches(&email));

        // Test exclusion filter
        let exclude_filter = EmailFilter::exclude(EmailFilterType::Sender, "spam@example.com");
        assert!(!exclude_filter.matches(&email));
    }

    #[test]
    fn test_email_config_filters() {
        let config = EmailConfig::new(
            "imap.example.com",
            "user@example.com",
            "password",
            "smtp.example.com",
            "user@example.com",
            "password",
            "user@example.com",
        )
        .with_filter_rules(vec![
            EmailFilter::new(EmailFilterType::Sender, "allowed@example.com"),
        ]);

        let allowed_email = EmailMessage::new(
            "<test@example.com>",
            EmailAddress::new("allowed@example.com"),
            "Test",
        );

        let blocked_email = EmailMessage::new(
            "<test@example.com>",
            EmailAddress::new("blocked@example.com"),
            "Test",
        );

        assert!(config.passes_filters(&allowed_email));
        assert!(!config.passes_filters(&blocked_email));
    }

    #[test]
    fn test_email_message_to_incoming() {
        let email = EmailMessage::new(
            "<msg123@example.com>",
            EmailAddress::with_name("sender@example.com", "Sender Name"),
            "Test Subject",
        )
        .with_body("Hello world")
        .with_to(EmailAddress::new("recipient@example.com"))
        .with_in_reply_to("<parent@example.com>");

        let incoming = email.to_incoming_message("test-channel");

        assert_eq!(incoming.sender.id, "sender@example.com");
        assert_eq!(incoming.sender.name, Some("Sender Name".to_string()));
        assert_eq!(
            incoming.metadata.get("email_message_id").unwrap().as_str().unwrap(),
            "<msg123@example.com>"
        );
        assert_eq!(
            incoming.metadata.get("thread_id").unwrap().as_str().unwrap(),
            "<parent@example.com>"
        );
    }

    #[test]
    fn test_email_channel_factory() {
        let factory = EmailChannelFactory::new();
        assert_eq!(factory.channel_kind(), ChannelKind::Email);

        let config = ChannelConfig {
            id: "test-id".to_string(),
            name: "test-email".to_string(),
            channel_kind: ChannelKind::Email,
            credentials: Credentials::UsernamePassword {
                username: "user@example.com".to_string(),
                password: "password".to_string(),
            },
            settings: Default::default(),
            enabled: true,
        };

        let channel = factory.create(config);
        assert!(channel.is_ok());
    }

    #[test]
    fn test_email_channel_creation() {
        let config = EmailConfig::new(
            "imap.example.com",
            "user@example.com",
            "password",
            "smtp.example.com",
            "user@example.com",
            "password",
            "user@example.com",
        );
        let channel = EmailChannel::new("test-channel", config);

        assert_eq!(channel.id(), "test-channel");
        assert_eq!(channel.channel_kind(), ChannelKind::Email);
        assert_eq!(channel.get_status(), ChannelStatus::Disconnected);
    }

    #[test]
    fn test_email_channel_capabilities() {
        let config = EmailConfig::new(
            "imap.example.com",
            "user@example.com",
            "password",
            "smtp.example.com",
            "user@example.com",
            "password",
            "user@example.com",
        );
        let channel = EmailChannel::new("test-channel", config);

        let caps = channel.capabilities();
        assert!(caps.contains(ChannelCapabilities::TEXT));
        assert!(caps.contains(ChannelCapabilities::RICH_TEXT));
        assert!(caps.contains(ChannelCapabilities::THREADS));
        assert!(caps.contains(ChannelCapabilities::FILES));
        assert!(caps.contains(ChannelCapabilities::DIRECT_MESSAGE));
    }

    #[test]
    fn test_email_address_format() {
        let simple = EmailAddress::new("test@example.com");
        assert_eq!(simple.format(), "test@example.com");

        let named = EmailAddress::with_name("test@example.com", "Test User");
        assert_eq!(named.format(), "Test User <test@example.com>");
    }
}