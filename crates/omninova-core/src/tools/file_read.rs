use crate::security::sandbox::resolve_workspace_relative;
use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;

const MAX_FILE_SIZE_BYTES: u64 = 10 * 1024 * 1024;

pub struct FileReadTool {
    workspace_dir: PathBuf,
}

impl FileReadTool {
    pub fn new(workspace_dir: impl Into<PathBuf>) -> Self {
        Self {
            workspace_dir: workspace_dir.into(),
        }
    }
}

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        "Read file contents with line numbers. Supports partial reading via offset and limit."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "offset": { "type": "integer" },
                "limit": { "type": "integer" }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;

        let resolved = match resolve_workspace_relative(&self.workspace_dir, path).await {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                });
            }
        };

        match tokio::fs::metadata(&resolved).await {
            Ok(meta) if meta.len() > MAX_FILE_SIZE_BYTES => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!(
                        "File too large: {} bytes (limit: {MAX_FILE_SIZE_BYTES} bytes)",
                        meta.len()
                    )),
                });
            }
            Ok(_) => {}
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Failed to read file metadata: {e}")),
                });
            }
        }

        let contents = match tokio::fs::read_to_string(&resolved).await {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Failed to read file: {e}")),
                });
            }
        };

        let lines: Vec<&str> = contents.lines().collect();
        let total = lines.len();
        if total == 0 {
            return Ok(ToolResult {
                success: true,
                output: String::new(),
                error: None,
            });
        }

        let offset = args
            .get("offset")
            .and_then(|v| v.as_u64())
            .map(|v| usize::try_from(v.max(1)).unwrap_or(usize::MAX).saturating_sub(1))
            .unwrap_or(0);
        let start = offset.min(total);

        let end = match args.get("limit").and_then(|v| v.as_u64()) {
            Some(l) => {
                let limit = usize::try_from(l).unwrap_or(usize::MAX);
                (start.saturating_add(limit)).min(total)
            }
            None => total,
        };

        if start >= end {
            return Ok(ToolResult {
                success: true,
                output: format!("[No lines in range, file has {total} lines]"),
                error: None,
            });
        }

        let numbered = lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{}: {}", start + i + 1, line))
            .collect::<Vec<_>>()
            .join("\n");

        let summary = if start > 0 || end < total {
            format!("\n[Lines {}-{} of {total}]", start + 1, end)
        } else {
            format!("\n[{total} lines total]")
        };

        Ok(ToolResult {
            success: true,
            output: format!("{numbered}{summary}"),
            error: None,
        })
    }
}
