use omninova_core::account::{AccountStore, AccountInfo, NewAccount, AccountUpdate};
use omninova_core::agent::{AgentStore, AgentUpdate, NewAgent, MbtiType, PersonalityTraits, PersonalityConfig, AgentService, ChatResult, StreamManager, StreamEvent, CommandRegistry, CommandContext, CommandResult, CommandInfo, parse_command, AgentStyleConfig};
use omninova_core::memory::{WorkingMemory, WorkingMemoryEntry, MemoryStats, EpisodicMemoryStore, EpisodicMemory, NewEpisodicMemory, EpisodicMemoryStats, SemanticMemoryStore, SemanticMemory, SemanticMemoryStats, SemanticSearchResult, NewSemanticMemory, EmbeddingService, DEFAULT_EMBEDDING_DIM, DEFAULT_OPENAI_EMBEDDING_MODEL, MemoryManager, MemoryLayer, MemoryQuery, MemoryQueryResult, MemoryManagerStats, UnifiedMemoryEntry, PerformanceStats, BenchmarkResults};
use omninova_core::backup::{
    BackupService, BackupMeta, BackupFormat,
    ImportMode, ImportOptions,
    deserialize_backup, validate_backup,
};
use omninova_core::channels::{ChannelKind, InboundMessage, ChannelManager, ChannelInfo as CoreChannelInfo};
use omninova_core::channels::traits::{ChannelConfig, ChannelSettings};
use omninova_core::channels::types::Credentials;
use omninova_core::channels::behavior::{ChannelBehaviorConfig, ChannelBehaviorStore};
use omninova_core::config::{Config, ModelProviderConfig, ProviderConfig, RobotConfig, ChannelsConfig, ChannelEntry, ConfigManager};
use omninova_core::db::{create_pool, create_builtin_runner, DbPool, DbPoolConfig};
use omninova_core::gateway::{
    GatewayHealth, GatewayInboundResponse, GatewayRuntime, GatewaySessionTreeQuery,
    GatewaySessionTreeResponse,
    auth::{ApiKeyStore, ApiKeyPermission, CreateApiKeyRequest, ApiKeyCreated, ApiKeyInfo},
    logging::{ApiLogStore, ApiRequestLog, RequestLogFilter, ApiUsageStats, EndpointStats, ApiKeyStats},
};
use omninova_core::memory::factory::build_memory_from_config;
use omninova_core::privacy::{
    PrivacySettings, StorageInfo, ClearOptions, ClearResult,
};
use omninova_core::providers::{ProviderSelection, build_provider_with_selection, ProviderStore, NewProviderConfig, ProviderConfigUpdate};
use omninova_core::routing::RouteDecision;
use omninova_core::security::{EncryptionKeyManager, KeyringService, KeyReference};
use omninova_core::session::{Message, MessageStore, NewMessage, NewSession, Session, SessionStore, SessionUpdate};
use omninova_core::skills::import_skills_from_dir;
use omninova_core::skills::{SkillRegistry, SkillExecutor, SkillMetadata, SkillResult, SkillError, SkillContext, Skill, OpenClawSkillAdapter};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::{Mutex, broadcast};
use tokio::task::JoinHandle;
use tokio::time::{Duration, sleep};

struct AppState {
    runtime: GatewayRuntime,
    gateway_task: Option<JoinHandle<Result<(), String>>>,
    last_gateway_error: Option<String>,
    /// Broadcast channel for gateway status events (Task 2.5)
    gateway_status_tx: broadcast::Sender<GatewayStatusPayload>,
    db_pool: Option<DbPool>,
    agent_store: Option<AgentStore>,
    account_store: Option<AccountStore>,
    provider_store: Option<ProviderStore>,
    session_store: Option<SessionStore>,
    message_store: Option<MessageStore>,
    /// Config manager with file watcher for hot reload.
    /// This field keeps the watcher alive for the lifetime of the app.
    #[allow(dead_code)]
    config_manager: Option<Arc<ConfigManager>>,
    /// Keyring service for secure API key storage
    keyring_service: Option<Arc<KeyringService>>,
    /// Stream manager for tracking active streaming sessions
    stream_manager: StreamManager,
    /// Command registry for chat command execution (e.g., /help, /clear, /export)
    command_registry: Arc<CommandRegistry>,
    /// Working memory (L1) for short-term session context
    working_memory: Arc<Mutex<WorkingMemory>>,
    /// Episodic memory store (L2) for long-term memory
    episodic_memory_store: Option<Arc<EpisodicMemoryStore>>,
    /// Semantic memory store (L3) for vector-based similarity search
    semantic_memory_store: Option<Arc<SemanticMemoryStore>>,
    /// Unified Memory Manager coordinating L1, L2, and L3
    memory_manager: Option<Arc<Mutex<MemoryManager>>>,
    /// Channel manager for multi-channel connectivity (Story 6.7)
    channel_manager: Option<Arc<Mutex<ChannelManager>>>,
    /// Channel behavior config store (Story 6.8)
    channel_behavior_store: Option<Arc<omninova_core::channels::behavior::SqliteBehaviorStore>>,
    /// Skill registry for skill management (Story 7.5)
    skill_registry: Option<Arc<SkillRegistry>>,
    /// Skill executor for tracking execution logs (Story 7.6)
    skill_executor: Option<Arc<SkillExecutor>>,
    /// API Key store for gateway authentication (Story 8.3)
    api_key_store: Option<Arc<ApiKeyStore>>,
    /// API Log store for request logging (Story 8.4)
    api_log_store: Option<Arc<ApiLogStore>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SetupProviderConfig {
    id: String,
    name: String,
    #[serde(rename = "type")]
    provider_type: String,
    api_key_env: Option<String>,
    base_url: Option<String>,
    #[serde(default)]
    models: Vec<String>,
    #[serde(default)]
    enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SetupAppConfig {
    api_key: Option<String>,
    api_url: Option<String>,
    default_provider: Option<String>,
    default_model: Option<String>,
    workspace_dir: String,
    omninoval_gateway_url: Option<String>,
    omninoval_config_dir: Option<String>,
    robot: Option<RobotConfig>,
    #[serde(default)]
    providers: Vec<SetupProviderConfig>,
    #[serde(default)]
    channels: Option<SetupChannelsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SetupChannelsConfig {
    #[serde(default)]
    telegram: Option<SetupChannelEntry>,
    #[serde(default)]
    discord: Option<SetupChannelEntry>,
    #[serde(default)]
    slack: Option<SetupChannelEntry>,
    #[serde(default)]
    whatsapp: Option<SetupChannelEntry>,
    #[serde(default)]
    wechat: Option<SetupChannelEntry>,
    #[serde(default)]
    feishu: Option<SetupChannelEntry>,
    #[serde(default)]
    lark: Option<SetupChannelEntry>,
    #[serde(default)]
    dingtalk: Option<SetupChannelEntry>,
    #[serde(default)]
    matrix: Option<SetupChannelEntry>,
    #[serde(default)]
    email: Option<SetupChannelEntry>,
    #[serde(default)]
    msteams: Option<SetupChannelEntry>,
    #[serde(default)]
    irc: Option<SetupChannelEntry>,
    #[serde(default)]
    webhook: Option<SetupChannelEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SetupChannelEntry {
    #[serde(default)]
    enabled: bool,
    token: Option<String>,
    token_env: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct GatewayStatusPayload {
    running: bool,
    url: String,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UiInboundPayload {
    #[serde(default)]
    channel: Option<ChannelKind>,
    user_id: Option<String>,
    session_id: Option<String>,
    text: String,
    #[serde(default)]
    metadata: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderHealthPayload {
    id: String,
    name: String,
    enabled: bool,
    is_default: bool,
    model: Option<String>,
    base_url: Option<String>,
    healthy: Option<bool>,
}

#[tauri::command]
async fn process_message(
    message: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let runtime = {
        let app_state = state.lock().await;
        app_state.runtime.clone()
    };
    runtime
        .chat(&message)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_config(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let runtime = {
        let app_state = state.lock().await;
        app_state.runtime.clone()
    };
    let cfg = runtime.get_config().await;
    serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_config(
    config_json: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let new_cfg: Config =
        serde_json::from_str(&config_json).map_err(|e| format!("Invalid config JSON: {e}"))?;

    new_cfg
        .validate_or_bail()
        .map_err(|e| format!("Config validation failed: {e}"))?;

    let runtime = {
        let app_state = state.lock().await;
        app_state.runtime.clone()
    };
    runtime.set_config(new_cfg.clone()).await.map_err(|e| e.to_string())?;
    let cfg = runtime.get_config().await;
    cfg.save().map_err(|e| e.to_string())?;

    // Emit config:changed event
    let timestamp = chrono::Local::now().to_rfc3339();
    let payload = serde_json::json!({
        "config": new_cfg,
        "timestamp": timestamp
    });
    app.emit("config:changed", &payload)
        .map_err(|e| format!("Failed to emit config:changed event: {e}"))?;

    Ok(())
}

#[tauri::command]
async fn reload_config(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let cfg = match Config::load_or_init() {
        Ok(c) => c,
        Err(e) => {
            // Emit config:error event
            let timestamp = chrono::Local::now().to_rfc3339();
            let payload = serde_json::json!({
                "error": e.to_string(),
                "timestamp": timestamp
            });
            let _ = app.emit("config:error", &payload);
            return Err(e.to_string());
        }
    };

    let runtime = {
        let app_state = state.lock().await;
        app_state.runtime.clone()
    };
    runtime.set_config(cfg.clone()).await.map_err(|e| e.to_string())?;

    // Emit config:changed event
    let timestamp = chrono::Local::now().to_rfc3339();
    let payload = serde_json::json!({
        "config": cfg,
        "timestamp": timestamp
    });
    app.emit("config:changed", &payload)
        .map_err(|e| format!("Failed to emit config:changed event: {e}"))?;

    serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())
}

/// Get the path to the configuration file
#[tauri::command]
fn get_config_path() -> String {
    omninova_core::config::resolve_config_path()
        .to_string_lossy()
        .to_string()
}

/// Subscribe to config change notifications
/// Returns the current config and enables event emission for future changes
#[tauri::command]
async fn subscribe_config_changes(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let runtime = {
        let app_state = state.lock().await;
        app_state.runtime.clone()
    };
    let cfg = runtime.get_config().await;

    // Emit initial config
    let config_json = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
    app.emit("config:initial", &config_json)
        .map_err(|e| format!("Failed to emit config:initial event: {e}"))?;

    Ok(config_json)
}

#[tauri::command]
async fn get_setup_config(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<SetupAppConfig, String> {
    let runtime = {
        let app_state = state.lock().await;
        app_state.runtime.clone()
    };

    let cfg = runtime.get_config().await;
    Ok(setup_config_from_core(&cfg))
}

#[tauri::command]
async fn save_setup_config(
    config: SetupAppConfig,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let state_ref = state.inner().clone();
    sync_gateway_task_state(&state_ref).await;

    let runtime = {
        let app_state = state_ref.lock().await;
        app_state.runtime.clone()
    };

    let current = runtime.get_config().await;
    let current_gateway_url = format!("http://{}:{}", current.gateway.host, current.gateway.port);
    let next = setup_config_to_core(current, config)?;
    let next_gateway_url = format!("http://{}:{}", next.gateway.host, next.gateway.port);

    next.save().map_err(|e| e.to_string())?;
    next.save_active_workspace().map_err(|e| e.to_string())?;
    runtime.set_config(next).await.map_err(|e| e.to_string())?;

    if current_gateway_url != next_gateway_url {
        stop_gateway_inner(&state_ref).await;
    }

    Ok(())
}

#[tauri::command]
async fn gateway_status(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<GatewayStatusPayload, String> {
    let state_ref = state.inner().clone();
    sync_gateway_task_state(&state_ref).await;
    Ok(gateway_status_from_state(&state_ref).await)
}

#[tauri::command]
async fn gateway_health(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<GatewayHealth, String> {
    let runtime = {
        let app_state = state.lock().await;
        app_state.runtime.clone()
    };
    Ok(runtime.health().await)
}

#[tauri::command]
async fn provider_health_overview(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<ProviderHealthPayload>, String> {
    let runtime = {
        let app_state = state.lock().await;
        app_state.runtime.clone()
    };
    let cfg = runtime.get_config().await;

    let provider_ids = collect_provider_ids(&cfg);
    let mut items = Vec::with_capacity(provider_ids.len());
    for id in provider_ids {
        let enabled = cfg
            .model_providers
            .get(&id)
            .map(|provider| provider.enabled)
            .or_else(|| {
                cfg.providers
                    .iter()
                    .find(|provider| provider.id == id)
                    .map(|provider| provider.enabled)
            })
            .unwrap_or(id == cfg.default_provider.clone().unwrap_or_default());
        let model = cfg
            .model_providers
            .get(&id)
            .and_then(|provider| provider.default_model.clone())
            .or_else(|| {
                cfg.providers
                    .iter()
                    .find(|provider| provider.id == id)
                    .and_then(|provider| provider.models.first().cloned())
            })
            .or_else(|| cfg.default_model.clone());
        let base_url = cfg
            .model_providers
            .get(&id)
            .and_then(|provider| provider.base_url.clone())
            .or_else(|| {
                cfg.providers
                    .iter()
                    .find(|provider| provider.id == id)
                    .and_then(|provider| provider.base_url.clone())
            })
            .or_else(|| default_provider_base_url(&id, &cfg));
        let healthy = if enabled {
            let provider = build_provider_with_selection(
                &cfg,
                &ProviderSelection {
                    provider: Some(id.clone()),
                    model: model.clone(),
                },
            );
            Some(provider.health_check().await)
        } else {
            None
        };
        items.push(ProviderHealthPayload {
            name: display_provider_name(&id),
            id: id.clone(),
            enabled,
            is_default: cfg.default_provider.as_deref() == Some(id.as_str()),
            model,
            base_url,
            healthy,
        });
    }
    Ok(items)
}

#[tauri::command]
async fn route_inbound_message(
    payload: UiInboundPayload,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<RouteDecision, String> {
    let runtime = {
        let app_state = state.lock().await;
        app_state.runtime.clone()
    };
    let inbound = inbound_from_payload(payload);
    Ok(runtime.route(&inbound).await)
}

#[tauri::command]
async fn process_inbound_message(
    payload: UiInboundPayload,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<GatewayInboundResponse, String> {
    let runtime = {
        let app_state = state.lock().await;
        app_state.runtime.clone()
    };
    let inbound = inbound_from_payload(payload);
    runtime
        .process_inbound(&inbound)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn session_tree_snapshot(
    query: Option<GatewaySessionTreeQuery>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<GatewaySessionTreeResponse, String> {
    let runtime = {
        let app_state = state.lock().await;
        app_state.runtime.clone()
    };
    let query = query.unwrap_or_default();
    runtime
        .session_tree_snapshot_filtered(&query)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
struct DepStatusPayload {
    name: String,
    installed: bool,
    version: Option<String>,
    detail: String,
}

#[tauri::command]
async fn check_browser_dep() -> Result<DepStatusPayload, String> {
    let status = check_command_installed("agent-browser", "--version").await;
    Ok(status)
}

#[tauri::command]
async fn install_browser_dep() -> Result<DepStatusPayload, String> {
    let npm_out = tokio::process::Command::new("npm")
        .args(["install", "-g", "agent-browser"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("npm install failed: {e}"))?;
    if !npm_out.status.success() {
        let stderr = String::from_utf8_lossy(&npm_out.stderr);
        return Err(format!("npm install -g agent-browser failed: {stderr}"));
    }

    let chromium_out = tokio::process::Command::new("agent-browser")
        .arg("install")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("agent-browser install failed: {e}"))?;
    if !chromium_out.status.success() {
        let stderr = String::from_utf8_lossy(&chromium_out.stderr);
        return Err(format!("agent-browser install (Chromium) failed: {stderr}"));
    }

    let status = check_command_installed("agent-browser", "--version").await;
    Ok(status)
}

async fn check_command_installed(bin: &str, version_flag: &str) -> DepStatusPayload {
    match tokio::process::Command::new(bin)
        .arg(version_flag)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let version = raw
                .split_whitespace()
                .find(|s| s.chars().next().map_or(false, |c| c.is_ascii_digit()))
                .map(ToString::to_string);
            DepStatusPayload {
                name: bin.to_string(),
                installed: true,
                version,
                detail: raw,
            }
        }
        _ => DepStatusPayload {
            name: bin.to_string(),
            installed: false,
            version: None,
            detail: "not installed".to_string(),
        },
    }
}

#[tauri::command]
async fn start_gateway(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<GatewayStatusPayload, String> {
    let state_ref = state.inner().clone();
    sync_gateway_task_state(&state_ref).await;

    {
        let mut app_state = state_ref.lock().await;
        if app_state.gateway_task.is_none() {
            let runtime = app_state.runtime.clone();
            app_state.last_gateway_error = None;
            app_state.gateway_task = Some(tokio::spawn(async move {
                runtime.serve_http().await.map_err(|error| error.to_string())
            }));
        }
    }

    sleep(Duration::from_millis(250)).await;
    sync_gateway_task_state(&state_ref).await;
    let status = gateway_status_from_state(&state_ref).await;

    if !status.running {
        // Emit error event
        let _ = app.emit("gateway:error", &status);
        return Err(
            status
                .last_error
                .clone()
                .unwrap_or_else(|| "网关启动失败".to_string()),
        );
    }

    // Emit started event via broadcast channel and Tauri event
    {
        let app_state = state_ref.lock().await;
        let _ = app_state.gateway_status_tx.send(status.clone());
    }
    let _ = app.emit("gateway:started", &status);

    Ok(status)
}

#[tauri::command]
async fn stop_gateway(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<GatewayStatusPayload, String> {
    let state_ref = state.inner().clone();
    stop_gateway_inner(&state_ref).await;
    let status = gateway_status_from_state(&state_ref).await;

    // Emit stopped event via broadcast channel and Tauri event
    {
        let app_state = state_ref.lock().await;
        let _ = app_state.gateway_status_tx.send(status.clone());
    }
    let _ = app.emit("gateway:stopped", &status);

    Ok(status)
}

#[tauri::command]
async fn import_skills(
    source_dir: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;
    let config = app_state.runtime.get_config().await;

    let target = config.skills.open_skills_dir.as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| config.workspace_dir.join("skills"));

    let source = PathBuf::from(source_dir);

    match import_skills_from_dir(&source, &target, true) {
        Ok(count) => Ok(format!("Successfully imported {} skills.", count)),
        Err(e) => Err(e.to_string()),
    }
}

// ============================================================================
// Database Commands
// ============================================================================

/// Database status response
#[derive(Debug, Clone, Serialize)]
struct DatabaseStatus {
    /// Whether the database is initialized
    initialized: bool,
    /// Path to the database file
    path: String,
    /// Number of applied migrations
    migrations_applied: usize,
    /// Number of pending migrations
    migrations_pending: usize,
    /// Whether WAL mode is enabled
    wal_enabled: bool,
}

/// Initialize the database
#[tauri::command]
async fn init_database(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<DatabaseStatus, String> {
    let mut app_state = state.lock().await;

    // Check if already initialized
    if app_state.db_pool.is_some() {
        let pool = app_state.db_pool.as_ref().unwrap();
        return get_database_status_from_pool(pool);
    }

    // Get default database path
    let db_path = omninova_core::db::pool::default_db_path()
        .map_err(|e| format!("Failed to get database path: {e}"))?;

    // Create the pool
    let pool = create_pool(&db_path, DbPoolConfig::default())
        .map_err(|e| format!("Failed to create database pool: {e}"))?;

    // Run migrations
    let conn = pool.get()
        .map_err(|e| format!("Failed to get database connection: {e}"))?;

    let runner = create_builtin_runner();
    let report = runner.run(&conn)
        .map_err(|e| format!("Failed to run migrations: {e}"))?;

    tracing::info!(
        "Database initialized: {} migrations applied, {} skipped",
        report.applied.len(),
        report.skipped.len()
    );

    // Store the pool
    app_state.db_pool = Some(pool.clone());

    get_database_status_from_pool(&pool)
}

/// Get database status
#[tauri::command]
async fn get_database_status(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<DatabaseStatus, String> {
    let app_state = state.lock().await;

    match &app_state.db_pool {
        Some(pool) => get_database_status_from_pool(pool),
        None => {
            let db_path = omninova_core::db::pool::default_db_path()
                .map_err(|e| format!("Failed to get database path: {e}"))?;

            Ok(DatabaseStatus {
                initialized: false,
                path: db_path.to_string_lossy().to_string(),
                migrations_applied: 0,
                migrations_pending: 1,
                wal_enabled: false,
            })
        }
    }
}

/// Helper to get database status from pool
fn get_database_status_from_pool(pool: &DbPool) -> Result<DatabaseStatus, String> {
    let conn = pool.get()
        .map_err(|e| format!("Failed to get connection: {e}"))?;

    // Check WAL mode
    let wal_mode: String = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap_or_else(|_| "unknown".to_string());

    // Get migration status
    let runner = create_builtin_runner();
    let status = runner.status(&conn)
        .map_err(|e| format!("Failed to get migration status: {e}"))?;

    let db_path = omninova_core::db::pool::default_db_path()
        .map_err(|e| format!("Failed to get database path: {e}"))?;

    Ok(DatabaseStatus {
        initialized: true,
        path: db_path.to_string_lossy().to_string(),
        migrations_applied: status.applied,
        migrations_pending: status.pending,
        wal_enabled: wal_mode.to_lowercase() == "wal",
    })
}

// ============================================================================
// Agent Commands
// ============================================================================

/// Initialize agent store
#[tauri::command]
async fn init_agent_store(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let mut app_state = state.lock().await;

    // Check if already initialized
    if app_state.agent_store.is_some() {
        return Ok(());
    }

    // Ensure database pool is initialized
    let pool = if let Some(pool) = &app_state.db_pool {
        pool.clone()
    } else {
        let db_path = omninova_core::db::pool::default_db_path()
            .map_err(|e| format!("Failed to get database path: {e}"))?;

        let pool = create_pool(&db_path, DbPoolConfig::default())
            .map_err(|e| format!("Failed to create database pool: {e}"))?;

        // Run migrations
        let conn = pool.get()
            .map_err(|e| format!("Failed to get database connection: {e}"))?;

        let runner = create_builtin_runner();
        runner.run(&conn)
            .map_err(|e| format!("Failed to run migrations: {e}"))?;

        app_state.db_pool = Some(pool.clone());
        pool
    };

    // Create agent store
    let store = AgentStore::new(pool);
    store.initialize().map_err(|e| e.to_string())?;

    app_state.agent_store = Some(store);
    Ok(())
}

/// Get all agents
#[tauri::command]
async fn get_agents(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;

    let agents = store.find_all().map_err(|e| e.to_string())?;
    serde_json::to_string(&agents).map_err(|e| e.to_string())
}

/// Get agent by UUID
#[tauri::command]
async fn get_agent_by_id(
    uuid: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;

    match store.find_by_uuid(&uuid).map_err(|e| e.to_string())? {
        Some(agent) => serde_json::to_string(&agent).map_err(|e| e.to_string()),
        None => Err(format!("Agent not found: {}", uuid)),
    }
}

/// Create a new agent
#[tauri::command]
async fn create_agent(
    agent_json: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;

    let agent: NewAgent = serde_json::from_str(&agent_json)
        .map_err(|e| format!("Invalid agent JSON: {e}"))?;

    let created = store.create(&agent).map_err(|e| e.to_string())?;
    serde_json::to_string(&created).map_err(|e| e.to_string())
}

/// Update an agent
#[tauri::command]
async fn update_agent(
    uuid: String,
    updates_json: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;

    let updates: AgentUpdate = serde_json::from_str(&updates_json)
        .map_err(|e| format!("Invalid update JSON: {e}"))?;

    let updated = store.update(&uuid, &updates).map_err(|e| e.to_string())?;
    serde_json::to_string(&updated).map_err(|e| e.to_string())
}

/// Delete an agent
#[tauri::command]
async fn delete_agent(
    uuid: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let app_state = state.lock().await;

    let store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;

    store.delete(&uuid).map_err(|e| e.to_string())
}

/// Duplicate an agent
///
/// Creates a copy of an existing agent with:
/// - New UUID
/// - Name suffixed with " (副本)"
/// - Status always set to 'active'
/// - Copied configuration fields (description, domain, mbti_type, system_prompt)
#[tauri::command]
async fn duplicate_agent(
    uuid: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;

    let duplicated = store.duplicate(&uuid).map_err(|e| e.to_string())?;
    serde_json::to_string(&duplicated).map_err(|e| e.to_string())
}

// ============================================================================
// Agent Style Configuration Commands (Story 7.1)
// ============================================================================

/// Get agent style configuration
#[tauri::command]
async fn get_agent_style_config(
    uuid: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;

    let agent = store.find_by_uuid(&uuid).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Agent not found: {}", uuid))?;

    let style_config = agent.get_style_config();
    serde_json::to_string(&style_config).map_err(|e| e.to_string())
}

/// Update agent style configuration
#[tauri::command]
async fn update_agent_style_config(
    uuid: String,
    style_config_json: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;

    // Validate the style config JSON by parsing it
    let _config: omninova_core::agent::AgentStyleConfig = serde_json::from_str(&style_config_json)
        .map_err(|e| format!("Invalid style config JSON: {e}"))?;

    let updated = store.update_style_config(&uuid, &style_config_json).map_err(|e| e.to_string())?;
    serde_json::to_string(&updated).map_err(|e| e.to_string())
}

/// Preview style effect on sample text
#[tauri::command]
async fn preview_style_effect(
    style_config_json: String,
    sample_text: String,
) -> Result<String, String> {
    use omninova_core::channels::behavior::ResponseStyleProcessor;

    let config: omninova_core::agent::AgentStyleConfig = serde_json::from_str(&style_config_json)
        .map_err(|e| format!("Invalid style config JSON: {e}"))?;

    // Apply style transformation
    let mut result = ResponseStyleProcessor::apply_style(&sample_text, config.response_style);

    // Apply max length if specified
    if config.max_response_length > 0 {
        result = ResponseStyleProcessor::truncate(&result, config.max_response_length);
    }

    Ok(result)
}

/// Get agent context window configuration
/// [Source: Story 7.2 - 上下文窗口配置]
#[tauri::command]
async fn get_context_window_config(
    uuid: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;

    let agent = store.find_by_uuid(&uuid).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Agent not found: {}", uuid))?;

    let config = agent.get_context_window_config();
    serde_json::to_string(&config).map_err(|e| e.to_string())
}

/// Update agent context window configuration
/// [Source: Story 7.2 - 上下文窗口配置]
#[tauri::command]
async fn update_context_window_config(
    uuid: String,
    config_json: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;

    // Validate the config JSON by parsing it
    let _config: omninova_core::agent::ContextWindowConfig = serde_json::from_str(&config_json)
        .map_err(|e| format!("Invalid context window config JSON: {e}"))?;

    let updated = store.update_context_window_config(&uuid, &config_json).map_err(|e| e.to_string())?;
    serde_json::to_string(&updated).map_err(|e| e.to_string())
}

/// Estimate token count for text
/// [Source: Story 7.2 - 上下文窗口配置]
#[tauri::command]
async fn estimate_tokens(
    text: String,
) -> Result<usize, String> {
    use omninova_core::agent::TokenCounter;
    Ok(TokenCounter::count_text(&text))
}

/// Get model context window recommendations
/// [Source: Story 7.2 - 上下文窗口配置]
#[tauri::command]
async fn get_model_context_recommendations(
    model_name: String,
) -> Result<Option<(usize, usize)>, String> {
    use omninova_core::agent::get_model_context_recommendation;
    Ok(get_model_context_recommendation(&model_name))
}

// ============================================================================
// Agent Trigger Keywords Configuration Commands (Story 7.3)
// ============================================================================

/// Get agent trigger keywords configuration
/// [Source: Story 7.3 - 触发关键词配置]
#[tauri::command]
async fn get_trigger_keywords_config(
    uuid: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;

    let agent = store.find_by_uuid(&uuid).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Agent not found: {}", uuid))?;

    let config = agent.get_trigger_keywords_config();
    serde_json::to_string(&config).map_err(|e| e.to_string())
}

/// Update agent trigger keywords configuration
/// [Source: Story 7.3 - 触发关键词配置]
#[tauri::command]
async fn update_trigger_keywords_config(
    uuid: String,
    config_json: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;

    // Validate the config JSON by parsing it
    let _config: omninova_core::agent::AgentTriggerConfig = serde_json::from_str(&config_json)
        .map_err(|e| format!("Invalid trigger keywords config JSON: {e}"))?;

    let updated = store.update_trigger_keywords_config(&uuid, &config_json).map_err(|e| e.to_string())?;
    serde_json::to_string(&updated).map_err(|e| e.to_string())
}

/// Test trigger keyword match against sample text
/// [Source: Story 7.3 - 触发关键词配置]
#[tauri::command]
async fn test_trigger_match(
    config_json: String,
    test_text: String,
) -> Result<String, String> {
    use omninova_core::agent::TriggerConfigService;

    let config: omninova_core::agent::AgentTriggerConfig = serde_json::from_str(&config_json)
        .map_err(|e| format!("Invalid trigger keywords config JSON: {e}"))?;

    let result = TriggerConfigService::test_all_keywords(&config.keywords, &test_text);
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Test a single trigger keyword against sample text
/// [Source: Story 7.3 - 触发关键词配置]
#[tauri::command]
async fn test_single_trigger(
    keyword_json: String,
    test_text: String,
) -> Result<String, String> {
    use omninova_core::agent::TriggerConfigService;
    use omninova_core::channels::behavior::TriggerKeyword;

    let keyword: TriggerKeyword = serde_json::from_str(&keyword_json)
        .map_err(|e| format!("Invalid trigger keyword JSON: {e}"))?;

    let result = TriggerConfigService::test_trigger(&keyword, &test_text);
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Validate a trigger keyword pattern
/// [Source: Story 7.3 - 触发关键词配置]
#[tauri::command]
async fn validate_trigger_keyword(
    keyword_json: String,
) -> Result<(), String> {
    use omninova_core::agent::TriggerConfigService;
    use omninova_core::channels::behavior::TriggerKeyword;

    let keyword: TriggerKeyword = serde_json::from_str(&keyword_json)
        .map_err(|e| format!("Invalid trigger keyword JSON: {e}"))?;

    TriggerConfigService::validate_keyword(&keyword)
}

// ============================================================================
// Privacy Config Commands
// ============================================================================

/// Get agent privacy configuration
/// [Source: Story 7.4 - 数据处理与隐私设置]
#[tauri::command]
async fn get_privacy_config(
    uuid: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;

    let agent = store.find_by_uuid(&uuid).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Agent not found: {}", uuid))?;

    let config = agent.get_privacy_config();
    serde_json::to_string(&config).map_err(|e| e.to_string())
}

/// Update agent privacy configuration
/// [Source: Story 7.4 - 数据处理与隐私设置]
#[tauri::command]
async fn update_privacy_config(
    uuid: String,
    config_json: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;

    // Validate the config JSON by parsing it
    let _config: omninova_core::agent::AgentPrivacyConfig = serde_json::from_str(&config_json)
        .map_err(|e| format!("Invalid privacy config JSON: {e}"))?;

    let updated = store.update_privacy_config(&uuid, &config_json).map_err(|e| e.to_string())?;
    serde_json::to_string(&updated).map_err(|e| e.to_string())
}

// ============================================================================
// Unified Agent Configuration Commands (Story 7.7)
// ============================================================================

/// Unified agent configuration response
#[derive(serde::Serialize)]
struct AgentConfigurationResponse {
    style_config: Option<String>,
    context_window_config: Option<String>,
    trigger_keywords_config: Option<String>,
    privacy_config: Option<String>,
}

/// Get unified agent configuration
/// [Source: Story 7.7 - ConfigurationPanel 组件]
#[tauri::command]
async fn get_agent_configuration(
    agent_id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<AgentConfigurationResponse, String> {
    let app_state = state.lock().await;

    let store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;

    let agent = store.find_by_uuid(&agent_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Agent not found: {}", agent_id))?;

    // Get all configs
    let style_config = Some(serde_json::to_string(&agent.get_style_config()).map_err(|e| e.to_string())?);
    let context_window_config = Some(serde_json::to_string(&agent.get_context_window_config()).map_err(|e| e.to_string())?);
    let trigger_keywords_config = Some(serde_json::to_string(&agent.get_trigger_keywords_config()).map_err(|e| e.to_string())?);
    let privacy_config = Some(serde_json::to_string(&agent.get_privacy_config()).map_err(|e| e.to_string())?);

    Ok(AgentConfigurationResponse {
        style_config,
        context_window_config,
        trigger_keywords_config,
        privacy_config,
    })
}

/// Update unified agent configuration
/// [Source: Story 7.7 - ConfigurationPanel 组件]
#[tauri::command]
async fn update_agent_configuration(
    agent_id: String,
    style_config: Option<String>,
    context_config: Option<String>,
    trigger_config: Option<String>,
    privacy_config: Option<String>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let app_state = state.lock().await;

    let store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;

    // Verify agent exists
    let _agent = store.find_by_uuid(&agent_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Agent not found: {}", agent_id))?;

    // Update each config if provided
    if let Some(config) = style_config {
        let _parsed: omninova_core::agent::AgentStyleConfig = serde_json::from_str(&config)
            .map_err(|e| format!("Invalid style config JSON: {e}"))?;
        store.update_style_config(&agent_id, &config).map_err(|e| e.to_string())?;
    }

    if let Some(config) = context_config {
        let _parsed: omninova_core::agent::ContextWindowConfig = serde_json::from_str(&config)
            .map_err(|e| format!("Invalid context config JSON: {e}"))?;
        store.update_context_window_config(&agent_id, &config).map_err(|e| e.to_string())?;
    }

    if let Some(config) = trigger_config {
        let _parsed: omninova_core::agent::AgentTriggerConfig = serde_json::from_str(&config)
            .map_err(|e| format!("Invalid trigger config JSON: {e}"))?;
        store.update_trigger_keywords_config(&agent_id, &config).map_err(|e| e.to_string())?;
    }

    if let Some(config) = privacy_config {
        let _parsed: omninova_core::agent::AgentPrivacyConfig = serde_json::from_str(&config)
            .map_err(|e| format!("Invalid privacy config JSON: {e}"))?;
        store.update_privacy_config(&agent_id, &config).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Test sensitive data filter against sample content
/// [Source: Story 7.4 - 数据处理与隐私设置]
#[tauri::command]
async fn test_sensitive_filter(
    filter_json: String,
    content: String,
) -> Result<String, String> {
    use omninova_core::agent::PrivacyConfigService;
    use omninova_core::agent::SensitiveDataFilter;

    let filter: SensitiveDataFilter = serde_json::from_str(&filter_json)
        .map_err(|e| format!("Invalid sensitive filter JSON: {e}"))?;

    let filtered = PrivacyConfigService::filter_content(&filter, &content);
    serde_json::to_string(&filtered).map_err(|e| e.to_string())
}

/// Validate exclusion rule pattern
/// [Source: Story 7.4 - 数据处理与隐私设置]
#[tauri::command]
async fn validate_exclusion_pattern(
    pattern: String,
) -> Result<(), String> {
    use omninova_core::agent::PrivacyConfigService;

    PrivacyConfigService::validate_exclusion_pattern(&pattern)
}

/// Validate custom filter pattern
/// [Source: Story 7.4 - 数据处理与隐私设置]
#[tauri::command]
async fn validate_filter_pattern(
    pattern: String,
) -> Result<(), String> {
    use omninova_core::agent::PrivacyConfigService;

    PrivacyConfigService::validate_filter_pattern(&pattern)
}

// ============================================================================
// Skill System Commands (Story 7.5)
// ============================================================================

/// Initialize the skill registry
#[tauri::command]
async fn init_skill_registry(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let mut app_state = state.lock().await;
    let registry = Arc::new(SkillRegistry::new());
    let executor = Arc::new(SkillExecutor::new(Arc::clone(&registry), None));
    app_state.skill_registry = Some(registry);
    app_state.skill_executor = Some(executor);
    Ok(())
}

/// List all available skills
#[tauri::command]
async fn list_available_skills(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;
    let registry = app_state.skill_registry.as_ref()
        .ok_or("Skill registry not initialized")?;

    let skills = registry.list_all().await;
    serde_json::to_string(&skills).map_err(|e| e.to_string())
}

/// Get skill info by ID
#[tauri::command]
async fn get_skill_info(
    skill_id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;
    let registry = app_state.skill_registry.as_ref()
        .ok_or("Skill registry not initialized")?;

    let skill = registry.get(&skill_id).await
        .ok_or_else(|| format!("Skill not found: {}", skill_id))?;

    serde_json::to_string(skill.metadata()).map_err(|e| e.to_string())
}

/// Skill execution request from frontend
#[derive(Debug, Deserialize)]
struct SkillExecutionRequest {
    skill_id: String,
    agent_id: String,
    session_id: Option<String>,
    user_input: String,
    config: HashMap<String, Value>,
}

/// Execute a skill
#[tauri::command]
async fn execute_skill(
    request: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let req: SkillExecutionRequest = serde_json::from_str(&request)
        .map_err(|e| format!("Invalid request: {}", e))?;

    let app_state = state.lock().await;
    let registry = app_state.skill_registry.as_ref()
        .ok_or("Skill registry not initialized")?;

    // Create executor
    let executor = SkillExecutor::new(Arc::clone(registry), None);

    // Build context
    let context = SkillContext::new(&req.agent_id, &req.user_input)
        .with_session_opt(req.session_id.as_deref())
        .with_config_map(req.config.into_iter()
            .map(|(k, v)| (k, v))
            .collect());

    // Execute (need to release lock before executing)
    drop(app_state);

    let result = executor.execute(&req.skill_id, context).await
        .map_err(|e| e.to_string())?;

    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Validate skill configuration
#[tauri::command]
async fn validate_skill_config(
    skill_id: String,
    config: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let app_state = state.lock().await;
    let registry = app_state.skill_registry.as_ref()
        .ok_or("Skill registry not initialized")?;

    let skill = registry.get(&skill_id).await
        .ok_or_else(|| format!("Skill not found: {}", skill_id))?;

    let config: HashMap<String, Value> = serde_json::from_str(&config)
        .map_err(|e| format!("Invalid config JSON: {}", e))?;

    skill.validate(&config).map_err(|e| e.to_string())
}

/// Register a custom skill from OpenClaw format
#[tauri::command]
async fn register_custom_skill(
    skill_yaml: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    use omninova_core::skills::OpenClawSkillAdapter;

    let app_state = state.lock().await;
    let registry = app_state.skill_registry.as_ref()
        .ok_or("Skill registry not initialized")?;

    let skill = OpenClawSkillAdapter::from_yaml(&skill_yaml)
        .map_err(|e| format!("Failed to parse skill: {}", e))?;

    let metadata = skill.metadata().clone();
    registry.register(Arc::new(skill)).await
        .map_err(|e| format!("Failed to register skill: {}", e))?;

    serde_json::to_string(&metadata).map_err(|e| e.to_string())
}

/// List skills by tag
#[tauri::command]
async fn list_skills_by_tag(
    tag: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;
    let registry = app_state.skill_registry.as_ref()
        .ok_or("Skill registry not initialized")?;

    let skills = registry.list_by_tag(&tag).await;
    serde_json::to_string(&skills).map_err(|e| e.to_string())
}

/// Get all available skill tags
#[tauri::command]
async fn list_skill_tags(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;
    let registry = app_state.skill_registry.as_ref()
        .ok_or("Skill registry not initialized")?;

    let tags = registry.list_tags().await;
    serde_json::to_string(&tags).map_err(|e| e.to_string())
}

// ============================================================================
// Agent Skill Configuration Commands (Story 7.6)
// ============================================================================

/// Agent skill configuration from frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentSkillConfigRequest {
    agent_id: String,
    enabled_skills: Vec<String>,
    skill_configs: HashMap<String, Value>,
}

/// Get skill configuration for an agent
#[tauri::command]
async fn get_agent_skill_config(
    agent_id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    // For now, return default empty config
    // TODO: Implement persistent storage for agent skill configs
    let config = AgentSkillConfigRequest {
        agent_id,
        enabled_skills: Vec::new(),
        skill_configs: HashMap::new(),
    };
    serde_json::to_string(&config).map_err(|e| e.to_string())
}

/// Update skill configuration for an agent
#[tauri::command]
async fn update_agent_skill_config(
    config: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let _config: AgentSkillConfigRequest = serde_json::from_str(&config)
        .map_err(|e| format!("Invalid config JSON: {}", e))?;

    // TODO: Implement persistent storage for agent skill configs
    // For now, this is a no-op that just validates the input

    Ok(())
}

/// Toggle a skill for an agent
#[tauri::command]
async fn toggle_agent_skill(
    agent_id: String,
    skill_id: String,
    enabled: bool,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    // For now, return updated config
    // TODO: Implement persistent storage for agent skill configs
    let config = AgentSkillConfigRequest {
        agent_id,
        enabled_skills: if enabled { vec![skill_id] } else { Vec::new() },
        skill_configs: HashMap::new(),
    };
    serde_json::to_string(&config).map_err(|e| e.to_string())
}

/// Get execution logs for skills
#[tauri::command]
async fn get_skill_execution_logs(
    limit: Option<usize>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    // Use the skill executor if available
    if let Some(ref executor) = app_state.skill_executor {
        let logs = executor.get_execution_logs(limit.unwrap_or(50)).await;
        return serde_json::to_string(&logs).map_err(|e| e.to_string());
    }

    // Return empty array if executor not initialized
    serde_json::to_string(&Vec::<omninova_core::skills::ExecutionLog>::new())
        .map_err(|e| e.to_string())
}

/// Get usage statistics for skills
#[tauri::command]
async fn get_skill_usage_stats(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    // Use the skill executor if available
    if let Some(ref executor) = app_state.skill_executor {
        let logs = executor.get_execution_logs(1000).await;

        // Calculate statistics from logs
        let mut stats: HashMap<String, omninova_core::skills::ExecutionLog> = HashMap::new();

        for log in logs {
            // This is a simplified version - in production we'd want proper aggregation
            stats.insert(log.skill_id.clone(), log);
        }

        return serde_json::to_string(&stats).map_err(|e| e.to_string());
    }

    // Return empty map if executor not initialized
    serde_json::to_string(&HashMap::<String, omninova_core::skills::ExecutionLog>::new())
        .map_err(|e| e.to_string())
}

// ============================================================================
// MBTI Personality Commands
// ============================================================================

/// MBTI type info for frontend display
#[derive(Debug, Clone, Serialize)]
struct MbtiTypeInfo {
    /// Type code (e.g., "INTJ")
    code: String,
    /// Chinese name (e.g., "战略家")
    chinese_name: String,
    /// English name (e.g., "Architect")
    english_name: String,
    /// Personality group
    group: String,
}

/// Get all MBTI types with their basic info
#[tauri::command]
fn get_mbti_types() -> Result<String, String> {
    let types: Vec<MbtiTypeInfo> = MbtiType::all()
        .iter()
        .map(|&mbti| MbtiTypeInfo {
            code: mbti.to_string(),
            chinese_name: mbti.chinese_name().to_string(),
            english_name: mbti.english_name().to_string(),
            group: mbti.group().to_string(),
        })
        .collect();

    serde_json::to_string(&types).map_err(|e| e.to_string())
}

/// Get personality traits for a specific MBTI type
#[tauri::command]
fn get_mbti_traits(mbti_type: String) -> Result<String, String> {
    let mbti: MbtiType = mbti_type
        .parse()
        .map_err(|e: omninova_core::agent::MbtiError| e.to_string())?;

    let traits: PersonalityTraits = mbti.traits();
    serde_json::to_string(&traits).map_err(|e| e.to_string())
}

/// Get personality configuration for a specific MBTI type
#[tauri::command]
fn get_mbti_config(mbti_type: String) -> Result<String, String> {
    let mbti: MbtiType = mbti_type
        .parse()
        .map_err(|e: omninova_core::agent::MbtiError| e.to_string())?;

    let config: PersonalityConfig = mbti.config();
    serde_json::to_string(&config).map_err(|e| e.to_string())
}

// ============================================================================
// Account Commands
// ============================================================================

/// Initialize account store
#[tauri::command]
async fn init_account_store(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let mut app_state = state.lock().await;

    // Check if already initialized
    if app_state.account_store.is_some() {
        return Ok(());
    }

    // Ensure database pool is initialized
    let pool = if let Some(pool) = &app_state.db_pool {
        pool.clone()
    } else {
        let db_path = omninova_core::db::pool::default_db_path()
            .map_err(|e| format!("Failed to get database path: {e}"))?;

        let pool = create_pool(&db_path, DbPoolConfig::default())
            .map_err(|e| format!("Failed to create database pool: {e}"))?;

        // Run migrations
        let conn = pool.get()
            .map_err(|e| format!("Failed to get database connection: {e}"))?;

        let runner = create_builtin_runner();
        runner.run(&conn)
            .map_err(|e| format!("Failed to run migrations: {e}"))?;

        app_state.db_pool = Some(pool.clone());
        pool
    };

    // Create account store
    let store = AccountStore::new(pool);

    app_state.account_store = Some(store);
    Ok(())
}

/// Get the current account info (without sensitive data)
#[tauri::command]
async fn get_account(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Option<AccountInfo>, String> {
    let app_state = state.lock().await;

    let store = app_state.account_store.as_ref()
        .ok_or_else(|| "Account store not initialized. Call init_account_store first.".to_string())?;

    store.get_info().map_err(|e| e.to_string())
}

/// Create a new account (single user mode, only one account allowed)
#[tauri::command]
async fn create_account(
    username: String,
    password: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let app_state = state.lock().await;

    let store = app_state.account_store.as_ref()
        .ok_or_else(|| "Account store not initialized. Call init_account_store first.".to_string())?;

    let new_account = NewAccount { username, password };
    store.create(&new_account).map_err(|e| e.to_string())?;

    Ok(())
}

/// Verify account password
#[tauri::command]
async fn verify_password(
    password: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<bool, String> {
    let app_state = state.lock().await;

    let store = app_state.account_store.as_ref()
        .ok_or_else(|| "Account store not initialized. Call init_account_store first.".to_string())?;

    store.verify_password(&password).map_err(|e| e.to_string())
}

/// Check if an account exists
#[tauri::command]
async fn has_account(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<bool, String> {
    let app_state = state.lock().await;

    let store = app_state.account_store.as_ref()
        .ok_or_else(|| "Account store not initialized. Call init_account_store first.".to_string())?;

    store.exists().map_err(|e| e.to_string())
}

/// Update account password
#[tauri::command]
async fn update_password(
    current_password: String,
    new_password: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let app_state = state.lock().await;

    let store = app_state.account_store.as_ref()
        .ok_or_else(|| "Account store not initialized. Call init_account_store first.".to_string())?;

    store.update_password(&current_password, &new_password)
        .map_err(|e| e.to_string())
}

/// Get whether password is required on startup
#[tauri::command]
async fn get_require_password_on_startup(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<bool, String> {
    let app_state = state.lock().await;

    let store = app_state.account_store.as_ref()
        .ok_or_else(|| "Account store not initialized. Call init_account_store first.".to_string())?;

    let account = store.get().map_err(|e| e.to_string())?;

    match account {
        Some(acc) => Ok(acc.require_password_on_startup),
        None => Err("Account not found".to_string()),
    }
}

/// Set whether to require password on startup
#[tauri::command]
async fn set_require_password_on_startup(
    require: bool,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let app_state = state.lock().await;

    let store = app_state.account_store.as_ref()
        .ok_or_else(|| "Account store not initialized. Call init_account_store first.".to_string())?;

    store.set_require_password_on_startup(require).map_err(|e| e.to_string())
}

/// Update account settings
#[tauri::command]
async fn update_account(
    updates_json: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<AccountInfo, String> {
    let app_state = state.lock().await;

    let store = app_state.account_store.as_ref()
        .ok_or_else(|| "Account store not initialized. Call init_account_store first.".to_string())?;

    let updates: AccountUpdate = serde_json::from_str(&updates_json)
        .map_err(|e| format!("Invalid update JSON: {e}"))?;

    let updated = store.update(&updates).map_err(|e| e.to_string())?;

    Ok(updated.into())
}

/// Delete the account
#[tauri::command]
async fn delete_account(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let app_state = state.lock().await;

    let store = app_state.account_store.as_ref()
        .ok_or_else(|| "Account store not initialized. Call init_account_store first.".to_string())?;

    store.delete().map_err(|e| e.to_string())
}

// ============================================================================
// Provider Commands
// ============================================================================

/// Initialize the provider store
#[tauri::command]
async fn init_provider_store(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let mut app_state = state.lock().await;

    // Use existing db_pool or create new one
    let pool = match &app_state.db_pool {
        Some(pool) => pool.clone(),
        None => return Err("Database not initialized. Call init_database first.".to_string()),
    };

    let store = ProviderStore::new(pool);

    app_state.provider_store = Some(store);
    Ok(())
}

/// Get all provider configurations
#[tauri::command]
async fn get_provider_configs(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let store = app_state.provider_store.as_ref()
        .ok_or_else(|| "Provider store not initialized. Call init_provider_store first.".to_string())?;

    let configs = store.find_all().map_err(|e| e.to_string())?;
    serde_json::to_string(&configs).map_err(|e| e.to_string())
}

/// Get a provider configuration by ID
#[tauri::command]
async fn get_provider_config_by_id(
    id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let store = app_state.provider_store.as_ref()
        .ok_or_else(|| "Provider store not initialized. Call init_provider_store first.".to_string())?;

    match store.find_by_id(&id).map_err(|e| e.to_string())? {
        Some(config) => serde_json::to_string(&config).map_err(|e| e.to_string()),
        None => Err(format!("Provider config not found: {}", id)),
    }
}

/// Create a new provider configuration
#[tauri::command]
async fn create_provider_config(
    config_json: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let mut app_state = state.lock().await;

    let store = app_state.provider_store.as_ref()
        .ok_or_else(|| "Provider store not initialized. Call init_provider_store first.".to_string())?;

    // Parse the incoming config which may contain apiKey
    let incoming: serde_json::Value = serde_json::from_str(&config_json)
        .map_err(|e| format!("Invalid provider config JSON: {e}"))?;

    let name = incoming["name"].as_str()
        .ok_or_else(|| "Provider name is required".to_string())?
        .to_string();

    let provider_type_str = incoming["providerType"].as_str()
        .or_else(|| incoming["provider_type"].as_str())
        .ok_or_else(|| "Provider type is required".to_string())?;

    // Build the NewProviderConfig
    let mut new_config = NewProviderConfig {
        name: name.clone(),
        provider_type: provider_type_str.parse()
            .map_err(|e: String| format!("Invalid provider type: {e}"))?,
        api_key_ref: None,
        base_url: incoming.get("baseUrl")
            .or_else(|| incoming.get("base_url"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        default_model: incoming.get("defaultModel")
            .or_else(|| incoming.get("default_model"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        settings: incoming.get("settings")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        is_default: incoming.get("isDefault")
            .or_else(|| incoming.get("is_default"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    };

    // If API key is provided, save it to keyring
    let api_key = incoming.get("apiKey")
        .or_else(|| incoming.get("api_key"))
        .and_then(|v| v.as_str());

    if let Some(api_key) = api_key {
        if !api_key.is_empty() {
            let keyring = app_state.keyring_service.as_ref()
                .ok_or_else(|| "Keyring service not initialized. Call init_keyring_service first.".to_string())?;

            let reference = keyring.save_provider_key(&name, api_key).await
                .map_err(|e| format!("Failed to save API key: {e}"))?;

            new_config.api_key_ref = Some(reference.to_url());
        }
    }

    let created = store.create(&new_config).map_err(|e| e.to_string())?;
    serde_json::to_string(&created).map_err(|e| e.to_string())
}

/// Update a provider configuration
#[tauri::command]
async fn update_provider_config(
    id: String,
    updates_json: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let mut app_state = state.lock().await;

    let store = app_state.provider_store.as_ref()
        .ok_or_else(|| "Provider store not initialized. Call init_provider_store first.".to_string())?;

    // Parse the incoming updates which may contain apiKey
    let incoming: serde_json::Value = serde_json::from_str(&updates_json)
        .map_err(|e| format!("Invalid provider config update JSON: {e}"))?;

    // Get existing provider to find the name for keyring operations
    let existing = store.find_by_id(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Provider with id {} not found", id))?;

    // Build the ProviderConfigUpdate
    let mut updates = ProviderConfigUpdate {
        name: incoming.get("name")
            .or_else(|| incoming.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        api_key_ref: None,
        base_url: incoming.get("baseUrl")
            .or_else(|| incoming.get("base_url"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        default_model: incoming.get("defaultModel")
            .or_else(|| incoming.get("default_model"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        settings: incoming.get("settings")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        is_default: incoming.get("isDefault")
            .or_else(|| incoming.get("is_default"))
            .and_then(|v| v.as_bool()),
    };

    // If API key is provided, save it to keyring
    let api_key = incoming.get("apiKey")
        .or_else(|| incoming.get("api_key"))
        .and_then(|v| v.as_str());

    if let Some(api_key) = api_key {
        if !api_key.is_empty() {
            let keyring = app_state.keyring_service.as_ref()
                .ok_or_else(|| "Keyring service not initialized. Call init_keyring_service first.".to_string())?;

            // Use the new name if provided, otherwise use existing name
            let provider_name = updates.name.as_ref().unwrap_or(&existing.name);

            let reference = keyring.save_provider_key(provider_name, api_key).await
                .map_err(|e| format!("Failed to save API key: {e}"))?;

            updates.api_key_ref = Some(reference.to_url());
        }
    }

    let updated = store.update(&id, &updates).map_err(|e| e.to_string())?;
    serde_json::to_string(&updated).map_err(|e| e.to_string())
}

/// Delete a provider configuration
#[tauri::command]
async fn delete_provider_config(
    id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let mut app_state = state.lock().await;

    let store = app_state.provider_store.as_ref()
        .ok_or_else(|| "Provider store not initialized. Call init_provider_store first.".to_string())?;

    // Get the provider to find the name for keyring deletion
    let provider = store.find_by_id(&id)
        .map_err(|e| e.to_string())?;

    // Delete from store first
    store.delete(&id).map_err(|e| e.to_string())?;

    // If provider existed and had a keyring reference, delete the API key
    if let Some(provider) = provider {
        if let Some(ref api_key_ref) = provider.api_key_ref {
            if api_key_ref.starts_with("keyring://") {
                if let Some(ref keyring) = app_state.keyring_service {
                    let provider_name = &api_key_ref[10..];
                    // Ignore errors when deleting from keyring - best effort
                    let _ = keyring.delete_provider_key(provider_name).await;
                }
            }
        }
    }

    Ok(())
}

/// Set a provider as default
#[tauri::command]
async fn set_default_provider_config(
    id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let app_state = state.lock().await;

    let store = app_state.provider_store.as_ref()
        .ok_or_else(|| "Provider store not initialized. Call init_provider_store first.".to_string())?;

    store.set_default(&id).map_err(|e| e.to_string())
}

/// Test provider connection
#[tauri::command]
async fn test_provider_connection(
    config_json: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    // Parse the config to get provider type
    let config: omninova_core::providers::ProviderConfig = serde_json::from_str(&config_json)
        .map_err(|e| format!("Invalid provider config JSON: {e}"))?;

    // Create a provider using the registry
    let provider_type = config.provider_type.clone();
    let registry = omninova_core::providers::registry::global_registry();

    // Try to get API key from keyring first, then fall back to environment
    let api_key = {
        let app_state = state.lock().await;

        // Try keyring service first if api_key_ref is set
        if let Some(ref api_key_ref) = config.api_key_ref {
            if let Some(ref keyring) = app_state.keyring_service {
                // Extract provider name from the reference URL
                // Format: keyring://<provider_name>
                if api_key_ref.starts_with("keyring://") {
                    let provider_name = &api_key_ref[10..];
                    match keyring.get_provider_key(provider_name).await {
                        Ok(key) => Some(key),
                        Err(_) => {
                            // Fall back to environment variable
                            std::env::var(config.provider_type.api_key_env_var()).ok()
                        }
                    }
                } else {
                    std::env::var(config.provider_type.api_key_env_var()).ok()
                }
            } else {
                std::env::var(config.provider_type.api_key_env_var()).ok()
            }
        } else {
            // No api_key_ref, try environment variable
            std::env::var(config.provider_type.api_key_env_var()).ok()
        }
    };

    let provider = registry.create_provider(
        &provider_type,
        config.base_url.as_deref(),
        api_key.as_deref(),
        config.default_model.as_deref(),
        0.7,
    );

    // Perform health check
    let is_healthy = provider.health_check().await;

    let result = serde_json::json!({
        "provider_type": config.provider_type.to_string(),
        "name": config.name,
        "model": config.default_model,
        "healthy": is_healthy,
    });

    serde_json::to_string(&result).map_err(|e| e.to_string())
}

// ============================================================================
// Agent Provider Assignment Commands (Story 3.7)
// ============================================================================

/// Agent provider validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentProviderValidation {
    /// Whether the provider is valid for agent assignment
    is_valid: bool,
    /// Error messages if validation fails
    errors: Vec<String>,
    /// Warning messages (non-blocking issues)
    warnings: Vec<String>,
    /// Suggested alternative provider IDs
    suggestions: Vec<String>,
}

/// Set the default provider for an agent
///
/// Updates the agent's default_provider_id field.
/// Validates that the provider exists before setting.
#[tauri::command]
async fn set_agent_default_provider(
    agent_uuid: String,
    provider_id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    // Get stores
    let agent_store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;
    let provider_store = app_state.provider_store.as_ref()
        .ok_or_else(|| "Provider store not initialized. Call init_provider_store first.".to_string())?;

    // Validate provider exists
    let provider = provider_store.find_by_id(&provider_id)
        .map_err(|e| format!("Failed to find provider: {e}"))?
        .ok_or_else(|| format!("未找到提供商配置: {}", provider_id))?;

    // Update agent with new default provider
    let updates = omninova_core::agent::AgentUpdate {
        default_provider_id: Some(provider_id),
        ..Default::default()
    };

    let updated_agent = agent_store.update(&agent_uuid, &updates)
        .map_err(|e| format!("更新代理默认提供商失败: {e}"))?;

    // Return updated agent as JSON
    serde_json::to_string(&updated_agent).map_err(|e| e.to_string())
}

/// Get the provider configuration for an agent
///
/// Returns the agent's default provider if set, otherwise returns the global default provider.
/// Returns null if no provider is configured.
#[tauri::command]
async fn get_agent_provider(
    agent_uuid: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Option<String>, String> {
    let app_state = state.lock().await;

    // Get stores
    let agent_store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;
    let provider_store = app_state.provider_store.as_ref()
        .ok_or_else(|| "Provider store not initialized. Call init_provider_store first.".to_string())?;

    // Find the agent
    let agent = agent_store.find_by_uuid(&agent_uuid)
        .map_err(|e| format!("查找代理失败: {e}"))?
        .ok_or_else(|| format!("未找到代理: {}", agent_uuid))?;

    // Get the agent's default provider or fall back to global default
    let provider = if let Some(ref provider_id) = agent.default_provider_id {
        // Agent has a specific default provider
        provider_store.find_by_id(provider_id)
            .map_err(|e| format!("查找提供商失败: {e}"))?
    } else {
        // Fall back to global default provider
        provider_store.find_default()
            .map_err(|e| format!("查找默认提供商失败: {e}"))?
    };

    // Return provider as JSON string or null
    match provider {
        Some(p) => Ok(Some(serde_json::to_string(&p).map_err(|e| e.to_string())?)),
        None => Ok(None),
    }
}

/// Validate a provider for agent assignment
///
/// Checks if a provider is suitable for use with an agent:
/// - Provider exists
/// - Provider has API key configured (if required)
/// - Provider is not deleted
///
/// Returns validation result with suggestions for alternatives.
#[tauri::command]
async fn validate_provider_for_agent(
    provider_id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    // Get stores
    let provider_store = app_state.provider_store.as_ref()
        .ok_or_else(|| "Provider store not initialized. Call init_provider_store first.".to_string())?;
    let keyring_service = app_state.keyring_service.as_ref();

    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut suggestions = Vec::new();

    // Check if provider exists
    let provider = match provider_store.find_by_id(&provider_id)
        .map_err(|e| format!("查找提供商失败: {e}"))?
    {
        Some(p) => p,
        None => {
            errors.push(format!("未找到提供商配置: {}", provider_id));

            // Get all available providers as suggestions
            let all_providers = provider_store.find_all()
                .map_err(|e| format!("获取提供商列表失败: {e}"))?;

            for p in all_providers.iter().take(3) {
                suggestions.push(p.id.clone());
            }

            let result = AgentProviderValidation {
                is_valid: false,
                errors,
                warnings,
                suggestions,
            };

            return serde_json::to_string(&result).map_err(|e| e.to_string());
        }
    };

    // Check if API key is configured (for providers that require it)
    let provider_type = provider.provider_type;
    let requires_api_key = !matches!(
        provider_type,
        omninova_core::providers::ProviderType::Ollama
            | omninova_core::providers::ProviderType::LlamaCpp
            | omninova_core::providers::ProviderType::Vllm
            | omninova_core::providers::ProviderType::Sglang
            | omninova_core::providers::ProviderType::LmStudio
            | omninova_core::providers::ProviderType::Mock
    );

    if requires_api_key {
        // Check if API key reference is set
        let has_key_ref = provider.api_key_ref.is_some();

        // Check if key exists in keyring
        let key_exists = if let (Some(ref api_key_ref), Some(keyring)) = (&provider.api_key_ref, keyring_service) {
            if api_key_ref.starts_with("keyring://") {
                let provider_name = &api_key_ref[10..];
                keyring.provider_key_exists(provider_name).await.unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };

        // Check environment variable as fallback
        let env_var = provider_type.api_key_env_var();
        let has_env_key = std::env::var(&env_var).is_ok();

        if !has_key_ref && !has_env_key {
            errors.push("提供商缺少 API 密钥配置".to_string());
        } else if has_key_ref && !key_exists && !has_env_key {
            warnings.push("API 密钥引用已设置但密钥可能不存在".to_string());
        }
    }

    // Get alternative providers for suggestions if there are errors
    if !errors.is_empty() {
        let all_providers = provider_store.find_all()
            .map_err(|e| format!("获取提供商列表失败: {e}"))?;

        for p in all_providers.iter()
            .filter(|p| p.id != provider_id)
            .take(3)
        {
            suggestions.push(p.id.clone());
        }
    }

    let result = AgentProviderValidation {
        is_valid: errors.is_empty(),
        errors,
        warnings,
        suggestions,
    };

    serde_json::to_string(&result).map_err(|e| e.to_string())
}

// ============================================================================
// Backup Commands
// ============================================================================

/// Import options for JSON serialization from frontend
#[derive(Debug, Clone, Deserialize)]
struct ImportOptionsJson {
    /// Import mode: "overwrite" or "merge"
    mode: String,
    /// Whether to import agents
    include_agents: bool,
    /// Whether to import providers
    include_providers: bool,
    /// Whether to import channels
    include_channels: bool,
    /// Whether to import skills
    include_skills: bool,
    /// Whether to import account settings
    include_account: bool,
}

impl From<ImportOptionsJson> for ImportOptions {
    fn from(json: ImportOptionsJson) -> Self {
        let mode = match json.mode.as_str() {
            "overwrite" => ImportMode::Overwrite,
            "merge" => ImportMode::Merge,
            _ => ImportMode::Merge, // Default to merge
        };
        ImportOptions {
            mode,
            include_agents: json.include_agents,
            include_providers: json.include_providers,
            include_channels: json.include_channels,
            include_skills: json.include_skills,
            include_account: json.include_account,
        }
    }
}

/// Export configuration backup
///
/// Returns a JSON or YAML string containing all configuration data:
/// - Agent configurations
/// - Provider configurations
/// - Channel configurations
/// - Skill configurations
/// - Account settings (without password)
#[tauri::command]
async fn export_config_backup(
    format: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    // Get stores
    let agent_store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized".to_string())?;
    let account_store = app_state.account_store.as_ref()
        .ok_or_else(|| "Account store not initialized".to_string())?;

    // Get current config
    let config = app_state.runtime.get_config().await;

    // Create backup service
    let service = BackupService::new(agent_store.clone(), account_store.clone());

    // Determine format
    let backup_format = match format.to_lowercase().as_str() {
        "yaml" | "yml" => BackupFormat::Yaml,
        _ => BackupFormat::Json,
    };

    // Export backup
    service.export_backup_to_string(&config, backup_format)
        .map_err(|e| e.to_string())
}

/// Validate a backup file
///
/// Parses and validates the backup file content.
/// Returns backup metadata if valid, or an error if invalid.
#[tauri::command]
async fn validate_backup_file(
    content: String,
) -> Result<BackupMeta, String> {
    // Parse backup data
    let backup = deserialize_backup(&content)
        .map_err(|e| e.to_string())?;

    // Validate backup
    validate_backup(&backup)
        .map_err(|e| e.to_string())?;

    Ok(backup.meta)
}

/// Import configuration backup
///
/// Imports backup data with the specified options.
/// Returns the number of items imported.
#[tauri::command]
async fn import_config_backup(
    content: String,
    options_json: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    // Get stores
    let agent_store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized".to_string())?;
    let account_store = app_state.account_store.as_ref()
        .ok_or_else(|| "Account store not initialized".to_string())?;

    // Parse backup data
    let backup = deserialize_backup(&content)
        .map_err(|e| e.to_string())?;

    // Validate backup
    validate_backup(&backup)
        .map_err(|e| e.to_string())?;

    // Parse import options
    let options: ImportOptionsJson = serde_json::from_str(&options_json)
        .map_err(|e| format!("Invalid options JSON: {e}"))?;

    // Create backup service
    let service = BackupService::new(agent_store.clone(), account_store.clone());

    // Import backup
    let result = service.import_backup(&backup, &options.into())
        .map_err(|e| e.to_string())?;

    // Return result as JSON
    serde_json::to_string(&serde_json::json!({
        "agents_imported": result.agents_imported,
        "account_imported": result.account_imported,
    })).map_err(|e| e.to_string())
}

// ============================================
// Privacy & Security Commands
// ============================================

/// Gets the current privacy settings.
/// Returns default settings if no settings file exists.
#[tauri::command]
async fn get_privacy_settings() -> Result<String, String> {
    // Try to load existing settings from config directory
    let config_dir = omninova_core::privacy::get_config_directory()
        .map_err(|e| e.to_string())?;

    let settings_path = config_dir.join("privacy_settings.json");

    if settings_path.exists() {
        let content = tokio::fs::read_to_string(&settings_path)
            .await
            .map_err(|e| format!("Failed to read privacy settings: {e}"))?;
        let settings: PrivacySettings = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse privacy settings: {e}"))?;
        serde_json::to_string(&settings).map_err(|e| e.to_string())
    } else {
        // Return default settings
        let settings = PrivacySettings::default();
        serde_json::to_string(&settings).map_err(|e| e.to_string())
    }
}

/// Updates the privacy settings.
/// Settings are persisted to a JSON file in the config directory.
#[tauri::command]
async fn update_privacy_settings(
    settings_json: String,
) -> Result<(), String> {
    let mut settings: PrivacySettings = serde_json::from_str(&settings_json)
        .map_err(|e| format!("Invalid privacy settings JSON: {e}"))?;

    // Update timestamp
    settings.touch();

    // Get config directory
    let config_dir = omninova_core::privacy::get_config_directory()
        .map_err(|e| e.to_string())?;

    // Ensure directory exists
    tokio::fs::create_dir_all(&config_dir)
        .await
        .map_err(|e| format!("Failed to create config directory: {e}"))?;

    // Write settings to file
    let settings_path = config_dir.join("privacy_settings.json");
    let content = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {e}"))?;

    tokio::fs::write(&settings_path, content)
        .await
        .map_err(|e| format!("Failed to write privacy settings: {e}"))?;

    Ok(())
}

/// Gets data storage information including sizes of database, config, logs, and cache.
#[tauri::command]
async fn get_data_storage_info() -> Result<String, String> {
    let storage_info = StorageInfo::calculate()
        .await
        .map_err(|e| format!("Failed to calculate storage info: {e}"))?;

    serde_json::to_string(&storage_info).map_err(|e| e.to_string())
}

/// Clears conversation history based on the provided options.
/// Returns statistics about what was deleted.
#[tauri::command]
async fn clear_conversation_history(
    options_json: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let options: ClearOptions = serde_json::from_str(&options_json)
        .map_err(|e| format!("Invalid clear options JSON: {e}"))?;

    // Clone the pool so we don't hold the lock during the operation
    let db_pool = {
        let app_state = state.lock().await;
        app_state.db_pool.clone()
            .ok_or_else(|| "Database not initialized".to_string())?
    };

    // Execute clear operation (synchronous rusqlite operation)
    let result = clear_conversation_history_inner(&db_pool, &options)
        .map_err(|e| e.to_string())?;

    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Inner function to clear conversation history from database.
fn clear_conversation_history_inner(
    db_pool: &DbPool,
    options: &ClearOptions,
) -> Result<ClearResult, anyhow::Error> {
    let conn = db_pool.get()
        .map_err(|e| anyhow::anyhow!("Failed to get database connection: {}", e))?;

    let mut result = ClearResult::empty();

    match options.scope {
        omninova_core::privacy::ClearScope::All => {
            // Delete all messages first (due to foreign key constraints)
            let messages_deleted = conn.execute("DELETE FROM messages", [])?;
            // Delete all conversations
            let sessions_deleted = conn.execute("DELETE FROM conversations", [])?;

            result.messages_deleted = messages_deleted as u64;
            result.sessions_deleted = sessions_deleted as u64;
        }
        omninova_core::privacy::ClearScope::SpecificAgents => {
            // For MVP, we'll implement agent-specific clearing
            // This requires agent_ids to be provided
            if let Some(agent_ids) = &options.agent_ids {
                for agent_id in agent_ids {
                    // Delete messages for conversations belonging to this agent
                    let messages_deleted = conn.execute(
                        "DELETE FROM messages WHERE conversation_id IN \
                         (SELECT id FROM conversations WHERE agent_id = ?1)",
                        [agent_id],
                    )?;

                    // Delete conversations for this agent
                    let sessions_deleted = conn.execute(
                        "DELETE FROM conversations WHERE agent_id = ?1",
                        [agent_id],
                    )?;

                    result.messages_deleted += messages_deleted as u64;
                    result.sessions_deleted += sessions_deleted as u64;
                }
            }
        }
        omninova_core::privacy::ClearScope::DateRange => {
            if let Some(date_range) = &options.date_range {
                // Convert timestamps to datetime strings
                let start_dt = chrono::DateTime::from_timestamp(date_range.start, 0)
                    .unwrap_or_else(|| chrono::Utc::now())
                    .to_rfc3339();
                let end_dt = chrono::DateTime::from_timestamp(date_range.end, 0)
                    .unwrap_or_else(|| chrono::Utc::now())
                    .to_rfc3339();

                // Delete messages within date range
                let messages_deleted = conn.execute(
                    "DELETE FROM messages WHERE created_at >= ?1 AND created_at <= ?2",
                    rusqlite::params![&start_dt, &end_dt],
                )?;

                // Delete orphan conversations (no messages left)
                let sessions_deleted = conn.execute(
                    "DELETE FROM conversations WHERE id NOT IN \
                     (SELECT DISTINCT conversation_id FROM messages WHERE conversation_id IS NOT NULL)",
                    [],
                )?;

                result.messages_deleted = messages_deleted as u64;
                result.sessions_deleted = sessions_deleted as u64;
            }
        }
    }

    // Calculate approximate space freed (rough estimate based on average message size)
    // In production, you might want to track actual sizes
    result.space_freed = result.messages_deleted * 500; // ~500 bytes per message estimate

    // Run VACUUM to reclaim space
    conn.execute("VACUUM", [])?;

    Ok(result)
}

/// Toggles encryption on or off.
/// When enabling, generates a new encryption key and stores it in the OS keychain.
/// When disabling, removes the encryption key from the keychain.
#[tauri::command]
async fn toggle_encryption(enabled: bool) -> Result<(), String> {
    let key_manager = EncryptionKeyManager::new();

    if enabled {
        // Generate a random password to protect the encryption key
        // This password is used to encrypt the master key, but since we don't
        // want the user to manage it, we generate a random one.
        let password = uuid::Uuid::new_v4().to_string();

        // Generate and store encryption key
        key_manager.enable_encryption(&password)
            .await
            .map_err(|e| format!("Failed to enable encryption: {e}"))?;
    } else {
        // Disable encryption (clear the key)
        key_manager.disable_encryption()
            .await
            .map_err(|e| format!("Failed to disable encryption: {e}"))?;
    }

    Ok(())
}

/// Checks if encryption is available on this system.
/// Returns true if the OS keychain is accessible.
#[tauri::command]
async fn is_encryption_available() -> Result<bool, String> {
    // EncryptionKeyManager::new() always succeeds
    // The actual keychain accessibility is tested when enabling encryption
    // For now, we return true to indicate encryption is available
    Ok(true)
}

// ============================================================================
// Keyring Commands (API Key Management)
// ============================================================================

/// Initialize the keyring service
#[tauri::command]
async fn init_keyring_service(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let mut app_state = state.lock().await;

    let service = KeyringService::new();
    let store_type = service.store_type().to_string();

    app_state.keyring_service = Some(Arc::new(service));

    Ok(store_type)
}

/// Save an API key for a provider
#[tauri::command]
async fn save_api_key(
    provider: String,
    api_key: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let service = app_state.keyring_service.as_ref()
        .ok_or_else(|| "Keyring service not initialized. Call init_keyring_service first.".to_string())?;

    let reference = service.save_provider_key(&provider, &api_key).await
        .map_err(|e| e.to_string())?;

    Ok(reference.to_url())
}

/// Get an API key for a provider
#[tauri::command]
async fn get_api_key(
    provider: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let service = app_state.keyring_service.as_ref()
        .ok_or_else(|| "Keyring service not initialized. Call init_keyring_service first.".to_string())?;

    service.get_provider_key(&provider).await
        .map_err(|e| e.to_string())
}

/// Delete an API key for a provider
#[tauri::command]
async fn delete_api_key(
    provider: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let app_state = state.lock().await;

    let service = app_state.keyring_service.as_ref()
        .ok_or_else(|| "Keyring service not initialized. Call init_keyring_service first.".to_string())?;

    service.delete_provider_key(&provider).await
        .map_err(|e| e.to_string())
}

/// Check if an API key exists for a provider
#[tauri::command]
async fn api_key_exists(
    provider: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<bool, String> {
    let app_state = state.lock().await;

    let service = app_state.keyring_service.as_ref()
        .ok_or_else(|| "Keyring service not initialized. Call init_keyring_service first.".to_string())?;

    service.provider_key_exists(&provider).await
        .map_err(|e| e.to_string())
}

/// Get the type of keyring storage being used
#[tauri::command]
async fn get_keyring_store_type(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let service = app_state.keyring_service.as_ref()
        .ok_or_else(|| "Keyring service not initialized. Call init_keyring_service first.".to_string())?;

    Ok(service.store_type().to_string())
}

// ============================================================================
// API Key Management Commands (Story 8.3)
// ============================================================================

/// Initialize the API key store
#[tauri::command]
async fn init_api_key_store(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let mut app_state = state.lock().await;

    // Check if already initialized
    if app_state.api_key_store.is_some() {
        return Ok(());
    }

    // Ensure database pool is initialized
    let pool = if let Some(pool) = &app_state.db_pool {
        pool.clone()
    } else {
        let db_path = omninova_core::db::pool::default_db_path()
            .map_err(|e| format!("Failed to get database path: {e}"))?;

        let pool = create_pool(&db_path, DbPoolConfig::default())
            .map_err(|e| format!("Failed to create database pool: {e}"))?;

        // Run migrations
        let conn = pool.get()
            .map_err(|e| format!("Failed to get database connection: {e}"))?;

        let runner = create_builtin_runner();
        runner.run(&conn)
            .map_err(|e| format!("Failed to run migrations: {e}"))?;

        app_state.db_pool = Some(pool.clone());
        pool
    };

    // Create API key store
    app_state.api_key_store = Some(Arc::new(ApiKeyStore::new(Arc::new(pool))));

    Ok(())
}

/// Create a new API key
#[tauri::command]
async fn create_api_key(
    name: String,
    permissions: Vec<String>,
    expires_in_days: Option<u32>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<ApiKeyCreated, String> {
    let app_state = state.lock().await;

    let store = app_state.api_key_store.as_ref()
        .ok_or_else(|| "API key store not initialized. Call init_api_key_store first.".to_string())?;

    // Parse permissions
    let perms: Vec<ApiKeyPermission> = permissions
        .iter()
        .filter_map(|p| match p.to_lowercase().as_str() {
            "read" => Some(ApiKeyPermission::Read),
            "write" => Some(ApiKeyPermission::Write),
            "admin" => Some(ApiKeyPermission::Admin),
            _ => None,
        })
        .collect();

    if perms.is_empty() {
        return Err("At least one valid permission (read, write, admin) is required".to_string());
    }

    let request = CreateApiKeyRequest {
        name,
        permissions: perms,
        expires_in_days,
    };

    store.create(request).map_err(|e| e.to_string())
}

/// List all API keys
#[tauri::command]
async fn list_api_keys(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<ApiKeyInfo>, String> {
    let app_state = state.lock().await;

    let store = app_state.api_key_store.as_ref()
        .ok_or_else(|| "API key store not initialized. Call init_api_key_store first.".to_string())?;

    store.list_all().map_err(|e| e.to_string())
}

/// Revoke an API key
#[tauri::command]
async fn revoke_api_key(
    id: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<bool, String> {
    let app_state = state.lock().await;

    let store = app_state.api_key_store.as_ref()
        .ok_or_else(|| "API key store not initialized. Call init_api_key_store first.".to_string())?;

    store.revoke(id).map_err(|e| e.to_string())
}

/// Delete a gateway API key permanently
#[tauri::command]
async fn delete_gateway_api_key(
    id: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<bool, String> {
    let app_state = state.lock().await;

    let store = app_state.api_key_store.as_ref()
        .ok_or_else(|| "API key store not initialized. Call init_api_key_store first.".to_string())?;

    store.delete(id).map_err(|e| e.to_string())
}

/// Get gateway API key by ID
#[tauri::command]
async fn get_gateway_api_key(
    id: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Option<ApiKeyInfo>, String> {
    let app_state = state.lock().await;

    let store = app_state.api_key_store.as_ref()
        .ok_or_else(|| "API key store not initialized. Call init_api_key_store first.".to_string())?;

    match store.find_by_id(id).map_err(|e| e.to_string())? {
        Some(key) => Ok(Some(ApiKeyInfo::from(key))),
        None => Ok(None),
    }
}

// ============================================================================
// API Log Management Commands (Story 8.4)
// ============================================================================

/// Initialize the API log store
#[tauri::command]
async fn init_api_log_store(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let mut app_state = state.lock().await;

    // Check if already initialized
    if app_state.api_log_store.is_some() {
        return Ok(());
    }

    // Ensure database pool is initialized
    let pool = if let Some(pool) = &app_state.db_pool {
        pool.clone()
    } else {
        let db_path = omninova_core::db::pool::default_db_path()
            .map_err(|e| format!("Failed to get database path: {e}"))?;

        let pool = create_pool(&db_path, DbPoolConfig::default())
            .map_err(|e| format!("Failed to create database pool: {e}"))?;

        // Run migrations
        let conn = pool.get()
            .map_err(|e| format!("Failed to get database connection: {e}"))?;

        let runner = create_builtin_runner();
        runner.run(&conn)
            .map_err(|e| format!("Failed to run migrations: {e}"))?;

        app_state.db_pool = Some(pool.clone());
        pool
    };

    // Create API log store
    app_state.api_log_store = Some(Arc::new(ApiLogStore::new(Arc::new(pool))));

    Ok(())
}

/// List API request logs with filtering and pagination
#[tauri::command]
async fn list_api_logs(
    filter: RequestLogFilter,
    limit: Option<u64>,
    offset: Option<u64>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<ApiRequestLog>, String> {
    let app_state = state.lock().await;

    let store = app_state.api_log_store.as_ref()
        .ok_or_else(|| "API log store not initialized. Call init_api_log_store first.".to_string())?;

    let limit = limit.unwrap_or(100);
    let offset = offset.unwrap_or(0);

    // Cap limit to prevent excessive queries
    let limit = std::cmp::min(limit, 1000);

    store.query(&filter, limit, offset).map_err(|e| e.to_string())
}

/// Get count of API logs matching filter
#[tauri::command]
async fn count_api_logs(
    filter: RequestLogFilter,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<u64, String> {
    let app_state = state.lock().await;

    let store = app_state.api_log_store.as_ref()
        .ok_or_else(|| "API log store not initialized. Call init_api_log_store first.".to_string())?;

    store.count(&filter).map_err(|e| e.to_string())
}

/// Get API usage statistics for a time range
#[tauri::command]
async fn get_api_usage_stats(
    start_time: i64,
    end_time: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<ApiUsageStats, String> {
    let app_state = state.lock().await;

    let store = app_state.api_log_store.as_ref()
        .ok_or_else(|| "API log store not initialized. Call init_api_log_store first.".to_string())?;

    store.get_stats(start_time, end_time).map_err(|e| e.to_string())
}

/// Get per-endpoint statistics
#[tauri::command]
async fn get_endpoint_stats(
    start_time: i64,
    end_time: i64,
    limit: Option<u64>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<EndpointStats>, String> {
    let app_state = state.lock().await;

    let store = app_state.api_log_store.as_ref()
        .ok_or_else(|| "API log store not initialized. Call init_api_log_store first.".to_string())?;

    let limit = limit.unwrap_or(20);

    store.get_endpoint_stats(start_time, end_time, limit).map_err(|e| e.to_string())
}

/// Get per-API-key statistics
#[tauri::command]
async fn get_api_key_stats(
    start_time: i64,
    end_time: i64,
    limit: Option<u64>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<ApiKeyStats>, String> {
    let app_state = state.lock().await;

    let store = app_state.api_log_store.as_ref()
        .ok_or_else(|| "API log store not initialized. Call init_api_log_store first.".to_string())?;

    let limit = limit.unwrap_or(20);

    store.get_api_key_stats(start_time, end_time, limit).map_err(|e| e.to_string())
}

/// Export API logs to JSON or CSV format
#[tauri::command]
async fn export_api_logs(
    filter: RequestLogFilter,
    format: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let store = app_state.api_log_store.as_ref()
        .ok_or_else(|| "API log store not initialized. Call init_api_log_store first.".to_string())?;

    match format.to_lowercase().as_str() {
        "json" => store.export_json(&filter).map_err(|e| e.to_string()),
        "csv" => store.export_csv(&filter).map_err(|e| e.to_string()),
        _ => Err("Invalid format. Use 'json' or 'csv'.".to_string()),
    }
}

/// Clear API logs before a specific timestamp
#[tauri::command]
async fn clear_api_logs(
    before_timestamp: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<u64, String> {
    let app_state = state.lock().await;

    let store = app_state.api_log_store.as_ref()
        .ok_or_else(|| "API log store not initialized. Call init_api_log_store first.".to_string())?;

    store.clear_before(before_timestamp).map_err(|e| e.to_string())
}

/// Clear all API logs
#[tauri::command]
async fn clear_all_api_logs(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<u64, String> {
    let app_state = state.lock().await;

    let store = app_state.api_log_store.as_ref()
        .ok_or_else(|| "API log store not initialized. Call init_api_log_store first.".to_string())?;

    store.clear_all().map_err(|e| e.to_string())
}

// ============================================================================
// Session Management Commands (Story 4.1)
// ============================================================================

/// Initialize the session store
#[tauri::command]
async fn init_session_store(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let mut app_state = state.lock().await;

    // Check if already initialized
    if app_state.session_store.is_some() && app_state.message_store.is_some() {
        return Ok(());
    }

    // Ensure database pool is initialized
    let pool = if let Some(pool) = &app_state.db_pool {
        pool.clone()
    } else {
        let db_path = omninova_core::db::pool::default_db_path()
            .map_err(|e| format!("Failed to get database path: {e}"))?;

        let pool = create_pool(&db_path, DbPoolConfig::default())
            .map_err(|e| format!("Failed to create database pool: {e}"))?;

        // Run migrations
        let conn = pool.get()
            .map_err(|e| format!("Failed to get database connection: {e}"))?;

        let runner = create_builtin_runner();
        runner.run(&conn)
            .map_err(|e| format!("Failed to run migrations: {e}"))?;

        app_state.db_pool = Some(pool.clone());
        pool
    };

    // Create stores
    app_state.session_store = Some(SessionStore::new(pool.clone()));
    app_state.message_store = Some(MessageStore::new(pool.clone()));
    app_state.episodic_memory_store = Some(Arc::new(EpisodicMemoryStore::new(Arc::new(pool.clone()))));

    // Create unified MemoryManager
    app_state.memory_manager = Some(Arc::new(Mutex::new(MemoryManager::new(
        Arc::new(pool),
        None, // No embedding service by default - L3 disabled until configured
        DEFAULT_EMBEDDING_DIM,
        1, // Default agent ID
    ))));

    Ok(())
}

/// Create a new session
#[tauri::command]
async fn create_session(
    new_session: NewSession,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Session, String> {
    let app_state = state.lock().await;

    let store = app_state.session_store.as_ref()
        .ok_or_else(|| "Session store not initialized. Call init_session_store first.".to_string())?;

    store.create(&new_session).map_err(|e| e.to_string())
}

/// Get a session by ID
#[tauri::command]
async fn get_session(
    id: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Option<Session>, String> {
    let app_state = state.lock().await;

    let store = app_state.session_store.as_ref()
        .ok_or_else(|| "Session store not initialized. Call init_session_store first.".to_string())?;

    store.find_by_id(id).map_err(|e| e.to_string())
}

/// List all sessions for an agent
#[tauri::command]
async fn list_sessions_by_agent(
    agent_id: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<Session>, String> {
    let app_state = state.lock().await;

    let store = app_state.session_store.as_ref()
        .ok_or_else(|| "Session store not initialized. Call init_session_store first.".to_string())?;

    store.find_by_agent(agent_id).map_err(|e| e.to_string())
}

/// Update a session
#[tauri::command]
async fn update_session(
    id: i64,
    update: SessionUpdate,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Session, String> {
    let app_state = state.lock().await;

    let store = app_state.session_store.as_ref()
        .ok_or_else(|| "Session store not initialized. Call init_session_store first.".to_string())?;

    store.update(id, &update).map_err(|e| e.to_string())
}

/// Delete a session
#[tauri::command]
async fn delete_session(
    id: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let app_state = state.lock().await;

    let store = app_state.session_store.as_ref()
        .ok_or_else(|| "Session store not initialized. Call init_session_store first.".to_string())?;

    store.delete(id).map_err(|e| e.to_string())
}

/// Create a new message
#[tauri::command]
async fn create_message(
    new_message: NewMessage,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Message, String> {
    let app_state = state.lock().await;

    let store = app_state.message_store.as_ref()
        .ok_or_else(|| "Message store not initialized. Call init_session_store first.".to_string())?;

    store.create(&new_message).map_err(|e| e.to_string())
}

/// List all messages for a session
#[tauri::command]
async fn list_messages_by_session(
    session_id: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<Message>, String> {
    let app_state = state.lock().await;

    let store = app_state.message_store.as_ref()
        .ok_or_else(|| "Message store not initialized. Call init_session_store first.".to_string())?;

    store.find_by_session(session_id).map_err(|e| e.to_string())
}

// ============================================================================
// Chat Commands (Story 4.2)
// ============================================================================

/// Request to send a message to an agent
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendMessageRequest {
    /// Agent ID (numeric database ID)
    agent_id: i64,
    /// Message content
    message: String,
    /// Optional provider selection
    provider_id: Option<String>,
    /// Optional model override
    model: Option<String>,
    /// Optional ID of a message to quote/reply to
    quote_message_id: Option<i64>,
}

/// Request to send a message to an existing session
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendMessageToSessionRequest {
    /// Session ID
    session_id: i64,
    /// Message content
    message: String,
    /// Optional provider selection
    provider_id: Option<String>,
    /// Optional model override
    model: Option<String>,
    /// Optional ID of a message to quote/reply to
    quote_message_id: Option<i64>,
}

/// Request to create a new session and send the first message
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateSessionAndSendRequest {
    /// Agent ID (numeric database ID)
    agent_id: i64,
    /// Optional session title
    title: Option<String>,
    /// Message content
    message: String,
    /// Optional provider selection
    provider_id: Option<String>,
    /// Optional model override
    model: Option<String>,
    /// Optional ID of a message to quote/reply to
    quote_message_id: Option<i64>,
}

/// Memory context entry for frontend display
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MemoryContextEntry {
    id: String,
    content: String,
    similarity_score: Option<f32>,
    importance: u8,
    source_layer: String,
    created_at: i64,
}

/// Memory context result for frontend display
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MemoryContextInfo {
    entries: Vec<MemoryContextEntry>,
    total_chars: usize,
    retrieval_time_ms: u64,
}

impl From<omninova_core::agent::MemoryContextResult> for MemoryContextInfo {
    fn from(result: omninova_core::agent::MemoryContextResult) -> Self {
        MemoryContextInfo {
            entries: result.memories.into_iter().map(|m| MemoryContextEntry {
                id: m.entry.id,
                content: m.entry.content,
                similarity_score: m.entry.similarity_score,
                importance: m.entry.importance,
                source_layer: m.entry.source_layer.to_string(),
                created_at: m.entry.created_at,
            }).collect(),
            total_chars: result.total_chars,
            retrieval_time_ms: result.retrieval_time_ms,
        }
    }
}

/// Response from chat commands
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatResponse {
    /// The assistant's response text
    response: String,
    /// The session ID (existing or newly created)
    session_id: i64,
    /// The message ID of the assistant's response
    message_id: i64,
    /// Memory context used for this response (if any)
    memory_context: Option<MemoryContextInfo>,
}

impl From<ChatResult> for ChatResponse {
    fn from(result: ChatResult) -> Self {
        ChatResponse {
            response: result.response,
            session_id: result.session_id,
            message_id: result.message_id,
            memory_context: result.memory_context.map(MemoryContextInfo::from),
        }
    }
}

/// Send a message to an agent, creating a new session if needed
#[tauri::command]
async fn send_message(
    request: SendMessageRequest,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<ChatResponse, String> {
    let app_state = state.lock().await;

    // Get required stores
    let agent_store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;
    let db_pool = app_state.db_pool.as_ref()
        .ok_or_else(|| "Database not initialized. Call init_database first.".to_string())?;

    // Get the agent to find its default provider
    let agent = agent_store.find_by_id(request.agent_id)
        .map_err(|e| format!("Failed to find agent: {e}"))?
        .ok_or_else(|| format!("Agent not found: {}", request.agent_id))?;

    // Build provider from config
    let cfg = app_state.runtime.get_config().await;
    let provider_selection = ProviderSelection {
        provider: request.provider_id.or(agent.default_provider_id),
        model: request.model,
    };
    let provider = build_provider_with_selection(&cfg, &provider_selection);

    // Build memory from config
    let memory = build_memory_from_config(&cfg).await
        .map_err(|e| format!("Failed to initialize memory: {e}"))?;

    // Create agent service with memory manager support
    let mut service = AgentService::new(db_pool.clone(), memory, Vec::new());

    // Set memory manager if available for context enhancement
    if let Some(ref memory_manager) = app_state.memory_manager {
        service.set_memory_manager(Arc::clone(memory_manager));
    }

    // Set memory context config from app config
    service.set_memory_context_config(cfg.memory.context.clone());

    // Send message (creates new session)
    let result = service.chat(request.agent_id, None, &request.message, provider.as_ref(), request.quote_message_id)
        .await
        .map_err(|e| format!("Chat failed: {e}"))?;

    Ok(ChatResponse::from(result))
}

/// Send a message to an existing session
#[tauri::command]
async fn send_message_to_session(
    request: SendMessageToSessionRequest,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<ChatResponse, String> {
    let app_state = state.lock().await;

    // Get required stores
    let session_store = app_state.session_store.as_ref()
        .ok_or_else(|| "Session store not initialized. Call init_session_store first.".to_string())?;
    let db_pool = app_state.db_pool.as_ref()
        .ok_or_else(|| "Database not initialized. Call init_database first.".to_string())?;

    // Get the session to find its agent
    let session = session_store.find_by_id(request.session_id)
        .map_err(|e| format!("Failed to find session: {e}"))?
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    // Get agent to find provider
    let agent_store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;
    let agent = agent_store.find_by_id(session.agent_id)
        .map_err(|e| format!("Failed to find agent: {e}"))?
        .ok_or_else(|| format!("Agent not found: {}", session.agent_id))?;

    // Build provider from config
    let cfg = app_state.runtime.get_config().await;
    let provider_selection = ProviderSelection {
        provider: request.provider_id.or(agent.default_provider_id),
        model: request.model,
    };
    let provider = build_provider_with_selection(&cfg, &provider_selection);

    // Build memory from config
    let memory = build_memory_from_config(&cfg).await
        .map_err(|e| format!("Failed to initialize memory: {e}"))?;

    // Create agent service with memory manager support
    let mut service = AgentService::new(db_pool.clone(), memory, Vec::new());

    // Set memory manager if available for context enhancement
    if let Some(ref memory_manager) = app_state.memory_manager {
        service.set_memory_manager(Arc::clone(memory_manager));
    }

    // Set memory context config from app config
    service.set_memory_context_config(cfg.memory.context.clone());

    // Send message to existing session
    let result = service.chat(session.agent_id, Some(request.session_id), &request.message, provider.as_ref(), request.quote_message_id)
        .await
        .map_err(|e| format!("Chat failed: {e}"))?;

    Ok(ChatResponse::from(result))
}

/// Create a new session and send the first message
#[tauri::command]
async fn create_session_and_send(
    request: CreateSessionAndSendRequest,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<ChatResponse, String> {
    let app_state = state.lock().await;

    // Get required stores
    let agent_store = app_state.agent_store.as_ref()
        .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?;
    let db_pool = app_state.db_pool.as_ref()
        .ok_or_else(|| "Database not initialized. Call init_database first.".to_string())?;

    // Get the agent to find its default provider
    let agent = agent_store.find_by_id(request.agent_id)
        .map_err(|e| format!("Failed to find agent: {e}"))?
        .ok_or_else(|| format!("Agent not found: {}", request.agent_id))?;

    // Build provider from config
    let cfg = app_state.runtime.get_config().await;
    let provider_selection = ProviderSelection {
        provider: request.provider_id.or(agent.default_provider_id),
        model: request.model,
    };
    let provider = build_provider_with_selection(&cfg, &provider_selection);

    // Build memory from config
    let memory = build_memory_from_config(&cfg).await
        .map_err(|e| format!("Failed to initialize memory: {e}"))?;

    // Create agent service with memory manager support
    let mut service = AgentService::new(db_pool.clone(), memory, Vec::new());

    // Set memory manager if available for context enhancement
    if let Some(ref memory_manager) = app_state.memory_manager {
        service.set_memory_manager(Arc::clone(memory_manager));
    }

    // Set memory context config from app config
    service.set_memory_context_config(cfg.memory.context.clone());

    // Create session and send first message
    let result = service.create_session_and_chat(request.agent_id, request.title, &request.message, provider.as_ref(), request.quote_message_id)
        .await
        .map_err(|e| format!("Chat failed: {e}"))?;

    Ok(ChatResponse::from(result))
}

// ============================================================================
// Streaming Chat Commands (Story 4.3)
// ============================================================================

/// Request to start a streaming chat session
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamChatRequest {
    /// Agent ID (numeric database ID)
    agent_id: i64,
    /// Optional session ID (if continuing a conversation)
    session_id: Option<i64>,
    /// Message content
    message: String,
    /// Optional provider selection
    provider_id: Option<String>,
    /// Optional model override
    model: Option<String>,
    /// Optional ID of a message to quote/reply to
    quote_message_id: Option<i64>,
}

/// Start a streaming chat session.
///
/// Events are emitted to the frontend:
/// - `stream:start`: Stream has started
/// - `stream:delta`: Incremental content received
/// - `stream:toolCall`: Tool call during streaming
/// - `stream:done`: Stream completed successfully
/// - `stream:error`: Stream encountered an error
#[tauri::command]
async fn stream_chat(
    app: tauri::AppHandle,
    request: StreamChatRequest,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let (db_pool, agent_store, stream_manager, runtime) = {
        let app_state = state.lock().await;

        // Get required stores
        let db_pool = app_state.db_pool.as_ref()
            .ok_or_else(|| "Database not initialized. Call init_database first.".to_string())?
            .clone();
        let agent_store = app_state.agent_store.as_ref()
            .ok_or_else(|| "Agent store not initialized. Call init_agent_store first.".to_string())?
            .clone();
        let runtime = app_state.runtime.clone();
        let stream_manager = app_state.stream_manager.clone();

        Ok::<_, String>((db_pool, agent_store, stream_manager, runtime))
    }?;

    // Get the agent to find its default provider
    let agent = agent_store.find_by_id(request.agent_id)
        .map_err(|e| format!("Failed to find agent: {e}"))?
        .ok_or_else(|| format!("Agent not found: {}", request.agent_id))?;

    // Build provider from config
    let cfg = runtime.get_config().await;
    let provider_selection = ProviderSelection {
        provider: request.provider_id.or(agent.default_provider_id),
        model: request.model,
    };
    let provider = build_provider_with_selection(&cfg, &provider_selection);

    // Build memory from config
    let memory = build_memory_from_config(&cfg).await
        .map_err(|e| format!("Failed to initialize memory: {e}"))?;

    // Create agent service
    let service = AgentService::new(db_pool.clone(), memory, Vec::new());

    // Create event emitter closure
    let app_clone = app.clone();
    let emit_event = move |event: StreamEvent| {
        let event_name = event.event_name();
        if let Err(e) = app_clone.emit(event_name, &event) {
            tracing::error!("Failed to emit stream event {}: {}", event_name, e);
        }
    };

    // Wrap stream_manager in Arc for sharing
    let stream_manager_arc = Arc::new(stream_manager);

    // Run the streaming chat
    let result = service.chat_stream(
        request.agent_id,
        request.session_id,
        &request.message,
        provider.as_ref(),
        emit_event,
        Some(stream_manager_arc.clone()),
        request.quote_message_id,
    ).await.map_err(|e| format!("Streaming chat failed: {e}"))?;

    // Clean up the stream after completion
    stream_manager_arc.remove(result.session_id).await;

    Ok(())
}

/// Cancel an active streaming session.
///
/// This will stop the stream and emit a `stream:error` event with code `CANCELLED`.
#[tauri::command]
async fn cancel_stream(
    session_id: i64,
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<bool, String> {
    let stream_manager = {
        let app_state = state.lock().await;
        app_state.stream_manager.clone()
    };

    // Check if stream exists and cancel it
    let was_active = stream_manager.cancel(session_id).await;

    if was_active {
        // Emit cancellation event
        let event = StreamEvent::error("CANCELLED", "用户取消了流式响应");
        if let Err(e) = app.emit(event.event_name(), &event) {
            tracing::error!("Failed to emit cancel event: {}", e);
        }

        // Clean up the stream
        stream_manager.remove(session_id).await;
    }

    Ok(was_active)
}

// ============================================================================
// Command Execution Commands (Story 4.10)
// ============================================================================

/// Execute a chat command (e.g., /help, /clear, /export)
///
/// This command parses the input and executes it if it's a valid command.
/// Returns the command result to be displayed to the user.
#[tauri::command]
async fn execute_command(
    input: String,
    session_id: i64,
    agent_id: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<CommandResult, String> {
    // Parse the command
    let parsed = parse_command(&input)
        .ok_or_else(|| "输入不是有效的命令。命令应以 / 开头。".to_string())?;

    // Get the command registry
    let registry = {
        let app_state = state.lock().await;
        app_state.command_registry.clone()
    };

    // Create context
    let context = CommandContext {
        session_id,
        agent_id,
    };

    // Execute the command
    registry
        .execute(&parsed.name, parsed.args, context)
        .await
        .map_err(|e| e.to_string())
}

/// List all available commands
///
/// Returns a list of CommandInfo objects for all registered commands.
#[tauri::command]
async fn list_commands(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<CommandInfo>, String> {
    let registry = {
        let app_state = state.lock().await;
        app_state.command_registry.clone()
    };

    Ok(registry.list_info())
}

// ============================================================================
// Working Memory Commands (Story 5.1)
// ============================================================================

/// Get all working memory entries for the current session
///
/// Returns entries in chronological order (oldest first)
#[tauri::command]
async fn get_working_memory(
    limit: usize,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<WorkingMemoryEntry>, String> {
    let working_memory = {
        let app_state = state.lock().await;
        app_state.working_memory.clone()
    };

    let memory = working_memory.lock().await;
    memory.get_context(limit).await.map_err(|e| e.to_string())
}

/// Clear all working memory entries
#[tauri::command]
async fn clear_working_memory(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let working_memory = {
        let app_state = state.lock().await;
        app_state.working_memory.clone()
    };

    let memory = working_memory.lock().await;
    memory.clear().await.map_err(|e| e.to_string())
}

/// Get working memory statistics
#[tauri::command]
async fn get_memory_stats(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<MemoryStats, String> {
    let working_memory = {
        let app_state = state.lock().await;
        app_state.working_memory.clone()
    };

    let memory = working_memory.lock().await;
    Ok(memory.stats())
}

/// Set the session context for working memory
#[tauri::command]
async fn set_working_memory_session(
    session_id: i64,
    agent_id: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let working_memory = {
        let app_state = state.lock().await;
        app_state.working_memory.clone()
    };

    let mut memory = working_memory.lock().await;
    memory.set_session(session_id, agent_id);
    Ok(())
}

/// Push a context entry to working memory
#[tauri::command]
async fn push_working_memory_context(
    role: String,
    content: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let working_memory = {
        let app_state = state.lock().await;
        app_state.working_memory.clone()
    };

    let memory = working_memory.lock().await;
    memory.push_context(&role, &content).await.map_err(|e| e.to_string())
}

// ============================================================================
// Episodic Memory Commands (Story 5.2)
// ============================================================================

/// Store a new episodic memory
#[tauri::command]
async fn store_episodic_memory(
    agent_id: i64,
    session_id: Option<i64>,
    content: String,
    importance: u8,
    metadata: Option<String>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<i64, String> {
    let app_state = state.lock().await;

    let store = app_state.episodic_memory_store.as_ref()
        .ok_or_else(|| "Episodic memory store not initialized. Call init_session_store first.".to_string())?;

    let new_memory = NewEpisodicMemory {
        agent_id,
        session_id,
        content,
        importance,
        is_marked: false,
        metadata,
    };

    store.create(&new_memory).map_err(|e| e.to_string())
}

/// Get episodic memories by agent ID
#[tauri::command]
async fn get_episodic_memories(
    agent_id: i64,
    limit: usize,
    offset: usize,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<EpisodicMemory>, String> {
    let app_state = state.lock().await;

    let store = app_state.episodic_memory_store.as_ref()
        .ok_or_else(|| "Episodic memory store not initialized. Call init_session_store first.".to_string())?;

    store.find_by_agent(agent_id, limit, offset).map_err(|e| e.to_string())
}

/// Get episodic memories by session ID
#[tauri::command]
async fn get_episodic_memories_by_session(
    session_id: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<EpisodicMemory>, String> {
    let app_state = state.lock().await;

    let store = app_state.episodic_memory_store.as_ref()
        .ok_or_else(|| "Episodic memory store not initialized. Call init_session_store first.".to_string())?;

    store.find_by_session(session_id).map_err(|e| e.to_string())
}

/// Get episodic memories by importance
#[tauri::command]
async fn get_episodic_memories_by_importance(
    min_importance: u8,
    limit: usize,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<EpisodicMemory>, String> {
    let app_state = state.lock().await;

    let store = app_state.episodic_memory_store.as_ref()
        .ok_or_else(|| "Episodic memory store not initialized. Call init_session_store first.".to_string())?;

    store.find_by_importance(min_importance, limit).map_err(|e| e.to_string())
}

/// Delete an episodic memory
#[tauri::command]
async fn delete_episodic_memory(
    id: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<bool, String> {
    let app_state = state.lock().await;

    let store = app_state.episodic_memory_store.as_ref()
        .ok_or_else(|| "Episodic memory store not initialized. Call init_session_store first.".to_string())?;

    store.delete(id).map_err(|e| e.to_string())
}

/// Get episodic memory statistics
#[tauri::command]
async fn get_episodic_memory_stats(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<EpisodicMemoryStats, String> {
    let app_state = state.lock().await;

    let store = app_state.episodic_memory_store.as_ref()
        .ok_or_else(|| "Episodic memory store not initialized. Call init_session_store first.".to_string())?;

    store.stats().map_err(|e| e.to_string())
}

/// Export episodic memories for an agent
#[tauri::command]
async fn export_episodic_memories(
    agent_id: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;

    let store = app_state.episodic_memory_store.as_ref()
        .ok_or_else(|| "Episodic memory store not initialized. Call init_session_store first.".to_string())?;

    store.export_to_json(agent_id).map_err(|e| e.to_string())
}

/// Import episodic memories from JSON
#[tauri::command]
async fn import_episodic_memories(
    json: String,
    skip_duplicates: bool,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<usize, String> {
    let app_state = state.lock().await;

    let store = app_state.episodic_memory_store.as_ref()
        .ok_or_else(|| "Episodic memory store not initialized. Call init_session_store first.".to_string())?;

    store.import_from_json(&json, skip_duplicates).map_err(|e| e.to_string())
}

// ============================================================================
// Memory Mark Commands (Story 5.8 - 重要片段标记功能)
// ============================================================================

/// Mark an episodic memory as important
#[tauri::command]
async fn mark_episodic_memory_important(
    id: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<bool, String> {
    let app_state = state.lock().await;

    let store = app_state.episodic_memory_store.as_ref()
        .ok_or_else(|| "Episodic memory store not initialized. Call init_session_store first.".to_string())?;

    store.set_marked(id, true).map_err(|e| e.to_string())
}

/// Unmark an episodic memory (remove important flag)
#[tauri::command]
async fn unmark_episodic_memory_important(
    id: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<bool, String> {
    let app_state = state.lock().await;

    let store = app_state.episodic_memory_store.as_ref()
        .ok_or_else(|| "Episodic memory store not initialized. Call init_session_store first.".to_string())?;

    store.set_marked(id, false).map_err(|e| e.to_string())
}

/// Get marked episodic memories for an agent
#[tauri::command]
async fn get_marked_episodic_memories(
    agent_id: i64,
    limit: usize,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<EpisodicMemory>, String> {
    let app_state = state.lock().await;

    let store = app_state.episodic_memory_store.as_ref()
        .ok_or_else(|| "Episodic memory store not initialized. Call init_session_store first.".to_string())?;

    store.find_marked(agent_id, limit).map_err(|e| e.to_string())
}

/// Mark or unmark a message as important
///
/// Marked messages receive higher importance scores
/// when stored to episodic memory (L2).
///
/// [Source: Story 5.8 - 重要片段标记功能]
#[tauri::command]
async fn mark_message_important(
    message_id: i64,
    is_marked: bool,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<bool, String> {
    let app_state = state.lock().await;

    let store = app_state.message_store.as_ref()
        .ok_or_else(|| "Message store not initialized. Call init_session_store first.".to_string())?;

    store.set_marked(message_id, is_marked).map_err(|e| e.to_string())
}

/// End a session and persist working memory to L2 episodic memory
///
/// This should be called when a user closes a session to ensure
/// important context is saved to long-term storage.
#[tauri::command]
async fn end_session(
    agent_id: i64,
    session_id: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<usize, String> {
    let app_state = state.lock().await;

    // Check if episodic memory is enabled
    let episodic_store = match app_state.episodic_memory_store.as_ref() {
        Some(store) => store,
        None => {
            tracing::info!("Episodic memory not configured, skipping L1->L2 persistence");
            return Ok(0);
        }
    };

    // Get all entries from working memory
    let entries = app_state.working_memory.lock().await.get_context(0).await
        .map_err(|e| e.to_string())?;

    let mut count = 0;
    for entry in entries {
        // Determine importance based on role (user messages are more important)
        let importance = match entry.role.as_str() {
            "user" => 7,
            "assistant" => 5,
            "system" => 8,
            _ => 5,
        };

        let new_memory = NewEpisodicMemory {
            agent_id,
            session_id: Some(session_id),
            content: entry.content.clone(),
            importance,
            is_marked: false,
            metadata: Some(serde_json::json!({
                "role": entry.role,
                "timestamp": entry.timestamp,
            }).to_string()),
        };

        episodic_store.create(&new_memory).map_err(|e| e.to_string())?;
        count += 1;
    }

    tracing::info!("Persisted {} entries from L1 to L2 for session {}", count, session_id);
    Ok(count)
}

// ============================================================================
// Semantic Memory Commands (Story 5.3 - L3 Semantic Memory Layer)
// ============================================================================

/// Index an episodic memory to the semantic layer
/// This generates an embedding and stores it in the semantic memory store
#[tauri::command]
async fn index_episodic_memory(
    episodic_memory_id: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<i64, String> {
    let app_state = state.lock().await;

    let semantic_store = app_state.semantic_memory_store.as_ref()
        .ok_or_else(|| "Semantic memory store not initialized".to_string())?;

    let episodic_store = app_state.episodic_memory_store.as_ref()
        .ok_or_else(|| "Episodic memory store not initialized".to_string())?;

    // Get the episodic memory content
    let episodic = episodic_store.get(episodic_memory_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Episodic memory {} not found", episodic_memory_id))?;

    // Store with auto-generated embedding
    semantic_store.store_with_embedding(
        episodic_memory_id,
        &episodic.content,
        DEFAULT_OPENAI_EMBEDDING_MODEL,
    ).await.map_err(|e| e.to_string())
}

/// Search semantic memories by similarity
#[tauri::command]
async fn search_semantic_memories(
    query: String,
    k: usize,
    agent_id: Option<i64>,
    threshold: Option<f32>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<SemanticSearchResult>, String> {
    let app_state = state.lock().await;

    let store = app_state.semantic_memory_store.as_ref()
        .ok_or_else(|| "Semantic memory store not initialized".to_string())?;

    let threshold = threshold.unwrap_or(0.7);
    store.search_similar(&query, k, agent_id, threshold)
        .await
        .map_err(|e| e.to_string())
}

/// Get semantic memory statistics
#[tauri::command]
async fn get_semantic_memory_stats(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<SemanticMemoryStats, String> {
    let app_state = state.lock().await;

    let store = app_state.semantic_memory_store.as_ref()
        .ok_or_else(|| "Semantic memory store not initialized".to_string())?;

    store.stats().map_err(|e| e.to_string())
}

/// Delete a semantic memory embedding
#[tauri::command]
async fn delete_semantic_memory(
    id: i64,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<bool, String> {
    let app_state = state.lock().await;

    let store = app_state.semantic_memory_store.as_ref()
        .ok_or_else(|| "Semantic memory store not initialized".to_string())?;

    store.delete(id).map_err(|e| e.to_string())
}

/// Rebuild semantic index for an agent
/// This regenerates all embeddings from episodic memories
#[tauri::command]
async fn rebuild_semantic_index(
    agent_id: i64,
    model: Option<String>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<usize, String> {
    let app_state = state.lock().await;

    let store = app_state.semantic_memory_store.as_ref()
        .ok_or_else(|| "Semantic memory store not initialized".to_string())?;

    let model = model.unwrap_or_else(|| DEFAULT_OPENAI_EMBEDDING_MODEL.to_string());
    store.rebuild_index(agent_id, &model)
        .await
        .map_err(|e| e.to_string())
}

// ============================================================================
// Unified Memory Manager Commands (Story 5.4)
// ============================================================================

/// Store a memory using the unified MemoryManager
///
/// - Always stores to L1 (working memory)
/// - Optionally persists to L2 if persist_to_l2 is true
/// - Optionally indexes to L3 if index_to_l3 is true
#[tauri::command]
async fn memory_store(
    content: String,
    role: String,
    importance: u8,
    persist_to_l2: bool,
    index_to_l3: bool,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    // Get MemoryManager reference quickly, release AppState lock
    let manager = {
        let app_state = state.lock().await;
        app_state.memory_manager.clone()
            .ok_or_else(|| "Memory manager not initialized".to_string())?
    };

    // Perform operation with only MemoryManager lock held
    let manager = manager.lock().await;
    manager.store(&content, &role, importance, persist_to_l2, index_to_l3)
        .await
        .map_err(|e| format!("Failed to store memory: {}", e))
}

/// Retrieve memories using the unified MemoryManager
///
/// Queries layers based on the specified layer parameter:
/// - "L1": Only working memory
/// - "L2": Only episodic memory
/// - "L3": Only semantic memory
/// - "All" or other: Try L1 first, then L2, then L3
#[tauri::command]
async fn memory_retrieve(
    agent_id: i64,
    session_id: Option<i64>,
    layer: String,
    limit: usize,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<MemoryQueryResult, String> {
    let layer = match layer.as_str() {
        "L1" => MemoryLayer::L1,
        "L2" => MemoryLayer::L2,
        "L3" => MemoryLayer::L3,
        _ => MemoryLayer::All,
    };

    let query = MemoryQuery {
        agent_id,
        session_id,
        layer,
        limit,
        offset: 0,
        min_importance: None,
        time_range: None,
    };

    // Get MemoryManager reference quickly, release AppState lock
    let manager = {
        let app_state = state.lock().await;
        app_state.memory_manager.clone()
            .ok_or_else(|| "Memory manager not initialized".to_string())?
    };

    // Perform operation with only MemoryManager lock held
    let manager = manager.lock().await;
    manager.retrieve(query).await.map_err(|e| format!("Failed to retrieve memories: {}", e))
}

/// Search memories using semantic similarity (L3)
///
/// Returns memories sorted by similarity score (descending).
#[tauri::command]
async fn memory_search(
    query: String,
    k: usize,
    threshold: f32,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<UnifiedMemoryEntry>, String> {
    // Get MemoryManager reference quickly, release AppState lock
    let manager = {
        let app_state = state.lock().await;
        app_state.memory_manager.clone()
            .ok_or_else(|| "Memory manager not initialized".to_string())?
    };

    // Perform operation with only MemoryManager lock held
    let manager = manager.lock().await;
    manager.search(&query, k, threshold).await.map_err(|e| format!("Failed to search memories: {}", e))
}

/// Delete a memory from the specified layer(s)
///
/// - "L1": Not supported (L1 doesn't support direct deletion)
/// - "L2": Delete from episodic memory
/// - "L3": Delete from semantic memory only
/// - "All" or other: Delete from L2 and L3
#[tauri::command]
async fn memory_delete(
    id: String,
    layer: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<bool, String> {
    let layer = match layer.as_str() {
        "L1" => MemoryLayer::L1,
        "L2" => MemoryLayer::L2,
        "L3" => MemoryLayer::L3,
        _ => MemoryLayer::All,
    };

    // Get MemoryManager reference quickly, release AppState lock
    let manager = {
        let app_state = state.lock().await;
        app_state.memory_manager.clone()
            .ok_or_else(|| "Memory manager not initialized".to_string())?
    };

    // Perform operation with only MemoryManager lock held
    let manager = manager.lock().await;
    manager.delete(&id, layer).await.map_err(|e| format!("Failed to delete memory: {}", e))
}

/// Get statistics for all memory layers
#[tauri::command]
async fn memory_get_stats(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<MemoryManagerStats, String> {
    // Get MemoryManager reference quickly, release AppState lock
    let manager = {
        let app_state = state.lock().await;
        app_state.memory_manager.clone()
            .ok_or_else(|| "Memory manager not initialized".to_string())?
    };

    // Perform operation with only MemoryManager lock held
    let manager = manager.lock().await;
    manager.get_stats().await.map_err(|e| format!("Failed to get memory stats: {}", e))
}

/// Set the current session context for the MemoryManager
#[tauri::command]
async fn memory_set_session(
    session_id: i64,
    agent_id: Option<i64>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    // Get MemoryManager reference quickly, release AppState lock
    let manager = {
        let app_state = state.lock().await;
        app_state.memory_manager.clone()
            .ok_or_else(|| "Memory manager not initialized".to_string())?
    };

    // Perform operation with only MemoryManager lock held
    let manager = manager.lock().await;
    manager.set_session(session_id, agent_id).await;
    Ok(())
}

/// Persist session memories from L1 to L2
#[tauri::command]
async fn memory_persist_session(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<usize, String> {
    // Get MemoryManager reference quickly, release AppState lock
    let manager = {
        let app_state = state.lock().await;
        app_state.memory_manager.clone()
            .ok_or_else(|| "Memory manager not initialized".to_string())?
    };

    // Perform operation with only MemoryManager lock held
    let manager = manager.lock().await;
    manager.persist_session().await.map_err(|e| format!("Failed to persist session: {}", e))
}

/// Get memory performance statistics
#[tauri::command]
async fn memory_get_performance_stats(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<PerformanceStats, String> {
    // Get MemoryManager reference quickly, release AppState lock
    let manager = {
        let app_state = state.lock().await;
        app_state.memory_manager.clone()
            .ok_or_else(|| "Memory manager not initialized".to_string())?
    };

    // Perform operation with only MemoryManager lock held
    let manager = manager.lock().await;
    Ok(manager.get_performance_stats().await)
}

/// Run memory performance benchmark
#[tauri::command]
async fn memory_benchmark(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<BenchmarkResults, String> {
    // Get MemoryManager reference quickly, release AppState lock
    let manager = {
        let app_state = state.lock().await;
        app_state.memory_manager.clone()
            .ok_or_else(|| "Memory manager not initialized".to_string())?
    };

    // Perform operation with only MemoryManager lock held
    let manager = manager.lock().await;
    manager.benchmark().await.map_err(|e| format!("Benchmark failed: {}", e))
}

// ============================================================================
// Channel Status Commands (Story 6.7)
// ============================================================================

/// Channel info for frontend (matches TypeScript interface)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UiChannelInfo {
    id: String,
    name: String,
    kind: String,
    status: String,
    capabilities: u32,
    messages_sent: u64,
    messages_received: u64,
    last_activity: Option<i64>,
    error_message: Option<String>,
}

impl From<CoreChannelInfo> for UiChannelInfo {
    fn from(info: CoreChannelInfo) -> Self {
        Self {
            id: info.id,
            name: info.name,
            kind: info.kind.to_string(),
            status: format!("{:?}", info.status).to_lowercase(),
            capabilities: info.capabilities.bits(),
            messages_sent: info.messages_sent,
            messages_received: info.messages_received,
            last_activity: info.last_activity,
            error_message: info.error_message,
        }
    }
}

/// Initialize channel manager
///
/// Creates the channel manager instance. Call this after database initialization.
#[tauri::command]
async fn init_channel_manager(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let mut app_state = state.lock().await;

    if app_state.channel_manager.is_some() {
        return Ok(());
    }

    let manager = ChannelManager::new();
    app_state.channel_manager = Some(Arc::new(Mutex::new(manager)));

    Ok(())
}

/// Get all channels with their status
///
/// Returns a list of all configured channels with their current connection status.
#[tauri::command]
async fn get_all_channels(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<UiChannelInfo>, String> {
    let app_state = state.lock().await;

    // If channel manager is initialized, get channels from it
    if let Some(ref channel_manager) = app_state.channel_manager {
        let manager = channel_manager.lock().await;
        let channels = manager.list_channels();
        return Ok(channels.into_iter().map(UiChannelInfo::from).collect());
    }

    // Otherwise, return channels from config as "disconnected"
    let runtime = app_state.runtime.clone();
    let cfg = runtime.get_config().await;

    let mut channels = Vec::new();

    // Helper to add channel if enabled
    let add_channel = |kind: &str, entry: &omninova_core::config::ChannelEntry, list: &mut Vec<UiChannelInfo>| {
        if entry.enabled {
            list.push(UiChannelInfo {
                id: format!("{}-config", kind.to_lowercase()),
                name: kind.to_string(),
                kind: kind.to_lowercase(),
                status: "disconnected".to_string(),
                capabilities: 0,
                messages_sent: 0,
                messages_received: 0,
                last_activity: None,
                error_message: None,
            });
        }
    };

    if let Some(ref telegram) = cfg.channels_config.telegram {
        add_channel("Telegram", telegram, &mut channels);
    }
    if let Some(ref discord) = cfg.channels_config.discord {
        add_channel("Discord", discord, &mut channels);
    }
    if let Some(ref slack) = cfg.channels_config.slack {
        add_channel("Slack", slack, &mut channels);
    }
    if let Some(ref whatsapp) = cfg.channels_config.whatsapp {
        add_channel("WhatsApp", whatsapp, &mut channels);
    }
    if let Some(ref wechat) = cfg.channels_config.wechat {
        add_channel("WeChat", wechat, &mut channels);
    }
    if let Some(ref feishu) = cfg.channels_config.feishu {
        add_channel("Feishu", feishu, &mut channels);
    }
    if let Some(ref lark) = cfg.channels_config.lark {
        add_channel("Lark", lark, &mut channels);
    }
    if let Some(ref dingtalk) = cfg.channels_config.dingtalk {
        add_channel("DingTalk", dingtalk, &mut channels);
    }
    if let Some(ref matrix) = cfg.channels_config.matrix {
        add_channel("Matrix", matrix, &mut channels);
    }
    if let Some(ref email) = cfg.channels_config.email {
        add_channel("Email", email, &mut channels);
    }
    if let Some(ref msteams) = cfg.channels_config.msteams {
        add_channel("MSTeams", msteams, &mut channels);
    }
    if let Some(ref irc) = cfg.channels_config.irc {
        add_channel("IRC", irc, &mut channels);
    }
    if let Some(ref webhook) = cfg.channels_config.webhook {
        add_channel("Webhook", webhook, &mut channels);
    }

    Ok(channels)
}

/// Connect a channel
///
/// Attempts to establish connection to the specified channel.
#[tauri::command]
async fn connect_channel(
    channel_id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let app_state = state.lock().await;

    let channel_manager = app_state.channel_manager.as_ref()
        .ok_or_else(|| "Channel manager not initialized. Call init_channel_manager first.".to_string())?;

    let mut manager = channel_manager.lock().await;
    manager.connect_channel(&channel_id).await
        .map_err(|e| format!("连接渠道失败: {}", e))
}

/// Disconnect a channel
///
/// Gracefully disconnects the specified channel.
#[tauri::command]
async fn disconnect_channel(
    channel_id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let app_state = state.lock().await;

    let channel_manager = app_state.channel_manager.as_ref()
        .ok_or_else(|| "Channel manager not initialized. Call init_channel_manager first.".to_string())?;

    let mut manager = channel_manager.lock().await;
    manager.disconnect_channel(&channel_id).await
        .map_err(|e| format!("断开渠道失败: {}", e))
}

/// Retry channel connection
///
/// Retries connection for a channel in error state.
#[tauri::command]
async fn retry_channel_connection(
    channel_id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let app_state = state.lock().await;

    let channel_manager = app_state.channel_manager.as_ref()
        .ok_or_else(|| "Channel manager not initialized. Call init_channel_manager first.".to_string())?;

    let mut manager = channel_manager.lock().await;
    manager.try_reconnect(&channel_id).await
        .map_err(|e| format!("重试连接失败: {}", e))
}

// ============================================================================
// Channel Configuration Commands (Story 6.8)
// ============================================================================

/// Channel config for create/update requests
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChannelConfigRequest {
    name: String,
    kind: String,
    enabled: bool,
    behavior: serde_json::Value,
    agent_id: Option<String>,
}

/// Create a new channel
///
/// Creates a new channel configuration and returns the channel info.
#[tauri::command]
async fn create_channel(
    config: ChannelConfigRequest,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<UiChannelInfo, String> {
    let app_state = state.lock().await;

    let channel_manager = app_state.channel_manager.as_ref()
        .ok_or_else(|| "Channel manager not initialized. Call init_channel_manager first.".to_string())?;

    let mut manager = channel_manager.lock().await;

    // Generate a new channel ID
    let channel_id = format!("{}-{}", config.kind.to_lowercase(), uuid::Uuid::new_v4());

    // Parse channel kind from string using serde
    let kind: ChannelKind = serde_json::from_value(serde_json::json!(config.kind))
        .map_err(|e| format!("无效的渠道类型: {}", e))?;

    // Parse behavior config
    let behavior: ChannelBehaviorConfig =
        serde_json::from_value(config.behavior.clone())
            .map_err(|e| format!("解析行为配置失败: {}", e))?;

    // Create ChannelSettings with the behavior config
    let settings = ChannelSettings {
        behavior: behavior.clone(),
        ..Default::default()
    };

    // Create channel config (credentials will be set separately via save_channel_credentials)
    let channel_config = ChannelConfig::new(
        channel_id.clone(),
        kind.clone(),
        Credentials::None,
    )
    .with_name(config.name.clone())
    .with_settings(settings);

    // Create channel in manager
    manager.create_channel(channel_config)
        .map_err(|e| format!("创建渠道失败: {}", e))?;

    // Save behavior config to database if available
    if let Some(ref behavior_store) = app_state.channel_behavior_store {
        ChannelBehaviorStore::save(behavior_store.as_ref(), &channel_id, &behavior)
            .map_err(|e| format!("保存行为配置失败: {}", e))?;
    }

    Ok(UiChannelInfo {
        id: channel_id,
        name: config.name,
        kind: config.kind,
        status: "disconnected".to_string(),
        capabilities: 0,
        messages_sent: 0,
        messages_received: 0,
        last_activity: None,
        error_message: None,
    })
}

/// Update an existing channel
///
/// Updates the channel configuration.
#[tauri::command]
async fn update_channel(
    channel_id: String,
    config: ChannelConfigRequest,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let app_state = state.lock().await;

    let channel_manager = app_state.channel_manager.as_ref()
        .ok_or_else(|| "Channel manager not initialized".to_string())?;

    let mut manager = channel_manager.lock().await;

    // Parse behavior config
    let behavior: ChannelBehaviorConfig =
        serde_json::from_value(config.behavior.clone())
            .map_err(|e| format!("解析行为配置失败: {}", e))?;

    // Update behavior config in manager
    manager.update_behavior_config(&channel_id, behavior.clone())
        .map_err(|e| format!("更新渠道行为配置失败: {}", e))?;

    // Update behavior config in database if available
    if let Some(ref behavior_store) = app_state.channel_behavior_store {
        ChannelBehaviorStore::save(behavior_store.as_ref(), &channel_id, &behavior)
            .map_err(|e| format!("保存行为配置失败: {}", e))?;
    }

    Ok(())
}

/// Delete a channel
///
/// Removes a channel configuration and disconnects it.
#[tauri::command]
async fn delete_channel(
    channel_id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let app_state = state.lock().await;

    let channel_manager = app_state.channel_manager.as_ref()
        .ok_or_else(|| "Channel manager not initialized. Call init_channel_manager first.".to_string())?;

    let mut manager = channel_manager.lock().await;

    // Disconnect and remove channel
    manager.remove_channel(&channel_id)
        .map_err(|e| format!("删除渠道失败: {}", e))?;

    // Delete behavior config from database if available
    if let Some(ref behavior_store) = app_state.channel_behavior_store {
        let _ = ChannelBehaviorStore::delete(behavior_store.as_ref(), &channel_id); // Ignore error if not found
    }

    Ok(())
}

/// Test channel connection
///
/// Tests if the channel configuration is valid by attempting to connect.
#[tauri::command]
async fn test_channel_connection(
    channel_id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<bool, String> {
    let app_state = state.lock().await;

    let channel_manager = app_state.channel_manager.as_ref()
        .ok_or_else(|| "Channel manager not initialized. Call init_channel_manager first.".to_string())?;

    let mut manager = channel_manager.lock().await;

    // Try to connect and check status
    match manager.connect_channel(&channel_id).await {
        Ok(()) => {
            // Wait a moment and check status
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            let status = manager.get_channel_info(&channel_id)
                .map(|info| info.status)
                .unwrap_or(omninova_core::channels::types::ChannelStatus::Disconnected);
            Ok(status == omninova_core::channels::types::ChannelStatus::Connected)
        }
        Err(e) => {
            Err(format!("连接测试失败: {}", e))
        }
    }
}

/// Save channel credentials
///
/// Saves channel credentials to secure storage (OS Keychain).
#[tauri::command]
async fn save_channel_credentials(
    channel_id: String,
    credentials: serde_json::Value,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let app_state = state.lock().await;

    // Use keyring service if available
    if let Some(ref keyring) = app_state.keyring_service {
        let credentials_json = serde_json::to_string(&credentials)
            .map_err(|e| format!("序列化凭据失败: {}", e))?;

        let reference = KeyReference::new("channels", &channel_id, "credentials");
        keyring.save_secret(&reference, &credentials_json)
            .await
            .map_err(|e| format!("保存凭据失败: {}", e))?;

        return Ok(());
    }

    // Fallback: store in config (not recommended for production)
    Err("Keychain service not available. Credentials storage requires OS keychain integration.".to_string())
}

/// Get channel credentials
///
/// Retrieves channel credentials from secure storage.
#[tauri::command]
async fn get_channel_credentials(
    channel_id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<serde_json::Value, String> {
    let app_state = state.lock().await;

    // Use keyring service if available
    if let Some(ref keyring) = app_state.keyring_service {
        let reference = KeyReference::new("channels", &channel_id, "credentials");
        let credentials_json = keyring.get_secret(&reference)
            .await
            .map_err(|e| format!("获取凭据失败: {}", e))?;

        let credentials: serde_json::Value = serde_json::from_str(&credentials_json)
            .map_err(|e| format!("解析凭据失败: {}", e))?;

        return Ok(credentials);
    }

    Err("Keychain service not available. Credentials retrieval requires OS keychain integration.".to_string())
}

/// Get channel config
///
/// Retrieves the full channel configuration including behavior settings.
#[tauri::command]
async fn get_channel_config(
    channel_id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<serde_json::Value, String> {
    let app_state = state.lock().await;

    let channel_manager = app_state.channel_manager.as_ref()
        .ok_or_else(|| "Channel manager not initialized".to_string())?;

    let manager = channel_manager.lock().await;

    // Get channel info from manager
    let channel_info = manager.get_channel_info(&channel_id)
        .ok_or_else(|| format!("渠道不存在: {}", channel_id))?;

    // Get behavior config from database if available
    let behavior = if let Some(ref behavior_store) = app_state.channel_behavior_store {
        ChannelBehaviorStore::load(behavior_store.as_ref(), &channel_id)
            .map_err(|e| format!("加载行为配置失败: {}", e))?
            .unwrap_or_default()
    } else {
        ChannelBehaviorConfig::default()
    };

    Ok(serde_json::json!({
        "id": channel_info.id,
        "name": channel_info.name,
        "kind": channel_info.kind,
        "enabled": channel_info.status != omninova_core::channels::types::ChannelStatus::Disconnected,
        "behavior": behavior,
        "status": channel_info.status,
    }))
}

/// Save channel behavior config
///
/// Updates just the behavior configuration for a channel.
#[tauri::command]
async fn save_channel_behavior(
    channel_id: String,
    behavior: omninova_core::channels::behavior::ChannelBehaviorConfig,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let app_state = state.lock().await;

    if let Some(ref behavior_store) = app_state.channel_behavior_store {
        ChannelBehaviorStore::save(behavior_store.as_ref(), &channel_id, &behavior)
            .map_err(|e| format!("保存行为配置失败: {}", e))?;
        return Ok(());
    }

    Err("Behavior store not initialized".to_string())
}

async fn gateway_status_from_state(state: &Arc<Mutex<AppState>>) -> GatewayStatusPayload {
    let (runtime, running, last_error): (GatewayRuntime, bool, Option<String>) = {
        let app_state = state.lock().await;
        (
            app_state.runtime.clone(),
            app_state.gateway_task.is_some(),
            app_state.last_gateway_error.clone(),
        )
    };
    let cfg = runtime.get_config().await;

    GatewayStatusPayload {
        running,
        url: format!("http://{}:{}", cfg.gateway.host, cfg.gateway.port),
        last_error,
    }
}

async fn sync_gateway_task_state(state: &Arc<Mutex<AppState>>) {
    let finished_task = {
        let mut app_state = state.lock().await;
        if app_state
            .gateway_task
            .as_ref()
            .is_some_and(|task| task.is_finished())
        {
            app_state.gateway_task.take()
        } else {
            None
        }
    };

    let Some(task) = finished_task else {
        return;
    };

    let last_error = match task.await {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(error.to_string()),
        Err(error) if error.is_cancelled() => None,
        Err(error) => Some(error.to_string()),
    };

    let mut app_state = state.lock().await;
    app_state.last_gateway_error = last_error.clone();

    // Emit error event via broadcast if gateway failed
    if let Some(error) = last_error {
        let runtime = app_state.runtime.clone();
        let cfg = runtime.get_config().await;
        let status = GatewayStatusPayload {
            running: false,
            url: format!("http://{}:{}", cfg.gateway.host, cfg.gateway.port),
            last_error: Some(error),
        };
        let _ = app_state.gateway_status_tx.send(status);
    }
}

async fn stop_gateway_inner(state: &Arc<Mutex<AppState>>) {
    let mut app_state = state.lock().await;
    if let Some(task) = app_state.gateway_task.take() {
        task.abort();
    }
    app_state.last_gateway_error = None;
}

fn setup_config_from_core(config: &Config) -> SetupAppConfig {
    let mut providers = if !config.model_providers.is_empty() {
        config
            .model_providers
            .iter()
            .map(|(id, provider)| SetupProviderConfig {
                id: id.clone(),
                name: display_provider_name(id),
                provider_type: id.clone(),
                api_key_env: provider.api_key_env.clone(),
                base_url: provider.base_url.clone(),
                models: with_default_model(provider.models.clone(), provider.default_model.clone()),
                enabled: provider.enabled,
            })
            .collect::<Vec<_>>()
    } else {
        config
            .providers
            .iter()
            .map(|provider| SetupProviderConfig {
                id: provider.id.clone(),
                name: provider.name.clone(),
                provider_type: provider.provider_type.clone(),
                api_key_env: provider.api_key_env.clone(),
                base_url: provider.base_url.clone(),
                models: provider.models.clone(),
                enabled: provider.enabled,
            })
            .collect::<Vec<_>>()
    };

    providers.sort_by(|left, right| left.name.cmp(&right.name));

    SetupAppConfig {
        api_key: config.api_key.clone(),
        api_url: config.api_url.clone(),
        default_provider: config.default_provider.clone(),
        default_model: config.default_model.clone(),
        workspace_dir: config.workspace_dir.to_string_lossy().to_string(),
        omninoval_gateway_url: Some(format!(
            "http://{}:{}",
            config.gateway.host, config.gateway.port
        )),
        omninoval_config_dir: config
            .config_path
            .parent()
            .map(|path| path.to_string_lossy().to_string()),
        robot: config.robot.clone(),
        providers,
        channels: Some(channels_from_core(&config.channels_config)),
    }
}

fn channel_entry_from_core(entry: &Option<ChannelEntry>) -> Option<SetupChannelEntry> {
    let entry = entry.as_ref()?;
    Some(SetupChannelEntry {
        enabled: entry.enabled,
        token: entry.token.clone(),
        token_env: entry.token_env.clone(),
    })
}

fn channels_from_core(cfg: &ChannelsConfig) -> SetupChannelsConfig {
    SetupChannelsConfig {
        telegram: channel_entry_from_core(&cfg.telegram),
        discord: channel_entry_from_core(&cfg.discord),
        slack: channel_entry_from_core(&cfg.slack),
        whatsapp: channel_entry_from_core(&cfg.whatsapp),
        wechat: channel_entry_from_core(&cfg.wechat),
        feishu: channel_entry_from_core(&cfg.feishu),
        lark: channel_entry_from_core(&cfg.lark),
        dingtalk: channel_entry_from_core(&cfg.dingtalk),
        matrix: channel_entry_from_core(&cfg.matrix),
        email: channel_entry_from_core(&cfg.email),
        msteams: channel_entry_from_core(&cfg.msteams),
        irc: channel_entry_from_core(&cfg.irc),
        webhook: channel_entry_from_core(&cfg.webhook),
    }
}

fn inbound_from_payload(payload: UiInboundPayload) -> InboundMessage {
    InboundMessage {
        channel: payload.channel.unwrap_or(ChannelKind::Cli),
        user_id: normalize_optional_string(payload.user_id),
        session_id: normalize_optional_string(payload.session_id),
        text: payload.text.trim().to_string(),
        metadata: payload.metadata,
    }
}

fn collect_provider_ids(config: &Config) -> Vec<String> {
    let mut ids = config.model_providers.keys().cloned().collect::<Vec<_>>();
    if ids.is_empty() {
        ids.extend(config.providers.iter().map(|provider| provider.id.clone()));
    } else {
        for provider in &config.providers {
            if !ids.iter().any(|id| id == &provider.id) {
                ids.push(provider.id.clone());
            }
        }
    }
    if let Some(default_provider) = config.default_provider.clone() {
        if !ids.iter().any(|id| id == &default_provider) {
            ids.push(default_provider);
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

fn default_provider_base_url(id: &str, config: &Config) -> Option<String> {
    if let Some(api_url) = config.api_url.clone() {
        return Some(api_url);
    }
    match id {
        "openrouter" => Some("https://openrouter.ai/api/v1".to_string()),
        "ollama" => Some("http://localhost:11434/v1".to_string()),
        "deepseek" => Some("https://api.deepseek.com".to_string()),
        "qwen" => Some("https://dashscope.aliyuncs.com/compatible-mode/v1".to_string()),
        "moonshot" => Some("https://api.moonshot.cn/v1".to_string()),
        "groq" => Some("https://api.groq.com/openai/v1".to_string()),
        "xai" => Some("https://api.x.ai/v1".to_string()),
        "mistral" => Some("https://api.mistral.ai/v1".to_string()),
        "lmstudio" => Some("http://localhost:1234/v1".to_string()),
        _ => None,
    }
}

fn setup_config_to_core(
    mut current: Config,
    setup: SetupAppConfig,
) -> Result<Config, String> {
    current.api_key = normalize_optional_string(setup.api_key);
    current.api_url = normalize_optional_string(setup.api_url);
    current.default_provider = normalize_optional_string(setup.default_provider);
    current.default_model = normalize_optional_string(setup.default_model);

    if !setup.workspace_dir.trim().is_empty() {
        current.workspace_dir = expand_tilde_path(&setup.workspace_dir);
    }

    if let Some(config_dir) = normalize_optional_string(setup.omninoval_config_dir) {
        current.config_path = expand_tilde_path(&config_dir).join("config.toml");
    }

    if let Some(gateway_url) = normalize_optional_string(setup.omninoval_gateway_url) {
        let (host, port) = parse_gateway_url(&gateway_url)?;
        current.gateway.host = host;
        current.gateway.port = port;
    }

    current.robot = setup.robot;
    current.providers = setup
        .providers
        .iter()
        .map(|provider| ProviderConfig {
            id: provider.id.clone(),
            name: provider.name.clone(),
            provider_type: provider.provider_type.clone(),
            api_key_env: normalize_optional_string(provider.api_key_env.clone()),
            base_url: normalize_optional_string(provider.base_url.clone()),
            models: provider.models.clone(),
            enabled: provider.enabled,
        })
        .collect();
    current.model_providers = setup
        .providers
        .iter()
        .map(|provider| {
            let provider_default_model = if current.default_provider.as_deref() == Some(&provider.id)
            {
                current.default_model.clone()
            } else {
                provider.models.first().cloned()
            };

            (
                provider.id.clone(),
                ModelProviderConfig {
                    api_key: None,
                    api_key_env: normalize_optional_string(provider.api_key_env.clone()),
                    base_url: normalize_optional_string(provider.base_url.clone()),
                    default_model: provider_default_model,
                    models: provider.models.clone(),
                    enabled: provider.enabled,
                    timeout_secs: None,
                },
            )
        })
        .collect::<HashMap<_, _>>();

    if let Some(channels) = setup.channels {
        current.channels_config = channels_to_core(channels);
    }

    current.validate_or_bail().map_err(|e| e.to_string())?;
    Ok(current)
}

fn channel_entry_to_core(entry: Option<SetupChannelEntry>) -> Option<ChannelEntry> {
    let entry = entry?;
    if !entry.enabled && entry.token.is_none() && entry.token_env.is_none() {
        return None;
    }
    Some(ChannelEntry {
        enabled: entry.enabled,
        token: normalize_optional_string(entry.token),
        token_env: normalize_optional_string(entry.token_env),
        extra: HashMap::new(),
    })
}

fn channels_to_core(setup: SetupChannelsConfig) -> ChannelsConfig {
    ChannelsConfig {
        telegram: channel_entry_to_core(setup.telegram),
        discord: channel_entry_to_core(setup.discord),
        slack: channel_entry_to_core(setup.slack),
        whatsapp: channel_entry_to_core(setup.whatsapp),
        wechat: channel_entry_to_core(setup.wechat),
        feishu: channel_entry_to_core(setup.feishu),
        lark: channel_entry_to_core(setup.lark),
        dingtalk: channel_entry_to_core(setup.dingtalk),
        matrix: channel_entry_to_core(setup.matrix),
        email: channel_entry_to_core(setup.email),
        msteams: channel_entry_to_core(setup.msteams),
        irc: channel_entry_to_core(setup.irc),
        webhook: channel_entry_to_core(setup.webhook),
        ..ChannelsConfig::default()
    }
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn expand_tilde_path(value: &str) -> PathBuf {
    if value == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(value));
    }

    if let Some(rest) = value.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            return home.join(rest);
        }
    }

    PathBuf::from(value)
}

fn parse_gateway_url(value: &str) -> Result<(String, u16), String> {
    let normalized = value
        .trim()
        .trim_end_matches('/')
        .strip_prefix("http://")
        .or_else(|| value.trim().trim_end_matches('/').strip_prefix("https://"))
        .unwrap_or(value.trim().trim_end_matches('/'))
        .split('/')
        .next()
        .unwrap_or_default()
        .to_string();

    let (host, port) = normalized
        .rsplit_once(':')
        .ok_or_else(|| "Gateway 地址格式应为 http://host:port".to_string())?;

    let port = port
        .parse::<u16>()
        .map_err(|_| "Gateway 端口无效".to_string())?;

    if host.trim().is_empty() {
        return Err("Gateway 主机不能为空".to_string());
    }

    Ok((host.to_string(), port))
}

fn with_default_model(models: Vec<String>, default_model: Option<String>) -> Vec<String> {
    match default_model {
        Some(default_model) if !models.contains(&default_model) => {
            let mut next = vec![default_model];
            next.extend(models);
            next
        }
        _ => models,
    }
}

fn display_provider_name(id: &str) -> String {
    match id {
        "openai" => "OpenAI".to_string(),
        "anthropic" => "Anthropic".to_string(),
        "gemini" => "Google Gemini".to_string(),
        "deepseek" => "DeepSeek".to_string(),
        "qwen" => "Qwen / DashScope".to_string(),
        "moonshot" => "Moonshot".to_string(),
        "groq" => "Groq".to_string(),
        "openrouter" => "OpenRouter".to_string(),
        "ollama" => "Ollama (Local)".to_string(),
        "lmstudio" => "LM Studio (Local)".to_string(),
        "xai" => "xAI".to_string(),
        "mistral" => "Mistral".to_string(),
        other => other.to_string(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    omninova_core::init().expect("Failed to initialize core");

    let config = Config::load_or_init().expect("Failed to load config");
    let report = config.validate();
    for w in &report.warnings {
        eprintln!("[config warning] {w}");
    }
    if !report.is_ok() {
        for e in &report.errors {
            eprintln!("[config error] {e}");
        }
    }

    let config_path = config.config_path.clone();
    let runtime = GatewayRuntime::new(config.clone());

    // Get the shared config reference from GatewayRuntime
    let shared_config = runtime.config_ref();

    // Create ConfigManager with the shared config reference so changes propagate to GatewayRuntime
    let mut config_manager = ConfigManager::with_shared_config(shared_config, config_path.clone());

    // Register callback to log config changes
    config_manager.on_change(|_new_config| {
        tracing::info!("Config file changed, new config loaded");
        // The shared Arc<RwLock<Config>> is already updated by ConfigWatcher
        // GatewayRuntime will see the changes when it calls get_config()
    });

    // Start watching for config file changes
    match config_manager.start_watching() {
        Ok(()) => tracing::info!("Started config file watcher for {:?}", config_path),
        Err(e) => tracing::error!("Failed to start config watcher: {}", e),
    }

    let config_manager = Arc::new(config_manager);

    // Create broadcast channel for gateway status events (Task 2.5)
    let (gateway_status_tx, _) = broadcast::channel::<GatewayStatusPayload>(16);

    let state = Arc::new(Mutex::new(AppState {
        runtime,
        gateway_task: None,
        last_gateway_error: None,
        gateway_status_tx,
        db_pool: None,
        agent_store: None,
        account_store: None,
        provider_store: None,
        session_store: None,
        message_store: None,
        config_manager: Some(config_manager),
        keyring_service: None,
        stream_manager: StreamManager::new(),
        command_registry: Arc::new(CommandRegistry::with_defaults()),
        working_memory: Arc::new(Mutex::new(WorkingMemory::new())),
        episodic_memory_store: None,
        semantic_memory_store: None,
        memory_manager: None,
        channel_manager: None,
        channel_behavior_store: None,
        skill_registry: None,
        skill_executor: None,
        api_key_store: None,
        api_log_store: None,
    }));

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            process_message,
            get_config,
            save_config,
            reload_config,
            get_config_path,
            subscribe_config_changes,
            get_setup_config,
            save_setup_config,
            gateway_status,
            gateway_health,
            provider_health_overview,
            route_inbound_message,
            process_inbound_message,
            session_tree_snapshot,
            check_browser_dep,
            install_browser_dep,
            start_gateway,
            stop_gateway,
            import_skills,
            init_database,
            get_database_status,
            init_agent_store,
            get_agents,
            get_agent_by_id,
            create_agent,
            update_agent,
            delete_agent,
            duplicate_agent,
            // Agent Style Config commands (Story 7.1)
            get_agent_style_config,
            update_agent_style_config,
            preview_style_effect,
            // Context Window Config commands (Story 7.2)
            get_context_window_config,
            update_context_window_config,
            estimate_tokens,
            get_model_context_recommendations,
            // Trigger Keywords Config commands (Story 7.3)
            get_trigger_keywords_config,
            update_trigger_keywords_config,
            test_trigger_match,
            test_single_trigger,
            validate_trigger_keyword,
            // Privacy Config commands (Story 7.4)
            get_privacy_config,
            update_privacy_config,
            // Unified Agent Configuration commands (Story 7.7)
            get_agent_configuration,
            update_agent_configuration,
            test_sensitive_filter,
            validate_exclusion_pattern,
            validate_filter_pattern,
            // Skill System commands (Story 7.5)
            init_skill_registry,
            list_available_skills,
            get_skill_info,
            execute_skill,
            validate_skill_config,
            register_custom_skill,
            list_skills_by_tag,
            list_skill_tags,
            // Agent Skill Configuration commands (Story 7.6)
            get_agent_skill_config,
            update_agent_skill_config,
            toggle_agent_skill,
            get_skill_execution_logs,
            get_skill_usage_stats,
            get_mbti_types,
            get_mbti_traits,
            get_mbti_config,
            // Account commands
            init_account_store,
            get_account,
            create_account,
            verify_password,
            has_account,
            update_password,
            get_require_password_on_startup,
            set_require_password_on_startup,
            update_account,
            delete_account,
            // Provider commands
            init_provider_store,
            get_provider_configs,
            get_provider_config_by_id,
            create_provider_config,
            update_provider_config,
            delete_provider_config,
            set_default_provider_config,
            test_provider_connection,
            // Agent Provider Assignment commands (Story 3.7)
            set_agent_default_provider,
            get_agent_provider,
            validate_provider_for_agent,
            // Backup commands
            export_config_backup,
            validate_backup_file,
            import_config_backup,
            // Privacy & Security commands
            get_privacy_settings,
            update_privacy_settings,
            get_data_storage_info,
            clear_conversation_history,
            toggle_encryption,
            is_encryption_available,
            // Keyring commands
            init_keyring_service,
            save_api_key,
            get_api_key,
            delete_api_key,
            api_key_exists,
            get_keyring_store_type,
            // API Key commands (Story 8.3)
            init_api_key_store,
            create_api_key,
            list_api_keys,
            revoke_api_key,
            delete_gateway_api_key,
            get_gateway_api_key,
            // API Log commands (Story 8.4)
            init_api_log_store,
            list_api_logs,
            count_api_logs,
            get_api_usage_stats,
            get_endpoint_stats,
            get_api_key_stats,
            export_api_logs,
            clear_api_logs,
            clear_all_api_logs,
            // Session commands (Story 4.1)
            init_session_store,
            create_session,
            get_session,
            list_sessions_by_agent,
            update_session,
            delete_session,
            create_message,
            list_messages_by_session,
            // Chat commands (Story 4.2)
            send_message,
            send_message_to_session,
            create_session_and_send,
            // Streaming commands (Story 4.3)
            stream_chat,
            cancel_stream,
            // Command execution commands (Story 4.10)
            execute_command,
            list_commands,
            // Working memory commands (Story 5.1)
            get_working_memory,
            clear_working_memory,
            get_memory_stats,
            set_working_memory_session,
            push_working_memory_context,
            // Episodic memory commands (Story 5.2)
            store_episodic_memory,
            get_episodic_memories,
            get_episodic_memories_by_session,
            get_episodic_memories_by_importance,
            delete_episodic_memory,
            get_episodic_memory_stats,
            export_episodic_memories,
            import_episodic_memories,
            // Memory mark commands (Story 5.8 - 重要片段标记功能)
            mark_episodic_memory_important,
            unmark_episodic_memory_important,
            get_marked_episodic_memories,
            // Message mark commands (Story 5.8 - Important fragment marking)
            mark_message_important,
            // Session lifecycle commands (Story 5.2 - L1→L2 persistence)
            end_session,
            // Semantic memory commands (Story 5.3 - L3 Semantic Memory)
            index_episodic_memory,
            search_semantic_memories,
            get_semantic_memory_stats,
            delete_semantic_memory,
            rebuild_semantic_index,
            // Unified Memory Manager commands (Story 5.4)
            memory_store,
            memory_retrieve,
            memory_search,
            memory_delete,
            memory_get_stats,
            memory_set_session,
            memory_persist_session,
            // Performance metrics commands (Story 5.5)
            memory_get_performance_stats,
            memory_benchmark,
            // Channel status commands (Story 6.7)
            init_channel_manager,
            get_all_channels,
            connect_channel,
            disconnect_channel,
            retry_channel_connection,
            // Channel configuration commands (Story 6.8)
            create_channel,
            update_channel,
            delete_channel,
            test_channel_connection,
            save_channel_credentials,
            get_channel_credentials,
            get_channel_config,
            save_channel_behavior,
        ])
        .setup(|_app| {
            #[cfg(debug_assertions)]
            {
                let window = _app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
