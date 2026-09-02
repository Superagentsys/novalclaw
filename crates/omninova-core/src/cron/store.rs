//! Persistent storage for automation jobs.
//!
//! The store reads and writes `cron.json` on every operation instead of caching
//! jobs in memory. The desktop client edits jobs through Tauri commands while
//! the gateway scheduler updates run state from another task, and both may hold
//! their own `CronStore`; going through the file each time keeps them from
//! clobbering each other's writes.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub name: String,
    /// `every 30m` style interval or a five field cron expression.
    pub schedule: String,
    /// Instruction handed to the agent when the job fires.
    #[serde(default)]
    pub prompt: String,
    /// Legacy shell payload, kept so jobs created before automations still run.
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub description: String,
    /// Template the job was created from, for UI grouping.
    #[serde(default)]
    pub template_id: Option<String>,
    /// Explicit route captured from the desktop model picker so scheduled
    /// execution follows the same provider/model as an interactive chat.
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// Minutes east of UTC used when evaluating cron expressions.
    #[serde(default)]
    pub tz_offset_minutes: i32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub last_run: Option<String>,
    #[serde(default)]
    pub last_status: Option<CronJobStatus>,
    #[serde(default)]
    pub next_run: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub created_at: String,
    /// When set, the scheduler runs this job as a long-horizon task wake.
    #[serde(default)]
    pub task_id: Option<String>,
}

impl CronJob {
    /// A job carries an agent instruction unless it is a legacy shell job.
    pub fn is_agent_job(&self) -> bool {
        !self.prompt.trim().is_empty() || self.command.trim().is_empty()
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CronJobStatus {
    Success,
    Failed,
    Running,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StoreFile {
    #[serde(default)]
    jobs: Vec<CronJob>,
}

#[derive(Clone)]
pub struct CronStore {
    path: PathBuf,
    write_guard: Arc<Mutex<()>>,
}

impl CronStore {
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

    pub async fn list(&self) -> Vec<CronJob> {
        self.read_all().await
    }

    pub async fn get(&self, id: &str) -> Option<CronJob> {
        self.read_all().await.into_iter().find(|job| job.id == id)
    }

    pub async fn add(&self, job: CronJob) -> Result<()> {
        let _guard = self.write_guard.lock().await;
        let mut jobs = self.read_all().await;
        if jobs.iter().any(|existing| existing.id == job.id) {
            anyhow::bail!("job with id '{}' already exists", job.id);
        }
        jobs.push(job);
        self.write_all(&jobs).await
    }

    /// Inserts the job, replacing any existing entry with the same id.
    pub async fn upsert(&self, job: CronJob) -> Result<CronJob> {
        let _guard = self.write_guard.lock().await;
        let mut jobs = self.read_all().await;
        match jobs.iter_mut().find(|existing| existing.id == job.id) {
            Some(existing) => *existing = job.clone(),
            None => jobs.push(job.clone()),
        }
        self.write_all(&jobs).await?;
        Ok(job)
    }

    pub async fn remove(&self, id: &str) -> Result<bool> {
        let _guard = self.write_guard.lock().await;
        let mut jobs = self.read_all().await;
        let before = jobs.len();
        jobs.retain(|job| job.id != id);
        if jobs.len() == before {
            return Ok(false);
        }
        self.write_all(&jobs).await?;
        Ok(true)
    }

    pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<bool> {
        let _guard = self.write_guard.lock().await;
        let mut jobs = self.read_all().await;
        let Some(job) = jobs.iter_mut().find(|job| job.id == id) else {
            return Ok(false);
        };
        job.enabled = enabled;
        // Re-enabling recomputes the next fire time on the following tick.
        if !enabled {
            job.next_run = None;
        }
        self.write_all(&jobs).await?;
        Ok(true)
    }

    pub async fn set_route(
        &self,
        id: &str,
        provider: Option<String>,
        model: Option<String>,
    ) -> Result<bool> {
        let _guard = self.write_guard.lock().await;
        let mut jobs = self.read_all().await;
        let Some(job) = jobs.iter_mut().find(|job| job.id == id) else {
            return Ok(false);
        };
        job.provider = provider.filter(|value| !value.trim().is_empty());
        job.model = model.filter(|value| !value.trim().is_empty());
        self.write_all(&jobs).await?;
        Ok(true)
    }

    pub async fn update_status(
        &self,
        id: &str,
        status: CronJobStatus,
        last_error: Option<String>,
    ) -> Result<()> {
        let _guard = self.write_guard.lock().await;
        let mut jobs = self.read_all().await;
        if let Some(job) = jobs.iter_mut().find(|job| job.id == id) {
            job.last_run = Some(now_timestamp());
            job.last_status = Some(status);
            job.last_error = last_error;
        }
        self.write_all(&jobs).await
    }

    pub async fn set_next_run(&self, id: &str, next_run: Option<String>) -> Result<()> {
        let _guard = self.write_guard.lock().await;
        let mut jobs = self.read_all().await;
        if let Some(job) = jobs.iter_mut().find(|job| job.id == id) {
            job.next_run = next_run;
        }
        self.write_all(&jobs).await
    }

    async fn read_all(&self) -> Vec<CronJob> {
        let Ok(raw) = tokio::fs::read_to_string(&self.path).await else {
            return Vec::new();
        };
        serde_json::from_str::<StoreFile>(&raw)
            .map(|file| file.jobs)
            .unwrap_or_default()
    }

    async fn write_all(&self, jobs: &[CronJob]) -> Result<()> {
        let payload = serde_json::to_string_pretty(&StoreFile {
            jobs: jobs.to_vec(),
        })?;
        tokio::fs::write(&self.path, payload).await?;
        Ok(())
    }
}

pub fn now_timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}

pub fn format_timestamp(value: time::OffsetDateTime) -> String {
    value
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}

pub fn parse_timestamp(value: &str) -> Option<time::OffsetDateTime> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339).ok()
}
