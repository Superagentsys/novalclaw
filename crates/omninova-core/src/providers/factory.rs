use crate::config::{Config, ModelProviderConfig};
use crate::providers::{AnthropicProvider, GeminiProvider, MockProvider, OpenAiProvider, Provider};

#[derive(Debug, Clone, Default)]
pub struct ProviderSelection {
    pub provider: Option<String>,
    pub model: Option<String>,
}

pub fn build_provider_from_config(config: &Config) -> Box<dyn Provider> {
    build_provider_with_selection(config, &ProviderSelection::default())
}

pub fn build_provider_with_selection(
    config: &Config,
    selection: &ProviderSelection,
) -> Box<dyn Provider> {
    let provider_name = selection
        .provider
        .as_deref()
        .or(config.default_provider.as_deref())
        .unwrap_or("openai")
        .to_lowercase();

    let profile = config.model_providers.get(&provider_name);
    let api_key = resolve_api_key(&provider_name, config, profile);
    let model = selection
        .model
        .clone()
        .unwrap_or_else(|| resolve_model(config, profile));
    let base_url = resolve_base_url(&provider_name, config, profile);
    let temp = config.default_temperature;

    match provider_name.as_str() {
        "anthropic" => Box::new(AnthropicProvider::new(
            base_url.as_deref(),
            api_key.as_deref(),
            model,
            temp,
            None,
        )),
        "gemini" => Box::new(GeminiProvider::new(
            base_url.as_deref(),
            api_key.as_deref(),
            model,
            temp,
            None,
        )),
        "mock" => Box::new(MockProvider::new("mock-provider")),
        "openai" | "openrouter" | "ollama" => Box::new(OpenAiProvider::new(
            base_url.as_deref(),
            api_key.as_deref(),
            model,
            temp,
            None,
        )),
        _ => Box::new(MockProvider::new(format!("unknown-provider:{provider_name}"))),
    }
}

fn resolve_model(config: &Config, profile: Option<&ModelProviderConfig>) -> String {
    profile
        .and_then(|p| p.default_model.clone())
        .or_else(|| config.default_model.clone())
        .unwrap_or_else(|| "gpt-4o-mini".to_string())
}

fn resolve_base_url(
    provider_name: &str,
    config: &Config,
    profile: Option<&ModelProviderConfig>,
) -> Option<String> {
    if let Some(url) = profile.and_then(|p| p.base_url.clone()) {
        return Some(url);
    }
    if let Some(url) = config.api_url.clone() {
        return Some(url);
    }
    match provider_name {
        "openrouter" => Some("https://openrouter.ai/api/v1".to_string()),
        "ollama" => Some("http://localhost:11434/v1".to_string()),
        "anthropic" => std::env::var("ANTHROPIC_BASE_URL").ok(),
        "gemini" => std::env::var("GEMINI_BASE_URL").ok(),
        _ => None,
    }
}

fn resolve_api_key(
    provider_name: &str,
    config: &Config,
    profile: Option<&ModelProviderConfig>,
) -> Option<String> {
    if let Some(k) = profile.and_then(|p| p.api_key.clone()) {
        return Some(k);
    }
    if let Some(env_key_name) = profile.and_then(|p| p.api_key_env.clone()) {
        if let Ok(v) = std::env::var(env_key_name) {
            if !v.trim().is_empty() {
                return Some(v);
            }
        }
    }
    if let Some(k) = config.api_key.clone() {
        return Some(k);
    }

    let env_var_name = match provider_name {
        "anthropic" => "ANTHROPIC_API_KEY",
        "gemini" => "GEMINI_API_KEY",
        "openrouter" => "OPENROUTER_API_KEY",
        "ollama" => "OLLAMA_API_KEY",
        _ => "OPENAI_API_KEY",
    };
    std::env::var(env_var_name)
        .ok()
        .filter(|v| !v.trim().is_empty())
}
