use crate::channels::InboundMessage;
use crate::config::Config;
use crate::observability::{record_approval_event, record_tool_call};
use crate::routing::RouteDecision;
use crate::security::approval::{ApprovalController, PendingApproval};
use crate::security::audit::{AuditLogger, AuditRequestContext};
use crate::security::estop::EstopController;
use crate::security::tool_policy::{evaluate_tool_call, ToolPolicyDecision};
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub config: Config,
    estop: EstopController,
    approvals: ApprovalController,
    audit: AuditLogger,
}

impl SecurityContext {
    pub fn from_config(config: &Config) -> Self {
        Self {
            config: config.clone(),
            estop: EstopController::from_config(config),
            approvals: ApprovalController::from_workspace(&config.workspace_dir),
            audit: AuditLogger::from_config(config),
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
        }
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
        self.ensure_active().await?;
        Ok(evaluate_tool_call(&self.config, tool_name, arguments))
    }

    pub async fn gate_tool_execution(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<ToolExecutionGate> {
        if let Some(grant) = self
            .approvals
            .consume_matching_grant(tool_name, arguments)
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
                    .create(tool_name, arguments.clone(), &reason)
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
