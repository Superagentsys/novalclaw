//! HTTP control plane shared by the Web UI and CLI.
//!
//! The desktop app talks to an in-process runtime through Tauri commands. Web
//! and headless clients hit the same surface over HTTP: a JSON invoke endpoint
//! that mirrors those commands, plus an SSE stream of live agent-run events.

use crate::channels::{ChannelKind, InboundMessage};
use crate::cron::{
    now_timestamp, CronJob, CronRunStore, CronScheduler, CronStore, Schedule,
};
use crate::gateway::{AgentJobExecutor, GatewayRuntime};
use crate::knowledge::{KnowledgeStore, KnowledgeUpsert};
use crate::skills::{
    import_skills_from_dir, list_command_palette, list_skill_catalog, load_skills_from_dir,
    skill_runtime_snapshot, skillhub_categories, skillhub_install, skillhub_list, skillhub_rollback,
};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use futures_util::stream::unfold;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

#[derive(Debug, Deserialize)]
pub struct InvokeRequest {
    pub command: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Serialize)]
pub struct InvokeResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SessionQuery {
    session_id: Option<String>,
    channel: Option<String>,
}

pub fn router() -> Router<GatewayRuntime> {
    Router::new()
        .route("/api/v1/invoke", post(http_invoke))
        .route("/api/v1/events", get(http_events))
        .route("/api", get(http_api_index))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
}

pub fn with_web_ui(router: Router<GatewayRuntime>) -> Router<GatewayRuntime> {
    match discover_web_dir() {
        Some(dir) => {
            let index = dir.join("index.html");
            let service = ServeDir::new(&dir)
                .append_index_html_on_directories(true)
                .fallback(ServeFile::new(index));
            router.nest_service("/app", service)
        }
        None => router.route("/app", get(http_web_missing)),
    }
}

fn discover_web_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("OMNINOVA_WEB_DIR") {
        let path = PathBuf::from(dir);
        if path.join("index.html").is_file() {
            return Some(path);
        }
    }
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let candidates = [
        PathBuf::from("apps/omninova-tauri/dist"),
        PathBuf::from("dist"),
        exe_dir
            .as_ref()
            .map(|dir| dir.join("web"))
            .unwrap_or_else(|| PathBuf::from("web")),
        exe_dir
            .as_ref()
            .map(|dir| dir.join("../Resources/web"))
            .unwrap_or_else(|| PathBuf::from("Resources/web")),
    ];
    candidates
        .into_iter()
        .find(|path| path.join("index.html").is_file())
}

async fn http_api_index() -> Json<Value> {
    Json(json!({
        "service": "OmniNova Gateway",
        "web": "/app",
        "invoke": "POST /api/v1/invoke",
        "events": "GET /api/v1/events",
        "health": "/health",
        "chat": "/chat",
    }))
}

async fn http_web_missing() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        "Web UI is not bundled. Build it with: cd apps/omninova-tauri && npm run build\nThen restart `omninova gateway run` or set OMNINOVA_WEB_DIR.",
    )
}

async fn http_events(
    State(runtime): State<GatewayRuntime>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let rx = runtime.subscribe_events();
    let stream = unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(value) => {
                    let event = Event::default().data(value.to_string());
                    return Some((Ok(event), rx));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return None,
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn http_invoke(
    State(runtime): State<GatewayRuntime>,
    Json(req): Json<InvokeRequest>,
) -> impl IntoResponse {
    match dispatch(&runtime, &req.command, req.args).await {
        Ok(result) => Json(InvokeResponse {
            ok: true,
            result: Some(result),
            error: None,
        })
        .into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(InvokeResponse {
                ok: false,
                result: None,
                error: Some(error),
            }),
        )
            .into_response(),
    }
}

async fn dispatch(runtime: &GatewayRuntime, command: &str, args: Value) -> Result<Value, String> {
    match command {
        "get_config" | "reload_config" => {
            let cfg = runtime.get_config().await;
            serde_json::to_value(&cfg).map_err(|e| e.to_string())
        }
        "get_setup_config" => {
            let cfg = runtime.get_config().await;
            serde_json::to_value(setup_view(&cfg)).map_err(|e| e.to_string())
        }
        "save_config" => {
            let json = args_string(&args, &["configJson", "config_json"])?;
            let next: crate::config::Config =
                serde_json::from_str(&json).map_err(|e| format!("Invalid config JSON: {e}"))?;
            runtime.set_config(next.clone()).await.map_err(|e| e.to_string())?;
            next.save().map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "save_setup_config" => {
            let mut cfg = runtime.get_config().await;
            if let Some(incoming) = args.get("config") {
                merge_setup_into_config(&mut cfg, incoming)?;
            }
            runtime.set_config(cfg.clone()).await.map_err(|e| e.to_string())?;
            cfg.save().map_err(|e| e.to_string())?;
            Ok(json!({ "gateway_restarted": false }))
        }
        "gateway_status" | "gateway_health" => {
            let health = runtime.health().await;
            let cfg = runtime.get_config().await;
            Ok(json!({
                "running": true,
                "url": format!("http://{}:{}", cfg.gateway.host, cfg.gateway.port),
                "gatewayHost": cfg.gateway.host,
                "gatewayPort": cfg.gateway.port,
                "ok": health.ok,
                "provider": health.provider,
                "providerHealthy": health.provider_healthy,
                "memoryHealthy": health.memory_healthy,
            }))
        }
        "start_gateway" | "stop_gateway" | "restart_gateway" => Ok(json!({
            "running": true,
            "note": "web/cli mode: this process is the gateway"
        })),
        "test_gateway_health" => {
            let health = runtime.health().await;
            Ok(json!({
                "ok": health.ok,
                "statusCode": 200,
                "message": "ok"
            }))
        }
        "route_inbound_message" => {
            let inbound = inbound_from_args(&args)?;
            let decision = runtime.route(&inbound).await;
            serde_json::to_value(decision).map_err(|e| e.to_string())
        }
        "process_message" => {
            let text = args_string(&args, &["message", "text"])?;
            runtime.chat(&text).await.map_err(|e| e.to_string()).map(Value::String)
        }
        "process_inbound_message" => {
            let inbound = inbound_from_args(&args)?;
            let resp = runtime.process_inbound(&inbound).await.map_err(|e| e.to_string())?;
            serde_json::to_value(resp).map_err(|e| e.to_string())
        }
        "process_inbound_message_streaming" => {
            stream_inbound(runtime, inbound_from_args(&args)?).await
        }
        "cancel_agent_run" => {
            let run_id = args_string(&args, &["runId", "run_id"])?;
            runtime.cancel_agent_run(&run_id).await.map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "session_tree_snapshot" => {
            let snapshot = runtime
                .session_tree_snapshot()
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(snapshot).map_err(|e| e.to_string())
        }
        "get_chat_session_history" => {
            let query: SessionQuery = serde_json::from_value(args.clone()).unwrap_or(SessionQuery {
                session_id: args_opt_string(&args, &["sessionId", "session_id"]),
                channel: args_opt_string(&args, &["channel"]),
            });
            let session_id = query
                .session_id
                .ok_or_else(|| "session_id is required".to_string())?;
            let channel = parse_channel(query.channel.as_deref());
            let history = runtime.get_session_history(&channel, &session_id).await;
            serde_json::to_value(history).map_err(|e| e.to_string())
        }
        "project_session_context" => {
            let session_id = args_string(&args, &["sessionId", "session_id"])?;
            let channel = parse_channel(args_opt_string(&args, &["channel"]).as_deref());
            let provider = args_opt_string(&args, &["provider"]);
            let model = args_opt_string(&args, &["model"]);
            let projected = runtime
                .project_session_context(&channel, &session_id, provider, model)
                .await;
            serde_json::to_value(projected).map_err(|e| e.to_string())
        }
        "delete_chat_session" => {
            let session_id = args_string(&args, &["sessionId", "session_id"])?;
            let channel = parse_channel(args_opt_string(&args, &["channel"]).as_deref());
            let removed = runtime
                .delete_session(&channel, &session_id)
                .await
                .map_err(|e| e.to_string())?;
            Ok(Value::Bool(removed))
        }
        "automation_list_jobs" => {
            let store = open_cron_store(runtime).await?;
            serde_json::to_value(store.list().await).map_err(|e| e.to_string())
        }
        "automation_upsert_job" => upsert_job(runtime, args.get("input").cloned().unwrap_or(args)).await,
        "automation_delete_job" => {
            let id = args_string(&args, &["id"])?;
            let store = open_cron_store(runtime).await?;
            let removed = store.remove(&id).await.map_err(|e| e.to_string())?;
            if removed {
                let _ = open_cron_runs(runtime).await?.remove_for_job(&id).await;
            }
            Ok(Value::Bool(removed))
        }
        "automation_set_enabled" => {
            let id = args_string(&args, &["id"])?;
            let enabled = args
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| "enabled is required".to_string())?;
            let store = open_cron_store(runtime).await?;
            store
                .set_enabled(&id, enabled)
                .await
                .map_err(|e| e.to_string())?;
            if enabled {
                if let Some(job) = store.get(&id).await {
                    if let Ok(schedule) = Schedule::parse(&job.schedule) {
                        let next = schedule.next_run_iso(job.tz_offset_minutes);
                        let _ = store.set_next_run(&id, next).await;
                    }
                }
            }
            serde_json::to_value(store.get(&id).await).map_err(|e| e.to_string())
        }
        "automation_run_now" => {
            let id = args_string(&args, &["id"])?;
            let store = open_cron_store(runtime).await?;
            let runs = open_cron_runs(runtime).await?;
            let executor = Arc::new(AgentJobExecutor::new(runtime.clone()));
            let run = CronScheduler::new(store, runs, executor)
                .trigger_now(&id)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(run).map_err(|e| e.to_string())
        }
        "automation_list_runs" => {
            let limit = args.get("limit").and_then(Value::as_u64).map(|n| n as usize);
            let runs = open_cron_runs(runtime).await?;
            serde_json::to_value(runs.list(limit).await).map_err(|e| e.to_string())
        }
        "automation_clear_runs" => {
            open_cron_runs(runtime)
                .await?
                .clear()
                .await
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "skills_package_summary" => skills_summary(runtime).await,
        "list_skill_catalog" => {
            let cfg = runtime.get_config().await;
            serde_json::to_value(list_skill_catalog(&cfg)).map_err(|e| e.to_string())
        }
        "list_command_palette" => {
            let cfg = runtime.get_config().await;
            let query = args_opt_string(&args, &["query", "q"]).unwrap_or_default();
            let palette = crate::skills::filter_command_palette(&list_command_palette(&cfg), &query);
            serde_json::to_value(palette).map_err(|e| e.to_string())
        }
        "import_skills" => {
            let from = args_string(&args, &["from", "source"])?;
            let cfg = runtime.get_config().await;
            let target = skills_dir(&cfg);
            let count = import_skills_from_dir(Path::new(&from), &target, true)
                .map_err(|e| e.to_string())?;
            Ok(json!({ "imported": count }))
        }
        "skillhub_browse" => {
            let source = args_opt_string(&args, &["source"]).unwrap_or_else(|| "featured".into());
            let category = args_opt_string(&args, &["category"]);
            let keyword = args_opt_string(&args, &["keyword"]);
            let page = args.get("page").and_then(Value::as_u64).unwrap_or(1) as u32;
            let page_size = args.get("pageSize").and_then(Value::as_u64).unwrap_or(24) as u32;
            let items = skillhub_list(&source, category.as_deref(), keyword.as_deref(), page, page_size)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(items).map_err(|e| e.to_string())
        }
        "skillhub_category_list" => {
            let items = skillhub_categories().await.map_err(|e| e.to_string())?;
            serde_json::to_value(items).map_err(|e| e.to_string())
        }
        "skillhub_install_skill" => {
            let slug = args_string(&args, &["slug"])?;
            let namespace = args_opt_string(&args, &["namespace"]);
            let version = args_opt_string(&args, &["version", "tag"]);
            let cfg = runtime.get_config().await;
            let target = skills_dir(&cfg);
            let (installed_slug, installed) = skillhub_install(
                &target,
                &slug,
                namespace.as_deref(),
                version.as_deref(),
            )
            .await
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "slug": installed_slug,
                "installed": installed,
                "dir": target.to_string_lossy(),
            }))
        }
        "skillhub_rollback_skill" => {
            let slug = args_string(&args, &["slug"])?;
            let cfg = runtime.get_config().await;
            let target = skills_dir(&cfg);
            let (rolled_back_slug, installed) =
                skillhub_rollback(&target, &slug).map_err(|e| e.to_string())?;
            Ok(json!({
                "slug": rolled_back_slug,
                "installed": installed,
                "dir": target.to_string_lossy(),
            }))
        }
        "knowledge_list" => {
            let collection = args_opt_string(&args, &["collection"]);
            let store = open_knowledge_store(runtime).await?;
            serde_json::to_value(store.list(collection.as_deref()).await).map_err(|e| e.to_string())
        }
        "knowledge_collections" => {
            let store = open_knowledge_store(runtime).await?;
            serde_json::to_value(store.collections().await).map_err(|e| e.to_string())
        }
        "knowledge_get" => {
            let id = args_string(&args, &["id"])?;
            let store = open_knowledge_store(runtime).await?;
            match store.get(&id).await.map_err(|e| e.to_string())? {
                Some((doc, content)) => Ok(json!({ "document": doc, "content": content })),
                None => Err(format!("document not found: {id}")),
            }
        }
        "knowledge_upsert" => knowledge_upsert(runtime, args.get("input").cloned().unwrap_or(args)).await,
        "knowledge_import" => knowledge_import(runtime, &args).await,
        "knowledge_delete" => {
            let id = args_string(&args, &["id"])?;
            let store = open_knowledge_store(runtime).await?;
            Ok(Value::Bool(store.remove(&id).await.map_err(|e| e.to_string())?))
        }
        "knowledge_set_enabled" => {
            let id = args_string(&args, &["id"])?;
            let enabled = args
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| "enabled is required".to_string())?;
            let store = open_knowledge_store(runtime).await?;
            serde_json::to_value(store.set_enabled(&id, enabled).await.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())
        }
        "knowledge_search" => {
            let query = args_string(&args, &["query"])?;
            let collection = args_opt_string(&args, &["collection"]);
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(12) as usize;
            let store = open_knowledge_store(runtime).await?;
            serde_json::to_value(store.search(&query, collection.as_deref(), limit).await)
                .map_err(|e| e.to_string())
        }
        "open_workspace_dir" | "open_task_artifact" | "cli_install_status"
        | "cli_install_to_user_path" | "capture_desktop_screenshot"
        | "read_composer_attachments" | "check_browser_dep" | "install_browser_dep"
        | "feishu_diagnostics" | "dingtalk_diagnostics" | "test_dingtalk_public_route"
        | "test_gateway_public_health" => Ok(json!({
            "ok": false,
            "unavailable": true,
            "reason": format!("{command} is desktop-only")
        })),
        "task_artifact_preview" => preview_artifact(runtime, &args).await,
        other => Err(format!("unknown command: {other}")),
    }
}

async fn stream_inbound(runtime: &GatewayRuntime, inbound: InboundMessage) -> Result<Value, String> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
    let bus = runtime.clone();
    let forward = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            bus.publish_event(event);
        }
    });
    let result = runtime
        .process_inbound_streaming(&inbound, tx)
        .await
        .map_err(|e| e.to_string());
    let _ = forward.await;
    serde_json::to_value(result?).map_err(|e| e.to_string())
}

async fn upsert_job(runtime: &GatewayRuntime, input: Value) -> Result<Value, String> {
    let name = input
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let prompt = input
        .get("prompt")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let schedule = input
        .get("schedule")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() || prompt.is_empty() || schedule.is_empty() {
        return Err("name, prompt and schedule are required".into());
    }
    let tz = input
        .get("tzOffsetMinutes")
        .or_else(|| input.get("tz_offset_minutes"))
        .and_then(Value::as_i64)
        .unwrap_or(0) as i32;
    let parsed = Schedule::parse(&schedule).map_err(|e| e.to_string())?;
    let next = parsed.next_run_iso(tz);
    let store = open_cron_store(runtime).await?;
    let existing_id = input
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_string);
    let existing = match &existing_id {
        Some(id) => store.get(id).await,
        None => None,
    };
    let job = CronJob {
        id: existing
            .as_ref()
            .map(|job| job.id.clone())
            .unwrap_or_else(|| format!("job-{}", now_timestamp().replace([':', '.'], "-"))),
        name,
        schedule,
        prompt,
        command: String::new(),
        description: input
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        template_id: input
            .get("templateId")
            .or_else(|| input.get("template_id"))
            .and_then(Value::as_str)
            .map(str::to_string),
        provider: input
            .get("provider")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| existing.as_ref().and_then(|job| job.provider.clone())),
        model: input
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| existing.as_ref().and_then(|job| job.model.clone())),
        tz_offset_minutes: tz,
        enabled: input
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        last_run: existing.as_ref().and_then(|job| job.last_run.clone()),
        last_status: existing.as_ref().and_then(|job| job.last_status),
        next_run: next,
        last_error: existing.as_ref().and_then(|job| job.last_error.clone()),
        created_at: existing
            .as_ref()
            .map(|job| job.created_at.clone())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(now_timestamp),
        task_id: existing.as_ref().and_then(|job| job.task_id.clone()),
    };
    store.upsert(job.clone()).await.map_err(|e| e.to_string())?;
    serde_json::to_value(job).map_err(|e| e.to_string())
}

async fn skills_summary(runtime: &GatewayRuntime) -> Result<Value, String> {
    let cfg = runtime.get_config().await;
    let snapshot = skill_runtime_snapshot(&cfg);
    let dir = snapshot.skills_dir.clone();
    let skills = load_skills_from_dir(&dir).unwrap_or_default();
    Ok(json!({
        "dir": dir.to_string_lossy(),
        "configuredSkillsDir": snapshot.configured_skills_dir.to_string_lossy(),
        "openSkillsEnabled": snapshot.open_skills_enabled,
        "generation": snapshot.generation,
        "total": skills.len(),
        "names": snapshot.loaded_names,
        "items": skills.iter().map(|s| json!({
            "name": s.metadata.name,
            "description": s.metadata.description,
        })).collect::<Vec<_>>(),
        "slugs": snapshot.installed_slugs,
        "runtimeVisibleSlugs": snapshot.runtime_visible_slugs,
    }))
}

fn is_preview_image_extension(extension: &str) -> bool {
    matches!(
        extension,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "tif" | "tiff" | "svg"
    )
}

fn safe_svg_preview_data_url(bytes: &[u8]) -> Result<String, String> {
    let source = std::str::from_utf8(bytes)
        .map_err(|_| "SVG is not valid UTF-8 and cannot be previewed safely".to_string())?;
    let normalized = source.to_ascii_lowercase();
    let unsafe_markers = [
        "<script",
        "javascript:",
        "<foreignobject",
        " onload=",
        " onclick=",
        " onerror=",
        "@import",
        "url(http",
        "href=\"http",
        "href='http",
    ];
    if unsafe_markers.iter().any(|marker| normalized.contains(marker)) {
        return Err("SVG contains active or external content; inline preview was blocked".to_string());
    }
    Ok(format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

async fn preview_artifact(runtime: &GatewayRuntime, args: &Value) -> Result<Value, String> {
    const MAX_IMAGE_BYTES: u64 = 24 * 1024 * 1024;
    const MAX_SVG_BYTES: u64 = 4 * 1024 * 1024;
    let path = args_string(args, &["path"])?;
    let cfg = runtime.get_config().await;
    let workspace = args_opt_string(args, &["workspacePath", "workspace_path"])
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| cfg.workspace_dir.clone());
    let resolved = if Path::new(&path).is_absolute() {
        PathBuf::from(&path)
    } else {
        workspace.join(&path)
    };
    if !resolved.is_file() {
        return Err(format!("file not found: {}", resolved.display()));
    }
    let metadata = tokio::fs::metadata(&resolved).await.map_err(|e| e.to_string())?;
    let name = resolved
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();
    let ext = resolved
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if is_preview_image_extension(&ext) {
        let limit = if ext == "svg" { MAX_SVG_BYTES } else { MAX_IMAGE_BYTES };
        if metadata.len() > limit {
            return Ok(json!({
                "path": resolved,
                "name": name,
                "kind": "image",
                "extension": ext,
                "size": metadata.len(),
                "dataUrl": Value::Null,
                "textPreview": Value::Null,
            }));
        }
        let bytes = tokio::fs::read(&resolved).await.map_err(|e| e.to_string())?;
        let data_url = if ext == "svg" {
            safe_svg_preview_data_url(&bytes)?
        } else {
            let decoded = image::load_from_memory(&bytes)
                .map_err(|error| format!("image preview failed: {error}"))?;
            let thumbnail = decoded.thumbnail(640, 480);
            let mut output = Cursor::new(Vec::new());
            thumbnail
                .write_to(&mut output, image::ImageFormat::Png)
                .map_err(|error| format!("image thumbnail failed: {error}"))?;
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(output.into_inner())
            )
        };
        return Ok(json!({
            "path": resolved,
            "name": name,
            "kind": "image",
            "extension": ext,
            "size": metadata.len(),
            "dataUrl": data_url,
            "textPreview": Value::Null,
        }));
    }
    let bytes = tokio::fs::read(&resolved).await.map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let preview: String = text.chars().take(16_000).collect();
    Ok(json!({
        "path": resolved,
        "name": name,
        "kind": "text",
        "extension": ext,
        "size": bytes.len(),
        "dataUrl": Value::Null,
        "textPreview": preview,
    }))
}

async fn open_cron_store(runtime: &GatewayRuntime) -> Result<CronStore, String> {
    let cfg = runtime.get_config().await;
    CronStore::open(cfg.workspace_dir.join("cron.json"))
        .await
        .map_err(|e| e.to_string())
}

async fn open_cron_runs(runtime: &GatewayRuntime) -> Result<CronRunStore, String> {
    let cfg = runtime.get_config().await;
    CronRunStore::open(cfg.workspace_dir.join("cron_runs.json"))
        .await
        .map_err(|e| e.to_string())
}

async fn open_knowledge_store(runtime: &GatewayRuntime) -> Result<KnowledgeStore, String> {
    let cfg = runtime.get_config().await;
    KnowledgeStore::open_in(&cfg.workspace_dir)
        .await
        .map_err(|e| e.to_string())
}

async fn knowledge_upsert(runtime: &GatewayRuntime, input: Value) -> Result<Value, String> {
    let store = open_knowledge_store(runtime).await?;
    let tags = tags_from_value(&input);
    let doc = store
        .upsert(KnowledgeUpsert {
            id: args_opt_string(&input, &["id"]),
            title: args_opt_string(&input, &["title"]).unwrap_or_default(),
            collection: args_opt_string(&input, &["collection"]).unwrap_or_else(|| "default".into()),
            source: args_opt_string(&input, &["source"]).unwrap_or_else(|| "note".into()),
            source_path: args_opt_string(&input, &["sourcePath", "source_path"]),
            kind: args_opt_string(&input, &["kind"]).unwrap_or_else(|| "md".into()),
            tags,
            content: args_opt_string(&input, &["content"]).unwrap_or_default(),
            enabled: input.get("enabled").and_then(Value::as_bool).unwrap_or(true),
        })
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(doc).map_err(|e| e.to_string())
}

async fn knowledge_import(runtime: &GatewayRuntime, args: &Value) -> Result<Value, String> {
    let store = open_knowledge_store(runtime).await?;
    let collection = args_opt_string(args, &["collection"]);
    let tags = tags_from_value(args);
    let mut imported = Vec::new();
    if let Some(paths) = args.get("paths").and_then(Value::as_array) {
        for path in paths {
            let Some(path) = path.as_str() else { continue };
            let doc = store
                .import_path(Path::new(path), collection.as_deref(), tags.clone())
                .await
                .map_err(|e| format!("{path}: {e}"))?;
            imported.push(doc);
        }
    }
    if let Some(files) = args.get("files").and_then(Value::as_array) {
        for file in files {
            let name = file
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("untitled.md");
            let content = file.get("content").and_then(Value::as_str).unwrap_or("");
            let doc = store
                .import_bytes(name, content.as_bytes(), collection.as_deref(), tags.clone())
                .await
                .map_err(|e| format!("{name}: {e}"))?;
            imported.push(doc);
        }
    }
    if imported.is_empty() {
        return Err("provide paths or files".into());
    }
    serde_json::to_value(imported).map_err(|e| e.to_string())
}

fn tags_from_value(value: &Value) -> Vec<String> {
    if let Some(tags) = value.get("tags").and_then(Value::as_array) {
        return tags
            .iter()
            .filter_map(Value::as_str)
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect();
    }
    args_opt_string(value, &["tags"])
        .unwrap_or_default()
        .split(',')
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect()
}

fn inbound_from_args(args: &Value) -> Result<InboundMessage, String> {
    let payload = args.get("payload").unwrap_or(args);
    let mut metadata: HashMap<String, Value> = payload
        .get("metadata")
        .and_then(Value::as_object)
        .map(|map| map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();
    if let Some(invocations) = payload
        .get("skillInvocations")
        .or_else(|| payload.get("skill_invocations"))
    {
        metadata
            .entry("skill_invocations".to_string())
            .or_insert(invocations.clone());
    }
    Ok(InboundMessage {
        channel: payload
            .get("channel")
            .and_then(Value::as_str)
            .map(parse_channel_str)
            .unwrap_or(ChannelKind::Web),
        user_id: payload
            .get("userId")
            .or_else(|| payload.get("user_id"))
            .and_then(Value::as_str)
            .map(str::to_string),
        session_id: payload
            .get("sessionId")
            .or_else(|| payload.get("session_id"))
            .and_then(Value::as_str)
            .map(str::to_string),
        text: payload
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        metadata,
    })
}

fn setup_view(cfg: &crate::config::Config) -> Value {
    serde_json::to_value(cfg).unwrap_or(Value::Null)
}

fn merge_setup_into_config(
    cfg: &mut crate::config::Config,
    incoming: &Value,
) -> Result<(), String> {
    let mut current = serde_json::to_value(&*cfg).map_err(|e| e.to_string())?;
    if let (Some(cur), Some(inc)) = (current.as_object_mut(), incoming.as_object()) {
        for (key, value) in inc {
            if key == "robot" {
                continue;
            }
            cur.insert(key.clone(), value.clone());
        }
    }
    *cfg = serde_json::from_value(current).map_err(|e| e.to_string())?;
    Ok(())
}

fn args_string(args: &Value, keys: &[&str]) -> Result<String, String> {
    args_opt_string(args, keys).ok_or_else(|| format!("missing {}", keys.join("/")))
}

fn args_opt_string(args: &Value, keys: &[&str]) -> Option<String> {
    let payload = args.get("payload").unwrap_or(args);
    let query = payload.get("query").unwrap_or(payload);
    keys.iter().find_map(|key| {
        payload
            .get(*key)
            .or_else(|| query.get(*key))
            .or_else(|| args.get(*key))
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

fn parse_channel(label: Option<&str>) -> ChannelKind {
    label.map(parse_channel_str).unwrap_or(ChannelKind::Web)
}

fn parse_channel_str(label: &str) -> ChannelKind {
    match label.to_ascii_lowercase().as_str() {
        "cli" => ChannelKind::Cli,
        "web" | "webchat" => ChannelKind::Web,
        "feishu" => ChannelKind::Feishu,
        "dingtalk" => ChannelKind::Dingtalk,
        other => ChannelKind::Other(other.to_string()),
    }
}

fn skills_dir(cfg: &crate::config::Config) -> PathBuf {
    crate::config::resolve_configured_skills_dir(cfg)
}
