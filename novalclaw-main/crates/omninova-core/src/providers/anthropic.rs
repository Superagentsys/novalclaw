use crate::providers::{ChatRequest, ChatResponse, OpenAiProvider, Provider};
use async_trait::async_trait;

/// Anthropic provider adapter.
///
/// Notes:
/// - Uses an OpenAI-compatible endpoint when available.
/// - This keeps the core architecture provider-pluggable while we phase in
///   native Anthropic message schema support later.
pub struct AnthropicProvider {
    inner: OpenAiProvider,
}

impl AnthropicProvider {
    pub fn new(
        base_url: Option<&str>,
        api_key: Option<&str>,
        model: impl Into<String>,
        temperature: f64,
        max_tokens: Option<u32>,
    ) -> Self {
        Self {
            inner: OpenAiProvider::new(base_url, api_key, model, temperature, max_tokens),
        }
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn chat(&self, request: ChatRequest<'_>) -> anyhow::Result<ChatResponse> {
        self.inner.chat(request).await
    }

    async fn health_check(&self) -> bool {
        self.inner.health_check().await
    }
}
