//! Log viewer service implementation.
//!
//! Provides log file reading, filtering, and management capabilities including:
//! - Log entry parsing and querying
//! - Level and keyword filtering
//! - Time range filtering
//! - Log export and cleanup

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::OnceLock;
use thiserror::Error;

/// Default log file path
const DEFAULT_LOG_PATH: &str = ".omninova/logs/omninova.log";

/// Default page size for log queries
const DEFAULT_PAGE_SIZE: usize = 100;

/// Global log viewer service instance
static LOG_VIEWER: OnceLock<LogViewerService> = OnceLock::new();

/// Log level enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Copy, Hash)]
#[serde(rename_all = "UPPERCASE")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    /// Get log level from string (case-insensitive)
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "ERROR" => Some(Self::Error),
            "WARN" | "WARNING" => Some(Self::Warn),
            "INFO" => Some(Self::Info),
            "DEBUG" => Some(Self::Debug),
            "TRACE" => Some(Self::Trace),
            _ => None,
        }
    }

    /// Get all log levels
    pub fn all() -> Vec<Self> {
        vec![Self::Error, Self::Warn, Self::Info, Self::Debug, Self::Trace]
    }
}

impl Default for LogLevel {
    fn default() -> Self {
        Self::Info
    }
}

/// Log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Timestamp (Unix seconds)
    pub timestamp: i64,
    /// Log level
    pub level: LogLevel,
    /// Source module/target
    pub target: String,
    /// Log message content
    pub message: String,
}

/// Log query parameters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LogQuery {
    /// Start time (Unix seconds)
    pub start_time: Option<i64>,
    /// End time (Unix seconds)
    pub end_time: Option<i64>,
    /// Log level filter
    pub levels: Option<Vec<LogLevel>>,
    /// Keyword search (case-insensitive)
    pub keyword: Option<String>,
    /// Pagination offset
    pub offset: Option<usize>,
    /// Pagination limit
    pub limit: Option<usize>,
}

/// Log statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogStats {
    /// Log file size in bytes
    pub file_size: u64,
    /// Total entry count (approximate)
    pub entry_count: usize,
    /// Oldest entry timestamp
    pub oldest_entry: Option<i64>,
    /// Newest entry timestamp
    pub newest_entry: Option<i64>,
}

/// Log export format
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Json,
    Text,
    Csv,
}

/// Log viewer error type
#[derive(Debug, Clone, Error)]
pub enum LogViewerError {
    #[error("Log file not found: {0}")]
    FileNotFound(String),
    #[error("Failed to read log file: {0}")]
    ReadError(String),
    #[error("Failed to write log file: {0}")]
    WriteError(String),
    #[error("Invalid log format: {0}")]
    InvalidFormat(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

/// Log viewer service
pub struct LogViewerService {
    /// Log file path
    log_path: PathBuf,
}

impl Default for LogViewerService {
    fn default() -> Self {
        let log_path = home::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(DEFAULT_LOG_PATH);
        Self::new(log_path)
    }
}

impl LogViewerService {
    /// Create a new log viewer service
    pub fn new(log_path: PathBuf) -> Self {
        Self { log_path }
    }

    /// Get the global log viewer service instance
    pub fn global() -> &'static LogViewerService {
        LOG_VIEWER.get_or_init(LogViewerService::default)
    }

    /// Query log entries
    pub fn query(&self, query: LogQuery) -> Result<Vec<LogEntry>, LogViewerError> {
        if !self.log_path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&self.log_path)
            .map_err(|e| LogViewerError::ReadError(e.to_string()))?;

        let reader = BufReader::new(file);
        let mut entries = Vec::new();
        let limit = query.limit.unwrap_or(DEFAULT_PAGE_SIZE);
        let offset = query.offset.unwrap_or(0);
        let levels = query.levels.clone();
        let keyword = query.keyword.as_ref().map(|k| k.to_lowercase());
        let start_time = query.start_time;
        let end_time = query.end_time;

        for line in reader.lines() {
            let line = line.map_err(|e| LogViewerError::ReadError(e.to_string()))?;

            if let Some(entry) = self.parse_line(&line) {
                // Apply filters
                if let Some(ref levels) = levels {
                    if !levels.contains(&entry.level) {
                        continue;
                    }
                }

                if let Some(start) = start_time {
                    if entry.timestamp < start {
                        continue;
                    }
                }

                if let Some(end) = end_time {
                    if entry.timestamp > end {
                        continue;
                    }
                }

                if let Some(ref kw) = keyword {
                    if !entry.message.to_lowercase().contains(kw)
                        && !entry.target.to_lowercase().contains(kw)
                    {
                        continue;
                    }
                }

                entries.push(entry);
            }
        }

        // Sort by timestamp (newest first)
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Apply pagination
        let paginated: Vec<LogEntry> = entries
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();

        Ok(paginated)
    }

    /// Get log statistics
    pub fn stats(&self) -> Result<LogStats, LogViewerError> {
        if !self.log_path.exists() {
            return Ok(LogStats {
                file_size: 0,
                entry_count: 0,
                oldest_entry: None,
                newest_entry: None,
            });
        }

        let metadata = fs::metadata(&self.log_path)
            .map_err(|e| LogViewerError::ReadError(e.to_string()))?;

        let file = File::open(&self.log_path)
            .map_err(|e| LogViewerError::ReadError(e.to_string()))?;

        let reader = BufReader::new(file);
        let mut count = 0;
        let mut oldest: Option<i64> = None;
        let mut newest: Option<i64> = None;

        for line in reader.lines() {
            if let Ok(line) = line {
                if let Some(entry) = self.parse_line(&line) {
                    count += 1;
                    oldest = Some(oldest.map_or(entry.timestamp, |o| o.min(entry.timestamp)));
                    newest = Some(newest.map_or(entry.timestamp, |n| n.max(entry.timestamp)));
                }
            }
        }

        Ok(LogStats {
            file_size: metadata.len(),
            entry_count: count,
            oldest_entry: oldest,
            newest_entry: newest,
        })
    }

    /// Export log entries to a file
    pub fn export(
        &self,
        format: ExportFormat,
        query: LogQuery,
        output_path: &PathBuf,
    ) -> Result<(), LogViewerError> {
        let entries = self.query(query)?;

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(output_path)
            .map_err(|e| LogViewerError::WriteError(e.to_string()))?;

        match format {
            ExportFormat::Json => {
                let json = serde_json::to_string_pretty(&entries)
                    .map_err(|e| LogViewerError::WriteError(e.to_string()))?;
                file.write_all(json.as_bytes())
                    .map_err(|e| LogViewerError::WriteError(e.to_string()))?;
            }
            ExportFormat::Text => {
                for entry in &entries {
                    let time = DateTime::from_timestamp(entry.timestamp, 0)
                        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_else(|| entry.timestamp.to_string());
                    let line = format!(
                        "[{}] {:?} {}: {}\n",
                        time, entry.level, entry.target, entry.message
                    );
                    file.write_all(line.as_bytes())
                        .map_err(|e| LogViewerError::WriteError(e.to_string()))?;
                }
            }
            ExportFormat::Csv => {
                file.write_all(b"timestamp,level,target,message\n")
                    .map_err(|e| LogViewerError::WriteError(e.to_string()))?;
                for entry in &entries {
                    let line = format!(
                        "{},{},\"{}\",\"{}\"\n",
                        entry.timestamp,
                        serde_json::to_string(&entry.level).unwrap_or_default(),
                        entry.target.replace('"', "\"\""),
                        entry.message.replace('"', "\"\"")
                    );
                    file.write_all(line.as_bytes())
                        .map_err(|e| LogViewerError::WriteError(e.to_string()))?;
                }
            }
        }

        Ok(())
    }

    /// Clear logs (optionally before a given time)
    pub fn clear(&self, before: Option<i64>) -> Result<(), LogViewerError> {
        if !self.log_path.exists() {
            return Ok(());
        }

        match before {
            Some(timestamp) => {
                // Keep entries after the timestamp
                let file = File::open(&self.log_path)
                    .map_err(|e| LogViewerError::ReadError(e.to_string()))?;
                let reader = BufReader::new(file);
                let mut kept_lines = Vec::new();

                for line in reader.lines() {
                    if let Ok(line) = line {
                        if let Some(entry) = self.parse_line(&line) {
                            if entry.timestamp >= timestamp {
                                kept_lines.push(line);
                            }
                        } else {
                            // Keep unparseable lines
                            kept_lines.push(line);
                        }
                    }
                }

                // Rewrite the file
                let mut file = OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(&self.log_path)
                    .map_err(|e| LogViewerError::WriteError(e.to_string()))?;

                for line in kept_lines {
                    writeln!(file, "{}", line)
                        .map_err(|e| LogViewerError::WriteError(e.to_string()))?;
                }
            }
            None => {
                // Clear all logs
                OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(&self.log_path)
                    .map_err(|e| LogViewerError::WriteError(e.to_string()))?;
            }
        }

        Ok(())
    }

    /// Get log file path
    pub fn log_path(&self) -> &PathBuf {
        &self.log_path
    }

    /// Parse a log line into a LogEntry
    fn parse_line(&self, line: &str) -> Option<LogEntry> {
        // Try to parse as JSON first (structured logging)
        if line.starts_with('{') {
            if let Ok(json_entry) = serde_json::from_str::<JsonLogEntry>(line) {
                return Some(LogEntry {
                    timestamp: json_entry.timestamp,
                    level: json_entry.level,
                    target: json_entry.target,
                    message: json_entry.message,
                });
            }
        }

        // Try to parse tracing-subscriber format: "YYYY-MM-DDTHH:MM:SS LEVEL target: message"
        // or "[TIMESTAMP] LEVEL target: message"
        self.parse_tracing_format(line)
    }

    /// Parse tracing-subscriber formatted log line
    fn parse_tracing_format(&self, line: &str) -> Option<LogEntry> {
        let parts: Vec<&str> = line.splitn(4, ' ').collect();
        if parts.len() < 4 {
            return None;
        }

        // Parse timestamp
        let timestamp = self.parse_timestamp(parts[0])?;

        // Parse level
        let level = LogLevel::from_str(parts[1])?;

        // Parse target (may end with ':')
        let target = parts[2].trim_end_matches(':').to_string();

        // Message is the rest
        let message = parts[3].to_string();

        Some(LogEntry {
            timestamp,
            level,
            target,
            message,
        })
    }

    /// Parse timestamp from various formats
    fn parse_timestamp(&self, s: &str) -> Option<i64> {
        // Try ISO format with timezone: 2026-03-26T12:00:00Z or 2026-03-26T12:00:00+00:00
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            return Some(dt.timestamp());
        }

        // Try ISO format without timezone: 2026-03-26T12:00:00 (assume UTC)
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
            return Some(dt.and_utc().timestamp());
        }

        // Try Unix timestamp
        if let Ok(ts) = s.parse::<i64>() {
            return Some(ts);
        }

        // Try common log format: 2026-03-26 12:00:00
        let cleaned = s.trim_start_matches('[').trim_end_matches(']');
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(cleaned, "%Y-%m-%d %H:%M:%S") {
            return Some(dt.and_utc().timestamp());
        }

        None
    }
}

/// JSON log entry for structured logging
#[derive(Debug, Clone, Deserialize)]
struct JsonLogEntry {
    #[serde(default)]
    timestamp: i64,
    level: LogLevel,
    target: String,
    message: String,
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Get the global log viewer service
pub fn log_viewer() -> &'static LogViewerService {
    LogViewerService::global()
}

/// Query logs using the global service
pub fn query_logs(query: LogQuery) -> Result<Vec<LogEntry>, LogViewerError> {
    log_viewer().query(query)
}

/// Get log statistics using the global service
pub fn log_stats() -> Result<LogStats, LogViewerError> {
    log_viewer().stats()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_from_str() {
        assert_eq!(LogLevel::from_str("ERROR"), Some(LogLevel::Error));
        assert_eq!(LogLevel::from_str("error"), Some(LogLevel::Error));
        assert_eq!(LogLevel::from_str("WARN"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::from_str("WARNING"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::from_str("INFO"), Some(LogLevel::Info));
        assert_eq!(LogLevel::from_str("DEBUG"), Some(LogLevel::Debug));
        assert_eq!(LogLevel::from_str("TRACE"), Some(LogLevel::Trace));
        assert_eq!(LogLevel::from_str("invalid"), None);
    }

    #[test]
    fn test_log_level_all() {
        let all = LogLevel::all();
        assert_eq!(all.len(), 5);
        assert!(all.contains(&LogLevel::Error));
        assert!(all.contains(&LogLevel::Warn));
        assert!(all.contains(&LogLevel::Info));
        assert!(all.contains(&LogLevel::Debug));
        assert!(all.contains(&LogLevel::Trace));
    }

    #[test]
    fn test_log_query_default() {
        let query = LogQuery::default();
        assert!(query.start_time.is_none());
        assert!(query.end_time.is_none());
        assert!(query.levels.is_none());
        assert!(query.keyword.is_none());
        assert!(query.offset.is_none());
        assert!(query.limit.is_none());
    }

    #[test]
    fn test_log_entry_serialization() {
        let entry = LogEntry {
            timestamp: 1234567890,
            level: LogLevel::Info,
            target: "test::module".to_string(),
            message: "Test message".to_string(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"timestamp\":1234567890"));
        assert!(json.contains("\"level\":\"INFO\""));
        assert!(json.contains("\"target\":\"test::module\""));
        assert!(json.contains("\"message\":\"Test message\""));

        let deserialized: LogEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.timestamp, entry.timestamp);
        assert_eq!(deserialized.level, entry.level);
        assert_eq!(deserialized.target, entry.target);
        assert_eq!(deserialized.message, entry.message);
    }

    #[test]
    fn test_parse_tracing_format() {
        let service = LogViewerService::default();

        // Test standard tracing format
        let line = "2026-03-26T12:00:00 INFO test::module: This is a test message";
        let entry = service.parse_line(line);
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.target, "test::module");
        assert_eq!(entry.message, "This is a test message");
    }

    #[test]
    fn test_parse_json_format() {
        let service = LogViewerService::default();

        let json = r#"{"timestamp":1234567890,"level":"INFO","target":"test::module","message":"Test message"}"#;
        let entry = service.parse_line(json);
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.timestamp, 1234567890);
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.target, "test::module");
        assert_eq!(entry.message, "Test message");
    }

    #[test]
    fn test_log_stats_nonexistent() {
        let service = LogViewerService::new(PathBuf::from("/nonexistent/path/logs.log"));
        let stats = service.stats().unwrap();
        assert_eq!(stats.file_size, 0);
        assert_eq!(stats.entry_count, 0);
        assert!(stats.oldest_entry.is_none());
        assert!(stats.newest_entry.is_none());
    }
}