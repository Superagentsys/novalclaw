use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Running,
    Sleeping,
    WaitingApproval,
    Blocked,
    Done,
    Failed,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Sleeping => "sleeping",
            Self::WaitingApproval => "waiting_approval",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "running" => Self::Running,
            "waiting_approval" => Self::WaitingApproval,
            "blocked" => Self::Blocked,
            "done" => Self::Done,
            "failed" => Self::Failed,
            _ => Self::Sleeping,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskCheckpoint {
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub done: Vec<String>,
    #[serde(default)]
    pub next: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub blocker: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub goal: String,
    pub session_id: Option<String>,
    pub status: TaskStatus,
    pub checkpoint: TaskCheckpoint,
    pub wake_schedule: String,
    pub max_rounds: u32,
    pub deadline_at: Option<i64>,
    pub max_total_tokens: Option<u64>,
    pub rounds_used: u32,
    pub tokens_used: u64,
    pub lease_until: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub enum WakeDecision {
    Run { prompt: String },
    Skip { reason: String },
    Stop { reason: String },
}
