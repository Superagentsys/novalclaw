//! Semantic Memory Store
//!
//! L3 semantic memory storage using vector embeddings for similarity search.
//! Stores embeddings in SQLite blob columns and performs in-memory cosine similarity.
//!
//! [Source: Story 5.3 - L3 语义记忆层实现]

use anyhow::{Context, Result};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::db::pool::{DbPool, DbConnection};
use super::embedding::{EmbeddingService, cosine_similarity};

/// A semantic memory entry with vector embedding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticMemory {
    /// Unique identifier
    pub id: i64,
    /// Reference to the source episodic memory
    pub episodic_memory_id: i64,
    /// The embedding vector
    pub embedding: Vec<f32>,
    /// Dimension of the embedding vector
    pub embedding_dim: usize,
    /// Model used to generate the embedding
    pub embedding_model: String,
    /// Unix timestamp of creation
    pub created_at: i64,
    /// Unix timestamp of last update
    pub updated_at: i64,
}

/// Data for creating a new semantic memory entry
#[derive(Debug, Clone)]
pub struct NewSemanticMemory {
    /// Reference to the source episodic memory
    pub episodic_memory_id: i64,
    /// The embedding vector
    pub embedding: Vec<f32>,
    /// Model used to generate the embedding
    pub embedding_model: String,
}

/// Result of a semantic similarity search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchResult {
    /// The semantic memory entry
    pub memory: SemanticMemory,
    /// Similarity score (0.0 to 1.0)
    pub score: f32,
    /// The original content from episodic memory (if available)
    pub content: Option<String>,
}

/// Statistics about semantic memories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticMemoryStats {
    /// Total number of indexed memories
    pub total_count: usize,
    /// Number of embeddings by model
    pub by_model: std::collections::HashMap<String, usize>,
    /// Average embedding dimension
    pub avg_dimension: f64,
}

/// L3 Semantic Memory Store
///
/// Manages semantic memory storage using vector embeddings in SQLite.
/// Supports similarity search using cosine similarity.
pub struct SemanticMemoryStore {
    db: Arc<DbPool>,
    embedding_service: Arc<EmbeddingService>,
    embedding_dim: usize,
}

impl SemanticMemoryStore {
    /// Create a new semantic memory store.
    ///
    /// # Arguments
    /// * `db` - Database pool for persistence
    /// * `embedding_service` - Service for generating embeddings
    /// * `embedding_dim` - Expected embedding dimension
    pub fn new(
        db: Arc<DbPool>,
        embedding_service: Arc<EmbeddingService>,
        embedding_dim: usize,
    ) -> Self {
        Self {
            db,
            embedding_service,
            embedding_dim,
        }
    }

    /// Store a semantic memory with its embedding.
    ///
    /// # Arguments
    /// * `entry` - The semantic memory entry to store
    ///
    /// # Returns
    /// The ID of the created entry.
    pub async fn create(&self, entry: &NewSemanticMemory) -> Result<i64> {
        // Validate embedding dimension
        if entry.embedding.len() != self.embedding_dim {
            anyhow::bail!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.embedding_dim,
                entry.embedding.len()
            );
        }

        // Serialize embedding to bytes (little-endian f32)
        let embedding_bytes = self.serialize_embedding(&entry.embedding)?;

        let conn = self.db.get()?;
        let now = chrono::Utc::now().timestamp();

        conn.execute(
            r#"
            INSERT INTO memory_embeddings (episodic_memory_id, embedding, embedding_dim, embedding_model, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            rusqlite::params![
                entry.episodic_memory_id,
                embedding_bytes,
                entry.embedding.len() as i32,
                entry.embedding_model,
                now,
                now,
            ],
        ).context("Failed to insert semantic memory")?;

        Ok(conn.last_insert_rowid())
    }

    /// Store a semantic memory by generating embedding from text.
    ///
    /// This method generates an embedding for the given text and stores it.
    ///
    /// # Arguments
    /// * `episodic_memory_id` - Reference to the source episodic memory
    /// * `text` - Text content to embed
    /// * `model` - Embedding model name
    ///
    /// # Returns
    /// The ID of the created entry.
    pub async fn store_with_embedding(
        &self,
        episodic_memory_id: i64,
        text: &str,
        model: &str,
    ) -> Result<i64> {
        let embedding = self.embedding_service.generate_embedding(text).await?;

        let entry = NewSemanticMemory {
            episodic_memory_id,
            embedding,
            embedding_model: model.to_string(),
        };

        self.create(&entry).await
    }

    /// Search for semantically similar memories.
    ///
    /// # Arguments
    /// * `query` - The query text to search for
    /// * `k` - Maximum number of results to return
    /// * `agent_id` - Optional filter by agent ID (via episodic_memories)
    /// * `threshold` - Minimum similarity threshold (0.0 to 1.0)
    ///
    /// # Returns
    /// A list of search results sorted by similarity score (descending).
    pub async fn search_similar(
        &self,
        query: &str,
        k: usize,
        agent_id: Option<i64>,
        threshold: f32,
    ) -> Result<Vec<SemanticSearchResult>> {
        // Generate query embedding
        let query_embedding = self.embedding_service.generate_embedding(query).await?;

        // Get all embeddings from database
        let conn = self.db.get()?;
        let memories = self.get_all_embeddings(&conn, agent_id)?;

        // Calculate similarities
        let mut scored: Vec<_> = memories
            .into_iter()
            .map(|m| {
                let score = cosine_similarity(&query_embedding, &m.embedding);
                (m, score)
            })
            .filter(|(_, score)| *score >= threshold)
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top-k and fetch content from episodic_memories
        let mut results = Vec::with_capacity(k.min(scored.len()));
        for (memory, score) in scored.into_iter().take(k) {
            let content = self.get_episodic_content(&conn, memory.episodic_memory_id)?;
            results.push(SemanticSearchResult {
                memory,
                score,
                content,
            });
        }

        Ok(results)
    }

    /// Get a semantic memory by ID.
    pub fn get(&self, id: i64) -> Result<Option<SemanticMemory>> {
        let conn = self.db.get()?;

        let result = conn.query_row(
            r#"
            SELECT id, episodic_memory_id, embedding, embedding_dim, embedding_model, created_at, updated_at
            FROM memory_embeddings
            WHERE id = ?1
            "#,
            [id],
            |row| self.row_to_semantic_memory(row),
        ).optional()?;

        Ok(result)
    }

    /// Get semantic memory by episodic memory ID.
    pub fn get_by_episodic_id(&self, episodic_memory_id: i64) -> Result<Option<SemanticMemory>> {
        let conn = self.db.get()?;

        let result = conn.query_row(
            r#"
            SELECT id, episodic_memory_id, embedding, embedding_dim, embedding_model, created_at, updated_at
            FROM memory_embeddings
            WHERE episodic_memory_id = ?1
            "#,
            [episodic_memory_id],
            |row| self.row_to_semantic_memory(row),
        ).optional()?;

        Ok(result)
    }

    /// Update the embedding for an existing semantic memory.
    pub async fn update_embedding(&self, id: i64, embedding: Vec<f32>, model: &str) -> Result<bool> {
        if embedding.len() != self.embedding_dim {
            anyhow::bail!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.embedding_dim,
                embedding.len()
            );
        }

        let embedding_bytes = self.serialize_embedding(&embedding)?;
        let conn = self.db.get()?;
        let now = chrono::Utc::now().timestamp();

        let affected = conn.execute(
            r#"
            UPDATE memory_embeddings
            SET embedding = ?2, embedding_model = ?3, updated_at = ?4
            WHERE id = ?1
            "#,
            rusqlite::params![id, embedding_bytes, model, now],
        ).context("Failed to update embedding")?;

        Ok(affected > 0)
    }

    /// Delete a semantic memory by ID.
    pub fn delete(&self, id: i64) -> Result<bool> {
        let conn = self.db.get()?;
        let affected = conn.execute("DELETE FROM memory_embeddings WHERE id = ?1", [id])?;
        Ok(affected > 0)
    }

    /// Delete semantic memory by episodic memory ID.
    pub fn delete_by_episodic_id(&self, episodic_memory_id: i64) -> Result<bool> {
        let conn = self.db.get()?;
        let affected = conn.execute(
            "DELETE FROM memory_embeddings WHERE episodic_memory_id = ?1",
            [episodic_memory_id],
        )?;
        Ok(affected > 0)
    }

    /// Count total semantic memories.
    pub fn count(&self) -> Result<usize> {
        let conn = self.db.get()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM memory_embeddings", [], |row| {
            row.get(0)
        })?;
        Ok(count as usize)
    }

    /// Get statistics about semantic memories.
    pub fn stats(&self) -> Result<SemanticMemoryStats> {
        let conn = self.db.get()?;

        let total_count: i64 = conn.query_row("SELECT COUNT(*) FROM memory_embeddings", [], |row| {
            row.get(0)
        })?;

        // Get count by model
        let mut by_model = std::collections::HashMap::new();
        let mut stmt = conn.prepare(
            "SELECT embedding_model, COUNT(*) FROM memory_embeddings GROUP BY embedding_model"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (model, count) = row?;
            by_model.insert(model, count as usize);
        }

        // Get average dimension
        let avg_dimension: f64 = conn.query_row(
            "SELECT AVG(embedding_dim) FROM memory_embeddings",
            [],
            |row| row.get::<_, Option<f64>>(0),
        )?.unwrap_or(0.0);

        Ok(SemanticMemoryStats {
            total_count: total_count as usize,
            by_model,
            avg_dimension,
        })
    }

    /// Rebuild the semantic index for an agent.
    ///
    /// This clears existing embeddings and regenerates them from episodic memories.
    pub async fn rebuild_index(&self, agent_id: i64, model: &str) -> Result<usize> {
        // Get all episodic memories for this agent
        let conn = self.db.get()?;
        let episodic_memories = self.get_episodic_memories_by_agent(&conn, agent_id)?;

        // Delete existing embeddings for these memories
        for episodic in &episodic_memories {
            self.delete_by_episodic_id(episodic.id)?;
        }

        // Generate new embeddings
        let mut count = 0;
        for episodic in episodic_memories {
            match self.store_with_embedding(episodic.id, &episodic.content, model).await {
                Ok(_) => count += 1,
                Err(e) => {
                    tracing::warn!(
                        "Failed to index episodic memory {}: {}",
                        episodic.id,
                        e
                    );
                }
            }
        }

        Ok(count)
    }

    // ==================== Private Helper Methods ====================

    /// Serialize embedding vector to bytes (little-endian f32).
    fn serialize_embedding(&self, embedding: &[f32]) -> Result<Vec<u8>> {
        let mut bytes = Vec::with_capacity(embedding.len() * 4);
        for &f in embedding {
            bytes.extend_from_slice(&f.to_le_bytes());
        }
        Ok(bytes)
    }

    /// Deserialize embedding vector from bytes.
    fn deserialize_embedding(&self, bytes: &[u8]) -> Result<Vec<f32>> {
        if bytes.len() % 4 != 0 {
            anyhow::bail!("Invalid embedding bytes length: {}", bytes.len());
        }

        let mut embedding = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks_exact(4) {
            let f = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            embedding.push(f);
        }
        Ok(embedding)
    }

    /// Convert a database row to a SemanticMemory.
    fn row_to_semantic_memory(&self, row: &rusqlite::Row) -> Result<SemanticMemory, rusqlite::Error> {
        let id: i64 = row.get(0)?;
        let episodic_memory_id: i64 = row.get(1)?;
        let embedding_bytes: Vec<u8> = row.get(2)?;
        let embedding_dim: i32 = row.get(3)?;
        let embedding_model: String = row.get(4)?;
        let created_at: i64 = row.get(5)?;
        let updated_at: i64 = row.get(6)?;

        let embedding = self.deserialize_embedding(&embedding_bytes)
            .map_err(|_| rusqlite::Error::InvalidQuery)?;

        Ok(SemanticMemory {
            id,
            episodic_memory_id,
            embedding,
            embedding_dim: embedding_dim as usize,
            embedding_model,
            created_at,
            updated_at,
        })
    }

    /// Get all embeddings from the database.
    fn get_all_embeddings(&self, conn: &DbConnection, agent_id: Option<i64>) -> Result<Vec<SemanticMemory>> {
        let memories = if let Some(agent_id) = agent_id {
            // Join with episodic_memories to filter by agent
            let mut stmt = conn.prepare(
                r#"
                SELECT me.id, me.episodic_memory_id, me.embedding, me.embedding_dim,
                       me.embedding_model, me.created_at, me.updated_at
                FROM memory_embeddings me
                INNER JOIN episodic_memories em ON me.episodic_memory_id = em.id
                WHERE em.agent_id = ?1
                "#
            )?;
            let rows = stmt.query_map([agent_id], |row| self.row_to_semantic_memory(row))?;
            rows.collect::<Result<Vec<_>, _>>()
                .context("Failed to query embeddings by agent")?
        } else {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, episodic_memory_id, embedding, embedding_dim, embedding_model, created_at, updated_at
                FROM memory_embeddings
                "#
            )?;
            let rows = stmt.query_map([], |row| self.row_to_semantic_memory(row))?;
            rows.collect::<Result<Vec<_>, _>>()
                .context("Failed to query all embeddings")?
        };

        Ok(memories)
    }

    /// Get content from episodic memory.
    fn get_episodic_content(&self, conn: &DbConnection, episodic_memory_id: i64) -> Result<Option<String>> {
        let result = conn.query_row(
            "SELECT content FROM episodic_memories WHERE id = ?1",
            [episodic_memory_id],
            |row| row.get::<_, String>(0),
        ).optional()?;

        Ok(result)
    }

    /// Get episodic memories by agent.
    fn get_episodic_memories_by_agent(
        &self,
        conn: &DbConnection,
        agent_id: i64,
    ) -> Result<Vec<super::episodic::EpisodicMemory>> {
        let mut stmt = conn.prepare(
            r#"
            SELECT id, agent_id, session_id, content, importance, is_marked, metadata, created_at
            FROM episodic_memories
            WHERE agent_id = ?1
            ORDER BY created_at DESC
            "#
        )?;

        let rows = stmt.query_map([agent_id], |row| {
            Ok(super::episodic::EpisodicMemory {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                session_id: row.get(2)?,
                content: row.get(3)?,
                importance: row.get(4)?,
                is_marked: row.get::<_, i64>(5)? != 0,
                metadata: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;

        let memories = rows.collect::<Result<Vec<_>, _>>()
            .context("Failed to query episodic memories")?;

        Ok(memories)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::pool::{create_pool, DbPoolConfig};
    use crate::db::migrations::create_builtin_runner;
    use crate::memory::episodic::{EpisodicMemoryStore, NewEpisodicMemory};
    use crate::providers::{EmbeddingResponse, Provider, ChatRequest, ChatResponse};
    use async_trait::async_trait;
    use tempfile::tempdir;

    /// Mock provider for testing embeddings
    struct MockEmbeddingProvider {
        dimension: usize,
    }

    impl MockEmbeddingProvider {
        fn new(dimension: usize) -> Self {
            Self { dimension }
        }
    }

    #[async_trait]
    impl Provider for MockEmbeddingProvider {
        fn name(&self) -> &str {
            "mock-embedding"
        }

        async fn chat(&self, _request: ChatRequest<'_>) -> anyhow::Result<ChatResponse> {
            anyhow::bail!("Mock provider does not support chat")
        }

        async fn embeddings(&self, request: crate::providers::EmbeddingRequest<'_>) -> anyhow::Result<EmbeddingResponse> {
            // Generate a deterministic embedding based on text hash
            let mut embedding = vec![0.0f32; self.dimension];
            let bytes = request.text.as_bytes();
            for (i, &byte) in bytes.iter().cycle().take(self.dimension).enumerate() {
                embedding[i] = (byte as f32) / 255.0;
            }
            // Normalize
            let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for e in &mut embedding {
                    *e /= norm;
                }
            }
            Ok(EmbeddingResponse {
                embedding,
                model: "mock-model".to_string(),
                usage: None,
            })
        }

        async fn health_check(&self) -> bool {
            true
        }

        fn supports_embeddings(&self) -> bool {
            true
        }
    }

    fn create_test_pool() -> Arc<DbPool> {
        let dir = tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test_semantic.db");

        let pool = create_pool(&db_path, DbPoolConfig::default()).expect("Failed to create pool");

        // Run migrations
        let conn = pool.get().expect("Failed to get connection");
        create_builtin_runner().run(&conn).expect("Failed to run migrations");

        // Create test agents (needed for foreign key constraints)
        conn.execute(
            "INSERT OR IGNORE INTO agents (id, name, agent_uuid, status) VALUES (1, 'Test Agent 1', 'test-uuid-1', 'active')",
            [],
        ).expect("Failed to create test agent 1");

        // Create test sessions (needed for foreign key constraints)
        conn.execute(
            "INSERT OR IGNORE INTO sessions (id, agent_id, title) VALUES (1, 1, 'Test Session 1')",
            [],
        ).expect("Failed to create test session 1");

        Arc::new(pool)
    }

    fn setup_test_store() -> (SemanticMemoryStore, EpisodicMemoryStore) {
        let db = create_test_pool();

        let provider = Arc::new(MockEmbeddingProvider::new(3));
        let embedding_service = Arc::new(EmbeddingService::new(provider, None, 3));

        let semantic_store = SemanticMemoryStore::new(db.clone(), embedding_service, 3);
        let episodic_store = EpisodicMemoryStore::new(db);

        (semantic_store, episodic_store)
    }

    #[tokio::test]
    async fn test_create_and_get_semantic_memory() {
        let (semantic_store, episodic_store) = setup_test_store();

        // Create an episodic memory first
        let episodic_id = episodic_store
            .create(&NewEpisodicMemory::new(1, None, "Test content".to_string(), 5, false, None).unwrap())
            .expect("Failed to create episodic memory");

        // Create semantic memory
        let embedding = vec![0.1, 0.2, 0.3];
        let entry = NewSemanticMemory {
            episodic_memory_id: episodic_id,
            embedding,
            embedding_model: "test-model".to_string(),
        };

        let id = semantic_store.create(&entry).await.expect("Failed to create semantic memory");
        assert!(id > 0);

        // Get it back
        let result = semantic_store.get(id).expect("Failed to get semantic memory");
        assert!(result.is_some());
        let memory = result.unwrap();
        assert_eq!(memory.episodic_memory_id, episodic_id);
        assert_eq!(memory.embedding_dim, 3);
        assert_eq!(memory.embedding_model, "test-model");
    }

    #[tokio::test]
    async fn test_dimension_validation() {
        let (semantic_store, episodic_store) = setup_test_store();

        let episodic_id = episodic_store
            .create(&NewEpisodicMemory::new(1, None, "Test".to_string(), 5, false, None).unwrap())
            .expect("Failed to create episodic memory");

        // Wrong dimension
        let entry = NewSemanticMemory {
            episodic_memory_id: episodic_id,
            embedding: vec![0.1, 0.2], // Wrong dimension (should be 3)
            embedding_model: "test".to_string(),
        };

        let result = semantic_store.create(&entry).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_store_with_embedding() {
        let (semantic_store, episodic_store) = setup_test_store();

        let episodic_id = episodic_store
            .create(&NewEpisodicMemory::new(1, None, "Hello world".to_string(), 5, false, None).unwrap())
            .expect("Failed to create episodic memory");

        let id = semantic_store
            .store_with_embedding(episodic_id, "Hello world", "mock-model")
            .await
            .expect("Failed to store with embedding");

        let memory = semantic_store.get(id).expect("Failed to get").unwrap();
        assert_eq!(memory.embedding_dim, 3);
    }

    #[tokio::test]
    async fn test_search_similar() {
        let (semantic_store, episodic_store) = setup_test_store();

        // Create episodic memories
        let id1 = episodic_store
            .create(&NewEpisodicMemory::new(1, None, "Apple is a fruit".to_string(), 5, false, None).unwrap())
            .expect("Failed to create episodic");

        let id2 = episodic_store
            .create(&NewEpisodicMemory::new(1, None, "Banana is yellow".to_string(), 5, false, None).unwrap())
            .expect("Failed to create episodic");

        // Store embeddings
        semantic_store.store_with_embedding(id1, "Apple is a fruit", "mock-model").await.unwrap();
        semantic_store.store_with_embedding(id2, "Banana is yellow", "mock-model").await.unwrap();

        // Search
        let results = semantic_store
            .search_similar("Apple", 10, None, 0.0)
            .await
            .expect("Failed to search");

        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_delete() {
        let (semantic_store, episodic_store) = setup_test_store();

        let episodic_id = episodic_store
            .create(&NewEpisodicMemory::new(1, None, "Test".to_string(), 5, false, None).unwrap())
            .expect("Failed to create episodic");

        let id = semantic_store
            .store_with_embedding(episodic_id, "Test", "mock-model")
            .await
            .expect("Failed to store");

        assert!(semantic_store.delete(id).expect("Failed to delete"));
        assert!(semantic_store.get(id).expect("Failed to get").is_none());
    }

    #[tokio::test]
    async fn test_stats() {
        let (semantic_store, episodic_store) = setup_test_store();

        let id1 = episodic_store
            .create(&NewEpisodicMemory::new(1, None, "Test 1".to_string(), 5, false, None).unwrap())
            .expect("Failed to create episodic");

        let id2 = episodic_store
            .create(&NewEpisodicMemory::new(1, None, "Test 2".to_string(), 5, false, None).unwrap())
            .expect("Failed to create episodic");

        semantic_store.store_with_embedding(id1, "Test 1", "mock-model").await.unwrap();
        semantic_store.store_with_embedding(id2, "Test 2", "mock-model").await.unwrap();

        let stats = semantic_store.stats().expect("Failed to get stats");
        assert_eq!(stats.total_count, 2);
    }
}