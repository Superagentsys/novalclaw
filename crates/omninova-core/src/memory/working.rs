//! Working Memory Manager
//!
//! L1 working memory layer for managing session context.
//! Provides a wrapper around LruMemory with session-level operations
//! and optional persistence to L2 (episodic memory).
//!
//! [Source: Story 5.1 - L1 工作记忆层实现]

use crate::memory::lru::LruMemory;
use crate::memory::traits::{Memory, MemoryCategory, MemoryEntry};
use serde::{Deserialize, Serialize};

/// Default capacity for working memory (100 entries ~ 4096 tokens context)
const DEFAULT_CAPACITY: usize = 100;

/// Statistics about working memory usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    /// Maximum capacity
    pub capacity: usize,
    /// Current number of entries
    pub used: usize,
    /// Session ID if active
    pub session_id: Option<i64>,
    /// Agent ID if associated
    pub agent_id: Option<i64>,
}

/// Working memory entry with role information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkingMemoryEntry {
    /// Entry ID
    pub id: String,
    /// Role (user, assistant, system)
    pub role: String,
    /// Message content
    pub content: String,
    /// Timestamp
    pub timestamp: String,
}

impl From<&MemoryEntry> for WorkingMemoryEntry {
    fn from(entry: &MemoryEntry) -> Self {
        // Parse role from key (format: "role:index" or just "role")
        let role = entry.key.split(':').next().unwrap_or("unknown").to_string();

        Self {
            id: entry.id.clone(),
            role,
            content: entry.content.clone(),
            timestamp: entry.timestamp.clone(),
        }
    }
}

/// L1 Working Memory Manager
///
/// Manages short-term memory for active sessions with:
/// - LRU eviction when capacity is reached
/// - Session-scoped context management
/// - Optional persistence to L2 (episodic memory)
pub struct WorkingMemory {
    /// L1 cache backend
    l1_cache: LruMemory,
    /// Current session ID
    session_id: Option<i64>,
    /// Associated agent ID
    agent_id: Option<i64>,
}

impl WorkingMemory {
    /// Create a new WorkingMemory with default capacity
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Create a new WorkingMemory with specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            l1_cache: LruMemory::with_capacity(capacity),
            session_id: None,
            agent_id: None,
        }
    }

    /// Set the session context
    pub fn set_session(&mut self, session_id: i64, agent_id: i64) {
        self.session_id = Some(session_id);
        self.agent_id = Some(agent_id);
    }

    /// Clear the session context
    pub fn clear_session(&mut self) {
        self.session_id = None;
        self.agent_id = None;
    }

    /// Get the current session ID
    pub fn session_id(&self) -> Option<i64> {
        self.session_id
    }

    /// Get the current agent ID
    pub fn agent_id(&self) -> Option<i64> {
        self.agent_id
    }

    /// Push a context entry (conversation message) to working memory
    ///
    /// The key format is "role:index" where role is user/assistant/system
    /// and index is the message order in the conversation.
    pub async fn push_context(&self, role: &str, content: &str) -> anyhow::Result<()> {
        // Generate a key with role prefix
        let count = self.l1_cache.len();
        let key = format!("{}:{}", role, count);

        let category = match role {
            "user" => MemoryCategory::Conversation,
            "assistant" => MemoryCategory::Conversation,
            "system" => MemoryCategory::Core,
            _ => MemoryCategory::Custom(role.to_string()),
        };

        let session_id_str = self.session_id.map(|id| id.to_string());

        self.l1_cache
            .store(&key, content, category, session_id_str.as_deref())
            .await
    }

    /// Get all context entries for LLM consumption
    ///
    /// Returns entries in chronological order (oldest first)
    pub async fn get_context(&self, limit: usize) -> anyhow::Result<Vec<WorkingMemoryEntry>> {
        let session_id_str = self.session_id.map(|id| id.to_string());

        let mut entries = self
            .l1_cache
            .list(None, session_id_str.as_deref())
            .await?;

        // Sort by key index (format: "role:index")
        // This maintains insertion order
        entries.sort_by(|a, b| {
            let a_idx: usize = a.key.split(':').nth(1).unwrap_or("0").parse().unwrap_or(0);
            let b_idx: usize = b.key.split(':').nth(1).unwrap_or("0").parse().unwrap_or(0);
            a_idx.cmp(&b_idx)
        });

        if limit > 0 && entries.len() > limit {
            // Keep the most recent 'limit' entries
            entries = entries.split_off(entries.len() - limit);
        }

        Ok(entries.iter().map(WorkingMemoryEntry::from).collect())
    }

    /// Get formatted context string for LLM prompt
    ///
    /// Returns a string like:
    /// ```text
    /// user: Hello
    /// assistant: Hi there!
    /// user: How are you?
    /// ```
    pub async fn get_context_string(&self, limit: usize) -> anyhow::Result<String> {
        let entries = self.get_context(limit).await?;

        let mut result = String::new();
        for entry in entries {
            result.push_str(&format!("{}: {}\n", entry.role, entry.content));
        }

        Ok(result.trim_end().to_string())
    }

    /// Clear all context from working memory
    pub async fn clear(&self) -> anyhow::Result<()> {
        // Get all keys
        let session_id_str = self.session_id.map(|id| id.to_string());
        let entries = self.l1_cache.list(None, session_id_str.as_deref()).await?;

        // Remove each entry
        for entry in entries {
            self.l1_cache.forget(&entry.key).await?;
        }

        Ok(())
    }

    /// Persist all entries to L2 (episodic memory)
    ///
    /// This is called when a session ends to save important context
    /// to long-term storage.
    pub async fn persist_to_l2(&self, l2: &dyn Memory) -> anyhow::Result<usize> {
        let session_id_str = self.session_id.map(|id| id.to_string());
        let entries = self.l1_cache.list(None, session_id_str.as_deref()).await?;

        let mut count = 0;
        for entry in entries {
            l2.store(
                &entry.key,
                &entry.content,
                entry.category.clone(),
                entry.session_id.as_deref(),
            )
            .await?;
            count += 1;
        }

        Ok(count)
    }

    /// Get memory statistics
    pub fn stats(&self) -> MemoryStats {
        MemoryStats {
            capacity: self.l1_cache.capacity(),
            used: self.l1_cache.len(),
            session_id: self.session_id,
            agent_id: self.agent_id,
        }
    }

    /// Check if working memory is empty
    pub fn is_empty(&self) -> bool {
        self.l1_cache.is_empty()
    }

    /// Get current number of entries
    pub fn len(&self) -> usize {
        self.l1_cache.len()
    }

    /// Get the capacity
    pub fn capacity(&self) -> usize {
        self.l1_cache.capacity()
    }
}

impl Default for WorkingMemory {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::backend::InMemoryMemory;

    #[test]
    fn test_working_memory_new() {
        let memory = WorkingMemory::new();
        assert_eq!(memory.capacity(), DEFAULT_CAPACITY);
        assert!(memory.is_empty());
        assert!(memory.session_id().is_none());
        assert!(memory.agent_id().is_none());
    }

    #[test]
    fn test_working_memory_with_capacity() {
        let memory = WorkingMemory::with_capacity(50);
        assert_eq!(memory.capacity(), 50);
    }

    #[test]
    fn test_set_session() {
        let mut memory = WorkingMemory::new();
        memory.set_session(1, 42);

        assert_eq!(memory.session_id(), Some(1));
        assert_eq!(memory.agent_id(), Some(42));
    }

    #[test]
    fn test_clear_session() {
        let mut memory = WorkingMemory::new();
        memory.set_session(1, 42);
        memory.clear_session();

        assert!(memory.session_id().is_none());
        assert!(memory.agent_id().is_none());
    }

    #[tokio::test]
    async fn test_push_and_get_context() {
        let mut memory = WorkingMemory::new();
        memory.set_session(1, 42);

        memory.push_context("user", "Hello").await.unwrap();
        memory.push_context("assistant", "Hi there!").await.unwrap();
        memory.push_context("user", "How are you?").await.unwrap();

        assert_eq!(memory.len(), 3);

        let entries = memory.get_context(0).await.unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].role, "user");
        assert_eq!(entries[0].content, "Hello");
        assert_eq!(entries[1].role, "assistant");
        assert_eq!(entries[1].content, "Hi there!");
    }

    #[tokio::test]
    async fn test_get_context_with_limit() {
        let mut memory = WorkingMemory::new();
        memory.set_session(1, 42);

        memory.push_context("user", "Msg 1").await.unwrap();
        memory.push_context("assistant", "Msg 2").await.unwrap();
        memory.push_context("user", "Msg 3").await.unwrap();
        memory.push_context("assistant", "Msg 4").await.unwrap();

        let entries = memory.get_context(2).await.unwrap();
        assert_eq!(entries.len(), 2);
        // Should get the most recent 2 entries
        assert_eq!(entries[0].content, "Msg 3");
        assert_eq!(entries[1].content, "Msg 4");
    }

    #[tokio::test]
    async fn test_get_context_string() {
        let mut memory = WorkingMemory::new();
        memory.set_session(1, 42);

        memory.push_context("user", "Hello").await.unwrap();
        memory.push_context("assistant", "Hi there!").await.unwrap();

        let context = memory.get_context_string(0).await.unwrap();
        assert_eq!(context, "user: Hello\nassistant: Hi there!");
    }

    #[tokio::test]
    async fn test_clear() {
        let mut memory = WorkingMemory::new();
        memory.set_session(1, 42);

        memory.push_context("user", "Hello").await.unwrap();
        assert_eq!(memory.len(), 1);

        memory.clear().await.unwrap();
        assert_eq!(memory.len(), 0);
        assert!(memory.is_empty());
    }

    #[tokio::test]
    async fn test_persist_to_l2() {
        let mut memory = WorkingMemory::new();
        memory.set_session(1, 42);

        memory.push_context("user", "Hello").await.unwrap();
        memory.push_context("assistant", "Hi there!").await.unwrap();

        let l2 = InMemoryMemory::new();
        let count = memory.persist_to_l2(&l2).await.unwrap();

        assert_eq!(count, 2);
        assert_eq!(l2.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_stats() {
        let mut memory = WorkingMemory::new();
        memory.set_session(1, 42);

        let stats = memory.stats();
        assert_eq!(stats.capacity, DEFAULT_CAPACITY);
        assert_eq!(stats.used, 0);
        assert_eq!(stats.session_id, Some(1));
        assert_eq!(stats.agent_id, Some(42));

        memory.push_context("user", "Hello").await.unwrap();
        let stats = memory.stats();
        assert_eq!(stats.used, 1);
    }

    #[tokio::test]
    async fn test_lru_eviction_in_working_memory() {
        let mut memory = WorkingMemory::with_capacity(3);
        memory.set_session(1, 42);

        // Add 4 entries (capacity is 3)
        memory.push_context("user", "Msg 1").await.unwrap();
        memory.push_context("assistant", "Msg 2").await.unwrap();
        memory.push_context("user", "Msg 3").await.unwrap();
        memory.push_context("assistant", "Msg 4").await.unwrap();

        // Should have 3 entries due to LRU eviction
        assert_eq!(memory.len(), 3);
    }

    #[test]
    fn test_working_memory_entry_from() {
        let memory_entry = MemoryEntry {
            id: "test-1".to_string(),
            key: "user:0".to_string(),
            content: "Hello".to_string(),
            category: MemoryCategory::Conversation,
            timestamp: "12345".to_string(),
            session_id: Some("1".to_string()),
            score: None,
        };

        let working_entry = WorkingMemoryEntry::from(&memory_entry);
        assert_eq!(working_entry.role, "user");
        assert_eq!(working_entry.content, "Hello");
    }
}