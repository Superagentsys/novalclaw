use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

const MAX_OUTPUT_BYTES: usize = 128 * 1024;

pub struct ContentSearchTool {
    workspace_dir: PathBuf,
}

impl ContentSearchTool {
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
impl Tool for ContentSearchTool {
    fn name(&self) -> &str {
        "content_search"
    }

    fn description(&self) -> &str {
        "Search file contents by regex pattern within the workspace. Uses ripgrep if available, otherwise falls back to grep."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Regex pattern to search for" },
                "path": { "type": "string", "description": "Subdirectory to search in (relative)" },
                "include": { "type": "string", "description": "File glob filter (e.g. *.rs)" },
                "case_sensitive": { "type": "boolean" },
                "max_results": { "type": "integer", "minimum": 1, "maximum": 500 },
                "context_before": { "type": "integer", "minimum": 0 },
                "context_after": { "type": "integer", "minimum": 0 }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'pattern' parameter"))?;
        let sub_path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let include = args.get("include").and_then(|v| v.as_str());
        let case_sensitive = args.get("case_sensitive").and_then(|v| v.as_bool()).unwrap_or(true);
        let max_results = args.get("max_results").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
        let context_before = args.get("context_before").and_then(|v| v.as_u64()).unwrap_or(0);
        let context_after = args.get("context_after").and_then(|v| v.as_u64()).unwrap_or(0);

        if sub_path.contains("..") || sub_path.starts_with('/') {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Path must be relative and not contain '..'".to_string()),
            });
        }

        let search_dir = self.workspace_dir.join(sub_path);
        let has_rg = Command::new("rg")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);

        let output = if has_rg {
            let mut cmd = Command::new("rg");
            cmd.arg("--line-number").arg("--no-heading").arg("--color=never");
            if !case_sensitive {
                cmd.arg("-i");
            }
            cmd.arg(format!("-m{max_results}"));
            if context_before > 0 {
                cmd.arg(format!("-B{context_before}"));
            }
            if context_after > 0 {
                cmd.arg(format!("-A{context_after}"));
            }
            if let Some(glob) = include {
                cmd.arg("-g").arg(glob);
            }
            cmd.arg(pattern).arg(&search_dir);
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            cmd.output().await
        } else {
            let mut cmd = Command::new("grep");
            cmd.arg("-rn").arg("-E");
            if !case_sensitive {
                cmd.arg("-i");
            }
            if context_before > 0 {
                cmd.arg(format!("-B{context_before}"));
            }
            if context_after > 0 {
                cmd.arg(format!("-A{context_after}"));
            }
            if let Some(glob) = include {
                cmd.arg("--include").arg(glob);
            }
            cmd.arg(pattern).arg(&search_dir);
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            cmd.output().await
        };

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                if stdout.trim().is_empty() && stderr.trim().is_empty() {
                    return Ok(ToolResult {
                        success: true,
                        output: "No matches found.".to_string(),
                        error: None,
                    });
                }
                let merged = if stderr.trim().is_empty() {
                    stdout
                } else {
                    format!("{stdout}\n{stderr}")
                };
                Ok(ToolResult {
                    success: true,
                    output: Self::truncate(merged),
                    error: None,
                })
            }
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Search command failed: {e}")),
            }),
        }
    }
}
