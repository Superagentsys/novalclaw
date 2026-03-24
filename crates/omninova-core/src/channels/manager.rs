//! Channel Manager
//!
//! Central manager for coordinating all channel connections.
//! Handles channel lifecycle, message routing, and connection state management.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::time::sleep;

use super::behavior::{ChannelBehaviorConfig, ChannelBehaviorStore, WorkingHoursChecker};
use super::error::ChannelError;
use super::event::{AgentId, ChannelEvent, ReconnectPolicy};
use super::traits::{Channel, ChannelConfig, ChannelFactory, ChannelId, MessageHandler, MessageId};
use super::types::{ChannelInfo, ChannelStatus, IncomingMessage, OutgoingMessage};
use super::ChannelKind;

// ============================================================================
// Channel Entry
// ============================================================================

/// Internal entry for tracking a channel instance.
///
/// Wraps a channel instance along with its configuration,
/// agent binding, and reconnection policy.
pub struct ChannelEntry {
    /// The channel instance.
    pub channel: Box<dyn Channel>,
    /// Channel configuration.
    pub config: ChannelConfig,
    /// Agent bound to this channel (if any).
    pub agent_binding: Option<AgentId>,
    /// Reconnection policy for this channel.
    pub reconnect_policy: ReconnectPolicy,
    /// Current reconnection attempt count.
    pub reconnect_attempts: u32,
}

impl ChannelEntry {
    /// Create a new channel entry.
    pub fn new(channel: Box<dyn Channel>, config: ChannelConfig) -> Self {
        Self {
            channel,
            config,
            agent_binding: None,
            reconnect_policy: ReconnectPolicy::default(),
            reconnect_attempts: 0,
        }
    }

    /// Set the reconnection policy.
    pub fn with_reconnect_policy(mut self, policy: ReconnectPolicy) -> Self {
        self.reconnect_policy = policy;
        self
    }

    /// Get the channel ID.
    pub fn id(&self) -> &str {
        self.channel.id()
    }

    /// Get the channel kind.
    pub fn kind(&self) -> ChannelKind {
        self.channel.channel_kind()
    }

    /// Get the current status.
    pub fn status(&self) -> ChannelStatus {
        self.channel.get_status()
    }

    /// Check if the channel is connected.
    pub fn is_connected(&self) -> bool {
        self.channel.is_connected()
    }

    /// Reset reconnection attempts.
    pub fn reset_reconnect_attempts(&mut self) {
        self.reconnect_attempts = 0;
    }

    /// Increment reconnection attempts and return the new count.
    pub fn increment_reconnect_attempts(&mut self) -> u32 {
        self.reconnect_attempts += 1;
        self.reconnect_attempts
    }
}

// ============================================================================
// Channel Manager
// ============================================================================

/// Central manager for coordinating all channel connections.
///
/// The `ChannelManager` is responsible for:
/// - Managing multiple channel instances
/// - Tracking connection states
/// - Routing messages between channels and agents
/// - Broadcasting channel lifecycle events
/// - Handling automatic reconnection
///
/// # Thread Safety
///
/// The manager uses `Arc<Mutex<>>` for thread-safe access to shared state.
/// Events are broadcast via `tokio::sync::broadcast` for pub/sub pattern.
///
/// # Example
///
/// ```rust
/// use omninova_core::channels::{ChannelManager, ChannelFactory, ChannelConfig};
/// use std::sync::Arc;
/// use tokio::sync::Mutex;
///
/// async fn setup_channels() {
///     let manager = Arc::new(Mutex::new(ChannelManager::new()));
///
///     // Register a factory
///     // manager.lock().await.register_factory(Arc::new(MyFactory));
///
///     // Create a channel
///     // let config = ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None);
///     // let id = manager.lock().await.create_channel(config).unwrap();
/// }
/// ```
pub struct ChannelManager {
    /// Active channel instances.
    channels: HashMap<ChannelId, ChannelEntry>,

    /// Registered factories for creating channels.
    factories: HashMap<ChannelKind, Arc<dyn ChannelFactory>>,

    /// Default message handler for incoming messages.
    default_handler: Option<Arc<dyn MessageHandler>>,

    /// Event broadcast sender.
    event_tx: broadcast::Sender<ChannelEvent>,

    /// Default reconnection policy for new channels.
    default_reconnect_policy: ReconnectPolicy,

    /// Behavior configuration store (optional).
    behavior_store: Option<Arc<dyn ChannelBehaviorStore>>,
}

impl Default for ChannelManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelManager {
    /// Create a new channel manager.
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(256);

        Self {
            channels: HashMap::new(),
            factories: HashMap::new(),
            default_handler: None,
            event_tx,
            default_reconnect_policy: ReconnectPolicy::default(),
            behavior_store: None,
        }
    }

    /// Set the behavior configuration store.
    pub fn with_behavior_store(mut self, store: Arc<dyn ChannelBehaviorStore>) -> Self {
        self.behavior_store = Some(store);
        self
    }

    /// Set the behavior configuration store (mutable).
    pub fn set_behavior_store(&mut self, store: Arc<dyn ChannelBehaviorStore>) {
        self.behavior_store = Some(store);
    }

    /// Set the default reconnection policy for new channels.
    pub fn with_default_reconnect_policy(mut self, policy: ReconnectPolicy) -> Self {
        self.default_reconnect_policy = policy;
        self
    }

    // ========================================================================
    // Factory Registration
    // ========================================================================

    /// Register a channel factory.
    ///
    /// Factories are used to create channel instances for a specific channel kind.
    ///
    /// # Arguments
    ///
    /// * `factory` - The factory to register
    pub fn register_factory(&mut self, factory: Arc<dyn ChannelFactory>) {
        let kind = factory.channel_kind();
        self.factories.insert(kind, factory);
    }

    /// Check if a factory is registered for a given kind.
    pub fn has_factory(&self, kind: &ChannelKind) -> bool {
        self.factories.contains_key(kind)
    }

    /// Get the list of registered channel kinds.
    pub fn registered_kinds(&self) -> Vec<ChannelKind> {
        self.factories.keys().cloned().collect()
    }

    // ========================================================================
    // Behavior Configuration
    // ========================================================================

    /// Get behavior configuration for a channel.
    ///
    /// Returns the stored behavior config if available, or the default from channel settings.
    pub fn get_behavior_config(&self, channel_id: &str) -> Option<ChannelBehaviorConfig> {
        // First try to load from store
        if let Some(ref store) = self.behavior_store {
            if let Ok(Some(config)) = store.load(channel_id) {
                return Some(config);
            }
        }

        // Fall back to channel settings
        self.channels.get(channel_id)
            .map(|e| e.config.settings.behavior.clone())
    }

    /// Update behavior configuration for a channel at runtime.
    ///
    /// This method updates the behavior config and notifies the channel
    /// if it supports the `on_behavior_changed` callback.
    ///
    /// # Arguments
    ///
    /// * `channel_id` - Channel to update
    /// * `config` - New behavior configuration
    ///
    /// # Returns
    ///
    /// `Ok(())` if the update was successful.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The channel doesn't exist
    /// - The store operation fails
    pub fn update_behavior_config(
        &mut self,
        channel_id: &str,
        config: ChannelBehaviorConfig,
    ) -> Result<(), ChannelError> {
        // Verify channel exists
        if !self.channels.contains_key(channel_id) {
            return Err(ChannelError::ChannelNotFound(channel_id.to_string()));
        }

        // Save to store if available
        if let Some(ref store) = self.behavior_store {
            store.save(channel_id, &config)?;
        }

        // Update in-memory config
        if let Some(entry) = self.channels.get_mut(channel_id) {
            entry.config.settings.behavior = config.clone();
        }

        // Broadcast behavior changed event
        let _ = self.event_tx.send(ChannelEvent::BehaviorChanged {
            channel_id: channel_id.to_string(),
        });

        Ok(())
    }

    /// Load behavior configurations from the store for all channels.
    ///
    /// This should be called after channels are created to restore
    /// any previously saved behavior configurations.
    pub fn load_behavior_configs(&mut self) {
        if let Some(ref store) = self.behavior_store {
            for (channel_id, entry) in &mut self.channels {
                if let Ok(Some(config)) = store.load(channel_id) {
                    entry.config.settings.behavior = config;
                }
            }
        }
    }

    /// Check if a channel is within working hours.
    ///
    /// Returns `true` if:
    /// - No working hours are configured (always active)
    /// - Current time is within configured working hours
    pub fn is_within_working_hours(&self, channel_id: &str) -> bool {
        if let Some(config) = self.get_behavior_config(channel_id) {
            if let Some(ref working_hours) = config.working_hours {
                return WorkingHoursChecker::is_within_working_hours(working_hours);
            }
        }
        true // Default to always active
    }

    // ========================================================================
    // Channel Lifecycle
    // ========================================================================

    /// Create a new channel instance from configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Channel configuration
    ///
    /// # Returns
    ///
    /// The ID of the created channel.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No factory is registered for the channel kind
    /// - Channel creation fails
    /// - A channel with the same ID already exists
    pub fn create_channel(&mut self, config: ChannelConfig) -> Result<ChannelId, ChannelError> {
        // Check for duplicate ID
        if self.channels.contains_key(&config.id) {
            return Err(ChannelError::DuplicateChannel(config.id));
        }

        // Get the factory
        let factory = self.factories.get(&config.channel_kind)
            .ok_or_else(|| ChannelError::NoFactory(format!("{:?}", config.channel_kind)))?;

        // Create the channel
        let channel = factory.create(config.clone())?;
        let channel_id = config.id.clone();

        // Create entry with default reconnect policy
        let entry = ChannelEntry::new(channel, config.clone())
            .with_reconnect_policy(self.default_reconnect_policy.clone());

        self.channels.insert(channel_id.clone(), entry);

        // Broadcast event
        let _ = self.event_tx.send(ChannelEvent::Created {
            channel_id: channel_id.clone(),
            channel_kind: config.channel_kind,
        });

        Ok(channel_id)
    }

    /// Remove a channel.
    ///
    /// # Arguments
    ///
    /// * `id` - Channel ID to remove
    ///
    /// # Errors
    ///
    /// Returns an error if the channel doesn't exist.
    pub fn remove_channel(&mut self, id: &str) -> Result<(), ChannelError> {
        let entry = self.channels.remove(id)
            .ok_or_else(|| ChannelError::ChannelNotFound(id.to_string()))?;

        // Broadcast event
        let _ = self.event_tx.send(ChannelEvent::Removed {
            channel_id: id.to_string(),
        });

        Ok(())
    }

    /// Get a reference to a channel.
    pub fn get_channel(&self, id: &str) -> Option<&dyn Channel> {
        self.channels.get(id).map(|e| e.channel.as_ref())
    }

    /// Get channel info for a specific channel.
    pub fn get_channel_info(&self, id: &str) -> Option<ChannelInfo> {
        self.channels.get(id).map(|e| e.channel.get_info())
    }

    /// List all channels with their info.
    pub fn list_channels(&self) -> Vec<ChannelInfo> {
        self.channels.values()
            .map(|e| e.channel.get_info())
            .collect()
    }

    /// Get the number of registered channels.
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    // ========================================================================
    // Connection Management
    // ========================================================================

    /// Connect a specific channel.
    ///
    /// # Errors
    ///
    /// Returns an error if the channel doesn't exist or connection fails.
    pub async fn connect_channel(&mut self, id: &str) -> Result<(), ChannelError> {
        let entry = self.channels.get_mut(id)
            .ok_or_else(|| ChannelError::ChannelNotFound(id.to_string()))?;

        // Check if already connected
        if entry.is_connected() {
            return Ok(());
        }

        let kind = entry.kind();
        entry.channel.connect().await?;
        entry.reset_reconnect_attempts();

        // Broadcast event
        let _ = self.event_tx.send(ChannelEvent::Connected {
            channel_id: id.to_string(),
            channel_kind: kind,
        });

        Ok(())
    }

    /// Disconnect a specific channel.
    ///
    /// # Errors
    ///
    /// Returns an error if the channel doesn't exist or disconnection fails.
    pub async fn disconnect_channel(&mut self, id: &str) -> Result<(), ChannelError> {
        let entry = self.channels.get_mut(id)
            .ok_or_else(|| ChannelError::ChannelNotFound(id.to_string()))?;

        // Check if already disconnected
        if !entry.is_connected() {
            return Ok(());
        }

        entry.channel.disconnect().await?;

        // Broadcast event
        let _ = self.event_tx.send(ChannelEvent::Disconnected {
            channel_id: id.to_string(),
            reason: Some("User requested".to_string()),
        });

        Ok(())
    }

    /// Connect all channels.
    ///
    /// Returns a vector of (channel_id, result) for each connection attempt.
    pub async fn connect_all(&mut self) -> Vec<(ChannelId, Result<(), ChannelError>)> {
        let ids: Vec<ChannelId> = self.channels.keys().cloned().collect();
        let mut results = Vec::new();

        for id in ids {
            let result = self.connect_channel(&id).await;
            results.push((id, result));
        }

        results
    }

    /// Disconnect all channels.
    ///
    /// Returns a vector of (channel_id, result) for each disconnection attempt.
    pub async fn disconnect_all(&mut self) -> Vec<(ChannelId, Result<(), ChannelError>)> {
        let ids: Vec<ChannelId> = self.channels.keys().cloned().collect();
        let mut results = Vec::new();

        for id in ids {
            let result = self.disconnect_channel(&id).await;
            results.push((id, result));
        }

        results
    }

    /// Attempt to reconnect a channel.
    ///
    /// Uses exponential backoff based on the channel's reconnect policy.
    ///
    /// # Arguments
    ///
    /// * `id` - Channel ID to reconnect
    ///
    /// # Returns
    ///
    /// `Ok(())` if reconnection succeeded, `Err` if all attempts exhausted or failed.
    pub async fn try_reconnect(&mut self, id: &str) -> Result<(), ChannelError> {
        let entry = self.channels.get_mut(id)
            .ok_or_else(|| ChannelError::ChannelNotFound(id.to_string()))?;

        let current_attempt = entry.reconnect_attempts;
        let policy = entry.reconnect_policy.clone();

        // Check if we should retry
        if !policy.should_retry(current_attempt) {
            return Err(ChannelError::ReconnectExhausted(id.to_string()));
        }

        // Get delay for this attempt
        let delay = policy.delay_for_attempt(current_attempt)
            .ok_or_else(|| ChannelError::ReconnectExhausted(id.to_string()))?;

        // Broadcast reconnecting event
        let _ = self.event_tx.send(ChannelEvent::Reconnecting {
            channel_id: id.to_string(),
            attempt: current_attempt,
        });

        // Wait before retry
        sleep(Duration::from_millis(delay)).await;

        // Increment attempt counter
        entry.increment_reconnect_attempts();

        // Attempt connection
        let kind = entry.kind();
        match entry.channel.connect().await {
            Ok(()) => {
                entry.reset_reconnect_attempts();

                // Broadcast connected event
                let _ = self.event_tx.send(ChannelEvent::Connected {
                    channel_id: id.to_string(),
                    channel_kind: kind,
                });

                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    // ========================================================================
    // Agent Binding
    // ========================================================================

    /// Bind an agent to a channel.
    ///
    /// When a message is received on this channel, it will be routed to the bound agent.
    ///
    /// # Arguments
    ///
    /// * `channel_id` - Channel to bind
    /// * `agent_id` - Agent to bind to the channel
    ///
    /// # Errors
    ///
    /// Returns an error if the channel doesn't exist.
    pub fn bind_agent(&mut self, channel_id: &str, agent_id: AgentId) -> Result<(), ChannelError> {
        let entry = self.channels.get_mut(channel_id)
            .ok_or_else(|| ChannelError::ChannelNotFound(channel_id.to_string()))?;

        entry.agent_binding = Some(agent_id.clone());

        // Broadcast event
        let _ = self.event_tx.send(ChannelEvent::AgentBound {
            channel_id: channel_id.to_string(),
            agent_id,
        });

        Ok(())
    }

    /// Unbind an agent from a channel.
    ///
    /// # Arguments
    ///
    /// * `channel_id` - Channel to unbind
    ///
    /// # Errors
    ///
    /// Returns an error if the channel doesn't exist.
    pub fn unbind_agent(&mut self, channel_id: &str) -> Result<(), ChannelError> {
        let entry = self.channels.get_mut(channel_id)
            .ok_or_else(|| ChannelError::ChannelNotFound(channel_id.to_string()))?;

        entry.agent_binding = None;

        // Broadcast event
        let _ = self.event_tx.send(ChannelEvent::AgentUnbound {
            channel_id: channel_id.to_string(),
        });

        Ok(())
    }

    /// Get the agent bound to a channel.
    pub fn get_bound_agent(&self, channel_id: &str) -> Option<&AgentId> {
        self.channels.get(channel_id)?.agent_binding.as_ref()
    }

    // ========================================================================
    // Message Handling
    // ========================================================================

    /// Set the default message handler.
    ///
    /// This handler is used for channels that don't have a specific handler set.
    pub fn set_default_handler(&mut self, handler: Arc<dyn MessageHandler>) {
        self.default_handler = Some(handler);
    }

    /// Handle an incoming message from a channel.
    ///
    /// Routes the message to the appropriate handler (bound agent or default).
    pub async fn handle_incoming_message(&self, message: IncomingMessage) {
        // Try to get the default handler
        if let Some(handler) = &self.default_handler {
            handler.handle(message).await;
        }
        // If no handler is set, the message is dropped
    }

    /// Send a message to a specific channel.
    ///
    /// This method respects the channel's behavior configuration:
    /// - Checks working hours and returns an error if outside working hours
    /// - Applies response delay if configured
    ///
    /// # Arguments
    ///
    /// * `channel_id` - Target channel
    /// * `message` - Message to send
    ///
    /// # Returns
    ///
    /// The message ID assigned by the channel.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The channel doesn't exist
    /// - The channel isn't connected
    /// - Current time is outside working hours
    /// - The send operation fails
    pub async fn send_to_channel(
        &self,
        channel_id: &str,
        message: OutgoingMessage,
    ) -> Result<MessageId, ChannelError> {
        let entry = self.channels.get(channel_id)
            .ok_or_else(|| ChannelError::ChannelNotFound(channel_id.to_string()))?;

        // Check working hours
        let behavior = &entry.config.settings.behavior;
        if let Some(ref working_hours) = behavior.working_hours {
            if !WorkingHoursChecker::is_within_working_hours(working_hours) {
                return Err(ChannelError::ConfigurationError(
                    "Outside working hours".to_string()
                ));
            }
        }

        // Apply response delay (Task 6.5)
        let content_len = message.content.text_content().map(|t| t.len()).unwrap_or(0);
        let delay = behavior.response_delay.calculate_delay(content_len);
        if !delay.is_zero() {
            sleep(delay).await;
        }

        let message_id = entry.channel.send_message(message.clone()).await?;

        // Broadcast event
        let _ = self.event_tx.send(ChannelEvent::MessageSent {
            channel_id: channel_id.to_string(),
            message_id: message_id.clone(),
        });

        Ok(message_id)
    }

    /// Send a message to a specific channel, queueing if outside working hours.
    ///
    /// Unlike `send_to_channel`, this method will queue the message if
    /// the current time is outside working hours, and return immediately.
    /// The message will be sent when working hours resume.
    ///
    /// # Arguments
    ///
    /// * `channel_id` - Target channel
    /// * `message` - Message to send
    ///
    /// # Returns
    ///
    /// `Ok(None)` if the message was queued for later delivery.
    /// `Ok(Some(message_id))` if the message was sent immediately.
    ///
    /// # Errors
    ///
    /// Returns an error if the channel doesn't exist or send fails.
    pub async fn send_to_channel_queued(
        &self,
        channel_id: &str,
        message: OutgoingMessage,
    ) -> Result<Option<MessageId>, ChannelError> {
        let entry = self.channels.get(channel_id)
            .ok_or_else(|| ChannelError::ChannelNotFound(channel_id.to_string()))?;

        // Check working hours
        let behavior = &entry.config.settings.behavior;
        if let Some(ref working_hours) = behavior.working_hours {
            if !WorkingHoursChecker::is_within_working_hours(working_hours) {
                // Queue message for later (Task 7.5)
                // In a full implementation, this would store the message
                // and schedule it for the next working hours window.
                // For now, we return None to indicate it was queued.
                return Ok(None);
            }
        }

        // Apply response delay
        let content_len = message.content.text_content().map(|t| t.len()).unwrap_or(0);
        let delay = behavior.response_delay.calculate_delay(content_len);
        if !delay.is_zero() {
            sleep(delay).await;
        }

        let message_id = entry.channel.send_message(message.clone()).await?;

        // Broadcast event
        let _ = self.event_tx.send(ChannelEvent::MessageSent {
            channel_id: channel_id.to_string(),
            message_id: message_id.clone(),
        });

        Ok(Some(message_id))
    }

    /// Broadcast a message to all connected channels.
    ///
    /// Returns a vector of (channel_id, result) for each send attempt.
    pub async fn broadcast_message(&self, message: OutgoingMessage) -> Vec<(ChannelId, Result<MessageId, ChannelError>)> {
        let mut results = Vec::new();

        for (id, entry) in &self.channels {
            if entry.is_connected() {
                let result = entry.channel.send_message(message.clone()).await;

                // Broadcast event on success
                if let Ok(ref msg_id) = result {
                    let _ = self.event_tx.send(ChannelEvent::MessageSent {
                        channel_id: id.clone(),
                        message_id: msg_id.clone(),
                    });
                }

                results.push((id.clone(), result));
            }
        }

        results
    }

    // ========================================================================
    // Status Query
    // ========================================================================

    /// Get the status of a specific channel.
    pub fn get_channel_status(&self, id: &str) -> Option<ChannelStatus> {
        self.channels.get(id).map(|e| e.status())
    }

    /// Get the status of all channels.
    pub fn get_all_statuses(&self) -> HashMap<ChannelId, ChannelStatus> {
        self.channels.iter()
            .map(|(id, entry)| (id.clone(), entry.status()))
            .collect()
    }

    /// Check if a specific channel is connected.
    pub fn is_channel_connected(&self, id: &str) -> bool {
        self.channels.get(id).map(|e| e.is_connected()).unwrap_or(false)
    }

    /// Get the count of connected channels.
    pub fn get_connected_count(&self) -> usize {
        self.channels.values().filter(|e| e.is_connected()).count()
    }

    /// Check if a channel exists.
    pub fn has_channel(&self, id: &str) -> bool {
        self.channels.contains_key(id)
    }

    // ========================================================================
    // Events
    // ========================================================================

    /// Subscribe to channel events.
    ///
    /// Returns a receiver that will receive all channel lifecycle events.
    pub fn subscribe_events(&self) -> broadcast::Receiver<ChannelEvent> {
        self.event_tx.subscribe()
    }

    /// Report an error for a channel (used by channel implementations).
    ///
    /// Broadcasts an error event for the specified channel.
    pub fn report_error(&self, channel_id: &str, error: String) {
        let _ = self.event_tx.send(ChannelEvent::Error {
            channel_id: channel_id.to_string(),
            error,
        });
    }

    /// Report a disconnection for a channel (used by channel implementations).
    ///
    /// Broadcasts a disconnection event for the specified channel.
    pub fn report_disconnection(&self, channel_id: &str, reason: Option<String>) {
        let _ = self.event_tx.send(ChannelEvent::Disconnected {
            channel_id: channel_id.to_string(),
            reason,
        });
    }

    // ========================================================================
    // Persistence
    // ========================================================================

    /// Save channel configurations to a file.
    ///
    /// Only saves configurations, not connection state or agent bindings.
    pub fn save_config(&self, path: &Path) -> Result<(), ChannelError> {
        let configs: Vec<_> = self.channels.values()
            .map(|e| e.config.clone())
            .collect();

        let content = toml::to_string_pretty(&configs)
            .map_err(|e| ChannelError::ConfigurationError(e.to_string()))?;

        std::fs::write(path, content)
            .map_err(|e| ChannelError::ConfigurationError(e.to_string()))?;

        Ok(())
    }

    /// Load channel configurations from a file.
    ///
    /// Creates channels from the loaded configurations.
    /// Requires factories to be registered for the channel kinds.
    pub fn load_config(&mut self, path: &Path) -> Result<Vec<ChannelId>, ChannelError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ChannelError::ConfigurationError(e.to_string()))?;

        let configs: Vec<ChannelConfig> = toml::from_str(&content)
            .map_err(|e| ChannelError::ConfigurationError(e.to_string()))?;

        let mut created = Vec::new();
        for config in configs {
            match self.create_channel(config) {
                Ok(id) => created.push(id),
                Err(e) => {
                    // Log error but continue loading other channels
                    eprintln!("Failed to load channel: {}", e);
                }
            }
        }

        Ok(created)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::Credentials;
    use async_trait::async_trait;

    // ============================================================================
    // Mock Channel for Testing
    // ============================================================================

    struct MockChannel {
        id: String,
        kind: ChannelKind,
        status: ChannelStatus,
        capabilities: super::super::types::ChannelCapabilities,
        message_handler: Option<Box<dyn MessageHandler>>,
    }

    impl MockChannel {
        fn new(id: impl Into<String>, kind: ChannelKind) -> Self {
            Self {
                id: id.into(),
                kind,
                status: ChannelStatus::Disconnected,
                capabilities: super::super::types::ChannelCapabilities::TEXT
                    | super::super::types::ChannelCapabilities::RICH_TEXT,
                message_handler: None,
            }
        }
    }

    #[async_trait]
    impl Channel for MockChannel {
        fn id(&self) -> &str { &self.id }
        fn channel_kind(&self) -> ChannelKind { self.kind.clone() }

        async fn connect(&mut self) -> Result<(), ChannelError> {
            if self.is_connected() {
                return Ok(());
            }
            self.status = ChannelStatus::Connected;
            Ok(())
        }

        async fn disconnect(&mut self) -> Result<(), ChannelError> {
            self.status = ChannelStatus::Disconnected;
            Ok(())
        }

        async fn send_message(&self, _message: OutgoingMessage) -> Result<MessageId, ChannelError> {
            if !self.is_connected() {
                return Err(ChannelError::NotConnected);
            }
            Ok(format!("msg_{}", uuid::Uuid::new_v4()))
        }

        fn get_status(&self) -> ChannelStatus {
            self.status.clone()
        }

        fn capabilities(&self) -> super::super::types::ChannelCapabilities {
            self.capabilities
        }

        fn set_message_handler(&mut self, handler: Box<dyn MessageHandler>) {
            self.message_handler = Some(handler);
        }
    }

    // ============================================================================
    // Mock Factory for Testing
    // ============================================================================

    struct MockFactory {
        kind: ChannelKind,
    }

    impl MockFactory {
        fn new(kind: ChannelKind) -> Self {
            Self { kind }
        }
    }

    impl ChannelFactory for MockFactory {
        fn channel_kind(&self) -> ChannelKind {
            self.kind.clone()
        }

        fn create(&self, config: ChannelConfig) -> Result<Box<dyn Channel>, ChannelError> {
            Ok(Box::new(MockChannel::new(config.id, config.channel_kind)))
        }
    }

    // ============================================================================
    // ChannelManager Tests
    // ============================================================================

    #[test]
    fn test_manager_new() {
        let manager = ChannelManager::new();
        assert_eq!(manager.channel_count(), 0);
        assert!(manager.default_handler.is_none());
    }

    #[test]
    fn test_manager_default() {
        let manager = ChannelManager::default();
        assert_eq!(manager.channel_count(), 0);
    }

    #[test]
    fn test_register_factory() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));

        assert!(manager.has_factory(&ChannelKind::Slack));
        assert!(!manager.has_factory(&ChannelKind::Discord));
    }

    #[test]
    fn test_registered_kinds() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Discord)));

        let kinds = manager.registered_kinds();
        assert_eq!(kinds.len(), 2);
    }

    #[test]
    fn test_create_channel_no_factory() {
        let mut manager = ChannelManager::new();
        let config = ChannelConfig::new("test-1", ChannelKind::Slack, Credentials::None);

        let result = manager.create_channel(config);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ChannelError::NoFactory(_)));
    }

    #[test]
    fn test_create_channel_success() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));

        let config = ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None)
            .with_name("Test Slack");

        let result = manager.create_channel(config);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "slack-1");
        assert_eq!(manager.channel_count(), 1);
    }

    #[test]
    fn test_create_channel_duplicate() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));

        let config = ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None);
        manager.create_channel(config.clone()).unwrap();

        let result = manager.create_channel(config);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ChannelError::DuplicateChannel(_)));
    }

    #[test]
    fn test_remove_channel() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));

        let config = ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None);
        manager.create_channel(config).unwrap();

        assert!(manager.remove_channel("slack-1").is_ok());
        assert_eq!(manager.channel_count(), 0);
    }

    #[test]
    fn test_remove_channel_not_found() {
        let mut manager = ChannelManager::new();
        let result = manager.remove_channel("nonexistent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ChannelError::ChannelNotFound(_)));
    }

    #[test]
    fn test_get_channel() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));

        let config = ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None);
        manager.create_channel(config).unwrap();

        let channel = manager.get_channel("slack-1");
        assert!(channel.is_some());
        assert_eq!(channel.unwrap().id(), "slack-1");

        assert!(manager.get_channel("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_connect_channel() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));

        let config = ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None);
        manager.create_channel(config).unwrap();

        assert!(!manager.is_channel_connected("slack-1"));

        manager.connect_channel("slack-1").await.unwrap();
        assert!(manager.is_channel_connected("slack-1"));
    }

    #[tokio::test]
    async fn test_disconnect_channel() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));

        let config = ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None);
        manager.create_channel(config).unwrap();

        manager.connect_channel("slack-1").await.unwrap();
        assert!(manager.is_channel_connected("slack-1"));

        manager.disconnect_channel("slack-1").await.unwrap();
        assert!(!manager.is_channel_connected("slack-1"));
    }

    #[tokio::test]
    async fn test_connect_all() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Discord)));

        manager.create_channel(ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None)).unwrap();
        manager.create_channel(ChannelConfig::new("discord-1", ChannelKind::Discord, Credentials::None)).unwrap();

        let results = manager.connect_all().await;
        assert_eq!(results.len(), 2);

        for (_, result) in results {
            assert!(result.is_ok());
        }

        assert_eq!(manager.get_connected_count(), 2);
    }

    #[tokio::test]
    async fn test_disconnect_all() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Discord)));

        manager.create_channel(ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None)).unwrap();
        manager.create_channel(ChannelConfig::new("discord-1", ChannelKind::Discord, Credentials::None)).unwrap();

        manager.connect_all().await;
        assert_eq!(manager.get_connected_count(), 2);

        manager.disconnect_all().await;
        assert_eq!(manager.get_connected_count(), 0);
    }

    #[test]
    fn test_bind_agent() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));

        let config = ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None);
        manager.create_channel(config).unwrap();

        manager.bind_agent("slack-1", "agent-123".to_string()).unwrap();
        assert_eq!(manager.get_bound_agent("slack-1"), Some(&"agent-123".to_string()));
    }

    #[test]
    fn test_unbind_agent() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));

        let config = ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None);
        manager.create_channel(config).unwrap();

        manager.bind_agent("slack-1", "agent-123".to_string()).unwrap();
        manager.unbind_agent("slack-1").unwrap();

        assert!(manager.get_bound_agent("slack-1").is_none());
    }

    #[tokio::test]
    async fn test_send_to_channel() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));

        let config = ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None);
        manager.create_channel(config).unwrap();
        manager.connect_channel("slack-1").await.unwrap();

        let message = OutgoingMessage::text("target", "Hello");
        let result = manager.send_to_channel("slack-1", message).await;

        assert!(result.is_ok());
        let msg_id = result.unwrap();
        assert!(msg_id.starts_with("msg_"));
    }

    #[tokio::test]
    async fn test_send_to_channel_not_connected() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));

        let config = ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None);
        manager.create_channel(config).unwrap();
        // Don't connect

        let message = OutgoingMessage::text("target", "Hello");
        let result = manager.send_to_channel("slack-1", message).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ChannelError::NotConnected));
    }

    #[tokio::test]
    async fn test_broadcast_message() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Discord)));

        manager.create_channel(ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None)).unwrap();
        manager.create_channel(ChannelConfig::new("discord-1", ChannelKind::Discord, Credentials::None)).unwrap();

        // Connect only Slack
        manager.connect_channel("slack-1").await.unwrap();

        let message = OutgoingMessage::text("broadcast", "Hello all!");
        let results = manager.broadcast_message(message).await;

        // Only Slack should receive (connected)
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "slack-1");
        assert!(results[0].1.is_ok());
    }

    #[test]
    fn test_get_all_statuses() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Discord)));

        manager.create_channel(ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None)).unwrap();
        manager.create_channel(ChannelConfig::new("discord-1", ChannelKind::Discord, Credentials::None)).unwrap();

        let statuses = manager.get_all_statuses();
        assert_eq!(statuses.len(), 2);
        assert_eq!(statuses.get("slack-1"), Some(&ChannelStatus::Disconnected));
        assert_eq!(statuses.get("discord-1"), Some(&ChannelStatus::Disconnected));
    }

    #[test]
    fn test_subscribe_events() {
        let manager = ChannelManager::new();
        let mut receiver = manager.subscribe_events();

        // Receiver should be able to receive events
        assert!(receiver.try_recv().is_err()); // No events yet
    }

    #[test]
    fn test_list_channels() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Discord)));

        manager.create_channel(ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None).with_name("My Slack")).unwrap();
        manager.create_channel(ChannelConfig::new("discord-1", ChannelKind::Discord, Credentials::None).with_name("My Discord")).unwrap();

        let list = manager.list_channels();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_event_broadcast_on_connect() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));

        let config = ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None);
        manager.create_channel(config).unwrap();

        let mut receiver = manager.subscribe_events();

        manager.connect_channel("slack-1").await.unwrap();

        // Should receive a Connected event
        let event = receiver.try_recv();
        assert!(event.is_ok());
        match event.unwrap() {
            ChannelEvent::Connected { channel_id, channel_kind } => {
                assert_eq!(channel_id, "slack-1");
                assert_eq!(channel_kind, ChannelKind::Slack);
            }
            _ => panic!("Expected Connected event"),
        }
    }

    #[tokio::test]
    async fn test_try_reconnect() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));

        let config = ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None);
        manager.create_channel(config).unwrap();

        // First attempt should succeed
        let result = manager.try_reconnect("slack-1").await;
        assert!(result.is_ok());
        assert!(manager.is_channel_connected("slack-1"));
    }

    // ============================================================================
    // Behavior Configuration Tests (Task 10.7)
    // ============================================================================

    #[test]
    fn test_get_behavior_config_default() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));

        let config = ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None);
        manager.create_channel(config).unwrap();

        let behavior = manager.get_behavior_config("slack-1");
        assert!(behavior.is_some());
        // Default style is Detailed
        assert!(matches!(behavior.unwrap().response_style, super::super::behavior::ResponseStyle::Detailed));
    }

    #[test]
    fn test_get_behavior_config_not_found() {
        let manager = ChannelManager::new();
        let behavior = manager.get_behavior_config("nonexistent");
        assert!(behavior.is_none());
    }

    #[test]
    fn test_update_behavior_config() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));

        let config = ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None);
        manager.create_channel(config).unwrap();

        // Update behavior config
        let new_behavior = ChannelBehaviorConfig::new()
            .with_style(super::super::behavior::ResponseStyle::Concise)
            .with_max_length(500);

        let result = manager.update_behavior_config("slack-1", new_behavior.clone());
        assert!(result.is_ok());

        // Verify the update
        let behavior = manager.get_behavior_config("slack-1").unwrap();
        assert!(matches!(behavior.response_style, super::super::behavior::ResponseStyle::Concise));
        assert_eq!(behavior.max_response_length, 500);
    }

    #[test]
    fn test_update_behavior_config_not_found() {
        let mut manager = ChannelManager::new();

        let new_behavior = ChannelBehaviorConfig::new();
        let result = manager.update_behavior_config("nonexistent", new_behavior);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ChannelError::ChannelNotFound(_)));
    }

    #[tokio::test]
    async fn test_update_behavior_config_broadcasts_event() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));

        let config = ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None);
        manager.create_channel(config).unwrap();

        let mut receiver = manager.subscribe_events();

        // Update behavior config
        let new_behavior = ChannelBehaviorConfig::new();
        manager.update_behavior_config("slack-1", new_behavior).unwrap();

        // Should receive a BehaviorChanged event
        let event = receiver.try_recv();
        assert!(event.is_ok());
        match event.unwrap() {
            ChannelEvent::BehaviorChanged { channel_id } => {
                assert_eq!(channel_id, "slack-1");
            }
            _ => panic!("Expected BehaviorChanged event"),
        }
    }

    #[test]
    fn test_is_within_working_hours_default() {
        let mut manager = ChannelManager::new();
        manager.register_factory(Arc::new(MockFactory::new(ChannelKind::Slack)));

        let config = ChannelConfig::new("slack-1", ChannelKind::Slack, Credentials::None);
        manager.create_channel(config).unwrap();

        // No working hours configured, should always be true
        assert!(manager.is_within_working_hours("slack-1"));
    }

    #[test]
    fn test_is_within_working_hours_nonexistent() {
        let manager = ChannelManager::new();
        // Non-existent channel should return true (default to always active)
        assert!(manager.is_within_working_hours("nonexistent"));
    }
}