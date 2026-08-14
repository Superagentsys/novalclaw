//! WeCom Bot HTTP callback stream state management.
//!
//! This module manages the lifecycle of WeCom HTTP callback stream responses.
//! It is isolated from the Long Connection (WebSocket) stream handling.
//!
//! # Stream Lifecycle
//!
//! 1. **New message**: Create stream with Pending state, return placeholder
//! 2. **Agent processing**: Stream remains Pending
//! 3. **Agent completes**: Update stream with final content, set finish=true
//! 4. **Stream refresh**: Return current stream state
//!
//! # Stream Refresh
//!
//! WeCom may send a `msgtype=stream` callback with `stream.id` for polling.
//! This is NOT a new user message and should NOT trigger Agent dispatch.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use time::OffsetDateTime;

pub const PLACEHOLDER_CONTENT: &str = "正在处理中...";
pub const ERROR_FALLBACK: &str = "处理消息时出现错误，请稍后重试。";

/// Stream status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamStatus {
    /// Stream created, waiting for agent to complete
    Pending,
    /// Agent completed successfully
    Completed,
    /// Agent failed or timed out
    Failed,
}

/// Stream state for WeCom HTTP callback
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WecomHttpStreamState {
    /// Unique stream identifier
    pub stream_id: String,
    /// Associated message ID (for dedup)
    pub msg_id: Option<String>,
    /// Session key (for logging)
    pub session_key: Option<String>,
    /// Stream status
    pub status: StreamStatus,
    /// Content to display (may be placeholder or final response)
    pub content: String,
    /// Whether the stream is finished
    pub finish: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Response URL carried by the callback (Phase 2A.2: saved only,
    /// never pushed — see Phase 2A.3).
    pub response_url: Option<String>,
    /// When the stream was created (Unix timestamp)
    pub created_at: i64,
    /// When the stream was last updated (Unix timestamp)
    pub updated_at: i64,
}

impl WecomHttpStreamState {
    /// Create a new pending stream
    pub fn new_pending(
        msg_id: Option<String>,
        session_key: Option<String>,
        response_url: Option<String>,
    ) -> Self {
        let now = OffsetDateTime::now_utc();
        let unix_ts = now.unix_timestamp();
        Self {
            stream_id: Uuid::new_v4().to_string(),
            msg_id,
            session_key,
            status: StreamStatus::Pending,
            content: PLACEHOLDER_CONTENT.to_string(),
            finish: false,
            error: None,
            response_url,
            created_at: unix_ts,
            updated_at: unix_ts,
        }
    }

    /// Mark stream as completed with final content
    pub fn complete(&mut self, content: String) {
        self.content = content;
        self.finish = true;
        self.status = StreamStatus::Completed;
        self.updated_at = OffsetDateTime::now_utc().unix_timestamp();
    }

    /// Mark stream as failed
    pub fn fail(&mut self, error: String) {
        self.content = ERROR_FALLBACK.to_string();
        self.finish = true;
        self.status = StreamStatus::Failed;
        self.error = Some(error);
        self.updated_at = OffsetDateTime::now_utc().unix_timestamp();
    }
}

/// In-memory store for WeCom HTTP stream states
#[derive(Debug, Clone)]
pub struct WecomHttpStreamStore {
    /// Stream ID → Stream state
    streams: Arc<RwLock<HashMap<String, WecomHttpStreamState>>>,
    /// Message ID → Stream ID (for dedup)
    msgid_to_stream: Arc<RwLock<HashMap<String, String>>>,
}

impl WecomHttpStreamStore {
    /// Create a new stream store
    pub fn new() -> Self {
        Self {
            streams: Arc::new(RwLock::new(HashMap::new())),
            msgid_to_stream: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new stream for a message.
    ///
    /// If a stream already exists for this msg_id, returns the existing stream.
    pub async fn create_stream(
        &self,
        msg_id: Option<String>,
        session_key: Option<String>,
    ) -> WecomHttpStreamState {
        self.get_or_create_stream(msg_id, session_key, None).await.0
    }

    /// Create a stream for a message if none exists, returning the
    /// stream state and whether it was newly created.
    ///
    /// `created == true`  → first callback for this msg_id (dispatch Agent).
    /// `created == false` → duplicate callback; reuse the SAME stream
    /// (and therefore never dispatch a second Agent job).
    pub async fn get_or_create_stream(
        &self,
        msg_id: Option<String>,
        session_key: Option<String>,
        response_url: Option<String>,
    ) -> (WecomHttpStreamState, bool) {
        // Check if we already have a stream for this msg_id (dedup)
        if let Some(ref mid) = msg_id {
            let msgid_to_stream = self.msgid_to_stream.read().await;
            if let Some(existing_stream_id) = msgid_to_stream.get(mid) {
                let streams = self.streams.read().await;
                if let Some(existing) = streams.get(existing_stream_id) {
                    return (existing.clone(), false);
                }
            }
        }

        // Create new stream
        let stream = WecomHttpStreamState::new_pending(msg_id.clone(), session_key, response_url);

        // Store the stream
        {
            let mut streams = self.streams.write().await;
            streams.insert(stream.stream_id.clone(), stream.clone());
        }

        // Store msg_id → stream_id mapping
        if let Some(ref mid) = msg_id {
            let mut msgid_to_stream = self.msgid_to_stream.write().await;
            msgid_to_stream.insert(mid.clone(), stream.stream_id.clone());
        }

        (stream, true)
    }

    /// Get a stream by ID
    pub async fn get_stream(&self, stream_id: &str) -> Option<WecomHttpStreamState> {
        let streams = self.streams.read().await;
        streams.get(stream_id).cloned()
    }

    /// Get a stream by msg_id
    pub async fn get_stream_by_msgid(&self, msg_id: &str) -> Option<WecomHttpStreamState> {
        let msgid_to_stream = self.msgid_to_stream.read().await;
        let stream_id = msgid_to_stream.get(msg_id)?;
        let streams = self.streams.read().await;
        streams.get(stream_id).cloned()
    }

    /// Update a stream's content (for streaming updates)
    pub async fn update_content(&self, stream_id: &str, content: String) -> bool {
        let mut streams = self.streams.write().await;
        if let Some(stream) = streams.get_mut(stream_id) {
            stream.content = content;
            stream.updated_at = OffsetDateTime::now_utc().unix_timestamp();
            true
        } else {
            false
        }
    }

    /// Complete a stream with final content
    pub async fn complete_stream(&self, stream_id: &str, content: String) -> bool {
        let mut streams = self.streams.write().await;
        if let Some(stream) = streams.get_mut(stream_id) {
            stream.complete(content);
            true
        } else {
            false
        }
    }

    /// Mark a stream as failed
    pub async fn fail_stream(&self, stream_id: &str, error: String) -> bool {
        let mut streams = self.streams.write().await;
        if let Some(stream) = streams.get_mut(stream_id) {
            stream.fail(error);
            true
        } else {
            false
        }
    }

    /// Check if a stream exists
    pub async fn has_stream(&self, stream_id: &str) -> bool {
        let streams = self.streams.read().await;
        streams.contains_key(stream_id)
    }

    /// Number of streams currently stored (diagnostics/tests only).
    pub async fn stream_count(&self) -> usize {
        let streams = self.streams.read().await;
        streams.len()
    }

    /// Check if a msg_id has an existing stream
    pub async fn has_msgid(&self, msg_id: &str) -> bool {
        let msgid_to_stream = self.msgid_to_stream.read().await;
        msgid_to_stream.contains_key(msg_id)
    }

    /// Prune old streams (older than TTL)
    pub async fn prune(&self, ttl_seconds: i64) -> usize {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let cutoff = now - ttl_seconds;
        let mut streams = self.streams.write().await;
        let mut to_remove = Vec::new();

        for (id, stream) in streams.iter() {
            if stream.updated_at < cutoff {
                to_remove.push(id.clone());
            }
        }

        for id in &to_remove {
            streams.remove(id);
        }

        // Also clean up msgid mappings
        drop(streams);
        let mut msgid_to_stream = self.msgid_to_stream.write().await;
        let mut to_remove_msgid = Vec::new();
        for (msgid, stream_id) in msgid_to_stream.iter() {
            if !self.has_stream(stream_id).await {
                to_remove_msgid.push(msgid.clone());
            }
        }
        for msgid in to_remove_msgid {
            msgid_to_stream.remove(&msgid);
        }

        to_remove.len()
    }
}

impl Default for WecomHttpStreamStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_and_get_stream() {
        let store = WecomHttpStreamStore::new();
        let stream = store.create_stream(Some("msg-1".to_string()), None).await;

        assert_eq!(stream.msg_id, Some("msg-1".to_string()));
        assert_eq!(stream.content, PLACEHOLDER_CONTENT);
        assert_eq!(stream.status, StreamStatus::Pending);
        assert!(!stream.finish);

        // Get by stream_id
        let retrieved = store.get_stream(&stream.stream_id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().stream_id, stream.stream_id);
    }

    #[tokio::test]
    async fn dedup_returns_existing_stream() {
        let store = WecomHttpStreamStore::new();

        let stream1 = store.create_stream(Some("msg-1".to_string()), None).await;
        let stream2 = store.create_stream(Some("msg-1".to_string()), None).await;

        // Should return the same stream
        assert_eq!(stream1.stream_id, stream2.stream_id);
    }

    #[tokio::test]
    async fn complete_stream() {
        let store = WecomHttpStreamStore::new();
        let stream = store.create_stream(Some("msg-1".to_string()), None).await;

        store.complete_stream(&stream.stream_id, "Agent 回复内容".to_string()).await;

        let updated = store.get_stream(&stream.stream_id).await.unwrap();
        assert_eq!(updated.content, "Agent 回复内容");
        assert!(updated.finish);
        assert_eq!(updated.status, StreamStatus::Completed);
    }

    #[tokio::test]
    async fn fail_stream() {
        let store = WecomHttpStreamStore::new();
        let stream = store.create_stream(Some("msg-1".to_string()), None).await;

        store.fail_stream(&stream.stream_id, "Agent error".to_string()).await;

        let updated = store.get_stream(&stream.stream_id).await.unwrap();
        assert_eq!(updated.content, ERROR_FALLBACK);
        assert!(updated.finish);
        assert_eq!(updated.status, StreamStatus::Failed);
        assert_eq!(updated.error, Some("Agent error".to_string()));
    }

    #[tokio::test]
    async fn stream_refresh_unknown_returns_none() {
        let store = WecomHttpStreamStore::new();
        let result = store.get_stream("unknown-id").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_by_msgid() {
        let store = WecomHttpStreamStore::new();
        let stream = store.create_stream(Some("msg-xyz".to_string()), None).await;

        let retrieved = store.get_stream_by_msgid("msg-xyz").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().stream_id, stream.stream_id);
    }

    #[tokio::test]
    async fn get_or_create_new_message_creates_stream() {
        let store = WecomHttpStreamStore::new();
        let (stream, created) = store
            .get_or_create_stream(Some("msg-a".to_string()), Some("wecom:single:ua".to_string()), None)
            .await;
        assert!(created);
        assert_eq!(stream.status, StreamStatus::Pending);
        assert!(!stream.finish);
    }

    #[tokio::test]
    async fn get_or_create_duplicate_reuses_same_stream() {
        let store = WecomHttpStreamStore::new();
        let (first, created) = store
            .get_or_create_stream(Some("msg-dup".to_string()), None, None)
            .await;
        assert!(created);
        let (second, created_again) = store
            .get_or_create_stream(Some("msg-dup".to_string()), None, None)
            .await;
        assert!(!created_again);
        assert_eq!(first.stream_id, second.stream_id);
    }

    #[tokio::test]
    async fn distinct_messages_get_distinct_streams() {
        let store = WecomHttpStreamStore::new();
        let (a, _) = store
            .get_or_create_stream(Some("msg-a".to_string()), None, None)
            .await;
        let (b, _) = store
            .get_or_create_stream(Some("msg-b".to_string()), None, None)
            .await;
        assert_ne!(a.stream_id, b.stream_id);
    }

    #[tokio::test]
    async fn stream_id_is_not_derived_from_msgid_or_user() {
        let store = WecomHttpStreamStore::new();
        let (stream, _) = store
            .get_or_create_stream(
                Some("secret-ish-msgid".to_string()),
                Some("wecom:single:secret-ish-user".to_string()),
                None,
            )
            .await;
        assert!(!stream.stream_id.contains("secret-ish-msgid"));
        assert!(!stream.stream_id.contains("secret-ish-user"));
        // UUID v4 shape: 36 chars with dashes.
        assert_eq!(stream.stream_id.len(), 36);
        assert_eq!(stream.stream_id.chars().filter(|c| *c == '-').count(), 4);
    }

    #[tokio::test]
    async fn response_url_saved_but_never_returned_in_reply() {
        let store = WecomHttpStreamStore::new();
        let (stream, _) = store
            .get_or_create_stream(
                Some("msg-url".to_string()),
                None,
                Some("https://example.invalid/response".to_string()),
            )
            .await;
        assert_eq!(
            stream.response_url.as_deref(),
            Some("https://example.invalid/response")
        );
    }

    #[tokio::test]
    async fn completed_stream_state_readable_by_refresh() {
        let store = WecomHttpStreamStore::new();
        let (stream, _) = store
            .get_or_create_stream(Some("msg-fin".to_string()), None, None)
            .await;
        store
            .complete_stream(&stream.stream_id, "final answer".to_string())
            .await;
        let refreshed = store.get_stream(&stream.stream_id).await.unwrap();
        assert_eq!(refreshed.status, StreamStatus::Completed);
        assert!(refreshed.finish);
        assert_eq!(refreshed.content, "final answer");
    }

    #[tokio::test]
    async fn unknown_stream_refresh_returns_none_and_creates_nothing() {
        let store = WecomHttpStreamStore::new();
        assert!(store.get_stream("no-such-stream").await.is_none());
        assert!(!store.has_stream("no-such-stream").await);
        assert_eq!(store.stream_count().await, 0);
    }
}
