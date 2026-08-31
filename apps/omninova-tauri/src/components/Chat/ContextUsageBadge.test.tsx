import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import {
  applyContextUsageLifecycle,
  applyContextUsageSnapshot,
  beginContextUsageRefresh,
  emptyContextUsageState,
  failContextUsageProjection,
  selectContextUsageView,
  type ContextUsageIdentity,
} from "../AgentRun/contextUsageState.ts";
import type {
  AgentRunEventContextLifecycle,
  ContextUsageSnapshot,
} from "../AgentRun/types.ts";
import {
  CONTEXT_USAGE_VIEWPORT_MARGIN,
  computeContextUsagePopoverPosition,
  ContextUsageBadgeContent,
  isOutsideContextUsageClick,
  measureContextUsagePopoverWidth,
  resolveContextProgressRingState,
  resolveContextProgressRingTone,
} from "./ContextUsageBadge.tsx";
import {
  ContextProgressRing,
  normalizeContextProgressPercentage,
} from "./ContextProgressRing.tsx";

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
    assert.match(html, /上下文输入 63%/);
    assert.match(html, /data-context-ring-determinate="true"/);
    assert.match(html, /data-context-ring-progress="63"/);
    assert.match(html, /role="progressbar"/);
    assert.match(html, /系统与规则/);
    assert.match(html, /~18K/);
    assert.match(html, /对话消息/);
    assert.match(html, /~214K/);
    assert.match(html, /估算构成/);
    assert.match(html, /发送前保守估算/);
  });

  it("mini polish removes the visible leading label and uses the smaller ring", () => {
    const html = renderSnapshot({
      estimated_input_tokens: 27_000,
      max_input_tokens: 583_232,
    }, false);
    assert.doesNotMatch(html, /context-usage-label-word/);
    assert.match(html, /style="width:20px;height:20px"/);
    assert.match(html, />5%<\/span>/);
    assert.match(html, /~27K/);
  });

  it("R2. budget popover distinguishes window, model max, request reserve, and safety", () => {
    const html = renderSnapshot({
      context_window_tokens: 1_000_000,
      model_max_output_tokens: 384_000,
      request_output_reserve_tokens: 32_000,
      output_reserve_tokens: 32_000,
      safety_reserve_tokens: 32_768,
      max_input_tokens: 935_232,
      pressure_threshold_tokens: 748_185,
      estimated_input_tokens: 18_000,
    }, true);
    assert.match(html, /模型上下文窗口/);
    assert.match(html, /模型最大输出上限/);
    assert.match(html, /本次输出预留/);
    assert.match(html, /安全预留/);
    assert.match(html, /当前输入预算/);
    assert.match(html, /自动维护阈值/);
    assert.match(html, /1\.0M/);
    assert.match(html, /384K/);
    assert.match(html, /32K/);
    assert.match(html, /935K/);
    assert.doesNotMatch(html, /<dt>最大输入预算<\/dt>/);
    assert.doesNotMatch(html, /保守回退/);
  });

  it("R2.1. unknown request cap popover marks conservative model-max fallback", () => {
    const html = renderSnapshot({
      context_window_tokens: 1_000_000,
      model_max_output_tokens: 384_000,
      request_output_reserve_tokens: 384_000,
      output_reserve_tokens: 384_000,
      safety_reserve_tokens: 32_768,
      max_input_tokens: 583_232,
      estimated_input_tokens: 18_000,
    }, true);
    assert.match(html, /本次输出预留/);
    assert.match(html, /保守回退：模型最大输出上限/);
    assert.match(html, /384K/);
  });

  it("R2.2. configured 32K request reserve shows the authoritative 935K compact denominator", () => {
    const html = renderSnapshot({
      context_window_tokens: 1_000_000,
      model_max_output_tokens: 384_000,
      request_output_reserve_tokens: 32_000,
      output_reserve_tokens: 32_000,
      safety_reserve_tokens: 32_768,
      max_input_tokens: 935_232,
      pressure_threshold_tokens: 748_185,
      estimated_input_tokens: 27_000,
    }, false);
    assert.match(html, /3% · ~27K \/ 935K/);
    assert.match(html, /data-context-ring-progress="3"/);
  });

  it("R2.2. model-max fallback reserve shows the authoritative 583K compact denominator", () => {
    const html = renderSnapshot({
      context_window_tokens: 1_000_000,
      model_max_output_tokens: 384_000,
      request_output_reserve_tokens: 384_000,
      output_reserve_tokens: 384_000,
      safety_reserve_tokens: 32_768,
      max_input_tokens: 583_232,
      pressure_threshold_tokens: 466_585,
      estimated_input_tokens: 27_000,
    }, false);
    assert.match(html, /5% · ~27K \/ 583K/);
    assert.match(html, /data-context-ring-progress="5"/);
  });

  it("R2.2. estimated measurement preserves the approximation marker", () => {
    const html = renderSnapshot({
      estimated_input_tokens: 27_000,
      max_input_tokens: 583_232,
      measurement_exact: false,
    }, false);
    assert.match(html, /~27K/);
    assert.match(html, /当前约 27K Token/);
  });

  it("R2.2. exact measurement does not invent an approximation marker", () => {
    const html = renderSnapshot({
      estimated_input_tokens: 27_000,
      max_input_tokens: 583_232,
      measurement_exact: true,
      measurement_provenance: "exact_tokenizer",
    }, false);
    assert.match(html, /当前 27K Token/);
    assert.doesNotMatch(html, /~27K/);
    assert.doesNotMatch(html, /当前约 27K Token/);
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
    assert.match(html, /data-context-ring-state="unknown"/);
    assert.match(html, /data-context-ring-determinate="false"/);
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
    assert.match(after, /正在压缩/);
    assert.match(after, /正在压缩上下文/);
    assert.match(after, /data-context-ring-state="compaction"/);
  });

  it("R2.2. normal state does not claim a compaction lifecycle", () => {
    const html = renderSnapshot({ estimated_input_tokens: 27_000 }, false);
    assert.match(html, /data-context-ring-state="normal"/);
    assert.doesNotMatch(html, /data-context-ring-state="compaction"/);
    assert.doesNotMatch(html, /正在压缩/);
  });

  it("R2.2. pressure tone compares usage with the supplied threshold", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({
      estimated_input_tokens: 440_000,
      max_input_tokens: 664_000,
      pressure_threshold_tokens: 500_000,
    }));
    let view = selectContextUsageView(state);
    assert.equal(resolveContextProgressRingTone(view), "warning");

    state = applyContextUsageSnapshot(state, snapshot({
      estimated_input_tokens: 510_000,
      max_input_tokens: 664_000,
      pressure_threshold_tokens: 500_000,
      request_revision: 13,
      measured_at: 2,
    }));
    view = selectContextUsageView(state);
    assert.equal(resolveContextProgressRingTone(view), "critical");
  });

  it("R2.2. ring rendering clamps only presentation input", () => {
    assert.equal(normalizeContextProgressPercentage(-10), 0);
    assert.equal(normalizeContextProgressPercentage(120), 100);
    assert.equal(normalizeContextProgressPercentage(null), null);
    const html = renderToStaticMarkup(
      <ContextProgressRing percentage={5} state="normal" tone="normal" />
    );
    assert.match(html, /data-context-ring-progress="5"/);
    assert.match(html, /stroke-dasharray="5 95"/);
  });
});

const badgeSource = readFileSync(
  fileURLToPath(new URL("./ContextUsageBadge.tsx", import.meta.url)),
  "utf8"
);
const badgeCss = readFileSync(
  fileURLToPath(new URL("./ContextUsageBadge.css", import.meta.url)),
  "utf8"
);

class FakeNode {
  constructor(private readonly parent: FakeNode | null = null) {}
  contains(node: unknown): boolean {
    let current = node as FakeNode | null;
    while (current) {
      if (current === this) return true;
      current = current.parent;
    }
    return false;
  }
}

describe("ContextUsageBadge popover layout", () => {
  it("A. Portal popover renders under document.body", () => {
    assert.match(badgeSource, /createPortal/);
    assert.match(badgeSource, /createPortal\(panel, document\.body\)/);
    assert.match(badgeCss, /position:\s*fixed/);
  });

  it("B. Left edge clamps popover left to the viewport margin", () => {
    const result = computeContextUsagePopoverPosition({
      trigger: { top: 500, left: 4, right: 84, bottom: 528, width: 80, height: 28 },
      viewport: { width: 1280, height: 800 },
      popover: { width: 360, height: 240 },
    });
    assert.equal(result.left >= CONTEXT_USAGE_VIEWPORT_MARGIN, true);
    assert.equal(result.left, CONTEXT_USAGE_VIEWPORT_MARGIN);
  });

  it("C. Right edge keeps the popover inside the viewport", () => {
    const viewport = { width: 800, height: 800 };
    const popover = { width: 360, height: 240 };
    const result = computeContextUsagePopoverPosition({
      trigger: { top: 500, left: 740, right: 792, bottom: 528, width: 52, height: 28 },
      viewport,
      popover,
    });
    assert.equal(result.left >= CONTEXT_USAGE_VIEWPORT_MARGIN, true);
    assert.equal(result.left + result.width <= viewport.width - CONTEXT_USAGE_VIEWPORT_MARGIN, true);
  });

  it("D. Insufficient space above falls below the trigger", () => {
    const result = computeContextUsagePopoverPosition({
      trigger: { top: 16, left: 200, right: 280, bottom: 44, width: 80, height: 28 },
      viewport: { width: 1280, height: 800 },
      popover: { width: 360, height: 320 },
    });
    assert.equal(result.placement, "below");
    assert.equal(result.top >= 44, true);
  });

  it("E. Breakdown layout keeps labels and right-aligned values", () => {
    const html = renderSnapshot({}, true);
    assert.match(html, /context-usage-row-label/);
    assert.match(html, /<dt[^>]*>[\s\S]*系统与规则[\s\S]*<\/dt>[\s\S]*<dd[^>]*>~18K<\/dd>/);
    assert.match(html, /<dt[^>]*>[\s\S]*对话消息[\s\S]*<\/dt>[\s\S]*<dd[^>]*>~214K<\/dd>/);
    assert.match(badgeCss, /grid-template-columns:\s*minmax\(0,\s*1fr\)\s+auto/);
    assert.match(badgeCss, /\.context-usage-row dd[\s\S]*white-space:\s*nowrap/);
    assert.match(badgeCss, /\.context-usage-row-label[\s\S]*overflow-wrap:\s*anywhere/);
  });

  it("F. Click outside closes the popover", () => {
    const trigger = new FakeNode();
    const panel = new FakeNode();
    const outside = new FakeNode();
    assert.equal(isOutsideContextUsageClick(outside, [trigger, panel]), true);
  });

  it("G. Click inside does not close the popover", () => {
    const trigger = new FakeNode();
    const panel = new FakeNode();
    const insidePanel = new FakeNode(panel);
    const insideTrigger = new FakeNode(trigger);
    assert.equal(isOutsideContextUsageClick(insidePanel, [trigger, panel]), false);
    assert.equal(isOutsideContextUsageClick(insideTrigger, [trigger, panel]), false);
    assert.equal(isOutsideContextUsageClick(panel, [trigger, panel]), false);
  });

  it("H. Escape closes and restores trigger focus", () => {
    assert.match(badgeSource, /event\.key !== "Escape"/);
    assert.match(badgeSource, /triggerRef\.current\?\.focus\(\)/);
    assert.match(badgeSource, /isOutsideContextUsageClick\(event\.target, \[triggerRef\.current, panelRef\.current\]\)/);
  });

  it("R2.2. native button preserves click, Enter, and Space activation semantics", () => {
    const html = renderSnapshot({ estimated_input_tokens: 27_000 }, false);
    assert.match(html, /<button[^>]*type="button"/);
    assert.match(html, /aria-haspopup="dialog"/);
    assert.match(html, /aria-expanded="false"/);
    assert.doesNotMatch(badgeSource, /onKeyDown=.*preventDefault/);
  });

  it("R2.2. aria label describes current and maximum input without fake precision", () => {
    const html = renderSnapshot({
      estimated_input_tokens: 27_000,
      max_input_tokens: 583_232,
      measurement_exact: false,
    }, false);
    assert.match(html, /aria-label="上下文输入 5%，当前约 27K Token，最大输入预算 583K Token"/);
  });

  it("R2.2. responsive tiers retain the button trigger and avoid toolbar overflow", () => {
    assert.match(badgeCss, /@media \(max-width: 900px\)/);
    assert.match(badgeCss, /@media \(max-width: 620px\)/);
    assert.match(badgeCss, /@media \(max-width: 480px\)/);
    assert.match(badgeCss, /\.context-usage-summary[\s\S]*display:\s*none/);
    assert.match(badgeCss, /\.context-usage-compact[\s\S]*display:\s*none/);
  });

  it("R2.2. reduced motion disables continuous ring animation", () => {
    assert.match(badgeCss, /@media \(prefers-reduced-motion: reduce\)/);
    assert.match(badgeCss, /\.context-progress-ring__motion[\s\S]*animation:\s*none/);
    assert.match(badgeCss, /\.context-progress-ring__activity[\s\S]*animation:\s*none/);
  });

  it("I. Unknown budget still has no percentage, progress bar, or fake denominator", () => {
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

  it("narrow viewport width stays inside the 24px gutter", () => {
    assert.equal(measureContextUsagePopoverWidth(300), 276);
    assert.equal(measureContextUsagePopoverWidth(1280), 360);
  });

  it("V1.2C parity rows show tokenizer vs ProviderActual numbers only", () => {
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
    const html = renderToStaticMarkup(
      <ContextUsageBadgeContent
        view={selectContextUsageView(state)}
        open={true}
        panelId="context-usage-parity"
        onToggle={() => undefined}
      />
    );
    assert.match(html, /本地 Tokenizer/);
    assert.match(html, /Provider 实际/);
    assert.match(html, /差值/);
    assert.match(html, /相对误差/);
    assert.match(html, /4,545/);
    assert.match(html, /4,561/);
    assert.match(html, /-16/);
    assert.doesNotMatch(html, /hello world/);
    assert.doesNotMatch(html, /tool_call/);
  });
});

describe("R1 session restore badge states", () => {
  it("D. last-known value stays visible while refreshing", () => {
    let state = emptyContextUsageState(identity);
    state = applyContextUsageSnapshot(state, snapshot({ estimated_input_tokens: 11_300 }));
    state = { ...state, refreshing: true };
    const html = renderToStaticMarkup(
      <ContextUsageBadgeContent
        view={selectContextUsageView(state)}
        open={true}
        panelId="refreshing"
        onToggle={() => undefined}
      />
    );
    assert.match(html, /11\.3K|~11K|11,300|11K/);
    assert.match(html, /正在刷新/);
    assert.doesNotMatch(html, />正在计算…</);
    assert.match(html, /data-context-ring-state="refreshing"/);
  });

  it("O. unavailable replaces infinite calculating", () => {
    let state = emptyContextUsageState(identity);
    state = failContextUsageProjection(beginContextUsageRefresh(state));
    const html = renderToStaticMarkup(
      <ContextUsageBadgeContent
        view={selectContextUsageView(state)}
        open={true}
        panelId="unavailable"
        onToggle={() => undefined}
      />
    );
    assert.match(html, /上下文暂不可用/);
    assert.doesNotMatch(html, /正在计算…/);
    assert.match(html, /data-context-ring-state="unavailable"/);
    assert.match(html, /data-context-ring-tone="error"/);
  });
});
