//! Planner and reflector for the Plan-Execute-Reflect loop.
//!
//! Both run as separate prompts (no tools) against the same provider as the
//! executor. The reflector is intentionally isolated from the executor prompt
//! to avoid optimism bias.

use crate::providers::{ChatMessage, ChatRequest, ChatResponse, Provider};
use anyhow::Result;

/// Verdict produced by the reflector after each executed step.
#[derive(Debug, Clone)]
pub enum Reflection {
    /// Task accomplished; `final_answer` is the user-facing reply.
    Complete { final_answer: String },
    /// Keep executing the remaining plan steps.
    Continue,
    /// The current plan won't accomplish the task; replan with this feedback.
    Replan { feedback: String },
}

/// Ask the model to decompose `task` into an ordered list of concrete steps.
/// Returns at least one step (falls back to the task itself on parse failure).
pub async fn generate_plan(
    provider: &dyn Provider,
    task: &str,
    max_steps: usize,
    feedback: Option<&str>,
) -> Result<(Vec<String>, ChatResponse)> {
    let system = "You are the planning module of an AI agent. Decompose the task into a short \
                  ordered list of concrete, self-contained steps that an executor with tools \
                  (files, shell, web, delegation) can perform. Use as few steps as possible — \
                  output a single step for simple tasks. Respond with ONLY a JSON array of \
                  strings, no commentary.";
    let mut user = format!(
        "Task:\n{task}\n\nRespond with a JSON array of at most {max_steps} step descriptions."
    );
    if let Some(feedback) = feedback {
        user.push_str(&format!(
            "\n\nA previous plan failed; reflector feedback (address it in the new plan):\n{feedback}"
        ));
    }

    let messages = [ChatMessage::system(system), ChatMessage::user(user)];
    let response = provider
        .chat(ChatRequest {
            messages: &messages,
            tools: None,
        })
        .await?;
    let steps = parse_plan(response.text.as_deref().unwrap_or(""), max_steps, task);
    Ok((steps, response))
}

/// Judge whether the task is accomplished given the execution transcript.
/// Parse failures degrade to `Continue` so the loop stays robust.
pub async fn reflect(
    provider: &dyn Provider,
    task: &str,
    transcript: &str,
    remaining_steps: usize,
) -> Result<(Reflection, ChatResponse)> {
    let system = "You are the reflector of an AI agent run. Judge strictly from the evidence \
                  whether the original task has been fully accomplished. Do not assume \
                  unverified success. Respond with ONLY a JSON object: \
                  {\"status\":\"complete\"|\"continue\"|\"replan\",\"reason\":\"...\",\
                  \"final_answer\":\"...\"}. \
                  Use \"complete\" with a user-facing final_answer when done; \"continue\" to \
                  proceed with the remaining plan; \"replan\" with reason when the plan cannot \
                  accomplish the task.";
    let user = format!(
        "Original task:\n{task}\n\nExecution transcript so far:\n{transcript}\n\n\
         Remaining planned steps: {remaining_steps}.\nRespond with the JSON verdict only."
    );

    let messages = [ChatMessage::system(system), ChatMessage::user(user)];
    let response = provider
        .chat(ChatRequest {
            messages: &messages,
            tools: None,
        })
        .await?;
    let verdict = parse_reflection(response.text.as_deref().unwrap_or(""));
    Ok((verdict, response))
}

/// Extract a JSON string array from model output; tolerate surrounding prose
/// and code fences. Falls back to a single-step plan.
fn parse_plan(text: &str, max_steps: usize, fallback_task: &str) -> Vec<String> {
    let start = text.find('[');
    let end = text.rfind(']');
    if let (Some(start), Some(end)) = (start, end) {
        if start < end {
            if let Ok(serde_json::Value::Array(items)) =
                serde_json::from_str::<serde_json::Value>(&text[start..=end])
            {
                let steps: Vec<String> = items
                    .into_iter()
                    .filter_map(|item| item.as_str().map(str::trim).map(String::from))
                    .filter(|step| !step.is_empty())
                    .take(max_steps.max(1))
                    .collect();
                if !steps.is_empty() {
                    return steps;
                }
            }
        }
    }
    vec![fallback_task.to_string()]
}

fn parse_reflection(text: &str) -> Reflection {
    let start = text.find('{');
    let end = text.rfind('}');
    let (Some(start), Some(end)) = (start, end) else {
        return Reflection::Continue;
    };
    if start >= end {
        return Reflection::Continue;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text[start..=end]) else {
        return Reflection::Continue;
    };
    let status = value
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("continue")
        .to_lowercase();
    match status.as_str() {
        "complete" => {
            let final_answer = value
                .get("final_answer")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from);
            match final_answer {
                Some(final_answer) => Reflection::Complete { final_answer },
                // Complete without an answer is unusable; keep executing.
                None => Reflection::Continue,
            }
        }
        "replan" => Reflection::Replan {
            feedback: value
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("previous plan judged insufficient")
                .to_string(),
        },
        _ => Reflection::Continue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plan_extracts_json_array() {
        let text = "Here is the plan:\n```json\n[\"step one\", \"step two\"]\n```";
        assert_eq!(parse_plan(text, 8, "task"), vec!["step one", "step two"]);
    }

    #[test]
    fn parse_plan_caps_steps_and_falls_back() {
        let text = "[\"a\", \"b\", \"c\"]";
        assert_eq!(parse_plan(text, 2, "task").len(), 2);
        assert_eq!(parse_plan("no json here", 8, "task"), vec!["task"]);
        assert_eq!(parse_plan("[]", 8, "task"), vec!["task"]);
    }

    #[test]
    fn parse_reflection_variants() {
        match parse_reflection("{\"status\":\"complete\",\"final_answer\":\"done\"}") {
            Reflection::Complete { final_answer } => assert_eq!(final_answer, "done"),
            other => panic!("unexpected: {other:?}"),
        }
        assert!(matches!(
            parse_reflection("{\"status\":\"replan\",\"reason\":\"missing data\"}"),
            Reflection::Replan { .. }
        ));
        assert!(matches!(
            parse_reflection("{\"status\":\"continue\"}"),
            Reflection::Continue
        ));
        // Complete without final_answer degrades to Continue.
        assert!(matches!(
            parse_reflection("{\"status\":\"complete\"}"),
            Reflection::Continue
        ));
        assert!(matches!(parse_reflection("garbage"), Reflection::Continue));
    }
}
