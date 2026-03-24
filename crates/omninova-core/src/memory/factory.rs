use crate::config::Config;
use crate::memory::backend::{InMemoryMemory, JsonFileMemory, MockMemory};
use crate::memory::search::SearchOptions;
use crate::memory::traits::Memory;
use std::sync::Arc;

pub async fn build_memory_from_config(config: &Config) -> anyhow::Result<Arc<dyn Memory>> {
    let backend = config.memory.backend.to_lowercase();
    let search_options = SearchOptions {
        expand_query: config.memory.search_expand_query,
        recency_weight: config.memory.search_recency_weight,
        recency_half_life_days: config.memory.search_recency_half_life_days,
    };
    match backend.as_str() {
        "mock" | "none" => Ok(Arc::new(MockMemory)),
        "in_memory" | "memory" => Ok(Arc::new(InMemoryMemory::new_with_options(
            search_options.clone(),
        ))),
        "json" | "json_file" | "sqlite" => {
            let path = config
                .memory
                .db_path
                .clone()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| config.workspace_dir.join(".omninova-memory.json"));
            let memory = JsonFileMemory::open_with_options(path, search_options).await?;
            Ok(Arc::new(memory))
        }
        _ => Ok(Arc::new(InMemoryMemory::new_with_options(search_options))),
    }
}
