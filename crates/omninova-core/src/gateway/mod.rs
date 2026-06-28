pub mod pairing;
pub mod ws;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use crate::channels::adapters::platform_webhook::{
    inbound_from_platform_webhook, verification_response,
};
use crate::channels::adapters::webhook::{WebhookInboundPayload, inbound_from_webhook};
use crate::channels::{ChannelKind, InboundMessage};
use crate::config::{resolve_effective_workspace_dir, Config};
use crate::memory::{Memory, factory::build_memory_from_config};
use crate::providers::ChatMessage;
use crate::providers::{ProviderSelection, build_provider_from_config, build_provider_with_selection};
use crate::routing::{RouteDecision, resolve_agent_route};
use crate::security::{
    ApprovalController, EstopController, EstopState, PendingApproval, SecurityContext,
    is_tool_globally_allowed, resolve_shell_allowlist,
};
use crate::skills::{format_skills_prompt, load_skills_from_dir};
use crate::tools::{
    AgentInvoker, BrowserTool, ContentSearchTool, DelegateRequest, DelegateTool, FileEditTool,
    FileListTool, FileReadTool, FileWriteTool, GitOperationsTool, GlobSearchTool, HttpRequestTool,
    MemoryRecallTool, MemoryStoreTool, PdfReadTool, ShellTool, Tool, WebFetchTool, WebSearchTool,
};
use crate::util::auth::verify_webhook_signature_with_policy_options;
use crate::agent::sanitize_messages_for_provider;
use crate::Agent;
use std::hash::{Hash, Hasher};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use tokio::sync::RwLock;
use tracing::{info, warn};

static SESSION_LOCK_WAIT_EVENTS: AtomicU64 = AtomicU64::new(0);
static SESSION_LOCK_TIMEOUT_EVENTS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct GatewayRuntime {
    config: Arc<RwLock<Config>>,
    pub(crate) memory: Arc<dyn Memory>,
    cron_store: Option<crate::cron::CronStore>,
    webhook_nonces: Arc<RwLock<HashMap<String, i64>>>,
    session_store_guard: Arc<tokio::sync::Mutex<()>>,
    active_inbound: Arc<AtomicUsize>,
    active_children_by_parent: Arc<RwLock<HashMap<String, usize>>>,
    session_tree: Arc<RwLock<HashMap<String, SessionLineageMeta>>>,
}

impl GatewayRuntime {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            memory: Arc::new(crate::InMemoryMemory::new()),
            cron_store: None,
            webhook_nonces: Arc::new(RwLock::new(HashMap::new())),
            session_store_guard: Arc::new(tokio::sync::Mutex::new(())),
            active_inbound: Arc::new(AtomicUsize::new(0)),
            active_children_by_parent: Arc::new(RwLock::new(HashMap::new())),
            session_tree: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_memory(config: Config, memory: Arc<dyn Memory>) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            memory,
            cron_store: None,
            webhook_nonces: Arc::new(RwLock::new(HashMap::new())),
            session_store_guard: Arc::new(tokio::sync::Mutex::new(())),
            active_inbound: Arc::new(AtomicUsize::new(0)),
            active_children_by_parent: Arc::new(RwLock::new(HashMap::new())),
            session_tree: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_cron_store(mut self, store: crate::cron::CronStore) -> Self {
        self.cron_store = Some(store);
        self
    }

    pub async fn health(&self) -> GatewayHealth {
        let cfg = self.config.read().await.clone();
        let provider = build_provider_from_config(&cfg);
        GatewayHealth {
            ok: true,
            provider: provider.name().to_string(),
            provider_healthy: provider.health_check().await,
            memory_healthy: self.memory.health_check().await,
        }
    }

    pub async fn get_config(&self) -> Config {
        self.config.read().await.clone()
    }

    pub async fn set_config(&self, mut config: Config) -> anyhow::Result<()> {
        config.validate_or_bail()?;
        let mut lock = self.config.write().await;
        config.config_path = lock.config_path.clone();
        *lock = config;
        Ok(())
    }

    pub async fn refresh_memory_from_config(&mut self) -> anyhow::Result<()> {
        let cfg = self.config.read().await.clone();
        self.memory = build_memory_from_config(&cfg).await?;
        Ok(())
    }

    pub async fn chat(&self, message: &str) -> anyhow::Result<String> {
        self.ensure_not_stopped().await?;
        let cfg = self.config.read().await.clone();
        let route_agent_name = cfg.agent.name.clone();
        let provider = build_provider_from_config(&cfg);
        let agent_delegate = cfg.agents.get(&route_agent_name);
        let effective_workspace = resolve_effective_workspace_dir(
            None,
            agent_delegate.and_then(|d| d.workspace_dir.as_deref()),
            &cfg.workspace_dir,
        )
        .ok_or_else(|| anyhow::anyhow!("未设置 Workspace 目录，请点击 Workspace 按钮选择一个工作目录"))?;
        let mut tools = create_tools_for_route(
            &cfg,
            &route_agent_name,
            self.memory.clone(),
            &effective_workspace,
        );
        attach_delegate_tool(
            &cfg,
            self,
            &route_agent_name,
            None,
            &ChannelKind::Web,
            0,
            &mut tools,
        );
        let mut agent_cfg = cfg.agent.clone();
        agent_cfg.max_tool_iterations = resolve_agent_max_tool_iterations(&cfg, &route_agent_name);

        if cfg.skills.open_skills_enabled {
            let skills_dir = cfg.skills.open_skills_dir.as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| effective_workspace.join("skills"));
            if let Ok(skills) = load_skills_from_dir(&skills_dir) {
                let prompt = format_skills_prompt(&skills);
                if !prompt.is_empty() {
                    let current = agent_cfg.system_prompt.unwrap_or_default();
                    agent_cfg.system_prompt = Some(format!("{}\n{}", current, prompt));
                }
            }
        }

        let security = SecurityContext::from_config(&cfg);
        let mut agent = Agent::new(provider, tools, self.memory.clone(), agent_cfg, security);
        agent.process_message(message).await
    }

    pub async fn route(&self, inbound: &InboundMessage) -> RouteDecision {
        let cfg = self.config.read().await.clone();
        resolve_agent_route(&cfg, inbound)
    }

    pub async fn process_inbound(&self, inbound: &InboundMessage) -> anyhow::Result<GatewayInboundResponse> {
        let started = std::time::Instant::now();
        self.ensure_not_stopped().await?;
        let cfg = self.config.read().await.clone();
        let route = resolve_agent_route(&cfg, inbound);
        let security = SecurityContext::for_inbound(&cfg, inbound, &route);
        let channel_label = security.audit().context().channel.clone();
        crate::observability::record_inbound_request(&channel_label);
        security
            .audit_inbound_start(inbound.text.chars().count())
            .await;

        let mut steps = vec![ExecutionStep::done(
            "接收请求",
            format!(
                "channel={:?}, session={}, trace={}",
                inbound.channel,
                inbound.session_id.as_deref().unwrap_or("-"),
                security.trace_id()
            ),
        )];

        let result = (async {
        let _slot = acquire_inbound_slot(&cfg, &self.active_inbound)?;
        let _child_slot =
            acquire_subagent_guard(&cfg, inbound, &self.active_children_by_parent).await?;
        steps.push(ExecutionStep::done(
            "路由选择",
            format!(
                "Agent: {}{}{}",
                route.agent_name,
                route.provider.as_ref().map(|p| format!(", Provider: {p}")).unwrap_or_default(),
                route.model.as_ref().map(|m| format!(", Model: {m}")).unwrap_or_default()
            ),
        ));
        security
            .audit_route(&format!(
                "agent={} provider={:?} model={:?}",
                route.agent_name, route.provider, route.model
            ))
            .await;

        // Resolve effective workspace for this request.
        // Priority: session metadata > per-agent workspace > global workspace.
        let agent_delegate = cfg.agents.get(&route.agent_name);
        let session_workspace_path: Option<std::path::PathBuf> = inbound
            .metadata
            .get("workspace_dir")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from);
        let effective_workspace = resolve_effective_workspace_dir(
            session_workspace_path.as_deref(),
            agent_delegate.and_then(|d| d.workspace_dir.as_deref()),
            &cfg.workspace_dir,
        );
        let effective_workspace = match effective_workspace {
            Some(w) if !w.as_os_str().is_empty() => w,
            _ => {
                let message = "未设置 Workspace 目录，请点击 Workspace 按钮选择一个工作目录";
                steps.push(ExecutionStep::error(
                    "Workspace",
                    message,
                ));
                anyhow::bail!(message);
            }
        };
        steps.push(ExecutionStep::done(
            "Workspace",
            format!("{}", effective_workspace.display()),
        ));
        security
            .audit_route(&format!(
                "agent={} workspace={}",
                route.agent_name,
                effective_workspace.display()
            ))
            .await;
        let lineage = self
            .validate_and_resolve_session_lineage(&cfg, inbound, &route.agent_name)
            .await?;
        if let Some(max_depth) = cfg
            .agents
            .get(&route.agent_name)
            .and_then(|delegate| delegate.max_depth)
        {
            if lineage.spawn_depth > max_depth {
                anyhow::bail!(
                    "delegate agent '{}' spawn depth {} exceeds limit {}",
                    route.agent_name,
                    lineage.spawn_depth,
                    max_depth
                );
            }
        }
        let selection = ProviderSelection {
            provider: route.provider.clone(),
            model: route.model.clone(),
        };
        let provider = build_provider_with_selection(&cfg, &selection);
        let mut tools = create_tools_for_route(
            &cfg,
            &route.agent_name,
            self.memory.clone(),
            &effective_workspace,
        );
        if attach_delegate_tool(
            &cfg,
            self,
            &route.agent_name,
            inbound.session_id.as_deref(),
            &inbound.channel,
            lineage.spawn_depth,
            &mut tools,
        ) {
            steps.push(ExecutionStep::done("子代理委派", "已启用 delegate 工具"));
        }
        steps.push(ExecutionStep::done(
            "准备工具",
            format!("可用工具数：{}", tools.len()),
        ));

        let mut agent_cfg = cfg.agent.clone();
        if let Some(delegate) = cfg.agents.get(&route.agent_name) {
            if let Some(prompt) = &delegate.system_prompt {
                agent_cfg.system_prompt = Some(prompt.clone());
            }
        }
        agent_cfg.max_tool_iterations = resolve_agent_max_tool_iterations(&cfg, &route.agent_name);

        // Always tell the agent where its workspace lives so it can answer
        // "where am I working?" and so the LLM has a single source of truth
        // for absolute paths. Path /home or /workspace style guesses should
        // never be used to answer that question.
        {
            let workspace_note = format!(
                "\n[环境信息] 当前 Workspace 目录是：{}。回答“你当前 workspace 在哪里”这类问题时，必须直接引用本路径，不要尝试通过 shell 或 file_read 探测 /workspace、/home、~ 等路径。\n",
                effective_workspace.display()
            );
            let current = agent_cfg.system_prompt.unwrap_or_default();
            agent_cfg.system_prompt = Some(format!("{current}{workspace_note}"));
        }

        if cfg.skills.open_skills_enabled {
            let skills_dir = cfg.skills.open_skills_dir.as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| effective_workspace.join("skills"));
            if let Ok(skills) = load_skills_from_dir(&skills_dir) {
                let prompt = format_skills_prompt(&skills);
                if !prompt.is_empty() {
                    let current = agent_cfg.system_prompt.unwrap_or_default();
                    agent_cfg.system_prompt = Some(format!("{}\n{}", current, prompt));
                    steps.push(ExecutionStep::done("加载技能提示", "已注入 workspace skills"));
                }
            }
        }

        let agent_security = security.clone();
        let mut agent = Agent::new(
            provider,
            tools,
            self.memory.clone(),
            agent_cfg.clone(),
            agent_security,
        );
        if let Some(session_id) = inbound.session_id.as_deref() {
            let _guard = self.session_store_guard.lock().await;
            match load_session_history(&cfg, &inbound.channel, session_id).await {
                Ok(history) if !history.is_empty() => {
                    let sanitized = sanitize_messages_for_provider(history);
                    steps.push(ExecutionStep::done(
                        "加载会话历史",
                        format!("历史消息数：{}", sanitized.len()),
                    ));
                    agent.import_messages(sanitized)
                }
                Ok(_) => steps.push(ExecutionStep::done("加载会话历史", "无历史消息")),
                Err(e) => {
                    steps.push(ExecutionStep::error("加载会话历史", e.to_string()));
                    warn!("failed to load session history for {}: {}", session_id, e)
                }
            }
        }

        let raw_vision_images = collect_desktop_vision_images(&cfg, inbound);
        let vision_images = if provider_supports_openai_vision(route.provider.as_deref()) {
            raw_vision_images.clone()
        } else {
            Vec::new()
        };
        if !raw_vision_images.is_empty() && vision_images.is_empty() {
            steps.push(ExecutionStep::error(
                "桌面视觉",
                "当前 Provider 不支持图像输入，请改用 OpenAI 兼容的视觉模型（如 GPT-4o、DeepSeek-VL、豆包视觉等）",
            ));
        } else if !vision_images.is_empty() {
            steps.push(ExecutionStep::done(
                "桌面视觉",
                format!("已附加 {} 张屏幕截图", vision_images.len()),
            ));
        }

        steps.push(ExecutionStep::running("Agent 执行", "调用模型；如模型请求工具，将继续执行工具循环"));
        let reply = if vision_images.is_empty() {
            agent.process_message(&inbound.text).await?
        } else {
            agent
                .process_message_with_images(&inbound.text, &vision_images)
                .await?
        };
        steps.push(ExecutionStep::done("Agent 执行", "模型返回最终回复"));
        steps.extend(extract_tool_steps(&agent.export_messages()));
        if let Some(session_id) = inbound.session_id.as_deref() {
            let _guard = self.session_store_guard.lock().await;
            let history_messages = agent
                .export_messages()
                .into_iter()
                .map(ChatMessage::strip_images_for_history)
                .collect();
            if let Err(e) = save_session_history(
                &cfg,
                &inbound.channel,
                session_id,
                history_messages,
                agent_cfg.max_history_messages,
                lineage.parent_session_key.clone(),
                lineage.parent_agent_id.clone(),
                route.agent_name.clone(),
                lineage.spawn_depth,
            )
            .await
            {
                steps.push(ExecutionStep::error("保存会话历史", e.to_string()));
                warn!("failed to save session history for {}: {}", session_id, e);
            } else {
                let count = agent.export_messages().len();
                security
                    .audit_session_persisted(session_id, count)
                    .await;
                steps.push(ExecutionStep::done("保存会话历史", session_id.to_string()));
            }
        }
        Ok(GatewayInboundResponse { route, reply, steps })
        })
        .await;

        match &result {
            Ok(resp) => {
                security
                    .audit_inbound_complete(true, &format!("reply_len={}", resp.reply.len()))
                    .await;
                crate::observability::record_inbound_duration(
                    &channel_label,
                    "ok",
                    started.elapsed().as_secs_f64(),
                );
            }
            Err(err) => {
                crate::observability::record_inbound_error("process_inbound");
                security
                    .audit_inbound_complete(false, &err.to_string())
                    .await;
                crate::observability::record_inbound_duration(
                    &channel_label,
                    "error",
                    started.elapsed().as_secs_f64(),
                );
            }
        }
        result
    }

    async fn validate_and_resolve_session_lineage(
        &self,
        cfg: &Config,
        inbound: &InboundMessage,
        route_agent_name: &str,
    ) -> anyhow::Result<SessionLineageMeta> {
        let Some(session_id) = inbound.session_id.as_deref() else {
            return Ok(SessionLineageMeta::default());
        };
        let key = session_key(&inbound.channel, session_id);
        let requested_parent_key = metadata_str(inbound, &["parent_session_id", "parentSessionId"])
            .map(|parent| session_key(&inbound.channel, parent));
        let requested_parent_agent_id =
            metadata_str(inbound, &["parent_agent_id", "parentAgentId"]).map(ToString::to_string);
        if requested_parent_agent_id.is_some() && requested_parent_key.is_none() {
            anyhow::bail!("parentAgentId requires parentSessionId");
        }
        let requested_depth = metadata_u32(inbound, &["spawn_depth", "spawnDepth"]);

        {
            let mut tree = self.session_tree.write().await;
            if let Some(existing) = tree.get_mut(&key) {
                if let Some(parent_key) = requested_parent_key.as_ref() {
                    if existing.parent_session_key.as_ref() != Some(parent_key) {
                        anyhow::bail!("session parent mismatch for '{}'", session_id);
                    }
                }
                if let Some(depth) = requested_depth {
                    if existing.spawn_depth != depth {
                        anyhow::bail!("session depth mismatch for '{}'", session_id);
                    }
                }
                if let Some(parent_agent_id) = requested_parent_agent_id.as_ref() {
                    if existing.parent_agent_id.as_deref() != Some(parent_agent_id.as_str()) {
                        anyhow::bail!("session parent agent mismatch for '{}'", session_id);
                    }
                }
                if existing.agent_name.as_deref() != Some(route_agent_name) {
                    anyhow::bail!("session agent mismatch for '{}'", session_id);
                }
                existing.updated_at = now_unix_ts();
                return Ok(existing.clone());
            }
        }

        if let Some(record) = load_session_record(cfg, &inbound.channel, session_id).await? {
            let resolved = SessionLineageMeta {
                parent_session_key: record.parent_session_key,
                parent_agent_id: record.parent_agent_id,
                agent_name: record.agent_name,
                spawn_depth: record.spawn_depth,
                updated_at: now_unix_ts(),
            };
            if let Some(parent_key) = requested_parent_key.as_ref() {
                if resolved.parent_session_key.as_ref() != Some(parent_key) {
                    anyhow::bail!("session parent mismatch for '{}'", session_id);
                }
            }
            if let Some(depth) = requested_depth {
                if resolved.spawn_depth != depth {
                    anyhow::bail!("session depth mismatch for '{}'", session_id);
                }
            }
            if let Some(parent_agent_id) = requested_parent_agent_id.as_ref() {
                if resolved.parent_agent_id.as_deref() != Some(parent_agent_id.as_str()) {
                    anyhow::bail!("session parent agent mismatch for '{}'", session_id);
                }
            }
            if resolved.agent_name.as_deref() != Some(route_agent_name) {
                anyhow::bail!("session agent mismatch for '{}'", session_id);
            }
            let mut tree = self.session_tree.write().await;
            tree.insert(key, resolved.clone());
            return Ok(resolved);
        }

        let mut resolved_parent_agent_id = requested_parent_agent_id;
        let resolved_parent_key = requested_parent_key;
        let resolved_depth = match resolved_parent_key.as_ref() {
            Some(parent_key) => {
                let parent_meta = self.resolve_parent_lineage(cfg, parent_key).await?;
                if let Some(expected_parent_agent_id) = resolved_parent_agent_id.as_ref() {
                    if parent_meta.agent_name.as_deref() != Some(expected_parent_agent_id.as_str()) {
                        anyhow::bail!(
                            "parentAgentId '{}' does not match parent session agent",
                            expected_parent_agent_id
                        );
                    }
                } else {
                    resolved_parent_agent_id = parent_meta.agent_name.clone();
                }
                let parent_depth = parent_meta.spawn_depth;
                let inferred = parent_depth.saturating_add(1);
                if let Some(depth) = requested_depth {
                    if depth != inferred {
                        anyhow::bail!(
                            "session depth mismatch: expected {}, got {}",
                            inferred,
                            depth
                        );
                    }
                }
                inferred
            }
            None => requested_depth.unwrap_or(0),
        };

        if let Some(max_depth) = cfg
            .agent_defaults_extended
            .subagents
            .as_ref()
            .and_then(|s| s.max_spawn_depth)
        {
            if resolved_depth > max_depth {
                anyhow::bail!(
                    "subagent spawn depth {} exceeds limit {}",
                    resolved_depth,
                    max_depth
                );
            }
        }

        let resolved = SessionLineageMeta {
            parent_session_key: resolved_parent_key,
            parent_agent_id: resolved_parent_agent_id,
            agent_name: Some(route_agent_name.to_string()),
            spawn_depth: resolved_depth,
            updated_at: now_unix_ts(),
        };
        let mut tree = self.session_tree.write().await;
        tree.insert(key, resolved.clone());
        Ok(resolved)
    }

    async fn resolve_parent_lineage(
        &self,
        cfg: &Config,
        parent_key: &str,
    ) -> anyhow::Result<SessionLineageMeta> {
        {
            let tree = self.session_tree.read().await;
            if let Some(node) = tree.get(parent_key) {
                return Ok(node.clone());
            }
        }
        if let Some(record) = load_session_record_by_key(cfg, parent_key).await? {
            return Ok(SessionLineageMeta {
                parent_session_key: record.parent_session_key,
                parent_agent_id: record.parent_agent_id,
                agent_name: record.agent_name,
                spawn_depth: record.spawn_depth,
                updated_at: record.updated_at,
            });
        }
        anyhow::bail!("parent session not found: {}", parent_key)
    }

    pub async fn estop_status(&self) -> anyhow::Result<EstopState> {
        let cfg = self.config.read().await.clone();
        EstopController::from_config(&cfg).load().await
    }

    pub async fn estop_pause(
        &self,
        level: Option<String>,
        domain: Option<String>,
        tool: Option<String>,
        reason: Option<String>,
    ) -> anyhow::Result<EstopState> {
        let cfg = self.config.read().await.clone();
        crate::observability::record_estop_event("pause");
        EstopController::from_config(&cfg)
            .pause(level, domain, tool, reason)
            .await
    }

    pub async fn estop_resume(&self) -> anyhow::Result<EstopState> {
        let cfg = self.config.read().await.clone();
        crate::observability::record_estop_event("resume");
        EstopController::from_config(&cfg).resume().await
    }

    pub async fn list_approvals(&self, pending_only: bool) -> anyhow::Result<Vec<PendingApproval>> {
        let cfg = self.config.read().await.clone();
        ApprovalController::from_workspace(&cfg.workspace_dir)
            .list(pending_only)
            .await
    }

    pub async fn approve_request(
        &self,
        id: &str,
        approved_by: Option<String>,
    ) -> anyhow::Result<PendingApproval> {
        let cfg = self.config.read().await.clone();
        crate::observability::record_approval_event("approved");
        ApprovalController::from_workspace(&cfg.workspace_dir)
            .approve(id, approved_by)
            .await
    }

    pub async fn reject_request(
        &self,
        id: &str,
        reason: Option<String>,
    ) -> anyhow::Result<PendingApproval> {
        let cfg = self.config.read().await.clone();
        crate::observability::record_approval_event("rejected");
        ApprovalController::from_workspace(&cfg.workspace_dir)
            .reject(id, reason)
            .await
    }

    pub async fn session_tree_snapshot(&self) -> anyhow::Result<GatewaySessionTreeResponse> {
        self.session_tree_snapshot_filtered(&GatewaySessionTreeQuery::default())
            .await
    }

    /// 供桌面聊天 UI 展示的会话历史（过滤 system/tool 与纯工具调用轮次）。
    pub async fn get_session_history(
        &self,
        channel: &ChannelKind,
        session_id: &str,
    ) -> GatewaySessionHistoryResponse {
        let cfg = self.config.read().await.clone();
        let messages = load_session_history(&cfg, channel, session_id)
            .await
            .unwrap_or_default();
        let updated_at = load_session_record(&cfg, channel, session_id)
            .await
            .ok()
            .flatten()
            .map(|r| r.updated_at);
        GatewaySessionHistoryResponse {
            session_id: session_id.to_string(),
            channel: channel_label(channel),
            messages: messages_for_chat_ui(&messages),
            updated_at,
        }
    }

    /// 删除某个会话：从内存血缘树与持久化会话存储中移除其记录。
    /// 返回是否确有记录被删除。
    pub async fn delete_session(
        &self,
        channel: &ChannelKind,
        session_id: &str,
    ) -> anyhow::Result<bool> {
        let cfg = self.config.read().await.clone();
        let key = session_key(channel, session_id);

        // 内存血缘树
        {
            let mut tree = self.session_tree.write().await;
            tree.remove(&key);
        }

        // 持久化存储
        let path = session_store_path(&cfg);
        let mut store = load_session_store(&path).await?;
        let removed = store.sessions.remove(&key).is_some();
        if removed {
            let serialized = serde_json::to_string_pretty(&store)?;
            atomic_write_string(&path, &serialized).await?;
        }
        Ok(removed)
    }

    pub async fn session_tree_snapshot_filtered(
        &self,
        query: &GatewaySessionTreeQuery,
    ) -> anyhow::Result<GatewaySessionTreeResponse> {
        let query = normalize_session_tree_query(query);
        if let (Some(min_depth), Some(max_depth)) = (query.min_spawn_depth, query.max_spawn_depth)
        {
            if min_depth > max_depth {
                anyhow::bail!("min_spawn_depth cannot be greater than max_spawn_depth");
            }
        }
        let cfg = self.config.read().await.clone();
        let now = now_unix_ts();
        let path = session_store_path(&cfg);
        let mut merged: HashMap<String, GatewaySessionTreeNode> = HashMap::new();

        let persisted = load_session_store(&path).await?;
        for (session_key, record) in persisted.sessions {
            if now - record.updated_at > cfg.gateway.session_ttl_secs as i64 {
                continue;
            }
            merged.insert(
                session_key,
                GatewaySessionTreeNode {
                    session_key: None,
                    channel: None,
                    session_id: None,
                    parent_session_key: record.parent_session_key,
                    parent_agent_id: record.parent_agent_id,
                    agent_name: record.agent_name,
                    spawn_depth: record.spawn_depth,
                    updated_at: record.updated_at,
                    source: "persisted".to_string(),
                },
            );
        }

        {
            let in_memory = self.session_tree.read().await;
            for (session_key, meta) in in_memory.iter() {
                let source = if merged.contains_key(session_key) {
                    "memory+persisted"
                } else {
                    "memory"
                };
                merged.insert(
                    session_key.clone(),
                    GatewaySessionTreeNode {
                        session_key: None,
                        channel: None,
                        session_id: None,
                        parent_session_key: meta.parent_session_key.clone(),
                        parent_agent_id: meta.parent_agent_id.clone(),
                        agent_name: meta.agent_name.clone(),
                        spawn_depth: meta.spawn_depth,
                        updated_at: meta.updated_at,
                        source: source.to_string(),
                    },
                );
            }
        }

        let mut sessions = merged
            .into_iter()
            .map(|(session_key, mut node)| {
                let (channel, session_id) = split_session_key(&session_key);
                node.session_key = Some(session_key);
                node.channel = channel;
                node.session_id = session_id;
                node
            })
            .collect::<Vec<_>>();
        let total_before_filter = sessions.len();
        sessions.retain(|entry| match_session_tree_filters(entry, &query));
        let total_after_filter = sessions.len();
        let source_counts_after_filter = count_session_sources(&sessions);
        let stats_after_filter = compute_session_tree_stats(&sessions);
        sort_session_tree_entries(&mut sessions, &query);
        let offset = query.offset.unwrap_or(0);
        if offset >= sessions.len() {
            sessions.clear();
        } else if offset > 0 {
            sessions = sessions.split_off(offset);
        }
        if let Some(limit) = query.limit {
            sessions.truncate(limit);
        }
        let returned = sessions.len();
        let has_more = offset.saturating_add(returned) < total_after_filter;
        let next_offset = if has_more {
            Some(offset.saturating_add(returned))
        } else {
            None
        };
        let prev_offset = if offset > 0 {
            Some(offset.saturating_sub(query.limit.unwrap_or(offset)))
        } else {
            None
        };

        let active_children_by_parent = self.active_children_by_parent.read().await.clone();
        Ok(GatewaySessionTreeResponse {
            sessions,
            active_children_by_parent,
            total_before_filter,
            total_after_filter,
            returned,
            offset,
            limit: query.limit,
            has_more,
            next_offset,
            prev_offset,
            next_cursor: next_offset,
            prev_cursor: prev_offset,
            source_counts_after_filter,
            stats_after_filter,
        })
    }

    async fn ensure_not_stopped(&self) -> anyhow::Result<()> {
        let cfg = self.config.read().await.clone();
        let estop = EstopController::from_config(&cfg);
        if estop.is_paused().await? {
            anyhow::bail!("agent is paused by emergency stop");
        }
        Ok(())
    }

    async fn validate_webhook_replay(&self, headers: &HeaderMap) -> anyhow::Result<()> {
        let cfg = self.config.read().await.clone();
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let ts = match headers
            .get("x-omninova-timestamp")
            .and_then(|v| v.to_str().ok())
        {
            Some(raw) => raw
                .parse::<i64>()
                .map_err(|e| anyhow::anyhow!("invalid x-omninova-timestamp header: {e}"))?,
            None => {
                if cfg.gateway.webhook_require_nonce {
                    anyhow::bail!("missing x-omninova-timestamp header")
                }
                return Ok(());
            }
        };

        if (now - ts).abs() > cfg.gateway.webhook_max_skew_secs as i64 {
            anyhow::bail!("webhook timestamp is outside allowed skew window");
        }

        let nonce = match headers
            .get("x-omninova-nonce")
            .and_then(|v| v.to_str().ok())
        {
            Some(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => {
                if cfg.gateway.webhook_require_nonce {
                    anyhow::bail!("missing x-omninova-nonce header")
                }
                return Ok(());
            }
        };

        let cache_key = format!("{nonce}:{ts}");
        let mut cache = self.webhook_nonces.write().await;
        cache.retain(|_, seen_at| now - *seen_at <= cfg.gateway.webhook_nonce_ttl_secs as i64);
        if cache.contains_key(&cache_key) {
            anyhow::bail!("replayed webhook request detected");
        }
        cache.insert(cache_key, now);
        Ok(())
    }

    /// Start an HTTP gateway server with `/`, `/health`, `/chat`, `/config`.
    pub async fn serve_http(mut self) -> anyhow::Result<()> {
        let cfg = self.get_config().await;
        let addr: SocketAddr = format!("{}:{}", cfg.gateway.host, cfg.gateway.port)
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid gateway bind address: {e}"))?;

        if self.cron_store.is_none() {
            let cron_path = cfg.workspace_dir.join("cron.json");
            match crate::cron::CronStore::open(&cron_path).await {
                Ok(store) => self.cron_store = Some(store),
                Err(e) => warn!(
                    "failed to initialize cron store at {}: {e}",
                    cron_path.display()
                ),
            }
        }

        let app = Router::new()
            .route("/", get(http_root))
            .route("/health", get(http_health))
            .route("/chat", post(http_chat))
            .route("/route", post(http_route))
            .route("/ingress", post(http_ingress))
            .route("/webhook", post(http_webhook))
            .route("/webhook/wechat", post(http_wechat_webhook))
            .route("/webhook/feishu", post(http_feishu_webhook))
            .route("/webhook/lark", post(http_lark_webhook))
            .route("/webhook/dingtalk", post(http_dingtalk_webhook))
            .route("/sessions/tree", get(http_sessions_tree))
            .route("/estop/status", get(http_estop_status))
            .route("/estop/pause", post(http_estop_pause))
            .route("/estop/resume", post(http_estop_resume))
            .route("/approvals", get(http_approvals_list))
            .route("/approvals/{id}/approve", post(http_approvals_approve))
            .route("/approvals/{id}/reject", post(http_approvals_reject))
            .route("/config", get(http_get_config).post(http_set_config))
            .route("/api/status", get(http_api_status))
            .route("/api/tools", get(http_api_tools))
            .route("/api/memory", get(http_api_memory_list).post(http_api_memory_store).delete(http_api_memory_forget))
            .route("/api/doctor", get(http_api_doctor))
            .route("/api/cron", get(http_api_cron_list).post(http_api_cron_add))
            .route("/metrics", get(http_metrics))
            .route("/ws/chat", get(ws::ws_chat_handler))
            .with_state(self);

        if cfg.observability.prometheus_enabled {
            let metrics_port = cfg.observability.prometheus_port.unwrap_or(9090);
            let metrics_addr: SocketAddr = format!("{}:{}", cfg.gateway.host, metrics_port)
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid prometheus bind address: {e}"))?;
            let metrics_app = Router::new().route("/metrics", get(http_metrics_standalone));
            tokio::spawn(async move {
                match tokio::net::TcpListener::bind(metrics_addr).await {
                    Ok(listener) => {
                        info!(
                            "prometheus metrics listening on http://{}/metrics",
                            metrics_addr
                        );
                        if let Err(e) = axum::serve(listener, metrics_app).await {
                            warn!("prometheus metrics server stopped: {e}");
                        }
                    }
                    Err(e) => warn!(
                        "failed to bind prometheus metrics on {}: {e}",
                        metrics_addr
                    ),
                }
            });
        }

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;
        Ok(())
    }
}

struct InboundSlotGuard {
    active: Arc<AtomicUsize>,
}

impl Drop for InboundSlotGuard {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::Release);
    }
}

struct ChildSlotGuard {
    parent_agent_id: Option<String>,
    active_children_by_parent: Arc<RwLock<HashMap<String, usize>>>,
}

impl Drop for ChildSlotGuard {
    fn drop(&mut self) {
        let Some(parent_agent_id) = self.parent_agent_id.clone() else {
            return;
        };
        let map = Arc::clone(&self.active_children_by_parent);
        tokio::spawn(async move {
            let mut lock = map.write().await;
            if let Some(count) = lock.get_mut(&parent_agent_id) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    lock.remove(&parent_agent_id);
                }
            }
        });
    }
}

fn acquire_inbound_slot(
    cfg: &Config,
    active: &Arc<AtomicUsize>,
) -> anyhow::Result<Option<InboundSlotGuard>> {
    let limit = cfg
        .agent_defaults_extended
        .max_concurrent
        .or_else(|| {
            cfg.agent_defaults_extended
                .subagents
                .as_ref()
                .and_then(|s| s.max_concurrent)
        });
    let Some(limit) = limit else {
        return Ok(None);
    };
    if limit == 0 {
        return Ok(None);
    }
    let limit = limit as usize;
    loop {
        let current = active.load(Ordering::Acquire);
        if current >= limit {
            anyhow::bail!("too many concurrent inbound requests (limit={limit})");
        }
        if active
            .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return Ok(Some(InboundSlotGuard {
                active: Arc::clone(active),
            }));
        }
    }
}

async fn acquire_subagent_guard(
    cfg: &Config,
    inbound: &InboundMessage,
    active_children_by_parent: &Arc<RwLock<HashMap<String, usize>>>,
) -> anyhow::Result<Option<ChildSlotGuard>> {
    let subagents = match &cfg.agent_defaults_extended.subagents {
        Some(v) => v,
        None => return Ok(None),
    };

    if let Some(max_depth) = subagents.max_spawn_depth {
        let depth = metadata_u32(inbound, &["spawn_depth", "spawnDepth"]).unwrap_or(0);
        if depth > max_depth {
            anyhow::bail!(
                "subagent spawn depth {} exceeds limit {}",
                depth,
                max_depth
            );
        }
    }

    let Some(limit) = subagents.max_children_per_agent else {
        return Ok(None);
    };
    if limit == 0 {
        return Ok(None);
    }
    let Some(parent_agent_id) =
        metadata_str(inbound, &["parent_agent_id", "parentAgentId"]).map(str::to_string)
    else {
        return Ok(None);
    };

    let mut lock = active_children_by_parent.write().await;
    let count = lock.entry(parent_agent_id.clone()).or_insert(0);
    if *count >= limit as usize {
        anyhow::bail!(
            "subagent children limit exceeded for parent '{}' (limit={})",
            parent_agent_id,
            limit
        );
    }
    *count += 1;
    drop(lock);

    Ok(Some(ChildSlotGuard {
        parent_agent_id: Some(parent_agent_id),
        active_children_by_parent: Arc::clone(active_children_by_parent),
    }))
}

fn provider_supports_openai_vision(provider: Option<&str>) -> bool {
    let Some(name) = provider.map(str::to_ascii_lowercase) else {
        return true;
    };
    !matches!(name.as_str(), "anthropic" | "gemini" | "mock")
}

fn metadata_bool(inbound: &InboundMessage, keys: &[&str]) -> bool {
    keys.iter().any(|key| {
        inbound
            .metadata
            .get(*key)
            .and_then(|value| {
                value
                    .as_bool()
                    .or_else(|| value.as_str().map(|s| s == "true" || s == "1"))
            })
            .unwrap_or(false)
    })
}

fn metadata_string_array(inbound: &InboundMessage, keys: &[&str]) -> Vec<String> {
    for key in keys {
        let Some(value) = inbound.metadata.get(*key) else {
            continue;
        };
        if let Some(items) = value.as_array() {
            let urls = items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .filter(|url| !url.is_empty())
                .collect::<Vec<_>>();
            if !urls.is_empty() {
                return urls;
            }
        }
        if let Some(text) = value.as_str() {
            if !text.is_empty() {
                return vec![text.to_string()];
            }
        }
    }
    Vec::new()
}

fn collect_desktop_vision_images(cfg: &Config, inbound: &InboundMessage) -> Vec<String> {
    let requested = metadata_bool(
        inbound,
        &[
            "desktop_vision",
            "desktopVision",
            "include_desktop_vision",
            "includeDesktopVision",
        ],
    );
    if !cfg.multimodal.desktop_vision_enabled && !requested {
        return Vec::new();
    }

    metadata_string_array(
        inbound,
        &[
            "desktop_vision_images",
            "desktopVisionImages",
            "screen_images",
            "screenImages",
        ],
    )
}

fn metadata_u32(inbound: &InboundMessage, keys: &[&str]) -> Option<u32> {
    for key in keys {
        let Some(value) = inbound.metadata.get(*key) else {
            continue;
        };
        if let Some(v) = value.as_u64() {
            return u32::try_from(v).ok();
        }
        if let Some(v) = value.as_str() {
            if let Ok(parsed) = v.parse::<u32>() {
                return Some(parsed);
            }
        }
    }
    None
}

fn metadata_str<'a>(inbound: &'a InboundMessage, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| inbound.metadata.get(*key).and_then(serde_json::Value::as_str))
}

fn extract_tool_steps(messages: &[ChatMessage]) -> Vec<ExecutionStep> {
    let mut steps = Vec::new();
    let mut tool_names_by_id = HashMap::new();

    for message in messages {
        match message.role.as_str() {
            "assistant" => {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&message.content) else {
                    continue;
                };
                let Some(tool_calls) = value.get("tool_calls").and_then(serde_json::Value::as_array) else {
                    continue;
                };
                for call in tool_calls {
                    let id = call.get("id").and_then(serde_json::Value::as_str).unwrap_or("-");
                    let name = call.get("name").and_then(serde_json::Value::as_str).unwrap_or("unknown_tool");
                    let args = call
                        .get("arguments")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("");
                    tool_names_by_id.insert(id.to_string(), name.to_string());
                    steps.push(ExecutionStep::done(
                        format!("调用工具：{name}"),
                        truncate_for_step(args, 240),
                    ));
                }
            }
            "tool" => {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&message.content) else {
                    continue;
                };
                let tool_call_id = value
                    .get("tool_call_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("-");
                let name = tool_names_by_id
                    .get(tool_call_id)
                    .map(String::as_str)
                    .unwrap_or("unknown_tool");
                let content = value
                    .get("content")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                steps.push(ExecutionStep::done(
                    format!("工具完成：{name}"),
                    truncate_for_step(content, 300),
                ));
            }
            _ => {}
        }
    }

    steps
}

fn truncate_for_step(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    let mut out = String::new();
    for (idx, ch) in trimmed.chars().enumerate() {
        if idx >= max_chars {
            out.push('…');
            return out;
        }
        out.push(ch);
    }
    out
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayHealth {
    pub ok: bool,
    pub provider: String,
    pub provider_healthy: bool,
    pub memory_healthy: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayChatRequest {
    pub message: String,
    pub session_id: Option<String>,
    pub user_id: Option<String>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayChatResponse {
    pub reply: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayConfigUpdateResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayRouteRequest {
    pub channel: Option<ChannelKind>,
    pub text: String,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayInboundResponse {
    pub route: RouteDecision,
    pub reply: String,
    #[serde(default)]
    pub steps: Vec<ExecutionStep>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionStep {
    pub title: String,
    pub status: String,
    pub detail: Option<String>,
}

impl ExecutionStep {
    fn done(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            status: "done".to_string(),
            detail: Some(detail.into()),
        }
    }

    fn running(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            status: "running".to_string(),
            detail: Some(detail.into()),
        }
    }

    fn error(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            status: "error".to_string(),
            detail: Some(detail.into()),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewaySessionTreeResponse {
    pub sessions: Vec<GatewaySessionTreeNode>,
    #[serde(default)]
    pub active_children_by_parent: HashMap<String, usize>,
    #[serde(default)]
    pub total_before_filter: usize,
    #[serde(default)]
    pub total_after_filter: usize,
    #[serde(default)]
    pub returned: usize,
    #[serde(default)]
    pub offset: usize,
    pub limit: Option<usize>,
    #[serde(default)]
    pub has_more: bool,
    pub next_offset: Option<usize>,
    pub prev_offset: Option<usize>,
    pub next_cursor: Option<usize>,
    pub prev_cursor: Option<usize>,
    #[serde(default)]
    pub source_counts_after_filter: HashMap<String, usize>,
    pub stats_after_filter: GatewaySessionTreeStats,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct GatewaySessionTreeStats {
    #[serde(default)]
    pub unique_agents: usize,
    #[serde(default)]
    pub unique_parent_agents: usize,
    #[serde(default)]
    pub max_spawn_depth: u32,
    #[serde(default)]
    pub min_updated_at: i64,
    #[serde(default)]
    pub max_updated_at: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct GatewaySessionTreeQuery {
    pub session_id: Option<String>,
    pub session_key: Option<String>,
    pub parent_session_id: Option<String>,
    pub parent_session_key: Option<String>,
    pub agent_name: Option<String>,
    pub parent_agent_id: Option<String>,
    pub channel: Option<String>,
    pub source: Option<String>,
    pub min_spawn_depth: Option<u32>,
    pub max_spawn_depth: Option<u32>,
    pub contains: Option<String>,
    pub case_insensitive: Option<bool>,
    pub cursor: Option<usize>,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
}

/// 聊天界面可展示的一条历史消息。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewaySessionHistoryResponse {
    pub session_id: String,
    pub channel: String,
    pub messages: Vec<GatewayChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewaySessionTreeNode {
    pub session_key: Option<String>,
    pub channel: Option<String>,
    pub session_id: Option<String>,
    pub parent_session_key: Option<String>,
    pub parent_agent_id: Option<String>,
    pub agent_name: Option<String>,
    pub spawn_depth: u32,
    pub updated_at: i64,
    pub source: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct GatewayEstopPauseRequest {
    pub level: Option<String>,
    pub domain: Option<String>,
    pub tool: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct GatewayApprovalActionRequest {
    pub approved_by: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct GatewayApprovalsQuery {
    pub pending_only: Option<bool>,
}

async fn http_root() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "OmniNova Gateway",
        "health": "/health",
        "chat": "/chat",
        "config": "/config",
        "channel_webhooks": {
            "wechat": "/webhook/wechat",
            "feishu": "/webhook/feishu",
            "lark": "/webhook/lark",
            "dingtalk": "/webhook/dingtalk"
        }
    }))
}

async fn http_health(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<GatewayHealth>, Json<GatewayError>> {
    Ok(Json(runtime.health().await))
}

async fn http_chat(
    State(runtime): State<GatewayRuntime>,
    Json(req): Json<GatewayChatRequest>,
) -> Result<Json<GatewayChatResponse>, Json<GatewayError>> {
    let inbound = InboundMessage {
        channel: ChannelKind::Web,
        user_id: req.user_id,
        session_id: req.session_id,
        text: req.message,
        metadata: req.metadata,
    };
    match runtime.process_inbound(&inbound).await {
        Ok(resp) => Ok(Json(GatewayChatResponse { reply: resp.reply })),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_get_config(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<Config>, Json<GatewayError>> {
    Ok(Json(runtime.get_config().await))
}

async fn http_route(
    State(runtime): State<GatewayRuntime>,
    Json(req): Json<GatewayRouteRequest>,
) -> Result<Json<RouteDecision>, Json<GatewayError>> {
    let inbound = InboundMessage {
        channel: req.channel.unwrap_or(ChannelKind::Cli),
        user_id: req.user_id,
        session_id: req.session_id,
        text: req.text,
        metadata: req.metadata,
    };
    Ok(Json(runtime.route(&inbound).await))
}

async fn http_set_config(
    State(runtime): State<GatewayRuntime>,
    Json(config): Json<Config>,
) -> Result<Json<GatewayConfigUpdateResponse>, Json<GatewayError>> {
    match runtime.set_config(config).await {
        Ok(()) => Ok(Json(GatewayConfigUpdateResponse { ok: true })),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_sessions_tree(
    State(runtime): State<GatewayRuntime>,
    Query(query): Query<GatewaySessionTreeQuery>,
) -> Result<Json<GatewaySessionTreeResponse>, Json<GatewayError>> {
    match runtime.session_tree_snapshot_filtered(&query).await {
        Ok(snapshot) => Ok(Json(snapshot)),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_ingress(
    State(runtime): State<GatewayRuntime>,
    Json(req): Json<GatewayRouteRequest>,
) -> Result<Json<GatewayInboundResponse>, Json<GatewayError>> {
    let inbound = InboundMessage {
        channel: req.channel.unwrap_or(ChannelKind::Cli),
        user_id: req.user_id,
        session_id: req.session_id,
        text: req.text,
        metadata: req.metadata,
    };
    match runtime.process_inbound(&inbound).await {
        Ok(resp) => Ok(Json(resp)),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_webhook(
    State(runtime): State<GatewayRuntime>,
    headers: HeaderMap,
    raw_body: String,
) -> Result<Json<GatewayInboundResponse>, Json<GatewayError>> {
    let cfg = runtime.get_config().await;
    if let Some(secret) = webhook_signing_secret(&cfg) {
        let allowed_algorithms = cfg
            .gateway
            .webhook_signature_algorithms
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let priority_algorithms = cfg
            .gateway
            .webhook_signature_priority
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let signature = headers
            .get("x-omninova-signature")
            .or_else(|| headers.get("x-signature"))
            .or_else(|| headers.get("x-hub-signature-256"))
            .and_then(|v| v.to_str().ok());
        let signed_payload = signed_webhook_payload(&cfg, &headers, &raw_body)
            .map_err(|e| Json(GatewayError { message: e.to_string() }))?;
        let verified = verify_webhook_signature_with_policy_options(
            &signed_payload,
            signature,
            &secret,
            &allowed_algorithms,
            &priority_algorithms,
            cfg.gateway.webhook_signature_strict_priority,
        )
            .map_err(|e| Json(GatewayError { message: e.to_string() }))?;
        if !verified {
            return Err(Json(GatewayError {
                message: "invalid webhook signature".to_string(),
            }));
        }
    }
    runtime
        .validate_webhook_replay(&headers)
        .await
        .map_err(|e| Json(GatewayError {
            message: e.to_string(),
        }))?;

    let payload: WebhookInboundPayload = serde_json::from_str(&raw_body).map_err(|e| {
        Json(GatewayError {
            message: format!("invalid webhook payload: {e}"),
        })
    })?;
    let inbound = inbound_from_webhook(payload);
    match runtime.process_inbound(&inbound).await {
        Ok(resp) => Ok(Json(resp)),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_wechat_webhook(
    State(runtime): State<GatewayRuntime>,
    headers: HeaderMap,
    raw_body: String,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    http_channel_webhook(runtime, headers, raw_body, ChannelKind::Wechat).await
}

async fn http_feishu_webhook(
    State(runtime): State<GatewayRuntime>,
    headers: HeaderMap,
    raw_body: String,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    http_channel_webhook(runtime, headers, raw_body, ChannelKind::Feishu).await
}

async fn http_lark_webhook(
    State(runtime): State<GatewayRuntime>,
    headers: HeaderMap,
    raw_body: String,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    http_channel_webhook(runtime, headers, raw_body, ChannelKind::Lark).await
}

async fn http_dingtalk_webhook(
    State(runtime): State<GatewayRuntime>,
    headers: HeaderMap,
    raw_body: String,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    http_channel_webhook(runtime, headers, raw_body, ChannelKind::Dingtalk).await
}

async fn http_channel_webhook(
    runtime: GatewayRuntime,
    headers: HeaderMap,
    raw_body: String,
    channel: ChannelKind,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    let cfg = runtime.get_config().await;
    if let Some(secret) = channel_webhook_signing_secret(&cfg, &channel) {
        let allowed_algorithms = cfg
            .gateway
            .webhook_signature_algorithms
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let priority_algorithms = cfg
            .gateway
            .webhook_signature_priority
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let signature = headers
            .get("x-omninova-signature")
            .or_else(|| headers.get("x-signature"))
            .or_else(|| headers.get("x-hub-signature-256"))
            .and_then(|v| v.to_str().ok());
        let signed_payload = signed_webhook_payload(&cfg, &headers, &raw_body)
            .map_err(|e| Json(GatewayError { message: e.to_string() }))?;
        let verified = verify_webhook_signature_with_policy_options(
            &signed_payload,
            signature,
            &secret,
            &allowed_algorithms,
            &priority_algorithms,
            cfg.gateway.webhook_signature_strict_priority,
        )
        .map_err(|e| Json(GatewayError {
            message: e.to_string(),
        }))?;
        if !verified {
            return Err(Json(GatewayError {
                message: "invalid webhook signature".to_string(),
            }));
        }
    }

    runtime
        .validate_webhook_replay(&headers)
        .await
        .map_err(|e| Json(GatewayError {
            message: e.to_string(),
        }))?;

    let payload: serde_json::Value = serde_json::from_str(&raw_body).map_err(|e| {
        Json(GatewayError {
            message: format!("invalid channel webhook payload: {e}"),
        })
    })?;

    if let Some(challenge) = verification_response(&payload) {
        return Ok(Json(challenge));
    }

    let inbound = inbound_from_platform_webhook(channel, payload).map_err(|e| {
        Json(GatewayError {
            message: e.to_string(),
        })
    })?;
    let response = runtime.process_inbound(&inbound).await.map_err(|e| {
        Json(GatewayError {
            message: e.to_string(),
        })
    })?;
    let value = serde_json::to_value(response).map_err(|e| {
        Json(GatewayError {
            message: e.to_string(),
        })
    })?;
    Ok(Json(value))
}

fn signed_webhook_payload(config: &Config, headers: &HeaderMap, raw_body: &str) -> anyhow::Result<String> {
    if !config.gateway.webhook_signing_include_timestamp {
        return Ok(raw_body.to_string());
    }
    let timestamp = headers
        .get("x-omninova-timestamp")
        .or_else(|| headers.get("x-timestamp"))
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty());
    match timestamp {
        Some(ts) => Ok(format!("{ts}.{raw_body}")),
        None => {
            if config.gateway.webhook_signing_require_timestamp {
                anyhow::bail!("missing timestamp header for webhook signature payload")
            }
            Ok(raw_body.to_string())
        }
    }
}

fn webhook_signing_secret(config: &Config) -> Option<String> {
    let webhook = config.channels_config.webhook.as_ref()?;
    if let Some(secret) = webhook
        .extra
        .get("signing_secret")
        .and_then(serde_json::Value::as_str)
    {
        return Some(secret.to_string());
    }
    if let Some(env_key) = webhook
        .extra
        .get("signing_secret_env")
        .and_then(serde_json::Value::as_str)
    {
        return std::env::var(env_key).ok().filter(|v| !v.trim().is_empty());
    }
    None
}

fn channel_webhook_signing_secret(config: &Config, channel: &ChannelKind) -> Option<String> {
    let entry = match channel {
        ChannelKind::Wechat => config.channels_config.wechat.as_ref(),
        ChannelKind::Feishu => config.channels_config.feishu.as_ref(),
        ChannelKind::Lark => config.channels_config.lark.as_ref(),
        ChannelKind::Dingtalk => config.channels_config.dingtalk.as_ref(),
        _ => None,
    };

    channel_entry_signing_secret(entry).or_else(|| webhook_signing_secret(config))
}

fn channel_entry_signing_secret(
    entry: Option<&crate::config::schema::ChannelEntry>,
) -> Option<String> {
    let entry = entry?;
    if let Some(secret) = entry
        .extra
        .get("signing_secret")
        .and_then(serde_json::Value::as_str)
    {
        return Some(secret.to_string());
    }
    if let Some(env_key) = entry
        .extra
        .get("signing_secret_env")
        .and_then(serde_json::Value::as_str)
    {
        return std::env::var(env_key).ok().filter(|v| !v.trim().is_empty());
    }
    None
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct SessionStoreFile {
    #[serde(default)]
    sessions: HashMap<String, SessionRecord>,
}

fn session_store_path(config: &Config) -> PathBuf {
    config.workspace_dir.join(".omninova-sessions.json")
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct SessionRecord {
    #[serde(default)]
    messages: Vec<ChatMessage>,
    #[serde(default)]
    parent_session_key: Option<String>,
    #[serde(default)]
    parent_agent_id: Option<String>,
    #[serde(default)]
    agent_name: Option<String>,
    #[serde(default)]
    spawn_depth: u32,
    updated_at: i64,
}

#[derive(Debug, Clone, Default)]
struct SessionLineageMeta {
    parent_session_key: Option<String>,
    parent_agent_id: Option<String>,
    agent_name: Option<String>,
    spawn_depth: u32,
    updated_at: i64,
}

fn session_key(channel: &ChannelKind, session_id: &str) -> String {
    format!("{:?}:{session_id}", channel).to_lowercase()
}

fn split_session_key(key: &str) -> (Option<String>, Option<String>) {
    let Some((channel, session_id)) = key.split_once(':') else {
        return (None, Some(key.to_string()));
    };
    (Some(channel.to_string()), Some(session_id.to_string()))
}

fn match_session_tree_filters(
    entry: &GatewaySessionTreeNode,
    query: &GatewaySessionTreeQuery,
) -> bool {
    let case_insensitive = query.case_insensitive.unwrap_or(true);
    let cmp = |left: Option<&str>, right: Option<&str>| -> bool {
        match (left, right) {
            (Some(l), Some(r)) if case_insensitive => l.eq_ignore_ascii_case(r),
            (Some(l), Some(r)) => l == r,
            _ => false,
        }
    };
    if let Some(session_id) = query.session_id.as_deref() {
        if !cmp(entry.session_id.as_deref(), Some(session_id)) {
            return false;
        }
    }
    if let Some(session_key) = query.session_key.as_deref() {
        if !cmp(entry.session_key.as_deref(), Some(session_key)) {
            return false;
        }
    }
    if let Some(parent_session_key) = query.parent_session_key.as_deref() {
        if !cmp(entry.parent_session_key.as_deref(), Some(parent_session_key)) {
            return false;
        }
    }
    if let Some(parent_session_id) = query.parent_session_id.as_deref() {
        let parent_session_id_actual = entry
            .parent_session_key
            .as_deref()
            .and_then(|key| split_session_key(key).1);
        if !cmp(parent_session_id_actual.as_deref(), Some(parent_session_id)) {
            return false;
        }
    }
    if let Some(agent_name) = query.agent_name.as_deref() {
        if !cmp(entry.agent_name.as_deref(), Some(agent_name)) {
            return false;
        }
    }
    if let Some(parent_agent_id) = query.parent_agent_id.as_deref() {
        if !cmp(entry.parent_agent_id.as_deref(), Some(parent_agent_id)) {
            return false;
        }
    }
    if let Some(channel) = query.channel.as_deref() {
        if !cmp(entry.channel.as_deref(), Some(channel)) {
            return false;
        }
    }
    if let Some(source) = query.source.as_deref() {
        if !cmp(Some(entry.source.as_str()), Some(source)) {
            return false;
        }
    }
    if let Some(min_depth) = query.min_spawn_depth {
        if entry.spawn_depth < min_depth {
            return false;
        }
    }
    if let Some(max_depth) = query.max_spawn_depth {
        if entry.spawn_depth > max_depth {
            return false;
        }
    }
    if let Some(contains) = query.contains.as_deref() {
        let hay = format!(
            "{}|{}|{}|{}",
            entry.session_key.clone().unwrap_or_default(),
            entry.session_id.clone().unwrap_or_default(),
            entry.agent_name.clone().unwrap_or_default(),
            entry.parent_session_key.clone().unwrap_or_default()
        );
        let contains_match = if case_insensitive {
            hay.to_lowercase().contains(&contains.to_lowercase())
        } else {
            hay.contains(contains)
        };
        if !contains_match {
            return false;
        }
    }
    true
}

fn sort_session_tree_entries(entries: &mut [GatewaySessionTreeNode], query: &GatewaySessionTreeQuery) {
    let sort_by = query.sort_by.as_deref().unwrap_or("updated_at");
    let asc = query
        .sort_order
        .as_deref()
        .map(|v| v == "asc")
        .unwrap_or(false);
    entries.sort_by(|a, b| {
        let ord = match sort_by {
            "spawn_depth" => a.spawn_depth.cmp(&b.spawn_depth),
            "session_id" => a.session_id.cmp(&b.session_id),
            "agent_name" => a.agent_name.cmp(&b.agent_name),
            _ => a.updated_at.cmp(&b.updated_at),
        };
        if asc { ord } else { ord.reverse() }
    });
}

fn normalize_session_tree_query(query: &GatewaySessionTreeQuery) -> GatewaySessionTreeQuery {
    let mut normalized = query.clone();
    normalized.session_id = normalized.session_id.map(|v| v.trim().to_string());
    normalized.session_key = normalized.session_key.map(|v| v.trim().to_string());
    normalized.parent_session_id = normalized.parent_session_id.map(|v| v.trim().to_string());
    normalized.parent_session_key = normalized.parent_session_key.map(|v| v.trim().to_string());
    normalized.agent_name = normalized.agent_name.map(|v| v.trim().to_string());
    normalized.parent_agent_id = normalized.parent_agent_id.map(|v| v.trim().to_string());
    normalized.channel = normalized.channel.map(|v| v.trim().to_string());
    normalized.source = normalized.source.map(|v| v.trim().to_string());
    normalized.contains = normalized.contains.map(|v| v.trim().to_string());
    normalized.sort_by = normalized
        .sort_by
        .map(|v| v.trim().to_lowercase())
        .filter(|v| matches!(v.as_str(), "updated_at" | "spawn_depth" | "session_id" | "agent_name"));
    normalized.sort_order = normalized
        .sort_order
        .map(|v| v.trim().to_lowercase())
        .filter(|v| matches!(v.as_str(), "asc" | "desc"));
    if normalized.offset.is_none() {
        normalized.offset = normalized.cursor;
    }
    normalized
}

fn count_session_sources(entries: &[GatewaySessionTreeNode]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for entry in entries {
        *counts.entry(entry.source.clone()).or_insert(0) += 1;
    }
    counts
}

fn compute_session_tree_stats(entries: &[GatewaySessionTreeNode]) -> GatewaySessionTreeStats {
    if entries.is_empty() {
        return GatewaySessionTreeStats::default();
    }
    let mut unique_agents = HashSet::new();
    let mut unique_parent_agents = HashSet::new();
    let mut max_spawn_depth = 0u32;
    let mut min_updated_at = i64::MAX;
    let mut max_updated_at = i64::MIN;

    for entry in entries {
        if let Some(agent_name) = entry.agent_name.as_deref() {
            unique_agents.insert(agent_name.to_string());
        }
        if let Some(parent_agent_id) = entry.parent_agent_id.as_deref() {
            unique_parent_agents.insert(parent_agent_id.to_string());
        }
        max_spawn_depth = max_spawn_depth.max(entry.spawn_depth);
        min_updated_at = min_updated_at.min(entry.updated_at);
        max_updated_at = max_updated_at.max(entry.updated_at);
    }

    GatewaySessionTreeStats {
        unique_agents: unique_agents.len(),
        unique_parent_agents: unique_parent_agents.len(),
        max_spawn_depth,
        min_updated_at,
        max_updated_at,
    }
}

fn now_unix_ts() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

fn channel_label(channel: &ChannelKind) -> String {
    match channel {
        ChannelKind::Web => "web".to_string(),
        ChannelKind::WebChat => "webchat".to_string(),
        ChannelKind::Cli => "cli".to_string(),
        other => format!("{:?}", other).to_lowercase(),
    }
}

/// 将网关持久化的 `ChatMessage` 转为 UI 气泡列表。
fn messages_for_chat_ui(messages: &[ChatMessage]) -> Vec<GatewayChatMessage> {
    let mut out = Vec::new();
    for msg in messages {
        match msg.role.as_str() {
            "user" => {
                let text = msg.content.trim();
                if !text.is_empty() {
                    out.push(GatewayChatMessage {
                        role: "user".into(),
                        content: msg.content.clone(),
                        agent: None,
                    });
                }
            }
            "assistant" => {
                if let Some(text) = assistant_text_for_ui(&msg.content) {
                    out.push(GatewayChatMessage {
                        role: "assistant".into(),
                        content: text,
                        agent: None,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

fn assistant_text_for_ui(content: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(content) {
        let has_tool_calls = value
            .get("tool_calls")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|a| !a.is_empty());
        if let Some(text) = value
            .get("content")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            return Some(text.to_string());
        }
        if has_tool_calls {
            return None;
        }
    }
    let trimmed = content.trim();
    if trimmed.is_empty() || trimmed.starts_with('{') {
        return None;
    }
    Some(trimmed.to_string())
}

async fn load_session_history(
    config: &Config,
    channel: &ChannelKind,
    session_id: &str,
) -> anyhow::Result<Vec<ChatMessage>> {
    let path = session_store_path(config);
    let store = load_session_store(&path).await?;
    let key = session_key(channel, session_id);
    let Some(record) = store.sessions.get(&key) else {
        return Ok(Vec::new());
    };
    let age = now_unix_ts() - record.updated_at;
    if age > config.gateway.session_ttl_secs as i64 {
        return Ok(Vec::new());
    }
    Ok(record.messages.clone())
}

async fn save_session_history(
    config: &Config,
    channel: &ChannelKind,
    session_id: &str,
    mut messages: Vec<ChatMessage>,
    max_history_messages: usize,
    parent_session_key: Option<String>,
    parent_agent_id: Option<String>,
    agent_name: String,
    spawn_depth: u32,
) -> anyhow::Result<()> {
    if max_history_messages > 0 && messages.len() > max_history_messages {
        let start = messages.len() - max_history_messages;
        messages = messages.split_off(start);
    }
    messages = sanitize_messages_for_provider(messages);

    let path = session_store_path(config);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut store = load_session_store(&path).await?;
    let now = now_unix_ts();
    store
        .sessions
        .retain(|_, record| now - record.updated_at <= config.gateway.session_ttl_secs as i64);

    let key = session_key(channel, session_id);
    store.sessions.insert(
        key,
        SessionRecord {
            messages,
            parent_session_key,
            parent_agent_id,
            agent_name: Some(agent_name),
            spawn_depth,
            updated_at: now,
        },
    );

    if store.sessions.len() > config.gateway.max_sessions {
        let mut entries: Vec<(String, SessionRecord)> = store.sessions.into_iter().collect();
        entries.sort_by(|a, b| b.1.updated_at.cmp(&a.1.updated_at));
        entries.truncate(config.gateway.max_sessions);
        store.sessions = entries.into_iter().collect();
    }

    let serialized = serde_json::to_string_pretty(&store)?;
    atomic_write_string(&path, &serialized).await?;
    Ok(())
}

async fn load_session_record(
    config: &Config,
    channel: &ChannelKind,
    session_id: &str,
) -> anyhow::Result<Option<SessionRecord>> {
    let key = session_key(channel, session_id);
    load_session_record_by_key(config, &key).await
}

async fn load_session_record_by_key(
    config: &Config,
    key: &str,
) -> anyhow::Result<Option<SessionRecord>> {
    let path = session_store_path(config);
    let store = load_session_store(&path).await?;
    Ok(store.sessions.get(key).cloned())
}

async fn load_session_store(path: &PathBuf) -> anyhow::Result<SessionStoreFile> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let _guard = acquire_lockfile_guard(path, 5_000, 60_000).await?;
    if !path.exists() {
        return Ok(SessionStoreFile::default());
    }
    let raw = tokio::fs::read_to_string(path).await.unwrap_or_default();
    match serde_json::from_str::<SessionStoreFile>(&raw) {
        Ok(v) => Ok(v),
        Err(e) => {
            let corrupt_path = path.with_extension(format!("corrupt.{}.json", now_unix_ts()));
            let _ = tokio::fs::rename(path, &corrupt_path).await;
            warn!(
                "session store corrupted (moved to {}): {}",
                corrupt_path.display(),
                e
            );
            Ok(SessionStoreFile::default())
        }
    }
}

async fn atomic_write_string(path: &PathBuf, content: &str) -> anyhow::Result<()> {
    let _guard = acquire_lockfile_guard(path, 5_000, 60_000).await?;
    let tmp = path.with_extension(format!("tmp.{}", now_unix_ts()));
    tokio::fs::write(&tmp, content).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

struct LockfileGuard {
    path: PathBuf,
}

impl Drop for LockfileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

async fn acquire_lockfile_guard(
    target: &PathBuf,
    timeout_ms: u64,
    stale_lock_ms: u64,
) -> anyhow::Result<LockfileGuard> {
    let lock_path = resolve_session_lock_path(target);
    let wait_started = std::time::Instant::now();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let mut retries: u32 = 0;

    loop {
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&lock_path)
        {
            Ok(_) => {
                let waited_ms = wait_started.elapsed().as_millis() as u64;
                if waited_ms >= 50 {
                    let events = SESSION_LOCK_WAIT_EVENTS.fetch_add(1, Ordering::Relaxed) + 1;
                    warn!(
                        "session lock contention: target={}, waited_ms={}, retries={}, total_events={}",
                        target.display(),
                        waited_ms,
                        retries,
                        events
                    );
                }
                return Ok(LockfileGuard { path: lock_path });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                retries = retries.saturating_add(1);
                if let Ok(meta) = std::fs::metadata(&lock_path) {
                    if let Ok(modified) = meta.modified() {
                        if let Ok(elapsed) = modified.elapsed() {
                            if elapsed > std::time::Duration::from_millis(stale_lock_ms) {
                                let _ = std::fs::remove_file(&lock_path);
                            }
                        }
                    }
                }
                if std::time::Instant::now() >= deadline {
                    let timeout_events = SESSION_LOCK_TIMEOUT_EVENTS.fetch_add(1, Ordering::Relaxed) + 1;
                    warn!(
                        "session lock timeout: target={}, retries={}, total_timeouts={}",
                        target.display(),
                        retries,
                        timeout_events
                    );
                    anyhow::bail!("timed out waiting for session store lock");
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            Err(e) => return Err(anyhow::anyhow!("failed to acquire lock: {e}")),
        }
    }
}

fn resolve_session_lock_path(target: &PathBuf) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    target.hash(&mut hasher);
    let hash = hasher.finish();
    let lock_name = format!("session_{hash:016x}.lock");

    let candidates = [
        std::env::var("OMNINOVA_LOCK_DIR").ok().map(PathBuf::from),
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".omninova").join("locks")),
        Some(std::env::temp_dir().join("omninova-locks")),
    ];

    for candidate in candidates.into_iter().flatten() {
        if std::fs::create_dir_all(&candidate).is_ok() {
            return candidate.join(&lock_name);
        }
    }

    target.with_extension("lock")
}

async fn http_estop_status(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<EstopState>, Json<GatewayError>> {
    match runtime.estop_status().await {
        Ok(state) => Ok(Json(state)),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_estop_pause(
    State(runtime): State<GatewayRuntime>,
    Json(req): Json<GatewayEstopPauseRequest>,
) -> Result<Json<EstopState>, Json<GatewayError>> {
    match runtime
        .estop_pause(req.level, req.domain, req.tool, req.reason)
        .await
    {
        Ok(state) => Ok(Json(state)),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_estop_resume(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<EstopState>, Json<GatewayError>> {
    match runtime.estop_resume().await {
        Ok(state) => Ok(Json(state)),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_approvals_list(
    State(runtime): State<GatewayRuntime>,
    Query(query): Query<GatewayApprovalsQuery>,
) -> Result<Json<Vec<PendingApproval>>, Json<GatewayError>> {
    match runtime.list_approvals(query.pending_only.unwrap_or(true)).await {
        Ok(items) => Ok(Json(items)),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_approvals_approve(
    State(runtime): State<GatewayRuntime>,
    Path(id): Path<String>,
    Json(req): Json<GatewayApprovalActionRequest>,
) -> Result<Json<PendingApproval>, Json<GatewayError>> {
    match runtime.approve_request(&id, req.approved_by).await {
        Ok(item) => Ok(Json(item)),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_approvals_reject(
    State(runtime): State<GatewayRuntime>,
    Path(id): Path<String>,
    Json(req): Json<GatewayApprovalActionRequest>,
) -> Result<Json<PendingApproval>, Json<GatewayError>> {
    match runtime.reject_request(&id, req.reason).await {
        Ok(item) => Ok(Json(item)),
        Err(e) => Err(Json(GatewayError {
            message: e.to_string(),
        })),
    }
}

async fn http_api_status(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    let health = runtime.health().await;
    let cfg = runtime.get_config().await;
    let tools = create_default_tools(&cfg);
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    Ok(Json(serde_json::json!({
        "gateway": {
            "ok": health.ok,
            "provider": health.provider,
            "provider_healthy": health.provider_healthy,
            "memory_healthy": health.memory_healthy,
        },
        "config": {
            "default_provider": cfg.default_provider,
            "default_model": cfg.default_model,
            "gateway_host": cfg.gateway.host,
            "gateway_port": cfg.gateway.port,
            "agent_name": cfg.agent.name,
        },
        "tools": tool_names,
        "agents": cfg.agents.keys().collect::<Vec<_>>(),
    })))
}

async fn http_api_tools(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    let cfg = runtime.get_config().await;
    let tools = create_default_tools(&cfg);
    let specs: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name(),
                "description": t.description(),
                "parameters": t.parameters_schema(),
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "tools": specs })))
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ApiMemoryStoreRequest {
    key: String,
    content: String,
    category: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ApiMemoryForgetRequest {
    key: String,
}

async fn http_api_memory_list(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    let entries = runtime.memory.list(None, None).await.map_err(|e| {
        Json(GatewayError {
            message: e.to_string(),
        })
    })?;
    let items: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "key": e.key,
                "content": e.content,
                "category": format!("{:?}", e.category),
                "timestamp": e.timestamp,
            })
        })
        .collect();
    Ok(Json(
        serde_json::json!({ "entries": items, "count": items.len() }),
    ))
}

async fn http_api_memory_store(
    State(runtime): State<GatewayRuntime>,
    Json(req): Json<ApiMemoryStoreRequest>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    use crate::memory::MemoryCategory;
    let category = match req.category.as_deref() {
        Some("daily") => MemoryCategory::Daily,
        Some("conversation") => MemoryCategory::Conversation,
        _ => MemoryCategory::Core,
    };
    runtime
        .memory
        .store(&req.key, &req.content, category, None)
        .await
        .map_err(|e| Json(GatewayError {
            message: e.to_string(),
        }))?;
    Ok(Json(serde_json::json!({ "ok": true, "key": req.key })))
}

async fn http_api_memory_forget(
    State(runtime): State<GatewayRuntime>,
    Json(req): Json<ApiMemoryForgetRequest>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    let removed = runtime.memory.forget(&req.key).await.map_err(|e| {
        Json(GatewayError {
            message: e.to_string(),
        })
    })?;
    Ok(Json(
        serde_json::json!({ "ok": true, "key": req.key, "removed": removed }),
    ))
}

async fn http_api_doctor(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    let health = runtime.health().await;
    let cfg = runtime.get_config().await;
    let estop = runtime.estop_status().await.ok();
    let session_tree = runtime.session_tree_snapshot().await.ok();
    let memory_count = runtime.memory.count().await.unwrap_or(0);

    let mut checks = Vec::new();
    checks.push(serde_json::json!({
        "check": "provider_health",
        "ok": health.provider_healthy,
        "detail": health.provider,
    }));
    checks.push(serde_json::json!({
        "check": "memory_health",
        "ok": health.memory_healthy,
        "detail": format!("{memory_count} entries"),
    }));
    checks.push(serde_json::json!({
        "check": "estop",
        "ok": estop.as_ref().map(|s| !s.paused).unwrap_or(true),
        "detail": estop.map(|s| if s.paused { "PAUSED" } else { "active" }.to_string()),
    }));
    checks.push(serde_json::json!({
        "check": "sessions",
        "ok": true,
        "detail": format!("{} active sessions", session_tree.map(|t| t.total_before_filter).unwrap_or(0)),
    }));
    checks.push(serde_json::json!({
        "check": "config",
        "ok": cfg.validate().is_ok(),
        "detail": format!("provider={}, model={}", cfg.default_provider.as_deref().unwrap_or("-"), cfg.default_model.as_deref().unwrap_or("-")),
    }));

    let all_ok = checks.iter().all(|c| c["ok"].as_bool().unwrap_or(false));
    Ok(Json(serde_json::json!({
        "ok": all_ok,
        "checks": checks,
        "penetration_assessment": crate::security::penetration_playbook::build_playbook_payload(),
    })))
}

async fn http_api_cron_list(
    State(runtime): State<GatewayRuntime>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    let Some(store) = &runtime.cron_store else {
        return Ok(Json(serde_json::json!({ "jobs": [], "note": "cron store not initialized" })));
    };
    let jobs = store.list();
    let items: Vec<serde_json::Value> = jobs
        .iter()
        .map(|j| {
            serde_json::json!({
                "id": j.id,
                "name": j.name,
                "schedule": j.schedule,
                "command": j.command,
                "enabled": j.enabled,
                "last_run": j.last_run,
                "last_status": j.last_status,
                "next_run": j.next_run,
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "jobs": items })))
}

#[derive(Debug, serde::Deserialize)]
struct ApiCronAddRequest {
    name: String,
    schedule: String,
    command: String,
}

async fn http_api_cron_add(
    State(runtime): State<GatewayRuntime>,
    Json(req): Json<ApiCronAddRequest>,
) -> Result<Json<serde_json::Value>, Json<GatewayError>> {
    let Some(store) = &runtime.cron_store else {
        return Err(Json(GatewayError {
            message: "cron store not initialized".to_string(),
        }));
    };
    let job = crate::cron::CronJob {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name,
        schedule: req.schedule,
        command: req.command,
        enabled: true,
        last_run: None,
        last_status: None,
        next_run: None,
        created_at: time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
    };
    let id = job.id.clone();
    store.add(job).await.map_err(|e| {
        Json(GatewayError {
            message: e.to_string(),
        })
    })?;
    Ok(Json(serde_json::json!({ "ok": true, "id": id })))
}

async fn http_metrics(
    State(runtime): State<GatewayRuntime>,
) -> Result<(StatusCode, String), StatusCode> {
    let cfg = runtime.get_config().await;
    if !cfg.observability.prometheus_enabled {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok((StatusCode::OK, crate::observability::encode_metrics()))
}

async fn http_metrics_standalone() -> (StatusCode, String) {
    (StatusCode::OK, crate::observability::encode_metrics())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayError {
    pub message: String,
}

pub fn create_default_tools(config: &Config) -> Vec<Box<dyn Tool>> {
    create_workspace_tools(&config.workspace_dir, config)
}

/// Build all workspace-scoped tools with the given effective workspace root.
pub fn create_workspace_tools(
    effective_workspace: &PathBuf,
    config: &Config,
) -> Vec<Box<dyn Tool>> {
    let workspace = effective_workspace.clone();
    let shell_allowlist = resolve_shell_allowlist(config);
    vec![
        Box::new(FileReadTool::new(workspace.clone())),
        Box::new(FileWriteTool::new(workspace.clone())),
        Box::new(FileEditTool::new(workspace.clone())),
        Box::new(FileListTool::new(workspace.clone())),
        Box::new(GlobSearchTool::new(workspace.clone())),
        Box::new(ContentSearchTool::new(workspace.clone())),
        Box::new(GitOperationsTool::new(workspace.clone())),
        Box::new(ShellTool::new(
            workspace.clone(),
            shell_allowlist,
            Some(30),
            config.clone(),
        )),
        Box::new(PdfReadTool::new(workspace)),
    ]
}

pub fn create_all_tools(config: &Config, memory: Arc<dyn Memory>) -> Vec<Box<dyn Tool>> {
    let mut tools = create_default_tools(config);

    if config.http_request.enabled {
        tools.push(Box::new(HttpRequestTool::new(
            config.http_request.allowed_domains.clone(),
        )));
    }

    if config.web_fetch.enabled {
        tools.push(Box::new(WebFetchTool::new(
            config.web_fetch.allowed_domains.clone(),
        )));
    }

    if config.web_search.enabled {
        if let Some(key) = &config.web_search.brave_api_key {
            tools.push(Box::new(WebSearchTool::new(key.clone())));
        }
    }

    if config.browser.enabled {
        tools.push(Box::new(BrowserTool::new(
            config.browser.allowed_domains.clone(),
            config.browser.native_headless,
            config.browser.attach_only,
            config.browser.cdp_url.clone(),
        )));
    }

    tools.push(Box::new(MemoryStoreTool::new(memory.clone())));
    tools.push(Box::new(MemoryRecallTool::new(memory)));

    tools
        .into_iter()
        .filter(|tool| is_tool_globally_allowed(config, tool.name()))
        .collect()
}

#[async_trait::async_trait]
impl AgentInvoker for GatewayRuntime {
    /// Run a delegated subtask on another configured agent. The child request
    /// goes through the full `process_inbound` pipeline, so routing, security,
    /// audit, lineage tracking and concurrency limits all apply unchanged.
    async fn invoke_agent(&self, request: DelegateRequest) -> anyhow::Result<String> {
        let cfg = self.config.read().await.clone();
        if !cfg.agents.contains_key(&request.agent) {
            anyhow::bail!("delegate target '{}' is not configured", request.agent);
        }

        let child_session_id = format!("subagent-{}", uuid::Uuid::new_v4());
        let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();
        metadata.insert("agent".into(), serde_json::json!(request.agent));
        metadata.insert(
            "spawn_depth".into(),
            serde_json::json!(request.child_depth),
        );
        // Parent lineage requires a registered parent session; sessionless
        // parents still propagate depth so spawn limits hold.
        if let Some(parent_session_id) = &request.parent_session_id {
            metadata.insert(
                "parent_session_id".into(),
                serde_json::json!(parent_session_id),
            );
            metadata.insert(
                "parent_agent_id".into(),
                serde_json::json!(request.parent_agent),
            );
        }

        let text = match &request.context {
            Some(context) => format!(
                "{}\n\n[Context from delegating agent '{}']\n{}",
                request.task, request.parent_agent, context
            ),
            None => request.task.clone(),
        };
        let inbound = InboundMessage {
            channel: request.channel.clone(),
            user_id: None,
            session_id: Some(child_session_id),
            text,
            metadata,
        };

        let timeout_secs = cfg
            .agent_defaults_extended
            .subagents
            .as_ref()
            .and_then(|s| s.run_timeout_seconds)
            .filter(|secs| *secs > 0);
        let response = match timeout_secs {
            Some(secs) => tokio::time::timeout(
                std::time::Duration::from_secs(secs as u64),
                self.process_inbound(&inbound),
            )
            .await
            .map_err(|_| {
                anyhow::anyhow!("subagent '{}' timed out after {}s", request.agent, secs)
            })??,
            None => self.process_inbound(&inbound).await?,
        };
        Ok(response.reply)
    }
}

/// Attach the `delegate` tool when the current agent is allowed to spawn
/// subagents. Returns whether the tool was attached.
fn attach_delegate_tool(
    cfg: &Config,
    runtime: &GatewayRuntime,
    route_agent_name: &str,
    parent_session_id: Option<&str>,
    channel: &ChannelKind,
    parent_depth: u32,
    tools: &mut Vec<Box<dyn Tool>>,
) -> bool {
    let mut targets: Vec<String> = cfg
        .agents
        .keys()
        .filter(|name| name.as_str() != route_agent_name)
        .cloned()
        .collect();
    if targets.is_empty() {
        return false;
    }
    if !is_tool_globally_allowed(cfg, "delegate") {
        return false;
    }
    // Delegate agents with an explicit tool allowlist must opt in.
    if let Some(delegate_cfg) = cfg.agents.get(route_agent_name) {
        if !delegate_cfg.allowed_tools.is_empty()
            && !delegate_cfg
                .allowed_tools
                .iter()
                .any(|t| t.eq_ignore_ascii_case("delegate"))
        {
            return false;
        }
    }
    let child_depth = parent_depth.saturating_add(1);
    if let Some(max_depth) = cfg
        .agent_defaults_extended
        .subagents
        .as_ref()
        .and_then(|s| s.max_spawn_depth)
    {
        if child_depth > max_depth {
            return false;
        }
    }
    targets.sort();
    tools.push(Box::new(DelegateTool::new(
        Arc::new(runtime.clone()),
        targets,
        route_agent_name.to_string(),
        parent_session_id.map(ToString::to_string),
        channel.clone(),
        child_depth,
    )));
    true
}

fn create_tools_for_route(
    config: &Config,
    route_agent_name: &str,
    _memory: Arc<dyn Memory>,
    effective_workspace: &PathBuf,
) -> Vec<Box<dyn Tool>> {
    let tools = create_workspace_tools(effective_workspace, config);
    let Some(delegate) = config.agents.get(route_agent_name) else {
        return tools;
    };
    if delegate.allowed_tools.is_empty() {
        return tools;
    }
    let allowed: HashSet<&str> = delegate.allowed_tools.iter().map(String::as_str).collect();
    tools
        .into_iter()
        .filter(|tool| allowed.contains(tool.name()))
        .collect()
}

fn resolve_agent_max_tool_iterations(config: &Config, route_agent_name: &str) -> usize {
    config
        .agents
        .get(route_agent_name)
        .and_then(|delegate| delegate.max_iterations)
        .unwrap_or(config.agent.max_tool_iterations)
}

#[cfg(test)]
mod tests {
    use super::{
        GatewayRuntime, GatewaySessionTreeQuery, SessionLineageMeta, acquire_inbound_slot,
        acquire_subagent_guard, attach_delegate_tool, create_tools_for_route,
        resolve_agent_max_tool_iterations, split_session_key,
    };
    use crate::channels::{ChannelKind, InboundMessage};
    use crate::config::{Config, DelegateAgentConfig};
    use serde_json::json;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use tokio::sync::RwLock;

    #[test]
    fn delegate_allowed_tools_filter_default_toolset() {
        let mut config = Config::default();
        config.agents.insert(
            "researcher".to_string(),
            DelegateAgentConfig {
                allowed_tools: vec!["file_read".to_string(), "shell".to_string()],
                ..DelegateAgentConfig::default()
            },
        );
        let effective_ws = PathBuf::from("/fake/workspace");

        let memory: Arc<dyn crate::memory::Memory> =
            Arc::new(crate::InMemoryMemory::new());
        let tools = create_tools_for_route(&config, "researcher", memory, &effective_ws);
        let names = tools.iter().map(|tool| tool.name()).collect::<Vec<_>>();
        assert_eq!(names, vec!["file_read", "shell"]);
    }

    #[test]
    fn attach_delegate_tool_requires_other_agents() {
        let mut config = Config::default();
        let runtime = GatewayRuntime::new(config.clone());
        let mut tools: Vec<Box<dyn crate::tools::Tool>> = Vec::new();

        // No configured agents -> not attached.
        assert!(!attach_delegate_tool(
            &config,
            &runtime,
            "omninova",
            None,
            &ChannelKind::Cli,
            0,
            &mut tools
        ));

        // One other agent -> attached.
        config.agents.insert(
            "researcher".to_string(),
            DelegateAgentConfig::default(),
        );
        assert!(attach_delegate_tool(
            &config,
            &runtime,
            "omninova",
            None,
            &ChannelKind::Cli,
            0,
            &mut tools
        ));
        assert_eq!(tools.last().map(|t| t.name()), Some("delegate"));

        // The only target equals the current agent -> not attached.
        let mut tools2: Vec<Box<dyn crate::tools::Tool>> = Vec::new();
        assert!(!attach_delegate_tool(
            &config,
            &runtime,
            "researcher",
            None,
            &ChannelKind::Cli,
            0,
            &mut tools2
        ));
    }

    #[test]
    fn attach_delegate_tool_respects_depth_and_allowlist() {
        let mut config = Config::default();
        config.agents.insert(
            "researcher".to_string(),
            DelegateAgentConfig::default(),
        );
        config.agents.insert(
            "writer".to_string(),
            DelegateAgentConfig {
                allowed_tools: vec!["file_read".to_string()],
                ..DelegateAgentConfig::default()
            },
        );
        config.agent_defaults_extended.subagents =
            Some(crate::config::schema::SubagentsConfig {
                max_spawn_depth: Some(1),
                ..crate::config::schema::SubagentsConfig::default()
            });
        let runtime = GatewayRuntime::new(config.clone());

        // Depth at limit -> child depth would exceed -> not attached.
        let mut tools: Vec<Box<dyn crate::tools::Tool>> = Vec::new();
        assert!(!attach_delegate_tool(
            &config,
            &runtime,
            "omninova",
            None,
            &ChannelKind::Cli,
            1,
            &mut tools
        ));

        // Within depth -> attached.
        assert!(attach_delegate_tool(
            &config,
            &runtime,
            "omninova",
            None,
            &ChannelKind::Cli,
            0,
            &mut tools
        ));

        // Agent with allowlist not containing "delegate" -> not attached.
        let mut tools2: Vec<Box<dyn crate::tools::Tool>> = Vec::new();
        assert!(!attach_delegate_tool(
            &config,
            &runtime,
            "writer",
            None,
            &ChannelKind::Cli,
            0,
            &mut tools2
        ));
    }

    #[test]
    fn delegate_max_iterations_overrides_agent_default() {
        let mut config = Config::default();
        config.agent.max_tool_iterations = 20;
        config.agents.insert(
            "researcher".to_string(),
            DelegateAgentConfig {
                max_iterations: Some(4),
                ..DelegateAgentConfig::default()
            },
        );

        assert_eq!(resolve_agent_max_tool_iterations(&config, "researcher"), 4);
        assert_eq!(resolve_agent_max_tool_iterations(&config, "omninova"), 20);
    }

    #[test]
    fn acquire_inbound_slot_enforces_limit() {
        let mut config = Config::default();
        config.agent_defaults_extended.max_concurrent = Some(1);
        let active = Arc::new(AtomicUsize::new(0));

        let first = acquire_inbound_slot(&config, &active).expect("first slot should succeed");
        assert!(first.is_some());

        let second = acquire_inbound_slot(&config, &active);
        assert!(second.is_err());

        drop(first);
        let third = acquire_inbound_slot(&config, &active).expect("slot should be released");
        assert!(third.is_some());
    }

    #[test]
    fn acquire_inbound_slot_uses_subagent_limit_fallback() {
        let mut config = Config::default();
        config.agent_defaults_extended.max_concurrent = None;
        config.agent_defaults_extended.subagents = Some(crate::config::schema::SubagentsConfig {
            max_concurrent: Some(1),
            ..crate::config::schema::SubagentsConfig::default()
        });
        let active = Arc::new(AtomicUsize::new(0));
        let first = acquire_inbound_slot(&config, &active).expect("first slot should succeed");
        assert!(first.is_some());
        let second = acquire_inbound_slot(&config, &active);
        assert!(second.is_err());
    }

    #[tokio::test]
    async fn subagent_guard_rejects_depth_over_limit() {
        let mut config = Config::default();
        config.agent_defaults_extended.subagents = Some(crate::config::schema::SubagentsConfig {
            max_spawn_depth: Some(2),
            ..crate::config::schema::SubagentsConfig::default()
        });
        let mut metadata = HashMap::new();
        metadata.insert("spawnDepth".to_string(), json!(3));
        let inbound = InboundMessage {
            channel: ChannelKind::Cli,
            text: "spawn".to_string(),
            metadata,
            ..InboundMessage::default()
        };
        let map = Arc::new(RwLock::new(HashMap::new()));
        let result = acquire_subagent_guard(&config, &inbound, &map).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn subagent_guard_enforces_children_per_parent() {
        let mut config = Config::default();
        config.agent_defaults_extended.subagents = Some(crate::config::schema::SubagentsConfig {
            max_children_per_agent: Some(1),
            ..crate::config::schema::SubagentsConfig::default()
        });
        let mut metadata = HashMap::new();
        metadata.insert("parentAgentId".to_string(), json!("main"));
        let inbound = InboundMessage {
            channel: ChannelKind::Cli,
            text: "spawn".to_string(),
            metadata,
            ..InboundMessage::default()
        };
        let map = Arc::new(RwLock::new(HashMap::new()));

        let first = acquire_subagent_guard(&config, &inbound, &map)
            .await
            .expect("first child should pass");
        assert!(first.is_some());

        let second = acquire_subagent_guard(&config, &inbound, &map).await;
        assert!(second.is_err());
    }

    fn temp_workspace() -> PathBuf {
        std::env::temp_dir().join(format!("omninova-test-{}", uuid::Uuid::new_v4()))
    }

    #[tokio::test]
    async fn session_lineage_registers_root_session() {
        let mut config = Config::default();
        config.workspace_dir = temp_workspace();
        let runtime = GatewayRuntime::new(config.clone());
        let inbound = InboundMessage {
            channel: ChannelKind::Cli,
            session_id: Some("root-1".to_string()),
            text: "root".to_string(),
            ..InboundMessage::default()
        };
        let meta = runtime
            .validate_and_resolve_session_lineage(&config, &inbound, "omninova")
            .await
            .expect("root session should register");
        assert_eq!(meta.spawn_depth, 0);
        assert!(meta.parent_session_key.is_none());
    }

    #[tokio::test]
    async fn session_lineage_validates_parent_child_depth() {
        let mut config = Config::default();
        config.workspace_dir = temp_workspace();
        let runtime = GatewayRuntime::new(config.clone());

        let root = InboundMessage {
            channel: ChannelKind::Cli,
            session_id: Some("parent".to_string()),
            text: "root".to_string(),
            ..InboundMessage::default()
        };
        runtime
            .validate_and_resolve_session_lineage(&config, &root, "omninova")
            .await
            .expect("root session should register");

        let mut child_meta = HashMap::new();
        child_meta.insert("parentSessionId".to_string(), json!("parent"));
        child_meta.insert("spawnDepth".to_string(), json!(1));
        let child = InboundMessage {
            channel: ChannelKind::Cli,
            session_id: Some("child".to_string()),
            text: "child".to_string(),
            metadata: child_meta,
            ..InboundMessage::default()
        };
        runtime
            .validate_and_resolve_session_lineage(&config, &child, "delegate")
            .await
            .expect("child depth should match parent");

        let mut bad_meta = HashMap::new();
        bad_meta.insert("parentSessionId".to_string(), json!("parent"));
        bad_meta.insert("spawnDepth".to_string(), json!(3));
        let bad_child = InboundMessage {
            channel: ChannelKind::Cli,
            session_id: Some("child-bad".to_string()),
            text: "child".to_string(),
            metadata: bad_meta,
            ..InboundMessage::default()
        };
        let result = runtime
            .validate_and_resolve_session_lineage(&config, &bad_child, "delegate")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn session_lineage_validates_parent_agent_binding() {
        let mut config = Config::default();
        config.workspace_dir = temp_workspace();
        let runtime = GatewayRuntime::new(config.clone());

        let root = InboundMessage {
            channel: ChannelKind::Cli,
            session_id: Some("parent-agent".to_string()),
            text: "root".to_string(),
            ..InboundMessage::default()
        };
        runtime
            .validate_and_resolve_session_lineage(&config, &root, "omninova")
            .await
            .expect("root session should register");

        let mut child_meta = HashMap::new();
        child_meta.insert("parentSessionId".to_string(), json!("parent-agent"));
        child_meta.insert("parentAgentId".to_string(), json!("wrong-agent"));
        child_meta.insert("spawnDepth".to_string(), json!(1));
        let child = InboundMessage {
            channel: ChannelKind::Cli,
            session_id: Some("child-agent-check".to_string()),
            text: "child".to_string(),
            metadata: child_meta,
            ..InboundMessage::default()
        };
        let result = runtime
            .validate_and_resolve_session_lineage(&config, &child, "delegate")
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn split_session_key_parses_channel_and_session() {
        let (channel, session_id) = split_session_key("cli:abc-123");
        assert_eq!(channel.as_deref(), Some("cli"));
        assert_eq!(session_id.as_deref(), Some("abc-123"));
    }

    #[tokio::test]
    async fn session_tree_snapshot_exposes_in_memory_nodes() {
        let mut config = Config::default();
        config.workspace_dir = temp_workspace();
        let runtime = GatewayRuntime::new(config.clone());
        {
            let mut lock = runtime.session_tree.write().await;
            lock.insert(
                "cli:debug-session".to_string(),
                SessionLineageMeta {
                    parent_session_key: Some("cli:parent".to_string()),
                    parent_agent_id: Some("omninova".to_string()),
                    agent_name: Some("delegate".to_string()),
                    spawn_depth: 1,
                    updated_at: super::now_unix_ts(),
                },
            );
        }
        let snapshot = runtime
            .session_tree_snapshot()
            .await
            .expect("snapshot should load");
        assert_eq!(snapshot.total_before_filter, 1);
        assert_eq!(snapshot.total_after_filter, 1);
        assert_eq!(snapshot.returned, 1);
        assert!(!snapshot.has_more);
        assert_eq!(snapshot.next_offset, None);
        assert_eq!(
            snapshot.source_counts_after_filter.get("memory"),
            Some(&1usize)
        );
        assert_eq!(snapshot.stats_after_filter.unique_agents, 1);
        assert_eq!(snapshot.stats_after_filter.unique_parent_agents, 1);
        assert_eq!(snapshot.stats_after_filter.max_spawn_depth, 1);
        assert!(snapshot
            .sessions
            .iter()
            .any(|entry| entry.session_key.as_deref() == Some("cli:debug-session")
                && entry.parent_agent_id.as_deref() == Some("omninova")));
    }

    #[tokio::test]
    async fn session_tree_snapshot_supports_query_filters() {
        let mut config = Config::default();
        config.workspace_dir = temp_workspace();
        let runtime = GatewayRuntime::new(config.clone());
        {
            let mut lock = runtime.session_tree.write().await;
            lock.insert(
                "cli:keep-me".to_string(),
                SessionLineageMeta {
                    parent_session_key: None,
                    parent_agent_id: Some("omninova".to_string()),
                    agent_name: Some("delegate-a".to_string()),
                    spawn_depth: 0,
                    updated_at: super::now_unix_ts(),
                },
            );
            lock.insert(
                "cli:drop-me".to_string(),
                SessionLineageMeta {
                    parent_session_key: None,
                    parent_agent_id: Some("omninova".to_string()),
                    agent_name: Some("delegate-b".to_string()),
                    spawn_depth: 0,
                    updated_at: super::now_unix_ts(),
                },
            );
        }

        let filtered = runtime
            .session_tree_snapshot_filtered(&GatewaySessionTreeQuery {
                session_id: Some("keep-me".to_string()),
                agent_name: Some("delegate-a".to_string()),
                channel: Some("cli".to_string()),
                source: Some("memory".to_string()),
                limit: Some(1),
                ..GatewaySessionTreeQuery::default()
            })
            .await
            .expect("filtered snapshot should load");

        assert_eq!(filtered.sessions.len(), 1);
        assert_eq!(filtered.total_before_filter, 2);
        assert_eq!(filtered.total_after_filter, 1);
        assert_eq!(filtered.returned, 1);
        assert!(!filtered.has_more);
        assert_eq!(filtered.next_offset, None);
        assert_eq!(
            filtered.source_counts_after_filter.get("memory"),
            Some(&1usize)
        );
        assert_eq!(
            filtered.sessions[0].session_key.as_deref(),
            Some("cli:keep-me")
        );
    }

    #[tokio::test]
    async fn session_tree_snapshot_supports_parent_and_depth_filters() {
        let mut config = Config::default();
        config.workspace_dir = temp_workspace();
        let runtime = GatewayRuntime::new(config.clone());
        {
            let mut lock = runtime.session_tree.write().await;
            lock.insert(
                "cli:parent-x".to_string(),
                SessionLineageMeta {
                    parent_session_key: None,
                    parent_agent_id: None,
                    agent_name: Some("OmniNova".to_string()),
                    spawn_depth: 0,
                    updated_at: super::now_unix_ts(),
                },
            );
            lock.insert(
                "cli:child-x-1".to_string(),
                SessionLineageMeta {
                    parent_session_key: Some("cli:parent-x".to_string()),
                    parent_agent_id: Some("OmniNova".to_string()),
                    agent_name: Some("Delegate-X".to_string()),
                    spawn_depth: 1,
                    updated_at: super::now_unix_ts(),
                },
            );
            lock.insert(
                "cli:child-x-2".to_string(),
                SessionLineageMeta {
                    parent_session_key: Some("cli:parent-x".to_string()),
                    parent_agent_id: Some("OmniNova".to_string()),
                    agent_name: Some("Delegate-Y".to_string()),
                    spawn_depth: 2,
                    updated_at: super::now_unix_ts(),
                },
            );
        }

        let filtered = runtime
            .session_tree_snapshot_filtered(&GatewaySessionTreeQuery {
                parent_session_id: Some("PARENT-X".to_string()),
                parent_agent_id: Some("omninova".to_string()),
                min_spawn_depth: Some(1),
                max_spawn_depth: Some(1),
                source: Some("MEMORY".to_string()),
                case_insensitive: Some(true),
                ..GatewaySessionTreeQuery::default()
            })
            .await
            .expect("filtered snapshot should load");

        assert_eq!(filtered.sessions.len(), 1);
        assert_eq!(
            filtered.sessions[0].session_key.as_deref(),
            Some("cli:child-x-1")
        );
    }

    #[tokio::test]
    async fn session_tree_snapshot_supports_sort_and_offset() {
        let mut config = Config::default();
        config.workspace_dir = temp_workspace();
        let runtime = GatewayRuntime::new(config.clone());
        {
            let mut lock = runtime.session_tree.write().await;
            lock.insert(
                "cli:s1".to_string(),
                SessionLineageMeta {
                    parent_session_key: None,
                    parent_agent_id: None,
                    agent_name: Some("B-Agent".to_string()),
                    spawn_depth: 2,
                    updated_at: super::now_unix_ts(),
                },
            );
            lock.insert(
                "cli:s2".to_string(),
                SessionLineageMeta {
                    parent_session_key: None,
                    parent_agent_id: None,
                    agent_name: Some("A-Agent".to_string()),
                    spawn_depth: 1,
                    updated_at: super::now_unix_ts(),
                },
            );
            lock.insert(
                "cli:s3".to_string(),
                SessionLineageMeta {
                    parent_session_key: None,
                    parent_agent_id: None,
                    agent_name: Some("C-Agent".to_string()),
                    spawn_depth: 3,
                    updated_at: super::now_unix_ts(),
                },
            );
        }

        let filtered = runtime
            .session_tree_snapshot_filtered(&GatewaySessionTreeQuery {
                sort_by: Some("spawn_depth".to_string()),
                sort_order: Some("asc".to_string()),
                offset: Some(1),
                limit: Some(1),
                ..GatewaySessionTreeQuery::default()
            })
            .await
            .expect("filtered snapshot should load");

        assert_eq!(filtered.total_before_filter, 3);
        assert_eq!(filtered.total_after_filter, 3);
        assert_eq!(filtered.offset, 1);
        assert_eq!(filtered.limit, Some(1));
        assert_eq!(filtered.returned, 1);
        assert!(filtered.has_more);
        assert_eq!(filtered.next_offset, Some(2));
        assert_eq!(filtered.prev_offset, Some(0));
        assert_eq!(filtered.next_cursor, Some(2));
        assert_eq!(filtered.prev_cursor, Some(0));
        assert_eq!(
            filtered.source_counts_after_filter.get("memory"),
            Some(&3usize)
        );
        assert_eq!(filtered.stats_after_filter.unique_agents, 3);
        assert_eq!(filtered.stats_after_filter.unique_parent_agents, 0);
        assert_eq!(filtered.stats_after_filter.max_spawn_depth, 3);
        assert_eq!(filtered.sessions[0].spawn_depth, 2);
        assert_eq!(filtered.sessions[0].session_key.as_deref(), Some("cli:s1"));
    }

    #[tokio::test]
    async fn session_tree_snapshot_supports_cursor_as_offset_alias() {
        let mut config = Config::default();
        config.workspace_dir = temp_workspace();
        let runtime = GatewayRuntime::new(config.clone());
        {
            let mut lock = runtime.session_tree.write().await;
            lock.insert(
                "cli:c1".to_string(),
                SessionLineageMeta {
                    parent_session_key: None,
                    parent_agent_id: None,
                    agent_name: Some("A".to_string()),
                    spawn_depth: 1,
                    updated_at: super::now_unix_ts(),
                },
            );
            lock.insert(
                "cli:c2".to_string(),
                SessionLineageMeta {
                    parent_session_key: None,
                    parent_agent_id: None,
                    agent_name: Some("B".to_string()),
                    spawn_depth: 2,
                    updated_at: super::now_unix_ts(),
                },
            );
        }

        let filtered = runtime
            .session_tree_snapshot_filtered(&GatewaySessionTreeQuery {
                sort_by: Some("spawn_depth".to_string()),
                sort_order: Some("asc".to_string()),
                cursor: Some(1),
                limit: Some(1),
                ..GatewaySessionTreeQuery::default()
            })
            .await
            .expect("cursor paging should work");

        assert_eq!(filtered.offset, 1);
        assert_eq!(filtered.sessions.len(), 1);
        assert_eq!(filtered.sessions[0].session_key.as_deref(), Some("cli:c2"));
    }
}
