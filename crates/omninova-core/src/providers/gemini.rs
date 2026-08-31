use crate::providers::context_budget::ContextBudget;
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

    pub fn with_context_budget(mut self, budget: Option<ContextBudget>) -> Self {
        self.inner = self.inner.with_context_budget(budget);
        self
    }

    pub fn with_generation_limit_source(
        mut self,
        source: crate::providers::generation_limit::GenerationLimitSource,
    ) -> Self {
        self.inner = self.inner.with_generation_limit_source(source);
        self
    }

    pub fn with_exact_tokenizer(mut self, name: Option<String>) -> Self {
        self.inner = self.inner.with_exact_tokenizer(name);
        self
    }
}

#[async_trait]
impl Provider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    fn model(&self) -> Option<&str> {
        self.inner.model()
    }

    async fn chat(&self, request: ChatRequest<'_>) -> anyhow::Result<ChatResponse> {
        self.inner.chat(request).await
    }

    async fn health_check(&self) -> bool {
        self.inner.health_check().await
    }

    fn context_budget(&self) -> Option<ContextBudget> {
        self.inner.context_budget()
    }

    fn exact_tokenizer(&self) -> Option<&str> {
        self.inner.exact_tokenizer()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ProviderTimeouts;

    #[test]
    fn gemini_model_identity_uses_configured_model() {
        let provider = GeminiProvider::new(
            Some("https://example.test/v1"),
            None,
            "gemini-2.5-pro",
            0.0,
            None,
            ProviderTimeouts::default(),
        );
        assert_eq!(provider.name(), "gemini");
        assert_eq!(provider.model(), Some("gemini-2.5-pro"));
    }
}
