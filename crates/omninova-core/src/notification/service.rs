//! Notification service implementation.
//!
//! Provides centralized notification management including:
//! - Desktop notification dispatch
//! - Notification history tracking
//! - Quiet hours enforcement
//! - Configuration management

use crate::notification::{
    Notification, NotificationConfig, NotificationError, NotificationType,
};
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use std::sync::OnceLock;

/// Default maximum history size
const DEFAULT_MAX_HISTORY: usize = 100;

/// Global notification service instance
static NOTIFICATION_SERVICE: OnceLock<NotificationService> = OnceLock::new();

/// Notification service for managing desktop notifications
pub struct NotificationService {
    /// Notification configuration
    config: RwLock<NotificationConfig>,
    /// Notification history
    history: RwLock<Vec<Notification>>,
    /// Maximum history size
    max_history: usize,
}

impl Default for NotificationService {
    fn default() -> Self {
        Self::new(NotificationConfig::default())
    }
}

impl NotificationService {
    /// Create a new notification service
    pub fn new(config: NotificationConfig) -> Self {
        Self {
            config: RwLock::new(config),
            history: RwLock::new(Vec::new()),
            max_history: DEFAULT_MAX_HISTORY,
        }
    }

    /// Get the global notification service instance
    pub fn global() -> &'static NotificationService {
        NOTIFICATION_SERVICE.get_or_init(NotificationService::default)
    }

    /// Send a notification
    ///
    /// Returns Err if notification was skipped due to settings or if there was an error.
    /// Check the error type to determine why the notification was not sent.
    pub fn send(&self, notification: Notification) -> Result<(), NotificationError> {
        // Check if notifications are enabled
        {
            let config = self.config.read();
            if !config.enabled {
                return Err(NotificationError::Disabled);
            }
        }

        // Check quiet hours
        if self.is_quiet_hours() {
            return Err(NotificationError::QuietHours);
        }

        // Check if notification type is enabled
        {
            let config = self.config.read();
            if !config.enabled_types.contains(&notification.notification_type) {
                return Err(NotificationError::TypeNotEnabled(notification.notification_type));
            }
        }

        // Add to history
        self.add_to_history(notification.clone());

        Ok(())
    }

    /// Check if current time is within quiet hours
    pub fn is_quiet_hours(&self) -> bool {
        self.is_quiet_hours_at(Utc::now().hour() as u8)
    }

    /// Check if a specific hour is within quiet hours
    pub fn is_quiet_hours_at(&self, hour: u8) -> bool {
        let config = self.config.read();
        if let (Some(start), Some(end)) = (config.quiet_hours_start, config.quiet_hours_end) {
            if start < end {
                // Quiet hours within same day (e.g., 12:00 - 14:00)
                hour >= start && hour < end
            } else {
                // Quiet hours across midnight (e.g., 22:00 - 08:00)
                hour >= start || hour < end
            }
        } else {
            false
        }
    }

    /// Get notification history
    pub fn get_history(&self, limit: Option<usize>) -> Vec<Notification> {
        let history = self.history.read();
        let limit = limit.unwrap_or(self.max_history).min(history.len());
        history.iter().rev().take(limit).cloned().collect()
    }

    /// Get all unread notifications
    pub fn get_unread(&self) -> Vec<Notification> {
        let history = self.history.read();
        history.iter().filter(|n| !n.read).cloned().collect()
    }

    /// Get unread count
    pub fn unread_count(&self) -> usize {
        let history = self.history.read();
        history.iter().filter(|n| !n.read).count()
    }

    /// Mark notification as read
    pub fn mark_as_read(&self, id: &str) -> bool {
        let mut history = self.history.write();
        if let Some(notification) = history.iter_mut().find(|n| n.id == id) {
            notification.read = true;
            true
        } else {
            false
        }
    }

    /// Mark all notifications as read
    pub fn mark_all_read(&self) {
        let mut history = self.history.write();
        for notification in history.iter_mut() {
            notification.read = true;
        }
    }

    /// Clear notification history
    pub fn clear_history(&self) {
        self.history.write().clear();
    }

    /// Update configuration
    pub fn update_config(&self, config: NotificationConfig) {
        *self.config.write() = config;
    }

    /// Get current configuration
    pub fn get_config(&self) -> NotificationConfig {
        self.config.read().clone()
    }

    /// Check if notifications are enabled
    pub fn is_enabled(&self) -> bool {
        self.config.read().enabled
    }

    /// Check if sound is enabled
    pub fn is_sound_enabled(&self) -> bool {
        self.config.read().sound_enabled
    }

    /// Check if a notification type is enabled
    pub fn is_type_enabled(&self, notification_type: NotificationType) -> bool {
        self.config.read().enabled_types.contains(&notification_type)
    }

    /// Add notification to history
    fn add_to_history(&self, notification: Notification) {
        let mut history = self.history.write();
        history.push(notification);

        // Prune old entries if exceeding max size
        if history.len() > self.max_history {
            let drain_count = history.len() - self.max_history;
            history.drain(0..drain_count);
        }
    }

    /// Get history count
    pub fn history_count(&self) -> usize {
        self.history.read().len()
    }
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Get the global notification service
pub fn notification_service() -> &'static NotificationService {
    NotificationService::global()
}

/// Send a notification using the global service
pub fn send_notification(notification: Notification) -> Result<(), NotificationError> {
    notification_service().send(notification)
}

/// Check if currently in quiet hours
pub fn is_quiet_hours() -> bool {
    notification_service().is_quiet_hours()
}

/// Get notification history
pub fn get_notification_history(limit: Option<usize>) -> Vec<Notification> {
    notification_service().get_history(limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_notification(notification_type: NotificationType) -> Notification {
        Notification::new(notification_type, "Test", "Body")
    }

    #[test]
    fn test_quiet_hours_same_day() {
        let config = NotificationConfig {
            quiet_hours_start: Some(12),
            quiet_hours_end: Some(14),
            ..Default::default()
        };
        let service = NotificationService::new(config);

        // Inside quiet hours
        assert!(service.is_quiet_hours_at(12));
        assert!(service.is_quiet_hours_at(13));

        // Outside quiet hours
        assert!(!service.is_quiet_hours_at(11));
        assert!(!service.is_quiet_hours_at(14));
        assert!(!service.is_quiet_hours_at(15));
    }

    #[test]
    fn test_quiet_hours_across_midnight() {
        let config = NotificationConfig {
            quiet_hours_start: Some(22),
            quiet_hours_end: Some(8),
            ..Default::default()
        };
        let service = NotificationService::new(config);

        // Inside quiet hours
        assert!(service.is_quiet_hours_at(22));
        assert!(service.is_quiet_hours_at(23));
        assert!(service.is_quiet_hours_at(0));
        assert!(service.is_quiet_hours_at(7));

        // Outside quiet hours
        assert!(!service.is_quiet_hours_at(8));
        assert!(!service.is_quiet_hours_at(12));
        assert!(!service.is_quiet_hours_at(21));
    }

    #[test]
    fn test_no_quiet_hours() {
        let config = NotificationConfig {
            quiet_hours_start: None,
            quiet_hours_end: None,
            ..Default::default()
        };
        let service = NotificationService::new(config);

        // No quiet hours
        for hour in 0..24 {
            assert!(!service.is_quiet_hours_at(hour));
        }
    }

    #[test]
    fn test_send_notification_disabled() {
        let config = NotificationConfig {
            enabled: false,
            ..Default::default()
        };
        let service = NotificationService::new(config);

        let notification = create_test_notification(NotificationType::Error);
        let result = service.send(notification);

        assert!(matches!(result, Err(NotificationError::Disabled)));
    }

    #[test]
    fn test_send_notification_type_not_enabled() {
        let config = NotificationConfig {
            enabled: true,
            enabled_types: vec![NotificationType::Error],
            ..Default::default()
        };
        let service = NotificationService::new(config);

        let notification = create_test_notification(NotificationType::AgentResponse);
        let result = service.send(notification);

        assert!(matches!(
            result,
            Err(NotificationError::TypeNotEnabled(NotificationType::AgentResponse))
        ));
    }

    #[test]
    fn test_send_notification_success() {
        let config = NotificationConfig {
            enabled: true,
            enabled_types: vec![NotificationType::Error],
            quiet_hours_start: None,
            quiet_hours_end: None,
            ..Default::default()
        };
        let service = NotificationService::new(config);

        let notification = create_test_notification(NotificationType::Error);
        let result = service.send(notification);

        assert!(result.is_ok());
        assert_eq!(service.history_count(), 1);
    }

    #[test]
    fn test_history_limit() {
        let config = NotificationConfig {
            enabled: true,
            enabled_types: vec![NotificationType::Error],
            quiet_hours_start: None,
            quiet_hours_end: None,
            ..Default::default()
        };
        let service = NotificationService::new(config);

        // Send more than max history
        for _ in 0..150 {
            let notification = create_test_notification(NotificationType::Error);
            service.send(notification).unwrap();
        }

        assert_eq!(service.history_count(), 100);
    }

    #[test]
    fn test_mark_as_read() {
        let config = NotificationConfig {
            enabled: true,
            enabled_types: vec![NotificationType::Error],
            quiet_hours_start: None,
            quiet_hours_end: None,
            ..Default::default()
        };
        let service = NotificationService::new(config);

        let notification = create_test_notification(NotificationType::Error);
        let id = notification.id.clone();
        service.send(notification).unwrap();

        assert_eq!(service.unread_count(), 1);
        assert!(service.mark_as_read(&id));
        assert_eq!(service.unread_count(), 0);
    }

    #[test]
    fn test_get_history_order() {
        let config = NotificationConfig {
            enabled: true,
            enabled_types: vec![NotificationType::Error],
            quiet_hours_start: None,
            quiet_hours_end: None,
            ..Default::default()
        };
        let service = NotificationService::new(config);

        for i in 0..5 {
            let notification = Notification::new(
                NotificationType::Error,
                format!("Test {}", i),
                "Body",
            );
            service.send(notification).unwrap();
        }

        let history = service.get_history(None);

        // Most recent first
        assert_eq!(history[0].title, "Test 4");
        assert_eq!(history[4].title, "Test 0");
    }

    #[test]
    fn test_clear_history() {
        let config = NotificationConfig {
            enabled: true,
            enabled_types: vec![NotificationType::Error],
            quiet_hours_start: None,
            quiet_hours_end: None,
            ..Default::default()
        };
        let service = NotificationService::new(config);

        for _ in 0..10 {
            let notification = create_test_notification(NotificationType::Error);
            service.send(notification).unwrap();
        }

        assert_eq!(service.history_count(), 10);
        service.clear_history();
        assert_eq!(service.history_count(), 0);
    }

    #[test]
    fn test_global_service() {
        let service = notification_service();
        assert!(service.is_enabled());
    }
}