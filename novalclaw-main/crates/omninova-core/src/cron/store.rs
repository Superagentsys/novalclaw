use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use parking_lot::RwLock;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub schedule: String,
    pub command: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub last_run: Option<String>,
    pub last_status: Option<CronJobStatus>,
    pub next_run: Option<String>,
    pub created_at: String,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    jobs: Arc<RwLock<Vec<CronJob>>>,
    path: PathBuf,
}

impl CronStore {
    pub async fn open(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let jobs = if path.exists() {
            let raw = tokio::fs::read_to_string(&path).await.unwrap_or_default();
            let store: StoreFile = serde_json::from_str(&raw).unwrap_or_default();
            store.jobs
        } else {
            Vec::new()
        };
        Ok(Self {
            jobs: Arc::new(RwLock::new(jobs)),
            path,
        })
    }

    pub fn list(&self) -> Vec<CronJob> {
        self.jobs.read().clone()
    }

    pub fn get(&self, id: &str) -> Option<CronJob> {
        self.jobs.read().iter().find(|j| j.id == id).cloned()
    }

    pub async fn add(&self, job: CronJob) -> anyhow::Result<()> {
        {
            let mut lock = self.jobs.write();
            if lock.iter().any(|j| j.id == job.id) {
                anyhow::bail!("job with id '{}' already exists", job.id);
            }
            lock.push(job);
        }
        self.flush().await
    }

    pub async fn remove(&self, id: &str) -> anyhow::Result<bool> {
        let removed = {
            let mut lock = self.jobs.write();
            let len_before = lock.len();
            lock.retain(|j| j.id != id);
            lock.len() < len_before
        };
        if removed {
            self.flush().await?;
        }
        Ok(removed)
    }

    pub async fn update_status(
        &self,
        id: &str,
        status: CronJobStatus,
        next_run: Option<String>,
    ) -> anyhow::Result<()> {
        {
            let mut lock = self.jobs.write();
            if let Some(job) = lock.iter_mut().find(|j| j.id == id) {
                job.last_run = Some(now_timestamp());
                job.last_status = Some(status);
                if next_run.is_some() {
                    job.next_run = next_run;
                }
            }
        }
        self.flush().await
    }

    pub async fn set_enabled(&self, id: &str, enabled: bool) -> anyhow::Result<bool> {
        let found = {
            let mut lock = self.jobs.write();
            if let Some(job) = lock.iter_mut().find(|j| j.id == id) {
                job.enabled = enabled;
                true
            } else {
                false
            }
        };
        if found {
            self.flush().await?;
        }
        Ok(found)
    }

    async fn flush(&self) -> anyhow::Result<()> {
        let store = StoreFile {
            jobs: self.jobs.read().clone(),
        };
        let payload = serde_json::to_string_pretty(&store)?;
        tokio::fs::write(&self.path, payload).await?;
        Ok(())
    }
}

fn now_timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}
