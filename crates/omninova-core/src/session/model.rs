//! Session and message data models for persistent storage
//!
//! This module defines the data structures for storing and managing
//! conversation sessions and messages in the database.

use serde::{Deserialize, Serialize};

/// Message role enum representing who sent the message
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// Message from the user
    User,
    /// Message from the AI assistant
    Assistant,
    /// System message (e.g., instructions, context)
    System,
}

impl Default for MessageRole {
    fn default() -> Self {
        Self::User
    }
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageRole::User => write!(f, "user"),
            MessageRole::Assistant => write!(f, "assistant"),
            MessageRole::System => write!(f, "system"),
        }
    }
}

impl std::str::FromStr for MessageRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "user" => Ok(MessageRole::User),
            "assistant" => Ok(MessageRole::Assistant),
            "system" => Ok(MessageRole::System),
            _ => Err(format!("Invalid message role: {}", s)),
        }
    }
}

impl MessageRole {
    /// Parse from database column, returning a rusqlite-compatible error on failure.
    /// This helper reduces boilerplate in FromRow implementations.
    pub fn from_db_string(s: &str, column_idx: usize) -> Result<Self, rusqlite::Error> {
        s.parse::<Self>().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                column_idx,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })
    }
}

/// Persistent session model stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Database auto-increment ID
    pub id: i64,
    /// ID of the agent this session belongs to
    pub agent_id: i64,
    /// Optional title for the session
    pub title: Option<String>,
    /// Unix timestamp of creation
    pub created_at: i64,
    /// Unix timestamp of last update
    pub updated_at: i64,
}

/// Data required to create a new session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSession {
    /// ID of the agent this session belongs to (required)
    pub agent_id: i64,
    /// Optional title for the session
    pub title: Option<String>,
}

/// Error type for session validation
#[derive(Debug, Clone, thiserror::Error)]
pub enum SessionValidationError {
    #[error("Agent ID is required")]
    MissingAgentId,
}

impl NewSession {
    /// Get current Unix timestamp
    ///
    /// # Panics
    ///
    /// Panics if the system clock is set before the Unix epoch (1970-01-01),
    /// which indicates a severely misconfigured system.
    pub fn current_timestamp() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System clock is set before Unix epoch")
            .as_secs() as i64
    }
}

/// Partial data for updating an existing session
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdate {
    /// New title for the session
    pub title: Option<String>,
}

/// Persistent message model stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// Database auto-increment ID
    pub id: i64,
    /// ID of the session this message belongs to
    pub session_id: i64,
    /// Role indicating who sent the message
    pub role: MessageRole,
    /// Content of the message
    pub content: String,
    /// Unix timestamp of creation
    pub created_at: i64,
    /// Optional ID of the quoted message (for reply functionality)
    pub quote_message_id: Option<i64>,
    /// Whether the message is marked as important
    ///
    /// Marked messages receive higher importance scores
    /// when stored to episodic memory (L2).
    ///
    /// [Source: Story 5.8 - 重要片段标记功能]
    #[serde(default)]
    pub is_marked: bool,
}

/// Data required to create a new message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMessage {
    /// ID of the session this message belongs to (required)
    pub session_id: i64,
    /// Role indicating who sends the message (required)
    pub role: MessageRole,
    /// Content of the message (required)
    pub content: String,
    /// Optional ID of the quoted message (for reply functionality)
    pub quote_message_id: Option<i64>,
}

/// Error type for message validation
#[derive(Debug, Clone, thiserror::Error)]
pub enum MessageValidationError {
    #[error("Session ID is required")]
    MissingSessionId,

    #[error("Message content is required")]
    EmptyContent,

    #[error("Message content exceeds maximum length of {0} characters")]
    ContentTooLong(usize),
}

impl NewMessage {
    /// Validate the message data
    pub fn validate(&self) -> Result<(), MessageValidationError> {
        if self.content.trim().is_empty() {
            return Err(MessageValidationError::EmptyContent);
        }

        const MAX_CONTENT_LENGTH: usize = 100_000; // 100K characters
        if self.content.len() > MAX_CONTENT_LENGTH {
            return Err(MessageValidationError::ContentTooLong(MAX_CONTENT_LENGTH));
        }

        Ok(())
    }

    /// Get current Unix timestamp
    pub fn current_timestamp() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System clock is set before Unix epoch")
            .as_secs() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_role_serialization() {
        let role = MessageRole::User;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, "\"user\"");

        let parsed: MessageRole = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, MessageRole::User);
    }

    #[test]
    fn test_message_role_from_str() {
        assert_eq!("user".parse::<MessageRole>().unwrap(), MessageRole::User);
        assert_eq!("ASSISTANT".parse::<MessageRole>().unwrap(), MessageRole::Assistant);
        assert_eq!("System".parse::<MessageRole>().unwrap(), MessageRole::System);
        assert!("invalid".parse::<MessageRole>().is_err());
    }

    #[test]
    fn test_message_role_display() {
        assert_eq!(format!("{}", MessageRole::User), "user");
        assert_eq!(format!("{}", MessageRole::Assistant), "assistant");
        assert_eq!(format!("{}", MessageRole::System), "system");
    }

    #[test]
    fn test_session_serialization() {
        let session = Session {
            id: 1,
            agent_id: 42,
            title: Some("Test Session".to_string()),
            created_at: 1700000000,
            updated_at: 1700000000,
        };

        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("\"agentId\":42"));
        assert!(json.contains("\"title\":\"Test Session\""));

        let parsed: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, session.id);
        assert_eq!(parsed.agent_id, session.agent_id);
    }

    #[test]
    fn test_new_session() {
        let new_session = NewSession {
            agent_id: 1,
            title: Some("New Chat".to_string()),
        };

        let json = serde_json::to_string(&new_session).unwrap();
        assert!(json.contains("\"agentId\":1"));

        let parsed: NewSession = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agent_id, 1);
    }

    #[test]
    fn test_session_update() {
        let update = SessionUpdate {
            title: Some("Updated Title".to_string()),
        };

        let json = serde_json::to_string(&update).unwrap();
        let parsed: SessionUpdate = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.title, Some("Updated Title".to_string()));
    }

    #[test]
    fn test_message_serialization() {
        let message = Message {
            id: 1,
            session_id: 42,
            role: MessageRole::User,
            content: "Hello, world!".to_string(),
            created_at: 1700000000,
            quote_message_id: None,
            is_marked: false,
        };

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("\"sessionId\":42"));
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"Hello, world!\""));

        let parsed: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, message.id);
        assert_eq!(parsed.role, MessageRole::User);
    }

    #[test]
    fn test_new_message() {
        let new_message = NewMessage {
            session_id: 1,
            role: MessageRole::Assistant,
            content: "I am here to help.".to_string(),
            quote_message_id: None,
        };

        let json = serde_json::to_string(&new_message).unwrap();
        assert!(json.contains("\"sessionId\":1"));
        assert!(json.contains("\"role\":\"assistant\""));

        let parsed: NewMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.session_id, 1);
        assert_eq!(parsed.role, MessageRole::Assistant);
    }

    #[test]
    fn test_new_message_validation_valid() {
        let message = NewMessage {
            session_id: 1,
            role: MessageRole::User,
            content: "Valid message".to_string(),
            quote_message_id: None,
        };
        assert!(message.validate().is_ok());
    }

    #[test]
    fn test_new_message_validation_empty_content() {
        let message = NewMessage {
            session_id: 1,
            role: MessageRole::User,
            content: "   ".to_string(), // Whitespace only
            quote_message_id: None,
        };
        assert!(matches!(message.validate(), Err(MessageValidationError::EmptyContent)));
    }

    #[test]
    fn test_new_message_validation_content_too_long() {
        let message = NewMessage {
            session_id: 1,
            role: MessageRole::User,
            content: "x".repeat(100_001), // Exceeds 100K limit
            quote_message_id: None,
        };
        assert!(matches!(message.validate(), Err(MessageValidationError::ContentTooLong(100_000))));
    }

    #[test]
    fn test_current_timestamp() {
        let ts = NewSession::current_timestamp();
        assert!(ts > 1700000000); // Should be after 2023
    }
}