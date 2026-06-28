use crate::security::sandbox::resolve_workspace_relative;
use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;

const MAX_ENTRIES: usize = 1024;
const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", ".omninova-sandbox-home"];

pub struct FileListTool {
    workspace_dir: PathBuf,
}

impl FileListTool {
    pub fn new(workspace_dir: impl Into<PathBuf>) -> Self {
        Self {
            workspace_dir: workspace_dir.into(),
        }
    }

    /// Recursively list files inside the workspace, prefixed with their
    /// workspace-relative path. Hidden directories and large VCS / build
    /// trees are skipped by default to keep the output usable.
    fn collect(workspace: &std::path::Path, current: &std::path::Path, out: &mut Vec<String>) -> std::io::Result<()> {
        if out.len() >= MAX_ENTRIES {
            return Ok(());
        }
        for entry in std::fs::read_dir(current)? {
            if out.len() >= MAX_ENTRIES {
                break;
            }
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if SKIP_DIRS.iter().any(|skip| *skip == name_str) {
                continue;
            }
            let file_type = entry.file_type()?;
            let path = entry.path();
            let rel = path
                .strip_prefix(workspace)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if file_type.is_dir() {
                out.push(format!("{rel}/"));
                Self::collect(workspace, &path, out)?;
            } else if file_type.is_file() {
                out.push(rel.to_string());
            } else if file_type.is_symlink() {
                out.push(format!("{rel}@"));
            }
        }
        Ok(())
    }
}

#[async_trait]
impl Tool for FileListTool {
    fn name(&self) -> &str {
        "file_list"
    }

    fn description(&self) -> &str {
        "List files and directories inside the workspace. By default lists the workspace root recursively."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Optional relative subdirectory inside workspace; default is the workspace root."
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let relative = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let resolved = match resolve_workspace_relative(&self.workspace_dir, relative).await {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                });
            }
        };

        let meta = match tokio::fs::metadata(&resolved).await {
            Ok(m) => m,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("failed to stat {relative}: {e}")),
                });
            }
        };
        if !meta.is_dir() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("{relative} is not a directory")),
            });
        }

        // Use blocking collection under spawn_blocking since std::fs::read_dir is sync.
        let workspace_root = self.workspace_dir.clone();
        let target = resolved;
        let collect_result = tokio::task::spawn_blocking(move || {
            let mut out: Vec<String> = Vec::new();
            let canon_workspace = std::fs::canonicalize(&workspace_root).unwrap_or(workspace_root);
            Self::collect(&canon_workspace, &target, &mut out)?;
            Ok::<_, std::io::Error>(out)
        })
        .await;

        let entries = match collect_result {
            Ok(Ok(entries)) => entries,
            Ok(Err(e)) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("failed to read directory: {e}")),
                });
            }
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("directory scan task failed: {e}")),
                });
            }
        };

        let truncated = entries.len() >= MAX_ENTRIES;
        let body = entries.join("\n");
        let suffix = if truncated {
            "\n[truncated at 1024 entries]"
        } else {
            ""
        };
        Ok(ToolResult {
            success: true,
            output: format!("Listing {relative} ({} entries):\n{body}{suffix}", entries.len()),
            error: None,
        })
    }
}
