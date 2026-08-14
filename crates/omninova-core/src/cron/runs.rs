//! Run history for automation jobs, persisted next to the job definitions.

use crate::cron::store::{now_timestamp, CronJobStatus};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Newest records are kept; older ones are dropped so the file stays small.
const MAX_RECORDS: usize = 200;
/// Replies can be long; only a preview is worth persisting for the run list.
const MAX_OUTPUT_CHARS: usize = 4_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronRun {
    pub id: String,
    pub job_id: String,
    pub job_name: String,
    /// `schedule` for timer fired runs, `manual` for "run now".
    pub trigger: String,
    pub status: CronJobStatus,
    pub started_at: String,
    #[serde(default)]
    pub finished_at: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RunsFile {
    #[serde(default)]
    runs: Vec<CronRun>,
}

#[derive(Clone)]
pub struct CronRunStore {
    path: PathBuf,
    write_guard: Arc<Mutex<()>>,
}

impl CronRunStore {
    pub async fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        Ok(Self {
            path,
            write_guard: Arc::new(Mutex::new(())),
        })
    }

    /// Most recent runs first.
    pub async fn list(&self, limit: Option<usize>) -> Vec<CronRun> {
        let mut runs = self.read_all().await;
        if let Some(limit) = limit {
            runs.truncate(limit);
        }
        runs
    }

    pub async fn record(&self, run: CronRun) -> Result<()> {
        let _guard = self.write_guard.lock().await;
        let mut runs = self.read_all().await;
        runs.insert(0, run);
        runs.truncate(MAX_RECORDS);
        self.write_all(&runs).await
    }

    pub async fn clear(&self) -> Result<()> {
        let _guard = self.write_guard.lock().await;
        self.write_all(&[]).await
    }

    /// Drops history for a job that no longer exists.
    pub async fn remove_for_job(&self, job_id: &str) -> Result<()> {
        let _guard = self.write_guard.lock().await;
        let mut runs = self.read_all().await;
        runs.retain(|run| run.job_id != job_id);
        self.write_all(&runs).await
    }

    async fn read_all(&self) -> Vec<CronRun> {
        let Ok(raw) = tokio::fs::read_to_string(&self.path).await else {
            return Vec::new();
        };
        serde_json::from_str::<RunsFile>(&raw)
            .map(|file| file.runs)
            .unwrap_or_default()
    }

    async fn write_all(&self, runs: &[CronRun]) -> Result<()> {
        let payload = serde_json::to_string_pretty(&RunsFile {
            runs: runs.to_vec(),
        })?;
        tokio::fs::write(&self.path, payload).await?;
        Ok(())
    }
}

/// Builds a completed run record, truncating the output preview.
pub fn build_run(
    job_id: &str,
    job_name: &str,
    trigger: &str,
    started_at: String,
    started_instant: std::time::Instant,
    result: Result<String>,
) -> CronRun {
    let duration_ms = started_instant.elapsed().as_millis() as u64;
    let (status, output, error) = match result {
        Ok(reply) => (CronJobStatus::Success, Some(truncate(&reply)), None),
        Err(error) => (CronJobStatus::Failed, None, Some(error.to_string())),
    };
    CronRun {
        id: uuid::Uuid::new_v4().to_string(),
        job_id: job_id.to_string(),
        job_name: job_name.to_string(),
        trigger: trigger.to_string(),
        status,
        started_at,
        finished_at: Some(now_timestamp()),
        duration_ms: Some(duration_ms),
        output,
        error,
    }
}

fn truncate(text: &str) -> String {
    if text.chars().count() <= MAX_OUTPUT_CHARS {
        return text.to_string();
    }
    let mut preview: String = text.chars().take(MAX_OUTPUT_CHARS).collect();
    preview.push('…');
    preview
}
