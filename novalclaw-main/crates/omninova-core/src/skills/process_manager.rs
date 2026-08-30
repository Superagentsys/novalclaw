//! External skill process lifecycle manager.
//!
//! Manages the start/stop/health-check/restart lifecycle of external processes
//! that provide MCP (or other) services to OmniNova, e.g., the TrendRadar
//! Python MCP Server.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Maximum time to wait for a process to gracefully shut down.
const GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

/// Interval between health checks when waiting for startup.
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_millis(500);

// ─── Skill Process ────────────────────────────────────────────────────────

/// Represents a managed external skill process.
pub struct SkillProcess {
    /// Unique name for this process (e.g., "trendradar-mcp").
    pub name: String,
    /// The command to execute.
    pub command: String,
    /// Arguments for the command.
    pub args: Vec<String>,
    /// Working directory for the process.
    pub working_dir: Option<PathBuf>,
    /// Environment variables to set.
    pub env_vars: Vec<(String, String)>,
    /// Optional: HTTP URL for health checks (e.g., "http://127.0.0.1:3333/mcp").
    pub health_check_url: Option<String>,
    /// Whether to automatically restart on crash.
    pub auto_restart: bool,
    /// Maximum number of restart attempts before giving up.
    pub max_restarts: u32,
    /// Delay between restart attempts.
    pub restart_delay: Duration,

    /// The managed child process handle.
    child: RwLock<Option<Child>>,
    /// Number of consecutive restart attempts.
    restart_count: RwLock<u32>,
}

impl SkillProcess {
    /// Create a new skill process definition (does NOT start it).
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            args: Vec::new(),
            working_dir: None,
            env_vars: Vec::new(),
            health_check_url: None,
            auto_restart: false,
            max_restarts: 3,
            restart_delay: Duration::from_secs(2),
            child: RwLock::new(None),
            restart_count: RwLock::new(0),
        }
    }

    /// Create a pre-configured SkillProcess for TrendRadar MCP server.
    pub fn for_trendradar(project_root: PathBuf, port: u16) -> Self {
        let mut proc = Self::new("trendradar-mcp", "python");
        proc.args = vec![
            "-m".to_string(),
            "mcp_server".to_string(),
            "--transport".to_string(),
            "http".to_string(),
            "--host".to_string(),
            "127.0.0.1".to_string(),
            "--port".to_string(),
            port.to_string(),
            "--project-root".to_string(),
            project_root.to_string_lossy().to_string(),
        ];
        proc.health_check_url = Some(format!("http://127.0.0.1:{port}/mcp"));
        proc.auto_restart = true;
        proc.max_restarts = 5;
        proc.restart_delay = Duration::from_secs(3);
        // Set UTF-8 encoding for Windows compatibility.
        proc.env_vars
            .push(("PYTHONUTF8".to_string(), "1".to_string()));
        proc
    }

    // ─── Builder Methods ───────────────────────────────────────────────

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    pub fn with_working_dir(mut self, dir: PathBuf) -> Self {
        self.working_dir = Some(dir);
        self
    }

    pub fn with_health_check_url(mut self, url: impl Into<String>) -> Self {
        self.health_check_url = Some(url.into());
        self
    }

    pub fn with_auto_restart(mut self, enabled: bool) -> Self {
        self.auto_restart = enabled;
        self
    }

    // ─── Lifecycle Methods ─────────────────────────────────────────────

    /// Start the process and optionally wait for it to become healthy.
    pub async fn start(&self) -> Result<()> {
        let mut child_lock = self.child.write().await;

        if child_lock.is_some() {
            warn!("Skill process '{}' is already running", self.name);
            return Ok(());
        }

        info!(
            "Starting skill process '{}': {} {}",
            self.name,
            self.command,
            self.args.join(" ")
        );

        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args);
        cmd.kill_on_drop(true);

        if let Some(ref dir) = self.working_dir {
            cmd.current_dir(dir);
        }

        for (key, value) in &self.env_vars {
            cmd.env(key, value);
        }

        let child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn skill process '{}'", self.name))?;

        *child_lock = Some(child);
        drop(child_lock);

        self.reset_restart_count().await;

        info!("Skill process '{}' spawned (pid unavailable on Windows via tokio)", self.name);

        Ok(())
    }

    /// Start the process and wait for health check to pass.
    pub async fn start_and_wait(&self, timeout: Duration) -> Result<()> {
        self.start().await?;

        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            if tokio::time::Instant::now() >= deadline {
                anyhow::bail!(
                    "Skill process '{}' failed to become healthy within {:?}",
                    self.name,
                    timeout
                );
            }

            if self.is_healthy().await {
                info!("Skill process '{}' is healthy", self.name);
                return Ok(());
            }

            sleep(HEALTH_CHECK_INTERVAL).await;
        }
    }

    /// Stop the process gracefully (SIGTERM) or forcefully (SIGKILL).
    pub async fn stop(&self) -> Result<()> {
        let mut child_lock = self.child.write().await;

        let Some(mut child) = child_lock.take() else {
            debug!("Skill process '{}' is not running", self.name);
            return Ok(());
        };

        info!("Stopping skill process '{}'...", self.name);

        // Try graceful shutdown first.
        if let Err(e) = child.start_kill() {
            warn!(
                "Failed to send kill signal to '{}', attempting wait: {e}",
                self.name
            );
        }

        // Give it time to exit gracefully.
        match tokio::time::timeout(GRACEFUL_SHUTDOWN_TIMEOUT, child.wait()).await {
            Ok(Ok(status)) => {
                info!(
                    "Skill process '{}' exited with: {:?}",
                    self.name, status
                );
            }
            Ok(Err(e)) => {
                error!(
                    "Error waiting for skill process '{}': {e}",
                    self.name
                );
            }
            Err(_elapsed) => {
                warn!(
                    "Skill process '{}' did not exit gracefully, killing...",
                    self.name
                );
                let _ = child.kill().await;
            }
        }

        Ok(())
    }

    /// Check if the process is running and healthy.
    pub async fn is_healthy(&self) -> bool {
        let child_lock = self.child.read().await;
        if child_lock.is_none() {
            return false;
        }
        drop(child_lock);

        // If a health check URL is configured, use it.
        if let Some(ref url) = self.health_check_url {
            return http_health_check(url).await;
        }

        // Otherwise, just check if the process exists.
        // On Unix we'd check /proc/{pid}; on Windows tokio doesn't
        // expose that info easily so we assume it's running.
        true
    }

    /// Query health status (with detail).
    pub async fn health_status(&self) -> SkillProcessHealth {
        if self.child.read().await.is_none() {
            return SkillProcessHealth {
                running: false,
                healthy: false,
                detail: "process not started".to_string(),
            };
        }

        let healthy = self.is_healthy().await;
        SkillProcessHealth {
            running: true,
            healthy,
            detail: if healthy {
                "healthy".to_string()
            } else {
                "process running but health check failed".to_string()
            },
        }
    }

    /// Restart the process.
    pub async fn restart(&self) -> Result<()> {
        info!("Restarting skill process '{}'...", self.name);
        self.stop().await?;
        sleep(Duration::from_millis(500)).await;
        self.start().await?;
        Ok(())
    }

    /// Handle a detected crash: optionally restart.
    pub async fn handle_crash(&self) -> Result<()> {
        warn!("Skill process '{}' appears to have crashed", self.name);

        if !self.auto_restart {
            warn!(
                "Auto-restart disabled for '{}', leaving stopped",
                self.name
            );
            return Ok(());
        }

        let count = self.increment_restart_count().await;
        let max = self.max_restarts;

        if count > max {
            error!(
                "Skill process '{}' exceeded max restarts ({max}), giving up",
                self.name
            );
            return Ok(());
        }

        info!(
            "Auto-restarting skill process '{}' (attempt {count}/{max})...",
            self.name
        );

        sleep(self.restart_delay).await;
        self.restart().await
    }

    // ─── Internal Helpers ──────────────────────────────────────────────

    async fn increment_restart_count(&self) -> u32 {
        let mut count = self.restart_count.write().await;
        *count += 1;
        *count
    }

    async fn reset_restart_count(&self) {
        *self.restart_count.write().await = 0;
    }
}

impl Drop for SkillProcess {
    fn drop(&mut self) {
        // We can't do async cleanup in Drop. The process is configured
        // with kill_on_drop so the OS will clean up.
        debug!("SkillProcess '{}' dropped", self.name);
    }
}

// ─── Health Status ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SkillProcessHealth {
    pub running: bool,
    pub healthy: bool,
    pub detail: String,
}

// ─── Process Registry ─────────────────────────────────────────────────────

/// A registry that manages multiple skill processes.
pub struct ProcessRegistry {
    processes: RwLock<Vec<Arc<SkillProcess>>>,
}

impl ProcessRegistry {
    pub fn new() -> Self {
        Self {
            processes: RwLock::new(Vec::new()),
        }
    }

    /// Register a process in the registry.
    pub async fn register(&self, process: Arc<SkillProcess>) {
        self.processes.write().await.push(process);
    }

    /// Start all registered processes.
    pub async fn start_all(&self) -> Vec<(String, Result<()>)> {
        let processes = self.processes.read().await;
        let mut results = Vec::new();

        for proc in processes.iter() {
            let name = proc.name.clone();
            let result = proc.start().await;
            results.push((name, result));
        }

        results
    }

    /// Stop all registered processes.
    pub async fn stop_all(&self) {
        let processes = self.processes.read().await;
        for proc in processes.iter() {
            let _ = proc.stop().await;
        }
    }

    /// Health check all registered processes.
    pub async fn health_all(&self) -> Vec<(String, SkillProcessHealth)> {
        let processes = self.processes.read().await;
        let mut results = Vec::new();

        for proc in processes.iter() {
            let name = proc.name.clone();
            let health = proc.health_status().await;
            results.push((name, health));
        }

        results
    }

    /// Find a process by name.
    pub async fn find(&self, name: &str) -> Option<Arc<SkillProcess>> {
        let processes = self.processes.read().await;
        processes.iter().find(|p| p.name == name).cloned()
    }
}

impl Default for ProcessRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────

async fn http_health_check(url: &str) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .connect_timeout(Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    match client.get(url).send().await {
        Ok(resp) => {
            let status = resp.status();
            status.is_success() || status.as_u16() == 405
        }
        Err(_) => false,
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_process_builder() {
        let proc = SkillProcess::new("test-proc", "echo")
            .with_args(vec!["hello".to_string()])
            .with_health_check_url("http://localhost:9999/health")
            .with_auto_restart(true);

        assert_eq!(proc.name, "test-proc");
        assert_eq!(proc.command, "echo");
        assert_eq!(proc.args, vec!["hello"]);
        assert_eq!(
            proc.health_check_url,
            Some("http://localhost:9999/health".to_string())
        );
        assert!(proc.auto_restart);
    }

    #[test]
    fn test_trendradar_factory() {
        let root = PathBuf::from("/tmp/TrendRadar");
        let proc = SkillProcess::for_trendradar(root.clone(), 3333);

        assert_eq!(proc.name, "trendradar-mcp");
        assert_eq!(proc.command, "python");
        assert!(proc.args.contains(&"mcp_server".to_string()));
        assert!(proc
            .health_check_url
            .as_ref()
            .unwrap()
            .contains("127.0.0.1:3333"));
        assert!(proc.auto_restart);
        assert_eq!(proc.max_restarts, 5);
        assert_eq!(proc.restart_delay, Duration::from_secs(3));
    }

    #[tokio::test]
    async fn test_process_registry() {
        let registry = ProcessRegistry::new();
        let proc = Arc::new(SkillProcess::new("dummy", "echo").with_args(vec![
            "test".to_string(),
        ]));

        registry.register(proc.clone()).await;

        let found = registry.find("dummy").await;
        assert!(found.is_some());

        let missing = registry.find("nonexistent").await;
        assert!(missing.is_none());
    }
}
