use crate::tools::traits::{Tool, ToolResult};
use crate::tools::workspace_walk::{
    canonical_workspace, is_gitignored, is_noise_dir, normalized_relative, root_gitignore,
    walk_workspace,
};
use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;

const DEFAULT_RESULTS: usize = 200;
const MAX_RESULTS: usize = 1_000;

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
        "Search workspace files by glob. Dependency caches, virtual environments, VCS data and root .gitignore matches are excluded."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Workspace-relative glob pattern (e.g. **/*.rs, src/**/*.ts)"
                },
                "max_results": {
                    "type": "integer", "minimum": 1, "maximum": 1000,
                    "description": "Maximum files to return (default: 200)."
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let pattern = match args.get("pattern").and_then(|v| v.as_str()) {
            Some(pattern) => pattern.to_string(),
            None => return Ok(ToolResult::failure("Missing 'pattern' parameter")),
        };
        let bytes = pattern.as_bytes();
        let windows_absolute = bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/');
        if pattern.contains("..")
            || pattern.starts_with('/')
            || pattern.starts_with('\\')
            || windows_absolute
        {
            return Ok(ToolResult::failure(
                "Pattern must be workspace-relative and must not contain '..' or absolute paths",
            ));
        }
        let matcher = match globset::GlobBuilder::new(&pattern)
            .literal_separator(false)
            .build()
        {
            Ok(glob) => glob.compile_matcher(),
            Err(e) => return Ok(ToolResult::failure(format!("Invalid glob pattern: {e}"))),
        };
        let limit = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or(DEFAULT_RESULTS)
            .clamp(1, MAX_RESULTS);
        let workspace = self.workspace_dir.clone();

        let scan = tokio::task::spawn_blocking(move || {
            let root = canonical_workspace(workspace)?;
            let ignored = root_gitignore(&root);
            let mut results = Vec::with_capacity(limit);
            let mut has_more = false;
            let walker = walk_workspace(&root, 64).into_iter().filter_entry(|entry| {
                entry.depth() == 0
                    || (!is_noise_dir(entry)
                        && !is_gitignored(&root, ignored.as_ref(), entry.path()))
            });
            for item in walker {
                let entry = match item {
                    Ok(entry) => entry,
                    Err(_) => continue,
                };
                if !entry.file_type().is_file() {
                    continue;
                }
                let relative = normalized_relative(&root, entry.path());
                if !matcher.is_match(&relative) {
                    continue;
                }
                if results.len() >= limit {
                    has_more = true;
                    break;
                }
                results.push(relative);
            }
            Ok::<_, std::io::Error>((results, has_more))
        })
        .await;

        let (results, has_more) = match scan {
            Ok(Ok(value)) => value,
            Ok(Err(e)) => return Ok(ToolResult::failure(format!("Cannot scan workspace: {e}"))),
            Err(e) => return Ok(ToolResult::failure(format!("glob search task failed: {e}"))),
        };
        if results.is_empty() {
            return Ok(ToolResult::success("No files matched the pattern."));
        }
        let count = results.len();
        let suffix = if has_more {
            format!("\n[more than {count} files matched; narrow the pattern or raise max_results]")
        } else {
            format!("\n[{count} files]")
        };
        Ok(ToolResult::success(format!(
            "{}{suffix}",
            results.join("\n")
        )))
    }
}
