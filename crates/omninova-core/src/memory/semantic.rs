//! Semantic memory: keyword search fused with embedding similarity.
//!
//! Wraps [`SqliteMemory`] because vectors need durable storage next to the
//! entries themselves. Embedding failures degrade to plain keyword recall
//! rather than failing the write or the search, so a misconfigured or offline
//! embedding endpoint never breaks the agent.

use crate::memory::embedding::{cosine_similarity, EmbeddingClient};
use crate::memory::sqlite_store::SqliteMemory;
use crate::memory::traits::{Memory, MemoryCategory, MemoryEntry};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;

/// Reciprocal-rank-fusion constant. 60 is the value from the original RRF
/// paper and avoids having to normalize two incomparable score scales.
const RRF_K: f64 = 60.0;

/// Upper bound on vectors scanned per recall. Cosine ranking is linear, so this
/// caps the cost of a single search on a large store.
const VECTOR_SCAN_LIMIT: usize = 5_000;

pub struct SemanticMemory {
    inner: Arc<SqliteMemory>,
    embedder: EmbeddingClient,
}

impl SemanticMemory {
    pub fn new(inner: Arc<SqliteMemory>, embedder: EmbeddingClient) -> Self {
        Self { inner, embedder }
    }

    async fn vector_ranking(
        &self,
        query: &str,
        session_id: Option<&str>,
    ) -> Result<Vec<MemoryEntry>> {
        let query_vector = self.embedder.embed(query).await?;
        let mut scored = self
            .inner
            .embedded_entries(session_id, VECTOR_SCAN_LIMIT)?
            .into_iter()
            .map(|(entry, vector)| {
                let score = cosine_similarity(&query_vector, &vector);
                (entry, score)
            })
            .filter(|(_, score)| *score > 0.0)
            .collect::<Vec<_>>();
        scored.sort_by(|(left_entry, left), (right_entry, right)| {
            right
                .total_cmp(left)
                .then_with(|| right_entry.timestamp.cmp(&left_entry.timestamp))
        });
        Ok(scored
            .into_iter()
            .map(|(mut entry, score)| {
                entry.score = Some(score as f64);
                entry
            })
            .collect())
    }
}

#[async_trait]
impl Memory for SemanticMemory {
    fn name(&self) -> &str {
        "sqlite+semantic"
    }

    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> Result<()> {
        self.inner.store(key, content, category, session_id).await?;
        match self.embedder.embed(content).await {
            Ok(vector) => {
                if let Err(error) = self.inner.set_embedding(key, &vector) {
                    warn!("failed to persist embedding for {key}: {error}");
                }
            }
            Err(error) => {
                warn!("embedding unavailable for {key}, keyword search only: {error}");
            }
        }
        Ok(())
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
    ) -> Result<Vec<MemoryEntry>> {
        // Over-fetch both lists so fusion has room to reorder before truncating.
        let fetch = if limit == 0 { 0 } else { limit.saturating_mul(3) };
        let keyword = self.inner.recall(query, fetch, session_id).await?;

        let vector = match self.vector_ranking(query, session_id).await {
            Ok(entries) => entries,
            Err(error) => {
                warn!("semantic recall degraded to keyword search: {error}");
                return Ok(truncate(keyword, limit));
            }
        };

        Ok(truncate(fuse(keyword, vector), limit))
    }

    async fn get(&self, key: &str) -> Result<Option<MemoryEntry>> {
        self.inner.get(key).await
    }

    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> Result<Vec<MemoryEntry>> {
        self.inner.list(category, session_id).await
    }

    async fn forget(&self, key: &str) -> Result<bool> {
        self.inner.forget(key).await
    }

    async fn count(&self) -> Result<usize> {
        self.inner.count().await
    }

    async fn health_check(&self) -> bool {
        self.inner.health_check().await
    }
}

fn truncate(mut entries: Vec<MemoryEntry>, limit: usize) -> Vec<MemoryEntry> {
    if limit > 0 {
        entries.truncate(limit);
    }
    entries
}

/// Reciprocal rank fusion of two ranked lists, keyed by memory key. Entries
/// found by both keyword and vector search accumulate two contributions and
/// therefore outrank entries found by only one.
fn fuse(keyword: Vec<MemoryEntry>, vector: Vec<MemoryEntry>) -> Vec<MemoryEntry> {
    let mut scores: HashMap<String, f64> = HashMap::new();
    let mut entries: HashMap<String, MemoryEntry> = HashMap::new();

    for list in [keyword, vector] {
        for (index, entry) in list.into_iter().enumerate() {
            let rank = (index + 1) as f64;
            *scores.entry(entry.key.clone()).or_insert(0.0) += 1.0 / (RRF_K + rank);
            entries.entry(entry.key.clone()).or_insert(entry);
        }
    }

    let mut fused = entries
        .into_iter()
        .map(|(key, mut entry)| {
            entry.score = scores.get(&key).copied();
            entry
        })
        .collect::<Vec<_>>();
    fused.sort_by(|left, right| {
        right
            .score
            .unwrap_or(0.0)
            .total_cmp(&left.score.unwrap_or(0.0))
            .then_with(|| right.timestamp.cmp(&left.timestamp))
            .then_with(|| left.key.cmp(&right.key))
    });
    fused
}

#[cfg(test)]
mod tests {
    use super::fuse;
    use crate::memory::traits::{MemoryCategory, MemoryEntry};

    fn entry(key: &str, ts: i64) -> MemoryEntry {
        MemoryEntry {
            id: format!("id-{key}"),
            key: key.to_string(),
            content: format!("content {key}"),
            category: MemoryCategory::Core,
            timestamp: ts.to_string(),
            session_id: None,
            score: None,
        }
    }

    #[test]
    fn fusion_deduplicates_by_key() {
        let keyword = vec![entry("a", 100), entry("b", 90)];
        let vector = vec![entry("b", 90), entry("c", 80)];

        let fused = fuse(keyword, vector);

        let keys = fused.iter().map(|e| e.key.as_str()).collect::<Vec<_>>();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"a"));
        assert!(keys.contains(&"b"));
        assert!(keys.contains(&"c"));
    }

    #[test]
    fn entries_in_both_lists_rank_first() {
        // "b" is second in each list; "a" leads only the keyword list.
        let keyword = vec![entry("a", 100), entry("b", 90)];
        let vector = vec![entry("c", 80), entry("b", 90)];

        let fused = fuse(keyword, vector);

        assert_eq!(fused.first().map(|e| e.key.as_str()), Some("b"));
        assert!(fused[0].score.unwrap() > fused[1].score.unwrap());
    }

    #[test]
    fn keyword_only_results_keep_their_order() {
        let keyword = vec![entry("a", 100), entry("b", 90), entry("c", 80)];

        let fused = fuse(keyword, Vec::new());

        let keys = fused.iter().map(|e| e.key.as_str()).collect::<Vec<_>>();
        assert_eq!(keys, vec!["a", "b", "c"]);
    }
}
