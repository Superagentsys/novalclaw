use super::log::{derive_messages, events_from_messages, repair_unclosed_tools};
use super::types::SessionEvent;
use crate::providers::ChatMessage;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn session_log_path(workspace_dir: &Path, session_key: &str) -> PathBuf {
    let safe: String = session_key
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    workspace_dir.join("sessions").join(format!("{safe}.jsonl"))
}

pub async fn load_messages(
    workspace_dir: &Path,
    session_key: &str,
) -> Result<Option<Vec<ChatMessage>>> {
    let path = session_log_path(workspace_dir, session_key);
    if !path.exists() {
        return Ok(None);
    }
    let raw = tokio::fs::read_to_string(&path)
        .await
        .with_context(|| format!("failed to read session log {}", path.display()))?;
    let mut events = parse_jsonl(&raw);
    if repair_unclosed_tools(&mut events) {
        let _ = write_events(&path, &events).await;
    }
    Ok(Some(derive_messages(&events)))
}

pub async fn save_messages(
    workspace_dir: &Path,
    session_key: &str,
    messages: &[ChatMessage],
) -> Result<()> {
    let path = session_log_path(workspace_dir, session_key);
    let events = events_from_messages(messages);
    write_events(&path, &events).await
}

pub async fn delete_messages(workspace_dir: &Path, session_key: &str) -> Result<bool> {
    let path = session_log_path(workspace_dir, session_key);
    match tokio::fs::remove_file(&path).await {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error)
            .with_context(|| format!("failed to delete session log {}", path.display())),
    }
}

async fn write_events(path: &Path, events: &[SessionEvent]) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut body = String::new();
    for event in events {
        body.push_str(&serde_json::to_string(event)?);
        body.push('\n');
    }
    let tmp = path.with_extension(format!("jsonl.tmp-{}", uuid::Uuid::new_v4()));
    tokio::fs::write(&tmp, body).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

fn parse_jsonl(raw: &str) -> Vec<SessionEvent> {
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<SessionEvent>(line).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn save_then_load_round_trips() {
        let dir = std::env::temp_dir().join(format!("omninova-session-{}", uuid::Uuid::new_v4()));
        let messages = vec![ChatMessage::user("ping"), ChatMessage::assistant("pong")];
        save_messages(&dir, "cli:demo", &messages)
            .await
            .expect("save");
        let loaded = load_messages(&dir, "cli:demo")
            .await
            .expect("load")
            .expect("present");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].content, "ping");
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn delete_messages_removes_jsonl() {
        let dir = std::env::temp_dir().join(format!("omninova-session-{}", uuid::Uuid::new_v4()));
        let messages = vec![ChatMessage::user("keep-me-out")];
        save_messages(&dir, "web:omninova-chat-session", &messages)
            .await
            .expect("save");
        assert!(
            delete_messages(&dir, "web:omninova-chat-session")
                .await
                .expect("delete")
        );
        let loaded = load_messages(&dir, "web:omninova-chat-session")
            .await
            .expect("load after delete");
        assert!(loaded.is_none());
        assert!(
            !delete_messages(&dir, "web:omninova-chat-session")
                .await
                .expect("delete missing")
        );
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
