import assert from "node:assert/strict";
import { test } from "node:test";
import {
  BACKEND_OPERATIONS,
  isRestrictedUrl,
  redactInputValue,
} from "../dist/protocol.js";

test("restricted chrome URLs are rejected", () => {
  assert.equal(isRestrictedUrl("chrome://settings"), true);
  assert.equal(isRestrictedUrl("chrome-extension://abc/popup.html"), true);
  assert.equal(isRestrictedUrl("https://chromewebstore.google.com/detail/x"), true);
  assert.equal(isRestrictedUrl("https://example.test/form"), false);
});

test("password values are not serialized", () => {
  assert.equal(redactInputValue("password", "super-secret-password"), undefined);
  assert.equal(redactInputValue("text", "alice"), "alice");
});

test("backend operations exclude cookies eval debugger and storage dumps", () => {
  assert.equal(BACKEND_OPERATIONS.includes("cookies"), false);
  assert.equal(BACKEND_OPERATIONS.includes("eval"), false);
  assert.equal(BACKEND_OPERATIONS.includes("debugger"), false);
  assert.equal(BACKEND_OPERATIONS.includes("storage_get"), false);
  assert.ok(BACKEND_OPERATIONS.includes("observe"));
  assert.ok(BACKEND_OPERATIONS.includes("authorize_tab_test_only"));
});
