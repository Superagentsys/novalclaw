import React, { memo, useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { AgentRunEvent, RunEvent } from "./types";
import { AgentRunEventCard } from "./AgentRunEventCard";

type DisplayEvent = RunEvent | AgentRunEvent | Record<string, unknown>;

interface AgentRunTimelineProps {
  events?: RunEvent[];
  isRunning?: boolean;
  elapsedSec?: number;
  defaultCollapsed?: boolean;
  liveSessionId?: string | null;
  onRunDone?: (success: boolean) => void;
}

function formatTime(sec: number) {
  if (sec < 60) return `${sec}s`;
  return `${Math.floor(sec / 60)}m ${sec % 60}s`;
}

function payloadOf(event: DisplayEvent): Record<string, unknown> {
  return event as Record<string, unknown>;
}

function eventTypeOf(event: DisplayEvent): string {
  const type = payloadOf(event).type;
  return typeof type === "string" ? type : "unknown";
}

function stringField(event: DisplayEvent, key: string): string {
  const value = payloadOf(event)[key];
  return typeof value === "string" ? value : "";
}

function eventToolCallId(event: DisplayEvent): string {
  return stringField(event, "tool_call_id");
}

function eventDiff(event: DisplayEvent): { additions: number; deletions: number } | null {
  const payload = payloadOf(event);
  const type = eventTypeOf(event);
  if ((type === "toolCompleted" || type === "tool_completed") && payload.diff_stats) {
    const diff = payload.diff_stats as { additions?: number; deletions?: number };
    return { additions: diff.additions ?? 0, deletions: diff.deletions ?? 0 };
  }
  if (type === "fileChanged" || type === "file_changed") {
    return {
      additions: typeof payload.additions === "number" ? payload.additions : 0,
      deletions: typeof payload.deletions === "number" ? payload.deletions : 0,
    };
  }
  return null;
}

function hashText(text: string): string {
  let hash = 0;
  for (let i = 0; i < text.length; i += 1) {
    hash = (hash * 31 + text.charCodeAt(i)) | 0;
  }
  return String(hash);
}

function eventKey(event: DisplayEvent): string {
  const type = eventTypeOf(event);
  const runId = stringField(event, "run_id");
  const toolCallId = eventToolCallId(event);
  if (type === "run_started" || type === "run_completed" || type === "error") {
    return `${type}:${runId}`;
  }
  if (type === "tool_started" || type === "toolStarted") {
    return `${type}:${runId}:${toolCallId || stringField(event, "tool_name")}`;
  }
  if (type === "tool_completed" || type === "toolCompleted") {
    return `${type}:${runId}:${toolCallId || stringField(event, "tool_name")}`;
  }
  if (type === "command_output" || type === "commandOutput") {
    return `${type}:${runId}:${toolCallId}:${String(payloadOf(event).is_final)}:${hashText(stringField(event, "output"))}`;
  }
  if (type === "file_changed" || type === "fileChanged") {
    return `${type}:${runId}:${stringField(event, "path")}:${payloadOf(event).additions}:${payloadOf(event).deletions}`;
  }
  return `${type}:${runId}:${JSON.stringify(event)}`;
}

function toDisplayEvent(event: AgentRunEvent | Record<string, unknown>): DisplayEvent {
  switch (event.type) {
    case "run_started":
    case "tool_started":
    case "tool_completed":
    case "command_output":
    case "file_changed":
    case "run_completed":
    case "error":
      return event;
    default:
      console.warn("[AgentRunTimeline] 未知事件:", event.type, event);
      return event;
  }
}

function upsertEvent(prev: DisplayEvent[], next: DisplayEvent): DisplayEvent[] {
  const nextType = eventTypeOf(next);
  const nextKey = eventKey(next);

  if (prev.some((event) => eventKey(event) === nextKey)) {
    return prev;
  }

  const toolCallId = eventToolCallId(next);
  if (nextType === "tool_completed" && toolCallId) {
    const startedIndex = prev.findIndex(
      (event) => eventTypeOf(event) === "tool_started" && eventToolCallId(event) === toolCallId
    );
    if (startedIndex >= 0) {
      return [...prev.slice(0, startedIndex), next, ...prev.slice(startedIndex + 1)];
    }
  }

  return [...prev, next];
}

function countToolSteps(events: DisplayEvent[]) {
  const total = new Set<string>();
  const completed = new Set<string>();
  events.forEach((event, index) => {
    const type = eventTypeOf(event);
    if (type !== "tool_started" && type !== "toolStarted" && type !== "tool_completed" && type !== "toolCompleted") {
      return;
    }
    const key = eventToolCallId(event) || `${stringField(event, "tool_name")}:${index}`;
    total.add(key);
    if (type === "tool_completed" || type === "toolCompleted") {
      completed.add(key);
    }
  });
  return { completed: completed.size, total: total.size };
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
    const [liveEvents, setLiveEvents] = useState<DisplayEvent[]>([]);
    const [isLiveRunning, setIsLiveRunning] = useState(false);
    const [liveElapsed, setLiveElapsed] = useState(0);
    const liveTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

    useEffect(() => {
      if (liveTimerRef.current) {
        clearInterval(liveTimerRef.current);
        liveTimerRef.current = null;
      }

      if (!liveSessionId) {
        setLiveEvents([]);
        setIsLiveRunning(false);
        setLiveElapsed(0);
        return;
      }

      let disposed = false;
      let unlisten: (() => void) | undefined;
      const startTime = Date.now();

      setLiveEvents([]);
      setIsLiveRunning(true);
      setLiveElapsed(0);
      liveTimerRef.current = setInterval(() => {
        setLiveElapsed(Math.floor((Date.now() - startTime) / 1000));
      }, 1000);

      listen<AgentRunEvent>("agent-run-event", (event) => {
        const payload = event.payload as AgentRunEvent;
        console.log("[agent-run-event payload]", event.payload);
        console.log("[agent-run-event-run-id]", payload.run_id);

        if (disposed || payload.run_id !== liveSessionId) return;

        const display = toDisplayEvent(payload);
        setLiveEvents((prev) => upsertEvent(prev, display));

        if (payload.type === "run_completed" || payload.type === "error") {
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
        if (liveTimerRef.current) {
          clearInterval(liveTimerRef.current);
          liveTimerRef.current = null;
        }
      };
    }, [liveSessionId, onRunDone]);

    const toggleCollapsed = useCallback(() => {
      setCollapsed((c) => !c);
    }, []);

    const allEvents: DisplayEvent[] = liveSessionId
      ? liveEvents
      : events.reduce<DisplayEvent[]>((acc, event) => upsertEvent(acc, event), []);
    const running = isRunning || isLiveRunning;
    const elapsed = liveSessionId ? liveElapsed : elapsedSec;
    const toolSteps = countToolSteps(allEvents);
    const totalDiff = allEvents.reduce<{ add: number; del: number }>(
      (acc, event) => {
        const diff = eventDiff(event);
        if (!diff) return acc;
        return { add: acc.add + diff.additions, del: acc.del + diff.deletions };
      },
      { add: 0, del: 0 }
    );

    const hasError = allEvents.some((event) => {
      const type = eventTypeOf(event);
      if (type === "error") return true;
      if (type === "toolCompleted" || type === "tool_completed") {
        return payloadOf(event).success === false;
      }
      return false;
    });

    const statusType = running ? "running" : hasError ? "error" : "done";
    const statusIcon = statusType === "running" ? "⏳" : statusType === "error" ? "✕" : "✓";
    const statusText =
      statusType === "running"
        ? `执行中 ${formatTime(elapsed)} · 已完成 ${toolSteps.completed}/${toolSteps.total} 步`
        : statusType === "error"
          ? `失败 · 共 ${toolSteps.total} 步 · 耗时 ${formatTime(elapsed)}`
          : `完成 · 共 ${toolSteps.total} 步 · 耗时 ${formatTime(elapsed)}`;

    const headerColor =
      statusType === "running"
        ? "bg-blue-500/10 text-blue-300 border-blue-500/20"
        : statusType === "error"
          ? "bg-red-500/10 text-red-300 border-red-500/20"
          : "bg-white/5 text-white/60 border-white/10";

    return (
      <div className={`rounded-lg border transition-all ${headerColor} overflow-hidden`} style={{ maxWidth: "100%" }}>
        <button
          onClick={toggleCollapsed}
          className="w-full flex items-center justify-between px-3 py-2 text-xs hover:bg-white/5 transition-colors cursor-pointer"
        >
          <span className="flex items-center gap-1.5">
            <span aria-hidden>{statusIcon}</span>
            <span className="font-medium">{statusText}</span>
            {(totalDiff.add > 0 || totalDiff.del > 0) && (
              <span className="text-white/30">
                · <span className="text-green-400/70">+{totalDiff.add}</span>{" "}
                <span className="text-red-400/70">-{totalDiff.del}</span>
              </span>
            )}
          </span>
          <span className="text-white/30">{collapsed ? "展开" : "收起"}</span>
        </button>

        {!collapsed && allEvents.length > 0 && (
          <div className="px-1 pb-1 space-y-0.5 max-h-64 overflow-y-auto">
            {allEvents.map((event) => (
              <AgentRunEventCard key={eventKey(event)} event={event} />
            ))}
          </div>
        )}

        {running && collapsed && (
          <div className="px-3 pb-1">
            <span className="inline-block w-1.5 h-1.5 rounded-full bg-blue-400 animate-pulse" />
          </div>
        )}
      </div>
    );
  }
);
