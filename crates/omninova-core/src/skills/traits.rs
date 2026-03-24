//! Skill Trait Definitions
//!
//! Core traits for the skill system that enables AI agents to have specialized capabilities.
//! All skills must implement the [`Skill`] trait.
//!
//! # Architecture
//!
//! The skill system follows a trait-based design pattern similar to the Provider and Channel traits:
//! - [`Skill`] - Core trait that all skills must implement
//! - [`SkillMetadata`] - Information about a skill
//! - [`SkillContext`] - Execution context passed to skills
//! - [`SkillResult`] - Result returned by skill execution
//!
//! # Example
//!
//! ```rust
//! use omninova_core::skills::{Skill, SkillMetadata, SkillContext, SkillResult, SkillError};
//! use async_trait::async_trait;
//! use std::collections::HashMap;
//!
//! struct WebSearchSkill;
//!
//! #[async_trait]
//! impl Skill for WebSearchSkill {
//!     fn metadata(&self) -> &SkillMetadata {
//!         // Return skill metadata
//!         todo!()
//!     }
//!
//!     fn validate(&self, config: &HashMap<String, serde_json::Value>) -> Result<(), SkillError> {
//!         // Validate configuration
//!         Ok(())
//!     }
//!
//!     async fn execute(&self, context: SkillContext) -> Result<SkillResult, SkillError> {
//!         // Execute skill logic
//!         todo!()
//!     }
//! }
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::providers::traits::ConversationMessage;

// Import from submodules
use super::context::SkillContext;
use super::error::SkillError;

// ============================================================================
// Skill Metadata
// ============================================================================

/// Metadata describing a skill.
///
/// Contains all the information needed to identify, describe, and manage a skill.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkillMetadata {
    /// Unique identifier for the skill (e.g., "web-search", "file-ops").
    pub id: String,

    /// Human-readable name of the skill.
    pub name: String,

    /// Version string (semver recommended).
    pub version: String,

    /// Detailed description of what the skill does.
    pub description: String,

    /// Author or creator of the skill.
    #[serde(default)]
    pub author: Option<String>,

    /// Tags for categorization and filtering.
    #[serde(default)]
    pub tags: Vec<String>,

    /// IDs of other skills this skill depends on.
    #[serde(default)]
    pub dependencies: Vec<String>,

    /// Whether this is a built-in skill.
    #[serde(default)]
    pub is_builtin: bool,

    /// JSON Schema for skill configuration.
    #[serde(default)]
    pub config_schema: Option<serde_json::Value>,

    /// Homepage or documentation URL.
    #[serde(default)]
    pub homepage: Option<String>,
}

impl SkillMetadata {
    /// Create new skill metadata with required fields.
    pub fn new(id: impl Into<String>, name: impl Into<String>, version: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            description: description.into(),
            author: None,
            tags: Vec::new(),
            dependencies: Vec::new(),
            is_builtin: false,
            config_schema: None,
            homepage: None,
        }
    }

    /// Set the author.
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// Add a tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Add a dependency.
    pub fn with_dependency(mut self, dep: impl Into<String>) -> Self {
        self.dependencies.push(dep.into());
        self
    }

    /// Mark as built-in.
    pub fn as_builtin(mut self) -> Self {
        self.is_builtin = true;
        self
    }

    /// Set config schema.
    pub fn with_config_schema(mut self, schema: serde_json::Value) -> Self {
        self.config_schema = Some(schema);
        self
    }

    /// Set homepage.
    pub fn with_homepage(mut self, url: impl Into<String>) -> Self {
        self.homepage = Some(url.into());
        self
    }
}

// ============================================================================
// Skill Result
// ============================================================================

/// Result returned by skill execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkillResult {
    /// Whether execution was successful.
    pub success: bool,

    /// Text content of the result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    /// Structured data returned by the skill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,

    /// Error message if not successful.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Execution duration in milliseconds.
    #[serde(default)]
    pub duration_ms: u64,

    /// Additional metadata.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

impl SkillResult {
    /// Create a successful result with text content.
    pub fn success(content: impl Into<String>) -> Self {
        Self {
            success: true,
            content: Some(content.into()),
            ..Default::default()
        }
    }

    /// Create a successful result with structured data.
    pub fn success_with_data(data: serde_json::Value) -> Self {
        Self {
            success: true,
            data: Some(data),
            ..Default::default()
        }
    }

    /// Create a successful result with both content and data.
    pub fn success_full(content: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            success: true,
            content: Some(content.into()),
            data: Some(data),
            ..Default::default()
        }
    }

    /// Create a failed result.
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            error: Some(error.into()),
            ..Default::default()
        }
    }

    /// Add execution duration.
    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = duration_ms;
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

// ============================================================================
// Skill Trait
// ============================================================================

/// Core skill trait — implement for any specialized agent capability.
///
/// All skills must implement this trait to be registered and executed
/// by the skill system.
///
/// # Required Methods
///
/// - [`metadata`] - Return skill metadata
/// - [`validate`] - Validate skill configuration
/// - [`execute`] - Execute the skill
///
/// # Optional Methods
///
/// - [`describe`] - Return a detailed description (default implementation provided)
///
/// # Thread Safety
///
/// All skill implementations must be `Send + Sync` to support
/// concurrent access from multiple threads.
///
/// # Example
///
/// ```rust
/// use omninova_core::skills::{Skill, SkillMetadata, SkillContext, SkillResult, SkillError};
/// use async_trait::async_trait;
/// use std::collections::HashMap;
/// use std::sync::Arc;
///
/// struct CalculatorSkill {
///     metadata: SkillMetadata,
/// }
///
/// impl CalculatorSkill {
///     fn new() -> Self {
///         Self {
///             metadata: SkillMetadata::new(
///                 "calculator",
///                 "Calculator",
///                 "1.0.0",
///                 "Performs basic arithmetic calculations"
///             ),
///         }
///     }
/// }
///
/// #[async_trait]
/// impl Skill for CalculatorSkill {
///     fn metadata(&self) -> &SkillMetadata {
///         &self.metadata
///     }
///
///     fn validate(&self, _config: &HashMap<String, serde_json::Value>) -> Result<(), SkillError> {
///         Ok(())
///     }
///
///     async fn execute(&self, context: SkillContext) -> Result<SkillResult, SkillError> {
///         // Parse and evaluate expression from user_input
///         Ok(SkillResult::success("42"))
///     }
/// }
/// ```
#[async_trait]
pub trait Skill: Send + Sync {
    /// Get skill metadata.
    fn metadata(&self) -> &SkillMetadata;

    /// Validate skill configuration.
    ///
    /// Called before execution to ensure configuration is valid.
    /// Returns an error if configuration is invalid.
    fn validate(&self, config: &HashMap<String, serde_json::Value>) -> Result<(), SkillError>;

    /// Describe the skill and its usage.
    ///
    /// Default implementation generates a description from metadata.
    fn describe(&self) -> String {
        let meta = self.metadata();
        let mut desc = format!(
            "## {}\n\n{}\n\n**Version:** {}",
            meta.name, meta.description, meta.version
        );

        if !meta.tags.is_empty() {
            desc.push_str(&format!("\n**Tags:** {}", meta.tags.join(", ")));
        }

        if !meta.dependencies.is_empty() {
            desc.push_str(&format!("\n**Dependencies:** {}", meta.dependencies.join(", ")));
        }

        if let Some(author) = &meta.author {
            desc.push_str(&format!("\n**Author:** {}", author));
        }

        desc
    }

    /// Execute the skill.
    ///
    /// Called to perform the skill's action with the given context.
    /// Returns a result containing the output or an error.
    async fn execute(&self, context: SkillContext) -> Result<SkillResult, SkillError>;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::context::SkillContext;

    // ============================================================================
    // SkillMetadata Tests
    // ============================================================================

    #[test]
    fn test_skill_metadata_new() {
        let meta = SkillMetadata::new("test-skill", "Test Skill", "1.0.0", "A test skill");
        assert_eq!(meta.id, "test-skill");
        assert_eq!(meta.name, "Test Skill");
        assert_eq!(meta.version, "1.0.0");
        assert_eq!(meta.description, "A test skill");
        assert!(meta.author.is_none());
        assert!(meta.tags.is_empty());
        assert!(meta.dependencies.is_empty());
        assert!(!meta.is_builtin);
    }

    #[test]
    fn test_skill_metadata_builder() {
        let meta = SkillMetadata::new("test", "Test", "1.0.0", "Test")
            .with_author("Test Author")
            .with_tag("productivity")
            .with_tag("automation")
            .with_dependency("base-skill")
            .as_builtin()
            .with_homepage("https://example.com/skill");

        assert_eq!(meta.author, Some("Test Author".to_string()));
        assert_eq!(meta.tags, vec!["productivity", "automation"]);
        assert_eq!(meta.dependencies, vec!["base-skill"]);
        assert!(meta.is_builtin);
        assert_eq!(meta.homepage, Some("https://example.com/skill".to_string()));
    }

    #[test]
    fn test_skill_metadata_serialize_deserialize() {
        let meta = SkillMetadata::new("test", "Test", "1.0.0", "Test skill")
            .with_author("Author")
            .with_tag("test");

        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: SkillMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "test");
        assert_eq!(deserialized.name, "Test");
        assert_eq!(deserialized.author, Some("Author".to_string()));
        assert_eq!(deserialized.tags, vec!["test"]);
    }

    // ============================================================================
    // SkillError Tests
    // ============================================================================

    #[test]
    fn test_skill_error_config() {
        let err = SkillError::config("Invalid setting");
        assert!(matches!(err, SkillError::ConfigurationError { .. }));
        assert!(err.to_string().contains("Invalid setting"));
    }

    #[test]
    fn test_skill_error_execution() {
        let err = SkillError::execution("Failed to process");
        assert!(matches!(err, SkillError::ExecutionError { .. }));
    }

    #[test]
    fn test_skill_error_timeout() {
        let err = SkillError::timeout(5000);
        assert!(matches!(err, SkillError::TimeoutError { timeout_ms: 5000 }));
    }

    #[test]
    fn test_skill_error_dependency() {
        let err = SkillError::dependency(vec!["skill-a".to_string(), "skill-b".to_string()]);
        if let SkillError::DependencyError { missing } = err {
            assert_eq!(missing, vec!["skill-a", "skill-b"]);
        } else {
            panic!("Expected DependencyError");
        }
    }

    #[test]
    fn test_skill_error_serialize_deserialize() {
        let err = SkillError::validation(vec!["Field is required".to_string()]);
        let json = serde_json::to_string(&err).unwrap();
        let deserialized: SkillError = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, SkillError::ValidationError { .. }));
    }

    // ============================================================================
    // SkillResult Tests
    // ============================================================================

    #[test]
    fn test_skill_result_success() {
        let result = SkillResult::success("Hello, World!");
        assert!(result.success);
        assert_eq!(result.content, Some("Hello, World!".to_string()));
        assert!(result.data.is_none());
        assert!(result.error.is_none());
    }

    #[test]
    fn test_skill_result_success_with_data() {
        let data = serde_json::json!({"count": 42});
        let result = SkillResult::success_with_data(data.clone());
        assert!(result.success);
        assert_eq!(result.data, Some(data));
    }

    #[test]
    fn test_skill_result_failure() {
        let result = SkillResult::failure("Something went wrong");
        assert!(!result.success);
        assert_eq!(result.error, Some("Something went wrong".to_string()));
    }

    #[test]
    fn test_skill_result_with_duration() {
        let result = SkillResult::success("OK").with_duration(150);
        assert_eq!(result.duration_ms, 150);
    }

    #[test]
    fn test_skill_result_with_metadata() {
        let result = SkillResult::success("OK")
            .with_metadata("source", "api")
            .with_metadata("version", "2.0");

        assert_eq!(result.metadata.get("source"), Some(&"api".to_string()));
        assert_eq!(result.metadata.get("version"), Some(&"2.0".to_string()));
    }

    // ============================================================================
    // SkillContext Tests
    // ============================================================================

    #[test]
    fn test_skill_context_new() {
        let ctx = SkillContext::new("agent-123", "Hello");
        assert_eq!(ctx.agent_id, "agent-123");
        assert_eq!(ctx.user_input, "Hello");
        assert!(ctx.session_id.is_none());
        assert!(ctx.conversation_history.is_empty());
        assert!(ctx.config.is_empty());
    }

    #[test]
    fn test_skill_context_builder() {
        let ctx = SkillContext::new("agent-123", "Hello")
            .with_session("session-456")
            .with_config("max_results", serde_json::json!(10))
            .with_metadata("source", "web");

        assert_eq!(ctx.session_id, Some("session-456".to_string()));
        assert_eq!(ctx.config.get("max_results"), Some(&serde_json::json!(10)));
        assert_eq!(ctx.metadata.get("source"), Some(&"web".to_string()));
    }

    #[test]
    fn test_skill_context_config_getters() {
        let mut ctx = SkillContext::new("agent", "input");
        ctx.config.insert("string_val".to_string(), serde_json::json!("hello"));
        ctx.config.insert("int_val".to_string(), serde_json::json!(42));
        ctx.config.insert("bool_val".to_string(), serde_json::json!(true));

        assert_eq!(ctx.config_string("string_val"), Some("hello"));
        assert_eq!(ctx.config_int("int_val"), Some(42));
        assert_eq!(ctx.config_bool("bool_val"), Some(true));
        assert_eq!(ctx.config_string("nonexistent"), None);
    }

    // ============================================================================
    // Mock Skill for Testing
    // ============================================================================

    struct MockSkill {
        metadata: SkillMetadata,
    }

    impl MockSkill {
        fn new(id: &str, name: &str) -> Self {
            Self {
                metadata: SkillMetadata::new(id, name, "1.0.0", "Mock skill for testing"),
            }
        }
    }

    #[async_trait]
    impl Skill for MockSkill {
        fn metadata(&self) -> &SkillMetadata {
            &self.metadata
        }

        fn validate(&self, config: &HashMap<String, serde_json::Value>) -> Result<(), SkillError> {
            if let Some(val) = config.get("required") {
                if val.as_str().map(|s| s.is_empty()).unwrap_or(true) {
                    return Err(SkillError::validation(vec!["'required' must not be empty".to_string()]));
                }
            }
            Ok(())
        }

        async fn execute(&self, context: SkillContext) -> Result<SkillResult, SkillError> {
            Ok(SkillResult::success(format!("Executed with input: {}", context.user_input)))
        }
    }

    #[tokio::test]
    async fn test_mock_skill_metadata() {
        let skill = MockSkill::new("mock", "Mock Skill");
        let meta = skill.metadata();
        assert_eq!(meta.id, "mock");
        assert_eq!(meta.name, "Mock Skill");
    }

    #[tokio::test]
    async fn test_mock_skill_validate_success() {
        let skill = MockSkill::new("mock", "Mock");
        let config = HashMap::new();
        assert!(skill.validate(&config).is_ok());
    }

    #[tokio::test]
    async fn test_mock_skill_validate_failure() {
        let skill = MockSkill::new("mock", "Mock");
        let mut config = HashMap::new();
        config.insert("required".to_string(), serde_json::json!(""));
        let result = skill.validate(&config);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_skill_execute() {
        let skill = MockSkill::new("mock", "Mock");
        let ctx = SkillContext::new("agent", "test input");
        let result = skill.execute(ctx).await.unwrap();
        assert!(result.success);
        assert!(result.content.unwrap().contains("test input"));
    }

    #[tokio::test]
    async fn test_mock_skill_describe() {
        let skill = MockSkill::new("mock", "Mock Skill");
        let desc = skill.describe();
        assert!(desc.contains("Mock Skill"));
        assert!(desc.contains("1.0.0"));
    }
}