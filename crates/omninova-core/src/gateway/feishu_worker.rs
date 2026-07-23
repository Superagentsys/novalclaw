//! Feishu async worker for background processing of webhook events
//! 
//! This module provides background job processing to avoid Feishu's 3-second webhook timeout.
//! Webhook handlers quickly ACK, while Runtime and send_text run in the background.

use crate::channels::ChannelKind;
use crate::channels::adapters::outbound::OutboundResult;
use crate::desktop_capture::{self, CaptureResult, MonitorResult};
use crate::gateway::OutboundMsgCache;
use crate::gateway::GatewayRuntime;
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
}

impl FeishuAsyncJob {
    /// Create a new async job from webhook data
    pub fn new(
        channel: ChannelKind,
        inbound: crate::channels::InboundMessage,
        raw_payload: serde_json::Value,
        is_chat_only: bool,
    ) -> Self {
        let feishu_mode = if is_chat_only { "chat_only" } else { "tool" };
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let job_id = format!("job_{}_{}", created_at, uuid::Uuid::new_v4().to_string()[..8].to_string());
        
        Self {
            channel,
            inbound,
            raw_payload,
            feishu_mode: feishu_mode.to_string(),
            is_chat_only,
            created_at,
            job_id,
        }
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
fn parse_monitor_duration_from_text(text: &str) -> u64 {
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

/// Send an outbound reply with proper timeout and logging
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
    
    // Check for tool intent in chat_only mode
    if job.is_chat_only {
        if let Some(intent) = detect_tool_intent(&job.inbound.text) {
            println!(
                "[{}-worker] short_circuit job_id={} intent={} text_len={}",
                channel_name, job_id, intent, text_len
            );
            
            let blocked_reply = chat_only_blocked_response();
            send_reply_with_timeout(&runtime, &job.inbound, &blocked_reply, &channel_name, OUTBOUND_TIMEOUT_SECS).await;
            
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
        println!(
            "[{}-worker] direct_monitor job_id={} text_len={}",
            channel_name, job_id, text_len
        );
        
        // Send progress reply
        let progress_reply = progress_reply_for_command("/monitor");
        send_reply_with_timeout(&runtime, &job.inbound, &progress_reply, &channel_name, OUTBOUND_TIMEOUT_SECS).await;
        
        // Run direct monitor
        let duration_secs = parse_monitor_duration_from_text(&job.inbound.text);
        let result = direct_monitor_runner(&job, &channel_name).await;
        
        // Format and send result
        let reply = format_monitor_result(&result, duration_secs);
        send_reply_with_timeout(&runtime, &job.inbound, &reply, &channel_name, OUTBOUND_TIMEOUT_SECS).await;
        
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
            send_reply_with_timeout(&runtime, &job.inbound, &progress_reply, &channel_name, OUTBOUND_TIMEOUT_SECS).await;
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
            send_reply_with_timeout(&runtime, &job.inbound, &error_reply, &channel_name, OUTBOUND_TIMEOUT_SECS).await;
            println!(
                "[{}-worker] failure_reply_sent job_id={}",
                channel_name, job_id
            );
            
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
            send_reply_with_timeout(&runtime, &job.inbound, &timeout_reply_msg, &channel_name, OUTBOUND_TIMEOUT_SECS).await;
            println!(
                "[{}-worker] timeout_reply_sent job_id={}",
                channel_name, job_id
            );
            
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
            let duration_ms = started_at.elapsed().as_millis() as u64;
            println!(
                "[{}-worker] job_completed job_id={} duration_ms={}",
                channel_name, job_id, duration_ms
            );
            return;
        }
        
        // Send reply via outbound
        println!("[{}-worker] outbound_start job_id={} reply_len={}", channel_name, job_id, reply.len());
        send_reply_with_timeout(&runtime, &job.inbound, &reply, &channel_name, OUTBOUND_TIMEOUT_SECS).await;
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
                "[{}-worker] recorded_outbound_message_id={}",
                channel_name, msg_id
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
}
