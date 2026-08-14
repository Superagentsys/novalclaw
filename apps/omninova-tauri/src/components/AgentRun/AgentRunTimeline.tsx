import React, { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listenAgentRunEvents } from "../../utils/events";
import type { AgentRunChangedFile, AgentRunEvent, AgentRunStep, RunEvent } from "./types";
import { AgentRunEventCard } from "./AgentRunEventCard";
import { AgentDiffPanel } from "../AgentDiff/AgentDiffPanel";
import { buildAgentDiffState } from "../AgentDiff/diffStore";

type RawEvent = RunEvent | AgentRunEvent | Record<string, unknown>;

interface AgentRunTimelineProps {
  events?: RunEvent[];
  isRunning?: boolean;
  elapsedSec?: number;
  defaultCollapsed?: boolean;
  liveSessionId?: string | null;
  onRunDone?: (success: boolean) => void;
}

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

function eventKey(event: RawEvent): string {
  const type = eventType(event);
  const runId = stringField(event, "run_id");
  const toolCallId = stringField(event, "tool_call_id");
  if (type === "run_started") {
    // Only one run_started per run_id — dedupe is authoritative here.
    return `run_started:${runId}`;
  }
  if (type === "run_completed" || type === "run_failed" || type === "run_cancelled" || type === "error") {
    return `${type}:${runId}`;
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
  return `${type}:${runId}:${hashText(JSON.stringify(event))}`;
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

function formatTime(sec: number): string {
  if (sec < 10) return `${sec.toFixed(1)}s`;
  if (sec < 60) return `${Math.round(sec)}s`;
  return `${Math.floor(sec / 60)}m ${Math.round(sec % 60)}s`;
}

function stepIdFor(event: RawEvent, fallbackIndex: number): string {
  const id = stringField(event, "tool_call_id");
  if (id) return id;
  const stepId = stringField(event, "step_id");
  if (stepId) return stepId;
  return `${stringField(event, "tool_name") || eventType(event)}:${fallbackIndex}`;
}

function cleanTitle(title: string, toolName: string): string {
  if (!title.trim()) {
    switch (toolName) {
      case "file_list":
      case "list_directory":
        return "正在列出目录";
      case "file_write":
      case "write_file":
        return "正在写入文件";
      case "git_operations":
      case "git":
        return "正在执行 Git 操作";
      case "shell":
      case "bash":
      case "run_command":
      case "Command":
        return "正在执行命令";
      default:
        return `正在执行工具：${toolName || "unknown"}`;
    }
  }
  return title;
}

function completedTitle(startTitle: string, toolName: string, success: boolean, summary: string): string {
  if (!success) return startTitle.replace(/^正在/, "") || summary;
  return startTitle
    .replace(/^正在列出目录/, "列出目录")
    .replace(/^正在列出文件/, "列出文件")
    .replace(/^正在读取文件/, "读取文件")
    .replace(/^正在写入文件/, "写入文件")
    .replace(/^正在编辑文件/, "编辑文件")
    .replace(/^正在搜索文件/, "搜索文件")
    .replace(/^正在搜索内容/, "搜索内容")
    .replace(/^正在执行命令/, "执行命令")
    .replace(/^正在执行 Git 操作/, "Git 操作")
    .replace(/^正在执行工具/, toolName || "工具");
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

function aggregateSteps(events: RawEvent[]): AgentRunStep[] {
  const steps: AgentRunStep[] = [];
  const byId = new Map<string, AgentRunStep>();
  let lastFileStepId: string | null = null;

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

  dedupeEvents(events).forEach((event, index) => {
    const type = eventType(event);
    const toolName = stringField(event, "tool_name");

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
      const step = ensureStep(id, "model", stringField(event, "title") || "正在分析请求");
      step.status = "running";
      return;
    }

    if (type === "model_delta") {
      const id = stepIdFor(event, index);
      const step = ensureStep(id, "model", "正在生成回复");
      const content = stringField(event, "content");
      if (content && !step.outputs.includes(content)) step.outputs.push(content);
      return;
    }

    if (type === "model_completed") {
      const id = stepIdFor(event, index);
      const step = ensureStep(id, "model", stringField(event, "title") || "模型阶段完成");
      step.status = "success";
      step.title = stringField(event, "title") || "模型阶段完成";
      return;
    }

    if (type === "tool_call_created") {
      const id = stepIdFor(event, index);
      const step = ensureStep(id, toolName, stringField(event, "title") || `准备调用工具：${toolName || "unknown"}`);
      step.status = "running";
      step.title = stringField(event, "title") || `准备调用工具：${toolName || "unknown"}`;
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
      const step = ensureStep(id, "file_patch", stringField(event, "title") || `准备修改 ${path}`);
      step.status = "running";
      step.title = stringField(event, "title") || `准备修改 ${path}`;
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
      const step = ensureStep(
        id,
        toolName,
        stringField(event, "title") || `需要授权：${toolName || "受限工具"}`
      );
      step.status = "warning";
      step.title = stringField(event, "title") || `需要授权：${toolName || "受限工具"}`;
      step.result_summary = reason;
      return;
    }
  });

  return steps.map((step) => {
    const diff = summarizeStepDiff(step);
    return { ...step, additions: diff.additions, deletions: diff.deletions };
  });
}

function overallStatus(events: RawEvent[], steps: AgentRunStep[], running: boolean) {
  const runCompleted = events.some((event) => eventType(event) === "run_completed");
  const runErrored = events.some((event) => eventType(event) === "error" || eventType(event) === "run_failed");
  const runCancelled = events.some((event) => eventType(event) === "run_cancelled");
  const failures = steps.filter((step) => step.status === "error").length + (runErrored ? 1 : 0);
  // run_completed / run_error are authoritative — once set they don't revert even if
  // isLiveRunning is still true in the same React render batch.
  if (runCompleted) return { type: "completed" as const, failures: 0 };
  if (runCancelled) return { type: "cancelled" as const, failures: 0 };
  if (runErrored) return { type: "partial" as const, failures };
  if (running) return { type: "running" as const, failures };
  if (failures > 0) return { type: "partial" as const, failures };
  return { type: "completed" as const, failures: 0 };
}

export const AgentRunTimeline: React.FC<AgentRunTimelineProps> = memo(
  function AgentRunTimeline({
    events = [],
    isRunning = false,
    elapsedSec = 0,
    defaultCollapsed = false,
    liveSessionId,
    onRunDone,
  }) {
    const [collapsed, setCollapsed] = useState(defaultCollapsed);
    const [liveEvents, setLiveEvents] = useState<RawEvent[]>([]);
    const [isLiveRunning, setIsLiveRunning] = useState(false);
    const [liveElapsed, setLiveElapsed] = useState(0);
    const liveTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
    const terminalRunIdsRef = useRef<Set<string>>(new Set());

    useEffect(() => {
      let disposed = false;

      if (liveTimerRef.current) {
        clearInterval(liveTimerRef.current);
        liveTimerRef.current = null;
      }

      if (!liveSessionId) {
        queueMicrotask(() => {
          if (disposed) return;
          setLiveEvents([]);
          setIsLiveRunning(false);
          setLiveElapsed(0);
        });
        return () => {
          disposed = true;
        };
      }

      let unlisten: (() => void) | undefined;
      const pendingModelDeltas = new Map<string, AgentRunEvent>();
      let deltaFlushTimer: ReturnType<typeof setTimeout> | null = null;
      const startTime = Date.now();

      const flushModelDeltas = () => {
        if (pendingModelDeltas.size === 0) return;
        const flushed = Array.from(pendingModelDeltas.values());
        pendingModelDeltas.clear();
        setLiveEvents((prev) => dedupeEvents([...prev, ...flushed]));
      };

      const scheduleDeltaFlush = () => {
        if (deltaFlushTimer) return;
        deltaFlushTimer = setTimeout(() => {
          deltaFlushTimer = null;
          flushModelDeltas();
        }, 150);
      };

      queueMicrotask(() => {
        if (disposed) return;
        setLiveEvents([]);
        setIsLiveRunning(true);
        setLiveElapsed(0);
      });
      liveTimerRef.current = setInterval(() => {
        setLiveElapsed((Date.now() - startTime) / 1000);
      }, 250);

      listenAgentRunEvents<AgentRunEvent>("agent-run-event", (event) => {
        const payload = event.payload as AgentRunEvent;
        if (import.meta.env.DEV && payload.type !== "model_delta") {
          console.log("[agent-run-event payload]", event.payload);
          console.log("[agent-run-event-run-id]", payload.run_id);
        }

        if (disposed || payload.run_id !== liveSessionId) return;
        const isTerminal =
          payload.type === "run_completed" ||
          payload.type === "run_failed" ||
          payload.type === "run_cancelled" ||
          payload.type === "error";
        if (terminalRunIdsRef.current.has(payload.run_id) && !isTerminal) {
          if (import.meta.env.DEV && payload.type !== "model_delta") {
            console.debug("[agent-run-event ignored after terminal]", payload);
          }
          return;
        }

        if (payload.type === "model_delta") {
          const key = `${payload.run_id}:${payload.step_id}`;
          const existing = pendingModelDeltas.get(key);
          pendingModelDeltas.set(key, {
            ...payload,
            content: `${existing?.type === "model_delta" ? existing.content : ""}${payload.content}`,
          });
          scheduleDeltaFlush();
          return;
        }

        flushModelDeltas();
        setLiveEvents((prev) => dedupeEvents([...prev, payload]));

        if (isTerminal) {
          terminalRunIdsRef.current.add(payload.run_id);
          setIsLiveRunning(false);
          if (liveTimerRef.current) {
            clearInterval(liveTimerRef.current);
            liveTimerRef.current = null;
          }
          onRunDone?.(payload.type === "run_completed");
        }
      }).then((fn) => {
        if (disposed) {
          fn();
        } else {
          unlisten = fn;
        }
      });

      return () => {
        disposed = true;
        unlisten?.();
        if (deltaFlushTimer) {
          clearTimeout(deltaFlushTimer);
          deltaFlushTimer = null;
        }
        if (liveTimerRef.current) {
          clearInterval(liveTimerRef.current);
          liveTimerRef.current = null;
        }
      };
    }, [liveSessionId, onRunDone]);

    const rawEvents = useMemo(
      () => (liveSessionId ? liveEvents : dedupeEvents(events)),
      [events, liveEvents, liveSessionId]
    );
    const steps = useMemo(() => aggregateSteps(rawEvents), [rawEvents]);
    const diffState = useMemo(() => buildAgentDiffState(rawEvents), [rawEvents]);
    const running = isRunning || isLiveRunning;
    const elapsed = liveSessionId ? liveElapsed : elapsedSec;
    const status = overallStatus(rawEvents, steps, running);
    const completedSteps = steps.filter((step) => step.status === "success" || step.status === "error").length;
    const totalDiff = steps.reduce(
      (acc, step) => ({ additions: acc.additions + step.additions, deletions: acc.deletions + step.deletions }),
      { additions: 0, deletions: 0 }
    );

    const statusText =
      status.type === "running"
        ? `执行中 · ${formatTime(elapsed)} · 已完成 ${completedSteps}/${steps.length} 步`
        : status.type === "cancelled"
          ? `已取消 · 共 ${steps.length} 步 · ${formatTime(elapsed)}`
        : status.type === "partial"
          ? `部分失败 · ${status.failures} 个失败 · ${steps.length} 步 · ${formatTime(elapsed)}`
          : `完成 · 共 ${steps.length} 步 · ${formatTime(elapsed)}`;

    const toggleCollapsed = useCallback(() => {
      setCollapsed((value) => !value);
    }, []);

    return (
      <section className={`agent-run-panel agent-run-panel--${status.type}`} aria-label="Agent 执行过程">
        <button type="button" className="agent-run-summary" onClick={toggleCollapsed}>
          <span className={`agent-run-summary-dot agent-run-summary-dot--${status.type}`} aria-hidden />
          <span className="agent-run-summary-text">{statusText}</span>
          {(totalDiff.additions > 0 || totalDiff.deletions > 0) && (
            <span className="agent-run-diff-badge">
              +{totalDiff.additions} -{totalDiff.deletions}
            </span>
          )}
          <span className="agent-run-summary-toggle">{collapsed ? "展开" : "收起"}</span>
        </button>

        {!collapsed && (
          <div className="agent-run-steps">
            <AgentDiffPanel diffState={diffState} />
            {steps.length > 0 ? (
              steps.map((step) => <AgentRunEventCard key={step.id} step={step} />)
            ) : (
              <div className="agent-run-empty">
                {running ? "正在等待工具调用…" : "本次运行没有工具步骤。"}
              </div>
            )}
          </div>
        )}
      </section>
    );
  }
);
