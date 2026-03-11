use crate::cron::store::{CronJobStatus, CronStore};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tracing::{info, warn};

pub struct CronScheduler {
    store: CronStore,
    poll_interval: Duration,
}

impl CronScheduler {
    pub fn new(store: CronStore, poll_interval_secs: u64) -> Self {
        Self {
            store,
            poll_interval: Duration::from_secs(poll_interval_secs.max(5)),
        }
    }

    pub async fn run(&self) {
        info!("cron scheduler started (poll interval: {:?})", self.poll_interval);
        loop {
            self.tick().await;
            tokio::time::sleep(self.poll_interval).await;
        }
    }

    async fn tick(&self) {
        let jobs = self.store.list();
        let now = time::OffsetDateTime::now_utc();

        for job in &jobs {
            if !job.enabled {
                continue;
            }
            let Some(next_run_str) = &job.next_run else {
                if let Some(schedule) = parse_interval_secs(&job.schedule) {
                    let next = now + time::Duration::seconds(schedule as i64);
                    let next_str = format_timestamp(next);
                    let _ = self.store.update_status(&job.id, CronJobStatus::Skipped, Some(next_str)).await;
                }
                continue;
            };

            let next_run = match time::OffsetDateTime::parse(
                next_run_str,
                &time::format_description::well_known::Rfc3339,
            ) {
                Ok(t) => t,
                Err(_) => continue,
            };

            if now < next_run {
                continue;
            }

            info!("cron: executing job '{}' ({})", job.name, job.id);
            let _ = self.store.update_status(&job.id, CronJobStatus::Running, None).await;

            let status = match execute_shell_job(&job.command).await {
                Ok(output) => {
                    info!("cron: job '{}' completed: {}", job.id, output.chars().take(200).collect::<String>());
                    CronJobStatus::Success
                }
                Err(e) => {
                    warn!("cron: job '{}' failed: {}", job.id, e);
                    CronJobStatus::Failed
                }
            };

            let next = if let Some(interval) = parse_interval_secs(&job.schedule) {
                Some(format_timestamp(now + time::Duration::seconds(interval as i64)))
            } else {
                None
            };

            let _ = self.store.update_status(&job.id, status, next).await;
        }
    }
}

fn parse_interval_secs(schedule: &str) -> Option<u64> {
    let s = schedule.trim().to_lowercase();
    if let Some(rest) = s.strip_prefix("every ") {
        let rest = rest.trim();
        if let Some(num) = rest.strip_suffix('s').or(rest.strip_suffix(" seconds")).or(rest.strip_suffix(" second")) {
            return num.trim().parse().ok();
        }
        if let Some(num) = rest.strip_suffix('m').or(rest.strip_suffix(" minutes")).or(rest.strip_suffix(" minute")) {
            return num.trim().parse::<u64>().ok().map(|n| n * 60);
        }
        if let Some(num) = rest.strip_suffix('h').or(rest.strip_suffix(" hours")).or(rest.strip_suffix(" hour")) {
            return num.trim().parse::<u64>().ok().map(|n| n * 3600);
        }
    }
    if let Ok(secs) = s.parse::<u64>() {
        return Some(secs);
    }
    None
}

fn format_timestamp(dt: time::OffsetDateTime) -> String {
    dt.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}

async fn execute_shell_job(command: &str) -> anyhow::Result<String> {
    let output = tokio::time::timeout(
        Duration::from_secs(300),
        Command::new("sh")
            .arg("-lc")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("job timed out after 300s"))?
    .map_err(|e| anyhow::anyhow!("failed to execute: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() {
        Ok(stdout.to_string())
    } else {
        anyhow::bail!("exit {}: {stderr}", output.status.code().unwrap_or(-1))
    }
}
