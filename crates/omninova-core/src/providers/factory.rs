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
        .unwrap_or_else(|| resolve_model(&provider_name, config, profile));
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
        "openai"
        | "openrouter"
        | "ollama"
        | "deepseek"
        | "qwen"
        | "moonshot"
        | "groq"
        | "xai"
        | "mistral"
        | "lmstudio"
        | "together"
        | "fireworks"
        | "novita"
        | "perplexity"
        | "cohere"
        | "doubao"
        | "qianfan"
        | "glm"
        | "minimax"
        | "nvidia"
        | "cloudflare"
        | "sglang"
        | "vllm"
        | "llamacpp" => Box::new(OpenAiProvider::new(
            base_url.as_deref(),
            api_key.as_deref(),
            model,
            temp,
            None,
        )),
        _ if provider_name.starts_with("custom:") => {
            let custom_url = provider_name.strip_prefix("custom:").unwrap_or_default();
            Box::new(OpenAiProvider::new(
                Some(custom_url),
                api_key.as_deref(),
                model,
                temp,
                None,
            ))
        }
        _ => Box::new(MockProvider::new(format!("unknown-provider:{provider_name}"))),
    }
}

fn resolve_model(
    provider_name: &str,
    config: &Config,
    profile: Option<&ModelProviderConfig>,
) -> String {
    if let Some(m) = profile.and_then(|p| p.default_model.clone()) {
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
