#!/usr/bin/env node

/**
 * Post-install script for OmniNova Claw.
 * Checks the staged/bundled agent-browser CLI. Does not require a global npm install.
 *
 * Usage:
 *   node scripts/postinstall-deps.mjs          # check only
 *   node scripts/postinstall-deps.mjs --install # prepare CLI + official Chromium install
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const INSTALL_MODE = process.argv.includes("--install");
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(__dirname, "..");

function stagedBrowserCli() {
  if (process.platform === "win32") {
    return path.join(
      appRoot,
      "src-tauri",
      "resources",
      "agent-browser",
      "windows",
      "agent-browser.exe"
    );
  }
  if (process.platform === "darwin") {
    return path.join(
      appRoot,
      "src-tauri",
      "resources",
      "agent-browser",
      "macos",
      "agent-browser"
    );
  }
  return path.join(
    appRoot,
    "src-tauri",
    "resources",
    "agent-browser",
    "linux",
    "agent-browser"
  );
}

process.stdout.write("  Checking agent-browser (staged CLI)... ");
const staged = stagedBrowserCli();
if (fs.existsSync(staged)) {
  const probe = spawnSync(staged, ["--version"], {
    encoding: "utf8",
    timeout: 15_000,
    windowsHide: true,
  });
  if (probe.status === 0) {
    console.log(`OK (${(probe.stdout || "").trim() || "staged"})`);
  } else {
    console.log(`present but --version failed (${staged})`);
  }
} else if (INSTALL_MODE) {
  console.log("not staged, preparing...");
  const prepare = spawnSync("npm", ["run", "prepare:browser-runtime"], {
    cwd: appRoot,
    stdio: "inherit",
    shell: process.platform === "win32",
    timeout: 120_000,
  });
  if (prepare.status !== 0) {
    console.error("    FAILED: npm run prepare:browser-runtime");
  } else {
    console.log("    Running official agent-browser install (Chrome for Testing)...");
    const chromium = spawnSync(
      process.execPath,
      [path.join(__dirname, "install-browser-chromium.mjs")],
      {
        cwd: appRoot,
        stdio: "inherit",
        timeout: 300_000,
      }
    );
    if (chromium.status !== 0) {
      console.error("    Chromium install failed (CLI is still staged).");
    }
  }
} else {
  console.log("MISSING (optional)");
  console.log("    Headless browser automation for AI agents");
  console.log("    Install: npm run prepare:browser-runtime");
  console.log("    Chromium: npm run setup:browser");
  console.log("    A global `npm install -g agent-browser` is not required.");
}

console.log("\nDependency check complete.");
