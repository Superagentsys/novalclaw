//! Notification management module.
//!
//! Provides desktop notification functionality including:
//! - Notification types and priorities
//! - Notification service with history tracking
//! - Quiet hours configuration
//! - Configuration persistence

pub mod service;
pub mod types;

pub use service::{
    get_notification_history, is_quiet_hours, notification_service, send_notification,
    NotificationService,
};
pub use types::{
    Notification, NotificationConfig, NotificationError, NotificationPriority, NotificationType,
};