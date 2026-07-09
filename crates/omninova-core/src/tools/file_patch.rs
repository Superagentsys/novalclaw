use crate::agent::AgentCancellationToken;
use crate::security::sandbox::{normalize_workspace_path, resolve_workspace_relative};
use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
struct PatchHunkInput {
    old_start: Option<usize>,
    old_lines: Option<usize>,
    new_start: Option<usize>,
    new_lines: Option<usize>,
    old_text: Option<String>,
    new_text: String,
    summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PatchHunkOutput {
    old_start: usize,
    old_lines: usize,
    new_start: usize,
    new_lines: usize,
    additions: i32,
    deletions: i32,
    summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_text: Option<String>,
    #[serde(default)]
    text_truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
struct PatchOutput {
    path: String,
    additions: i32,
    deletions: i32,
    hunks_count: usize,
    hunks: Vec<PatchHunkOutput>,
}

pub struct FilePatchTool {
    workspace_dir: PathBuf,
}

impl FilePatchTool {
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

fn truncate_chars(input: &str, max_chars: usize) -> (String, bool) {
    let mut out = String::new();
    let mut truncated = false;
    for (index, ch) in input.chars().enumerate() {
        if index >= max_chars {
            truncated = true;
            break;
        }
        out.push(ch);
    }
    (out, truncated)
}

fn text_preview(input: &str) -> (Option<String>, bool) {
    if input.is_empty() {
        return (None, false);
    }
    let (preview, truncated) = truncate_chars(input, 8_000);
    (Some(preview), truncated)
}

fn line_start_offset(text: &str, one_based_line: usize) -> Option<usize> {
    if one_based_line <= 1 {
        return Some(0);
    }
    let mut current_line = 1usize;
    for (idx, ch) in text.char_indices() {
        if ch == '\n' {
            current_line += 1;
            if current_line == one_based_line {
                return Some(idx + 1);
            }
        }
    }
    if current_line + 1 == one_based_line {
        return Some(text.len());
    }
    None
}

fn line_range_offset(text: &str, one_based_line: usize, line_count: usize) -> Option<(usize, usize)> {
    let start = line_start_offset(text, one_based_line)?;
    if line_count == 0 {
        return Some((start, start));
    }
    let end_line = one_based_line.saturating_add(line_count);
    let end = line_start_offset(text, end_line).unwrap_or(text.len());
    Some((start, end))
}

fn apply_hunk(content: &mut String, hunk: &PatchHunkInput) -> anyhow::Result<PatchHunkOutput> {
    let old_text = hunk.old_text.clone().unwrap_or_default();
    let captured_old_text: String;
    let summary = hunk
        .summary
        .clone()
        .unwrap_or_else(|| "局部修改".to_string());

    let (old_start, old_lines, new_start, new_lines, additions, deletions) =
        if !old_text.is_empty() {
            let Some(byte_start) = content.find(&old_text) else {
                anyhow::bail!("patch hunk did not match existing file content: {summary}");
            };
            let byte_end = byte_start + old_text.len();
            let old_start = content[..byte_start].lines().count() + 1;
            let old_lines = count_lines(&old_text);
            let new_lines = count_lines(&hunk.new_text);
            captured_old_text = old_text.clone();
            content.replace_range(byte_start..byte_end, &hunk.new_text);
            (
                old_start,
                old_lines,
                hunk.new_start.unwrap_or(old_start),
                hunk.new_lines.unwrap_or(new_lines),
                new_lines.saturating_sub(old_lines) as i32,
                old_lines.saturating_sub(new_lines) as i32,
            )
        } else {
            let old_start = hunk
                .old_start
                .ok_or_else(|| anyhow::anyhow!("patch hunk requires old_text or old_start"))?;
            let old_lines = hunk.old_lines.unwrap_or(0);
            let Some((byte_start, byte_end)) = line_range_offset(content, old_start, old_lines) else {
                anyhow::bail!("patch hunk line range is outside file: {summary}");
            };
            let new_lines = count_lines(&hunk.new_text);
            captured_old_text = content[byte_start..byte_end].to_string();
            content.replace_range(byte_start..byte_end, &hunk.new_text);
            (
                old_start,
                old_lines,
                hunk.new_start.unwrap_or(old_start),
                hunk.new_lines.unwrap_or(new_lines),
                new_lines.saturating_sub(old_lines) as i32,
                old_lines.saturating_sub(new_lines) as i32,
            )
        };
    let (old_text_preview, old_truncated) = text_preview(&captured_old_text);
    let (new_text_preview, new_truncated) = text_preview(&hunk.new_text);

    Ok(PatchHunkOutput {
        old_start,
        old_lines,
        new_start,
        new_lines,
        additions,
        deletions,
        summary,
        old_text: old_text_preview,
        new_text: new_text_preview,
        text_truncated: old_truncated || new_truncated,
    })
}

#[async_trait]
impl Tool for FilePatchTool {
    fn name(&self) -> &str {
        "file_patch"
    }

    fn description(&self) -> &str {
        "Apply structured local hunks to an existing workspace file. Use this for editing existing files instead of overwriting the whole file. Path must be workspace-relative, e.g. \"index.html\"."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Workspace-relative file path. Use \"index.html\", not D:\\\\workspace\\\\index.html."
                },
                "hunks": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "old_start": { "type": "integer", "description": "1-based old start line, used when old_text is omitted." },
                            "old_lines": { "type": "integer", "description": "Number of old lines replaced, or 0 for insertion." },
                            "new_start": { "type": "integer" },
                            "new_lines": { "type": "integer" },
                            "old_text": { "type": "string", "description": "Exact existing text to replace. Preferred for reliability." },
                            "new_text": { "type": "string", "description": "Replacement text." },
                            "summary": { "type": "string", "description": "Human-readable hunk summary, e.g. 添加世界杯 Hero 首屏." }
                        },
                        "required": ["new_text"]
                    }
                }
            },
            "required": ["path", "hunks"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        self.execute_with_cancel(args, AgentCancellationToken::default()).await
    }

    async fn execute_with_cancel(
        &self,
        args: serde_json::Value,
        cancel_token: AgentCancellationToken,
    ) -> anyhow::Result<ToolResult> {
        cancel_token.check()?;
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;
        let normalized_path = match normalize_workspace_path(&self.workspace_dir, path).await {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                });
            }
        };
        if normalized_path == "." {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("file_patch requires a file path, not the workspace root".to_string()),
            });
        }
        let resolved = match resolve_workspace_relative(&self.workspace_dir, &normalized_path).await {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                });
            }
        };
        if tokio::fs::metadata(&resolved).await.is_err() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("{normalized_path} does not exist; use file_write for new files")),
            });
        }

        let hunks: Vec<PatchHunkInput> = match serde_json::from_value(
            args.get("hunks").cloned().unwrap_or(serde_json::Value::Null),
        ) {
            Ok(hunks) => hunks,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("invalid hunks: {e}")),
                });
            }
        };
        if hunks.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("file_patch requires at least one hunk".to_string()),
            });
        }

        cancel_token.check()?;
        let mut content = match tokio::fs::read_to_string(&resolved).await {
            Ok(content) => content,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Failed to read file before patch: {e}")),
                });
            }
        };

        let mut applied = Vec::new();
        for hunk in &hunks {
            cancel_token.check()?;
            match apply_hunk(&mut content, hunk) {
                Ok(output) => applied.push(output),
                Err(e) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        cancel_token.check()?;
        if let Err(e) = tokio::fs::write(&resolved, content.as_bytes()).await {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to write patched file: {e}")),
            });
        }

        cancel_token.check()?;
        if let Err(e) = tokio::fs::read_to_string(&resolved).await {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Patched file could not be re-read: {e}")),
            });
        }

        let additions = applied.iter().map(|h| h.additions).sum();
        let deletions = applied.iter().map(|h| h.deletions).sum();
        let output = PatchOutput {
            path: normalized_path,
            additions,
            deletions,
            hunks_count: applied.len(),
            hunks: applied,
        };

        Ok(ToolResult {
            success: true,
            output: serde_json::to_string(&output)?,
            error: None,
        })
    }
}
