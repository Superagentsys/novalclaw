import { spawnSync } from "node:child_process";
import os from "node:os";
import process from "node:process";
import { desktopPlatformOrder, platformCommands } from "./platforms.mjs";

const scope = process.argv[2] ?? "all";
const knownPlatforms = new Set(Object.keys(platformCommands));

const run = (command, args = []) =>
  spawnSync(command, args, {
    encoding: "utf8",
    shell: process.platform === "win32",
  });

const commandExists = (command) => {
  const probe = process.platform === "win32" ? "where" : "which";
  const result = run(probe, [command]);
  return result.status === 0;
};

const getInstalledRustTargets = () => {
  const result = run("rustup", ["target", "list", "--installed"]);

  if (result.status !== 0) {
    return new Set();
  }

  return new Set(
    result.stdout
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean)
  );
};

const checks = [];
const addCheck = (label, ok, details) => checks.push({ label, ok, details });

const rustTargets = getInstalledRustTargets();

addCheck("Node.js", commandExists("node"), "Required for the Vite frontend.");
addCheck("npm", commandExists("npm"), "Required to run build scripts.");
addCheck("cargo", commandExists("cargo"), "Required for the Rust/Tauri backend.");
addCheck("rustup", commandExists("rustup"), "Required to manage Rust targets.");

const addRustTargetCheck = (label, target) => {
  addCheck(
    label,
    rustTargets.has(target),
    `Install with: rustup target add ${target}`
  );
};

const isSpecificPlatform = knownPlatforms.has(scope);
const wantsDesktop =
  scope === "all" ||
  scope === "desktop" ||
  (isSpecificPlatform && platformCommands[scope]?.category === "desktop");
const wantsMobile =
  scope === "all" ||
  scope === "mobile" ||
  (isSpecificPlatform && platformCommands[scope]?.category === "mobile");

if (wantsDesktop) {
  const desktopTargets =
    scope === "desktop" || scope === "all"
      ? desktopPlatformOrder
      : isSpecificPlatform
        ? [scope]
        : [];

  for (const platform of desktopTargets) {
    const target = platformCommands[platform]?.target;
    if (target) {
      addRustTargetCheck(`Rust target ${target}`, target);
    }
  }

  if (process.platform === "linux") {
    addCheck("pkg-config", commandExists("pkg-config"), "Needed by Tauri Linux builds.");
    addCheck("gcc", commandExists("gcc"), "Needed to compile native dependencies.");
  }

  if (process.platform === "darwin") {
    addCheck("xcodebuild", commandExists("xcodebuild"), "Needed for macOS packaging.");
  }

  if (process.platform === "win32") {
    addCheck("MSVC toolchain", commandExists("cl"), "Install Visual Studio C++ build tools.");
  }
}

if (wantsMobile) {
  const wantsAndroid =
    scope === "all" || scope === "mobile" || scope === "android" || scope === "android:init";
  const wantsIos =
    scope === "all" || scope === "mobile" || scope === "ios" || scope === "ios:init";

  if (wantsAndroid) {
    addRustTargetCheck("Rust target aarch64-linux-android", "aarch64-linux-android");
    addRustTargetCheck("Rust target armv7-linux-androideabi", "armv7-linux-androideabi");
    addRustTargetCheck("Rust target i686-linux-android", "i686-linux-android");
    addRustTargetCheck("Rust target x86_64-linux-android", "x86_64-linux-android");
    addCheck(
      "Java",
      commandExists("java"),
      "Android builds require a JDK."
    );
    addCheck(
      "Android SDK",
      Boolean(process.env.ANDROID_HOME || process.env.ANDROID_SDK_ROOT),
      "Set ANDROID_HOME or ANDROID_SDK_ROOT."
    );
    addCheck(
      "adb",
      commandExists("adb"),
      "Install Android platform tools."
    );
  }

  if (wantsIos) {
    addRustTargetCheck("Rust target aarch64-apple-ios", "aarch64-apple-ios");
    addRustTargetCheck("Rust target aarch64-apple-ios-sim", "aarch64-apple-ios-sim");

    const isMac = process.platform === "darwin";
    addCheck(
      "iOS host compatibility",
      isMac,
      "iOS builds require macOS."
    );

    if (isMac) {
      addCheck("Xcode", commandExists("xcodebuild"), "Required for iOS builds.");
    }
  }
}

console.log(`Build environment check for ${scope}`);
console.log(`Host: ${os.platform()} ${os.release()} (${os.arch()})\n`);

for (const check of checks) {
  const status = check.ok ? "OK" : "MISSING";
  console.log(`[${status}] ${check.label}`);
  if (!check.ok) {
    console.log(`       ${check.details}`);
  }
}

const failed = checks.filter((check) => !check.ok);

if (failed.length > 0) {
  console.log(`\nMissing items: ${failed.length}`);
  process.exit(1);
}

console.log("\nAll requested build prerequisites look good.");
