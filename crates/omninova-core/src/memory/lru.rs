//! LRU Memory Implementation
//!
//! A memory backend with LRU (Least Recently Used) eviction policy.
//! Provides O(1) access time and configurable capacity.
//!
//! [Source: Story 5.1 - L1 工作记忆层实现]

use crate::memory::traits::{Memory, MemoryCategory, MemoryEntry};
use async_trait::async_trait;
use lru::LruCache;
use parking_lot::RwLock;
use std::num::NonZeroUsize;
use std::sync::Arc;

/// Default capacity for LRU cache (100 entries)
const DEFAULT_CAPACITY: usize = 100;

/// LRU Memory with configurable capacity and O(1) access
///
/// This implementation provides:
/// - LRU eviction when capacity is reached
/// - O(1) get/put operations
/// - Thread-safe access via RwLock
/// - Configurable capacity
pub struct LruMemory {
    cache: Arc<RwLock<LruCache<String, MemoryEntry>>>,
    capacity: NonZeroUsize,
}

impl LruMemory {
    /// Create a new LruMemory with default capacity (100 entries)
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Create a new LruMemory with specified capacity
    ///
    /// # Panics
    ///
    /// Panics if capacity is 0.
    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(DEFAULT_CAPACITY).unwrap());
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(capacity))),
            capacity,
        }
    }

    /// Get the capacity of this cache
    pub fn capacity(&self) -> usize {
        self.capacity.get()
    }

    /// Get the current number of entries
    pub fn len(&self) -> usize {
        self.cache.read().len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.read().is_empty()
    }

    /// Generate a timestamp string
    fn now_timestamp() -> String {
        time::OffsetDateTime::now_utc().unix_timestamp().to_string()
    }

    /// Check if content matches query (case-insensitive substring)
    fn matches_query(content: &str, query: &str) -> bool {
        if query.trim().is_empty() {
            return true;
        }
        content.to_lowercase().contains(&query.to_lowercase())
    }
}

impl Default for LruMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Memory for LruMemory {
    fn name(&self) -> &str {
        "lru_memory"
    }

    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut cache = self.cache.write();
        let id = format!("lru-{}", uuid::Uuid::new_v4());

        let entry = MemoryEntry {
            id,
            key: key.to_string(),
            content: content.to_string(),
            category,
            timestamp: Self::now_timestamp(),
            session_id: session_id.map(ToString::to_string),
            score: None,
        };

        // LruCache::put will evict the least recently used entry if at capacity
        cache.put(key.to_string(), entry);

        Ok(())
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let cache = self.cache.read();
        let mut items: Vec<MemoryEntry> = cache
            .iter()
            .filter(|(_, entry)| {
                let session_match = match session_id {
                    Some(sid) => entry.session_id.as_deref() == Some(sid),
                    None => true,
                };
                session_match && Self::matches_query(&entry.content, query)
            })
            .map(|(_, entry)| entry.clone())
            .collect();

        // Sort by timestamp (most recent first)
        items.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        if limit > 0 {
            items.truncate(limit);
        }

        Ok(items)
    }

    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        // LruCache::get promotes the entry to most recently used
        let mut cache = self.cache.write();
        Ok(cache.get(&key.to_string()).cloned())
    }

    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let cache = self.cache.read();
        let mut items: Vec<MemoryEntry> = cache
            .iter()
            .filter(|(_, entry)| {
                let category_match = match category {
                    Some(cat) => &entry.category == cat,
                    None => true,
                };
                let session_match = match session_id {
                    Some(sid) => entry.session_id.as_deref() == Some(sid),
                    None => true,
                };
                category_match && session_match
            })
            .map(|(_, entry)| entry.clone())
            .collect();

        // Sort by timestamp (most recent first)
        items.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(items)
    }

    async fn forget(&self, key: &str) -> anyhow::Result<bool> {
        let mut cache = self.cache.write();
        Ok(cache.pop(&key.to_string()).is_some())
    }

    async fn count(&self) -> anyhow::Result<usize> {
        Ok(self.cache.read().len())
    }

    async fn health_check(&self) -> bool {
        true
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_memory_new() {
        let memory = LruMemory::new();
        assert_eq!(memory.capacity(), DEFAULT_CAPACITY);
        assert_eq!(memory.len(), 0);
        assert!(memory.is_empty());
    }

    #[test]
    fn test_lru_memory_with_capacity() {
        let memory = LruMemory::with_capacity(50);
        assert_eq!(memory.capacity(), 50);
    }

    #[test]
    fn test_lru_memory_default() {
        let memory = LruMemory::default();
        assert_eq!(memory.capacity(), DEFAULT_CAPACITY);
    }

    #[tokio::test]
    async fn test_store_and_get() {
        let memory = LruMemory::new();

        memory
            .store("key1", "content1", MemoryCategory::Core, None)
            .await
            .unwrap();

        let entry = memory.get("key1").await.unwrap();
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.key, "key1");
        assert_eq!(entry.content, "content1");
        assert_eq!(entry.category, MemoryCategory::Core);
    }

    #[tokio::test]
    async fn test_lru_eviction() {
        // Create cache with capacity 3
        let memory = LruMemory::with_capacity(3);

        // Add 3 entries
        memory.store("key1", "content1", MemoryCategory::Core, None).await.unwrap();
        memory.store("key2", "content2", MemoryCategory::Core, None).await.unwrap();
        memory.store("key3", "content3", MemoryCategory::Core, None).await.unwrap();

        assert_eq!(memory.len(), 3);

        // Add a 4th entry - should evict key1 (least recently used)
        memory.store("key4", "content4", MemoryCategory::Core, None).await.unwrap();

        assert_eq!(memory.len(), 3);

        // key1 should be evicted
        let entry1 = memory.get("key1").await.unwrap();
        assert!(entry1.is_none());

        // key2, key3, key4 should still exist
        assert!(memory.get("key2").await.unwrap().is_some());
        assert!(memory.get("key3").await.unwrap().is_some());
        assert!(memory.get("key4").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_lru_access_promotes() {
        let memory = LruMemory::with_capacity(3);

        memory.store("key1", "content1", MemoryCategory::Core, None).await.unwrap();
        memory.store("key2", "content2", MemoryCategory::Core, None).await.unwrap();
        memory.store("key3", "content3", MemoryCategory::Core, None).await.unwrap();

        // Access key1 to promote it to most recently used
        memory.get("key1").await.unwrap();

        // Add a new entry - should evict key2 (now least recently used)
        memory.store("key4", "content4", MemoryCategory::Core, None).await.unwrap();

        // key2 should be evicted, key1 should still exist
        assert!(memory.get("key1").await.unwrap().is_some());
        assert!(memory.get("key2").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_recall_with_query() {
        let memory = LruMemory::new();

        memory.store("key1", "hello world", MemoryCategory::Core, None).await.unwrap();
        memory.store("key2", "goodbye world", MemoryCategory::Core, None).await.unwrap();
        memory.store("key3", "hello universe", MemoryCategory::Core, None).await.unwrap();

        // Search for "hello"
        let results = memory.recall("hello", 10, None).await.unwrap();
        assert_eq!(results.len(), 2);

        // Search for "world"
        let results = memory.recall("world", 10, None).await.unwrap();
        assert_eq!(results.len(), 2);

        // Search with limit
        let results = memory.recall("world", 1, None).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_recall_with_session() {
        let memory = LruMemory::new();

        memory.store("key1", "content1", MemoryCategory::Core, Some("session1")).await.unwrap();
        memory.store("key2", "content2", MemoryCategory::Core, Some("session2")).await.unwrap();
        memory.store("key3", "content3", MemoryCategory::Core, Some("session1")).await.unwrap();

        // Query with session filter
        let results = memory.recall("", 10, Some("session1")).await.unwrap();
        assert_eq!(results.len(), 2);

        let results = memory.recall("", 10, Some("session2")).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_forget() {
        let memory = LruMemory::new();

        memory.store("key1", "content1", MemoryCategory::Core, None).await.unwrap();
        assert_eq!(memory.len(), 1);

        let removed = memory.forget("key1").await.unwrap();
        assert!(removed);
        assert_eq!(memory.len(), 0);

        let removed = memory.forget("nonexistent").await.unwrap();
        assert!(!removed);
    }

    #[tokio::test]
    async fn test_list_with_category() {
        let memory = LruMemory::new();

        memory.store("key1", "content1", MemoryCategory::Core, None).await.unwrap();
        memory.store("key2", "content2", MemoryCategory::Daily, None).await.unwrap();
        memory.store("key3", "content3", MemoryCategory::Core, None).await.unwrap();

        let results = memory.list(Some(&MemoryCategory::Core), None).await.unwrap();
        assert_eq!(results.len(), 2);

        let results = memory.list(Some(&MemoryCategory::Daily), None).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_count() {
        let memory = LruMemory::new();

        assert_eq!(memory.count().await.unwrap(), 0);

        memory.store("key1", "content1", MemoryCategory::Core, None).await.unwrap();
        assert_eq!(memory.count().await.unwrap(), 1);

        memory.store("key2", "content2", MemoryCategory::Core, None).await.unwrap();
        assert_eq!(memory.count().await.unwrap(), 2);

        memory.forget("key1").await.unwrap();
        assert_eq!(memory.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_health_check() {
        let memory = LruMemory::new();
        assert!(memory.health_check().await);
    }
}