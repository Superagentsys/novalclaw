use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

const MAX_OUTPUT_BYTES: usize = 128 * 1024;

pub struct GitOperationsTool {
    workspace_dir: PathBuf,
}

impl GitOperationsTool {
    pub fn new(workspace_dir: impl Into<PathBuf>) -> Self {
        Self {
            workspace_dir: workspace_dir.into(),
        }
    }

    fn truncate(s: String) -> String {
        if s.len() <= MAX_OUTPUT_BYTES {
            return s;
        }
        let mut out = s;
        out.truncate(MAX_OUTPUT_BYTES);
        out.push_str("\n\n[output truncated]");
        out
    }
}

#[async_trait]
impl Tool for GitOperationsTool {
    fn name(&self) -> &str {
        "git_operations"
    }

    fn description(&self) -> &str {
        "Perform structured Git operations: status, diff, log, branch, add, commit, checkout, stash."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["status", "diff", "log", "branch", "add", "commit", "checkout", "stash"]
                },
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Additional arguments for the operation"
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let operation = args
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'operation' parameter"))?;

        let extra_args: Vec<String> = args
            .get("args")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        for arg in &extra_args {
            if arg.contains("&&") || arg.contains(';') || arg.contains('|') || arg.contains('`') {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some("Shell injection characters are not allowed in git arguments".to_string()),
                });
            }
        }

        let allowed = ["status", "diff", "log", "branch", "add", "commit", "checkout", "stash"];
        if !allowed.contains(&operation) {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Unsupported git operation: {operation}")),
            });
        }

        let mut cmd = Command::new("git");
        cmd.arg(operation);
        for arg in &extra_args {
            cmd.arg(arg);
        }
        cmd.current_dir(&self.workspace_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        match cmd.output().await {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let merged = if stderr.trim().is_empty() {
                    stdout
                } else if stdout.trim().is_empty() {
                    stderr
                } else {
                    format!("{stdout}\n{stderr}")
                };

                Ok(ToolResult {
                    success: output.status.success(),
                    output: Self::truncate(merged),
                    error: if output.status.success() {
                        None
                    } else {
                        Some(format!("git {operation} exited with status {}", output.status.code().unwrap_or(-1)))
                    },
                })
            }
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to execute git: {e}")),
            }),
        }
    }
}
