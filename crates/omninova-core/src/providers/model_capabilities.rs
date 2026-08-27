//! Authoritative model capability registry.
//!
//! This is the single trusted table for:
//! - context window metadata
//! - max output metadata
//! - display token strategy
//!
//! Explicit provider profile configuration is highest priority. Built-in
//! entries are exact model IDs only; no substring/fuzzy matching is used.

use crate::config::ModelProviderConfig;

/// Which display token strategy applies for a resolved capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenStrategy {
    /// Use a provider-native token-count API.
    ProviderCountApi(ProviderCountApiKind),
    /// Use a trusted local tokenizer implementation by name.
    ExactLocalTokenizer(&'static str),
    /// No exact strategy is available; fall back to SafetyEstimate.
    Unavailable,
}

/// Supported provider-native count APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCountApiKind {
    /// Anthropic Messages API `POST /v1/messages/count_tokens`.
    AnthropicNative,
}

/// Resolved capabilities for one model/profile pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilities {
    /// Canonical model id when known; otherwise the requested model string.
    pub canonical_model: String,
    /// Provider family when explicitly known, e.g. "anthropic", "openai".
    pub provider_family: Option<String>,
    pub context_window_tokens: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub token_strategy: TokenStrategy,
}

/// A successful exact token measurement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenMeasurement {
    pub tokens: u64,
    pub source: &'static str,
    pub canonical_model: Option<String>,
    pub exact: bool,
}

/// Resolve capabilities with the authoritative order:
/// 1. explicit profile metadata
/// 2. trusted exact built-in registry entry
/// 3. Unknown
///
/// Exact DeepSeek V4 Flash local counting additionally requires the official
/// DeepSeek API endpoint unless the profile explicitly maps a tokenizer.
pub fn resolve_model_capabilities(
    model: &str,
    profile: Option<&ModelProviderConfig>,
) -> ModelCapabilities {
    resolve_model_capabilities_with_endpoint(
        model,
        profile,
        profile.and_then(|p| p.base_url.as_deref()),
    )
}

pub fn resolve_model_capabilities_with_endpoint(
    model: &str,
    profile: Option<&ModelProviderConfig>,
    endpoint: Option<&str>,
) -> ModelCapabilities {
    // Profile canonical mapping wins for canonical_model.
    let canonical = profile
        .and_then(|p| p.canonical_model.as_deref())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| model.to_string());

    let provider_family = profile
        .and_then(|p| p.provider_family.as_deref())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string);

    // Explicit profile context window / max output.
    let context_window_tokens = profile.and_then(|p| p.context_window_tokens);
    let max_output_tokens = profile.and_then(|p| p.max_output_tokens);

    // Explicit token strategy wins over built-in.
    if let Some(strategy) = profile.and_then(|p| p.exact_tokenizer.as_deref()) {
        let token_strategy = match strategy {
            "test_exact" => TokenStrategy::ExactLocalTokenizer("test_exact"),
            "anthropic_count_tokens" => TokenStrategy::ProviderCountApi(ProviderCountApiKind::AnthropicNative),
            "deepseek_v4_flash_0731" => TokenStrategy::ExactLocalTokenizer("deepseek_v4_flash_0731"),
            _ => TokenStrategy::Unavailable,
        };
        return ModelCapabilities {
            canonical_model: canonical,
            provider_family,
            context_window_tokens,
            max_output_tokens,
            token_strategy,
        };
    }

    // Built-in exact registry entry.
    if let Some(builtin) = builtin_capability(&canonical) {
        let mut token_strategy = builtin.token_strategy;
        let explicit_canonical_mapping = profile
            .and_then(|p| p.canonical_model.as_deref())
            .is_some_and(|value| !value.trim().is_empty());
        if matches!(
            token_strategy,
            TokenStrategy::ExactLocalTokenizer("deepseek_v4_flash_0731")
        ) && !is_official_deepseek_endpoint(endpoint)
            && !explicit_canonical_mapping
        {
            token_strategy = TokenStrategy::Unavailable;
        }
        return ModelCapabilities {
            canonical_model: canonical,
            provider_family: provider_family.or_else(|| builtin.provider_family.map(str::to_string)),
            context_window_tokens: context_window_tokens.or(builtin.context_window_tokens),
            max_output_tokens: max_output_tokens.or(builtin.max_output_tokens),
            token_strategy,
        };
    }

    ModelCapabilities {
        canonical_model: canonical,
        provider_family,
        context_window_tokens,
        max_output_tokens,
        token_strategy: TokenStrategy::Unavailable,
    }
}

/// Trusted exact built-in model entries. No fuzzy/prefix matching.
fn builtin_capability(model: &str) -> Option<BuiltinCapability> {
    match model {
        // Synthetic trusted model used by F2 tests.
        "omninova-test-exact" => Some(BuiltinCapability {
            provider_family: Some("test"),
            context_window_tokens: None,
            max_output_tokens: None,
            token_strategy: TokenStrategy::ExactLocalTokenizer("test_exact"),
        }),
        // Trusted canonical Claude models with native Anthropic count API.
        "claude-sonnet-4-5" => Some(BuiltinCapability {
            provider_family: Some("anthropic"),
            context_window_tokens: Some(200_000),
            max_output_tokens: Some(64_000),
            token_strategy: TokenStrategy::ProviderCountApi(ProviderCountApiKind::AnthropicNative),
        }),
        "claude-opus-4-5" => Some(BuiltinCapability {
            provider_family: Some("anthropic"),
            context_window_tokens: Some(200_000),
            max_output_tokens: Some(64_000),
            token_strategy: TokenStrategy::ProviderCountApi(ProviderCountApiKind::AnthropicNative),
        }),
        // Existing trusted GPT budget entries. Exact local tokenizer is not
        // implemented in V1.1A, so the strategy remains Unavailable.
        "gpt-4o" | "gpt-4o-mini" => Some(BuiltinCapability {
            provider_family: Some("openai"),
            context_window_tokens: Some(128_000),
            max_output_tokens: Some(16_384),
            token_strategy: TokenStrategy::Unavailable,
        }),
        "gpt-4-turbo" => Some(BuiltinCapability {
            provider_family: Some("openai"),
            context_window_tokens: Some(128_000),
            max_output_tokens: None,
            token_strategy: TokenStrategy::Unavailable,
        }),
        "deepseek-v4-flash" => Some(BuiltinCapability {
            provider_family: Some("deepseek"),
            context_window_tokens: Some(1_000_000),
            max_output_tokens: Some(384_000),
            token_strategy: TokenStrategy::ExactLocalTokenizer("deepseek_v4_flash_0731"),
        }),
        "deepseek-v4-pro" => Some(BuiltinCapability {
            provider_family: Some("deepseek"),
            context_window_tokens: Some(1_000_000),
            max_output_tokens: Some(384_000),
            token_strategy: TokenStrategy::Unavailable,
        }),
        _ => None,
    }
}

/// Official DeepSeek API host only. No substring/fuzzy matching.
pub fn is_official_deepseek_endpoint(endpoint: Option<&str>) -> bool {
    let Some(raw) = endpoint.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    let Ok(url) = url::Url::parse(raw) else {
        return false;
    };
    url.scheme() == "https" && url.host_str() == Some("api.deepseek.com")
}

struct BuiltinCapability {
    provider_family: Option<&'static str>,
    context_window_tokens: Option<u64>,
    max_output_tokens: Option<u64>,
    token_strategy: TokenStrategy,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_resolves_trusted_canonical_claude() {
        let caps = resolve_model_capabilities("claude-sonnet-4-5", None);
        assert_eq!(caps.canonical_model, "claude-sonnet-4-5");
        assert_eq!(caps.provider_family.as_deref(), Some("anthropic"));
        assert_eq!(caps.context_window_tokens, Some(200_000));
        assert!(matches!(
            caps.token_strategy,
            TokenStrategy::ProviderCountApi(ProviderCountApiKind::AnthropicNative)
        ));
    }

    #[test]
    fn unknown_claude_looking_alias_stays_unknown() {
        for alias in [
            "claude-custom",
            "claude-opus-proxy",
            "deepseek-v4-flash-custom",
            "deepseek-v4-flash-proxy",
            "my-deepseek",
            "deepseek-latest",
            "gpt-foo",
        ] {
            let caps = resolve_model_capabilities(alias, None);
            assert_eq!(caps.context_window_tokens, None);
            assert_eq!(caps.token_strategy, TokenStrategy::Unavailable);
        }
    }

    #[test]
    fn a_official_flash_resolves_million_context_and_384k_output() {
        let caps = resolve_model_capabilities("deepseek-v4-flash", None);
        assert_eq!(caps.canonical_model, "deepseek-v4-flash");
        assert_eq!(caps.provider_family.as_deref(), Some("deepseek"));
        assert_eq!(caps.context_window_tokens, Some(1_000_000));
        assert_eq!(caps.max_output_tokens, Some(384_000));
    }

    #[test]
    fn b_pro_resolves_budget_but_not_flash_tokenizer() {
        let caps = resolve_model_capabilities_with_endpoint(
            "deepseek-v4-pro",
            None,
            Some("https://api.deepseek.com/v1"),
        );
        assert_eq!(caps.context_window_tokens, Some(1_000_000));
        assert_eq!(caps.max_output_tokens, Some(384_000));
        assert_eq!(caps.token_strategy, TokenStrategy::Unavailable);
    }

    #[test]
    fn c_official_endpoint_enables_flash_exact_strategy() {
        let caps = resolve_model_capabilities_with_endpoint(
            "deepseek-v4-flash",
            None,
            Some("https://api.deepseek.com/v1"),
        );
        assert!(matches!(
            caps.token_strategy,
            TokenStrategy::ExactLocalTokenizer("deepseek_v4_flash_0731")
        ));
        assert!(is_official_deepseek_endpoint(Some("https://api.deepseek.com")));
        assert!(is_official_deepseek_endpoint(Some("https://api.deepseek.com/v1")));
    }

    #[test]
    fn d_third_party_endpoint_does_not_auto_enable_exact() {
        let caps = resolve_model_capabilities_with_endpoint(
            "deepseek-v4-flash",
            None,
            Some("https://wetoken.pro/v1"),
        );
        assert_eq!(caps.context_window_tokens, Some(1_000_000));
        assert_eq!(caps.token_strategy, TokenStrategy::Unavailable);
        assert!(!is_official_deepseek_endpoint(Some(
            "https://api.deepseek.com.evil.example/v1"
        )));
        assert!(!is_official_deepseek_endpoint(Some(
            "https://api.deepseek.com.proxy/v1"
        )));
    }

    #[test]
    fn e_explicit_canonical_mapping_enables_exact_strategy() {
        let mut profile = ModelProviderConfig::default();
        profile.canonical_model = Some("deepseek-v4-flash".into());
        profile.exact_tokenizer = Some("deepseek_v4_flash_0731".into());
        profile.base_url = Some("https://third-party.example/v1".into());
        let caps = resolve_model_capabilities("my-deepseek", Some(&profile));
        assert_eq!(caps.canonical_model, "deepseek-v4-flash");
        assert!(matches!(
            caps.token_strategy,
            TokenStrategy::ExactLocalTokenizer("deepseek_v4_flash_0731")
        ));
    }

    #[test]
    fn e_canonical_mapping_without_tokenizer_name_enables_exact() {
        let mut profile = ModelProviderConfig::default();
        profile.canonical_model = Some("deepseek-v4-flash".into());
        profile.base_url = Some("https://third-party.example/v1".into());
        let caps = resolve_model_capabilities("my-deepseek", Some(&profile));
        assert_eq!(caps.canonical_model, "deepseek-v4-flash");
        assert_eq!(caps.context_window_tokens, Some(1_000_000));
        assert!(matches!(
            caps.token_strategy,
            TokenStrategy::ExactLocalTokenizer("deepseek_v4_flash_0731")
        ));
    }

    #[test]
    fn explicit_profile_mapping_resolves_trusted_capability() {
        let mut profile = ModelProviderConfig::default();
        profile.canonical_model = Some("claude-sonnet-4-5".into());
        profile.exact_tokenizer = Some("anthropic_count_tokens".into());
        let caps = resolve_model_capabilities("my-alias", Some(&profile));
        assert_eq!(caps.canonical_model, "claude-sonnet-4-5");
        assert!(matches!(
            caps.token_strategy,
            TokenStrategy::ProviderCountApi(ProviderCountApiKind::AnthropicNative)
        ));
    }

    #[test]
    fn explicit_profile_window_overrides_builtin() {
        let mut profile = ModelProviderConfig::default();
        profile.context_window_tokens = Some(999_000);
        let caps = resolve_model_capabilities("gpt-4o", Some(&profile));
        assert_eq!(caps.context_window_tokens, Some(999_000));
    }
}
