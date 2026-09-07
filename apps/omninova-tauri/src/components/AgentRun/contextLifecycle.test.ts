import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  applyLifecycleToActivity,
  reduceContextLifecycleEvents,
} from "./contextLifecycle.ts";
import {
  CONTEXT_COMPACTION_COMPLETED_LABEL,
  CONTEXT_COMPACTION_FAILED_LABEL,
  CONTEXT_COMPACTION_STARTED_LABEL,
  CONTEXT_MAINTENANCE_CONDITION_LABEL,
  CONTEXT_PRESSURE_LABEL,
  CONTEXT_PRUNING_COMPLETED_LABEL,
  CONTEXT_PRUNING_STARTED_LABEL,
  CONTEXT_RECOVERY_COMPLETED_LABEL,
  CONTEXT_RECOVERY_FAILED_LABEL,
  CONTEXT_RECOVERY_STARTED_LABEL,
  CONTEXT_SECOND_OVERFLOW_DETAIL,
} from "./executionPresentation.ts";
import { aggregateSteps, processStepTitles } from "./agentRunSteps.ts";
import type {
  AgentRunEventContextLifecycle,
  AgentRunEventContextUsage,
  ContextLifecycleEvent,
  ContextLifecycleEventKind,
  ContextTelemetryMode,
} from "./types.ts";

function lifecycle(
  runId: string,
  operationId: string,
  kind: ContextLifecycleEventKind,
  options: {
    sessionId?: string;
    timestamp?: number;
    mode?: ContextTelemetryMode;
  } = {}
): AgentRunEventContextLifecycle {
  const event: ContextLifecycleEvent = {
    operation_id: operationId,
    run_id: runId,
    session_id: options.sessionId ?? "session-a",
    mode: options.mode ?? ("proactive" as ContextTelemetryMode),
    kind,
    timestamp: options.timestamp ?? 1_000,
  };
  return { type: "context_lifecycle", run_id: runId, event };
}

function usage(runId: string): AgentRunEventContextUsage {
  return {
    type: "context_usage",
    run_id: runId,
    snapshot: {
      session_id: "session-a",
      run_id: runId,
      request_revision: 1,
      provider: "openai",
      model: "gpt-test",
      measurement_kind: "final_request_estimate",
      estimated_input_tokens: 900_000,
      usage_ratio: 0.9,
      breakdown: {
        system_tokens: 10,
        conversation_tokens: 20,
        tool_schema_tokens: 0,
        tool_result_tokens: 0,
        request_overhead_tokens: 8,
      },
      measured_at: 1,
    },
  };
}

describe("O2 context lifecycle presentation", () => {
  it("A. pruning only never invents compaction", () => {
    const events = [
      { type: "model_started", run_id: "run-a", step_id: "m1", title: "等待模型响应" },
      { type: "model_completed", run_id: "run-a", step_id: "m1", title: "模型响应完成" },
      usage("run-a"),
      lifecycle("run-a", "op-prune", {
        type: "context_pruning_started",
        mode: "proactive",
        estimated_before: 812_000,
      }),
      lifecycle("run-a", "op-prune", {
        type: "context_pruning_completed",
        mode: "proactive",
        estimated_before: 812_000,
        estimated_after: 691_000,
        pruned_tool_result_count: 3,
      }),
      { type: "model_started", run_id: "run-a", step_id: "m2", title: "等待模型响应" },
    ];
    const steps = aggregateSteps(events, { runId: "run-a", sessionId: "session-a" });
    const titles = processStepTitles(steps);
    assert.equal(steps.filter((step) => step.id === "context:op-prune").length, 1);
    assert.equal(steps.find((step) => step.id === "context:op-prune")?.title, CONTEXT_PRUNING_COMPLETED_LABEL);
    assert.equal(steps.find((step) => step.id === "context:op-prune")?.status, "success");
    assert.match(steps.find((step) => step.id === "context:op-prune")?.result_summary ?? "", /3 个结果/);
    assert.equal(titles.includes(CONTEXT_COMPACTION_STARTED_LABEL), false);
    assert.equal(titles.includes(CONTEXT_COMPACTION_COMPLETED_LABEL), false);
    assert.equal(titles.includes("正在压缩上下文"), false);
    assert.deepEqual(titles, ["模型响应完成", CONTEXT_PRUNING_COMPLETED_LABEL, "等待模型响应"]);
  });

  it("B. compaction started then completed updates the same row", () => {
    const started = aggregateSteps([
      lifecycle("run-a", "op-c", {
        type: "context_compaction_started",
        mode: "proactive",
        estimated_before: 691_000,
      }, { timestamp: 1_000 }),
    ]);
    assert.equal(started.length, 1);
    assert.equal(started[0].id, "context:op-c");
    assert.equal(started[0].title, CONTEXT_COMPACTION_STARTED_LABEL);
    assert.equal(started[0].status, "running");

    const completed = aggregateSteps([
      lifecycle("run-a", "op-c", {
        type: "context_compaction_started",
        mode: "proactive",
        estimated_before: 691_000,
      }, { timestamp: 1_000 }),
      lifecycle("run-a", "op-c", {
        type: "context_compaction_completed",
        mode: "proactive",
        estimated_before: 691_000,
        estimated_after: 423_000,
      }, { timestamp: 3_800 }),
    ]);
    assert.equal(completed.length, 1);
    assert.equal(completed[0].id, "context:op-c");
    assert.equal(completed[0].title, CONTEXT_COMPACTION_COMPLETED_LABEL);
    assert.equal(completed[0].status, "success");
    assert.match(completed[0].result_summary ?? "", /~691K → ~423K/);
    assert.match(completed[0].result_summary ?? "", /2\.8s/);
  });

  it("C. compaction failure never shows completed", () => {
    const steps = aggregateSteps([
      lifecycle("run-a", "op-c", {
        type: "context_compaction_started",
        mode: "proactive",
        estimated_before: 1000,
      }),
      lifecycle("run-a", "op-c", {
        type: "context_compaction_failed",
        mode: "proactive",
        estimated_before: 1000,
        reason: "non_shrinking",
      }),
    ]);
    assert.equal(steps.length, 1);
    assert.equal(steps[0].title, CONTEXT_COMPACTION_FAILED_LABEL);
    assert.equal(steps[0].status, "error");
    assert.equal(steps[0].result_summary, "压缩后上下文未能缩小");
    assert.equal(processStepTitles(steps).includes(CONTEXT_COMPACTION_COMPLETED_LABEL), false);
  });

  it("D. pressure only is informational and invents no pruning or compaction", () => {
    const steps = aggregateSteps([
      lifecycle("run-a", "op-p", {
        type: "context_pressure_detected",
        mode: "proactive",
        estimated_before: 812_000,
        context_window_tokens: 1_000_000,
      }),
    ]);
    assert.equal(steps.length, 1);
    assert.equal(steps[0].title, CONTEXT_PRESSURE_LABEL);
    assert.equal(steps[0].status, "success");
    assert.equal(steps[0].result_summary, "~812K / 1.0M");
    assert.equal(processStepTitles(steps).includes(CONTEXT_PRUNING_STARTED_LABEL), false);
    assert.equal(processStepTitles(steps).includes(CONTEXT_COMPACTION_STARTED_LABEL), false);

    const unknown = aggregateSteps([
      lifecycle("run-a", "op-u", {
        type: "context_pressure_detected",
        mode: "unknown_budget_oversize",
        estimated_before: 12_000,
        context_window_tokens: null,
      }, { mode: "unknown_budget_oversize" }),
    ]);
    assert.equal(unknown[0].title, CONTEXT_MAINTENANCE_CONDITION_LABEL);
    assert.equal(unknown[0].result_summary?.includes("/"), false);
  });

  it("E. recovery success keeps distinct operation ids and event order", () => {
    const steps = aggregateSteps([
      lifecycle("run-a", "op-r", {
        type: "context_overflow_recovery_started",
        mode: "forced_overflow_recovery",
        estimated_before: 900_000,
      }, { mode: "forced_overflow_recovery", timestamp: 1 }),
      lifecycle("run-a", "op-c", {
        type: "context_compaction_started",
        mode: "forced_overflow_recovery",
        estimated_before: 800_000,
      }, { mode: "forced_overflow_recovery", timestamp: 2 }),
      lifecycle("run-a", "op-c", {
        type: "context_compaction_completed",
        mode: "forced_overflow_recovery",
        estimated_before: 800_000,
        estimated_after: 400_000,
      }, { mode: "forced_overflow_recovery", timestamp: 3 }),
      lifecycle("run-a", "op-r", {
        type: "context_overflow_recovery_completed",
        mode: "forced_overflow_recovery",
        estimated_after: 400_000,
      }, { mode: "forced_overflow_recovery", timestamp: 4 }),
    ]);
    assert.deepEqual(
      steps.map((step) => [step.id, step.title, step.status]),
      [
        ["context:op-r", CONTEXT_RECOVERY_COMPLETED_LABEL, "success"],
        ["context:op-c", CONTEXT_COMPACTION_COMPLETED_LABEL, "success"],
      ]
    );
    assert.notEqual(steps[0].id, steps[1].id);
  });

  it("F. recovery failure is one failed row", () => {
    const steps = aggregateSteps([
      lifecycle("run-a", "op-r", {
        type: "context_overflow_recovery_started",
        mode: "forced_overflow_recovery",
        estimated_before: 900_000,
      }, { mode: "forced_overflow_recovery" }),
      lifecycle("run-a", "op-r", {
        type: "context_overflow_recovery_failed",
        mode: "forced_overflow_recovery",
        reason: "context_window_exceeded",
      }, { mode: "forced_overflow_recovery" }),
    ]);
    assert.equal(steps.length, 1);
    assert.equal(steps[0].title, CONTEXT_RECOVERY_FAILED_LABEL);
    assert.equal(steps[0].status, "error");
    assert.equal(steps[0].result_summary, CONTEXT_SECOND_OVERFLOW_DETAIL);
    assert.equal(processStepTitles(steps).includes(CONTEXT_RECOVERY_COMPLETED_LABEL), false);
  });

  it("G. duplicate started/completed stay one completed operation", () => {
    const steps = aggregateSteps([
      lifecycle("run-a", "op-x", { type: "context_compaction_started", mode: "proactive", estimated_before: 1 }),
      lifecycle("run-a", "op-x", { type: "context_compaction_started", mode: "proactive", estimated_before: 1 }),
      lifecycle("run-a", "op-x", {
        type: "context_compaction_completed",
        mode: "proactive",
        estimated_before: 1,
        estimated_after: 1,
      }),
      lifecycle("run-a", "op-x", {
        type: "context_compaction_completed",
        mode: "proactive",
        estimated_before: 1,
        estimated_after: 1,
      }),
    ]);
    assert.equal(steps.length, 1);
    assert.equal(steps[0].status, "success");
    assert.equal(steps[0].title, CONTEXT_COMPACTION_COMPLETED_LABEL);
  });

  it("H. late started cannot regress a completed operation", () => {
    const steps = aggregateSteps([
      lifecycle("run-a", "op-x", {
        type: "context_compaction_completed",
        mode: "proactive",
        estimated_before: 10,
        estimated_after: 5,
      }),
      lifecycle("run-a", "op-x", { type: "context_compaction_started", mode: "proactive", estimated_before: 10 }),
    ]);
    assert.equal(steps.length, 1);
    assert.equal(steps[0].status, "success");
    assert.equal(steps[0].title, CONTEXT_COMPACTION_COMPLETED_LABEL);
  });

  it("I. run isolation keeps each inspector on its own events", () => {
    const mixed = [
      lifecycle("run-a", "op-a", { type: "context_pruning_started", mode: "proactive", estimated_before: 1 }, { sessionId: "session-a" }),
      lifecycle("run-b", "op-b", {
        type: "context_compaction_started",
        mode: "proactive",
        estimated_before: 1,
      }, { sessionId: "session-b" }),
    ];
    const a = aggregateSteps(mixed, { runId: "run-a", sessionId: "session-a" });
    const b = aggregateSteps(mixed, { runId: "run-b", sessionId: "session-b" });
    assert.deepEqual(processStepTitles(a), [CONTEXT_PRUNING_STARTED_LABEL]);
    assert.deepEqual(processStepTitles(b), [CONTEXT_COMPACTION_STARTED_LABEL]);
    assert.equal(a[0].id, "context:op-a");
    assert.equal(b[0].id, "context:op-b");
  });

  it("J. stored events stay stable across a Process tab remount simulation", () => {
    const events = [
      lifecycle("run-a", "op-c", { type: "context_compaction_started", mode: "proactive", estimated_before: 2 }, { timestamp: 10 }),
      lifecycle("run-a", "op-done", {
        type: "context_pruning_completed",
        mode: "proactive",
        estimated_before: 3,
        estimated_after: 2,
        pruned_tool_result_count: 1,
      }, { timestamp: 11 }),
    ];
    const first = aggregateSteps(events, { runId: "run-a", sessionId: "session-a" });
    const afterTabSwitch = aggregateSteps(events, { runId: "run-a", sessionId: "session-a" });
    assert.deepEqual(afterTabSwitch, first);
    assert.equal(first.find((step) => step.id === "context:op-c")?.status, "running");
    assert.equal(first.find((step) => step.id === "context:op-done")?.status, "success");
  });

  it("does not show pruned count when the Core field is absent", () => {
    const steps = aggregateSteps([
      lifecycle("run-a", "op-prune", {
        type: "context_pruning_completed",
        mode: "proactive",
        estimated_before: 100,
        estimated_after: 80,
      }),
    ]);
    assert.equal(steps[0].result_summary?.includes("个结果"), false);
  });

  it("does not invent duration from the frontend clock", () => {
    const views = reduceContextLifecycleEvents([
      lifecycle("run-a", "op-c", { type: "context_compaction_started", mode: "proactive", estimated_before: 1 }, { timestamp: 5_000 }),
      lifecycle("run-a", "op-c", {
        type: "context_compaction_completed",
        mode: "proactive",
        estimated_before: 1,
        estimated_after: 1,
      }, { timestamp: 5_000 }),
    ]);
    assert.equal(views[0].durationMs, 0);
  });

  it("does not display raw provider failure bodies", () => {
    const steps = aggregateSteps([
      lifecycle("run-a", "op-c", { type: "context_compaction_started", mode: "proactive", estimated_before: 1 }),
      lifecycle("run-a", "op-c", {
        type: "context_compaction_failed",
        mode: "proactive",
        estimated_before: 1,
        reason: '{"error":{"message":"sk-secret-body"}}',
      }),
    ]);
    assert.equal(steps[0].title, CONTEXT_COMPACTION_FAILED_LABEL);
    assert.equal(steps[0].result_summary, undefined);
  });

  it("marks a started operation incomplete after the agent run terminates", () => {
    const steps = aggregateSteps([
      lifecycle("run-a", "op-c", { type: "context_compaction_started", mode: "proactive", estimated_before: 1 }),
      { type: "run_cancelled", run_id: "run-a", reason: "user" },
    ]);
    assert.equal(steps[0].status, "warning");
    assert.equal(steps[0].title, "上下文压缩未完成");
  });

  it("upserts task activity in place by operation id", () => {
    const started = applyLifecycleToActivity(
      [],
      lifecycle("run-a", "op-c", { type: "context_compaction_started", mode: "proactive", estimated_before: 1 }),
      { runId: "run-a", sessionId: "session-a" }
    );
    const completed = applyLifecycleToActivity(
      started,
      lifecycle("run-a", "op-c", {
        type: "context_compaction_completed",
        mode: "proactive",
        estimated_before: 1,
        estimated_after: 1,
      }),
      { runId: "run-a", sessionId: "session-a" }
    );
    assert.equal(started.length, 1);
    assert.equal(completed.length, 1);
    assert.equal(completed[0].operationId, "context:op-c");
    assert.equal(completed[0].label, CONTEXT_COMPACTION_COMPLETED_LABEL);
    assert.equal(completed[0].status, "completed");
  });
});
