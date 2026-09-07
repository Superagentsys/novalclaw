//! Deterministic request generation-limit policy.
//!
//! Four concepts stay separate:
//! - `model_max_output_tokens`: model capability
//! - configured `request_max_output_tokens`: explicit profile override
//! - product default: OmniNova policy when profile is unset
//! - request-scoped override: ephemeral per logical request/run
//!
//! This is request policy, not model metadata. It does not classify tasks
//! or inspect prompts.

use crate::config::ModelProviderConfig;

/// OmniNova product default for one request's maximum output tokens.
///
/// Used only when neither a request override nor a profile override is set.
/// Clamped to the model maximum when that capability is known.
pub const PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS: u64 = 32_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GenerationLimitSource {
    RequestOverride,
    ProfileOverride,
    ProductDefault,
    #[default]
    ModelMaximumFallback,
}

impl GenerationLimitSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RequestOverride => "request_override",
            Self::ProfileOverride => "profile_override",
            Self::ProductDefault => "product_default",
            Self::ModelMaximumFallback => "model_maximum_fallback",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedGenerationLimit {
    /// Native request cap. `None` means omit `max_tokens`.
    pub effective_tokens: Option<u64>,
    pub source: GenerationLimitSource,
}

impl ResolvedGenerationLimit {
    pub fn native_max_tokens(self) -> Option<u32> {
        let tokens = self.effective_tokens.filter(|value| *value > 0)?;
        u32::try_from(tokens.min(u64::from(u32::MAX)))
            .ok()
            .filter(|value| *value > 0)
    }
}

/// Typed-boundary sanitizer for an external request override.
///
/// `None` and `0` are absent. Values above `u32::MAX` are rejected.
pub fn sanitize_request_generation_override(
    tokens: Option<u64>,
) -> Result<Option<u32>, &'static str> {
    match tokens {
        None | Some(0) => Ok(None),
        Some(value) if value > u64::from(u32::MAX) => {
            Err("request_max_output_tokens exceeds u32")
        }
        Some(value) => Ok(Some(value as u32)),
    }
}

/// Resolves the effective request generation limit from profile config.
/// Session-open and factory construction call this with no request override.
pub fn resolve_generation_limit(
    profile: Option<&ModelProviderConfig>,
    model_max_output_tokens: Option<u64>,
) -> ResolvedGenerationLimit {
    resolve_effective_request_generation_limit(
        None,
        profile.and_then(|item| item.request_max_output_tokens),
        Some(PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS),
        model_max_output_tokens,
    )
}

/// Core authority:
/// request override → profile override → product default → model-max fallback.
pub fn resolve_effective_request_generation_limit(
    request_override: Option<u64>,
    profile_override: Option<u64>,
    product_default: Option<u64>,
    model_max_output_tokens: Option<u64>,
) -> ResolvedGenerationLimit {
    let model_max = model_max_output_tokens.filter(|value| *value > 0);
    let request = request_override.filter(|value| *value > 0);
    let profile = profile_override.filter(|value| *value > 0);
    let product = product_default.filter(|value| *value > 0);

    let (raw, source) = if let Some(value) = request {
        (value, GenerationLimitSource::RequestOverride)
    } else if let Some(value) = profile {
        (value, GenerationLimitSource::ProfileOverride)
    } else if let Some(value) = product {
        (value, GenerationLimitSource::ProductDefault)
    } else {
        return ResolvedGenerationLimit {
            effective_tokens: None,
            source: GenerationLimitSource::ModelMaximumFallback,
        };
    };

    let effective = match model_max {
        Some(max) => raw.min(max),
        None => raw,
    };
    if effective == 0 {
        return ResolvedGenerationLimit {
            effective_tokens: None,
            source: GenerationLimitSource::ModelMaximumFallback,
        };
    }
    ResolvedGenerationLimit {
        effective_tokens: Some(effective),
        source,
    }
}

/// Core authority without a request override:
/// profile override → product default → model-max fallback (native uncapped).
pub fn resolve_generation_limit_with_product_default(
    profile_override: Option<u64>,
    model_max_output_tokens: Option<u64>,
    product_default: Option<u64>,
) -> ResolvedGenerationLimit {
    resolve_effective_request_generation_limit(
        None,
        profile_override,
        product_default,
        model_max_output_tokens,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModelProviderConfig;

    #[test]
    fn r24_a_explicit_profile_64k_wins() {
        let profile = ModelProviderConfig {
            request_max_output_tokens: Some(64_000),
            ..ModelProviderConfig::default()
        };
        let resolved = resolve_generation_limit(Some(&profile), Some(384_000));
        assert_eq!(resolved.effective_tokens, Some(64_000));
        assert_eq!(resolved.source, GenerationLimitSource::ProfileOverride);
    }

    #[test]
    fn r24_b_no_profile_override_uses_product_default() {
        let resolved = resolve_generation_limit(None, Some(384_000));
        assert_eq!(
            resolved.effective_tokens,
            Some(PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS)
        );
        assert_eq!(resolved.source, GenerationLimitSource::ProductDefault);
        assert_eq!(resolved.native_max_tokens(), Some(32_000));
    }

    #[test]
    fn r24_c_product_default_clamps_to_model_max() {
        let resolved = resolve_generation_limit(None, Some(16_384));
        assert_eq!(resolved.effective_tokens, Some(16_384));
        assert_eq!(resolved.source, GenerationLimitSource::ProductDefault);
    }

    #[test]
    fn r24_d_explicit_value_above_product_default_wins() {
        let profile = ModelProviderConfig {
            request_max_output_tokens: Some(64_000),
            ..ModelProviderConfig::default()
        };
        let resolved = resolve_generation_limit(Some(&profile), Some(384_000));
        assert!(resolved.effective_tokens.unwrap() > PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS);
        assert_eq!(resolved.source, GenerationLimitSource::ProfileOverride);
    }

    #[test]
    fn r24_e_clearing_explicit_returns_to_product_default() {
        let configured = ModelProviderConfig {
            request_max_output_tokens: Some(64_000),
            ..ModelProviderConfig::default()
        };
        assert_eq!(
            resolve_generation_limit(Some(&configured), Some(384_000)).source,
            GenerationLimitSource::ProfileOverride
        );
        let cleared = ModelProviderConfig {
            request_max_output_tokens: None,
            ..ModelProviderConfig::default()
        };
        let resolved = resolve_generation_limit(Some(&cleared), Some(384_000));
        assert_eq!(
            resolved.effective_tokens,
            Some(PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS)
        );
        assert_eq!(resolved.source, GenerationLimitSource::ProductDefault);
    }

    #[test]
    fn r24_g_h_provenance_matches_authority() {
        let profile = ModelProviderConfig {
            request_max_output_tokens: Some(32_000),
            ..ModelProviderConfig::default()
        };
        assert_eq!(
            resolve_generation_limit(Some(&profile), Some(384_000)).source,
            GenerationLimitSource::ProfileOverride
        );
        assert_eq!(
            resolve_generation_limit(None, Some(384_000)).source,
            GenerationLimitSource::ProductDefault
        );
    }

    #[test]
    fn r24_i_fallback_when_no_product_default_can_apply() {
        let resolved = resolve_generation_limit_with_product_default(None, Some(384_000), None);
        assert_eq!(resolved.effective_tokens, None);
        assert_eq!(resolved.source, GenerationLimitSource::ModelMaximumFallback);
        assert_eq!(resolved.native_max_tokens(), None);
    }

    #[test]
    fn r24_zero_profile_override_is_unset() {
        let profile = ModelProviderConfig {
            request_max_output_tokens: Some(0),
            ..ModelProviderConfig::default()
        };
        let resolved = resolve_generation_limit(Some(&profile), Some(384_000));
        assert_eq!(
            resolved.effective_tokens,
            Some(PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS)
        );
        assert_eq!(resolved.source, GenerationLimitSource::ProductDefault);
    }

    #[test]
    fn r24_unknown_model_max_still_uses_product_default() {
        let resolved = resolve_generation_limit(None, None);
        assert_eq!(
            resolved.effective_tokens,
            Some(PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS)
        );
        assert_eq!(resolved.source, GenerationLimitSource::ProductDefault);
    }

    #[test]
    fn r241_a_request_override_wins_over_profile_and_product() {
        let resolved = resolve_effective_request_generation_limit(
            Some(64_000),
            Some(32_000),
            Some(PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS),
            Some(384_000),
        );
        assert_eq!(resolved.effective_tokens, Some(64_000));
        assert_eq!(resolved.source, GenerationLimitSource::RequestOverride);
        assert_eq!(resolved.native_max_tokens(), Some(64_000));
    }

    #[test]
    fn r241_b_no_request_override_profile_wins() {
        let resolved = resolve_effective_request_generation_limit(
            None,
            Some(64_000),
            Some(PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS),
            Some(384_000),
        );
        assert_eq!(resolved.effective_tokens, Some(64_000));
        assert_eq!(resolved.source, GenerationLimitSource::ProfileOverride);
    }

    #[test]
    fn r241_c_no_request_or_profile_uses_product_default() {
        let resolved = resolve_effective_request_generation_limit(
            None,
            None,
            Some(PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS),
            Some(384_000),
        );
        assert_eq!(
            resolved.effective_tokens,
            Some(PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS)
        );
        assert_eq!(resolved.source, GenerationLimitSource::ProductDefault);
    }

    #[test]
    fn r241_d_request_override_above_model_max_clamps() {
        let resolved = resolve_effective_request_generation_limit(
            Some(500_000),
            Some(32_000),
            Some(PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS),
            Some(384_000),
        );
        assert_eq!(resolved.effective_tokens, Some(384_000));
        assert_eq!(resolved.source, GenerationLimitSource::RequestOverride);
    }

    #[test]
    fn r241_e_request_override_below_model_max_is_exact() {
        let resolved = resolve_effective_request_generation_limit(
            Some(8_000),
            Some(32_000),
            Some(PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS),
            Some(384_000),
        );
        assert_eq!(resolved.effective_tokens, Some(8_000));
        assert_eq!(resolved.source, GenerationLimitSource::RequestOverride);
    }

    #[test]
    fn r241_h_zero_request_override_is_absent() {
        let resolved = resolve_effective_request_generation_limit(
            Some(0),
            Some(32_000),
            Some(PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS),
            Some(384_000),
        );
        assert_eq!(resolved.effective_tokens, Some(32_000));
        assert_eq!(resolved.source, GenerationLimitSource::ProfileOverride);
    }

    #[test]
    fn r241_overflow_request_override_is_rejected() {
        assert_eq!(sanitize_request_generation_override(None).unwrap(), None);
        assert_eq!(sanitize_request_generation_override(Some(0)).unwrap(), None);
        assert_eq!(
            sanitize_request_generation_override(Some(64_000)).unwrap(),
            Some(64_000)
        );
        assert!(sanitize_request_generation_override(Some(u64::from(u32::MAX) + 1)).is_err());
    }

    #[test]
    fn r241_i_request_override_does_not_mutate_profile() {
        let profile = ModelProviderConfig {
            request_max_output_tokens: Some(32_000),
            ..ModelProviderConfig::default()
        };
        let before = profile.request_max_output_tokens;
        let resolved = resolve_effective_request_generation_limit(
            Some(64_000),
            profile.request_max_output_tokens,
            Some(PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS),
            Some(384_000),
        );
        assert_eq!(resolved.effective_tokens, Some(64_000));
        assert_eq!(profile.request_max_output_tokens, before);
        assert_eq!(profile.request_max_output_tokens, Some(32_000));
    }

    #[test]
    fn r241_j_request_override_does_not_persist_to_config_toml() {
        let mut config = crate::config::Config::default();
        config.model_providers.insert(
            "deepseek".into(),
            ModelProviderConfig {
                request_max_output_tokens: Some(32_000),
                ..ModelProviderConfig::default()
            },
        );
        let before = toml::to_string(&config).expect("serialize");
        let _ = resolve_effective_request_generation_limit(
            Some(64_000),
            config.model_providers["deepseek"].request_max_output_tokens,
            Some(PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS),
            Some(384_000),
        );
        let after = toml::to_string(&config).expect("serialize");
        assert_eq!(before, after);
        assert_eq!(
            config.model_providers["deepseek"].request_max_output_tokens,
            Some(32_000)
        );
        assert!(
            !after.contains("64000"),
            "request override must not appear in config.toml"
        );
    }

    #[test]
    fn r241_n_session_open_does_not_invent_request_override() {
        let resolved = resolve_generation_limit(None, Some(384_000));
        assert_eq!(resolved.source, GenerationLimitSource::ProductDefault);
        assert_ne!(resolved.source, GenerationLimitSource::RequestOverride);
        assert_eq!(
            resolved.effective_tokens,
            Some(PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS)
        );
    }
}
