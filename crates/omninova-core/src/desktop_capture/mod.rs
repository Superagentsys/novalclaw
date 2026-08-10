//! Desktop capture module for cross-platform screenshot and monitoring functionality.
//!
//! This module provides:
//! - Screenshot capture for desktop environments
//! - Change detection based on screenshot comparison
//! - Cross-platform support (Windows, macOS, Linux)
//!
//! Screenshots are saved to `{config_dir}/captures/` directory.

mod windows;

use serde::{Deserialize, Serialize};

/// Result of a single screenshot capture operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureResult {
    /// Whether the capture was successful.
    pub ok: bool,
    /// Path to the saved screenshot file.
    pub file_path: Option<String>,
    /// Width of the captured image in pixels.
    pub width: Option<u32>,
    /// Height of the captured image in pixels.
    pub height: Option<u32>,
    /// Size of the captured image file in bytes.
    pub file_size_bytes: Option<u64>,
    /// SHA256 hash of the captured image for change detection.
    pub hash: Option<String>,
    /// Error code if capture failed.
    pub error_code: Option<String>,
    /// Human-readable error message if capture failed.
    pub message: Option<String>,
}

impl CaptureResult {
    /// Check if the capture was successful.
    pub fn is_ok(&self) -> bool {
        self.ok
    }

    /// Create a successful capture result.
    pub fn success(
        file_path: String,
        width: u32,
        height: u32,
        file_size_bytes: u64,
        hash: String,
    ) -> Self {
        Self {
            ok: true,
            file_path: Some(file_path),
            width: Some(width),
            height: Some(height),
            file_size_bytes: Some(file_size_bytes),
            hash: Some(hash),
            error_code: None,
            message: None,
        }
    }

    /// Create a failed capture result.
    pub fn failure(error_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            file_path: None,
            width: None,
            height: None,
            file_size_bytes: None,
            hash: None,
            error_code: Some(error_code.into()),
            message: Some(message.into()),
        }
    }
}

/// Result of a monitoring session with start/end screenshots and change detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorResult {
    /// Whether the monitoring was successful.
    pub ok: bool,
    /// Requested monitoring duration in seconds.
    pub duration_secs: u64,
    /// Actual elapsed time in milliseconds.
    pub elapsed_ms: u64,
    /// Start screenshot capture result.
    pub start_capture: Option<Box<CaptureResult>>,
    /// End screenshot capture result.
    pub end_capture: Option<Box<CaptureResult>>,
    /// Whether a change was detected between start and end screenshots.
    pub changed: Option<bool>,
    /// Method used for change detection (e.g., "hash", "pixel_diff").
    pub change_method: Option<String>,
    /// Human-readable summary of changes detected.
    pub change_summary: Option<String>,
    /// Error code if monitoring failed.
    pub error_code: Option<String>,
    /// Human-readable message if monitoring failed.
    pub message: Option<String>,
}

impl MonitorResult {
    /// Create a successful monitoring result with change detection.
    pub fn success(
        duration_secs: u64,
        elapsed_ms: u64,
        start_capture: CaptureResult,
        end_capture: CaptureResult,
        changed: bool,
    ) -> Self {
        let change_summary = if changed {
            "检测到桌面内容有变化"
        } else {
            "桌面内容无明显变化"
        };

        Self {
            ok: true,
            duration_secs,
            elapsed_ms,
            start_capture: Some(Box::new(start_capture)),
            end_capture: Some(Box::new(end_capture)),
            changed: Some(changed),
            change_method: Some("hash".to_string()),
            change_summary: Some(change_summary.to_string()),
            error_code: None,
            message: None,
        }
    }

    /// Create a monitoring result when no change detection is available.
    pub fn success_no_detection(
        duration_secs: u64,
        elapsed_ms: u64,
        start_capture: CaptureResult,
        end_capture: CaptureResult,
    ) -> Self {
        Self {
            ok: true,
            duration_secs,
            elapsed_ms,
            start_capture: Some(Box::new(start_capture)),
            end_capture: Some(Box::new(end_capture)),
            changed: None,
            change_method: Some("unavailable".to_string()),
            change_summary: Some("暂不支持变化检测".to_string()),
            error_code: None,
            message: None,
        }
    }

    /// Create a failed monitoring result.
    pub fn failure(
        duration_secs: u64,
        elapsed_ms: u64,
        error_code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            ok: false,
            duration_secs,
            elapsed_ms,
            start_capture: None,
            end_capture: None,
            changed: None,
            change_method: None,
            change_summary: None,
            error_code: Some(error_code.into()),
            message: Some(message.into()),
        }
    }
}

/// Capture a screenshot and save to the captures directory.
///
/// # Arguments
/// * `captures_dir` - Directory to save screenshots (e.g., `{config_dir}/captures`)
/// * `prefix` - Filename prefix for the screenshot
///
/// # Returns
/// A `CaptureResult` with success or failure details.
pub async fn capture_screenshot(captures_dir: &std::path::Path, prefix: &str) -> CaptureResult {
    #[cfg(target_os = "windows")]
    {
        windows::capture_screen(captures_dir, prefix).await
    }

    #[cfg(target_os = "macos")]
    {
        // macOS implementation would go here
        CaptureResult::failure("macos_not_implemented", "macOS capture not implemented in this backend")
    }

    #[cfg(target_os = "linux")]
    {
        // Linux implementation would go here
        CaptureResult::failure("linux_not_implemented", "Linux capture not implemented in this backend")
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        CaptureResult::failure("unsupported_platform", "Unsupported platform for desktop capture")
    }
}

/// Perform desktop monitoring with start and end screenshots.
///
/// # Arguments
/// * `captures_dir` - Directory to save screenshots
/// * `duration_secs` - Duration to monitor in seconds
///
/// # Returns
/// A `MonitorResult` with start/end captures and change detection.
pub async fn monitor_desktop(captures_dir: &std::path::Path, duration_secs: u64) -> MonitorResult {
    use std::time::Instant;

    let started_at = Instant::now();

    // Capture start screenshot
    let start_capture = capture_screenshot(captures_dir, "monitor_start").await;
    if !start_capture.ok {
        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        return MonitorResult::failure(
            duration_secs,
            elapsed_ms,
            start_capture.error_code.clone().unwrap_or_else(|| "capture_failed".to_string()),
            start_capture.message.clone().unwrap_or_else(|| "Failed to capture start screenshot".to_string()),
        );
    }

    // Wait for the monitoring duration
    tokio::time::sleep(tokio::time::Duration::from_secs(duration_secs)).await;

    // Capture end screenshot
    let end_capture = capture_screenshot(captures_dir, "monitor_end").await;
    if !end_capture.ok {
        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        return MonitorResult::failure(
            duration_secs,
            elapsed_ms,
            end_capture.error_code.clone().unwrap_or_else(|| "capture_failed".to_string()),
            end_capture.message.clone().unwrap_or_else(|| "Failed to capture end screenshot".to_string()),
        );
    }

    let elapsed_ms = started_at.elapsed().as_millis() as u64;

    // Detect change based on hash comparison
    let changed = match (&start_capture.hash, &end_capture.hash) {
        (Some(start_hash), Some(end_hash)) => start_hash != end_hash,
        _ => false,
    };

    MonitorResult::success(duration_secs, elapsed_ms, start_capture, end_capture, changed)
}

/// Calculate SHA256 hash of file contents.
pub fn calculate_file_hash(path: &std::path::Path) -> Option<String> {
    use sha2::{Sha256, Digest};

    let data = std::fs::read(path).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let result = hasher.finalize();
    Some(hex::encode(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_result_success() {
        let result = CaptureResult::success(
            "/path/to/screenshot.png".to_string(),
            1920,
            1080,
            102400,
            "abc123".to_string(),
        );
        assert!(result.ok);
        assert_eq!(result.width, Some(1920));
        assert_eq!(result.height, Some(1080));
        assert_eq!(result.file_size_bytes, Some(102400));
        assert_eq!(result.hash, Some("abc123".to_string()));
        assert!(result.error_code.is_none());
    }

    #[test]
    fn test_capture_result_failure() {
        let result = CaptureResult::failure("no_permission", "No screen capture permission");
        assert!(!result.ok);
        assert!(result.file_path.is_none());
        assert_eq!(result.error_code, Some("no_permission".to_string()));
        assert_eq!(result.message, Some("No screen capture permission".to_string()));
    }

    #[test]
    fn test_monitor_result_with_change() {
        let start = CaptureResult::success("start.png".to_string(), 1920, 1080, 100, "hash1".to_string());
        let end = CaptureResult::success("end.png".to_string(), 1920, 1080, 100, "hash2".to_string());
        let result = MonitorResult::success(30, 30000, start, end, true);
        assert!(result.ok);
        assert_eq!(result.duration_secs, 30);
        assert!(result.changed.unwrap());
        assert_eq!(result.change_method, Some("hash".to_string()));
    }

    #[test]
    fn test_monitor_result_no_change() {
        let start = CaptureResult::success("start.png".to_string(), 1920, 1080, 100, "hash".to_string());
        let end = CaptureResult::success("end.png".to_string(), 1920, 1080, 100, "hash".to_string());
        let result = MonitorResult::success(30, 30000, start, end, false);
        assert!(result.ok);
        assert!(!result.changed.unwrap());
        assert_eq!(result.change_summary, Some("桌面内容无明显变化".to_string()));
    }
}
