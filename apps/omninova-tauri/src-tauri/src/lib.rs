mod cli_install;
mod composer_attachments;
mod desktop_capture;

use omninova_core::channels::{ChannelKind, InboundMessage};
use omninova_core::config::{
    ChannelEntry, ChannelsConfig, Config, GatewayPublicConfig, GatewayPublicMode, ModelProviderConfig,
    ProviderConfig, RobotConfig,
};
use omninova_core::gateway::{
    check_gateway_public_health, normalize_gateway_public_config,
    normalize_public_webhook_base_url, GatewayHealth, GatewayInboundResponse,
    GatewayPublicHealthStatus, GatewayRuntime, GatewayRuntimeStatus,
    GatewaySessionHistoryResponse, GatewaySessionTreeQuery, GatewaySessionTreeResponse,
};
use omninova_core::providers::{ProviderSelection, build_provider_with_selection};
use omninova_core::routing::RouteDecision;
use omninova_core::skills::{import_skills_from_dir, load_skills_from_dir};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WebviewWindowBuilder, WindowEvent};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
    last_gateway_started_at: Option<i64>,
    last_gateway_error: Option<String>,
    /// Error code for the last gateway error (e.g., "port_in_use", "already_running")
    last_gateway_error_code: Option<String>,
    last_public_health: Option<GatewayPublicHealthStatus>,
}

const EMBEDDED_AGENT_BROWSER_BIN_ENV: &str = "OMNINOVA_AGENT_BROWSER_BIN";
const WEBVIEW2_DATA_DIR_ENV: &str = "OMNINOVA_WEBVIEW2_DATA_DIR";
const OPEN_DEVTOOLS_ENV: &str = "OMNINOVA_OPEN_DEVTOOLS";
const WEBVIEW2_LOCK_SCAN_MAX_DEPTH: usize = 4;
const WEBVIEW2_LOCK_SCAN_MAX_RESULTS: usize = 32;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(target_os = "windows")]
fn hide_std_command_window(command: &mut StdCommand) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn hide_std_command_window(_command: &mut StdCommand) {}

#[cfg(target_os = "windows")]
fn hide_tokio_command_window(command: &mut tokio::process::Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn hide_tokio_command_window(_command: &mut tokio::process::Command) {}

#[derive(Debug)]
struct WebviewStartupDiagnostics {
    app_data_dir: PathBuf,
    user_data_dir: PathBuf,
    process_id: u32,
    other_omninova_processes: usize,
    webview2_processes: usize,
    gateway_port_in_use: bool,
    dev_server_port_in_use: bool,
    lock_like_files: Vec<PathBuf>,
}

fn resolve_webview_user_data_dir_with(
    home_dir: Option<&Path>,
    configured_dir: Option<&Path>,
) -> PathBuf {
    if let Some(configured_dir) = configured_dir.filter(|path| !path.as_os_str().is_empty()) {
        return configured_dir.to_path_buf();
    }

    home_dir
        .map(|home| home.join(".omninova").join("webview2"))
        .unwrap_or_else(|| PathBuf::from(".omninova").join("webview2"))
}

fn resolve_webview_user_data_dir() -> PathBuf {
    let configured = std::env::var_os(WEBVIEW2_DATA_DIR_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    resolve_webview_user_data_dir_with(user_home_dir().as_deref(), configured.as_deref())
}

fn is_webview_lock_like_file(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    normalized == "lock"
        || normalized == "lockfile"
        || normalized.starts_with("singleton")
        || normalized.ends_with(".lock")
}

fn collect_webview_lock_like_files(root: &Path) -> Vec<PathBuf> {
    fn visit(root: &Path, current: &Path, depth: usize, results: &mut Vec<PathBuf>) {
        if depth > WEBVIEW2_LOCK_SCAN_MAX_DEPTH
            || results.len() >= WEBVIEW2_LOCK_SCAN_MAX_RESULTS
        {
            return;
        }
        let Ok(entries) = std::fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            if results.len() >= WEBVIEW2_LOCK_SCAN_MAX_RESULTS {
                break;
            }
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                visit(root, &path, depth + 1, results);
            } else if file_type.is_file()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(is_webview_lock_like_file)
            {
                results.push(
                    path.strip_prefix(root)
                        .map(Path::to_path_buf)
                        .unwrap_or(path),
                );
            }
        }
    }

    let mut results = Vec::new();
    visit(root, root, 0, &mut results);
    results
}

#[cfg(target_os = "windows")]
fn windows_process_counts(current_pid: u32) -> (usize, usize) {
    let mut command = StdCommand::new("tasklist");
    hide_std_command_window(&mut command);
    let Ok(output) = command
        .args(["/FO", "CSV", "/NH"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    else {
        return (0, 0);
    };
    let listing = String::from_utf8_lossy(&output.stdout);
    let mut omninova = 0;
    let mut webview2 = 0;
    for line in listing.lines() {
        let mut fields = line
            .trim()
            .trim_matches('"')
            .split("\",\"")
            .map(str::trim);
        let Some(image_name) = fields.next() else {
            continue;
        };
        let pid = fields
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or_default();
        if image_name.eq_ignore_ascii_case("omninova-tauri.exe") && pid != current_pid {
            omninova += 1;
        } else if image_name.eq_ignore_ascii_case("msedgewebview2.exe") {
            webview2 += 1;
        }
    }
    (omninova, webview2)
}

#[cfg(not(target_os = "windows"))]
fn windows_process_counts(_current_pid: u32) -> (usize, usize) {
    (0, 0)
}

fn tcp_port_in_use(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_err()
}

fn collect_webview_startup_diagnostics(
    app_data_dir: PathBuf,
    user_data_dir: PathBuf,
) -> WebviewStartupDiagnostics {
    let process_id = std::process::id();
    let (other_omninova_processes, webview2_processes) = windows_process_counts(process_id);
    let lock_like_files = collect_webview_lock_like_files(&user_data_dir);
    WebviewStartupDiagnostics {
        app_data_dir,
        user_data_dir,
        process_id,
        other_omninova_processes,
        webview2_processes,
        gateway_port_in_use: tcp_port_in_use(10809),
        dev_server_port_in_use: tcp_port_in_use(5173),
        lock_like_files,
    }
}

fn log_webview_startup_diagnostics(diagnostics: &WebviewStartupDiagnostics) {
    eprintln!(
        "[webview-startup] app_data_dir={} user_data_dir={} process_id={}",
        diagnostics.app_data_dir.display(),
        diagnostics.user_data_dir.display(),
        diagnostics.process_id
    );
    eprintln!(
        "[webview-startup] other_omninova_processes={} webview2_processes={} port_10809_in_use={} port_5173_in_use={}",
        diagnostics.other_omninova_processes,
        diagnostics.webview2_processes,
        diagnostics.gateway_port_in_use,
        diagnostics.dev_server_port_in_use
    );
    let lock_names = diagnostics
        .lock_like_files
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    eprintln!(
        "[webview-startup] lock_like_files_present={} lock_like_files={lock_names:?}",
        !lock_names.is_empty()
    );
}

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
    let mut command = StdCommand::new(path);
    hide_std_command_window(&mut command);
    let Ok(output) = command
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
    #[serde(default)]
    gateway_public: GatewayPublicConfig,
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

const SETUP_SENSITIVE_VALUE_MARKER: &str = "***SET***";
const SETUP_CLEAR_SENSITIVE_FIELDS_KEY: &str = "__clear_sensitive_fields";
const SETUP_SENSITIVE_EXTRA_FIELDS: [&str; 3] = [
    "app_secret",
    "verification_token",
    "encrypt_key",
];

fn is_real_sensitive_value(value: &str) -> bool {
    !value.trim().is_empty() && value != SETUP_SENSITIVE_VALUE_MARKER
}

fn setup_requests_sensitive_clear(extra: &HashMap<String, serde_json::Value>, field: &str) -> bool {
    extra
        .get(SETUP_CLEAR_SENSITIVE_FIELDS_KEY)
        .and_then(serde_json::Value::as_str)
        .map(|fields| fields.split(',').any(|item| item.trim() == field))
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize)]
struct GatewayStatusPayload {
    running: bool,
    /// Local gateway base URL.
    url: String,
    /// Sanitized gateway status (never contains secrets).
    gateway_host: String,
    gateway_port: u16,
    feishu_webhook_url: Option<String>,
    feishu_card_callback_url: Option<String>,
    public_webhook_base_url: Option<String>,
    gateway_public_mode: String,
    quick_tunnel_non_production: bool,
    cloudflared_configured: bool,
    cloudflared_found: bool,
    named_tunnel_name_configured: bool,
    named_tunnel_hostname_configured: bool,
    named_tunnel_config_complete: bool,
    enabled_channels: Vec<String>,
    security_mode: Option<String>,
    outbound_mode: Option<String>,
    store_opened: bool,
    store_path: Option<String>,
    retry_worker_enabled: bool,
    last_started_at: Option<i64>,
    health_ok: bool,
    public_health: GatewayPublicHealthStatus,
    last_error: Option<String>,
    error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct GatewayLocalHealthPayload {
    ok: bool,
    status_code: Option<u16>,
    message: String,
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
    mut config: SetupAppConfig,
    validate_all_channels: Option<bool>,
    active_channel_id: Option<String>,
    changed_channel_ids: Option<Vec<String>>,
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
    let validation_scope = if validate_all_channels.unwrap_or(false) {
        ChannelValidationScope::AllEnabled
    } else {
        ChannelValidationScope::Current(active_channel_id.as_deref().unwrap_or("feishu"))
    };
    if let (Some(channels), Some(changed_channel_ids)) =
        (config.channels.as_mut(), changed_channel_ids.as_deref())
    {
        retain_changed_setup_channels(channels, changed_channel_ids);
    }
    let mut next = setup_config_to_core(current, config, validation_scope)?;
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
    let mut npm_command = tokio::process::Command::new("npm");
    hide_tokio_command_window(&mut npm_command);
    let npm_out = npm_command
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
    let mut chromium_command = tokio::process::Command::new(&agent_browser_cmd);
    hide_tokio_command_window(&mut chromium_command);
    let chromium_out = chromium_command
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
    let mut command = tokio::process::Command::new(bin);
    hide_tokio_command_window(&mut command);
    match command
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

fn enabled_channel_names(channels: &ChannelsConfig) -> Vec<&'static str> {
    [
        ("telegram", channels.telegram.as_ref()),
        ("discord", channels.discord.as_ref()),
        ("slack", channels.slack.as_ref()),
        ("whatsapp", channels.whatsapp.as_ref()),
        ("wechat", channels.wechat.as_ref()),
        ("feishu", channels.feishu.as_ref()),
        ("lark", channels.lark.as_ref()),
        ("dingtalk", channels.dingtalk.as_ref()),
        ("matrix", channels.matrix.as_ref()),
        ("email", channels.email.as_ref()),
        ("msteams", channels.msteams.as_ref()),
        ("irc", channels.irc.as_ref()),
        ("webhook", channels.webhook.as_ref()),
    ]
    .into_iter()
    .filter_map(|(name, entry)| entry.filter(|entry| entry.enabled).map(|_| name))
    .collect()
}

async fn preflight_gateway_bind(host: &str, port: u16) -> Result<(), (String, String)> {
    match tokio::net::TcpListener::bind((host, port)).await {
        Ok(listener) => {
            drop(listener);
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => Err((
            format!("Gateway 启动失败：端口 {port} 已被占用。"),
            "port_in_use".to_string(),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => Err((
            "Gateway 启动失败：权限不足，无法绑定监听地址。".to_string(),
            "permission_denied".to_string(),
        )),
        Err(error) => Err((
            format!("Gateway 启动失败：无法绑定监听地址：{error}"),
            "bind_failed".to_string(),
        )),
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
    validate_persisted_feishu_like_channels(&cfg.channels_config)
        .map_err(|error| format!("Gateway 启动失败：{error}"))?;
    eprintln!(
        "[gateway] enabled_channels={:?}",
        enabled_channel_names(&cfg.channels_config)
    );

    // Check if already running without holding the state lock across another
    // status lookup.
    let already_running = {
        let app_state = state_ref.lock().await;
        app_state.gateway_task.is_some()
    };
    if already_running {
        return Err("Gateway 已经在运行中，请先停止后再启动。".to_string());
    }

    // Fail fast with a stable, user-facing port error before spawning the
    // long-lived server task. The listener is immediately released and the
    // runtime performs the authoritative bind next.
    if let Err((message, code)) =
        preflight_gateway_bind(cfg.gateway.host.as_str(), cfg.gateway.port).await
    {
        let mut app_state = state_ref.lock().await;
        app_state.last_gateway_error = Some(message.clone());
        app_state.last_gateway_error_code = Some(code);
        return Err(message);
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

    {
        let mut app_state = state_ref.lock().await;
        app_state.last_gateway_started_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
        );
    }

    Ok(gateway_status_from_state(&state_ref).await)
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
async fn restart_gateway(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<GatewayStatusPayload, String> {
    let state_ref = state.inner().clone();
    stop_gateway_inner(&state_ref).await;
    sleep(Duration::from_millis(100)).await;
    start_gateway_inner(state_ref).await
}

#[tauri::command]
async fn test_gateway_health(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<GatewayLocalHealthPayload, String> {
    let state_ref = state.inner().clone();
    sync_gateway_task_state(&state_ref).await;
    let status = gateway_status_from_state(&state_ref).await;
    if !status.running {
        return Ok(GatewayLocalHealthPayload {
            ok: false,
            status_code: None,
            message: "Gateway 未运行，请先启动 Gateway。".to_string(),
        });
    }

    let connect_host = match status.gateway_host.as_str() {
        "0.0.0.0" | "::" | "[::]" => "127.0.0.1",
        host => host,
    };
    let probe = async {
        let mut stream = tokio::net::TcpStream::connect((connect_host, status.gateway_port))
            .await
            .map_err(|error| format!("连接本地 Gateway 失败：{error}"))?;
        let request = format!(
            "GET /health HTTP/1.1\r\nHost: {connect_host}:{}\r\nConnection: close\r\n\r\n",
            status.gateway_port
        );
        stream
            .write_all(request.as_bytes())
            .await
            .map_err(|error| format!("发送健康检查失败：{error}"))?;
        let mut response = vec![0_u8; 4096];
        let read = stream
            .read(&mut response)
            .await
            .map_err(|error| format!("读取健康检查失败：{error}"))?;
        let first_line = String::from_utf8_lossy(&response[..read])
            .lines()
            .next()
            .unwrap_or_default()
            .to_string();
        let status_code = first_line
            .split_whitespace()
            .nth(1)
            .and_then(|value| value.parse::<u16>().ok());
        Ok::<Option<u16>, String>(status_code)
    };

    match tokio::time::timeout(Duration::from_secs(3), probe).await {
        Ok(Ok(Some(200))) => Ok(GatewayLocalHealthPayload {
            ok: true,
            status_code: Some(200),
            message: "Gateway 本地健康检查通过（HTTP 200）。".to_string(),
        }),
        Ok(Ok(status_code)) => Ok(GatewayLocalHealthPayload {
            ok: false,
            status_code,
            message: format!(
                "Gateway 本地健康检查失败{}。",
                status_code
                    .map(|code| format!("（HTTP {code}）"))
                    .unwrap_or_default()
            ),
        }),
        Ok(Err(message)) => Ok(GatewayLocalHealthPayload {
            ok: false,
            status_code: None,
            message,
        }),
        Err(_) => Ok(GatewayLocalHealthPayload {
            ok: false,
            status_code: None,
            message: "Gateway 本地健康检查超时。".to_string(),
        }),
    }
}

#[tauri::command]
async fn test_gateway_public_health(
    base_url: Option<String>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<GatewayPublicHealthStatus, String> {
    let state_ref = state.inner().clone();
    sync_gateway_task_state(&state_ref).await;
    let (runtime, running) = {
        let app_state = state_ref.lock().await;
        (app_state.runtime.clone(), app_state.gateway_task.is_some())
    };
    let mut config = runtime.get_config().await;
    let requested_base_url = match base_url {
        Some(value) => match normalize_public_webhook_base_url(&value) {
            Some(value) => {
                // This is an ephemeral probe target from the current UI draft.
                // It must not persist or be overridden by a stale named-tunnel
                // field while the request is in flight.
                config.gateway_public.mode = GatewayPublicMode::ExternalPublicUrl;
                config.gateway_public.public_webhook_base_url = Some(value.clone());
                Some(value)
            }
            None => {
                let result = GatewayPublicHealthStatus::not_configured();
                let mut app_state = state_ref.lock().await;
                app_state.last_public_health = Some(result.clone());
                return Ok(result);
            }
        },
        None => omninova_core::gateway::resolve_public_webhook_base_url(&config),
    };
    let mut result = if running {
        check_gateway_public_health(&config).await
    } else if let Some(base_url) = requested_base_url {
        GatewayPublicHealthStatus {
            configured: true,
            ok: false,
            base_url: Some(base_url),
            checked_url: None,
            checked_at: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
            ),
            status_code: None,
            error_kind: Some("gateway_not_running".to_string()),
            error: Some("Gateway 未启动，无法检查公网入口。".to_string()),
        }
    } else {
        GatewayPublicHealthStatus::not_configured()
    };

    // Keep the configured base in the cached snapshot so stale results are
    // discarded automatically after the user changes Public Base URL.
    if result.base_url.is_none() {
        result.base_url = omninova_core::gateway::resolve_public_webhook_base_url(&config);
    }
    let mut app_state = state_ref.lock().await;
    app_state.last_public_health = Some(result.clone());
    Ok(result)
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
    let (runtime, running, last_started_at, last_error, error_code, last_public_health): (
        GatewayRuntime,
        bool,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<GatewayPublicHealthStatus>,
    ) = {
        let app_state = state.lock().await;
        (
            app_state.runtime.clone(),
            app_state.gateway_task.is_some(),
            app_state.last_gateway_started_at,
            app_state.last_gateway_error.clone(),
            app_state.last_gateway_error_code.clone(),
            app_state.last_public_health.clone(),
        )
    };

    let mut status = GatewayRuntimeStatus::from_runtime(
        running,
        &runtime,
        last_started_at,
        last_error.clone(),
    ).await;
    if let Some(public_health) = last_public_health {
        if public_health.base_url == status.public_webhook_base_url {
            status.public_health = public_health;
        }
    }

    GatewayStatusPayload {
        running,
        url: status.local_base_url.clone(),
        gateway_host: status.bind_host.clone(),
        gateway_port: status.bind_port,
        feishu_webhook_url: status.feishu_webhook_url,
        feishu_card_callback_url: status.feishu_card_callback_url,
        public_webhook_base_url: status.public_webhook_base_url,
        gateway_public_mode: status.gateway_public_mode,
        quick_tunnel_non_production: status.quick_tunnel_non_production,
        cloudflared_configured: status.cloudflared_configured,
        cloudflared_found: status.cloudflared_found,
        named_tunnel_name_configured: status.named_tunnel_name_configured,
        named_tunnel_hostname_configured: status.named_tunnel_hostname_configured,
        named_tunnel_config_complete: status.named_tunnel_config_complete,
        enabled_channels: status.enabled_channels,
        security_mode: status.security_mode,
        outbound_mode: status.outbound_mode,
        store_opened: status.store_opened,
        store_path: status.store_path,
        retry_worker_enabled: status.retry_worker_enabled,
        last_started_at: status.last_started_at,
        health_ok: status.health_ok,
        public_health: status.public_health,
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
    let task = {
        let mut app_state = state.lock().await;
        let task = app_state.gateway_task.take();
        app_state.last_gateway_error = None;
        app_state.last_gateway_error_code = None;
        // A successful public probe is no longer authoritative after the
        // local origin stops. Clear it so restart cannot resurrect stale OK.
        app_state.last_public_health = None;
        task
    };
    if let Some(task) = task {
        task.abort();
        let _ = task.await;
    }
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

    let mut gateway_public = config.gateway_public.clone();
    if gateway_public.public_webhook_base_url.is_none() {
        gateway_public.public_webhook_base_url = config
            .channels_config
            .feishu
            .as_ref()
            .and_then(|entry| entry.extra.get("public_webhook_base_url"))
            .and_then(serde_json::Value::as_str)
            .and_then(normalize_public_webhook_base_url);
    }
    normalize_gateway_public_config(&mut gateway_public);

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
        gateway_public,
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

    // Never send channel secrets to the renderer. Keep a marker so an
    // unrelated settings save preserves the stored value.
    let mut masked_extra = entry.extra.clone();
    for key in ["app_secret", "verification_token", "encrypt_key"] {
        if masked_extra
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
        {
            masked_extra.insert(
                key.to_string(),
                serde_json::Value::String(SETUP_SENSITIVE_VALUE_MARKER.to_string()),
            );
        }
    }
    // Accept the typed core fields introduced for webhook security as well as
    // legacy extra fields, while keeping the UI transport backwards compatible.
    for (key, value) in [
        ("verification_token", entry.verification_token.as_ref()),
        ("verification_token_env", entry.verification_token_env.as_ref()),
        ("encrypt_key", entry.encrypt_key.as_ref()),
        ("encrypt_key_env", entry.encrypt_key_env.as_ref()),
    ] {
        if !masked_extra.contains_key(key) && value.is_some_and(|v| !v.trim().is_empty()) {
            let value = if key.ends_with("_env") {
                value.expect("checked above").clone()
            } else {
                SETUP_SENSITIVE_VALUE_MARKER.to_string()
            };
            masked_extra.insert(key.to_string(), serde_json::Value::String(value));
        }
    }
    if !masked_extra.contains_key("security_mode") {
        if let Some(mode) = entry.security_mode.as_ref().filter(|v| !v.trim().is_empty()) {
            masked_extra.insert("security_mode".to_string(), serde_json::Value::String(mode.clone()));
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

#[derive(Debug, Clone, Copy)]
enum ChannelValidationScope<'a> {
    Current(&'a str),
    AllEnabled,
}

fn setup_config_to_core(
    mut current: Config,
    setup: SetupAppConfig,
    channel_validation_scope: ChannelValidationScope<'_>,
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

    let mut gateway_public = setup.gateway_public;
    if gateway_public.public_webhook_base_url.is_none() {
        gateway_public.public_webhook_base_url = current
            .channels_config
            .feishu
            .as_ref()
            .and_then(|entry| entry.extra.get("public_webhook_base_url"))
            .and_then(serde_json::Value::as_str)
            .and_then(normalize_public_webhook_base_url);
    }
    normalize_gateway_public_config(&mut gateway_public);
    current.gateway_public = gateway_public;

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
        let mut next_channels = channels_to_core(channels, &current.channels_config);
        if current
            .gateway_public
            .public_webhook_base_url
            .is_some()
        {
            if let Some(feishu) = next_channels.feishu.as_mut() {
                feishu.extra.remove("public_webhook_base_url");
            }
        }
        // Validate the merged config, not the renderer DTO. This lets an
        // unchanged `***SET***` marker safely preserve the real stored value
        // while still rejecting a marker with no corresponding secret.
        validate_persisted_channels_for_save(&next_channels, channel_validation_scope)?;
        current.channels_config = next_channels;
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
        // A disabled channel may still have credentials saved from an earlier
        // setup. Keep them so “关闭 Lark” never silently deletes them.
        return existing.cloned().map(|mut existing| {
            existing.enabled = false;
            existing
        });
    }
    
    // Start with existing extra, then overlay new values. A stale UI marker
    // from an older config is never a usable secret and must not survive.
    let mut merged_extra = existing
        .as_ref()
        .map(|e| e.extra.clone())
        .unwrap_or_default();
    for key in SETUP_SENSITIVE_EXTRA_FIELDS {
        if merged_extra
            .get(key)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !is_real_sensitive_value(value))
        {
            merged_extra.remove(key);
        }
    }
    if let Some(existing) = existing {
        for (key, value) in [
            ("verification_token", existing.verification_token.as_ref()),
            ("verification_token_env", existing.verification_token_env.as_ref()),
            ("encrypt_key", existing.encrypt_key.as_ref()),
            ("encrypt_key_env", existing.encrypt_key_env.as_ref()),
        ] {
            if !merged_extra.contains_key(key) {
                if let Some(value) = value.filter(|value| is_real_sensitive_value(value)) {
                    merged_extra.insert(key.to_string(), serde_json::Value::String(value.clone()));
                }
            }
        }
        if !merged_extra.contains_key("security_mode") {
            if let Some(mode) = existing.security_mode.as_ref().filter(|v| !v.trim().is_empty()) {
                merged_extra.insert("security_mode".to_string(), serde_json::Value::String(mode.clone()));
            }
        }
    }
    
    let clear_verification_token = setup_requests_sensitive_clear(&entry.extra, "verification_token");
    let clear_encrypt_key = setup_requests_sensitive_clear(&entry.extra, "encrypt_key");

    for (key, value) in entry.extra {
        if key == SETUP_CLEAR_SENSITIVE_FIELDS_KEY {
            continue;
        }
        let value_str = value.as_str().unwrap_or("");

        let explicit_clear = match key.as_str() {
            "app_secret" => entry.clear_app_secret,
            "verification_token" => clear_verification_token,
            "encrypt_key" => clear_encrypt_key,
            _ => false,
        };
        if SETUP_SENSITIVE_EXTRA_FIELDS.contains(&key.as_str()) {
            if explicit_clear {
                merged_extra.remove(&key);
            } else if is_real_sensitive_value(value_str) {
                merged_extra.insert(key, value);
            }
            continue;
        }

        if key == "public_webhook_base_url" {
            match normalize_public_webhook_base_url(value_str) {
                Some(base_url) => {
                    merged_extra.insert(key, serde_json::Value::String(base_url));
                }
                None => {
                    merged_extra.remove(&key);
                }
            }
            continue;
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
    if clear_verification_token {
        merged_extra.remove("verification_token");
    }
    if clear_encrypt_key {
        merged_extra.remove("encrypt_key");
    }
    
    Some(ChannelEntry {
        enabled: entry.enabled,
        token: normalize_optional_string(entry.token),
        token_env: normalize_optional_string(entry.token_env),
        security_mode: None,
        verification_token: None,
        verification_token_env: None,
        encrypt_key: None,
        encrypt_key_env: None,
        extra: merged_extra,
    })
}

fn merge_channel_entry(
    entry: Option<SetupChannelEntry>,
    existing: Option<&ChannelEntry>,
) -> Option<ChannelEntry> {
    match entry {
        Some(entry) => channel_entry_to_core(Some(entry), existing),
        None => existing.cloned(),
    }
}

/// Keep only channels the current editor explicitly changed. Missing entries
/// are merged from the persisted config by `channels_to_core`, preventing a
/// stale/default selected channel from disabling another active integration.
fn retain_changed_setup_channels(channels: &mut SetupChannelsConfig, changed: &[String]) {
    let has = |channel_id: &str| changed.iter().any(|item| item == channel_id);
    if !has("telegram") { channels.telegram = None; }
    if !has("discord") { channels.discord = None; }
    if !has("slack") { channels.slack = None; }
    if !has("whatsapp") { channels.whatsapp = None; }
    if !has("wechat") { channels.wechat = None; }
    if !has("feishu") { channels.feishu = None; }
    if !has("lark") { channels.lark = None; }
    if !has("dingtalk") { channels.dingtalk = None; }
    if !has("matrix") { channels.matrix = None; }
    if !has("email") { channels.email = None; }
    if !has("msteams") { channels.msteams = None; }
    if !has("irc") { channels.irc = None; }
    if !has("webhook") { channels.webhook = None; }
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
        telegram: merge_channel_entry(setup.telegram, current.telegram.as_ref()),
        discord: merge_channel_entry(setup.discord, current.discord.as_ref()),
        slack: merge_channel_entry(setup.slack, current.slack.as_ref()),
        whatsapp: merge_channel_entry(setup.whatsapp, current.whatsapp.as_ref()),
        wechat: merge_channel_entry(setup.wechat, current.wechat.as_ref()),
        feishu: merge_channel_entry(setup.feishu, current.feishu.as_ref()),
        lark: merge_channel_entry(setup.lark, current.lark.as_ref()),
        dingtalk: merge_channel_entry(setup.dingtalk, current.dingtalk.as_ref()),
        matrix: merge_channel_entry(setup.matrix, current.matrix.as_ref()),
        email: merge_channel_entry(setup.email, current.email.as_ref()),
        msteams: merge_channel_entry(setup.msteams, current.msteams.as_ref()),
        irc: merge_channel_entry(setup.irc, current.irc.as_ref()),
        webhook: merge_channel_entry(setup.webhook, current.webhook.as_ref()),
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
        last_gateway_started_at: None,
        last_gateway_error: None,
        last_gateway_error_code: None,
        last_public_health: None,
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
            restart_gateway,
            test_gateway_health,
            test_gateway_public_health,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            configure_embedded_agent_browser_env(app.handle());

            let app_data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from(".omninova"));
            let webview_user_data_dir = resolve_webview_user_data_dir();
            std::fs::create_dir_all(&webview_user_data_dir).map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!(
                        "无法创建 WebView2 用户数据目录 {}：{error}",
                        webview_user_data_dir.display()
                    ),
                )
            })?;
            let diagnostics = collect_webview_startup_diagnostics(
                app_data_dir,
                webview_user_data_dir.clone(),
            );
            log_webview_startup_diagnostics(&diagnostics);
            if diagnostics.other_omninova_processes > 0 {
                let message =
                    "检测到 OmniNova/WebView2 可能仍在运行，请关闭旧实例后重试。";
                eprintln!("[webview-startup] rejected reason=existing_omninova_instance");
                return Err(
                    std::io::Error::new(std::io::ErrorKind::AlreadyExists, message).into(),
                );
            }

            // Guard: skip manual main window creation when Tauri already
            // created one from app config (avoids `a webview with label 'main' already exists`).
            if app.get_webview_window("main").is_some() {
                eprintln!("[webview-startup] main_window_exists=true skip_manual_create=true");
            } else {
                let window_config = app.config().app.windows.first().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "缺少主窗口配置，无法创建 OmniNova 窗口。",
                    )
                })?;
                let window = WebviewWindowBuilder::from_config(app.handle(), window_config)?
                    .data_directory(webview_user_data_dir.clone())
                    .build()
                    .map_err(|error| {
                        let message = format!(
                            "无法创建 OmniNova 窗口。WebView2 用户数据目录可能正在使用中：{}。请关闭旧实例后重试。原始错误：{error}",
                            webview_user_data_dir.display()
                        );
                        eprintln!("[webview-startup] window_create_failed resource_may_be_in_use=true");
                        std::io::Error::new(std::io::ErrorKind::Other, message)
                    })?;
                eprintln!(
                    "[webview-startup] window_created=true user_data_dir={}",
                    webview_user_data_dir.display()
                );
                #[cfg(debug_assertions)]
                {
                    let open_devtools = std::env::var(OPEN_DEVTOOLS_ENV)
                        .map(|value| matches!(value.trim(), "1" | "true" | "TRUE"))
                        .unwrap_or(false);
                    if open_devtools {
                        window.open_devtools();
                    }
                }
            }

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

            Ok(())
        })
        .build(tauri::generate_context!());

    let app = match app {
        Ok(app) => app,
        Err(error) => {
            eprintln!("[app-startup] application_build_failed error={error}");
            return;
        }
    };

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
mod webview_startup_tests {
    use super::*;

    #[test]
    fn webview_user_data_dir_is_stable_under_omninova_home() {
        let home = PathBuf::from(r"C:\Users\Hero");
        let resolved = resolve_webview_user_data_dir_with(Some(&home), None);
        assert_eq!(resolved, home.join(".omninova").join("webview2"));
    }

    #[test]
    fn configured_webview_user_data_dir_is_preserved() {
        let home = PathBuf::from(r"C:\Users\Hero");
        let configured = PathBuf::from(r"D:\OmniNovaData\webview2");
        let resolved =
            resolve_webview_user_data_dir_with(Some(&home), Some(&configured));
        assert_eq!(resolved, configured);
    }

    #[test]
    fn lock_scan_only_returns_lock_like_relative_paths() {
        let root = std::env::temp_dir().join(format!(
            "omninova-webview-lock-test-{}",
            std::process::id()
        ));
        let nested = root.join("EBWebView").join("Default");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("LOCK"), b"").unwrap();
        std::fs::write(nested.join("Preferences"), b"{}").unwrap();

        let found = collect_webview_lock_like_files(&root);
        assert_eq!(
            found,
            vec![PathBuf::from("EBWebView").join("Default").join("LOCK")]
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn webview_diagnostics_shape_contains_no_secret_fields() {
        let diagnostics = WebviewStartupDiagnostics {
            app_data_dir: PathBuf::from(r"C:\Users\Hero\AppData\Local\com.omninova.claw"),
            user_data_dir: PathBuf::from(r"C:\Users\Hero\.omninova\webview2"),
            process_id: 42,
            other_omninova_processes: 0,
            webview2_processes: 3,
            gateway_port_in_use: false,
            dev_server_port_in_use: true,
            lock_like_files: vec![PathBuf::from("EBWebView").join("Default").join("LOCK")],
        };
        let rendered = format!("{diagnostics:?}").to_ascii_lowercase();
        for forbidden in [
            "app_secret",
            "verification_token",
            "encrypt_key",
            "tenant_access_token",
            "authorization",
        ] {
            assert!(!rendered.contains(forbidden));
        }
    }
}

#[cfg(test)]
mod channel_tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn setup_save_retains_only_explicitly_changed_channel_entries() {
        let mut setup = SetupChannelsConfig {
            feishu: Some(SetupChannelEntry {
                enabled: false,
                ..Default::default()
            }),
            dingtalk: Some(SetupChannelEntry {
                enabled: true,
                ..Default::default()
            }),
            ..Default::default()
        };

        retain_changed_setup_channels(&mut setup, &["dingtalk".to_string()]);

        assert!(setup.feishu.is_none());
        assert!(setup.dingtalk.is_some());
    }

    #[test]
    fn setup_save_does_not_disable_unchanged_enabled_channels() {
        let current = ChannelsConfig {
            feishu: Some(ChannelEntry {
                enabled: true,
                ..Default::default()
            }),
            dingtalk: Some(ChannelEntry {
                enabled: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut setup = SetupChannelsConfig {
            feishu: Some(SetupChannelEntry {
                enabled: false,
                ..Default::default()
            }),
            dingtalk: Some(SetupChannelEntry {
                enabled: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        retain_changed_setup_channels(&mut setup, &["dingtalk".to_string()]);

        let merged = channels_to_core(setup, &current);
        assert!(merged.feishu.as_ref().is_some_and(|entry| entry.enabled));
        assert!(merged.dingtalk.as_ref().is_some_and(|entry| entry.enabled));
    }

    #[test]
    fn setup_migrates_legacy_public_webhook_url_to_gateway_public() {
        let mut core = Config::default();
        core.channels_config.feishu = Some(ChannelEntry {
            enabled: false,
            extra: HashMap::from([(
                "public_webhook_base_url".to_string(),
                serde_json::json!("https://example.test/webhook/feishu/card"),
            )]),
            ..Default::default()
        });

        let mut setup = setup_config_from_core(&core);
        assert_eq!(
            setup.gateway_public.public_webhook_base_url.as_deref(),
            Some("https://example.test")
        );
        setup.gateway_public.mode =
            omninova_core::config::GatewayPublicMode::ExternalPublicUrl;

        let saved =
            setup_config_to_core(core, setup, ChannelValidationScope::Current("feishu"))
                .unwrap();
        assert_eq!(
            saved
                .gateway_public
                .public_webhook_base_url
                .as_deref(),
            Some("https://example.test")
        );
        assert!(!saved
            .channels_config
            .feishu
            .as_ref()
            .unwrap()
            .extra
            .contains_key("public_webhook_base_url"));
    }

    #[test]
    fn setup_named_tunnel_hostname_generates_and_persists_public_base() {
        let core = Config::default();
        let mut setup = setup_config_from_core(&core);
        setup.gateway_public = GatewayPublicConfig {
            mode: omninova_core::config::GatewayPublicMode::NamedCloudflareTunnel,
            public_webhook_base_url: Some("https://stale.trycloudflare.com".to_string()),
            cloudflared_path: Some(PathBuf::from(r"C:\Tools\cloudflared.exe")),
            named_tunnel_name: Some("omninova-fixed".to_string()),
            named_tunnel_hostname: Some(
                "https://Fixed.Example.Test/webhook/feishu/card".to_string(),
            ),
        };

        let saved =
            setup_config_to_core(core, setup, ChannelValidationScope::Current("feishu"))
                .expect("named tunnel setup saves");

        assert_eq!(
            saved.gateway_public.named_tunnel_hostname.as_deref(),
            Some("fixed.example.test")
        );
        assert_eq!(
            saved.gateway_public.public_webhook_base_url.as_deref(),
            Some("https://fixed.example.test")
        );
    }

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
            security_mode: None,
            verification_token: None,
            verification_token_env: None,
            encrypt_key: None,
            encrypt_key_env: None,
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
            security_mode: None,
            verification_token: None,
            verification_token_env: None,
            encrypt_key: None,
            encrypt_key_env: None,
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

    #[test]
    fn test_saving_feishu_preserves_an_omitted_lark_channel() {
        let existing = ChannelsConfig {
            lark: Some(ChannelEntry {
                enabled: true,
                extra: HashMap::from([
                    ("app_id".to_string(), serde_json::json!("lark_app")),
                    ("app_secret".to_string(), serde_json::json!("saved-secret")),
                ]),
                ..Default::default()
            }),
            ..Default::default()
        };
        let setup = SetupChannelsConfig {
            feishu: Some(setup_feishu_security_entry("dev", None, None, None)),
            ..Default::default()
        };

        let saved = channels_to_core(setup, &existing);
        let lark = saved.lark.expect("omitted Lark must be preserved");
        assert!(lark.enabled);
        assert_eq!(lark.extra.get("app_id"), Some(&serde_json::json!("lark_app")));
        assert_eq!(
            lark.extra.get("app_secret"),
            Some(&serde_json::json!("saved-secret"))
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

    #[test]
    fn test_disabling_existing_lark_preserves_its_credentials() {
        let existing = ChannelEntry {
            enabled: true,
            extra: HashMap::from([
                ("app_id".to_string(), serde_json::json!("lark_app")),
                ("app_secret".to_string(), serde_json::json!("saved-secret")),
            ]),
            ..Default::default()
        };
        let disabled = SetupChannelEntry {
            enabled: false,
            token: None,
            token_env: None,
            extra: HashMap::new(),
            clear_app_secret: false,
        };

        let saved = channel_entry_to_core(Some(disabled), Some(&existing))
            .expect("disabled existing channel should remain configured");
        assert!(!saved.enabled);
        assert_eq!(saved.extra.get("app_id"), Some(&serde_json::json!("lark_app")));
        assert_eq!(
            saved.extra.get("app_secret"),
            Some(&serde_json::json!("saved-secret"))
        );
    }

    #[test]
    fn test_lark_is_disabled_by_default() {
        let channels = ChannelsConfig::default();
        assert!(!channels.lark.as_ref().is_some_and(|entry| entry.enabled));
        assert!(!enabled_channel_names(&channels).contains(&"lark"));
    }

    #[test]
    fn test_feishu_only_enabled_channel_list_does_not_include_lark() {
        let channels = ChannelsConfig {
            feishu: Some(existing_feishu_security_entry("dev", None, None)),
            lark: Some(ChannelEntry {
                enabled: false,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(enabled_channel_names(&channels), vec!["feishu"]);
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
            security_mode: None,
            verification_token: None,
            verification_token_env: None,
            encrypt_key: None,
            encrypt_key_env: None,
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
            security_mode: None,
            verification_token: None,
            verification_token_env: None,
            encrypt_key: None,
            encrypt_key_env: None,
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
            security_mode: None,
            verification_token: None,
            verification_token_env: None,
            encrypt_key: None,
            encrypt_key_env: None,
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
            security_mode: None,
            verification_token: None,
            verification_token_env: None,
            encrypt_key: None,
            encrypt_key_env: None,
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
            security_mode: None,
            verification_token: None,
            verification_token_env: None,
            encrypt_key: None,
            encrypt_key_env: None,
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
            security_mode: None,
            verification_token: None,
            verification_token_env: None,
            encrypt_key: None,
            encrypt_key_env: None,
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

    #[test]
    fn test_feishu_security_fields_are_masked_and_preserved() {
        let existing = ChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            security_mode: Some("encrypted".to_string()),
            verification_token: Some("verification-token-must-not-leak".to_string()),
            verification_token_env: Some("FEISHU_VERIFICATION_TOKEN".to_string()),
            encrypt_key: Some("encrypt-key-must-not-leak".to_string()),
            encrypt_key_env: Some("FEISHU_ENCRYPT_KEY".to_string()),
            extra: HashMap::new(),
        };

        let setup = channel_entry_from_core(&Some(existing.clone())).expect("setup entry");
        assert_eq!(setup.extra.get("security_mode"), Some(&serde_json::json!("encrypted")));
        assert_eq!(setup.extra.get("verification_token"), Some(&serde_json::json!("***SET***")));
        assert_eq!(setup.extra.get("encrypt_key"), Some(&serde_json::json!("***SET***")));
        assert_eq!(setup.extra.get("verification_token_env"), Some(&serde_json::json!("FEISHU_VERIFICATION_TOKEN")));
        assert_eq!(setup.extra.get("encrypt_key_env"), Some(&serde_json::json!("FEISHU_ENCRYPT_KEY")));

        let round_tripped = channel_entry_to_core(Some(setup), Some(&existing)).expect("core entry");
        assert_eq!(round_tripped.extra.get("verification_token"), Some(&serde_json::json!("verification-token-must-not-leak")));
        assert_eq!(round_tripped.extra.get("encrypt_key"), Some(&serde_json::json!("encrypt-key-must-not-leak")));
    }

    fn setup_feishu_security_entry(
        mode: &str,
        verification_token: Option<&str>,
        encrypt_key: Option<&str>,
        clear_sensitive_fields: Option<&str>,
    ) -> SetupChannelEntry {
        let mut extra = HashMap::new();
        extra.insert("app_id".to_string(), serde_json::json!("test-app"));
        extra.insert("outbound_mode".to_string(), serde_json::json!("disabled"));
        extra.insert("security_mode".to_string(), serde_json::json!(mode));
        if let Some(value) = verification_token {
            extra.insert("verification_token".to_string(), serde_json::json!(value));
        }
        if let Some(value) = encrypt_key {
            extra.insert("encrypt_key".to_string(), serde_json::json!(value));
        }
        if let Some(value) = clear_sensitive_fields {
            extra.insert(
                SETUP_CLEAR_SENSITIVE_FIELDS_KEY.to_string(),
                serde_json::json!(value),
            );
        }
        SetupChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            extra,
            clear_app_secret: false,
        }
    }

    fn existing_feishu_security_entry(mode: &str, token: Option<&str>, key: Option<&str>) -> ChannelEntry {
        let mut extra = HashMap::new();
        extra.insert("app_id".to_string(), serde_json::json!("test-app"));
        extra.insert("outbound_mode".to_string(), serde_json::json!("disabled"));
        extra.insert("security_mode".to_string(), serde_json::json!(mode));
        if let Some(token) = token {
            extra.insert("verification_token".to_string(), serde_json::json!(token));
        }
        if let Some(key) = key {
            extra.insert("encrypt_key".to_string(), serde_json::json!(key));
        }
        ChannelEntry {
            enabled: true,
            extra,
            ..Default::default()
        }
    }

    #[test]
    fn test_feishu_verification_token_marker_preserves_old_value() {
        let existing = existing_feishu_security_entry("token", Some("old-token"), None);
        let merged = channel_entry_to_core(
            Some(setup_feishu_security_entry("token", Some(SETUP_SENSITIVE_VALUE_MARKER), None, None)),
            Some(&existing),
        )
        .expect("merged entry");

        assert_eq!(merged.extra.get("verification_token"), Some(&serde_json::json!("old-token")));
        assert_ne!(merged.extra.get("verification_token"), Some(&serde_json::json!(SETUP_SENSITIVE_VALUE_MARKER)));
        assert!(validate_persisted_feishu_like_channels(&ChannelsConfig {
            feishu: Some(merged),
            ..Default::default()
        })
        .is_ok());
    }

    #[test]
    fn test_feishu_encrypt_key_marker_preserves_old_value() {
        let existing = existing_feishu_security_entry("encrypted", Some("old-token"), Some("old-key"));
        let merged = channel_entry_to_core(
            Some(setup_feishu_security_entry(
                "encrypted",
                Some(SETUP_SENSITIVE_VALUE_MARKER),
                Some(SETUP_SENSITIVE_VALUE_MARKER),
                None,
            )),
            Some(&existing),
        )
        .expect("merged entry");

        assert_eq!(merged.extra.get("encrypt_key"), Some(&serde_json::json!("old-key")));
        assert_ne!(merged.extra.get("encrypt_key"), Some(&serde_json::json!(SETUP_SENSITIVE_VALUE_MARKER)));
    }

    #[test]
    fn test_feishu_clear_sensitive_fields_removes_old_values() {
        let existing = existing_feishu_security_entry("encrypted", Some("old-token"), Some("old-key"));
        let merged = channel_entry_to_core(
            Some(setup_feishu_security_entry(
                "dev",
                Some(SETUP_SENSITIVE_VALUE_MARKER),
                Some(SETUP_SENSITIVE_VALUE_MARKER),
                Some("verification_token,encrypt_key"),
            )),
            Some(&existing),
        )
        .expect("merged entry");

        assert!(!merged.extra.contains_key("verification_token"));
        assert!(!merged.extra.contains_key("encrypt_key"));
        assert!(!merged.extra.contains_key(SETUP_CLEAR_SENSITIVE_FIELDS_KEY));
    }

    #[test]
    fn test_feishu_security_mode_validation_uses_real_merged_values() {
        let dev = ChannelsConfig {
            feishu: Some(existing_feishu_security_entry("dev", None, None)),
            ..Default::default()
        };
        assert!(validate_persisted_feishu_like_channels(&dev).is_ok());

        let token_missing = ChannelsConfig {
            feishu: Some(existing_feishu_security_entry("token", None, None)),
            ..Default::default()
        };
        assert!(validate_persisted_feishu_like_channels(&token_missing).is_err());

        let encrypted_missing_key = ChannelsConfig {
            feishu: Some(existing_feishu_security_entry("encrypted", Some("token"), None)),
            ..Default::default()
        };
        assert!(validate_persisted_feishu_like_channels(&encrypted_missing_key).is_err());

        let marker_without_existing = channel_entry_to_core(
            Some(setup_feishu_security_entry("token", Some(SETUP_SENSITIVE_VALUE_MARKER), None, None)),
            None,
        )
        .expect("entry");
        assert!(validate_persisted_feishu_like_channels(&ChannelsConfig {
            feishu: Some(marker_without_existing),
            ..Default::default()
        })
        .is_err());
    }

    #[test]
    fn test_feishu_mode_transitions_do_not_block_dev_startup() {
        let token_entry = existing_feishu_security_entry("token", Some("saved-token"), None);
        assert!(validate_persisted_feishu_like_channels(&ChannelsConfig {
            feishu: Some(token_entry.clone()),
            ..Default::default()
        })
        .is_ok());

        let dev_after_token = channel_entry_to_core(
            Some(setup_feishu_security_entry("dev", Some(SETUP_SENSITIVE_VALUE_MARKER), None, None)),
            Some(&token_entry),
        )
        .expect("dev entry");
        assert!(validate_persisted_feishu_like_channels(&ChannelsConfig {
            feishu: Some(dev_after_token),
            ..Default::default()
        })
        .is_ok());
    }

    #[test]
    fn test_saving_feishu_does_not_validate_another_enabled_lark_channel() {
        let channels = ChannelsConfig {
            feishu: Some(existing_feishu_security_entry("dev", None, None)),
            lark: Some(ChannelEntry {
                enabled: true,
                ..Default::default()
            }),
            ..Default::default()
        };

        assert!(validate_persisted_channels_for_save(
            &channels,
            ChannelValidationScope::Current("feishu"),
        )
        .is_ok());

        let error = validate_persisted_channels_for_save(
            &channels,
            ChannelValidationScope::AllEnabled,
        )
        .expect_err("starting must validate every enabled channel");
        assert!(error.contains("Lark"));
        assert!(error.contains("App ID"));
    }

    #[test]
    fn test_disabled_lark_does_not_block_feishu_gateway_validation() {
        let channels = ChannelsConfig {
            feishu: Some(existing_feishu_security_entry("dev", None, None)),
            lark: Some(ChannelEntry {
                enabled: false,
                ..Default::default()
            }),
            ..Default::default()
        };

        assert!(validate_persisted_feishu_like_channels(&channels).is_ok());
    }

    #[test]
    fn test_feishu_missing_app_id_reports_feishu_name() {
        let channels = ChannelsConfig {
            feishu: Some(ChannelEntry {
                enabled: true,
                ..Default::default()
            }),
            ..Default::default()
        };

        let error = validate_persisted_feishu_like_channels(&channels)
            .expect_err("enabled Feishu without an App ID must fail");
        assert!(error.contains("Feishu"));
        assert!(error.contains("App ID"));
    }

    #[test]
    fn test_public_webhook_base_url_does_not_persist_the_webhook_path() {
        let entry = SetupChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            extra: HashMap::from([
                ("app_id".to_string(), serde_json::json!("cli_test")),
                (
                    "public_webhook_base_url".to_string(),
                    serde_json::json!("https://example.test/webhook/feishu"),
                ),
            ]),
            clear_app_secret: false,
        };

        let saved = channel_entry_to_core(Some(entry), None).expect("channel entry");
        assert_eq!(
            saved.extra.get("public_webhook_base_url"),
            Some(&serde_json::json!("https://example.test")),
        );
    }

    #[test]
    fn test_public_webhook_base_url_removes_card_callback_path() {
        let entry = SetupChannelEntry {
            enabled: true,
            token: None,
            token_env: None,
            extra: HashMap::from([
                ("app_id".to_string(), serde_json::json!("cli_test")),
                (
                    "public_webhook_base_url".to_string(),
                    serde_json::json!("https://example.test/webhook/feishu/card/"),
                ),
            ]),
            clear_app_secret: false,
        };

        let saved = channel_entry_to_core(Some(entry), None).expect("channel entry");
        assert_eq!(
            saved.extra.get("public_webhook_base_url"),
            Some(&serde_json::json!("https://example.test")),
        );
    }

    #[tokio::test]
    async fn test_gateway_bind_preflight_reports_occupied_port() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (message, code) = preflight_gateway_bind("127.0.0.1", port)
            .await
            .expect_err("occupied port must fail");

        assert_eq!(code, "port_in_use");
        assert_eq!(
            message,
            format!("Gateway 启动失败：端口 {port} 已被占用。")
        );
    }
}

fn configured_channel_extra(entry: &ChannelEntry, key: &str) -> bool {
    entry
        .extra
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(is_real_sensitive_value)
        .unwrap_or(false)
}

fn validate_channel_outbound(entry: &ChannelEntry, name: &str) -> Result<(), String> {
    let app_id_configured = entry
        .extra
        .get("app_id")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    if !app_id_configured {
        return Err(if name == "Lark" {
            "Lark 已启用但缺少 App ID。请补全 Lark 配置或关闭 Lark。".to_string()
        } else {
            format!("启用 {name} 时必须填写 App ID")
        });
    }

    let outbound_mode = entry
        .extra
        .get("outbound_mode")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("disabled");
    if matches!(outbound_mode, "real" | "mock")
        && !configured_channel_extra(entry, "app_secret")
    {
        return Err(format!("启用 {name} {outbound_mode} outbound 时必须填写 App Secret"));
    }
    Ok(())
}

fn validate_persisted_channel(channels: &ChannelsConfig, channel_id: &str) -> Result<(), String> {
    match channel_id {
        "feishu" => {
            let Some(feishu) = channels.feishu.as_ref().filter(|entry| entry.enabled) else {
                return Ok(());
            };
            validate_channel_outbound(feishu, "Feishu")?;
            let mode = feishu
                .security_mode
                .as_deref()
                .or_else(|| {
                    feishu
                        .extra
                        .get("security_mode")
                        .and_then(serde_json::Value::as_str)
                })
                .unwrap_or("dev");
            match mode {
                "dev" | "" => Ok(()),
                "token" if configured_channel_extra(feishu, "verification_token") => Ok(()),
                "token" => Err("Feishu token 模式必须填写 Verification Token".to_string()),
                "encrypted" if !configured_channel_extra(feishu, "verification_token") => {
                    Err("Feishu encrypted 模式必须填写 Verification Token".to_string())
                }
                "encrypted" if !configured_channel_extra(feishu, "encrypt_key") => {
                    Err("Feishu encrypted 模式必须填写 Encrypt Key".to_string())
                }
                "encrypted" => Ok(()),
                _ => Err("Feishu security_mode 必须是 dev、token 或 encrypted".to_string()),
            }
        }
        "lark" => {
            if let Some(lark) = channels.lark.as_ref().filter(|entry| entry.enabled) {
                validate_channel_outbound(lark, "Lark")?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_persisted_channels_for_save(
    channels: &ChannelsConfig,
    scope: ChannelValidationScope<'_>,
) -> Result<(), String> {
    match scope {
        ChannelValidationScope::Current(channel_id) => {
            validate_persisted_channel(channels, channel_id)
        }
        ChannelValidationScope::AllEnabled => validate_persisted_feishu_like_channels(channels),
    }
}

fn validate_persisted_feishu_like_channels(channels: &ChannelsConfig) -> Result<(), String> {
    validate_persisted_channel(channels, "feishu")?;
    validate_persisted_channel(channels, "lark")
}
