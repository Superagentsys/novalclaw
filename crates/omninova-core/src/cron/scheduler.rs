//! Polling scheduler that fires automation jobs and records their runs.

use crate::cron::runs::{build_run, CronRun, CronRunStore};
use crate::cron::schedule::{offset_from_minutes, Schedule};
use crate::cron::store::{
    format_timestamp, now_timestamp, parse_timestamp, CronJob, CronJobStatus, CronStore,
};
use crate::providers::ProviderHttpError;
use anyhow::Result;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tracing::{info, warn};

const DEFAULT_POLL_SECS: u64 = 30;
const SHELL_JOB_TIMEOUT_SECS: u64 = 300;
/// A desktop app can be suspended or closed across a scheduled wall-clock
/// time. Do not execute a stale daily/weekly cron slot hours later when the app
/// next opens; a short grace still absorbs normal polling and wake-up jitter.
const CRON_MISFIRE_GRACE_SECS: i64 = 5 * 60;

fn is_stale_cron_slot(
    schedule: &Schedule,
    now: time::OffsetDateTime,
    due_at: time::OffsetDateTime,
) -> bool {
    matches!(schedule, Schedule::Cron(_))
        && now > due_at
        && (now - due_at).whole_seconds() > CRON_MISFIRE_GRACE_SECS
}

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

            if is_stale_cron_slot(&schedule, now, due_at) {
                // Re-arm at the next intended local wall-clock occurrence.
                // Previously an overdue slot was executed immediately at app
                // startup, which made a 17:34 job appear at 14:14/13:56/etc.
                let next = schedule.next_after(now, tz).map(format_timestamp);
                if let Err(error) = self.store.set_next_run(&job.id, next).await {
                    warn!("failed to skip stale automation '{}': {error}", job.name);
                } else {
                    info!(
                        "automation: skipped stale slot for '{}' (due {}, now {})",
                        job.name,
                        format_timestamp(due_at),
                        format_timestamp(now)
                    );
                }
                continue;
            }

            self.execute(&job, "schedule").await;

            if self
                .store
                .get(&job.id)
                .await
                .is_some_and(|job| !job.enabled)
            {
                continue;
            }

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
        let access_failure = result.as_ref().err().and_then(provider_access_failure_pause);

        let mut run = build_run(
            &job.id,
            &job.name,
            trigger,
            started_at,
            started_instant,
            result,
        );

        if let Some((status, reason)) = access_failure {
            // A credential/access denial cannot become healthy merely because
            // the same scheduled prompt is replayed one minute later. Pause
            // this job until the user fixes its provider selection or access.
            // Persist through CronStore (cron.json), not an in-memory flag.
            run.error = Some(reason);
            let _ = self.store.set_enabled(&job.id, false).await;
            warn!(
                request_origin = "automation",
                http_status = status,
                action = "job_paused",
                pause_reason = run.error.as_deref().unwrap_or(""),
                "automation paused after terminal provider access failure"
            );
        }

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

fn provider_access_failure_status(error: &anyhow::Error) -> Option<u16> {
    provider_access_failure_pause(error).map(|(status, _)| status)
}

fn provider_access_failure_pause(error: &anyhow::Error) -> Option<(u16, String)> {
    let failure = error
        .downcast_ref::<ProviderHttpError>()
        .filter(|error| error.is_access_failure())?;
    Some((failure.status, sanitized_access_failure_reason(failure)))
}

fn sanitized_access_failure_reason(error: &ProviderHttpError) -> String {
    let code = match error.status {
        401 => "provider_unauthorized",
        _ => "provider_access_denied",
    };
    match sanitize_persisted_id(&error.provider) {
        Some(provider) => format!("{code} provider={provider}"),
        None => code.to_string(),
    }
}

fn sanitize_persisted_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return None;
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return None;
    }
    Some(trimmed.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use time::macros::datetime;

    #[test]
    fn skips_old_wall_clock_slot_after_desktop_restart() {
        let schedule = Schedule::parse("34 17 * * *").unwrap();
        let due = datetime!(2026-08-26 09:34:00 UTC);
        let restarted = datetime!(2026-08-27 06:14:00 UTC);
        assert!(is_stale_cron_slot(&schedule, restarted, due));
    }

    #[test]
    fn keeps_normal_polling_jitter_and_interval_jobs() {
        let cron = Schedule::parse("34 17 * * *").unwrap();
        let due = datetime!(2026-08-27 09:34:00 UTC);
        assert!(!is_stale_cron_slot(
            &cron,
            datetime!(2026-08-27 09:34:45 UTC),
            due,
        ));

        let interval = Schedule::parse("every 30m").unwrap();
        assert!(!is_stale_cron_slot(
            &interval,
            datetime!(2026-08-27 12:00:00 UTC),
            due,
        ));
    }

    struct AccessDeniedExecutor {
        calls: AtomicUsize,
        status: u16,
    }

    #[async_trait::async_trait]
    impl CronJobExecutor for AccessDeniedExecutor {
        async fn execute(&self, _job: &CronJob) -> Result<String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(anyhow::Error::new(ProviderHttpError {
                provider: "test-provider".into(),
                status: self.status,
                code: Some("access_denied".into()),
                message: "request denied".into(),
            }))
        }
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "omninova-cron-{name}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ))
    }

    fn due_job() -> CronJob {
        CronJob {
            id: "job-provider-access".into(),
            name: "provider access".into(),
            schedule: "every 1m".into(),
            prompt: "run".into(),
            command: String::new(),
            description: String::new(),
            template_id: None,
            provider: None,
            model: None,
            tz_offset_minutes: 0,
            enabled: true,
            last_run: None,
            last_status: None,
            next_run: Some("2000-01-01T00:00:00Z".into()),
            last_error: None,
            created_at: now_timestamp(),
            task_id: None,
        }
    }

    #[tokio::test]
    async fn terminal_provider_403_pauses_periodic_agent_job() {
        let root = temp_path("provider-403");
        let store = CronStore::open(root.join("cron.json")).await.unwrap();
        let runs = CronRunStore::open(root.join("runs.json")).await.unwrap();
        store.add(due_job()).await.unwrap();
        let executor = Arc::new(AccessDeniedExecutor {
            calls: AtomicUsize::new(0),
            status: 403,
        });
        let scheduler = CronScheduler::new(store.clone(), runs, executor.clone());

        scheduler.tick().await;
        scheduler.tick().await;

        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
        let saved = store.get("job-provider-access").await.unwrap();
        assert!(!saved.enabled, "403 must pause the repeating job");
        assert_eq!(saved.last_status, Some(CronJobStatus::Failed));
        assert_eq!(
            saved.last_error.as_deref(),
            Some("provider_access_denied provider=test-provider")
        );
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn terminal_provider_403_pause_survives_store_reload_without_redispatch() {
        let root = temp_path("provider-403-reload");
        let cron_path = root.join("cron.json");
        let runs_path = root.join("runs.json");
        let store = CronStore::open(&cron_path).await.unwrap();
        let runs = CronRunStore::open(&runs_path).await.unwrap();
        store.add(due_job()).await.unwrap();
        let executor = Arc::new(AccessDeniedExecutor {
            calls: AtomicUsize::new(0),
            status: 403,
        });
        let scheduler = CronScheduler::new(store, runs, executor.clone());

        scheduler.tick().await;
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
        drop(scheduler);

        let persisted = tokio::fs::read_to_string(&cron_path).await.unwrap();
        assert!(persisted.contains("\"enabled\": false"));
        assert!(persisted.contains("provider_access_denied"));
        assert!(!persisted.contains("request denied"));
        assert!(!persisted.contains("Authorization"));
        assert!(!persisted.contains("Bearer"));

        // Simulate OmniNova restart: a new CronStore/scheduler from the same file.
        let store = CronStore::open(&cron_path).await.unwrap();
        let saved = store.get("job-provider-access").await.unwrap();
        assert!(!saved.enabled, "pause must survive process restart");
        assert_eq!(
            saved.last_error.as_deref(),
            Some("provider_access_denied provider=test-provider")
        );
        assert!(saved.next_run.is_none());

        let runs = CronRunStore::open(&runs_path).await.unwrap();
        let recorded = runs.list(Some(1)).await;
        assert_eq!(
            recorded[0].error.as_deref(),
            Some("provider_access_denied provider=test-provider")
        );

        let scheduler = CronScheduler::new(store, runs, executor.clone());
        scheduler.tick().await;
        scheduler.tick().await;
        scheduler.tick().await;
        assert_eq!(
            executor.calls.load(Ordering::SeqCst),
            1,
            "paused job must not dispatch after reload"
        );
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[test]
    fn only_401_and_403_are_terminal_provider_access_failures() {
        for status in [401, 403] {
            let error = anyhow::Error::new(ProviderHttpError {
                provider: "test-provider".into(),
                status,
                code: None,
                message: "denied".into(),
            });
            assert_eq!(provider_access_failure_status(&error), Some(status));
        }
        let unrelated = anyhow::Error::new(ProviderHttpError {
            provider: "test-provider".into(),
            status: 400,
            code: None,
            message: "bad request".into(),
        });
        assert_eq!(provider_access_failure_status(&unrelated), None);
    }

    #[test]
    fn access_failure_pause_reason_is_sanitized() {
        let unauthorized = ProviderHttpError {
            provider: "wetoken".into(),
            status: 401,
            code: Some("sk-secret".into()),
            message: "Authorization: Bearer sk-secret".into(),
        };
        assert_eq!(
            sanitized_access_failure_reason(&unauthorized),
            "provider_unauthorized provider=wetoken"
        );

        let denied = ProviderHttpError {
            provider: "Authorization: Bearer leaked".into(),
            status: 403,
            code: None,
            message: "raw provider body with prompt data".into(),
        };
        let reason = sanitized_access_failure_reason(&denied);
        assert_eq!(reason, "provider_access_denied");
        assert!(!reason.contains("Authorization"));
        assert!(!reason.contains("Bearer"));
        assert!(!reason.contains("leaked"));
        assert!(!reason.contains("raw provider body"));
    }
}
