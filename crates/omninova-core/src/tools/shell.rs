use crate::config::Config;
use crate::agent::AgentCancellationToken;
use crate::security::sandbox::{ensure_sandbox_home, normalize_workspace_path, sandbox_env, sandbox_enabled};
use crate::tools::configure_background_command;
use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tracing::debug;

fn now_ts() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let t = now.as_secs();
    let h = (t / 3600) % 24;
    let m = (t / 60) % 60;
    let s = t % 60;
    let ms = now.subsec_millis();
    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms)
}

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
                let normalized = normalize_workspace_path(&self.workspace_dir, p).await?;
                let rel = Path::new(&normalized);
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
        configure_background_command(&mut child);
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

    async fn execute_streaming_with_cancel(
        &self,
        args: serde_json::Value,
        output_tx: tokio::sync::mpsc::UnboundedSender<(String, bool)>,
        cancel_token: AgentCancellationToken,
    ) -> anyhow::Result<ToolResult> {
        cancel_token.check()?;
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'command' parameter"))?
            .to_string();
        self.execute_streaming_impl(command, output_tx, false, Some(cancel_token))
            .await
    }
}

/// Dev-only streaming execution that skips allowlist checks.
/// Used exclusively by `debug_shell_stream` to test the full streaming pipeline.
/// Panics if called from production code.
impl ShellTool {
    pub(crate) async fn execute_streaming_unchecked(
        &self,
        command: String,
        output_tx: tokio::sync::mpsc::UnboundedSender<(String, bool)>,
    ) -> anyhow::Result<ToolResult> {
        #[cfg(debug_assertions)]
        {
            self.execute_streaming_impl(command, output_tx, true, None).await
        }
        #[cfg(not(debug_assertions))]
        {
            let _ = (command, output_tx);
            panic!("execute_streaming_unchecked must not be called in release builds");
        }
    }
}

/// Streaming shell tool execution with real-time output via channel.
impl ShellTool {
    /// Common implementation shared by both checked and unchecked streaming paths.
    async fn execute_streaming_impl(
        &self,
        command: String,
        output_tx: tokio::sync::mpsc::UnboundedSender<(String, bool)>,
        bypass_allowlist: bool,
        cancel_token: Option<AgentCancellationToken>,
    ) -> anyhow::Result<ToolResult> {
        let working_directory: Option<&str> = None;
        let timeout_secs = self.timeout_secs;

        if !bypass_allowlist {
            if let Err(e) = self.check_command_allowed(&command) {
                let content = e.to_string();
                debug!(target: "e2e", "[e2e-shell-send] timestamp={} is_stderr=true content=\"{}\"", now_ts(), preview(&content, 80));
                let _ = output_tx.send((content, true));
                return Ok(ToolResult { success: false, output: String::new(), error: Some(e.to_string()) });
            }
        }

        let cwd = match self.resolve_working_directory(working_directory).await {
            Ok(c) => c,
            Err(e) => {
                let content = format!("error: {e}");
                debug!(target: "e2e", "[e2e-shell-send] timestamp={} is_stderr=true content=\"{}\"", now_ts(), preview(&content, 80));
                let _ = output_tx.send((content, true));
                return Ok(ToolResult { success: false, output: String::new(), error: Some(e.to_string()) });
            }
        };

        let workspace = match tokio::fs::canonicalize(&self.workspace_dir).await {
            Ok(p) => p,
            Err(e) => {
                let content = format!("error: {e}");
                debug!(target: "e2e", "[e2e-shell-send] timestamp={} is_stderr=true content=\"{}\"", now_ts(), preview(&content, 80));
                let _ = output_tx.send((content, true));
                return Ok(ToolResult { success: false, output: String::new(), error: Some(format!("failed to resolve workspace dir: {e}")) });
            }
        };

        if self.command_targets_workspace_root(&command, &cwd, &workspace) {
            let content = "error: refused to delete workspace root".to_string();
            debug!(target: "e2e", "[e2e-shell-send] timestamp={} is_stderr=true content=\"{}\"", now_ts(), preview(&content, 80));
            let _ = output_tx.send((content, true));
            return Ok(ToolResult { success: false, output: String::new(), error: Some("Refusing to delete the Workspace root directory. Delete files inside the Workspace instead.".to_string()) });
        }

        if sandbox_enabled(&self.config) {
            if let Err(e) = ensure_sandbox_home(&self.config).await {
                let content = format!("sandbox error: {e}");
                debug!(target: "e2e", "[e2e-shell-send] timestamp={} is_stderr=true content=\"{}\"", now_ts(), preview(&content, 80));
                let _ = output_tx.send((content, true));
                return Ok(ToolResult { success: false, output: String::new(), error: Some(format!("sandbox init failed: {e}")) });
            }
        }

        let mut child = if bypass_allowlist {
            // Dev/debug path: bypass allowlist. Use cmd.exe /C to run arbitrary commands
            // reliably across all environments (Tauri, dev server, etc.).
            let mut c = Command::new("cmd.exe");
            c.args(["/C", &command]);
            debug!(target: "e2e", "[e2e-debug-spawn] program=cmd.exe args=/C {} cwd={}", preview(&command, 80), cwd.display());
            c
        } else if cfg!(target_os = "windows") {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-NonInteractive", "-Command", &command]);
            c
        } else {
            let mut c = Command::new("sh");
            c.arg("-lc").arg(&command);
            c
        };
        configure_background_command(&mut child);
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

        let mut child = match child.spawn() {
            Ok(c) => c,
            Err(e) => {
                let content = format!("spawn error: {e}");
                debug!(target: "e2e", "[e2e-shell-send] timestamp={} is_stderr=true content=\"{}\"", now_ts(), preview(&content, 80));
                let _ = output_tx.send((content, true));
                return Ok(ToolResult { success: false, output: String::new(), error: Some(format!("failed to spawn command: {e}")) });
            }
        };

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        let mut running = true;

        // Spawn stdout reader.
        let out_tx = output_tx.clone();
        if let Some(out) = stdout {
            tokio::spawn(async move {
                                use tracing::debug;
                let mut reader = BufReader::new(out);
                let mut line = String::new();
                while let Ok(n) = reader.read_line(&mut line).await {
                    if n == 0 { break; }
                    let content = std::mem::take(&mut line);
                    debug!(target: "e2e", "[e2e-shell-send] timestamp={} is_stderr=false content=\"{}\"", now_ts(), preview(&content, 80));
                    let _ = out_tx.send((content, false));
                }
            });
        }

        // Spawn stderr reader.
        let err_tx = output_tx.clone();
        if let Some(err) = stderr {
            tokio::spawn(async move {
                                use tracing::debug;
                let mut reader = BufReader::new(err);
                let mut line = String::new();
                while let Ok(n) = reader.read_line(&mut line).await {
                    if n == 0 { break; }
                    let content = std::mem::take(&mut line);
                    debug!(target: "e2e", "[e2e-shell-send] timestamp={} is_stderr=true content=\"{}\"", now_ts(), preview(&content, 80));
                    let _ = err_tx.send((content, true));
                }
            });
        }

        // Wait for child to exit with timeout.
        while Instant::now() < deadline && running {
            let remaining = deadline - Instant::now();
            let wait_result = if let Some(token) = cancel_token.clone() {
                tokio::select! {
                    result = tokio::time::timeout(remaining, child.wait()) => result,
                    _ = token.cancelled() => {
                        let _ = child.kill().await;
                        let content = "cancelled by user".to_string();
                        debug!(target: "e2e", "[e2e-shell-send] timestamp={} is_stderr=true content=\"{}\"", now_ts(), preview(&content, 80));
                        let _ = output_tx.send((content, true));
                        return Ok(ToolResult {
                            success: false,
                            output: String::new(),
                            error: Some("agent run cancelled".to_string()),
                        });
                    }
                }
            } else {
                tokio::time::timeout(remaining, child.wait()).await
            };
            match wait_result {
                Ok(Ok(status)) => {
                    running = false;
                    if !status.success() {
                        let content = format!("exited with status {}", status.code().unwrap_or(-1));
                        debug!(target: "e2e", "[e2e-shell-send] timestamp={} is_stderr=true content=\"{}\"", now_ts(), preview(&content, 80));
                        let _ = output_tx.send((content, true));
                    }
                    return Ok(ToolResult {
                        success: status.success(),
                        output: String::new(),
                        error: if status.success() { None } else { Some(format!("command exited with status {}", status.code().unwrap_or(-1))) },
                    });
                }
                Ok(Err(e)) => {
                    running = false;
                    let content = format!("wait error: {e}");
                    debug!(target: "e2e", "[e2e-shell-send] timestamp={} is_stderr=true content=\"{}\"", now_ts(), preview(&content, 80));
                    let _ = output_tx.send((content, true));
                    return Ok(ToolResult { success: false, output: String::new(), error: Some(format!("wait error: {e}")) });
                }
                Err(_) => {
                    let _ = child.kill().await;
                    let content = format!("timed out after {timeout_secs}s");
                    debug!(target: "e2e", "[e2e-shell-send] timestamp={} is_stderr=true content=\"{}\"", now_ts(), preview(&content, 80));
                    let _ = output_tx.send((content, true));
                    return Ok(ToolResult { success: false, output: String::new(), error: Some(format!("command timed out after {timeout_secs}s")) });
                }
            }
        }

        if running {
            let _ = child.kill().await;
            let content = format!("timed out after {timeout_secs}s");
            debug!(target: "e2e", "[e2e-shell-send] timestamp={} is_stderr=true content=\"{}\"", now_ts(), preview(&content, 80));
            let _ = output_tx.send((content, true));
            return Ok(ToolResult { success: false, output: String::new(), error: Some(format!("command timed out after {timeout_secs}s")) });
        }

        Ok(ToolResult { success: false, output: String::new(), error: Some("unexpected exit".into()) })
    }
}

/// E2E debug: returns the first `max_chars` chars of `s`, appending "..."
/// if `s` was longer. Never slices in the middle of a UTF-8 codepoint.
fn preview(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}
