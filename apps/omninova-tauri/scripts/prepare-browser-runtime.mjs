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

/**
 * Which bundle this staging is for. Tauri sets TAURI_ENV_PLATFORM/ARCH to the
 * *target* of `tauri build --target ...` when it runs beforeBuildCommand, so a
 * cross build (Intel bundle on an Apple Silicon runner) must follow those
 * rather than the host. Staging by host arch is what shipped an arm64
 * agent-browser inside the macos-intel bundle.
 */
function resolveTarget() {
  const explicit = process.argv
    .find((arg) => arg.startsWith("--target="))
    ?.slice("--target=".length);
  // OMNINOVA_BUILD_TARGET comes from build-platform.mjs, which owns the
  // `--target` flag, so it is preferred over Tauri's hook environment: if the
  // hook variables were ever absent we would silently fall back to the host
  // and stage the wrong architecture again.
  const triple =
    explicit || process.env.OMNINOVA_BUILD_TARGET || process.env.TAURI_ENV_TARGET_TRIPLE || "";
  const tripleSource = explicit
    ? "--target"
    : process.env.OMNINOVA_BUILD_TARGET
      ? "OMNINOVA_BUILD_TARGET"
      : "TAURI_ENV_TARGET_TRIPLE";
  if (triple) {
    const arch = triple.startsWith("aarch64")
      ? "arm64"
      : triple.startsWith("x86_64")
        ? "x64"
        : null;
    const platform = triple.includes("apple-darwin")
      ? "darwin"
      : triple.includes("windows")
        ? "win32"
        : triple.includes("linux")
          ? "linux"
          : null;
    if (!arch || !platform) {
      throw new Error(`Unsupported target triple: ${triple}`);
    }
    return { platform, arch, source: tripleSource };
  }

  const envPlatform = process.env.TAURI_ENV_PLATFORM;
  const envArch = process.env.TAURI_ENV_ARCH;
  if (envPlatform && envArch) {
    const platform = { darwin: "darwin", windows: "win32", linux: "linux" }[envPlatform];
    const arch = { x86_64: "x64", aarch64: "arm64" }[envArch];
    if (!platform || !arch) {
      throw new Error(
        `Unsupported Tauri target: TAURI_ENV_PLATFORM=${envPlatform} TAURI_ENV_ARCH=${envArch}`
      );
    }
    return { platform, arch, source: "TAURI_ENV_PLATFORM/ARCH" };
  }

  return { platform: process.platform, arch: process.arch, source: "host" };
}

function nativeBinaryName(target) {
  const { platform, arch } = target;
  if (platform === "win32") {
    // agent-browser publishes no win32-arm64 build; ARM64 Windows runs the x64
    // binary through emulation.
    return "agent-browser-win32-x64.exe";
  }
  if (platform === "darwin") {
    return `agent-browser-darwin-${arch === "arm64" ? "arm64" : "x64"}`;
  }
  if (platform === "linux") {
    // musl is detected from the host: there is no cross-libc build in the
    // matrix, so a musl target is always built on a musl host.
    const osKey = isMusl() ? "linux-musl" : "linux";
    return `agent-browser-${osKey}-${arch}`;
  }
  throw new Error(`Unsupported platform: ${platform}-${arch}`);
}

function destParts(target) {
  if (target.platform === "win32") {
    return ["windows", "agent-browser.exe"];
  }
  if (target.platform === "darwin") {
    return ["macos", "agent-browser"];
  }
  return ["linux", "agent-browser"];
}

/**
 * Machine type from a Mach-O / ELF / PE header. Staging the wrong architecture
 * used to produce a bundle that only failed once a user launched the browser,
 * so the build asserts on this instead of trusting the file name.
 */
function binaryArch(file) {
  const fd = fs.openSync(file, "r");
  try {
    const head = Buffer.alloc(64);
    fs.readSync(fd, head, 0, 64, 0);
    if (head.readUInt32LE(0) === 0xfeedfacf) {
      const cpu = head.readUInt32LE(4);
      if (cpu === 0x0100000c) return "arm64";
      if (cpu === 0x01000007) return "x64";
      return `mach-o:0x${cpu.toString(16)}`;
    }
    if (head.readUInt32BE(0) === 0xcafebabe) {
      return "mach-o:universal";
    }
    if (head.readUInt32BE(0) === 0x7f454c46) {
      const machine = head.readUInt16LE(18);
      if (machine === 0x3e) return "x64";
      if (machine === 0xb7) return "arm64";
      return `elf:0x${machine.toString(16)}`;
    }
    if (head.readUInt16LE(0) === 0x5a4d) {
      const peOffset = head.readUInt32LE(0x3c);
      const coff = Buffer.alloc(6);
      fs.readSync(fd, coff, 0, 6, peOffset);
      if (coff.readUInt32LE(0) !== 0x00004550) return "pe:unknown";
      const machine = coff.readUInt16LE(4);
      if (machine === 0x8664) return "x64";
      if (machine === 0xaa64) return "arm64";
      return `pe:0x${machine.toString(16)}`;
    }
    return "unknown";
  } finally {
    fs.closeSync(fd);
  }
}

const target = resolveTarget();
const binaryName = nativeBinaryName(target);
const source = path.join(pkgRoot, "bin", binaryName);
if (!fs.existsSync(source)) {
  console.error(
    "prepare-browser-runtime: native binary missing at %s (agent-browser postinstall should download it from GitHub releases).",
    source
  );
  process.exit(1);
}

const [osDir, destName] = destParts(target);
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
if (target.platform !== "win32") {
  fs.chmodSync(dest, 0o755);
}

const licenseSrc = path.join(pkgRoot, "LICENSE");
if (fs.existsSync(licenseSrc)) {
  fs.copyFileSync(licenseSrc, path.join(resourcesRoot, "LICENSE"));
}

// Windows ARM64 intentionally ships the x64 binary, so it is the only target
// allowed to disagree with its bundle architecture.
const expectedArch = target.platform === "win32" ? "x64" : target.arch;
const stagedArch = binaryArch(dest);
if (stagedArch !== expectedArch) {
  console.error(
    "prepare-browser-runtime: staged %s is %s but this build targets %s-%s. Refusing to bundle a browser that cannot run on the target.",
    destName,
    stagedArch,
    target.platform,
    target.arch
  );
  process.exit(1);
}

// A cross-built binary cannot be executed here, so --version only runs when the
// target matches the host. The header check above covers the rest.
const runnable = target.platform === process.platform && target.arch === process.arch;
let versionText = "(skipped: cross build)";
if (runnable) {
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
  versionText = (versionProbe.stdout || versionProbe.stderr || "").trim();
}

console.log(`prepared agent-browser ${pkg.version}`);
console.log(`  source: ${source}`);
console.log(`  dest:   ${dest}`);
console.log(`  target: ${target.platform}-${target.arch} (from ${target.source})`);
console.log(`  arch:   ${stagedArch}`);
console.log(`  --version: ${versionText}`);
console.log(`  size: ${fs.statSync(dest).size} bytes`);
console.log(`  host: ${os.platform()}-${os.arch()}`);
