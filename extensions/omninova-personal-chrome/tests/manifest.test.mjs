import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

test("transport-only permissions and stable public key", () => {
  const manifest = JSON.parse(readFileSync(join(root, "manifest.json"), "utf8"));
  assert.equal(manifest.manifest_version, 3);
  assert.deepEqual(manifest.permissions.sort(), ["alarms", "nativeMessaging", "storage"].sort());
  assert.equal(manifest.host_permissions, undefined);
  assert.equal(manifest.content_scripts, undefined);
  const forbidden = [
    "debugger",
    "cookies",
    "history",
    "downloads",
    "webNavigation",
    "scripting",
    "activeTab",
    "<all_urls>",
  ];
  const rendered = JSON.stringify(manifest);
  for (const name of forbidden) {
    assert.equal(rendered.includes(name), false, `must not request ${name}`);
  }
  assert.equal(typeof manifest.key, "string");
  assert.equal(manifest.key.includes("PRIVATE"), false);
  assert.equal(manifest.background.service_worker, "dist/background.js");
});
