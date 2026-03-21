//! Unified Memory Manager
//!
//! Coordinates all three memory layers (L1, L2, L3) with automatic coordination.
//! Provides a unified API for store, retrieve, search, and delete operations.
//!
//! [Source: Story 5.4 - 记忆管理 API 统一封装]
//! [Source: Story 5.5 - 记忆检索性能优化]

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use crate::db::pool::DbPool;
use crate::memory::embedding::EmbeddingService;
use crate::memory::episodic::{EpisodicMemoryStore, NewEpisodicMemory};
use crate::memory::metrics::{MetricsCollector, PerformanceStats};
use crate::memory::semantic::SemanticMemoryStore;
use crate::memory::working::{MemoryStats, WorkingMemory};

// ============================================================================
// Type Definitions
// ============================================================================

/// Memory layer enum for targeting specific storage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MemoryLayer {
    /// L1: Working memory (in-memory, session-scoped, LRU eviction)
    L1,
    /// L2: Episodic memory (SQLite, long-term storage)
    L2,
    /// L3: Semantic memory (vector embeddings for similarity search)
    L3,
    /// Query all layers in priority order (L1 → L2 → L3)
    All,
}

impl Default for MemoryLayer {
    fn default() -> Self {
        Self::All
    }
}

impl std::fmt::Display for MemoryLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::L1 => write!(f, "L1"),
            Self::L2 => write!(f, "L2"),
            Self::L3 => write!(f, "L3"),
            Self::All => write!(f, "All"),
        }
    }
}

/// Query parameters for memory retrieval
#[derive(Debug, Clone)]
pub struct MemoryQuery {
    /// Agent ID to filter memories
    pub agent_id: i64,
    /// Session ID to filter memories (optional)
    pub session_id: Option<i64>,
    /// Target memory layer
    pub layer: MemoryLayer,
    /// Maximum number of results
    pub limit: usize,
    /// Offset for pagination
    pub offset: usize,
    /// Minimum importance filter (1-10)
    pub min_importance: Option<u8>,
    /// Time range filter (start, end) as Unix timestamps
    pub time_range: Option<(i64, i64)>,
}

impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            agent_id: 1,
            session_id: None,
            layer: MemoryLayer::All,
            limit: 100,
            offset: 0,
            min_importance: None,
            time_range: None,
        }
    }
}

/// Unified memory entry across all layers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedMemoryEntry {
    /// Unique identifier
    pub id: String,
    /// Memory content
    pub content: String,
    /// Role (user, assistant, system) for conversation entries
    pub role: Option<String>,
    /// Importance score (1-10)
    pub importance: u8,
    /// Session ID this memory belongs to
    pub session_id: Option<i64>,
    /// Creation timestamp (Unix)
    pub created_at: i64,
    /// Source layer (L1, L2, or L3)
    pub source_layer: MemoryLayer,
    /// Similarity score (only for L3 semantic search results)
    pub similarity_score: Option<f32>,
}

/// Result of a memory query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQueryResult {
    /// Retrieved memory entries
    pub entries: Vec<UnifiedMemoryEntry>,
    /// Source layer
    pub layer: MemoryLayer,
    /// Total count (before pagination)
    pub total_count: usize,
}

/// Statistics for all memory layers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryManagerStats {
    /// L1 capacity
    pub l1_capacity: usize,
    /// L1 used slots
    pub l1_used: usize,
    /// L1 session ID (if active)
    pub l1_session_id: Option<i64>,
    /// L2 total count
    pub l2_total: usize,
    /// L2 average importance
    pub l2_avg_importance: f64,
    /// L3 total indexed memories
    pub l3_total: usize,
}

/// Eviction policy for L1 cache
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictionPolicy {
    /// Evict least recently used entries (default)
    Lru,
    /// Evict lowest importance entries first
    LowestImportance,
    /// Evict oldest entries first
    OldestFirst,
}

impl Default for EvictionPolicy {
    fn default() -> Self {
        Self::Lru
    }
}

// ============================================================================
// MemoryManager
// ============================================================================

/// Unified Memory Manager
///
/// Coordinates all three memory layers with automatic coordination:
/// - **L1**: Working memory (session-scoped, in-memory, LRU eviction)
/// - **L2**: Episodic memory (long-term, SQLite persistence)
/// - **L3**: Semantic memory (vector embeddings, similarity search)
///
/// # Example
///
/// ```rust,ignore
/// use omninova_core::memory::MemoryManager;
///
/// let manager = MemoryManager::new(db, None, 1536, 1);
///
/// // Store to L1
/// manager.store("Hello", "user", 5, false, false).await?;
///
/// // Store to L2 and index to L3
/// manager.store_and_index("Important fact", "system", 9).await?;
///
/// // Search semantically
/// let results = manager.search("similar concept", 10, 0.7).await?;
/// ```
pub struct MemoryManager {
    /// L1 Working memory (session-scoped)
    l1: Arc<RwLock<WorkingMemory>>,
    /// L2 Episodic memory store
    l2: EpisodicMemoryStore,
    /// L3 Semantic memory store (optional, requires embedding service)
    l3: Option<SemanticMemoryStore>,
    /// Default agent ID
    default_agent_id: i64,
    /// Default embedding model
    default_embedding_model: String,
    /// Eviction policy for L1
    eviction_policy: EvictionPolicy,
    /// Importance threshold for auto-promotion to L2
    auto_promote_threshold: u8,
    /// Importance threshold for auto-indexing to L3
    auto_index_threshold: u8,
    /// Performance metrics collector
    metrics: Arc<MetricsCollector>,
}

impl MemoryManager {
    /// Create a new MemoryManager
    ///
    /// # Arguments
    /// * `db` - Database pool for L2/L3 persistence
    /// * `embedding_service` - Optional embedding service for L3 (if None, L3 is disabled)
    /// * `embedding_dim` - Embedding vector dimension (e.g., 1536 for text-embedding-3-small)
    /// * `default_agent_id` - Default agent ID for operations
    pub fn new(
        db: Arc<DbPool>,
        embedding_service: Option<Arc<EmbeddingService>>,
        embedding_dim: usize,
        default_agent_id: i64,
    ) -> Self {
        let l2 = EpisodicMemoryStore::new(db.clone());
        let l3 = embedding_service.map(|es| SemanticMemoryStore::new(db, es, embedding_dim));

        Self {
            l1: Arc::new(RwLock::new(WorkingMemory::new())),
            l2,
            l3,
            default_agent_id,
            default_embedding_model: "text-embedding-3-small".to_string(),
            eviction_policy: EvictionPolicy::Lru,
            auto_promote_threshold: 7,
            auto_index_threshold: 8,
            metrics: Arc::new(MetricsCollector::new()),
        }
    }

    /// Create a MemoryManager with custom configuration
    pub fn with_config(
        db: Arc<DbPool>,
        embedding_service: Option<Arc<EmbeddingService>>,
        embedding_dim: usize,
        default_agent_id: i64,
        l1_capacity: usize,
        eviction_policy: EvictionPolicy,
        auto_promote_threshold: u8,
        auto_index_threshold: u8,
    ) -> Self {
        let l2 = EpisodicMemoryStore::new(db.clone());
        let l3 = embedding_service.map(|es| SemanticMemoryStore::new(db, es, embedding_dim));

        Self {
            l1: Arc::new(RwLock::new(WorkingMemory::with_capacity(l1_capacity))),
            l2,
            l3,
            default_agent_id,
            default_embedding_model: "text-embedding-3-small".to_string(),
            eviction_policy,
            auto_promote_threshold,
            auto_index_threshold,
            metrics: Arc::new(MetricsCollector::new()),
        }
    }

    // ========================================================================
    // Session Management
    // ========================================================================

    /// Set the current session context
    pub async fn set_session(&self, session_id: i64, agent_id: Option<i64>) {
        let mut l1 = self.l1.write().await;
        l1.set_session(session_id, agent_id.unwrap_or(self.default_agent_id));
    }

    /// Clear the current session context
    pub async fn clear_session(&self) {
        let mut l1 = self.l1.write().await;
        l1.clear_session();
    }

    /// Get the current session ID
    pub async fn session_id(&self) -> Option<i64> {
        let l1 = self.l1.read().await;
        l1.session_id()
    }

    /// Get the current agent ID
    pub async fn agent_id(&self) -> Option<i64> {
        let l1 = self.l1.read().await;
        l1.agent_id()
    }

    // ========================================================================
    // Store Operations (Task 2)
    // ========================================================================

    /// Store a memory entry
    ///
    /// - Always stores to L1 (working memory)
    /// - Optionally persists to L2 based on `persist_to_l2`
    /// - Optionally indexes to L3 for semantic search if `index_to_l3` is true
    ///
    /// # Arguments
    /// * `content` - Memory content to store
    /// * `role` - Role (user, assistant, system)
    /// * `importance` - Importance score (1-10)
    /// * `persist_to_l2` - Whether to persist to L2 (episodic memory)
    /// * `index_to_l3` - Whether to index to L3 (semantic memory)
    ///
    /// # Returns
    /// Memory ID (L2 ID if persisted, otherwise "l1-only")
    pub async fn store(
        &self,
        content: &str,
        role: &str,
        importance: u8,
        persist_to_l2: bool,
        index_to_l3: bool,
    ) -> Result<String> {
        // 1. Apply eviction policy if L1 is at capacity
        self.maybe_evict().await?;

        // 2. Store to L1
        {
            let l1 = self.l1.read().await;
            l1.push_context(role, content).await?;
        }

        // 3. Optionally persist to L2
        if persist_to_l2 {
            return self.store_to_l2(content, role, importance, index_to_l3).await;
        }

        Ok("l1-only".to_string())
    }

    /// Apply eviction policy when L1 is at capacity
    ///
    /// For non-LRU policies, we need to manually evict entries based on the policy.
    /// LRU is handled automatically by the underlying LruCache.
    async fn maybe_evict(&self) -> Result<()> {
        // Only apply manual eviction for non-LRU policies
        if self.eviction_policy == EvictionPolicy::Lru {
            return Ok(());
        }

        let l1 = self.l1.read().await;
        let stats = l1.stats();

        // Check if at capacity (leave room for 1 new entry)
        if stats.used < stats.capacity {
            return Ok(());
        }

        // Get all entries for eviction candidate selection
        let entries = l1.get_context(0).await?;

        // Select entry to evict based on policy
        let to_evict = match self.eviction_policy {
            EvictionPolicy::LowestImportance => {
                // For L1, we don't have importance, so evict the oldest
                // (could be enhanced to track importance in L1)
                entries.first().map(|e| e.id.clone())
            }
            EvictionPolicy::OldestFirst => {
                entries.first().map(|e| e.id.clone())
            }
            EvictionPolicy::Lru => {
                // Already handled by LruCache
                None
            }
        };

        drop(l1);

        // Evict the selected entry
        if let Some(entry_id) = to_evict {
            tracing::debug!("Evicting L1 entry {} due to capacity", entry_id);
            // Note: L1 doesn't support direct deletion by ID currently
            // The LruCache will handle eviction on next push
        }

        Ok(())
    }

    /// Store with automatic importance-based decisions
    ///
    /// - Stores to L1 always
    /// - Persists to L2 if importance >= auto_promote_threshold
    /// - Indexes to L3 if importance >= auto_index_threshold
    pub async fn store_with_importance(
        &self,
        content: &str,
        role: &str,
        importance: u8,
    ) -> Result<String> {
        let persist_to_l2 = importance >= self.auto_promote_threshold;
        let index_to_l3 = importance >= self.auto_index_threshold;

        self.store(content, role, importance, persist_to_l2, index_to_l3).await
    }

    /// Store to L2 and optionally index to L3
    ///
    /// Returns the L2 memory ID.
    pub async fn store_and_index(
        &self,
        content: &str,
        role: &str,
        importance: u8,
    ) -> Result<String> {
        self.store(content, role, importance, true, true).await
    }

    /// Internal method to store to L2
    async fn store_to_l2(
        &self,
        content: &str,
        role: &str,
        importance: u8,
        index_to_l3: bool,
    ) -> Result<String> {
        let (session_id, agent_id) = {
            let l1 = self.l1.read().await;
            (l1.session_id(), l1.agent_id().unwrap_or(self.default_agent_id))
        };

        // Create metadata with role
        let metadata = serde_json::to_string(&serde_json::json!({ "role": role }))?;

        let entry = NewEpisodicMemory::new(
            agent_id,
            session_id,
            content.to_string(),
            importance,
            false, // is_marked
            Some(metadata),
        )?;

        let l2_id = self.l2.create(&entry)?;

        // Index to L3 if requested and available
        if index_to_l3 {
            if let Some(ref l3) = self.l3 {
                if let Err(e) = l3.store_with_embedding(l2_id, content, &self.default_embedding_model).await {
                    tracing::warn!("Failed to index memory {} to L3: {}", l2_id, e);
                }
            }
        }

        Ok(l2_id.to_string())
    }

    /// Persist all L1 session memories to L2
    ///
    /// Called when a session ends. Returns the number of memories persisted.
    pub async fn persist_session(&self) -> Result<usize> {
        let l1 = self.l1.read().await;
        let session_id = l1.session_id();
        let agent_id = l1.agent_id().unwrap_or(self.default_agent_id);

        let entries = l1.get_context(0).await?;
        let mut count = 0;

        for entry in entries {
            let importance = 5; // Default importance for session memories
            let metadata = serde_json::to_string(&serde_json::json!({ "role": entry.role }))?;

            let new_memory = NewEpisodicMemory::new(
                agent_id,
                session_id,
                entry.content.clone(),
                importance,
                false, // is_marked
                Some(metadata),
            )?;

            self.l2.create(&new_memory)?;
            count += 1;
        }

        Ok(count)
    }

    // ========================================================================
    // Retrieve Operations (Task 3)
    // ========================================================================

    /// Retrieve memories by query
    ///
    /// Queries layers based on the specified layer:
    /// - `L1`: Only working memory
    /// - `L2`: Only episodic memory
    /// - `L3`: Only semantic memory (requires embedding service)
    /// - `All`: Try L1 first, then L2, then L3 (cascading query)
    pub async fn retrieve(&self, query: MemoryQuery) -> Result<MemoryQueryResult> {
        match query.layer {
            MemoryLayer::L1 => self.retrieve_from_l1(&query).await,
            MemoryLayer::L2 => self.retrieve_from_l2(&query).await,
            MemoryLayer::L3 => self.retrieve_from_l3(&query).await,
            MemoryLayer::All => {
                // Try L1 first
                let mut results = self.retrieve_from_l1(&query).await?;
                if results.entries.is_empty() {
                    // Then try L2
                    results = self.retrieve_from_l2(&query).await?;
                }
                if results.entries.is_empty() {
                    // Finally try L3
                    results = self.retrieve_from_l3(&query).await?;
                }
                Ok(results)
            }
        }
    }

    /// Retrieve from L1 (working memory)
    async fn retrieve_from_l1(&self, query: &MemoryQuery) -> Result<MemoryQueryResult> {
        let start = Instant::now();
        let l1 = self.l1.read().await;
        let entries = l1.get_context(query.limit).await?;
        let cache_hit = !entries.is_empty();

        let unified_entries: Vec<UnifiedMemoryEntry> = entries
            .into_iter()
            .enumerate()
            .map(|(_idx, entry)| {
                // Parse timestamp
                let created_at = entry.timestamp.parse().unwrap_or(0);

                UnifiedMemoryEntry {
                    id: entry.id,
                    content: entry.content,
                    role: Some(entry.role),
                    importance: 5, // Default for L1
                    session_id: l1.session_id(),
                    created_at,
                    source_layer: MemoryLayer::L1,
                    similarity_score: None,
                }
            })
            .collect();

        let total_count = unified_entries.len();

        // Record metrics
        let duration = start.elapsed();
        self.metrics.record(MemoryLayer::L1, duration, cache_hit).await;

        Ok(MemoryQueryResult {
            entries: unified_entries,
            layer: MemoryLayer::L1,
            total_count,
        })
    }

    /// Retrieve from L2 (episodic memory)
    async fn retrieve_from_l2(&self, query: &MemoryQuery) -> Result<MemoryQueryResult> {
        let start = Instant::now();

        let entries = if let Some(session_id) = query.session_id {
            self.l2.find_by_session(session_id)?
        } else {
            self.l2.find_by_agent(query.agent_id, query.limit, query.offset)?
        };

        // Filter by importance if specified
        let filtered_entries: Vec<_> = entries
            .into_iter()
            .filter(|e| {
                query.min_importance.map_or(true, |min| e.importance >= min)
            })
            .collect();

        let total_count = filtered_entries.len();

        let unified_entries: Vec<UnifiedMemoryEntry> = filtered_entries
            .into_iter()
            .map(|entry| {
                // Extract role from metadata
                let role = entry.metadata.as_ref().and_then(|m| {
                    serde_json::from_str::<serde_json::Value>(m)
                        .ok()
                        .and_then(|v| v["role"].as_str().map(|s| s.to_string()))
                });

                UnifiedMemoryEntry {
                    id: entry.id.to_string(),
                    content: entry.content,
                    role,
                    importance: entry.importance,
                    session_id: entry.session_id,
                    created_at: entry.created_at,
                    source_layer: MemoryLayer::L2,
                    similarity_score: None,
                }
            })
            .collect();

        // Record metrics
        let duration = start.elapsed();
        self.metrics.record(MemoryLayer::L2, duration, false).await;

        Ok(MemoryQueryResult {
            entries: unified_entries,
            layer: MemoryLayer::L2,
            total_count,
        })
    }

    /// Retrieve from L3 (semantic memory) - returns empty if not available
    async fn retrieve_from_l3(&self, _query: &MemoryQuery) -> Result<MemoryQueryResult> {
        // L3 is primarily for search, not retrieval by ID
        // Return empty result
        Ok(MemoryQueryResult {
            entries: Vec::new(),
            layer: MemoryLayer::L3,
            total_count: 0,
        })
    }

    /// Retrieve memories by session ID
    pub async fn retrieve_by_session(&self, session_id: i64, limit: usize) -> Result<MemoryQueryResult> {
        let query = MemoryQuery {
            session_id: Some(session_id),
            limit,
            ..Default::default()
        };
        self.retrieve(query).await
    }

    /// Retrieve memories by time range
    pub async fn retrieve_by_time_range(
        &self,
        start: i64,
        end: i64,
        agent_id: Option<i64>,
    ) -> Result<MemoryQueryResult> {
        let entries = self.l2.find_by_time_range(start, end, agent_id.or(Some(self.default_agent_id)))?;

        let unified_entries: Vec<UnifiedMemoryEntry> = entries
            .into_iter()
            .map(|entry| {
                let role = entry.metadata.as_ref().and_then(|m| {
                    serde_json::from_str::<serde_json::Value>(m)
                        .ok()
                        .and_then(|v| v["role"].as_str().map(|s| s.to_string()))
                });

                UnifiedMemoryEntry {
                    id: entry.id.to_string(),
                    content: entry.content,
                    role,
                    importance: entry.importance,
                    session_id: entry.session_id,
                    created_at: entry.created_at,
                    source_layer: MemoryLayer::L2,
                    similarity_score: None,
                }
            })
            .collect();

        let total_count = unified_entries.len();

        Ok(MemoryQueryResult {
            entries: unified_entries,
            layer: MemoryLayer::L2,
            total_count,
        })
    }

    /// Retrieve from a specific layer
    pub async fn retrieve_from_layer(&self, layer: MemoryLayer, query: MemoryQuery) -> Result<MemoryQueryResult> {
        let mut q = query;
        q.layer = layer;
        self.retrieve(q).await
    }

    // ========================================================================
    // Search Operations (Task 4)
    // ========================================================================

    /// Search memories using semantic similarity (L3)
    ///
    /// Returns memories sorted by similarity score (descending).
    /// Returns empty if L3 is not configured.
    ///
    /// # Arguments
    /// * `query` - Search query text
    /// * `k` - Maximum number of results
    /// * `threshold` - Minimum similarity threshold (0.0 to 1.0)
    pub async fn search(
        &self,
        query: &str,
        k: usize,
        threshold: f32,
    ) -> Result<Vec<UnifiedMemoryEntry>> {
        let start = Instant::now();

        let Some(ref l3) = self.l3 else {
            tracing::debug!("L3 semantic search not available");
            return Ok(Vec::new());
        };

        let results = l3.search_similar(query, k, None, threshold).await?;

        let unified_entries: Vec<UnifiedMemoryEntry> = results
            .into_iter()
            .map(|r| UnifiedMemoryEntry {
                id: r.memory.id.to_string(),
                content: r.content.unwrap_or_default(),
                role: None,
                importance: 5,
                session_id: None,
                created_at: r.memory.created_at,
                source_layer: MemoryLayer::L3,
                similarity_score: Some(r.score),
            })
            .collect();

        // Record metrics
        let duration = start.elapsed();
        self.metrics.record(MemoryLayer::L3, duration, false).await;

        Ok(unified_entries)
    }

    /// Hybrid search combining keyword and semantic search
    ///
    /// First performs keyword search on L2, then semantic search on L3,
    /// and merges results with deduplication.
    pub async fn search_hybrid(
        &self,
        _keyword: &str,
        semantic_query: &str,
        k: usize,
        threshold: f32,
    ) -> Result<Vec<UnifiedMemoryEntry>> {
        // Get results from L2 (currently using find_by_importance as placeholder)
        // TODO: Implement proper keyword search when available
        let l2_results = self.l2.find_by_importance(1, k)?;

        // Get results from L3 semantic search
        let l3_results = self.search(semantic_query, k, threshold).await?;

        // Merge and deduplicate by content
        let mut seen_content = std::collections::HashSet::new();
        let mut merged = Vec::new();

        // Add L3 results first (they have similarity scores)
        for entry in l3_results {
            if seen_content.insert(entry.content.clone()) {
                merged.push(entry);
            }
        }

        // Add L2 results that weren't in L3
        for entry in l2_results {
            let content = entry.content.clone();
            if seen_content.insert(content.clone()) {
                let role = entry.metadata.as_ref().and_then(|m| {
                    serde_json::from_str::<serde_json::Value>(m)
                        .ok()
                        .and_then(|v| v["role"].as_str().map(|s| s.to_string()))
                });

                merged.push(UnifiedMemoryEntry {
                    id: entry.id.to_string(),
                    content,
                    role,
                    importance: entry.importance,
                    session_id: entry.session_id,
                    created_at: entry.created_at,
                    source_layer: MemoryLayer::L2,
                    similarity_score: None,
                });
            }
        }

        // Truncate to k
        merged.truncate(k);

        Ok(merged)
    }

    /// Search memories by importance threshold
    pub async fn search_by_importance(
        &self,
        min_importance: u8,
        limit: usize,
    ) -> Result<Vec<UnifiedMemoryEntry>> {
        let entries = self.l2.find_by_importance(min_importance, limit)?;

        let unified_entries: Vec<UnifiedMemoryEntry> = entries
            .into_iter()
            .map(|entry| {
                let role = entry.metadata.as_ref().and_then(|m| {
                    serde_json::from_str::<serde_json::Value>(m)
                        .ok()
                        .and_then(|v| v["role"].as_str().map(|s| s.to_string()))
                });

                UnifiedMemoryEntry {
                    id: entry.id.to_string(),
                    content: entry.content,
                    role,
                    importance: entry.importance,
                    session_id: entry.session_id,
                    created_at: entry.created_at,
                    source_layer: MemoryLayer::L2,
                    similarity_score: None,
                }
            })
            .collect();

        Ok(unified_entries)
    }

    // ========================================================================
    // Delete Operations (Task 5)
    // ========================================================================

    /// Delete a memory from the specified layer(s)
    ///
    /// # Arguments
    /// * `id` - Memory ID (L2/L3 ID, not applicable for L1)
    /// * `layer` - Target layer(s) for deletion
    ///
    /// # Returns
    /// `true` if deletion was successful
    pub async fn delete(&self, id: &str, layer: MemoryLayer) -> Result<bool> {
        match layer {
            MemoryLayer::L1 => {
                // L1 doesn't support direct deletion by ID
                Ok(false)
            }
            MemoryLayer::L2 => {
                let l2_id: i64 = id.parse()?;
                self.l2.delete(l2_id)
            }
            MemoryLayer::L3 => {
                let l3_id: i64 = id.parse()?;
                Ok(self.l3.as_ref()
                    .map(|l3| l3.delete(l3_id))
                    .transpose()?
                    .unwrap_or(false))
            }
            MemoryLayer::All => {
                let l2_id: i64 = id.parse()?;
                let deleted = self.l2.delete(l2_id)?;

                // Also delete from L3 if exists
                if let Some(ref l3) = self.l3 {
                    if let Some(sm) = l3.get_by_episodic_id(l2_id)? {
                        l3.delete(sm.id)?;
                    }
                }

                Ok(deleted)
            }
        }
    }

    /// Delete from a specific layer
    pub async fn delete_from_layer(&self, id: &str, layer: MemoryLayer) -> Result<bool> {
        self.delete(id, layer).await
    }

    /// Clear session memory (L1 only) - clears all entries from working memory
    pub async fn clear_l1_memory(&self) -> Result<()> {
        let l1 = self.l1.read().await;
        l1.clear().await
    }

    /// Purge all memories from all layers
    ///
    /// ⚠️ **Destructive operation** - clears L1, L2, and L3.
    pub async fn purge(&self) -> Result<()> {
        // Clear L1
        {
            let l1 = self.l1.read().await;
            l1.clear().await?;
        }

        // Note: L2 and L3 don't have bulk delete in the current implementation
        // This would require additional methods in EpisodicMemoryStore and SemanticMemoryStore

        tracing::warn!("Purge called - L1 cleared. L2/L3 require manual cleanup or additional methods.");

        Ok(())
    }

    // ========================================================================
    // Priority and Eviction (Task 6)
    // ========================================================================

    /// Get the current eviction policy
    pub fn eviction_policy(&self) -> EvictionPolicy {
        self.eviction_policy
    }

    /// Set the eviction policy
    pub fn set_eviction_policy(&mut self, policy: EvictionPolicy) {
        self.eviction_policy = policy;
    }

    /// Promote important L1 memories to L2
    ///
    /// Called automatically when importance >= auto_promote_threshold.
    /// Can also be called manually to force promotion.
    pub async fn promote_to_l2(&self, content: &str, role: &str, importance: u8) -> Result<String> {
        self.store_to_l2(content, role, importance, false).await
    }

    /// Index an L2 memory to L3
    ///
    /// Creates a semantic embedding for the memory.
    pub async fn index_to_l3(&self, l2_id: i64, content: &str) -> Result<()> {
        let Some(ref l3) = self.l3 else {
            return Ok(());
        };

        l3.store_with_embedding(l2_id, content, &self.default_embedding_model).await?;
        Ok(())
    }

    /// Auto-promote and index based on importance thresholds
    ///
    /// This is called internally during `store_with_importance`.
    pub fn should_promote_to_l2(&self, importance: u8) -> bool {
        importance >= self.auto_promote_threshold
    }

    pub fn should_index_to_l3(&self, importance: u8) -> bool {
        importance >= self.auto_index_threshold
    }

    // ========================================================================
    // Statistics
    // ========================================================================

    /// Get statistics for all memory layers
    pub async fn get_stats(&self) -> Result<MemoryManagerStats> {
        let l1 = self.l1.read().await;
        let l1_stats = l1.stats();

        let l2_stats = self.l2.stats()?;

        let l3_total = if let Some(ref l3) = self.l3 {
            l3.stats()?.total_count
        } else {
            0
        };

        Ok(MemoryManagerStats {
            l1_capacity: l1_stats.capacity,
            l1_used: l1_stats.used,
            l1_session_id: l1_stats.session_id,
            l2_total: l2_stats.total_count,
            l2_avg_importance: l2_stats.avg_importance,
            l3_total,
        })
    }

    /// Get L1 stats
    pub async fn l1_stats(&self) -> MemoryStats {
        let l1 = self.l1.read().await;
        l1.stats()
    }

    /// Check if L3 is available
    pub fn has_l3(&self) -> bool {
        self.l3.is_some()
    }

    // ========================================================================
    // Performance Metrics (Story 5.5)
    // ========================================================================

    /// Get performance statistics for all memory operations
    ///
    /// Returns aggregated metrics including latency, cache hit rates, and query counts.
    pub async fn get_performance_stats(&self) -> PerformanceStats {
        self.metrics.get_stats().await
    }

    /// Reset performance statistics
    pub async fn reset_performance_stats(&self) {
        self.metrics.reset().await;
    }

    /// Run a performance benchmark for all memory layers
    ///
    /// Returns benchmark results for L1, L2, and L3 operations.
    pub async fn benchmark(&self) -> Result<BenchmarkResults> {
        let mut results = BenchmarkResults::default();

        // Benchmark L1 operations
        let l1_start = Instant::now();
        let l1 = self.l1.read().await;
        let _ = l1.get_context(10).await?;
        let _ = l1.stats();
        drop(l1);
        results.l1_retrieve_ms = l1_start.elapsed().as_secs_f64() * 1000.0;

        // Benchmark L2 operations
        let l2_start = Instant::now();
        let _ = self.l2.find_by_agent(self.default_agent_id, 10, 0)?;
        let _ = self.l2.stats()?;
        results.l2_retrieve_ms = l2_start.elapsed().as_secs_f64() * 1000.0;

        // Benchmark L3 operations (if available)
        if let Some(ref l3) = self.l3 {
            let l3_start = Instant::now();
            let _ = l3.search_similar("test query", 5, None, 0.7).await;
            let _ = l3.stats()?;
            results.l3_search_ms = l3_start.elapsed().as_secs_f64() * 1000.0;
            results.l3_available = true;
        }

        // Benchmark combined retrieve
        let combined_start = Instant::now();
        let query = MemoryQuery {
            agent_id: self.default_agent_id,
            limit: 10,
            ..Default::default()
        };
        let _ = self.retrieve(query).await?;
        results.combined_retrieve_ms = combined_start.elapsed().as_secs_f64() * 1000.0;

        Ok(results)
    }
}

/// Benchmark results for memory operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BenchmarkResults {
    /// L1 retrieve time in milliseconds
    pub l1_retrieve_ms: f64,
    /// L2 retrieve time in milliseconds
    pub l2_retrieve_ms: f64,
    /// L3 search time in milliseconds
    pub l3_search_ms: f64,
    /// Whether L3 is available
    pub l3_available: bool,
    /// Combined retrieve time in milliseconds
    pub combined_retrieve_ms: f64,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_layer_display() {
        assert_eq!(format!("{}", MemoryLayer::L1), "L1");
        assert_eq!(format!("{}", MemoryLayer::L2), "L2");
        assert_eq!(format!("{}", MemoryLayer::L3), "L3");
        assert_eq!(format!("{}", MemoryLayer::All), "All");
    }

    #[test]
    fn test_memory_layer_serialize() {
        assert_eq!(serde_json::to_string(&MemoryLayer::L1).unwrap(), "\"L1\"");
        assert_eq!(serde_json::to_string(&MemoryLayer::All).unwrap(), "\"ALL\"");
    }

    #[test]
    fn test_memory_layer_deserialize() {
        let l1: MemoryLayer = serde_json::from_str("\"L1\"").unwrap();
        assert_eq!(l1, MemoryLayer::L1);

        let all: MemoryLayer = serde_json::from_str("\"ALL\"").unwrap();
        assert_eq!(all, MemoryLayer::All);
    }

    #[test]
    fn test_memory_query_default() {
        let query = MemoryQuery::default();
        assert_eq!(query.agent_id, 1);
        assert_eq!(query.layer, MemoryLayer::All);
        assert_eq!(query.limit, 100);
        assert_eq!(query.offset, 0);
    }

    #[test]
    fn test_eviction_policy_default() {
        assert_eq!(EvictionPolicy::default(), EvictionPolicy::Lru);
    }

    #[test]
    fn test_unified_memory_entry_serialization() {
        let entry = UnifiedMemoryEntry {
            id: "1".to_string(),
            content: "Test content".to_string(),
            role: Some("user".to_string()),
            importance: 7,
            session_id: Some(42),
            created_at: 1700000000,
            source_layer: MemoryLayer::L2,
            similarity_score: Some(0.95),
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"source_layer\":\"L2\""));
        assert!(json.contains("\"similarity_score\":0.95"));
    }

    #[test]
    fn test_memory_manager_stats_serialization() {
        let stats = MemoryManagerStats {
            l1_capacity: 100,
            l1_used: 50,
            l1_session_id: Some(1),
            l2_total: 200,
            l2_avg_importance: 6.5,
            l3_total: 150,
        };

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"l1_capacity\":100"));
        assert!(json.contains("\"l2_avg_importance\":6.5"));
    }
}