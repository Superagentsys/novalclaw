//! Memory Performance Metrics
//!
//! Collects and reports performance metrics for memory operations.
//! Tracks query latency, cache hit rates, and operation counts across all memory layers.
//!
//! [Source: Story 5.5 - 记忆检索性能优化]

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::memory::MemoryLayer;

// ============================================================================
// Constants
// ============================================================================

/// Maximum number of recent queries to track for rolling statistics
const MAX_RECENT_QUERIES: usize = 1000;

/// Default duration for time-windowed statistics (5 minutes)
const DEFAULT_STATS_WINDOW_SECS: u64 = 300;

// ============================================================================
// Types
// ============================================================================

/// Query type for metrics categorization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryType {
    /// Store operation
    Store,
    /// Retrieve operation
    Retrieve,
    /// Search operation
    Search,
    /// Delete operation
    Delete,
    /// Stats operation
    Stats,
    /// Session operation
    Session,
}

impl std::fmt::Display for QueryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store => write!(f, "store"),
            Self::Retrieve => write!(f, "retrieve"),
            Self::Search => write!(f, "search"),
            Self::Delete => write!(f, "delete"),
            Self::Stats => write!(f, "stats"),
            Self::Session => write!(f, "session"),
        }
    }
}

/// Individual query metric record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryMetric {
    /// Type of query
    pub query_type: QueryType,
    /// Memory layer queried
    pub layer: MemoryLayer,
    /// Duration in milliseconds
    pub duration_ms: f64,
    /// Whether the query resulted in a cache hit (L1 only)
    pub cache_hit: bool,
    /// Number of results returned
    pub result_count: usize,
    /// Unix timestamp of the query
    pub timestamp: i64,
}

/// Aggregated performance statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceStats {
    // === L1 Stats ===
    /// L1 total queries
    pub l1_total_queries: u64,
    /// L1 cache hits
    pub l1_cache_hits: u64,
    /// L1 average latency (ms)
    pub l1_avg_latency_ms: f64,
    /// L1 max latency (ms)
    pub l1_max_latency_ms: f64,

    // === L2 Stats ===
    /// L2 total queries
    pub l2_total_queries: u64,
    /// L2 average latency (ms)
    pub l2_avg_latency_ms: f64,
    /// L2 max latency (ms)
    pub l2_max_latency_ms: f64,

    // === L3 Stats ===
    /// L3 total queries
    pub l3_total_queries: u64,
    /// L3 average latency (ms)
    pub l3_avg_latency_ms: f64,
    /// L3 max latency (ms)
    pub l3_max_latency_ms: f64,

    // === Overall Stats ===
    /// Total queries across all layers
    pub total_queries: u64,
    /// Overall cache hit rate (L1 hits / total L1 queries)
    pub overall_cache_hit_rate: f64,
    /// Overall average latency (ms)
    pub overall_avg_latency_ms: f64,
    /// Window size in seconds
    pub window_secs: u64,
}

impl PerformanceStats {
    /// Create empty stats
    pub fn new() -> Self {
        Self::default()
    }

    /// Get L1 cache hit rate as percentage
    pub fn l1_hit_rate_percent(&self) -> f64 {
        if self.l1_total_queries == 0 {
            0.0
        } else {
            (self.l1_cache_hits as f64 / self.l1_total_queries as f64) * 100.0
        }
    }
}

/// Rolling query record for time-windowed stats
#[derive(Debug, Clone)]
struct QueryRecord {
    layer: MemoryLayer,
    duration_ms: f64,
    cache_hit: bool,
    timestamp: Instant,
}

// ============================================================================
// MetricsCollector
// ============================================================================

/// Thread-safe metrics collector for memory operations
pub struct MetricsCollector {
    /// Recent query records (rolling window)
    recent_queries: Arc<RwLock<VecDeque<QueryRecord>>>,
    /// Window duration for stats
    window_duration: Duration,

    // Atomic counters for fast reads
    l1_total: AtomicU64,
    l1_hits: AtomicU64,
    l1_latency_sum_ns: AtomicU64,
    l1_max_ns: AtomicU64,

    l2_total: AtomicU64,
    l2_latency_sum_ns: AtomicU64,
    l2_max_ns: AtomicU64,

    l3_total: AtomicU64,
    l3_latency_sum_ns: AtomicU64,
    l3_max_ns: AtomicU64,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsCollector {
    /// Create a new metrics collector with default window
    pub fn new() -> Self {
        Self::with_window(DEFAULT_STATS_WINDOW_SECS)
    }

    /// Create a metrics collector with custom window size (seconds)
    pub fn with_window(window_secs: u64) -> Self {
        Self {
            recent_queries: Arc::new(RwLock::new(VecDeque::with_capacity(MAX_RECENT_QUERIES))),
            window_duration: Duration::from_secs(window_secs),
            l1_total: AtomicU64::new(0),
            l1_hits: AtomicU64::new(0),
            l1_latency_sum_ns: AtomicU64::new(0),
            l1_max_ns: AtomicU64::new(0),
            l2_total: AtomicU64::new(0),
            l2_latency_sum_ns: AtomicU64::new(0),
            l2_max_ns: AtomicU64::new(0),
            l3_total: AtomicU64::new(0),
            l3_latency_sum_ns: AtomicU64::new(0),
            l3_max_ns: AtomicU64::new(0),
        }
    }

    /// Record a query metric
    pub async fn record(&self, layer: MemoryLayer, duration: Duration, cache_hit: bool) {
        let duration_ns = duration.as_nanos() as u64;
        let duration_ms = duration.as_secs_f64() * 1000.0;

        // Update atomic counters based on layer
        match layer {
            MemoryLayer::L1 => {
                self.l1_total.fetch_add(1, Ordering::Relaxed);
                if cache_hit {
                    self.l1_hits.fetch_add(1, Ordering::Relaxed);
                }
                self.l1_latency_sum_ns.fetch_add(duration_ns, Ordering::Relaxed);
                Self::update_max(&self.l1_max_ns, duration_ns);
            }
            MemoryLayer::L2 => {
                self.l2_total.fetch_add(1, Ordering::Relaxed);
                self.l2_latency_sum_ns.fetch_add(duration_ns, Ordering::Relaxed);
                Self::update_max(&self.l2_max_ns, duration_ns);
            }
            MemoryLayer::L3 => {
                self.l3_total.fetch_add(1, Ordering::Relaxed);
                self.l3_latency_sum_ns.fetch_add(duration_ns, Ordering::Relaxed);
                Self::update_max(&self.l3_max_ns, duration_ns);
            }
            MemoryLayer::All => {
                // For "All" queries, we don't track per-layer stats
                // They're tracked separately for actual layer operations
            }
        }

        // Add to rolling window
        let record = QueryRecord {
            layer,
            duration_ms,
            cache_hit,
            timestamp: Instant::now(),
        };

        let mut queries = self.recent_queries.write().await;
        queries.push_back(record);

        // Prune old records
        Self::prune_old_records(&mut queries, self.window_duration);
    }

    /// Update max value atomically
    fn update_max(atomic: &AtomicU64, new_value: u64) {
        let mut current = atomic.load(Ordering::Relaxed);
        while new_value > current {
            match atomic.compare_exchange_weak(current, new_value, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }

    /// Prune records older than the window duration
    fn prune_old_records(queries: &mut VecDeque<QueryRecord>, window: Duration) {
        let cutoff = Instant::now() - window;
        while queries.front().map_or(false, |r| r.timestamp < cutoff) {
            queries.pop_front();
        }
    }

    /// Get current performance statistics
    pub async fn get_stats(&self) -> PerformanceStats {
        let l1_total = self.l1_total.load(Ordering::Relaxed);
        let l1_hits = self.l1_hits.load(Ordering::Relaxed);
        let l1_sum_ns = self.l1_latency_sum_ns.load(Ordering::Relaxed);
        let l1_max_ns = self.l1_max_ns.load(Ordering::Relaxed);

        let l2_total = self.l2_total.load(Ordering::Relaxed);
        let l2_sum_ns = self.l2_latency_sum_ns.load(Ordering::Relaxed);
        let l2_max_ns = self.l2_max_ns.load(Ordering::Relaxed);

        let l3_total = self.l3_total.load(Ordering::Relaxed);
        let l3_sum_ns = self.l3_latency_sum_ns.load(Ordering::Relaxed);
        let l3_max_ns = self.l3_max_ns.load(Ordering::Relaxed);

        let total = l1_total + l2_total + l3_total;
        let total_sum_ns = l1_sum_ns + l2_sum_ns + l3_sum_ns;

        PerformanceStats {
            l1_total_queries: l1_total,
            l1_cache_hits: l1_hits,
            l1_avg_latency_ms: if l1_total > 0 {
                (l1_sum_ns as f64 / l1_total as f64) / 1_000_000.0
            } else {
                0.0
            },
            l1_max_latency_ms: l1_max_ns as f64 / 1_000_000.0,

            l2_total_queries: l2_total,
            l2_avg_latency_ms: if l2_total > 0 {
                (l2_sum_ns as f64 / l2_total as f64) / 1_000_000.0
            } else {
                0.0
            },
            l2_max_latency_ms: l2_max_ns as f64 / 1_000_000.0,

            l3_total_queries: l3_total,
            l3_avg_latency_ms: if l3_total > 0 {
                (l3_sum_ns as f64 / l3_total as f64) / 1_000_000.0
            } else {
                0.0
            },
            l3_max_latency_ms: l3_max_ns as f64 / 1_000_000.0,

            total_queries: total,
            overall_cache_hit_rate: if l1_total > 0 {
                l1_hits as f64 / l1_total as f64
            } else {
                0.0
            },
            overall_avg_latency_ms: if total > 0 {
                (total_sum_ns as f64 / total as f64) / 1_000_000.0
            } else {
                0.0
            },
            window_secs: self.window_duration.as_secs(),
        }
    }

    /// Reset all statistics
    pub async fn reset(&self) {
        self.l1_total.store(0, Ordering::Relaxed);
        self.l1_hits.store(0, Ordering::Relaxed);
        self.l1_latency_sum_ns.store(0, Ordering::Relaxed);
        self.l1_max_ns.store(0, Ordering::Relaxed);

        self.l2_total.store(0, Ordering::Relaxed);
        self.l2_latency_sum_ns.store(0, Ordering::Relaxed);
        self.l2_max_ns.store(0, Ordering::Relaxed);

        self.l3_total.store(0, Ordering::Relaxed);
        self.l3_latency_sum_ns.store(0, Ordering::Relaxed);
        self.l3_max_ns.store(0, Ordering::Relaxed);

        let mut queries = self.recent_queries.write().await;
        queries.clear();
    }
}

// ============================================================================
// RAII Timer for automatic metric recording
// ============================================================================

/// RAII timer that records a metric when dropped
pub struct QueryTimer {
    collector: Arc<MetricsCollector>,
    layer: MemoryLayer,
    start: Instant,
    cache_hit: bool,
    cancelled: bool,
}

impl QueryTimer {
    /// Create a new timer
    pub fn new(collector: Arc<MetricsCollector>, layer: MemoryLayer) -> Self {
        Self {
            collector,
            layer,
            start: Instant::now(),
            cache_hit: false,
            cancelled: false,
        }
    }

    /// Mark this query as a cache hit (call before drop)
    pub fn set_cache_hit(&mut self, hit: bool) {
        self.cache_hit = hit;
    }

    /// Cancel this timer (won't record on drop)
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }
}

impl Drop for QueryTimer {
    fn drop(&mut self) {
        if !self.cancelled {
            let duration = self.start.elapsed();
            // Use tokio runtime handle if available, otherwise skip recording
            // This is a best-effort recording
            if let Ok(rt) = tokio::runtime::Handle::try_current() {
                let collector = self.collector.clone();
                let layer = self.layer;
                let cache_hit = self.cache_hit;
                rt.spawn(async move {
                    collector.record(layer, duration, cache_hit).await;
                });
            }
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
    fn test_query_type_display() {
        assert_eq!(format!("{}", QueryType::Store), "store");
        assert_eq!(format!("{}", QueryType::Retrieve), "retrieve");
        assert_eq!(format!("{}", QueryType::Search), "search");
    }

    #[test]
    fn test_performance_stats_hit_rate() {
        let mut stats = PerformanceStats::new();
        assert_eq!(stats.l1_hit_rate_percent(), 0.0);

        stats.l1_total_queries = 100;
        stats.l1_cache_hits = 75;
        assert_eq!(stats.l1_hit_rate_percent(), 75.0);
    }

    #[tokio::test]
    async fn test_metrics_collector_record() {
        let collector = MetricsCollector::new();

        // Record some queries
        collector.record(MemoryLayer::L1, Duration::from_millis(5), true).await;
        collector.record(MemoryLayer::L1, Duration::from_millis(8), false).await;
        collector.record(MemoryLayer::L2, Duration::from_millis(150), false).await;

        let stats = collector.get_stats().await;

        assert_eq!(stats.l1_total_queries, 2);
        assert_eq!(stats.l1_cache_hits, 1);
        assert_eq!(stats.l2_total_queries, 1);
        assert!(stats.l1_avg_latency_ms > 0.0);
    }

    #[tokio::test]
    async fn test_metrics_collector_reset() {
        let collector = MetricsCollector::new();

        collector.record(MemoryLayer::L1, Duration::from_millis(5), true).await;
        collector.reset().await;

        let stats = collector.get_stats().await;
        assert_eq!(stats.l1_total_queries, 0);
    }

    #[test]
    fn test_query_timer() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let collector = Arc::new(MetricsCollector::new());

        rt.block_on(async {
            {
                let mut timer = QueryTimer::new(collector.clone(), MemoryLayer::L1);
                timer.set_cache_hit(true);
                // Timer will record on drop
            }

            // Give async task time to complete
            tokio::time::sleep(Duration::from_millis(10)).await;

            let stats = collector.get_stats().await;
            assert_eq!(stats.l1_total_queries, 1);
            assert_eq!(stats.l1_cache_hits, 1);
        });
    }

    #[test]
    fn test_query_timer_cancel() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let collector = Arc::new(MetricsCollector::new());

        rt.block_on(async {
            {
                let mut timer = QueryTimer::new(collector.clone(), MemoryLayer::L1);
                timer.cancel();
                // Timer will NOT record on drop
            }

            tokio::time::sleep(Duration::from_millis(10)).await;

            let stats = collector.get_stats().await;
            assert_eq!(stats.l1_total_queries, 0);
        });
    }
}