import assert from "node:assert/strict";
import { describe, it } from "node:test";
import type { PersonalChromeAuthorizationStatusDto } from "./types.ts";
import {
  applyAuthoritativePersonalChromeStatus,
  personalChromeAuthorizationAction,
  personalChromeAuthorizationCopy,
  shouldShowPersonalChromeAuthorization,
} from "./personalChromeAuthorizationUi.ts";

function dto(
  extra: Partial<PersonalChromeAuthorizationStatusDto> = {}
): PersonalChromeAuthorizationStatusDto {
  return {
    run_id: "run-a",
    configured: true,
    state: "no_tab_authorized",
    transport_connected: true,
    protocol_version: 1,
    extension_tab_granted: false,
    desktop_run_granted: false,
    authorization_generation: null,
    production_factory_enabled: false,
    ready: false,
    error_code: null,
    ...extra,
  };
}

describe("Personal Chrome authorization UI projection", () => {
  it("hides when Personal Chrome is not configured and no grant exists", () => {
    assert.equal(shouldShowPersonalChromeAuthorization(dto({ configured: false })), false);
  });

  it("asks for the exact extension tab grant before Desktop approval", () => {
    const status = dto();
    assert.equal(personalChromeAuthorizationAction(status), "none");
    assert.match(personalChromeAuthorizationCopy(status).detail, /当前标签页/);
  });

  it("offers run-scoped approval only after the extension grant", () => {
    const status = dto({
      state: "awaiting_desktop_approval",
      extension_tab_granted: true,
    });
    assert.equal(personalChromeAuthorizationAction(status), "approve");
    assert.match(personalChromeAuthorizationCopy(status).detail, /本次任务/);
  });

  it("does not claim production readiness while the release gate is closed", () => {
    const status = dto({
      state: "authorized_release_gate_closed",
      extension_tab_granted: true,
      desktop_run_granted: true,
      authorization_generation: 7,
    });
    const copy = personalChromeAuthorizationCopy(status);
    assert.equal(personalChromeAuthorizationAction(status), "revoke");
    assert.match(copy.detail, /发布门禁关闭/);
    assert.doesNotMatch(copy.title, /已授权$/);
  });

  it("treats an extension generation change as stale authorization", () => {
    const status = dto({
      state: "authorization_stale",
      extension_tab_granted: true,
      desktop_run_granted: true,
      authorization_generation: null,
    });
    assert.equal(personalChromeAuthorizationAction(status), "revoke");
    assert.match(personalChromeAuthorizationCopy(status).title, /已变化/);
  });

  it("shows ready only from an authoritative ready status", () => {
    const status = dto({
      state: "ready",
      extension_tab_granted: true,
      desktop_run_granted: true,
      authorization_generation: 9,
      production_factory_enabled: true,
      ready: true,
    });
    assert.equal(personalChromeAuthorizationCopy(status).tone, "ready");
  });

  it("uses backend status on remount instead of remembered UI state", () => {
    const previous = dto({ extension_tab_granted: true, desktop_run_granted: true });
    const next = applyAuthoritativePersonalChromeStatus(previous, dto());
    assert.equal(next.extension_tab_granted, false);
    assert.equal(next.desktop_run_granted, false);
  });
});
