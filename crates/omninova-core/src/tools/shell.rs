use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const MAX_OUTPUT_BYTES: usize = 128 * 1024;

pub struct ShellTool {
    workspace_dir: PathBuf,
    allowed_commands: Vec<String>,
    timeout_secs: u64,
}

impl ShellTool {
    pub fn new(
        workspace_dir: impl Into<PathBuf>,
        allowed_commands: Vec<String>,
        timeout_secs: Option<u64>,
    ) -> Self {
        Self {
            workspace_dir: workspace_dir.into(),
            allowed_commands,
            timeout_secs: timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS).max(1),
        }
    }

    async fn resolve_working_directory(&self, relative: Option<&str>) -> anyhow::Result<PathBuf> {
        let wd = match relative {
            Some(p) if !p.trim().is_empty() => {
                let rel = Path::new(p);
                if rel.is_absolute() {
                    anyhow::bail!("absolute working_directory is not allowed");
                }
                self.workspace_dir.join(rel)
            }
            _ => self.workspace_dir.clone(),
        };

        let resolved = tokio::fs::canonicalize(&wd)
            .await
            .map_err(|e| anyhow::anyhow!("failed to resolve working directory: {e}"))?;
        let workspace = tokio::fs::canonicalize(&self.workspace_dir).await?;
        if !resolved.starts_with(&workspace) {
            anyhow::bail!("working_directory escapes workspace");
        }
        Ok(resolved)
    }

    fn check_command_allowed(&self, command: &str) -> anyhow::Result<()> {
        let first = command
            .split_whitespace()
            .next()
            .ok_or_else(|| anyhow::anyhow!("empty command"))?;
        if self.allowed_commands.iter().any(|c| c == first) {
            Ok(())
        } else {
            anyhow::bail!("command '{first}' is not allowed")
        }
    }

    fn truncate_output(s: String) -> String {
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
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Run safe shell commands inside workspace with allowlist and timeout."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string" },
                "working_directory": { "type": "string" },
                "timeout_secs": { "type": "integer", "minimum": 1 }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'command' parameter"))?;
        let working_directory = args.get("working_directory").and_then(|v| v.as_str());
        let timeout_secs = args
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.timeout_secs)
            .max(1);

        if let Err(e) = self.check_command_allowed(command) {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(e.to_string()),
            });
        }
        let cwd = match self.resolve_working_directory(working_directory).await {
            Ok(cwd) => cwd,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                });
            }
        };

        let mut child = Command::new("sh");
        child
            .arg("-lc")
            .arg(command)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = match timeout(Duration::from_secs(timeout_secs), child.output()).await {
            Ok(exec_result) => match exec_result {
                Ok(output) => output,
                Err(e) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("failed to execute command: {e}")),
                    });
                }
            },
            Err(_) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("command timed out after {timeout_secs}s")),
                });
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let merged = if stderr.trim().is_empty() {
            stdout
        } else if stdout.trim().is_empty() {
            stderr
        } else {
            format!("{stdout}\n{stderr}")
        };
        let merged = Self::truncate_output(merged);

        Ok(ToolResult {
            success: output.status.success(),
            output: merged,
            error: if output.status.success() {
                None
            } else {
                Some(format!(
                    "command exited with status {}",
                    output.status.code().unwrap_or(-1)
                ))
            },
        })
    }
}
