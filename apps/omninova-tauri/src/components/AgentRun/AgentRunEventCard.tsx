import React, { memo } from "react";
import type { RunEvent } from "./types";
import { getEventStatusLabel, getToolLabel } from "./types";

interface AgentRunEventCardProps {
  event: RunEvent;
  index: number;
}

/** Status icon for each event type. */
const StatusIcon = memo(function StatusIcon({ status }: { status: string }) {
  switch (status) {
    case "running":
      return (
        <span
          className="inline-block w-3.5 h-3.5 rounded-full border-2 border-blue-400 border-t-transparent animate-spin flex-shrink-0"
          aria-hidden
        />
      );
    case "success":
      return <span aria-hidden>✅</span>;
    case "error":
      return <span aria-hidden>❌</span>;
    default:
      return <span className="text-gray-400" aria-hidden>•</span>;
  }
});

export const AgentRunEventCard: React.FC<AgentRunEventCardProps> = memo(function AgentRunEventCard({
  event,
  index,
}) {
  const status = getEventStatusLabel(event);

  return (
    <div
      className="flex items-start gap-2 py-1.5 px-3 rounded-md bg-white/5 hover:bg-white/8 transition-colors text-xs group"
    >
      {/* Step number */}
      <span className="text-white/20 w-4 text-right flex-shrink-0 select-none">{index + 1}</span>

      {/* Status icon */}
      <StatusIcon status={status} />

      {/* Content */}
      <div className="flex-1 min-w-0">
        {event.type === "toolStarted" && (
          <span className="text-blue-300/80">{event.summary}</span>
        )}

        {event.type === "toolCompleted" && (
          <div className="space-y-0.5">
            <div className="flex items-center gap-2 flex-wrap">
              <span className={event.success ? "text-green-300" : "text-red-300"}>
                {event.success ? "✅" : "❌"}{" "}
                <span className="text-white/60">{getToolLabel(event.tool_name)}</span>
              </span>
              <span className="text-white/30">·</span>
              <span className="text-white/40">{event.duration_ms}ms</span>
              {event.diff_stats && (
                <>
                  <span className="text-white/30">·</span>
                  <span className="text-green-400/80">+{event.diff_stats.additions}</span>
                  <span className="text-red-400/80">-{event.diff_stats.deletions}</span>
                </>
              )}
            </div>
            {event.result_summary && event.result_summary.trim() && (
              <p
                className="text-white/40 truncate max-w-xs"
                title={event.result_summary}
              >
                {event.result_summary}
              </p>
            )}
          </div>
        )}

        {event.type === "fileChanged" && (
          <div className="flex items-center gap-2">
            <span aria-hidden>📝</span>
            <span className="text-white/70 truncate" title={event.path}>
              {event.path}
            </span>
            <span className="text-green-400/80">+{event.additions}</span>
            <span className="text-red-400/80">-{event.deletions}</span>
          </div>
        )}
      </div>
    </div>
  );
});
