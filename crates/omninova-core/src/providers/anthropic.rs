use crate::providers::context_budget::ContextBudget;
use crate::providers::anthropic_count::{count_anthropic_tokens, AnthropicCountConfig};
use crate::providers::model_capabilities::{
    resolve_model_capabilities, ProviderCountApiKind, TokenStrategy,
};
use crate::providers::{ChatMessage, ChatRequest, ChatResponse, OpenAiProvider, Provider, ProviderTimeouts};
use crate::tools::ToolSpec;
use async_trait::async_trait;
use std::time::Duration;

/// Anthropic provider adapter.
///
/// Notes:
/// - Uses an OpenAI-compatible endpoint when available.
/// - This keeps the core architecture provider-pluggable while we phase in
///   native Anthropic message schema support later.
pub struct AnthropicProvider {
    inner: OpenAiProvider,
    native_count_config: Option<AnthropicCountConfig>,
}

impl AnthropicProvider {
    pub fn new(
        base_url: Option<&str>,
        api_key: Option<&str>,
        model: impl Into<String>,
        temperature: f64,
        max_tokens: Option<u32>,
        timeouts: ProviderTimeouts,
    ) -> Self {
        let native_count_config = base_url.map(|base_url| AnthropicCountConfig {
            base_url: base_url.trim_end_matches('/').to_string(),
            credential: api_key.map(str::to_string),
        });
        Self {
            inner: OpenAiProvider::new(base_url, api_key, model, temperature, max_tokens, timeouts)
                .with_anthropic_native_count(base_url, api_key),
            native_count_config,
        }
    }

    pub fn with_anthropic_count_trusted(mut self, trusted: bool) -> Self {
        self.inner = self.inner.with_anthropic_count_trusted(trusted);
        self
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

    /// Observation-only native Anthropic count request.
    ///
    /// This never enters session history, never emits lifecycle events, and
    /// never triggers compaction/recovery. Any failure falls back to None so
    /// the caller can use the SafetyEstimate.
    pub async fn count_tokens_native(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
        timeout: Duration,
    ) -> Option<u64> {
        count_anthropic_tokens(
            self.native_count_config.as_ref()?,
            model,
            messages,
            tools,
            timeout,
        )
        .await
        .map(|measurement| measurement.tokens)
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn model(&self) -> Option<&str> {
        self.inner.model()
    }

    async fn chat(&self, request: ChatRequest<'_>) -> anyhow::Result<ChatResponse> {
        self.inner.chat(request).await
    }

    async fn measure_provider_count_tokens(
        &self,
        identity: Option<crate::observability::ContextRequestIdentity>,
        model: &str,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
        request_body: &str,
    ) {
        self.inner
            .measure_provider_count_tokens(identity, model, messages, tools, request_body)
            .await
    }

    fn can_use_provider_count_api(&self, model: &str) -> bool {
        self.inner.exact_tokenizer() == Some("anthropic_count_tokens")
            || matches!(
                resolve_model_capabilities(model, None).token_strategy,
                TokenStrategy::ProviderCountApi(ProviderCountApiKind::AnthropicNative)
            )
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
    use crate::observability::{
        current_context, with_context_telemetry, MeasurementKind, MeasurementProvenance,
        VecContextTelemetry,
    };
    use crate::config::{Config, ModelProviderConfig};
    use crate::providers::build_provider_from_config;
    use crate::providers::ProviderTimeouts;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    #[test]
    fn anthropic_model_identity_uses_configured_model() {
        let provider = AnthropicProvider::new(
            Some("https://example.test/v1"),
            None,
            "claude-sonnet-4-5",
            0.0,
            None,
            ProviderTimeouts::default(),
        );
        assert_eq!(provider.name(), "anthropic");
        assert_eq!(provider.model(), Some("claude-sonnet-4-5"));
    }

    async fn serve_count_response(body: serde_json::Value, status: u16) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let Ok((mut socket, _)) = listener.accept().await else { return };
            let mut buf = [0u8; 4096];
            let _ = socket.read(&mut buf).await;
            let body = body.to_string();
            let response = format!(
                "HTTP/1.1 {} X
Content-Type: application/json
Content-Length: {}
Connection: close

{}",
                status, body.len(), body
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
        });
        format!("http://{addr}")
    }

    async fn serve_chat_and_count(
        count_status: u16,
        count_delay: Duration,
    ) -> (String, Arc<Mutex<Vec<(String, String)>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        tokio::spawn(async move {
            loop {
                let Ok(Ok((mut socket, _))) =
                    tokio::time::timeout(Duration::from_millis(500), listener.accept()).await
                else {
                    break;
                };
                let recorded = Arc::clone(&recorded);
                tokio::spawn(async move {
                    let mut bytes = Vec::new();
                    let mut chunk = [0u8; 4096];
                    let header_end;
                    loop {
                        let read = socket.read(&mut chunk).await.unwrap_or(0);
                        if read == 0 {
                            return;
                        }
                        bytes.extend_from_slice(&chunk[..read]);
                        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                            header_end = index + 4;
                            break;
                        }
                    }
                    let headers = String::from_utf8_lossy(&bytes[..header_end]).to_string();
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                        .unwrap_or(0);
                    while bytes.len() < header_end + content_length {
                        let read = socket.read(&mut chunk).await.unwrap_or(0);
                        if read == 0 {
                            break;
                        }
                        bytes.extend_from_slice(&chunk[..read]);
                    }
                    let path = headers
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("")
                        .to_string();
                    let body = String::from_utf8_lossy(
                        &bytes[header_end..bytes.len().min(header_end + content_length)],
                    )
                    .to_string();
                    recorded.lock().unwrap().push((path.clone(), body));

                    let (status, response_body) = if path.ends_with("/v1/messages/count_tokens") {
                        tokio::time::sleep(count_delay).await;
                        (count_status, json!({"input_tokens": 4812}).to_string())
                    } else {
                        (
                            200,
                            json!({
                                "choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],
                                "usage":{"prompt_tokens":123,"completion_tokens":1}
                            })
                            .to_string(),
                        )
                    };
                    let response = format!(
                        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
                        response_body.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                });
            }
        });
        (format!("http://{addr}"), requests)
    }

    async fn wait_for_count_request(requests: &Arc<Mutex<Vec<(String, String)>>>) {
        for _ in 0..40 {
            if requests
                .lock()
                .unwrap()
                .iter()
                .any(|(path, _)| path.ends_with("/v1/messages/count_tokens"))
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn native_count_success_returns_input_tokens() {
        let base = serve_count_response(json!({"input_tokens": 4812}), 200).await;
        let provider = AnthropicProvider::new(Some(&base), Some("key"), "claude-sonnet-4-5", 0.0, None, ProviderTimeouts::default());
        let tokens = provider.count_tokens_native("claude-sonnet-4-5", &[], &[], std::time::Duration::from_secs(2)).await;
        assert_eq!(tokens, Some(4812));
    }

    #[tokio::test]
    async fn native_count_failure_returns_none() {
        let base = serve_count_response(json!({"error": "bad"}), 500).await;
        let provider = AnthropicProvider::new(Some(&base), Some("key"), "claude-sonnet-4-5", 0.0, None, ProviderTimeouts::default());
        let tokens = provider.count_tokens_native("claude-sonnet-4-5", &[], &[], std::time::Duration::from_secs(2)).await;
        assert_eq!(tokens, None);
    }

    #[tokio::test]
    async fn production_trusted_path_schedules_count_and_keeps_provider_actual_separate() {
        let (base, requests) = serve_chat_and_count(200, Duration::ZERO).await;
        let mut config = Config::default();
        config.default_provider = Some("anthropic".into());
        config.default_model = Some("claude-sonnet-4-5".into());
        config.model_providers.insert(
            "anthropic".into(),
            ModelProviderConfig {
                base_url: Some(base),
                default_model: Some("claude-sonnet-4-5".into()),
                models: vec!["claude-sonnet-4-5".into()],
                enabled: true,
                ..ModelProviderConfig::default()
            },
        );
        let provider = build_provider_from_config(&config);
        let sink = Arc::new(VecContextTelemetry::new());
        let messages = vec![ChatMessage::user("hello")];
        let response = with_context_telemetry(
            Some("session-count".into()),
            Some("run-count".into()),
            "anthropic",
            "claude-sonnet-4-5",
            sink.clone(),
            provider.chat(ChatRequest {
                messages: &messages,
                tools: None,
                request_max_output_tokens: None,
            }),
        )
        .await
        .expect("normal chat succeeds");
        assert_eq!(response.text.as_deref(), Some("ok"));
        wait_for_count_request(&requests).await;
        for _ in 0..40 {
            if sink.snapshots().iter().any(|snapshot| {
                snapshot.measurement_provenance == MeasurementProvenance::ProviderCountApi
            }) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let snapshots = sink.snapshots();
        let count = snapshots
            .iter()
            .find(|snapshot| {
                snapshot.measurement_provenance == MeasurementProvenance::ProviderCountApi
            })
            .expect("native count snapshot emitted");
        assert_eq!(count.estimated_input_tokens, 4812);
        assert!(count.measurement_exact);
        let actual = snapshots
            .iter()
            .find(|snapshot| snapshot.measurement_kind == MeasurementKind::ProviderActual)
            .expect("provider actual remains a distinct snapshot");
        assert_eq!(actual.provider_actual_input_tokens, Some(123));
        assert_eq!(sink.events().len(), 0, "count emits no lifecycle events");
    }

    #[tokio::test]
    async fn untrusted_claude_alias_schedules_zero_native_counts() {
        let (base, requests) = serve_chat_and_count(200, Duration::ZERO).await;
        let provider = AnthropicProvider::new(
            Some(&base),
            None,
            "claude-custom",
            0.0,
            None,
            ProviderTimeouts::default(),
        );
        let messages = vec![ChatMessage::user("hello")];
        provider
            .chat(ChatRequest {
                messages: &messages,
                tools: None,
                request_max_output_tokens: None,
            })
            .await
            .expect("normal chat succeeds");
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(requests
            .lock()
            .unwrap()
            .iter()
            .all(|(path, _)| !path.ends_with("/v1/messages/count_tokens")));
    }

    #[tokio::test]
    async fn count_failure_keeps_safety_estimate_and_normal_chat_continues() {
        let (base, requests) = serve_chat_and_count(500, Duration::ZERO).await;
        let provider = AnthropicProvider::new(
            Some(&base),
            None,
            "claude-sonnet-4-5",
            0.0,
            None,
            ProviderTimeouts::default(),
        )
        .with_anthropic_count_trusted(true);
        let sink = Arc::new(VecContextTelemetry::new());
        let messages = vec![ChatMessage::user("hello")];
        let response = with_context_telemetry(
            Some("session-failure".into()),
            Some("run-failure".into()),
            "anthropic",
            "claude-sonnet-4-5",
            sink.clone(),
            provider.chat(ChatRequest {
                messages: &messages,
                tools: None,
                request_max_output_tokens: None,
            }),
        )
        .await
        .expect("count failure cannot fail normal chat");
        assert_eq!(response.text.as_deref(), Some("ok"));
        wait_for_count_request(&requests).await;
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(sink.snapshots().iter().any(|snapshot| {
            snapshot.measurement_provenance == MeasurementProvenance::SafetyEstimate
        }));
        assert!(sink.snapshots().iter().all(|snapshot| {
            snapshot.measurement_provenance != MeasurementProvenance::ProviderCountApi
        }));
        assert!(sink.events().is_empty());
    }

    #[tokio::test]
    async fn unsupported_logical_content_falls_back_without_native_request() {
        let (base, requests) = serve_chat_and_count(200, Duration::ZERO).await;
        let provider = AnthropicProvider::new(
            Some(&base),
            None,
            "claude-sonnet-4-5",
            0.0,
            None,
            ProviderTimeouts::default(),
        )
        .with_anthropic_count_trusted(true);
        let sink = Arc::new(VecContextTelemetry::new());
        let messages = vec![ChatMessage::user_with_images(
            "look",
            vec!["data:image/png;base64,AAAA".into()],
        )];
        with_context_telemetry(
            Some("session-unsupported".into()),
            Some("run-unsupported".into()),
            "anthropic",
            "claude-sonnet-4-5",
            sink.clone(),
            provider.chat(ChatRequest {
                messages: &messages,
                tools: None,
                request_max_output_tokens: None,
            }),
        )
        .await
        .expect("unsupported count content cannot block normal chat");
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert!(requests
            .lock()
            .unwrap()
            .iter()
            .all(|(path, _)| !path.ends_with("/v1/messages/count_tokens")));
        assert!(sink.snapshots().iter().any(|snapshot| {
            snapshot.measurement_provenance == MeasurementProvenance::SafetyEstimate
                && !snapshot.measurement_exact
        }));
    }

    #[tokio::test]
    async fn stale_delayed_native_count_cannot_become_current() {
        let (base, _requests) = serve_chat_and_count(200, Duration::from_millis(120)).await;
        let provider = AnthropicProvider::new(
            Some(&base),
            None,
            "claude-sonnet-4-5",
            0.0,
            None,
            ProviderTimeouts::default(),
        )
        .with_anthropic_count_trusted(true);
        let sink = Arc::new(VecContextTelemetry::new());
        let messages = vec![ChatMessage::user("hello")];
        with_context_telemetry(
            Some("session-stale-count".into()),
            Some("run-stale-count".into()),
            "anthropic",
            "claude-sonnet-4-5",
            sink.clone(),
            async {
                provider
                    .chat(ChatRequest {
                        messages: &messages,
                        tools: None,
                        request_max_output_tokens: None,
                    })
                    .await?;
                current_context()
                    .expect("context remains installed")
                    .allocate_request_identity();
                tokio::time::sleep(Duration::from_millis(180)).await;
                Ok::<_, anyhow::Error>(())
            },
        )
        .await
        .unwrap();
        assert!(sink.snapshots().iter().all(|snapshot| {
            snapshot.measurement_provenance != MeasurementProvenance::ProviderCountApi
        }));
    }

    #[test]
    fn registry_requires_native_capability_for_count_strategy() {
        use crate::providers::model_capabilities::{resolve_model_capabilities, TokenStrategy};
        let caps = resolve_model_capabilities("claude-sonnet-4-5", None);
        assert!(matches!(caps.token_strategy, TokenStrategy::ProviderCountApi(_)));

        for alias in ["claude-custom", "claude-opus-proxy"] {
            let caps = resolve_model_capabilities(alias, None);
            assert_eq!(caps.token_strategy, TokenStrategy::Unavailable);
        }
    }
}
