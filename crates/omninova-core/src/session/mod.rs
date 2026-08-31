//! Per-session event log.
//!
//! The JSONL file is the durable source of truth. Messages sent to the model
//! are a projection of that log. The legacy `.omninova-sessions.json` blob is
//! still written as an index so session-tree APIs keep working.

mod log;
mod store;
mod types;

pub use log::{derive_messages, events_from_messages, repair_unclosed_tools};
pub use store::{delete_messages, load_messages, save_messages, session_log_path};
pub use types::{SessionEvent, SessionEventKind};

use crate::agent::history::{CHECKPOINT_MARKER, SUMMARY_MARKER, TASK_MARKER};

pub fn is_pinned_content(content: &str) -> bool {
    content.starts_with(TASK_MARKER) || content.starts_with(CHECKPOINT_MARKER)
}

pub fn is_summary_content(content: &str) -> bool {
    content.starts_with(SUMMARY_MARKER)
}
