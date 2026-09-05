//! Authoritative model context budget authority and conservative token
//! estimation for the final OpenAI-compatible Provider envelope.
//!
//! C1 establishes a hard local preflight: if OmniNova knows a model budget and
//! the estimated request exceeds it, the request is blocked before any HTTP
//! send. This module intentionally does NOT implement compaction or recovery.

use crate::config::{Config, ModelProviderConfig};
use crate::providers::generation_limit::{
    GenerationLimitSource, ResolvedGenerationLimit,
};
use crate::providers::ChatMessage;
use crate::tools::ToolSpec;

/// Conservative output reserve used only when neither a request cap nor a
/// model maximum is known. This is not a generation default to send.
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

/// Budget cost charged for one image part, independent of how many transport
/// bytes its data URL carries.
///
/// A vision model bills an image by tiles, not by the base64 characters that
/// happen to move it over the wire. A single 1280px desktop frame is roughly
/// 250KB of base64, so counting those characters as text overstates one
/// screenshot by about two orders of magnitude.
pub const IMAGE_BUDGET_TOKENS: u64 = 1_024;

/// Conservative per-tool-result soft cap used when no model context window is
/// known. It is deliberately not a "context window": it only prevents a single
/// oversized historical tool result from remaining fully model-visible when
/// there is no authoritative budget to detect pressure.
///
/// 48,000 estimated tokens is intentionally conservative:
/// - It is below common small/medium model limits (~64K-128K) even after
///   reserving output space.
/// - It is large enough to keep normal file reads / command outputs usable.
/// - It is small enough that a single ~1.9M-char tool result (roughly 2.4M
///   estimated tokens) is always caught.
pub const UNKNOWN_BUDGET_TOOL_SOFT_CAP_TOKENS: u64 = 48_000;

/// When the budget is unknown, we still protect the most recent tail so the
/// current tool interaction is not pruned before the model can consume it.
/// 8K estimated tokens is a small recent-turn budget for this best-effort path.
pub const UNKNOWN_BUDGET_RECENT_TAIL_TOKENS: u64 = 8_000;

/// Semantic marker for requests blocked locally by C1 before Provider dispatch.
pub const CONTEXT_BUDGET_EXCEEDED_MARKER: &str = "ContextBudgetExceeded";

/// Semantic marker for Provider-reported context overflow that is eligible for
/// bounded reactive recovery (forced maintenance + one retry).
pub const CONTEXT_WINDOW_EXCEEDED_MARKER: &str = "ContextWindowExceeded";

/// Optional numeric window extracted from a Provider overflow message for
/// diagnostics only. Never persisted as trusted model metadata in C3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderOverflowInfo {
    pub provider_reported_window: Option<u64>,
}

/// Recognizes a Provider-reported context overflow from an error string.
///
/// This intentionally only classifies explicit semantic overflow evidence:
/// common OpenAI-compatible codes/phrases are accepted, while generic HTTP 400
/// bodies without those phrases are not treated as overflow.
pub fn context_window_exceeded_info(error: &anyhow::Error) -> Option<ProviderOverflowInfo> {
    let text = error.to_string();
    let lower = text.to_ascii_lowercase();
    let patterns = [
        "context_length_exceeded",
        "context window exceeded",
        "maximum context length",
        "context length exceeded",
        "maximum input tokens",
        "too many input tokens",
        "input too long",
    ];
    let is_overflow = text.contains(CONTEXT_WINDOW_EXCEEDED_MARKER)
        || patterns.iter().any(|pattern| lower.contains(pattern));
    if !is_overflow {
        return None;
    }
    Some(ProviderOverflowInfo {
        provider_reported_window: extract_reported_window(&text),
    })
}

fn extract_reported_window(text: &str) -> Option<u64> {
    // Accept forms like "maximum context length is 1048576 tokens" or
    // "context window is 1048576 tokens".
    let lower = text.to_ascii_lowercase();
    for keyword in [
        "maximum context length is ",
        "context length is ",
        "context window is ",
        "max context length is ",
        "maximum context length ",
    ] {
        if let Some(pos) = lower.find(keyword) {
            let rest = &lower[pos + keyword.len()..];
            let number: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == ',')
                .collect();
            let cleaned = number.replace(',', "");
            if let Ok(value) = cleaned.parse::<u64>() {
                return Some(value);
            }
        }
    }
    None
}

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

/// Configuration error when reserved tokens leave no usable input budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBudgetConfigError {
    pub context_window_tokens: u64,
    pub output_reserve_tokens: u64,
    pub safety_reserve_tokens: u64,
}

impl std::fmt::Display for ContextBudgetConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: output reserve {} + safety reserve {} >= context window {}",
            CONTEXT_BUDGET_EXCEEDED_MARKER,
            self.output_reserve_tokens,
            self.safety_reserve_tokens,
            self.context_window_tokens
        )
    }
}

impl std::error::Error for ContextBudgetConfigError {}

/// Effective input budget after reserving output and safety space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextBudget {
    pub context_window_tokens: u64,
    pub model_max_output_tokens: Option<u64>,
    pub request_output_reserve_tokens: u64,
    pub output_reserve_tokens: u64,
    pub safety_reserve_tokens: u64,
    pub max_input_tokens: u64,
    pub source: ContextBudgetSource,
    pub request_generation_limit_source: GenerationLimitSource,
}

impl ContextBudget {
    pub fn new(
        context_window_tokens: u64,
        max_output_tokens: Option<u64>,
        source: ContextBudgetSource,
    ) -> Self {
        Self::from_parts(
            context_window_tokens,
            max_output_tokens,
            None,
            DEFAULT_SAFETY_RESERVE_TOKENS,
            source,
        )
    }

    /// Rebuilds the budget using an authoritative per-request output cap.
    ///
    /// `None` or `0` falls back to the model maximum (conservative). A known
    /// request cap is clamped to the model maximum when that capability is known.
    pub fn with_request_output_cap(self, request_output_cap: Option<u64>) -> Self {
        let source = if request_output_cap.filter(|value| *value > 0).is_some() {
            match self.request_generation_limit_source {
                GenerationLimitSource::ModelMaximumFallback => {
                    GenerationLimitSource::ProfileOverride
                }
                other => other,
            }
        } else {
            GenerationLimitSource::ModelMaximumFallback
        };
        self.with_resolved_generation_limit(ResolvedGenerationLimit {
            effective_tokens: request_output_cap.filter(|value| *value > 0),
            source,
        })
    }

    /// Applies an ephemeral request-scoped generation override when present.
    /// `None` or `0` leaves the existing profile/product policy unchanged.
    pub fn with_request_generation_override(self, request_override: Option<u32>) -> Self {
        let Some(tokens) = request_override.map(u64::from).filter(|value| *value > 0) else {
            return self;
        };
        self.with_resolved_generation_limit(
            crate::providers::generation_limit::resolve_effective_request_generation_limit(
                Some(tokens),
                None,
                None,
                self.model_max_output_tokens,
            ),
        )
    }

    pub fn with_resolved_generation_limit(self, resolved: ResolvedGenerationLimit) -> Self {
        let mut next = Self::from_parts(
            self.context_window_tokens,
            self.model_max_output_tokens,
            resolved.effective_tokens,
            self.safety_reserve_tokens,
            self.source,
        );
        next.request_generation_limit_source = resolved.source;
        next
    }

    fn from_parts(
        context_window_tokens: u64,
        model_max_output_tokens: Option<u64>,
        request_output_cap: Option<u64>,
        safety_reserve_tokens: u64,
        source: ContextBudgetSource,
    ) -> Self {
        let output_reserve_tokens =
            effective_request_output_tokens(request_output_cap, model_max_output_tokens);
        let reserved = output_reserve_tokens.saturating_add(safety_reserve_tokens);
        let max_input_tokens = context_window_tokens.saturating_sub(reserved);
        Self {
            context_window_tokens,
            model_max_output_tokens,
            request_output_reserve_tokens: output_reserve_tokens,
            output_reserve_tokens,
            safety_reserve_tokens,
            max_input_tokens,
            source,
            request_generation_limit_source: if request_output_cap.filter(|value| *value > 0).is_some()
            {
                GenerationLimitSource::ProfileOverride
            } else {
                GenerationLimitSource::ModelMaximumFallback
            },
        }
    }

    pub fn ensure_usable(&self) -> Result<(), ContextBudgetConfigError> {
        let reserved = self
            .output_reserve_tokens
            .saturating_add(self.safety_reserve_tokens);
        if reserved >= self.context_window_tokens {
            return Err(ContextBudgetConfigError {
                context_window_tokens: self.context_window_tokens,
                output_reserve_tokens: self.output_reserve_tokens,
                safety_reserve_tokens: self.safety_reserve_tokens,
            });
        }
        Ok(())
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

/// Resolves the single semantic output reserve for this request.
///
/// Authority: explicit request cap (when > 0) clamped to model max, else
/// model max, else the conservative default. Never invents a smaller cap
/// merely to enlarge the input budget.
pub fn effective_request_output_tokens(
    request_output_cap: Option<u64>,
    model_max_output_tokens: Option<u64>,
) -> u64 {
    let request = request_output_cap.filter(|value| *value > 0);
    match (request, model_max_output_tokens.filter(|value| *value > 0)) {
        (Some(request), Some(model_max)) => request.min(model_max),
        (Some(request), None) => request,
        (None, Some(model_max)) => model_max,
        (None, None) => DEFAULT_OUTPUT_RESERVE_TOKENS,
    }
}

/// Reads the provider-native output limit from a finalized request body.
///
/// Normalizes OpenAI `max_tokens` / `max_completion_tokens`, Anthropic
/// `max_tokens`, and Gemini `max_output_tokens` to one semantic value.
/// `0` is treated as missing (OpenAI-compatible providers omit a zero cap).
pub fn native_request_output_limit_from_json(body: &str) -> Option<u64> {
    let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
    for key in ["max_tokens", "max_output_tokens", "max_completion_tokens"] {
        if let Some(tokens) = value.get(key).and_then(json_positive_u64) {
            return Some(tokens);
        }
    }
    if let Some(tokens) = value
        .pointer("/generationConfig/maxOutputTokens")
        .and_then(json_positive_u64)
    {
        return Some(tokens);
    }
    None
}

fn json_positive_u64(value: &serde_json::Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))
        .filter(|tokens| *tokens > 0)
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
    ///
    /// Inline image payloads are charged at [`IMAGE_BUDGET_TOKENS`] each rather
    /// than by their base64 length, which keeps this envelope estimate on the
    /// same scale as [`Self::estimate_messages_with_tools`]. Measuring the two
    /// differently let proactive maintenance see a harmless context while C1
    /// blocked the very same request.
    pub fn estimate_request(&self, body: &str) -> u64 {
        let (text, images) = strip_inline_image_payloads(body);
        self.estimate_text(&text)
            .saturating_add(images.saturating_mul(IMAGE_BUDGET_TOKENS))
            .saturating_add(8)
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
                total = total
                    .saturating_add((images.len() as u64).saturating_mul(IMAGE_BUDGET_TOKENS));
            }
        }
        for tool in tools {
            let spec = serde_json::to_string(tool).unwrap_or_default();
            total = total.saturating_add(self.estimate_text(&spec));
        }
        total.saturating_add(8)
    }
}

/// Removes inline `data:` image payloads from a serialized request body and
/// reports how many were found, so the caller can charge each one a fixed
/// budget cost instead of its transport length.
///
/// Base64 inside a JSON string needs no escaping, so each payload runs from the
/// `data:` scheme to the closing quote. Bodies without an inline image are
/// returned untouched.
fn strip_inline_image_payloads(body: &str) -> (std::borrow::Cow<'_, str>, u64) {
    const SCHEME: &str = "data:image/";
    if !body.contains(SCHEME) {
        return (std::borrow::Cow::Borrowed(body), 0);
    }
    let mut kept = String::with_capacity(body.len());
    let mut images = 0u64;
    let mut rest = body;
    while let Some(start) = rest.find(SCHEME) {
        kept.push_str(&rest[..start]);
        let payload = &rest[start..];
        let end = payload.find('"').unwrap_or(payload.len());
        images += 1;
        rest = &payload[end..];
    }
    kept.push_str(rest);
    (std::borrow::Cow::Owned(kept), images)
}

/// Returns the largest estimated tool-result payload currently present in the
/// model-visible context. Used by the unknown-budget safety path.
pub fn largest_tool_result_tokens(messages: &[ChatMessage], estimator: &TokenEstimator) -> u64 {
    let mut largest = 0u64;
    for message in messages {
        if message.role != "tool" {
            continue;
        }
        let estimated = estimator.estimate_text(&message.content);
        if estimated > largest {
            largest = estimated;
        }
    }
    largest
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
    // Explicit profile window is highest priority, matching legacy behavior.
    if let Some(window) = profile.and_then(|p| p.context_window_tokens) {
        return Some(ContextBudget::new(
            window,
            profile.and_then(|p| p.max_output_tokens),
            ContextBudgetSource::ExplicitConfig,
        ));
    }

    // Trusted exact registry entry. Built-in entries are exact model IDs only.
    let caps = crate::providers::model_capabilities::resolve_model_capabilities(model, profile);
    let window = caps.context_window_tokens?;
    let source = if profile.map(|p| p.context_window_tokens.is_some()).unwrap_or(false) {
        ContextBudgetSource::ExplicitConfig
    } else {
        ContextBudgetSource::BuiltIn
    };
    Some(ContextBudget::new(
        window,
        profile
            .and_then(|p| p.max_output_tokens)
            .or(caps.max_output_tokens),
        source,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_reserves_output_and_safety() {
        let budget = ContextBudget::new(1_048_576, Some(16_384), ContextBudgetSource::ExplicitConfig);
        assert_eq!(budget.max_input_tokens, 999_424);
        assert_eq!(budget.output_reserve_tokens, 16_384);
        assert_eq!(budget.request_output_reserve_tokens, 16_384);
        assert_eq!(budget.model_max_output_tokens, Some(16_384));
        assert_eq!(budget.safety_reserve_tokens, 32_768);
    }

    #[test]
    fn budget_never_underflows() {
        let budget = ContextBudget::new(1_000, None, ContextBudgetSource::ExplicitConfig);
        assert_eq!(budget.max_input_tokens, 0);
        assert!(budget.ensure_usable().is_err());
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
        assert_eq!(budget.output_reserve_tokens, 16_384);
    }

    #[test]
    fn a_official_flash_budget_is_one_million_by_384k() {
        let cfg = Config::default();
        let budget = resolve_context_budget(&cfg, "deepseek-v4-flash", None).unwrap();
        assert_eq!(budget.context_window_tokens, 1_000_000);
        assert_eq!(budget.model_max_output_tokens, Some(384_000));
        assert_eq!(budget.output_reserve_tokens, 384_000);
        assert_eq!(budget.source, ContextBudgetSource::BuiltIn);
    }

    #[test]
    fn b_official_pro_budget_matches_flash_without_exact_tokenizer() {
        let cfg = Config::default();
        let budget = resolve_context_budget(&cfg, "deepseek-v4-pro", None).unwrap();
        assert_eq!(budget.context_window_tokens, 1_000_000);
        assert_eq!(budget.output_reserve_tokens, 384_000);
        let caps = crate::providers::model_capabilities::resolve_model_capabilities_with_endpoint(
            "deepseek-v4-pro",
            None,
            Some("https://api.deepseek.com/v1"),
        );
        assert_eq!(
            caps.token_strategy,
            crate::providers::model_capabilities::TokenStrategy::Unavailable
        );
    }

    #[test]
    fn unknown_model_alias_does_not_inherit_builtin_window() {
        let cfg = Config::default();
        assert!(resolve_context_budget(&cfg, "gpt-4o-my-proxy", None).is_none());
        assert!(resolve_context_budget(&cfg, "gpt-4o-mini-custom", None).is_none());
    }

    #[test]
    fn a_safety_estimator_formula_is_unchanged() {
        let estimator = TokenEstimator::new();
        let text = "hello";
        assert_eq!(
            estimator.estimate_text(text),
            text.chars().count() as u64 + (text.len() / 4) as u64 + 4
        );
        assert_eq!(estimator.estimate_request(text), estimator.estimate_text(text) + 8);
    }

    #[test]
    fn unknown_model_has_no_fabricated_budget() {
        let cfg = Config::default();
        assert!(resolve_context_budget(&cfg, "unknown-alias", None).is_none());
    }

    fn flash_budget() -> ContextBudget {
        ContextBudget::new(1_000_000, Some(384_000), ContextBudgetSource::BuiltIn)
    }

    #[test]
    fn r2_a_one_million_model_with_32k_request_uses_32k_reserve() {
        let budget = flash_budget().with_request_output_cap(Some(32_000));
        assert_eq!(budget.model_max_output_tokens, Some(384_000));
        assert_eq!(budget.request_output_reserve_tokens, 32_000);
        assert_eq!(budget.output_reserve_tokens, 32_000);
        assert_eq!(budget.safety_reserve_tokens, 32_768);
        assert_eq!(budget.max_input_tokens, 1_000_000 - 32_000 - 32_768);
        assert_ne!(budget.max_input_tokens, 583_232);
    }

    #[test]
    fn r2_b_one_million_model_with_384k_request_keeps_conservative_input() {
        let budget = flash_budget().with_request_output_cap(Some(384_000));
        assert_eq!(budget.output_reserve_tokens, 384_000);
        assert_eq!(budget.max_input_tokens, 1_000_000 - 384_000 - 32_768);
        assert_eq!(budget.max_input_tokens, 583_232);
    }

    #[test]
    fn r2_c_request_output_above_model_max_clamps_to_model_max() {
        let budget = flash_budget().with_request_output_cap(Some(500_000));
        assert_eq!(budget.request_output_reserve_tokens, 384_000);
        assert_eq!(budget.output_reserve_tokens, 384_000);
        assert_eq!(budget.max_input_tokens, 583_232);
    }

    #[test]
    fn r2_d_missing_request_output_falls_back_to_model_max() {
        let budget = flash_budget().with_request_output_cap(None);
        assert_eq!(budget.output_reserve_tokens, 384_000);
        assert_eq!(budget.max_input_tokens, 583_232);
        let zero = flash_budget().with_request_output_cap(Some(0));
        assert_eq!(zero.output_reserve_tokens, 384_000);
    }

    #[test]
    fn r2_h_pressure_threshold_recalculates_from_new_max_input() {
        let conservative = flash_budget();
        let dynamic = flash_budget().with_request_output_cap(Some(32_000));
        assert_eq!(
            conservative.pressure_threshold(),
            ratio_tokens(conservative.max_input_tokens, PRESSURE_THRESHOLD_RATIO)
        );
        assert_eq!(
            dynamic.pressure_threshold(),
            ratio_tokens(dynamic.max_input_tokens, PRESSURE_THRESHOLD_RATIO)
        );
        assert!(dynamic.pressure_threshold() > conservative.pressure_threshold());
    }

    #[test]
    fn r241_g_c2_request_override_updates_reserve_and_pressure() {
        let product = flash_budget().with_resolved_generation_limit(
            crate::providers::generation_limit::resolve_generation_limit(None, Some(384_000)),
        );
        assert_eq!(product.request_output_reserve_tokens, 32_000);
        assert_eq!(
            product.request_generation_limit_source,
            GenerationLimitSource::ProductDefault
        );
        let overridden = product.with_request_generation_override(Some(64_000));
        assert_eq!(overridden.request_output_reserve_tokens, 64_000);
        assert_eq!(overridden.output_reserve_tokens, 64_000);
        assert_eq!(overridden.max_input_tokens, 1_000_000 - 64_000 - 32_768);
        assert_eq!(
            overridden.request_generation_limit_source,
            GenerationLimitSource::RequestOverride
        );
        assert_eq!(
            overridden.pressure_threshold(),
            ratio_tokens(overridden.max_input_tokens, PRESSURE_THRESHOLD_RATIO)
        );
        assert!(overridden.pressure_threshold() < product.pressure_threshold());
        let unchanged = product.with_request_generation_override(None);
        assert_eq!(unchanged.request_output_reserve_tokens, 32_000);
        assert_eq!(
            unchanged.request_generation_limit_source,
            GenerationLimitSource::ProductDefault
        );
    }

    #[test]
    fn r2_i_safety_reserve_remains_unchanged() {
        let budget = flash_budget().with_request_output_cap(Some(32_000));
        assert_eq!(budget.safety_reserve_tokens, DEFAULT_SAFETY_RESERVE_TOKENS);
        assert_eq!(
            flash_budget().safety_reserve_tokens,
            DEFAULT_SAFETY_RESERVE_TOKENS
        );
    }

    #[test]
    fn r2_known_request_cap_without_model_max_uses_request_cap() {
        let budget = ContextBudget::new(1_000_000, None, ContextBudgetSource::ExplicitConfig)
            .with_request_output_cap(Some(32_000));
        assert_eq!(budget.model_max_output_tokens, None);
        assert_eq!(budget.output_reserve_tokens, 32_000);
        assert_eq!(budget.max_input_tokens, 1_000_000 - 32_000 - 32_768);
    }

    #[test]
    fn a_desktop_frame_costs_a_fixed_image_budget_not_its_base64_length() {
        let estimator = TokenEstimator::new();
        // A 1280px screenshot is roughly this much base64.
        let payload = "A".repeat(250_000);
        let body =
            format!(r#"{{"messages":[{{"image_url":{{"url":"data:image/jpeg;base64,{payload}"}}}}]}}"#);

        let estimated = estimator.estimate_request(&body);

        assert!(
            estimated < 4 * IMAGE_BUDGET_TOKENS,
            "one frame must not be billed by transport length: {estimated}"
        );
        let budget = ContextBudget::new(128_000, Some(8_192), ContextBudgetSource::ExplicitConfig);
        assert!(
            estimated <= budget.max_input_tokens,
            "a single screenshot must not trip the preflight on a 128K model"
        );
    }

    #[test]
    fn request_and_candidate_estimates_agree_on_image_cost() {
        let estimator = TokenEstimator::new();
        let payload = "A".repeat(250_000);
        let messages = vec![ChatMessage::user_with_images(
            "look",
            vec![format!("data:image/jpeg;base64,{payload}")],
        )];
        let body = crate::providers::native_request::native_context_view_json(&messages, &[]);

        let candidate = estimator.estimate_messages_with_tools(&messages, &[]);
        let request = estimator.estimate_request(&body);

        // Proactive maintenance measures the candidate while C1 measures the
        // envelope. When those disagreed, maintenance saw a healthy context and
        // C1 still blocked the identical request.
        assert!(
            request.abs_diff(candidate) < IMAGE_BUDGET_TOKENS,
            "candidate={candidate} request={request}"
        );
    }

    #[test]
    fn every_inline_image_payload_is_counted_and_removed() {
        let body = r#"[{"url":"data:image/jpeg;base64,AAAA"},{"url":"data:image/png;base64,BBBB"},{"text":"plain"}]"#;

        let (text, images) = strip_inline_image_payloads(body);

        assert_eq!(images, 2);
        assert!(!text.contains("AAAA") && !text.contains("BBBB"));
        assert!(text.contains("plain"), "surrounding JSON must survive: {text}");
    }

    #[test]
    fn bodies_without_images_are_not_reallocated() {
        let (text, images) = strip_inline_image_payloads(r#"{"content":"no pictures here"}"#);
        assert_eq!(images, 0);
        assert!(matches!(text, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn r2_overflow_reserve_is_a_config_error() {
        let budget = ContextBudget::new(1_000, Some(384_000), ContextBudgetSource::ExplicitConfig);
        let error = budget.ensure_usable().expect_err("unusable budget");
        assert!(error.to_string().contains(CONTEXT_BUDGET_EXCEEDED_MARKER));
        assert_eq!(budget.max_input_tokens, 0);
    }

    #[test]
    fn r2_native_request_json_normalizes_provider_output_fields() {
        assert_eq!(
            native_request_output_limit_from_json(r#"{"max_tokens":32000}"#),
            Some(32_000)
        );
        assert_eq!(
            native_request_output_limit_from_json(r#"{"max_completion_tokens":64000}"#),
            Some(64_000)
        );
        assert_eq!(
            native_request_output_limit_from_json(r#"{"max_output_tokens":8192}"#),
            Some(8_192)
        );
        assert_eq!(
            native_request_output_limit_from_json(
                r#"{"generationConfig":{"maxOutputTokens":4096}}"#
            ),
            Some(4_096)
        );
        assert_eq!(
            native_request_output_limit_from_json(r#"{"max_tokens":0}"#),
            None
        );
        assert_eq!(native_request_output_limit_from_json("hello"), None);
    }

    #[test]
    fn r2_l_switching_models_recalculates_budget() {
        let cfg = Config::default();
        let flash = resolve_context_budget(&cfg, "deepseek-v4-flash", None).unwrap();
        let gpt = resolve_context_budget(&cfg, "gpt-4o", None).unwrap();
        assert_eq!(flash.context_window_tokens, 1_000_000);
        assert_eq!(gpt.context_window_tokens, 128_000);
        assert_ne!(flash.max_input_tokens, gpt.max_input_tokens);
    }
}