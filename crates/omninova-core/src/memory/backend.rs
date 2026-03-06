use crate::memory::traits::{Memory, MemoryCategory, MemoryEntry};
use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub struct MockMemory;

#[async_trait]
impl Memory for MockMemory {
    fn name(&self) -> &str {
        "mock_memory"
    }

    async fn store(
        &self,
        _key: &str,
        _content: &str,
        _category: MemoryCategory,
        _session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn recall(
        &self,
        _query: &str,
        _limit: usize,
        _session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        Ok(vec![])
    }

    async fn get(&self, _key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        Ok(None)
    }

    async fn list(
        &self,
        _category: Option<&MemoryCategory>,
        _session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        Ok(vec![])
    }

    async fn forget(&self, _key: &str) -> anyhow::Result<bool> {
        Ok(true)
    }

    async fn count(&self) -> anyhow::Result<usize> {
        Ok(0)
    }

    async fn health_check(&self) -> bool {
        true
    }
}

#[derive(Clone, Default)]
pub struct InMemoryMemory {
    entries: Arc<RwLock<HashMap<String, MemoryEntry>>>,
}

impl InMemoryMemory {
    pub fn new() -> Self {
        Self::default()
    }

    fn now_timestamp() -> String {
        time::OffsetDateTime::now_utc().unix_timestamp().to_string()
    }

    fn matches_query(content: &str, query: &str) -> bool {
        if query.trim().is_empty() {
            return true;
        }
        content.to_lowercase().contains(&query.to_lowercase())
    }
}

#[async_trait]
impl Memory for InMemoryMemory {
    fn name(&self) -> &str {
        "in_memory"
    }

    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut lock = self.entries.write();
        let id = format!("mem-{}", uuid::Uuid::new_v4());
        lock.insert(
            key.to_string(),
            MemoryEntry {
                id,
                key: key.to_string(),
                content: content.to_string(),
                category,
                timestamp: Self::now_timestamp(),
                session_id: session_id.map(ToString::to_string),
                score: None,
            },
        );
        Ok(())
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let lock = self.entries.read();
        let mut items = lock
            .values()
            .filter(|entry| {
                let session_match = match session_id {
                    Some(sid) => entry.session_id.as_deref() == Some(sid),
                    None => true,
                };
                session_match && Self::matches_query(&entry.content, query)
            })
            .cloned()
            .collect::<Vec<_>>();

        items.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        if limit > 0 {
            items.truncate(limit);
        }
        Ok(items)
    }

    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        Ok(self.entries.read().get(key).cloned())
    }

    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let lock = self.entries.read();
        let mut items = lock
            .values()
            .filter(|entry| {
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
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(items)
    }

    async fn forget(&self, key: &str) -> anyhow::Result<bool> {
        Ok(self.entries.write().remove(key).is_some())
    }

    async fn count(&self) -> anyhow::Result<usize> {
        Ok(self.entries.read().len())
    }

    async fn health_check(&self) -> bool {
        true
    }
}

#[derive(Clone)]
pub struct JsonFileMemory {
    entries: Arc<RwLock<HashMap<String, MemoryEntry>>>,
    path: PathBuf,
}

impl JsonFileMemory {
    pub async fn open(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let entries = if path.exists() {
            let raw = tokio::fs::read_to_string(&path).await.unwrap_or_default();
            let list: Vec<MemoryEntry> = serde_json::from_str(&raw).unwrap_or_default();
            list.into_iter()
                .map(|e| (e.key.clone(), e))
                .collect::<HashMap<_, _>>()
        } else {
            HashMap::new()
        };

        Ok(Self {
            entries: Arc::new(RwLock::new(entries)),
            path,
        })
    }

    async fn flush(&self) -> anyhow::Result<()> {
        let snapshot = {
            let lock = self.entries.read();
            lock.values().cloned().collect::<Vec<_>>()
        };
        let payload = serde_json::to_string_pretty(&snapshot)?;
        tokio::fs::write(&self.path, payload).await?;
        Ok(())
    }
}

#[async_trait]
impl Memory for JsonFileMemory {
    fn name(&self) -> &str {
        "json_file"
    }

    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        {
            let mut lock = self.entries.write();
            let id = format!("mem-{}", uuid::Uuid::new_v4());
            lock.insert(
                key.to_string(),
                MemoryEntry {
                    id,
                    key: key.to_string(),
                    content: content.to_string(),
                    category,
                    timestamp: InMemoryMemory::now_timestamp(),
                    session_id: session_id.map(ToString::to_string),
                    score: None,
                },
            );
        }
        self.flush().await
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let lock = self.entries.read();
        let mut items = lock
            .values()
            .filter(|entry| {
                let session_match = match session_id {
                    Some(sid) => entry.session_id.as_deref() == Some(sid),
                    None => true,
                };
                session_match && InMemoryMemory::matches_query(&entry.content, query)
            })
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        if limit > 0 {
            items.truncate(limit);
        }
        Ok(items)
    }

    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        Ok(self.entries.read().get(key).cloned())
    }

    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let lock = self.entries.read();
        let mut items = lock
            .values()
            .filter(|entry| {
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
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(items)
    }

    async fn forget(&self, key: &str) -> anyhow::Result<bool> {
        let existed = self.entries.write().remove(key).is_some();
        if existed {
            self.flush().await?;
        }
        Ok(existed)
    }

    async fn count(&self) -> anyhow::Result<usize> {
        Ok(self.entries.read().len())
    }

    async fn health_check(&self) -> bool {
        true
    }
}
