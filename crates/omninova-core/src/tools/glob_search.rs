use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;

const MAX_RESULTS: usize = 1000;

pub struct GlobSearchTool {
    workspace_dir: PathBuf,
}

impl GlobSearchTool {
    pub fn new(workspace_dir: impl Into<PathBuf>) -> Self {
        Self {
            workspace_dir: workspace_dir.into(),
        }
    }
}

#[async_trait]
impl Tool for GlobSearchTool {
    fn name(&self) -> &str {
        "glob_search"
    }

    fn description(&self) -> &str {
        "Search for files matching a glob pattern within the workspace."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern (e.g. **/*.rs, src/**/*.ts)"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'pattern' parameter"))?;

        if pattern.contains("..") || pattern.starts_with('/') {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Pattern must not contain '..' or start with '/'".to_string()),
            });
        }

        let glob = match globset::GlobBuilder::new(pattern)
            .literal_separator(false)
            .build()
        {
            Ok(g) => g.compile_matcher(),
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Invalid glob pattern: {e}")),
                });
            }
        };

        let workspace = match tokio::fs::canonicalize(&self.workspace_dir).await {
            Ok(w) => w,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Cannot resolve workspace: {e}")),
                });
            }
        };

        let mut results = Vec::new();
        let mut stack = vec![workspace.clone()];
        while let Some(dir) = stack.pop() {
            let mut entries = match tokio::fs::read_dir(&dir).await {
                Ok(e) => e,
                Err(_) => continue,
            };
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.is_symlink() {
                    if let Ok(resolved) = tokio::fs::canonicalize(&path).await {
                        if !resolved.starts_with(&workspace) {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
                if path.is_dir() {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    if !name.starts_with('.') && name != "node_modules" && name != "target" {
                        stack.push(path);
                    }
                } else if let Ok(relative) = path.strip_prefix(&workspace) {
                    if glob.is_match(relative) {
                        results.push(relative.to_string_lossy().to_string());
                        if results.len() >= MAX_RESULTS {
                            break;
                        }
                    }
                }
            }
            if results.len() >= MAX_RESULTS {
                break;
            }
        }

        results.sort();
        let count = results.len();
        let output = if results.is_empty() {
            "No files matched the pattern.".to_string()
        } else {
            let truncated = if count >= MAX_RESULTS {
                format!("\n[truncated at {MAX_RESULTS} results]")
            } else {
                String::new()
            };
            format!("{}{truncated}\n[{count} files]", results.join("\n"))
        };

        Ok(ToolResult {
            success: true,
            output,
            error: None,
        })
    }
}
