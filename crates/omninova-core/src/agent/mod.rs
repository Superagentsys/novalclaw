pub mod agent;
pub mod budget;
pub mod dispatcher;
pub mod history;
pub mod planner;
pub mod prompt;

pub use agent::Agent;
pub use budget::BudgetTracker;
pub use history::sanitize_messages_for_provider;

/// Detailed event emitted during tool execution, forwarded to the UI timeline.
#[derive(Debug, Clone)]
pub enum ToolExecutionEvent {
    /// Tool execution started.
    Started {
        tool_name: String,
        summary: String,
    },
    /// Tool execution completed.
    Completed {
        tool_name: String,
        success: bool,
        duration_ms: u64,
        /// Short result summary for display (may be truncated).
        result_summary: String,
        /// For file write/edit: diff stats if git is available.
        diff_stats: Option<FileDiffStats>,
    },
    /// A file was modified (write / edit).
    FileChanged {
        path: String,
        additions: i32,
        deletions: i32,
    },
}

/// Diff statistics for a modified file.
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
