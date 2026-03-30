use omninova_core::channels::{ChannelKind, InboundMessage};
use omninova_core::config::{Config, ModelProviderConfig, ProviderConfig, RobotConfig, ChannelsConfig, ChannelEntry};
use omninova_core::gateway::{
    GatewayHealth, GatewayInboundResponse, GatewayRuntime, GatewaySessionTreeQuery,
    GatewaySessionTreeResponse,
};
use omninova_core::observability::{
    get_memory_stats, clear_cache, CacheConfig, MemoryStats,
};
use omninova_core::providers::{ProviderSelection, build_provider_with_selection};
use omninova_core::routing::RouteDecision;
use omninova_core::skills::{import_skills_from_dir, load_skills_from_dir};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command as StdCommand, Stdio};
use std::sync::Arc;
use std::time::Instant;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIcon, TrayIconBuilder},
    Manager, WindowEvent,
};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::{Duration, sleep};

struct AppState {
    runtime: GatewayRuntime,
    gateway_task: Option<JoinHandle<Result<(), String>>>,
    last_gateway_error: Option<String>,
}

// ============================================================================
// Run Mode Management
// ============================================================================

/// 运行模式枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Copy)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    /// 桌面模式（显示窗口）
    Desktop,
    /// 后台服务模式（最小化到托盘）
    Background,
}

impl Default for RunMode {
    fn default() -> Self {
        Self::Desktop
    }
}

/// 运行模式管理器
pub struct RunModeManager {
    mode: RwLock<RunMode>,
    auto_start: RwLock<bool>,
}

impl RunModeManager {
    pub fn new() -> Self {
        Self {
            mode: RwLock::new(RunMode::Desktop),
            auto_start: RwLock::new(false),
        }
    }

    pub fn get_mode(&self) -> RunMode {
        *self.mode.read()
    }

    pub fn set_mode(&self, mode: RunMode) {
        *self.mode.write() = mode;
    }

    pub fn is_auto_start(&self) -> bool {
        *self.auto_start.read()
    }

    pub fn set_auto_start(&self, enabled: bool) {
        *self.auto_start.write() = enabled;
    }
}

impl Default for RunModeManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Startup Performance Tracking
// ============================================================================

/// 启动里程碑
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupMilestone {
    /// 里程碑名称
    pub name: String,
    /// 相对于启动的时间（秒）
    pub elapsed_seconds: f64,
}

/// 启动报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupReport {
    /// 总启动时间（秒）
    pub total_seconds: f64,
    /// 各阶段里程碑
    pub milestones: Vec<StartupMilestone>,
    /// 是否已完成启动
    pub is_ready: bool,
}

/// 启动性能追踪器
pub struct StartupTracker {
    start_time: Instant,
    milestones: RwLock<Vec<StartupMilestone>>,
    is_ready: RwLock<bool>,
}

impl StartupTracker {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            milestones: RwLock::new(Vec::new()),
            is_ready: RwLock::new(false),
        }
    }

    /// 记录启动里程碑
    pub fn record_milestone(&self, name: &str) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        self.milestones.write().push(StartupMilestone {
            name: name.to_string(),
            elapsed_seconds: elapsed,
        });
        eprintln!("[startup] {} - {:.3}s", name, elapsed);
    }

    /// 标记启动完成
    pub fn mark_ready(&self) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        *self.is_ready.write() = true;
        eprintln!("[startup] Application ready - {:.3}s", elapsed);
    }

    /// 获取启动报告
    pub fn get_report(&self) -> StartupReport {
        StartupReport {
            total_seconds: self.start_time.elapsed().as_secs_f64(),
            milestones: self.milestones.read().clone(),
            is_ready: *self.is_ready.read(),
        }
    }
}

impl Default for StartupTracker {
    fn default() -> Self {
        Self::new()
    }
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
    let mut next = setup_config_to_core(current, config)?;
    let next_gateway_url = format!("http://{}:{}", next.gateway.host, next.gateway.port);

    if let Err(error) = save_config_with_fallback(&mut next) {
        eprintln!("[config warning] {error}");
    }
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
                    api_protocol: None,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillsPackageSummaryPayload {
    dir: String,
    total: usize,
    names: Vec<String>,
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

#[tauri::command]
async fn start_gateway(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<GatewayStatusPayload, String> {
    let state_ref = state.inner().clone();
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

    Ok(SkillsPackageSummaryPayload {
        dir: target.to_string_lossy().into_owned(),
        total: names.len(),
        names,
    })
}

// ============================================================================
// Run Mode Commands
// ============================================================================

/// 获取当前运行模式
#[tauri::command]
fn get_run_mode(manager: tauri::State<'_, RunModeManager>) -> RunMode {
    manager.get_mode()
}

/// 设置运行模式
#[tauri::command]
fn set_run_mode(
    mode: RunMode,
    app: tauri::AppHandle,
    manager: tauri::State<'_, RunModeManager>,
) -> Result<(), String> {
    manager.set_mode(mode);

    match mode {
        RunMode::Desktop => {
            // 显示窗口
            if let Some(window) = app.get_webview_window("main") {
                window.show().map_err(|e| e.to_string())?;
                window.set_focus().map_err(|e| e.to_string())?;
            }
        }
        RunMode::Background => {
            // 隐藏窗口到托盘
            if let Some(window) = app.get_webview_window("main") {
                window.hide().map_err(|e| e.to_string())?;
            }
        }
    }

    Ok(())
}

/// 获取开机自启动状态
#[tauri::command]
fn get_auto_start(manager: tauri::State<'_, RunModeManager>) -> bool {
    manager.is_auto_start()
}

/// 设置开机自启动
#[tauri::command]
async fn set_auto_start(
    enabled: bool,
    manager: tauri::State<'_, RunModeManager>,
) -> Result<(), String> {
    manager.set_auto_start(enabled);

    // 跨平台自启动实现
    #[cfg(target_os = "macos")]
    {
        let app_name = "OmniNova Claw";
        let bundle_id = "com.omninova.claw";
        let launch_agent_path = dirs::home_dir()
            .map(|h| h.join("Library/LaunchAgents").join(format!("{}.plist", bundle_id)));

        if enabled {
            // 创建 LaunchAgent plist
            if let Some(path) = launch_agent_path {
                let executable = std::env::current_exe()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();

                let plist_content = format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>--hidden</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>"#,
                    bundle_id, executable
                );

                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = std::fs::write(&path, plist_content) {
                    eprintln!("[autostart] Failed to create LaunchAgent: {}", e);
                }
            }
        } else {
            // 删除 LaunchAgent
            if let Some(path) = launch_agent_path {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let path = r"Software\Microsoft\Windows\CurrentVersion\Run";

        if let Ok(run_key) = hkcu.open_subkey_with_flags(path, KEY_WRITE) {
            if enabled {
                let executable = std::env::current_exe()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                let _ = run_key.set_value("OmniNovaClaw", &format!("\"{}\" --hidden", executable));
            } else {
                let _ = run_key.delete_value("OmniNovaClaw");
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let desktop_entry_path = dirs::config_dir()
            .map(|c| c.join("autostart").join("omninova-claw.desktop"));

        if enabled {
            if let Some(path) = desktop_entry_path {
                let executable = std::env::current_exe()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();

                let desktop_content = format!(
                    r#"[Desktop Entry]
Type=Application
Name=OmniNova Claw
Exec={} --hidden
Icon=omninova-claw
Comment=AI Agent Platform
Categories=Utility;
Terminal=false
"#,
                    executable
                );

                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&path, desktop_content);
            }
        } else {
            if let Some(path) = desktop_entry_path {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    Ok(())
}

// ============================================================================
// Startup Performance Commands
// ============================================================================

/// 获取启动报告
#[tauri::command]
fn get_startup_report(tracker: tauri::State<'_, StartupTracker>) -> StartupReport {
    tracker.get_report()
}

/// 记录启动里程碑
#[tauri::command]
fn record_startup_milestone(
    name: String,
    tracker: tauri::State<'_, StartupTracker>,
) {
    tracker.record_milestone(&name);
}

// ============================================================================
// Memory Management Commands
// ============================================================================

/// 获取内存使用统计
#[tauri::command]
fn get_app_memory_stats() -> MemoryStats {
    get_memory_stats()
}

/// 清理缓存
#[tauri::command]
fn clear_app_cache() -> Result<u64, String> {
    clear_cache().map_err(|e| e.to_string())
}

/// 获取缓存配置
#[tauri::command]
fn get_cache_config_command() -> CacheConfig {
    omninova_core::observability::memory_monitor().get_config()
}

/// 设置缓存配置
#[tauri::command]
fn set_cache_config_command(config: CacheConfig) -> Result<(), String> {
    omninova_core::observability::memory_monitor().set_config(config);
    Ok(())
}

// ============================================================================
// System Tray Setup
// ============================================================================

/// 设置系统托盘
fn setup_tray(app: &tauri::AppHandle) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    // 创建菜单项
    let show_window = MenuItem::new(app, "显示窗口", true, None::<&str>)?;
    let quit = MenuItem::new(app, "退出", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show_window, &quit])?;

    // 创建托盘图标
    let tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            match event.id.as_ref() {
                "显示窗口" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "退出" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            // 双击托盘图标显示窗口
            if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, button_state: tauri::tray::MouseButtonState::Up, .. } = event {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(tray)
}

/// 设置窗口关闭行为：关闭时最小化到托盘而非退出
fn setup_window_close_behavior(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let app_handle = app.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // 阻止默认关闭行为
                api.prevent_close();
                // 隐藏窗口到托盘
                if let Some(w) = app_handle.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
        });
    }
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

    ensure_desktop_automation_capabilities(&mut current);
    current.validate_or_bail().map_err(|e| e.to_string())?;
    Ok(current)
}

fn config_fallback_candidates(config: &Config) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(parent) = config.workspace_dir.parent() {
        candidates.push(parent.join(".omninova").join("config.toml"));
    }
    candidates.push(config.workspace_dir.join(".omninova").join("config.toml"));
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
    match config.save() {
        Ok(()) => {
            config
                .save_active_workspace()
                .map_err(|e| format!("{:#}", e))?;
            Ok(())
        }
        Err(primary_error) => {
            let original_path = config.config_path.clone();
            let primary_message = format!("{:#}", primary_error);
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

    let auto_approved_tools = ["browser", "shell", "file_read", "file_write", "file_edit"];
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

    changed
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

    let state = Arc::new(Mutex::new(AppState {
        runtime: GatewayRuntime::new(config),
        gateway_task: None,
        last_gateway_error: None,
    }));

    let run_mode_manager = RunModeManager::new();
    let startup_tracker = StartupTracker::new();
    startup_tracker.record_milestone("app_init");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .manage(run_mode_manager)
        .manage(startup_tracker)
        .invoke_handler(tauri::generate_handler![
            process_message,
            get_config,
            save_config,
            reload_config,
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
            skills_package_summary,
            // Run mode commands
            get_run_mode,
            set_run_mode,
            get_auto_start,
            set_auto_start,
            // Startup tracking commands
            get_startup_report,
            record_startup_milestone,
            // Memory management commands
            get_app_memory_stats,
            clear_app_cache,
            get_cache_config_command,
            set_cache_config_command,
        ])
        .setup(|app| {
            // Get startup tracker from state
            let startup_tracker = app.state::<StartupTracker>();
            startup_tracker.record_milestone("setup_start");

            configure_embedded_agent_browser_env(app.handle());
            startup_tracker.record_milestone("browser_env_configured");

            // Setup system tray
            match setup_tray(app.handle()) {
                Ok(_tray) => {
                    eprintln!("[tray] System tray initialized successfully");
                }
                Err(e) => {
                    eprintln!("[tray] Failed to initialize system tray: {}", e);
                }
            }
            startup_tracker.record_milestone("tray_setup_complete");

            // Setup window close behavior (minimize to tray)
            setup_window_close_behavior(app.handle());
            startup_tracker.record_milestone("window_behavior_configured");

            #[cfg(debug_assertions)]
            {
                if let Some(window) = app.get_webview_window("main") {
                    window.open_devtools();
                }
            }

            // Mark startup as ready
            startup_tracker.mark_ready();

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_mode_default() {
        let manager = RunModeManager::new();
        assert_eq!(manager.get_mode(), RunMode::Desktop);
    }

    #[test]
    fn test_run_mode_switch() {
        let manager = RunModeManager::new();
        assert_eq!(manager.get_mode(), RunMode::Desktop);

        manager.set_mode(RunMode::Background);
        assert_eq!(manager.get_mode(), RunMode::Background);

        manager.set_mode(RunMode::Desktop);
        assert_eq!(manager.get_mode(), RunMode::Desktop);
    }

    #[test]
    fn test_auto_start_default() {
        let manager = RunModeManager::new();
        assert!(!manager.is_auto_start());
    }

    #[test]
    fn test_auto_start_toggle() {
        let manager = RunModeManager::new();
        assert!(!manager.is_auto_start());

        manager.set_auto_start(true);
        assert!(manager.is_auto_start());

        manager.set_auto_start(false);
        assert!(!manager.is_auto_start());
    }

    #[test]
    fn test_run_mode_serialization() {
        let mode = RunMode::Desktop;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, r#""desktop""#);

        let mode = RunMode::Background;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, r#""background""#);
    }

    #[test]
    fn test_run_mode_deserialization() {
        let mode: RunMode = serde_json::from_str(r#""desktop""#).unwrap();
        assert_eq!(mode, RunMode::Desktop);

        let mode: RunMode = serde_json::from_str(r#""background""#).unwrap();
        assert_eq!(mode, RunMode::Background);
    }

    // ========================================
    // Startup Tracker Tests
    // ========================================

    #[test]
    fn test_startup_tracker_new() {
        let tracker = StartupTracker::new();
        let report = tracker.get_report();
        assert!(!report.is_ready);
        assert!(report.milestones.is_empty());
        assert!(report.total_seconds >= 0.0);
    }

    #[test]
    fn test_startup_tracker_milestones() {
        let tracker = StartupTracker::new();
        tracker.record_milestone("config_loaded");
        tracker.record_milestone("ui_ready");

        let report = tracker.get_report();
        assert_eq!(report.milestones.len(), 2);
        assert_eq!(report.milestones[0].name, "config_loaded");
        assert_eq!(report.milestones[1].name, "ui_ready");
        assert!(report.milestones[0].elapsed_seconds <= report.milestones[1].elapsed_seconds);
    }

    #[test]
    fn test_startup_tracker_mark_ready() {
        let tracker = StartupTracker::new();
        assert!(!tracker.get_report().is_ready);

        tracker.mark_ready();
        assert!(tracker.get_report().is_ready);
    }

    #[test]
    fn test_startup_report_serialization() {
        let report = StartupReport {
            total_seconds: 1.234,
            milestones: vec![
                StartupMilestone {
                    name: "init".to_string(),
                    elapsed_seconds: 0.5,
                },
            ],
            is_ready: true,
        };

        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"total_seconds\":1.234"));
        assert!(json.contains("\"is_ready\":true"));
        assert!(json.contains("\"name\":\"init\""));
    }
}
