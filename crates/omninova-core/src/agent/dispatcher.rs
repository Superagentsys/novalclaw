use crate::agent::budget::BudgetTracker;
use crate::agent::history::sanitize_messages_for_provider;
use crate::agent::AgentEvent;
use crate::providers::{ChatMessage, ChatRequest, ChatResponse, Provider, ToolCall};
use crate::security::SecurityContext;
use crate::tools::{Tool, ToolSpec};
use anyhow::Result;
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
                let tool_result = self.execute_tool_call(&tool_call).await?;
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
                let _ = events.send(AgentEvent::Step(format!("调用工具 {}", tool_call.name)));
                let tool_result = self.execute_tool_call(&tool_call).await?;
                let _ = events.send(AgentEvent::Step(format!("完成 {}", tool_call.name)));
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

    async fn execute_tool_call(&self, tool_call: &ToolCall) -> Result<String> {
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
                return Ok(format!("tool blocked by security policy: {reason}"));
            }
            crate::security::ToolExecutionGate::ApprovalRequired { pending } => {
                return Ok(format!(
                    "tool execution requires approval (id={}, tool={}, reason={}). \
                     Approve with: omninova approvals approve {}",
                    pending.id, pending.tool_name, pending.reason, pending.id
                ));
            }
            crate::security::ToolExecutionGate::Proceed { .. } => {}
        }

        let result = tool.execute(args.clone()).await?;
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
        Ok(output)
    }
}
