use omninova_core::account::{AccountStore, AccountInfo, NewAccount, AccountUpdate};
use omninova_core::agent::{AgentStore, AgentUpdate, NewAgent, MbtiType, PersonalityTraits, PersonalityConfig};
use omninova_core::backup::{
    BackupService, BackupMeta, BackupFormat,
    ImportMode, ImportOptions,
    deserialize_backup, validate_backup,
};
use omninova_core::channels::{ChannelKind, InboundMessage};
use omninova_core::config::{Config, ModelProviderConfig, ProviderConfig, RobotConfig, ChannelsConfig, ChannelEntry, ConfigManager};
use omninova_core::db::{create_pool, create_builtin_runner, DbPool, DbPoolConfig};
use omninova_core::gateway::{
    GatewayHealth, GatewayInboundResponse, GatewayRuntime, GatewaySessionTreeQuery,
    GatewaySessionTreeResponse,
};
use omninova_core::privacy::{
    PrivacySettings, StorageInfo, ClearOptions, ClearResult,
};
use omninova_core::providers::{ProviderSelection, build_provider_with_selection, ProviderStore, NewProviderConfig, ProviderConfigUpdate};
use omninova_core::routing::RouteDecision;
use omninova_core::security::EncryptionKeyManager;
use omninova_core::security::KeyringService;
use omninova_core::skills::import_skills_from_dir;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::{Duration, sleep};

struct AppState {
    runtime: GatewayRuntime,
    gateway_task: Option<JoinHandle<Result<(), String>>>,
    last_gateway_error: Option<String>,
    db_pool: Option<DbPool>,
    agent_store: Option<AgentStore>,
    account_store: Option<AccountStore>,
    provider_store: Option<ProviderStore>,
    /// Config manager with file watcher for hot reload.
    /// This field keeps the watcher alive for the lifetime of the app.
    #[allow(dead_code)]
    config_manager: Option<Arc<ConfigManager>>,
    /// Keyring service for secure API key storage
    keyring_service: Option<Arc<KeyringService>>,
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
        return Err(
            status
                .last_error
                .clone()
                .unwrap_or_else(|| "网关启动失败".to_string()),
        );
    }

    Ok(status)
}

#[tauri::command]
async fn stop_gateway(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<GatewayStatusPayload, String> {
    let state_ref = state.inner().clone();
    stop_gateway_inner(&state_ref).await;
    Ok(gateway_status_from_state(&state_ref).await)
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
    app_state.last_gateway_error = last_error;
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

    let state = Arc::new(Mutex::new(AppState {
        runtime,
        gateway_task: None,
        last_gateway_error: None,
        db_pool: None,
        agent_store: None,
        account_store: None,
        provider_store: None,
        config_manager: Some(config_manager),
        keyring_service: None,
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
