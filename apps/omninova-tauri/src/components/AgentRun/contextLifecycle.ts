/**
 * Context Runtime Process presentation.
 *
 * Core ContextLifecycle events are the only source of pruning/compaction/
 * recovery rows. ContextUsage is ignored here.
 */

import {
  CONTEXT_COMPACTION_COMPLETED_LABEL,
  CONTEXT_COMPACTION_FAILED_LABEL,
  CONTEXT_COMPACTION_INCOMPLETE_LABEL,
  CONTEXT_COMPACTION_STARTED_LABEL,
  CONTEXT_MAINTENANCE_CONDITION_LABEL,
  CONTEXT_PRESSURE_LABEL,
  CONTEXT_PRUNING_COMPLETED_LABEL,
  CONTEXT_PRUNING_INCOMPLETE_LABEL,
  CONTEXT_PRUNING_STARTED_LABEL,
  CONTEXT_RECOVERY_COMPLETED_LABEL,
  CONTEXT_RECOVERY_FAILED_LABEL,
  CONTEXT_RECOVERY_INCOMPLETE_LABEL,
  CONTEXT_RECOVERY_STARTED_LABEL,
  CONTEXT_SECOND_OVERFLOW_DETAIL,
} from "./executionPresentation";
import type {
  AgentRunEventContextLifecycle,
  AgentRunStep,
  AgentRunStepStatus,
  ContextLifecycleEvent,
  ContextLifecycleEventKind,
} from "./types";
import type { TaskActivityEntry } from "../../utils/taskHistory";
import { formatEstimatedTokens, formatWindowTokens } from "./contextTokens";

export { formatEstimatedTokens, formatWindowTokens };

export type ContextOperationFamily =
  | "pressure"
  | "pruning"
  | "compaction"
  | "overflow_recovery";

export type ContextOperationState = "running" | "completed" | "failed" | "incomplete";

export interface ContextIdentityFilter {
  runId?: string | null;
  sessionId?: string | null;
}

export interface ContextOperationView {
  operationId: string;
  runId: string | null;
  sessionId: string | null;
  family: ContextOperationFamily;
  state: ContextOperationState;
  title: string;
  detail?: string;
  durationMs?: number;
  startedAt?: number;
  finishedAt?: number;
}

const TERMINAL_STATES = new Set<ContextOperationState>(["completed", "failed", "incomplete"]);

export function contextOperationKey(operationId: string): string {
  return `context:${operationId}`;
}

export function isContextStep(step: Pick<AgentRunStep, "tool_name" | "id">): boolean {
  return step.tool_name.startsWith("context_") || step.id.startsWith("context:");
}

function kindType(kind: ContextLifecycleEventKind | undefined): string {
  return typeof kind?.type === "string" ? kind.type : "";
}

function kindRecord(kind: ContextLifecycleEventKind): Record<string, unknown> {
  return kind as Record<string, unknown>;
}

function optionalNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value : undefined;
}

function familyForKind(type: string): ContextOperationFamily | null {
  if (type === "context_pressure_detected") return "pressure";
  if (type === "context_pruning_started" || type === "context_pruning_completed") return "pruning";
  if (
    type === "context_compaction_started" ||
    type === "context_compaction_completed" ||
    type === "context_compaction_failed"
  ) {
    return "compaction";
  }
  if (
    type === "context_overflow_recovery_started" ||
    type === "context_overflow_recovery_completed" ||
    type === "context_overflow_recovery_failed"
  ) {
    return "overflow_recovery";
  }
  return null;
}

function incomingState(type: string): ContextOperationState | null {
  if (type === "context_pressure_detected") return "completed";
  if (type.endsWith("_started")) return "running";
  if (type.endsWith("_completed")) return "completed";
  if (type.endsWith("_failed")) return "failed";
  return null;
}

function isTerminalState(state: ContextOperationState): boolean {
  return TERMINAL_STATES.has(state);
}

export function formatCoreDurationMs(durationMs: number): string {
  if (durationMs < 1000) return `${Math.max(0, Math.round(durationMs))}ms`;
  const seconds = durationMs / 1000;
  if (seconds < 10) return `${seconds.toFixed(1)}s`;
  return `${Math.round(seconds)}s`;
}

function pressureTitle(mode: unknown): string {
  return mode === "unknown_budget_oversize" ? CONTEXT_MAINTENANCE_CONDITION_LABEL : CONTEXT_PRESSURE_LABEL;
}

function startedTitle(family: ContextOperationFamily): string {
  if (family === "pruning") return CONTEXT_PRUNING_STARTED_LABEL;
  if (family === "compaction") return CONTEXT_COMPACTION_STARTED_LABEL;
  if (family === "overflow_recovery") return CONTEXT_RECOVERY_STARTED_LABEL;
  return CONTEXT_PRESSURE_LABEL;
}

function failedTitle(family: ContextOperationFamily): string {
  if (family === "pruning") return CONTEXT_PRUNING_COMPLETED_LABEL;
  if (family === "compaction") return CONTEXT_COMPACTION_FAILED_LABEL;
  return CONTEXT_RECOVERY_FAILED_LABEL;
}

export function incompleteTitle(family: ContextOperationFamily): string {
  if (family === "pruning") return CONTEXT_PRUNING_INCOMPLETE_LABEL;
  if (family === "compaction") return CONTEXT_COMPACTION_INCOMPLETE_LABEL;
  if (family === "overflow_recovery") return CONTEXT_RECOVERY_INCOMPLETE_LABEL;
  return CONTEXT_PRESSURE_LABEL;
}

function sanitizedFailureDetail(family: ContextOperationFamily, reason: string | undefined): string | undefined {
  if (!reason) return undefined;
  if (reason === "context_window_exceeded") return CONTEXT_SECOND_OVERFLOW_DETAIL;
  if (reason === "non_shrinking") {
    return family === "compaction" ? "压缩后上下文未能缩小" : "维护后上下文未能缩小";
  }
  if (reason === "provider_error") return "摘要生成失败";
  if (reason === "empty_summary") return "摘要为空";
  if (reason === "retry_error") return "上下文恢复重试失败";
  return undefined;
}

function joinMeta(parts: Array<string | undefined>): string | undefined {
  const present = parts.filter((part): part is string => Boolean(part));
  return present.length ? present.join(" · ") : undefined;
}

function tokenRange(before: number | undefined, after: number | undefined): string | undefined {
  if (before === undefined || after === undefined) return undefined;
  return `${formatEstimatedTokens(before)} → ${formatEstimatedTokens(after)}`;
}

function pressureDetail(kind: Record<string, unknown>): string | undefined {
  const estimated = optionalNumber(kind.estimated_before);
  const window = optionalNumber(kind.context_window_tokens);
  if (estimated === undefined) return undefined;
  if (window === undefined) return formatEstimatedTokens(estimated);
  return `${formatEstimatedTokens(estimated)} / ${formatWindowTokens(window)}`;
}

function pruningCompletedDetail(kind: Record<string, unknown>): string | undefined {
  const count = optionalNumber(kind.pruned_tool_result_count);
  const range = tokenRange(optionalNumber(kind.estimated_before), optionalNumber(kind.estimated_after));
  return joinMeta([
    count === undefined ? undefined : `${count} 个结果`,
    range,
  ]);
}

function presentationForKind(
  family: ContextOperationFamily,
  state: ContextOperationState,
  kind: Record<string, unknown>,
  durationMs?: number
): { title: string; detail?: string } {
  const mode = kind.mode;
  if (state === "running") {
    return { title: startedTitle(family) };
  }
  if (state === "failed") {
    return {
      title: failedTitle(family),
      detail: sanitizedFailureDetail(family, optionalString(kind.reason)),
    };
  }
  if (state === "incomplete") {
    return { title: incompleteTitle(family) };
  }
  if (family === "pressure") {
    return { title: pressureTitle(mode), detail: pressureDetail(kind) };
  }
  if (family === "pruning") {
    return { title: CONTEXT_PRUNING_COMPLETED_LABEL, detail: pruningCompletedDetail(kind) };
  }
  if (family === "compaction") {
    return {
      title: CONTEXT_COMPACTION_COMPLETED_LABEL,
      detail:       joinMeta([
        tokenRange(optionalNumber(kind.estimated_before), optionalNumber(kind.estimated_after)),
        durationMs && durationMs > 0 ? formatCoreDurationMs(durationMs) : undefined,
      ]),
    };
  }
  return {
    title: CONTEXT_RECOVERY_COMPLETED_LABEL,
    detail: durationMs && durationMs > 0 ? formatCoreDurationMs(durationMs) : undefined,
  };
}

function matchesIdentity(
  event: ContextLifecycleEvent,
  envelopeRunId: string | undefined,
  identity: ContextIdentityFilter
): boolean {
  if (identity.runId) {
    if (envelopeRunId && envelopeRunId !== identity.runId) return false;
    if (event.run_id && event.run_id !== identity.runId) return false;
  }
  if (identity.sessionId && event.session_id && event.session_id !== identity.sessionId) {
    return false;
  }
  return true;
}

function durationFrom(startedAt?: number, finishedAt?: number): number | undefined {
  if (startedAt === undefined || finishedAt === undefined) return undefined;
  if (finishedAt < startedAt) return undefined;
  return finishedAt - startedAt;
}

export function applyContextLifecycleEvent(
  current: ContextOperationView | undefined,
  payload: AgentRunEventContextLifecycle,
  identity: ContextIdentityFilter = {}
): ContextOperationView | null {
  const event = payload.event;
  if (!event || typeof event.operation_id !== "string" || !event.operation_id) {
    return current ?? null;
  }
  if (!matchesIdentity(event, payload.run_id, identity)) {
    return null;
  }

  const type = kindType(event.kind);
  const family = familyForKind(type);
  const state = incomingState(type);
  if (!family || !state) {
    return current ?? null;
  }

  const kind = kindRecord(event.kind);
  const timestamp = optionalNumber(event.timestamp);

  if (current && isTerminalState(current.state) && state === "running") {
    return current;
  }
  if (current && isTerminalState(current.state) && isTerminalState(state)) {
    return current;
  }

  const startedAt =
    state === "running"
      ? timestamp ?? current?.startedAt
      : current?.startedAt ?? (state === "completed" && family === "pressure" ? timestamp : current?.startedAt);
  const finishedAt = isTerminalState(state) ? timestamp ?? current?.finishedAt : current?.finishedAt;
  const durationMs = durationFrom(startedAt, finishedAt);
  const presented = presentationForKind(family, state, kind, durationMs);

  return {
    operationId: event.operation_id,
    runId: event.run_id ?? payload.run_id ?? current?.runId ?? null,
    sessionId: event.session_id ?? current?.sessionId ?? null,
    family,
    state,
    title: presented.title,
    detail: presented.detail,
    durationMs,
    startedAt,
    finishedAt,
  };
}

export function reduceContextLifecycleEvents(
  events: Array<AgentRunEventContextLifecycle | { type?: string; run_id?: string; event?: ContextLifecycleEvent }>,
  identity: ContextIdentityFilter = {},
  runTerminated = false
): ContextOperationView[] {
  const order: string[] = [];
  const byId = new Map<string, ContextOperationView>();

  for (const raw of events) {
    if (raw.type !== "context_lifecycle" || !raw.event) continue;
    const payload = raw as AgentRunEventContextLifecycle;
    const existing = byId.get(payload.event.operation_id);
    const next = applyContextLifecycleEvent(existing, payload, identity);
    if (!next) continue;
    if (!byId.has(next.operationId)) {
      order.push(next.operationId);
    }
    byId.set(next.operationId, next);
  }

  return order.map((id) => {
    const view = byId.get(id)!;
    if (runTerminated && view.state === "running") {
      return {
        ...view,
        state: "incomplete",
        title: incompleteTitle(view.family),
        detail: undefined,
        durationMs: undefined,
      };
    }
    return view;
  });
}

function familyToolName(family: ContextOperationFamily): string {
  if (family === "pressure") return "context_pressure";
  if (family === "pruning") return "context_pruning";
  if (family === "compaction") return "context_compaction";
  return "context_overflow_recovery";
}

function stepStatus(state: ContextOperationState): AgentRunStepStatus {
  if (state === "running") return "running";
  if (state === "failed") return "error";
  if (state === "incomplete") return "warning";
  return "success";
}

export function contextOperationToStep(view: ContextOperationView): AgentRunStep {
  return {
    id: contextOperationKey(view.operationId),
    tool_name: familyToolName(view.family),
    title: view.title,
    status: stepStatus(view.state),
    duration_ms: view.durationMs,
    result_summary: view.detail,
    outputs: [],
    changed_files: [],
    patch_hunks: [],
    additions: 0,
    deletions: 0,
  };
}

function activityStatus(state: ContextOperationState): TaskActivityEntry["status"] {
  if (state === "running") return "running";
  if (state === "failed") return "failed";
  if (state === "incomplete") return "waiting";
  return "completed";
}

function activityTone(state: ContextOperationState): TaskActivityEntry["tone"] {
  if (state === "running") return "info";
  if (state === "failed") return "error";
  if (state === "incomplete") return "warning";
  return "success";
}

export function applyLifecycleToActivity(
  activity: TaskActivityEntry[],
  payload: AgentRunEventContextLifecycle,
  identity: ContextIdentityFilter
): TaskActivityEntry[] {
  const operationId = payload.event?.operation_id;
  if (!operationId) return activity;
  const key = contextOperationKey(operationId);
  const existingIndex = activity.findIndex((item) => item.operationId === key);
  const existing = existingIndex >= 0 ? activity[existingIndex] : undefined;
  const currentView: ContextOperationView | undefined = existing
    ? {
        operationId,
        runId: identity.runId ?? null,
        sessionId: identity.sessionId ?? null,
        family:
          existing.toolName === "context_pruning"
            ? "pruning"
            : existing.toolName === "context_compaction"
              ? "compaction"
              : existing.toolName === "context_overflow_recovery"
                ? "overflow_recovery"
                : "pressure",
        state:
          existing.status === "running"
            ? "running"
            : existing.status === "failed"
              ? "failed"
              : existing.status === "waiting" && existing.label.includes("未完成")
                ? "incomplete"
                : "completed",
        title: existing.label,
        detail: existing.detail,
        startedAt: existing.at,
      }
    : undefined;
  const next = applyContextLifecycleEvent(currentView, payload, identity);
  if (!next) return activity;
  const entry: TaskActivityEntry = {
    at: existing?.at ?? payload.event.timestamp ?? Date.now(),
    label: next.title,
    tone: activityTone(next.state),
    kind: "context",
    status: activityStatus(next.state),
    detail: next.detail,
    toolName: familyToolName(next.family),
    operationId: key,
  };
  if (existingIndex >= 0) {
    const copy = activity.slice();
    copy[existingIndex] = { ...copy[existingIndex], ...entry, at: copy[existingIndex].at };
    return copy;
  }
  return [...activity, entry];
}

export function finalizeOpenContextOperations(activity: TaskActivityEntry[]): TaskActivityEntry[] {
  return activity.map((item) => {
    if (item.kind !== "context" || item.status !== "running") return item;
    const family: ContextOperationFamily =
      item.toolName === "context_pruning"
        ? "pruning"
        : item.toolName === "context_compaction"
          ? "compaction"
          : item.toolName === "context_overflow_recovery"
            ? "overflow_recovery"
            : "pressure";
    if (family === "pressure") return item;
    return {
      ...item,
      label: incompleteTitle(family),
      status: "waiting",
      tone: "warning",
      detail: undefined,
    };
  });
}

export function isContextUsageEvent(type: string): boolean {
  return type === "context_usage";
}

export function isContextLifecycleEvent(type: string): boolean {
  return type === "context_lifecycle";
}
