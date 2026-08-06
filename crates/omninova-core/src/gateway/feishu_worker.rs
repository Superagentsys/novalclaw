//! Feishu async worker for background processing of webhook events
//! 
//! This module provides background job processing to avoid Feishu's 3-second webhook timeout.
//! Webhook handlers quickly ACK, while Runtime and send_text run in the background.

use crate::channels::ChannelKind;
use crate::channels::adapters::outbound::OutboundResult;
use crate::desktop_capture::{self, CaptureResult, MonitorResult};
use crate::gateway::feishu_store::{FeishuStore, EventStatus, JobStatus, ReplyKind, chrono_timestamp};
use crate::gateway::OutboundMsgCache;
use crate::gateway::{GatewayRuntime, MonitorFlightLease};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio::time::timeout;

/// Queue capacity - max number of pending jobs
const QUEUE_CAPACITY: usize = 100;

/// Runtime execution timeout
const RUNTIME_TIMEOUT_SECS: u64 = 120;

/// Outbound send timeout
const OUTBOUND_TIMEOUT_SECS: u64 = 20;

/// Max concurrent jobs per worker pool
const WORKER_CONCURRENCY: usize = 4;

/// Default monitor duration in seconds
const DEFAULT_MONITOR_DURATION_SECS: u64 = 30;

/// Max monitor duration in seconds
const MAX_MONITOR_DURATION_SECS: u64 = 60;

/// Feishu async job - represents a webhook event to be processed in background
#[derive(Debug, Clone)]
pub struct FeishuAsyncJob {
    /// Channel kind (Feishu)
    pub channel: ChannelKind,
    /// Parsed inbound message
    pub inbound: crate::channels::InboundMessage,
    /// Original raw payload for metadata extraction
    pub raw_payload: serde_json::Value,
    /// Chat mode: "chat_only" or "tool"
    pub feishu_mode: String,
    /// Whether this is chat_only mode
    pub is_chat_only: bool,
    /// Unix timestamp when job was created
    pub created_at: u64,
    /// Job ID for tracking
    pub job_id: String,
    /// Event key for persistence tracking
    pub event_key: String,
    /// Owner-scoped monitor lease acquired before the job is queued.
    /// It contains only a hashed chat key and an opaque owner id.
    pub(crate) monitor_guard_lease: Option<MonitorFlightLease>,
}

impl FeishuAsyncJob {
    /// Create a new async job from webhook data.
    /// If `job_id` is provided, it will be used (for consistency with persisted store).
    /// Otherwise, a new job_id is generated.
    pub fn new(
        channel: ChannelKind,
        inbound: crate::channels::InboundMessage,
        raw_payload: serde_json::Value,
        is_chat_only: bool,
        event_key: String,
        job_id: Option<String>,
    ) -> Self {
        let feishu_mode = if is_chat_only { "chat_only" } else { "tool" };
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let job_id = job_id.unwrap_or_else(|| {
            format!("job_{}_{}", created_at, &uuid::Uuid::new_v4().to_string()[..8])
        });

        Self {
            channel,
            inbound,
            raw_payload,
            feishu_mode: feishu_mode.to_string(),
            is_chat_only,
            created_at,
            job_id,
            event_key,
            monitor_guard_lease: None,
        }
    }

    /// Attach the monitor lease that the worker must renew and release.
    pub(crate) fn with_monitor_guard_lease(mut self, lease: MonitorFlightLease) -> Self {
        self.monitor_guard_lease = Some(lease);
        self
    }
}

/// Job queue sender type
pub type FeishuJobSender = mpsc::Sender<FeishuAsyncJob>;

/// Shared state for the Feishu worker
pub struct FeishuWorkerState {
    /// Job queue sender
    sender: Option<mpsc::Sender<FeishuAsyncJob>>,
    /// Job queue receiver (owned by worker)
    receiver: Option<mpsc::Receiver<FeishuAsyncJob>>,
    /// Current queue length (approximate)
    pub queue_len: Arc<RwLock<usize>>,
}

impl FeishuWorkerState {
    /// Create a new worker state with queue
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<FeishuAsyncJob>(QUEUE_CAPACITY);
        Self {
            sender: Some(sender),
            receiver: Some(receiver),
            queue_len: Arc::new(RwLock::new(0)),
        }
    }
    
    /// Get the sender for enqueuing jobs
    pub fn sender(&self) -> FeishuJobSender {
        self.sender.clone().expect("sender already taken")
    }
    
    /// Take the receiver to pass to the worker
    pub fn take_receiver(&mut self) -> mpsc::Receiver<FeishuAsyncJob> {
        self.receiver.take().expect("receiver already taken")
    }
    
    /// Try to enqueue a job
    pub async fn try_enqueue(&self, job: FeishuAsyncJob) -> Result<(), EnqueueError> {
        let queue_len = self.queue_len.read().await;
        if *queue_len >= QUEUE_CAPACITY {
            return Err(EnqueueError::QueueFull);
        }
        drop(queue_len);
        
        self.sender().send(job).await.map_err(|_| EnqueueError::QueueFull)?;
        
        let mut queue_len = self.queue_len.write().await;
        *queue_len += 1;
        
        Ok(())
    }
}

/// Error when enqueuing fails
#[derive(Debug)]
pub enum EnqueueError {
    /// Queue is full
    QueueFull,
}

/// Check if text contains tool intent (from security context)
fn detect_tool_intent(text: &str) -> Option<String> {
    let tool_intent_patterns = [
        "删除", "删掉", "删去", "新建文件", "写文件", "修改文件", "编辑文件",
        "查看文件", "d盘", "d:", "e盘", "e:", "执行命令", "运行命令",
        "执行脚本", "运行脚本", "git commit", "git push", "监控桌面", "截屏",
        "打开浏览器",
    ];
    
    let text_lower = text.to_lowercase();
    for pattern in tool_intent_patterns {
        if text_lower.contains(&pattern.to_lowercase()) {
            let intent = if pattern.contains("删除") {
                "file_delete"
            } else if pattern.contains("git") {
                "git_operation"
            } else if pattern.contains("监控") || pattern.contains("截屏") {
                "desktop_monitor"
            } else {
                "tool_intent"
            };
            return Some(intent.to_string());
        }
    }
    None
}

/// Get fixed security response for chat_only tool intent
fn chat_only_blocked_response() -> String {
    "当前飞书普通聊天模式不直接执行工具任务。如需处理文件，请发送：/file <任务描述>。删除文件属于高风险操作，需要确认后才能执行。".to_string()
}

// =============================================================================
// Command palette / interactive card
// =============================================================================

/// Check if the inbound text is a menu trigger that should open the command
/// palette card instead of routing through chat_only / tool mode.
pub fn is_menu_trigger(text: &str) -> bool {
    let text_trimmed = text.trim();
    if text_trimmed.is_empty() {
        return false;
    }
    if text_trimmed == "/" {
        return true;
    }
    if text_trimmed == "/功能" || text_trimmed == "/菜单" || text_trimmed == "/帮助" || text_trimmed == "/help" {
        return true;
    }
    let lower = text_trimmed.to_lowercase();
    matches!(lower.as_str(), "菜单" | "帮助" | "help" | "功能" | "menu" | "menu_trigger")
}

/// Allowed action keys for card action callbacks.
/// Anything outside this set is rejected as "unknown action".
pub const ALLOWED_CARD_ACTIONS: &[&str] = &[
    "monitor_30s",
    "monitor_60s",
    "gateway_status",
    "recent_jobs",
    "help",
];

/// Build the command palette interactive card JSON.
pub fn build_command_palette_card() -> serde_json::Value {
    serde_json::json!({
        "config": {
            "wide_screen_mode": true
        },
        "header": {
            "template": "blue",
            "title": {
                "tag": "plain_text",
                "content": "OmniNova Agent 功能菜单"
            }
        },
        "elements": [
            {
                "tag": "div",
                "text": {
                    "tag": "lark_md",
                    "content": "请选择要执行的操作。普通聊天可以直接发送文字；工具任务请使用按钮或 slash 命令。"
                }
            },
            {
                "tag": "hr"
            },
            {
                "tag": "div",
                "text": {
                    "tag": "plain_text",
                    "content": "🟢 普通聊天说明"
                }
            },
            {
                "tag": "action",
                "actions": [
                    {
                        "tag": "button",
                        "text": {
                            "tag": "plain_text",
                            "content": "桌面监控 30 秒"
                        },
                        "type": "primary",
                        "value": {
                            "action": "monitor_30s"
                        }
                    },
                    {
                        "tag": "button",
                        "text": {
                            "tag": "plain_text",
                            "content": "桌面监控 60 秒"
                        },
                        "type": "primary",
                        "value": {
                            "action": "monitor_60s"
                        }
                    }
                ]
            },
            {
                "tag": "action",
                "actions": [
                    {
                        "tag": "button",
                        "text": {
                            "tag": "plain_text",
                            "content": "Gateway 状态"
                        },
                        "type": "default",
                        "value": {
                            "action": "gateway_status"
                        }
                    },
                    {
                        "tag": "button",
                        "text": {
                            "tag": "plain_text",
                            "content": "最近任务"
                        },
                        "type": "default",
                        "value": {
                            "action": "recent_jobs"
                        }
                    },
                    {
                        "tag": "button",
                        "text": {
                            "tag": "plain_text",
                            "content": "帮助说明"
                        },
                        "type": "default",
                        "value": {
                            "action": "help"
                        }
                    }
                ]
            },
            {
                "tag": "note",
                "elements": [
                    {
                        "tag": "plain_text",
                        "content": "高风险工具不在普通聊天中直接执行。"
                    }
                ]
            }
        ]
    })
}

/// Reply text for an "unknown action" rejection.
pub fn unknown_action_reply() -> String {
    "未知操作，已忽略。请发送 / 打开功能菜单。".to_string()
}

/// Help / usage reply text.
pub fn help_reply() -> String {
    let mut s = String::new();
    s.push_str("OmniNova Agent 使用帮助\n\n");
    s.push_str("普通聊天：直接发消息，比如“你好”。\n");
    s.push_str("工具任务：使用 /monitor 桌面 30秒 / /monitor 桌面 60秒，或点击功能菜单中的按钮。\n");
    s.push_str("功能菜单：发送 / 或 菜单 / help / 帮助 / 功能。\n");
    s.push_str("安全说明：高风险工具默认不在飞书普通聊天中直接执行。\n");
    s
}

/// Generate a Gateway status reply text from runtime and security config.
pub fn gateway_status_reply(
    security_mode: Option<&str>,
    verification_token_configured: bool,
    encrypt_key_configured: bool,
    outbound_mode: Option<&str>,
    store_path_exists: bool,
    store_path: &str,
    pending_jobs: i64,
    pending_outbox: i64,
) -> String {
    let security_mode = security_mode.unwrap_or("dev");
    let insecure = matches!(security_mode, "dev") && !verification_token_configured && !encrypt_key_configured;
    let outbound_mode = outbound_mode.unwrap_or("disabled");
    let store_status = if store_path_exists { "ok" } else { "missing" };

    let mut s = String::new();
    s.push_str("Gateway 状态\n\n");
    s.push_str(&format!("gateway running : true\n"));
    s.push_str(&format!("security_mode   : {}\n", security_mode));
    s.push_str(&format!("insecure dev    : {}\n", insecure));
    s.push_str(&format!("verification_token configured : {}\n", verification_token_configured));
    s.push_str(&format!("encrypt_key configured : {}\n", encrypt_key_configured));
    s.push_str(&format!("outbound_mode   : {}\n", outbound_mode));
    s.push_str(&format!("store path      : {}\n", store_path));
    s.push_str(&format!("store status    : {}\n", store_status));
    s.push_str(&format!("pending jobs    : {}\n", pending_jobs));
    s.push_str(&format!("pending outbox  : {}\n", pending_outbox));
    s
}

/// Thin text-only wrapper used by card action handlers when no runtime is available.
pub fn gateway_status_reply_text(
    security_mode: Option<&str>,
    verification_token_configured: bool,
    encrypt_key_configured: bool,
    outbound_mode: Option<&str>,
) -> String {
    gateway_status_reply(
        security_mode,
        verification_token_configured,
        encrypt_key_configured,
        outbound_mode,
        false,
        "(state.sqlite)",
        0,
        0,
    )
}

/// Format recent jobs into a readable text body (max 5 jobs).
pub fn recent_jobs_reply_text(jobs: &[RecentJobLine]) -> String {
    if jobs.is_empty() {
        return "最近 0 条任务：暂无。".to_string();
    }
    let mut s = String::new();
    s.push_str(&format!("最近 {} 条任务（仅摘要，不含 payload）\n\n", jobs.len()));
    for (idx, job) in jobs.iter().enumerate() {
        s.push_str(&format!(
            "{}. {} | mode={} | status={} | attempts={} | error={}\n   created_at={} | completed_at={}\n",
            idx + 1,
            job.job_id_short,
            job.mode,
            job.status,
            job.attempts,
            job.error_code.as_deref().unwrap_or("-"),
            job.created_at,
            job.completed_at.as_deref().unwrap_or("-"),
        ));
    }
    s
}

/// Compact summary of a job, used by `recent_jobs_reply_text`.
#[derive(Debug, Clone)]
pub struct RecentJobLine {
    pub job_id_short: String,
    pub mode: String,
    pub status: String,
    pub attempts: i64,
    pub error_code: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

/// Internal helper: short ID for display.
fn short_id(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        // Keep the tail for ease of cross-reference with logs.
        let total = s.chars().count();
        let skip = total.saturating_sub(max.saturating_sub(1));
        let truncated: String = s.chars().skip(skip).collect();
        format!("…{}", truncated)
    }
}

/// Convert a Feishu job row from the store into a recent-job summary line.
/// Truncates the full job_id to keep the line short.
pub fn summarize_job_for_card(
    job_id_full: &str,
    mode: &str,
    status: &str,
    attempts: i64,
    error_code: Option<&str>,
    created_at: i64,
    completed_at: Option<i64>,
) -> RecentJobLine {
    let created_str = chrono_timestamp_to_iso_ms(created_at);
    let completed_str = completed_at.map(chrono_timestamp_to_iso_ms);
    RecentJobLine {
        job_id_short: short_id(job_id_full, 24),
        mode: mode.to_string(),
        status: status.to_string(),
        attempts,
        error_code: error_code.map(String::from),
        created_at: created_str,
        completed_at: completed_str,
    }
}

/// Convert a Unix ms timestamp into an ISO-8601-ish yyyy-MM-dd HH:mm:ss string.
fn chrono_timestamp_to_iso_ms(ts: i64) -> String {
    let secs = ts / 1000;
    let days_since_epoch = secs / 86400;
    let secs_in_day = secs % 86400;
    let hours = secs_in_day / 3600;
    let mins = (secs_in_day % 3600) / 60;
    let s = secs_in_day % 60;
    let mut remaining_days = days_since_epoch;
    let mut year: i64 = 1970;
    let days_in_year = |y: i64| if is_leap_year_int(y) { 366 } else { 365 };
    while remaining_days >= days_in_year(year) {
        remaining_days -= days_in_year(year);
        year += 1;
    }
    let days_in_month = if is_leap_year_int(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1;
    for (i, d) in days_in_month.iter().enumerate() {
        if remaining_days < *d as i64 {
            month = i + 1;
            break;
        }
        remaining_days -= *d as i64;
    }
    let day = remaining_days + 1;
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, hours, mins, s
    )
}

fn is_leap_year_int(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Get progress reply for long-running tool mode tasks
fn progress_reply_for_command(command: &str) -> String {
    match command {
        "/monitor" => "已收到监控任务，正在执行。完成后我会在这里返回结果。".to_string(),
        "/run" => "已收到执行命令，正在运行。完成后我会返回结果。".to_string(),
        _ => "已收到任务，正在处理中。".to_string(),
    }
}

/// Get timeout reply message
fn timeout_reply() -> String {
    "任务执行超时，当前未完成。请缩短任务范围后重试；如果是监控任务，建议使用较短时长，例如：/monitor 桌面 30 秒。".to_string()
}

/// Get runtime error reply message
fn runtime_error_reply() -> String {
    "任务执行失败，未完成操作。请稍后重试，或在桌面端查看详细日志。".to_string()
}

/// Extract slash command from text
fn extract_slash_command(text: &str) -> Option<String> {
    let text_trimmed = text.trim();
    if text_trimmed.starts_with('/') {
        let parts: Vec<&str> = text_trimmed.split_whitespace().collect();
        if !parts.is_empty() {
            return Some(parts[0].to_string());
        }
    }
    None
}

/// Check if job is a monitor command in tool mode
fn is_monitor_command(job: &FeishuAsyncJob) -> bool {
    if job.is_chat_only {
        return false;
    }
    
    let text_lower = job.inbound.text.to_lowercase();
    text_lower.starts_with("/monitor") || text_lower.starts_with("/monitor ")
}

/// Result of direct monitor runner
#[derive(Debug)]
pub enum MonitorRunnerResult {
    /// Screenshot captured successfully
    Success {
        duration_secs: u64,
        start_capture: CaptureResult,
        end_capture: CaptureResult,
        changed: bool,
        elapsed_ms: u64,
    },
    /// Desktop capture backend not available
    Unsupported {
        reason: String,
    },
    /// Execution failed
    Failed {
        error: String,
    },
}

/// Shell command guard - checks for potentially dangerous or interactive commands
fn validate_shell_command(command: &str) -> Result<(), String> {
    let cmd_lower = command.to_lowercase();
    let cmd_trimmed = cmd_lower.trim();
    
    // Block commands that would prompt for interactive input
    let interactive_patterns = [
        "test-path",
        "read-host",
        "confirm",
        "get-credential",
    ];
    
    for pattern in interactive_patterns {
        if cmd_trimmed.starts_with(pattern) {
            let first_word = command.split_whitespace().next().unwrap_or(command);
            return Err(format!("命令包含交互式提示，已拒绝执行: {}", first_word));
        }
    }
    
    // Block commands with PowerShell interactive prompts
    if cmd_trimmed.contains("请为以下参数提供值") 
        || cmd_trimmed.contains("supply values for the following parameters")
        || (cmd_trimmed.contains("cmdlet") && cmd_trimmed.contains("interactive")) {
        return Err("检测到交互式 PowerShell 命令，已拒绝执行".to_string());
    }
    
    // Block if command is just a cmdlet name without arguments
    let parts: Vec<&str> = command.split_whitespace().collect();
    if let Some(first) = parts.first() {
        let first_lower = first.to_lowercase();
        // Known cmdlets that require parameters
        if first_lower == "test-path" || first_lower == "test-path.exe" {
            // Check if there's a path argument
            if parts.len() < 2 {
                return Err("Test-Path 命令缺少路径参数，已拒绝执行".to_string());
            }
        }
    }
    
    Ok(())
}

/// Get the captures directory path
fn get_captures_dir() -> std::path::PathBuf {
    // Use config directory or default to ~/.omninova/captures
    if let Some(config_dir) = directories::ProjectDirs::from("com", "omninova", "OmniNova") {
        config_dir.config_dir().join("captures")
    } else {
        std::env::temp_dir().join("omninova-captures")
    }
}

/// Run desktop monitoring using the desktop_capture module
async fn run_desktop_monitor(duration_secs: u64) -> MonitorResult {
    let captures_dir = get_captures_dir();
    desktop_capture::monitor_desktop(&captures_dir, duration_secs).await
}

/// Get image dimensions from PNG file
async fn get_image_dimensions(path: &std::path::Path) -> Option<(u32, u32)> {
    let data = tokio::fs::read(path).await.ok()?;
    if data.len() < 24 {
        return None;
    }
    // PNG header: width at offset 16, height at offset 20 (big-endian u32)
    let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    Some((width, height))
}

/// Execute direct monitor command without LLM
pub async fn direct_monitor_runner(
    job: &FeishuAsyncJob,
    _channel_name: &str,
) -> MonitorRunnerResult {
    let job_id = &job.job_id;
    let text = &job.inbound.text;
    
    println!(
        "[feishu-monitor] direct_runner_start job_id={}",
        job_id
    );
    
    // Parse duration from command
    let duration_secs = parse_monitor_duration_from_text(text);
    let capped = duration_secs == MAX_MONITOR_DURATION_SECS && text.to_lowercase().contains("分钟");
    println!(
        "[feishu-monitor] parsed duration_secs={} capped={}",
        duration_secs, capped
    );
    
    // Run desktop monitoring
    let captures_dir = get_captures_dir();
    let result = desktop_capture::monitor_desktop(&captures_dir, duration_secs).await;
    
    // Log results
    if result.ok {
        if let Some(ref start) = result.start_capture {
            println!(
                "[desktop-capture] windows_start job_id={} path={:?} width={} height={} size_bytes={}",
                job_id,
                start.file_path,
                start.width.unwrap_or(0),
                start.height.unwrap_or(0),
                start.file_size_bytes.unwrap_or(0)
            );
        }
        if let Some(ref end) = result.end_capture {
            println!(
                "[desktop-capture] windows_end job_id={} path={:?} width={} height={} size_bytes={}",
                job_id,
                end.file_path,
                end.width.unwrap_or(0),
                end.height.unwrap_or(0),
                end.file_size_bytes.unwrap_or(0)
            );
        }
        if let (Some(changed), Some(method)) = (result.changed, &result.change_method) {
            println!(
                "[feishu-monitor] compare job_id={} changed={} method={}",
                job_id, changed, method
            );
        }
        println!(
            "[feishu-monitor] completed job_id={} duration_ms={}",
            job_id, result.elapsed_ms
        );
    } else {
        println!(
            "[feishu-monitor] failed job_id={} error_code={:?}",
            job_id, result.error_code
        );
    }
    
    // Convert to MonitorRunnerResult
    if result.ok {
        let start = result.start_capture.unwrap();
        let end = result.end_capture.unwrap();
        MonitorRunnerResult::Success {
            duration_secs,
            start_capture: *start,
            end_capture: *end,
            changed: result.changed.unwrap_or(false),
            elapsed_ms: result.elapsed_ms,
        }
    } else {
        MonitorRunnerResult::Failed {
            error: result.message.unwrap_or_else(|| result.error_code.unwrap_or_default()),
        }
    }
}

/// Format monitor result for Feishu reply
fn format_monitor_result(result: &MonitorRunnerResult, duration_secs: u64) -> String {
    match result {
        MonitorRunnerResult::Success { start_capture, end_capture, changed, elapsed_ms, .. } => {
            let start_info = format_capture_info(start_capture);
            let end_info = format_capture_info(end_capture);
            
            let change_text = match changed {
                true => "有变化",
                false => "无明显变化",
            };
            
            format!(
                "桌面监控完成\n\n监控时长：{} 秒\n截图状态：成功\n开始截图：{}\n结束截图：{}\n变化检测：{}\n截图保存位置：\n{}\n{}\n\n注意：变化检测仅基于截图文件哈希，非视觉语义分析。",
                duration_secs,
                start_info,
                end_info,
                change_text,
                start_capture.file_path.as_deref().unwrap_or("未知"),
                end_capture.file_path.as_deref().unwrap_or("未知")
            )
        }
        MonitorRunnerResult::Unsupported { reason } => {
            format!(
                "桌面监控失败：{}\n\n当前飞书 /monitor 命令已识别，但此运行环境尚未接入桌面监控后端。\n请在桌面端启用桌面捕获能力后重试。",
                reason
            )
        }
        MonitorRunnerResult::Failed { error } => {
            format!(
                "桌面监控失败：{}\n\n请检查屏幕录制/截图权限，或缩短监控时长后重试。",
                error
            )
        }
    }
}

/// Format capture info for display
fn format_capture_info(capture: &CaptureResult) -> String {
    if !capture.ok {
        return "失败".to_string();
    }
    
    let dim_str = match (capture.width, capture.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => format!("{}x{}", w, h),
        _ => "未知".to_string(),
    };
    
    let size_str = capture.file_size_bytes
        .map(|s| format!("{:.1} KB", s as f64 / 1024.0))
        .unwrap_or_else(|| "未知".to_string());
    
    format!("{}，{}", dim_str, size_str)
}

/// Parse monitor duration from text
pub(crate) fn parse_monitor_duration_from_text(text: &str) -> u64 {
    let text_lower = text.to_lowercase();
    
    // Manual parsing without regex
    let chars: Vec<char> = text_lower.chars().collect();
    let mut minutes: u64 = 0;
    let mut seconds: u64 = 0;
    
    let mut i = 0;
    while i < chars.len() {
        // Try to parse a number
        let mut num_str = String::new();
        let mut j = i;
        while j < chars.len() && chars[j].is_ascii_digit() {
            num_str.push(chars[j]);
            j += 1;
        }
        
        if !num_str.is_empty() {
            if let Ok(num) = num_str.parse::<u64>() {
                // Skip whitespace
                let mut k = j;
                while k < chars.len() && chars[k].is_whitespace() {
                    k += 1;
                }
                
                // Check for 分 or 分钟
                if k < chars.len() {
                    if chars[k] == '分' {
                        minutes = num;
                        i = k + 1;
                        continue;
                    }
                    // Check for 秒
                    if k + 1 <= chars.len() && k > 0 && chars[k-1] != '分' {
                        if chars[k] == '秒' {
                            seconds = num;
                            i = k + 1;
                            continue;
                        }
                    }
                    // Check for s (English seconds)
                    if k + 1 <= chars.len() && k > 0 && !chars[k-1].is_ascii_digit() {
                        if chars[k] == 's' {
                            seconds = num;
                            i = k + 1;
                            continue;
                        }
                    }
                }
                // No unit found, skip the number
                i = j;
                continue;
            }
        }
        i += 1;
    }
    
    let total_seconds = minutes * 60 + seconds;
    
    // If no duration specified, use default
    if total_seconds == 0 {
        return DEFAULT_MONITOR_DURATION_SECS;
    }
    
    // If duration exceeds max, cap it
    total_seconds.min(MAX_MONITOR_DURATION_SECS)
}

/// Send an outbound reply through outbox with proper timeout, logging and persistence.
/// 
/// # Privacy semantics
/// - `ReplyKind::LlmFinal` (or any free-form LLM reply) is NOT recorded with full body.
///   Such outbox entries are inserted as ABANDONED for audit only.
/// - `ReplyKind::Progress`, `Timeout`, `Failure`, `ChatOnlyBlocked`, `Unsupported`
///   are template replies that can be reconstructed and are recorded as PENDING.
/// - `ReplyKind::MonitorFinal` stores structured result_json for later reconstruction.
async fn send_reply_with_outbox(
    runtime: &GatewayRuntime,
    inbound: &crate::channels::InboundMessage,
    reply: &str,
    channel_name: &str,
    job_id: &str,
    event_key: &str,
    reply_kind: &str,
    timeout_secs: u64,
) {
    let store = match runtime.feishu_store() {
        Some(s) => s,
        None => {
            // Fallback to direct send if no store
            send_reply_with_timeout(runtime, inbound, reply, channel_name, timeout_secs).await;
            return;
        }
    };

    let kind = ReplyKind::from_str(reply_kind);
    
    // Privacy-first: free-form LLM replies are audit-only, never retryable
    if matches!(kind, Some(ReplyKind::LlmFinal) | None) {
        let outbound_id = format!("{}_{}_audit_{}", job_id, reply_kind, chrono_timestamp());
        let chat_id = extract_chat_id(inbound);
        let outbox_input = crate::gateway::feishu_store::FeishuOutboxInput {
            outbound_id: outbound_id.clone(),
            job_id: Some(job_id.to_string()),
            event_key: Some(event_key.to_string()),
            channel: channel_name.to_string(),
            chat_id: chat_id.clone(),
            reply_kind: Some("audit_only".to_string()),
            reply: None,
            result_json: None,
        };
        
        // Attempt to deliver the reply first (we still want the user to get the message)
        let outbound_result = timeout(
            Duration::from_secs(timeout_secs),
            deliver_platform_reply_and_record(runtime, inbound, reply, channel_name)
        ).await;
        
        match outbound_result {
            Ok(Ok(Some(result))) => {
                let platform_msg_id = result.platform_message_id.clone().unwrap_or_default();
                let outbox = crate::gateway::feishu_store::FeishuOutboxInput {
                    outbound_id,
                    job_id: Some(job_id.to_string()),
                    event_key: Some(event_key.to_string()),
                    channel: channel_name.to_string(),
                    chat_id,
                    reply_kind: Some("audit_only".to_string()),
                    reply: Some(reply.to_string()),
                    result_json: None,
                };
                if let Err(e) = store.insert_outbox_abandoned(
                    &outbox,
                    "full_reply_not_stored_for_privacy"
                ) {
                    println!("[{}-outbox] failed to audit: {}", channel_name, e);
                }
                println!(
                    "[{}-outbox] audit outbound_id={} reply_kind=llm_final platform_message_id_present={}",
                    channel_name,
                    outbox.outbound_id,
                    !platform_msg_id.is_empty()
                );
            }
            _ => {
                println!("[{}-outbox] llm_send_failed_audit_only reply_kind={}", channel_name, reply_kind);
            }
        }
        return;
    }

    // Template replies are stored as outbox-able PENDING
    let chat_id = extract_chat_id(inbound);
    let outbound_id = format!("{}_{}_{}", job_id, reply_kind, chrono_timestamp());

    let outbox_input = crate::gateway::feishu_store::FeishuOutboxInput {
        outbound_id: outbound_id.clone(),
        job_id: Some(job_id.to_string()),
        event_key: Some(event_key.to_string()),
        channel: channel_name.to_string(),
        chat_id: chat_id.clone(),
        reply_kind: Some(reply_kind.to_string()),
        reply: None, // Don't store full reply for privacy
        result_json: None,
    };

    if let Err(e) = store.insert_outbox(&outbox_input) {
        println!("[{}-outbox] failed to insert: {}", channel_name, e);
        // Fallback to direct send
        send_reply_with_timeout(runtime, inbound, reply, channel_name, timeout_secs).await;
        return;
    }

    // Update to SENDING
    if let Err(e) = store.outbox_sending(&outbound_id) {
        println!("[{}-outbox] failed to update to sending: {}", channel_name, e);
    }

    // Actually send the message
    let outbound_result = timeout(
        Duration::from_secs(timeout_secs),
        deliver_platform_reply_and_record(runtime, inbound, reply, channel_name)
    ).await;

    match outbound_result {
        Ok(Ok(Some(result))) => {
            // Success - mark as SENT with platform_message_id
            let platform_msg_id = result.platform_message_id.clone().unwrap_or_default();
            if let Err(e) = store.outbox_sent(&outbound_id, &platform_msg_id) {
                println!("[{}-outbox] failed to mark sent: {}", channel_name, e);
            }
            println!(
                "[{}-outbox] sent outbound_id={} platform_message_id_present={}",
                channel_name,
                outbound_id,
                !platform_msg_id.is_empty()
            );
        }
        Ok(Ok(None)) => {
            // Skipped (no target)
            if let Err(e) = store.outbox_sent(&outbound_id, "skipped") {
                println!("[{}-outbox] failed to mark skipped: {}", channel_name, e);
            }
            println!("[{}-outbox] skipped outbound_id={}", channel_name, outbound_id);
        }
        Ok(Err(e)) => {
            // Error during send
            let retryable = store.outbox_failed(&outbound_id, "send_error", &e.to_string()).unwrap_or(false);
            println!(
                "[{}-outbox] failed outbound_id={} error={} retryable={}",
                channel_name, outbound_id, e, retryable
            );
        }
        Err(_) => {
            // Timeout
            let retryable = store.outbox_failed(&outbound_id, "timeout", &format!("Send timeout after {}s", timeout_secs)).unwrap_or(false);
            println!(
                "[{}-outbox] timeout outbound_id={} timeout_secs={} retryable={}",
                channel_name, outbound_id, timeout_secs, retryable
            );
        }
    }
}

/// Extract chat_id from inbound metadata
fn extract_chat_id(inbound: &crate::channels::InboundMessage) -> Option<String> {
    inbound.session_id.clone()
        .or_else(|| inbound.metadata.get("chat_id").and_then(|v| v.as_str()).map(String::from))
        .or_else(|| inbound.metadata.get("message_chat_id").and_then(|v| v.as_str()).map(String::from))
        .or_else(|| inbound.metadata.get("conversation_id").and_then(|v| v.as_str()).map(String::from))
}

// =============================================================================
// Interactive card sending (command palette / card actions)
// =============================================================================

/// Send the command palette card. We persist a brief audit row but never
/// store the full card JSON. The card can be reconstructed from
/// `reply_kind=command_palette_card`.
async fn send_command_palette_card(
    runtime: &GatewayRuntime,
    inbound: &crate::channels::InboundMessage,
    channel_name: &str,
    job_id: &str,
    event_key: &str,
) {
    let card = build_command_palette_card();
    let card_chars = card.to_string().chars().count();

    println!("[feishu-card] send_palette_start card_chars={}", card_chars);

    let store_opt = runtime.feishu_store();
    let config = runtime.get_config().await;

    let outbound_id = format!("{}_{}_palette_{}", job_id, chrono_timestamp(), chrono_timestamp());

    if let Some(ref store) = store_opt {
        let chat_id = extract_chat_id(inbound);
        let outbox_input = crate::gateway::feishu_store::FeishuOutboxInput {
            outbound_id: outbound_id.clone(),
            job_id: Some(job_id.to_string()),
            event_key: Some(event_key.to_string()),
            channel: channel_name.to_string(),
            chat_id,
            reply_kind: Some(ReplyKind::CommandPaletteCard.as_str().to_string()),
            reply: None,
            result_json: None,
        };
        let _ = store.insert_outbox(&outbox_input);
        let _ = store.outbox_sending(&outbound_id);
    }

    match crate::gateway::deliver_interactive_card(&config, inbound, &card).await {
        Ok(result) => {
            let pm_id = result.platform_message_id.clone().unwrap_or_default();
            if let Some(ref store) = store_opt {
                let _ = store.outbox_sent(&outbound_id, &pm_id);
            }
            println!(
                "[{}-card] palette_sent outbound_id={} platform_message_id_present={}",
                channel_name,
                outbound_id,
                !pm_id.is_empty()
            );
        }
        Err(e) => {
            if let Some(ref store) = store_opt {
                let _ = store.outbox_failed(&outbound_id, "card_send_failed", &e);
            }
            println!(
                "[{}-card] palette_failed outbound_id={} error={}",
                channel_name, outbound_id, e
            );
        }
    }

    println!("[feishu-card] send_palette_ok");
}

/// Send a card action result reply (e.g. status text, jobs list).
/// Persists a short reply_preview; never stores full payload.
pub(crate) async fn send_card_action_result(
    runtime: &GatewayRuntime,
    inbound: &crate::channels::InboundMessage,
    text: &str,
    reply_kind: ReplyKind,
    channel_name: &str,
    job_id: &str,
    event_key: &str,
) {
    send_reply_with_outbox(
        runtime,
        inbound,
        text,
        channel_name,
        job_id,
        event_key,
        reply_kind.as_str(),
        OUTBOUND_TIMEOUT_SECS,
    )
    .await;
}

/// Try to dispatch a card action callback into an existing handling pipeline.
/// Returns true only if the action is in the allow-list.
pub fn resolve_card_action(action: &str) -> bool {
    ALLOWED_CARD_ACTIONS.contains(&action)
}

/// Return the canonical action key (string) for an action string.
/// Used as an audit field; never logs the full callback payload.
pub fn canonical_card_action(action: &str) -> Option<&'static str> {
    ALLOWED_CARD_ACTIONS
        .iter()
        .copied()
        .find(|a| *a == action)
}

/// Construct a ReplyTarget from inbound metadata. Used by card sending.
pub fn build_reply_target(
    inbound: &crate::channels::InboundMessage,
    channel_name: &str,
) -> Option<crate::channels::adapters::outbound::ReplyTarget> {
    let chat_id = extract_chat_id(inbound)?;
    let channel = match channel_name {
        "feishu" => ChannelKind::Feishu,
        "lark" => ChannelKind::Lark,
        _ => inbound.channel.clone(),
    };
    Some(crate::channels::adapters::outbound::ReplyTarget {
        channel,
        chat_id,
        message_id: inbound.metadata.get("message_id").and_then(|v| v.as_str()).map(String::from),
        user_id: inbound.user_id.clone(),
    })
}

/// Determine if a Feishu inbound text routed from the webhook should open
/// the command palette instead of going through chat / tool runtime.
pub fn should_open_palette(text: &str) -> bool {
    is_menu_trigger(text)
}

/// Reconstruct a reply body from its reply_kind and result_json.
/// Returns Some(reply) if the reply can be reconstructed, None otherwise.
pub fn reconstruct_reply(kind: ReplyKind, result_json: Option<&str>, chat_only_intent: Option<&str>) -> Option<String> {
    match kind {
        ReplyKind::Progress => Some(format!(
            "已收到监控任务，正在执行。完成后我会在这里返回结果。"
        )),
        ReplyKind::Timeout => Some(format!(
            "抱歉，任务执行超时。请缩短任务时长后重试。"
        )),
        ReplyKind::Failure => Some(format!(
            "任务执行失败，请稍后重试。如果问题持续，请联系管理员。"
        )),
        ReplyKind::ChatOnlyBlocked => Some(format!(
            "当前消息触发了安全限制。请使用 /monitor、/run、/file、/workspace、/agent 等命令明确表达工具意图。"
        )),
        ReplyKind::Unsupported => Some(format!(
            "当前消息类型暂不支持自动处理。"
        )),
        ReplyKind::MonitorFinal => {
            // Reconstruct from result_json (e.g., /monitor results)
            result_json.and_then(|json| {
                serde_json::from_str::<serde_json::Value>(json).ok()
            }).map(|result| {
                // Reconstruct monitor reply from structured data
                let changed_str = if result.get("changed").and_then(|v| v.as_bool()).unwrap_or(false) {
                    "有变化"
                } else {
                    "无明显变化"
                };
                let duration = result.get("duration_secs").and_then(|v| v.as_u64()).unwrap_or(30);
                let start_path = result.get("start_path").and_then(|v| v.as_str()).unwrap_or("(unknown)");
                let end_path = result.get("end_path").and_then(|v| v.as_str()).unwrap_or("(unknown)");
                format!(
                    "桌面监控完成\n监控时长：{} 秒\n截图状态：成功\n变化检测：{}\n开始截图：{}\n结束截图：{}",
                    duration, changed_str, start_path, end_path
                )
            })
        }
        ReplyKind::LlmFinal => None, // Free-form LLM reply cannot be reconstructed
        ReplyKind::CommandPaletteCard => Some(
            "[card] OmniNova Agent 功能菜单 - 重建（详细卡片未持久化）".to_string()
        ),
        ReplyKind::CardActionResult => {
            // The card action result is a plain text reply, reconstructed from result_json if present
            result_json.and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
                .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(String::from))
        }
        ReplyKind::GatewayStatusReply => result_json.and_then(|j| {
            serde_json::from_str::<serde_json::Value>(j).ok()
                .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(String::from))
        }),
        ReplyKind::RecentJobsReply => result_json.and_then(|j| {
            serde_json::from_str::<serde_json::Value>(j).ok()
                .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(String::from))
        }),
    }
}

/// Legacy direct send without outbox (for fallback only)
async fn send_reply_with_timeout(
    runtime: &GatewayRuntime,
    inbound: &crate::channels::InboundMessage,
    reply: &str,
    channel_name: &str,
    timeout_secs: u64,
) {
    let outbound_result = timeout(
        Duration::from_secs(timeout_secs),
        deliver_platform_reply_and_record(runtime, inbound, reply, channel_name)
    ).await;
    
    match outbound_result {
        Ok(Ok(Some(result))) => {
            println!(
                "[{}-worker] outbound_ok reply_len={} outbound_result={:?}",
                channel_name, reply.len(), result
            );
        }
        Ok(Ok(None)) => {
            println!(
                "[{}-worker] outbound_skipped reply_len={}",
                channel_name, reply.len()
            );
        }
        Ok(Err(e)) => {
            println!(
                "[{}-worker] outbound_failed error={}",
                channel_name, e
            );
        }
        Err(_) => {
            println!(
                "[{}-worker] outbound_timeout timeout_secs={}",
                channel_name, timeout_secs
            );
        }
    }
}

/// Process a single job in the background worker
pub async fn process_feishu_job(
    job: FeishuAsyncJob,
    runtime: GatewayRuntime,
) {
    let started_at = Instant::now();
    let channel_name = format!("{:?}", job.channel).to_lowercase();
    let job_id = &job.job_id;
    let event_key = &job.event_key;
    
    // Get instance ID for locking
    let instance_id = format!("pid_{}", std::process::id());
    
    // Update job to PROCESSING status
    if let Some(ref store) = runtime.feishu_store() {
        if let Err(e) = store.job_start_processing(&job.job_id, &instance_id) {
            println!("[{}-worker] failed to update job status: {}", channel_name, e);
        }
    }
    
    // Extract reply targets from raw payload (for logging only)
    let message_id_present = job.raw_payload.get("event")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.get("message_id"))
        .is_some();
    
    let chat_id_present = job.raw_payload.get("event")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.get("chat_id"))
        .is_some()
        || job.raw_payload.get("event")
            .and_then(|e| e.get("chat_id"))
            .is_some();
    
    let text_len = job.inbound.text.len();
    
    println!(
        "[{}-worker] job_started job_id={} event_id_present={} message_id_present={} chat_id_present={} mode={}",
        channel_name,
        job_id,
        job.raw_payload.get("header").and_then(|h| h.get("event_id")).is_some(),
        message_id_present,
        chat_id_present,
        job.feishu_mode
    );
    
    // Command palette dispatch: triggers like "/", "菜单", "帮助", "help", "功能"
    // are NOT routed through chat_only / tool runtime. We send the
    // interactive card and mark the job as completed.
    if is_menu_trigger(&job.inbound.text) {
        println!(
            "[feishu-router] mode=command_palette reason=menu_trigger job_id={} text_len={}",
            job_id, text_len
        );
        send_command_palette_card(&runtime, &job.inbound, &channel_name, job_id, event_key).await;
        if let Some(ref store) = runtime.feishu_store() {
            let _ = store.job_completed(job_id);
            let _ = store.update_event_status(event_key, EventStatus::Processed);
        }
        let duration_ms = started_at.elapsed().as_millis() as u64;
        println!(
            "[{}-worker] job_completed job_id={} duration_ms={} status=command_palette",
            channel_name, job_id, duration_ms
        );
        return;
    }
    
    // Check for tool intent in chat_only mode
    if job.is_chat_only {
        if let Some(intent) = detect_tool_intent(&job.inbound.text) {
            println!(
                "[{}-worker] short_circuit job_id={} intent={} text_len={}",
                channel_name, job_id, intent, text_len
            );
            
            let blocked_reply = chat_only_blocked_response();
            send_reply_with_outbox(&runtime, &job.inbound, &blocked_reply, &channel_name, job_id, event_key, "blocked", OUTBOUND_TIMEOUT_SECS).await;
            
            // Mark job and event as completed
            if let Some(ref store) = runtime.feishu_store() {
                let _ = store.job_completed(job_id);
                let _ = store.update_event_status(event_key, EventStatus::Processed);
            }
            
            let duration_ms = started_at.elapsed().as_millis() as u64;
            println!(
                "[{}-worker] job_completed job_id={} duration_ms={} status=short_circuit",
                channel_name, job_id, duration_ms
            );
            return;
        }
    }
    
    // For tool mode /monitor command, use direct runner instead of Runtime
    if is_monitor_command(&job) {
        let duration_secs = parse_monitor_duration_from_text(&job.inbound.text);
        let ttl_secs = crate::gateway::monitor_guard_ttl_secs(duration_secs);
        let guard = runtime.monitor_flight_guard();
        let guard_source = if job.inbound.metadata.contains_key("card_action") {
            "card"
        } else {
            "slash"
        };
        let chat_present = job.inbound.session_id.is_some();

        // Normal webhook paths acquire before enqueueing. The fallback acquisition
        // keeps direct/unit-created jobs safe without allowing card jobs to acquire twice.
        let active_guard_lease = if let Some(lease) = job.monitor_guard_lease.clone() {
            guard.renew(&lease).await.then_some(lease)
        } else if let Some(chat_id) = job.inbound.session_id.as_deref() {
            let acquired = guard.try_acquire_with_ttl(chat_id, ttl_secs).await;
            if acquired.is_some() {
                println!(
                    "[{}-monitor] singleflight_acquired source={} command=/monitor chat_present=true ttl_secs={}",
                    channel_name, guard_source, ttl_secs
                );
            }
            acquired
        } else {
            None
        };

        // ==== Guard busy: short-circuit without running ====
        if chat_present && active_guard_lease.is_none() {
            println!(
                "[{}-monitor] singleflight_busy source={} command=/monitor chat_present=true",
                channel_name, guard_source
            );
            let busy_reply = "已有桌面监控任务正在执行，请等待当前任务完成后再试。";
            let _ = send_reply_with_outbox(
                &runtime, &job.inbound, busy_reply, &channel_name, job_id, event_key,
                "busy", OUTBOUND_TIMEOUT_SECS
            ).await;
            if let Some(ref store) = runtime.feishu_store() {
                let _ = store.job_completed(job_id);
                let _ = store.update_event_status(event_key, EventStatus::Processed);
            }
            println!(
                "[{}-worker] job_completed job_id={} duration_ms={} status=skipped_guard_busy",
                channel_name, job_id, started_at.elapsed().as_millis() as u64
            );
            return;
        }

        println!(
            "[{}-worker] direct_monitor job_id={} text_len={} guard_source={}",
            channel_name, job_id, text_len, guard_source
        );

        // Send progress reply
        let progress_reply = progress_reply_for_command("/monitor");
        send_reply_with_outbox(
            &runtime, &job.inbound, &progress_reply, &channel_name,
            job_id, event_key, "progress", OUTBOUND_TIMEOUT_SECS
        ).await;

        // Queueing and the progress reply can consume part of the lease. Renew
        // immediately before capture so the full monitor duration remains
        // protected. If the owner has expired or been replaced, this stale job
        // must not start a second capture.
        if let Some(ref lease) = active_guard_lease {
            if !guard.renew(lease).await {
                println!(
                    "[{}-monitor] singleflight_busy source={} command=/monitor chat_present=true reason=lease_expired_before_capture",
                    channel_name, guard_source
                );
                let busy_reply = "已有桌面监控任务正在执行，请等待当前任务完成后再试。";
                let _ = send_reply_with_outbox(
                    &runtime, &job.inbound, busy_reply, &channel_name, job_id, event_key,
                    "busy", OUTBOUND_TIMEOUT_SECS
                ).await;
                if let Some(ref store) = runtime.feishu_store() {
                    let _ = store.job_completed(job_id);
                    let _ = store.update_event_status(event_key, EventStatus::Processed);
                }
                println!(
                    "[{}-worker] job_completed job_id={} duration_ms={} status=skipped_guard_expired",
                    channel_name, job_id, started_at.elapsed().as_millis() as u64
                );
                return;
            }
        }

        // Run direct monitor
        let result = direct_monitor_runner(&job, &channel_name).await;

        // Format and send result
        let reply = format_monitor_result(&result, duration_secs);
        send_reply_with_outbox(
            &runtime, &job.inbound, &reply, &channel_name,
            job_id, event_key, "final", OUTBOUND_TIMEOUT_SECS
        ).await;

        // ==== Release monitor single-flight guard ====
        if let Some(ref lease) = active_guard_lease {
            let released = guard.release(lease).await;
            let monitor_succeeded = matches!(result, MonitorRunnerResult::Success { .. });
            println!(
                "[{}-monitor] singleflight_released source=worker job_id={} success={} released={}",
                channel_name, job_id, monitor_succeeded, released
            );
        }

        // Mark job and event as completed
        if let Some(ref store) = runtime.feishu_store() {
            let _ = store.job_completed(job_id);
            let _ = store.update_event_status(event_key, EventStatus::Processed);
        }

        let duration_ms = started_at.elapsed().as_millis() as u64;
        let status = match &result {
            MonitorRunnerResult::Success { .. } => "success",
            MonitorRunnerResult::Unsupported { .. } => "unsupported",
            MonitorRunnerResult::Failed { .. } => "failed",
        };
        println!(
            "[{}-worker] job_completed job_id={} duration_ms={} status={}",
            channel_name, job_id, duration_ms, status
        );

        return;
    }
    
    // For other tool mode commands, send progress reply
    let slash_command = extract_slash_command(&job.inbound.text);
    let should_send_progress = !job.is_chat_only && slash_command.is_some();
    
    if should_send_progress {
        if let Some(ref command) = slash_command {
            let progress_reply = progress_reply_for_command(command);
            println!(
                "[{}-worker] progress_reply_start job_id={} command={}",
                channel_name, job_id, command
            );
            send_reply_with_outbox(&runtime, &job.inbound, &progress_reply, &channel_name, job_id, event_key, "progress", OUTBOUND_TIMEOUT_SECS).await;
            println!(
                "[{}-worker] progress_reply_sent job_id={} command={}",
                channel_name, job_id, command
            );
        }
    }
    
    // Process through Runtime with timeout
    println!("[{}-worker] runtime_start job_id={}", channel_name, job_id);
    
    let runtime_result = timeout(
        Duration::from_secs(RUNTIME_TIMEOUT_SECS),
        runtime.process_inbound(&job.inbound)
    ).await;
    
    let reply = match runtime_result {
        Ok(Ok(response)) => {
            println!(
                "[{}-worker] runtime_done job_id={} reply_len={}",
                channel_name, job_id, response.reply.len()
            );
            Some(response.reply)
        }
        Ok(Err(e)) => {
            println!(
                "[{}-worker] runtime_failed job_id={} error={}",
                channel_name, job_id, e
            );
            
            // Send error reply
            let error_reply = runtime_error_reply();
            println!(
                "[{}-worker] failure_reply_start job_id={}",
                channel_name, job_id
            );
            send_reply_with_outbox(&runtime, &job.inbound, &error_reply, &channel_name, job_id, event_key, "failure", OUTBOUND_TIMEOUT_SECS).await;
            println!(
                "[{}-worker] failure_reply_sent job_id={}",
                channel_name, job_id
            );
            
            // Mark job as failed
            if let Some(ref store) = runtime.feishu_store() {
                let _ = store.job_failed(job_id, "runtime_error", &format!("{}", e));
            }
            
            let duration_ms = started_at.elapsed().as_millis() as u64;
            println!(
                "[{}-worker] job_completed job_id={} duration_ms={} status=runtime_failed",
                channel_name, job_id, duration_ms
            );
            return;
        }
        Err(_) => {
            println!(
                "[{}-worker] runtime_timeout job_id={} timeout_secs={}",
                channel_name, job_id, RUNTIME_TIMEOUT_SECS
            );
            
            // Send timeout reply
            let timeout_reply_msg = timeout_reply();
            println!(
                "[{}-worker] timeout_reply_start job_id={}",
                channel_name, job_id
            );
            send_reply_with_outbox(&runtime, &job.inbound, &timeout_reply_msg, &channel_name, job_id, event_key, "timeout", OUTBOUND_TIMEOUT_SECS).await;
            println!(
                "[{}-worker] timeout_reply_sent job_id={}",
                channel_name, job_id
            );
            
            // Mark job as failed (timeout)
            if let Some(ref store) = runtime.feishu_store() {
                let _ = store.job_failed(job_id, "timeout", &format!("Runtime timeout after {}s", RUNTIME_TIMEOUT_SECS));
            }
            
            let duration_ms = started_at.elapsed().as_millis() as u64;
            println!(
                "[{}-worker] job_completed job_id={} duration_ms={} status=timeout",
                channel_name, job_id, duration_ms
            );
            return;
        }
    };
    
    // Handle successful runtime response
    if let Some(reply) = reply {
        if reply.trim().is_empty() {
            println!("[{}-worker] skipped_empty_reply job_id={}", channel_name, job_id);
            
            // Mark job and event as completed
            if let Some(ref store) = runtime.feishu_store() {
                let _ = store.job_completed(job_id);
                let _ = store.update_event_status(event_key, EventStatus::Processed);
            }
            
            let duration_ms = started_at.elapsed().as_millis() as u64;
            println!(
                "[{}-worker] job_completed job_id={} duration_ms={}",
                channel_name, job_id, duration_ms
            );
            return;
        }
        
        // Send reply via outbound
        println!("[{}-worker] outbound_start job_id={} reply_len={}", channel_name, job_id, reply.len());
        send_reply_with_outbox(&runtime, &job.inbound, &reply, &channel_name, job_id, event_key, "final", OUTBOUND_TIMEOUT_SECS).await;
        
        // Mark job and event as completed
        if let Some(ref store) = runtime.feishu_store() {
            let _ = store.job_completed(job_id);
            let _ = store.update_event_status(event_key, EventStatus::Processed);
        }
    }
    
    let duration_ms = started_at.elapsed().as_millis() as u64;
    println!(
        "[{}-worker] job_completed job_id={} duration_ms={}",
        channel_name, job_id, duration_ms
    );
}

/// Deliver reply and record outbound message_id
async fn deliver_platform_reply_and_record(
    runtime: &GatewayRuntime,
    inbound: &crate::channels::InboundMessage,
    reply: &str,
    channel_name: &str,
) -> anyhow::Result<Option<OutboundResult>> {
    use crate::gateway::deliver_platform_reply;
    
    let result = deliver_platform_reply(
        &runtime.get_config().await,
        inbound,
        reply,
    ).await;
    
    // Record outbound message_id for self-message filtering
    if let Some(ref outbound_result) = result {
        if let Some(ref msg_id) = outbound_result.platform_message_id {
            OutboundMsgCache::global().record_outbound(channel_name, msg_id).await;
            println!(
                "[{}-worker] recorded_outbound_message_id=true",
                channel_name
            );
        }
    }
    
    Ok(result)
}

/// Spawn the worker task that processes jobs from the queue
/// Jobs are dispatched to a worker pool to avoid blocking
pub fn spawn_worker(
    mut receiver: mpsc::Receiver<FeishuAsyncJob>,
    runtime: GatewayRuntime,
    queue_len: Arc<RwLock<usize>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Worker pool semaphore to limit concurrent jobs
        let semaphore = Arc::new(tokio::sync::Semaphore::new(WORKER_CONCURRENCY));
        let runtime = Arc::new(runtime);
        
        // Spawn worker tasks that will process jobs concurrently
        let mut handles = Vec::new();
        
        // Main worker loop - dispatches jobs to the pool
        while let Some(job) = receiver.recv().await {
            // Decrement queue length
            {
                let mut len = queue_len.write().await;
                if *len > 0 {
                    *len -= 1;
                }
            }
            
            // Clone Arc values for the spawned task
            let job_id = job.job_id.clone();
            let channel_name = format!("{:?}", job.channel).to_lowercase();
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let runtime = runtime.clone();
            
            println!(
                "[{}-worker] dispatched job_id={} active_workers={}",
                channel_name, job_id, WORKER_CONCURRENCY - semaphore.available_permits()
            );
            
            // Spawn a task to process this job
            let handle = tokio::spawn(async move {
                // Process the job
                process_feishu_job(job, runtime.as_ref().clone()).await;
                // Release permit when done
                drop(permit);
            });
            
            handles.push(handle);
            
            // Clean up completed handles periodically to avoid memory buildup
            if handles.len() > 20 {
                handles.retain(|h| !h.is_finished());
            }
        }
        
        // Wait for all remaining jobs to complete
        for handle in handles {
            let _ = handle.await;
        }
    })
}

/// Run the retry/recovery worker once on startup.
/// Scans retryable outbox and re-sends them.
/// This is meant to be called once at Gateway startup.
pub async fn run_retry_worker_once(runtime: &GatewayRuntime) {
    println!("[feishu-retry] started");
    
    let store = match runtime.feishu_store() {
        Some(s) => s,
        None => {
            println!("[feishu-retry] no_store_available");
            return;
        }
    };
    
    // Get all recoverable outbox items
    let recoverable = match store.get_recoverable_outbox() {
        Ok(items) => items,
        Err(e) => {
            println!("[feishu-retry] failed to get recoverable: {}", e);
            return;
        }
    };
    
    println!("[feishu-retry] scan count={}", recoverable.len());
    
    let mut retryable_count = 0;
    let mut abandoned_count = 0;
    let mut failed_count = 0;
    let mut sent_count = 0;
    
    for item in recoverable {
        let kind = item.reply_kind.as_deref()
            .and_then(ReplyKind::from_str);
        
        match kind {
            // Audit-only kinds: cannot reconstruct body, mark abandoned
            Some(ReplyKind::LlmFinal) | None => {
                println!(
                    "[feishu-retry] abandoned outbound_id={} reason=privacy_no_full_body",
                    item.outbound_id
                );
                let _ = store.outbox_mark_failed_privacy(
                    &item.outbound_id,
                    "llm_reply_not_reconstructible"
                );
                abandoned_count += 1;
            }
            
            // Retryable template / restructurable kinds
            Some(k) => {
                // Begin retry: increment attempts, set SENDING
                let can_retry = match store.outbox_begin_retry(&item.outbound_id) {
                    Ok(v) => v,
                    Err(e) => {
                        println!("[feishu-retry] begin_retry_error outbound_id={} error={}", item.outbound_id, e);
                        failed_count += 1;
                        continue;
                    }
                };
                
                if !can_retry {
                    // Either already SENT or max attempts exceeded
                    let _status = store.get_outbox(&item.outbound_id)
                        .ok()
                        .flatten()
                        .map(|o| o.status);
                    println!("[feishu-retry] skip outbound_id={} reason=cannot_retry_or_sent", item.outbound_id);
                    continue;
                }
                
                retryable_count += 1;
                
                // Try to reconstruct the reply body
                let reconstructed = reconstruct_reply(k, item.result_json.as_deref(), None);
                
                let reply_body = match reconstructed {
                    Some(s) => s,
                    None => {
                        // Cannot reconstruct (missing result_json or required fields)
                        println!(
                            "[feishu-retry] reconstruct_incomplete outbound_id={} reply_kind={}",
                            item.outbound_id,
                            item.reply_kind.as_deref().unwrap_or("?")
                        );
                        let _ = store.outbox_mark_reconstruct_incomplete(
                            &item.outbound_id,
                            "missing_result_json_or_required_fields"
                        );
                        failed_count += 1;
                        continue;
                    }
                };
                
                // Reconstruct inbound from outbox metadata
                let inbound = match reconstruct_inbound_for_retry(&item) {
                    Some(i) => i,
                    None => {
                        println!("[feishu-retry] cannot_reconstruct_inbound outbound_id={}", item.outbound_id);
                        let _ = store.outbox_mark_reconstruct_incomplete(
                            &item.outbound_id,
                            "missing_chat_id_for_retry"
                        );
                        failed_count += 1;
                        continue;
                    }
                };
                
                // Send via existing pipeline
                let channel_name = item.channel.as_str();
                let send_result = timeout(
                    Duration::from_secs(OUTBOUND_TIMEOUT_SECS),
                    deliver_platform_reply_and_record(runtime, &inbound, &reply_body, channel_name)
                ).await;
                
                match send_result {
                    Ok(Ok(Some(result))) => {
                        let platform_msg_id = result.platform_message_id.clone().unwrap_or_default();
                        if let Err(e) = store.outbox_sent(&item.outbound_id, &platform_msg_id) {
                            println!("[feishu-retry] mark_sent_error outbound_id={} error={}", item.outbound_id, e);
                        } else {
                            println!(
                                "[feishu-retry] sent outbound_id={} platform_message_id_present={}",
                                item.outbound_id,
                                !platform_msg_id.is_empty()
                            );
                            sent_count += 1;
                        }
                    }
                    Ok(Ok(None)) => {
                        // Disabled / skipped - mark sent as skipped
                        let _ = store.outbox_sent(&item.outbound_id, "skipped");
                        println!("[feishu-retry] skipped outbound_id={} reason=disabled_or_skipped", item.outbound_id);
                    }
                    Ok(Err(e)) => {
                        let retryable = store.outbox_failed(
                            &item.outbound_id,
                            "send_error",
                            &e.to_string()
                        ).unwrap_or(false);
                        println!(
                            "[feishu-retry] failed outbound_id={} retryable={} error={}",
                            item.outbound_id,
                            retryable,
                            e
                        );
                        failed_count += 1;
                    }
                    Err(_) => {
                        let retryable = store.outbox_failed(
                            &item.outbound_id,
                            "timeout",
                            &format!("Retry send timeout after {}s", OUTBOUND_TIMEOUT_SECS)
                        ).unwrap_or(false);
                        println!(
                            "[feishu-retry] timeout outbound_id={} retryable={}",
                            item.outbound_id, retryable
                        );
                        failed_count += 1;
                    }
                }
            }
        }
    }
    
    println!(
        "[feishu-retry] completed sent={} retryable={} abandoned={} failed={}",
        sent_count, retryable_count, abandoned_count, failed_count
    );
}

/// Reconstruct a minimal InboundMessage from a persisted outbox for retry.
/// Only carries chat_id and channel - enough for `deliver_platform_reply`.
fn reconstruct_inbound_for_retry(item: &crate::gateway::feishu_store::FeishuOutbox) -> Option<crate::channels::InboundMessage> {
    use crate::channels::{ChannelKind, InboundMessage};
    use std::collections::HashMap;
    
    let chat_id = item.chat_id.clone()
        .filter(|s| !s.is_empty())?;
    
    let channel = match item.channel.as_str() {
        "feishu" => ChannelKind::Feishu,
        "lark" => ChannelKind::Lark,
        _ => return None,
    };
    
    let mut metadata = HashMap::new();
    metadata.insert(
        "chat_id".to_string(),
        serde_json::Value::String(chat_id.clone()),
    );
    metadata.insert(
        "conversation_id".to_string(),
        serde_json::Value::String(chat_id.clone()),
    );
    if let Some(event_key) = &item.event_key {
        metadata.insert(
            "event_key".to_string(),
            serde_json::Value::String(event_key.clone()),
        );
    }
    
    Some(InboundMessage {
        channel,
        user_id: None,
        session_id: Some(chat_id),
        text: String::new(),
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::{ChannelKind, InboundMessage};

    fn make_test_job(text: &str, is_chat_only: bool) -> FeishuAsyncJob {
        FeishuAsyncJob {
            channel: ChannelKind::Feishu,
            inbound: InboundMessage {
                text: text.to_string(),
                channel: ChannelKind::Feishu,
                user_id: None,
                session_id: None,
                metadata: std::collections::HashMap::new(),
            },
            raw_payload: serde_json::json!({}),
            feishu_mode: if is_chat_only { "chat_only".to_string() } else { "tool".to_string() },
            is_chat_only,
            created_at: 0,
            job_id: "test".to_string(),
            event_key: "test:event_key".to_string(),
            monitor_guard_lease: None,
        }
    }

    // Helper to test duration parsing directly
    fn test_duration(input: &str, expected: u64) {
        let result = parse_monitor_duration_from_text(input);
        assert_eq!(result, expected, "parse_duration(\"{}\") = {} but expected {}", input, result, expected);
    }

    #[test]
    fn test_parse_no_duration_defaults_to_30() {
        // No duration specified, should default to 30
        test_duration("/monitor desktop", 30);
        test_duration("/monitor", 30);
    }

    #[test]
    fn test_parse_30_seconds() {
        test_duration("/monitor 30秒", 30);
        test_duration("/monitor 30s", 30);
    }

    #[test]
    fn test_parse_1_minute() {
        // 1分钟 = 60 seconds
        test_duration("/monitor 1分钟", 60);
    }

    #[test]
    fn test_parse_2_minutes_capped() {
        // 2分钟 = 120 seconds, capped to 60
        test_duration("/monitor 2分钟", 60);
    }

    #[test]
    fn test_parse_120_seconds_capped() {
        // 120秒 capped to 60
        test_duration("/monitor 120秒", 60);
    }

    #[test]
    fn test_extract_slash_command() {
        assert_eq!(extract_slash_command("/monitor desktop"), Some("/monitor".to_string()));
        assert_eq!(extract_slash_command("/run ls -la"), Some("/run".to_string()));
        assert_eq!(extract_slash_command("hello"), None);
    }

    #[test]
    fn test_is_monitor_command() {
        let monitor_job = make_test_job("/monitor desktop 30秒", false);
        assert!(is_monitor_command(&monitor_job));

        let chat_only_job = make_test_job("/monitor desktop 30秒", true);
        assert!(!is_monitor_command(&chat_only_job));

        let hello_job = make_test_job("hello", false);
        assert!(!is_monitor_command(&hello_job));
    }
    
    // ========== Retry worker reconstruction tests (v0.8.7.5.2) ==========
    
    #[test]
    fn test_reconstruct_reply_progress() {
        let result = reconstruct_reply(ReplyKind::Progress, None, None);
        assert!(result.is_some());
        let s = result.unwrap();
        assert!(s.contains("已收到监控任务") || s.contains("监控"));
    }
    
    #[test]
    fn test_reconstruct_reply_timeout() {
        let result = reconstruct_reply(ReplyKind::Timeout, None, None);
        assert!(result.is_some());
        let s = result.unwrap();
        assert!(s.contains("超时"));
    }
    
    #[test]
    fn test_reconstruct_reply_failure() {
        let result = reconstruct_reply(ReplyKind::Failure, None, None);
        assert!(result.is_some());
        let s = result.unwrap();
        assert!(s.contains("失败"));
    }
    
    #[test]
    fn test_reconstruct_reply_chat_only_blocked() {
        let result = reconstruct_reply(ReplyKind::ChatOnlyBlocked, None, None);
        assert!(result.is_some());
        let s = result.unwrap();
        assert!(s.contains("/monitor") || s.contains("安全"));
    }
    
    #[test]
    fn test_reconstruct_reply_unsupported() {
        let result = reconstruct_reply(ReplyKind::Unsupported, None, None);
        assert!(result.is_some());
        let s = result.unwrap();
        assert!(s.contains("不支持"));
    }
    
    #[test]
    fn test_reconstruct_reply_monitor_final_with_full_json() {
        let json = serde_json::json!({
            "duration_secs": 30,
            "changed": true,
            "start_path": "C:\\start.png",
            "end_path": "C:\\end.png"
        });
        let result = reconstruct_reply(ReplyKind::MonitorFinal, Some(&json.to_string()), None);
        assert!(result.is_some());
        let s = result.unwrap();
        assert!(s.contains("桌面监控完成"));
        assert!(s.contains("30"));
        assert!(s.contains("有变化"));
    }
    
    #[test]
    fn test_reconstruct_reply_monitor_final_missing_changed_field() {
        // Should still reconstruct (use defaults for missing fields)
        let json = serde_json::json!({
            "duration_secs": 60,
            "start_path": "C:\\start.png",
            "end_path": "C:\\end.png"
        });
        let result = reconstruct_reply(ReplyKind::MonitorFinal, Some(&json.to_string()), None);
        assert!(result.is_some());
        let s = result.unwrap();
        assert!(s.contains("60"));
        assert!(s.contains("无明显变化")); // Default when changed missing
    }
    
    #[test]
    fn test_reconstruct_reply_monitor_final_no_json_returns_none() {
        // Without result_json, monitor_final CANNOT be reconstructed
        let result = reconstruct_reply(ReplyKind::MonitorFinal, None, None);
        assert!(result.is_none());
    }
    
    #[test]
    fn test_reconstruct_reply_llm_final_returns_none() {
        // LLM final never reconstructible
        let result = reconstruct_reply(ReplyKind::LlmFinal, Some("any json"), None);
        assert!(result.is_none());
        
        let result = reconstruct_reply(ReplyKind::LlmFinal, None, None);
        assert!(result.is_none());
    }
    
    #[test]
    fn test_reconstruct_inbound_for_retry_uses_chat_id() {
        let outbox = crate::gateway::feishu_store::FeishuOutbox {
            id: 1,
            outbound_id: "out_test".to_string(),
            job_id: None,
            event_key: Some("evt_test".to_string()),
            channel: "feishu".to_string(),
            chat_id: Some("chat_xyz".to_string()),
            reply_kind: Some("timeout_reply".to_string()),
            status: crate::gateway::feishu_store::OutboxStatus::Pending,
            attempts: 0,
            max_attempts: 3,
            next_attempt_at: None,
            platform_message_id: None,
            reply_hash: None,
            reply_preview: None,
            result_json: None,
            created_at: 0,
            updated_at: 0,
            sent_at: None,
            error_code: None,
            error_message: None,
        };
        
        let inbound = reconstruct_inbound_for_retry(&outbox);
        assert!(inbound.is_some());
        let inbound = inbound.unwrap();
        assert_eq!(inbound.session_id, Some("chat_xyz".to_string()));
        assert!(inbound.metadata.get("chat_id").is_some());
        assert!(inbound.metadata.get("conversation_id").is_some());
        assert!(inbound.metadata.get("event_key").is_some());
    }
    
    #[test]
    fn test_reconstruct_inbound_for_retry_missing_chat_id_returns_none() {
        let outbox = crate::gateway::feishu_store::FeishuOutbox {
            id: 1,
            outbound_id: "out_no_chat".to_string(),
            job_id: None,
            event_key: None,
            channel: "feishu".to_string(),
            chat_id: None, // Missing!
            reply_kind: Some("timeout_reply".to_string()),
            status: crate::gateway::feishu_store::OutboxStatus::Pending,
            attempts: 0,
            max_attempts: 3,
            next_attempt_at: None,
            platform_message_id: None,
            reply_hash: None,
            reply_preview: None,
            result_json: None,
            created_at: 0,
            updated_at: 0,
            sent_at: None,
            error_code: None,
            error_message: None,
        };
        
        let inbound = reconstruct_inbound_for_retry(&outbox);
        assert!(inbound.is_none());
    }
    
    #[test]
    fn test_reconstruct_inbound_unsupported_channel_returns_none() {
        let outbox = crate::gateway::feishu_store::FeishuOutbox {
            id: 1,
            outbound_id: "out_other".to_string(),
            job_id: None,
            event_key: None,
            channel: "slack".to_string(), // Not supported for retry
            chat_id: Some("chat_xyz".to_string()),
            reply_kind: Some("timeout_reply".to_string()),
            status: crate::gateway::feishu_store::OutboxStatus::Pending,
            attempts: 0,
            max_attempts: 3,
            next_attempt_at: None,
            platform_message_id: None,
            reply_hash: None,
            reply_preview: None,
            result_json: None,
            created_at: 0,
            updated_at: 0,
            sent_at: None,
            error_code: None,
            error_message: None,
        };
        
        let inbound = reconstruct_inbound_for_retry(&outbox);
        assert!(inbound.is_none());
    }

    // ========== Command palette tests (v0.8.9) ==========
    use crate::channels::adapters::outbound::{ChannelOutboundSender, MockOutboundSender, ReplyTarget};

    #[test]
    fn test_is_menu_trigger_slash() {
        assert!(is_menu_trigger("/"));
        assert!(is_menu_trigger(" / "));
    }

    #[test]
    fn test_is_menu_trigger_chinese_tokens() {
        assert!(is_menu_trigger("菜单"));
        assert!(is_menu_trigger("帮助"));
        assert!(is_menu_trigger("功能"));
    }

    #[test]
    fn test_is_menu_trigger_help_keyword() {
        assert!(is_menu_trigger("help"));
        assert!(is_menu_trigger("HELP"));
        assert!(is_menu_trigger(" Help "));
    }

    #[test]
    fn test_is_menu_trigger_non_menu_text_not_triggered() {
        assert!(!is_menu_trigger("hello"));
        assert!(!is_menu_trigger("你好"));
        assert!(!is_menu_trigger("/monitor desktop"));
        assert!(!is_menu_trigger(""));
        assert!(!is_menu_trigger("   "));
    }

    #[test]
    fn test_canonical_card_action_allow_list() {
        assert_eq!(canonical_card_action("monitor_30s"), Some("monitor_30s"));
        assert_eq!(canonical_card_action("monitor_60s"), Some("monitor_60s"));
        assert_eq!(canonical_card_action("gateway_status"), Some("gateway_status"));
        assert_eq!(canonical_card_action("recent_jobs"), Some("recent_jobs"));
        assert_eq!(canonical_card_action("help"), Some("help"));
    }

    #[test]
    fn test_canonical_card_action_unknown_returns_none() {
        assert!(canonical_card_action("file_delete").is_none());
        assert!(canonical_card_action("rm_rf").is_none());
        assert!(canonical_card_action("").is_none());
        assert!(canonical_card_action("monitor_120s").is_none());
        assert!(canonical_card_action("bash").is_none());
    }

    #[test]
    fn test_resolve_card_action_returns_bool() {
        assert!(resolve_card_action("monitor_30s"));
        assert!(resolve_card_action("help"));
        assert!(!resolve_card_action("evil"));
        assert!(!resolve_card_action(""));
    }

    #[test]
    fn test_build_command_palette_card_has_required_buttons() {
        let card = build_command_palette_card();
        let s = card.to_string();
        assert!(s.contains("OmniNova Agent 功能菜单"));
        assert!(s.contains("monitor_30s"));
        assert!(s.contains("monitor_60s"));
        assert!(s.contains("gateway_status"));
        assert!(s.contains("recent_jobs"));
        assert!(s.contains("help"));
        // Privacy: card must NOT contain any obviously sensitive boilerplate.
        assert!(!s.contains("app_secret"));
        assert!(!s.contains("tenant_access_token"));
        assert!(!s.contains("Authorization"));
    }

    #[test]
    fn test_unknown_action_reply_is_safe() {
        let s = unknown_action_reply();
        assert!(s.contains("未知操作"));
        assert!(s.contains("/"));
    }

    #[test]
    fn test_help_reply_does_not_leak_secrets() {
        let s = help_reply();
        assert!(s.contains("普通聊天"));
        assert!(s.contains("/monitor"));
        assert!(!s.contains("app_secret"));
        assert!(!s.contains("token"));
    }

    #[test]
    fn test_gateway_status_reply_includes_basic_fields() {
        let s = gateway_status_reply(
            Some("token"),
            true,
            false,
            Some("real"),
            true,
            "C:\\Users\\Hero\\.omninova\\state.sqlite",
            3,
            1,
        );
        assert!(s.contains("Gateway 状态"));
        assert!(s.contains("token"));
        assert!(s.contains("real"));
        assert!(s.contains("state.sqlite"));
        // Privacy: must not reveal app_secret / token values.
        assert!(!s.contains("app_secret"));
    }

    #[test]
    fn test_recent_jobs_reply_text_empty() {
        let lines = recent_jobs_reply_text(&[]);
        assert!(lines.contains("最近"));
        assert!(lines.contains("0"));
    }

    #[test]
    fn test_recent_jobs_reply_text_with_summary_lines() {
        let jobs = vec![
            summarize_job_for_card(
                "abcdef-1234-5678-9abcdef01234",
                "tool",
                "COMPLETED",
                1,
                None,
                1_700_000_000_000,
                Some(1_700_000_060_000),
            ),
            summarize_job_for_card(
                "failing-job-with-error",
                "chat_only",
                "FAILED",
                3,
                Some("timeout"),
                1_700_000_120_000,
                Some(1_700_000_180_000),
            ),
        ];
        let s = recent_jobs_reply_text(&jobs);
        // Two jobs summary must be present
        assert!(s.contains("最近 2 条"));
        assert!(s.contains("COMPLETED"));
        assert!(s.contains("FAILED"));
        assert!(s.contains("timeout"));
        // Privacy: do not leak payload_json (we never asked for it)
        assert!(!s.contains("payload_json"));
    }

    #[test]
    fn test_summarize_job_for_card_shortens_id() {
        let line = summarize_job_for_card(
            "this_is_a_very_long_job_id_that_should_be_shortened_for_display",
            "tool",
            "RUNNING",
            1,
            None,
            1_700_000_000_000,
            None,
        );
        // The job id must be truncated to <= 24 chars (per short_id).
        assert!(line.job_id_short.chars().count() <= 24);
    }

    #[test]
    fn test_reconstruct_command_palette_card() {
        let result = reconstruct_reply(ReplyKind::CommandPaletteCard, None, None);
        assert!(result.is_some());
        assert!(result.unwrap().contains("功能菜单"));
    }

    #[test]
    fn test_reconstruct_card_action_result_with_text() {
        let json = serde_json::json!({ "text": "card action result" });
        let result = reconstruct_reply(
            ReplyKind::CardActionResult,
            Some(&json.to_string()),
            None,
        );
        assert_eq!(result, Some("card action result".to_string()));
    }

    #[test]
    fn test_reconstruct_card_action_result_without_text_returns_none() {
        let result = reconstruct_reply(ReplyKind::CardActionResult, None, None);
        assert!(result.is_none());
    }

    #[test]
    fn test_reconstruct_gateway_status_with_text() {
        let json = serde_json::json!({ "text": "Gateway 状态 - test" });
        let result = reconstruct_reply(
            ReplyKind::GatewayStatusReply,
            Some(&json.to_string()),
            None,
        );
        assert_eq!(result, Some("Gateway 状态 - test".to_string()));
    }

    #[test]
    fn test_reconstruct_recent_jobs_with_text() {
        let json = serde_json::json!({ "text": "recent jobs summary" });
        let result = reconstruct_reply(
            ReplyKind::RecentJobsReply,
            Some(&json.to_string()),
            None,
        );
        assert_eq!(result, Some("recent jobs summary".to_string()));
    }

    #[test]
    fn test_reply_kind_command_palette_card_is_retryable() {
        assert!(ReplyKind::CommandPaletteCard.is_retryable());
    }

    #[test]
    fn test_reply_kind_card_action_result_is_retryable() {
        assert!(ReplyKind::CardActionResult.is_retryable());
    }

    #[test]
    fn test_reply_kind_gateway_status_is_retryable() {
        assert!(ReplyKind::GatewayStatusReply.is_retryable());
    }

    #[test]
    fn test_reply_kind_recent_jobs_is_retryable() {
        assert!(ReplyKind::RecentJobsReply.is_retryable());
    }

    #[test]
    fn test_reply_kind_command_palette_str_roundtrip() {
        assert_eq!(
            ReplyKind::from_str(ReplyKind::CommandPaletteCard.as_str()),
            Some(ReplyKind::CommandPaletteCard)
        );
        assert_eq!(
            ReplyKind::from_str(ReplyKind::CardActionResult.as_str()),
            Some(ReplyKind::CardActionResult)
        );
        assert_eq!(
            ReplyKind::from_str(ReplyKind::GatewayStatusReply.as_str()),
            Some(ReplyKind::GatewayStatusReply)
        );
        assert_eq!(
            ReplyKind::from_str(ReplyKind::RecentJobsReply.as_str()),
            Some(ReplyKind::RecentJobsReply)
        );
    }

    // ========== Outbound / platform_webhook tests for send_interactive_card ==========

    #[test]
    fn test_mock_outbound_send_interactive_card_falls_back_to_text() {
        let sender = MockOutboundSender::new();
        let target = ReplyTarget {
            channel: ChannelKind::Feishu,
            chat_id: "oc_test".to_string(),
            message_id: None,
            user_id: None,
        };
        let card = build_command_palette_card();
        let result = tokio_test_block_on(async {
            sender.send_interactive_card(&target, &card).await
        });
        assert!(result.ok);
        // The fallback sends a short summary text but still records a message.
        assert_eq!(sender.count(), 1);
    }

    /// Tiny inline helper so tests don't need to add tokio as a test
    /// dependency. We use tokio's block_on via the production runtime
    /// since `omninova-core` already depends on tokio.
    fn tokio_test_block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio test runtime")
            .block_on(future)
    }
}
