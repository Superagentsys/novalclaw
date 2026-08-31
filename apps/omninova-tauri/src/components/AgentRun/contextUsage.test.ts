import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  applyContextUsageLifecycle,
  applyContextUsageSnapshot,
  applySessionOpenCandidate,
  beginContextUsageRefresh,
  emptyContextUsageState,
  failContextUsageProjection,
  finishContextUsageRun,
  restorePersistedContextProjection,
  selectContextUsageView,
  switchContextUsageIdentity,
  type ContextUsageIdentity,
} from "./contextUsageState.ts";
import { primaryUsagePercent } from "./contextTokens.ts";
import type {
  AgentRunEventContextLifecycle,
  ContextLifecycleEventKind,
  ContextMeasurementKind,
  ContextUsageBreakdown,
  ContextUsageSnapshot,
} from "./types.ts";

const identity: ContextUsageIdentity = {
  sessionId: "session-a",
  provider: "openai",
  model: "gpt-test",
  liveRunId: "run-a",
};

const breakdown = (total: number): ContextUsageBreakdown => ({
  system_tokens: 18_000,
  conversation_tokens: Math.max(0, total - 18_000 - 6_000 - 4_000),
  tool_schema_tokens: 6_000,
  tool_result_tokens: 0,
  request_overhead_tokens: 4_000,
});

function snapshot(partial: Partial<ContextUsageSnapshot> & Pick<ContextUsageSnapshot, "measurement_kind" | "estimated_input_tokens" | "request_revision">): ContextUsageSnapshot {
  return {
    session_id: "session-a",
    run_id: "run-a",
    provider: "openai",
    model: "gpt-test",
    context_window_tokens: 1_000_000,
    max_input_tokens: 664_000,
    output_reserve_tokens: 384_000,
    pressure_threshold_tokens: 531_000,
    breakdown: breakdown(partial.estimated_input_tokens),
    measured_at: partial.request_revision,
    ...partial,
  };
}

function lifecycle(
  operationId: string,
  kind: ContextLifecycleEventKind
): AgentRunEventContextLifecycle {
  return {
    type: "context_lifecycle",
    run_id: "run-a",
    event: {
      operation_id: operationId,
      run_id: "run-a",
      session_id: "session-a",
      mode: "proactive",
      kind,
      timestamp: 1,
    },
  };
}

describe("O3 context usage visualization", () => {
  it("A. known budget percentage uses max_input not context_window", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 421_000,
      request_revision: 1,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.percent, primaryUsagePercent(421_000, 664_000));
    assert.equal(view.percent, 63);
    assert.notEqual(view.percent, primaryUsagePercent(421_000, 1_000_000));
    assert.equal(view.maxInputTokens, 664_000);
    assert.equal(view.contextWindowTokens, 1_000_000);
  });

  it("R2. frontend displays backend budget fields and does not derive max_input", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "candidate_estimate",
      estimated_input_tokens: 18_000,
      request_revision: 1,
      context_window_tokens: 1_000_000,
      model_max_output_tokens: 384_000,
      request_output_reserve_tokens: 32_000,
      output_reserve_tokens: 32_000,
      safety_reserve_tokens: 32_768,
      max_input_tokens: 935_232,
      pressure_threshold_tokens: 748_185,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.contextWindowTokens, 1_000_000);
    assert.equal(view.modelMaxOutputTokens, 384_000);
    assert.equal(view.requestOutputReserveTokens, 32_000);
    assert.equal(view.requestReserveIsConservativeFallback, false);
    assert.equal(view.safetyReserveTokens, 32_768);
    assert.equal(view.maxInputTokens, 935_232);
    assert.equal(view.pressureThresholdTokens, 748_185);
  });

  it("R2.1 conservative fallback is labeled when reserve equals model max", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "candidate_estimate",
      estimated_input_tokens: 18_000,
      request_revision: 1,
      context_window_tokens: 1_000_000,
      model_max_output_tokens: 384_000,
      request_output_reserve_tokens: 384_000,
      output_reserve_tokens: 384_000,
      safety_reserve_tokens: 32_768,
      max_input_tokens: 583_232,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.requestOutputReserveTokens, 384_000);
    assert.equal(view.requestReserveIsConservativeFallback, true);
    assert.equal(view.maxInputTokens, 583_232);
  });

  it("B. 600K of 664K is about 90 percent not 60", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 600_000,
      request_revision: 1,
      usage_ratio: 0.6,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.percent, 90);
    assert.notEqual(view.percent, 60);
  });

  it("C. unknown budget has no percent, bar, or denominator", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "candidate_estimate",
      estimated_input_tokens: 421_000,
      request_revision: 1,
      max_input_tokens: null,
      context_window_tokens: null,
      pressure_threshold_tokens: null,
      output_reserve_tokens: null,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.knownBudget, false);
    assert.equal(view.percent, null);
    assert.equal(view.barPercent, null);
    assert.equal(view.maxInputTokens, null);
    assert.match(view.compactText, /窗口未知/);
    assert.match(view.compactText, /上下文估算/);
    assert.equal(view.compactText.includes("%"), false);
    assert.equal(view.compactText.includes("/"), false);
  });

  it("D. Final supersedes Candidate on the same revision", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "candidate_estimate",
      estimated_input_tokens: 400_000,
      request_revision: 7,
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 421_000,
      request_revision: 7,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.measurementKind, "final_request_estimate");
    assert.equal(view.estimatedTokens, 421_000);
    assert.equal(view.measurementLabel, "发送前保守估算");
  });

  it("E. actual stays separate from the estimate", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 421_000,
      request_revision: 42,
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "provider_actual",
      estimated_input_tokens: 421_000,
      provider_actual_input_tokens: 417_000,
      request_revision: 42,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.estimatedTokens, 421_000);
    assert.equal(view.measurementKind, "final_request_estimate");
    assert.equal(view.lastActualTokens, 417_000);
  });

  it("F. new candidate stays current and old actual is previous", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 421_000,
      request_revision: 42,
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "provider_actual",
      estimated_input_tokens: 421_000,
      provider_actual_input_tokens: 417_000,
      request_revision: 42,
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "candidate_estimate",
      estimated_input_tokens: 430_000,
      request_revision: 43,
      breakdown: breakdown(430_000),
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.estimatedTokens, 430_000);
    assert.equal(view.measurementKind, "candidate_estimate");
    assert.equal(view.measurementLabel, "当前保守估算");
    assert.equal(view.lastActualTokens, 417_000);
    assert.equal(view.revision, 43);
  });

  it("G. out-of-order actuals do not regress", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 500_000,
      request_revision: 2,
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "provider_actual",
      estimated_input_tokens: 500_000,
      provider_actual_input_tokens: 498_000,
      request_revision: 2,
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "provider_actual",
      estimated_input_tokens: 400_000,
      provider_actual_input_tokens: 399_000,
      request_revision: 1,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.estimatedTokens, 500_000);
    assert.equal(view.lastActualTokens, 498_000);
  });

  it("H. model switch invalidates denominator and percentage immediately", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 421_000,
      request_revision: 1,
    }));
    state = switchContextUsageIdentity(state, {
      ...identity,
      model: "other-model",
    });
    const view = selectContextUsageView(state);
    assert.equal(view.placeholder, true);
    assert.equal(view.percent, null);
    assert.equal(view.maxInputTokens, null);
    assert.equal(view.estimatedTokens, null);
    assert.equal(view.compactText, "上下文输入 —");
  });

  it("I. breakdown belongs to the same snapshot as the displayed total", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "candidate_estimate",
      estimated_input_tokens: 100_000,
      request_revision: 1,
      breakdown: { ...breakdown(100_000), conversation_tokens: 1 },
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 200_000,
      request_revision: 2,
      breakdown: { ...breakdown(200_000), conversation_tokens: 99_000, tool_result_tokens: 73_000 },
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.estimatedTokens, 200_000);
    assert.equal(view.revision, 2);
    assert.equal(view.breakdown?.conversation_tokens, 99_000);
    assert.equal(view.breakdown?.tool_result_tokens, 73_000);
  });

  it("J. threshold marker uses telemetry, not a hardcoded 80 percent", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 200_000,
      request_revision: 1,
      max_input_tokens: 1_000_000,
      pressure_threshold_tokens: 400_000,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.thresholdPercent, 40);
    assert.notEqual(view.thresholdPercent, 80);
  });

  it("K. compaction started shows status and keeps the last snapshot number", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 545_000,
      request_revision: 8,
    }));
    state = applyContextUsageLifecycle(state, lifecycle("op-c", {
      type: "context_compaction_started",
      mode: "proactive",
      estimated_before: 545_000,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.activeStatus, "compaction");
    assert.equal(view.activeStatusLabel, "正在压缩上下文");
    assert.equal(view.estimatedTokens, 545_000);
  });

  it("L. pressure only must not show compaction status", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "candidate_estimate",
      estimated_input_tokens: 500_000,
      request_revision: 1,
    }));
    state = applyContextUsageLifecycle(state, lifecycle("op-p", {
      type: "context_pressure_detected",
      mode: "proactive",
      estimated_before: 500_000,
      context_window_tokens: 1_000_000,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.activeStatus, null);
    assert.notEqual(view.activeStatusLabel, "正在压缩上下文");
  });

  it("M. pruning only must not show compaction status", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "candidate_estimate",
      estimated_input_tokens: 500_000,
      request_revision: 1,
    }));
    state = applyContextUsageLifecycle(state, lifecycle("op-prune", {
      type: "context_pruning_started",
      mode: "proactive",
      estimated_before: 500_000,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.activeStatus, "pruning");
    assert.notEqual(view.activeStatusLabel, "正在压缩上下文");
    assert.equal(view.compactText.includes("正在压缩"), false);
  });

  it("N. compaction completed does not invent a lower token count", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 545_000,
      request_revision: 8,
    }));
    state = applyContextUsageLifecycle(state, lifecycle("op-c", {
      type: "context_compaction_started",
      mode: "proactive",
      estimated_before: 545_000,
    }));
    state = applyContextUsageLifecycle(state, lifecycle("op-c", {
      type: "context_compaction_completed",
      mode: "proactive",
      estimated_before: 545_000,
      estimated_after: 200_000,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.activeStatus, null);
    assert.equal(view.estimatedTokens, 545_000);
  });

  it("O. stale session/run/model events are ignored", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 421_000,
      request_revision: 1,
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 10_000,
      request_revision: 9,
      session_id: "session-b",
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 11_000,
      request_revision: 9,
      model: "other-model",
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 12_000,
      request_revision: 9,
      run_id: "run-other",
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.estimatedTokens, 421_000);
    assert.equal(view.revision, 1);
  });

  it("P. a late actual from an older run cannot become the new run actual", () => {
    let state = emptyContextUsageState({ ...identity, liveRunId: "run-b" });
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "provider_actual",
      estimated_input_tokens: 400_000,
      provider_actual_input_tokens: 399_000,
      request_revision: 9,
      run_id: "run-a",
    }));
    assert.equal(selectContextUsageView(state).lastActualTokens, null);
  });

  it("Q. a run terminal clears real active status without inventing new usage", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 545_000,
      request_revision: 8,
    }));
    state = applyContextUsageLifecycle(state, lifecycle("op-c", {
      type: "context_compaction_started",
      mode: "proactive",
      estimated_before: 545_000,
    }));
    state = finishContextUsageRun(state, "run-a");
    const view = selectContextUsageView(state);
    assert.equal(view.activeStatus, null);
    assert.equal(view.estimatedTokens, 545_000);
  });

  it("F2 F. actual total and estimated breakdown keep different provenance", () => {
    let state = emptyContextUsageState(identity);
    const estimatedBreakdown = breakdown(421_000);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 421_000,
      request_revision: 12,
      measurement_provenance: "safety_estimate",
      breakdown: estimatedBreakdown,
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "provider_actual",
      estimated_input_tokens: 421_000,
      provider_actual_input_tokens: 4_777,
      request_revision: 12,
      measurement_provenance: "provider_actual",
      breakdown: estimatedBreakdown,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.estimatedTokens, 421_000);
    assert.equal(view.measurementProvenance, "safety_estimate");
    assert.equal(view.measurementLabel, "发送前保守估算");
    assert.equal(view.lastActualTokens, 4_777);
    assert.equal(view.actualIsCurrent, true);
    assert.equal(view.actualLabel, "当前请求实际");
    assert.deepEqual(view.breakdown, estimatedBreakdown);
  });

  it("F2 G. frontend never rescales estimated breakdown to ProviderActual", () => {
    let state = emptyContextUsageState(identity);
    const estimatedBreakdown = breakdown(421_000);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 421_000,
      request_revision: 3,
      breakdown: estimatedBreakdown,
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "provider_actual",
      estimated_input_tokens: 421_000,
      provider_actual_input_tokens: 4_777,
      request_revision: 3,
      breakdown: estimatedBreakdown,
    }));
    const view = selectContextUsageView(state);
    const breakdownTotal =
      (view.breakdown?.system_tokens ?? 0) +
      (view.breakdown?.conversation_tokens ?? 0) +
      (view.breakdown?.tool_schema_tokens ?? 0) +
      (view.breakdown?.tool_result_tokens ?? 0) +
      (view.breakdown?.request_overhead_tokens ?? 0);
    assert.equal(view.lastActualTokens, 4_777);
    assert.equal(view.estimatedTokens, 421_000);
    assert.equal(breakdownTotal, 421_000);
    assert.notEqual(breakdownTotal, view.lastActualTokens);
    assert.deepEqual(view.breakdown, estimatedBreakdown);
  });

  it("F2 previous actual is labelled after a newer candidate", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 19_696,
      request_revision: 3,
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "provider_actual",
      estimated_input_tokens: 19_696,
      provider_actual_input_tokens: 4_777,
      request_revision: 3,
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "candidate_estimate",
      estimated_input_tokens: 21_000,
      request_revision: 4,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.actualIsCurrent, false);
    assert.equal(view.actualLabel, "上次请求实际");
    assert.equal(view.lastActualTokens, 4_777);
    assert.equal(view.estimatedTokens, 21_000);
  });

  it("F2 ProviderCountApi renders exact without tilde", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 4_812,
      request_revision: 3,
      measurement_provenance: "provider_count_api",
      measurement_exact: true,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.measurementProvenance, "provider_count_api");
    assert.equal(view.totalFormat, "exact");
    assert.equal(view.measurementLabel, "发送前精确计数");
    assert.equal(view.estimatedTokens, 4_812);
  });

  it("ProviderCountApi source does not imply exact precision", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 4_812,
      request_revision: 3,
      measurement_provenance: "provider_count_api",
      measurement_exact: false,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.measurementProvenance, "provider_count_api");
    assert.equal(view.measurementExact, false);
    assert.equal(view.totalFormat, "estimate");
    assert.equal(view.measurementLabel, "发送前保守估算");
  });

  it("V1.2C A. same revision exact tokenizer + actual creates parity", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 4_545,
      request_revision: 7,
      measurement_provenance: "exact_tokenizer",
      measurement_exact: false,
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "provider_actual",
      estimated_input_tokens: 4_545,
      provider_actual_input_tokens: 4_561,
      request_revision: 7,
      measurement_provenance: "provider_actual",
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.measurementProvenance, "exact_tokenizer");
    assert.equal(view.measurementExact, false);
    assert.ok(view.parity);
    assert.equal(view.parity?.localTokens, 4_545);
    assert.equal(view.parity?.actualTokens, 4_561);
    assert.equal(view.parity?.delta, -16);
    assert.equal(view.parity?.absError, 16);
  });

  it("V1.2C B. different revisions are never compared", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 100,
      request_revision: 8,
      measurement_provenance: "exact_tokenizer",
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "provider_actual",
      estimated_input_tokens: 100,
      provider_actual_input_tokens: 110,
      request_revision: 7,
      measurement_provenance: "provider_actual",
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.parity, null);
    assert.equal(view.actualIsCurrent, false);
  });

  it("V1.2C C. different provider/model are never compared", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 100,
      request_revision: 3,
      measurement_provenance: "exact_tokenizer",
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "provider_actual",
      estimated_input_tokens: 100,
      provider_actual_input_tokens: 110,
      request_revision: 3,
      provider: "other",
      model: "other-model",
      measurement_provenance: "provider_actual",
    }));
    assert.equal(selectContextUsageView(state).parity, null);
    assert.equal(selectContextUsageView(state).lastActualTokens, null);
  });

  it("V1.2C D. actual arriving before local remains safe then pairs", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "provider_actual",
      estimated_input_tokens: 4_561,
      provider_actual_input_tokens: 4_561,
      request_revision: 9,
      measurement_provenance: "provider_actual",
    }));
    assert.equal(selectContextUsageView(state).parity, null);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 4_545,
      request_revision: 9,
      measurement_provenance: "exact_tokenizer",
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.parity?.localTokens, 4_545);
    assert.equal(view.parity?.actualTokens, 4_561);
  });

  it("V1.2C E. local arriving before actual works", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 200,
      request_revision: 4,
      measurement_provenance: "exact_tokenizer",
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "provider_actual",
      estimated_input_tokens: 200,
      provider_actual_input_tokens: 210,
      request_revision: 4,
      measurement_provenance: "provider_actual",
    }));
    assert.equal(selectContextUsageView(state).parity?.delta, -10);
  });

  it("V1.2C F. stale revision does not replace current UI state", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 4_545,
      request_revision: 5,
      measurement_provenance: "exact_tokenizer",
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "provider_actual",
      estimated_input_tokens: 4_545,
      provider_actual_input_tokens: 4_561,
      request_revision: 5,
      measurement_provenance: "provider_actual",
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 99,
      request_revision: 4,
      measurement_provenance: "exact_tokenizer",
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "provider_actual",
      estimated_input_tokens: 99,
      provider_actual_input_tokens: 88,
      request_revision: 4,
      measurement_provenance: "provider_actual",
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.estimatedTokens, 4_545);
    assert.equal(view.lastActualTokens, 4_561);
    assert.equal(view.revision, 5);
    assert.equal(view.parity?.localTokens, 4_545);
    assert.equal(view.parity?.actualTokens, 4_561);
  });

  it("V1.2C G. parity diagnostic does not alter context lifecycle", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 4_545,
      request_revision: 2,
      measurement_provenance: "exact_tokenizer",
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "provider_actual",
      estimated_input_tokens: 4_545,
      provider_actual_input_tokens: 4_561,
      request_revision: 2,
      measurement_provenance: "provider_actual",
    }));
    const before = selectContextUsageView(state).parity;
    state = applyContextUsageLifecycle(state, lifecycle("op-parity", {
      type: "context_compaction_started",
      mode: "proactive",
      estimated_before: 4_545,
    }));
    const view = selectContextUsageView(state);
    assert.deepEqual(view.parity, before);
    assert.equal(view.activeStatus, "compaction");
    assert.equal(view.estimatedTokens, 4_545);
    assert.equal(view.lastActualTokens, 4_561);
  });

  it("V1.2C H. SafetyEstimate is not compared as ExactTokenizer parity", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 4_545,
      request_revision: 6,
      measurement_provenance: "safety_estimate",
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "provider_actual",
      estimated_input_tokens: 4_545,
      provider_actual_input_tokens: 4_561,
      request_revision: 6,
      measurement_provenance: "provider_actual",
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.measurementProvenance, "safety_estimate");
    assert.equal(view.parity, null);
    assert.equal(view.lastActualTokens, 4_561);
  });

  it("V1.2C I. parity diagnostic contains only numbers and metadata", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 4_545,
      request_revision: 1,
      measurement_provenance: "exact_tokenizer",
    }));
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "provider_actual",
      estimated_input_tokens: 4_545,
      provider_actual_input_tokens: 4_561,
      request_revision: 1,
      measurement_provenance: "provider_actual",
    }));
    const json = JSON.stringify(selectContextUsageView(state).parity);
    assert.match(json, /"localTokens":4545/);
    assert.match(json, /"actualTokens":4561/);
    for (const forbidden of ["hello", "prompt", "messages", "tools", "content"]) {
      assert.equal(json.includes(forbidden), false, json);
    }
  });
});

describe("R1 session-driven context projection UI", () => {
  it("D. last-known snapshot remains visible while local refresh runs", () => {
    let state = emptyContextUsageState(identity);
    state = restorePersistedContextProjection(state, {
      snapshot: snapshot({
        measurement_kind: "candidate_estimate",
        estimated_input_tokens: 11_300,
        request_revision: 4,
      }),
    });
    const refreshing = selectContextUsageView(state);
    assert.equal(refreshing.refreshing, true);
    assert.equal(refreshing.placeholder, false);
    assert.equal(refreshing.estimatedTokens, 11_300);
    assert.match(refreshing.compactText, /正在刷新/);
    state = applySessionOpenCandidate(state, snapshot({
      measurement_kind: "candidate_estimate",
      estimated_input_tokens: 12_000,
      request_revision: 1,
      run_id: null,
    }));
    const next = selectContextUsageView(state);
    assert.equal(next.refreshing, false);
    assert.equal(next.estimatedTokens, 12_000);
  });

  it("O. reconstruction failure exits loading to unavailable", () => {
    let state = emptyContextUsageState(identity);
    state = beginContextUsageRefresh(state);
    assert.equal(selectContextUsageView(state).placeholder, false);
    assert.match(selectContextUsageView(state).compactText, /正在刷新/);
    state = failContextUsageProjection(state);
    const view = selectContextUsageView(state);
    assert.equal(view.unavailable, true);
    assert.equal(view.refreshing, false);
    assert.equal(view.placeholder, false);
    assert.equal(view.compactText, "上下文暂不可用");
  });

  it("I. ProviderActual stays previous actual after session-open candidate", () => {
    let state = emptyContextUsageState(identity);
    state = restorePersistedContextProjection(state, {
      snapshot: snapshot({
        measurement_kind: "candidate_estimate",
        estimated_input_tokens: 11_300,
        request_revision: 4,
      }),
      last_actual: {
        input_tokens: 4_777,
        request_revision: 4,
        run_id: "run-a",
        provider: "openai",
        model: "gpt-test",
      },
    });
    state = applySessionOpenCandidate(state, snapshot({
      measurement_kind: "candidate_estimate",
      estimated_input_tokens: 12_100,
      request_revision: 1,
      run_id: null,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.estimatedTokens, 12_100);
    assert.equal(view.lastActualTokens, 4_777);
    assert.equal(view.actualLabel, "上次请求实际");
    assert.equal(view.actualIsCurrent, false);
  });

  it("F. provider/model mismatch does not restore old snapshot as current", () => {
    let state = emptyContextUsageState({
      ...identity,
      provider: "anthropic",
      model: "claude-sonnet",
    });
    state = restorePersistedContextProjection(state, {
      snapshot: snapshot({
        measurement_kind: "candidate_estimate",
        estimated_input_tokens: 99,
        request_revision: 1,
        provider: "deepseek",
        model: "deepseek-chat",
      }),
    });
    const view = selectContextUsageView(state);
    assert.equal(view.estimatedTokens, null);
    assert.equal(view.refreshing, true);
  });

  it("breakdown for exact tokenizer is labeled as independent estimates", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      measurement_kind: "candidate_estimate",
      estimated_input_tokens: 11_300,
      request_revision: 1,
      measurement_provenance: "exact_tokenizer",
      measurement_exact: false,
    }));
    const view = selectContextUsageView(state);
    assert.equal(view.breakdownIndependent, true);
    assert.equal(view.breakdownCaption, "以下项目为独立估算");
    assert.equal(view.measurementLabel, "本地 Tokenizer · 待 Provider 对账");
  });
});
