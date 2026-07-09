use crate::agent::budget::BudgetTracker;
use crate::agent::event_bus::EventBus;
use crate::agent::history::sanitize_messages_for_provider;
use crate::agent::tool_runner::ToolRunner;
use crate::agent::{build_tool_prepare_summary, build_tool_summary, AgentCancellationToken, AgentEvent, ToolExecutionEvent};
use crate::providers::{ChatMessage, ChatRequest, ChatResponse, Provider, ToolCall};
use crate::security::SecurityContext;
use crate::tools::{Tool, ToolSpec};
use anyhow::Result;
use std::sync::Arc;
use tracing;

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

/// Stateless dispatcher for one agent turn:
/// model response -> tool call(s) -> tool result message(s) -> next model response.
pub struct AgentDispatcher<'a> {
    provider: &'a dyn Provider,
    tools: &'a [Box<dyn Tool>],
    tool_specs: &'a [ToolSpec],
    max_tool_iterations: usize,
    security: &'a SecurityContext,
    budget: &'a BudgetTracker,
    /// Optional EventBus for real-time structured events.
    event_bus: Option<EventBus>,
    cancel_token: Option<AgentCancellationToken>,
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
            event_bus: self.event_bus.clone(),
            cancel_token: self.cancel_token.clone(),
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
            event_bus: None,
            cancel_token: None,
        }
    }

    /// Sets the EventBus for real-time structured events.
    pub fn with_event_bus(mut self, bus: Option<EventBus>) -> Self {
        self.event_bus = bus;
        self
    }

    pub fn with_cancel_token(mut self, cancel_token: Option<AgentCancellationToken>) -> Self {
        self.cancel_token = cancel_token;
        self
    }

    /// Run the tool-calling loop against `messages` and return final assistant text.
    /// Uses the legacy path (no real-time events).
    pub async fn run(&self, messages: &mut Vec<ChatMessage>) -> Result<String> {
        *messages = sanitize_messages_for_provider(std::mem::take(messages));
        let iteration_cap = self.max_tool_iterations.max(1);

        for iteration in 0..iteration_cap {
            if let Some(token) = &self.cancel_token {
                token.check()?;
            }
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
                let (tool_result, _exec_event) =
                    self.execute_tool_call_internal(&tool_call, &args, None).await?;
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

    /// Streaming variant with real-time EventBus support.
    /// Drives the tool-calling loop with budget awareness and emits structured
    /// `AgentRunEvent`s via the EventBus immediately as each tool executes.
    pub async fn run_streaming_with_bus(&self, messages: &mut Vec<ChatMessage>) -> Result<String> {
        *messages = sanitize_messages_for_provider(std::mem::take(messages));
        let iteration_cap = self.max_tool_iterations.max(1);
        let mut written_files: Vec<String> = Vec::new();

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
                if let Some(ref bus) = self.event_bus {
                    bus.run_failed(format!("budget exceeded: {reason}"));
                }
                return Err(anyhow::anyhow!(text));
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
            let model_title = if iteration == 0 {
                "正在分析项目文件".to_string()
            } else {
                "正在整理最终回复".to_string()
            };
            let model_step_id = self
                .event_bus
                .as_ref()
                .map(|bus| bus.model_started(model_title.clone()));
            let bus_for_delta = self.event_bus.clone();
            let model_step_for_delta = model_step_id.clone();
            let _handle = tokio::spawn(async move {
                while let Some(delta) = tok_rx.recv().await {
                    if let (Some(bus), Some(step_id)) = (&bus_for_delta, &model_step_for_delta) {
                        bus.model_delta(step_id.clone(), delta);
                    }
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
                }
            }
            let response = chat_result?;
            if let Some(token) = &self.cancel_token {
                token.check()?;
            }
            if let (Some(bus), Some(step_id)) = (&self.event_bus, model_step_id) {
                bus.model_completed(step_id, "模型阶段完成".to_string());
            }

            if response.tool_calls.is_empty() {
                let validation_summary = self.validate_web_artifacts(&written_files, messages).await?;
                let mut text = response.text.unwrap_or_default();
                if let Some(summary) = validation_summary {
                    if !text.contains("验证结果") {
                        if !text.trim().is_empty() {
                            text.push_str("\n\n");
                        }
                        text.push_str(&summary);
                    }
                }
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
                if let Some(token) = &self.cancel_token {
                    token.check()?;
                }
                let args: serde_json::Value = serde_json::from_str(&tool_call.arguments)
                    .unwrap_or(serde_json::Value::Null);
                if let Some(ref bus) = self.event_bus {
                    bus.tool_call_created(
                        tool_call.id.clone(),
                        tool_call.name.clone(),
                        build_tool_prepare_summary(&tool_call.name, &args),
                    );
                }

                // Execute tool via internal helper (with EventBus support).
                let (tool_result, exec_event) = self
                    .execute_tool_call_internal(&tool_call, &args, self.event_bus.clone())
                    .await?;
                if matches!(
                    tool_call.name.as_str(),
                    "file_write" | "write_file" | "file_edit" | "edit_file" | "str_replace_editor" | "file_patch" | "apply_patch"
                ) {
                    if let Some(ToolExecutionEvent::Completed { success: true, .. }) = &exec_event {
                        if let Some(path) = args
                            .get("path")
                            .or_else(|| args.get("file_path"))
                            .and_then(|v| v.as_str())
                        {
                            if !written_files.iter().any(|item| item == path) {
                                written_files.push(path.to_string());
                            }
                        }
                    }
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

    /// Streaming variant with token event forwarding to `events` (legacy path).
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
                let _ = events.send(AgentEvent::ToolExecution(ToolExecutionEvent::Started {
                    tool_call_id: tool_call.id.clone(),
                    tool_name: tool_call.name.clone(),
                    summary,
                }));
                let (tool_result, exec_event) = self
                    .execute_tool_call_internal(&tool_call, &args, None)
                    .await?;
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

    /// Variant without token events ?? used by the gateway HTTP path.
    pub async fn run_streaming_no_tokens(&self, messages: &mut Vec<ChatMessage>) -> Result<String> {
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

            // Drain token stream so provider future can complete.
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

                let (tool_result, _exec_event) = self
                    .execute_tool_call_internal(&tool_call, &args, self.event_bus.clone())
                    .await?;

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

    /// Collects tool execution events for the final reply (legacy path).
    pub async fn run_with_events(
        &self,
        messages: &mut Vec<ChatMessage>,
    ) -> Result<(String, Vec<ToolExecutionEvent>)> {
        let collected = Arc::new(std::sync::Mutex::new(Vec::new()));
        let on_event = {
            let c = collected.clone();
            move |evt: ToolExecutionEvent| {
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
            event_bus: self.event_bus.clone(),
            cancel_token: self.cancel_token.clone(),
        };
        let reply = dispatcher.run_streaming_no_tokens(messages).await?;
        let events = Arc::try_unwrap(collected)
            .ok()
            .map(|m| m.into_inner().unwrap_or_default())
            .unwrap_or_default();
        Ok((reply, events))
    }

    async fn validate_read_file(
        &self,
        file_read: &dyn Tool,
        token: AgentCancellationToken,
        target: &str,
    ) -> Result<Option<String>> {
        token.check()?;
        let args = serde_json::json!({ "path": target });
        let tool_call_id = format!("validation-{}", uuid::Uuid::new_v4());
        let title = format!("验证文件存在：{target}");
        let step_id = if let Some(bus) = &self.event_bus {
            bus.tool_call_created(tool_call_id.clone(), "file_read".to_string(), format!("准备{title}"));
            bus.tool_started(tool_call_id.clone(), "file_read".to_string(), format!("开始{title}"), None)
        } else {
            uuid::Uuid::new_v4().to_string()
        };
        let timer = std::time::Instant::now();
        let result = file_read.execute_with_cancel(args, token).await;
        let elapsed_ms = timer.elapsed().as_millis() as u64;
        match result {
            Ok(tool_result) => {
                let success = tool_result.success;
                let summary = if success {
                    format!("{target} 存在")
                } else {
                    tool_result.error.clone().unwrap_or_else(|| format!("{target} 不存在或无法读取"))
                };
                if let Some(bus) = &self.event_bus {
                    bus.tool_completed(
                        step_id,
                        tool_call_id,
                        "file_read".to_string(),
                        success,
                        elapsed_ms,
                        summary,
                        None,
                    );
                }
                Ok(if success { Some(tool_result.output) } else { None })
            }
            Err(err) => {
                if let Some(bus) = &self.event_bus {
                    bus.tool_completed(step_id, tool_call_id, "file_read".to_string(), false, elapsed_ms, err.to_string(), None);
                }
                Err(err)
            }
        }
    }

    async fn validate_web_artifacts(
        &self,
        written_files: &[String],
        messages: &[ChatMessage],
    ) -> Result<Option<String>> {
        let normalized_written: Vec<String> = written_files
            .iter()
            .map(|path| path.replace('\\', "/").to_lowercase())
            .collect();
        let wrote_index = normalized_written
            .iter()
            .any(|path| path == "index.html" || path.ends_with("/index.html"));
        let wrote_style = normalized_written
            .iter()
            .any(|path| path == "style.css" || path.ends_with("/style.css"));
        let wrote_script = normalized_written
            .iter()
            .any(|path| path == "script.js" || path.ends_with("/script.js"));
        if !(wrote_index || wrote_style || wrote_script) {
            return Ok(None);
        }

        let user_requested_three_files = messages
            .iter()
            .rev()
            .find(|message| message.role == "user")
            .map(|message| {
                let lower = message.content.to_lowercase();
                lower.contains("index.html") && lower.contains("style.css") && lower.contains("script.js")
            })
            .unwrap_or(false);
        let multi_file_mode = user_requested_three_files || wrote_style || wrote_script;

        let Some(file_read) = self.tools.iter().find(|tool| tool.name() == "file_read") else {
            return Err(anyhow::anyhow!("验证失败：file_read 工具不可用"));
        };

        let token = self.cancel_token.clone().unwrap_or_default();
        let mut index_output = self
            .validate_read_file(file_read.as_ref(), token.clone(), "index.html")
            .await?;
        if index_output.is_none() {
            return Err(anyhow::anyhow!("验证失败：index.html 不存在或无法读取"));
        }

        if multi_file_mode {
            let style_output = self
                .validate_read_file(file_read.as_ref(), token.clone(), "style.css")
                .await?;
            let script_output = self
                .validate_read_file(file_read.as_ref(), token.clone(), "script.js")
                .await?;
            if style_output.is_none() || script_output.is_none() {
                return Err(anyhow::anyhow!(
                    "验证失败：多文件网站需要 index.html、style.css、script.js 都存在"
                ));
            }

            let mut index_text = index_output.clone().unwrap_or_default();
            let mut refs_ok = index_text.contains("style.css") && index_text.contains("script.js");
            if !refs_ok {
                token.check()?;
                let mut hunks = Vec::new();
                if !index_text.contains("style.css") {
                    hunks.push(serde_json::json!({
                        "old_text": "</head>",
                        "new_text": "  <link rel=\"stylesheet\" href=\"style.css\">\n</head>",
                        "summary": "修复 style.css 引用"
                    }));
                }
                if !index_text.contains("script.js") {
                    hunks.push(serde_json::json!({
                        "old_text": "</body>",
                        "new_text": "  <script src=\"script.js\"></script>\n</body>",
                        "summary": "修复 script.js 引用"
                    }));
                }

                if hunks.is_empty() {
                    return Err(anyhow::anyhow!("验证失败：index.html 未同时引用 style.css 和 script.js"));
                }

                let patch_call = ToolCall {
                    id: format!("validation-fix-{}", uuid::Uuid::new_v4()),
                    name: "file_patch".to_string(),
                    arguments: serde_json::json!({
                        "path": "index.html",
                        "hunks": hunks,
                    })
                    .to_string(),
                };
                let patch_args: serde_json::Value = serde_json::from_str(&patch_call.arguments)?;
                if let Some(bus) = &self.event_bus {
                    bus.tool_call_created(
                        patch_call.id.clone(),
                        patch_call.name.clone(),
                        build_tool_prepare_summary(&patch_call.name, &patch_args),
                    );
                }
                let (patch_result, _) = self
                    .execute_tool_call_internal(&patch_call, &patch_args, self.event_bus.clone())
                    .await?;
                if !patch_result.contains("\"hunks_count\"") {
                    return Err(anyhow::anyhow!(
                        "验证失败：index.html 未同时引用 style.css 和 script.js，且自动修复失败：{}",
                        patch_result
                    ));
                }

                index_output = self
                    .validate_read_file(file_read.as_ref(), token.clone(), "index.html")
                    .await?;
                index_text = index_output.clone().unwrap_or_default();
                refs_ok = index_text.contains("style.css") && index_text.contains("script.js");
            }

            if let Some(bus) = &self.event_bus {
                let tool_call_id = format!("validation-{}", uuid::Uuid::new_v4());
                let title = "验证 index.html 引用 style.css 和 script.js".to_string();
                bus.tool_call_created(tool_call_id.clone(), "validation".to_string(), format!("准备{title}"));
                let step_id = bus.tool_started(tool_call_id.clone(), "validation".to_string(), format!("开始{title}"), None);
                bus.tool_completed(
                    step_id,
                    tool_call_id,
                    "validation".to_string(),
                    refs_ok,
                    0,
                    if refs_ok {
                        "index.html 已引用 style.css 和 script.js".to_string()
                    } else {
                        "index.html 未同时引用 style.css 和 script.js".to_string()
                    },
                    None,
                );
            }
            if !refs_ok {
                return Err(anyhow::anyhow!("验证失败：index.html 未同时引用 style.css 和 script.js"));
            }

            return Ok(Some(
                "验证结果：\n- index.html 存在\n- style.css 存在\n- script.js 存在\n- index.html 引用检查通过".to_string(),
            ));
        }

        let index_text = index_output.unwrap_or_default();
        let inline_ok = index_text.contains("<style") && index_text.contains("<script");
        if let Some(bus) = &self.event_bus {
            let tool_call_id = format!("validation-{}", uuid::Uuid::new_v4());
            let title = "验证 index.html 内联 CSS/JS".to_string();
            bus.tool_call_created(tool_call_id.clone(), "validation".to_string(), format!("准备{title}"));
            let step_id = bus.tool_started(tool_call_id.clone(), "validation".to_string(), format!("开始{title}"), None);
            bus.tool_completed(
                step_id,
                tool_call_id,
                "validation".to_string(),
                inline_ok,
                0,
                if inline_ok {
                    "index.html 包含内联 <style> 和 <script>".to_string()
                } else {
                    "index.html 未包含内联 <style> 和 <script>".to_string()
                },
                None,
            );
        }
        if !inline_ok {
            return Err(anyhow::anyhow!(
                "验证失败：单文件网站需要 index.html 包含内联 <style> 和 <script>"
            ));
        }

        Ok(Some(
            "验证结果：\n- index.html 存在\n- index.html 包含内联 <style>\n- index.html 包含内联 <script>".to_string(),
        ))
    }

    /// Internal tool execution — delegates to ToolRunner.
    /// Handles both legacy ToolExecutionEvent and the new AgentRunEvent via EventBus.
    async fn execute_tool_call_internal(
        &self,
        tool_call: &ToolCall,
        args: &serde_json::Value,
        event_bus: Option<EventBus>,
    ) -> Result<(String, Option<ToolExecutionEvent>)> {
        let runner = ToolRunner::new(self.tools, self.security)
            .with_event_bus(event_bus)
            .with_cancel_token(self.cancel_token.clone());
        runner.run_tool(tool_call, args).await
    }
}

/// E2E debug: returns the first `max_chars` chars of `s`, appending "..."
/// if `s` was longer. Never slices in the middle of a UTF-8 codepoint.
fn preview(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}
