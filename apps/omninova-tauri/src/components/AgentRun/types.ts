/**
 * Types for the real-time execution timeline (AgentRun).
 * Mirrors the Rust AgentRunEvent enum in gateway/mod.rs.
 */

export interface RunDiffStats {
  additions: number;
  deletions: number;
}

export type AgentRunEvent =
  | AgentRunEventRunStarted
  | AgentRunEventToolStarted
  | AgentRunEventToolCompleted
  | AgentRunEventCommandOutput
  | AgentRunEventFileChanged
  | AgentRunEventRunCompleted
  | AgentRunEventError;

export interface AgentRunEventRunStarted {
  type: "run_started";
  run_id: string;
  agent_name: string;
  session_id: string | null;
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

export interface AgentRunEventCommandOutput {
  type: "command_output";
  run_id: string;
  tool_call_id: string;
  tool_name: string;
  output: string;
  is_final: boolean;
}

export interface AgentRunEventFileChanged {
  type: "file_changed";
  run_id: string;
  path: string;
  additions: number;
  deletions: number;
}

export interface AgentRunEventRunCompleted {
  type: "run_completed";
  run_id: string;
  success: boolean;
  reply_preview: string;
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
  is_final: boolean;
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
    case "tool_started":
    case "command_output":
    case "toolStarted":
    case "commandOutput":
      return "running";
    case "run_completed":
    case "fileChanged":
    case "file_changed":
      return "success";
    case "error":
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
    browser: "浏览器",
    pdf_read: "读取 PDF",
  };
  return labels[toolName] ?? toolName;
}
