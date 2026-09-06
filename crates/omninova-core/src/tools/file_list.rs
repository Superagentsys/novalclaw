use crate::security::sandbox::resolve_tool_path;
use crate::tools::traits::{Tool, ToolResult};
use crate::tools::workspace_walk::{
    canonical_workspace, is_gitignored, is_noise_dir, normalized_relative, root_gitignore,
    walk_workspace,
};
use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;

const DEFAULT_ENTRIES: usize = 200;
const MAX_ENTRIES: usize = 500;
const DEFAULT_DEPTH: usize = 12;
const MAX_DEPTH: usize = 32;

pub struct FileListTool {
    workspace_dir: PathBuf,
    full_access: bool,
}

impl FileListTool {
    pub fn new(workspace_dir: impl Into<PathBuf>) -> Self {
        Self {
            workspace_dir: workspace_dir.into(),
            full_access: false,
        }
    }

    pub fn with_full_access(mut self, full_access: bool) -> Self {
        self.full_access = full_access;
        self
    }
}

#[async_trait]
impl Tool for FileListTool {
    fn name(&self) -> &str {
        "file_list"
    }

    fn description(&self) -> &str {
        "List workspace files with deterministic pagination. Dependency caches, virtual environments, VCS data and root .gitignore matches are excluded."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Optional workspace-relative subdirectory; use \".\" for the workspace root."
                },
                "offset": {
                    "type": "integer", "minimum": 0,
                    "description": "Number of matching entries to skip (default: 0)."
                },
                "max_entries": {
                    "type": "integer", "minimum": 1, "maximum": 500,
                    "description": "Maximum entries to return (default: 200)."
                },
                "max_depth": {
                    "type": "integer", "minimum": 1, "maximum": 32,
                    "description": "Maximum traversal depth below path (default: 12)."
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let relative = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let offset = args
            .get("offset")
            .and_then(|v| v.as_u64())
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or(0);
        let limit = args
            .get("max_entries")
            .and_then(|v| v.as_u64())
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or(DEFAULT_ENTRIES)
            .clamp(1, MAX_ENTRIES);
        let max_depth = args
            .get("max_depth")
            .and_then(|v| v.as_u64())
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or(DEFAULT_DEPTH)
            .clamp(1, MAX_DEPTH);

        let resolved = match resolve_tool_path(&self.workspace_dir, relative, self.full_access).await {
            Ok(p) => p,
            Err(e) => return Ok(ToolResult::failure(e.to_string())),
        };
        match tokio::fs::metadata(&resolved).await {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => {
                return Ok(ToolResult::failure(format!(
                    "{relative} is not a directory"
                )))
            }
            Err(e) => {
                return Ok(ToolResult::failure(format!(
                    "failed to stat {relative}: {e}"
                )))
            }
        }

        let workspace = self.workspace_dir.clone();
        let full_access = self.full_access;
        let scan = tokio::task::spawn_blocking(move || {
            let target = std::fs::canonicalize(resolved)?;
            let workspace = canonical_workspace(workspace)?;
            let root = if full_access && !target.starts_with(&workspace) {
                target.clone()
            } else {
                workspace
            };
            let ignored = root_gitignore(&root);
            let mut seen = 0usize;
            let mut entries = Vec::with_capacity(limit);
            let mut has_more = false;

            let walker = walk_workspace(&target, max_depth)
                .into_iter()
                .filter_entry(|entry| {
                    entry.depth() == 0
                        || (!is_noise_dir(entry)
                            && !is_gitignored(&root, ignored.as_ref(), entry.path()))
                });
            for item in walker {
                let entry = match item {
                    Ok(entry) => entry,
                    Err(_) => continue,
                };
                if entry.depth() == 0 {
                    continue;
                }
                if seen < offset {
                    seen += 1;
                    continue;
                }
                if entries.len() >= limit {
                    has_more = true;
                    break;
                }
                let mut rel = normalized_relative(&root, entry.path());
                if entry.file_type().is_dir() {
                    rel.push('/');
                } else if entry.file_type().is_symlink() {
                    rel.push('@');
                }
                entries.push(rel);
                seen += 1;
            }
            Ok::<_, std::io::Error>((entries, has_more))
        })
        .await;

        let (entries, has_more) = match scan {
            Ok(Ok(value)) => value,
            Ok(Err(e)) => {
                return Ok(ToolResult::failure(format!(
                    "failed to read directory: {e}"
                )))
            }
            Err(e) => {
                return Ok(ToolResult::failure(format!(
                    "directory scan task failed: {e}"
                )))
            }
        };

        let body = entries.join("\n");
        let next = offset.saturating_add(entries.len());
        let suffix = if has_more {
            format!("\n[more entries available; continue with offset={next}]")
        } else {
            String::new()
        };
        Ok(ToolResult::success(format!(
            "Listing {relative} ({} entries, offset {offset}):\n{body}{suffix}",
            entries.len()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_excludes_virtual_environments_caches_and_gitignored_paths() {
        let root =
            std::env::temp_dir().join(format!("omninova-file-list-{}", uuid::Uuid::new_v4()));
        for path in ["src", ".venv/lib", "__pycache__", "ignored"] {
            std::fs::create_dir_all(root.join(path)).unwrap();
        }
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(root.join(".venv/lib/noise.py"), "noise\n").unwrap();
        std::fs::write(root.join("__pycache__/noise.pyc"), "noise\n").unwrap();
        std::fs::write(root.join("ignored/noise.txt"), "noise\n").unwrap();
        std::fs::write(root.join(".gitignore"), "ignored/\n").unwrap();

        let result = FileListTool::new(&root)
            .execute(json!({"path": "."}))
            .await
            .unwrap();
        assert!(result.success, "{:?}", result.error);
        assert!(result.output.contains("src/main.rs"), "{}", result.output);
        assert!(!result.output.contains(".venv"), "{}", result.output);
        assert!(!result.output.contains("__pycache__"), "{}", result.output);
        assert!(!result.output.contains("ignored/"), "{}", result.output);
        let _ = std::fs::remove_dir_all(root);
    }
}
