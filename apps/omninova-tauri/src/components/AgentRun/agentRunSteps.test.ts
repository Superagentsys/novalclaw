import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { aggregateSteps } from "./agentRunSteps.ts";
import { getEventStatusLabel, toolCompletedSucceeded } from "./types.ts";

describe("tool_completed status vs success", () => {
  it("treats Core status=success as a completed tool", () => {
    assert.equal(toolCompletedSucceeded({ status: "success" }), true);
    assert.equal(toolCompletedSucceeded({ status: "error" }), false);
    assert.equal(toolCompletedSucceeded({ success: true }), true);
    assert.equal(toolCompletedSucceeded({ success: false, status: "success" }), false);
  });

  it("does not mark a successful computer_use screenshot as failed", () => {
    const steps = aggregateSteps([
      {
        type: "tool_started",
        run_id: "run-excel",
        step_id: "s1",
        tool_call_id: "c1",
        tool_name: "computer_use",
        title: "开始执行工具：computer_use",
      },
      {
        type: "tool_completed",
        run_id: "run-excel",
        step_id: "s1",
        tool_call_id: "c1",
        tool_name: "computer_use",
        status: "success",
        duration_ms: 900,
        result_summary:
          '{"ok":true,"action":"screenshot","foreground_app":{"name":"工作簿1 - Excel"}}',
      },
    ]);
    assert.equal(steps[0]?.status, "success");
    assert.equal(steps[0]?.title, "桌面操作完成");
    assert.equal(
      getEventStatusLabel({ type: "tool_completed", status: "success" }),
      "success"
    );
  });

  it("does not turn browser takeover notifications into timeline steps", () => {
    const steps = aggregateSteps([
      {
        type: "browser_takeover_state_changed",
        run_id: "run-a",
        session_id: "session-a",
        phase: "human_controlled",
        generation: 1,
      },
      {
        type: "tool_started",
        run_id: "run-a",
        step_id: "s1",
        tool_call_id: "c1",
        tool_name: "browser",
        title: "开始执行工具：browser",
      },
    ]);
    assert.equal(steps.length, 1);
    assert.equal(steps[0]?.tool_name, "browser");
  });
});
