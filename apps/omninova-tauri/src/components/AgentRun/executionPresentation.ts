/**
 * Centralized presentation mapping for execution events and errors.
 *
 * Runtime semantics stay authoritative; this layer only translates structured
 * event/tool/error information into safe user-facing labels.
 */

const TOOL_RUNNING_LABELS: Record<string, string> = {
  file_list: "正在列出文件",
  list_directory: "正在列出文件",
  file_read: "正在读取文件",
  read_file: "正在读取文件",
  file_write: "正在写入文件",
  office_create: "正在生成 Office 文件",
  write_file: "正在写入文件",
  file_edit: "正在修改文件",
  edit_file: "正在修改文件",
  str_replace_editor: "正在修改文件",
  file_patch: "正在修改文件",
  apply_patch: "正在修改文件",
  file_list_enhanced: "正在列出文件",
  glob_search: "正在搜索文件",
  glob: "正在搜索文件",
  content_search: "正在搜索内容",
  search: "正在搜索内容",
  grep: "正在搜索内容",
  shell: "正在执行命令",
  bash: "正在执行命令",
  run_command: "正在执行命令",
  Command: "正在执行命令",
  git_operations: "正在执行 Git 操作",
  git: "正在执行 Git 操作",
  delegate: "正在委托子任务",
  web_search: "正在网络搜索",
  web_fetch: "正在获取网页",
  http_request: "正在发送 HTTP 请求",
  memory_store: "正在存储记忆",
  memory_recall: "正在回忆记忆",
  knowledge_search: "正在检索知识库",
  browser: "正在操作浏览器",
  computer_use: "正在操作桌面",
  pdf_read: "正在读取 PDF",
};

const TOOL_COMPLETED_LABELS: Record<string, string> = {
  file_list: "列出文件",
  list_directory: "列出文件",
  file_read: "读取文件",
  read_file: "读取文件",
  file_write: "写入文件",
  office_create: "生成 Office 文件",
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
  shell: "命令执行完成",
  bash: "命令执行完成",
  run_command: "命令执行完成",
  Command: "命令执行完成",
  git_operations: "Git 操作完成",
  git: "Git 操作完成",
  delegate: "委托子任务完成",
  web_search: "网络搜索完成",
  web_fetch: "网页获取完成",
  http_request: "HTTP 请求完成",
  memory_store: "记忆存储完成",
  memory_recall: "记忆回忆完成",
  knowledge_search: "知识库检索完成",
  browser: "浏览器操作完成",
  computer_use: "桌面操作完成",
  pdf_read: "PDF 读取完成",
};

export function getToolRunningLabel(toolName: string): string {
  const label = TOOL_RUNNING_LABELS[toolName];
  if (label) return label;
  return `正在调用工具：${toolName || "unknown"}`;
}

export function getToolCompletedLabel(toolName: string): string {
  const label = TOOL_COMPLETED_LABELS[toolName];
  if (label) return label;
  return `工具执行完成：${toolName || "unknown"}`;
}

export function getToolFailedLabel(toolName: string): string {
  const completed = TOOL_COMPLETED_LABELS[toolName];
  if (completed) return `${completed}失败`;
  return `工具执行失败：${toolName || "unknown"}`;
}

export const MODEL_STARTED_LABEL = "等待模型响应";
export const MODEL_STREAMING_LABEL = "正在接收模型输出";
export const MODEL_COMPLETED_LABEL = "模型响应完成";

/** Context Runtime Process labels. Runtime semantics stay in Core events. */
export const CONTEXT_PRESSURE_LABEL = "检测到上下文压力";
export const CONTEXT_MAINTENANCE_CONDITION_LABEL = "检测到上下文维护条件";
export const CONTEXT_PRUNING_STARTED_LABEL = "正在裁剪大型工具结果";
export const CONTEXT_PRUNING_COMPLETED_LABEL = "已裁剪大型工具结果";
export const CONTEXT_COMPACTION_STARTED_LABEL = "正在压缩上下文";
export const CONTEXT_COMPACTION_COMPLETED_LABEL = "上下文压缩完成";
export const CONTEXT_COMPACTION_FAILED_LABEL = "上下文压缩失败";
export const CONTEXT_COMPACTION_INCOMPLETE_LABEL = "上下文压缩未完成";
export const CONTEXT_PRUNING_INCOMPLETE_LABEL = "工具结果裁剪未完成";
export const CONTEXT_RECOVERY_STARTED_LABEL = "正在执行上下文恢复";
export const CONTEXT_RECOVERY_COMPLETED_LABEL = "上下文恢复完成";
export const CONTEXT_RECOVERY_FAILED_LABEL = "上下文恢复失败";
export const CONTEXT_RECOVERY_INCOMPLETE_LABEL = "上下文恢复未完成";
export const CONTEXT_SECOND_OVERFLOW_DETAIL = "自动维护后仍超过模型上下文容量";

const SECRET_PATTERNS: RegExp[] = [
  /sk-[A-Za-z0-9_-]{8,}/gi,
  /(?:bearer|authorization)\s+[A-Za-z0-9._~+/=-]+/gi,
  /(?:api[_-]?key|apikey|token|password|secret)\s*[=:]\s*[^\s,;"']+/gi,
];

export function sanitizeDisplayText(value: string): string {
  let output = value;
  for (const pattern of SECRET_PATTERNS) {
    output = output.replace(pattern, "[redacted]");
  }
  return output;
}

export interface ErrorPresentation {
  title: string;
  detail: string;
}

export interface ErrorPresentationHint {
  type?: string;
  toolName?: string;
}

export function getErrorPresentation(
  raw: string,
  hint: ErrorPresentationHint = {}
): ErrorPresentation {
  const detail = sanitizeDisplayText(raw || "").trim();
  const lower = detail.toLowerCase();

  if (hint.type === "run_cancelled" || hint.type === "cancelled") {
    return { title: "任务已取消", detail: detail || "任务已被用户取消。" };
  }

  if (hint.type === "run_failed" || hint.type === "error" || hint.type === "task_failed") {
    if (/(401|authentication|unauthorized|invalid api key|api key.*invalid|invalid.*api key)/i.test(lower)) {
      return { title: "模型认证失败", detail: detail || "API 返回 HTTP 401" };
    }
    if (/(429|rate.?limit|too many requests|请求受限)/i.test(lower)) {
      return { title: "模型请求受限", detail: detail || "API 返回 HTTP 429" };
    }
    if (/(timeout|timed out|超时)/i.test(lower)) {
      return { title: "模型请求超时", detail: detail };
    }
    if (/(connect|tls|unexpected eof|connection reset|connection closed|close_notify|socket|network|transport)/i.test(lower)) {
      return { title: "模型连接中断", detail: detail };
    }
    if (/(model_not_found|model.*not.*found|no.*model|模型不存在)/i.test(lower)) {
      return { title: "模型不存在或不可用", detail: detail };
    }
    // Context overflow is checked first, and the safety pattern below no longer
    // matches a bare "safety": the preflight error reports safety_reserve_tokens,
    // so an oversized request (a desktop screenshot, a long run) used to be
    // reported to the user as a model content block.
    if (/(contextbudgetexceeded|contextwindowexceeded|context_length_exceeded|context window|maximum context|上下文.*(超|溢出|过长))/i.test(lower)) {
      return { title: "上下文超出模型窗口", detail: detail };
    }
    if (/(content_filter|content moderation|safety (filter|polic|system)|内容被.*拦截)/i.test(lower)) {
      return { title: "内容被模型安全策略拦截", detail: detail };
    }
    if (/(permission|denied|not allowed|forbidden|blocked|权限不足)/i.test(lower)) {
      return { title: "权限不足或操作被拦截", detail: detail };
    }
    return { title: "任务执行失败", detail };
  }

  if (hint.toolName) {
    if (/(permission|denied|not allowed|forbidden|blocked|权限不足)/i.test(lower)) {
      return { title: "工具权限不足", detail };
    }
    if (/(not found|no such file|does not exist|不存在)/i.test(lower)) {
      return { title: "文件或目标不存在", detail };
    }
    if (/^(git|git_operations)$/.test(hint.toolName)) {
      if (/(not a git repository|status 128|diff exited with status 129)/i.test(lower)) {
        return { title: "Git 操作失败", detail: detail || "当前 Workspace 不是 Git 仓库，无法执行该操作。" };
      }
      return { title: "Git 操作失败", detail };
    }
    if (/(exit code|exited with status|退出码|exit status)/i.test(lower) || /^(shell|bash|run_command|Command)$/.test(hint.toolName)) {
      return { title: "命令执行失败", detail };
    }
    return { title: getToolFailedLabel(hint.toolName), detail };
  }

  return { title: "任务执行失败", detail };
}
