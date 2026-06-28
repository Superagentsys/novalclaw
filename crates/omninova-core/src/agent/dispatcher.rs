use crate::agent::budget::BudgetTracker;
use crate::agent::history::sanitize_messages_for_provider;
use crate::agent::{AgentEvent, FileDiffStats, ToolExecutionEvent};
use crate::providers::{ChatMessage, ChatRequest, ChatResponse, Provider, ToolCall};
use crate::security::SecurityContext;
use crate::tools::{Tool, ToolSpec};
use anyhow::Result;
use std::time::Instant;
use tokio::sync::mpsc::UnboundedSender;

/// Stateless dispatcher for one agent turn:
/// model response -> tool call(s) -> tool result message(s) -> next model response.
pub struct AgentDispatcher<'a> {
    provider: &'a dyn Provider,
    tools: &'a [Box<dyn Tool>],
    tool_specs: &'a [ToolSpec],
    max_tool_iterations: usize,
    security: &'a SecurityContext,
    budget: &'a BudgetTracker,
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
                let (tool_result, _exec_event) = self.execute_tool_call(&tool_call).await?;
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

    /// One streamed model call: forwards text deltas to `events` as
    /// [`AgentEvent::Token`] and returns the assembled response.
    async fn stream_once(
        &self,
        messages: &[ChatMessage],
        with_tools: bool,
        events: &UnboundedSender<AgentEvent>,
    ) -> Result<ChatResponse> {
        let (tok_tx, mut tok_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let ev = events.clone();
        let fwd = tokio::spawn(async move {
            while let Some(t) = tok_rx.recv().await {
                let _ = ev.send(AgentEvent::Token(t));
            }
        });
        let request = ChatRequest {
            messages,
            tools: if with_tools && !self.tool_specs.is_empty() {
                Some(self.tool_specs)
            } else {
                None
            },
        };
        let result = self.provider.chat_stream(request, tok_tx).await;
        let _ = fwd.await;
        result
    }

    /// Streaming variant of [`run`]: emits token deltas and tool steps over
    /// `events` while driving the same budget-aware tool-calling loop.
    pub async fn run_streaming(
        &self,
        messages: &mut Vec<ChatMessage>,
        events: &UnboundedSender<AgentEvent>,
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

            let chat_result = self.stream_once(messages, true, events).await;
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
                let tool_args: Option<serde_json::Value> =
                    serde_json::from_str(&tool_call.arguments).ok();
                let summary = tool_args
                    .as_ref()
                    .map(|a| build_tool_summary(&tool_call.name, a))
                    .unwrap_or_else(|| format!("正在执行工具：{}", tool_call.name));
                let _ = events.send(AgentEvent::ToolExecution(ToolExecutionEvent::Started {
                    tool_name: tool_call.name.clone(),
                    summary,
                }));
                let (tool_result, exec_event) = self.execute_tool_call(&tool_call).await?;
                if let Some(evt) = exec_event {
                    let _ = events.send(AgentEvent::ToolExecution(evt));
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

    /// Like `run_streaming` but also collects `ToolExecutionEvent`s.
    /// Returns `(reply_text, collected_events)`.
    pub async fn run_with_events(
        &self,
        messages: &mut Vec<ChatMessage>,
    ) -> Result<(String, Vec<ToolExecutionEvent>)> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<AgentEvent>();

        let handle = tokio::spawn(async move {
            let mut events = Vec::new();
            while let Some(evt) = rx.recv().await {
                match evt {
                    AgentEvent::ToolExecution(te) => events.push(te),
                    AgentEvent::Done(_) => break,
                    _ => {}
                }
            }
            events
        });

        let reply = self.run_streaming(messages, &tx).await;
        let events = handle.await.unwrap_or_default();

        let reply_text = match reply {
            Ok(t) => t,
            Err(e) => return Err(e),
        };

        Ok((reply_text, events))
    }

    async fn execute_tool_call(
        &self,
        tool_call: &ToolCall,
    ) -> Result<(String, Option<ToolExecutionEvent>)> {
        let tool = self
            .tools
            .iter()
            .find(|t| t.name() == tool_call.name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {}", tool_call.name))?;

        let args: serde_json::Value = serde_json::from_str(&tool_call.arguments)
            .map_err(|e| anyhow::anyhow!("Invalid tool arguments JSON: {e}"))?;

        match self
            .security
            .gate_tool_execution(tool_call.name.as_str(), &args)
            .await?
        {
            crate::security::ToolExecutionGate::Blocked { reason } => {
                let msg = format!("tool blocked by security policy: {reason}");
                return Ok((
                    msg.clone(),
                    Some(ToolExecutionEvent::Completed {
                        tool_name: tool_call.name.clone(),
                        success: false,
                        duration_ms: 0,
                        result_summary: msg,
                        diff_stats: None,
                    }),
                ));
            }
            crate::security::ToolExecutionGate::ApprovalRequired { pending } => {
                let msg = format!(
                    "tool execution requires approval (id={}, tool={}, reason={}). \
                     Approve with: omninova approvals approve {}",
                    pending.id, pending.tool_name, pending.reason, pending.id
                );
                return Ok((
                    msg.clone(),
                    Some(ToolExecutionEvent::Completed {
                        tool_name: tool_call.name.clone(),
                        success: false,
                        duration_ms: 0,
                        result_summary: msg,
                        diff_stats: None,
                    }),
                ));
            }
            crate::security::ToolExecutionGate::Proceed { .. } => {}
        }

        let started = Instant::now();
        let result = tool.execute(args.clone()).await?;
        let elapsed_ms = started.elapsed().as_millis() as u64;
        let success = result.success;

        let output = if success {
            result.output
        } else {
            result
                .error
                .clone()
                .unwrap_or_else(|| "tool execution failed".to_string())
        };

        self.security
            .audit_tool_call(
                tool_call.name.as_str(),
                &args,
                success,
                if success { "ok" } else { output.as_str() },
            )
            .await;

        let result_summary = if success {
            truncate_for_display(&output, 300)
        } else {
            truncate_for_display(&output, 200)
        };

        let diff_stats = if success {
            extract_diff_stats(&tool_call.name, &args, &output)
        } else {
            None
        };

        let event = Some(ToolExecutionEvent::Completed {
            tool_name: tool_call.name.clone(),
            success,
            duration_ms: elapsed_ms,
            result_summary,
            diff_stats,
        });

        Ok((output, event))
    }
}

/// Build a short human-readable summary of what a tool is doing.
fn build_tool_summary(tool_name: &str, args: &serde_json::Value) -> String {
    match tool_name {
        "file_read" => args
            .get("path")
            .and_then(|v| v.as_str())
            .map(|p| format!("正在读取文件：{}", p))
            .unwrap_or_else(|| "正在读取文件".to_string()),
        "file_write" => args
            .get("path")
            .and_then(|v| v.as_str())
            .map(|p| format!("正在写入文件：{}", p))
            .unwrap_or_else(|| "正在写入文件".to_string()),
        "file_edit" => args
            .get("path")
            .and_then(|v| v.as_str())
            .map(|p| format!("正在修改文件：{}", p))
            .unwrap_or_else(|| "正在修改文件".to_string()),
        "file_list" => args
            .get("path")
            .and_then(|v| v.as_str())
            .map(|p| format!("正在列出目录：{}", p))
            .unwrap_or_else(|| "正在列出目录".to_string()),
        "glob_search" => args
            .get("pattern")
            .and_then(|v| v.as_str())
            .map(|p| format!("正在搜索文件：{}", p))
            .unwrap_or_else(|| "正在搜索文件".to_string()),
        "content_search" => args
            .get("query")
            .and_then(|v| v.as_str())
            .map(|q| format!("正在搜索内容：{}", q))
            .unwrap_or_else(|| "正在搜索内容".to_string()),
        "shell" => args
            .get("command")
            .and_then(|v| v.as_str())
            .map(|c| {
                let preview = if c.len() > 60 {
                    format!("{}...", &c[..60])
                } else {
                    c.to_string()
                };
                format!("正在执行命令：{}", preview)
            })
            .unwrap_or_else(|| "正在执行命令".to_string()),
        "git_operations" => args
            .get("operation")
            .and_then(|v| v.as_str())
            .map(|op| format!("正在执行 Git 操作：{}", op))
            .unwrap_or_else(|| "正在执行 Git 操作".to_string()),
        "delegate" => args
            .get("task")
            .and_then(|v| v.as_str())
            .map(|t| format!("正在委托子任务：{}", t))
            .unwrap_or_else(|| "正在委托子任务".to_string()),
        _ => format!("正在执行工具：{}", tool_name),
    }
}

/// Truncate a tool output for UI display.
fn truncate_for_display(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        s.to_string()
    } else {
        format!("{}...\n[输出已截断]", &s[..max_chars])
    }
}

/// Attempt to extract git diff stats from a file write/edit result.
fn extract_diff_stats(
    tool_name: &str,
    args: &serde_json::Value,
    output: &str,
) -> Option<FileDiffStats> {
    match tool_name {
        "file_write" | "file_edit" => {
            if output.contains("is not a git repository")
                || output.contains("fatal: not a git repository")
            {
                return None;
            }
            let mut additions = 0i32;
            let mut deletions = 0i32;
            for line in output.lines() {
                if let Some(rest) = line.strip_prefix('\t') {
                    let parts: Vec<&str> = rest.split('\t').collect();
                    if parts.len() >= 2 {
                        if let (Ok(a), Ok(d)) = (
                            parts[0].parse::<i32>(),
                            parts[1].parse::<i32>(),
                        ) {
                            additions += a;
                            deletions += d;
                        }
                    }
                }
            }
            if additions > 0 || deletions > 0 {
                Some(FileDiffStats { additions, deletions })
            } else {
                None
            }
        }
        _ => None,
    }
}
