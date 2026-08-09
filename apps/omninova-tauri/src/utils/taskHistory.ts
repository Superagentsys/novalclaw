export const TASK_HISTORY_STORAGE_KEY = "omninova-task-history-v1";

/** Max number of task records kept locally (older ones are dropped). */
const MAX_TASKS = 100;

export type TaskStatus = "running" | "completed" | "failed" | "cancelled";

/** One agent run initiated by the user, tracked for the 历史任务 panel. */
export interface TaskHistoryEntry {
  runId: string;
  /** User prompt that started the task (used as the title). */
  title: string;
  /** Chat avatar (session) the task belongs to. */
  avatarId: string;
  agentName: string;
  sessionId: string;
  status: TaskStatus;
  /** epoch ms */
  startedAt: number;
  /** epoch ms */
  endedAt?: number;
  durationMs?: number;
  resultPreview?: string;
}

export function loadTaskHistory(): TaskHistoryEntry[] {
  try {
    const raw = localStorage.getItem(TASK_HISTORY_STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed as TaskHistoryEntry[];
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
  running: "进行中",
  completed: "已完成",
  failed: "失败",
  cancelled: "已取消",
};

export function taskStatusLabel(status: TaskStatus): string {
  return STATUS_LABELS[status] ?? status;
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
