import type { AgentRunEvent, RunEvent } from "../AgentRun/types";
import type {
  AgentChangedFile,
  AgentDiffChangeType,
  AgentDiffFileStatus,
  AgentDiffHunk,
  AgentDiffLine,
  AgentDiffRunState,
} from "./types";

type RawEvent = RunEvent | AgentRunEvent | Record<string, unknown>;

function payloadOf(event: RawEvent): Record<string, unknown> {
  return event as Record<string, unknown>;
}

function eventType(event: RawEvent): string {
  const value = payloadOf(event).type;
  return typeof value === "string" ? value : "unknown";
}

function stringField(event: RawEvent, key: string): string {
  const value = payloadOf(event)[key];
  return typeof value === "string" ? value : "";
}

function optionalStringField(event: RawEvent, key: string): string | undefined {
  const value = payloadOf(event)[key];
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function numberField(event: RawEvent, key: string): number {
  const value = payloadOf(event)[key];
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function boolField(event: RawEvent, key: string): boolean {
  return payloadOf(event)[key] === true;
}

function diffField(event: RawEvent): { additions: number; deletions: number } | null {
  const value = payloadOf(event).diff_stats;
  if (!value || typeof value !== "object") return null;
  const diff = value as { additions?: unknown; deletions?: unknown };
  return {
    additions: typeof diff.additions === "number" ? diff.additions : 0,
    deletions: typeof diff.deletions === "number" ? diff.deletions : 0,
  };
}

function hashText(text: string): string {
  let hash = 0;
  for (let i = 0; i < text.length; i += 1) {
    hash = (hash * 31 + text.charCodeAt(i)) | 0;
  }
  return String(hash);
}

function eventKey(event: RawEvent): string {
  const type = eventType(event);
  const runId = stringField(event, "run_id");
  const stepId = stringField(event, "step_id");
  const toolCallId = stringField(event, "tool_call_id");
  const path = stringField(event, "path");

  if (type === "patch_hunk") {
    return [
      runId,
      type,
      stepId,
      toolCallId,
      path,
      numberField(event, "old_start"),
      numberField(event, "new_start"),
      numberField(event, "additions"),
      numberField(event, "deletions"),
      hashText(stringField(event, "summary")),
    ].join(":");
  }

  if (type === "file_changed" || type === "fileChanged") {
    return [
      runId,
      type,
      stepId,
      toolCallId,
      path,
      numberField(event, "additions"),
      numberField(event, "deletions"),
    ].join(":");
  }

  if (type === "patch_started" || type === "patch_applied" || type === "patch_failed") {
    return [runId, type, stepId, toolCallId, path].join(":");
  }

  if (type === "tool_completed" || type === "toolCompleted") {
    return [
      runId,
      type,
      stepId,
      toolCallId,
      stringField(event, "tool_name"),
      hashText(stringField(event, "result_summary")),
    ].join(":");
  }

  if (type === "run_completed" || type === "run_failed" || type === "run_cancelled") {
    return [runId, type].join(":");
  }

  return [runId, type, stepId, toolCallId, path, hashText(JSON.stringify(event))].join(":");
}

function dedupeEvents(events: RawEvent[]): RawEvent[] {
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

function normalizePath(path: string): string {
  return path.replace(/\\/g, "/").replace(/^\.\//, "") || ".";
}

function detectChangeType(event: RawEvent): AgentDiffChangeType {
  const raw = stringField(event, "change_type").toLowerCase();
  if (raw === "created" || raw === "added") return "added";
  if (raw === "deleted" || raw === "removed") return "deleted";
  if (raw === "modified") return "modified";

  const additions = numberField(event, "additions");
  const deletions = numberField(event, "deletions");
  if (additions > 0 && deletions === 0) return "added";
  if (deletions > 0 && additions === 0) return "deleted";
  if (additions > 0 || deletions > 0) return "modified";
  return "unknown";
}

function splitLines(text: string): string[] {
  if (text === "") return [];
  const lines = text.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
  if (lines.length > 1 && lines[lines.length - 1] === "") lines.pop();
  return lines;
}

function buildLineDiff(
  oldText: string | undefined,
  newText: string | undefined,
  oldStart: number,
  newStart: number
): AgentDiffLine[] {
  if (typeof oldText !== "string" || typeof newText !== "string") return [];

  const oldLines = splitLines(oldText);
  const newLines = splitLines(newText);
  let prefix = 0;

  while (
    prefix < oldLines.length &&
    prefix < newLines.length &&
    oldLines[prefix] === newLines[prefix]
  ) {
    prefix += 1;
  }

  let oldSuffix = oldLines.length;
  let newSuffix = newLines.length;
  while (
    oldSuffix > prefix &&
    newSuffix > prefix &&
    oldLines[oldSuffix - 1] === newLines[newSuffix - 1]
  ) {
    oldSuffix -= 1;
    newSuffix -= 1;
  }

  const lines: AgentDiffLine[] = [];
  for (let i = 0; i < prefix; i += 1) {
    lines.push({
      type: "context",
      oldLine: oldStart + i,
      newLine: newStart + i,
      content: oldLines[i],
    });
  }

  for (let i = prefix; i < oldSuffix; i += 1) {
    lines.push({
      type: "remove",
      oldLine: oldStart + i,
      content: oldLines[i],
    });
  }

  for (let i = prefix; i < newSuffix; i += 1) {
    lines.push({
      type: "add",
      newLine: newStart + i,
      content: newLines[i],
    });
  }

  for (let i = oldSuffix; i < oldLines.length; i += 1) {
    const suffixIndex = i - oldSuffix;
    lines.push({
      type: "context",
      oldLine: oldStart + i,
      newLine: newStart + newSuffix + suffixIndex,
      content: oldLines[i],
    });
  }

  return lines;
}

function addToolCallId(file: AgentChangedFile, toolCallId: string) {
  if (toolCallId && !file.toolCallIds.includes(toolCallId)) {
    file.toolCallIds.push(toolCallId);
  }
}

function ensureFile(
  state: AgentDiffRunState,
  path: string,
  status: AgentDiffFileStatus,
  timestamp: number
): AgentChangedFile {
  const normalized = normalizePath(path);
  const existing = state.files[normalized];
  if (existing) {
    if (
      status === "completed" ||
      status === "failed" ||
      status === "interrupted" ||
      status === "active" ||
      existing.status === "pending"
    ) {
      existing.status = status;
    }
    existing.lastEventAt = timestamp;
    return existing;
  }

  const file: AgentChangedFile = {
    path: normalized,
    changeType: "unknown",
    additions: 0,
    deletions: 0,
    status,
    hunks: [],
    lastEventAt: timestamp,
    toolCallIds: [],
  };
  state.files[normalized] = file;
  state.orderedPaths.push(normalized);
  return file;
}

function hunkId(event: RawEvent): string {
  return [
    stringField(event, "tool_call_id"),
    normalizePath(stringField(event, "path")),
    numberField(event, "old_start"),
    numberField(event, "new_start"),
    numberField(event, "additions"),
    numberField(event, "deletions"),
    hashText(stringField(event, "summary")),
  ].join(":");
}

function syntheticFileWriteHunk(event: RawEvent, path: string, changeType: AgentDiffChangeType): AgentDiffHunk | null {
  const oldText = optionalStringField(event, "old_text") ?? "";
  const newText = optionalStringField(event, "new_text") ?? "";
  const hasPreview = oldText.length > 0 || newText.length > 0;
  if (!hasPreview) return null;

  const oldLines = splitLines(oldText).length;
  const newLines = splitLines(newText).length;
  const oldStart = oldLines > 0 ? 1 : 0;
  const newStart = newLines > 0 ? 1 : 0;
  const summary =
    changeType === "added"
      ? "新建文件"
      : changeType === "deleted"
        ? "删除文件"
        : "整文件写入";

  return {
    id: [
      "synthetic-file-write",
      stringField(event, "run_id"),
      stringField(event, "tool_call_id"),
      normalizePath(path),
      numberField(event, "additions"),
      numberField(event, "deletions"),
    ].join(":"),
    source: "file_write",
    path: normalizePath(path),
    oldStart,
    oldLines,
    newStart,
    newLines,
    additions: numberField(event, "additions"),
    deletions: numberField(event, "deletions"),
    summary,
    oldText,
    newText,
    textTruncated: boolField(event, "content_truncated"),
    contentTotalChars: numberField(event, "content_total_chars") || undefined,
    contentPreviewChars: numberField(event, "content_preview_chars") || undefined,
    lines: buildLineDiff(oldText, newText, oldStart || 1, newStart || 1),
  };
}

function recomputeFileStats(file: AgentChangedFile) {
  if (file.hunks.length > 0) {
    file.additions = file.hunks.reduce((sum, hunk) => sum + hunk.additions, 0);
    file.deletions = file.hunks.reduce((sum, hunk) => sum + hunk.deletions, 0);
    file.summaryOnly = false;
  }
}

function markOpenFiles(state: AgentDiffRunState, status: AgentDiffFileStatus) {
  for (const path of state.orderedPaths) {
    const file = state.files[path];
    if (file.status === "active" || file.status === "pending") {
      file.status = status;
    }
  }
}

export function buildAgentDiffState(events: RawEvent[]): AgentDiffRunState | null {
  let state: AgentDiffRunState | null = null;

  const ensureState = (runId: string): AgentDiffRunState => {
    if (state) return state;
    state = {
      runId,
      files: {},
      orderedPaths: [],
      totals: { files: 0, additions: 0, deletions: 0 },
    };
    return state;
  };

  dedupeEvents(events).forEach((event, index) => {
    const type = eventType(event);
    const runId = stringField(event, "run_id");
    if (!runId) return;

    if (type === "patch_started") {
      const path = stringField(event, "path");
      if (!path) return;
      const current = ensureState(runId);
      const file = ensureFile(current, path, "active", index);
      addToolCallId(file, stringField(event, "tool_call_id"));
      current.activePath = file.path;
      return;
    }

    if (type === "patch_hunk") {
      const path = stringField(event, "path");
      if (!path) return;
      const current = ensureState(runId);
      const file = ensureFile(current, path, "active", index);
      const id = hunkId(event);
      if (file.hunks.some((hunk) => hunk.id === id)) return;

      const oldText = optionalStringField(event, "old_text");
      const newText = optionalStringField(event, "new_text");
      const oldStart = numberField(event, "old_start");
      const newStart = numberField(event, "new_start");
      const hunk: AgentDiffHunk = {
        id,
        source: "patch",
        path: file.path,
        oldStart,
        oldLines: numberField(event, "old_lines"),
        newStart,
        newLines: numberField(event, "new_lines"),
        additions: numberField(event, "additions"),
        deletions: numberField(event, "deletions"),
        summary: stringField(event, "summary") || "局部修改",
        oldText,
        newText,
        textTruncated: boolField(event, "text_truncated"),
        lines: buildLineDiff(oldText, newText, oldStart || 1, newStart || oldStart || 1),
      };
      file.hunks.push(hunk);
      file.changeType = file.changeType === "unknown" ? "modified" : file.changeType;
      addToolCallId(file, stringField(event, "tool_call_id"));
      recomputeFileStats(file);
      current.activePath = file.path;
      return;
    }

    if (type === "patch_applied") {
      const path = stringField(event, "path");
      if (!path) return;
      const current = ensureState(runId);
      const file = ensureFile(current, path, "completed", index);
      file.status = "completed";
      file.changeType = file.changeType === "unknown" ? "modified" : file.changeType;
      addToolCallId(file, stringField(event, "tool_call_id"));
      if (file.hunks.length === 0) {
        file.additions = numberField(event, "additions");
        file.deletions = numberField(event, "deletions");
        file.summaryOnly = true;
      } else {
        recomputeFileStats(file);
      }
      return;
    }

    if (type === "patch_failed") {
      const path = stringField(event, "path");
      if (!path) return;
      const current = ensureState(runId);
      const file = ensureFile(current, path, "failed", index);
      file.status = "failed";
      addToolCallId(file, stringField(event, "tool_call_id"));
      return;
    }

    if (type === "file_changed" || type === "fileChanged") {
      const path = stringField(event, "path");
      if (!path) return;
      const current = ensureState(runId);
      const file = ensureFile(current, path, "completed", index);
      addToolCallId(file, stringField(event, "tool_call_id"));
      if (file.changeType === "unknown") file.changeType = detectChangeType(event);
      if (file.hunks.length === 0) {
        const synthetic = syntheticFileWriteHunk(event, file.path, file.changeType);
        if (synthetic) {
          file.hunks.push(synthetic);
          recomputeFileStats(file);
        } else {
          file.additions = numberField(event, "additions");
          file.deletions = numberField(event, "deletions");
          file.summaryOnly = true;
        }
      }
      return;
    }

    if (type === "tool_completed" || type === "toolCompleted") {
      const current = state;
      if (!current) return;
      const diff = diffField(event);
      if (!diff) return;
      const toolCallId = stringField(event, "tool_call_id");
      const file = current.orderedPaths
        .map((path) => current.files[path])
        .find((item) => item.toolCallIds.includes(toolCallId));
      if (file && file.hunks.length === 0 && file.additions === 0 && file.deletions === 0) {
        file.additions = diff.additions;
        file.deletions = diff.deletions;
        file.summaryOnly = true;
      }
      return;
    }

    if (type === "run_completed") {
      const current = state ?? ensureState(runId);
      current.terminalStatus = "completed";
      markOpenFiles(current, "completed");
      return;
    }

    if (type === "run_failed") {
      const current = state ?? ensureState(runId);
      current.terminalStatus = "failed";
      markOpenFiles(current, "failed");
      return;
    }

    if (type === "run_cancelled") {
      const current = state ?? ensureState(runId);
      current.terminalStatus = "cancelled";
      markOpenFiles(current, "interrupted");
    }
  });

  const finalState = state as AgentDiffRunState | null;
  if (!finalState || finalState.orderedPaths.length === 0) return null;

  for (const path of finalState.orderedPaths) {
    recomputeFileStats(finalState.files[path]);
  }

  finalState.orderedPaths.sort((a, b) => a.localeCompare(b, undefined, { numeric: true, sensitivity: "base" }));

  finalState.totals = finalState.orderedPaths.reduce(
    (totals: AgentDiffRunState["totals"], path: string) => {
      const file = finalState.files[path];
      return {
        files: totals.files + 1,
        additions: totals.additions + file.additions,
        deletions: totals.deletions + file.deletions,
      };
    },
    { files: 0, additions: 0, deletions: 0 }
  );

  return finalState;
}
