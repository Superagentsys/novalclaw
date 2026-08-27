/**
 * Pure ContextUsage reducer.
 *
 * ContextUsage snapshots are quantitative telemetry. Lifecycle status is
 * operational truth from typed ContextLifecycle events only.
 */

import {
  CONTEXT_COMPACTION_STARTED_LABEL,
  CONTEXT_PRUNING_STARTED_LABEL,
  CONTEXT_RECOVERY_STARTED_LABEL,
} from "./executionPresentation";
import {
  clampPercent,
  formatTokenCount,
  primaryUsagePercent,
} from "./contextTokens";
import type {
  AgentRunEventContextLifecycle,
  ContextLifecycleEvent,
  ContextMeasurementKind,
  ContextMeasurementProvenance,
  ContextUsageBreakdown,
  ContextUsageSnapshot,
} from "./types";

export interface ContextUsageIdentity {
  sessionId: string;
  provider: string;
  model: string;
  liveRunId: string | null;
}

export interface ContextUsageActual {
  inputTokens: number;
  revision: number;
  runId: string | null;
  provider: string;
  model: string;
}

export interface ContextUsageState {
  identity: ContextUsageIdentity;
  current: ContextUsageSnapshot | null;
  lastActual: ContextUsageActual | null;
  compactionOperationId: string | null;
  pruningOperationId: string | null;
  recoveryOperationId: string | null;
}

export type ContextActiveStatus = "compaction" | "pruning" | "recovery" | null;

export interface ContextUsageParity {
  localTokens: number;
  actualTokens: number;
  delta: number;
  absError: number;
  relativeErrorPercent: number;
}

export interface ContextUsageView {
  placeholder: boolean;
  knownBudget: boolean;
  estimatedTokens: number | null;
  maxInputTokens: number | null;
  contextWindowTokens: number | null;
  outputReserveTokens: number | null;
  pressureThresholdTokens: number | null;
  percent: number | null;
  barPercent: number | null;
  thresholdPercent: number | null;
  breakdown: ContextUsageBreakdown | null;
  measurementKind: ContextMeasurementKind | null;
  measurementProvenance: ContextMeasurementProvenance | null;
  measurementExact: boolean;
  measurementLabel: string;
  lastActualTokens: number | null;
  actualIsCurrent: boolean;
  actualLabel: string | null;
  totalFormat: "estimate" | "exact";
  revision: number | null;
  activeStatus: ContextActiveStatus;
  activeStatusLabel: string | null;
  compactText: string;
  parity: ContextUsageParity | null;
}

const KIND_RANK: Record<ContextMeasurementKind, number> = {
  candidate_estimate: 0,
  final_request_estimate: 1,
  provider_actual: 2,
};

export function emptyContextUsageState(identity: ContextUsageIdentity): ContextUsageState {
  return {
    identity: { ...identity },
    current: null,
    lastActual: null,
    compactionOperationId: null,
    pruningOperationId: null,
    recoveryOperationId: null,
  };
}

export function identityKey(identity: Pick<ContextUsageIdentity, "sessionId" | "provider" | "model">): string {
  return `${identity.sessionId}\0${identity.provider}\0${identity.model}`;
}

export function switchContextUsageIdentity(
  state: ContextUsageState,
  identity: ContextUsageIdentity
): ContextUsageState {
  if (identityKey(state.identity) === identityKey(identity)) {
    if (state.identity.liveRunId === identity.liveRunId) return state;
    return { ...state, identity: { ...identity } };
  }
  return emptyContextUsageState(identity);
}

export function setContextUsageLiveRun(
  state: ContextUsageState,
  liveRunId: string | null
): ContextUsageState {
  if (state.identity.liveRunId === liveRunId) return state;
  return {
    ...state,
    identity: { ...state.identity, liveRunId },
  };
}

/**
 * A run terminal event is authoritative: an operation that never emitted its
 * own terminal lifecycle event must no longer be presented as active.
 * Quantitative snapshots remain visible until a newer authoritative snapshot
 * arrives or the session/model identity changes.
 */
export function finishContextUsageRun(
  state: ContextUsageState,
  runId: string
): ContextUsageState {
  if (state.identity.liveRunId && state.identity.liveRunId !== runId) {
    return state;
  }
  return {
    ...state,
    identity: { ...state.identity, liveRunId: null },
    compactionOperationId: null,
    pruningOperationId: null,
    recoveryOperationId: null,
  };
}

export function snapshotMatchesIdentity(
  snapshot: Pick<ContextUsageSnapshot, "session_id" | "provider" | "model">,
  identity: ContextUsageIdentity
): boolean {
  if (snapshot.session_id && snapshot.session_id !== identity.sessionId) return false;
  if (identity.provider && snapshot.provider && snapshot.provider !== identity.provider) return false;
  if (identity.model && snapshot.model && snapshot.model !== identity.model) return false;
  return true;
}

function estimateKind(kind: ContextMeasurementKind): boolean {
  return kind === "candidate_estimate" || kind === "final_request_estimate";
}

function shouldTakeEstimate(
  incoming: ContextUsageSnapshot,
  current: ContextUsageSnapshot | null,
  liveRunId: string | null
): boolean {
  if (!current) return true;
  const incomingRun = incoming.run_id ?? null;
  const currentRun = current.run_id ?? null;
  if (incomingRun && currentRun && incomingRun !== currentRun) {
    if (liveRunId && incomingRun === liveRunId) return true;
    if (liveRunId && currentRun === liveRunId) return false;
    return false;
  }
  if (incoming.request_revision > current.request_revision) return true;
  if (incoming.request_revision < current.request_revision) return false;
  return KIND_RANK[incoming.measurement_kind] > KIND_RANK[current.measurement_kind];
}

function shouldTakeActual(
  incoming: ContextUsageActual,
  lastActual: ContextUsageActual | null,
  liveRunId: string | null,
  current: ContextUsageSnapshot | null
): boolean {
  if (!lastActual) return true;
  const incomingRun = incoming.runId;
  if (incomingRun && lastActual.runId && incomingRun !== lastActual.runId) {
    if (liveRunId && incomingRun === liveRunId) return true;
    if (liveRunId && lastActual.runId === liveRunId) return false;
    if (current?.run_id && incomingRun === current.run_id) return true;
    if (current?.run_id && lastActual.runId === current.run_id && incomingRun !== current.run_id) {
      return false;
    }
    return false;
  }
  return incoming.revision >= lastActual.revision;
}

export function applyContextUsageSnapshot(
  state: ContextUsageState,
  snapshot: ContextUsageSnapshot
): ContextUsageState {
  if (!snapshotMatchesIdentity(snapshot, state.identity)) {
    return state;
  }
  if (
    state.identity.liveRunId &&
    snapshot.run_id &&
    snapshot.run_id !== state.identity.liveRunId
  ) {
    return state;
  }

  if (snapshot.measurement_kind === "provider_actual") {
    const actual: ContextUsageActual = {
      inputTokens: snapshot.provider_actual_input_tokens ?? 0,
      revision: snapshot.request_revision,
      runId: snapshot.run_id ?? null,
      provider: snapshot.provider,
      model: snapshot.model,
    };
    if (actual.inputTokens <= 0) return state;
    if (!shouldTakeActual(actual, state.lastActual, state.identity.liveRunId, state.current)) {
      return state;
    }
    return { ...state, lastActual: actual };
  }

  if (!estimateKind(snapshot.measurement_kind)) {
    return state;
  }
  if (!shouldTakeEstimate(snapshot, state.current, state.identity.liveRunId)) {
    return state;
  }
  return { ...state, current: snapshot };
}

function lifecycleKindType(event: ContextLifecycleEvent | undefined): string {
  return typeof event?.kind?.type === "string" ? event.kind.type : "";
}

function lifecycleMatches(event: ContextLifecycleEvent, identity: ContextUsageIdentity): boolean {
  if (event.session_id && event.session_id !== identity.sessionId) return false;
  if (identity.liveRunId && event.run_id && event.run_id !== identity.liveRunId) return false;
  return true;
}

export function applyContextUsageLifecycle(
  state: ContextUsageState,
  payload: AgentRunEventContextLifecycle
): ContextUsageState {
  const event = payload.event;
  if (!event || !lifecycleMatches(event, state.identity)) return state;
  const type = lifecycleKindType(event);
  const operationId = event.operation_id;
  if (!operationId) return state;

  if (type === "context_compaction_started") {
    return { ...state, compactionOperationId: operationId };
  }
  if (type === "context_compaction_completed" || type === "context_compaction_failed") {
    if (state.compactionOperationId && state.compactionOperationId !== operationId) return state;
    return { ...state, compactionOperationId: null };
  }
  if (type === "context_pruning_started") {
    return { ...state, pruningOperationId: operationId };
  }
  if (type === "context_pruning_completed") {
    if (state.pruningOperationId && state.pruningOperationId !== operationId) return state;
    return { ...state, pruningOperationId: null };
  }
  if (type === "context_overflow_recovery_started") {
    return { ...state, recoveryOperationId: operationId };
  }
  if (type === "context_overflow_recovery_completed" || type === "context_overflow_recovery_failed") {
    if (state.recoveryOperationId && state.recoveryOperationId !== operationId) return state;
    return { ...state, recoveryOperationId: null };
  }
  return state;
}

export function measurementLabel(
  kind: ContextMeasurementKind | null,
  exact: boolean
): string {
  if (exact) {
    return kind === "final_request_estimate" ? "发送前精确计数" : "精确计数";
  }
  if (kind === "final_request_estimate") return "发送前保守估算";
  if (kind === "provider_actual") return "实际";
  return "当前保守估算";
}

function snapshotProvenance(snapshot: ContextUsageSnapshot | null): ContextMeasurementProvenance | null {
  if (!snapshot) return null;
  return snapshot.measurement_provenance ?? "safety_estimate";
}

function actualMatchesCurrent(
  actual: ContextUsageActual | null,
  current: ContextUsageSnapshot | null
): boolean {
  if (!actual || !current) return false;
  if (actual.revision !== current.request_revision) return false;
  if (actual.runId && current.run_id && actual.runId !== current.run_id) return false;
  if (actual.provider && current.provider && actual.provider !== current.provider) return false;
  if (actual.model && current.model && actual.model !== current.model) return false;
  return true;
}

export function selectTokenizerParity(
  current: ContextUsageSnapshot | null,
  actual: ContextUsageActual | null
): ContextUsageParity | null {
  if (!current || !actual) return null;
  if (current.measurement_provenance !== "exact_tokenizer") return null;
  if (!actualMatchesCurrent(actual, current)) return null;
  if (actual.inputTokens <= 0) return null;
  const localTokens = current.estimated_input_tokens;
  const actualTokens = actual.inputTokens;
  const delta = localTokens - actualTokens;
  const absError = Math.abs(delta);
  return {
    localTokens,
    actualTokens,
    delta,
    absError,
    relativeErrorPercent: (absError / actualTokens) * 100,
  };
}

export function formatParityRelativeError(percent: number): string {
  return `${percent.toFixed(2).replace(/\.?0+$/, "")}%`.replace(/^%$/, "0%");
}

function activeStatus(state: ContextUsageState): { status: ContextActiveStatus; label: string | null } {
  if (state.compactionOperationId) {
    return { status: "compaction", label: CONTEXT_COMPACTION_STARTED_LABEL };
  }
  if (state.pruningOperationId) {
    return { status: "pruning", label: CONTEXT_PRUNING_STARTED_LABEL };
  }
  if (state.recoveryOperationId) {
    return { status: "recovery", label: CONTEXT_RECOVERY_STARTED_LABEL };
  }
  return { status: null, label: null };
}

function compactStatusShort(status: ContextActiveStatus): string | null {
  if (status === "compaction") return "正在压缩";
  if (status === "pruning") return "正在裁剪";
  if (status === "recovery") return "正在恢复";
  return null;
}

export function selectContextUsageView(state: ContextUsageState): ContextUsageView {
  const current = state.current;
  const estimatedTokens = current?.estimated_input_tokens ?? null;
  const maxInputTokens = current?.max_input_tokens ?? null;
  const knownBudget = maxInputTokens != null && maxInputTokens > 0;
  const percent = estimatedTokens == null ? null : primaryUsagePercent(estimatedTokens, maxInputTokens);
  const threshold =
    knownBudget && current?.pressure_threshold_tokens != null && current.pressure_threshold_tokens > 0
      ? clampPercent((current.pressure_threshold_tokens / (maxInputTokens as number)) * 100)
      : null;
  const active = activeStatus(state);
  const statusShort = compactStatusShort(active.status);

  const provenance = snapshotProvenance(current);
  const measurementExact = current?.measurement_exact === true;
  const totalFormat: "estimate" | "exact" = measurementExact ? "exact" : "estimate";
  const matchesCurrentActual = actualMatchesCurrent(state.lastActual, current);
  const lastActualTokens = state.lastActual?.inputTokens ?? null;
  const actualLabel =
    lastActualTokens == null ? null : matchesCurrentActual ? "当前请求实际" : "上次请求实际";
  const parity = selectTokenizerParity(current, state.lastActual);

  let compactText = "上下文输入 —";
  if (estimatedTokens != null) {
    const totalText = formatTokenCount(estimatedTokens, totalFormat);
    if (!knownBudget || maxInputTokens == null || percent == null) {
      compactText = `上下文估算 ${totalText} · 窗口未知`;
    } else {
      compactText = `上下文输入  ${percent}% · ${totalText} / ${formatTokenCount(maxInputTokens, "exact")}`;
    }
  }
  if (statusShort) {
    compactText = estimatedTokens == null
      ? `上下文输入  — · ${statusShort}`
      : knownBudget && maxInputTokens != null && percent != null
        ? `上下文输入  ${percent}% · ${formatTokenCount(estimatedTokens, totalFormat)} / ${formatTokenCount(maxInputTokens, "exact")} · ${statusShort}`
        : `上下文估算 ${formatTokenCount(estimatedTokens as number, totalFormat)} · 窗口未知 · ${statusShort}`;
  }

  return {
    placeholder: current == null,
    knownBudget,
    estimatedTokens,
    maxInputTokens,
    contextWindowTokens: current?.context_window_tokens ?? null,
    outputReserveTokens: current?.output_reserve_tokens ?? null,
    pressureThresholdTokens: current?.pressure_threshold_tokens ?? null,
    percent,
    barPercent: percent == null ? null : clampPercent(percent),
    thresholdPercent: threshold,
    breakdown: current?.breakdown ?? null,
    measurementKind: current?.measurement_kind ?? null,
    measurementProvenance: provenance,
    measurementExact,
    measurementLabel: measurementLabel(
      current?.measurement_kind ?? null,
      measurementExact
    ),
    lastActualTokens,
    actualIsCurrent: matchesCurrentActual,
    actualLabel,
    totalFormat,
    revision: current?.request_revision ?? null,
    activeStatus: active.status,
    activeStatusLabel: active.label,
    compactText,
    parity,
  };
}

export const BREAKDOWN_LABELS: Array<{ key: keyof ContextUsageBreakdown; label: string }> = [
  { key: "system_tokens", label: "系统与规则" },
  { key: "conversation_tokens", label: "对话消息" },
  { key: "tool_schema_tokens", label: "工具定义" },
  { key: "tool_result_tokens", label: "工具结果" },
  { key: "request_overhead_tokens", label: "请求结构开销" },
];
