/**
 * Types for the real-time execution timeline (AgentRun).
 * Mirrors the Rust `RunEvent` / `RunDiffStats` types in gateway/mod.rs.
 */

/** Diff statistics for a modified file. */
export interface RunDiffStats {
  additions: number;
  deletions: number;
}

/** Union of all run events emitted during a tool call loop. */
export type RunEvent =
  | RunEventToolStarted
  | RunEventToolCompleted
  | RunEventFileChanged;

export interface RunEventToolStarted {
  type: "toolStarted";
  tool_name: string;
  summary: string;
}

export interface RunEventToolCompleted {
  type: "toolCompleted";
  tool_name: string;
  success: boolean;
  duration_ms: number;
  result_summary: string;
  diff_stats: RunDiffStats | null;
}

export interface RunEventFileChanged {
  type: "fileChanged";
  path: string;
  additions: number;
  deletions: number;
}

/** Human-readable status label for an event. */
export function getEventStatusLabel(event: RunEvent): string {
  switch (event.type) {
    case "toolStarted":
      return "running";
    case "toolCompleted":
      return event.success ? "success" : "error";
    case "fileChanged":
      return "success";
  }
}

/** Short Chinese label for a tool name. */
export function getToolLabel(toolName: string): string {
  const labels: Record<string, string> = {
    file_read: "读取文件",
    file_write: "写入文件",
    file_edit: "修改文件",
    file_list: "列出目录",
    glob_search: "搜索文件",
    content_search: "搜索内容",
    shell: "执行命令",
    git_operations: "Git 操作",
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
