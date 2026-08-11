export const TASK_HISTORY_STORAGE_KEY = "omninova-task-history-v1";

/** Max number of task records kept locally (older ones are dropped). */
const MAX_TASKS = 100;

export type TaskStatus =
  | "needs_approval"
  | "waiting_input"
  | "running"
  | "completed"
  | "failed"
  | "cancelled"
  | "interrupted";

export interface TaskChangedFile {
  path: string;
  additions: number;
  deletions: number;
  changeType?: "created" | "modified" | "deleted" | "added" | "removed";
}

export interface TaskActivityEntry {
  at: number;
  label: string;
  tone?: "info" | "success" | "warning" | "error";
  /** Compact process category. Raw model deltas and command output are never stored. */
  kind?: "lifecycle" | "model" | "tool" | "file" | "approval";
  status?: "running" | "completed" | "failed" | "waiting";
  detail?: string;
  toolName?: string;
  path?: string;
}

/** One agent run initiated by the user, tracked for the 历史任务 panel. */
export interface TaskHistoryEntry {
  runId: string;
  /** User prompt that started the task (used as the title). */
  title: string;
  /** Chat avatar (session) the task belongs to. */
  avatarId: string;
  agentName: string;
  sessionId: string;
  /** Effective workspace used by this run, for resolving relative artifact paths. */
  workspacePath?: string;
  status: TaskStatus;
  /** epoch ms */
  startedAt: number;
  /** epoch ms */
  endedAt?: number;
  durationMs?: number;
  resultPreview?: string;
  changedFiles?: TaskChangedFile[];
  activity?: TaskActivityEntry[];
  attentionReason?: string;
  approvalTool?: string;
  nextAction?: string;
}

export function loadTaskHistory(): TaskHistoryEntry[] {
  try {
    const raw = localStorage.getItem(TASK_HISTORY_STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    const now = Date.now();
    return (parsed as TaskHistoryEntry[]).map((entry) => {
      // A persisted active state cannot still be backed by a live Tauri run
      // after the application has restarted. Mark it explicitly instead of
      // leaving a permanent "进行中" record.
      if (
        entry.status === "running" ||
        entry.status === "needs_approval" ||
        entry.status === "waiting_input"
      ) {
        return {
          ...entry,
          status: "interrupted" as const,
          endedAt: entry.endedAt ?? now,
          durationMs: entry.durationMs ?? Math.max(0, now - entry.startedAt),
          attentionReason: "应用已关闭或任务连接已中断，请检查后重新执行。",
          nextAction: "重新执行任务",
          activity: [
            ...(entry.activity ?? []),
            {
              at: now,
              label: "应用重新启动，未完成任务已标记为中断",
              tone: "warning" as const,
              kind: "lifecycle" as const,
              status: "failed" as const,
            },
          ].slice(-120),
        };
      }
      return entry;
    });
  } catch {
    return [];
  }
}

export function saveTaskHistory(list: TaskHistoryEntry[]): void {
  try {
    localStorage.setItem(
      TASK_HISTORY_STORAGE_KEY,
      JSON.stringify(list.slice(0, MAX_TASKS)),
    );
  } catch {
    // localStorage 满或不可用时忽略
  }
}

/** Prepend a new task; keeps newest first and caps the list length. */
export function addTask(
  list: TaskHistoryEntry[],
  entry: TaskHistoryEntry,
): TaskHistoryEntry[] {
  return [entry, ...list.filter((t) => t.runId !== entry.runId)].slice(
    0,
    MAX_TASKS,
  );
}

/** Patch an existing task by runId (no-op if it is not present). */
export function patchTask(
  list: TaskHistoryEntry[],
  runId: string,
  patch: Partial<TaskHistoryEntry>,
): TaskHistoryEntry[] {
  return list.map((t) => (t.runId === runId ? { ...t, ...patch } : t));
}

const STATUS_LABELS: Record<TaskStatus, string> = {
  needs_approval: "需要授权",
  waiting_input: "等待输入",
  running: "进行中",
  completed: "已完成",
  failed: "失败",
  cancelled: "已取消",
  interrupted: "已中断",
};

export function taskStatusLabel(status: TaskStatus): string {
  return STATUS_LABELS[status] ?? status;
}

export function taskNeedsAttention(status: TaskStatus): boolean {
  return (
    status === "needs_approval" ||
    status === "waiting_input" ||
    status === "failed" ||
    status === "interrupted"
  );
}

export function formatTaskTime(ms: number): string {
  if (!ms) return "";
  const d = new Date(ms);
  const now = new Date();
  const sameDay =
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate();
  const time = d.toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
  if (sameDay) return time;
  const date = d.toLocaleDateString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
  });
  return `${date} ${time}`;
}

export function formatDuration(ms?: number): string {
  if (ms == null || ms < 0) return "";
  if (ms < 1000) return `${ms}ms`;
  const totalSec = Math.round(ms / 1000);
  if (totalSec < 60) return `${totalSec}s`;
  const min = Math.floor(totalSec / 60);
  const sec = totalSec % 60;
  return `${min}m${sec.toString().padStart(2, "0")}s`;
}
