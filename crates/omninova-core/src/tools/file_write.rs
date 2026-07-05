use crate::security::sandbox::resolve_workspace_relative;
use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde::Serialize;
use serde_json::json;
use std::path::PathBuf;

const PREVIEW_MAX_CHARS: usize = 12_000;
const PREVIEW_MAX_LINES: usize = 300;

#[derive(Debug, Clone, Serialize)]
struct TextPreview {
    text: String,
    truncated: bool,
    total_chars: usize,
    preview_chars: usize,
}

#[derive(Debug, Clone, Serialize)]
struct FileWriteOutput {
    message: String,
    path: String,
    bytes: usize,
    change_type: String,
    additions: i32,
    deletions: i32,
    old_text: Option<String>,
    new_text: Option<String>,
    content_truncated: bool,
    content_total_chars: usize,
    content_preview_chars: usize,
}

pub struct FileWriteTool {
    workspace_dir: PathBuf,
}

impl FileWriteTool {
    pub fn new(workspace_dir: impl Into<PathBuf>) -> Self {
        Self {
            workspace_dir: workspace_dir.into(),
        }
    }
}

fn count_lines(text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        text.lines().count().max(1)
    }
}

fn preview_text(input: &str) -> TextPreview {
    let total_chars = input.chars().count();
    let mut text = String::new();
    let mut line_count = 1usize;
    let mut truncated = false;

    for (index, ch) in input.chars().enumerate() {
        if index >= PREVIEW_MAX_CHARS || line_count > PREVIEW_MAX_LINES {
            truncated = true;
            break;
        }
        text.push(ch);
        if ch == '\n' {
            line_count += 1;
        }
    }

    TextPreview {
        preview_chars: text.chars().count(),
        text,
        truncated: truncated || total_chars > PREVIEW_MAX_CHARS,
        total_chars,
    }
}

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "file_write"
    }

    fn description(&self) -> &str {
        "Write content to a file inside workspace. Path must be workspace-relative; use \"index.html\" or \"sub/file.txt\", never an absolute path like D:\\project\\index.html. Prefer file_patch for existing files unless a full rewrite is explicitly requested."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Workspace-relative path. Use \"index.html\", not D:\\\\workspace\\\\index.html." },
                "content": { "type": "string", "description": "Content to write" }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'content' parameter"))?;

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

        let old_content = match tokio::fs::read_to_string(&resolved).await {
            Ok(existing) => Some(existing),
            Err(_) => None,
        };

        if let Err(e) = tokio::fs::write(&resolved, content).await {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to write file: {e}")),
            });
        }

        let old_preview = old_content.as_deref().map(preview_text);
        let new_preview = preview_text(content);
        let old_lines = old_content.as_deref().map(count_lines).unwrap_or(0);
        let new_lines = count_lines(content);
        let change_type = if old_content.is_some() { "modified" } else { "created" };
        let output = FileWriteOutput {
            message: format!("Wrote {} bytes to {path}", content.len()),
            path: path.to_string(),
            bytes: content.len(),
            change_type: change_type.to_string(),
            additions: new_lines as i32,
            deletions: if old_content.is_some() { old_lines as i32 } else { 0 },
            old_text: old_preview.as_ref().map(|preview| preview.text.clone()),
            new_text: Some(new_preview.text.clone()),
            content_truncated: old_preview
                .as_ref()
                .map(|preview| preview.truncated)
                .unwrap_or(false)
                || new_preview.truncated,
            content_total_chars: old_preview
                .as_ref()
                .map(|preview| preview.total_chars)
                .unwrap_or(0)
                + new_preview.total_chars,
            content_preview_chars: old_preview
                .as_ref()
                .map(|preview| preview.preview_chars)
                .unwrap_or(0)
                + new_preview.preview_chars,
        };

        Ok(ToolResult {
            success: true,
            output: serde_json::to_string(&output)?,
            error: None,
        })
    }
}
