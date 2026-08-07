//! DingTalk job store for tracking inbound events and responses.
//!
//! This module provides simple in-memory job tracking for DingTalk webhook events.
//! It does NOT persist to SQLite (Phase 1); job records live in memory only.

use crate::channels::InboundMessage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Job status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobStatus {
    Received,
    Processing,
    Completed,
    Failed,
}

/// A tracked DingTalk job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingtalkJob {
    pub job_id: String,
    pub inbound: InboundMessage,
    pub status: JobStatus,
    pub created_at: u64,
    pub updated_at: u64,
    pub error_message: Option<String>,
}

/// In-memory DingTalk job store
pub struct DingtalkStore {
    jobs: Arc<RwLock<HashMap<String, DingtalkJob>>>,
}

impl Default for DingtalkStore {
    fn default() -> Self {
        Self::new()
    }
}

impl DingtalkStore {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Store a new inbound job
    pub async fn store_inbound(&self, job: DingtalkJob) {
        let mut jobs = self.jobs.write().await;
        jobs.insert(job.job_id.clone(), job);
    }

    /// Update job status
    pub async fn update_status(&self, job_id: &str, status: JobStatus) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = status;
            job.updated_at = now_secs();
        }
    }

    /// Mark job as failed with error message
    pub async fn mark_failed(&self, job_id: &str, error: String) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = JobStatus::Failed;
            job.error_message = Some(error);
            job.updated_at = now_secs();
        }
    }

    /// Get recent jobs (last N, sorted by created_at desc)
    pub async fn get_recent_jobs(&self, limit: usize) -> Vec<DingtalkJob> {
        let jobs = self.jobs.read().await;
        let mut all: Vec<_> = jobs.values().cloned().collect();
        all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        all.into_iter().take(limit).collect()
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_and_retrieve_job() {
        let store = DingtalkStore::new();
        let job = DingtalkJob {
            job_id: "test-job-1".to_string(),
            inbound: InboundMessage {
                channel: crate::channels::ChannelKind::Dingtalk,
                user_id: Some("user123".to_string()),
                session_id: Some("session456".to_string()),
                text: "hello".to_string(),
                metadata: Default::default(),
            },
            status: JobStatus::Received,
            created_at: 1000,
            updated_at: 1000,
            error_message: None,
        };

        store.store_inbound(job.clone()).await;
        let recent = store.get_recent_jobs(10).await;
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].job_id, "test-job-1");
    }

    #[tokio::test]
    async fn test_update_status() {
        let store = DingtalkStore::new();
        let job = DingtalkJob {
            job_id: "test-job-2".to_string(),
            inbound: InboundMessage {
                channel: crate::channels::ChannelKind::Dingtalk,
                user_id: None,
                session_id: Some("sess".to_string()),
                text: "hi".to_string(),
                metadata: Default::default(),
            },
            status: JobStatus::Received,
            created_at: 2000,
            updated_at: 2000,
            error_message: None,
        };

        store.store_inbound(job).await;
        store.update_status("test-job-2", JobStatus::Processing).await;

        let recent = store.get_recent_jobs(10).await;
        assert_eq!(recent[0].status, JobStatus::Processing);
    }

    #[tokio::test]
    async fn test_mark_failed() {
        let store = DingtalkStore::new();
        let job = DingtalkJob {
            job_id: "test-job-3".to_string(),
            inbound: InboundMessage {
                channel: crate::channels::ChannelKind::Dingtalk,
                user_id: None,
                session_id: None,
                text: "test".to_string(),
                metadata: Default::default(),
            },
            status: JobStatus::Processing,
            created_at: 3000,
            updated_at: 3000,
            error_message: None,
        };

        store.store_inbound(job).await;
        store.mark_failed("test-job-3", "network error".to_string()).await;

        let recent = store.get_recent_jobs(10).await;
        assert_eq!(recent[0].status, JobStatus::Failed);
        assert_eq!(recent[0].error_message.as_deref(), Some("network error"));
    }

    #[tokio::test]
    async fn test_get_recent_jobs_limit() {
        let store = DingtalkStore::new();
        for i in 0..5 {
            let job = DingtalkJob {
                job_id: format!("job-{i}"),
                inbound: InboundMessage {
                    channel: crate::channels::ChannelKind::Dingtalk,
                    user_id: None,
                    session_id: None,
                    text: format!("text-{i}"),
                    metadata: Default::default(),
                },
                status: JobStatus::Completed,
                created_at: i,
                updated_at: i,
                error_message: None,
            };
            store.store_inbound(job).await;
        }

        let recent = store.get_recent_jobs(3).await;
        assert_eq!(recent.len(), 3);
        // Most recent first
        assert!(recent[0].created_at > recent[1].created_at);
    }
}
