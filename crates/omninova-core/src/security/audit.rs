use crate::config::Config;
use crate::observability::record_audit_event;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct AuditRequestContext {
    pub trace_id: String,
    pub channel: String,
    pub session_id: Option<String>,
    pub user_id: Option<String>,
    pub agent_name: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AuditLogger {
    enabled: bool,
    record_arguments: bool,
    log_file: PathBuf,
    context: AuditRequestContext,
}

impl AuditLogger {
    pub fn from_config(config: &Config) -> Self {
        let log_file = config
            .security
            .audit
            .log_file
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| config.workspace_dir.join(".omninova-audit.log"));
        Self {
            enabled: config.security.audit.enabled,
            record_arguments: config.security.audit.record_arguments,
            log_file,
            context: AuditRequestContext::default(),
        }
    }

    pub fn with_context(mut self, context: AuditRequestContext) -> Self {
        self.context = context;
        self
    }

    pub fn context(&self) -> &AuditRequestContext {
        &self.context
    }

    pub async fn record_event(
        &self,
        event: &str,
        success: bool,
        detail: &str,
        extra: serde_json::Value,
    ) {
        if !self.enabled {
            return;
        }
        record_audit_event(event);
        let mut line = serde_json::json!({
            "ts": time::OffsetDateTime::now_utc().unix_timestamp(),
            "event": event,
            "success": success,
            "detail": detail,
            "trace_id": self.context.trace_id,
            "channel": self.context.channel,
            "session_id": self.context.session_id,
            "user_id": self.context.user_id,
            "agent": self.context.agent_name,
            "provider": self.context.provider,
            "model": self.context.model,
        });
        if let Some(obj) = line.as_object_mut() {
            if let Some(map) = extra.as_object() {
                for (k, v) in map {
                    obj.insert(k.clone(), v.clone());
                }
            }
        }
        self.append_line(&line).await;
    }

    pub async fn record_inbound_start(&self, text_len: usize) {
        self.record_event(
            "inbound_received",
            true,
            "gateway accepted inbound message",
            serde_json::json!({ "text_len": text_len }),
        )
        .await;
    }

    pub async fn record_route(&self, route_detail: &str) {
        self.record_event(
            "route_selected",
            true,
            route_detail,
            serde_json::json!({}),
        )
        .await;
    }

    pub async fn record_estop_check(&self, paused: bool) {
        self.record_event(
            "estop_checked",
            !paused,
            if paused { "paused" } else { "active" },
            serde_json::json!({ "paused": paused }),
        )
        .await;
    }

    pub async fn record_provider_call(
        &self,
        iteration: usize,
        tool_call_count: usize,
        success: bool,
        detail: &str,
    ) {
        self.record_event(
            "provider_call",
            success,
            detail,
            serde_json::json!({
                "iteration": iteration,
                "tool_call_count": tool_call_count,
            }),
        )
        .await;
    }

    pub async fn record_tool_execution(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        success: bool,
        detail: &str,
    ) {
        let args = if self.record_arguments {
            arguments.clone()
        } else {
            serde_json::Value::Null
        };
        self.record_event(
            "tool_execution",
            success,
            detail,
            serde_json::json!({
                "tool": tool_name,
                "arguments": args,
            }),
        )
        .await;
    }

    pub async fn record_tool_blocked(&self, tool_name: &str, reason: &str) {
        self.record_event(
            "tool_blocked",
            false,
            reason,
            serde_json::json!({ "tool": tool_name }),
        )
        .await;
    }

    pub async fn record_tool_approval_required(
        &self,
        tool_name: &str,
        approval_id: &str,
        reason: &str,
    ) {
        self.record_event(
            "tool_approval_required",
            true,
            reason,
            serde_json::json!({
                "tool": tool_name,
                "approval_id": approval_id,
            }),
        )
        .await;
    }

    pub async fn record_session_persisted(&self, session_id: &str, message_count: usize) {
        self.record_event(
            "session_persisted",
            true,
            session_id,
            serde_json::json!({ "message_count": message_count }),
        )
        .await;
    }

    pub async fn record_inbound_complete(&self, success: bool, detail: &str) {
        self.record_event(
            "inbound_complete",
            success,
            detail,
            serde_json::json!({}),
        )
        .await;
    }

    async fn append_line(&self, value: &serde_json::Value) {
        if let Some(parent) = self.log_file.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let mut entry = value.to_string();
        entry.push('\n');
        let path = self.log_file.clone();
        let _ = async {
            use tokio::io::AsyncWriteExt;
            if let Ok(mut file) = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await
            {
                let _ = file.write_all(entry.as_bytes()).await;
            }
        }
        .await;
    }
}
