use crate::security::sandbox::resolve_workspace_relative;
use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;

const MAX_FILE_SIZE_BYTES: u64 = 10 * 1024 * 1024;
const DEFAULT_LINE_LIMIT: usize = 400;
const MAX_LINE_LIMIT: usize = 2_000;

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
        "Read text or extract DOC/DOCX/PPTX/XLSX/PDF document content locally with line numbers. Path must be workspace-relative; use '.' for the workspace root and never pass an absolute path like D:\\project."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Workspace-relative file path. Use \"index.html\", not D:\\\\workspace\\\\index.html." },
                "offset": { "type": "integer", "minimum": 1, "description": "1-based first line (default: 1)" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 2000, "description": "Maximum lines to return (default: 400, maximum: 2000)" }
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

        let contents = match crate::document_text::read(&resolved).await {
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
            .map(|v| {
                usize::try_from(v.max(1))
                    .unwrap_or(usize::MAX)
                    .saturating_sub(1)
            })
            .unwrap_or(0);
        let start = offset.min(total);

        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or(DEFAULT_LINE_LIMIT)
            .clamp(1, MAX_LINE_LIMIT);
        let end = start.saturating_add(limit).min(total);

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

        let summary = if end < total {
            format!(
                "\n[Lines {}-{} of {total}; continue with offset={} and limit={limit}]",
                start + 1,
                end,
                end + 1
            )
        } else if start > 0 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn default_read_is_bounded_and_returns_continuation_offset() {
        let root =
            std::env::temp_dir().join(format!("omninova-file-read-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let body = (1..=450)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(root.join("large.txt"), body).unwrap();

        let result = FileReadTool::new(&root)
            .execute(json!({"path": "large.txt"}))
            .await
            .unwrap();
        assert!(result.success, "{:?}", result.error);
        assert!(result.output.contains("400: line 400"));
        assert!(!result.output.contains("401: line 401"));
        assert!(result.output.contains("continue with offset=401"));
        let _ = std::fs::remove_dir_all(root);
    }
}
