import React, { memo, useState, useCallback } from "react";
import type { RunEvent } from "./types";
import { AgentRunEventCard } from "./AgentRunEventCard";

interface AgentRunTimelineProps {
  /** List of run events to display. */
  events: RunEvent[];
  /** Whether the agent is currently running. */
  isRunning?: boolean;
  /** Total elapsed seconds since run started. */
  elapsedSec?: number;
  /** Collapsed by default? */
  defaultCollapsed?: boolean;
}

export const AgentRunTimeline: React.FC<AgentRunTimelineProps> = memo(
  function AgentRunTimeline({
    events,
    isRunning = false,
    elapsedSec = 0,
    defaultCollapsed = false,
  }) {
    const [collapsed, setCollapsed] = useState(defaultCollapsed);

    const toggleCollapsed = useCallback(() => {
      setCollapsed((c) => !c);
    }, []);

    const startedEvents = events.filter((e) => e.type === "toolStarted").length;
    const completedEvents = events.filter((e) => e.type === "toolCompleted").length;
    const totalDiff = events.reduce(
      (acc, e) => {
        if (e.type === "toolCompleted" && e.diff_stats) {
          return { add: acc.add + e.diff_stats.additions, del: acc.del + e.diff_stats.deletions };
        }
        if (e.type === "fileChanged") {
          return { add: acc.add + e.additions, del: acc.del + e.deletions };
        }
        return acc;
      },
      { add: 0, del: 0 }
    );

    const formatTime = (sec: number) => {
      if (sec < 60) return `${sec}s`;
      return `${Math.floor(sec / 60)}m ${sec % 60}s`;
    };

    // Compute overall status
    const hasError = events.some((e) => e.type === "toolCompleted" && !e.success);
    const statusType = isRunning ? "running" : hasError ? "error" : "done";
    const statusIcon = statusType === "running" ? "⚡" : statusType === "error" ? "❌" : "✅";
    const statusText =
      statusType === "running"
        ? `执行中 ${formatTime(elapsedSec)} · ${completedEvents}/${startedEvents} 步`
        : statusType === "error"
        ? `失败 · ${completedEvents} 步`
        : `完成 · ${formatTime(elapsedSec)} · ${completedEvents} 步`;

    const headerColor =
      statusType === "running"
        ? "bg-blue-500/10 text-blue-300 border-blue-500/20"
        : statusType === "error"
        ? "bg-red-500/10 text-red-300 border-red-500/20"
        : "bg-white/5 text-white/60 border-white/10";

    return (
      <div
        className={`rounded-lg border transition-all ${headerColor} overflow-hidden`}
        style={{ maxWidth: "100%" }}
      >
        {/* Header / toggle */}
        <button
          onClick={toggleCollapsed}
          className="w-full flex items-center justify-between px-3 py-2 text-xs hover:bg-white/5 transition-colors cursor-pointer"
        >
          <span className="flex items-center gap-1.5">
            <span aria-hidden>{statusIcon}</span>
            <span className="font-medium">{statusText}</span>
            {totalDiff.add > 0 && (
              <span className="text-white/30">
                · <span className="text-green-400/70">+{totalDiff.add}</span>{" "}
                <span className="text-red-400/70">-{totalDiff.del}</span>
              </span>
            )}
          </span>
          <span className="text-white/30">
            {collapsed ? "▶" : "▼"} {events.length > 0 ? events.length : ""}
          </span>
        </button>

        {/* Expanded event list */}
        {!collapsed && events.length > 0 && (
          <div className="px-1 pb-1 space-y-0.5 max-h-64 overflow-y-auto">
            {events.map((event, idx) => (
              <AgentRunEventCard key={idx} event={event} index={idx} />
            ))}
          </div>
        )}

        {/* Running animation indicator */}
        {isRunning && collapsed && (
          <div className="px-3 pb-1">
            <span className="inline-block w-1.5 h-1.5 rounded-full bg-blue-400 animate-pulse" />
          </div>
        )}
      </div>
    );
  }
);
