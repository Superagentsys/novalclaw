//! OpenClaw Skill Adapter
//!
//! Provides compatibility with the OpenClaw skill format (ARCH-15).
//! This adapter allows loading and executing skills defined in the OpenClaw
//! YAML/JSON format.
//!
//! # OpenClaw Skill Format
//!
//! OpenClaw skills are defined using a simple YAML or JSON structure:
//!
//! ```yaml
//! name: "Web Search"
//! description: "Search the web for information"
//! version: "1.0.0"
//! prompt_template: |
//!   Search the web for: {{input}}
//!   Return the most relevant results.
//! parameters:
//!   - name: "max_results"
//!     type: "integer"
//!     description: "Maximum number of results"
//!     required: false
//!     default: 5
//! output_format: "json"
//! examples:
//!   - "What is the capital of France?"
//! ```
//!
//! # Usage
//!
//! ```rust
//! use omninova_core::skills::openclaw::OpenClawSkillAdapter;
//!
//! let yaml = r#"
//! name: "Calculator"
//! description: "Performs calculations"
//! prompt_template: "Calculate: {{input}}"
//! "#;
//!
//! let skill = OpenClawSkillAdapter::from_yaml(yaml).unwrap();
//! println!("Loaded skill: {}", skill.metadata().name);
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::traits::{Skill, SkillMetadata, SkillResult};
use super::context::SkillContext;
use super::error::SkillError;

// ============================================================================
// OpenClaw Skill Definition
// ============================================================================

/// OpenClaw skill definition format.
///
/// This struct represents the OpenClaw skill format for deserialization
/// from YAML or JSON files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawSkillDefinition {
    /// Skill name.
    pub name: String,

    /// Skill description.
    pub description: String,

    /// Skill version (defaults to "1.0.0").
    #[serde(default = "default_version")]
    pub version: String,

    /// Prompt template with `{{placeholder}}` variables.
    pub prompt_template: String,

    /// Input parameters definition.
    #[serde(default)]
    pub parameters: Vec<OpenClawParameter>,

    /// Expected output format (e.g., "json", "text").
    #[serde(default)]
    pub output_format: Option<String>,

    /// Usage examples.
    #[serde(default)]
    pub examples: Vec<String>,

    /// Author information.
    #[serde(default)]
    pub author: Option<String>,

    /// Tags for categorization.
    #[serde(default)]
    pub tags: Vec<String>,

    /// Homepage URL.
    #[serde(default)]
    pub homepage: Option<String>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

/// OpenClaw parameter definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawParameter {
    /// Parameter name.
    pub name: String,

    /// Parameter type (string, integer, boolean, etc.).
    #[serde(rename = "type")]
    pub param_type: String,

    /// Parameter description.
    #[serde(default)]
    pub description: Option<String>,

    /// Whether the parameter is required.
    #[serde(default)]
    pub required: bool,

    /// Default value if not provided.
    #[serde(default)]
    pub default: Option<serde_json::Value>,

    /// Enumeration of allowed values.
    #[serde(default)]
    pub enum_values: Option<Vec<String>>,
}

impl OpenClawParameter {
    /// Create a new parameter.
    pub fn new(name: impl Into<String>, param_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            param_type: param_type.into(),
            description: None,
            required: false,
            default: None,
            enum_values: None,
        }
    }

    /// Set description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Mark as required.
    pub fn as_required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Set default value.
    pub fn with_default(mut self, value: serde_json::Value) -> Self {
        self.default = Some(value);
        self
    }
}

// ============================================================================
// OpenClaw Skill Adapter
// ============================================================================

/// Adapter for OpenClaw-format skills.
///
/// Implements the `Skill` trait for skills defined in the OpenClaw format.
/// The adapter parses the skill definition and renders prompt templates.
pub struct OpenClawSkillAdapter {
    /// Original skill definition.
    definition: OpenClawSkillDefinition,

    /// Converted metadata.
    metadata: SkillMetadata,

    /// Computed skill ID.
    skill_id: String,
}

impl OpenClawSkillAdapter {
    /// Load a skill from YAML content.
    ///
    /// # Errors
    ///
    /// Returns an error if the YAML is malformed or required fields are missing.
    pub fn from_yaml(yaml_content: &str) -> Result<Self, SkillError> {
        let definition: OpenClawSkillDefinition = serde_yaml::from_str(yaml_content).map_err(|e| {
            SkillError::ConfigurationError {
                message: format!("Failed to parse OpenClaw skill YAML: {}", e),
            }
        })?;

        Self::from_definition(definition)
    }

    /// Load a skill from JSON content.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON is malformed or required fields are missing.
    pub fn from_json(json_content: &str) -> Result<Self, SkillError> {
        let definition: OpenClawSkillDefinition = serde_json::from_str(json_content).map_err(|e| {
            SkillError::ConfigurationError {
                message: format!("Failed to parse OpenClaw skill JSON: {}", e),
            }
        })?;

        Self::from_definition(definition)
    }

    /// Create an adapter from a definition.
    fn from_definition(definition: OpenClawSkillDefinition) -> Result<Self, SkillError> {
        // Generate skill ID from name
        let skill_id = format!(
            "openclaw-{}",
            definition.name.to_lowercase().replace(' ', "-")
        );

        // Build metadata
        let mut metadata = SkillMetadata::new(
            &skill_id,
            &definition.name,
            &definition.version,
            &definition.description,
        );

        // Add author
        if let Some(author) = &definition.author {
            metadata = metadata.with_author(author);
        }

        // Add tags
        metadata.tags = definition.tags.clone();
        metadata.tags.push("openclaw".to_string());

        // Add homepage
        if let Some(homepage) = &definition.homepage {
            metadata = metadata.with_homepage(homepage);
        }

        Ok(Self {
            definition,
            metadata,
            skill_id,
        })
    }

    /// Get the skill ID.
    pub fn skill_id(&self) -> &str {
        &self.skill_id
    }

    /// Get the original definition.
    pub fn definition(&self) -> &OpenClawSkillDefinition {
        &self.definition
    }

    /// Render the prompt template with context values.
    ///
    /// Replaces `{{placeholder}}` patterns with values from:
    /// 1. `{{input}}` - replaced with user input
    /// 2. Other placeholders - replaced from config values
    pub fn render_prompt(&self, context: &SkillContext) -> String {
        let mut prompt = self.definition.prompt_template.clone();

        // Replace user input
        prompt = prompt.replace("{{input}}", &context.user_input);

        // Replace config parameters
        for (key, value) in &context.config {
            let placeholder = format!("{{{{{}}}}}", key);
            if let Some(str_val) = value.as_str() {
                prompt = prompt.replace(&placeholder, str_val);
            } else {
                prompt = prompt.replace(&placeholder, &value.to_string());
            }
        }

        // Apply default values for missing parameters
        for param in &self.definition.parameters {
            if !context.config.contains_key(&param.name) {
                if let Some(default) = &param.default {
                    let placeholder = format!("{{{{{}}}}}", param.name);
                    if let Some(str_val) = default.as_str() {
                        prompt = prompt.replace(&placeholder, str_val);
                    } else {
                        prompt = prompt.replace(&placeholder, &default.to_string());
                    }
                }
            }
        }

        prompt
    }

    /// Get the output format.
    pub fn output_format(&self) -> Option<&str> {
        self.definition.output_format.as_deref()
    }

    /// Get examples.
    pub fn examples(&self) -> &[String] {
        &self.definition.examples
    }

    /// Get parameters.
    pub fn parameters(&self) -> &[OpenClawParameter] {
        &self.definition.parameters
    }
}

#[async_trait]
impl Skill for OpenClawSkillAdapter {
    fn metadata(&self) -> &SkillMetadata {
        &self.metadata
    }

    fn validate(&self, config: &HashMap<String, serde_json::Value>) -> Result<(), SkillError> {
        // Validate required parameters
        let missing: Vec<String> = self.definition.parameters
            .iter()
            .filter(|p| p.required && !config.contains_key(&p.name) && p.default.is_none())
            .map(|p| p.name.clone())
            .collect();

        if !missing.is_empty() {
            return Err(SkillError::validation(
                missing.iter().map(|m| format!("Missing required parameter: {}", m)).collect()
            ));
        }

        // Validate enum values
        for param in &self.definition.parameters {
            if let Some(value) = config.get(&param.name) {
                if let Some(enum_vals) = &param.enum_values {
                    if let Some(str_val) = value.as_str() {
                        if !enum_vals.contains(&str_val.to_string()) {
                            return Err(SkillError::validation(vec![format!(
                                "Parameter '{}' must be one of: {:?}",
                                param.name, enum_vals
                            )]));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn execute(&self, context: SkillContext) -> Result<SkillResult, SkillError> {
        let start = std::time::Instant::now();

        // Render the prompt
        let prompt = self.render_prompt(&context);

        // For OpenClaw skills, we return the rendered prompt
        // The actual LLM call will be handled by the Agent Dispatcher
        let result = SkillResult::success_full(
            prompt.clone(),
            serde_json::json!({
                "skill_type": "openclaw",
                "skill_id": self.skill_id,
                "skill_name": self.definition.name,
                "output_format": self.definition.output_format,
            })
        )
        .with_duration(start.elapsed().as_millis() as u64);

        Ok(result)
    }

    fn describe(&self) -> String {
        let mut desc = format!(
            "## {}\n\n{}\n\n**Version:** {}\n**Format:** OpenClaw",
            self.definition.name,
            self.definition.description,
            self.definition.version
        );

        if !self.definition.parameters.is_empty() {
            desc.push_str("\n\n**Parameters:**\n");
            for param in &self.definition.parameters {
                let required = if param.required { " (required)" } else { "" };
                desc.push_str(&format!(
                    "- `{}{}` ({}): {}\n",
                    param.name,
                    required,
                    param.param_type,
                    param.description.as_deref().unwrap_or("No description")
                ));
            }
        }

        if !self.definition.examples.is_empty() {
            desc.push_str("\n**Examples:**\n");
            for example in &self.definition.examples {
                desc.push_str(&format!("- {}\n", example));
            }
        }

        desc
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_YAML: &str = r#"
name: "Web Search"
description: "Search the web for information"
version: "1.0.0"
promptTemplate: |
  Search the web for: {{input}}
  Return the most relevant results.
"#;

    const FULL_YAML: &str = r#"
name: "Calculator"
description: "Performs calculations"
version: "2.0.0"
author: "OmniNova Team"
promptTemplate: "Calculate: {{input}} with precision {{precision}}"
parameters:
  - name: "precision"
    type: "integer"
    description: "Number of decimal places"
    required: false
    default: 2
outputFormat: "json"
examples:
  - "What is 2 + 2?"
  - "Calculate 15% of 200"
tags:
  - "math"
  - "productivity"
homepage: "https://example.com/calculator"
"#;

    const JSON_SKILL: &str = r#"{
        "name": "JSON Skill",
        "description": "A skill defined in JSON",
        "version": "1.0.0",
        "promptTemplate": "Process: {{input}}"
    }"#;

    // ============================================================================
    // OpenClawSkillDefinition Tests
    // ============================================================================

    #[test]
    fn test_parse_simple_yaml() {
        let skill = OpenClawSkillAdapter::from_yaml(SIMPLE_YAML).unwrap();
        assert_eq!(skill.metadata().name, "Web Search");
        assert_eq!(skill.metadata().version, "1.0.0");
    }

    #[test]
    fn test_parse_full_yaml() {
        let skill = OpenClawSkillAdapter::from_yaml(FULL_YAML).unwrap();
        assert_eq!(skill.metadata().name, "Calculator");
        assert_eq!(skill.metadata().version, "2.0.0");
        assert_eq!(skill.metadata().author, Some("OmniNova Team".to_string()));
        assert!(skill.metadata().tags.contains(&"math".to_string()));
        assert!(skill.metadata().tags.contains(&"productivity".to_string()));
        assert!(skill.metadata().tags.contains(&"openclaw".to_string()));
        assert_eq!(skill.parameters().len(), 1);
        assert_eq!(skill.examples().len(), 2);
    }

    #[test]
    fn test_parse_json() {
        let skill = OpenClawSkillAdapter::from_json(JSON_SKILL).unwrap();
        assert_eq!(skill.metadata().name, "JSON Skill");
        assert_eq!(skill.metadata().description, "A skill defined in JSON");
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let result = OpenClawSkillAdapter::from_yaml("invalid: [");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_json() {
        let result = OpenClawSkillAdapter::from_json("{invalid}");
        assert!(result.is_err());
    }

    #[test]
    fn test_skill_id_generation() {
        let skill = OpenClawSkillAdapter::from_yaml(SIMPLE_YAML).unwrap();
        assert_eq!(skill.skill_id(), "openclaw-web-search");
    }

    // ============================================================================
    // Parameter Tests
    // ============================================================================

    #[test]
    fn test_parameter_new() {
        let param = OpenClawParameter::new("test", "string");
        assert_eq!(param.name, "test");
        assert_eq!(param.param_type, "string");
        assert!(!param.required);
        assert!(param.default.is_none());
    }

    #[test]
    fn test_parameter_builder() {
        let param = OpenClawParameter::new("count", "integer")
            .with_description("Number of items")
            .as_required()
            .with_default(serde_json::json!(10));

        assert_eq!(param.description, Some("Number of items".to_string()));
        assert!(param.required);
        assert_eq!(param.default, Some(serde_json::json!(10)));
    }

    // ============================================================================
    // Prompt Rendering Tests
    // ============================================================================

    #[test]
    fn test_render_prompt_basic() {
        let skill = OpenClawSkillAdapter::from_yaml(SIMPLE_YAML).unwrap();
        let ctx = SkillContext::new("agent", "What is the weather?");
        let prompt = skill.render_prompt(&ctx);

        assert!(prompt.contains("What is the weather?"));
        assert!(prompt.contains("Search the web for"));
    }

    #[test]
    fn test_render_prompt_with_params() {
        let skill = OpenClawSkillAdapter::from_yaml(FULL_YAML).unwrap();
        let ctx = SkillContext::new("agent", "2 + 2")
            .with_config("precision", serde_json::json!(4));

        let prompt = skill.render_prompt(&ctx);
        assert!(prompt.contains("Calculate: 2 + 2 with precision 4"));
    }

    #[test]
    fn test_render_prompt_with_default() {
        let skill = OpenClawSkillAdapter::from_yaml(FULL_YAML).unwrap();
        let ctx = SkillContext::new("agent", "2 + 2");
        // No precision config, should use default 2

        let prompt = skill.render_prompt(&ctx);
        assert!(prompt.contains("with precision 2"));
    }

    // ============================================================================
    // Validation Tests
    // ============================================================================

    const REQUIRED_PARAM_YAML: &str = r#"
name: "Required"
description: "Has required param"
promptTemplate: "{{input}} {{required_param}}"
parameters:
  - name: "required_param"
    type: "string"
    required: true
"#;

    #[tokio::test]
    async fn test_validate_success() {
        let skill = OpenClawSkillAdapter::from_yaml(REQUIRED_PARAM_YAML).unwrap();
        let mut config = HashMap::new();
        config.insert("required_param".to_string(), serde_json::json!("value"));

        assert!(skill.validate(&config).is_ok());
    }

    #[tokio::test]
    async fn test_validate_missing_required() {
        let skill = OpenClawSkillAdapter::from_yaml(REQUIRED_PARAM_YAML).unwrap();
        let config = HashMap::new();

        let result = skill.validate(&config);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate_enum() {
        let yaml = r#"
name: "Enum Test"
description: "Test"
promptTemplate: "{{input}}"
parameters:
  - name: "mode"
    type: "string"
    enumValues: ["fast", "slow"]
"#;
        let skill = OpenClawSkillAdapter::from_yaml(yaml).unwrap();

        // Valid enum value
        let mut config = HashMap::new();
        config.insert("mode".to_string(), serde_json::json!("fast"));
        assert!(skill.validate(&config).is_ok());

        // Invalid enum value
        config.insert("mode".to_string(), serde_json::json!("medium"));
        assert!(skill.validate(&config).is_err());
    }

    // ============================================================================
    // Execute Tests
    // ============================================================================

    #[tokio::test]
    async fn test_execute() {
        let skill = OpenClawSkillAdapter::from_yaml(SIMPLE_YAML).unwrap();
        let ctx = SkillContext::new("agent", "test query");

        let result = skill.execute(ctx).await.unwrap();
        assert!(result.success);
        assert!(result.content.unwrap().contains("test query"));
        assert!(result.data.is_some());

        let data = result.data.unwrap();
        assert_eq!(data["skill_type"], "openclaw");
        assert_eq!(data["skill_name"], "Web Search");
    }

    // ============================================================================
    // Describe Tests
    // ============================================================================

    #[test]
    fn test_describe() {
        let skill = OpenClawSkillAdapter::from_yaml(FULL_YAML).unwrap();
        let desc = skill.describe();

        assert!(desc.contains("Calculator"));
        assert!(desc.contains("2.0.0"));
        assert!(desc.contains("OpenClaw"));
        assert!(desc.contains("precision"));
        assert!(desc.contains("Examples"));
    }

    // ============================================================================
    // Skill Trait Tests
    // ============================================================================

    #[tokio::test]
    async fn test_skill_trait_metadata() {
        let skill = OpenClawSkillAdapter::from_yaml(SIMPLE_YAML).unwrap();
        let meta = skill.metadata();
        assert_eq!(meta.id, "openclaw-web-search");
        assert_eq!(meta.name, "Web Search");
    }

    #[tokio::test]
    async fn test_skill_trait_validate_ok() {
        let skill = OpenClawSkillAdapter::from_yaml(SIMPLE_YAML).unwrap();
        let config = HashMap::new();
        assert!(skill.validate(&config).is_ok());
    }
}