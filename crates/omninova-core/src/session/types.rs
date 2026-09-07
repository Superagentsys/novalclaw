use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionEvent {
    pub seq: u64,
    pub ts: i64,
    pub kind: SessionEventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEventKind {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: String,
    },
    ToolResult {
        tool_call_id: String,
        content: String,
        #[serde(default)]
        interrupted: bool,
        /// Full original tool output when the model-visible content was pruned.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        original_content: Option<String>,
    },
    Compact {
        summary: String,
        hidden_through_seq: u64,
    },
    Interrupt {
        reason: String,
    },
}
