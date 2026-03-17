//! Provider registry for dynamic provider instantiation
//!
//! This module provides a registry pattern for managing and creating
//! LLM provider instances dynamically at runtime.

use crate::providers::config::ProviderType;
use crate::providers::{AnthropicProvider, GeminiProvider, MockProvider, OllamaProvider, OpenAiProvider, Provider};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

/// Factory function type for creating a provider
type ProviderFactory = Box<dyn Fn(Option<&str>, Option<&str>, Option<&str>, f32) -> Box<dyn Provider> + Send + Sync>;

/// Provider registry for dynamic provider creation
#[derive(Clone)]
pub struct ProviderRegistry {
    /// Registered provider factories
    factories: Arc<RwLock<HashMap<String, ProviderFactory>>>,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        let mut factories: HashMap<String, ProviderFactory> = HashMap::new();

        // Register built-in providers
        Self::register_builtin_providers(&mut factories);

        Self {
            factories: Arc::new(RwLock::new(factories)),
        }
    }

    /// Register all built-in providers
    fn register_builtin_providers(factories: &mut HashMap<String, ProviderFactory>) {
        // OpenAI-compatible providers (use OpenAiProvider)
        let openai_compatible = vec![
            "openai", "deepseek", "qwen", "moonshot", "groq", "xai", "mistral",
            "lmstudio", "openrouter", "together", "fireworks", "novita", "perplexity",
            "cohere", "doubao", "qianfan", "glm", "minimax", "nvidia", "cloudflare",
            "sglang", "vllm", "llamacpp",
        ];

        for provider_name in openai_compatible {
            let name = provider_name.to_string();
            factories.insert(
                name.clone(),
                Box::new(move |base_url, api_key, model, temp| {
                    Box::new(OpenAiProvider::new(base_url, api_key, model.unwrap_or("gpt-4o-mini").to_string(), temp as f64, None))
                }),
            );
        }

        // Anthropic provider
        factories.insert(
            "anthropic".to_string(),
            Box::new(|base_url, api_key, model, temp| {
                Box::new(AnthropicProvider::new(
                    base_url,
                    api_key,
                    model.unwrap_or("claude-3-5-sonnet-latest").to_string(),
                    temp as f64,
                    None,
                ))
            }),
        );

        // Gemini provider
        factories.insert(
            "gemini".to_string(),
            Box::new(|base_url, api_key, model, temp| {
                Box::new(GeminiProvider::new(
                    base_url,
                    api_key,
                    model.unwrap_or("gemini-2.0-flash").to_string(),
                    temp as f64,
                    None,
                ))
            }),
        );

        // Ollama provider (native API)
        factories.insert(
            "ollama".to_string(),
            Box::new(|base_url, _api_key, model, temp| {
                let url = base_url.or(Some("http://localhost:11434"));
                Box::new(OllamaProvider::new(
                    url,
                    model.unwrap_or("llama3.2").to_string(),
                    temp as f64,
                ))
            }),
        );

        // Mock provider for testing
        factories.insert(
            "mock".to_string(),
            Box::new(|_base_url, _api_key, _model, _temp| {
                Box::new(MockProvider::new("mock-provider"))
            }),
        );
    }

    /// Register a custom provider factory
    pub fn register<F>(&self, provider_type: &str, factory: F)
    where
        F: Fn(Option<&str>, Option<&str>, Option<&str>, f32) -> Box<dyn Provider> + Send + Sync + 'static,
    {
        let mut factories = self.factories.write().expect("Failed to acquire write lock");
        factories.insert(provider_type.to_lowercase(), Box::new(factory));
    }

    /// Create a provider instance
    ///
    /// # Arguments
    /// * `provider_type` - The type of provider to create
    /// * `base_url` - Optional base URL override
    /// * `api_key` - Optional API key
    /// * `model` - Optional model name (uses provider default if not specified)
    /// * `temperature` - Temperature setting for the provider
    ///
    /// # Returns
    /// A boxed provider instance, or a MockProvider if the type is not found
    pub fn create_provider(
        &self,
        provider_type: &ProviderType,
        base_url: Option<&str>,
        api_key: Option<&str>,
        model: Option<&str>,
        temperature: f32,
    ) -> Box<dyn Provider> {
        let factories = self.factories.read().expect("Failed to acquire read lock");
        let type_str = provider_type.to_string();

        if let Some(factory) = factories.get(&type_str) {
            factory(base_url, api_key, model, temperature)
        } else {
            // Return mock provider for unknown types
            tracing::warn!("Unknown provider type '{}', returning mock provider", type_str);
            Box::new(MockProvider::new(format!("unknown-{}", type_str)))
        }
    }

    /// Get a list of registered provider types
    pub fn list_provider_types(&self) -> Vec<String> {
        let factories = self.factories.read().expect("Failed to acquire read lock");
        let mut types: Vec<String> = factories.keys().cloned().collect();
        types.sort();
        types
    }

    /// Check if a provider type is registered
    pub fn is_registered(&self, provider_type: &str) -> bool {
        let factories = self.factories.read().expect("Failed to acquire read lock");
        factories.contains_key(&provider_type.to_lowercase())
    }

    /// Get the number of registered providers
    pub fn count(&self) -> usize {
        let factories = self.factories.read().expect("Failed to acquire read lock");
        factories.len()
    }

    /// Create a provider from a ProviderType with defaults
    pub fn create_from_type(&self, provider_type: ProviderType) -> Box<dyn Provider> {
        let base_url = provider_type.default_base_url().map(|s| s.to_string());
        let default_model = provider_type.default_model().to_string();

        self.create_provider(
            &provider_type,
            base_url.as_deref(),
            None,
            Some(&default_model),
            0.7,
        )
    }
}

/// Global provider registry instance
static GLOBAL_REGISTRY: OnceLock<ProviderRegistry> = OnceLock::new();

/// Get the global provider registry (thread-safe singleton)
pub fn global_registry() -> ProviderRegistry {
    GLOBAL_REGISTRY.get_or_init(ProviderRegistry::new).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = ProviderRegistry::new();
        assert!(registry.count() > 0);
    }

    #[test]
    fn test_registry_list_provider_types() {
        let registry = ProviderRegistry::new();
        let types = registry.list_provider_types();

        assert!(types.contains(&"openai".to_string()));
        assert!(types.contains(&"anthropic".to_string()));
        assert!(types.contains(&"gemini".to_string()));
        assert!(types.contains(&"ollama".to_string()));
        assert!(types.contains(&"deepseek".to_string()));
    }

    #[test]
    fn test_registry_is_registered() {
        let registry = ProviderRegistry::new();

        assert!(registry.is_registered("openai"));
        assert!(registry.is_registered("anthropic"));
        assert!(registry.is_registered("OPENAI")); // case insensitive
        assert!(!registry.is_registered("nonexistent"));
    }

    #[test]
    fn test_registry_create_openai_provider() {
        let registry = ProviderRegistry::new();

        let provider = registry.create_provider(
            &ProviderType::OpenAI,
            None,
            Some("test-key"),
            Some("gpt-4o"),
            0.7,
        );

        assert_eq!(provider.name(), "openai");
    }

    #[test]
    fn test_registry_create_anthropic_provider() {
        let registry = ProviderRegistry::new();

        let provider = registry.create_provider(
            &ProviderType::Anthropic,
            None,
            Some("test-key"),
            Some("claude-3-opus"),
            0.5,
        );

        assert_eq!(provider.name(), "anthropic");
    }

    #[test]
    fn test_registry_create_gemini_provider() {
        let registry = ProviderRegistry::new();

        let provider = registry.create_provider(
            &ProviderType::Gemini,
            None,
            Some("test-key"),
            None,
            0.7,
        );

        assert_eq!(provider.name(), "gemini");
    }

    #[test]
    fn test_registry_create_ollama_provider() {
        let registry = ProviderRegistry::new();

        let provider = registry.create_provider(
            &ProviderType::Ollama,
            None,
            None,
            Some("llama3.2"),
            0.7,
        );

        // Ollama now uses native provider
        assert_eq!(provider.name(), "ollama");
    }

    #[test]
    fn test_registry_create_mock_provider() {
        let registry = ProviderRegistry::new();

        let provider = registry.create_provider(
            &ProviderType::Mock,
            None,
            None,
            None,
            0.7,
        );

        assert_eq!(provider.name(), "mock-provider");
    }

    #[test]
    fn test_registry_create_unknown_provider() {
        let registry = ProviderRegistry::new();

        // Custom type not registered, should return mock
        let provider = registry.create_provider(
            &ProviderType::Custom,
            None,
            None,
            None,
            0.7,
        );

        // Should return mock for unknown types
        assert!(provider.name().contains("mock") || provider.name().contains("custom"));
    }

    #[test]
    fn test_registry_register_custom() {
        let registry = ProviderRegistry::new();
        let initial_count = registry.count();

        // Register a custom provider that overrides an existing type
        registry.register("openai", |_base_url, _api_key, model, _temp| {
            Box::new(MockProvider::new(model.unwrap_or("custom-openai")))
        });

        // Count should remain same (override, not add)
        assert_eq!(registry.count(), initial_count);

        // The custom factory should now be used for OpenAI type
        let provider = registry.create_provider(
            &ProviderType::OpenAI,
            None,
            None,
            Some("test-model"),
            0.7,
        );
        assert_eq!(provider.name(), "test-model");
    }

    #[test]
    fn test_registry_create_from_type() {
        let registry = ProviderRegistry::new();

        let provider = registry.create_from_type(ProviderType::OpenAI);
        assert_eq!(provider.name(), "openai");

        let provider = registry.create_from_type(ProviderType::Anthropic);
        assert_eq!(provider.name(), "anthropic");
    }

    #[test]
    fn test_registry_default() {
        let registry = ProviderRegistry::default();
        assert!(registry.is_registered("openai"));
    }

    #[test]
    fn test_global_registry() {
        let registry = global_registry();
        assert!(registry.is_registered("openai"));

        // Call again to test singleton behavior
        let registry2 = global_registry();
        assert!(registry2.is_registered("anthropic"));
    }

    #[test]
    fn test_all_provider_types_creatable() {
        let registry = ProviderRegistry::new();

        // Test that all ProviderType enum variants can create a provider
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
            // Should not panic
            let _provider = registry.create_from_type(pt);
        }
    }
}