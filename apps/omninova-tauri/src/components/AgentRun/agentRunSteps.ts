import {
  MODEL_COMPLETED_LABEL,
  MODEL_STARTED_LABEL,
  MODEL_STREAMING_LABEL,
  getToolCompletedLabel,
  getToolFailedLabel,
  getToolRunningLabel,
} from "./executionPresentation";
import {
  applyContextLifecycleEvent,
  contextOperationKey,
  contextOperationToStep,
  incompleteTitle,
  isContextLifecycleEvent,
  isContextStep,
  isContextUsageEvent,
  type ContextOperationView,
} from "./contextLifecycle";
import type {
  AgentRunChangedFile,
  AgentRunEvent,
  AgentRunEventContextLifecycle,
  AgentRunStep,
  RunEvent,
} from "./types";

export type RawEvent = RunEvent | AgentRunEvent | Record<string, unknown>;

export interface AggregateStepsOptions {
  runId?: string | null;
  sessionId?: string | null;
}

function payloadOf(event: RawEvent): Record<string, unknown> {
  return event as Record<string, unknown>;
}

export function eventType(event: RawEvent): string {
  const value = payloadOf(event).type;
  return typeof value === "string" ? value : "unknown";
}

function stringField(event: RawEvent, key: string): string {
  const value = payloadOf(event)[key];
  return typeof value === "string" ? value : "";
}

function numberField(event: RawEvent, key: string): number {
  const value = payloadOf(event)[key];
  return typeof value === "number" ? value : 0;
}

function boolField(event: RawEvent, key: string, fallback = false): boolean {
  const value = payloadOf(event)[key];
  return typeof value === "boolean" ? value : fallback;
}

function diffField(event: RawEvent): { additions: number; deletions: number } | null {
  const value = payloadOf(event).diff_stats;
  if (!value || typeof value !== "object") return null;
  const diff = value as { additions?: number; deletions?: number };
  return {
    additions: diff.additions ?? 0,
    deletions: diff.deletions ?? 0,
  };
}

function hashText(text: string): string {
  let hash = 0;
  for (let i = 0; i < text.length; i += 1) {
    hash = (hash * 31 + text.charCodeAt(i)) | 0;
  }
  return String(hash);
}

export function eventKey(event: RawEvent): string {
  const type = eventType(event);
  const runId = stringField(event, "run_id");
  const toolCallId = stringField(event, "tool_call_id");
  if (type === "run_started") {
    return `run_started:${runId}`;
  }
  if (type === "run_completed" || type === "run_failed" || type === "run_cancelled" || type === "error") {
    return `${type}:${runId}`;
  }
  if (type === "approval_required" || type === "approval_approved" || type === "approval_rejected" || type === "approval_cancelled") {
    return `${type}:${runId}:${stringField(event, "approval_id")}`;
  }
  if (type === "model_started" || type === "model_completed") {
    return `${type}:${runId}:${stringField(event, "step_id")}:${hashText(stringField(event, "title"))}`;
  }
  if (type === "model_delta") {
    return `${type}:${runId}:${stringField(event, "step_id")}:${hashText(stringField(event, "content"))}`;
  }
  if (type === "tool_call_created") {
    return `${type}:${runId}:${toolCallId || stringField(event, "step_id")}:${hashText(stringField(event, "title"))}`;
  }
  if (type === "tool_started" || type === "toolStarted" || type === "tool_completed" || type === "toolCompleted") {
    return `${type}:${runId}:${toolCallId || stringField(event, "tool_name")}:${hashText(stringField(event, "summary") + stringField(event, "result_summary"))}`;
  }
  if (type === "command_output" || type === "commandOutput") {
    return `${type}:${runId}:${toolCallId}:${String(payloadOf(event).is_stderr)}:${hashText(stringField(event, "output"))}`;
  }
  if (type === "file_changed" || type === "fileChanged") {
    return `${type}:${runId}:${stringField(event, "path")}:${numberField(event, "additions")}:${numberField(event, "deletions")}`;
  }
  if (type === "patch_started") {
    return `${type}:${runId}:${toolCallId || stringField(event, "step_id")}:${stringField(event, "path")}`;
  }
  if (type === "patch_hunk") {
    return `${type}:${runId}:${toolCallId}:${stringField(event, "path")}:${numberField(event, "old_start")}:${numberField(event, "new_start")}:${hashText(stringField(event, "summary"))}`;
  }
  if (type === "patch_applied" || type === "patch_failed") {
    return `${type}:${runId}:${toolCallId}:${stringField(event, "path")}`;
  }
  if (isContextLifecycleEvent(type)) {
    const nested = payloadOf(event).event;
    const operationId =
      nested && typeof nested === "object" && typeof (nested as { operation_id?: unknown }).operation_id === "string"
        ? (nested as { operation_id: string }).operation_id
        : "";
    const kind = nested && typeof nested === "object" ? (nested as { kind?: { type?: unknown } }).kind : undefined;
    const kindType = typeof kind?.type === "string" ? kind.type : "";
    return `context_lifecycle:${runId}:${operationId}:${kindType}`;
  }
  if (isContextUsageEvent(type)) {
    const snapshot = payloadOf(event).snapshot as { request_revision?: unknown; measurement_kind?: unknown } | undefined;
    return `context_usage:${runId}:${String(snapshot?.request_revision ?? "")}:${String(snapshot?.measurement_kind ?? "")}`;
  }
  return `${type}:${runId}:${hashText(JSON.stringify(event))}`;
}

export function dedupeEvents(events: RawEvent[]): RawEvent[] {
  const seen = new Set<string>();
  const next: RawEvent[] = [];
  for (const event of events) {
    const key = eventKey(event);
    if (seen.has(key)) continue;
    seen.add(key);
    next.push(event);
  }
  return next;
}

function stepIdFor(event: RawEvent, fallbackIndex: number): string {
  const id = stringField(event, "tool_call_id");
  if (id) return id;
  const stepId = stringField(event, "step_id");
  if (stepId) return stepId;
  return `${stringField(event, "tool_name") || eventType(event)}:${fallbackIndex}`;
}

function cleanTitle(title: string, toolName: string): string {
  if (toolName === "model") {
    return title.trim() ? title : MODEL_STARTED_LABEL;
  }
  return getToolRunningLabel(toolName);
}

function completedTitle(_startTitle: string, toolName: string, success: boolean, _summary: string): string {
  if (success) return getToolCompletedLabel(toolName);
  return getToolFailedLabel(toolName);
}

function upsertChangedFile(files: AgentRunChangedFile[], file: AgentRunChangedFile): AgentRunChangedFile[] {
  const idx = files.findIndex((item) => item.path === file.path);
  if (idx < 0) return [...files, file];
  const next = files.slice();
  next[idx] = file;
  return next;
}

function summarizeStepDiff(step: AgentRunStep): { additions: number; deletions: number } {
  if (step.patch_hunks.length > 0) {
    return step.patch_hunks.reduce(
      (acc, hunk) => ({ additions: acc.additions + hunk.additions, deletions: acc.deletions + hunk.deletions }),
      { additions: 0, deletions: 0 }
    );
  }
  if (step.changed_files.length > 0) {
    return step.changed_files.reduce(
      (acc, file) => ({ additions: acc.additions + file.additions, deletions: acc.deletions + file.deletions }),
      { additions: 0, deletions: 0 }
    );
  }
  return { additions: step.additions, deletions: step.deletions };
}

function isRunTerminalType(type: string): boolean {
  return type === "run_completed" || type === "run_failed" || type === "run_cancelled" || type === "error";
}

function familyFromToolName(toolName: string): ContextOperationView["family"] {
  if (toolName === "context_pruning") return "pruning";
  if (toolName === "context_compaction") return "compaction";
  if (toolName === "context_overflow_recovery") return "overflow_recovery";
  return "pressure";
}

export function aggregateSteps(events: RawEvent[], options: AggregateStepsOptions = {}): AgentRunStep[] {
  const steps: AgentRunStep[] = [];
  const byId = new Map<string, AgentRunStep>();
  const contextViews = new Map<string, ContextOperationView>();
  let lastFileStepId: string | null = null;
  const identity = { runId: options.runId, sessionId: options.sessionId };
  let runTerminated = false;

  const ensureStep = (id: string, toolName: string, title: string): AgentRunStep => {
    const existing = byId.get(id);
    if (existing) return existing;
    const step: AgentRunStep = {
      id,
      tool_name: toolName,
      title: cleanTitle(title, toolName),
      status: "running",
      outputs: [],
      changed_files: [],
      patch_hunks: [],
      additions: 0,
      deletions: 0,
    };
    byId.set(id, step);
    steps.push(step);
    return step;
  };

  const replaceContextStep = (view: ContextOperationView) => {
    const step = contextOperationToStep(view);
    const existing = byId.get(step.id);
    if (existing) {
      existing.tool_name = step.tool_name;
      existing.title = step.title;
      existing.status = step.status;
      existing.duration_ms = step.duration_ms;
      existing.result_summary = step.result_summary;
      return;
    }
    byId.set(step.id, step);
    steps.push(step);
  };

  dedupeEvents(events).forEach((event, index) => {
    const type = eventType(event);
    const toolName = stringField(event, "tool_name");

    if (isRunTerminalType(type)) {
      runTerminated = true;
    }

    if (isContextUsageEvent(type)) {
      return;
    }

    if (isContextLifecycleEvent(type)) {
      const payload = event as unknown as AgentRunEventContextLifecycle;
      const operationId = payload.event?.operation_id;
      if (!operationId) return;
      const next = applyContextLifecycleEvent(contextViews.get(operationId), payload, identity);
      if (!next) return;
      contextViews.set(operationId, next);
      replaceContextStep(next);
      return;
    }

    if (type === "tool_started" || type === "toolStarted") {
      const id = stepIdFor(event, index);
      const title = stringField(event, "title") || stringField(event, "summary");
      const step = ensureStep(id, toolName, title);
      step.status = "running";
      if (title) step.title = cleanTitle(title, toolName);
      if (["file_write", "write_file", "file_edit", "edit_file", "str_replace_editor", "file_patch", "apply_patch"].includes(toolName)) {
        lastFileStepId = id;
      }
      return;
    }

    if (type === "model_started") {
      const id = stepIdFor(event, index);
      const step = ensureStep(id, "model", stringField(event, "title") || MODEL_STARTED_LABEL);
      step.status = "running";
      return;
    }

    if (type === "model_delta") {
      const id = stepIdFor(event, index);
      const step = ensureStep(id, "model", MODEL_STREAMING_LABEL);
      const content = stringField(event, "content");
      if (content && !step.outputs.includes(content)) step.outputs.push(content);
      return;
    }

    if (type === "model_completed") {
      const id = stepIdFor(event, index);
      const step = ensureStep(id, "model", stringField(event, "title") || MODEL_COMPLETED_LABEL);
      step.status = "success";
      step.title = stringField(event, "title") || MODEL_COMPLETED_LABEL;
      return;
    }

    if (type === "tool_call_created") {
      const id = stepIdFor(event, index);
      const step = ensureStep(id, toolName, getToolRunningLabel(toolName));
      step.status = "running";
      step.title = getToolRunningLabel(toolName);
      return;
    }

    if (type === "command_output" || type === "commandOutput") {
      const id = stepIdFor(event, index);
      const step = ensureStep(id, toolName, "");
      const output = stringField(event, "output") || stringField(event, "content");
      if (output && !step.outputs.includes(output)) step.outputs.push(output);
      return;
    }

    if (type === "patch_started") {
      const id = stepIdFor(event, index);
      const path = stringField(event, "path") || "文件";
      const step = ensureStep(id, "file_patch", `正在修改文件：${path}`);
      step.status = "running";
      step.title = `正在修改文件：${path}`;
      lastFileStepId = id;
      return;
    }

    if (type === "patch_hunk") {
      const id = stepIdFor(event, index);
      const path = stringField(event, "path") || "文件";
      const step = ensureStep(id, "file_patch", `正在修改文件：${path}`);
      const hunk = {
        path,
        old_start: numberField(event, "old_start"),
        old_lines: numberField(event, "old_lines"),
        new_start: numberField(event, "new_start"),
        new_lines: numberField(event, "new_lines"),
        additions: numberField(event, "additions"),
        deletions: numberField(event, "deletions"),
        summary: stringField(event, "summary") || "局部修改",
      };
      const exists = step.patch_hunks.some(
        (item) =>
          item.path === hunk.path &&
          item.old_start === hunk.old_start &&
          item.new_start === hunk.new_start &&
          item.summary === hunk.summary
      );
      if (!exists) step.patch_hunks.push(hunk);
      step.additions = step.patch_hunks.reduce((sum, item) => sum + item.additions, 0);
      step.deletions = step.patch_hunks.reduce((sum, item) => sum + item.deletions, 0);
      return;
    }

    if (type === "patch_applied") {
      const id = stepIdFor(event, index);
      const path = stringField(event, "path") || "文件";
      const step = ensureStep(id, "file_patch", `修改文件：${path}`);
      step.status = "success";
      step.title = `修改文件：${path}`;
      step.result_summary = stringField(event, "result_summary") || `已应用 ${numberField(event, "hunks_count")} 个 hunk`;
      step.changed_files = upsertChangedFile(step.changed_files, {
        path,
        additions: numberField(event, "additions"),
        deletions: numberField(event, "deletions"),
      });
      step.additions = numberField(event, "additions");
      step.deletions = numberField(event, "deletions");
      return;
    }

    if (type === "patch_failed") {
      const id = stepIdFor(event, index);
      const path = stringField(event, "path") || "文件";
      const step = ensureStep(id, "file_patch", `修改文件：${path}`);
      step.status = "error";
      step.result_summary = stringField(event, "error") || "patch failed";
      return;
    }

    if (type === "file_changed" || type === "fileChanged") {
      const targetId = stringField(event, "tool_call_id")
        ? stepIdFor(event, index)
        : lastFileStepId ?? stepIdFor(event, index);
      const step = ensureStep(targetId, "file_write", "正在写入文件");
      const file = {
        path: stringField(event, "path") || "文件",
        additions: numberField(event, "additions"),
        deletions: numberField(event, "deletions"),
      };
      step.changed_files = upsertChangedFile(step.changed_files, file);
      const diff = summarizeStepDiff(step);
      step.additions = diff.additions;
      step.deletions = diff.deletions;
      return;
    }

    if (type === "tool_completed" || type === "toolCompleted") {
      const id = stepIdFor(event, index);
      const step = ensureStep(id, toolName, "");
      const success = boolField(event, "success", true);
      const resultSummary = stringField(event, "result_summary");
      const diff = diffField(event);
      step.status = success ? "success" : "error";
      step.duration_ms = numberField(event, "duration_ms");
      step.result_summary = resultSummary;
      if (diff && step.changed_files.length === 0) {
        step.additions = diff.additions;
        step.deletions = diff.deletions;
      }
      step.title = completedTitle(step.title, toolName, success, resultSummary);
      return;
    }

    if (type === "approval_required") {
      const id = stepIdFor(event, index);
      const reason = stringField(event, "reason") || "该工具需要人工确认后才能继续。";
      const step = ensureStep(id, toolName, `等待审批：${getToolRunningLabel(toolName)}`);
      step.status = "warning";
      step.title = `等待审批：${getToolRunningLabel(toolName)}`;
      step.result_summary = reason;
      return;
    }

    if (type === "approval_approved") {
      const id = stepIdFor(event, index);
      const step = ensureStep(id, toolName, `用户已批准：${getToolRunningLabel(toolName)}`);
      step.status = "success";
      step.title = `用户已批准：${getToolRunningLabel(toolName)}`;
      return;
    }

    if (type === "approval_rejected") {
      const id = stepIdFor(event, index);
      const step = ensureStep(id, toolName, `用户已拒绝：${getToolRunningLabel(toolName)}`);
      step.status = "warning";
      step.title = `用户已拒绝：${getToolRunningLabel(toolName)}`;
      step.result_summary = stringField(event, "reason") || "用户拒绝执行该操作";
      return;
    }

    if (type === "approval_cancelled") {
      const id = stepIdFor(event, index);
      const step = ensureStep(id, toolName, `审批已取消：${getToolRunningLabel(toolName)}`);
      step.status = "warning";
      step.title = `审批已取消：${getToolRunningLabel(toolName)}`;
      step.result_summary = stringField(event, "reason") || "任务已停止，审批已取消";
      return;
    }
  });

  if (runTerminated) {
    for (const step of steps) {
      if (step.status !== "running" || !isContextStep(step)) continue;
      const family = familyFromToolName(step.tool_name);
      if (family === "pressure") continue;
      step.status = "warning";
      step.title = incompleteTitle(family);
      step.result_summary = undefined;
      step.duration_ms = undefined;
    }
  }

  return steps.map((step) => {
    const diff = summarizeStepDiff(step);
    return { ...step, additions: diff.additions, deletions: diff.deletions };
  });
}

export function processStepTitles(steps: AgentRunStep[]): string[] {
  return steps.map((step) => step.title);
}

export { contextOperationKey };
