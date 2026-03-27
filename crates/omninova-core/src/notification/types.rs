//! Notification types and data structures.
//!
//! Provides types for desktop notification management including:
//! - Notification types (agent response, error, system update, etc.)
//! - Notification priorities
//! - Notification records
//! - Configuration

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Notification type enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Copy)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    /// Agent response completed
    AgentResponse,
    /// Error notification
    Error,
    /// System update
    SystemUpdate,
    /// Channel message
    ChannelMessage,
    /// Performance warning
    PerformanceWarning,
    /// Custom notification
    Custom,
}

impl Default for NotificationType {
    fn default() -> Self {
        Self::Custom
    }
}

/// Notification priority levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Copy)]
#[serde(rename_all = "lowercase")]
pub enum NotificationPriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl Default for NotificationPriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Notification record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// Unique identifier
    pub id: String,
    /// Notification type
    #[serde(rename = "notificationType")]
    pub notification_type: NotificationType,
    /// Notification title
    pub title: String,
    /// Notification body content
    pub body: String,
    /// Priority level
    pub priority: NotificationPriority,
    /// Creation timestamp (Unix seconds)
    pub created_at: i64,
    /// Whether notification has been read
    pub read: bool,
    /// Associated metadata (e.g., agent_id, session_id)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

impl Notification {
    /// Create a new notification with auto-generated ID
    pub fn new(
        notification_type: NotificationType,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            notification_type,
            title: title.into(),
            body: body.into(),
            priority: NotificationPriority::Normal,
            created_at: chrono::Utc::now().timestamp(),
            read: false,
            metadata: None,
        }
    }

    /// Set notification priority
    pub fn with_priority(mut self, priority: NotificationPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set notification metadata
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Mark notification as read
    pub fn mark_read(&mut self) {
        self.read = true;
    }
}

/// Notification configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    /// Whether desktop notifications are enabled
    pub enabled: bool,
    /// Enabled notification types
    pub enabled_types: Vec<NotificationType>,
    /// Whether notification sound is enabled
    pub sound_enabled: bool,
    /// Quiet hours start time (hour 0-23)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quiet_hours_start: Option<u8>,
    /// Quiet hours end time (hour 0-23)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quiet_hours_end: Option<u8>,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            enabled_types: vec![
                NotificationType::Error,
                NotificationType::SystemUpdate,
                NotificationType::PerformanceWarning,
            ],
            sound_enabled: true,
            quiet_hours_start: Some(22),
            quiet_hours_end: Some(8),
        }
    }
}

/// Notification error type
#[derive(Debug, Clone, thiserror::Error)]
pub enum NotificationError {
    #[error("Notification disabled")]
    Disabled,
    #[error("In quiet hours")]
    QuietHours,
    #[error("Notification type not enabled: {0:?}")]
    TypeNotEnabled(NotificationType),
    #[error("Desktop notification failed: {0}")]
    DesktopError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_creation() {
        let notification = Notification::new(
            NotificationType::Error,
            "Test Title",
            "Test Body",
        );

        assert!(!notification.id.is_empty());
        assert_eq!(notification.notification_type, NotificationType::Error);
        assert_eq!(notification.title, "Test Title");
        assert_eq!(notification.body, "Test Body");
        assert!(!notification.read);
    }

    #[test]
    fn test_notification_priority() {
        let notification = Notification::new(
            NotificationType::Error,
            "Test",
            "Body",
        ).with_priority(NotificationPriority::Urgent);

        assert_eq!(notification.priority, NotificationPriority::Urgent);
    }

    #[test]
    fn test_notification_mark_read() {
        let mut notification = Notification::new(
            NotificationType::Error,
            "Test",
            "Body",
        );

        assert!(!notification.read);
        notification.mark_read();
        assert!(notification.read);
    }

    #[test]
    fn test_notification_config_default() {
        let config = NotificationConfig::default();

        assert!(config.enabled);
        assert!(config.sound_enabled);
        assert_eq!(config.quiet_hours_start, Some(22));
        assert_eq!(config.quiet_hours_end, Some(8));
        assert_eq!(config.enabled_types.len(), 3);
    }

    #[test]
    fn test_notification_type_serde() {
        let nt = NotificationType::AgentResponse;
        let json = serde_json::to_string(&nt).unwrap();
        assert_eq!(json, "\"agent_response\"");

        let deserialized: NotificationType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, nt);
    }
}