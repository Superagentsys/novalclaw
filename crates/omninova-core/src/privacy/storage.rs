//! Storage information module for OmniNova Claw
//!
//! Provides functionality for calculating and displaying storage usage
//! including database, config, logs, and cache sizes.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Storage information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StorageInfo {
    /// Config directory path
    pub config_path: String,
    /// Data directory path
    pub data_path: String,
    /// Total storage size in bytes
    pub total_size: u64,
    /// Storage breakdown by category
    pub breakdown: StorageBreakdown,
}

impl StorageInfo {
    /// Calculate storage info for the application
    pub async fn calculate() -> Result<Self> {
        let data_dir = get_data_directory()?;
        let config_dir = get_config_directory()?;

        let breakdown = StorageBreakdown {
            database: calculate_directory_size(&data_dir.join("data")).await.unwrap_or(0),
            config: calculate_file_size(&config_dir.join("config.toml")).await.unwrap_or(0),
            logs: calculate_directory_size(&data_dir.join("logs")).await.unwrap_or(0),
            cache: calculate_directory_size(&data_dir.join("cache")).await.unwrap_or(0),
        };

        let total_size = breakdown.database + breakdown.config + breakdown.logs + breakdown.cache;

        Ok(Self {
            config_path: config_dir.to_string_lossy().to_string(),
            data_path: data_dir.to_string_lossy().to_string(),
            total_size,
            breakdown,
        })
    }

    /// Format total size as human-readable string
    pub fn formatted_total_size(&self) -> String {
        format_size(self.total_size)
    }
}

/// Storage breakdown by category
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StorageBreakdown {
    /// Database size in bytes
    pub database: u64,
    /// Config file size in bytes
    pub config: u64,
    /// Log files size in bytes
    pub logs: u64,
    /// Cache size in bytes
    pub cache: u64,
}

impl StorageBreakdown {
    /// Get the largest category
    pub fn largest_category(&self) -> (&'static str, u64) {
        let categories = [
            ("database", self.database),
            ("config", self.config),
            ("logs", self.logs),
            ("cache", self.cache),
        ];

        categories
            .iter()
            .max_by_key(|(_, size)| size)
            .map(|(name, size)| (*name, *size))
            .unwrap_or(("none", 0))
    }
}

/// Clear options for conversation history
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClearOptions {
    /// Clear scope
    pub scope: ClearScope,
    /// Agent IDs to clear (when scope is SpecificAgents)
    pub agent_ids: Option<Vec<String>>,
    /// Date range to clear (when scope is DateRange)
    pub date_range: Option<DateRange>,
}

impl Default for ClearOptions {
    fn default() -> Self {
        Self {
            scope: ClearScope::All,
            agent_ids: None,
            date_range: None,
        }
    }
}

/// Clear scope
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ClearScope {
    /// Clear all conversation history
    All,
    /// Clear specific agents' history
    SpecificAgents,
    /// Clear history within a date range
    DateRange,
}

/// Date range for clearing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DateRange {
    /// Start timestamp (inclusive)
    pub start: i64,
    /// End timestamp (inclusive)
    pub end: i64,
}

impl DateRange {
    /// Create a new date range
    pub fn new(start: i64, end: i64) -> Self {
        Self { start, end }
    }

    /// Create a range for the last N days
    pub fn last_days(days: i64) -> Self {
        let now = chrono::Utc::now().timestamp();
        let start = now - (days * 86400);
        Self { start, end: now }
    }
}

/// Clear result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClearResult {
    /// Number of messages deleted
    pub messages_deleted: u64,
    /// Number of sessions deleted
    pub sessions_deleted: u64,
    /// Space freed in bytes
    pub space_freed: u64,
}

impl Default for ClearResult {
    fn default() -> Self {
        Self {
            messages_deleted: 0,
            sessions_deleted: 0,
            space_freed: 0,
        }
    }
}

impl ClearResult {
    /// Create an empty result
    pub fn empty() -> Self {
        Self::default()
    }

    /// Check if anything was deleted
    pub fn has_deletions(&self) -> bool {
        self.messages_deleted > 0 || self.sessions_deleted > 0
    }

    /// Format space freed as human-readable string
    pub fn formatted_space_freed(&self) -> String {
        format_size(self.space_freed)
    }
}

/// Format size in bytes as human-readable string
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Get the data directory path
pub fn get_data_directory() -> Result<PathBuf> {
    directories::ProjectDirs::from("com", "omninoval", "omninoval")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .context("Cannot determine data directory")
}

/// Get the config directory path
pub fn get_config_directory() -> Result<PathBuf> {
    directories::ProjectDirs::from("com", "omninoval", "omninoval")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .context("Cannot determine config directory")
}

/// Calculate the size of a file
pub async fn calculate_file_size(path: &PathBuf) -> Result<u64> {
    if !path.exists() {
        return Ok(0);
    }

    let metadata = tokio::fs::metadata(path)
        .await
        .context("Failed to get file metadata")?;

    Ok(metadata.len())
}

/// Calculate the total size of a directory
pub fn calculate_directory_size<'a>(
    path: &'a PathBuf,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + 'a>> {
    Box::pin(async move {
        if !path.exists() {
            return Ok(0);
        }

        let mut total_size = 0;
        let mut entries = tokio::fs::read_dir(path)
            .await
            .context("Failed to read directory")?;

        while let Some(entry) = entries.next_entry().await.context("Failed to read directory entry")? {
            let path = entry.path();
            let metadata = entry.metadata().await.context("Failed to get metadata")?;

            if metadata.is_file() {
                total_size += metadata.len();
            } else if metadata.is_dir() {
                total_size += calculate_directory_size(&path).await.unwrap_or(0);
            }
        }

        Ok(total_size)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
        assert_eq!(format_size(1048576), "1.00 MB");
        assert_eq!(format_size(1572864), "1.50 MB");
        assert_eq!(format_size(1073741824), "1.00 GB");
        assert_eq!(format_size(1610612736), "1.50 GB");
    }

    #[test]
    fn test_storage_breakdown_largest() {
        let breakdown = StorageBreakdown {
            database: 1000,
            config: 100,
            logs: 500,
            cache: 200,
        };

        let (name, size) = breakdown.largest_category();
        assert_eq!(name, "database");
        assert_eq!(size, 1000);
    }

    #[test]
    fn test_clear_options_default() {
        let options = ClearOptions::default();
        assert_eq!(options.scope, ClearScope::All);
        assert!(options.agent_ids.is_none());
        assert!(options.date_range.is_none());
    }

    #[test]
    fn test_date_range_last_days() {
        let range = DateRange::last_days(7);
        let duration = range.end - range.start;
        // Should be approximately 7 days (allowing for some time elapsed during test)
        assert!(duration >= 7 * 86400 - 10);
        assert!(duration <= 7 * 86400 + 10);
    }

    #[test]
    fn test_clear_result_empty() {
        let result = ClearResult::empty();
        assert!(!result.has_deletions());
        assert_eq!(result.formatted_space_freed(), "0 B");
    }

    #[test]
    fn test_clear_result_with_deletions() {
        let result = ClearResult {
            messages_deleted: 100,
            sessions_deleted: 5,
            space_freed: 1048576, // 1 MB
        };
        assert!(result.has_deletions());
        assert_eq!(result.formatted_space_freed(), "1.00 MB");
    }

    #[test]
    fn test_storage_info_serialization() {
        let info = StorageInfo {
            config_path: "/path/to/config".to_string(),
            data_path: "/path/to/data".to_string(),
            total_size: 1048576,
            breakdown: StorageBreakdown {
                database: 512000,
                config: 1024,
                logs: 256000,
                cache: 280000,
            },
        };

        let json = serde_json::to_string(&info).expect("Serialization should succeed");
        let deserialized: StorageInfo =
            serde_json::from_str(&json).expect("Deserialization should succeed");

        assert_eq!(info, deserialized);
    }

    #[tokio::test]
    async fn test_calculate_file_size_nonexistent() {
        let path = PathBuf::from("/nonexistent/path/file.txt");
        let size = calculate_file_size(&path).await.expect("Should return 0 for nonexistent file");
        assert_eq!(size, 0);
    }

    #[tokio::test]
    async fn test_calculate_directory_size_nonexistent() {
        let path = PathBuf::from("/nonexistent/path/directory");
        let size = calculate_directory_size(&path).await.expect("Should return 0 for nonexistent directory");
        assert_eq!(size, 0);
    }
}