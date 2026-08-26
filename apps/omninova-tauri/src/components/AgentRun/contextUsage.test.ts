import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  applyContextUsageLifecycle,
  applyContextUsageSnapshot,
  emptyContextUsageState,
  finishContextUsageRun,
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
    assert.equal(view.measurementLabel, "发送前估算");
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
    assert.equal(view.measurementLabel, "当前估算");
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
    assert.equal(view.compactText, "上下文 —");
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
});
