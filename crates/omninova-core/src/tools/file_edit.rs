use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::path::{Path, PathBuf};

pub struct FileEditTool {
    workspace_dir: PathBuf,
}

impl FileEditTool {
    pub fn new(workspace_dir: impl Into<PathBuf>) -> Self {
        Self {
            workspace_dir: workspace_dir.into(),
        }
    }

    async fn resolve_allowed_path(&self, relative: &str) -> anyhow::Result<PathBuf> {
        let rel = Path::new(relative);
        if rel.is_absolute() {
            anyhow::bail!("absolute paths are not allowed");
        }
        let full_path = self.workspace_dir.join(rel);
        let parent = full_path.parent().map(ToOwned::to_owned).unwrap_or_default();
        tokio::fs::create_dir_all(&parent).await?;
        let resolved = tokio::fs::canonicalize(&full_path).await.unwrap_or(full_path);
        let workspace = tokio::fs::canonicalize(&self.workspace_dir).await?;
        if !resolved.starts_with(&workspace) {
            anyhow::bail!("path escapes workspace");
        }
        Ok(resolved)
    }
}

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &str {
        "file_edit"
    }

    fn description(&self) -> &str {
        "Write file content. Supports overwrite or append modes."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" },
                "mode": { "type": "string", "enum": ["overwrite", "append"] }
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
        let mode = args
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("overwrite");

        let resolved = match self.resolve_allowed_path(path).await {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                });
            }
        };

        let write_result = match mode {
            "append" => {
                use tokio::io::AsyncWriteExt;
                let mut file = tokio::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&resolved)
                    .await?;
                file.write_all(content.as_bytes()).await
            }
            _ => tokio::fs::write(&resolved, content).await,
        };

        if let Err(e) = write_result {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to write file: {e}")),
            });
        }

        Ok(ToolResult {
            success: true,
            output: "ok".to_string(),
            error: None,
        })
    }
}
