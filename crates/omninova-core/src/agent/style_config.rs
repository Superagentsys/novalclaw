//! Agent Style Configuration
//!
//! Defines response style settings for AI agents.
//! [Source: Story 7.1 - 代理响应风格配置]

use serde::{Deserialize, Serialize};
use crate::channels::behavior::ResponseStyle;

/// Agent style configuration for response customization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentStyleConfig {
    /// Primary response style
    #[serde(default)]
    pub response_style: ResponseStyle,
    /// Verbosity level (0.0-1.0, affects detail level)
    /// 0.0 = very brief, 1.0 = very detailed
    #[serde(default = "default_verbosity")]
    pub verbosity: f32,
    /// Maximum response length (0 = no limit)
    #[serde(default)]
    pub max_response_length: usize,
    /// Enable friendly additions (greetings, sign-offs)
    #[serde(default = "default_friendly_tone")]
    pub friendly_tone: bool,
}

fn default_verbosity() -> f32 {
    0.5
}

fn default_friendly_tone() -> bool {
    true
}

impl Default for AgentStyleConfig {
    fn default() -> Self {
        Self {
            response_style: ResponseStyle::default(),
            verbosity: default_verbosity(),
            max_response_length: 0,
            friendly_tone: default_friendly_tone(),
        }
    }
}

/// Verbosity presets for easy configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerbosityPreset {
    /// Brief responses (0.2)
    Brief,
    /// Normal responses (0.5)
    Normal,
    /// Detailed responses (0.8)
    Detailed,
}

impl VerbosityPreset {
    /// Get the numeric verbosity value for this preset
    pub fn value(&self) -> f32 {
        match self {
            Self::Brief => 0.2,
            Self::Normal => 0.5,
            Self::Detailed => 0.8,
        }
    }

    /// Convert a verbosity value to the nearest preset
    pub fn from_value(value: f32) -> Self {
        if value <= 0.35 {
            Self::Brief
        } else if value >= 0.65 {
            Self::Detailed
        } else {
            Self::Normal
        }
    }
}

impl AgentStyleConfig {
    /// Create a new style config with the given response style
    pub fn new(style: ResponseStyle) -> Self {
        Self {
            response_style: style,
            ..Default::default()
        }
    }

    /// Set the response style
    pub fn with_response_style(mut self, style: ResponseStyle) -> Self {
        self.response_style = style;
        self
    }

    /// Set the verbosity level (clamped to 0.0-1.0)
    pub fn with_verbosity(mut self, verbosity: f32) -> Self {
        self.verbosity = verbosity.clamp(0.0, 1.0);
        self
    }

    /// Set the maximum response length
    pub fn with_max_length(mut self, length: usize) -> Self {
        self.max_response_length = length;
        self
    }

    /// Set the friendly tone flag
    pub fn with_friendly_tone(mut self, friendly: bool) -> Self {
        self.friendly_tone = friendly;
        self
    }

    /// Parse from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Get the effective max length based on verbosity
    /// Returns a suggested max length for the response processor
    pub fn effective_max_length(&self) -> Option<usize> {
        if self.max_response_length > 0 {
            Some(self.max_response_length)
        } else {
            // No explicit limit set, calculate from verbosity
            // High verbosity = longer allowed, low verbosity = shorter
            if self.verbosity < 0.3 {
                Some(500) // Brief
            } else if self.verbosity < 0.7 {
                None // No limit for normal
            } else {
                None // No limit for detailed
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AgentStyleConfig::default();
        assert_eq!(config.response_style, ResponseStyle::Detailed);
        assert_eq!(config.verbosity, 0.5);
        assert_eq!(config.max_response_length, 0);
        assert!(config.friendly_tone);
    }

    #[test]
    fn test_builder_pattern() {
        let config = AgentStyleConfig::new(ResponseStyle::Concise)
            .with_verbosity(0.3)
            .with_max_length(1000)
            .with_friendly_tone(false);

        assert_eq!(config.response_style, ResponseStyle::Concise);
        assert_eq!(config.verbosity, 0.3);
        assert_eq!(config.max_response_length, 1000);
        assert!(!config.friendly_tone);
    }

    #[test]
    fn test_verbosity_clamping() {
        let config = AgentStyleConfig::default().with_verbosity(1.5);
        assert_eq!(config.verbosity, 1.0);

        let config = AgentStyleConfig::default().with_verbosity(-0.5);
        assert_eq!(config.verbosity, 0.0);
    }

    #[test]
    fn test_serialization() {
        let config = AgentStyleConfig::new(ResponseStyle::Casual)
            .with_verbosity(0.7)
            .with_max_length(2000);

        let json = config.to_json().unwrap();
        assert!(json.contains("\"responseStyle\":\"casual\""));
        assert!(json.contains("\"verbosity\":0.7"));
        assert!(json.contains("\"maxResponseLength\":2000"));

        let parsed: AgentStyleConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn test_from_json() {
        let json = r#"{"responseStyle":"formal","verbosity":0.8,"maxResponseLength":0,"friendlyTone":false}"#;
        let config = AgentStyleConfig::from_json(json).unwrap();
        assert_eq!(config.response_style, ResponseStyle::Formal);
        assert_eq!(config.verbosity, 0.8);
        assert!(!config.friendly_tone);
    }

    #[test]
    fn test_verbosity_preset() {
        assert_eq!(VerbosityPreset::Brief.value(), 0.2);
        assert_eq!(VerbosityPreset::Normal.value(), 0.5);
        assert_eq!(VerbosityPreset::Detailed.value(), 0.8);

        assert_eq!(VerbosityPreset::from_value(0.1), VerbosityPreset::Brief);
        assert_eq!(VerbosityPreset::from_value(0.5), VerbosityPreset::Normal);
        assert_eq!(VerbosityPreset::from_value(0.9), VerbosityPreset::Detailed);
    }

    #[test]
    fn test_effective_max_length() {
        // Brief verbosity
        let config = AgentStyleConfig::default().with_verbosity(0.2);
        assert_eq!(config.effective_max_length(), Some(500));

        // Normal verbosity
        let config = AgentStyleConfig::default().with_verbosity(0.5);
        assert_eq!(config.effective_max_length(), None);

        // Explicit max length overrides verbosity
        let config = AgentStyleConfig::default()
            .with_verbosity(0.2)
            .with_max_length(1000);
        assert_eq!(config.effective_max_length(), Some(1000));
    }
}