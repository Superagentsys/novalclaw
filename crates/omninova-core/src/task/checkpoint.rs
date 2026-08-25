use super::store::TaskStore;
use super::types::Task;
use crate::agent::history::{CHECKPOINT_MARKER, TASK_MARKER};
use crate::providers::ChatMessage;
use anyhow::Result;
use std::path::Path;

pub fn merge_pinned_messages(
    store: &TaskStore,
    session_id: &str,
    mut messages: Vec<ChatMessage>,
) -> Vec<ChatMessage> {
    let Ok(Some(task)) = store.get_by_session(session_id) else {
        return messages;
    };
    messages.retain(|message| {
        !(message.role == "system"
            && (message.content.starts_with(TASK_MARKER)
                || message.content.starts_with(CHECKPOINT_MARKER)))
    });
    let pins = pinned_messages(&task);
    let insert_at = messages
        .iter()
        .take_while(|message| message.role == "system")
        .count();
    for (offset, pin) in pins.into_iter().enumerate() {
        messages.insert(insert_at + offset, pin);
    }
    messages
}

pub fn pinned_messages(task: &Task) -> Vec<ChatMessage> {
    let mut pins = vec![ChatMessage::system(format!(
        "{} {}",
        TASK_MARKER, task.goal
    ))];
    let summary = if task.checkpoint.summary.is_empty() {
        format!("status={}", task.status.as_str())
    } else {
        task.checkpoint.summary.clone()
    };
    pins.push(ChatMessage::system(format!(
        "{} {}",
        CHECKPOINT_MARKER, summary
    )));
    pins
}

pub async fn write_workspace_files(workspace: &Path, task: &Task) -> Result<()> {
    let task_md = format!(
        "# Task\n\n- id: {}\n- status: {}\n- goal: {}\n",
        task.id,
        task.status.as_str(),
        task.goal
    );
    let progress_md = format!(
        "# Progress\n\n## Summary\n{}\n\n## Done\n{}\n\n## Next\n{}\n\n## Evidence\n{}\n\n## Blocker\n{}\n",
        task.checkpoint.summary,
        bullets(&task.checkpoint.done),
        bullets(&task.checkpoint.next),
        bullets(&task.checkpoint.evidence),
        task.checkpoint.blocker,
    );
    tokio::fs::write(workspace.join("TASK.md"), task_md).await?;
    tokio::fs::write(workspace.join("PROGRESS.md"), progress_md).await?;
    Ok(())
}

fn bullets(items: &[String]) -> String {
    if items.is_empty() {
        "- (none)\n".to_string()
    } else {
        items
            .iter()
            .map(|item| format!("- {item}\n"))
            .collect()
    }
}
