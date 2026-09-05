use super::store::now_unix_ts;
use super::types::{Task, TaskStatus, WakeDecision};

const LEASE_SECS: i64 = 10 * 60;

pub fn prepare_wake(task: &mut Task) -> WakeDecision {
    match task.status {
        TaskStatus::Done | TaskStatus::Failed => {
            return WakeDecision::Stop {
                reason: format!("task already {}", task.status.as_str()),
            };
        }
        TaskStatus::Blocked | TaskStatus::WaitingApproval => {
            return WakeDecision::Skip {
                reason: format!("task is {}", task.status.as_str()),
            };
        }
        TaskStatus::Running | TaskStatus::Sleeping => {}
    }

    let now = now_unix_ts();
    if let Some(deadline) = task.deadline_at {
        if now > deadline {
            task.status = TaskStatus::Failed;
            task.updated_at = now;
            return WakeDecision::Stop {
                reason: "deadline passed".to_string(),
            };
        }
    }
    if task.rounds_used >= task.max_rounds {
        task.status = TaskStatus::Failed;
        task.updated_at = now;
        return WakeDecision::Stop {
            reason: format!("round budget exhausted ({})", task.max_rounds),
        };
    }
    if task.lease_until.unwrap_or(0) > now {
        return WakeDecision::Skip {
            reason: "previous round still holds the lease".to_string(),
        };
    }

    task.status = TaskStatus::Running;
    task.rounds_used = task.rounds_used.saturating_add(1);
    task.lease_until = Some(now + LEASE_SECS);
    task.updated_at = now;

    let next = if task.checkpoint.next.is_empty() {
        "推进目标一小步，结束前调用 task_checkpoint。".to_string()
    } else {
        task.checkpoint.next.join("；")
    };
    let prompt = format!(
        "{}\n{}\n\n目标：{}\n上一检查点：{}\n已完成：{}\n下一步：{}\n证据：{}\n阻塞：{}\n\n本回合最多做一小段工作（大约十几步工具）。结束前必须调用 task_checkpoint，status 为 continue、complete 或 blocked。不要声称全部完成除非目标已达成。若这是桌面操作任务：先 computer_use screenshot，对照上一张证据图，网页用 browser。",
        crate::agent::history::TASK_MARKER,
        crate::agent::history::CHECKPOINT_MARKER,
        task.goal,
        task.checkpoint.summary,
        task.checkpoint.done.join("；"),
        next,
        task.checkpoint.evidence.join("；"),
        task.checkpoint.blocker,
    );
    WakeDecision::Run { prompt }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::TaskCheckpoint;

    fn sample() -> Task {
        Task {
            id: "t1".into(),
            goal: "跟标书".into(),
            session_id: Some("s1".into()),
            status: TaskStatus::Sleeping,
            checkpoint: TaskCheckpoint {
                summary: "已列提纲".into(),
                done: vec!["提纲".into()],
                next: vec!["写第一章".into()],
                evidence: vec![],
                blocker: String::new(),
            },
            wake_schedule: "every 30m".into(),
            max_rounds: 8,
            deadline_at: None,
            max_total_tokens: None,
            rounds_used: 0,
            tokens_used: 0,
            lease_until: None,
            updated_at: 0,
        }
    }

    #[test]
    fn sleeping_task_starts_a_round() {
        let mut task = sample();
        let decision = prepare_wake(&mut task);
        assert!(matches!(decision, WakeDecision::Run { .. }));
        assert_eq!(task.status, TaskStatus::Running);
        assert_eq!(task.rounds_used, 1);
    }

    #[test]
    fn done_task_stops() {
        let mut task = sample();
        task.status = TaskStatus::Done;
        assert!(matches!(prepare_wake(&mut task), WakeDecision::Stop { .. }));
    }

    #[test]
    fn active_lease_skips() {
        let mut task = sample();
        task.lease_until = Some(now_unix_ts() + 60);
        assert!(matches!(prepare_wake(&mut task), WakeDecision::Skip { .. }));
    }
}
