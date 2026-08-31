export type ContextProgressRingState =
  | "normal"
  | "unknown"
  | "refreshing"
  | "compaction"
  | "calculating"
  | "unavailable";

export type ContextProgressRingTone = "neutral" | "normal" | "warning" | "critical" | "error";

export interface ContextProgressRingProps {
  percentage: number | null;
  state: ContextProgressRingState;
  tone: ContextProgressRingTone;
  size?: number;
}

export function normalizeContextProgressPercentage(percentage: number | null): number | null {
  if (percentage == null || !Number.isFinite(percentage)) return null;
  return Math.min(100, Math.max(0, percentage));
}

/**
 * Presentation-only SVG ring. Context semantics stay in ContextUsageBadge;
 * this component only renders a supplied determinate or transient state.
 */
export function ContextProgressRing({
  percentage,
  state,
  tone,
  size = 20,
}: ContextProgressRingProps) {
  const normalized = normalizeContextProgressPercentage(percentage);
  const determinate = normalized != null && state !== "unknown" && state !== "calculating";
  const displayedProgress = determinate ? normalized : 28;

  return (
    <React.Fragment>
      <span
        className={`context-progress-ring is-${state} tone-${tone}`}
        data-context-ring-state={state}
        data-context-ring-tone={tone}
        data-context-ring-determinate={determinate ? "true" : "false"}
        data-context-ring-progress={determinate ? String(normalized) : undefined}
        style={{ width: size, height: size }}
        aria-hidden="true"
      >
        <svg
          className="context-progress-ring__svg"
          viewBox="0 0 28 28"
          width={size}
          height={size}
          focusable="false"
        >
          <circle className="context-progress-ring__track" cx="14" cy="14" r="10.5" />
          <g className="context-progress-ring__motion">
            <circle
              className="context-progress-ring__value"
              cx="14"
              cy="14"
              r="10.5"
              pathLength="100"
              strokeDasharray={`${displayedProgress} ${100 - displayedProgress}`}
              transform="rotate(-90 14 14)"
            />
            <circle className="context-progress-ring__activity" cx="14" cy="3.5" r="1.6" />
          </g>
        </svg>
      </span>
    </React.Fragment>
  );
}
import React from "react";
