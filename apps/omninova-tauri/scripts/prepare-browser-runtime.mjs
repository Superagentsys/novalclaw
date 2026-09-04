#!/usr/bin/env node

/**
 * Extract the pinned `agent-browser` native CLI into Tauri resources.
 *
 * Source of truth: apps/omninova-tauri/node_modules/agent-browser
 * (exact version in package.json, fetched by npm — never a user-home copy).
 *
 * Output (current OS only):
 *   src-tauri/resources/agent-browser/<os>/agent-browser[.exe]
 *   src-tauri/resources/agent-browser/LICENSE
 */

import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(__dirname, "..");
const resourcesRoot = path.join(appRoot, "src-tauri", "resources", "agent-browser");
const pkgRoot = path.join(appRoot, "node_modules", "agent-browser");
const appPkg = JSON.parse(
  fs.readFileSync(path.join(appRoot, "package.json"), "utf8")
);
const pinned = appPkg.devDependencies?.["agent-browser"];

if (!pinned || pinned.startsWith("^") || pinned.startsWith("~") || pinned === "*") {
  console.error(
    "prepare-browser-runtime: pin an exact agent-browser version in devDependencies (got %s)",
    pinned ?? "(missing)"
  );
  process.exit(1);
}

if (!fs.existsSync(path.join(pkgRoot, "package.json"))) {
  console.error(
    "prepare-browser-runtime: %s is missing. Run: npm install",
    pkgRoot
  );
  process.exit(1);
}

const pkg = JSON.parse(fs.readFileSync(path.join(pkgRoot, "package.json"), "utf8"));
if (pkg.version !== pinned) {
  console.error(
    "prepare-browser-runtime: installed agent-browser@%s does not match pin %s. Run: npm install",
    pkg.version,
    pinned
  );
  process.exit(1);
}

function isMusl() {
  if (process.platform !== "linux") {
    return false;
  }
  try {
    const result = execFileSync("ldd", ["--version"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
    return result.toLowerCase().includes("musl");
  } catch {
    return (
      fs.existsSync("/lib/ld-musl-x86_64.so.1") ||
      fs.existsSync("/lib/ld-musl-aarch64.so.1")
    );
  }
}

function nativeBinaryName() {
  const plat = process.platform;
  const effectiveArch =
    plat === "win32" && process.arch === "arm64" ? "x64" : process.arch;
  if (plat === "win32") {
    return `agent-browser-win32-${effectiveArch}.exe`;
  }
  if (plat === "darwin") {
    const darwinArch = effectiveArch === "arm64" ? "arm64" : "x64";
    return `agent-browser-darwin-${darwinArch}`;
  }
  if (plat === "linux") {
    const osKey = isMusl() ? "linux-musl" : "linux";
    return `agent-browser-${osKey}-${effectiveArch}`;
  }
  throw new Error(`Unsupported platform: ${plat}-${process.arch}`);
}

function destParts() {
  if (process.platform === "win32") {
    return ["windows", "agent-browser.exe"];
  }
  if (process.platform === "darwin") {
    return ["macos", "agent-browser"];
  }
  return ["linux", "agent-browser"];
}

const binaryName = nativeBinaryName();
const source = path.join(pkgRoot, "bin", binaryName);
if (!fs.existsSync(source)) {
  console.error(
    "prepare-browser-runtime: native binary missing at %s (agent-browser postinstall should download it from GitHub releases).",
    source
  );
  process.exit(1);
}

const [osDir, destName] = destParts();
const destDir = path.join(resourcesRoot, osDir);
const dest = path.join(destDir, destName);
fs.mkdirSync(destDir, { recursive: true });
const fileSha256 = (file) =>
  createHash("sha256").update(fs.readFileSync(file)).digest("hex");
const destinationIsCurrent =
  fs.existsSync(dest) &&
  fs.statSync(source).size === fs.statSync(dest).size &&
  fileSha256(source) === fileSha256(dest);
if (destinationIsCurrent) {
  console.log(`reusing staged agent-browser ${pkg.version} (binary unchanged)`);
} else {
  try {
    fs.copyFileSync(source, dest);
  } catch (error) {
    if (error?.code === "EBUSY") {
      console.error(
        "prepare-browser-runtime: staged CLI is in use and differs from the pinned binary. Close OmniNova browser sessions and retry."
      );
    }
    throw error;
  }
}
if (process.platform !== "win32") {
  fs.chmodSync(dest, 0o755);
}

const licenseSrc = path.join(pkgRoot, "LICENSE");
if (fs.existsSync(licenseSrc)) {
  fs.copyFileSync(licenseSrc, path.join(resourcesRoot, "LICENSE"));
}

const versionProbe = spawnSync(dest, ["--version"], {
  encoding: "utf8",
  timeout: 15_000,
  windowsHide: true,
});
if (versionProbe.error || versionProbe.status !== 0) {
  console.error(
    "prepare-browser-runtime: staged binary failed --version: %s",
    versionProbe.stderr || versionProbe.error?.message || "(no output)"
  );
  process.exit(1);
}

const versionText = (versionProbe.stdout || versionProbe.stderr || "").trim();
console.log(`prepared agent-browser ${pkg.version}`);
console.log(`  source: ${source}`);
console.log(`  dest:   ${dest}`);
console.log(`  --version: ${versionText}`);
console.log(`  size: ${fs.statSync(dest).size} bytes`);
console.log(`  host: ${os.platform()}-${os.arch()}`);
