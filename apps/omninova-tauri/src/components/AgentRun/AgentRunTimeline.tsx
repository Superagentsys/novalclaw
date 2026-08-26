import React, { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listenAgentRunEvents } from "../../utils/events";
import { aggregateSteps, dedupeEvents, eventType, type RawEvent } from "./agentRunSteps";
import type { AgentRunEvent, AgentRunEventContextLifecycle, AgentRunStep, RunEvent } from "./types";
import { AgentRunEventCard } from "./AgentRunEventCard";
import { AgentDiffPanel } from "../AgentDiff/AgentDiffPanel";
import { buildAgentDiffState } from "../AgentDiff/diffStore";

interface CachedLiveRun {
  events: RawEvent[];
  startedAt: number;
  running: boolean;
  elapsedSec: number;
}

// The inspector can switch between Agents while several runs remain active.
// Keep a bounded per-run snapshot outside the component so remounting or
// rebinding the timeline never makes an existing run look as if it restarted.
const LIVE_RUN_CACHE = new Map<string, CachedLiveRun>();
const MAX_CACHED_RUNS = 24;
const MAX_CACHED_EVENTS = 800;

function cachedRun(runId: string): CachedLiveRun {
  const existing = LIVE_RUN_CACHE.get(runId);
  if (existing) return existing;
  const created = { events: [], startedAt: Date.now(), running: true, elapsedSec: 0 };
  LIVE_RUN_CACHE.set(runId, created);
  while (LIVE_RUN_CACHE.size > MAX_CACHED_RUNS) {
    const oldest = LIVE_RUN_CACHE.keys().next().value;
    if (!oldest) break;
    LIVE_RUN_CACHE.delete(oldest);
  }
  return created;
}

function cacheRunEvents(runId: string, events: RawEvent[], running = true): RawEvent[] {
  const snapshot = cachedRun(runId);
  const next = dedupeEvents([...snapshot.events, ...events]).slice(-MAX_CACHED_EVENTS);
  snapshot.events = next;
  snapshot.running = running;
  snapshot.elapsedSec = Math.max(snapshot.elapsedSec, (Date.now() - snapshot.startedAt) / 1000);
  return next;
}

interface AgentRunTimelineProps {
  events?: RunEvent[];
  isRunning?: boolean;
  elapsedSec?: number;
  defaultCollapsed?: boolean;
  liveSessionId?: string | null;
  sessionId?: string | null;
  onRunDone?: (success: boolean) => void;
}

function formatTime(sec: number): string {
  if (sec < 10) return `${sec.toFixed(1)}s`;
  if (sec < 60) return `${Math.round(sec)}s`;
  return `${Math.floor(sec / 60)}m ${Math.round(sec % 60)}s`;
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

function belongsToRun(payload: AgentRunEvent, runId: string, sessionId?: string | null): boolean {
  if (payload.run_id !== runId) return false;
  if (payload.type !== "context_lifecycle") return true;
  const lifecycle = payload as AgentRunEventContextLifecycle;
  const eventSession = lifecycle.event?.session_id;
  const eventRun = lifecycle.event?.run_id;
  if (eventRun && eventRun !== runId) return false;
  if (sessionId && eventSession && eventSession !== sessionId) return false;
  return true;
}

export const AgentRunTimeline: React.FC<AgentRunTimelineProps> = memo(
  function AgentRunTimeline({
    events = [],
    isRunning = false,
    elapsedSec = 0,
    defaultCollapsed = false,
    liveSessionId,
    sessionId,
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
      const snapshot = cachedRun(liveSessionId);
      const startTime = snapshot.startedAt;

      const flushModelDeltas = () => {
        if (pendingModelDeltas.size === 0) return;
        const flushed = Array.from(pendingModelDeltas.values());
        pendingModelDeltas.clear();
        setLiveEvents(() => cacheRunEvents(liveSessionId, flushed, true));
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
        setLiveEvents(snapshot.events);
        setIsLiveRunning(snapshot.running);
        setLiveElapsed(Math.max(snapshot.elapsedSec, (Date.now() - startTime) / 1000));
      });
      if (snapshot.running) {
        liveTimerRef.current = setInterval(() => {
          const elapsed = (Date.now() - startTime) / 1000;
          snapshot.elapsedSec = elapsed;
          setLiveElapsed(elapsed);
        }, 250);
      }

      listenAgentRunEvents<AgentRunEvent>("agent-run-event", (event) => {
        const payload = event.payload as AgentRunEvent;
        if (import.meta.env.DEV && payload.type !== "model_delta") {
          console.log("[agent-run-event payload]", event.payload);
          console.log("[agent-run-event-run-id]", payload.run_id);
        }

        if (disposed) return;
        if (payload.run_id !== liveSessionId) {
          // Preserve lifecycle/tool/file events for background Agents. Model
          // deltas are intentionally omitted here to keep the cache compact.
          if (payload.type !== "model_delta") {
            const terminal =
              payload.type === "run_completed" ||
              payload.type === "run_failed" ||
              payload.type === "run_cancelled" ||
              payload.type === "error";
            cacheRunEvents(payload.run_id, [payload], !terminal);
          }
          return;
        }
        if (!belongsToRun(payload, liveSessionId, sessionId)) return;

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
        setLiveEvents(() => cacheRunEvents(liveSessionId, [payload], !isTerminal));

        if (isTerminal) {
          terminalRunIdsRef.current.add(payload.run_id);
          snapshot.running = false;
          snapshot.elapsedSec = (Date.now() - startTime) / 1000;
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
    }, [liveSessionId, onRunDone, sessionId]);

    const rawEvents = useMemo(
      () => (liveSessionId ? liveEvents : dedupeEvents(events)),
      [events, liveEvents, liveSessionId]
    );
    const steps = useMemo(
      () =>
        aggregateSteps(rawEvents, {
          runId: liveSessionId ?? undefined,
          sessionId,
        }),
      [liveSessionId, rawEvents, sessionId]
    );
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
