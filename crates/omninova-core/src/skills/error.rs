//! Skill Error Types
//!
//! Error types for the skill system.

use serde::{Deserialize, Serialize};

/// Skill error type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, thiserror::Error)]
#[serde(rename_all = "camelCase")]
pub enum SkillError {
    /// Configuration error.
    #[error("Configuration error: {message}")]
    ConfigurationError {
        /// Error message.
        message: String,
    },

    /// Execution error.
    #[error("Execution error: {message}")]
    ExecutionError {
        /// Error message.
        message: String,
    },

    /// Timeout error.
    #[error("Execution timed out after {timeout_ms}ms")]
    TimeoutError {
        /// Timeout duration in milliseconds.
        timeout_ms: u64,
    },

    /// Permission error.
    #[error("Permission denied: required {required}")]
    PermissionError {
        /// Required permission.
        required: String,
    },

    /// Dependency error.
    #[error("Missing dependencies: {missing:?}")]
    DependencyError {
        /// List of missing dependency IDs.
        missing: Vec<String>,
    },

    /// Validation error.
    #[error("Validation failed: {errors:?}")]
    ValidationError {
        /// List of validation errors.
        errors: Vec<String>,
    },

    /// Skill not found.
    #[error("Skill not found: {skill_id}")]
    NotFoundError {
        /// Skill ID that was not found.
        skill_id: String,
    },

    /// Not registered error.
    #[error("Skill not registered: {skill_id}")]
    NotRegisteredError {
        /// Skill ID.
        skill_id: String,
    },
}

impl SkillError {
    /// Create a configuration error.
    pub fn config(message: impl Into<String>) -> Self {
        Self::ConfigurationError {
            message: message.into(),
        }
    }

    /// Create an execution error.
    pub fn execution(message: impl Into<String>) -> Self {
        Self::ExecutionError {
            message: message.into(),
        }
    }

    /// Create a timeout error.
    pub fn timeout(timeout_ms: u64) -> Self {
        Self::TimeoutError { timeout_ms }
    }

    /// Create a permission error.
    pub fn permission(required: impl Into<String>) -> Self {
        Self::PermissionError {
            required: required.into(),
        }
    }

    /// Create a dependency error.
    pub fn dependency(missing: Vec<String>) -> Self {
        Self::DependencyError { missing }
    }

    /// Create a validation error.
    pub fn validation(errors: Vec<String>) -> Self {
        Self::ValidationError { errors }
    }

    /// Create a not found error.
    pub fn not_found(skill_id: impl Into<String>) -> Self {
        Self::NotFoundError {
            skill_id: skill_id.into(),
        }
    }

    /// Create a not registered error.
    pub fn not_registered(skill_id: impl Into<String>) -> Self {
        Self::NotRegisteredError {
            skill_id: skill_id.into(),
        }
    }
}