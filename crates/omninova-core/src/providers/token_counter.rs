//! Optional model-aware display token counter.
//!
//! C1/C2 keep using [`TokenEstimator`](super::context_budget::TokenEstimator).
//! This module is display-only: it never tokenizes raw HTTP JSON and never
//! guesses a tokenizer family from a model alias.

use crate::config::ModelProviderConfig;
use crate::providers::context_budget::TokenEstimator;
use crate::providers::ChatMessage;
use crate::tools::ToolSpec;

/// How a display total was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMeasurementKind {
    Exact,
    Unavailable,
}

/// Display total with explicit provenance. Never labels a safety estimate as exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMeasurement {
    Exact(u64),
    SafetyEstimate(u64),
}

impl DisplayMeasurement {
    pub fn tokens(self) -> u64 {
        match self {
            Self::Exact(value) | Self::SafetyEstimate(value) => value,
        }
    }

    pub fn kind(self) -> DisplayMeasurementKind {
        match self {
            Self::Exact(_) => DisplayMeasurementKind::Exact,
            Self::SafetyEstimate(_) => DisplayMeasurementKind::Unavailable,
        }
    }
}

/// Chat-token counter bound to one trusted tokenizer implementation.
pub trait TokenCounter: Send + Sync {
    fn count_chat_tokens(&self, messages: &[ChatMessage], tools: &[ToolSpec]) -> Option<u64>;
}

/// Resolves a trusted tokenizer name.
///
/// Order:
/// 1. explicit profile `exact_tokenizer` if that implementation exists
/// 2. trusted exact registry entry
/// 3. unavailable
///
/// Unknown tokenizer names are unavailable. Model-name similarity is never used.
pub fn resolve_exact_tokenizer_name(
    model: &str,
    profile: Option<&ModelProviderConfig>,
) -> Option<&'static str> {
    let caps = crate::providers::model_capabilities::resolve_model_capabilities(model, profile);
    match caps.token_strategy {
        crate::providers::model_capabilities::TokenStrategy::ExactLocalTokenizer(name) => {
            tokenizer_impl(name).map(|_| name)
        }
        crate::providers::model_capabilities::TokenStrategy::ProviderCountApi(
            crate::providers::model_capabilities::ProviderCountApiKind::AnthropicNative,
        ) => Some("anthropic_count_tokens"),
        _ => None,
    }
}

fn tokenizer_impl(name: &str) -> Option<&'static dyn TokenCounter> {
    match name {
        "test_exact" => Some(&TestExactChatCounter),
        _ => None,
    }
}

/// Counts chat messages/tools with a trusted implementation. Returns `None`
/// when no exact tokenizer is configured. Never counts serialized HTTP JSON.
pub fn count_exact_chat_tokens(
    model: &str,
    configured_tokenizer: Option<&str>,
    messages: &[ChatMessage],
    tools: &[ToolSpec],
) -> Option<u64> {
    let name = configured_tokenizer
        .filter(|name| *name == "test_exact")
        .or_else(|| {
            let caps = crate::providers::model_capabilities::resolve_model_capabilities(model, None);
            match caps.token_strategy {
                crate::providers::model_capabilities::TokenStrategy::ExactLocalTokenizer(name) => {
                    Some(name)
                }
                _ => None,
            }
        })?;
    tokenizer_impl(name)?.count_chat_tokens(messages, tools)
}

/// Display meter: exact chat tokens when trusted, otherwise the conservative
/// safety estimator. C1/C2 callers must not use this for blocking/maintenance.
pub fn measure_display_tokens(
    model: &str,
    configured_tokenizer: Option<&str>,
    messages: &[ChatMessage],
    tools: &[ToolSpec],
    request_body: Option<&str>,
) -> DisplayMeasurement {
    if let Some(exact) = count_exact_chat_tokens(model, configured_tokenizer, messages, tools) {
        return DisplayMeasurement::Exact(exact);
    }
    let estimator = TokenEstimator::new();
    let safety = if let Some(body) = request_body {
        estimator.estimate_request(body)
    } else {
        estimator.estimate_messages_with_tools(messages, tools)
    };
    DisplayMeasurement::SafetyEstimate(safety)
}

/// Synthetic exact counter used only when a model/profile explicitly opts into
/// the `test_exact` implementation. It counts visible chat content, not HTTP JSON.
struct TestExactChatCounter;

impl TokenCounter for TestExactChatCounter {
    fn count_chat_tokens(&self, messages: &[ChatMessage], tools: &[ToolSpec]) -> Option<u64> {
        let mut total = 0u64;
        for message in messages {
            total = total.saturating_add((message.content.chars().count() as u64) / 4);
        }
        for tool in tools {
            total = total.saturating_add(tool.name.len() as u64);
        }
        Some(total.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ChatMessage;
    use crate::tools::ToolSpec;

    fn sample_messages() -> Vec<ChatMessage> {
        vec![
            ChatMessage::system("sys"),
            ChatMessage::user("hello world"),
        ]
    }

    #[test]
    fn c_exact_tokenizer_only_for_explicitly_trusted_model() {
        let messages = sample_messages();
        assert!(count_exact_chat_tokens("gpt-4o", None, &messages, &[]).is_none());
        assert!(count_exact_chat_tokens("gpt-4o-alias", None, &messages, &[]).is_none());
        assert!(count_exact_chat_tokens("omninova-test-exact", None, &messages, &[]).is_some());
        let mut profile = ModelProviderConfig::default();
        profile.exact_tokenizer = Some("test_exact".into());
        assert_eq!(
            resolve_exact_tokenizer_name("any-custom", Some(&profile)),
            Some("test_exact")
        );
        profile.exact_tokenizer = Some("tiktoken_json_guess".into());
        assert_eq!(resolve_exact_tokenizer_name("any-custom", Some(&profile)), None);
    }

    #[test]
    fn d_unknown_tokenizer_falls_back_to_safety_estimate() {
        let messages = sample_messages();
        let display = measure_display_tokens("unknown-alias", None, &messages, &[], None);
        match display {
            DisplayMeasurement::SafetyEstimate(tokens) => {
                assert_eq!(
                    tokens,
                    TokenEstimator::new().estimate_messages_with_tools(&messages, &[])
                );
            }
            DisplayMeasurement::Exact(_) => panic!("unknown model must not be exact"),
        }
    }

    #[test]
    fn exact_counter_does_not_count_raw_json_envelope() {
        let messages = vec![ChatMessage::user("abcd")];
        let tools = vec![ToolSpec {
            name: "t".into(),
            description: "d".into(),
            parameters: serde_json::json!({"type": "object"}),
        }];
        let exact = count_exact_chat_tokens("omninova-test-exact", None, &messages, &tools).unwrap();
        let json_body = serde_json::json!({
            "model": "omninova-test-exact",
            "messages": [{"role": "user", "content": "abcd"}],
            "tools": tools,
        })
        .to_string();
        let json_estimate = TokenEstimator::new().estimate_request(&json_body);
        assert_ne!(exact, json_estimate);
        assert!(exact < json_estimate);
    }
}
