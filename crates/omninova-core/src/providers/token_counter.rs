//! Optional model-aware display token counter.
//!
//! C1/C2 keep using [`TokenEstimator`](super::context_budget::TokenEstimator).
//! This module is display-only: it never tokenizes raw HTTP JSON and never
//! guesses a tokenizer family from a model alias.

use crate::config::ModelProviderConfig;
use crate::providers::context_budget::TokenEstimator;
use crate::providers::deepseek_v4::{
    count_deepseek_v4_flash_tokens, settings_from_request_body, DeepSeekV4RequestSettings,
    TOKENIZER_NAME,
};
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
    Exact {
        tokens: u64,
        production_validated: bool,
    },
    SafetyEstimate(u64),
}

impl DisplayMeasurement {
    pub fn tokens(self) -> u64 {
        match self {
            Self::Exact { tokens, .. } | Self::SafetyEstimate(tokens) => tokens,
        }
    }

    pub fn kind(self) -> DisplayMeasurementKind {
        match self {
            Self::Exact { .. } => DisplayMeasurementKind::Exact,
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
    endpoint: Option<&str>,
) -> Option<&'static str> {
    let caps = crate::providers::model_capabilities::resolve_model_capabilities_with_endpoint(
        model, profile, endpoint,
    );
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
        "deepseek_v4_flash_0731" => Some(&DeepSeekV4FlashCounter),
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
    match try_exact_display(model, configured_tokenizer, messages, tools, None) {
        Some(DisplayMeasurement::Exact { tokens, .. }) => Some(tokens),
        _ => None,
    }
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
    if let Some(exact) =
        try_exact_display(model, configured_tokenizer, messages, tools, request_body)
    {
        return exact;
    }
    let estimator = TokenEstimator::new();
    let safety = if let Some(body) = request_body {
        estimator.estimate_request(body)
    } else {
        estimator.estimate_messages_with_tools(messages, tools)
    };
    DisplayMeasurement::SafetyEstimate(safety)
}

fn try_exact_display(
    model: &str,
    configured_tokenizer: Option<&str>,
    messages: &[ChatMessage],
    tools: &[ToolSpec],
    request_body: Option<&str>,
) -> Option<DisplayMeasurement> {
    let name = resolve_tokenizer_name(model, configured_tokenizer)?;
    match name.as_str() {
        "test_exact" => tokenizer_impl("test_exact")?
            .count_chat_tokens(messages, tools)
            .map(|tokens| DisplayMeasurement::Exact {
                tokens,
                production_validated: true,
            }),
        TOKENIZER_NAME => match count_deepseek_v4_flash_tokens(
            messages,
            tools,
            settings_from_request_body(request_body),
        ) {
            Ok(measured) => Some(DisplayMeasurement::Exact {
                tokens: measured.tokens,
                production_validated: false,
            }),
            Err(_) => None,
        },
        _ => None,
    }
}

fn resolve_tokenizer_name(model: &str, configured: Option<&str>) -> Option<String> {
    if let Some(name) = configured {
        if tokenizer_impl(name).is_some() {
            return Some(name.to_string());
        }
    }
    match crate::providers::model_capabilities::resolve_model_capabilities(model, None).token_strategy
    {
        crate::providers::model_capabilities::TokenStrategy::ExactLocalTokenizer(name)
            if tokenizer_impl(name).is_some() =>
        {
            Some(name.to_string())
        }
        _ => None,
    }
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

struct DeepSeekV4FlashCounter;

impl TokenCounter for DeepSeekV4FlashCounter {
    fn count_chat_tokens(&self, messages: &[ChatMessage], tools: &[ToolSpec]) -> Option<u64> {
        count_deepseek_v4_flash_tokens(messages, tools, DeepSeekV4RequestSettings::default())
            .ok()
            .map(|measured| measured.tokens)
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
            resolve_exact_tokenizer_name("any-custom", Some(&profile), None),
            Some("test_exact")
        );
        profile.exact_tokenizer = Some("tiktoken_json_guess".into());
        assert_eq!(
            resolve_exact_tokenizer_name("any-custom", Some(&profile), None),
            None
        );
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
            DisplayMeasurement::Exact { .. } => panic!("unknown model must not be exact"),
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

    #[test]
    fn f_configured_deepseek_tokenizer_counts_simple_chat() {
        let messages = sample_messages();
        let display = measure_display_tokens(
            "any-alias",
            Some(TOKENIZER_NAME),
            &messages,
            &[],
            None,
        );
        match display {
            DisplayMeasurement::Exact {
                tokens,
                production_validated,
            } => {
                assert!(tokens > 0);
                assert!(!production_validated);
            }
            DisplayMeasurement::SafetyEstimate(_) => {
                panic!("configured DeepSeek tokenizer must count locally")
            }
        }
        assert!(count_exact_chat_tokens("deepseek-v4-flash", None, &messages, &[]).is_none());
    }

    #[test]
    fn d_third_party_flash_model_id_is_not_automatically_exact() {
        let messages = sample_messages();
        let display = measure_display_tokens("deepseek-v4-flash", None, &messages, &[], None);
        assert!(matches!(display, DisplayMeasurement::SafetyEstimate(_)));
    }

    #[test]
    fn m_unsupported_images_fall_back_to_safety_estimate() {
        let messages = vec![ChatMessage::user_with_images(
            "look",
            vec!["data:image/png;base64,AAAA".into()],
        )];
        let display = measure_display_tokens(
            "deepseek-v4-flash",
            Some(TOKENIZER_NAME),
            &messages,
            &[],
            None,
        );
        match display {
            DisplayMeasurement::SafetyEstimate(tokens) => {
                assert_eq!(
                    tokens,
                    TokenEstimator::new().estimate_messages_with_tools(&messages, &[])
                );
            }
            DisplayMeasurement::Exact { .. } => panic!("unsupported content must not be exact"),
        }
    }

    #[test]
    fn thinking_request_settings_change_display_count() {
        let messages = vec![ChatMessage::user("What is 2+2?")];
        let chat_body = r#"{"model":"deepseek-v4-flash"}"#;
        let thinking_body = r#"{"model":"deepseek-v4-flash","thinking":true}"#;
        let high_body = r#"{"model":"deepseek-v4-flash","thinking":true,"reasoning_effort":"high"}"#;
        assert_eq!(
            settings_from_request_body(Some(chat_body)).thinking_mode,
            crate::providers::deepseek_v4::ThinkingMode::Chat
        );
        assert_eq!(
            settings_from_request_body(Some(thinking_body)).thinking_mode,
            crate::providers::deepseek_v4::ThinkingMode::Thinking
        );
        let chat_enc = crate::providers::deepseek_v4::encode_omninova_messages(
            &messages,
            &[],
            settings_from_request_body(Some(chat_body)),
        )
        .unwrap();
        let thinking_enc = crate::providers::deepseek_v4::encode_omninova_messages(
            &messages,
            &[],
            settings_from_request_body(Some(thinking_body)),
        )
        .unwrap();
        assert_ne!(chat_enc, thinking_enc);
        let chat = measure_display_tokens(
            "alias",
            Some(TOKENIZER_NAME),
            &messages,
            &[],
            Some(chat_body),
        );
        let high = measure_display_tokens(
            "alias",
            Some(TOKENIZER_NAME),
            &messages,
            &[],
            Some(high_body),
        );
        match (chat, high) {
            (
                DisplayMeasurement::Exact { tokens: a, .. },
                DisplayMeasurement::Exact { tokens: b, .. },
            ) => assert!(b > a),
            _ => panic!("both thinking modes must count exactly"),
        }
    }
}
