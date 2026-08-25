//! Authoritative model context budget authority and conservative token
//! estimation for the final OpenAI-compatible Provider envelope.
//!
//! C1 establishes a hard local preflight: if OmniNova knows a model budget and
//! the estimated request exceeds it, the request is blocked before any HTTP
//! send. This module intentionally does NOT implement compaction or recovery.

use crate::config::{Config, ModelProviderConfig};
use crate::providers::ChatMessage;
use crate::tools::ToolSpec;

/// Default output reserve used when a provider/model has no explicit
/// `max_output_tokens`. This is intentionally conservative.
pub const DEFAULT_OUTPUT_RESERVE_TOKENS: u64 = 16_384;

/// Fixed safety reserve kept between the effective input budget and the
/// provider-declared context window.
pub const DEFAULT_SAFETY_RESERVE_TOKENS: u64 = 32_768;

/// Maintenance starts at this ratio of the effective input budget.
pub const PRESSURE_THRESHOLD_RATIO: f64 = 0.80;

/// Maintenance attempts to bring the context down to this ratio of the input
/// budget when pressure is detected.
pub const TARGET_AFTER_COMPACTION_RATIO: f64 = 0.55;

/// Approximate share of the effective input budget kept as raw recent tail.
pub const RECENT_TAIL_RATIO: f64 = 0.20;

/// Maximum maintenance passes per pressure event.
pub const MAX_MAINTENANCE_PASSES: usize = 2;

/// Cap for source text handed to the summarizer if the old prefix is huge.
pub const MAX_SUMMARY_SOURCE_CHARS: usize = 200_000;

/// Where the resolved context-window size came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextBudgetSource {
    /// Explicit per-model/provider configuration.
    ExplicitConfig,
    /// Trusted built-in exact model metadata.
    BuiltIn,
    /// No authoritative value is known.
    Unknown,
}

impl ContextBudgetSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitConfig => "explicit_config",
            Self::BuiltIn => "builtin",
            Self::Unknown => "unknown",
        }
    }
}

/// Raw context window metadata for one provider/model pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextWindowSpec {
    pub context_window_tokens: u64,
    pub max_output_tokens: Option<u64>,
    pub source: ContextBudgetSource,
}

/// Effective input budget after reserving output and safety space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextBudget {
    pub context_window_tokens: u64,
    pub output_reserve_tokens: u64,
    pub safety_reserve_tokens: u64,
    pub max_input_tokens: u64,
    pub source: ContextBudgetSource,
}

impl ContextBudget {
    pub fn new(
        context_window_tokens: u64,
        max_output_tokens: Option<u64>,
        source: ContextBudgetSource,
    ) -> Self {
        let output_reserve_tokens = max_output_tokens.unwrap_or(DEFAULT_OUTPUT_RESERVE_TOKENS);
        let safety_reserve_tokens = DEFAULT_SAFETY_RESERVE_TOKENS;
        let reserved = output_reserve_tokens.saturating_add(safety_reserve_tokens);
        let max_input_tokens = context_window_tokens.saturating_sub(reserved);
        Self {
            context_window_tokens,
            output_reserve_tokens,
            safety_reserve_tokens,
            max_input_tokens,
            source,
        }
    }

    pub fn pressure_threshold(&self) -> u64 {
        ratio_tokens(self.max_input_tokens, PRESSURE_THRESHOLD_RATIO)
    }

    pub fn target_after_compaction(&self) -> u64 {
        ratio_tokens(self.max_input_tokens, TARGET_AFTER_COMPACTION_RATIO)
    }

    pub fn recent_tail_budget(&self) -> u64 {
        ratio_tokens(self.max_input_tokens, RECENT_TAIL_RATIO)
    }
}

fn ratio_tokens(value: u64, ratio: f64) -> u64 {
    ((value as f64) * ratio).floor() as u64
}

/// Conservative token estimator.
///
/// It intentionally over-counts so a request that is near the boundary is more
/// likely to be blocked locally rather than rejected by the Provider. It is not
/// a model-specific tokenizer; different OpenAI-compatible providers may use
/// different tokenizers.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenEstimator;

impl TokenEstimator {
    pub fn new() -> Self {
        Self
    }

    /// Text estimate: every character counts as one token, plus a quarter of
    /// byte length for conservative JSON/ASCII overhead, plus a small message
    /// overhead. This is deliberately an upper-bound style estimate.
    pub fn estimate_text(&self, text: &str) -> u64 {
        text.chars().count() as u64 + (text.len() / 4) as u64 + 4
    }

    /// Final request envelope estimate: serialize the actual native body and
    /// count it conservatively. This measures system/user/assistant/tool
    /// messages, tool schemas, and all model-visible fields in one step.
    pub fn estimate_request(&self, body: &str) -> u64 {
        self.estimate_text(body) + 8
    }

    /// Estimate an Agent context candidate: all messages plus tool schemas.
    /// This mirrors the final request envelope closely enough for proactive
    /// maintenance while C1 still performs the final hard preflight.
    pub fn estimate_messages_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> u64 {
        let mut total = 0u64;
        for message in messages {
            total = total.saturating_add(self.estimate_text(&message.content));
            if let Some(images) = &message.images {
                total = total.saturating_add((images.len() as u64).saturating_mul(1_024));
            }
        }
        for tool in tools {
            let spec = serde_json::to_string(tool).unwrap_or_default();
            total = total.saturating_add(self.estimate_text(&spec));
        }
        total.saturating_add(8)
    }
}

/// Resolves a context budget from config and/or built-in metadata.
///
/// Resolution order:
/// 1. explicit per-model/provider config
/// 2. trusted built-in exact model metadata
/// 3. unknown (no fabricated value)
pub fn resolve_context_budget(
    _config: &Config,
    model: &str,
    profile: Option<&ModelProviderConfig>,
) -> Option<ContextBudget> {
    if let Some(window) = profile.and_then(|p| p.context_window_tokens) {
        return Some(ContextBudget::new(
            window,
            profile.and_then(|p| p.max_output_tokens),
            ContextBudgetSource::ExplicitConfig,
        ));
    }

    if let Some(window) = builtin_context_window(model) {
        return Some(ContextBudget::new(
            window,
            profile.and_then(|p| p.max_output_tokens),
            ContextBudgetSource::BuiltIn,
        ));
    }

    None
}

fn builtin_context_window(model: &str) -> Option<u64> {
    // Exact trusted metadata only. No name-prefix guessing.
    match model {
        "gpt-4o" | "gpt-4o-mini" | "gpt-4-turbo" => Some(128_000),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_reserves_output_and_safety() {
        let budget = ContextBudget::new(1_048_576, Some(16_384), ContextBudgetSource::ExplicitConfig);
        assert_eq!(budget.max_input_tokens, 999_424);
        assert_eq!(budget.output_reserve_tokens, 16_384);
        assert_eq!(budget.safety_reserve_tokens, 32_768);
    }

    #[test]
    fn budget_never_underflows() {
        let budget = ContextBudget::new(1_000, None, ContextBudgetSource::ExplicitConfig);
        assert_eq!(budget.max_input_tokens, 0);
    }

    #[test]
    fn estimator_counts_large_text_conservatively() {
        let estimator = TokenEstimator::new();
        let large = "x".repeat(1_680_613);
        let estimate = estimator.estimate_text(&large);
        assert!(
            estimate >= 1_680_613,
            "conservative estimate must not undercount"
        );
    }

    #[test]
    fn explicit_config_wins_over_builtin() {
        let cfg = Config::default();
        let mut profile = ModelProviderConfig::default();
        profile.context_window_tokens = Some(256_000);
        let budget = resolve_context_budget(&cfg, "gpt-4o", Some(&profile)).unwrap();
        assert_eq!(budget.context_window_tokens, 256_000);
        assert_eq!(budget.source, ContextBudgetSource::ExplicitConfig);
    }

    #[test]
    fn builtin_exact_model_is_used_when_no_config() {
        let cfg = Config::default();
        let budget = resolve_context_budget(&cfg, "gpt-4o", None).unwrap();
        assert_eq!(budget.context_window_tokens, 128_000);
        assert_eq!(budget.source, ContextBudgetSource::BuiltIn);
    }

    #[test]
    fn unknown_model_has_no_fabricated_budget() {
        let cfg = Config::default();
        assert!(resolve_context_budget(&cfg, "unknown-alias", None).is_none());
    }
}