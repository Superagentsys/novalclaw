//! Agent Runtime Event Bus.
//!
//! Centralized event emission for the agent execution timeline. Every tool
//! execution event flows through the bus, which:
//!   1. Assigns a monotonically increasing `seq` to each event.
//!   2. Forwards events to the Tauri frontend in real time.
//!   3. Collects all events for final replay.

use crate::agent::agent_event::{AgentRunEvent, DiffStats};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;
use uuid::Uuid;

fn now_ts() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let t = now.as_secs();
    let h = (t / 3600) % 24;
    let m = (t / 60) % 60;
    let s = t % 60;
    let ms = now.subsec_millis();
    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms)
}

/// Sender half of the event bus — cheap to clone (Arc).
#[derive(Clone)]
pub struct EventBus {
    inner: Arc<EventBusInner>,
}

struct EventBusInner {
    run_id: String,
    seq: Mutex<u64>,
    /// Channel to the background drain task that sends to Tauri.
    tx: mpsc::UnboundedSender<AgentRunEvent>,
    /// All events collected in order for final replay.
    collected: Mutex<Vec<AgentRunEvent>>,
    tool_steps: Mutex<HashMap<String, String>>,
    started_tool_calls: Mutex<HashSet<String>>,
    terminal_sent: Mutex<bool>,
    /// Whether the drain task is still alive.
    /// Set to true when drain() is called.
    closed: Mutex<bool>,
}

impl EventBus {
    /// Creates a new event bus for a run. The `run_id` is fixed for the lifetime
    /// of this bus. The returned `(EventBus, drain_handle)` must be used together:
    ///   - Drive `drain_handle` to completion to send buffered events to `emit_fn`.
    ///   - Drop `drain_handle` early to stop forwarding (events still go to `collected`).
    pub fn new(
        run_id: String,
        emit_fn: impl Fn(AgentRunEvent) + Send + 'static,
    ) -> (Self, EventBusDrainHandle) {
        let (tx, rx) = mpsc::unbounded_channel();
        let inner = Arc::new(EventBusInner {
            run_id,
            seq: Mutex::new(0),
            tx,
            collected: Mutex::new(Vec::new()),
            tool_steps: Mutex::new(HashMap::new()),
            started_tool_calls: Mutex::new(HashSet::new()),
            terminal_sent: Mutex::new(false),
            closed: Mutex::new(false),
        });

        let bus = Self { inner: Arc::clone(&inner) };
        let handle = EventBusDrainHandle {
            inner,
            rx: Mutex::new(Some(rx)),
            emit_fn: Box::new(emit_fn),
        };

        (bus, handle)
    }

    /// Returns the run_id for this event bus.
    pub fn run_id(&self) -> &str {
        &self.inner.run_id
    }

    /// Emits a run_started event.
    pub fn run_started(&self, agent_name: String, session_id: Option<String>, parent_step_id: Option<String>) {
        self.emit(AgentRunEvent::run_started {
            run_id: self.inner.run_id.clone(),
            agent_name,
            session_id,
            parent_step_id,
        });
    }

    /// Emits a step_started event.
    pub fn step_started(&self, title: String, parent_step_id: Option<String>) -> String {
        let step_id = Uuid::new_v4().to_string();
        self.emit(AgentRunEvent::step_started {
            run_id: self.inner.run_id.clone(),
            step_id: step_id.clone(),
            parent_step_id,
            title,
        });
        step_id
    }

    pub fn model_started(&self, title: String) -> String {
        let step_id = Uuid::new_v4().to_string();
        self.emit(AgentRunEvent::model_started {
            run_id: self.inner.run_id.clone(),
            step_id: step_id.clone(),
            title,
        });
        step_id
    }

    pub fn model_delta(&self, step_id: String, content: String) {
        self.emit(AgentRunEvent::model_delta {
            run_id: self.inner.run_id.clone(),
            step_id,
            content,
        });
    }

    pub fn model_completed(&self, step_id: String, title: String) {
        self.emit(AgentRunEvent::model_completed {
            run_id: self.inner.run_id.clone(),
            step_id,
            title,
        });
    }

    pub fn tool_call_created(&self, tool_call_id: String, tool_name: String, title: String) -> String {
        let step_id = self.tool_step_id(&tool_call_id);
        self.emit(AgentRunEvent::tool_call_created {
            run_id: self.inner.run_id.clone(),
            step_id: step_id.clone(),
            tool_call_id,
            tool_name,
            title,
        });
        step_id
    }

    /// Emits a tool_started event and returns the step_id.
    /// Convenience wrapper that generates a step_id.
    pub fn tool_started(
        &self,
        tool_call_id: String,
        tool_name: String,
        title: String,
        parent_step_id: Option<String>,
    ) -> String {
        let step_id = self.tool_step_id(&tool_call_id);
        {
            let mut started = self.inner.started_tool_calls.lock().unwrap();
            if !started.insert(tool_call_id.clone()) {
                return step_id;
            }
        }
        self.emit(AgentRunEvent::tool_started {
            run_id: self.inner.run_id.clone(),
            step_id: step_id.clone(),
            parent_step_id,
            tool_call_id,
            tool_name,
            title,
        });
        step_id
    }

    pub fn skill_activated(
        &self,
        skill_id: String,
        display_name: String,
        source: String,
    ) {
        self.emit(AgentRunEvent::skill_activated {
            run_id: self.inner.run_id.clone(),
            skill_id,
            display_name,
            source,
        });
    }

    fn tool_step_id(&self, tool_call_id: &str) -> String {
        let mut tool_steps = self.inner.tool_steps.lock().unwrap();
        tool_steps
            .entry(tool_call_id.to_string())
            .or_insert_with(|| Uuid::new_v4().to_string())
            .clone()
    }

    /// Emits a command_output chunk.
    pub fn command_output(
        &self,
        step_id: String,
        tool_call_id: String,
        tool_name: String,
        content: String,
        is_stderr: bool,
    ) {
        self.emit(AgentRunEvent::command_output {
            run_id: self.inner.run_id.clone(),
            step_id,
            tool_call_id,
            tool_name,
            content,
            is_stderr,
        });
    }

    /// Emits a file_changed event.
    pub fn file_changed(
        &self,
        step_id: String,
        tool_call_id: Option<String>,
        path: String,
        additions: i32,
        deletions: i32,
        change_type: Option<crate::agent::agent_event::ChangeType>,
        old_text: Option<String>,
        new_text: Option<String>,
        content_truncated: bool,
        content_total_chars: Option<usize>,
        content_preview_chars: Option<usize>,
    ) {
        self.emit(AgentRunEvent::file_changed {
            run_id: self.inner.run_id.clone(),
            step_id,
            tool_call_id,
            path,
            additions,
            deletions,
            change_type,
            old_text,
            new_text,
            content_truncated,
            content_total_chars,
            content_preview_chars,
        });
    }

    pub fn patch_started(
        &self,
        step_id: String,
        tool_call_id: String,
        path: String,
        title: String,
    ) {
        self.emit(AgentRunEvent::patch_started {
            run_id: self.inner.run_id.clone(),
            step_id,
            tool_call_id,
            path,
            title,
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn patch_hunk(
        &self,
        step_id: String,
        tool_call_id: String,
        path: String,
        old_start: usize,
        old_lines: usize,
        new_start: usize,
        new_lines: usize,
        additions: i32,
        deletions: i32,
        summary: String,
        old_text: Option<String>,
        new_text: Option<String>,
        text_truncated: bool,
    ) {
        self.emit(AgentRunEvent::patch_hunk {
            run_id: self.inner.run_id.clone(),
            step_id,
            tool_call_id,
            path,
            old_start,
            old_lines,
            new_start,
            new_lines,
            additions,
            deletions,
            summary,
            old_text,
            new_text,
            text_truncated,
        });
    }

    pub fn patch_applied(
        &self,
        step_id: String,
        tool_call_id: String,
        path: String,
        additions: i32,
        deletions: i32,
        hunks_count: usize,
        result_summary: String,
    ) {
        self.emit(AgentRunEvent::patch_applied {
            run_id: self.inner.run_id.clone(),
            step_id,
            tool_call_id,
            path,
            additions,
            deletions,
            hunks_count,
            result_summary,
        });
    }

    pub fn patch_failed(
        &self,
        step_id: String,
        tool_call_id: String,
        path: String,
        error: String,
    ) {
        self.emit(AgentRunEvent::patch_failed {
            run_id: self.inner.run_id.clone(),
            step_id,
            tool_call_id,
            path,
            error,
        });
    }

    /// Emits a tool_completed event.
    pub fn tool_completed(
        &self,
        step_id: String,
        tool_call_id: String,
        tool_name: String,
        success: bool,
        duration_ms: u64,
        result_summary: String,
        diff_stats: Option<DiffStats>,
    ) {
        use crate::agent::agent_event::StepStatus;
        self.emit(AgentRunEvent::tool_completed {
            run_id: self.inner.run_id.clone(),
            step_id,
            tool_call_id,
            tool_name,
            status: if success { StepStatus::Success } else { StepStatus::Error },
            duration_ms,
            result_summary,
            diff_stats,
        });
    }

    /// Emits an approval_required event.
    pub fn approval_required(
        &self,
        step_id: String,
        tool_call_id: String,
        approval_id: String,
        tool_name: String,
        title: String,
        reason: String,
        arguments: serde_json::Value,
    ) {
        self.emit(AgentRunEvent::approval_required {
            run_id: self.inner.run_id.clone(),
            step_id,
            tool_call_id,
            approval_id,
            tool_name,
            title,
            reason,
            arguments,
        });
    }

    /// Emits approval_approved.
    pub fn approval_approved(
        &self,
        step_id: String,
        approval_id: String,
        tool_call_id: String,
        tool_name: String,
    ) {
        self.emit(AgentRunEvent::approval_approved {
            run_id: self.inner.run_id.clone(),
            step_id,
            approval_id,
            tool_call_id,
            tool_name,
        });
    }

    /// Emits approval_rejected.
    pub fn approval_rejected(
        &self,
        step_id: String,
        approval_id: String,
        tool_call_id: String,
        tool_name: String,
        reason: String,
    ) {
        self.emit(AgentRunEvent::approval_rejected {
            run_id: self.inner.run_id.clone(),
            step_id,
            approval_id,
            tool_call_id,
            tool_name,
            reason,
        });
    }

    /// Emits approval_cancelled.
    pub fn approval_cancelled(
        &self,
        step_id: String,
        approval_id: String,
        tool_call_id: String,
        tool_name: String,
    ) {
        self.emit(AgentRunEvent::approval_cancelled {
            run_id: self.inner.run_id.clone(),
            step_id,
            approval_id,
            tool_call_id,
            tool_name,
        });
    }

    /// Emits run_completed.
    pub fn run_completed(&self, reply: String, reply_preview: String) {
        self.emit_terminal(AgentRunEvent::run_completed {
            run_id: self.inner.run_id.clone(),
            reply,
            reply_preview,
        });
    }

    /// Emits run_failed.
    pub fn run_failed(&self, error: String) {
        self.emit_terminal(AgentRunEvent::run_failed {
            run_id: self.inner.run_id.clone(),
            error,
        });
    }

    pub fn run_cancelled(&self, reason: String) {
        self.emit_terminal(AgentRunEvent::run_cancelled {
            run_id: self.inner.run_id.clone(),
            reason,
        });
    }

    /// Returns all collected events (for replay).
    pub fn collect(&self) -> Vec<AgentRunEvent> {
        self.inner.collected.lock().unwrap().clone()
    }

    fn emit(&self, event: AgentRunEvent) {
        let is_terminal = matches!(
            event,
            AgentRunEvent::run_completed { .. }
                | AgentRunEvent::run_failed { .. }
                | AgentRunEvent::run_cancelled { .. }
        );
        if !is_terminal && *self.inner.terminal_sent.lock().unwrap() {
            tracing::debug!(
                target: "e2e",
                "[e2e-bus-ignored-after-terminal] timestamp={} run_id={} type={}",
                now_ts(),
                self.inner.run_id,
                event_type_name(&event)
            );
            return;
        }

        let seq = {
            let mut s = self.inner.seq.lock().unwrap();
            *s += 1;
            *s
        };

        // E2E debug: log event type and key field.
        let type_name = event_type_name(&event);
        let content_preview = match &event {
            AgentRunEvent::run_started { agent_name, .. } => agent_name.clone(),
            AgentRunEvent::step_started { title, .. } => title.clone(),
            AgentRunEvent::model_started { title, .. } => title.clone(),
            AgentRunEvent::model_delta { content, .. } => content.clone(),
            AgentRunEvent::model_completed { title, .. } => title.clone(),
            AgentRunEvent::tool_call_created { title, .. } => title.clone(),
            AgentRunEvent::tool_started { title, .. } => title.clone(),
            AgentRunEvent::command_output { content, .. } => content.clone(),
            AgentRunEvent::file_changed { path, .. } => path.clone(),
            AgentRunEvent::patch_started { title, .. } => title.clone(),
            AgentRunEvent::patch_hunk { summary, .. } => summary.clone(),
            AgentRunEvent::patch_applied { result_summary, .. } => result_summary.clone(),
            AgentRunEvent::patch_failed { error, .. } => error.clone(),
            AgentRunEvent::tool_completed { result_summary, .. } => result_summary.clone(),
            AgentRunEvent::skill_activated { display_name, .. } => display_name.clone(),
            AgentRunEvent::run_completed { reply_preview, .. } => reply_preview.clone(),
            AgentRunEvent::run_failed { error, .. } => error.clone(),
            AgentRunEvent::run_cancelled { reason, .. } => reason.clone(),
            AgentRunEvent::approval_required { reason, .. } => reason.clone(),
            AgentRunEvent::approval_approved { tool_name, .. } => tool_name.clone(),
            AgentRunEvent::approval_rejected { reason, .. } => reason.clone(),
            AgentRunEvent::approval_cancelled { tool_name, .. } => tool_name.clone(),
        };
        tracing::debug!(target: "e2e", "[e2e-bus-emit] timestamp={} run_id={} type={} content=\"{}\"", now_ts(), self.inner.run_id, type_name, preview(&content_preview, 80));

        // Always collect for replay.
        self.inner.collected.lock().unwrap().push(event.clone());

        // Forward to Tauri emitter if the channel is still open.
        let _ = self.inner.tx.send(event);
    }

    fn emit_terminal(&self, event: AgentRunEvent) {
        {
            let mut terminal_sent = self.inner.terminal_sent.lock().unwrap();
            if *terminal_sent {
                return;
            }
            *terminal_sent = true;
        }
        self.emit(event);
    }
}

fn event_type_name(event: &AgentRunEvent) -> &'static str {
    match event {
        AgentRunEvent::run_started { .. } => "run_started",
        AgentRunEvent::step_started { .. } => "step_started",
        AgentRunEvent::model_started { .. } => "model_started",
        AgentRunEvent::model_delta { .. } => "model_delta",
        AgentRunEvent::model_completed { .. } => "model_completed",
        AgentRunEvent::tool_call_created { .. } => "tool_call_created",
        AgentRunEvent::tool_started { .. } => "tool_started",
        AgentRunEvent::command_output { .. } => "command_output",
        AgentRunEvent::file_changed { .. } => "file_changed",
        AgentRunEvent::patch_started { .. } => "patch_started",
        AgentRunEvent::patch_hunk { .. } => "patch_hunk",
        AgentRunEvent::patch_applied { .. } => "patch_applied",
        AgentRunEvent::patch_failed { .. } => "patch_failed",
        AgentRunEvent::tool_completed { .. } => "tool_completed",
        AgentRunEvent::skill_activated { .. } => "skill_activated",
        AgentRunEvent::run_completed { .. } => "run_completed",
        AgentRunEvent::run_failed { .. } => "run_failed",
        AgentRunEvent::run_cancelled { .. } => "run_cancelled",
        AgentRunEvent::approval_required { .. } => "approval_required",
        AgentRunEvent::approval_approved { .. } => "approval_approved",
        AgentRunEvent::approval_rejected { .. } => "approval_rejected",
        AgentRunEvent::approval_cancelled { .. } => "approval_cancelled",
    }
}

/// Handle for the background drain task that forwards events to Tauri.
pub struct EventBusDrainHandle {
    inner: Arc<EventBusInner>,
    rx: Mutex<Option<mpsc::UnboundedReceiver<AgentRunEvent>>>,
    emit_fn: Box<dyn Fn(AgentRunEvent) + Send + 'static>,
}

impl EventBusDrainHandle {
    /// Drives the drain task to completion, forwarding all events to `emit_fn`.
    /// Call this after the run completes.
    pub async fn drain(mut self) {
        let run_id = self.inner.run_id.clone();
        {
            let mut closed = self.inner.closed.lock().unwrap();
            *closed = true;
        }
        // Release the shared inner Arc BEFORE draining: it holds the bus's
        // sender (`tx`). Keeping it alive here would keep the internal channel
        // open forever, so the drain task would never exit — leaking one task
        // per run and keeping the caller's forwarded-channel sender alive
        // (which hangs any caller that awaits channel close, e.g. the CLI
        // streaming printer).
        drop(self.inner);
        let mut rx = self.rx.lock().unwrap().take();
        if let Some(ref mut r) = rx {
            while let Some(evt) = r.recv().await {
                let type_name: &'static str = match &evt {
                    AgentRunEvent::run_started { .. } => "run_started",
                    AgentRunEvent::step_started { .. } => "step_started",
                    AgentRunEvent::model_started { .. } => "model_started",
                    AgentRunEvent::model_delta { .. } => "model_delta",
                    AgentRunEvent::model_completed { .. } => "model_completed",
                    AgentRunEvent::tool_call_created { .. } => "tool_call_created",
                    AgentRunEvent::tool_started { .. } => "tool_started",
                    AgentRunEvent::command_output { .. } => "command_output",
                    AgentRunEvent::file_changed { .. } => "file_changed",
                    AgentRunEvent::patch_started { .. } => "patch_started",
                    AgentRunEvent::patch_hunk { .. } => "patch_hunk",
                    AgentRunEvent::patch_applied { .. } => "patch_applied",
                    AgentRunEvent::patch_failed { .. } => "patch_failed",
                    AgentRunEvent::tool_completed { .. } => "tool_completed",
                    AgentRunEvent::skill_activated { .. } => "skill_activated",
                    AgentRunEvent::run_completed { .. } => "run_completed",
                    AgentRunEvent::run_failed { .. } => "run_failed",
                    AgentRunEvent::run_cancelled { .. } => "run_cancelled",
                    AgentRunEvent::approval_required { .. } => "approval_required",
                    AgentRunEvent::approval_approved { .. } => "approval_approved",
                    AgentRunEvent::approval_rejected { .. } => "approval_rejected",
                    AgentRunEvent::approval_cancelled { .. } => "approval_cancelled",
                };
                tracing::debug!(target: "e2e", "[e2e-bus-drain] timestamp={} run_id={} type={}", now_ts(), run_id, type_name);
                (self.emit_fn)(evt);
            }
        }
    }

    /// Drains all currently buffered events synchronously (non-async).
    /// Useful when the async task has already ended.
    pub fn drain_sync(&self) {
        // No-op for sync drain — events were already forwarded via the channel.
        // The drain task handles async forwarding.
    }
}

pub fn build_tool_prepare_summary(tool_name: &str, args: &serde_json::Value) -> String {
    build_tool_phase_summary(tool_name, args, "准备")
}

pub fn build_tool_start_summary(tool_name: &str, args: &serde_json::Value) -> String {
    build_tool_phase_summary(tool_name, args, "开始")
}

fn build_tool_phase_summary(tool_name: &str, args: &serde_json::Value, phase: &str) -> String {
    let path = args
        .get("path")
        .or_else(|| args.get("file_path"))
        .and_then(|v| v.as_str());
    match tool_name {
        "file_list" | "list_directory" => path
            .map(|p| format!("{phase}列出目录：{p}"))
            .unwrap_or_else(|| format!("{phase}列出目录")),
        "file_read" | "read_file" => path
            .map(|p| format!("{phase}读取文件：{p}"))
            .unwrap_or_else(|| format!("{phase}读取文件")),
        "file_write" | "write_file" => path
            .map(|p| format!("{phase}写入文件：{p}"))
            .unwrap_or_else(|| format!("{phase}写入文件")),
        "file_edit" | "edit_file" | "str_replace_editor" => path
            .map(|p| format!("{phase}编辑文件：{p}"))
            .unwrap_or_else(|| format!("{phase}编辑文件")),
        "file_patch" | "apply_patch" => path
            .map(|p| format!("{phase}修改文件：{p}"))
            .unwrap_or_else(|| format!("{phase}修改文件")),
        "glob_search" | "glob" => args
            .get("pattern")
            .and_then(|v| v.as_str())
            .map(|p| format!("{phase}搜索文件：{p}"))
            .unwrap_or_else(|| format!("{phase}搜索文件")),
        "content_search" | "search" | "grep" => args
            .get("query")
            .and_then(|v| v.as_str())
            .map(|q| format!("{phase}搜索内容：{q}"))
            .unwrap_or_else(|| format!("{phase}搜索内容")),
        "shell" | "bash" | "run_command" | "Command" => args
            .get("command")
            .or_else(|| args.get("script"))
            .and_then(|v| v.as_str())
            .map(|cmd| {
                let display: String = cmd.chars().take(80).collect();
                let suffix = if cmd.chars().count() > 80 { "..." } else { "" };
                format!("{phase}执行命令：{display}{suffix}")
            })
            .unwrap_or_else(|| format!("{phase}执行命令")),
        "git_operations" | "git" => args
            .get("operation")
            .and_then(|v| v.as_str())
            .map(|op| format!("{phase}执行 Git 操作：{op}"))
            .unwrap_or_else(|| format!("{phase}执行 Git 操作")),
        "browser" => args
            .get("url")
            .and_then(|v| v.as_str())
            .map(|url| format!("{phase}访问网页：{url}"))
            .unwrap_or_else(|| format!("{phase}操作浏览器")),
        "delegate" => format!("{phase}委托子任务"),
        _ => format!("{phase}执行工具：{tool_name}"),
    }
}

/// Helper to build a tool Chinese summary from the tool name and args.
pub fn build_tool_summary(tool_name: &str, args: &serde_json::Value) -> String {
    match tool_name {
        "file_list" | "list_directory" => {
            if let Some(p) = args.get("path").and_then(|v| v.as_str()) {
                format!("正在列出目录：{}", p)
            } else {
                "正在列出目录".into()
            }
        }
        "file_read" | "read_file" => {
            if let Some(p) = args.get("path").and_then(|v| v.as_str()) {
                format!("正在读取文件：{}", p)
            } else {
                "正在读取文件".into()
            }
        }
        "file_write" | "write_file" => {
            if let Some(p) = args.get("path").and_then(|v| v.as_str()) {
                format!("正在写入文件：{}", p)
            } else {
                "正在写入文件".into()
            }
        }
        "file_edit" | "edit_file" | "str_replace_editor" => {
            if let Some(p) = args.get("file_path").or_else(|| args.get("path")).and_then(|v| v.as_str()) {
                format!("正在编辑文件：{}", p)
            } else {
                "正在编辑文件".into()
            }
        }
        "file_patch" | "apply_patch" => {
            if let Some(p) = args.get("path").and_then(|v| v.as_str()) {
                format!("正在修改文件：{}", p)
            } else {
                "正在修改文件".into()
            }
        }
        "glob_search" | "glob" => {
            if let Some(p) = args.get("pattern").and_then(|v| v.as_str()) {
                format!("正在搜索文件：{}", p)
            } else {
                "正在搜索文件".into()
            }
        }
        "content_search" | "search" | "grep" => {
            if let Some(q) = args.get("query").and_then(|v| v.as_str()) {
                format!("正在搜索内容：{}", q)
            } else {
                "正在搜索内容".into()
            }
        }
        "shell" | "bash" | "run_command" | "Command" => {
            if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                let display = preview(cmd, 80);
                format!("正在执行命令：{}", display)
            } else if let Some(cmd) = args.get("script").and_then(|v| v.as_str()) {
                let display = preview(cmd, 80);
                format!("正在执行脚本：{}", display)
            } else {
                "正在执行命令".into()
            }
        }
        "git_operations" | "git" => {
            if let Some(op) = args.get("operation").and_then(|v| v.as_str()) {
                format!("正在执行 Git 操作：{}", op)
            } else {
                "正在执行 Git 操作".into()
            }
        }
        "browser" => {
            if let Some(u) = args.get("url").and_then(|v| v.as_str()) {
                format!("正在访问网页：{}", u)
            } else {
                "正在操作浏览器".into()
            }
        }
        "delegate" => "正在委托子任务".into(),
        _ => format!("正在执行工具：{}", tool_name),
    }
}

/// Helper to truncate a string for UI display.
pub fn truncate_for_display(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    if s.len() <= max_chars {
        return s.to_string();
    }
    let head: String = s.chars().take(max_chars).collect();
    format!("{}\n... [输出已截断，原始长度 {} 字符]", head, s.len())
}

/// Helper to extract diff stats from `git diff --numstat` output.
pub fn extract_diff_stats(tool_name: &str, output: &str) -> Option<DiffStats> {
    if !matches!(tool_name, "file_write" | "write_file" | "file_edit" | "edit_file" | "str_replace_editor" | "git_operations" | "git") {
        return None;
    }
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            let additions: i32 = parts[0].parse().unwrap_or(0);
            let deletions: i32 = parts[1].parse().unwrap_or(0);
            if additions > 0 || deletions > 0 {
                return Some(DiffStats { additions, deletions });
            }
        }
    }
    None
}

/// Compute diff stats from written content when git is not available.
pub fn compute_content_diff(tool_name: &str, args: &serde_json::Value, output: &str) -> Option<DiffStats> {
    if !matches!(tool_name, "file_write" | "write_file" | "file_edit" | "edit_file" | "str_replace_editor") {
        return None;
    }
    if output.to_lowercase().contains("success")
        || output.contains("写入成功")
        || output.contains("已保存")
        || output.contains("saved")
        || output.contains("Wrote")
    {
        let content_lines = args
            .get("content")
            .and_then(|v| v.as_str())
            .map(|s| s.lines().count())
            .unwrap_or(0);
        if content_lines > 0 {
            return Some(DiffStats { additions: content_lines as i32, deletions: 0 });
        }
    }
    None
}

/// Helper struct for timing tool execution.
pub struct TimedBlock {
    start: Instant,
}

impl TimedBlock {
    pub fn new() -> Self {
        Self { start: Instant::now() }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

impl Default for TimedBlock {
    fn default() -> Self {
        Self::new()
    }
}

/// E2E debug: returns the first `max_chars` chars of `s`, appending "..."
/// if `s` was longer. Never slices in the middle of a UTF-8 codepoint.
fn preview(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::agent_event::AgentRunEvent;

    #[tokio::test]
    async fn drain_completes_when_the_bus_is_dropped() {
        // Regression test for the drain leak: the drain handle used to keep the
        // bus's sender alive via the shared inner Arc, so the channel never
        // closed and the drain task never exited. Dropping every EventBus must
        // let drain() finish and release its emit_fn.
        let forwarded = Arc::new(Mutex::new(Vec::new()));
        let forwarded_inner = Arc::clone(&forwarded);
        let (bus, drain_handle) = EventBus::new("test-run".into(), move |evt| {
            forwarded_inner.lock().unwrap().push(evt);
        });
        bus.run_started("test-agent".into(), None, None);
        bus.model_delta("m1".into(), "hello".into());
        bus.run_completed("done".into(), "done".into());
        drop(bus);

        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::spawn(async move { drain_handle.drain().await }),
        )
        .await
        .expect("drain must complete once all senders are dropped")
        .expect("drain task must not panic");

        let mut events = forwarded
            .lock()
            .unwrap()
            .iter()
            .map(|evt| match evt {
                AgentRunEvent::run_started { .. } => "run_started",
                AgentRunEvent::model_delta { .. } => "model_delta",
                AgentRunEvent::run_completed { .. } => "run_completed",
                _ => "other",
            })
            .map(str::to_string)
            .collect::<Vec<_>>();
        events.sort();
        assert_eq!(events, vec!["model_delta", "run_completed", "run_started"]);
    }
}
