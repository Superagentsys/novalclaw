//! Provider configuration model for database storage
//!
//! This module defines data structures for storing and managing
//! LLM provider configurations persistently.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Supported LLM provider types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    // Major providers
    OpenAI,
    Anthropic,
    Gemini,
    // Local/Self-hosted
    Ollama,
    LmStudio,
    LlamaCpp,
    Vllm,
    Sglang,
    // Aggregators
    OpenRouter,
    Together,
    Fireworks,
    Novita,
    // Chinese providers
    DeepSeek,
    Qwen,
    Moonshot,
    Doubao,
    Qianfan,
    Glm,
    Minimax,
    // Other providers
    Groq,
    Xai,
    Mistral,
    Perplexity,
    Cohere,
    Nvidia,
    Cloudflare,
    // Mock for testing
    Mock,
    // Custom provider
    Custom,
}

impl Default for ProviderType {
    fn default() -> Self {
        Self::OpenAI
    }
}

impl fmt::Display for ProviderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ProviderType::OpenAI => "openai",
            ProviderType::Anthropic => "anthropic",
            ProviderType::Gemini => "gemini",
            ProviderType::Ollama => "ollama",
            ProviderType::LmStudio => "lmstudio",
            ProviderType::LlamaCpp => "llamacpp",
            ProviderType::Vllm => "vllm",
            ProviderType::Sglang => "sglang",
            ProviderType::OpenRouter => "openrouter",
            ProviderType::Together => "together",
            ProviderType::Fireworks => "fireworks",
            ProviderType::Novita => "novita",
            ProviderType::DeepSeek => "deepseek",
            ProviderType::Qwen => "qwen",
            ProviderType::Moonshot => "moonshot",
            ProviderType::Doubao => "doubao",
            ProviderType::Qianfan => "qianfan",
            ProviderType::Glm => "glm",
            ProviderType::Minimax => "minimax",
            ProviderType::Groq => "groq",
            ProviderType::Xai => "xai",
            ProviderType::Mistral => "mistral",
            ProviderType::Perplexity => "perplexity",
            ProviderType::Cohere => "cohere",
            ProviderType::Nvidia => "nvidia",
            ProviderType::Cloudflare => "cloudflare",
            ProviderType::Mock => "mock",
            ProviderType::Custom => "custom",
        };
        write!(f, "{}", s)
    }
}

impl FromStr for ProviderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "openai" => Ok(ProviderType::OpenAI),
            "anthropic" => Ok(ProviderType::Anthropic),
            "gemini" => Ok(ProviderType::Gemini),
            "ollama" => Ok(ProviderType::Ollama),
            "lmstudio" => Ok(ProviderType::LmStudio),
            "llamacpp" => Ok(ProviderType::LlamaCpp),
            "vllm" => Ok(ProviderType::Vllm),
            "sglang" => Ok(ProviderType::Sglang),
            "openrouter" => Ok(ProviderType::OpenRouter),
            "together" => Ok(ProviderType::Together),
            "fireworks" => Ok(ProviderType::Fireworks),
            "novita" => Ok(ProviderType::Novita),
            "deepseek" => Ok(ProviderType::DeepSeek),
            "qwen" => Ok(ProviderType::Qwen),
            "moonshot" => Ok(ProviderType::Moonshot),
            "doubao" => Ok(ProviderType::Doubao),
            "qianfan" => Ok(ProviderType::Qianfan),
            "glm" => Ok(ProviderType::Glm),
            "minimax" => Ok(ProviderType::Minimax),
            "groq" => Ok(ProviderType::Groq),
            "xai" => Ok(ProviderType::Xai),
            "mistral" => Ok(ProviderType::Mistral),
            "perplexity" => Ok(ProviderType::Perplexity),
            "cohere" => Ok(ProviderType::Cohere),
            "nvidia" => Ok(ProviderType::Nvidia),
            "cloudflare" => Ok(ProviderType::Cloudflare),
            "mock" => Ok(ProviderType::Mock),
            "custom" => Ok(ProviderType::Custom),
            _ => Err(format!("Invalid provider type: {}", s)),
        }
    }
}

impl ProviderType {
    /// Get the default model for this provider type
    pub fn default_model(&self) -> &'static str {
        match self {
            ProviderType::OpenAI => "gpt-4o-mini",
            ProviderType::Anthropic => "claude-3-5-sonnet-latest",
            ProviderType::Gemini => "gemini-2.0-flash",
            ProviderType::Ollama => "llama3.2",
            ProviderType::LmStudio => "local-model",
            ProviderType::LlamaCpp => "local-model",
            ProviderType::Vllm => "local-model",
            ProviderType::Sglang => "local-model",
            ProviderType::OpenRouter => "anthropic/claude-3.5-sonnet",
            ProviderType::Together => "meta-llama/Llama-3.3-70B-Instruct-Turbo",
            ProviderType::Fireworks => "accounts/fireworks/models/llama-v3p1-70b-instruct",
            ProviderType::Novita => "meta-llama/llama-3.1-70b-instruct",
            ProviderType::DeepSeek => "deepseek-chat",
            ProviderType::Qwen => "qwen-max",
            ProviderType::Moonshot => "moonshot-v1-8k",
            ProviderType::Doubao => "doubao-pro-32k",
            ProviderType::Qianfan => "ernie-4.0-8k",
            ProviderType::Glm => "glm-4",
            ProviderType::Minimax => "abab6.5s-chat",
            ProviderType::Groq => "llama-3.3-70b-versatile",
            ProviderType::Xai => "grok-2-latest",
            ProviderType::Mistral => "mistral-small-latest",
            ProviderType::Perplexity => "llama-3.1-sonar-large-128k-online",
            ProviderType::Cohere => "command-r-plus",
            ProviderType::Nvidia => "meta/llama-3.1-70b-instruct",
            ProviderType::Cloudflare => "@cf/meta/llama-3.1-70b-instruct",
            ProviderType::Mock => "mock-model",
            ProviderType::Custom => "custom-model",
        }
    }

    /// Get the default base URL for this provider type
    pub fn default_base_url(&self) -> Option<&'static str> {
        match self {
            ProviderType::OpenRouter => Some("https://openrouter.ai/api/v1"),
            ProviderType::Ollama => Some("http://localhost:11434/v1"),
            ProviderType::LmStudio => Some("http://localhost:1234/v1"),
            ProviderType::LlamaCpp => Some("http://localhost:8080/v1"),
            ProviderType::Vllm => Some("http://localhost:8000/v1"),
            ProviderType::Sglang => Some("http://localhost:30000/v1"),
            ProviderType::DeepSeek => Some("https://api.deepseek.com/v1"),
            ProviderType::Qwen => Some("https://dashscope.aliyuncs.com/compatible-mode/v1"),
            ProviderType::Moonshot => Some("https://api.moonshot.cn/v1"),
            ProviderType::Groq => Some("https://api.groq.com/openai/v1"),
            ProviderType::Xai => Some("https://api.x.ai/v1"),
            ProviderType::Mistral => Some("https://api.mistral.ai/v1"),
            ProviderType::Together => Some("https://api.together.xyz/v1"),
            ProviderType::Fireworks => Some("https://api.fireworks.ai/inference/v1"),
            ProviderType::Novita => Some("https://api.novita.ai/v3/openai"),
            ProviderType::Perplexity => Some("https://api.perplexity.ai"),
            ProviderType::Cohere => Some("https://api.cohere.ai/v1"),
            ProviderType::Doubao => Some("https://ark.cn-beijing.volces.com/api/v3"),
            ProviderType::Qianfan => Some("https://aip.baidubce.com/rpc/2.0/ai_custom/v1/wenxinworkshop"),
            ProviderType::Glm => Some("https://open.bigmodel.cn/api/paas/v4"),
            ProviderType::Minimax => Some("https://api.minimax.chat/v1"),
            ProviderType::Nvidia => Some("https://integrate.api.nvidia.com/v1"),
            ProviderType::Cloudflare => Some("https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/v1"),
            ProviderType::OpenAI | ProviderType::Anthropic | ProviderType::Gemini | ProviderType::Mock | ProviderType::Custom => None,
        }
    }

    /// Get the environment variable name for the API key
    pub fn api_key_env_var(&self) -> &'static str {
        match self {
            ProviderType::OpenAI => "OPENAI_API_KEY",
            ProviderType::Anthropic => "ANTHROPIC_API_KEY",
            ProviderType::Gemini => "GEMINI_API_KEY",
            ProviderType::OpenRouter => "OPENROUTER_API_KEY",
            ProviderType::Ollama => "OLLAMA_API_KEY",
            ProviderType::LmStudio => "LMSTUDIO_API_KEY",
            ProviderType::LlamaCpp => "LLAMACPP_API_KEY",
            ProviderType::Vllm => "VLLM_API_KEY",
            ProviderType::Sglang => "SGLANG_API_KEY",
            ProviderType::DeepSeek => "DEEPSEEK_API_KEY",
            ProviderType::Qwen => "DASHSCOPE_API_KEY",
            ProviderType::Moonshot => "MOONSHOT_API_KEY",
            ProviderType::Groq => "GROQ_API_KEY",
            ProviderType::Xai => "XAI_API_KEY",
            ProviderType::Mistral => "MISTRAL_API_KEY",
            ProviderType::Together => "TOGETHER_API_KEY",
            ProviderType::Fireworks => "FIREWORKS_API_KEY",
            ProviderType::Novita => "NOVITA_API_KEY",
            ProviderType::Perplexity => "PERPLEXITY_API_KEY",
            ProviderType::Cohere => "COHERE_API_KEY",
            ProviderType::Doubao => "DOUBAO_API_KEY",
            ProviderType::Qianfan => "QIANFAN_API_KEY",
            ProviderType::Glm => "GLM_API_KEY",
            ProviderType::Minimax => "MINIMAX_API_KEY",
            ProviderType::Nvidia => "NVIDIA_API_KEY",
            ProviderType::Cloudflare => "CLOUDFLARE_API_KEY",
            ProviderType::Mock | ProviderType::Custom => "CUSTOM_API_KEY",
        }
    }

    /// Check if this provider requires an API key
    pub fn requires_api_key(&self) -> bool {
        !matches!(self, ProviderType::Ollama | ProviderType::LmStudio | ProviderType::LlamaCpp | ProviderType::Vllm | ProviderType::Sglang | ProviderType::Mock)
    }

    /// Parse from database string, returning a rusqlite-compatible error on failure
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

/// API protocol type for custom providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiProtocol {
    /// OpenAI-compatible API (/v1/chat/completions)
    Openai,
    /// Anthropic-compatible API (/v1/messages)
    Anthropic,
}

impl Default for ApiProtocol {
    fn default() -> Self {
        Self::Openai
    }
}

impl fmt::Display for ApiProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiProtocol::Openai => write!(f, "openai"),
            ApiProtocol::Anthropic => write!(f, "anthropic"),
        }
    }
}

impl FromStr for ApiProtocol {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "openai" => Ok(ApiProtocol::Openai),
            "anthropic" => Ok(ApiProtocol::Anthropic),
            _ => Err(format!("Invalid API protocol: {}", s)),
        }
    }
}

impl ApiProtocol {
    /// Parse from database string, returning a rusqlite-compatible error on failure
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

/// Provider configuration stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    /// Unique identifier (UUID)
    pub id: String,
    /// Display name
    pub name: String,
    /// Provider type
    pub provider_type: ProviderType,
    /// Reference to keychain entry for API key (not the actual key)
    pub api_key_ref: Option<String>,
    /// Base URL override
    pub base_url: Option<String>,
    /// Default model to use
    pub default_model: Option<String>,
    /// Provider-specific settings (JSON)
    pub settings: Option<String>,
    /// Whether this is the default provider
    pub is_default: bool,
    /// API protocol (only for custom provider type)
    pub api_protocol: Option<ApiProtocol>,
    /// Unix timestamp of creation
    pub created_at: i64,
    /// Unix timestamp of last update
    pub updated_at: i64,
}

/// Data required to create a new provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewProviderConfig {
    /// Display name (required, 1-100 characters)
    pub name: String,
    /// Provider type (required)
    pub provider_type: ProviderType,
    /// API key reference (optional, will be stored in keychain)
    pub api_key_ref: Option<String>,
    /// Base URL override (optional)
    pub base_url: Option<String>,
    /// Default model (optional, uses provider default if not specified)
    pub default_model: Option<String>,
    /// Provider-specific settings (optional)
    pub settings: Option<String>,
    /// Set as default provider (optional)
    pub is_default: bool,
    /// API protocol (only for custom provider type)
    pub api_protocol: Option<ApiProtocol>,
}

/// Partial data for updating an existing provider configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigUpdate {
    /// New display name
    pub name: Option<String>,
    /// New API key reference
    pub api_key_ref: Option<String>,
    /// New base URL
    pub base_url: Option<String>,
    /// New default model
    pub default_model: Option<String>,
    /// New settings
    pub settings: Option<String>,
    /// Set as default
    pub is_default: Option<bool>,
    /// New API protocol
    pub api_protocol: Option<ApiProtocol>,
}

/// Error type for provider configuration validation
#[derive(Debug, Clone, thiserror::Error)]
pub enum ProviderConfigValidationError {
    #[error("Provider name is required and cannot be empty")]
    EmptyName,

    #[error("Provider name exceeds maximum length of {0} characters")]
    NameTooLong(usize),

    #[error("Provider name already exists")]
    DuplicateName,
}

impl NewProviderConfig {
    /// Generate a UUID for a new provider config
    pub fn generate_id() -> String {
        Uuid::new_v4().to_string()
    }

    /// Get current Unix timestamp
    pub fn current_timestamp() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System clock is set before Unix epoch")
            .as_secs() as i64
    }

    /// Validate the provider config data
    pub fn validate(&self) -> Result<(), ProviderConfigValidationError> {
        let trimmed_name = self.name.trim();

        if trimmed_name.is_empty() {
            return Err(ProviderConfigValidationError::EmptyName);
        }

        const MAX_NAME_LENGTH: usize = 100;
        if trimmed_name.len() > MAX_NAME_LENGTH {
            return Err(ProviderConfigValidationError::NameTooLong(MAX_NAME_LENGTH));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_type_serialization() {
        let pt = ProviderType::OpenAI;
        let json = serde_json::to_string(&pt).unwrap();
        assert_eq!(json, "\"openai\"");

        let parsed: ProviderType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ProviderType::OpenAI);
    }

    #[test]
    fn test_provider_type_from_str() {
        assert_eq!("openai".parse::<ProviderType>().unwrap(), ProviderType::OpenAI);
        assert_eq!("ANTHROPIC".parse::<ProviderType>().unwrap(), ProviderType::Anthropic);
        assert_eq!("DeepSeek".parse::<ProviderType>().unwrap(), ProviderType::DeepSeek);
        assert!("invalid".parse::<ProviderType>().is_err());
    }

    #[test]
    fn test_provider_type_display() {
        assert_eq!(format!("{}", ProviderType::OpenAI), "openai");
        assert_eq!(format!("{}", ProviderType::Anthropic), "anthropic");
        assert_eq!(format!("{}", ProviderType::DeepSeek), "deepseek");
    }

    #[test]
    fn test_provider_type_default_model() {
        assert_eq!(ProviderType::OpenAI.default_model(), "gpt-4o-mini");
        assert_eq!(ProviderType::Anthropic.default_model(), "claude-3-5-sonnet-latest");
        assert_eq!(ProviderType::DeepSeek.default_model(), "deepseek-chat");
    }

    #[test]
    fn test_provider_type_default_base_url() {
        assert!(ProviderType::OpenAI.default_base_url().is_none());
        assert_eq!(ProviderType::Ollama.default_base_url(), Some("http://localhost:11434/v1"));
        assert_eq!(ProviderType::DeepSeek.default_base_url(), Some("https://api.deepseek.com/v1"));
    }

    #[test]
    fn test_provider_type_api_key_env_var() {
        assert_eq!(ProviderType::OpenAI.api_key_env_var(), "OPENAI_API_KEY");
        assert_eq!(ProviderType::Anthropic.api_key_env_var(), "ANTHROPIC_API_KEY");
        assert_eq!(ProviderType::DeepSeek.api_key_env_var(), "DEEPSEEK_API_KEY");
    }

    #[test]
    fn test_provider_type_requires_api_key() {
        assert!(ProviderType::OpenAI.requires_api_key());
        assert!(ProviderType::Anthropic.requires_api_key());
        assert!(!ProviderType::Ollama.requires_api_key());
        assert!(!ProviderType::LmStudio.requires_api_key());
        assert!(!ProviderType::Mock.requires_api_key());
    }

    #[test]
    fn test_provider_config_serialization() {
        let config = ProviderConfig {
            id: "test-uuid".to_string(),
            name: "My OpenAI".to_string(),
            provider_type: ProviderType::OpenAI,
            api_key_ref: Some("keychain://openai-key".to_string()),
            base_url: None,
            default_model: Some("gpt-4o".to_string()),
            settings: None,
            is_default: true,
            api_protocol: None,
            created_at: 1700000000,
            updated_at: 1700000000,
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"providerType\":\"openai\""));
        assert!(json.contains("\"isDefault\":true"));

        let parsed: ProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, config.id);
        assert_eq!(parsed.provider_type, ProviderType::OpenAI);
    }

    #[test]
    fn test_new_provider_config() {
        let new_config = NewProviderConfig {
            name: "New Provider".to_string(),
            provider_type: ProviderType::Anthropic,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            is_default: false,
            api_protocol: None,
        };

        let json = serde_json::to_string(&new_config).unwrap();
        assert!(json.contains("\"name\":\"New Provider\""));
        assert!(json.contains("\"providerType\":\"anthropic\""));

        let parsed: NewProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "New Provider");
        assert_eq!(parsed.provider_type, ProviderType::Anthropic);
    }

    #[test]
    fn test_provider_config_update() {
        let update = ProviderConfigUpdate {
            name: Some("Updated Name".to_string()),
            default_model: Some("claude-3-opus".to_string()),
            is_default: Some(true),
            ..Default::default()
        };

        let json = serde_json::to_string(&update).unwrap();
        let parsed: ProviderConfigUpdate = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, Some("Updated Name".to_string()));
        assert_eq!(parsed.default_model, Some("claude-3-opus".to_string()));
        assert!(parsed.base_url.is_none());
    }

    #[test]
    fn test_generate_id() {
        let id = NewProviderConfig::generate_id();
        assert_eq!(id.len(), 36); // UUID format: 8-4-4-4-12
        assert!(id.contains('-'));
    }

    #[test]
    fn test_current_timestamp() {
        let ts = NewProviderConfig::current_timestamp();
        assert!(ts > 1700000000); // Should be after 2023
    }

    #[test]
    fn test_new_provider_config_validation_valid() {
        let config = NewProviderConfig {
            name: "Valid Provider".to_string(),
            provider_type: ProviderType::OpenAI,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            is_default: false,
            api_protocol: None,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_new_provider_config_validation_empty_name() {
        let config = NewProviderConfig {
            name: "   ".to_string(), // Whitespace only
            provider_type: ProviderType::OpenAI,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            is_default: false,
            api_protocol: None,
        };
        assert!(matches!(config.validate(), Err(ProviderConfigValidationError::EmptyName)));
    }

    #[test]
    fn test_new_provider_config_validation_name_too_long() {
        let config = NewProviderConfig {
            name: "x".repeat(101), // 101 characters, exceeds limit
            provider_type: ProviderType::OpenAI,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            is_default: false,
            api_protocol: None,
        };
        assert!(matches!(config.validate(), Err(ProviderConfigValidationError::NameTooLong(100))));
    }

    #[test]
    fn test_new_provider_config_validation_max_length() {
        let config = NewProviderConfig {
            name: "x".repeat(100), // Exactly 100 characters
            provider_type: ProviderType::OpenAI,
            api_key_ref: None,
            base_url: None,
            default_model: None,
            settings: None,
            is_default: false,
            api_protocol: None,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_all_provider_types_serializable() {
        // Test that all provider types can be serialized and deserialized
        let types = vec![
            ProviderType::OpenAI, ProviderType::Anthropic, ProviderType::Gemini,
            ProviderType::Ollama, ProviderType::LmStudio, ProviderType::LlamaCpp,
            ProviderType::Vllm, ProviderType::Sglang, ProviderType::OpenRouter,
            ProviderType::Together, ProviderType::Fireworks, ProviderType::Novita,
            ProviderType::DeepSeek, ProviderType::Qwen, ProviderType::Moonshot,
            ProviderType::Doubao, ProviderType::Qianfan, ProviderType::Glm,
            ProviderType::Minimax, ProviderType::Groq, ProviderType::Xai,
            ProviderType::Mistral, ProviderType::Perplexity, ProviderType::Cohere,
            ProviderType::Nvidia, ProviderType::Cloudflare, ProviderType::Mock,
            ProviderType::Custom,
        ];

        for pt in types {
            let json = serde_json::to_string(&pt).unwrap();
            let parsed: ProviderType = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, pt, "Failed for {:?}", pt);
        }
    }
}