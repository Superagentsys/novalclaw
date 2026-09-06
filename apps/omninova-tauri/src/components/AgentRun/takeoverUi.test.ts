import assert from "node:assert/strict";
import { describe, it } from "node:test";
import type { BrowserTakeoverStateDto } from "./types.ts";
import {
  applyAuthoritativeTakeoverState,
  shouldRefreshTakeoverFromEvent,
  shouldShowTakeoverCard,
  takeoverErrorCopy,
  takeoverPrimaryAction,
  takeoverStatusCopy,
} from "./takeoverUi.ts";

function dto(phase: string, extra: Partial<BrowserTakeoverStateDto> = {}): BrowserTakeoverStateDto {
  return {
    run_id: "run-a",
    session_id: "session-a",
    phase,
    owner: phase === "human_controlled" || phase === "timed_out" ? "human" : "agent",
    generation: 0,
    eligible: extra.eligible ?? true,
    headless: extra.headless ?? false,
    since_ms: 0,
    ...extra,
  };
}

describe("takeover UI projection", () => {
  it("shows Take Control for an eligible AgentControlled session", () => {
    const state = dto("agent_controlled");
    assert.equal(shouldShowTakeoverCard(state), true);
    assert.equal(takeoverPrimaryAction(state), "take");
    assert.match(takeoverStatusCopy(state).title, /Agent/);
  });

  it("renders waiting copy for TakeoverRequested and does not claim human control", () => {
    const copy = takeoverStatusCopy(dto("takeover_requested"));
    assert.match(copy.detail, /等待当前浏览器操作完成/);
    assert.doesNotMatch(copy.title, /属于你/);
    assert.equal(takeoverPrimaryAction(dto("takeover_requested")), "none");
  });

  it("renders Release for HumanControlled", () => {
    const state = dto("human_controlled");
    assert.equal(takeoverPrimaryAction(state), "release");
    assert.match(takeoverStatusCopy(state).title, /属于你/);
  });

  it("states TimedOut has not returned control to the Agent", () => {
    const copy = takeoverStatusCopy(dto("timed_out"));
    assert.match(copy.detail, /尚未交还给 Agent/);
    assert.equal(takeoverPrimaryAction(dto("timed_out")), "release");
  });

  it("renders refresh copy for Resynchronizing", () => {
    const copy = takeoverStatusCopy(dto("resynchronizing"));
    assert.match(copy.detail, /刷新浏览器状态/);
    assert.equal(takeoverPrimaryAction(dto("resynchronizing")), "none");
  });

  it("renders BrowserLost as a failure, not a successful release", () => {
    const state = dto("browser_lost", { eligible: false });
    assert.equal(shouldShowTakeoverCard(state), true);
    const copy = takeoverStatusCopy(state);
    assert.match(copy.title, /已丢失/);
    assert.match(copy.detail, /不是一次成功的交还/);
    assert.equal(copy.tone, "error");
  });

  it("does not locally assume HumanControlled from a click result of TakeoverRequested", () => {
    const previous = dto("agent_controlled");
    const next = applyAuthoritativeTakeoverState(previous, dto("takeover_requested"));
    assert.equal(next.phase, "takeover_requested");
    assert.notEqual(next.phase, "human_controlled");
  });

  it("matching events refresh the current run only", () => {
    assert.equal(
      shouldRefreshTakeoverFromEvent("browser_takeover_state_changed", "run-a", "run-a"),
      true
    );
    assert.equal(
      shouldRefreshTakeoverFromEvent("browser_takeover_state_changed", "run-b", "run-a"),
      false
    );
    assert.equal(shouldRefreshTakeoverFromEvent("tool_started", "run-a", "run-a"), false);
  });

  it("treats remount as an authoritative query, not previous-click ownership", () => {
    const restored = applyAuthoritativeTakeoverState(null, dto("human_controlled"));
    assert.equal(restored.phase, "human_controlled");
    assert.equal(takeoverPrimaryAction(restored), "release");
  });

  it("presents headless failure without implying a restart", () => {
    const copy = takeoverStatusCopy(dto("agent_controlled", { headless: true, eligible: false }));
    assert.match(copy.detail, /无头模式/);
    assert.doesNotMatch(copy.detail, /已重启|重新启动/);
    assert.match(
      takeoverErrorCopy("BrowserTakeoverUnsupportedHeadless: headed required"),
      /无头模式/
    );
  });

  it("hides the idle card for ineligible headless AgentControlled state", () => {
    assert.equal(
      shouldShowTakeoverCard(dto("agent_controlled", { headless: true, eligible: false })),
      false
    );
  });
});
