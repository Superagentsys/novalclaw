//! Channel Error Types
//!
//! Error types for channel operations including connection failures,
//! authentication errors, and message handling issues.

/// Channel operation errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum ChannelError {
    /// Failed to connect to the channel service
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Authentication or authorization failed
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded{}", .retry_after.map(|s| format!(", retry after {}s", s)).unwrap_or_default())]
    RateLimitExceeded {
        /// Seconds until rate limit resets
        retry_after: Option<u64>,
    },

    /// Failed to send message
    #[error("Failed to send message: {0}")]
    MessageSendFailed(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    /// Channel not found in registry
    #[error("Channel not found: {0}")]
    ChannelNotFound(String),

    /// Channel operation timeout
    #[error("Operation timeout: {0}")]
    Timeout(String),

    /// Invalid message format
    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    /// Channel is not connected
    #[error("Channel is not connected")]
    NotConnected,

    /// Channel is already connected
    #[error("Channel is already connected")]
    AlreadyConnected,

    /// Unsupported operation for this channel type
    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Channel with this ID already exists
    #[error("Channel already exists: {0}")]
    DuplicateChannel(String),

    /// No factory registered for channel kind
    #[error("No factory registered for channel kind: {0}")]
    NoFactory(String),

    /// Reconnection attempts exhausted
    #[error("Reconnection attempts exhausted for channel: {0}")]
    ReconnectExhausted(String),

    /// Database error
    #[error("Database error: {0}")]
    DatabaseError(String),
}

impl ChannelError {
    /// Create a connection failed error
    pub fn connection_failed(msg: impl Into<String>) -> Self {
        Self::ConnectionFailed(msg.into())
    }

    /// Create an authentication failed error
    pub fn auth_failed(msg: impl Into<String>) -> Self {
        Self::AuthenticationFailed(msg.into())
    }

    /// Create a rate limit error with optional retry time
    pub fn rate_limit(retry_after: Option<u64>) -> Self {
        Self::RateLimitExceeded { retry_after }
    }

    /// Create a message send failed error
    pub fn send_failed(msg: impl Into<String>) -> Self {
        Self::MessageSendFailed(msg.into())
    }

    /// Create a configuration error
    pub fn config(msg: impl Into<String>) -> Self {
        Self::ConfigurationError(msg.into())
    }

    /// Create a channel not found error
    pub fn not_found(id: impl Into<String>) -> Self {
        Self::ChannelNotFound(id.into())
    }

    /// Check if this error is recoverable (retry might succeed)
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::ConnectionFailed(_)
                | Self::RateLimitExceeded { .. }
                | Self::Timeout(_)
                | Self::NotConnected
        )
    }

    /// Check if this error requires reconnection
    pub fn requires_reconnect(&self) -> bool {
        matches!(
            self,
            Self::ConnectionFailed(_)
                | Self::AuthenticationFailed(_)
                | Self::NotConnected
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_failed() {
        let err = ChannelError::connection_failed("timeout");
        assert_eq!(err.to_string(), "Connection failed: timeout");
        assert!(err.is_recoverable());
        assert!(err.requires_reconnect());
    }

    #[test]
    fn test_auth_failed() {
        let err = ChannelError::auth_failed("invalid token");
        assert_eq!(err.to_string(), "Authentication failed: invalid token");
        assert!(!err.is_recoverable());
        assert!(err.requires_reconnect());
    }

    #[test]
    fn test_rate_limit_no_retry() {
        let err = ChannelError::rate_limit(None);
        assert_eq!(err.to_string(), "Rate limit exceeded");
        assert!(err.is_recoverable());
        assert!(!err.requires_reconnect());
    }

    #[test]
    fn test_rate_limit_with_retry() {
        let err = ChannelError::rate_limit(Some(30));
        assert_eq!(err.to_string(), "Rate limit exceeded, retry after 30s");
        assert!(err.is_recoverable());
        assert!(!err.requires_reconnect());
    }

    #[test]
    fn test_send_failed() {
        let err = ChannelError::send_failed("network error");
        assert_eq!(err.to_string(), "Failed to send message: network error");
        assert!(!err.is_recoverable());
        assert!(!err.requires_reconnect());
    }

    #[test]
    fn test_config_error() {
        let err = ChannelError::config("missing token");
        assert_eq!(err.to_string(), "Configuration error: missing token");
        assert!(!err.is_recoverable());
        assert!(!err.requires_reconnect());
    }

    #[test]
    fn test_channel_not_found() {
        let err = ChannelError::not_found("slack-123");
        assert_eq!(err.to_string(), "Channel not found: slack-123");
        assert!(!err.is_recoverable());
        assert!(!err.requires_reconnect());
    }

    #[test]
    fn test_timeout() {
        let err = ChannelError::Timeout("30s".into());
        assert_eq!(err.to_string(), "Operation timeout: 30s");
        assert!(err.is_recoverable());
        assert!(!err.requires_reconnect());
    }

    #[test]
    fn test_not_connected() {
        let err = ChannelError::NotConnected;
        assert_eq!(err.to_string(), "Channel is not connected");
        assert!(err.is_recoverable());
        assert!(err.requires_reconnect());
    }

    #[test]
    fn test_already_connected() {
        let err = ChannelError::AlreadyConnected;
        assert_eq!(err.to_string(), "Channel is already connected");
        assert!(!err.is_recoverable());
        assert!(!err.requires_reconnect());
    }

    #[test]
    fn test_unsupported_operation() {
        let err = ChannelError::UnsupportedOperation("threads".into());
        assert_eq!(err.to_string(), "Unsupported operation: threads");
        assert!(!err.is_recoverable());
        assert!(!err.requires_reconnect());
    }

    #[test]
    fn test_duplicate_channel() {
        let err = ChannelError::DuplicateChannel("slack-123".to_string());
        assert_eq!(err.to_string(), "Channel already exists: slack-123");
        assert!(!err.is_recoverable());
        assert!(!err.requires_reconnect());
    }

    #[test]
    fn test_no_factory() {
        let err = ChannelError::NoFactory("Slack".to_string());
        assert!(err.to_string().contains("No factory registered"));
        assert!(!err.is_recoverable());
        assert!(!err.requires_reconnect());
    }

    #[test]
    fn test_reconnect_exhausted() {
        let err = ChannelError::ReconnectExhausted("slack-123".to_string());
        assert_eq!(err.to_string(), "Reconnection attempts exhausted for channel: slack-123");
        assert!(!err.is_recoverable());
        assert!(!err.requires_reconnect());
    }
}