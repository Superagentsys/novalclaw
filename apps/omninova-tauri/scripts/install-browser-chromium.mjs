#!/usr/bin/env node

/**
 * Run the official `agent-browser install` using the staged native CLI.
 * Does not reimplement Chrome-for-Testing download; does not use global npm.
 */

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(__dirname, "..");
const resourcesRoot = path.join(appRoot, "src-tauri", "resources", "agent-browser");

function stagedBinary() {
  if (process.platform === "win32") {
    return path.join(resourcesRoot, "windows", "agent-browser.exe");
  }
  if (process.platform === "darwin") {
    return path.join(resourcesRoot, "macos", "agent-browser");
  }
  return path.join(resourcesRoot, "linux", "agent-browser");
}

const bin = stagedBinary();
if (!fs.existsSync(bin)) {
  console.error(
    "install-browser-chromium: staged CLI missing at %s. Run: npm run prepare:browser-runtime",
    bin
  );
  process.exit(1);
}

console.log(`$ ${bin} install`);
const result = spawnSync(bin, ["install"], {
  stdio: "inherit",
  timeout: 300_000,
  windowsHide: true,
});
if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}
process.exit(result.status ?? 1);
