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
        let names = extract_command_names(command);
        if names.is_empty() {
            anyhow::bail!("empty command");
        }
        for name in &names {
            if !self.allowed_commands.iter().any(|c| c == name) {
                anyhow::bail!("command '{name}' is not allowed");
            }
        }
        Ok(())
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

        if sandbox_enabled(&self.config) {
            if let Err(e) = ensure_sandbox_home(&self.config).await {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("sandbox init failed: {e}")),
                });
            }
        }

        let mut child = Command::new("sh");
        child
            .arg("-lc")
            .arg(command)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(windows)]
        {
            if let Some(new_path) = windows_augmented_path() {
                child.env("PATH", new_path);
            }
        }

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

/// Extract every command name that will be invoked by a shell command string.
/// Handles variable assignments (`VAR=value cmd`), subshells (`T=$(cmd)`/backticks),
/// and separators (`;`, `|`, `&`, `&&`, `||`, newline).
/// Used by both ShellTool and tool_policy to enforce the same allowlist logic.
pub(crate) fn extract_command_names(command: &str) -> Vec<String> {
    let mut names = Vec::new();
    for segment in command.split([';', '|', '&', '\n']) {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        // First, pull out any commands inside $(...) / backticks anywhere in the
        // segment (subshells can contain real commands regardless of position).
        extract_subshell_commands(segment, &mut names);

        // Then find the segment's own leading command: skip `VAR=value` assignment
        // prefixes, then take the first real command token. If the value of an
        // assignment opens a subshell (`T=$(...`), the actual command lives inside
        // it (already captured above) and there is no further leading command, so
        // we stop scanning this segment.
        let mut in_subshell_value = false;
        for token in segment.split_whitespace() {
            if let Some(pos) = token.find('=') {
                let lhs = &token[..pos];
                let is_assignment = !lhs.is_empty()
                    && lhs
                        .chars()
                        .next()
                        .map_or(false, |c| c.is_ascii_alphabetic() || c == '_')
                    && lhs.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                if is_assignment {
                    // If the rhs starts a subshell/backtick, the rest of the segment
                    // belongs to that construct — don't treat later tokens as commands.
                    let rhs = &token[pos + 1..];
                    if rhs.contains("$(") || rhs.contains('`') {
                        in_subshell_value = true;
                    }
                    continue;
                }
            }
            if in_subshell_value {
                break;
            }
            let cmd_name = token
                .trim_start_matches('`')
                .trim_start_matches("$(")
                .trim_start_matches('(');
            if !cmd_name.is_empty() && !cmd_name.starts_with('-') {
                names.push(cmd_name.to_string());
            }
            break;
        }
    }
    names.sort();
    names.dedup();
    names
}

fn extract_subshell_commands(s: &str, out: &mut Vec<String>) {
    // $( ... ) blocks (non-nested; good enough for allowlist policy).
    let mut rest = s;
    while let Some(start) = rest.find("$(") {
        rest = &rest[start + 2..];
        let end = rest.find(')').unwrap_or(rest.len());
        out.extend(leading_commands_only(&rest[..end]));
        rest = &rest[end.min(rest.len())..];
    }
    // Backtick subshells (odd-indexed split parts are inside backticks).
    for chunk in s.split('`').collect::<Vec<_>>().iter().skip(1).step_by(2) {
        out.extend(leading_commands_only(chunk));
    }
}

/// Extract just the leading command name(s) from each separator-delimited part of
/// a (sub)command string, without recursing back into subshell scanning.
fn leading_commands_only(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    for segment in s.split([';', '|', '&', '\n']) {
        for token in segment.split_whitespace() {
            if let Some(pos) = token.find('=') {
                let lhs = &token[..pos];
                let is_assignment = !lhs.is_empty()
                    && lhs
                        .chars()
                        .next()
                        .map_or(false, |c| c.is_ascii_alphabetic() || c == '_')
                    && lhs.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                if is_assignment {
                    continue;
                }
            }
            let name = token
                .trim_start_matches('`')
                .trim_start_matches("$(")
                .trim_start_matches('(')
                .trim_end_matches(')');
            if !name.is_empty() && !name.starts_with('-') {
                out.push(name.to_string());
            }
            break;
        }
    }
    out
}

/// Build an augmented PATH for spawning `sh -lc` on Windows, prepending the Git
/// for Windows tool directories (which provide curl/grep/sed/etc) so commands
/// work regardless of which shell launched the agent. Returns None if nothing to
/// add. Shared by ShellTool and the cron scheduler so both behave identically.
#[cfg(windows)]
pub(crate) fn windows_augmented_path() -> Option<String> {
    use std::path::{Path, PathBuf};
    let git_root = std::process::Command::new("where")
        .arg("git")
        .output()
        .ok()
        .and_then(|o| {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout
                .lines()
                .next()
                .map(str::trim)
                .and_then(|l| Path::new(l).parent()?.parent().map(PathBuf::from))
        });
    let extra: Vec<String> = if let Some(root) = git_root {
        ["mingw64/bin", "usr/bin", "bin"]
            .iter()
            .map(|s| root.join(s))
            .filter(|p| p.is_dir())
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    } else {
        [
            r"C:\Program Files\Git\mingw64\bin",
            r"C:\Program Files\Git\usr\bin",
            r"C:\Program Files\Git\bin",
        ]
        .iter()
        .filter(|p| Path::new(p).is_dir())
        .map(|p| p.to_string())
        .collect()
    };
    if extra.is_empty() {
        return None;
    }
    let cur = std::env::var("PATH").unwrap_or_default();
    Some(format!("{};{}", extra.join(";"), cur))
}
