//! System resource monitoring module.
//!
//! Provides real-time monitoring of CPU, memory, and disk usage.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use sysinfo::{Disk, Disks, System};

/// Memory warning threshold in MB
const MEMORY_WARNING_THRESHOLD_MB: u64 = 500;

/// History retention duration (1 hour)
const HISTORY_RETENTION: Duration = Duration::from_secs(3600);

/// Maximum history entries per resource type
const MAX_HISTORY_ENTRIES: usize = 360; // One entry per 10 seconds for 1 hour

/// Global system monitor instance
static MONITOR: OnceLock<SystemMonitor> = OnceLock::new();

/// System resource usage snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSnapshot {
    /// Timestamp of the snapshot (Unix timestamp in seconds)
    pub timestamp: u64,
    /// CPU usage percentage (0-100)
    pub cpu_usage: f32,
    /// Memory usage in MB
    pub memory_used_mb: u64,
    /// Total memory in MB
    pub memory_total_mb: u64,
    /// Memory usage percentage (0-100)
    pub memory_usage_percent: f32,
    /// Disk usage information
    pub disks: Vec<DiskUsage>,
}

/// Disk usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskUsage {
    /// Disk mount point / name
    pub name: String,
    /// Total disk space in GB
    pub total_gb: u64,
    /// Used disk space in GB
    pub used_gb: u64,
    /// Available disk space in GB
    pub available_gb: u64,
    /// Usage percentage (0-100)
    pub usage_percent: f32,
}

/// Resource usage history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: u64,
    pub value: f32,
}

/// System resource history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceHistory {
    /// CPU usage history (last hour)
    pub cpu: Vec<HistoryEntry>,
    /// Memory usage history (last hour)
    pub memory: Vec<HistoryEntry>,
}

/// System monitor
pub struct SystemMonitor {
    system: parking_lot::Mutex<System>,
    disks: parking_lot::Mutex<Disks>,
    history: parking_lot::Mutex<ResourceHistory>,
    last_update: parking_lot::Mutex<Instant>,
}

impl SystemMonitor {
    /// Create a new system monitor
    fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();

        let mut disks = Disks::new_with_refreshed_list();

        Self {
            system: parking_lot::Mutex::new(system),
            disks: parking_lot::Mutex::new(disks),
            history: parking_lot::Mutex::new(ResourceHistory {
                cpu: Vec::with_capacity(MAX_HISTORY_ENTRIES),
                memory: Vec::with_capacity(MAX_HISTORY_ENTRIES),
            }),
            last_update: parking_lot::Mutex::new(Instant::now()),
        }
    }

    /// Get the global monitor instance
    pub fn global() -> &'static SystemMonitor {
        MONITOR.get_or_init(Self::new)
    }

    /// Refresh system information
    fn refresh(&self) {
        let mut system = self.system.lock();
        system.refresh_cpu_all();
        system.refresh_memory();

        let now = Instant::now();
        let mut last_update = self.last_update.lock();

        // Update history every 10 seconds minimum
        if now.duration_since(*last_update) >= Duration::from_secs(10) {
            *last_update = now;

            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            // Calculate CPU usage (average across all CPUs)
            let cpu_usage = system.global_cpu_usage();
            let memory_usage = system.used_memory() as f64 / 1024.0 / 1024.0; // Convert to MB

            let mut history = self.history.lock();

            // Add new entries
            history.cpu.push(HistoryEntry {
                timestamp,
                value: cpu_usage,
            });
            history.memory.push(HistoryEntry {
                timestamp,
                value: memory_usage as f32,
            });

            // Prune old entries (older than 1 hour)
            let cutoff = timestamp.saturating_sub(3600);
            history.cpu.retain(|e| e.timestamp > cutoff);
            history.memory.retain(|e| e.timestamp > cutoff);

            // Also enforce max entries limit
            let cpu_len = history.cpu.len();
            if cpu_len > MAX_HISTORY_ENTRIES {
                history.cpu.drain(0..cpu_len - MAX_HISTORY_ENTRIES);
            }
            let mem_len = history.memory.len();
            if mem_len > MAX_HISTORY_ENTRIES {
                history.memory.drain(0..mem_len - MAX_HISTORY_ENTRIES);
            }
        }
    }

    /// Get current resource usage snapshot
    pub fn get_snapshot(&self) -> ResourceSnapshot {
        self.refresh();

        let system = self.system.lock();
        let disks = self.disks.lock();

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let cpu_usage = system.global_cpu_usage();

        let memory_used = system.used_memory();
        let memory_total = system.total_memory();
        let memory_used_mb = memory_used / 1024 / 1024;
        let memory_total_mb = memory_total / 1024 / 1024;
        let memory_usage_percent = if memory_total > 0 {
            (memory_used as f64 / memory_total as f64 * 100.0) as f32
        } else {
            0.0
        };

        let disk_usage: Vec<DiskUsage> = disks
            .iter()
            .filter_map(|disk| {
                let total = disk.total_space();
                let available = disk.available_space();
                let used = total.saturating_sub(available);

                if total == 0 {
                    return None;
                }

                Some(DiskUsage {
                    name: disk.mount_point().to_string_lossy().to_string(),
                    total_gb: total / 1024 / 1024 / 1024,
                    used_gb: used / 1024 / 1024 / 1024,
                    available_gb: available / 1024 / 1024 / 1024,
                    usage_percent: (used as f64 / total as f64 * 100.0) as f32,
                })
            })
            .collect();

        ResourceSnapshot {
            timestamp,
            cpu_usage,
            memory_used_mb,
            memory_total_mb,
            memory_usage_percent,
            disks: disk_usage,
        }
    }

    /// Get resource usage history
    pub fn get_history(&self) -> ResourceHistory {
        let history = self.history.lock();
        history.clone()
    }

    /// Check if memory usage warning should be shown
    pub fn is_memory_warning(&self) -> bool {
        let system = self.system.lock();
        let memory_used_mb = system.used_memory() / 1024 / 1024;
        memory_used_mb >= MEMORY_WARNING_THRESHOLD_MB
    }

    /// Export resource data in the specified format
    pub fn export(&self, format: ExportFormat) -> String {
        let snapshot = self.get_snapshot();
        let history = self.get_history();

        match format {
            ExportFormat::Json => {
                #[derive(Serialize)]
                struct ExportData {
                    snapshot: ResourceSnapshot,
                    history: ResourceHistory,
                }
                serde_json::to_string_pretty(&ExportData { snapshot, history })
                    .unwrap_or_else(|_| "{}".to_string())
            }
            ExportFormat::Csv => {
                let mut csv = String::from("timestamp,cpu_usage,memory_mb\n");
                for entry in &history.cpu {
                    let mem_entry = history
                        .memory
                        .iter()
                        .find(|m| m.timestamp == entry.timestamp);
                    csv.push_str(&format!(
                        "{},{},{}\n",
                        entry.timestamp,
                        entry.value,
                        mem_entry.map(|m| m.value).unwrap_or(0.0)
                    ));
                }
                csv
            }
        }
    }
}

/// Export format for resource data
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Json,
    Csv,
}

/// Convenience function to get the global monitor
pub fn monitor() -> &'static SystemMonitor {
    SystemMonitor::global()
}

/// Convenience function to get current resource snapshot
pub fn snapshot() -> ResourceSnapshot {
    monitor().get_snapshot()
}

/// Convenience function to get resource history
pub fn history() -> ResourceHistory {
    monitor().get_history()
}

/// Convenience function to check memory warning
pub fn is_memory_warning() -> bool {
    monitor().is_memory_warning()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_snapshot() {
        let monitor = SystemMonitor::new();
        let snapshot = monitor.get_snapshot();

        // Basic sanity checks
        assert!(snapshot.cpu_usage >= 0.0 && snapshot.cpu_usage <= 100.0);
        assert!(snapshot.memory_total_mb > 0);
        assert!(snapshot.memory_used_mb <= snapshot.memory_total_mb);
    }

    #[test]
    fn test_memory_warning() {
        let monitor = SystemMonitor::new();
        // Just ensure it doesn't panic
        let _warning = monitor.is_memory_warning();
    }

    #[test]
    fn test_history() {
        let monitor = SystemMonitor::new();
        let history = monitor.get_history();

        // Initial history should be empty or have few entries
        assert!(history.cpu.len() <= MAX_HISTORY_ENTRIES);
        assert!(history.memory.len() <= MAX_HISTORY_ENTRIES);
    }

    #[test]
    fn test_export_json() {
        let monitor = SystemMonitor::new();
        let json = monitor.export(ExportFormat::Json);
        assert!(json.starts_with('{'));
    }

    #[test]
    fn test_export_csv() {
        let monitor = SystemMonitor::new();
        let csv = monitor.export(ExportFormat::Csv);
        assert!(csv.starts_with("timestamp,cpu_usage"));
    }
}