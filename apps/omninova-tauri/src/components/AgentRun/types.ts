/**
 * Types for the real-time execution timeline (AgentRun).
 * Mirrors the Rust AgentRunEvent enum in gateway/mod.rs.
 */

export interface RunDiffStats {
  additions: number;
  deletions: number;
}

export type AgentRunStepStatus = "running" | "success" | "error" | "warning";

export interface AgentRunChangedFile {
  path: string;
  additions: number;
  deletions: number;
}

export interface AgentRunPatchHunk {
  path: string;
  old_start: number;
  old_lines: number;
  new_start: number;
  new_lines: number;
  additions: number;
  deletions: number;
  summary: string;
  old_text?: string;
  new_text?: string;
  text_truncated?: boolean;
}

export interface AgentRunStep {
  id: string;
  tool_name: string;
  title: string;
  status: AgentRunStepStatus;
  duration_ms?: number;
  result_summary?: string;
  outputs: string[];
  changed_files: AgentRunChangedFile[];
  patch_hunks: AgentRunPatchHunk[];
  additions: number;
  deletions: number;
}

export type AgentRunEvent =
  | AgentRunEventRunStarted
  | AgentRunEventModelStarted
  | AgentRunEventModelDelta
  | AgentRunEventModelCompleted
  | AgentRunEventToolCallCreated
  | AgentRunEventToolStarted
  | AgentRunEventToolCompleted
  | AgentRunEventSkillActivated
  | AgentRunEventApprovalRequired
  | AgentRunEventCommandOutput
  | AgentRunEventFileChanged
  | AgentRunEventPatchStarted
  | AgentRunEventPatchHunk
  | AgentRunEventPatchApplied
  | AgentRunEventPatchFailed
  | AgentRunEventRunCompleted
  | AgentRunEventRunFailed
  | AgentRunEventRunCancelled
  | AgentRunEventError;

export interface AgentRunEventRunStarted {
  type: "run_started";
  run_id: string;
  agent_name: string;
  session_id: string | null;
}

export interface AgentRunEventModelStarted {
  type: "model_started";
  run_id: string;
  step_id: string;
  title: string;
}

export interface AgentRunEventModelDelta {
  type: "model_delta";
  run_id: string;
  step_id: string;
  content: string;
}

export interface AgentRunEventModelCompleted {
  type: "model_completed";
  run_id: string;
  step_id: string;
  title: string;
}

export interface AgentRunEventToolCallCreated {
  type: "tool_call_created";
  run_id: string;
  step_id: string;
  tool_call_id: string;
  tool_name: string;
  title: string;
}

export interface AgentRunEventToolStarted {
  type: "tool_started";
  run_id: string;
  tool_call_id: string;
  tool_name: string;
  summary: string;
}

export interface AgentRunEventToolCompleted {
  type: "tool_completed";
  run_id: string;
  tool_call_id: string;
  tool_name: string;
  success: boolean;
  duration_ms: number;
  result_summary: string;
  diff_stats: RunDiffStats | null;
}

export interface AgentRunEventSkillActivated {
  type: "skill_activated";
  run_id: string;
  skill_id: string;
  display_name: string;
  source: string;
}

export interface AgentRunEventApprovalRequired {
  type: "approval_required";
  run_id: string;
  step_id: string;
  tool_call_id: string;
  approval_id: string;
  tool_name: string;
  title: string;
  reason: string;
  arguments: Record<string, unknown>;
}

export interface AgentRunEventCommandOutput {
  type: "command_output";
  run_id: string;
  tool_call_id: string;
  tool_name: string;
  output: string;
  is_stderr: boolean;
}

export interface AgentRunEventFileChanged {
  type: "file_changed";
  run_id: string;
  step_id?: string;
  tool_call_id?: string | null;
  path: string;
  additions: number;
  deletions: number;
  change_type?: "created" | "modified" | "deleted" | "added" | "removed";
  old_text?: string;
  new_text?: string;
  content_truncated?: boolean;
  content_total_chars?: number;
  content_preview_chars?: number;
}

export interface AgentRunEventPatchStarted {
  type: "patch_started";
  run_id: string;
  step_id: string;
  tool_call_id: string;
  path: string;
  title: string;
}

export interface AgentRunEventPatchHunk {
  type: "patch_hunk";
  run_id: string;
  step_id: string;
  tool_call_id: string;
  path: string;
  old_start: number;
  old_lines: number;
  new_start: number;
  new_lines: number;
  additions: number;
  deletions: number;
  summary: string;
  old_text?: string;
  new_text?: string;
  text_truncated?: boolean;
}

export interface AgentRunEventPatchApplied {
  type: "patch_applied";
  run_id: string;
  step_id: string;
  tool_call_id: string;
  path: string;
  additions: number;
  deletions: number;
  hunks_count: number;
  result_summary: string;
}

export interface AgentRunEventPatchFailed {
  type: "patch_failed";
  run_id: string;
  step_id: string;
  tool_call_id: string;
  path: string;
  error: string;
}

export interface AgentRunEventRunCompleted {
  type: "run_completed";
  run_id: string;
  success: boolean;
  reply?: string;
  reply_preview: string;
}

export interface AgentRunEventRunFailed {
  type: "run_failed";
  run_id: string;
  error: string;
}

export interface AgentRunEventRunCancelled {
  type: "run_cancelled";
  run_id: string;
  reason: string;
}

export interface AgentRunEventError {
  type: "error";
  run_id: string;
  message: string;
}

export type RunEvent =
  | RunEventToolStarted
  | RunEventToolCompleted
  | RunEventCommandOutput
  | RunEventFileChanged;

export interface RunEventToolStarted {
  type: "toolStarted";
  tool_call_id?: string;
  tool_name: string;
  summary: string;
}

export interface RunEventToolCompleted {
  type: "toolCompleted";
  tool_call_id?: string;
  tool_name: string;
  success: boolean;
  duration_ms: number;
  result_summary: string;
  diff_stats: RunDiffStats | null;
}

export interface RunEventCommandOutput {
  type: "commandOutput";
  tool_call_id?: string;
  tool_name: string;
  output: string;
  is_stderr: boolean;
}

export interface RunEventFileChanged {
  type: "fileChanged";
  path: string;
  additions: number;
  deletions: number;
}

export function getEventStatusLabel(
  event: RunEvent | AgentRunEvent | { type?: string; success?: boolean }
): string {
  switch (event.type) {
    case "run_started":
    case "model_started":
    case "model_delta":
    case "tool_call_created":
    case "skill_activated":
    case "tool_started":
    case "command_output":
    case "patch_started":
    case "patch_hunk":
    case "toolStarted":
    case "commandOutput":
      return "running";
    case "approval_required":
      return "warning";
    case "run_completed":
    case "model_completed":
    case "fileChanged":
    case "file_changed":
    case "patch_applied":
      return "success";
    case "error":
    case "run_failed":
    case "run_cancelled":
    case "patch_failed":
      return "error";
    case "tool_completed":
    case "toolCompleted":
      return event.success ? "success" : "error";
    default:
      return "unknown";
  }
}

export function getToolLabel(toolName: string): string {
  const labels: Record<string, string> = {
    file_list: "列出文件",
    list_directory: "列出文件",
    file_read: "读取文件",
    read_file: "读取文件",
    file_write: "写入文件",
    write_file: "写入文件",
    file_edit: "修改文件",
    edit_file: "修改文件",
    str_replace_editor: "修改文件",
    file_patch: "修改文件",
    apply_patch: "修改文件",
    file_list_enhanced: "列出文件",
    glob_search: "搜索文件",
    glob: "搜索文件",
    content_search: "搜索内容",
    search: "搜索内容",
    grep: "搜索内容",
    shell: "执行命令",
    bash: "执行命令",
    run_command: "执行命令",
    Command: "执行命令",
    git_operations: "Git 操作",
    git: "Git 操作",
    delegate: "委托任务",
    web_search: "网络搜索",
    web_fetch: "获取网页",
    http_request: "HTTP 请求",
    memory_store: "存储记忆",
    memory_recall: "回忆记忆",
    knowledge_search: "检索知识库",
    browser: "浏览器",
    pdf_read: "读取 PDF",
  };
  return labels[toolName] ?? toolName;
}
