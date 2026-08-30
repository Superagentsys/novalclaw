use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A single memory entry
#[derive(Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub key: String,
    pub content: String,
    pub category: MemoryCategory,
    pub timestamp: String,
    pub session_id: Option<String>,
    pub score: Option<f64>,
}

impl std::fmt::Debug for MemoryEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryEntry")
            .field("id", &self.id)
            .field("key", &self.key)
            .field("content", &self.content)
            .field("category", &self.category)
            .field("timestamp", &self.timestamp)
            .field("score", &self.score)
            .finish_non_exhaustive()
    }
}

/// Memory categories for organization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    /// Long-term facts, preferences, decisions
    Core,
    /// Daily session logs
    Daily,
    /// Conversation context
    Conversation,
    /// User-defined custom category
    Custom(String),
}

impl std::fmt::Display for MemoryCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Core => write!(f, "core"),
            Self::Daily => write!(f, "daily"),
            Self::Conversation => write!(f, "conversation"),
            Self::Custom(name) => write!(f, "{name}"),
        }
    }
}

/// Core memory trait — implement for any persistence backend
#[async_trait]
pub trait Memory: Send + Sync {
    /// Backend name
    fn name(&self) -> &str;

    /// Store a memory entry, optionally scoped to a session
    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()>;

    /// Recall memories matching a query (keyword search), optionally scoped to a session
    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>>;

    /// Get a specific memory by key
    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>>;

    /// List all memory keys, optionally filtered by category and/or session
    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>>;

    /// Remove a memory by key
    async fn forget(&self, key: &str) -> anyhow::Result<bool>;

    /// Count total memories
    async fn count(&self) -> anyhow::Result<usize>;

    /// Health check
    async fn health_check(&self) -> bool;
}
