//! Agent data model for persistent storage
//!
//! This module defines the data structures for storing and managing
//! AI agents in the database.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Agent status enum representing the lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    /// Agent is active and can be used for conversations
    Active,
    /// Agent is temporarily disabled
    Inactive,
    /// Agent has been archived and is no longer in active use
    Archived,
}

impl Default for AgentStatus {
    fn default() -> Self {
        Self::Active
    }
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentStatus::Active => write!(f, "active"),
            AgentStatus::Inactive => write!(f, "inactive"),
            AgentStatus::Archived => write!(f, "archived"),
        }
    }
}

impl std::str::FromStr for AgentStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(AgentStatus::Active),
            "inactive" => Ok(AgentStatus::Inactive),
            "archived" => Ok(AgentStatus::Archived),
            _ => Err(format!("Invalid agent status: {}", s)),
        }
    }
}

impl AgentStatus {
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

/// Persistent agent model stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentModel {
    /// Database auto-increment ID
    pub id: i64,
    /// Unique identifier for the agent (UUID v4)
    pub agent_uuid: String,
    /// Display name of the agent
    pub name: String,
    /// Optional description of the agent's purpose
    pub description: Option<String>,
    /// Optional domain/specialization area
    pub domain: Option<String>,
    /// MBTI personality type (e.g., "INTJ", "ENFP")
    pub mbti_type: Option<String>,
    /// System prompt for the agent
    pub system_prompt: Option<String>,
    /// Current status of the agent
    pub status: AgentStatus,
    /// Default LLM provider ID for this agent
    pub default_provider_id: Option<String>,
    /// Unix timestamp of creation
    pub created_at: i64,
    /// Unix timestamp of last update
    pub updated_at: i64,
}

/// Data required to create a new agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAgent {
    /// Display name of the agent (required, 1-100 characters)
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Optional domain/specialization
    pub domain: Option<String>,
    /// Optional MBTI personality type
    pub mbti_type: Option<String>,
    /// Optional system prompt
    pub system_prompt: Option<String>,
    /// Optional default LLM provider ID
    pub default_provider_id: Option<String>,
}

/// Error type for agent validation
#[derive(Debug, Clone, thiserror::Error)]
pub enum AgentValidationError {
    #[error("Agent name is required and cannot be empty")]
    EmptyName,

    #[error("Agent name exceeds maximum length of {0} characters")]
    NameTooLong(usize),
}

impl NewAgent {
    /// Validate the agent data
    pub fn validate(&self) -> Result<(), AgentValidationError> {
        let trimmed_name = self.name.trim();

        if trimmed_name.is_empty() {
            return Err(AgentValidationError::EmptyName);
        }

        const MAX_NAME_LENGTH: usize = 100;
        if trimmed_name.len() > MAX_NAME_LENGTH {
            return Err(AgentValidationError::NameTooLong(MAX_NAME_LENGTH));
        }

        Ok(())
    }
}

/// Partial data for updating an existing agent
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentUpdate {
    /// New display name
    pub name: Option<String>,
    /// New description
    pub description: Option<String>,
    /// New domain
    pub domain: Option<String>,
    /// New MBTI personality type
    pub mbti_type: Option<String>,
    /// New system prompt
    pub system_prompt: Option<String>,
    /// New status
    pub status: Option<AgentStatus>,
    /// New default provider ID
    pub default_provider_id: Option<String>,
}

impl NewAgent {
    /// Generate a UUID for a new agent
    pub fn generate_uuid() -> String {
        Uuid::new_v4().to_string()
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_status_serialization() {
        let status = AgentStatus::Active;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"active\"");

        let parsed: AgentStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, AgentStatus::Active);
    }

    #[test]
    fn test_agent_status_from_str() {
        assert_eq!("active".parse::<AgentStatus>().unwrap(), AgentStatus::Active);
        assert_eq!("INACTIVE".parse::<AgentStatus>().unwrap(), AgentStatus::Inactive);
        assert_eq!("Archived".parse::<AgentStatus>().unwrap(), AgentStatus::Archived);
        assert!("invalid".parse::<AgentStatus>().is_err());
    }

    #[test]
    fn test_agent_status_display() {
        assert_eq!(format!("{}", AgentStatus::Active), "active");
        assert_eq!(format!("{}", AgentStatus::Inactive), "inactive");
        assert_eq!(format!("{}", AgentStatus::Archived), "archived");
    }

    #[test]
    fn test_agent_model_serialization() {
        let agent = AgentModel {
            id: 1,
            agent_uuid: "test-uuid-123".to_string(),
            name: "Test Agent".to_string(),
            description: Some("A test agent".to_string()),
            domain: Some("coding".to_string()),
            mbti_type: Some("INTJ".to_string()),
            system_prompt: Some("You are a helpful assistant.".to_string()),
            status: AgentStatus::Active,
            default_provider_id: Some("openai-provider".to_string()),
            created_at: 1700000000,
            updated_at: 1700000000,
        };

        let json = serde_json::to_string(&agent).unwrap();
        assert!(json.contains("\"agentUuid\":\"test-uuid-123\""));
        assert!(json.contains("\"mbtiType\":\"INTJ\""));
        assert!(json.contains("\"defaultProviderId\":\"openai-provider\""));

        let parsed: AgentModel = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, agent.id);
        assert_eq!(parsed.agent_uuid, agent.agent_uuid);
        assert_eq!(parsed.default_provider_id, Some("openai-provider".to_string()));
    }

    #[test]
    fn test_new_agent() {
        let new_agent = NewAgent {
            name: "New Agent".to_string(),
            description: Some("Description".to_string()),
            domain: None,
            mbti_type: Some("ENFP".to_string()),
            system_prompt: None,
            default_provider_id: Some("anthropic-provider".to_string()),
        };

        let json = serde_json::to_string(&new_agent).unwrap();
        assert!(json.contains("\"name\":\"New Agent\""));
        assert!(json.contains("\"defaultProviderId\":\"anthropic-provider\""));

        let parsed: NewAgent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "New Agent");
        assert_eq!(parsed.default_provider_id, Some("anthropic-provider".to_string()));
    }

    #[test]
    fn test_agent_update() {
        let update = AgentUpdate {
            name: Some("Updated Name".to_string()),
            status: Some(AgentStatus::Inactive),
            default_provider_id: Some("new-provider".to_string()),
            ..Default::default()
        };

        let json = serde_json::to_string(&update).unwrap();
        let parsed: AgentUpdate = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, Some("Updated Name".to_string()));
        assert_eq!(parsed.status, Some(AgentStatus::Inactive));
        assert_eq!(parsed.default_provider_id, Some("new-provider".to_string()));
        assert!(parsed.description.is_none());
    }

    #[test]
    fn test_generate_uuid() {
        let uuid = NewAgent::generate_uuid();
        assert!(uuid.len() == 36); // UUID format: 8-4-4-4-12
        assert!(uuid.contains('-'));
    }

    #[test]
    fn test_current_timestamp() {
        let ts = NewAgent::current_timestamp();
        assert!(ts > 1700000000); // Should be after 2023
    }

    #[test]
    fn test_new_agent_validation_valid() {
        let agent = NewAgent {
            name: "Valid Agent".to_string(),
            description: None,
            domain: None,
            mbti_type: None,
            system_prompt: None,
            default_provider_id: None,
        };
        assert!(agent.validate().is_ok());
    }

    #[test]
    fn test_new_agent_validation_empty_name() {
        let agent = NewAgent {
            name: "   ".to_string(), // Whitespace only
            description: None,
            domain: None,
            mbti_type: None,
            system_prompt: None,
            default_provider_id: None,
        };
        assert!(matches!(agent.validate(), Err(AgentValidationError::EmptyName)));
    }

    #[test]
    fn test_new_agent_validation_name_too_long() {
        let agent = NewAgent {
            name: "x".repeat(101), // 101 characters, exceeds limit
            description: None,
            domain: None,
            mbti_type: None,
            system_prompt: None,
            default_provider_id: None,
        };
        assert!(matches!(agent.validate(), Err(AgentValidationError::NameTooLong(100))));
    }

    #[test]
    fn test_new_agent_validation_max_length() {
        let agent = NewAgent {
            name: "x".repeat(100), // Exactly 100 characters
            description: None,
            domain: None,
            mbti_type: None,
            system_prompt: None,
            default_provider_id: None,
        };
        assert!(agent.validate().is_ok());
    }
}