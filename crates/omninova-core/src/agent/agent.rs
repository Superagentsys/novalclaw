use crate::agent::budget::BudgetTracker;
use crate::agent::dispatcher::AgentDispatcher;
use crate::agent::event_bus::EventBus;
use crate::agent::context::maintain_context;
use crate::agent::planner::{self, Reflection};
use crate::agent::prompt::{bootstrap_system_messages, reconstruct_model_visible_messages};
use crate::agent::{AgentCancellationToken, AgentRunEvent, ToolExecutionEvent};
use crate::config::AgentConfig;
use crate::memory::{Memory, MemoryCategory};
use crate::observability::{with_context_telemetry, SharedSink};
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

const SUMMARIZER_PROMPT: &str = "你是对话摘要器。把下面较早的对话浓缩成要点，必须保留：\
用户的目标与偏好、已确认的事实与决定、未完成的任务与下一步、关键文件路径与命令、阻塞原因。\
省略寒暄与重复内容，不要编造未出现的信息。用中文输出，不超过 1200 字。\
不要改写或省略以 [任务] 或 [检查点] 开头的内容。";

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
        self.apply_request_system_prompt();

        let _ = self
            .memory
            .store(
                &format!("conversation/{}", uuid::Uuid::new_v4()),
                message,
                MemoryCategory::Conversation,
                None,
            )
            .await;

        self.compact_context_if_needed().await;

        if images.is_empty() {
            self.messages.push(ChatMessage::user(message));
        } else {
            self.messages
                .push(ChatMessage::user_with_images(message, images.to_vec()));
        }
        self.refresh_candidate();

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
                                 self.config.max_history_messages,
                                 self.config.compact_context,
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
        self.apply_request_system_prompt();

        let _ = self
            .memory
            .store(
                &format!("conversation/{}", uuid::Uuid::new_v4()),
                message,
                MemoryCategory::Conversation,
                None,
            )
            .await;

        self.compact_context_if_needed().await;
        self.messages.push(ChatMessage::user(message));
        self.refresh_candidate();

        let budget = BudgetTracker::new(self.config.budget.clone());
        let dispatcher = AgentDispatcher::new(
                             self.provider.as_ref(),
                             &self.tools,
                             &self.tool_specs,
                             self.config.max_tool_iterations,
                             &self.security,
                             &budget,
                             self.config.max_history_messages,
                             self.config.compact_context,
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
        self.apply_request_system_prompt();

        let _ = self
            .memory
            .store(
                &format!("conversation/{}", uuid::Uuid::new_v4().to_string()),
                message,
                MemoryCategory::Conversation,
                None,
            )
            .await;

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

        bus.run_started(self.config.name.clone(), external_session_id.clone(), None);

        let provider_name = self
            .security
            .audit()
            .context()
            .provider
            .clone()
            .unwrap_or_else(|| self.provider.name().to_string());
        let model_name = self
            .provider
            .model()
            .map(str::to_string)
            .or_else(|| self.security.audit().context().model.clone())
            .unwrap_or_default();
        let sink: SharedSink = Arc::new(bus.clone());
        let session_id = external_session_id.clone();
        let dispatch_result = with_context_telemetry(
            session_id,
            Some(run_id.clone()),
            provider_name,
            model_name,
            sink,
            async {
                crate::observability::set_exact_tokenizer(
                    self.provider.exact_tokenizer().map(str::to_string),
                );
                self.compact_context_if_needed().await;
                self.messages.push(ChatMessage::user(message));
                self.refresh_candidate();
                crate::observability::emit_candidate_usage(
                    &self.messages,
                    &self.tool_specs,
                    self.provider.context_budget().as_ref(),
                );
                let dispatcher = AgentDispatcher::new(
                    self.provider.as_ref(),
                    &self.tools,
                    &self.tool_specs,
                    self.config.max_tool_iterations,
                    &self.security,
                    &budget,
                    self.config.max_history_messages,
                    self.config.compact_context,
                )
                .with_event_bus(Some(bus.clone()))
                .with_cancel_token(Some(cancel_token.clone()));
                tokio::select! {
                    result = dispatcher.run_streaming_with_bus(&mut self.messages) => result,
                    _ = cancel_token.cancelled() => Err(anyhow::anyhow!("agent run cancelled")),
                }
            },
        )
        .await;

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
        self.apply_request_system_prompt();

        let _ = self
            .memory
            .store(
                &format!("conversation/{}", uuid::Uuid::new_v4()),
                message,
                MemoryCategory::Conversation,
                None,
            )
            .await;

        self.compact_context_if_needed().await;
        self.messages.push(ChatMessage::user(message));
        self.refresh_candidate();

        let budget = BudgetTracker::new(self.config.budget.clone());
        let dispatcher = AgentDispatcher::new(
                             self.provider.as_ref(),
                             &self.tools,
                             &self.tool_specs,
                             self.config.max_tool_iterations,
                             &self.security,
                             &budget,
                             self.config.max_history_messages,
                             self.config.compact_context,
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
                                     self.config.max_history_messages,
                                     self.config.compact_context,
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
                self.refresh_candidate();
                let dispatcher = AgentDispatcher::new(
                                     self.provider.as_ref(),
                                     &self.tools,
                                     &self.tool_specs,
                                     self.config.max_tool_iterations,
                                     &self.security,
                                     budget,
                                     self.config.max_history_messages,
                                     self.config.compact_context,
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
                                self.refresh_candidate();
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
        self.refresh_candidate();
        let dispatcher = AgentDispatcher::new(
                             self.provider.as_ref(),
                             &self.tools,
                             &self.tool_specs,
                             self.config.max_tool_iterations,
                             &self.security,
                             budget,
                             self.config.max_history_messages,
                             self.config.compact_context,
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
        self.refresh_candidate();
        Ok(text)
    }

    fn refresh_candidate(&self) {
        crate::observability::emit_candidate_usage(
            &self.messages,
            &self.tool_specs,
            self.provider.context_budget().as_ref(),
        );
    }

    /// Condense older turns into a summary once history outgrows
    /// `max_history_messages`, instead of dropping them outright.
    ///
    /// A failed summarizer call degrades to truncation that still preserves the
    /// leading system messages, so a provider hiccup never blocks the turn.
    async fn compact_context_if_needed(&mut self) {
        if !self.config.compact_context {
            return;
        }
        self.messages = maintain_context(
            self.provider.as_ref(),
            std::mem::take(&mut self.messages),
            &self.tool_specs,
            self.config.max_history_messages,
            true,
        )
        .await;
    }

    pub fn export_non_system_messages(&self) -> Vec<ChatMessage> {
        self.messages
            .iter()
            .filter(|message| message.role != "system")
            .cloned()
            .collect()
    }

    pub fn restore_history_with_fresh_system(&mut self, history: Vec<ChatMessage>) {
        self.messages = bootstrap_system_messages(&self.config);
        self.messages.extend(
            history
                .into_iter()
                .filter(|message| message.role != "system"),
        );
    }

    pub fn import_messages(&mut self, messages: Vec<ChatMessage>) {
        self.messages = messages;
    }

    pub fn export_messages(&self) -> Vec<ChatMessage> {
        self.messages.clone()
    }

    /// Skill instructions are request-scoped working context. Rebuild the
    /// bootstrap system prompt from this request's config and drop leftover
    /// full SKILL.md payloads before the next model turn.
    pub(crate) fn apply_request_system_prompt(&mut self) {
        self.messages = reconstruct_model_visible_messages(
            &self.config,
            std::mem::take(&mut self.messages),
        );
    }

    /// Restore ledger history and current system/tool surface for projection.
    /// Does not prune, compact, append a user message, or call the Provider.
    pub(crate) fn reconstruct_for_projection(&mut self, history: Vec<ChatMessage>) {
        self.import_messages(history);
        self.apply_request_system_prompt();
    }

    /// Measure the reconstructed model-visible context locally.
    /// Never calls Provider, pruning, or compaction.
    pub(crate) async fn project_open_context(
        &self,
        session_id: Option<String>,
    ) -> crate::observability::ContextUsageSnapshot {
        let provider_name = self
            .security
            .audit()
            .context()
            .provider
            .clone()
            .unwrap_or_else(|| self.provider.name().to_string());
        let model_name = self
            .security
            .audit()
            .context()
            .model
            .clone()
            .or_else(|| self.provider.model().map(str::to_string))
            .unwrap_or_default();
        let budget = self.provider.context_budget();
        crate::observability::measure_projected_context(
            session_id,
            &provider_name,
            &model_name,
            self.provider.exact_tokenizer().map(str::to_string),
            budget.as_ref(),
            &self.messages,
            &self.tool_specs,
        )
        .await
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
    use crate::providers::{ChatRequest, ChatResponse, MockProvider, TokenUsage, ToolCall};
    use crate::tools::ToolResult;
    use async_trait::async_trait;
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    struct SequenceProvider {
        responses: Mutex<VecDeque<ChatResponse>>,
        requests: Arc<Mutex<Vec<Vec<ChatMessage>>>>,
    }

    impl SequenceProvider {
        fn new(responses: Vec<ChatResponse>) -> (Self, Arc<Mutex<Vec<Vec<ChatMessage>>>>) {
            let requests = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    responses: Mutex::new(responses.into()),
                    requests: requests.clone(),
                },
                requests,
            )
        }
    }

    #[async_trait]
    impl Provider for SequenceProvider {
        fn name(&self) -> &str {
            "sequence"
        }

        fn model(&self) -> Option<&str> {
            Some("sequence-model")
        }

        async fn chat(&self, request: ChatRequest<'_>) -> anyhow::Result<ChatResponse> {
            self.requests.lock().unwrap().push(request.messages.to_vec());
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| anyhow::anyhow!("no queued provider response"))
        }

        async fn health_check(&self) -> bool {
            true
        }
    }

    struct AlreadyActiveUseSkillTool;

    #[async_trait]
    impl Tool for AlreadyActiveUseSkillTool {
        fn name(&self) -> &str {
            "use_skill"
        }

        fn description(&self) -> &str {
            "Test already-active skill"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            json!({
                "type": "object",
                "properties": { "skill_id": { "type": "string" } },
                "required": ["skill_id"]
            })
        }

        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            Ok(ToolResult {
                success: true,
                output: json!({
                    "ok": true,
                    "already_active": true,
                    "skill_id": "skill:test",
                    "display_name": "Test"
                })
                .to_string(),
                error: None,
            })
        }
    }

    fn response(text: Option<&str>, tool_calls: Vec<ToolCall>, finish_reason: &str) -> ChatResponse {
        ChatResponse {
            text: text.map(ToString::to_string),
            tool_calls,
            usage: Some(TokenUsage {
                input_tokens: Some(10),
                output_tokens: Some(text.map(|value| value.chars().count() as u64).unwrap_or(0)),
            }),
            reasoning_content: None,
            finish_reason: Some(finish_reason.to_string()),
        }
    }

    fn agent_with_provider(
        provider: Box<dyn Provider>,
        tools: Vec<Box<dyn Tool>>,
        config: AgentConfig,
    ) -> Agent {
        let memory: Arc<dyn Memory> = Arc::new(crate::InMemoryMemory::new());
        let security = SecurityContext::from_config(&Config::default());
        Agent::new(provider, tools, memory, config, security)
    }

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
    async fn explicit_skill_stream_request_keeps_active_prompt_and_user_message() {
        let (provider, requests) = SequenceProvider::new(vec![response(
            Some("visible skill reply"),
            Vec::new(),
            "stop",
        )]);
        let mut config = AgentConfig::default();
        config.system_prompt = Some(
            "base\n\n## Active Skill\n\nSkill `skill:test` is already active.".to_string(),
        );
        let mut agent = agent_with_provider(Box::new(provider), Vec::new(), config);
        let events = Arc::new(Mutex::new(Vec::<AgentRunEvent>::new()));
        let event_sink = events.clone();

        let (reply, _) = agent
            .process_message_with_events_streaming(
                "请按照技能规则介绍能力。",
                Box::new(move |event| event_sink.lock().unwrap().push(event)),
                Some("run-explicit-message".to_string()),
                Some("session-explicit-message".to_string()),
                AgentCancellationToken::new(),
            )
            .await
            .expect("explicit skill request should complete");

        assert_eq!(reply, "visible skill reply");
        let captured = requests.lock().unwrap();
        let first = captured.first().expect("provider request");
        assert!(first.iter().any(|message| {
            message.role == "system"
                && crate::skills::is_skill_working_context_system(&message.content)
        }));
        assert!(first.iter().any(|message| {
            message.role == "user" && message.content == "请按照技能规则介绍能力。"
        }));
    }

    #[tokio::test]
    async fn empty_model_output_surfaces_run_failed_instead_of_run_completed() {
        let (provider, _) = SequenceProvider::new(vec![response(None, Vec::new(), "stop")]);
        let mut config = AgentConfig::default();
        config.system_prompt = Some("## Active Skill\n\nSkill is active.".to_string());
        let mut agent = agent_with_provider(Box::new(provider), Vec::new(), config);
        let events = Arc::new(Mutex::new(Vec::<AgentRunEvent>::new()));
        let event_sink = events.clone();

        let error = agent
            .process_message_with_events_streaming(
                "task",
                Box::new(move |event| event_sink.lock().unwrap().push(event)),
                Some("run-empty-output".to_string()),
                None,
                AgentCancellationToken::new(),
            )
            .await
            .expect_err("empty output must fail visibly");
        assert!(error
            .to_string()
            .contains("模型本次未返回可显示的内容"));
        tokio::task::yield_now().await;
        let captured = events.lock().unwrap();
        assert!(captured
            .iter()
            .any(|event| matches!(event, AgentRunEvent::run_failed { .. })));
        assert!(!captured
            .iter()
            .any(|event| matches!(event, AgentRunEvent::run_completed { .. })));
    }

    #[tokio::test]
    async fn skill_content_filter_surfaces_specific_provider_incompatibility() {
        let (provider, _) =
            SequenceProvider::new(vec![response(None, Vec::new(), "content_filter")]);
        let mut config = AgentConfig::default();
        config.system_prompt = Some("## Active Skill\n\nSkill is active.".to_string());
        let mut agent = agent_with_provider(Box::new(provider), Vec::new(), config);
        let events = Arc::new(Mutex::new(Vec::<AgentRunEvent>::new()));
        let event_sink = events.clone();

        let error = agent
            .process_message_with_events_streaming(
                "harmless task",
                Box::new(move |event| event_sink.lock().unwrap().push(event)),
                Some("run-content-filter".to_string()),
                None,
                AgentCancellationToken::new(),
            )
            .await
            .expect_err("provider content filter must fail without fallback");
        assert_eq!(
            error.to_string(),
            "当前模型服务拒绝了该技能的提示内容。技能已成功加载，但与当前模型/服务的安全策略不兼容。请更换技能或模型。"
        );
        tokio::task::yield_now().await;
        let captured = events.lock().unwrap();
        assert!(captured
            .iter()
            .any(|event| matches!(event, AgentRunEvent::run_failed { .. })));
        assert!(!captured
            .iter()
            .any(|event| matches!(event, AgentRunEvent::run_completed { .. })));
    }

    #[tokio::test]
    async fn already_active_use_skill_tool_call_continues_to_followup_text() {
        let tool_call = ToolCall {
            id: "call-use-skill".to_string(),
            name: "use_skill".to_string(),
            arguments: json!({ "skill_id": "skill:test" }).to_string(),
        };
        let (provider, requests) = SequenceProvider::new(vec![
            response(None, vec![tool_call], "tool_calls"),
            response(Some("continued after already-active tool"), Vec::new(), "stop"),
        ]);
        let mut config = AgentConfig::default();
        config.system_prompt = Some("## Active Skill\n\nSkill is active.".to_string());
        let mut agent = agent_with_provider(
            Box::new(provider),
            vec![Box::new(AlreadyActiveUseSkillTool)],
            config,
        );

        let (reply, _) = agent
            .process_message_with_events_streaming(
                "task",
                Box::new(|_| {}),
                Some("run-tool-followup".to_string()),
                None,
                AgentCancellationToken::new(),
            )
            .await
            .expect("tool call should continue to a second model iteration");

        assert_eq!(reply, "continued after already-active tool");
        let captured = requests.lock().unwrap();
        assert_eq!(captured.len(), 2);
        assert!(captured[1]
            .iter()
            .any(|message| message.role == "tool" && message.content.contains("already_active")));
    }

    #[tokio::test]
    async fn normal_streaming_request_still_returns_visible_text() {
        let (provider, _) = SequenceProvider::new(vec![response(
            Some("normal visible reply"),
            Vec::new(),
            "stop",
        )]);
        let mut agent = agent_with_provider(
            Box::new(provider),
            Vec::new(),
            AgentConfig::default(),
        );

        let (reply, _) = agent
            .process_message_with_events_streaming(
                "hello",
                Box::new(|_| {}),
                Some("run-normal-stream".to_string()),
                None,
                AgentCancellationToken::new(),
            )
            .await
            .expect("normal request should complete");
        assert_eq!(reply, "normal visible reply");
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

    fn usage_snapshots(events: &[AgentRunEvent]) -> Vec<crate::observability::ContextUsageSnapshot> {
        events
            .iter()
            .filter_map(|event| match event {
                AgentRunEvent::context_usage { snapshot, .. } => Some(snapshot.clone()),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn h_tool_result_append_emits_candidate_immediately() {
        let tools = vec![crate::tools::ToolSpec {
            name: "echo".into(),
            description: "echo".into(),
            parameters: json!({"type": "object", "properties": {}}),
        }];
        let mut messages = vec![
            ChatMessage::system("sys"),
            ChatMessage::user("run echo"),
            ChatMessage::assistant(
                r#"{"tool_calls":[{"id":"call-echo","name":"echo","arguments":"{}"}]}"#,
            ),
        ];
        let sink = Arc::new(crate::observability::VecContextTelemetry::new());
        crate::observability::with_context_telemetry(
            Some("session-h".into()),
            Some("run-h".into()),
            "sequence",
            "sequence-model",
            sink.clone(),
            async {
                crate::observability::emit_candidate_usage(&messages, &tools, None);
                let tool_payload = serde_json::json!({
                    "tool_call_id": "call-echo",
                    "content": "T".repeat(3_000),
                })
                .to_string();
                messages.push(ChatMessage::tool(tool_payload));
                crate::observability::emit_candidate_usage(&messages, &tools, None);
            },
        )
        .await;
        let snapshots = sink.snapshots();
        assert_eq!(snapshots.len(), 2);
        assert!(snapshots.iter().all(|s| {
            s.measurement_kind == crate::observability::MeasurementKind::CandidateEstimate
        }));
        assert!(snapshots[1].request_revision > snapshots[0].request_revision);
        assert!(
            snapshots[1].breakdown.tool_result_tokens > 1_000,
            "appended tool result must appear in Candidate: {:?}",
            snapshots[1].breakdown
        );
        assert!(snapshots[1].estimated_input_tokens > snapshots[0].estimated_input_tokens);
        assert_eq!(snapshots[1].session_id.as_deref(), Some("session-h"));
        assert_eq!(snapshots[1].model, "sequence-model");
    }

    #[tokio::test]
    async fn h_user_assistant_tool_context_mutation_refreshes_candidate_revision() {
        let tools = vec![crate::tools::ToolSpec {
            name: "echo".into(),
            description: "echo".into(),
            parameters: json!({"type": "object", "properties": {}}),
        }];
        let mut messages = vec![ChatMessage::system("sys")];
        let sink = Arc::new(crate::observability::VecContextTelemetry::new());
        crate::observability::with_context_telemetry(
            Some("session-h2".into()),
            Some("run-h2".into()),
            "sequence",
            "sequence-model",
            sink.clone(),
            async {
                crate::observability::emit_candidate_usage(&messages, &tools, None);
                messages.push(ChatMessage::user("run echo"));
                crate::observability::emit_candidate_usage(&messages, &tools, None);
                messages.push(ChatMessage::assistant(
                    r#"{"tool_calls":[{"id":"call-echo","name":"echo","arguments":"{}"}]}"#,
                ));
                crate::observability::emit_candidate_usage(&messages, &tools, None);
                messages.push(ChatMessage::tool(
                    serde_json::json!({"tool_call_id":"call-echo","content":"ok"}).to_string(),
                ));
                crate::observability::emit_candidate_usage(&messages, &tools, None);
            },
        )
        .await;
        let snapshots = sink.snapshots();
        assert_eq!(snapshots.len(), 4);
        for window in snapshots.windows(2) {
            assert!(window[1].request_revision > window[0].request_revision);
            assert_eq!(
                window[1].measurement_kind,
                crate::observability::MeasurementKind::CandidateEstimate
            );
        }
    }

    #[tokio::test]
    async fn k_session_restore_emits_initial_candidate_after_assembly() {
        let (provider, requests) = SequenceProvider::new(vec![response(
            Some("next"),
            Vec::new(),
            "stop",
        )]);
        let mut config = AgentConfig::default();
        config.compact_context = false;
        config.system_prompt = Some("restored system".into());
        let mut agent = agent_with_provider(Box::new(provider), Vec::new(), config);
        agent.import_messages(vec![
            ChatMessage::system("restored system"),
            ChatMessage::user("earlier question"),
            ChatMessage::assistant("earlier answer ".to_string() + &"z".repeat(200)),
        ]);
        let events = Arc::new(Mutex::new(Vec::<AgentRunEvent>::new()));
        let event_sink = events.clone();
        agent
            .process_message_with_events_streaming(
                "follow up",
                Box::new(move |event| event_sink.lock().unwrap().push(event)),
                Some("run-k".into()),
                Some("session-k".into()),
                AgentCancellationToken::new(),
            )
            .await
            .expect("restored session");
        let snapshots = usage_snapshots(&events.lock().unwrap());
        let first = snapshots.first().expect("restored session must emit Candidate");
        assert_eq!(
            first.measurement_kind,
            crate::observability::MeasurementKind::CandidateEstimate
        );
        assert_eq!(first.session_id.as_deref(), Some("session-k"));
        assert_eq!(first.run_id.as_deref(), Some("run-k"));
        assert_eq!(first.provider, "sequence");
        assert_eq!(first.model, "sequence-model");
        assert_eq!(first.request_revision, 1);
        assert!(first.estimated_input_tokens > 0);
        let after_user = snapshots.iter().find(|s| {
            s.measurement_kind == crate::observability::MeasurementKind::CandidateEstimate
                && s.request_revision > 1
        });
        assert!(
            after_user.is_some(),
            "user append must emit a newer Candidate: {:?}",
            snapshots
                .iter()
                .map(|s| (s.measurement_kind, s.request_revision))
                .collect::<Vec<_>>()
        );
        assert!(
            first.breakdown.system_tokens > 0 || first.breakdown.conversation_tokens > 0,
            "restored history missing from candidate: {:?}",
            first.breakdown
        );
        assert!(
            !requests.lock().unwrap().is_empty(),
            "provider still runs after the restored candidate"
        );
    }
}
