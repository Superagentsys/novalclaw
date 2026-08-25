use crate::cron::{now_timestamp, CronJob, CronStore};
use crate::task::{now_unix_ts, write_workspace_files, Task, TaskCheckpoint, TaskStatus, TaskStore};
use crate::tools::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;

pub struct TaskCheckpointTool {
    workspace: PathBuf,
    session_id: Option<String>,
}

impl TaskCheckpointTool {
    pub fn new(workspace: PathBuf, session_id: Option<String>) -> Self {
        Self {
            workspace,
            session_id,
        }
    }
}

#[async_trait]
impl Tool for TaskCheckpointTool {
    fn name(&self) -> &str {
        "task_checkpoint"
    }

    fn description(&self) -> &str {
        "Write durable progress for a long-running task (hours to days). Call this before ending a turn. status=continue schedules the next wake; complete stops the task; blocked waits for a human."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "goal": { "type": "string", "description": "Immutable task goal. Required when creating a new task." },
                "status": {
                    "type": "string",
                    "enum": ["continue", "complete", "blocked"],
                    "description": "continue = sleep until next wake; complete = done; blocked = needs a human"
                },
                "summary": { "type": "string", "description": "What was accomplished in this round" },
                "done": { "type": "array", "items": { "type": "string" } },
                "next": { "type": "array", "items": { "type": "string" } },
                "evidence": { "type": "array", "items": { "type": "string" }, "description": "File paths or URLs" },
                "blocker": { "type": "string" },
                "task_id": { "type": "string" }
            },
            "required": ["status", "summary"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let status_raw = args
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("continue");
        let summary = args
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let strings = |key: &str| -> Vec<String> {
            args.get(key)
                .and_then(|v| v.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };

        let store = match TaskStore::open(self.workspace.join(".omninova-tasks.db")) {
            Ok(store) => store,
            Err(error) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("failed to open task store: {error}")),
                });
            }
        };

        let task_id = args
            .get("task_id")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| self.session_id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let mut task = store
            .get(&task_id)
            .ok()
            .flatten()
            .or_else(|| {
                self.session_id
                    .as_deref()
                    .and_then(|session| store.get_by_session(session).ok().flatten())
            })
            .unwrap_or_else(|| Task {
                id: task_id.clone(),
                goal: args
                    .get("goal")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                session_id: self.session_id.clone(),
                status: TaskStatus::Sleeping,
                checkpoint: TaskCheckpoint::default(),
                wake_schedule: "every 30m".into(),
                max_rounds: 256,
                deadline_at: None,
                max_total_tokens: None,
                rounds_used: 0,
                tokens_used: 0,
                lease_until: None,
                updated_at: now_unix_ts(),
            });

        if task.goal.trim().is_empty() {
            task.goal = args
                .get("goal")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
        }
        if task.goal.trim().is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("goal is required when creating a task".to_string()),
            });
        }

        task.checkpoint = TaskCheckpoint {
            summary,
            done: strings("done"),
            next: strings("next"),
            evidence: strings("evidence"),
            blocker: args
                .get("blocker")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        };
        if self.session_id.is_some() {
            task.session_id = self.session_id.clone();
        }
        task.lease_until = None;
        task.updated_at = now_unix_ts();
        task.status = match status_raw {
            "complete" => TaskStatus::Done,
            "blocked" => TaskStatus::Blocked,
            _ => TaskStatus::Sleeping,
        };

        if let Err(error) = store.upsert(&task) {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("failed to persist task: {error}")),
            });
        }
        let _ = write_workspace_files(&self.workspace, &task).await;
        if let Err(error) = ensure_task_cron(&self.workspace, &task).await {
            return Ok(ToolResult {
                success: true,
                output: json!({
                    "task_id": task.id,
                    "status": task.status.as_str(),
                    "cron_warning": error.to_string(),
                })
                .to_string(),
                error: None,
            });
        }

        Ok(ToolResult {
            success: true,
            output: json!({
                "task_id": task.id,
                "status": task.status.as_str(),
            })
            .to_string(),
            error: None,
        })
    }
}

async fn ensure_task_cron(workspace: &PathBuf, task: &Task) -> anyhow::Result<()> {
    let store = CronStore::open(workspace.join("cron.json")).await?;
    let job_id = format!("task-{}", task.id);
    let enabled = matches!(task.status, TaskStatus::Sleeping | TaskStatus::Running);
    if let Some(mut existing) = store.get(&job_id).await {
        existing.enabled = enabled;
        existing.task_id = Some(task.id.clone());
        existing.prompt = format!(
            "推进长期任务 {}。读 TASK.md / PROGRESS.md 与检查点，做一小段，结束前调用 task_checkpoint。",
            task.id
        );
        store.upsert(existing).await?;
        if !enabled {
            let _ = store.set_enabled(&job_id, false).await;
        }
        return Ok(());
    }
    if !enabled {
        return Ok(());
    }
    store
        .add(CronJob {
            id: job_id,
            name: format!("task:{}", task.goal.chars().take(40).collect::<String>()),
            schedule: task.wake_schedule.clone(),
            prompt: format!(
                "推进长期任务 {}。读 TASK.md / PROGRESS.md 与检查点，做一小段，结束前调用 task_checkpoint。",
                task.id
            ),
            command: String::new(),
            description: task.goal.clone(),
            template_id: Some("long-horizon-task".into()),
            tz_offset_minutes: 480,
            enabled: true,
            last_run: None,
            last_status: None,
            next_run: None,
            last_error: None,
            created_at: now_timestamp(),
            task_id: Some(task.id.clone()),
        })
        .await?;
    Ok(())
}
