//! Agent performance metrics monitoring module.
//!
//! Provides real-time monitoring of agent response times, success rates,
//! and provider performance comparison.

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Default retention period for detailed metrics (24 hours in seconds)
const DEFAULT_RETENTION_SECONDS: u64 = 24 * 60 * 60;

/// Maximum number of detailed metric records per agent
const MAX_DETAILED_RECORDS_PER_AGENT: usize = 10_000;

/// Response time warning threshold in milliseconds
pub const RESPONSE_TIME_WARNING_THRESHOLD_MS: u64 = 3000;

/// Success rate warning threshold (percentage)
pub const SUCCESS_RATE_WARNING_THRESHOLD: f64 = 95.0;

/// Single request performance record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequestMetric {
    /// Agent identifier
    pub agent_id: String,
    /// Provider name (e.g., "openai", "anthropic")
    pub provider: String,
    /// Model name
    pub model: String,
    /// Unix timestamp in seconds
    pub timestamp: i64,
    /// Response time in milliseconds
    pub response_time_ms: u64,
    /// Whether the request succeeded
    pub success: bool,
    /// Error code if failed
    pub error_code: Option<String>,
    /// Input token count (if available)
    pub tokens_input: Option<u64>,
    /// Output token count (if available)
    pub tokens_output: Option<u64>,
}

/// Aggregated agent performance statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentPerformanceStats {
    /// Agent identifier
    pub agent_id: String,
    /// Agent name (for display)
    #[serde(default)]
    pub agent_name: String,
    /// Total number of requests
    pub total_requests: u64,
    /// Successful requests count
    pub successful_requests: u64,
    /// Failed requests count
    pub failed_requests: u64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Minimum response time in milliseconds
    pub min_response_time_ms: u64,
    /// Maximum response time in milliseconds
    pub max_response_time_ms: u64,
    /// Success rate (0-100)
    pub success_rate: f64,
    /// P50 (median) response time in milliseconds
    pub p50_response_time_ms: u64,
    /// P95 response time in milliseconds
    pub p95_response_time_ms: u64,
    /// P99 response time in milliseconds
    pub p99_response_time_ms: u64,
}

/// Provider performance statistics for comparison
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderPerformanceStats {
    /// Provider name
    pub provider: String,
    /// Total number of requests
    pub total_requests: u64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Success rate (0-100)
    pub success_rate: f64,
}

/// Time series data point for charts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDataPoint {
    /// Unix timestamp in seconds
    pub timestamp: i64,
    /// Metric value
    pub value: f64,
}

/// Time range for queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    /// Start timestamp (Unix seconds)
    pub from: i64,
    /// End timestamp (Unix seconds)
    pub to: i64,
}

impl Default for TimeRange {
    fn default() -> Self {
        let now = Utc::now().timestamp();
        Self {
            from: now - 3600, // Default: last 1 hour
            to: now,
        }
    }
}

/// Metric type for time series queries
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    /// Response time metrics
    ResponseTime,
    /// Success rate metrics
    SuccessRate,
    /// Request count metrics
    RequestCount,
}

/// Agent performance metrics monitor
pub struct AgentMetricsMonitor {
    /// Recent request metrics by agent_id
    metrics_by_agent: RwLock<HashMap<String, Vec<AgentRequestMetric>>>,
    /// Global recent metrics (all agents)
    global_metrics: RwLock<Vec<AgentRequestMetric>>,
    /// Data retention duration
    retention: Duration,
}

impl Default for AgentMetricsMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentMetricsMonitor {
    /// Create a new metrics monitor with default retention
    pub fn new() -> Self {
        Self {
            metrics_by_agent: RwLock::new(HashMap::new()),
            global_metrics: RwLock::new(Vec::new()),
            retention: Duration::from_secs(DEFAULT_RETENTION_SECONDS),
        }
    }

    /// Create a metrics monitor with custom retention
    pub fn with_retention(retention: Duration) -> Self {
        Self {
            metrics_by_agent: RwLock::new(HashMap::new()),
            global_metrics: RwLock::new(Vec::new()),
            retention,
        }
    }

    /// Record a request metric
    pub fn record(&self, metric: AgentRequestMetric) {
        // Add to agent-specific metrics
        {
            let mut metrics_by_agent = self.metrics_by_agent.write();
            let agent_metrics = metrics_by_agent.entry(metric.agent_id.clone()).or_default();
            agent_metrics.push(metric.clone());

            // Enforce max records per agent
            if agent_metrics.len() > MAX_DETAILED_RECORDS_PER_AGENT {
                let drain_count = agent_metrics.len() - MAX_DETAILED_RECORDS_PER_AGENT;
                agent_metrics.drain(0..drain_count);
            }
        }

        // Add to global metrics
        {
            let mut global_metrics = self.global_metrics.write();
            global_metrics.push(metric);

            // Enforce max global records (scaled by typical agent count)
            const MAX_GLOBAL_RECORDS: usize = MAX_DETAILED_RECORDS_PER_AGENT * 10;
            if global_metrics.len() > MAX_GLOBAL_RECORDS {
                let drain_count = global_metrics.len() - MAX_GLOBAL_RECORDS;
                global_metrics.drain(0..drain_count);
            }
        }
    }

    /// Get performance statistics for a specific agent
    pub fn get_agent_stats(&self, agent_id: &str, time_range: Option<TimeRange>) -> AgentPerformanceStats {
        let metrics_by_agent = self.metrics_by_agent.read();
        let agent_metrics = metrics_by_agent.get(agent_id);

        let filtered: Vec<&AgentRequestMetric> = if let Some(metrics) = agent_metrics {
            if let Some(range) = time_range {
                metrics
                    .iter()
                    .filter(|m| m.timestamp >= range.from && m.timestamp <= range.to)
                    .collect()
            } else {
                metrics.iter().collect()
            }
        } else {
            return AgentPerformanceStats {
                agent_id: agent_id.to_string(),
                ..Default::default()
            };
        };

        Self::calculate_stats(agent_id, &filtered)
    }

    /// Get performance statistics for all agents
    pub fn get_all_agent_stats(&self, time_range: Option<TimeRange>) -> Vec<AgentPerformanceStats> {
        let metrics_by_agent = self.metrics_by_agent.read();

        metrics_by_agent
            .keys()
            .map(|agent_id| self.get_agent_stats(agent_id, time_range.clone()))
            .collect()
    }

    /// Get provider performance comparison
    pub fn get_provider_stats(&self, time_range: Option<TimeRange>) -> Vec<ProviderPerformanceStats> {
        let global_metrics = self.global_metrics.read();

        let filtered: Vec<&AgentRequestMetric> = if let Some(range) = time_range {
            global_metrics
                .iter()
                .filter(|m| m.timestamp >= range.from && m.timestamp <= range.to)
                .collect()
        } else {
            global_metrics.iter().collect()
        };

        // Group by provider
        let mut by_provider: HashMap<String, Vec<&AgentRequestMetric>> = HashMap::new();
        for metric in &filtered {
            by_provider
                .entry(metric.provider.clone())
                .or_default()
                .push(*metric);
        }

        // Calculate stats for each provider
        by_provider
            .into_iter()
            .map(|(provider, metrics)| {
                let total = metrics.len() as u64;
                let successful = metrics.iter().filter(|m| m.success).count() as u64;
                let total_time: u64 = metrics.iter().map(|m| m.response_time_ms).sum();
                let avg_time = if total > 0 {
                    total_time as f64 / total as f64
                } else {
                    0.0
                };
                let success_rate = if total > 0 {
                    (successful as f64 / total as f64) * 100.0
                } else {
                    0.0
                };

                ProviderPerformanceStats {
                    provider,
                    total_requests: total,
                    avg_response_time_ms: avg_time,
                    success_rate,
                }
            })
            .collect()
    }

    /// Get time series data for an agent
    pub fn get_time_series(
        &self,
        agent_id: &str,
        metric_type: MetricType,
        time_range: Option<TimeRange>,
        interval_seconds: Option<i64>,
    ) -> Vec<MetricDataPoint> {
        let metrics_by_agent = self.metrics_by_agent.read();
        let agent_metrics = metrics_by_agent.get(agent_id);

        let filtered: Vec<&AgentRequestMetric> = if let Some(metrics) = agent_metrics {
            if let Some(range) = &time_range {
                metrics
                    .iter()
                    .filter(|m| m.timestamp >= range.from && m.timestamp <= range.to)
                    .collect()
            } else {
                metrics.iter().collect()
            }
        } else {
            return Vec::new();
        };

        let interval = interval_seconds.unwrap_or(60); // Default: 1 minute intervals

        // Group metrics by interval
        let mut buckets: HashMap<i64, Vec<&AgentRequestMetric>> = HashMap::new();
        for metric in &filtered {
            let bucket = metric.timestamp / interval * interval;
            buckets.entry(bucket).or_default().push(*metric);
        }

        // Calculate data points based on metric type
        buckets
            .into_iter()
            .map(|(timestamp, metrics)| {
                let value = match metric_type {
                    MetricType::ResponseTime => {
                        let total: u64 = metrics.iter().map(|m| m.response_time_ms).sum();
                        let count = metrics.len();
                        if count > 0 {
                            total as f64 / count as f64
                        } else {
                            0.0
                        }
                    }
                    MetricType::SuccessRate => {
                        let total = metrics.len();
                        let successful = metrics.iter().filter(|m| m.success).count();
                        if total > 0 {
                            (successful as f64 / total as f64) * 100.0
                        } else {
                            0.0
                        }
                    }
                    MetricType::RequestCount => metrics.len() as f64,
                };

                MetricDataPoint { timestamp, value }
            })
            .collect()
    }

    /// Prune expired data
    pub fn prune_expired(&self) {
        let cutoff = Utc::now().timestamp() - self.retention.as_secs() as i64;

        {
            let mut global_metrics = self.global_metrics.write();
            global_metrics.retain(|m| m.timestamp > cutoff);
        }

        {
            let mut metrics_by_agent = self.metrics_by_agent.write();
            for metrics in metrics_by_agent.values_mut() {
                metrics.retain(|m| m.timestamp > cutoff);
            }
        }
    }

    /// Calculate statistics from a list of metrics
    fn calculate_stats(agent_id: &str, metrics: &[&AgentRequestMetric]) -> AgentPerformanceStats {
        if metrics.is_empty() {
            return AgentPerformanceStats {
                agent_id: agent_id.to_string(),
                ..Default::default()
            };
        }

        let total = metrics.len() as u64;
        let successful = metrics.iter().filter(|m| m.success).count() as u64;
        let failed = total - successful;

        let response_times: Vec<u64> = metrics.iter().map(|m| m.response_time_ms).collect();
        let total_time: u64 = response_times.iter().sum();
        let avg_time = total_time as f64 / total as f64;
        let min_time = *response_times.iter().min().unwrap_or(&0);
        let max_time = *response_times.iter().max().unwrap_or(&0);

        let success_rate = (successful as f64 / total as f64) * 100.0;

        // Calculate percentiles
        let mut sorted_times = response_times.clone();
        sorted_times.sort_unstable();
        let p50 = percentile(&sorted_times, 50);
        let p95 = percentile(&sorted_times, 95);
        let p99 = percentile(&sorted_times, 99);

        AgentPerformanceStats {
            agent_id: agent_id.to_string(),
            total_requests: total,
            successful_requests: successful,
            failed_requests: failed,
            avg_response_time_ms: avg_time,
            min_response_time_ms: min_time,
            max_response_time_ms: max_time,
            success_rate,
            p50_response_time_ms: p50,
            p95_response_time_ms: p95,
            p99_response_time_ms: p99,
            ..Default::default()
        }
    }

    /// Get total metrics count
    pub fn total_metrics_count(&self) -> usize {
        self.global_metrics.read().len()
    }
}

/// Calculate percentile from sorted data
fn percentile(sorted_data: &[u64], p: u8) -> u64 {
    if sorted_data.is_empty() {
        return 0;
    }
    if p >= 100 {
        return *sorted_data.last().unwrap();
    }

    let idx = ((sorted_data.len() as f64) * (p as f64 / 100.0)).ceil() as usize;
    sorted_data[idx.min(sorted_data.len()) - 1]
}

/// Global metrics monitor instance
static METRICS_MONITOR: std::sync::OnceLock<AgentMetricsMonitor> = std::sync::OnceLock::new();

/// Get the global metrics monitor
pub fn global_monitor() -> &'static AgentMetricsMonitor {
    METRICS_MONITOR.get_or_init(AgentMetricsMonitor::new)
}

/// Record a request metric to the global monitor
pub fn record(metric: AgentRequestMetric) {
    global_monitor().record(metric);
}

/// Get agent stats from the global monitor
pub fn get_agent_stats(agent_id: &str, time_range: Option<TimeRange>) -> AgentPerformanceStats {
    global_monitor().get_agent_stats(agent_id, time_range)
}

/// Get all agent stats from the global monitor
pub fn get_all_agent_stats(time_range: Option<TimeRange>) -> Vec<AgentPerformanceStats> {
    global_monitor().get_all_agent_stats(time_range)
}

/// Get provider stats from the global monitor
pub fn get_provider_stats(time_range: Option<TimeRange>) -> Vec<ProviderPerformanceStats> {
    global_monitor().get_provider_stats(time_range)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_metric(agent_id: &str, provider: &str, response_ms: u64, success: bool) -> AgentRequestMetric {
        AgentRequestMetric {
            agent_id: agent_id.to_string(),
            provider: provider.to_string(),
            model: "test-model".to_string(),
            timestamp: Utc::now().timestamp(),
            response_time_ms: response_ms,
            success,
            error_code: if success { None } else { Some("error".to_string()) },
            tokens_input: Some(100),
            tokens_output: Some(50),
        }
    }

    #[test]
    fn test_record_metric() {
        let monitor = AgentMetricsMonitor::new();
        let metric = create_test_metric("agent-1", "openai", 100, true);
        monitor.record(metric);

        let stats = monitor.get_agent_stats("agent-1", None);
        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.successful_requests, 1);
    }

    #[test]
    fn test_get_agent_stats() {
        let monitor = AgentMetricsMonitor::new();

        // Record multiple metrics
        monitor.record(create_test_metric("agent-1", "openai", 100, true));
        monitor.record(create_test_metric("agent-1", "openai", 200, true));
        monitor.record(create_test_metric("agent-1", "openai", 300, false));

        let stats = monitor.get_agent_stats("agent-1", None);

        assert_eq!(stats.total_requests, 3);
        assert_eq!(stats.successful_requests, 2);
        assert_eq!(stats.failed_requests, 1);
        assert!((stats.avg_response_time_ms - 200.0).abs() < 0.01);
        assert_eq!(stats.min_response_time_ms, 100);
        assert_eq!(stats.max_response_time_ms, 300);
        assert!((stats.success_rate - 66.67).abs() < 0.1);
    }

    #[test]
    fn test_percentile_calculation() {
        let data = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        assert_eq!(percentile(&data, 50), 50);
        assert_eq!(percentile(&data, 90), 90);
        assert_eq!(percentile(&data, 95), 100);
        assert_eq!(percentile(&data, 99), 100);
    }

    #[test]
    fn test_percentile_empty() {
        let data: Vec<u64> = vec![];
        assert_eq!(percentile(&data, 50), 0);
    }

    #[test]
    fn test_provider_stats() {
        let monitor = AgentMetricsMonitor::new();

        monitor.record(create_test_metric("agent-1", "openai", 100, true));
        monitor.record(create_test_metric("agent-2", "openai", 200, true));
        monitor.record(create_test_metric("agent-3", "anthropic", 150, false));

        let provider_stats = monitor.get_provider_stats(None);

        assert_eq!(provider_stats.len(), 2);

        let openai_stats = provider_stats.iter().find(|p| p.provider == "openai").unwrap();
        assert_eq!(openai_stats.total_requests, 2);
        assert_eq!(openai_stats.success_rate, 100.0);

        let anthropic_stats = provider_stats.iter().find(|p| p.provider == "anthropic").unwrap();
        assert_eq!(anthropic_stats.total_requests, 1);
        assert_eq!(anthropic_stats.success_rate, 0.0);
    }

    #[test]
    fn test_time_series() {
        let monitor = AgentMetricsMonitor::new();

        // Record metrics with different timestamps
        let now = Utc::now().timestamp();
        let mut metric = create_test_metric("agent-1", "openai", 100, true);
        metric.timestamp = now - 120;
        monitor.record(metric);

        let mut metric = create_test_metric("agent-1", "openai", 200, true);
        metric.timestamp = now - 60;
        monitor.record(metric);

        let mut metric = create_test_metric("agent-1", "openai", 300, true);
        metric.timestamp = now;
        monitor.record(metric);

        let time_series = monitor.get_time_series(
            "agent-1",
            MetricType::ResponseTime,
            None,
            Some(60),
        );

        assert!(!time_series.is_empty());
    }

    #[test]
    fn test_data_pruning() {
        let monitor = AgentMetricsMonitor::with_retention(Duration::from_secs(10));

        // Record an old metric
        let mut old_metric = create_test_metric("agent-1", "openai", 100, true);
        old_metric.timestamp = Utc::now().timestamp() - 100; // 100 seconds ago
        monitor.record(old_metric);

        // Record a new metric
        monitor.record(create_test_metric("agent-1", "openai", 200, true));

        assert_eq!(monitor.total_metrics_count(), 2);

        monitor.prune_expired();

        assert_eq!(monitor.total_metrics_count(), 1);
    }

    #[test]
    fn test_time_range_filtering() {
        let monitor = AgentMetricsMonitor::new();
        let now = Utc::now().timestamp();

        // Record metrics with different timestamps
        let mut metric1 = create_test_metric("agent-1", "openai", 100, true);
        metric1.timestamp = now - 200;
        monitor.record(metric1);

        let mut metric2 = create_test_metric("agent-1", "openai", 200, true);
        metric2.timestamp = now - 100;
        monitor.record(metric2);

        let mut metric3 = create_test_metric("agent-1", "openai", 300, true);
        metric3.timestamp = now;
        monitor.record(metric3);

        // Query only last 150 seconds
        let time_range = TimeRange {
            from: now - 150,
            to: now,
        };

        let stats = monitor.get_agent_stats("agent-1", Some(time_range));
        assert_eq!(stats.total_requests, 2); // Only metric2 and metric3
    }

    #[test]
    fn test_global_monitor() {
        let monitor = global_monitor();
        let initial_count = monitor.total_metrics_count();

        record(create_test_metric("test-agent", "test-provider", 500, true));

        assert_eq!(monitor.total_metrics_count(), initial_count + 1);
    }
}