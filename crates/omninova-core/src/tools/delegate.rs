//! Delegate tool: lets an agent hand a subtask to another named agent at
//! runtime (Orchestrator-Workers). Session lineage (parent/depth) and child
//! concurrency limits are enforced by the gateway via the same metadata used
//! for externally driven subagents.

use crate::channels::ChannelKind;
use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use std::sync::Arc;

/// A delegation request resolved by the runtime.
#[derive(Debug, Clone)]
pub struct DelegateRequest {
    /// Target agent name (must exist in `config.agents`).
    pub agent: String,
    /// Self-contained task text for the sub-agent.
    pub task: String,
    /// Optional extra context prepended to the task.
    pub context: Option<String>,
    /// Name of the delegating (parent) agent.
    pub parent_agent: String,
    /// Session id of the parent request, when it has one.
    pub parent_session_id: Option<String>,
    /// Channel of the parent request; the child session is created on it.
    pub channel: ChannelKind,
    /// Spawn depth of the child (parent depth + 1).
    pub child_depth: u32,
}

/// Runtime capable of executing a delegated subtask on another agent.
///
/// Implemented by `GatewayRuntime`. Defined here so tools don't depend on the
/// gateway module (avoids a dependency cycle).
#[async_trait]
pub trait AgentInvoker: Send + Sync {
    async fn invoke_agent(&self, request: DelegateRequest) -> anyhow::Result<String>;
}

/// Tool exposed to the LLM for delegating subtasks to other configured agents.
pub struct DelegateTool {
    invoker: Arc<dyn AgentInvoker>,
    /// Agents that may be delegated to (excludes the current agent).
    targets: Vec<String>,
    parent_agent: String,
    parent_session_id: Option<String>,
    channel: ChannelKind,
    child_depth: u32,
}

impl DelegateTool {
    pub fn new(
        invoker: Arc<dyn AgentInvoker>,
        targets: Vec<String>,
        parent_agent: String,
        parent_session_id: Option<String>,
        channel: ChannelKind,
        child_depth: u32,
    ) -> Self {
        Self {
            invoker,
            targets,
            parent_agent,
            parent_session_id,
            channel,
            child_depth,
        }
    }
}

#[async_trait]
impl Tool for DelegateTool {
    fn name(&self) -> &str {
        "delegate"
    }

    fn description(&self) -> &str {
        "Delegate a self-contained subtask to another specialized agent and get its final answer. \
         Use this to split complex work across agents. The sub-agent has no access to this \
         conversation, so include all necessary details in the task."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "agent": {
                    "type": "string",
                    "description": format!(
                        "Target agent name. Available agents: {}",
                        self.targets.join(", ")
                    ),
                    "enum": self.targets,
                },
                "task": {
                    "type": "string",
                    "description": "Clear, self-contained task for the sub-agent."
                },
                "context": {
                    "type": "string",
                    "description": "Optional extra context (facts, constraints, prior findings)."
                }
            },
            "required": ["agent", "task"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let agent = args
            .get("agent")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        let task = args
            .get("task")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        let context = args
            .get("context")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if task.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("'task' must not be empty".into()),
            });
        }
        if !self.targets.iter().any(|t| t == &agent) {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "unknown delegate target '{}'. Available agents: {}",
                    agent,
                    self.targets.join(", ")
                )),
            });
        }

        let request = DelegateRequest {
            agent: agent.clone(),
            task,
            context,
            parent_agent: self.parent_agent.clone(),
            parent_session_id: self.parent_session_id.clone(),
            channel: self.channel.clone(),
            child_depth: self.child_depth,
        };

        match self.invoker.invoke_agent(request).await {
            Ok(reply) => Ok(ToolResult {
                success: true,
                output: format!("[agent {agent} reply]\n{reply}"),
                error: None,
            }),
            // Surface runtime refusals (depth/children limits, timeouts) to the
            // LLM as tool failures so it can adapt instead of aborting the turn.
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("delegation to '{agent}' failed: {e}")),
            }),
        }
    }
}
