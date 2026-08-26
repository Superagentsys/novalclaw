import assert from "node:assert/strict";
import { describe, it } from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import {
  applyContextUsageLifecycle,
  applyContextUsageSnapshot,
  emptyContextUsageState,
  selectContextUsageView,
  type ContextUsageIdentity,
} from "../AgentRun/contextUsageState.ts";
import type {
  AgentRunEventContextLifecycle,
  ContextUsageSnapshot,
} from "../AgentRun/types.ts";
import { ContextUsageBadgeContent } from "./ContextUsageBadge.tsx";

const identity: ContextUsageIdentity = {
  sessionId: "session-ui",
  provider: "openai",
  model: "gpt-test",
  liveRunId: "run-ui",
};

function snapshot(partial: Partial<ContextUsageSnapshot> = {}): ContextUsageSnapshot {
  return {
    session_id: "session-ui",
    run_id: "run-ui",
    provider: "openai",
    model: "gpt-test",
    context_window_tokens: 1_000_000,
    max_input_tokens: 664_000,
    output_reserve_tokens: 384_000,
    pressure_threshold_tokens: 531_000,
    estimated_input_tokens: 421_000,
    provider_actual_input_tokens: null,
    usage_ratio: 421_000 / 664_000,
    measurement_kind: "final_request_estimate",
    request_revision: 12,
    breakdown: {
      system_tokens: 18_000,
      conversation_tokens: 214_000,
      tool_schema_tokens: 6_000,
      tool_result_tokens: 179_000,
      request_overhead_tokens: 4_000,
    },
    measured_at: 1,
    ...partial,
  };
}

function renderSnapshot(partial: Partial<ContextUsageSnapshot>, open: boolean): string {
  let state = emptyContextUsageState(identity);
  state = applyContextUsageSnapshot(state, snapshot(partial));
  return renderToStaticMarkup(
    <ContextUsageBadgeContent
      view={selectContextUsageView(state)}
      open={open}
      panelId="context-usage-test"
      onToggle={() => undefined}
    />
  );
}

describe("ContextUsageBadge compact presentation", () => {
  it("A. known budget renders compact badge and authoritative popover", () => {
    const html = renderSnapshot({}, true);
    assert.match(html, /上下文/);
    assert.match(html, /63% · ~421K \/ 664K/);
    assert.match(html, /上下文已用 63%/);
    assert.match(html, /role="progressbar"/);
    assert.match(html, /系统与规则/);
    assert.match(html, /~18K/);
    assert.match(html, /对话消息/);
    assert.match(html, /~214K/);
    assert.match(html, /Revision 12/);
  });

  it("B. unknown budget has no percentage, progressbar, or fake denominator", () => {
    const html = renderSnapshot({
      context_window_tokens: null,
      max_input_tokens: null,
      output_reserve_tokens: null,
      pressure_threshold_tokens: null,
    }, true);
    assert.match(html, /~421K · 窗口未知/);
    assert.match(html, /上下文估算/);
    assert.doesNotMatch(html, /role="progressbar"/);
    assert.doesNotMatch(html, /63%/);
    assert.doesNotMatch(html, /664K/);
  });

  it("C. closed and open output follow the popover controller state", () => {
    const closed = renderSnapshot({}, false);
    const open = renderSnapshot({}, true);
    assert.match(closed, /aria-expanded="false"/);
    assert.doesNotMatch(closed, /role="dialog"/);
    assert.match(open, /aria-expanded="true"/);
    assert.match(open, /role="dialog"/);
  });

  it("D. compacting appears only after a typed compaction-started event", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({ estimated_input_tokens: 545_000 }));
    const before = renderToStaticMarkup(
      <ContextUsageBadgeContent
        view={selectContextUsageView(state)}
        open={true}
        panelId="before-compaction"
        onToggle={() => undefined}
      />
    );
    assert.doesNotMatch(before, /压缩中/);

    const lifecycle: AgentRunEventContextLifecycle = {
      type: "context_lifecycle",
      run_id: "run-ui",
      event: {
        operation_id: "compact-ui",
        run_id: "run-ui",
        session_id: "session-ui",
        mode: "proactive",
        kind: {
          type: "context_compaction_started",
          mode: "proactive",
          estimated_before: 545_000,
        },
        timestamp: 2,
      },
    };
    state = applyContextUsageLifecycle(state, lifecycle);
    const after = renderToStaticMarkup(
      <ContextUsageBadgeContent
        view={selectContextUsageView(state)}
        open={true}
        panelId="after-compaction"
        onToggle={() => undefined}
      />
    );
    assert.match(after, /82% · ~545K \/ 664K/);
    assert.match(after, /压缩中/);
    assert.match(after, /正在压缩上下文/);
  });
});
