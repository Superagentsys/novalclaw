//! Unified Agent Runtime Event types.
//!
//! All tool execution events flow through this single event bus, providing
//! a consistent schema for real-time UI streaming and final run replay.

use serde::{Deserialize, Serialize};

/// Execution status of a step or tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Pending,
    Running,
    Success,
    Error,
}

/// File change type for `file_changed` events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeType {
    Created,
    Modified,
    Deleted,
}

/// Unified agent runtime event — single source of truth for timeline & replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentRunEvent {
    // ─── Lifecycle ────────────────────────────────────────────────────────────

    /// Run has started.
    run_started {
        run_id: String,
        agent_name: String,
        session_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_step_id: Option<String>,
    },

    /// A named step (group of tools) has started.
    step_started {
        run_id: String,
        step_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_step_id: Option<String>,
        title: String,
    },

    /// The model has started a visible execution phase. This is a safe status
    /// update, not chain-of-thought.
    model_started {
        run_id: String,
        step_id: String,
        title: String,
    },

    /// Optional visible model output delta when a provider supports streaming.
    model_delta {
        run_id: String,
        step_id: String,
        content: String,
    },

    /// The model phase has completed.
    model_completed {
        run_id: String,
        step_id: String,
        title: String,
    },

    /// A tool call has been prepared by the model but not executed yet.
    tool_call_created {
        run_id: String,
        step_id: String,
        tool_call_id: String,
        tool_name: String,
        title: String,
    },

    /// A tool call has started executing.
    tool_started {
        run_id: String,
        step_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_step_id: Option<String>,
        tool_call_id: String,
        tool_name: String,
        /// Human-readable Chinese summary, e.g. "正在列出目录：src"
        title: String,
    },

    /// Streaming output from a command-style tool (shell, git, etc.).
    command_output {
        run_id: String,
        step_id: String,
        tool_call_id: String,
        tool_name: String,
        /// Output chunk (one line or a few lines).
        content: String,
        /// Whether this chunk came from stderr.
        #[serde(default)]
        is_stderr: bool,
    },

    /// A file was created, modified, or deleted.
    file_changed {
        run_id: String,
        step_id: String,
        tool_call_id: Option<String>,
        path: String,
        /// Number of lines added.
        additions: i32,
        /// Number of lines removed.
        deletions: i32,
        #[serde(skip_serializing_if = "Option::is_none")]
        change_type: Option<ChangeType>,
    },

    /// A structured patch is about to be applied to a file.
    patch_started {
        run_id: String,
        step_id: String,
        tool_call_id: String,
        path: String,
        title: String,
    },

    /// One real hunk from a structured patch.
    patch_hunk {
        run_id: String,
        step_id: String,
        tool_call_id: String,
        path: String,
        old_start: usize,
        old_lines: usize,
        new_start: usize,
        new_lines: usize,
        additions: i32,
        deletions: i32,
        summary: String,
    },

    /// A structured patch was applied successfully.
    patch_applied {
        run_id: String,
        step_id: String,
        tool_call_id: String,
        path: String,
        additions: i32,
        deletions: i32,
        hunks_count: usize,
        result_summary: String,
    },

    /// A structured patch failed.
    patch_failed {
        run_id: String,
        step_id: String,
        tool_call_id: String,
        path: String,
        error: String,
    },

    /// A tool call has finished.
    tool_completed {
        run_id: String,
        step_id: String,
        tool_call_id: String,
        tool_name: String,
        status: StepStatus,
        /// Execution duration in milliseconds.
        duration_ms: u64,
        /// Short result summary for display (truncated to ~2000 chars).
        #[serde(skip_serializing_if = "String::is_empty")]
        result_summary: String,
        /// Diff stats for file write/edit tools (git or content-based fallback).
        #[serde(skip_serializing_if = "Option::is_none")]
        diff_stats: Option<DiffStats>,
    },

    /// A tool requires user approval before executing.
    approval_required {
        run_id: String,
        step_id: String,
        tool_call_id: String,
        tool_name: String,
        title: String,
        /// Human-readable reason for the approval requirement.
        reason: String,
    },

    /// Run has completed successfully.
    run_completed {
        run_id: String,
        /// Full final assistant reply text.
        #[serde(skip_serializing_if = "String::is_empty")]
        reply: String,
        /// Short preview of the final reply text.
        #[serde(skip_serializing_if = "String::is_empty")]
        reply_preview: String,
    },

    /// Run has failed with an error.
    run_failed {
        run_id: String,
        error: String,
    },

    /// Run was cancelled by the user.
    run_cancelled {
        run_id: String,
        reason: String,
    },
}

/// Diff statistics for a file change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffStats {
    pub additions: i32,
    pub deletions: i32,
}

impl AgentRunEvent {
    /// Returns the run_id if present on this event variant.
    pub fn run_id(&self) -> &str {
        match self {
            Self::run_started { run_id, .. } => run_id,
            Self::step_started { run_id, .. } => run_id,
            Self::model_started { run_id, .. } => run_id,
            Self::model_delta { run_id, .. } => run_id,
            Self::model_completed { run_id, .. } => run_id,
            Self::tool_call_created { run_id, .. } => run_id,
            Self::tool_started { run_id, .. } => run_id,
            Self::command_output { run_id, .. } => run_id,
            Self::file_changed { run_id, .. } => run_id,
            Self::patch_started { run_id, .. } => run_id,
            Self::patch_hunk { run_id, .. } => run_id,
            Self::patch_applied { run_id, .. } => run_id,
            Self::patch_failed { run_id, .. } => run_id,
            Self::tool_completed { run_id, .. } => run_id,
            Self::approval_required { run_id, .. } => run_id,
            Self::run_completed { run_id, .. } => run_id,
            Self::run_failed { run_id, .. } => run_id,
            Self::run_cancelled { run_id, .. } => run_id,
        }
    }

    /// Returns the step_id if present on this event variant.
    pub fn step_id(&self) -> Option<&str> {
        match self {
            Self::run_started { .. } => None,
            Self::step_started { step_id, .. } => Some(step_id),
            Self::model_started { step_id, .. } => Some(step_id),
            Self::model_delta { step_id, .. } => Some(step_id),
            Self::model_completed { step_id, .. } => Some(step_id),
            Self::tool_call_created { step_id, .. } => Some(step_id),
            Self::tool_started { step_id, .. } => Some(step_id),
            Self::command_output { step_id, .. } => Some(step_id),
            Self::file_changed { step_id, .. } => Some(step_id),
            Self::patch_started { step_id, .. } => Some(step_id),
            Self::patch_hunk { step_id, .. } => Some(step_id),
            Self::patch_applied { step_id, .. } => Some(step_id),
            Self::patch_failed { step_id, .. } => Some(step_id),
            Self::tool_completed { step_id, .. } => Some(step_id),
            Self::approval_required { step_id, .. } => Some(step_id),
            Self::run_completed { .. } => None,
            Self::run_failed { .. } => None,
            Self::run_cancelled { .. } => None,
        }
    }
}
