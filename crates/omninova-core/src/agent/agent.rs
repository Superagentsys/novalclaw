use crate::agent::budget::BudgetTracker;
use crate::agent::dispatcher::AgentDispatcher;
use crate::agent::planner::{self, Reflection};
use crate::agent::prompt::bootstrap_system_messages;
use crate::config::AgentConfig;
use crate::memory::{Memory, MemoryCategory};
use crate::providers::{ChatMessage, Provider};
use crate::security::SecurityContext;
use crate::tools::{Tool, ToolSpec};
use anyhow::Result;
use std::sync::Arc;
use tracing::warn;

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
