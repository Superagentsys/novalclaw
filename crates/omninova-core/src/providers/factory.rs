use crate::config::{Config, ModelProviderConfig, ProviderConfig};
use crate::providers::context_budget::resolve_context_budget;
use crate::providers::{
    AnthropicProvider, ChatRequest, ChatResponse, GeminiProvider, MockProvider, OpenAiProvider,
    Provider,
};
use async_trait::async_trait;

#[derive(Debug, Clone, Default)]
pub struct ProviderSelection {
    pub provider: Option<String>,
    pub model: Option<String>,
}

const OPENAI_COMPATIBLE: &[&str] = &[
    "openai",
    "openrouter",
    "ollama",
    "deepseek",
    "qwen",
    "moonshot",
    "groq",
    "xai",
    "mistral",
    "lmstudio",
    "together",
    "fireworks",
    "novita",
    "perplexity",
    "cohere",
    "doubao",
    "qianfan",
    "glm",
    "minimax",
    "nvidia",
    "cloudflare",
    "sglang",
    "vllm",
    "llamacpp",
    "custom",
];

pub fn build_provider_from_config(config: &Config) -> Box<dyn Provider> {
    build_provider_with_selection(config, &ProviderSelection::default())
}

pub fn build_provider_with_selection(
    config: &Config,
    selection: &ProviderSelection,
) -> Box<dyn Provider> {
    let (provider_name, selected_model) = match resolve_effective_selection(config, selection) {
        Ok(selection) => selection,
        Err(reason) => {
            tracing::warn!(
                target: "omninova_core::providers::selection",
                provider = %selection
                    .provider
                    .as_deref()
                    .or(config.default_provider.as_deref())
                    .unwrap_or("openai"),
                reason = %reason,
                "[provider-selection-blocked]"
            );
            return Box::new(UnavailableProvider { reason });
        }
    };

    let listed = find_listed_provider(config, &provider_name);
    let profile = find_model_provider(config, &provider_name)
        .or_else(|| listed.and_then(|item| find_model_provider(config, &item.id)));
    let kind = listed
        .map(|item| item.provider_type.to_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| provider_name.clone());

    let api_key = resolve_api_key(&provider_name, config, profile)
        .or_else(|| listed.and_then(|item| resolve_listed_api_key(item)));
    let model = selected_model.unwrap_or_else(|| {
        resolve_model(
            &kind,
            config,
            profile,
            listed.and_then(|item| item.models.first().cloned()),
        )
    });
    let base_url = profile
        .and_then(|item| item.base_url.clone())
        .or_else(|| listed.and_then(|item| item.base_url.clone()))
        .map(|url| normalize_openai_base_url(&kind, url))
        .or_else(|| resolve_known_base_url(&provider_name))
        .or_else(|| resolve_known_base_url(&kind))
        .or_else(|| {
            if listed.is_none() {
                config
                    .api_url
                    .clone()
                    .map(|url| normalize_openai_base_url(&kind, url))
            } else {
                None
            }
        });
    let temp = config.default_temperature;
    let timeouts = crate::providers::ProviderTimeouts::from_config(&config.provider_runtime);
    let transport_mode = profile.map(|p| p.transport.mode).unwrap_or_default();
    let context_budget = resolve_context_budget(config, &model, profile);

    let dispatch = resolve_dispatch_kind(&provider_name, &kind, base_url.is_some());
    match dispatch.as_str() {
        "anthropic" => Box::new(
            AnthropicProvider::new(
                base_url.as_deref(),
                api_key.as_deref(),
                model,
                temp,
                None,
                timeouts,
            )
            .with_transport_mode(transport_mode)
            .with_context_budget(context_budget),
        ),
        "gemini" => Box::new(
            GeminiProvider::new(
                base_url.as_deref(),
                api_key.as_deref(),
                model,
                temp,
                None,
                timeouts,
            )
            .with_transport_mode(transport_mode)
            .with_context_budget(context_budget),
        ),
        "mock" => Box::new(MockProvider::new("mock-provider")),
        "openai-compat" => {
            let custom_url = provider_name
                .strip_prefix("custom:")
                .map(str::to_string)
                .or(base_url);
            Box::new(
                OpenAiProvider::new(
                    custom_url.as_deref(),
                    api_key.as_deref(),
                    model,
                    temp,
                    None,
                    timeouts,
                )
                .with_transport_mode(transport_mode)
                .with_context_budget(context_budget),
            )
        }
        _ if OPENAI_COMPATIBLE.contains(&dispatch.as_str()) => Box::new(
            OpenAiProvider::new(
                base_url.as_deref(),
                api_key.as_deref(),
                model,
                temp,
                None,
                timeouts,
            )
            .with_transport_mode(transport_mode)
            .with_context_budget(context_budget),
        ),
        _ => Box::new(MockProvider::new(format!("unknown-provider:{provider_name}"))),
    }
}

fn resolve_effective_selection(
    config: &Config,
    selection: &ProviderSelection,
) -> Result<(String, Option<String>), String> {
    // `Some` is treated as requested intent (Agent, Automation, UI route, or
    // request metadata). `None` is the implicit/default path.
    let provider_name = selection
        .provider
        .as_deref()
        .or(config.default_provider.as_deref())
        .unwrap_or("openai")
        .trim()
        .to_lowercase();
    let selected_model = selection
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    if configured_provider_enabled(config, &provider_name) == Some(false) {
        return Err(format!(
            "provider '{provider_name}' is disabled and cannot be dispatched"
        ));
    }

    if let Some(model) = selected_model.as_deref() {
        if !configured_model_is_selectable(config, &provider_name, model) {
            return Err(format!(
                "model '{model}' is not available on provider '{provider_name}'"
            ));
        }
    }

    Ok((provider_name, selected_model))
}

/// `model_providers` is the normalized runtime authority. The legacy
/// list-style entry is consulted only when no named profile exists.
fn configured_provider_enabled(config: &Config, provider_name: &str) -> Option<bool> {
    find_model_provider(config, provider_name)
        .map(|provider| provider.enabled)
        .or_else(|| find_listed_provider(config, provider_name).map(|provider| provider.enabled))
}

fn configured_model_is_selectable(config: &Config, provider_name: &str, model: &str) -> bool {
    let models = if let Some(profile) = find_model_provider(config, provider_name) {
        &profile.models
    } else if let Some(listed) = find_listed_provider(config, provider_name) {
        &listed.models
    } else {
        // Built-ins and OpenAI-compatible providers without an explicit model
        // catalog keep their historical free-form model behavior.
        return true;
    };
    models.is_empty() || models.iter().any(|candidate| candidate == model)
}

fn find_model_provider<'a>(
    config: &'a Config,
    provider_name: &str,
) -> Option<&'a ModelProviderConfig> {
    config.model_providers.get(provider_name).or_else(|| {
        config
            .model_providers
            .iter()
            .find(|(id, _)| id.eq_ignore_ascii_case(provider_name))
            .map(|(_, provider)| provider)
    })
}

fn find_listed_provider<'a>(config: &'a Config, provider_name: &str) -> Option<&'a ProviderConfig> {
    config
        .providers
        .iter()
        .find(|item| item.id.eq_ignore_ascii_case(provider_name))
}

struct UnavailableProvider {
    reason: String,
}

#[async_trait]
impl Provider for UnavailableProvider {
    fn name(&self) -> &str {
        "unavailable"
    }

    async fn chat(&self, _request: ChatRequest<'_>) -> anyhow::Result<ChatResponse> {
        anyhow::bail!("Provider selection blocked: {}", self.reason)
    }

    async fn health_check(&self) -> bool {
        false
    }
}

fn resolve_dispatch_kind(provider_name: &str, kind: &str, has_base_url: bool) -> String {
    if provider_name.starts_with("custom:") || kind == "custom" {
        return "openai-compat".into();
    }
    if kind == "anthropic" || provider_name == "anthropic" {
        return "anthropic".into();
    }
    if kind == "gemini" || provider_name == "gemini" {
        return "gemini".into();
    }
    if kind == "mock" || provider_name == "mock" {
        return "mock".into();
    }
    if OPENAI_COMPATIBLE.contains(&kind) {
        return kind.to_string();
    }
    if OPENAI_COMPATIBLE.contains(&provider_name) {
        return provider_name.to_string();
    }
    if has_base_url {
        return "openai-compat".into();
    }
    provider_name.to_string()
}

fn resolve_model(
    provider_name: &str,
    config: &Config,
    profile: Option<&ModelProviderConfig>,
    listed_model: Option<String>,
) -> String {
    if let Some(m) = profile.and_then(|p| p.default_model.clone()) {
        return m;
    }
    if let Some(m) = listed_model {
        return m;
    }
    if let Some(m) = config.default_model.clone() {
        return m;
    }
    match provider_name {
        "deepseek" => "deepseek-chat".to_string(),
        "qwen" => "qwen-max".to_string(),
        "moonshot" => "moonshot-v1-8k".to_string(),
        "groq" => "llama-3.3-70b-versatile".to_string(),
        "xai" => "grok-2-latest".to_string(),
        "mistral" => "mistral-small-latest".to_string(),
        "ollama" => "llama3.2".to_string(),
        "lmstudio" => "local-model".to_string(),
        "openrouter" => "anthropic/claude-3.5-sonnet".to_string(),
        "anthropic" => "claude-3-5-sonnet-latest".to_string(),
        "gemini" => "gemini-2.0-flash".to_string(),
        "together" => "meta-llama/Llama-3.3-70B-Instruct-Turbo".to_string(),
        "fireworks" => "accounts/fireworks/models/llama-v3p1-70b-instruct".to_string(),
        "perplexity" => "llama-3.1-sonar-large-128k-online".to_string(),
        "cohere" => "command-r-plus".to_string(),
        "doubao" => "doubao-seed-2-0-pro-260215".to_string(),
        "qianfan" => "ernie-4.0-8k".to_string(),
        "glm" => "glm-4".to_string(),
        "minimax" => "abab6.5s-chat".to_string(),
        "nvidia" => "meta/llama-3.1-70b-instruct".to_string(),
        "cloudflare" => "@cf/meta/llama-3.1-70b-instruct".to_string(),
        "novita" => "meta-llama/llama-3.1-70b-instruct".to_string(),
        _ => "gpt-4o-mini".to_string(),
    }
}

/// 归一化 OpenAI 兼容服务的 base_url。
///
/// `OpenAiProvider` 会拼接 `{base_url}/chat/completions`，因此 base_url 必须已包含
/// `/v1`。Ollama 的原生端口 `http://localhost:11434` 常被直接填入，这里对本地
/// OpenAI 兼容 provider 自动补上 `/v1`，避免请求打到错误路径（404）。
fn normalize_openai_base_url(provider_name: &str, url: String) -> String {
    let trimmed = url.trim().trim_end_matches('/').to_string();
    let needs_v1 = matches!(provider_name, "ollama" | "lmstudio")
        && !trimmed.ends_with("/v1")
        && !trimmed.contains("/v1/");
    if needs_v1 {
        format!("{trimmed}/v1")
    } else {
        trimmed
    }
}

fn resolve_known_base_url(provider_name: &str) -> Option<String> {
    match provider_name {
        "openrouter" => Some("https://openrouter.ai/api/v1".to_string()),
        "ollama" => Some("http://localhost:11434/v1".to_string()),
        "deepseek" => Some("https://api.deepseek.com/v1".to_string()),
        "qwen" => Some("https://dashscope.aliyuncs.com/compatible-mode/v1".to_string()),
        "moonshot" => Some("https://api.moonshot.cn/v1".to_string()),
        "groq" => Some("https://api.groq.com/openai/v1".to_string()),
        "xai" => Some("https://api.x.ai/v1".to_string()),
        "mistral" => Some("https://api.mistral.ai/v1".to_string()),
        "lmstudio" => Some("http://localhost:1234/v1".to_string()),
        "together" => Some("https://api.together.xyz/v1".to_string()),
        "fireworks" => Some("https://api.fireworks.ai/inference/v1".to_string()),
        "novita" => Some("https://api.novita.ai/v3/openai".to_string()),
        "perplexity" => Some("https://api.perplexity.ai".to_string()),
        "cohere" => Some("https://api.cohere.ai/v1".to_string()),
        "doubao" => Some("https://ark.cn-beijing.volces.com/api/v3".to_string()),
        "qianfan" => Some("https://aip.baidubce.com/rpc/2.0/ai_custom/v1/wenxinworkshop".to_string()),
        "glm" => Some("https://open.bigmodel.cn/api/paas/v4".to_string()),
        "minimax" => Some("https://api.minimax.chat/v1".to_string()),
        "nvidia" => Some("https://integrate.api.nvidia.com/v1".to_string()),
        "cloudflare" => Some("https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/v1".to_string()),
        "sglang" => Some("http://localhost:30000/v1".to_string()),
        "vllm" => Some("http://localhost:8000/v1".to_string()),
        "llamacpp" => Some("http://localhost:8080/v1".to_string()),
        "anthropic" => std::env::var("ANTHROPIC_BASE_URL").ok(),
        "gemini" => std::env::var("GEMINI_BASE_URL").ok(),
        _ => None,
    }
}

fn resolve_listed_api_key(provider: &ProviderConfig) -> Option<String> {
    provider
        .api_key_env
        .as_deref()
        .and_then(resolve_secret_or_env)
}

fn resolve_secret_or_env(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(from_env) = std::env::var(trimmed) {
        if !from_env.trim().is_empty() {
            return Some(from_env);
        }
    }
    if looks_like_raw_secret(trimmed) {
        return Some(trimmed.to_string());
    }
    None
}

fn looks_like_raw_secret(value: &str) -> bool {
    value.len() >= 8
        && (value.contains('-')
            || value.chars().any(|ch| ch.is_ascii_lowercase())
            || !value
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured_provider(id: &str, model: &str, enabled: bool) -> (String, ModelProviderConfig) {
        (
            id.to_string(),
            ModelProviderConfig {
                base_url: Some("https://provider.example/v1".into()),
                default_model: Some(model.into()),
                models: vec![model.into()],
                enabled,
                ..ModelProviderConfig::default()
            },
        )
    }

    #[test]
    fn custom_listed_provider_uses_openai_compatible_client() {
        let mut config = Config::default();
        config.providers.push(ProviderConfig {
            id: "custom-workbuddy".into(),
            name: "WorkBuddy".into(),
            provider_type: "openai".into(),
            api_key_env: Some("sk-test-key".into()),
            base_url: Some("https://api.example.com/v1".into()),
            models: vec!["glm-5.1".into()],
            enabled: true,
        });
        config.model_providers.insert(
            "custom-workbuddy".into(),
            ModelProviderConfig {
                api_key_env: Some("sk-test-key".into()),
                base_url: Some("https://api.example.com/v1".into()),
                default_model: Some("glm-5.1".into()),
                models: vec!["glm-5.1".into()],
                enabled: true,
                ..ModelProviderConfig::default()
            },
        );

        let provider = build_provider_with_selection(
            &config,
            &ProviderSelection {
                provider: Some("custom-workbuddy".into()),
                model: Some("glm-5.1".into()),
            },
        );
        assert_eq!(provider.name(), "openai");
    }

    async fn assert_local_unavailable(provider: Box<dyn Provider>, expected_fragment: &str) {
        assert_eq!(provider.name(), "unavailable");
        assert_eq!(provider.model(), None);
        let messages = vec![crate::providers::ChatMessage::user("hello")];
        let error = provider
            .chat(crate::providers::ChatRequest {
                messages: &messages,
                tools: None,
            })
            .await
            .expect_err("blocked selection must fail locally without HTTP");
        assert!(
            error.to_string().contains(expected_fragment),
            "unexpected local error: {error}"
        );
    }

    #[tokio::test]
    async fn explicit_disabled_provider_is_blocked_without_substitution() {
        let mut config = Config::default();
        config.default_provider = Some("enabled-default".into());
        config.default_model = Some("current-model".into());
        config.model_providers.extend([
            configured_provider("enabled-default", "current-model", true),
            configured_provider("stale-provider", "stale-model", false),
        ]);

        let provider = build_provider_with_selection(
            &config,
            &ProviderSelection {
                provider: Some("stale-provider".into()),
                model: Some("stale-model".into()),
            },
        );
        assert_local_unavailable(provider, "is disabled").await;
    }

    #[tokio::test]
    async fn disabled_default_provider_is_blocked_before_outbound_request() {
        let mut config = Config::default();
        config.default_provider = Some("disabled-default".into());
        config.default_model = Some("disabled-model".into());
        config.model_providers.extend([
            configured_provider("disabled-default", "disabled-model", false),
            configured_provider("other-enabled", "other-model", true),
        ]);

        let provider = build_provider_from_config(&config);
        assert_local_unavailable(provider, "is disabled").await;
    }

    #[test]
    fn implicit_default_uses_enabled_provider_model() {
        let mut config = Config::default();
        config.default_provider = Some("enabled-default".into());
        let (id, profile) = configured_provider("enabled-default", "current-model", true);
        config.model_providers.insert(id, profile);

        let provider = build_provider_from_config(&config);
        assert_eq!(provider.name(), "openai");
        assert_eq!(provider.model(), Some("current-model"));
    }

    #[tokio::test]
    async fn explicit_unavailable_model_is_blocked_without_substitution() {
        let mut config = Config::default();
        config.default_provider = Some("custom-current".into());
        config.default_model = Some("current-model".into());
        let (id, profile) = configured_provider("custom-current", "current-model", true);
        config.model_providers.insert(id, profile);

        let provider = build_provider_with_selection(
            &config,
            &ProviderSelection {
                provider: Some("custom-current".into()),
                model: Some("removed-model".into()),
            },
        );
        assert_local_unavailable(provider, "is not available").await;
    }

    #[test]
    fn implicit_model_uses_provider_default() {
        let mut config = Config::default();
        config.default_provider = Some("custom-current".into());
        let (id, profile) = configured_provider("custom-current", "current-model", true);
        config.model_providers.insert(id, profile);

        let provider = build_provider_with_selection(
            &config,
            &ProviderSelection {
                provider: Some("custom-current".into()),
                model: None,
            },
        );
        assert_eq!(provider.name(), "openai");
        assert_eq!(provider.model(), Some("current-model"));
    }

    #[test]
    fn named_profile_is_authoritative_over_enabled_legacy_duplicate() {
        let mut config = Config::default();
        config.default_provider = Some("duplicate".into());
        config.default_model = Some("legacy-model".into());
        let (id, profile) = configured_provider("duplicate", "modern-model", false);
        config.model_providers.insert(id, profile);
        config.providers.push(ProviderConfig {
            id: "duplicate".into(),
            name: "Duplicate".into(),
            provider_type: "openai".into(),
            api_key_env: None,
            base_url: Some("https://legacy.example/v1".into()),
            models: vec!["legacy-model".into()],
            enabled: true,
        });

        assert_eq!(
            configured_provider_enabled(&config, "duplicate"),
            Some(false)
        );
        assert_eq!(build_provider_from_config(&config).name(), "unavailable");
    }

    #[test]
    fn api_key_env_name_resolves_environment_value_without_logging_it() {
        let _guard = crate::config::env::test_env_lock().lock().unwrap();
        const NAME: &str = "OMNINOVA_FACTORY_TEST_KEY";
        const VALUE: &str = "test-secret-value";
        std::env::set_var(NAME, VALUE);
        let resolved = resolve_secret_or_env(NAME);
        std::env::remove_var(NAME);
        assert_eq!(resolved.as_deref(), Some(VALUE));
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
        if let Some(value) = resolve_secret_or_env(&env_key_name) {
            return Some(value);
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
        "deepseek" => "DEEPSEEK_API_KEY",
        "qwen" => "DASHSCOPE_API_KEY",
        "moonshot" => "MOONSHOT_API_KEY",
        "groq" => "GROQ_API_KEY",
        "xai" => "XAI_API_KEY",
        "mistral" => "MISTRAL_API_KEY",
        "lmstudio" => "LMSTUDIO_API_KEY",
        "together" => "TOGETHER_API_KEY",
        "fireworks" => "FIREWORKS_API_KEY",
        "novita" => "NOVITA_API_KEY",
        "perplexity" => "PERPLEXITY_API_KEY",
        "cohere" => "COHERE_API_KEY",
        "doubao" => "DOUBAO_API_KEY",
        "qianfan" => "QIANFAN_API_KEY",
        "glm" => "GLM_API_KEY",
        "minimax" => "MINIMAX_API_KEY",
        "nvidia" => "NVIDIA_API_KEY",
        "cloudflare" => "CLOUDFLARE_API_KEY",
        _ => "OPENAI_API_KEY",
    };
    std::env::var(env_var_name)
        .ok()
        .filter(|v| !v.trim().is_empty())
}
