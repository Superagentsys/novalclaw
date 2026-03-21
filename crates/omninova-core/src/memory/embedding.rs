//! Embedding Service for L3 Semantic Memory
//!
//! This module provides the `EmbeddingService` for generating vector embeddings
//! from text content. It wraps around the Provider trait's embeddings method
//! and provides a simplified interface for semantic memory operations.
//!
//! [Source: Story 5.3 - L3 Semantic Memory Layer]

use crate::providers::{EmbeddingRequest, Provider};
use std::sync::Arc;

/// Default embedding dimension for text-embedding-3-small
pub const DEFAULT_EMBEDDING_DIM: usize = 1536;

/// Default embedding model for OpenAI
pub const DEFAULT_OPENAI_EMBEDDING_MODEL: &str = "text-embedding-3-small";

/// Default embedding model for Ollama
pub const DEFAULT_OLLAMA_EMBEDDING_MODEL: &str = "nomic-embed-text";

/// Service for generating text embeddings.
///
/// Wraps a Provider to generate vector embeddings for semantic memory operations.
/// Supports both cloud providers (OpenAI) and local providers (Ollama).
pub struct EmbeddingService {
    /// The provider to use for generating embeddings
    provider: Arc<dyn Provider>,
    /// The embedding model to use
    model: Option<String>,
    /// Expected embedding dimension (for validation)
    dimension: usize,
}

impl EmbeddingService {
    /// Create a new embedding service with the given provider.
    ///
    /// # Arguments
    /// * `provider` - The provider to use for generating embeddings
    /// * `model` - Optional model override (uses provider default if None)
    /// * `dimension` - Expected embedding dimension
    pub fn new(provider: Arc<dyn Provider>, model: Option<String>, dimension: usize) -> Self {
        Self {
            provider,
            model,
            dimension,
        }
    }

    /// Generate an embedding for a single text.
    ///
    /// # Arguments
    /// * `text` - The text to embed
    ///
    /// # Returns
    /// The embedding vector as `Vec<f32>`.
    ///
    /// # Errors
    /// Returns an error if the provider doesn't support embeddings or the API call fails.
    pub async fn generate_embedding(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let request = if let Some(model) = self.model.as_deref() {
            EmbeddingRequest::new(text).with_model(model)
        } else {
            EmbeddingRequest::new(text)
        };

        let response = self.provider.embeddings(request).await?;

        // Validate dimension if known
        if response.dimension() != self.dimension {
            tracing::warn!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.dimension,
                response.dimension()
            );
        }

        Ok(response.embedding)
    }

    /// Generate embeddings for multiple texts in batch.
    ///
    /// This method calls the embedding API for each text individually.
    /// For large batches, consider using a provider that supports batch embedding natively.
    ///
    /// # Arguments
    /// * `texts` - The texts to embed
    ///
    /// # Returns
    /// A vector of embedding vectors.
    ///
    /// # Errors
    /// Returns an error if any embedding generation fails.
    pub async fn generate_embeddings(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        let mut embeddings = Vec::with_capacity(texts.len());

        for text in texts {
            let embedding = self.generate_embedding(text).await?;
            embeddings.push(embedding);
        }

        Ok(embeddings)
    }

    /// Check if the underlying provider supports embeddings.
    pub fn supports_embeddings(&self) -> bool {
        self.provider.supports_embeddings()
    }

    /// Get the expected embedding dimension.
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Get the model being used for embeddings.
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// Get the provider name.
    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }
}

/// Compute cosine similarity between two vectors.
///
/// Returns a value between -1.0 and 1.0, where 1.0 means identical.
/// Returns 0.0 if either vector has zero magnitude.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        tracing::warn!(
            "Vector dimension mismatch in cosine_similarity: {} vs {}",
            a.len(),
            b.len()
        );
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_partial() {
        let a = vec![1.0, 1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        // cos(45deg) = sqrt(2)/2 ≈ 0.707
        assert!((sim - 0.7071068).abs() < 1e-5);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_dimension_mismatch() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 1e-6);
    }
}