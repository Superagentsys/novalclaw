import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

test("permissions stay transport-plus-authorized-tab and exclude debugger/cookies", () => {
  const manifest = JSON.parse(readFileSync(join(root, "manifest.json"), "utf8"));
  assert.equal(manifest.manifest_version, 3);
  assert.deepEqual(
    manifest.permissions.sort(),
    ["activeTab", "alarms", "nativeMessaging", "scripting", "storage", "tabs"].sort()
  );
  assert.equal(manifest.host_permissions, undefined);
  assert.equal(manifest.content_scripts, undefined);
  assert.deepEqual(manifest.optional_host_permissions.sort(), ["http://*/*", "https://*/*"].sort());
  const forbidden = [
    "debugger",
    "cookies",
    "history",
    "downloads",
    "webNavigation",
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

test("optional host permission is requested by the popup user gesture", () => {
  const popup = readFileSync(join(root, "src", "popup.ts"), "utf8");
  const background = readFileSync(join(root, "src", "background.ts"), "utf8");
  assert.match(popup, /chrome\.permissions\.request/);
  assert.doesNotMatch(background, /chrome\.permissions\.request/);
  assert.match(background, /chrome\.permissions\.contains/);
});
