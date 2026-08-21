//! Unified tool executor for the Agent Runtime.
//!
//! All tool calls flow through `ToolRunner::run_tool`, which:
//!   - Emits `tool_started` before execution.
//!   - Handles security gate (blocked / approval required / proceed).
//!   - For shell: drains streaming output before emitting `tool_completed`.
//!   - For file-write/edit: emits `file_changed` then `tool_completed`.
//!   - For all others: emits `tool_completed` with result summary.
//!   - Computes and reports `duration_ms` accurately.

use crate::agent::agent_event::{AgentRunEvent, ChangeType, DiffStats, StepStatus};
use crate::agent::event_bus::{
    build_tool_start_summary, compute_content_diff, extract_diff_stats, truncate_for_display, EventBus,
    TimedBlock,
};
use crate::agent::AgentCancellationToken;
use crate::agent::{FileDiffStats, ToolExecutionEvent};
use crate::providers::ToolCall;
use crate::security::{ApprovalStatus, SecurityContext, ToolExecutionGate};
use crate::tools::{Tool, ToolResult};
use anyhow::Result;
use std::sync::Arc;
use tokio::time::{sleep, Duration, Instant};

fn now_ts() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let t = now.as_secs();
    let h = (t / 3600) % 24;
    let m = (t / 60) % 60;
    let s = t % 60;
    let ms = now.subsec_millis();
    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms)
}

fn preview(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

fn approval_arguments_for_display(arguments: &serde_json::Value) -> serde_json::Value {
    let Some(object) = arguments.as_object() else {
        return arguments.clone();
    };
    serde_json::Value::Object(
        object
            .iter()
            .map(|(key, value)| {
                let lowered = key.to_ascii_lowercase();
                let safe_value = if ["secret", "token", "password", "api_key", "apikey"]
                    .iter()
                    .any(|needle| lowered.contains(needle))
                {
                    serde_json::Value::String("[已隐藏敏感值]".to_string())
                } else if let Some(text) = value.as_str() {
                    serde_json::Value::String(preview(text, 2000))
                } else {
                    value.clone()
                };
                (key.clone(), safe_value)
            })
            .collect(),
    )
}

fn tool_path_arg(args: &serde_json::Value) -> String {
    args.get("path")
        .or_else(|| args.get("file_path"))
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_string()
}

fn patch_diff_stats(output: &str) -> Option<DiffStats> {
    let value: serde_json::Value = serde_json::from_str(output).ok()?;
    Some(DiffStats {
        additions: value.get("additions")?.as_i64()? as i32,
        deletions: value.get("deletions")?.as_i64()? as i32,
    })
}

fn file_write_output(output: &str) -> Option<serde_json::Value> {
    let value: serde_json::Value = serde_json::from_str(output).ok()?;
    if value.get("path").and_then(|v| v.as_str()).is_some()
        && value.get("new_text").is_some()
        && value.get("change_type").is_some()
    {
        Some(value)
    } else {
        None
    }
}

fn file_write_diff_stats(output: &str) -> Option<DiffStats> {
    let value = file_write_output(output)?;
    Some(DiffStats {
        additions: value.get("additions").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
        deletions: value.get("deletions").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
    })
}

fn file_write_change_type(value: &serde_json::Value) -> ChangeType {
    match value.get("change_type").and_then(|v| v.as_str()).unwrap_or("modified") {
        "created" | "added" => ChangeType::Created,
        "deleted" | "removed" => ChangeType::Deleted,
        _ => ChangeType::Modified,
    }
}

/// Unified tool runner — the single entry point for all tool executions
/// in the agent runtime.
///
/// Each tool call goes through `run_tool`, which:
///   1. Emits `tool_started` and generates a `step_id`.
///   2. Checks the security gate.
///   3. Executes the tool (shell tools use streaming output).
///   4. Emits `file_changed` (for write/edit tools).
///   5. Waits for all streaming output, then emits `tool_completed`.
pub struct ToolRunner<'a> {
    tools: &'a [Box<dyn Tool>],
    security: &'a SecurityContext,
    /// Optional EventBus. When None, no structured events are emitted.
    event_bus: Option<EventBus>,
    cancel_token: Option<AgentCancellationToken>,
}

impl<'a> Clone for ToolRunner<'a> {
    fn clone(&self) -> Self {
        Self {
            tools: self.tools,
            security: self.security,
            event_bus: self.event_bus.clone(),
            cancel_token: self.cancel_token.clone(),
        }
    }
}

impl<'a> ToolRunner<'a> {
    /// Creates a new ToolRunner.
    pub fn new(tools: &'a [Box<dyn Tool>], security: &'a SecurityContext) -> Self {
        Self {
            tools,
            security,
            event_bus: None,
            cancel_token: None,
        }
    }

    /// Injects the EventBus for structured event emission.
    pub fn with_event_bus(mut self, bus: Option<EventBus>) -> Self {
        self.event_bus = bus;
        self
    }

    pub fn with_cancel_token(mut self, cancel_token: Option<AgentCancellationToken>) -> Self {
        self.cancel_token = cancel_token;
        self
    }

    /// Runs a single tool call end-to-end:
    ///   - Emits `tool_started` (seq=1)
    ///   - Checks security gate (Blocked / ApprovalRequired / Proceed)
    ///   - Executes the tool (streaming for shell, regular for others)
    ///   - Emits `file_changed` for write/edit tools
    ///   - Waits for all streaming output, then emits `tool_completed` (seq=N)
    ///
    /// Returns `(output_string, ToolExecutionEvent)`.
    pub async fn run_tool(
        &self,
        tool_call: &ToolCall,
        args: &serde_json::Value,
    ) -> Result<(String, Option<ToolExecutionEvent>)> {
        let tool = self
            .tools
            .iter()
            .find(|t| t.name() == tool_call.name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {}", tool_call.name))?;
        if let Some(token) = &self.cancel_token {
            token.check()?;
        }

        let title = build_tool_start_summary(&tool_call.name, args);
        let is_shell = tool.name() == "shell";
        let is_file_write = matches!(
            tool_call.name.as_str(),
            "file_write" | "write_file" | "file_edit" | "edit_file" | "str_replace_editor"
        );
        let is_file_patch = matches!(tool_call.name.as_str(), "file_patch" | "apply_patch");

        // ── Emit tool_started ────────────────────────────────────────────────
        let step_id = if let Some(ref bus) = self.event_bus {
            bus.tool_started(
                tool_call.id.clone(),
                tool_call.name.clone(),
                title.clone(),
                None,
            )
        } else {
            uuid::Uuid::new_v4().to_string()
        };

        // ── Security gate ───────────────────────────────────────────────────
        let gate = self.security.gate_tool_execution(tool_call.name.as_str(), args).await;
        if let Some(token) = &self.cancel_token {
            token.check()?;
        }

        match gate {
            Ok(ToolExecutionGate::Blocked { reason }) => {
                let msg = format!("tool blocked by security policy: {reason}");
                if let Some(ref bus) = self.event_bus {
                    bus.tool_completed(
                        step_id.clone(),
                        tool_call.id.clone(),
                        tool_call.name.clone(),
                        false,
                        0,
                        msg.clone(),
                        None,
                    );
                }
                return Ok((
                    msg.clone(),
                    Some(ToolExecutionEvent::Completed {
                        tool_call_id: tool_call.id.clone(),
                        tool_name: tool_call.name.clone(),
                        success: false,
                        duration_ms: 0,
                        result_summary: msg,
                        diff_stats: None,
                    }),
                ));
            }
            Ok(ToolExecutionGate::ApprovalRequired { pending }) => {
                if let Some(ref bus) = self.event_bus {
                    bus.approval_required(
                        step_id.clone(),
                        tool_call.id.clone(),
                        pending.id.clone(),
                        tool_call.name.clone(),
                        title,
                        pending.reason.clone(),
                        approval_arguments_for_display(&pending.arguments),
                    );
                }

                // Keep this exact tool call alive while the desktop displays
                // its approval card. Approving resumes here with the original
                // arguments; no second model request or vague "retry" prompt is
                // needed, so conversational context and the intended operation
                // cannot drift.
                let wait_started = Instant::now();
                loop {
                    if let Some(token) = &self.cancel_token {
                        token.check()?;
                    }
                    if wait_started.elapsed() > Duration::from_secs(15 * 60) {
                        return Err(anyhow::anyhow!("tool approval timed out"));
                    }

                    match self.security.tool_approval(&pending.id).await? {
                        Some(item) if item.status == ApprovalStatus::Approved => {
                            match self
                                .security
                                .gate_tool_execution(&tool_call.name, args)
                                .await?
                            {
                                ToolExecutionGate::Proceed { .. } => break,
                                _ => {
                                    return Err(anyhow::anyhow!(
                                        "approved tool request could not be consumed"
                                    ));
                                }
                            }
                        }
                        Some(item) if item.status == ApprovalStatus::Rejected => {
                            let detail = item
                                .reject_reason
                                .unwrap_or_else(|| "用户拒绝了本次操作".to_string());
                            return Err(anyhow::anyhow!("tool execution rejected: {detail}"));
                        }
                        Some(_) => sleep(Duration::from_millis(180)).await,
                        // The small JSON approval store is rewritten when the
                        // UI decides. A read may briefly observe the rewrite;
                        // keep waiting instead of turning that harmless race
                        // into a failed agent run.
                        None => sleep(Duration::from_millis(180)).await,
                    }
                }
            }
            Ok(ToolExecutionGate::Proceed { .. }) | Err(_) => {}
        }

        // ── Execute tool ────────────────────────────────────────────────────
        let timer = TimedBlock::new();
        if let Some(token) = &self.cancel_token {
            token.check()?;
        }

        let (result, output_handle) = if is_shell {
            if let Some(ref bus) = self.event_bus {
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(String, bool)>();

                let run_id = bus.run_id().to_string();
                let step_id_inner = step_id.clone();
                let tool_call_id_inner = tool_call.id.clone();
                let tool_name_inner = tool_call.name.clone();
                let bus_for_output = bus.clone();

                // Reader task: drains channel and emits command_output.
                // Events are emitted in order BEFORE tool_completed because
                // the dispatcher awaits this handle.
                let output_handle = tokio::spawn(async move {
                    while let Some((content, is_stderr)) = rx.recv().await {
                        if !content.is_empty() {
                            tracing::debug!(
                                target: "e2e",
                                "[e2e-runner-recv] timestamp={} run_id={} is_stderr={} content=\"{}\"",
                                now_ts(),
                                run_id,
                                is_stderr,
                                preview(&content, 80)
                            );
                            bus_for_output.command_output(
                                step_id_inner.clone(),
                                tool_call_id_inner.clone(),
                                tool_name_inner.clone(),
                                content,
                                is_stderr,
                            );
                        }
                    }
                });

                // Execute shell (uses OutputSender internally).
                let exec_result = if let Some(token) = self.cancel_token.clone() {
                    tool.execute_streaming_with_cancel(args.clone(), tx, token).await
                } else {
                    tool.execute_streaming(args.clone(), tx).await
                };
                (exec_result, Some(output_handle))
            } else {
                let exec_result = if let Some(token) = self.cancel_token.clone() {
                    tool.execute_with_cancel(args.clone(), token).await
                } else {
                    tool.execute(args.clone()).await
                };
                (exec_result, None)
            }
        } else {
            if is_file_write || is_file_patch {
                if let Some(token) = &self.cancel_token {
                    token.check()?;
                }
            }
            if is_file_patch {
                if let Some(ref bus) = self.event_bus {
                    let path = tool_path_arg(args);
                    bus.patch_started(
                        step_id.clone(),
                        tool_call.id.clone(),
                        path.clone(),
                        format!("准备修改 {path}"),
                    );
                }
            }
            let exec_result = if let Some(token) = self.cancel_token.clone() {
                tool.execute_with_cancel(args.clone(), token).await
            } else {
                tool.execute(args.clone()).await
            };
            (exec_result, None)
        };

        let elapsed_ms = timer.elapsed_ms();

        // ── Wait for streaming output to drain (shell) ─────────────────────
        if let Some(h) = output_handle {
            let _ = h.await;
        }

        // ── Emit result events ───────────────────────────────────────────────
        match result {
            Ok(tool_result) => {
                let is_success = tool_result.success;
                let output = if is_success {
                    tool_result.output.clone()
                } else {
                    tool_result
                        .error
                        .clone()
                        .unwrap_or(tool_result.output)
                };
                let truncated = truncate_for_display(&output, 2000);
                let diff_stats = extract_diff_stats(&tool_call.name, &output)
                    .map(|ds| FileDiffStats {
                        additions: ds.additions,
                        deletions: ds.deletions,
                    })
                    .or_else(|| {
                        if is_file_patch {
                            patch_diff_stats(&output).map(|ds| FileDiffStats {
                                additions: ds.additions,
                                deletions: ds.deletions,
                            })
                        } else if is_file_write {
                            file_write_diff_stats(&output).map(|ds| FileDiffStats {
                                additions: ds.additions,
                                deletions: ds.deletions,
                            })
                        } else {
                            None
                        }
                    })
                    .or_else(|| {
                        compute_content_diff(&tool_call.name, args, &output).map(|ds| FileDiffStats {
                            additions: ds.additions,
                            deletions: ds.deletions,
                        })
                    });

                if let Some(ref bus) = self.event_bus {
                    if is_file_patch {
                        let path = tool_path_arg(args);
                        if is_success {
                            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&output) {
                                let path = value
                                    .get("path")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&path)
                                    .to_string();
                                if let Some(hunks) = value.get("hunks").and_then(|v| v.as_array()) {
                                    for hunk in hunks {
                                        bus.patch_hunk(
                                            step_id.clone(),
                                            tool_call.id.clone(),
                                            path.clone(),
                                            hunk.get("old_start").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                                            hunk.get("old_lines").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                                            hunk.get("new_start").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                                            hunk.get("new_lines").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                                            hunk.get("additions").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                                            hunk.get("deletions").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                                            hunk.get("summary").and_then(|v| v.as_str()).unwrap_or("局部修改").to_string(),
                                            hunk.get("old_text").and_then(|v| v.as_str()).map(str::to_string),
                                            hunk.get("new_text").and_then(|v| v.as_str()).map(str::to_string),
                                            hunk.get("text_truncated").and_then(|v| v.as_bool()).unwrap_or(false),
                                        );
                                    }
                                }
                                let additions = value.get("additions").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                                let deletions = value.get("deletions").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                                let hunks_count = value.get("hunks_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                                bus.patch_applied(
                                    step_id.clone(),
                                    tool_call.id.clone(),
                                    path.clone(),
                                    additions,
                                    deletions,
                                    hunks_count,
                                    format!("已应用 {hunks_count} 个 hunk"),
                                );
                                bus.file_changed(
                                    step_id.clone(),
                                    Some(tool_call.id.clone()),
                                    path,
                                    additions,
                                    deletions,
                                    Some(ChangeType::Modified),
                                    None,
                                    None,
                                    false,
                                    None,
                                    None,
                                );
                            }
                        } else {
                            bus.patch_failed(
                                step_id.clone(),
                                tool_call.id.clone(),
                                path,
                                truncated.clone(),
                            );
                        }
                    }
                    // file_changed for write/edit tools.
                    if is_file_write {
                        let path = tool_path_arg(args);
                        let write_output = file_write_output(&output);

                        if let Some(ref ds) = diff_stats {
                            let change_type = write_output
                                .as_ref()
                                .map(file_write_change_type)
                                .unwrap_or_else(|| {
                                    if is_success
                                        && (output.contains("created")
                                            || output.contains("写入成功")
                                            || output.contains("已创建"))
                                    {
                                        ChangeType::Created
                                    } else {
                                        ChangeType::Modified
                                    }
                                });
                            let old_text = write_output
                                .as_ref()
                                .and_then(|value| value.get("old_text").and_then(|v| v.as_str()).map(str::to_string));
                            let new_text = write_output
                                .as_ref()
                                .and_then(|value| value.get("new_text").and_then(|v| v.as_str()).map(str::to_string));
                            let content_truncated = write_output
                                .as_ref()
                                .and_then(|value| value.get("content_truncated").and_then(|v| v.as_bool()))
                                .unwrap_or(false);
                            let content_total_chars = write_output
                                .as_ref()
                                .and_then(|value| value.get("content_total_chars").and_then(|v| v.as_u64()).map(|n| n as usize));
                            let content_preview_chars = write_output
                                .as_ref()
                                .and_then(|value| value.get("content_preview_chars").and_then(|v| v.as_u64()).map(|n| n as usize));
                            bus.file_changed(
                                step_id.clone(),
                                Some(tool_call.id.clone()),
                                path.clone(),
                                ds.additions,
                                ds.deletions,
                                Some(change_type),
                                old_text,
                                new_text,
                                content_truncated,
                                content_total_chars,
                                content_preview_chars,
                            );
                        } else if let Some(ds) = compute_content_diff(&tool_call.name, args, &output) {
                            bus.file_changed(
                                step_id.clone(),
                                Some(tool_call.id.clone()),
                                path,
                                ds.additions,
                                ds.deletions,
                                None,
                                None,
                                None,
                                false,
                                None,
                                None,
                            );
                        }
                    }

                    bus.tool_completed(
                        step_id.clone(),
                        tool_call.id.clone(),
                        tool_call.name.clone(),
                        is_success,
                        elapsed_ms,
                        truncated.clone(),
                        diff_stats
                            .as_ref()
                            .map(|ds| DiffStats {
                                additions: ds.additions,
                                deletions: ds.deletions,
                            }),
                    );
                    if tool_call.name == "use_skill" && is_success {
                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&output) {
                            let skill_id = value
                                .get("skill_id")
                                .and_then(|item| item.as_str())
                                .unwrap_or("")
                                .trim();
                            let display_name = value
                                .get("display_name")
                                .and_then(|item| item.as_str())
                                .unwrap_or(skill_id)
                                .trim();
                            let source = value
                                .get("selection_source")
                                .and_then(|item| item.as_str())
                                .unwrap_or("auto_use_skill")
                                .trim();
                            if !skill_id.is_empty() {
                                bus.skill_activated(
                                    skill_id.to_string(),
                                    display_name.to_string(),
                                    source.to_string(),
                                );
                            }
                        }
                    }
                }

                Ok((
                    output,
                    Some(ToolExecutionEvent::Completed {
                        tool_call_id: tool_call.id.clone(),
                        tool_name: tool_call.name.clone(),
                        success: is_success,
                        duration_ms: elapsed_ms,
                        result_summary: truncated,
                        diff_stats,
                    }),
                ))
            }
            Err(e) => {
                let msg = e.to_string();
                if let Some(ref bus) = self.event_bus {
                    bus.tool_completed(
                        step_id.clone(),
                        tool_call.id.clone(),
                        tool_call.name.clone(),
                        false,
                        elapsed_ms,
                        msg.clone(),
                        None,
                    );
                }
                Ok((
                    msg.clone(),
                    Some(ToolExecutionEvent::Completed {
                        tool_call_id: tool_call.id.clone(),
                        tool_name: tool_call.name.clone(),
                        success: false,
                        duration_ms: elapsed_ms,
                        result_summary: msg,
                        diff_stats: None,
                    }),
                ))
            }
        }
    }
}
