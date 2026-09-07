#!/usr/bin/env node

/**
 * Clean-style smoke: staged native CLI only.
 * Unsets OMNINOVA_AGENT_BROWSER_BIN and drops npm from PATH.
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(__dirname, "..");
const bin =
  process.platform === "win32"
    ? path.join(
        appRoot,
        "src-tauri",
        "resources",
        "agent-browser",
        "windows",
        "agent-browser.exe"
      )
    : process.platform === "darwin"
      ? path.join(appRoot, "src-tauri", "resources", "agent-browser", "macos", "agent-browser")
      : path.join(appRoot, "src-tauri", "resources", "agent-browser", "linux", "agent-browser");

if (!fs.existsSync(bin)) {
  console.error("staged CLI missing:", bin);
  process.exit(1);
}

const session = `w27-clean-${Date.now().toString(36)}`;

function pathWithoutNpm(original) {
  const parts = (original || "")
    .split(path.delimiter)
    .filter((dir) => {
      const lower = dir.toLowerCase();
      return (
        !lower.includes("\\npm") &&
        !lower.includes("/npm") &&
        !lower.includes("node_modules") &&
        !lower.endsWith("\\npm") &&
        !lower.includes("roaming\\npm")
      );
    });
  return parts.join(path.delimiter);
}

const env = {
  ...process.env,
  PATH: pathWithoutNpm(process.env.PATH || process.env.Path || ""),
};
delete env.OMNINOVA_AGENT_BROWSER_BIN;

function run(args, timeoutMs = 60_000) {
  console.log(`$ ${path.basename(bin)} ${args.join(" ")}`);
  const result = spawnSync(bin, args, {
    encoding: "utf8",
    timeout: timeoutMs,
    env,
  });
  const stdout = result.stdout || "";
  const stderr = result.stderr || "";
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    console.error(stdout);
    console.error(stderr);
    throw new Error(`exit ${result.status}`);
  }
  const text = (stdout || stderr).trim();
  console.log(text.slice(0, 800));
  return text;
}

const prefix = ["--session", session, "--namespace", "omninova", "--json"];
try {
  run(["--version"], 15_000);
  run([...prefix, "open", "https://example.com"], 120_000);
  const title = run([...prefix, "get", "title"], 30_000);
  const snap = run([...prefix, "snapshot"], 30_000);
  if (!/example/i.test(title) && !/example/i.test(snap)) {
    console.error("title/snapshot did not mention example.com");
    process.exit(1);
  }
  console.log("CLEAN_STYLE_SMOKE=PASS");
  console.log(`BINARY=${bin}`);
  console.log(`HOST=${os.platform()}-${os.arch()}`);
} finally {
  try {
    run([...prefix, "close"], 30_000);
  } catch (error) {
    console.error("close failed:", error.message);
  }
}
