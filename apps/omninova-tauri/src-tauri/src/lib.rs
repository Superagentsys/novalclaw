use omninova_core::config::Config;
use omninova_core::tools::{FileEditTool, FileReadTool, Tool};
use omninova_core::{Agent, MockMemory, MockProvider, OpenAiProvider};
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;

struct AppState {
    config: Config,
}

#[tauri::command]
async fn process_message(
    message: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;
    let cfg = &app_state.config;

    let provider: Box<dyn omninova_core::providers::Provider> = match cfg.api_key.as_deref() {
        Some(key) if !key.trim().is_empty() => {
            let model = cfg
                .default_model
                .clone()
                .unwrap_or_else(|| "gpt-4o-mini".into());
            Box::new(OpenAiProvider::new(
                cfg.api_url.as_deref(),
                Some(key),
                model,
                cfg.default_temperature,
                None,
            ))
        }
        _ => Box::new(MockProvider::new("mock-provider")),
    };

    let memory = Arc::new(MockMemory);
    let workspace_dir = cfg.workspace_dir.clone();
    let tools: Vec<Box<dyn Tool>> = vec![
        Box::new(FileReadTool::new(workspace_dir.clone())),
        Box::new(FileEditTool::new(workspace_dir)),
    ];
    let agent_cfg = cfg.agent.clone();
    let mut agent = Agent::new(provider, tools, memory, agent_cfg);
    agent
        .process_message(&message)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_config(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let app_state = state.lock().await;
    serde_json::to_string_pretty(&app_state.config).map_err(|e| e.to_string())
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

    let mut app_state = state.lock().await;
    let config_path = app_state.config.config_path.clone();
    app_state.config = new_cfg;
    app_state.config.config_path = config_path;

    app_state.config.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn reload_config(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let mut app_state = state.lock().await;
    let cfg = Config::load_or_init().map_err(|e| e.to_string())?;
    app_state.config = cfg;
    serde_json::to_string_pretty(&app_state.config).map_err(|e| e.to_string())
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

    let state = Arc::new(Mutex::new(AppState { config }));

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            process_message,
            get_config,
            save_config,
            reload_config,
        ])
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
