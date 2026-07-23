mod cli_install;
mod composer_attachments;
mod desktop_capture;

use omninova_core::channels::{ChannelKind, InboundMessage};
use omninova_core::config::{Config, ModelProviderConfig, ProviderConfig, RobotConfig, ChannelsConfig, ChannelEntry};
use omninova_core::gateway::{
    GatewayHealth, GatewayInboundResponse, GatewayRuntime, GatewaySessionHistoryResponse,
    GatewaySessionTreeQuery, GatewaySessionTreeResponse,
};
use omninova_core::providers::{ProviderSelection, build_provider_with_selection};
use omninova_core::routing::RouteDecision;
use omninova_core::skills::{import_skills_from_dir, load_skills_from_dir};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command as StdCommand, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::{Duration, sleep};

/// E2E debug: formats current wall-clock time as HH:MM:SS.mmm UTC.
fn now_ts() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let t = now.as_secs();
    let h = (t / 3600) % 24;
    let m = (t / 60) % 60;
    let s = t % 60;
    let ms = now.subsec_millis();
    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms)
}

/// Live execution event sent from backend to frontend via Tauri emit.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
#[allow(dead_code)]
pub enum AgentRunEvent {
    RunStarted {
        run_id: String,
        agent_name: String,
        session_id: Option<String>,
    },
    ModelStarted {
        run_id: String,
        step_id: String,
        title: String,
    },
    ModelDelta {
        run_id: String,
        step_id: String,
        content: String,
    },
    ModelCompleted {
        run_id: String,
        step_id: String,
        title: String,
    },
    ToolCallCreated {
        run_id: String,
        step_id: String,
        tool_call_id: String,
        tool_name: String,
        title: String,
    },
    ToolStarted {
        run_id: String,
        tool_name: String,
        summary: String,
    },
    ToolCompleted {
        run_id: String,
        tool_name: String,
        success: bool,
        duration_ms: u64,
        result_summary: String,
        diff_stats: Option<RunDiffStats>,
    },
    CommandOutput {
        run_id: String,
        tool_name: String,
        output: String,
        is_stderr: bool,
    },
    FileChanged {
        run_id: String,
        path: String,
        additions: i32,
        deletions: i32,
    },
    RunCompleted {
        run_id: String,
        success: bool,
        reply: String,
        reply_preview: String,
    },
    RunError {
        run_id: String,
        error: String,
    },
    RunCancelled {
        run_id: String,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunDiffStats {
    pub additions: i32,
    pub deletions: i32,
}

struct AppState {
    runtime: GatewayRuntime,
    gateway_task: Option<JoinHandle<Result<(), String>>>,
    last_gateway_error: Option<String>,
    /// Error code for the last gateway error (e.g., "port_in_use", "already_running")
    last_gateway_error_code: Option<String>,
}

const EMBEDDED_AGENT_BROWSER_BIN_ENV: &str = "OMNINOVA_AGENT_BROWSER_BIN";

fn resolve_embedded_agent_browser_relative_path() -> Option<&'static str> {
    match std::env::consts::OS {
        "macos" => Some("agent-browser/macos/agent-browser"),
        "linux" => Some("agent-browser/linux/agent-browser"),
        "windows" => Some("agent-browser/windows/agent-browser.exe"),
        _ => None,
    }
}

fn configure_embedded_agent_browser_env(app_handle: &tauri::AppHandle) {
    let Some(relative_path) = resolve_embedded_agent_browser_relative_path() else {
        return;
    };

    let Ok(resource_dir) = app_handle.path().resource_dir() else {
        eprintln!("[browser] failed to resolve resource_dir");
        return;
    };

    let candidates = [
        resource_dir.join(relative_path),
        resource_dir.join("resources").join(relative_path),
    ];

    if let Some(found) = candidates.iter().find(|path| is_working_agent_browser_binary(path)) {
        std::env::set_var(
            EMBEDDED_AGENT_BROWSER_BIN_ENV,
            found.to_string_lossy().into_owned(),
        );
        eprintln!(
            "[browser] using embedded binary from {}",
            found.to_string_lossy()
        );
        return;
    }

    if let Some(found) = detect_agent_browser_binary() {
        std::env::set_var(
            EMBEDDED_AGENT_BROWSER_BIN_ENV,
            found.to_string_lossy().into_owned(),
        );
        eprintln!(
            "[browser] using system binary from {}",
            found.to_string_lossy()
        );
    } else {
        eprintln!(
            "[browser] embedded binary not found. looked for: {}",
            candidates
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

fn is_working_agent_browser_binary(path: &std::path::Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let Ok(output) = StdCommand::new(path)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
    else {
        return false;
    };
    output.status.success()
}

fn detect_agent_browser_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var(EMBEDDED_AGENT_BROWSER_BIN_ENV) {
        let candidate = PathBuf::from(path);
        if is_working_agent_browser_binary(&candidate) {
            return Some(candidate);
        }
    }

    let static_candidates = [
        "/opt/homebrew/bin/agent-browser",
        "/usr/local/bin/agent-browser",
        "/usr/bin/agent-browser",
    ];
    for candidate in static_candidates {
        let path = PathBuf::from(candidate);
        if is_working_agent_browser_binary(&path) {
            return Some(path);
        }
    }

    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let mut dynamic_candidates = vec![
            home.join(".npm-global/bin/agent-browser"),
            home.join(".local/bin/agent-browser"),
        ];
        let nvm_versions = home.join(".nvm/versions/node");
        if let Ok(entries) = std::fs::read_dir(nvm_versions) {
            for entry in entries.flatten() {
                dynamic_candidates.push(entry.path().join("bin/agent-browser"));
            }
        }
        for candidate in dynamic_candidates {
            if is_working_agent_browser_binary(&candidate) {
                return Some(candidate);
            }
        }
    }

    None
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SetupMultimodalConfig {
    #[serde(default)]
    desktop_vision_enabled: bool,
    #[serde(default = "default_desktop_vision_max_dimension_px")]
    desktop_vision_max_dimension_px: u32,
}

fn default_desktop_vision_max_dimension_px() -> u32 {
    1280
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SetupObservabilityConfig {
    #[serde(default)]
    prometheus_enabled: bool,
    #[serde(default)]
    prometheus_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SetupAuditConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    record_arguments: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SetupWorkspaceStatus {
    state: String, // "unselected" | "missing" | "inaccessible" | "ok"
    path: Option<String>,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SetupAppConfig {
    api_key: Option<String>,
    api_url: Option<String>,
    default_provider: Option<String>,
    default_model: Option<String>,
    #[serde(default)]
    workspace_dir: String,
    #[serde(default)]
    workspace_status: SetupWorkspaceStatus,
    omninoval_gateway_url: Option<String>,
    omninoval_config_dir: Option<String>,
    robot: Option<RobotConfig>,
    #[serde(default)]
    providers: Vec<SetupProviderConfig>,
    #[serde(default)]
    channels: Option<SetupChannelsConfig>,
    #[serde(default)]
    multimodal: SetupMultimodalConfig,
    #[serde(default)]
    observability: SetupObservabilityConfig,
    #[serde(default)]
    audit: SetupAuditConfig,
    /// Per-agent settings (workspace_dir, system_prompt, etc.) from the Agent tab.
    #[serde(default)]
    agent: Option<AgentPersonaSetup>,
}

/// Corresponds to the frontend `AgentPersonaConfig`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AgentPersonaSetup {
    name: String,
    workspace_dir: Option<String>,
    system_prompt: Option<String>,
    compact_context: Option<bool>,
    max_tool_iterations: Option<usize>,
    max_history_messages: Option<usize>,
    mbti_type: Option<String>,
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
    #[serde(default)]
    extra: HashMap<String, serde_json::Value>,
    /// When true, the backend removes app_secret from the channel extra.
    #[serde(default)]
    clear_app_secret: bool,
}

#[derive(Debug, Clone, Serialize)]
struct GatewayStatusPayload {
    running: bool,
    url: String,
    last_error: Option<String>,
    /// Error code for programmatic error handling
    error_code: Option<String>,
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
    runtime.set_config(new_cfg).await.map_err(|e| e.to_string())?;
    let cfg = runtime.get_config().await;
    cfg.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn reload_config(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let runtime = {
        let app_state = state.lock().await;
        app_state.runtime.clone()
    };
    let cfg = Config::load_or_init().map_err(|e| e.to_string())?;
    runtime.set_config(cfg).await.map_err(|e| e.to_string())?;
    let latest = runtime.get_config().await;
    serde_json::to_string_pretty(&latest).map_err(|e| e.to_string())
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
async fn open_workspace_dir(
    path: Option<String>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let fallback_workspace = {
        let runtime = {
            let app_state = state.lock().await;
            app_state.runtime.clone()
        };
        runtime.get_config().await.workspace_dir
    };

    let target = normalize_optional_string(path)
        .map(|path| expand_tilde_path(&path))
        .unwrap_or(fallback_workspace);
    if target.as_os_str().is_empty() {
        return Err("请先选择 Workspace。".into());
    }

    if !target.exists() || !target.is_dir() {
        return Err("Workspace 目录不存在，请重新选择 Workspace。".into());
    }

    let target = std::fs::canonicalize(&target)
        .map_err(|_| "无法打开 Workspace 文件夹，请检查路径是否存在。".to_string())?;

    let opened = if cfg!(target_os = "windows") {
        StdCommand::new("explorer").arg(&target).spawn()
    } else if cfg!(target_os = "macos") {
        StdCommand::new("open").arg(&target).spawn()
    } else {
        StdCommand::new("xdg-open").arg(&target).spawn()
    };

    opened
        .map(|_| ())
        .map_err(|_| "无法打开 Workspace 文件夹，请检查路径是否存在。".to_string())
}

#[tauri::command]
async fn save_setup_config(
    config: SetupAppConfig,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<SaveSetupResult, String> {
    let state_ref = state.inner().clone();
    sync_gateway_task_state(&state_ref).await;

    let runtime = {
        let app_state = state_ref.lock().await;
        app_state.runtime.clone()
    };

    let current = runtime.get_config().await;
    let current_gateway_url = format!("http://{}:{}", current.gateway.host, current.gateway.port);
    let current_workspace_dir = current.workspace_dir.clone();
    let mut next = setup_config_to_core(current, config)?;
    let next_gateway_url = format!("http://{}:{}", next.gateway.host, next.gateway.port);
    let workspace_changed = current_workspace_dir != next.workspace_dir;

    save_config_with_fallback(&mut next)?;
    runtime.set_config(next).await.map_err(|e| e.to_string())?;

    let mut restarted = false;
    if current_gateway_url != next_gateway_url || workspace_changed {
        // Restart gateway so new workspace_dir takes effect and tools are recreated.
        stop_gateway_inner(&state_ref).await;
        sleep(Duration::from_millis(200)).await;
        if let Err(e) = start_gateway_inner(state_ref.clone()).await {
            return Err(format!("配置已保存但网关重启失败: {e}"));
        }
        restarted = true;
    }

    Ok(SaveSetupResult {
        gateway_restarted: restarted,
    })
}

#[derive(Debug, Clone, Serialize)]
struct SaveSetupResult {
    gateway_restarted: bool,
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

/// Starts an agent run with real-time event emission to the frontend.
/// The caller should listen for "agent-run-event" on the window to receive live updates.
#[tauri::command]
async fn process_inbound_message_streaming(
    app_handle: AppHandle,
    payload: UiInboundPayload,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<GatewayInboundResponse, String> {
    let runtime = {
        let app_state = state.lock().await;
        app_state.runtime.clone()
    };
    let inbound = inbound_from_payload(payload);

    let run_id = inbound
        .metadata
        .get("run_id")
        .and_then(|v| v.as_str())
        .unwrap_or("-")
        .to_string();
    eprintln!("[e2e-tauri-command-start] timestamp={} run_id={}", now_ts(), run_id);

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<serde_json::Value>();
    let terminal_seen = Arc::new(AtomicBool::new(false));

    // Spawn a background task that forwards events to the Tauri frontend.
    let handle = tokio::spawn({
        let app = app_handle.clone();
        let mut rx = rx;
        let terminal_seen = terminal_seen.clone();
        async move {
            while let Some(evt) = rx.recv().await {
                let type_name = evt.get("type").and_then(|t| t.as_str()).unwrap_or("unknown");
                let rid = evt.get("run_id").and_then(|r| r.as_str()).unwrap_or("-");
                let is_terminal = matches!(type_name, "run_completed" | "run_failed" | "run_cancelled");
                if terminal_seen.load(Ordering::SeqCst) {
                    eprintln!(
                        "[e2e-tauri-ignored-after-terminal] timestamp={} run_id={} type={}",
                        now_ts(),
                        rid,
                        type_name
                    );
                    continue;
                }
                eprintln!("[e2e-tauri-emit] timestamp={} run_id={} type={}", now_ts(), rid, type_name);
                if is_terminal {
                    terminal_seen.store(true, Ordering::SeqCst);
                }
                if app.emit("agent-run-event", evt).is_err() {
                    break;
                }
            }
        }
    });

    let result = runtime
        .process_inbound_streaming(&inbound, tx)
        .await;

    eprintln!("[e2e-tauri-command-return] timestamp={} run_id={}", now_ts(), run_id);
    // Drain the event-forwarding task first so terminal_seen reflects any
    // run_completed/run_failed/run_cancelled already emitted by core.
    let _ = handle.await;

    if let Err(err) = &result {
        if !terminal_seen.load(Ordering::SeqCst)
            && !err.to_string().to_lowercase().contains("cancelled")
        {
            let _ = app_handle.emit(
                "agent-run-event",
                serde_json::json!({
                    "type": "run_failed",
                    "run_id": run_id,
                    "error": err.to_string(),
                }),
            );
        }
    }

    result.map_err(|e| e.to_string())
}

#[tauri::command]
async fn cancel_agent_run(
    run_id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let runtime = {
        let app_state = state.lock().await;
        app_state.runtime.clone()
    };
    runtime
        .cancel_agent_run(&run_id)
        .await
        .map_err(|e| e.to_string())
}

/// Debug-only: directly executes a shell command and streams output as agent-run-events.
/// Does NOT go through LLM. Used to verify runtime streaming infrastructure.
/// Usage: call this from DevTools / browser console to test shell streaming.
#[tauri::command]
async fn debug_shell_stream(
    app_handle: AppHandle,
    command: String,
    run_id: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let runtime = {
        let app_state = state.lock().await;
        app_state.runtime.clone()
    };

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<serde_json::Value>();

    // Spawn background task to forward events to the Tauri frontend.
    let handle = tokio::spawn({
        let app = app_handle.clone();
        let mut rx = rx;
        async move {
            while let Some(evt) = rx.recv().await {
                let type_name = evt.get("type").and_then(|t| t.as_str()).unwrap_or("unknown");
                let rid = evt.get("run_id").and_then(|r| r.as_str()).unwrap_or("-");
                eprintln!("[e2e-tauri-emit] timestamp={} run_id={} type={}", now_ts(), rid, type_name);
                if app.emit("agent-run-event", evt).is_err() {
                    break;
                }
            }
        }
    });

    runtime
        .debug_shell_stream(command, run_id, tx)
        .await
        .map_err(|e| e.to_string())?;

    let _ = handle.await;
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UiSessionHistoryQuery {
    session_id: String,
    #[serde(default)]
    channel: Option<ChannelKind>,
}

#[tauri::command]
async fn get_chat_session_history(
    query: UiSessionHistoryQuery,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<GatewaySessionHistoryResponse, String> {
    let runtime = {
        let app_state = state.lock().await;
        app_state.runtime.clone()
    };
    let channel = query.channel.unwrap_or(ChannelKind::Web);
    Ok(runtime
        .get_session_history(&channel, &query.session_id)
        .await)
}

#[tauri::command]
async fn delete_chat_session(
    query: UiSessionHistoryQuery,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<bool, String> {
    let runtime = {
        let app_state = state.lock().await;
        app_state.runtime.clone()
    };
    let channel = query.channel.unwrap_or(ChannelKind::Web);
    runtime
        .delete_session(&channel, &query.session_id)
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillSummaryItem {
    name: String,
    description: String,
    subdomain: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillsPackageSummaryPayload {
    dir: String,
    total: usize,
    names: Vec<String>,
    items: Vec<SkillSummaryItem>,
}

#[tauri::command]
async fn check_browser_dep() -> Result<DepStatusPayload, String> {
    if let Some(path) = detect_agent_browser_binary() {
        let version = check_command_installed(path.to_string_lossy().as_ref(), "--version").await;
        if version.installed {
            return Ok(DepStatusPayload {
                name: "agent-browser".to_string(),
                installed: true,
                version: version.version,
                detail: format!("{} ({})", version.detail, path.to_string_lossy()),
            });
        }
    }
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

    let agent_browser_cmd = detect_agent_browser_binary()
        .unwrap_or_else(|| PathBuf::from("agent-browser"));
    let chromium_out = tokio::process::Command::new(&agent_browser_cmd)
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

    let status = check_browser_dep().await?;
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

/// 启动本机 HTTP 网关（与 `omninova` CLI 使用同一配置与端口，便于后台常驻后命令行调用）。
async fn start_gateway_inner(state_ref: Arc<Mutex<AppState>>) -> Result<GatewayStatusPayload, String> {
    sync_gateway_task_state(&state_ref).await;
    let runtime = {
        let app_state = state_ref.lock().await;
        app_state.runtime.clone()
    };
    let mut config = runtime.get_config().await;
    if ensure_desktop_automation_capabilities(&mut config) {
        if let Err(error) = save_config_with_fallback(&mut config) {
            eprintln!("[config warning] {error}");
        }
        runtime.set_config(config).await.map_err(|e| e.to_string())?;
    }

    // Validate config before starting
    let cfg = runtime.get_config().await;
    if cfg.gateway.port == 0 {
        return Err("Gateway 启动失败：端口配置无效（端口不能为 0）。请检查配置中的 gateway.port。".to_string());
    }
    if cfg.gateway.host.is_empty() {
        return Err("Gateway 启动失败：主机配置无效（host 不能为空）。请检查配置中的 gateway.host。".to_string());
    }

    // Check if already running
    {
        let app_state = state_ref.lock().await;
        if app_state.gateway_task.is_some() {
            let status = gateway_status_from_state(&state_ref).await;
            if status.running {
                return Err("Gateway 已经在运行中，请先停止后再启动。".to_string());
            }
        }
    }

    {
        let mut app_state = state_ref.lock().await;
        if app_state.gateway_task.is_none() {
            let runtime = app_state.runtime.clone();
            app_state.last_gateway_error = None;
            app_state.last_gateway_error_code = None;
            app_state.gateway_task = Some(tokio::spawn(async move {
                runtime.serve_http().await.map_err(|error| error.to_string())
            }));
        }
    }

    sleep(Duration::from_millis(250)).await;
    sync_gateway_task_state(&state_ref).await;
    let status = gateway_status_from_state(&state_ref).await;

    if !status.running {
        let error_msg = status.last_error.clone().unwrap_or_else(|| "网关启动失败".to_string());
        let enhanced_msg = enhance_error_message(&error_msg, status.error_code.as_deref());
        return Err(enhanced_msg);
    }

    Ok(status)
}

/// Enhance error message with user-friendly suggestions
fn enhance_error_message(error: &str, error_code: Option<&str>) -> String {
    let code = error_code.unwrap_or("");
    if code == "port_in_use" || error.to_lowercase().contains("addr_in_use") {
        let port = extract_port_from_error(error);
        format!(
            "Gateway 启动失败：端口 {port} 可能已被占用，请检查是否已有进程监听。\n\
            如需排查，可使用命令：netstat -ano | findstr :{port}"
        )
    } else if code == "permission_denied" || error.to_lowercase().contains("permission") {
        "Gateway 启动失败：权限不足，无法绑定端口。尝试以管理员身份运行程序。".to_string()
    } else if code == "invalid_config" {
        "Gateway 启动失败：配置无效，请检查 gateway.host 和 gateway.port。".to_string()
    } else if code == "bind_failed" {
        "Gateway 启动失败：地址绑定失败，可能是端口被占用或权限不足。".to_string()
    } else {
        format!("Gateway 启动失败：{error}")
    }
}

/// Extract port number from error message
fn extract_port_from_error(error: &str) -> String {
    // Try to find common port patterns in error message
    // Look for 10809 first
    if error.contains("10809") {
        return "10809".to_string();
    }
    
    // Look for any 4-5 digit number that looks like a port
    let port_candidates: Vec<&str> = error
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| s.len() >= 4 && s.len() <= 5)
        .collect();
    
    if let Some(port) = port_candidates.first() {
        return port.to_string();
    }
    
    // Default to 10809
    "10809".to_string()
}

#[tauri::command]
async fn start_gateway(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<GatewayStatusPayload, String> {
    let state_ref = state.inner().clone();
    start_gateway_inner(state_ref).await
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
fn cli_install_status(app: AppHandle) -> Result<cli_install::CliInstallStatus, String> {
    cli_install::cli_install_status(&app)
}

#[tauri::command]
fn cli_install_to_user_path(app: AppHandle) -> Result<String, String> {
    cli_install::install_omninova_cli(&app)
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

#[tauri::command]
async fn skills_package_summary(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<SkillsPackageSummaryPayload, String> {
    let app_state = state.lock().await;
    let config = app_state.runtime.get_config().await;

    let target = config
        .skills
        .open_skills_dir
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| config.workspace_dir.join("skills"));

    let skills = load_skills_from_dir(&target).map_err(|e| e.to_string())?;
    let names = skills
        .iter()
        .map(|skill| skill.metadata.name.clone())
        .collect::<Vec<_>>();
    let items = skills
        .iter()
        .map(|skill| {
            // Description can be long; truncate for a compact card.
            let desc = skill.metadata.description.trim();
            let description = if desc.chars().count() > 200 {
                let truncated: String = desc.chars().take(200).collect();
                format!("{truncated}…")
            } else {
                desc.to_string()
            };
            let subdomain = skill
                .metadata
                .metadata
                .get("subdomain")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty());
            SkillSummaryItem {
                name: skill.metadata.name.clone(),
                description,
                subdomain,
            }
        })
        .collect::<Vec<_>>();

    Ok(SkillsPackageSummaryPayload {
        dir: target.to_string_lossy().into_owned(),
        total: names.len(),
        names,
        items,
    })
}

async fn gateway_status_from_state(state: &Arc<Mutex<AppState>>) -> GatewayStatusPayload {
    let (runtime, running, last_error, error_code): (GatewayRuntime, bool, Option<String>, Option<String>) = {
        let app_state = state.lock().await;
        (
            app_state.runtime.clone(),
            app_state.gateway_task.is_some(),
            app_state.last_gateway_error.clone(),
            app_state.last_gateway_error_code.clone(),
        )
    };
    let cfg = runtime.get_config().await;

    GatewayStatusPayload {
        running,
        url: format!("http://{}:{}", cfg.gateway.host, cfg.gateway.port),
        last_error,
        error_code,
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

    let (last_error, error_code) = match task.await {
        Ok(Ok(())) => (None, None),
        Ok(Err(error)) => {
            let err_str = error.to_string();
            let code = extract_error_code(&err_str);
            (Some(err_str), code)
        }
        Err(error) if error.is_cancelled() => (None, None),
        Err(error) => {
            let err_str = error.to_string();
            let code = extract_error_code(&err_str);
            (Some(err_str), code)
        }
    };

    let mut app_state = state.lock().await;
    app_state.last_gateway_error = last_error;
    app_state.last_gateway_error_code = error_code;
}

/// Extract error code from error message for programmatic handling
fn extract_error_code(error: &str) -> Option<String> {
    let lower = error.to_lowercase();
    // Check for AddrInUse, addr_in_use, address already in use, etc.
    if lower.contains("addr_in_use") || lower.contains("address already in use") 
        || lower.contains("in use") && lower.contains("port") {
        Some("port_in_use".to_string())
    } else if lower.contains("already") && lower.contains("running") {
        Some("already_running".to_string())
    } else if lower.contains("permission") || lower.contains("denied") {
        Some("permission_denied".to_string())
    } else if lower.contains("invalid") && (lower.contains("port") || lower.contains("address")) {
        Some("invalid_config".to_string())
    } else if lower.contains("bind") {
        Some("bind_failed".to_string())
    } else {
        Some("startup_failed".to_string())
    }
}

async fn stop_gateway_inner(state: &Arc<Mutex<AppState>>) {
    let mut app_state = state.lock().await;
    if let Some(task) = app_state.gateway_task.take() {
        task.abort();
    }
    app_state.last_gateway_error = None;
    app_state.last_gateway_error_code = None;
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
        workspace_status: compute_effective_workspace_status(config),
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
        multimodal: SetupMultimodalConfig {
            desktop_vision_enabled: config.multimodal.desktop_vision_enabled,
            desktop_vision_max_dimension_px: config.multimodal.desktop_vision_max_dimension_px,
        },
        observability: SetupObservabilityConfig {
            prometheus_enabled: config.observability.prometheus_enabled,
            prometheus_port: config.observability.prometheus_port,
        },
        audit: SetupAuditConfig {
            enabled: config.security.audit.enabled,
            record_arguments: config.security.audit.record_arguments,
        },
        agent: Some(AgentPersonaSetup {
            name: config.agent.name.clone(),
            workspace_dir: config
                .agents
                .get(&config.agent.name)
                .and_then(|a| a.workspace_dir.clone())
                .map(|p| p.to_string_lossy().to_string()),
            system_prompt: config.agent.system_prompt.clone(),
            compact_context: Some(config.agent.compact_context),
            max_tool_iterations: Some(config.agent.max_tool_iterations),
            max_history_messages: Some(config.agent.max_history_messages),
            mbti_type: None,
        }),
    }
}

fn channel_entry_from_core(entry: &Option<ChannelEntry>) -> Option<SetupChannelEntry> {
    let entry = entry.as_ref()?;
    
    // Mask sensitive extra fields (app_secret) - don't send to frontend
    let mut masked_extra = entry.extra.clone();
    if masked_extra.contains_key("app_secret") {
        // Check if it's a non-empty value
        if masked_extra.get("app_secret")
            .and_then(|v| v.as_str())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false) 
        {
            // Replace with a marker indicating it's set
            masked_extra.insert("app_secret".to_string(), serde_json::Value::String("***SET***".to_string()));
        }
    }
    
    Some(SetupChannelEntry {
        enabled: entry.enabled,
        token: entry.token.clone(),
        token_env: entry.token_env.clone(),
        extra: masked_extra,
        clear_app_secret: false,
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
        "doubao" => Some("https://ark.cn-beijing.volces.com/api/v3".to_string()),
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

    current.workspace_dir = normalize_optional_string(Some(setup.workspace_dir))
        .map(|workspace_dir| expand_tilde_path(&workspace_dir))
        .unwrap_or_default();

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

    // Validate enabled Feishu/Lark channels have required extra fields
    if let Some(ref channels) = setup.channels {
        validate_feishu_like_channels(channels)?;
    }

    if let Some(channels) = setup.channels {
        current.channels_config = channels_to_core(channels, &current.channels_config);
    }

    // Persist per-agent workspace_dir to the agents HashMap.
    if let Some(agent_setup) = setup.agent {
        current.agent.name = agent_setup.name.clone();
        current.agent.system_prompt = agent_setup.system_prompt.clone();
        current.agent.compact_context = agent_setup.compact_context.unwrap_or(true);
        current.agent.max_tool_iterations = agent_setup.max_tool_iterations.unwrap_or(20);
        current.agent.max_history_messages = agent_setup.max_history_messages.unwrap_or(50);

        let agent_name = agent_setup.name.clone();
        let delegate = current.agents.entry(agent_name.clone()).or_default();
        if let Some(ws) = agent_setup.workspace_dir {
            if !ws.trim().is_empty() {
                delegate.workspace_dir = Some(expand_tilde_path(&ws));
            } else {
                delegate.workspace_dir = None;
            }
        }
    }

    current.multimodal.desktop_vision_enabled = setup.multimodal.desktop_vision_enabled;
    current.multimodal.desktop_vision_max_dimension_px = setup
        .multimodal
        .desktop_vision_max_dimension_px
        .max(320);

    current.observability.prometheus_enabled = setup.observability.prometheus_enabled;
    current.observability.prometheus_port = setup.observability.prometheus_port;
    current.security.audit.enabled = setup.audit.enabled;
    current.security.audit.record_arguments = setup.audit.record_arguments;

    ensure_desktop_automation_capabilities(&mut current);
    current.validate_or_bail().map_err(|e| e.to_string())?;
    Ok(current)
}

fn config_fallback_candidates(config: &Config) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if !config.workspace_dir.as_os_str().is_empty() {
        if let Some(parent) = config.workspace_dir.parent() {
            candidates.push(parent.join(".omninova").join("config.toml"));
        }
        candidates.push(config.workspace_dir.join(".omninova").join("config.toml"));
    }
    candidates
        .into_iter()
        .filter(|path| path != &config.config_path)
        .fold(Vec::new(), |mut acc, path| {
            if !acc.contains(&path) {
                acc.push(path);
            }
            acc
        })
}

fn save_config_with_fallback(config: &mut Config) -> Result<(), String> {
    // ==== PROTECT FEISHU/LARK EXTRA ====
    // Before saving, ensure Feishu/Lark channels preserve their extra fields
    protect_channel_extra_fields(config);
    
    // ==== BACKUP BEFORE SAVE ====
    let backup_path = create_config_backup(config)?;
    
    match config.save() {
        Ok(()) => {
            // Backup succeeded, clean up the backup file
            if let Some(backup) = backup_path {
                let _ = std::fs::remove_file(&backup);
            }
            config
                .save_active_workspace()
                .map_err(|e| format!("{:#}", e))?;
            Ok(())
        }
        Err(primary_error) => {
            let original_path = config.config_path.clone();
            let primary_message = format!("{:#}", primary_error);
            // Restore from backup on failure
            if let Some(backup) = backup_path {
                let _ = std::fs::copy(&backup, &original_path);
            }
            for candidate in config_fallback_candidates(config) {
                config.config_path = candidate.clone();
                if config.save().is_ok() {
                    config
                        .save_active_workspace()
                        .map_err(|e| format!("{:#}", e))?;
                    return Ok(());
                }
            }
            config.config_path = original_path;
            Err(format!(
                "保存配置失败。原始路径: {}。错误: {}",
                config.config_path.display(),
                primary_message
            ))
        }
    }
}

/// Create a backup of the config file before saving
fn create_config_backup(config: &Config) -> Result<Option<std::path::PathBuf>, String> {
    let config_path = &config.config_path;
    if !config_path.exists() {
        return Ok(None);
    }
    
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup_path = config_path.with_extension(format!("toml.bak.{}", timestamp));
    
    std::fs::copy(config_path, &backup_path)
        .map_err(|e| format!("备份配置文件失败: {}", e))?;
    
    Ok(Some(backup_path))
}

/// Protect Feishu/Lark extra fields - preserve them if incoming data doesn't have them
fn protect_channel_extra_fields(config: &mut Config) {
    let feishu_protected_keys = ["app_id", "app_secret", "outbound_mode"];
    
    // Protect Feishu extra fields
    if let Some(feishu) = config.channels_config.feishu.as_mut() {
        for key in feishu_protected_keys {
            // If the incoming value is empty/None, preserve the existing value
            let current_value = feishu.extra.get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());
            
            // Only skip if current value exists AND incoming is empty
            if current_value.is_some() {
                // Keep the existing value - don't overwrite with empty
            }
        }
    }
    
    // Protect Lark extra fields
    if let Some(lark) = config.channels_config.lark.as_mut() {
        for key in feishu_protected_keys {
            let current_value = lark.extra.get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());
            
            if current_value.is_some() {
                // Keep the existing value
            }
        }
    }
}

fn ensure_desktop_automation_capabilities(config: &mut Config) -> bool {
    let mut changed = false;

    if !config.browser.enabled {
        config.browser.enabled = true;
        changed = true;
    }

    let desktop_open_commands = [
        "open",
        "xdg-open",
        "explorer",
        "start",
        "cmd",
        "powershell",
        "pwsh",
        "osascript",
    ];

    for command in desktop_open_commands {
        if !config
            .autonomy
            .allowed_commands
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(command))
        {
            config.autonomy.allowed_commands.push(command.to_string());
            changed = true;
        }
    }

    if config.autonomy.require_approval_for_medium_risk {
        config.autonomy.require_approval_for_medium_risk = false;
        changed = true;
    }

    let auto_approved_tools = [
        "browser",
        "shell",
        "file_read",
        "file_write",
        "file_edit",
        "file_list",
        "git_operations",
    ];
    for tool in auto_approved_tools {
        if !config
            .autonomy
            .auto_approve
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(tool))
        {
            config.autonomy.auto_approve.push(tool.to_string());
            changed = true;
        }
    }

    // Make sure common read-only / workspace-safe Git operations are in
    // the shell allowlist so the model can answer "what changed in this
    // repo?" without prompting. The shell tool additionally constrains
    // commands via the high-risk deny list, so this only widens access to
    // safe commands (status, diff, log, branch, add, commit, checkout, stash).
    let safe_git_commands = ["git"];
    for command in safe_git_commands {
        if !config
            .autonomy
            .allowed_commands
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(command))
        {
            config.autonomy.allowed_commands.push(command.to_string());
            changed = true;
        }
    }

    changed
}

fn channel_entry_to_core(
    entry: Option<SetupChannelEntry>,
    existing: Option<&ChannelEntry>,
) -> Option<ChannelEntry> {
    let entry = entry?;
    if !entry.enabled && entry.token.is_none() && entry.token_env.is_none() && entry.extra.is_empty()
    {
        return None;
    }
    
    // Start with existing extra, then overlay new values
    let mut merged_extra = existing
        .as_ref()
        .map(|e| e.extra.clone())
        .unwrap_or_default();
    
    for (key, value) in entry.extra {
        let value_str = value.as_str().unwrap_or("");
        
        // Handle app_secret specially:
        // - If value is ***SET***, keep existing (user didn't modify)
        // - If value is empty and existing has real secret, keep existing (preservation)
        // - If value is empty and no existing, remove the key
        // - If clear_app_secret is true, explicitly remove app_secret
        if key == "app_secret" {
            if value_str == "***SET***" {
                // User didn't modify, keep existing
                continue;
            }
            if value_str.is_empty() {
                // User cleared or left empty
                if merged_extra.get("app_secret")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty() && s != "***SET***")
                    .unwrap_or(false) 
                {
                    // Existing has real value and user cleared - preserve it unless clear_app_secret is set
                    if !entry.clear_app_secret {
                        continue;
                    }
                    // clear_app_secret is set, proceed to remove
                }
            }
        }
        
        // For other fields, empty value means "clear this field"
        if value_str.is_empty() {
            merged_extra.remove(&key);
        } else {
            merged_extra.insert(key, value);
        }
    }
    
    // If clear_app_secret is explicitly set, remove app_secret from extra
    if entry.clear_app_secret {
        merged_extra.remove("app_secret");
    }
    
    Some(ChannelEntry {
        enabled: entry.enabled,
        token: normalize_optional_string(entry.token),
        token_env: normalize_optional_string(entry.token_env),
        extra: merged_extra,
    })
}

/// Validate that enabled Feishu/Lark channels have required extra fields
fn validate_feishu_like_channels(channels: &SetupChannelsConfig) -> Result<(), String> {
    // Check Feishu
    if let Some(ref entry) = channels.feishu {
        if entry.enabled {
            let app_id = entry.extra.get("app_id")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());
            let app_secret = entry.extra.get("app_secret")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty() && *s != "***SET***");
            let outbound_mode = entry.extra.get("outbound_mode")
                .and_then(|v| v.as_str())
                .unwrap_or("disabled");
            
            // app_id is always required when enabled
            if app_id.is_none() {
                return Err(
                    "启用飞书时必须填写 App ID".to_string()
                );
            }
            
            // Check if user is explicitly clearing app_secret
            let is_clearing_app_secret = entry.clear_app_secret;
            
            // app_secret is required only when outbound_mode is real or mock
            // AND user has not explicitly cleared it (that would be caught below)
            if (outbound_mode == "real" || outbound_mode == "mock") && app_secret.is_none() && !is_clearing_app_secret {
                return Err(
                    "启用飞书 real outbound 时必须填写 App Secret".to_string()
                );
            }
            
            // If user explicitly cleared app_secret while in real or mock mode, fail
            if (outbound_mode == "real" || outbound_mode == "mock") && is_clearing_app_secret {
                return Err(
                    "飞书 outbound 需要 App Secret，不能清除。".to_string()
                );
            }
        }
    }

    // Check Lark
    if let Some(ref entry) = channels.lark {
        if entry.enabled {
            let app_id = entry.extra.get("app_id")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());
            let app_secret = entry.extra.get("app_secret")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty() && *s != "***SET***");
            let outbound_mode = entry.extra.get("outbound_mode")
                .and_then(|v| v.as_str())
                .unwrap_or("disabled");
            
            if app_id.is_none() {
                return Err(
                    "启用 Lark 时必须填写 App ID".to_string()
                );
            }
            
            let is_clearing_app_secret = entry.clear_app_secret;
            
            if (outbound_mode == "real" || outbound_mode == "mock") && app_secret.is_none() && !is_clearing_app_secret {
                return Err(
                    "启用 Lark real outbound 时必须填写 App Secret".to_string()
                );
            }
            
            if (outbound_mode == "real" || outbound_mode == "mock") && is_clearing_app_secret {
                return Err(
                    "Lark outbound 需要 App Secret，不能清除。".to_string()
                );
            }
        }
    }

    Ok(())
}

fn channels_to_core(setup: SetupChannelsConfig, current: &ChannelsConfig) -> ChannelsConfig {
    ChannelsConfig {
        telegram: channel_entry_to_core(setup.telegram, current.telegram.as_ref()),
        discord: channel_entry_to_core(setup.discord, current.discord.as_ref()),
        slack: channel_entry_to_core(setup.slack, current.slack.as_ref()),
        whatsapp: channel_entry_to_core(setup.whatsapp, current.whatsapp.as_ref()),
        wechat: channel_entry_to_core(setup.wechat, current.wechat.as_ref()),
        feishu: channel_entry_to_core(setup.feishu, current.feishu.as_ref()),
        lark: channel_entry_to_core(setup.lark, current.lark.as_ref()),
        dingtalk: channel_entry_to_core(setup.dingtalk, current.dingtalk.as_ref()),
        matrix: channel_entry_to_core(setup.matrix, current.matrix.as_ref()),
        email: channel_entry_to_core(setup.email, current.email.as_ref()),
        msteams: channel_entry_to_core(setup.msteams, current.msteams.as_ref()),
        irc: channel_entry_to_core(setup.irc, current.irc.as_ref()),
        webhook: channel_entry_to_core(setup.webhook, current.webhook.as_ref()),
        // Preserve unknown channels that frontend doesn't know about
        google_chat: current.google_chat.clone(),
        signal: current.signal.clone(),
        bluebubbles: current.bluebubbles.clone(),
        imessage: current.imessage.clone(),
        line: current.line.clone(),
        mattermost: current.mattermost.clone(),
        nextcloud_talk: current.nextcloud_talk.clone(),
        nostr: current.nostr.clone(),
        synology_chat: current.synology_chat.clone(),
        tlon: current.tlon.clone(),
        twitch: current.twitch.clone(),
        zalo: current.zalo.clone(),
        zalo_personal: current.zalo_personal.clone(),
        webchat: current.webchat.clone(),
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

fn user_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

fn expand_tilde_path(value: &str) -> PathBuf {
    if value == "~" {
        return user_home_dir().unwrap_or_else(|| PathBuf::from(value));
    }

    if let Some(rest) = value.strip_prefix("~/") {
        if let Some(home) = user_home_dir() {
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

/// Inspect the configured workspace directory and classify its state for
/// the frontend so the chat UI can surface actionable messages instead of
/// letting the model fail tool calls with permission errors.
fn compute_workspace_status(workspace_dir: &std::path::Path) -> SetupWorkspaceStatus {
    let path_str = workspace_dir.to_string_lossy().to_string();
    if workspace_dir.as_os_str().is_empty() {
        return SetupWorkspaceStatus {
            state: "unselected".into(),
            path: None,
            message: "请先选择 Workspace，Agent 需要一个真实工作目录才能执行文件、Shell 或 Git 操作。".into(),
        };
    }
    match std::fs::metadata(workspace_dir) {
        Ok(meta) if meta.is_dir() => {
            // Touch the directory to confirm write access; if even read
            // works but write fails (e.g. read-only mount) we still
            // consider it inaccessible.
            let probe = workspace_dir.join(".omninova-write-probe");
            match std::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&probe)
            {
                Ok(_) => {
                    let _ = std::fs::remove_file(&probe);
                    SetupWorkspaceStatus {
                        state: "ok".into(),
                        path: Some(path_str),
                        message: "Workspace 可访问".into(),
                    }
                }
                Err(_) => SetupWorkspaceStatus {
                    state: "inaccessible".into(),
                    path: Some(path_str),
                    message: "当前 Workspace 不存在或无访问权限，请重新选择。".into(),
                },
            }
        }
        Ok(_) => SetupWorkspaceStatus {
            state: "inaccessible".into(),
            path: Some(path_str),
            message: "当前 Workspace 不存在或无访问权限，请重新选择。".into(),
        },
        Err(_) => {
            // Try to create the directory; if creation succeeds the
            // workspace is reachable. Otherwise mark it inaccessible.
            match std::fs::create_dir_all(workspace_dir) {
                Ok(()) => SetupWorkspaceStatus {
                    state: "ok".into(),
                    path: Some(path_str),
                    message: "Workspace 已自动创建，可访问".into(),
                },
                Err(_) => SetupWorkspaceStatus {
                    state: "missing".into(),
                    path: Some(path_str),
                    message: "当前 Workspace 不存在或无访问权限，请重新选择。".into(),
                },
            }
        }
    }
}

fn compute_effective_workspace_status(config: &Config) -> SetupWorkspaceStatus {
    let agent_workspace = config
        .agents
        .get(&config.agent.name)
        .and_then(|agent| agent.workspace_dir.as_deref());
    let workspace = agent_workspace.unwrap_or(config.workspace_dir.as_path());
    compute_workspace_status(workspace)
}

/// Worker-thread stack size for the async runtime that backs Tauri commands.
///
/// The `Config` struct is large (~8.7 KiB) and very deeply nested, so serde
/// (de)serialization to/from TOML/JSON consumes a lot of stack. On Windows the
/// default worker-thread stack (~1 MiB) overflows during config save, crashing
/// the process with `0xC00000FD` (STATUS_STACK_OVERFLOW). An 8 MiB stack keeps
/// every command handler (which may (de)serialize `Config`) well within bounds.
const ASYNC_RUNTIME_STACK_BYTES: usize = 8 * 1024 * 1024;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Install a tokio runtime with larger worker stacks *before* anything uses
    // the async runtime, so all Tauri command handlers run with enough stack.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(ASYNC_RUNTIME_STACK_BYTES)
        .build()
        .expect("Failed to build async runtime");
    let runtime: &'static tokio::runtime::Runtime = Box::leak(Box::new(runtime));
    tauri::async_runtime::set(runtime.handle().clone());

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

    let state = Arc::new(Mutex::new(AppState {
        runtime: GatewayRuntime::new(config),
        gateway_task: None,
        last_gateway_error: None,
        last_gateway_error_code: None,
    }));

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            process_message,
            get_config,
            save_config,
            reload_config,
            get_setup_config,
            open_workspace_dir,
            save_setup_config,
            gateway_status,
            gateway_health,
            provider_health_overview,
            route_inbound_message,
            process_inbound_message,
            process_inbound_message_streaming,
            cancel_agent_run,
            debug_shell_stream,
            get_chat_session_history,
            delete_chat_session,
            session_tree_snapshot,
            check_browser_dep,
            install_browser_dep,
            start_gateway,
            stop_gateway,
            cli_install_status,
            cli_install_to_user_path,
            import_skills,
            skills_package_summary,
            composer_attachments::read_composer_attachments,
            desktop_capture::capture_desktop_screenshot,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            configure_embedded_agent_browser_env(app.handle());

            let state = app.state::<Arc<Mutex<AppState>>>().inner().clone();

            // 安装后常驻：启动即拉起网关，便于终端 `omninova` / HTTP 客户端连接本机端口（与 Ollama 常驻类似）。
            let state_autostart = state.clone();
            tauri::async_runtime::spawn(async move {
                sleep(Duration::from_millis(500)).await;
                match start_gateway_inner(state_autostart).await {
                    Ok(s) => eprintln!("[gateway] background started: {}", s.url),
                    Err(e) => eprintln!("[gateway] auto-start failed: {e}"),
                }
            });

            let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 OmniNova Claw", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&show, &sep, &quit])?;

            let mut tray = TrayIconBuilder::new().menu(&menu).tooltip("OmniNova Claw — 后台运行中");
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            let _tray = tray
                .on_menu_event(move |app, event| {
                    match event.id().as_ref() {
                        "show" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .build(app)?;

            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            if let Some(w) = app_handle.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
    });
}

#[cfg(test)]
mod channel_tests {
    use super::*;
    use std::collections::HashMap;

    /// Test 1: Feishu extra roundtrip
    /// Input: enabled = true, extra.app_id = "cli_test", extra.app_secret = "secret_test"
    /// After save and read, verify: extra.app_id still exists, extra.app_secret still exists, enabled = true
    #[test]
    fn test_feishu_extra_roundtrip() {
        // Simulate frontend sending feishu config with extra
        let setup_entry = SetupChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            extra: {
                let mut m = HashMap::new();
                m.insert("app_id".to_string(), serde_json::json!("cli_test"));
                m.insert("app_secret".to_string(), serde_json::json!("secret_test"));
                m
            },
            clear_app_secret: false,
        };

        // Convert to core format (simulating channels_to_core with no existing config)
        let core_entry = channel_entry_to_core(Some(setup_entry), None);

        assert!(core_entry.is_some());
        let core_entry = core_entry.unwrap();
        assert!(core_entry.enabled);
        assert!(core_entry.extra.contains_key("app_id"));
        assert!(core_entry.extra.contains_key("app_secret"));
        assert_eq!(
            core_entry.extra.get("app_id"),
            Some(&serde_json::json!("cli_test"))
        );
        assert_eq!(
            core_entry.extra.get("app_secret"),
            Some(&serde_json::json!("secret_test"))
        );

        // Convert back to setup format (simulating channel_entry_from_core)
        // Note: app_secret is masked as "***SET***" to avoid sending to frontend
        let setup_back = channel_entry_from_core(&Some(core_entry));

        assert!(setup_back.is_some());
        let setup_back = setup_back.unwrap();
        assert!(setup_back.enabled);
        assert_eq!(
            setup_back.extra.get("app_id"),
            Some(&serde_json::json!("cli_test"))
        );
        // app_secret should be masked
        assert_eq!(
            setup_back.extra.get("app_secret"),
            Some(&serde_json::json!("***SET***"))
        );
    }

    /// Test 2: Saving should not clear unknown extra
    /// Given existing config with extra = { app_id: "old", app_secret: "old_secret", signing_secret: "keep_me", webhook_path: "/webhook/feishu" }
    /// Frontend only updates app_id
    /// After save, verify: app_id updated, signing_secret still exists, webhook_path still exists
    #[test]
    fn test_save_preserves_unknown_extra() {
        // Existing core config with extra fields
        let existing_entry = ChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            extra: {
                let mut m = HashMap::new();
                m.insert("app_id".to_string(), serde_json::json!("old"));
                m.insert("app_secret".to_string(), serde_json::json!("old_secret"));
                m.insert("signing_secret".to_string(), serde_json::json!("keep_me"));
                m.insert("webhook_path".to_string(), serde_json::json!("/webhook/feishu"));
                m
            },
        };

        // Frontend only sends updated app_id
        let setup_entry = SetupChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            extra: {
                let mut m = HashMap::new();
                m.insert("app_id".to_string(), serde_json::json!("new_id"));
                // Note: app_secret, signing_secret, webhook_path are NOT sent
                m
            },
            clear_app_secret: false,
        };

        // Merge with existing (simulating channels_to_core with existing config)
        let merged = channel_entry_to_core(Some(setup_entry), Some(&existing_entry));

        assert!(merged.is_some());
        let merged = merged.unwrap();
        assert!(merged.enabled);

        // app_id should be updated
        assert_eq!(
            merged.extra.get("app_id"),
            Some(&serde_json::json!("new_id"))
        );

        // Other extra fields should be preserved
        assert!(merged.extra.contains_key("app_secret"));
        assert_eq!(
            merged.extra.get("app_secret"),
            Some(&serde_json::json!("old_secret"))
        );
        assert!(merged.extra.contains_key("signing_secret"));
        assert_eq!(
            merged.extra.get("signing_secret"),
            Some(&serde_json::json!("keep_me"))
        );
        assert!(merged.extra.contains_key("webhook_path"));
        assert_eq!(
            merged.extra.get("webhook_path"),
            Some(&serde_json::json!("/webhook/feishu"))
        );
    }

    /// Test 3: Unknown channels should be preserved
    /// Given existing config has unknown channel "google_chat"
    /// After saving feishu config, google_chat should still exist
    #[test]
    fn test_unknown_channels_preserved() {
        // Existing config with google_chat
        let mut existing_channels = ChannelsConfig::default();
        existing_channels.google_chat = Some(ChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            extra: {
                let mut m = HashMap::new();
                m.insert("webhook_url".to_string(), serde_json::json!("https://chat.google.com/webhook"));
                m
            },
        });

        // Frontend sends feishu config only
        let setup = SetupChannelsConfig {
            feishu: Some(SetupChannelEntry {
                enabled: true,
                token: None,
                token_env: None,
                extra: {
                    let mut m = HashMap::new();
                    m.insert("app_id".to_string(), serde_json::json!("cli_test"));
                    m.insert("app_secret".to_string(), serde_json::json!("secret"));
                    m
                },
                clear_app_secret: false,
            }),
            ..Default::default()
        };

        // Merge (simulating channels_to_core with existing config)
        let merged = channels_to_core(setup, &existing_channels);

        // feishu should be present
        assert!(merged.feishu.is_some());

        // google_chat should also still exist
        assert!(merged.google_chat.is_some());
        let google_chat = merged.google_chat.unwrap();
        assert!(google_chat.enabled);
        assert_eq!(
            google_chat.extra.get("webhook_url"),
            Some(&serde_json::json!("https://chat.google.com/webhook"))
        );
    }

    /// Test 4: Disabled channel with empty fields should return None
    #[test]
    fn test_disabled_empty_channel_returns_none() {
        let setup_entry = SetupChannelEntry {
            enabled: false,
            token: None,
            token_env: None,
            extra: HashMap::new(),
            clear_app_secret: false,
        };

        let core_entry = channel_entry_to_core(Some(setup_entry), None);
        assert!(core_entry.is_none());
    }

    /// Test 5: Enabled channel with only extra should be saved
    #[test]
    fn test_enabled_with_only_extra_is_saved() {
        let setup_entry = SetupChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            extra: {
                let mut m = HashMap::new();
                m.insert("app_id".to_string(), serde_json::json!("cli_test"));
                m
            },
            clear_app_secret: false,
        };

        let core_entry = channel_entry_to_core(Some(setup_entry), None);
        assert!(core_entry.is_some());
        let core_entry = core_entry.unwrap();
        assert!(core_entry.enabled);
        assert!(core_entry.extra.contains_key("app_id"));
    }

    /// Test 6: Feishu enabled but missing app_id should fail validation
    #[test]
    fn test_feishu_enabled_missing_app_id_fails() {
        let channels = SetupChannelsConfig {
            feishu: Some(SetupChannelEntry {
                enabled: true,
                token: None,
                token_env: None,
                extra: {
                    let mut m = HashMap::new();
                    // app_id is missing
                    m.insert("app_secret".to_string(), serde_json::json!("secret_test"));
                    m
                },
                clear_app_secret: false,
            }),
            ..Default::default()
        };

        let result = validate_feishu_like_channels(&channels);
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Error should mention App ID requirement
        assert!(err.contains("App ID"));
    }

    /// Test 7: Feishu enabled with real outbound_mode but missing app_secret should fail
    #[test]
    fn test_feishu_enabled_missing_app_secret_fails() {
        let channels = SetupChannelsConfig {
            feishu: Some(SetupChannelEntry {
                enabled: true,
                token: None,
                token_env: None,
                extra: {
                    let mut m = HashMap::new();
                    m.insert("app_id".to_string(), serde_json::json!("cli_test"));
                    m.insert("outbound_mode".to_string(), serde_json::json!("real"));
                    // app_secret is missing
                    m
                },
                clear_app_secret: false,
            }),
            ..Default::default()
        };

        let result = validate_feishu_like_channels(&channels);
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Error should mention App Secret requirement for real mode
        assert!(err.contains("App Secret"));
    }

    /// Test 8: Lark enabled but missing app_id/app_secret should fail validation
    #[test]
    fn test_lark_enabled_missing_fields_fails() {
        let channels = SetupChannelsConfig {
            lark: Some(SetupChannelEntry {
                enabled: true,
                token: None,
                token_env: None,
                extra: HashMap::new(), // Both app_id and app_secret missing
                clear_app_secret: false,
            }),
            ..Default::default()
        };

        let result = validate_feishu_like_channels(&channels);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Lark"));
    }

    /// Test 9: Feishu disabled with empty fields should pass validation
    #[test]
    fn test_feishu_disabled_empty_fields_passes() {
        let channels = SetupChannelsConfig {
            feishu: Some(SetupChannelEntry {
                enabled: false,
                token: None,
                token_env: None,
                extra: HashMap::new(), // Empty is OK when disabled
                clear_app_secret: false,
            }),
            ..Default::default()
        };

        let result = validate_feishu_like_channels(&channels);
        assert!(result.is_ok());
    }

    /// Test 10: Feishu enabled with both app_id and app_secret should pass and preserve other extra
    #[test]
    fn test_feishu_enabled_with_required_fields_passes() {
        let channels = SetupChannelsConfig {
            feishu: Some(SetupChannelEntry {
                enabled: true,
                token: None,
                token_env: None,
                extra: {
                    let mut m = HashMap::new();
                    m.insert("app_id".to_string(), serde_json::json!("cli_test"));
                    m.insert("app_secret".to_string(), serde_json::json!("secret_test"));
                    m.insert("signing_secret".to_string(), serde_json::json!("keep_me"));
                    m
                },
                clear_app_secret: false,
            }),
            ..Default::default()
        };

        let result = validate_feishu_like_channels(&channels);
        assert!(result.is_ok());

        // Also verify that extra merge preserves other fields
        let existing_entry = ChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            extra: {
                let mut m = HashMap::new();
                m.insert("webhook_path".to_string(), serde_json::json!("/webhook/feishu"));
                m
            },
        };

        let merged = channel_entry_to_core(
            Some(SetupChannelEntry {
                enabled: true,
                token: None,
                token_env: None,
                extra: channels.feishu.as_ref().unwrap().extra.clone(),
                clear_app_secret: false,
            }),
            Some(&existing_entry),
        );

        assert!(merged.is_some());
        let merged = merged.unwrap();
        assert!(merged.extra.contains_key("app_id"));
        assert!(merged.extra.contains_key("app_secret"));
        assert!(merged.extra.contains_key("signing_secret"));
        assert!(merged.extra.contains_key("webhook_path")); // Preserved from existing
    }

    // =============================================================================
    // Gateway error message tests
    // =============================================================================

    /// Test: extract_error_code identifies port_in_use
    #[test]
    fn test_extract_error_code_port_in_use() {
        let error = "Address already in use:AddrInUse (os error 10048)";
        let code = super::extract_error_code(error);
        assert_eq!(code, Some("port_in_use".to_string()));
    }

    /// Test: extract_error_code identifies already_running
    #[test]
    fn test_extract_error_code_already_running() {
        let error = "Gateway already running on port 10809";
        let code = super::extract_error_code(error);
        assert_eq!(code, Some("already_running".to_string()));
    }

    /// Test: extract_error_code identifies permission_denied
    #[test]
    fn test_extract_error_code_permission_denied() {
        let error = "Permission denied when binding to port";
        let code = super::extract_error_code(error);
        assert_eq!(code, Some("permission_denied".to_string()));
    }

    /// Test: extract_error_code identifies invalid_config
    #[test]
    fn test_extract_error_code_invalid_config() {
        let error = "Invalid port configuration: 0";
        let code = super::extract_error_code(error);
        assert_eq!(code, Some("invalid_config".to_string()));
    }

    /// Test: extract_error_code identifies bind_failed
    #[test]
    fn test_extract_error_code_bind_failed() {
        let error = "Failed to bind to address";
        let code = super::extract_error_code(error);
        assert_eq!(code, Some("bind_failed".to_string()));
    }

    /// Test: extract_error_code falls back to startup_failed
    #[test]
    fn test_extract_error_code_startup_failed() {
        let error = "Unknown startup error";
        let code = super::extract_error_code(error);
        assert_eq!(code, Some("startup_failed".to_string()));
    }

    /// Test: enhance_error_message adds helpful text for port_in_use
    #[test]
    fn test_enhance_error_message_port_in_use() {
        let error = "Address already in use:AddrInUse (os error 10048)";
        let enhanced = super::enhance_error_message(error, Some("port_in_use"));
        // Should mention port and troubleshooting
        assert!(enhanced.contains("端口"));
        assert!(enhanced.contains("netstat"));
    }

    /// Test: enhance_error_message for permission_denied
    #[test]
    fn test_enhance_error_message_permission_denied() {
        let error = "Permission denied";
        let enhanced = super::enhance_error_message(error, Some("permission_denied"));
        assert!(enhanced.contains("权限不足"));
        assert!(enhanced.contains("管理员"));
    }

    /// Test: extract_port_from_error finds port in error
    #[test]
    fn test_extract_port_from_error() {
        let error = "Address already in use:AddrInUse in 0.0.0.0:10809";
        let port = super::extract_port_from_error(error);
        assert_eq!(port, "10809");
    }

    /// Test: extract_port_from_error defaults to 10809
    #[test]
    fn test_extract_port_from_error_default() {
        let error = "Some other error";
        let port = super::extract_port_from_error(error);
        assert_eq!(port, "10809");
    }

    // =============================================================================
    // App Secret clear_app_secret tests
    // =============================================================================

    /// Test 1: app_secret="***SET***" 时保留旧值
    #[test]
    fn test_app_secret_set_marker_preserves_old_value() {
        let existing_entry = ChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            extra: {
                let mut m = HashMap::new();
                m.insert("app_id".to_string(), serde_json::json!("cli_test"));
                m.insert("app_secret".to_string(), serde_json::json!("old_secret_value"));
                m
            },
        };

        let setup_entry = SetupChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            extra: {
                let mut m = HashMap::new();
                m.insert("app_id".to_string(), serde_json::json!("cli_test"));
                m.insert("app_secret".to_string(), serde_json::json!("***SET***"));
                m
            },
            clear_app_secret: false,
        };

        let merged = channel_entry_to_core(Some(setup_entry), Some(&existing_entry));

        assert!(merged.is_some());
        let merged = merged.unwrap();
        // app_secret should be preserved from existing
        assert_eq!(
            merged.extra.get("app_secret"),
            Some(&serde_json::json!("old_secret_value"))
        );
    }

    /// Test 2: app_secret="" 且未 clear 时保留旧值
    #[test]
    fn test_empty_app_secret_without_clear_preserves_old_value() {
        let existing_entry = ChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            extra: {
                let mut m = HashMap::new();
                m.insert("app_id".to_string(), serde_json::json!("cli_test"));
                m.insert("app_secret".to_string(), serde_json::json!("old_secret_value"));
                m
            },
        };

        let setup_entry = SetupChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            extra: {
                let mut m = HashMap::new();
                m.insert("app_id".to_string(), serde_json::json!("cli_test"));
                m.insert("app_secret".to_string(), serde_json::json!("")); // Empty
                m
            },
            clear_app_secret: false,
        };

        let merged = channel_entry_to_core(Some(setup_entry), Some(&existing_entry));

        assert!(merged.is_some());
        let merged = merged.unwrap();
        // app_secret should be preserved from existing
        assert_eq!(
            merged.extra.get("app_secret"),
            Some(&serde_json::json!("old_secret_value"))
        );
    }

    /// Test 3: 输入新 app_secret 时覆盖旧值
    #[test]
    fn test_new_app_secret_overwrites_old_value() {
        let existing_entry = ChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            extra: {
                let mut m = HashMap::new();
                m.insert("app_id".to_string(), serde_json::json!("cli_test"));
                m.insert("app_secret".to_string(), serde_json::json!("old_secret_value"));
                m
            },
        };

        let setup_entry = SetupChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            extra: {
                let mut m = HashMap::new();
                m.insert("app_id".to_string(), serde_json::json!("cli_test"));
                m.insert("app_secret".to_string(), serde_json::json!("new_secret_value"));
                m
            },
            clear_app_secret: false,
        };

        let merged = channel_entry_to_core(Some(setup_entry), Some(&existing_entry));

        assert!(merged.is_some());
        let merged = merged.unwrap();
        // app_secret should be updated to new value
        assert_eq!(
            merged.extra.get("app_secret"),
            Some(&serde_json::json!("new_secret_value"))
        );
    }

    /// Test 4: clear_app_secret=true 时清空旧值
    #[test]
    fn test_clear_app_secret_true_removes_old_value() {
        let existing_entry = ChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            extra: {
                let mut m = HashMap::new();
                m.insert("app_id".to_string(), serde_json::json!("cli_test"));
                m.insert("app_secret".to_string(), serde_json::json!("old_secret_value"));
                m
            },
        };

        let setup_entry = SetupChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            extra: {
                let mut m = HashMap::new();
                m.insert("app_id".to_string(), serde_json::json!("cli_test"));
                m.insert("app_secret".to_string(), serde_json::json!(""));
                m
            },
            clear_app_secret: true,
        };

        let merged = channel_entry_to_core(Some(setup_entry), Some(&existing_entry));

        assert!(merged.is_some());
        let merged = merged.unwrap();
        // app_secret should be removed
        assert!(!merged.extra.contains_key("app_secret"));
    }

    /// Test 5: outbound_mode=real + clear_app_secret=true 时保存失败
    #[test]
    fn test_feishu_real_mode_clear_app_secret_fails() {
        let channels = SetupChannelsConfig {
            feishu: Some(SetupChannelEntry {
                enabled: true,
                token: None,
                token_env: None,
                extra: {
                    let mut m = HashMap::new();
                    m.insert("app_id".to_string(), serde_json::json!("cli_test"));
                    m.insert("outbound_mode".to_string(), serde_json::json!("real"));
                    // app_secret not provided
                    m
                },
                clear_app_secret: true, // Explicitly clearing
            }),
            ..Default::default()
        };

        let result = validate_feishu_like_channels(&channels);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("App Secret") || err.contains("outbound"));
    }

    /// Test 6: outbound_mode=mock + clear_app_secret=true 时保存失败
    #[test]
    fn test_feishu_mock_mode_clear_app_secret_fails() {
        let channels = SetupChannelsConfig {
            feishu: Some(SetupChannelEntry {
                enabled: true,
                token: None,
                token_env: None,
                extra: {
                    let mut m = HashMap::new();
                    m.insert("app_id".to_string(), serde_json::json!("cli_test"));
                    m.insert("outbound_mode".to_string(), serde_json::json!("mock"));
                    // app_secret not provided
                    m
                },
                clear_app_secret: true, // Explicitly clearing
            }),
            ..Default::default()
        };

        let result = validate_feishu_like_channels(&channels);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("App Secret") || err.contains("outbound"));
    }

    /// Test 7: outbound_mode=disabled + clear_app_secret=true 时允许保存
    #[test]
    fn test_feishu_disabled_mode_clear_app_secret_allowed() {
        let channels = SetupChannelsConfig {
            feishu: Some(SetupChannelEntry {
                enabled: true,
                token: None,
                token_env: None,
                extra: {
                    let mut m = HashMap::new();
                    m.insert("app_id".to_string(), serde_json::json!("cli_test"));
                    m.insert("outbound_mode".to_string(), serde_json::json!("disabled"));
                    // app_secret not provided
                    m
                },
                clear_app_secret: true,
            }),
            ..Default::default()
        };

        let result = validate_feishu_like_channels(&channels);
        assert!(result.is_ok());
    }

    /// Test 8: app_secret 不应出现在前端预览和日志中 (通过 masked value 验证)
    #[test]
    fn test_app_secret_masked_in_channel_entry_from_core() {
        let core_entry = ChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            extra: {
                let mut m = HashMap::new();
                m.insert("app_id".to_string(), serde_json::json!("cli_test"));
                m.insert("app_secret".to_string(), serde_json::json!("super_secret_value_123"));
                m
            },
        };

        let setup_back = channel_entry_from_core(&Some(core_entry));

        assert!(setup_back.is_some());
        let setup_back = setup_back.unwrap();
        // app_secret should be masked
        assert_eq!(
            setup_back.extra.get("app_secret"),
            Some(&serde_json::json!("***SET***"))
        );
        // The real secret should NOT be present
        assert_ne!(
            setup_back.extra.get("app_secret"),
            Some(&serde_json::json!("super_secret_value_123"))
        );
    }
}
