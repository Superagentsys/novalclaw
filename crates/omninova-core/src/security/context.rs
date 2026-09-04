use crate::channels::InboundMessage;
use crate::config::Config;
use crate::observability::{record_approval_event, record_tool_call};
use crate::routing::RouteDecision;
use crate::security::approval::{ApprovalController, PendingApproval};
use crate::security::audit::{AuditLogger, AuditRequestContext};
use crate::security::estop::EstopController;
use crate::security::tool_policy::{evaluate_tool_call, ToolPolicyDecision};
use anyhow::Result;

fn safe_approval_arguments(arguments: &serde_json::Value) -> serde_json::Value {
    let Some(object) = arguments.as_object() else {
        return arguments.clone();
    };
    serde_json::Value::Object(
        object
            .iter()
            .map(|(key, value)| {
                let lowered = key.to_ascii_lowercase();
                let safe_value = if ["secret", "token", "password", "api_key", "apikey"]
                    .iter()
                    .any(|needle| lowered.contains(needle))
                {
                    serde_json::Value::String("[已隐藏敏感值]".to_string())
                } else if let Some(text) = value.as_str() {
                    serde_json::Value::String(text.chars().take(2000).collect())
                } else {
                    value.clone()
                };
                (key.clone(), safe_value)
            })
            .collect(),
    )
}

fn risk_level_for_tool(tool_name: &str) -> &'static str {
    match tool_name {
        "file_delete" | "delete_file" | "git_clean" | "git_reset" | "git_restore" | "git_checkout" => "high",
        "shell" | "bash" | "run_command" | "Command" => "medium",
        _ => "medium",
    }
}

/// Feishu chat-only system prompt - used when inbound.metadata.chat_only=true
pub const FEISHU_CHAT_ONLY_SYSTEM_PROMPT: &str = r#"你是 OmniNova 飞书聊天助手。

【模式说明】
当前处于飞书普通聊天模式，普通飞书消息只允许：
- 聊天、对话、问答
- 解释、总结、建议
- 代码语法、逻辑解释

【禁止操作】
不得执行以下工具或操作：
- 文件操作：读取、创建、编辑、删除文件
- Shell 命令：执行终端命令、运行脚本
- Git 操作：commit、push、pull、merge
- 桌面监控：截屏、桌面监控
- 浏览器自动化：网页浏览、点击、输入
- 工作区操作：访问 D:\、E:\ 等本地磁盘

【如何使用工具】
如果用户要求执行工具任务，请提示他使用以下 slash command：
- /file <任务描述>  — 文件操作
- /run <任务描述>   — 执行命令
- /monitor <任务描述> — 桌面监控
- /tool <任务描述>  — 其他工具

【高风险操作】
涉及删除文件、格式化、执行危险命令等高风险操作，需要用户明确确认后才能执行。

【回复规范】
- 不要声称可以直接处理 D:\、E:\ 或其他本地文件路径
- 不要建议用户可以执行 shell 命令
- 只在聊天范围内回答问题"#;

/// Fixed response when chat-only mode detects tool intent
pub const FEISHU_CHAT_ONLY_BLOCKED_RESPONSE: &str = 
    "当前飞书普通聊天模式不直接执行工具任务。如需处理文件，请发送：/file <任务描述>。删除文件属于高风险操作，需要确认后才能执行。";

/// Keywords that indicate tool intent in user text
const TOOL_INTENT_PATTERNS: &[&str] = &[
    // File operations
    "删除", "删掉", "删去", "删掉", "移除",
    "创建文件", "新建文件", "写文件", "修改文件", "编辑文件",
    "查看文件", "读取文件", "打开文件",
    // Path/directory
    "查看 d盘", "查看 d:", "d盘", "d:\\", "e盘", "e:\\",
    "访问 d", "访问 e", "打开 d:", "打开 e:",
    "d盘文件", "e盘文件", "查看 d", "查看 e",
    // Shell/command
    "执行命令", "运行命令", "执行脚本", "运行脚本",
    "执行程序", "运行程序",
    "在终端", "在命令行", "在 shell",
    // Git
    "git commit", "git push", "git pull", "git merge",
    "提交代码", "推送代码", "拉取代码",
    // Desktop monitoring
    "监控桌面", "截屏", "截图", "屏幕截图",
    "监控 1 分钟", "监控 30 秒",
    // Browser
    "打开浏览器", "浏览网页", "访问网站",
];

#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub config: Config,
    estop: EstopController,
    approvals: ApprovalController,
    audit: AuditLogger,
    /// Inbound metadata for channel-specific policies (e.g., feishu chat_only mode)
    inbound_metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl SecurityContext {
    pub fn from_config(config: &Config) -> Self {
        Self {
            config: config.clone(),
            estop: EstopController::from_config(config),
            approvals: ApprovalController::from_workspace(&config.workspace_dir),
            audit: AuditLogger::from_config(config),
            inbound_metadata: std::collections::HashMap::new(),
        }
    }

    pub fn for_inbound(config: &Config, inbound: &InboundMessage, route: &RouteDecision) -> Self {
        let audit_ctx = AuditRequestContext {
            trace_id: format!("trace-{}", uuid::Uuid::new_v4()),
            channel: format!("{:?}", inbound.channel),
            session_id: inbound.session_id.clone(),
            user_id: inbound.user_id.clone(),
            agent_name: Some(route.agent_name.clone()),
            provider: route.provider.clone(),
            model: route.model.clone(),
        };
        Self {
            config: config.clone(),
            estop: EstopController::from_config(config),
            approvals: ApprovalController::from_workspace(&config.workspace_dir),
            audit: AuditLogger::from_config(config).with_context(audit_ctx),
            inbound_metadata: inbound.metadata.clone(),
        }
    }

    /// Set inbound metadata (used for testing)
    pub fn set_inbound_metadata(&mut self, metadata: std::collections::HashMap<String, serde_json::Value>) {
        self.inbound_metadata = metadata;
    }

    pub fn trace_id(&self) -> &str {
        &self.audit.context().trace_id
    }

    pub fn estop(&self) -> &EstopController {
        &self.estop
    }

    pub fn approvals(&self) -> &ApprovalController {
        &self.approvals
    }

    pub fn audit(&self) -> &AuditLogger {
        &self.audit
    }

    pub fn inbound_task_id(&self) -> Option<String> {
        self.inbound_metadata
            .get("task_id")
            .and_then(|value| value.as_str())
            .map(str::to_string)
    }

    pub fn should_defer_approval(&self) -> bool {
        match self.config.approvals.wait_mode.as_deref() {
            Some("deferred") => true,
            Some("blocking") => false,
            _ => {
                let channel = self.audit.context().channel.to_ascii_lowercase();
                channel.contains("feishu")
                    || channel.contains("lark")
                    || channel.contains("wecom")
                    || channel.contains("dingtalk")
            }
        }
    }

    /// Check if chat-only mode is enabled for this inbound (e.g., Feishu without slash commands)
    pub fn is_chat_only(&self) -> bool {
        self.inbound_metadata
            .get("chat_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Get the system prompt for chat-only mode
    pub fn chat_only_system_prompt(&self) -> Option<String> {
        if self.is_chat_only() {
            Some(FEISHU_CHAT_ONLY_SYSTEM_PROMPT.to_string())
        } else {
            None
        }
    }

    /// Detect if user text contains tool intent in chat-only mode
    pub fn detect_tool_intent(&self, text: &str) -> Option<String> {
        if !self.is_chat_only() {
            return None;
        }

        let text_lower = text.to_lowercase();
        for pattern in TOOL_INTENT_PATTERNS {
            if text_lower.contains(&pattern.to_lowercase()) {
                // Categorize the intent
                let intent = categorize_tool_intent(pattern);
                println!(
                    "[feishu-policy] blocked_tool_intent reason=chat_only intent={} pattern={} text_len={}",
                    intent,
                    pattern,
                    text.len()
                );
                return Some(intent);
            }
        }
        None
    }

    /// Get the blocked response for tool intent
    pub fn tool_intent_blocked_response(&self) -> String {
        FEISHU_CHAT_ONLY_BLOCKED_RESPONSE.to_string()
    }

    /// Check if a tool is blocked by chat-only policy
    pub fn is_tool_blocked_by_chat_only(&self, tool_name: &str) -> bool {
        if !self.is_chat_only() {
            return false;
        }
        
        // Tools blocked in chat-only mode
        const BLOCKED_TOOLS: &[&str] = &[
            "shell",
            "bash",
            "sh",
            "cmd",
            "powershell",
            "file_read",
            "read_file",
            "file_write",
            "write_file",
            "file_edit",
            "edit_file",
            "str_replace_editor",
            "file_patch",
            "apply_patch",
            "workspace",
            "desktop_monitor",
            "monitor_desktop",
            "browser_automation",
            "browser_navigate",
            "browser_click",
            "browser_type",
            "command",
            "exec",
            "run_command",
            "desktop_vision",
            "screenshot",
            "screen_capture",
            "computer_use",
            "desktop_use",
            "git",
            "git_clone",
            "git_commit",
            "git_push",
            "git_pull",
        ];
        
        let tool_lower = tool_name.to_lowercase();
        BLOCKED_TOOLS.iter().any(|blocked| tool_lower.contains(blocked))
    }

    pub async fn audit_inbound_start(&self, text_len: usize) {
        self.audit.record_inbound_start(text_len).await;
    }

    pub async fn audit_route(&self, detail: &str) {
        self.audit.record_route(detail).await;
    }

    pub async fn audit_provider_call(
        &self,
        iteration: usize,
        tool_call_count: usize,
        success: bool,
        detail: &str,
    ) {
        self.audit
            .record_provider_call(iteration, tool_call_count, success, detail)
            .await;
    }

    pub async fn audit_session_persisted(&self, session_id: &str, message_count: usize) {
        self.audit
            .record_session_persisted(session_id, message_count)
            .await;
    }

    pub async fn audit_inbound_complete(&self, success: bool, detail: &str) {
        self.audit.record_inbound_complete(success, detail).await;
    }

    pub async fn ensure_active(&self) -> Result<()> {
        let paused = self.estop.is_paused().await?;
        self.audit.record_estop_check(paused).await;
        if paused {
            anyhow::bail!("agent is paused by emergency stop (E-Stop)");
        }
        Ok(())
    }

    pub async fn preflight_tool(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<ToolPolicyDecision> {
        // ==== CHAT-ONLY MODE CHECK ====
        // Block dangerous tools in Feishu chat-only mode
        if self.is_chat_only() && self.is_tool_blocked_by_chat_only(tool_name) {
            println!(
                "[feishu-policy] blocked_tool_call tool={} reason=chat_only",
                tool_name
            );
            return Ok(ToolPolicyDecision::Deny {
                reason: "当前飞书普通聊天模式不允许执行工具。如需执行任务，请使用 /run、/monitor 或 /file 命令。".to_string(),
            });
        }
        
        self.ensure_active().await?;
        Ok(evaluate_tool_call(&self.config, tool_name, arguments))
    }

    pub async fn gate_tool_execution(
        &self,
        run_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<ToolExecutionGate> {
        if let Some(grant) = self
            .approvals
            .consume_matching_grant(run_id, tool_call_id, tool_name, arguments)
            .await?
        {
            record_approval_event("consumed");
            return Ok(ToolExecutionGate::Proceed {
                note: Some(format!("consumed approval {}", grant.id)),
            });
        }

        match self.preflight_tool(tool_name, arguments).await? {
            ToolPolicyDecision::Allow => Ok(ToolExecutionGate::Proceed { note: None }),
            ToolPolicyDecision::Deny { reason } => {
                record_tool_call(tool_name, "denied");
                self.audit.record_tool_blocked(tool_name, &reason).await;
                Ok(ToolExecutionGate::Blocked { reason })
            }
            ToolPolicyDecision::RequireApproval { reason } => {
                let pending = self
                    .approvals
                    .create(
                        run_id,
                        tool_call_id,
                        tool_name,
                        arguments.clone(),
                        safe_approval_arguments(arguments),
                        tool_name,
                        risk_level_for_tool(tool_name),
                        "需要审批",
                        &reason,
                    )
                    .await?;
                record_approval_event("requested");
                record_tool_call(tool_name, "approval_required");
                self.audit
                    .record_tool_approval_required(tool_name, &pending.id, &reason)
                    .await;
                Ok(ToolExecutionGate::ApprovalRequired { pending })
            }
        }
    }

    /// Reads the current decision for a pending tool approval.
    pub async fn tool_approval(&self, id: &str) -> Result<Option<PendingApproval>> {
        self.approvals.get(id).await
    }

    pub async fn audit_tool_call(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        success: bool,
        detail: &str,
    ) {
        record_tool_call(tool_name, if success { "ok" } else { "error" });
        self.audit
            .record_tool_execution(tool_name, arguments, success, detail)
            .await;
    }
}

/// Categorize tool intent based on pattern
fn categorize_tool_intent(pattern: &str) -> String {
    let p = pattern.to_lowercase();
    // Check path access first since it's most specific
    if p.contains("d盘") || p.contains("d:") || p.contains("e盘") || p.contains("e:") 
        || p.contains("访问 d") || p.contains("访问 e") 
        || p.contains("d盘文件") || p.contains("e盘文件")
        || (p.contains("查看 d") || p.contains("查看 e"))
    {
        "path_access".to_string()
    } else if p.contains("删除") || p.contains("删掉") || p.contains("删去") {
        "file_delete".to_string()
    } else if p.contains("创建") || p.contains("新建") || p.contains("写文件") || p.contains("修改") || p.contains("编辑") {
        "file_write".to_string()
    } else if p.contains("查看") && p.contains("文件") {
        "file_read".to_string()
    } else if p.contains("执行命令") || p.contains("运行命令") || p.contains("脚本") || p.contains("程序") || p.contains("终端") || p.contains("shell") || p.contains("命令行") {
        "shell_exec".to_string()
    } else if p.contains("git") || p.contains("commit") || p.contains("push") || p.contains("pull") || p.contains("提交") || p.contains("推送") || p.contains("拉取") {
        "git_operation".to_string()
    } else if p.contains("监控桌面") || p.contains("截屏") || p.contains("截图") || p.contains("屏幕") {
        "desktop_monitor".to_string()
    } else if p.contains("浏览器") || p.contains("网页") || p.contains("网站") {
        "browser_automation".to_string()
    } else {
        "tool_intent".to_string()
    }
}

#[derive(Debug, Clone)]
pub enum ToolExecutionGate {
    Proceed {
        note: Option<String>,
    },
    Blocked {
        reason: String,
    },
    ApprovalRequired {
        pending: PendingApproval,
    },
}

impl ToolExecutionGate {
    pub fn blocked_message(&self) -> Option<String> {
        match self {
            Self::Blocked { reason } => Some(reason.clone()),
            Self::ApprovalRequired { pending } => Some(format!(
                "tool execution requires approval (id={}, tool={}, reason={}). \
                 Approve with: omninova approvals approve {}",
                pending.id, pending.tool_name, pending.reason, pending.id
            )),
            Self::Proceed { .. } => None,
        }
    }
}
