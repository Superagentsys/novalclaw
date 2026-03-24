//! Skill Executor
//!
//! Executes skills with timeout control, caching, and logging.
//!
//! # Architecture
//!
//! The executor provides:
//! - Async skill execution with timeout
//! - Result caching for repeated calls
//! - Execution logging for debugging
//! - Batch execution support
//!
//! # Example
//!
//! ```rust,ignore
//! use omninova_core::skills::{SkillExecutor, SkillRegistry};
//! use std::sync::Arc;
//!
//! let registry = Arc::new(SkillRegistry::new());
//! let executor = SkillExecutor::new(registry, None);
//!
//! // Execute a skill
//! let result = executor.execute("web-search", context).await?;
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::time::timeout;
use tracing::{debug, error, info, instrument, warn};

use super::context::SkillContext;
use super::error::SkillError;
use super::registry::SkillRegistry;
use super::traits::{Skill, SkillResult};

// ============================================================================
// Executor Configuration
// ============================================================================

/// Configuration for the skill executor.
#[derive(Debug, Clone)]
pub struct SkillExecutorConfig {
    /// Default timeout for skill execution in milliseconds.
    pub default_timeout_ms: u64,

    /// Maximum timeout allowed in milliseconds.
    pub max_timeout_ms: u64,

    /// Enable result caching.
    pub enable_cache: bool,

    /// Cache TTL in seconds.
    pub cache_ttl_secs: u64,

    /// Maximum cache entries.
    pub max_cache_entries: usize,

    /// Enable execution logging.
    pub enable_logging: bool,
}

impl Default for SkillExecutorConfig {
    fn default() -> Self {
        Self {
            default_timeout_ms: 30_000,  // 30 seconds
            max_timeout_ms: 300_000,     // 5 minutes
            enable_cache: true,
            cache_ttl_secs: 300,         // 5 minutes
            max_cache_entries: 100,
            enable_logging: true,
        }
    }
}

// ============================================================================
// Execution Log
// ============================================================================

/// Log entry for skill execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionLog {
    /// Skill ID that was executed.
    pub skill_id: String,

    /// Agent ID that executed the skill.
    pub agent_id: String,

    /// Session ID if available.
    pub session_id: Option<String>,

    /// Whether execution was successful.
    pub success: bool,

    /// Execution duration in milliseconds.
    pub duration_ms: u64,

    /// Error message if failed.
    pub error: Option<String>,

    /// Timestamp of execution.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ExecutionLog {
    fn new(
        skill_id: &str,
        agent_id: &str,
        session_id: Option<&str>,
        success: bool,
        duration_ms: u64,
        error: Option<String>,
    ) -> Self {
        Self {
            skill_id: skill_id.to_string(),
            agent_id: agent_id.to_string(),
            session_id: session_id.map(|s| s.to_string()),
            success,
            duration_ms,
            error,
            timestamp: chrono::Utc::now(),
        }
    }
}

// ============================================================================
// Cache Entry
// ============================================================================

#[derive(Debug, Clone)]
struct CacheEntry {
    result: SkillResult,
    created_at: Instant,
    cache_key: String,
}

impl CacheEntry {
    fn new(cache_key: String, result: SkillResult) -> Self {
        Self {
            result,
            created_at: Instant::now(),
            cache_key,
        }
    }

    fn is_expired(&self, ttl_secs: u64) -> bool {
        self.created_at.elapsed() > Duration::from_secs(ttl_secs)
    }
}

// ============================================================================
// Skill Executor
// ============================================================================

/// Skill executor with timeout, caching, and logging.
pub struct SkillExecutor {
    /// Reference to the skill registry.
    registry: Arc<SkillRegistry>,

    /// Executor configuration.
    config: SkillExecutorConfig,

    /// Execution log (recent executions).
    execution_log: Arc<tokio::sync::RwLock<Vec<ExecutionLog>>>,

    /// Result cache.
    cache: Arc<tokio::sync::RwLock<Vec<CacheEntry>>>,
}

impl SkillExecutor {
    /// Create a new skill executor.
    pub fn new(registry: Arc<SkillRegistry>, config: Option<SkillExecutorConfig>) -> Self {
        Self {
            registry,
            config: config.unwrap_or_default(),
            execution_log: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            cache: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }

    /// Execute a skill by ID.
    ///
    /// # Arguments
    ///
    /// * `skill_id` - The ID of the skill to execute
    /// * `context` - The execution context
    ///
    /// # Errors
    ///
    /// Returns `SkillError` if:
    /// - The skill is not found
    /// - Configuration validation fails
    /// - Execution times out
    /// - The skill execution fails
    #[instrument(skip(self, context), fields(skill_id = %skill_id))]
    pub async fn execute(
        &self,
        skill_id: &str,
        context: SkillContext,
    ) -> Result<SkillResult, SkillError> {
        let start = Instant::now();
        let agent_id = context.agent_id.clone();
        let session_id = context.session_id.clone();

        // Check cache first
        if self.config.enable_cache {
            let cache_key = self.make_cache_key(skill_id, &context);
            if let Some(cached) = self.check_cache(&cache_key).await {
                debug!("Cache hit for skill: {}", skill_id);
                return Ok(cached);
            }
        }

        // Get the skill
        let skill = match self.registry.get(skill_id).await {
            Some(s) => s,
            None => {
                // Log the failed execution
                let duration_ms = start.elapsed().as_millis() as u64;
                self.log_execution(
                    skill_id,
                    &agent_id,
                    session_id.as_deref(),
                    false,
                    duration_ms,
                    Some(format!("Skill not found: {}", skill_id)),
                ).await;
                return Err(SkillError::not_found(skill_id));
            }
        };

        // Validate configuration
        if let Err(e) = skill.validate(&context.config) {
            let duration_ms = start.elapsed().as_millis() as u64;
            self.log_execution(
                skill_id,
                &agent_id,
                session_id.as_deref(),
                false,
                duration_ms,
                Some(e.to_string()),
            ).await;
            return Err(e);
        }

        // Execute with timeout
        let timeout_ms = self.get_timeout(&context);
        let result = self.execute_with_timeout(skill.as_ref(), context.clone(), timeout_ms).await;

        // Log execution
        let duration_ms = start.elapsed().as_millis() as u64;
        self.log_execution(
            skill_id,
            &agent_id,
            session_id.as_deref(),
            result.is_ok(),
            duration_ms,
            result.as_ref().err().map(|e| e.to_string()),
        ).await;

        // Cache successful result
        if result.is_ok() && self.config.enable_cache {
            let cache_key = self.make_cache_key(skill_id, &context);
            if let Ok(ref res) = result {
                self.cache_result(cache_key, res.clone()).await;
            }
        }

        result
    }

    /// Execute with timeout.
    async fn execute_with_timeout(
        &self,
        skill: &dyn Skill,
        context: SkillContext,
        timeout_ms: u64,
    ) -> Result<SkillResult, SkillError> {
        let timeout_duration = Duration::from_millis(timeout_ms);

        match timeout(timeout_duration, skill.execute(context)).await {
            Ok(result) => result,
            Err(_) => Err(SkillError::TimeoutError { timeout_ms }),
        }
    }

    /// Get timeout for execution.
    fn get_timeout(&self, context: &SkillContext) -> u64 {
        // Check if timeout is specified in context config
        if let Some(timeout_ms) = context.config_int("timeout_ms") {
            let timeout = timeout_ms as u64;
            timeout.min(self.config.max_timeout_ms)
        } else {
            self.config.default_timeout_ms
        }
    }

    /// Make a cache key for the skill execution.
    fn make_cache_key(&self, skill_id: &str, context: &SkillContext) -> String {
        // Use skill_id + agent_id + user_input hash for cache key
        // This is a simple implementation; could be improved with better hashing
        format!("{}:{}:{}", skill_id, context.agent_id, context.user_input.len())
    }

    /// Check cache for a cached result.
    async fn check_cache(&self, cache_key: &str) -> Option<SkillResult> {
        let cache = self.cache.read().await;
        cache.iter()
            .find(|entry| entry.cache_key == cache_key && !entry.is_expired(self.config.cache_ttl_secs))
            .map(|entry| entry.result.clone())
    }

    /// Cache a result.
    async fn cache_result(&self, cache_key: String, result: SkillResult) {
        let mut cache = self.cache.write().await;

        // Remove expired entries
        cache.retain(|entry| !entry.is_expired(self.config.cache_ttl_secs));

        // Enforce max entries
        while cache.len() >= self.config.max_cache_entries {
            cache.remove(0);
        }

        cache.push(CacheEntry::new(cache_key, result));
    }

    /// Log an execution.
    async fn log_execution(
        &self,
        skill_id: &str,
        agent_id: &str,
        session_id: Option<&str>,
        success: bool,
        duration_ms: u64,
        error: Option<String>,
    ) {
        if !self.config.enable_logging {
            return;
        }

        // Log to tracing
        if success {
            info!(
                skill_id = %skill_id,
                agent_id = %agent_id,
                duration_ms = duration_ms,
                "Skill execution successful"
            );
        } else {
            warn!(
                skill_id = %skill_id,
                agent_id = %agent_id,
                duration_ms = duration_ms,
                error = ?error,
                "Skill execution failed"
            );
        }

        // Store in execution log
        let log = ExecutionLog::new(skill_id, agent_id, session_id, success, duration_ms, error);
        let mut logs = self.execution_log.write().await;

        // Keep last 100 log entries
        if logs.len() >= 100 {
            logs.remove(0);
        }
        logs.push(log);
    }

    /// Execute multiple skills in sequence.
    pub async fn execute_batch(
        &self,
        skills: Vec<(&str, SkillContext)>,
    ) -> Vec<Result<SkillResult, SkillError>> {
        let mut results = Vec::with_capacity(skills.len());
        for (skill_id, context) in skills {
            results.push(self.execute(skill_id, context).await);
        }
        results
    }

    /// Execute multiple skills in parallel.
    pub async fn execute_parallel(
        &self,
        skills: Vec<(&str, SkillContext)>,
    ) -> Vec<Result<SkillResult, SkillError>> {
        let futures: Vec<_> = skills
            .into_iter()
            .map(|(skill_id, context)| self.execute(skill_id, context))
            .collect();

        futures_util::future::join_all(futures).await
    }

    /// Get recent execution logs.
    pub async fn get_execution_logs(&self, limit: usize) -> Vec<ExecutionLog> {
        let logs = self.execution_log.read().await;
        logs.iter().rev().take(limit).cloned().collect()
    }

    /// Clear execution logs.
    pub async fn clear_logs(&self) {
        let mut logs = self.execution_log.write().await;
        logs.clear();
    }

    /// Clear cache.
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Get cache statistics.
    pub async fn cache_stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        let total = cache.len();
        let expired = cache.iter().filter(|e| e.is_expired(self.config.cache_ttl_secs)).count();

        CacheStats {
            total_entries: total,
            expired_entries: expired,
            active_entries: total - expired,
            max_entries: self.config.max_cache_entries,
        }
    }
}

/// Cache statistics.
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub active_entries: usize,
    pub max_entries: usize,
}

impl Clone for SkillExecutor {
    fn clone(&self) -> Self {
        Self {
            registry: Arc::clone(&self.registry),
            config: self.config.clone(),
            execution_log: Arc::clone(&self.execution_log),
            cache: Arc::clone(&self.cache),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_config_default() {
        let config = SkillExecutorConfig::default();
        assert_eq!(config.default_timeout_ms, 30_000);
        assert!(config.enable_cache);
        assert_eq!(config.cache_ttl_secs, 300);
    }

    #[tokio::test]
    async fn test_executor_cache_key() {
        let registry = Arc::new(SkillRegistry::new());
        let executor = SkillExecutor::new(registry, None);

        let context = SkillContext::new("agent-1", "Hello");
        let key = executor.make_cache_key("test-skill", &context);

        assert!(key.contains("test-skill"));
        assert!(key.contains("agent-1"));
    }

    #[tokio::test]
    async fn test_executor_logs() {
        let registry = Arc::new(SkillRegistry::new());
        let executor = SkillExecutor::new(registry, None);

        // Try to execute non-existent skill
        let context = SkillContext::new("agent-1", "test");
        let _ = executor.execute("nonexistent", context).await;

        let logs = executor.get_execution_logs(10).await;
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].skill_id, "nonexistent");
        assert!(!logs[0].success);
    }

    #[tokio::test]
    async fn test_executor_clear_cache() {
        let registry = Arc::new(SkillRegistry::new());
        let executor = SkillExecutor::new(registry, None);

        // Clear cache
        executor.clear_cache().await;

        let stats = executor.cache_stats().await;
        assert_eq!(stats.total_entries, 0);
    }

    #[tokio::test]
    async fn test_executor_clear_logs() {
        let registry = Arc::new(SkillRegistry::new());
        let executor = SkillExecutor::new(registry, None);

        // Clear logs
        executor.clear_logs().await;

        let logs = executor.get_execution_logs(10).await;
        assert!(logs.is_empty());
    }
}