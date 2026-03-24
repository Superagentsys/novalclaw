//! Channel Event Types
//!
//! Event types for channel lifecycle notifications and state changes.

use serde::{Deserialize, Serialize};

use super::ChannelKind;
use super::traits::ChannelId;

/// Agent identifier type alias.
pub type AgentId = String;

/// Policy for automatic reconnection attempts.
///
/// Controls how the channel manager handles connection failures
/// and automatic reconnection with exponential backoff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectPolicy {
    /// Maximum number of reconnection attempts.
    ///
    /// Set to 0 for unlimited retries.
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,

    /// Initial delay before first retry in milliseconds.
    #[serde(default = "default_initial_delay")]
    pub initial_delay_ms: u64,

    /// Maximum delay between retries in milliseconds.
    #[serde(default = "default_max_delay")]
    pub max_delay_ms: u64,

    /// Multiplier for exponential backoff.
    ///
    /// Each retry delay is multiplied by this factor.
    #[serde(default = "default_multiplier")]
    pub multiplier: f64,
}

fn default_max_attempts() -> u32 { 5 }
fn default_initial_delay() -> u64 { 1000 }
fn default_max_delay() -> u64 { 60000 }
fn default_multiplier() -> f64 { 2.0 }

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            max_attempts: default_max_attempts(),
            initial_delay_ms: default_initial_delay(),
            max_delay_ms: default_max_delay(),
            multiplier: default_multiplier(),
        }
    }
}

impl ReconnectPolicy {
    /// Create a new reconnect policy with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum number of retry attempts.
    pub fn with_max_attempts(mut self, attempts: u32) -> Self {
        self.max_attempts = attempts;
        self
    }

    /// Set the initial delay in milliseconds.
    pub fn with_initial_delay(mut self, delay_ms: u64) -> Self {
        self.initial_delay_ms = delay_ms;
        self
    }

    /// Set the maximum delay in milliseconds.
    pub fn with_max_delay(mut self, delay_ms: u64) -> Self {
        self.max_delay_ms = delay_ms;
        self
    }

    /// Set the backoff multiplier.
    pub fn with_multiplier(mut self, multiplier: f64) -> Self {
        self.multiplier = multiplier;
        self
    }

    /// Calculate the delay for a given attempt number.
    ///
    /// Returns `None` if the attempt exceeds `max_attempts` (when max_attempts > 0).
    pub fn delay_for_attempt(&self, attempt: u32) -> Option<u64> {
        // Check if we've exceeded max attempts (0 = unlimited)
        if self.max_attempts > 0 && attempt >= self.max_attempts {
            return None;
        }

        // Calculate exponential backoff
        // attempt 0: initial_delay
        // attempt 1: initial_delay * multiplier
        // attempt 2: initial_delay * multiplier^2
        // etc.
        let delay = if attempt == 0 {
            self.initial_delay_ms
        } else {
            let exponential = self.initial_delay_ms as f64
                * self.multiplier.powi(attempt as i32);
            exponential.min(self.max_delay_ms as f64) as u64
        };

        // Also cap at max_delay for attempt 0 case
        Some(delay.min(self.max_delay_ms))
    }

    /// Check if retries should continue for the given attempt.
    pub fn should_retry(&self, attempt: u32) -> bool {
        self.max_attempts == 0 || attempt < self.max_attempts
    }
}

/// Channel lifecycle events.
///
/// These events are broadcast by the channel manager to notify
/// subscribers about channel state changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChannelEvent {
    /// Channel successfully connected.
    Connected {
        /// Channel instance ID.
        channel_id: ChannelId,
        /// Channel kind (Slack, Discord, etc.).
        channel_kind: ChannelKind,
    },

    /// Channel disconnected.
    Disconnected {
        /// Channel instance ID.
        channel_id: ChannelId,
        /// Reason for disconnection (if known).
        reason: Option<String>,
    },

    /// Channel encountered an error.
    Error {
        /// Channel instance ID.
        channel_id: ChannelId,
        /// Error message.
        error: String,
    },

    /// Channel is attempting to reconnect.
    Reconnecting {
        /// Channel instance ID.
        channel_id: ChannelId,
        /// Current attempt number (0-indexed).
        attempt: u32,
    },

    /// Message received from channel.
    MessageReceived {
        /// Channel instance ID.
        channel_id: ChannelId,
        /// Message ID.
        message_id: String,
    },

    /// Message sent to channel.
    MessageSent {
        /// Channel instance ID.
        channel_id: ChannelId,
        /// Message ID.
        message_id: String,
    },

    /// Channel created.
    Created {
        /// Channel instance ID.
        channel_id: ChannelId,
        /// Channel kind.
        channel_kind: ChannelKind,
    },

    /// Channel removed.
    Removed {
        /// Channel instance ID.
        channel_id: ChannelId,
    },

    /// Agent bound to channel.
    AgentBound {
        /// Channel instance ID.
        channel_id: ChannelId,
        /// Agent ID.
        agent_id: AgentId,
    },

    /// Agent unbound from channel.
    AgentUnbound {
        /// Channel instance ID.
        channel_id: ChannelId,
    },

    /// Behavior configuration changed for a channel.
    BehaviorChanged {
        /// Channel instance ID.
        channel_id: ChannelId,
    },
}

impl ChannelEvent {
    /// Get the channel ID for this event.
    pub fn channel_id(&self) -> &str {
        match self {
            Self::Connected { channel_id, .. } => channel_id,
            Self::Disconnected { channel_id, .. } => channel_id,
            Self::Error { channel_id, .. } => channel_id,
            Self::Reconnecting { channel_id, .. } => channel_id,
            Self::MessageReceived { channel_id, .. } => channel_id,
            Self::MessageSent { channel_id, .. } => channel_id,
            Self::Created { channel_id, .. } => channel_id,
            Self::Removed { channel_id } => channel_id,
            Self::AgentBound { channel_id, .. } => channel_id,
            Self::AgentUnbound { channel_id } => channel_id,
            Self::BehaviorChanged { channel_id } => channel_id,
        }
    }

    /// Check if this is a connection-related event.
    pub fn is_connection_event(&self) -> bool {
        matches!(
            self,
            Self::Connected { .. }
                | Self::Disconnected { .. }
                | Self::Error { .. }
                | Self::Reconnecting { .. }
        )
    }

    /// Check if this is a message-related event.
    pub fn is_message_event(&self) -> bool {
        matches!(
            self,
            Self::MessageReceived { .. } | Self::MessageSent { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // ReconnectPolicy Tests
    // ============================================================================

    #[test]
    fn test_reconnect_policy_default() {
        let policy = ReconnectPolicy::default();
        assert_eq!(policy.max_attempts, 5);
        assert_eq!(policy.initial_delay_ms, 1000);
        assert_eq!(policy.max_delay_ms, 60000);
        assert_eq!(policy.multiplier, 2.0);
    }

    #[test]
    fn test_reconnect_policy_builder() {
        let policy = ReconnectPolicy::new()
            .with_max_attempts(10)
            .with_initial_delay(500)
            .with_max_delay(30000)
            .with_multiplier(1.5);

        assert_eq!(policy.max_attempts, 10);
        assert_eq!(policy.initial_delay_ms, 500);
        assert_eq!(policy.max_delay_ms, 30000);
        assert_eq!(policy.multiplier, 1.5);
    }

    #[test]
    fn test_reconnect_policy_delay_calculation() {
        let policy = ReconnectPolicy::default();

        // First attempt: 1000ms
        assert_eq!(policy.delay_for_attempt(0), Some(1000));

        // Second attempt: 2000ms
        assert_eq!(policy.delay_for_attempt(1), Some(2000));

        // Third attempt: 4000ms
        assert_eq!(policy.delay_for_attempt(2), Some(4000));

        // Fourth attempt: 8000ms
        assert_eq!(policy.delay_for_attempt(3), Some(8000));
    }

    #[test]
    fn test_reconnect_policy_delay_max_cap() {
        let policy = ReconnectPolicy::new()
            .with_initial_delay(1000)
            .with_max_delay(5000)
            .with_multiplier(2.0);

        // Delays should be capped at max_delay
        assert_eq!(policy.delay_for_attempt(0), Some(1000));
        assert_eq!(policy.delay_for_attempt(1), Some(2000));
        assert_eq!(policy.delay_for_attempt(2), Some(4000));
        assert_eq!(policy.delay_for_attempt(3), Some(5000)); // Capped
        // Attempt 10 exceeds max_attempts (default 5), so returns None
        assert_eq!(policy.delay_for_attempt(10), None);
    }

    #[test]
    fn test_reconnect_policy_delay_max_cap_unlimited() {
        let policy = ReconnectPolicy::new()
            .with_max_attempts(0) // Unlimited
            .with_initial_delay(1000)
            .with_max_delay(5000)
            .with_multiplier(2.0);

        // With unlimited attempts, even high attempt numbers return capped delay
        assert_eq!(policy.delay_for_attempt(10), Some(5000)); // Capped
    }

    #[test]
    fn test_reconnect_policy_unlimited_retries() {
        let policy = ReconnectPolicy::new()
            .with_max_attempts(0); // Unlimited

        // Should always return a delay
        assert!(policy.delay_for_attempt(0).is_some());
        assert!(policy.delay_for_attempt(100).is_some());
        assert!(policy.should_retry(1000));
    }

    #[test]
    fn test_reconnect_policy_max_attempts_exceeded() {
        let policy = ReconnectPolicy::new()
            .with_max_attempts(3);

        assert!(policy.should_retry(0));
        assert!(policy.should_retry(1));
        assert!(policy.should_retry(2));
        assert!(!policy.should_retry(3)); // Exceeded

        assert_eq!(policy.delay_for_attempt(0), Some(1000));
        assert_eq!(policy.delay_for_attempt(1), Some(2000));
        assert_eq!(policy.delay_for_attempt(2), Some(4000));
        assert_eq!(policy.delay_for_attempt(3), None); // Exceeded
    }

    // ============================================================================
    // ChannelEvent Tests
    // ============================================================================

    #[test]
    fn test_channel_event_connected() {
        let event = ChannelEvent::Connected {
            channel_id: "ch-1".to_string(),
            channel_kind: ChannelKind::Slack,
        };

        assert_eq!(event.channel_id(), "ch-1");
        assert!(event.is_connection_event());
        assert!(!event.is_message_event());
    }

    #[test]
    fn test_channel_event_disconnected() {
        let event = ChannelEvent::Disconnected {
            channel_id: "ch-1".to_string(),
            reason: Some("Network error".to_string()),
        };

        assert_eq!(event.channel_id(), "ch-1");
        assert!(event.is_connection_event());
    }

    #[test]
    fn test_channel_event_error() {
        let event = ChannelEvent::Error {
            channel_id: "ch-1".to_string(),
            error: "Connection refused".to_string(),
        };

        assert_eq!(event.channel_id(), "ch-1");
        assert!(event.is_connection_event());
    }

    #[test]
    fn test_channel_event_reconnecting() {
        let event = ChannelEvent::Reconnecting {
            channel_id: "ch-1".to_string(),
            attempt: 2,
        };

        assert_eq!(event.channel_id(), "ch-1");
        assert!(event.is_connection_event());
    }

    #[test]
    fn test_channel_event_message_received() {
        let event = ChannelEvent::MessageReceived {
            channel_id: "ch-1".to_string(),
            message_id: "msg-123".to_string(),
        };

        assert_eq!(event.channel_id(), "ch-1");
        assert!(!event.is_connection_event());
        assert!(event.is_message_event());
    }

    #[test]
    fn test_channel_event_message_sent() {
        let event = ChannelEvent::MessageSent {
            channel_id: "ch-1".to_string(),
            message_id: "msg-456".to_string(),
        };

        assert_eq!(event.channel_id(), "ch-1");
        assert!(event.is_message_event());
    }

    #[test]
    fn test_channel_event_created() {
        let event = ChannelEvent::Created {
            channel_id: "ch-1".to_string(),
            channel_kind: ChannelKind::Discord,
        };

        assert_eq!(event.channel_id(), "ch-1");
        assert!(!event.is_connection_event());
        assert!(!event.is_message_event());
    }

    #[test]
    fn test_channel_event_removed() {
        let event = ChannelEvent::Removed {
            channel_id: "ch-1".to_string(),
        };

        assert_eq!(event.channel_id(), "ch-1");
    }

    #[test]
    fn test_channel_event_agent_bound() {
        let event = ChannelEvent::AgentBound {
            channel_id: "ch-1".to_string(),
            agent_id: "agent-123".to_string(),
        };

        assert_eq!(event.channel_id(), "ch-1");
    }

    #[test]
    fn test_channel_event_serialize() {
        let event = ChannelEvent::Connected {
            channel_id: "ch-1".to_string(),
            channel_kind: ChannelKind::Slack,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("connected"));
        assert!(json.contains("ch-1"));
        assert!(json.contains("slack"));
    }

    #[test]
    fn test_channel_event_deserialize() {
        let json = r#"{"type":"disconnected","channel_id":"ch-1","reason":"timeout"}"#;
        let event: ChannelEvent = serde_json::from_str(json).unwrap();

        match event {
            ChannelEvent::Disconnected { channel_id, reason } => {
                assert_eq!(channel_id, "ch-1");
                assert_eq!(reason, Some("timeout".to_string()));
            }
            _ => panic!("Expected Disconnected event"),
        }
    }
}