use crate::providers::{ChatRequest, ChatResponse, OpenAiProvider, Provider, ProviderTimeouts};
use async_trait::async_trait;

/// Gemini provider adapter over OpenAI-compatible gateways.
pub struct GeminiProvider {
    inner: OpenAiProvider,
}

impl GeminiProvider {
    pub fn new(
        base_url: Option<&str>,
        api_key: Option<&str>,
        model: impl Into<String>,
        temperature: f64,
        max_tokens: Option<u32>,
        timeouts: ProviderTimeouts,
    ) -> Self {
        Self {
            inner: OpenAiProvider::new(base_url, api_key, model, temperature, max_tokens, timeouts),
        }
    }

    /// Applies a per-provider HTTP transport mode to the underlying client.
    pub fn with_transport_mode(mut self, mode: crate::config::TransportMode) -> Self {
        self.inner = self.inner.with_transport_mode(mode);
        self
    }
}

#[async_trait]
impl Provider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    async fn chat(&self, request: ChatRequest<'_>) -> anyhow::Result<ChatResponse> {
        self.inner.chat(request).await
    }

    async fn health_check(&self) -> bool {
        self.inner.health_check().await
    }
}
