//! Embedding client for semantic memory search.
//!
//! Targets the OpenAI-compatible `POST {base_url}/embeddings` shape, which
//! DeepSeek, Qwen, Doubao, Ollama and others also expose, so one client covers
//! every provider the rest of the app already supports.

use crate::config::EmbeddingConfig;
use anyhow::{Context, Result};
use serde_json::json;
use std::time::Duration;

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "text-embedding-3-small";

#[derive(Clone)]
pub struct EmbeddingClient {
    client: reqwest::Client,
    endpoint: String,
    model: String,
    api_key: Option<String>,
}

impl EmbeddingClient {
    /// Build a client, or `None` when the config does not name a provider.
    ///
    /// A missing API key is tolerated because local runtimes (Ollama, vLLM)
    /// accept unauthenticated requests.
    pub fn from_config(config: &EmbeddingConfig) -> Option<Self> {
        let provider = config.provider.as_deref().map(str::trim).unwrap_or("");
        if provider.is_empty() {
            return None;
        }
        let base_url = config
            .base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_BASE_URL)
            .trim_end_matches('/')
            .to_string();
        let model = config
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_MODEL)
            .to_string();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .ok()?;
        Some(Self {
            client,
            endpoint: format!("{base_url}/embeddings"),
            model,
            api_key: config
                .api_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string),
        })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut request = self
            .client
            .post(&self.endpoint)
            .json(&json!({ "model": self.model, "input": text }));
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }

        let response = request
            .send()
            .await
            .context("embedding request failed")?
            .error_for_status()
            .context("embedding endpoint returned an error status")?;
        let body: serde_json::Value = response
            .json()
            .await
            .context("failed to decode embedding response")?;

        let vector = body
            .get("data")
            .and_then(|data| data.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("embedding"))
            .and_then(|values| values.as_array())
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_f64().map(|v| v as f32))
                    .collect::<Vec<f32>>()
            })
            .unwrap_or_default();

        if vector.is_empty() {
            anyhow::bail!("embedding response contained no vector");
        }
        Ok(vector)
    }
}

/// Cosine similarity in `[-1, 1]`; `0.0` when either side is degenerate.
pub fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0_f32;
    let mut left_norm = 0.0_f32;
    let mut right_norm = 0.0_f32;
    for (a, b) in left.iter().zip(right.iter()) {
        dot += a * b;
        left_norm += a * a;
        right_norm += b * b;
    }
    if left_norm <= 0.0 || right_norm <= 0.0 {
        return 0.0;
    }
    dot / (left_norm.sqrt() * right_norm.sqrt())
}

#[cfg(test)]
mod tests {
    use super::{cosine_similarity, EmbeddingClient};
    use crate::config::EmbeddingConfig;

    #[test]
    fn disabled_without_provider() {
        let config = EmbeddingConfig::default();
        assert!(EmbeddingClient::from_config(&config).is_none());
    }

    #[test]
    fn local_runtime_without_api_key_is_allowed() {
        let config = EmbeddingConfig {
            provider: Some("ollama".into()),
            model: Some("nomic-embed-text".into()),
            api_key: None,
            base_url: Some("http://127.0.0.1:11434/v1/".into()),
        };
        let client = EmbeddingClient::from_config(&config).expect("client");
        assert_eq!(client.model(), "nomic-embed-text");
    }

    #[test]
    fn cosine_handles_identical_and_mismatched_vectors() {
        assert!((cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert_eq!(cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]), 0.0);
        assert_eq!(cosine_similarity(&[1.0], &[1.0, 0.0]), 0.0);
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[0.0, 0.0]), 0.0);
    }
}
