use crate::config::Config;
use crate::security::sandbox::{ensure_sandbox_home, sandbox_env, sandbox_enabled};
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
    config: Config,
}

impl ShellTool {
    pub fn new(
        workspace_dir: impl Into<PathBuf>,
        allowed_commands: Vec<String>,
        timeout_secs: Option<u64>,
        config: Config,
    ) -> Self {
        Self {
            workspace_dir: workspace_dir.into(),
            allowed_commands,
            timeout_secs: timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS).max(1),
            config,
        }
    }

    async fn resolve_working_directory(&self, relative: Option<&str>) -> anyhow::Result<PathBuf> {
        let wd = match relative {
            Some(p) if !p.trim().is_empty() => {
                let rel = Path::new(p);
                if rel.is_absolute() {
                    anyhow::bail!("absolute working_directory is not allowed");
                }
                if rel.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                    anyhow::bail!("path traversal is not allowed");
                }
                if !self.workspace_dir.exists() {
                    tokio::fs::create_dir_all(&self.workspace_dir).await.map_err(|e| {
                        anyhow::anyhow!("workspace dir does not exist and could not be created: {e}")
                    })?;
                }
                let workspace_canon = tokio::fs::canonicalize(&self.workspace_dir).await?;
                workspace_canon.join(rel)
            }
            _ => {
                if !self.workspace_dir.exists() {
                    tokio::fs::create_dir_all(&self.workspace_dir).await.map_err(|e| {
                        anyhow::anyhow!("workspace dir does not exist and could not be created: {e}")
                    })?;
                }
                tokio::fs::canonicalize(&self.workspace_dir).await?
            }
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

    fn is_remove_command(command: &str) -> bool {
        let lower = command.to_ascii_lowercase();
        lower.split_whitespace().any(|token| {
            matches!(
                token.trim_matches(|c| c == '"' || c == '\'' || c == '&' || c == ';'),
                "rm" | "rmdir" | "rd" | "remove-item" | "del"
            )
        })
    }

    fn normalize_command_path_token(token: &str) -> String {
        let trimmed = token
            .trim_matches(|c| {
                matches!(
                    c,
                    '"' | '\'' | '`' | ',' | ';' | '(' | ')' | '[' | ']'
                )
            })
            .trim_end_matches(['\\', '/'])
            .to_ascii_lowercase();
        trimmed
            .strip_prefix("-path=")
            .or_else(|| trimmed.strip_prefix("-literalpath="))
            .unwrap_or(&trimmed)
            .to_string()
    }

    fn command_targets_workspace_root(
        &self,
        command: &str,
        cwd: &Path,
        workspace: &Path,
    ) -> bool {
        if !Self::is_remove_command(command) {
            return false;
        }

        let cwd_is_workspace = cwd == workspace;
        let workspace_str = workspace
            .to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_ascii_lowercase();

        command.split_whitespace().any(|raw| {
            let token = Self::normalize_command_path_token(raw);
            if token.is_empty() || token.contains('*') {
                return false;
            }
            if cwd_is_workspace && matches!(token.as_str(), "." | "./" | ".\\") {
                return true;
            }
            token == workspace_str
        })
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
        let workspace = match tokio::fs::canonicalize(&self.workspace_dir).await {
            Ok(path) => path,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("failed to resolve workspace dir: {e}")),
                });
            }
        };
        if self.command_targets_workspace_root(command, &cwd, &workspace) {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(
                    "Refusing to delete the Workspace root directory. Delete files inside the Workspace instead.".to_string(),
                ),
            });
        }

        if sandbox_enabled(&self.config) {
            if let Err(e) = ensure_sandbox_home(&self.config).await {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("sandbox init failed: {e}")),
                });
            }
        }

        // On Windows there is no `sh -lc`; route the command through PowerShell
        // so default allow-listed commands like `pwd`, `ls`, `cat`, `git`
        // continue to work without each caller having to know the platform.
        let mut child = if cfg!(target_os = "windows") {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-NonInteractive", "-Command", command]);
            c
        } else {
            let mut c = Command::new("sh");
            c.arg("-lc").arg(command);
            c
        };
        child
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if sandbox_enabled(&self.config) && self.config.security.sandbox.strip_environment {
            child.env_clear();
            for (key, value) in sandbox_env(&self.config) {
                child.env(key, value);
            }
        }

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
