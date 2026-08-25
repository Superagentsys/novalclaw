use crate::security::sandbox::resolve_workspace_relative;
use crate::tools::traits::{Tool, ToolResult};
use crate::tools::workspace_walk::{
    canonical_workspace, is_gitignored, is_noise_dir, normalized_relative, root_gitignore,
    walk_workspace,
};
use async_trait::async_trait;
use globset::GlobBuilder;
use regex::RegexBuilder;
use serde_json::json;
use std::path::{Path, PathBuf};

const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_SEARCH_FILE_BYTES: u64 = 4 * 1024 * 1024;
const DEFAULT_RESULTS: usize = 100;
const MAX_RESULTS: usize = 500;
const MAX_CONTEXT_LINES: usize = 20;

pub struct ContentSearchTool {
    workspace_dir: PathBuf,
}

impl ContentSearchTool {
    pub fn new(workspace_dir: impl Into<PathBuf>) -> Self {
        Self {
            workspace_dir: workspace_dir.into(),
        }
    }

    fn truncate(mut output: String) -> String {
        if output.len() <= MAX_OUTPUT_BYTES {
            return output;
        }
        let mut cutoff = MAX_OUTPUT_BYTES;
        while !output.is_char_boundary(cutoff) {
            cutoff -= 1;
        }
        output.truncate(cutoff);
        output.push_str("\n[output truncated; narrow path/include/pattern]");
        output
    }

    fn search_internal(
        workspace: PathBuf,
        search_path: PathBuf,
        pattern: String,
        include: Option<String>,
        case_sensitive: bool,
        max_results: usize,
        context_before: usize,
        context_after: usize,
    ) -> Result<String, String> {
        let root = canonical_workspace(workspace).map_err(|e| e.to_string())?;
        let target = std::fs::canonicalize(search_path).map_err(|e| e.to_string())?;
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(!case_sensitive)
            .build()
            .map_err(|e| format!("Invalid regex pattern: {e}"))?;
        let include = match include {
            Some(pattern) => Some(
                GlobBuilder::new(&pattern)
                    .literal_separator(false)
                    .build()
                    .map_err(|e| format!("Invalid include glob: {e}"))?
                    .compile_matcher(),
            ),
            None => None,
        };
        let ignored = root_gitignore(&root);
        let mut output = Vec::new();
        let mut matches = 0usize;

        let walker = walk_workspace(&target, 64)
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
            if matches >= max_results || !entry.file_type().is_file() {
                continue;
            }
            let relative = normalized_relative(&root, entry.path());
            if include
                .as_ref()
                .is_some_and(|matcher| !matcher.is_match(Path::new(&relative)))
            {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) if metadata.len() <= MAX_SEARCH_FILE_BYTES => metadata,
                _ => continue,
            };
            if metadata.len() == 0 {
                continue;
            }
            let bytes = match std::fs::read(entry.path()) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            };
            if bytes.iter().take(8_192).any(|byte| *byte == 0) {
                continue;
            }
            let contents = match std::str::from_utf8(&bytes) {
                Ok(contents) => contents,
                Err(_) => continue,
            };
            let lines: Vec<&str> = contents.lines().collect();
            let mut last_emitted: Option<usize> = None;
            for (index, line) in lines.iter().enumerate() {
                if matches >= max_results {
                    break;
                }
                if !regex.is_match(line) {
                    continue;
                }
                matches += 1;
                let start = index.saturating_sub(context_before);
                let end = index
                    .saturating_add(context_after)
                    .saturating_add(1)
                    .min(lines.len());
                if last_emitted.is_some_and(|last| start > last.saturating_add(1)) {
                    output.push("--".to_string());
                }
                for line_index in start..end {
                    if last_emitted.is_some_and(|last| line_index <= last) {
                        continue;
                    }
                    let separator = if line_index == index { ':' } else { '-' };
                    output.push(format!(
                        "{relative}{separator}{}{separator}{}",
                        line_index + 1,
                        lines[line_index]
                    ));
                    last_emitted = Some(line_index);
                }
            }
        }

        if output.is_empty() {
            Ok("No matches found.".to_string())
        } else {
            output.push(format!("[{matches} matching lines]"));
            Ok(Self::truncate(output.join("\n")))
        }
    }
}

#[async_trait]
impl Tool for ContentSearchTool {
    fn name(&self) -> &str {
        "content_search"
    }

    fn description(&self) -> &str {
        "Search workspace file contents with a built-in regex engine. It does not require rg/grep and excludes dependency caches, virtual environments, VCS data and root .gitignore matches."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Regex pattern to search for" },
                "path": { "type": "string", "description": "Workspace-relative file or directory (default: .)" },
                "include": { "type": "string", "description": "File glob filter (e.g. *.rs or src/**/*.ts)" },
                "case_sensitive": { "type": "boolean" },
                "max_results": { "type": "integer", "minimum": 1, "maximum": 500 },
                "context_before": { "type": "integer", "minimum": 0, "maximum": 20 },
                "context_after": { "type": "integer", "minimum": 0, "maximum": 20 }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let pattern = match args.get("pattern").and_then(|v| v.as_str()) {
            Some(pattern) if !pattern.is_empty() => pattern.to_string(),
            _ => return Ok(ToolResult::failure("Missing 'pattern' parameter")),
        };
        let sub_path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let search_path = match resolve_workspace_relative(&self.workspace_dir, sub_path).await {
            Ok(path) => path,
            Err(e) => return Ok(ToolResult::failure(e.to_string())),
        };
        let include = args
            .get("include")
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        let case_sensitive = args
            .get("case_sensitive")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or(DEFAULT_RESULTS)
            .clamp(1, MAX_RESULTS);
        let context_before = args
            .get("context_before")
            .and_then(|v| v.as_u64())
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or(0)
            .min(MAX_CONTEXT_LINES);
        let context_after = args
            .get("context_after")
            .and_then(|v| v.as_u64())
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or(0)
            .min(MAX_CONTEXT_LINES);
        let workspace = self.workspace_dir.clone();

        match tokio::task::spawn_blocking(move || {
            Self::search_internal(
                workspace,
                search_path,
                pattern,
                include,
                case_sensitive,
                max_results,
                context_before,
                context_after,
            )
        })
        .await
        {
            Ok(Ok(output)) => Ok(ToolResult::success(output)),
            Ok(Err(error)) => Ok(ToolResult::failure(error)),
            Err(error) => Ok(ToolResult::failure(format!(
                "content search task failed: {error}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("omninova-content-search-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join(".venv/lib")).unwrap();
        std::fs::create_dir_all(root.join("ignored")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn needle() {}\n").unwrap();
        std::fs::write(root.join(".venv/lib/noise.rs"), "fn needle() {}\n").unwrap();
        std::fs::write(root.join("ignored/skip.rs"), "fn needle() {}\n").unwrap();
        std::fs::write(root.join(".gitignore"), "ignored/\n").unwrap();
        root
    }

    #[tokio::test]
    async fn built_in_search_works_without_external_commands_and_ignores_noise() {
        let root = workspace();
        let result = ContentSearchTool::new(&root)
            .execute(json!({"pattern": "needle", "include": "*.rs"}))
            .await
            .unwrap();
        assert!(result.success, "{:?}", result.error);
        assert!(
            result.output.contains("src/main.rs:1:"),
            "{}",
            result.output
        );
        assert!(!result.output.contains(".venv"), "{}", result.output);
        assert!(!result.output.contains("ignored/"), "{}", result.output);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn invalid_regex_is_a_clear_tool_failure() {
        let root = workspace();
        let result = ContentSearchTool::new(&root)
            .execute(json!({"pattern": "["}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap_or_default().contains("Invalid regex"));
        let _ = std::fs::remove_dir_all(root);
    }
}
