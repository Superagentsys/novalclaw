use crate::config::{Config, ModelProviderConfig, ProviderConfig};
use crate::providers::{AnthropicProvider, GeminiProvider, MockProvider, OpenAiProvider, Provider};

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
    let provider_name = selection
        .provider
        .as_deref()
        .or(config.default_provider.as_deref())
        .unwrap_or("openai")
        .to_lowercase();

    let listed = find_listed_provider(config, &provider_name);
    let profile = config
        .model_providers
        .get(&provider_name)
        .or_else(|| listed.and_then(|item| config.model_providers.get(&item.id)));
    let kind = listed
        .map(|item| item.provider_type.to_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| provider_name.clone());

    let api_key = resolve_api_key(&provider_name, config, profile)
        .or_else(|| listed.and_then(|item| resolve_listed_api_key(item)));
    let model = selection.model.clone().unwrap_or_else(|| {
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
            .with_transport_mode(transport_mode),
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
            .with_transport_mode(transport_mode),
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
                .with_transport_mode(transport_mode),
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
            .with_transport_mode(transport_mode),
        ),
        _ => Box::new(MockProvider::new(format!("unknown-provider:{provider_name}"))),
    }
}

fn find_listed_provider<'a>(config: &'a Config, provider_name: &str) -> Option<&'a ProviderConfig> {
    config
        .providers
        .iter()
        .find(|item| item.id.eq_ignore_ascii_case(provider_name))
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

    #[test]
    #[ignore = "known bug: 纯大写无连字符的密钥会被当成环境变量名，resolve 失败"]
    fn uppercase_raw_secret_is_not_treated_as_env_var_name() {
        let secret = "ABCDEFGH12345678";
        assert_eq!(
            resolve_secret_or_env(secret),
            Some(secret.to_string()),
            "用户把密钥填进 api_key_env 时应直接使用，而不是去读同名环境变量"
        );
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
