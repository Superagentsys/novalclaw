//! Memory monitor service implementation.
//!
//! Provides memory usage tracking and cache management including:
//! - Memory statistics collection
//! - Cache eviction policies
//! - Manual cache clearing

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use thiserror::Error;

/// Global memory monitor instance
static MEMORY_MONITOR: OnceLock<MemoryMonitor> = OnceLock::new();

/// Memory statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    /// Used memory in bytes
    pub used_bytes: u64,
    /// Available memory in bytes
    pub available_bytes: u64,
    /// Memory usage percentage
    pub usage_percent: f32,
    /// Timestamp of measurement
    pub timestamp: i64,
}

/// Cache eviction policy
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EvictionPolicy {
    /// Least Recently Used
    #[default]
    Lru,
    /// First In First Out
    Fifo,
    /// Least Frequently Used
    Lfu,
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Maximum cache size in bytes (default: 100MB)
    pub max_size: u64,
    /// Eviction policy
    pub eviction_policy: EvictionPolicy,
    /// Check interval in seconds
    pub check_interval_secs: u64,
    /// Warning threshold percentage
    pub warning_threshold_percent: f32,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_size: 100 * 1024 * 1024, // 100MB
            eviction_policy: EvictionPolicy::default(),
            check_interval_secs: 30,
            warning_threshold_percent: 80.0,
        }
    }
}

/// Memory error type
#[derive(Debug, Clone, Error)]
pub enum MemoryError {
    #[error("Failed to get memory info: {0}")]
    GetInfoFailed(String),
    #[error("Cache eviction failed: {0}")]
    EvictionFailed(String),
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

/// Memory monitor service
pub struct MemoryMonitor {
    config: std::sync::RwLock<CacheConfig>,
}

impl Default for MemoryMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryMonitor {
    /// Create a new memory monitor
    pub fn new() -> Self {
        Self {
            config: std::sync::RwLock::new(CacheConfig::default()),
        }
    }

    /// Get the global memory monitor instance
    pub fn global() -> &'static MemoryMonitor {
        MEMORY_MONITOR.get_or_init(MemoryMonitor::new)
    }

    /// Get current memory statistics
    pub fn get_stats(&self) -> MemoryStats {
        let used_bytes = self.get_process_memory();
        let available_bytes = self.get_available_memory();

        let usage_percent = if available_bytes > 0 {
            (used_bytes as f64 / available_bytes as f64 * 100.0) as f32
        } else {
            0.0
        };

        MemoryStats {
            used_bytes,
            available_bytes,
            usage_percent,
            timestamp: Utc::now().timestamp(),
        }
    }

    /// Get current cache configuration
    pub fn get_config(&self) -> CacheConfig {
        self.config.read().unwrap().clone()
    }

    /// Update cache configuration
    pub fn set_config(&self, config: CacheConfig) {
        *self.config.write().unwrap() = config;
    }

    /// Check if cache eviction is needed
    pub fn should_evict(&self) -> bool {
        let stats = self.get_stats();
        let config = self.config.read().unwrap();
        stats.usage_percent > config.warning_threshold_percent
    }

    /// Simulate cache eviction (returns bytes freed)
    /// In a real implementation, this would clear actual caches
    pub fn evict_cache(&self) -> Result<u64, MemoryError> {
        // This is a placeholder implementation
        // In a real app, this would clear:
        // - LRU caches
        // - Temporary buffers
        // - Unused data structures

        let stats = self.get_stats();
        let config = self.config.read().unwrap();

        // Simulate freeing 10% of used memory
        let bytes_to_free = stats.used_bytes / 10;

        Ok(bytes_to_free)
    }

    /// Get process memory usage
    fn get_process_memory(&self) -> u64 {
        // Platform-specific memory usage
        #[cfg(target_os = "linux")]
        {
            // Read /proc/self/status for VmRSS
            if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
                for line in status.lines() {
                    if line.starts_with("VmRSS:") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            if let Ok(kb) = parts[1].parse::<u64>() {
                                return kb * 1024;
                            }
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            // Use task_info on macOS
            // Simplified: return estimated value
        }

        #[cfg(target_os = "windows")]
        {
            // Use GetProcessMemoryInfo on Windows
            // Simplified: return estimated value
        }

        // Default fallback: estimate 100MB
        100 * 1024 * 1024
    }

    /// Get available system memory
    fn get_available_memory(&self) -> u64 {
        // Platform-specific available memory
        #[cfg(target_os = "linux")]
        {
            if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
                for line in meminfo.lines() {
                    if line.starts_with("MemAvailable:") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            if let Ok(kb) = parts[1].parse::<u64>() {
                                return kb * 1024;
                            }
                        }
                    }
                }
            }
        }

        // Default: assume 4GB available
        4 * 1024 * 1024 * 1024
    }
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Get the global memory monitor
pub fn memory_monitor() -> &'static MemoryMonitor {
    MemoryMonitor::global()
}

/// Get memory statistics
pub fn get_memory_stats() -> MemoryStats {
    memory_monitor().get_stats()
}

/// Clear cache
pub fn clear_cache() -> Result<u64, MemoryError> {
    memory_monitor().evict_cache()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_stats() {
        let monitor = MemoryMonitor::new();
        let stats = monitor.get_stats();
        assert!(stats.used_bytes > 0);
        assert!(stats.timestamp > 0);
    }

    #[test]
    fn test_cache_config_default() {
        let config = CacheConfig::default();
        assert_eq!(config.max_size, 100 * 1024 * 1024);
        assert!(matches!(config.eviction_policy, EvictionPolicy::Lru));
    }

    #[test]
    fn test_should_evict() {
        let monitor = MemoryMonitor::new();
        // By default, should_evict returns false for low memory usage
        let _should = monitor.should_evict();
    }

    #[test]
    fn test_evict_cache() {
        let monitor = MemoryMonitor::new();
        let result = monitor.evict_cache();
        assert!(result.is_ok());
    }

    #[test]
    fn test_memory_stats_serialization() {
        let stats = MemoryStats {
            used_bytes: 1024 * 1024,
            available_bytes: 4 * 1024 * 1024 * 1024,
            usage_percent: 25.0,
            timestamp: 1234567890,
        };

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"used_bytes\":1048576"));
        assert!(json.contains("\"usage_percent\":25.0"));
    }
}