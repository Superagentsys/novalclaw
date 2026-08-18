use crate::config::Config;
use crate::memory::backend::{InMemoryMemory, JsonFileMemory, MockMemory};
use crate::memory::embedding::EmbeddingClient;
use crate::memory::search::SearchOptions;
use crate::memory::semantic::SemanticMemory;
use crate::memory::sqlite_store::SqliteMemory;
use crate::memory::traits::Memory;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::warn;

const SQLITE_FILE: &str = ".omninova-memory.db";
const JSON_FILE: &str = ".omninova-memory.json";

pub async fn build_memory_from_config(config: &Config) -> anyhow::Result<Arc<dyn Memory>> {
    let search_options = search_options(config);
    match normalized_backend(config).as_str() {
        "mock" | "none" => Ok(Arc::new(MockMemory)),
        "in_memory" | "memory" => Ok(Arc::new(InMemoryMemory::new_with_options(search_options))),
        "json" | "json_file" => {
            let Some(path) = resolve_path(config, JSON_FILE) else {
                return Ok(fallback_without_workspace(search_options));
            };
            let memory = JsonFileMemory::open_with_options(path, search_options).await?;
            Ok(Arc::new(memory))
        }
        _ => {
            let Some(path) = resolve_path(config, SQLITE_FILE) else {
                return Ok(fallback_without_workspace(search_options));
            };
            Ok(build_sqlite(config, path, search_options)?)
        }
    }
}

/// Synchronous variant for constructors that cannot await, such as
/// [`crate::gateway::GatewayRuntime::new`].
///
/// Never fails: any backend error degrades to the in-process store with a
/// warning, because refusing to start the gateway over a memory file is worse
/// than running without durable memory.
pub fn build_memory_from_config_blocking(config: &Config) -> Arc<dyn Memory> {
    let search_options = search_options(config);
    match normalized_backend(config).as_str() {
        "mock" | "none" => Arc::new(MockMemory),
        "in_memory" | "memory" => Arc::new(InMemoryMemory::new_with_options(search_options)),
        "json" | "json_file" => {
            let Some(path) = resolve_path(config, JSON_FILE) else {
                return fallback_without_workspace(search_options);
            };
            match JsonFileMemory::open_blocking_with_options(path, search_options.clone()) {
                Ok(memory) => Arc::new(memory),
                Err(error) => {
                    warn!("json memory unavailable, using in-process memory: {error}");
                    Arc::new(InMemoryMemory::new_with_options(search_options))
                }
            }
        }
        _ => {
            let Some(path) = resolve_path(config, SQLITE_FILE) else {
                return fallback_without_workspace(search_options);
            };
            match build_sqlite(config, path, search_options.clone()) {
                Ok(memory) => memory,
                Err(error) => {
                    warn!("sqlite memory unavailable, using in-process memory: {error}");
                    Arc::new(InMemoryMemory::new_with_options(search_options))
                }
            }
        }
    }
}

fn build_sqlite(
    config: &Config,
    path: PathBuf,
    search_options: SearchOptions,
) -> anyhow::Result<Arc<dyn Memory>> {
    let store = Arc::new(SqliteMemory::open_with_options(path, search_options)?);
    match EmbeddingClient::from_config(&config.memory.embedding) {
        Some(embedder) => Ok(Arc::new(SemanticMemory::new(store, embedder))),
        None => Ok(store),
    }
}

fn search_options(config: &Config) -> SearchOptions {
    SearchOptions {
        expand_query: config.memory.search_expand_query,
        recency_weight: config.memory.search_recency_weight,
        recency_half_life_days: config.memory.search_recency_half_life_days,
    }
}

fn normalized_backend(config: &Config) -> String {
    config.memory.backend.trim().to_lowercase()
}

/// Explicit `db_path` wins; otherwise the file lives in the workspace. Returns
/// `None` when neither is available, which is the case for a freshly
/// constructed `Config::default()` (empty workspace) — writing a database into
/// the process CWD there would litter unrelated directories.
fn resolve_path(config: &Config, file_name: &str) -> Option<PathBuf> {
    if let Some(explicit) = config
        .memory
        .db_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(PathBuf::from(explicit));
    }
    let workspace = &config.workspace_dir;
    if workspace.as_os_str().is_empty() || workspace.components().next().is_none() {
        return None;
    }
    Some(workspace.join(file_name))
}

fn fallback_without_workspace(search_options: SearchOptions) -> Arc<dyn Memory> {
    Arc::new(InMemoryMemory::new_with_options(search_options))
}

#[cfg(test)]
mod tests {
    use super::{build_memory_from_config, build_memory_from_config_blocking};
    use crate::config::Config;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_workspace(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("omninova-factory-{label}-{nonce}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn default_config_without_workspace_stays_in_process() {
        let memory = build_memory_from_config_blocking(&Config::default());
        assert_eq!(memory.name(), "in_memory");
    }

    #[test]
    fn default_backend_is_durable_sqlite_when_workspace_is_set() {
        let workspace = temp_workspace("sqlite");
        let mut config = Config::default();
        config.workspace_dir = workspace.clone();

        let memory = build_memory_from_config_blocking(&config);

        assert_eq!(memory.name(), "sqlite");
        assert!(workspace.join(".omninova-memory.db").exists());
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn mock_backend_is_respected() {
        let mut config = Config::default();
        config.memory.backend = "none".into();
        assert_eq!(build_memory_from_config_blocking(&config).name(), "mock_memory");
    }

    #[tokio::test]
    async fn async_builder_matches_blocking_backend_choice() {
        let workspace = temp_workspace("async");
        let mut config = Config::default();
        config.workspace_dir = workspace.clone();

        let memory = build_memory_from_config(&config).await.unwrap();

        assert_eq!(memory.name(), "sqlite");
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[tokio::test]
    async fn json_backend_is_still_available() {
        let workspace = temp_workspace("json");
        let mut config = Config::default();
        config.workspace_dir = workspace.clone();
        config.memory.backend = "json".into();

        let memory = build_memory_from_config(&config).await.unwrap();

        assert_eq!(memory.name(), "json_file");
        let _ = std::fs::remove_dir_all(workspace);
    }
}
