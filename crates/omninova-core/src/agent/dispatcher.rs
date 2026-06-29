use crate::agent::budget::BudgetTracker;
use crate::agent::history::sanitize_messages_for_provider;
use crate::agent::{AgentEvent, FileDiffStats, ToolExecutionEvent};
use crate::providers::{ChatMessage, ChatRequest, ChatResponse, Provider, ToolCall};
use crate::security::SecurityContext;
use crate::tools::{Tool, ToolSpec};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;

/// Builds a human-readable Chinese summary for a tool call.
pub fn build_tool_summary(tool_name: &str, args: &serde_json::Value) -> String {
    match tool_name {
        "file_list" | "list_directory" => {
            if let Some(p) = args.get("path").and_then(|v| v.as_str()) {
                format!("正在列出目录：{}", p)
            } else {
                "正在列出目录".into()
            }
        }
        "file_read" | "read_file" => {
            if let Some(p) = args.get("path").and_then(|v| v.as_str()) {
                format!("正在读取文件：{}", p)
            } else {
                "正在读取文件".into()
            }
        }
        "file_write" | "write_file" => {
            if let Some(p) = args.get("path").and_then(|v| v.as_str()) {
                format!("正在写入文件：{}", p)
            } else {
                "正在写入文件".into()
            }
        }
        "file_edit" | "edit_file" | "str_replace_editor" => {
            if let Some(p) = args.get("file_path").and_then(|v| v.as_str()) {
                format!("正在编辑文件：{}", p)
            } else {
                "正在编辑文件".into()
            }
        }
        "glob_search" | "glob" => {
            if let Some(p) = args.get("pattern").and_then(|v| v.as_str()) {
                format!("正在搜索文件：{}", p)
            } else {
                "正在搜索文件".into()
            }
        }
        "content_search" | "search" | "grep" => {
            if let Some(q) = args.get("query").and_then(|v| v.as_str()) {
                format!("正在搜索内容：{}", q)
            } else {
                "正在搜索内容".into()
            }
        }
        "shell" | "bash" | "run_command" | "Command" => {
            if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                let display = if cmd.len() > 80 { format!("{}...", &cmd[..80]) } else { cmd.to_string() };
                format!("正在执行命令：{}", display)
            } else if let Some(cmd) = args.get("script").and_then(|v| v.as_str()) {
                let display = if cmd.len() > 80 { format!("{}...", &cmd[..80]) } else { cmd.to_string() };
                format!("正在执行脚本：{}", display)
            } else {
                "正在执行命令".into()
            }
        }
        "git_operations" | "git" => {
            if let Some(op) = args.get("operation").and_then(|v| v.as_str()) {
                format!("正在执行 Git 操作：{}", op)
            } else {
                "正在执行 Git 操作".into()
            }
        }
        "browser" => {
            if let Some(u) = args.get("url").and_then(|v| v.as_str()) {
                format!("正在访问网页：{}", u)
            } else {
                "正在操作浏览器".into()
            }
        }
        "delegate" => "正在委托子任务".into(),
        _ => format!("正在执行工具：{}", tool_name),
    }
}

/// Truncates a string for UI display.
pub fn truncate_for_display(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    if s.len() <= max_chars {
        return s.to_string();
    }
    let head: String = s.chars().take(max_chars).collect();
    format!("{}\n... [输出已截断，原始长度 {} 字符]", head, s.len())
}

/// Extracts diff stats from git diff --numstat output.
fn extract_diff_stats(tool_name: &str, output: &str) -> Option<FileDiffStats> {
    if !matches!(tool_name, "file_write" | "write_file" | "file_edit" | "edit_file" | "str_replace_editor") {
        return None;
    }
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            let additions: i32 = parts[0].parse().unwrap_or(0);
            let deletions: i32 = parts[1].parse().unwrap_or(0);
            if additions > 0 || deletions > 0 {
                return Some(FileDiffStats { additions, deletions });
            }
        }
    }
    None
}

/// Tries to compute diff stats from the written content itself (non-git fallback).
fn compute_content_diff(tool_name: &str, args: &serde_json::Value, output: &str) -> Option<FileDiffStats> {
    if !matches!(tool_name, "file_write" | "write_file" | "file_edit" | "edit_file" | "str_replace_editor") {
        return None;
    }
    if matches!(tool_name, "file_write" | "write_file") {
        let content_lines = args.get("content")
            .and_then(|v| v.as_str())
            .map(|s| s.lines().count().max(usize::from(!s.is_empty())))
            .unwrap_or(0);
        if content_lines > 0 {
            return Some(FileDiffStats { additions: content_lines as i32, deletions: 0 });
        }
    }
    // If output says success and contains a path, estimate additions from content.
    if output.to_lowercase().contains("success")
        || output.to_lowercase().starts_with("wrote ")
        || output.contains("写入成功")
        || output.contains("已保存")
        || output.contains("saved")
        || output == "ok"
    {
        let content_lines = args
            .get("content")
            .and_then(|v| v.as_str())
            .map(|s| s.lines().count().max(usize::from(!s.is_empty())))
            .unwrap_or(0);
        if content_lines > 0 {
            return Some(FileDiffStats { additions: content_lines as i32, deletions: 0 });
        }
    }
    None
}

/// Stateless dispatcher for one agent turn:
/// model response -> tool call(s) -> tool result message(s) -> next model response.
pub struct AgentDispatcher<'a> {
    provider: &'a dyn Provider,
    tools: &'a [Box<dyn Tool>],
    tool_specs: &'a [ToolSpec],
    max_tool_iterations: usize,
    security: &'a SecurityContext,
    budget: &'a BudgetTracker,
    /// Optional synchronous callback invoked immediately after each tool execution
    /// (Started and Completed events). This is the real-time event path.
    /// Wrapped in Arc so AgentDispatcher remains Clone.
    on_tool_event: Option<Arc<dyn Fn(ToolExecutionEvent) + Send + Sync + 'static>>,
}

impl<'a> Clone for AgentDispatcher<'a> {
    fn clone(&self) -> Self {
        Self {
            provider: self.provider,
            tools: self.tools,
            tool_specs: self.tool_specs,
            max_tool_iterations: self.max_tool_iterations,
            security: self.security,
            budget: self.budget,
            on_tool_event: self.on_tool_event.clone(),
        }
    }
}

impl<'a> AgentDispatcher<'a> {
    pub fn new(
        provider: &'a dyn Provider,
        tools: &'a [Box<dyn Tool>],
        tool_specs: &'a [ToolSpec],
        max_tool_iterations: usize,
        security: &'a SecurityContext,
        budget: &'a BudgetTracker,
    ) -> Self {
        Self {
            provider,
            tools,
            tool_specs,
            max_tool_iterations,
            security,
            budget,
            on_tool_event: None,
        }
    }

    /// Sets the real-time event callback. Events are emitted synchronously
    /// as tools execute, with no channel or spawned-task overhead.
    pub fn with_on_tool_event(
        mut self,
        on_event: Option<Arc<dyn Fn(ToolExecutionEvent) + Send + Sync + 'static>>,
    ) -> Self {
        self.on_tool_event = on_event;
        self
    }

    /// Emits a tool event synchronously if a callback is registered.
    fn emit_tool_event(&self, event: ToolExecutionEvent) {
        if let Some(ref cb) = self.on_tool_event {
            cb(event);
        }
    }

    /// Run the tool-calling loop against `messages` and return final assistant text.
    pub async fn run(&self, messages: &mut Vec<ChatMessage>) -> Result<String> {
        *messages = sanitize_messages_for_provider(std::mem::take(messages));
        let iteration_cap = self.max_tool_iterations.max(1);

        for iteration in 0..iteration_cap {
            if let Some(reason) = self.budget.check() {
                let text = format!(
                    "[budget exceeded] {reason}. Stopping here ({}).",
                    self.budget.summary()
                );
                self.security
                    .audit()
                    .record_event(
                        "budget_exceeded",
                        false,
                        &reason,
                        serde_json::json!({ "stage": "dispatcher", "iteration": iteration }),
                    )
                    .await;
                messages.push(ChatMessage::assistant(&text));
                return Ok(text);
            }

            let provider_name = self
                .security
                .audit()
                .context()
                .provider
                .clone()
                .unwrap_or_else(|| "default".to_string());

            let chat_result = self
                .provider
                .chat(ChatRequest {
                    messages,
                    tools: if self.tool_specs.is_empty() {
                        None
                    } else {
                        Some(self.tool_specs)
                    },
                })
                .await;

            if let Ok(response) = &chat_result {
                self.budget.record_call(response.usage.as_ref());
            }

            match &chat_result {
                Ok(response) => {
                    crate::observability::record_provider_call(&provider_name, "ok");
                    self.security
                        .audit_provider_call(
                            iteration,
                            response.tool_calls.len(),
                            true,
                            "provider returned response",
                        )
                        .await;
                }
                Err(err) => {
                    crate::observability::record_provider_call(&provider_name, "error");
                    self.security
                        .audit_provider_call(iteration, 0, false, &err.to_string())
                        .await;
                }
            }

            let response = chat_result?;

            if response.tool_calls.is_empty() {
                let text = response.text.unwrap_or_default();
                messages.push(ChatMessage::assistant(&text));
                return Ok(text);
            }

            let assistant_payload = serde_json::json!({
                "content": response.text,
                "reasoning_content": response.reasoning_content,
                "tool_calls": response.tool_calls,
            })
            .to_string();
            messages.push(ChatMessage::assistant(assistant_payload));

            for tool_call in response.tool_calls {
                let args: serde_json::Value = serde_json::from_str(&tool_call.arguments)
                    .unwrap_or(serde_json::Value::Null);
                let (tool_result, _exec_event) = self.execute_tool_call(&tool_call, &args).await?;
                let tool_payload = serde_json::json!({
                    "tool_call_id": tool_call.id,
                    "content": tool_result,
                })
                .to_string();
                messages.push(ChatMessage::tool(tool_payload));
            }
        }

        Ok("tool call loop limit reached".to_string())
    }

    /// One model call that streams text deltas via the returned channel sender.
    async fn stream_once(
        &self,
        messages: &[ChatMessage],
        with_tools: bool,
        tok_tx: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> Result<ChatResponse> {
        let request = ChatRequest {
            messages,
            tools: if with_tools && !self.tool_specs.is_empty() {
                Some(self.tool_specs)
            } else {
                None
            },
        };
        self.provider.chat_stream(request, tok_tx).await
    }

    /// Streaming variant of [`run`]: drives the tool-calling loop with budget
    /// awareness and emits tool events via the synchronous `on_tool_event` callback
    /// immediately as each tool starts/completes.  Token deltas are forwarded
    /// to `events` as [`AgentEvent::Token`].
    pub async fn run_streaming(
        &self,
        messages: &mut Vec<ChatMessage>,
        events: &tokio::sync::mpsc::UnboundedSender<AgentEvent>,
    ) -> Result<String> {
        *messages = sanitize_messages_for_provider(std::mem::take(messages));
        let iteration_cap = self.max_tool_iterations.max(1);

        for iteration in 0..iteration_cap {
            if let Some(reason) = self.budget.check() {
                let text = format!(
                    "[budget exceeded] {reason}. Stopping here ({}).",
                    self.budget.summary()
                );
                self.security
                    .audit()
                    .record_event(
                        "budget_exceeded",
                        false,
                        &reason,
                        serde_json::json!({ "stage": "dispatcher_stream", "iteration": iteration }),
                    )
                    .await;
                messages.push(ChatMessage::assistant(&text));
                let _ = events.send(AgentEvent::Done(text.clone()));
                return Ok(text);
            }

            let provider_name = self
                .security
                .audit()
                .context()
                .provider
                .clone()
                .unwrap_or_else(|| "default".to_string());

            let (tok_tx, mut tok_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            let tok_tx_clone = tok_tx.clone();
            let ev_clone = events.clone();

            // Spawn a task to forward token deltas to the events channel.
            let _handle = tokio::spawn(async move {
                while let Some(t) = tok_rx.recv().await {
                    let _ = ev_clone.send(AgentEvent::Token(t));
                }
            });

            let chat_result = self.stream_once(messages, true, tok_tx_clone).await;

            if let Ok(response) = &chat_result {
                self.budget.record_call(response.usage.as_ref());
            }
            match &chat_result {
                Ok(response) => {
                    crate::observability::record_provider_call(&provider_name, "ok");
                    self.security
                        .audit_provider_call(
                            iteration,
                            response.tool_calls.len(),
                            true,
                            "provider returned response",
                        )
                        .await;
                }
                Err(err) => {
                    crate::observability::record_provider_call(&provider_name, "error");
                    self.security
                        .audit_provider_call(iteration, 0, false, &err.to_string())
                        .await;
                    let _ = events.send(AgentEvent::Error(err.to_string()));
                }
            }
            let response = chat_result?;

            if response.tool_calls.is_empty() {
                let text = response.text.unwrap_or_default();
                messages.push(ChatMessage::assistant(&text));
                let _ = events.send(AgentEvent::Done(text.clone()));
                return Ok(text);
            }

            let assistant_payload = serde_json::json!({
                "content": response.text,
                "reasoning_content": response.reasoning_content,
                "tool_calls": response.tool_calls,
            })
            .to_string();
            messages.push(ChatMessage::assistant(assistant_payload));

            for tool_call in response.tool_calls {
                let args: serde_json::Value = serde_json::from_str(&tool_call.arguments)
                    .unwrap_or(serde_json::Value::Null);
                let summary = build_tool_summary(&tool_call.name, &args);

                // Emit Started synchronously — immediately, before execute_tool_call.
                self.emit_tool_event(ToolExecutionEvent::Started {
                    tool_call_id: tool_call.id.clone(),
                    tool_name: tool_call.name.clone(),
                    summary,
                });

                let (tool_result, exec_event) = self.execute_tool_call(&tool_call, &args).await?;

                // Emit Completed/Failed synchronously — immediately after execution.
                if let Some(evt) = exec_event {
                    self.emit_tool_event(evt);
                }

                let tool_payload = serde_json::json!({
                    "tool_call_id": tool_call.id,
                    "content": tool_result,
                })
                .to_string();
                messages.push(ChatMessage::tool(tool_payload));
            }
        }

        let _ = events.send(AgentEvent::Done(String::new()));
        Ok("tool call loop limit reached".to_string())
    }

    /// Variant of `run_streaming` without token events (used by the HTTP gateway
    /// for real-time UI streaming). Tool events are still emitted synchronously.
    pub async fn run_streaming_no_tokens(
        &self,
        messages: &mut Vec<ChatMessage>,
    ) -> Result<String> {
        *messages = sanitize_messages_for_provider(std::mem::take(messages));
        let iteration_cap = self.max_tool_iterations.max(1);

        for iteration in 0..iteration_cap {
            if let Some(reason) = self.budget.check() {
                let text = format!(
                    "[budget exceeded] {reason}. Stopping here ({}).",
                    self.budget.summary()
                );
                self.security
                    .audit()
                    .record_event(
                        "budget_exceeded",
                        false,
                        &reason,
                        serde_json::json!({ "stage": "dispatcher_stream", "iteration": iteration }),
                    )
                    .await;
                messages.push(ChatMessage::assistant(&text));
                return Ok(text);
            }

            let provider_name = self
                .security
                .audit()
                .context()
                .provider
                .clone()
                .unwrap_or_else(|| "default".to_string());

            let (tok_tx, mut tok_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            let chat_result = self.stream_once(messages, true, tok_tx).await;

            // Drain token stream so provider future can complete (no forwarding needed).
            let _handle = tokio::spawn(async move {
                while tok_rx.recv().await.is_some() {}
            });

            if let Ok(response) = &chat_result {
                self.budget.record_call(response.usage.as_ref());
            }
            match &chat_result {
                Ok(response) => {
                    crate::observability::record_provider_call(&provider_name, "ok");
                    self.security
                        .audit_provider_call(
                            iteration,
                            response.tool_calls.len(),
                            true,
                            "provider returned response",
                        )
                        .await;
                }
                Err(err) => {
                    crate::observability::record_provider_call(&provider_name, "error");
                    self.security
                        .audit_provider_call(iteration, 0, false, &err.to_string())
                        .await;
                }
            }
            let response = chat_result?;

            if response.tool_calls.is_empty() {
                let text = response.text.unwrap_or_default();
                messages.push(ChatMessage::assistant(&text));
                return Ok(text);
            }

            let assistant_payload = serde_json::json!({
                "content": response.text,
                "reasoning_content": response.reasoning_content,
                "tool_calls": response.tool_calls,
            })
            .to_string();
            messages.push(ChatMessage::assistant(assistant_payload));

            for tool_call in response.tool_calls {
                let args: serde_json::Value = serde_json::from_str(&tool_call.arguments)
                    .unwrap_or(serde_json::Value::Null);
                let summary = build_tool_summary(&tool_call.name, &args);

                self.emit_tool_event(ToolExecutionEvent::Started {
                    tool_call_id: tool_call.id.clone(),
                    tool_name: tool_call.name.clone(),
                    summary,
                });

                let (tool_result, exec_event) = self.execute_tool_call(&tool_call, &args).await?;

                if let Some(evt) = exec_event {
                    self.emit_tool_event(evt);
                }

                let tool_payload = serde_json::json!({
                    "tool_call_id": tool_call.id,
                    "content": tool_result,
                })
                .to_string();
                messages.push(ChatMessage::tool(tool_payload));
            }
        }

        Ok("tool call loop limit reached".to_string())
    }

    /// Like `run_streaming` but also collects `ToolExecutionEvent`s for the final response.
    pub async fn run_with_events(
        &self,
        messages: &mut Vec<ChatMessage>,
    ) -> Result<(String, Vec<ToolExecutionEvent>)> {
        let collected = Arc::new(std::sync::Mutex::new(Vec::new()));
        let realtime = self.on_tool_event.clone();
        let on_event = {
            let c = collected.clone();
            move |evt: ToolExecutionEvent| {
                if let Some(cb) = realtime.as_ref() {
                    cb(evt.clone());
                }
                if let Ok(mut v) = c.lock() {
                    v.push(evt);
                }
            }
        };
        let dispatcher = AgentDispatcher {
            provider: self.provider,
            tools: self.tools,
            tool_specs: self.tool_specs,
            max_tool_iterations: self.max_tool_iterations,
            security: self.security,
            budget: self.budget,
            on_tool_event: Some(Arc::new(on_event)),
        };
        let reply = dispatcher.run_streaming_no_tokens(messages).await?;
        let events = Arc::try_unwrap(collected)
            .ok()
            .map(|m| m.into_inner().unwrap_or_default())
            .unwrap_or_default();
        Ok((reply, events))
    }

    async fn execute_tool_call(
        &self,
        tool_call: &ToolCall,
        args: &serde_json::Value,
    ) -> Result<(String, Option<ToolExecutionEvent>)> {
        let tool = self
            .tools
            .iter()
            .find(|t| t.name() == tool_call.name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {}", tool_call.name))?;

        match self
            .security
            .gate_tool_execution(tool_call.name.as_str(), args)
            .await?
        {
            crate::security::ToolExecutionGate::Blocked { reason } => {
                let msg = format!("tool blocked by security policy: {reason}");
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
            crate::security::ToolExecutionGate::ApprovalRequired { .. } => {
                let msg = "tool execution requires user approval";
                return Ok((
                    msg.to_string(),
                    Some(ToolExecutionEvent::Completed {
                        tool_call_id: tool_call.id.clone(),
                        tool_name: tool_call.name.clone(),
                        success: false,
                        duration_ms: 0,
                        result_summary: msg.to_string(),
                        diff_stats: None,
                    }),
                ));
            }
            crate::security::ToolExecutionGate::Proceed { .. } => {}
        }

        let start = Instant::now();
        let result = tool.execute(args.clone()).await;
        let elapsed = start.elapsed();

        match result {
            Ok(tool_result) => {
                let output = if tool_result.success {
                    tool_result.output.clone()
                } else {
                    tool_result.error.clone().unwrap_or(tool_result.output)
                };
                let truncated = truncate_for_display(&output, 2000);
                let diff_stats = extract_diff_stats(&tool_call.name, &output)
                    .or_else(|| compute_content_diff(&tool_call.name, args, &output));

                if is_command_output_tool(&tool_call.name) && !output.trim().is_empty() {
                    self.emit_tool_event(ToolExecutionEvent::CommandOutput {
                        tool_call_id: tool_call.id.clone(),
                        tool_name: tool_call.name.clone(),
                        output: truncate_for_display(&output, 20 * 1024),
                        is_final: true,
                    });
                }

                // Emit FileChanged events immediately for file write/edit tools.
                if tool_call.name == "file_write" || tool_call.name == "write_file"
                    || tool_call.name == "file_edit" || tool_call.name == "edit_file"
                    || tool_call.name == "str_replace_editor"
                {
                    let path = args
                        .get("path")
                        .or_else(|| args.get("file_path"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("?")
                        .to_string();
                    if let Some(ds) = diff_stats.clone() {
                        self.emit_tool_event(ToolExecutionEvent::FileChanged {
                            path,
                            additions: ds.additions,
                            deletions: ds.deletions,
                        });
                    } else if let Some(ds) = compute_content_diff(&tool_call.name, args, &output) {
                        self.emit_tool_event(ToolExecutionEvent::FileChanged {
                            path,
                            additions: ds.additions,
                            deletions: ds.deletions,
                        });
                    }
                }

                Ok((
                    output,
                    Some(ToolExecutionEvent::Completed {
                        tool_call_id: tool_call.id.clone(),
                        tool_name: tool_call.name.clone(),
                        success: tool_result.success,
                        duration_ms: elapsed.as_millis() as u64,
                        result_summary: truncated,
                        diff_stats,
                    }),
                ))
            }
            Err(e) => {
                let msg = e.to_string();
                if is_command_output_tool(&tool_call.name) && !msg.trim().is_empty() {
                    self.emit_tool_event(ToolExecutionEvent::CommandOutput {
                        tool_call_id: tool_call.id.clone(),
                        tool_name: tool_call.name.clone(),
                        output: truncate_for_display(&msg, 20 * 1024),
                        is_final: true,
                    });
                }
                Ok((
                    msg.clone(),
                    Some(ToolExecutionEvent::Completed {
                        tool_call_id: tool_call.id.clone(),
                        tool_name: tool_call.name.clone(),
                        success: false,
                        duration_ms: elapsed.as_millis() as u64,
                        result_summary: msg,
                        diff_stats: None,
                    }),
                ))
            }
        }
    }
}

fn is_command_output_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "shell" | "bash" | "run_command" | "Command" | "git_operations" | "git"
    )
}
