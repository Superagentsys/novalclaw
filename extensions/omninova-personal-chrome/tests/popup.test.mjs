import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { popupAuthorizationView } from "../dist/popup.js";

describe("popup authorization projection", () => {
  it("defaults to off when transport is disconnected", () => {
    const view = popupAuthorizationView("disconnected", null);
    assert.match(view.title, /未连接/);
    assert.equal(view.canAuthorize, false);
    assert.equal(view.canRevoke, false);
  });

  it("offers an explicit current-tab grant when connected", () => {
    const view = popupAuthorizationView("connected", null);
    assert.match(view.title, /未授权/);
    assert.equal(view.canAuthorize, true);
    assert.equal(view.canRevoke, false);
  });

  it("offers revoke for the one authorized tab", () => {
    const view = popupAuthorizationView("connected", 12, 12);
    assert.match(view.title, /当前标签页已授权/);
    assert.equal(view.canAuthorize, true);
    assert.equal(view.canRevoke, true);
  });

  it("distinguishes an authorized other tab without silently switching", () => {
    const view = popupAuthorizationView("connected", 12, 13);
    assert.match(view.title, /其他标签页/);
    assert.match(view.detail, /明确切换/);
    assert.equal(view.canAuthorize, true);
    assert.equal(view.canRevoke, true);
  });

  it("does not authorize across a protocol mismatch", () => {
    const view = popupAuthorizationView("protocol_mismatch", null);
    assert.match(view.title, /协议不兼容/);
    assert.equal(view.canAuthorize, false);
  });
});
