//! Polling scheduler that fires automation jobs and records their runs.

use crate::cron::runs::{build_run, CronRun, CronRunStore};
use crate::cron::schedule::{offset_from_minutes, Schedule};
use crate::cron::store::{
    format_timestamp, now_timestamp, parse_timestamp, CronJob, CronJobStatus, CronStore,
};
use anyhow::Result;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tracing::{info, warn};

const DEFAULT_POLL_SECS: u64 = 30;
const SHELL_JOB_TIMEOUT_SECS: u64 = 300;

/// Guards against two schedulers running in one process: the desktop app starts
/// one at launch and the embedded gateway would otherwise start a second,
/// firing every job twice.
static SCHEDULER_STARTED: AtomicBool = AtomicBool::new(false);

/// Runs the instruction of an automation job. Implemented by the gateway so the
/// cron module stays independent of the agent pipeline.
#[async_trait::async_trait]
pub trait CronJobExecutor: Send + Sync {
    async fn execute(&self, job: &CronJob) -> Result<String>;
}

#[derive(Clone)]
pub struct CronScheduler {
    store: CronStore,
    runs: CronRunStore,
    executor: Arc<dyn CronJobExecutor>,
    poll_interval: Duration,
}

impl CronScheduler {
    pub fn new(store: CronStore, runs: CronRunStore, executor: Arc<dyn CronJobExecutor>) -> Self {
        Self {
            store,
            runs,
            executor,
            poll_interval: Duration::from_secs(DEFAULT_POLL_SECS),
        }
    }

    pub fn with_poll_interval(mut self, seconds: u64) -> Self {
        self.poll_interval = Duration::from_secs(seconds.max(5));
        self
    }

    /// Spawns the polling loop unless one is already running in this process.
    /// Returns whether this call started it.
    pub fn spawn_once(self) -> bool {
        if SCHEDULER_STARTED.swap(true, Ordering::SeqCst) {
            return false;
        }
        tokio::spawn(async move {
            self.run().await;
        });
        true
    }

    pub async fn run(&self) {
        info!(
            "automation scheduler started (poll interval: {:?})",
            self.poll_interval
        );
        loop {
            self.tick().await;
            tokio::time::sleep(self.poll_interval).await;
        }
    }

    async fn tick(&self) {
        let jobs = self.store.list().await;
        for job in jobs {
            if !job.enabled {
                continue;
            }

            let schedule = match Schedule::parse(&job.schedule) {
                Ok(schedule) => schedule,
                Err(error) => {
                    warn!(
                        "automation '{}' has an invalid schedule '{}': {error}",
                        job.name, job.schedule
                    );
                    continue;
                }
            };
            let tz = offset_from_minutes(job.tz_offset_minutes);
            let now = time::OffsetDateTime::now_utc();

            let Some(due_at) = job.next_run.as_deref().and_then(parse_timestamp) else {
                // First sighting of the job (or it was just re-enabled): only
                // arm the next fire time so enabling never triggers instantly.
                let next = schedule.next_after(now, tz).map(format_timestamp);
                if let Err(error) = self.store.set_next_run(&job.id, next).await {
                    warn!("failed to arm automation '{}': {error}", job.name);
                }
                continue;
            };

            if now < due_at {
                continue;
            }

            self.execute(&job, "schedule").await;

            // Reschedule from now rather than from the missed slot so a laptop
            // that was asleep for a week does not replay a week of runs.
            let next = schedule
                .next_after(time::OffsetDateTime::now_utc(), tz)
                .map(format_timestamp);
            if let Err(error) = self.store.set_next_run(&job.id, next).await {
                warn!("failed to reschedule automation '{}': {error}", job.name);
            }
        }
    }

    /// Runs a job immediately without disturbing its schedule.
    pub async fn trigger_now(&self, job_id: &str) -> Result<CronRun> {
        let job = self
            .store
            .get(job_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("automation '{job_id}' not found"))?;
        Ok(self.execute(&job, "manual").await)
    }

    async fn execute(&self, job: &CronJob, trigger: &str) -> CronRun {
        info!("automation: running '{}' ({})", job.name, trigger);
        let started_at = now_timestamp();
        let started_instant = Instant::now();

        let _ = self
            .store
            .update_status(&job.id, CronJobStatus::Running, None)
            .await;

        let result = if job.is_agent_job() {
            self.executor.execute(job).await
        } else {
            run_shell_job(&job.command).await
        };

        let run = build_run(
            &job.id,
            &job.name,
            trigger,
            started_at,
            started_instant,
            result,
        );

        if let Some(error) = &run.error {
            warn!("automation '{}' failed: {error}", job.name);
        }
        let _ = self
            .store
            .update_status(&job.id, run.status, run.error.clone())
            .await;
        if let Err(error) = self.runs.record(run.clone()).await {
            warn!("failed to persist automation run: {error}");
        }
        run
    }
}

async fn run_shell_job(command: &str) -> Result<String> {
    let output = tokio::time::timeout(
        Duration::from_secs(SHELL_JOB_TIMEOUT_SECS),
        Command::new("sh")
            .arg("-lc")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("job timed out after {SHELL_JOB_TIMEOUT_SECS}s"))?
    .map_err(|error| anyhow::anyhow!("failed to execute: {error}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() {
        Ok(stdout.to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("exit {}: {stderr}", output.status.code().unwrap_or(-1))
    }
}
