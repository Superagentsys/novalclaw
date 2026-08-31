pub mod agent;
pub mod agent_event;
pub mod budget;
mod checkpoint_semantics;
pub mod context;
pub mod dispatcher;
pub mod event_bus;
pub mod history;
pub mod planner;
pub mod prompt;
pub mod run_control;
pub mod tool_runner;

#[cfg(test)]
mod r3_e2e;
#[cfg(test)]
mod r4_semantic;
#[cfg(test)]
mod r5_acceptance;

pub use agent::Agent;
pub use prompt::{bootstrap_system_messages, reconstruct_model_visible_messages};
pub use agent_event::{AgentRunEvent, DiffStats, StepStatus, ChangeType};
pub use budget::BudgetTracker;
pub use event_bus::{EventBus, EventBusDrainHandle, build_tool_summary, build_tool_prepare_summary, build_tool_start_summary, truncate_for_display, extract_diff_stats, compute_content_diff, TimedBlock};
pub use history::{sanitize_messages_for_provider, truncate_history_preserving_system};
pub use run_control::AgentCancellationToken;

/// Legacy event type — kept for internal dispatcher use.
/// Prefer `AgentRunEvent` from `agent_event` for new code.
#[derive(Debug, Clone)]
pub enum ToolExecutionEvent {
    Started {
        tool_call_id: String,
        tool_name: String,
        summary: String,
    },
    Completed {
        tool_call_id: String,
        tool_name: String,
        success: bool,
        duration_ms: u64,
        result_summary: String,
        diff_stats: Option<FileDiffStats>,
    },
    CommandOutput {
        tool_call_id: String,
        tool_name: String,
        output: String,
        is_stderr: bool,
    },
    FileChanged {
        path: String,
        additions: i32,
        deletions: i32,
    },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileDiffStats {
    pub additions: i32,
    pub deletions: i32,
}

/// Incremental events emitted while an agent turn runs, for streaming UIs.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// A streamed text delta of the (final) assistant answer.
    Token(String),
    /// A progress/tool step (human-readable).
    Step(String),
    /// The turn finished; carries the full final answer text.
    Done(String),
    /// The turn failed.
    Error(String),
    /// Tool execution details (richer than Step).
    ToolExecution(ToolExecutionEvent),
}
