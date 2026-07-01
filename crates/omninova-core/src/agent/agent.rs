use crate::agent::budget::BudgetTracker;
use crate::agent::dispatcher::AgentDispatcher;
use crate::agent::event_bus::EventBus;
use crate::agent::planner::{self, Reflection};
use crate::agent::prompt::bootstrap_system_messages;
use crate::agent::{AgentCancellationToken, AgentRunEvent, ToolExecutionEvent};
use crate::config::AgentConfig;
use crate::memory::{Memory, MemoryCategory};
use crate::providers::{ChatMessage, Provider};
use crate::security::SecurityContext;
use crate::tools::{Tool, ToolSpec};
use anyhow::Result;
use std::sync::Arc;
use tracing::warn;

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

fn truncate_chars_with_ellipsis(input: &str, max_chars: usize) -> String {
    let mut out: String = input.chars().take(max_chars).collect();
    if input.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

/// Cap on per-step result text quoted back into reflector prompts.
const REFLECT_RESULT_SNIPPET_CHARS: usize = 2_000;

pub struct Agent {
    provider: Box<dyn Provider>,
    tools: Vec<Box<dyn Tool>>,
    tool_specs: Vec<ToolSpec>,
    memory: Arc<dyn Memory>,
    config: AgentConfig,
    security: SecurityContext,
    messages: Vec<ChatMessage>,
}

impl Agent {
    pub fn new(
        provider: Box<dyn Provider>,
        tools: Vec<Box<dyn Tool>>,
        memory: Arc<dyn Memory>,
        config: AgentConfig,
        security: SecurityContext,
    ) -> Self {
        let tool_specs = tools.iter().map(|t| t.spec()).collect();
        Self {
            provider,
            tools,
            tool_specs,
            memory,
            config,
            security,
            messages: Vec::new(),
        }
    }

    pub async fn process_message(&mut self, message: &str) -> Result<String> {
        self.process_message_with_images(message, &[]).await
    }

    pub async fn process_message_with_images(
        &mut self,
        message: &str,
        images: &[String],
    ) -> Result<String> {
        if self.messages.is_empty() {
            self.messages.extend(bootstrap_system_messages(&self.config));
        }

        let _ = self
            .memory
            .store(
                &format!("conversation/{}", uuid::Uuid::new_v4()),
                message,
                MemoryCategory::Conversation,
                None,
            )
            .await;

        if images.is_empty() {
            self.messages.push(ChatMessage::user(message));
        } else {
            self.messages
                .push(ChatMessage::user_with_images(message, images.to_vec()));
        }

        // One budget spans the whole request: planner, executor and reflector
        // calls all draw from it.
        let budget = BudgetTracker::new(self.config.budget.clone());

        if self.config.planning.enabled {
            self.run_plan_execute_reflect(message, &budget).await
        } else {
            let dispatcher = AgentDispatcher::new(
                self.provider.as_ref(),
                &self.tools,
                &self.tool_specs,
                self.config.max_tool_iterations,
                &self.security,
                &budget,
            );
            dispatcher.run(&mut self.messages).await
        }
    }

    /// Like `process_message` but also collects rich tool-execution events
    /// for a live execution timeline. Returns `(reply_text, collected_events)`.
    pub async fn process_message_with_events(
        &mut self,
        message: &str,
    ) -> Result<(String, Vec<ToolExecutionEvent>)> {
        if self.messages.is_empty() {
            self.messages.extend(bootstrap_system_messages(&self.config));
        }

        let _ = self
            .memory
            .store(
                &format!("conversation/{}", uuid::Uuid::new_v4()),
                message,
                MemoryCategory::Conversation,
                None,
            )
            .await;

        self.messages.push(ChatMessage::user(message));

        let budget = BudgetTracker::new(self.config.budget.clone());
        let dispatcher = AgentDispatcher::new(
            self.provider.as_ref(),
            &self.tools,
            &self.tool_specs,
            self.config.max_tool_iterations,
            &self.security,
            &budget,
        );

        let (reply, events) = dispatcher.run_with_events(&mut self.messages).await?;

        Ok((reply, events))
    }

    /// Like `process_message_with_events` but also calls `on_event` immediately
    /// for each tool execution event, enabling real-time event streaming.
    /// Uses the new `EventBus` for structured, seq-ordered events.
    /// Processes a user message with real-time event streaming to the frontend.
    /// Events (tool_started, command_output, tool_completed, etc.) are forwarded
    /// via `on_event` as they happen, without waiting for the run to complete.
    ///
    /// `external_run_id` — if provided, the frontend already generated this run_id
    /// and all events must use it. Otherwise a new one is generated.
    /// `external_session_id` — session context for the run.
    pub async fn process_message_with_events_streaming(
        &mut self,
        message: &str,
        on_event: Box<dyn Fn(AgentRunEvent) + Send + Sync + 'static>,
        external_run_id: Option<String>,
        external_session_id: Option<String>,
        cancel_token: AgentCancellationToken,
    ) -> Result<(String, Vec<AgentRunEvent>)> {
        if self.messages.is_empty() {
            self.messages.extend(bootstrap_system_messages(&self.config));
        }

        let _ = self
            .memory
            .store(
                &format!("conversation/{}", uuid::Uuid::new_v4().to_string()),
                message,
                MemoryCategory::Conversation,
                None,
            )
            .await;

        self.messages.push(ChatMessage::user(message));

        let budget = BudgetTracker::new(self.config.budget.clone());

        // Use the frontend-provided run_id if available, otherwise generate one.
        let run_id = external_run_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        tracing::debug!(target: "e2e", "[e2e-agent-start] timestamp={} run_id={}", now_ts(), run_id);

        // Box<dyn Fn> is callable directly — deref coercion handles it.
        let emit_fn = move |evt: AgentRunEvent| {
            on_event(evt);
        };
        let (bus, drain_handle) = EventBus::new(run_id.clone(), emit_fn);

        // The drain task runs in background and exits when the channel closes.
        // We intentionally do NOT await drain_handle here — the caller does not
        // need to wait for flush. Awaiting it would deadlock: drain needs the
        // channel to close, which needs bus to drop, which needs this function
        // to return.
        tokio::spawn(async move {
            drain_handle.drain().await;
        });

        bus.run_started(self.config.name.clone(), external_session_id, None);

        let dispatcher = AgentDispatcher::new(
            self.provider.as_ref(),
            &self.tools,
            &self.tool_specs,
            self.config.max_tool_iterations,
            &self.security,
            &budget,
        )
        .with_event_bus(Some(bus.clone()))
        .with_cancel_token(Some(cancel_token.clone()));

        let run_future = dispatcher.run_streaming_with_bus(&mut self.messages);
        let dispatch_result = tokio::select! {
            result = run_future => result,
            _ = cancel_token.cancelled() => Err(anyhow::anyhow!("agent run cancelled")),
        };

        let reply_text = match dispatch_result {
            Ok(reply) => {
                let preview = truncate_chars_with_ellipsis(&reply, 200);
                if cancel_token.is_cancelled() {
                    bus.run_cancelled("任务已取消".to_string());
                    tracing::debug!(target: "e2e", "[e2e-agent-dispatch-cancelled-late] timestamp={} run_id={}", now_ts(), run_id);
                    Err(anyhow::anyhow!("agent run cancelled"))
                } else {
                    bus.run_completed(reply.clone(), preview);
                    tracing::debug!(target: "e2e", "[e2e-agent-dispatch-ok] timestamp={} run_id={} reply_len={}", now_ts(), run_id, reply.len());
                    Ok(reply)
                }
            }
            Err(e) => {
                if cancel_token.is_cancelled() {
                    bus.run_cancelled("任务已取消".to_string());
                    tracing::debug!(target: "e2e", "[e2e-agent-dispatch-cancelled] timestamp={} run_id={}", now_ts(), run_id);
                } else {
                    bus.run_failed(e.to_string());
                    tracing::debug!(target: "e2e", "[e2e-agent-dispatch-err] timestamp={} run_id={} error={}", now_ts(), run_id, e);
                }
                Err(e)
            }
        }?;

        // Events were already streamed via on_event as they happened.
        // bus.collect() is safe here — it only reads from the collected Vec,
        // it does NOT wait for the drain task.
        let events = bus.collect();

        tracing::debug!(target: "e2e", "[e2e-agent-return] timestamp={} run_id={} reply_len={} event_count={}", now_ts(), run_id, reply_text.len(), events.len());
        Ok((reply_text, events))
    }

    /// Streaming variant of [`process_message`]: drives the budget-aware tool
    /// loop while forwarding token deltas and tool steps over `events`. Uses the
    /// plain streaming ReAct path (Plan-Execute-Reflect is not streamed).
    pub async fn process_message_streaming(
        &mut self,
        message: &str,
        events: &tokio::sync::mpsc::UnboundedSender<crate::agent::AgentEvent>,
    ) -> Result<String> {
        if self.messages.is_empty() {
            self.messages.extend(bootstrap_system_messages(&self.config));
        }

        let _ = self
            .memory
            .store(
                &format!("conversation/{}", uuid::Uuid::new_v4()),
                message,
                MemoryCategory::Conversation,
                None,
            )
            .await;

        self.messages.push(ChatMessage::user(message));

        let budget = BudgetTracker::new(self.config.budget.clone());
        let dispatcher = AgentDispatcher::new(
            self.provider.as_ref(),
            &self.tools,
            &self.tool_specs,
            self.config.max_tool_iterations,
            &self.security,
            &budget,
        );
        dispatcher.run_streaming(&mut self.messages, events).await
    }

    /// Plan-Execute-Reflect loop: a planner decomposes the task, the executor
    /// (ReAct tool loop) runs one step at a time, and an isolated reflector
    /// judges progress after each step, optionally triggering a replan.
    async fn run_plan_execute_reflect(
        &mut self,
        task: &str,
        budget: &BudgetTracker,
    ) -> Result<String> {
        let max_plan_steps = self.config.planning.max_plan_steps.max(1);
        let max_replans = self.config.planning.max_replans;

        let mut plan = match planner::generate_plan(
            self.provider.as_ref(),
            task,
            max_plan_steps,
            None,
        )
        .await
        {
            Ok((steps, response)) => {
                budget.record_call(response.usage.as_ref());
                steps
            }
            Err(e) => {
                // Planner unavailable: degrade to the plain ReAct loop rather
                // than failing the request.
                warn!("planner failed, falling back to ReAct: {e}");
                let dispatcher = AgentDispatcher::new(
                    self.provider.as_ref(),
                    &self.tools,
                    &self.tool_specs,
                    self.config.max_tool_iterations,
                    &self.security,
                    budget,
                );
                return dispatcher.run(&mut self.messages).await;
            }
        };

        let mut replans_used = 0usize;
        let mut executed: Vec<(String, String)> = Vec::new();

        'plan: loop {
            let current_plan = plan.clone();
            for (idx, step) in current_plan.iter().enumerate() {
                if let Some(reason) = budget.check() {
                    return self.finish_on_budget(&reason, &executed).await;
                }

                self.messages.push(ChatMessage::user(format!(
                    "[Plan step {}/{}] {}\nExecute this step now using the available tools. \
                     Original task: {}",
                    idx + 1,
                    current_plan.len(),
                    step,
                    task
                )));
                let dispatcher = AgentDispatcher::new(
                    self.provider.as_ref(),
                    &self.tools,
                    &self.tool_specs,
                    self.config.max_tool_iterations,
                    &self.security,
                    budget,
                );
                let step_result = dispatcher.run(&mut self.messages).await?;
                executed.push((step.clone(), step_result));

                if let Some(reason) = budget.check() {
                    return self.finish_on_budget(&reason, &executed).await;
                }

                let transcript = render_transcript(&executed);
                let remaining = current_plan.len() - idx - 1;
                match planner::reflect(self.provider.as_ref(), task, &transcript, remaining).await
                {
                    Ok((verdict, response)) => {
                        budget.record_call(response.usage.as_ref());
                        match verdict {
                            Reflection::Complete { final_answer } => {
                                self.messages.push(ChatMessage::assistant(&final_answer));
                                return Ok(final_answer);
                            }
                            Reflection::Continue => {}
                            Reflection::Replan { feedback } => {
                                if replans_used >= max_replans {
                                    warn!("replan budget exhausted; continuing current plan");
                                    continue;
                                }
                                replans_used += 1;
                                match planner::generate_plan(
                                    self.provider.as_ref(),
                                    task,
                                    max_plan_steps,
                                    Some(&feedback),
                                )
                                .await
                                {
                                    Ok((new_plan, response)) => {
                                        budget.record_call(response.usage.as_ref());
                                        plan = new_plan;
                                        continue 'plan;
                                    }
                                    Err(e) => {
                                        warn!("replan failed, continuing current plan: {e}");
                                    }
                                }
                            }
                        }
                    }
                    // Reflector failures must not kill the run; keep executing.
                    Err(e) => warn!("reflector failed, continuing: {e}"),
                }
            }
            break;
        }

        // Plan exhausted without a Complete verdict: synthesize a final answer
        // from the accumulated context.
        self.messages.push(ChatMessage::user(format!(
            "All planned steps have been executed. Based on the results above, provide the \
             final answer to the original task now. Original task: {task}"
        )));
        let dispatcher = AgentDispatcher::new(
            self.provider.as_ref(),
            &self.tools,
            &self.tool_specs,
            self.config.max_tool_iterations,
            &self.security,
            budget,
        );
        dispatcher.run(&mut self.messages).await
    }

    /// Budget exhausted mid-plan: report partial progress instead of failing.
    async fn finish_on_budget(
        &mut self,
        reason: &str,
        executed: &[(String, String)],
    ) -> Result<String> {
        self.security
            .audit()
            .record_event(
                "budget_exceeded",
                false,
                reason,
                serde_json::json!({ "stage": "plan_execute_reflect" }),
            )
            .await;
        let text = format!(
            "[budget exceeded] {reason}. Partial progress:\n{}",
            render_transcript(executed)
        );
        self.messages.push(ChatMessage::assistant(&text));
        Ok(text)
    }

    pub fn import_messages(&mut self, messages: Vec<ChatMessage>) {
        self.messages = messages;
    }

    pub fn export_messages(&self) -> Vec<ChatMessage> {
        self.messages.clone()
    }
}

fn render_transcript(executed: &[(String, String)]) -> String {
    if executed.is_empty() {
        return "(no steps executed yet)".to_string();
    }
    executed
        .iter()
        .enumerate()
        .map(|(i, (step, result))| {
            format!(
                "Step {}: {}\nResult: {}",
                i + 1,
                step,
                truncate_chars(result, REFLECT_RESULT_SNIPPET_CHARS)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars).collect();
    format!("{truncated}…[truncated]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::providers::MockProvider;

    fn mock_agent(agent_cfg: AgentConfig) -> Agent {
        let provider = Box::new(MockProvider::new("mock"));
        let memory: Arc<dyn Memory> = Arc::new(crate::InMemoryMemory::new());
        let security = SecurityContext::from_config(&Config::default());
        Agent::new(provider, Vec::new(), memory, agent_cfg, security)
    }

    #[tokio::test]
    async fn react_path_returns_provider_text() {
        let mut agent = mock_agent(AgentConfig::default());
        let reply = agent.process_message("hello").await.expect("reply");
        assert_eq!(reply, "Mock response from provider");
    }

    #[tokio::test]
    async fn plan_execute_reflect_completes_with_mock_provider() {
        // Mock provider returns plain text: the plan parser falls back to a
        // single-step plan, the reflector verdict degrades to Continue, and
        // the loop ends with the synthesis run.
        let mut cfg = AgentConfig::default();
        cfg.planning.enabled = true;
        let mut agent = mock_agent(cfg);
        let reply = agent
            .process_message("complex multi-part task")
            .await
            .expect("reply");
        assert_eq!(reply, "Mock response from provider");
        // History contains the plan-step prompt and the synthesis prompt.
        let texts: Vec<String> = agent
            .export_messages()
            .iter()
            .map(|m| m.content.clone())
            .collect();
        assert!(texts.iter().any(|t| t.contains("[Plan step 1/1]")));
        assert!(
            texts
                .iter()
                .any(|t| t.contains("All planned steps have been executed"))
        );
    }

    #[tokio::test]
    async fn budget_zero_provider_calls_short_circuits() {
        let mut cfg = AgentConfig::default();
        cfg.budget.max_provider_calls = Some(0);
        let mut agent = mock_agent(cfg);
        let reply = agent.process_message("hello").await.expect("reply");
        assert!(reply.contains("[budget exceeded]"), "got: {reply}");
        assert!(reply.contains("provider-call budget"));
    }

    #[tokio::test]
    async fn budget_wall_time_zero_short_circuits_per_loop() {
        let mut cfg = AgentConfig::default();
        cfg.planning.enabled = true;
        cfg.budget.max_wall_time_secs = Some(0);
        let mut agent = mock_agent(cfg);
        let reply = agent.process_message("task").await.expect("reply");
        assert!(reply.contains("[budget exceeded]"), "got: {reply}");
    }
}
