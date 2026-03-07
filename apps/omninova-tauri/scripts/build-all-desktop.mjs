import { spawnSync } from "node:child_process";
import process from "node:process";
import { desktopPlatformOrder } from "./platforms.mjs";

const extraArgs = process.argv.slice(2);
const results = [];
const normalizedEnv = {
  ...process.env,
  // Tauri expects CI to be the literal string true/false.
  CI: process.env.CI === "1" ? "true" : process.env.CI,
};

for (const platform of desktopPlatformOrder) {
  console.log(`\n=== Building ${platform} ===`);

  const tauriArgs = [
    "run",
    "tauri",
    "--",
    ...["build"],
  ];

  if (platform === "linux") {
    tauriArgs.push("--target", "x86_64-unknown-linux-gnu");
  } else if (platform === "linux:arm64") {
    tauriArgs.push("--target", "aarch64-unknown-linux-gnu");
  } else if (platform === "macos:intel") {
    tauriArgs.push("--target", "x86_64-apple-darwin");
  } else if (platform === "macos:apple") {
    tauriArgs.push("--target", "aarch64-apple-darwin");
  } else if (platform === "windows") {
    tauriArgs.push("--target", "x86_64-pc-windows-msvc");
  } else if (platform === "windows:arm64") {
    tauriArgs.push("--target", "aarch64-pc-windows-msvc");
  }

  tauriArgs.push(...extraArgs);

  const result = spawnSync("npm", tauriArgs, {
    stdio: "inherit",
    shell: process.platform === "win32",
    env: normalizedEnv,
  });

  results.push({
    platform,
    success: result.status === 0,
    status: result.status ?? 1,
  });
}

const succeeded = results.filter((item) => item.success).map((item) => item.platform);
const failed = results.filter((item) => !item.success).map((item) => item.platform);

console.log("\n=== Desktop Build Summary ===");
console.log(
  `Succeeded: ${succeeded.length > 0 ? succeeded.join(", ") : "none"}`
);
console.log(`Failed: ${failed.length > 0 ? failed.join(", ") : "none"}`);

if (failed.length > 0) {
  console.log(
    "\nSome targets failed. This is expected if the current machine is missing the corresponding Rust targets, SDKs, or native packaging toolchains."
  );
  process.exit(1);
}
