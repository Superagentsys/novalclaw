use crate::memory::traits::{Memory, MemoryCategory, MemoryEntry};
use crate::memory::search::rank_entries;
use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::warn;

#[derive(Clone)]
pub struct FileMemory {
    path: PathBuf,
    entries: Arc<RwLock<HashMap<String, MemoryEntry>>>,
}

impl FileMemory {
    pub fn new(path: PathBuf) -> Self {
        let entries = if path.exists() {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<HashMap<String, MemoryEntry>>(&content) {
                    Ok(map) => map,
                    Err(e) => {
                        warn!("Failed to parse memory file {:?}: {}", path, e);
                        HashMap::new()
                    }
                },
                Err(e) => {
                    warn!("Failed to read memory file {:?}: {}", path, e);
                    HashMap::new()
                }
            }
        } else {
            HashMap::new()
        };

        Self {
            path,
            entries: Arc::new(RwLock::new(entries)),
        }
    }

    fn save(&self) -> anyhow::Result<()> {
        let entries = self.entries.read();
        let content = serde_json::to_string_pretty(&*entries)?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, content)?;
        Ok(())
    }

    fn now_timestamp() -> String {
        time::OffsetDateTime::now_utc().unix_timestamp().to_string()
    }
}

#[async_trait]
impl Memory for FileMemory {
    fn name(&self) -> &str {
        "file_memory"
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
                    timestamp: Self::now_timestamp(),
                    session_id: session_id.map(ToString::to_string),
                    score: None,
                },
            );
        }
        self.save()
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let lock = self.entries.read();
        let query_lower = query.to_lowercase();
        
        let mut matches: Vec<MemoryEntry> = lock
            .values()
            .filter(|entry| {
                if let Some(sid) = session_id {
                    if entry.session_id.as_deref() != Some(sid) {
                        return false;
                    }
                }
                if query.trim().is_empty() {
                    return true;
                }
                entry.content.to_lowercase().contains(&query_lower) || 
                entry.key.to_lowercase().contains(&query_lower)
            })
            .cloned()
            .collect();
        matches = rank_entries(query, matches);
        matches.truncate(limit);
        
        Ok(matches)
    }

    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        let lock = self.entries.read();
        Ok(lock.get(key).cloned())
    }

    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let lock = self.entries.read();
        let mut results: Vec<MemoryEntry> = lock
            .values()
            .filter(|entry| {
                if let Some(cat) = category {
                    if &entry.category != cat {
                        return false;
                    }
                }
                if let Some(sid) = session_id {
                    if entry.session_id.as_deref() != Some(sid) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();
            
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(results)
    }

    async fn forget(&self, key: &str) -> anyhow::Result<bool> {
        let removed = {
            let mut lock = self.entries.write();
            lock.remove(key).is_some()
        };
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    async fn count(&self) -> anyhow::Result<usize> {
        let lock = self.entries.read();
        Ok(lock.len())
    }

    async fn health_check(&self) -> bool {
        // Try to read the file
        self.path.exists() || self.path.parent().map(|p| p.exists()).unwrap_or(false)
    }
}
