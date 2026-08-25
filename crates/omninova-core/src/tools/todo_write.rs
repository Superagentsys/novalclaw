use crate::task::TaskStore;
use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;

pub struct TodoWriteTool {
    workspace: PathBuf,
    session_id: Option<String>,
}

impl TodoWriteTool {
    pub fn new(workspace: PathBuf, session_id: Option<String>) -> Self {
        Self {
            workspace,
            session_id,
        }
    }
}

#[async_trait]
impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "todo_write"
    }

    fn description(&self) -> &str {
        "Replace the session todo list. Use for multi-step work so later turns (and long-horizon wakes) can see what is left."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "content": { "type": "string" },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"]
                            }
                        },
                        "required": ["content", "status"]
                    }
                }
            },
            "required": ["items"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let Some(session_id) = self.session_id.as_deref() else {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("todo_write requires a session".to_string()),
            });
        };
        let items = args
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let store = match TaskStore::open(self.workspace.join(".omninova-tasks.db")) {
            Ok(store) => store,
            Err(error) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("failed to open todo store: {error}")),
                });
            }
        };
        match store.put_todos(session_id, &items) {
            Ok(()) => Ok(ToolResult {
                success: true,
                output: json!({ "count": items.len() }).to_string(),
                error: None,
            }),
            Err(error) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(error.to_string()),
            }),
        }
    }
}
