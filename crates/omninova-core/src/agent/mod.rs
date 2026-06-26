pub mod agent;
pub mod budget;
pub mod dispatcher;
pub mod history;
pub mod planner;
pub mod prompt;

pub use agent::Agent;
pub use budget::BudgetTracker;
pub use history::sanitize_messages_for_provider;

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
}
