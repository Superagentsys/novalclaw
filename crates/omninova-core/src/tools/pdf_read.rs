use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;

pub struct PdfReadTool {
    workspace_dir: PathBuf,
}

impl PdfReadTool {
    pub fn new(workspace_dir: impl Into<PathBuf>) -> Self {
        Self {
            workspace_dir: workspace_dir.into(),
        }
    }
}

#[async_trait]
impl Tool for PdfReadTool {
    fn name(&self) -> &str {
        "pdf_read"
    }

    fn description(&self) -> &str {
        "Read and extract text content from a PDF file within the workspace."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Relative path to the PDF file" }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let path_str = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;

        if path_str.contains("..") || path_str.starts_with('/') {
             return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Path must be relative and not contain '..'".to_string()),
            });
        }

        let file_path = self.workspace_dir.join(path_str);
        if !file_path.exists() {
             return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("File not found: {}", path_str)),
            });
        }

        // pdf-extract is synchronous, so we should run it in spawn_blocking
        let path_clone = file_path.clone();
        let result = tokio::task::spawn_blocking(move || {
            pdf_extract::extract_text(&path_clone)
        }).await?;

        match result {
            Ok(text) => Ok(ToolResult {
                success: true,
                output: text,
                error: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to extract text from PDF: {}", e)),
            }),
        }
    }
}
