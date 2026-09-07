pub mod anthropic;
pub(crate) mod anthropic_count;
pub mod context_budget;
pub(crate) mod deepseek_v4;
pub mod factory;
pub mod gemini;
pub mod generation_limit;
pub mod model_capabilities;
pub mod native_request;
pub mod openai;
pub mod token_counter;
pub mod traits;

pub use anthropic::AnthropicProvider;
pub use factory::{build_provider_from_config, build_provider_with_selection, ProviderSelection};
pub use generation_limit::{
    resolve_effective_request_generation_limit, resolve_generation_limit, GenerationLimitSource,
    PRODUCT_DEFAULT_REQUEST_MAX_OUTPUT_TOKENS, ResolvedGenerationLimit,
};
pub use gemini::GeminiProvider;
pub use traits::{
    ChatMessage, ChatRequest, ChatResponse, ConversationMessage, Provider, ProviderHttpError,
    TokenUsage, ToolCall, ToolResultMessage,
};

pub use openai::{MockProvider, OpenAiProvider, ProviderTimeoutKind, ProviderTimeouts};
pub use model_capabilities::{
    resolve_model_capabilities, ModelCapabilities, ProviderCountApiKind, TokenMeasurement,
    TokenStrategy,
};

/// Whether a provider takes images as OpenAI-style `image_url` content parts.
/// Anthropic and Gemini need their own encodings, so callers must not attach
/// screenshots for them. An unnamed provider is assumed OpenAI-compatible.
pub fn provider_accepts_openai_images(provider: Option<&str>) -> bool {
    let Some(name) = provider.map(str::to_ascii_lowercase) else {
        return true;
    };
    !matches!(name.as_str(), "anthropic" | "gemini" | "mock")
}
