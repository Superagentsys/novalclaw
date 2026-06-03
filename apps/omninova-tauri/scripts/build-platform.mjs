import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";
import process from "node:process";
import { platformCommands } from "./platforms.mjs";

const normalizedEnv = {
  ...process.env,
  // Tauri expects CI to be the literal string true/false.
  CI: process.env.CI === "1" ? "true" : process.env.CI,
};

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const srcTauriDir = path.resolve(scriptDir, "../src-tauri");

/**
 * 在桌面应用打包前，先把随包 CLI（omninova 二进制）以 release 构建出来，
 * 使 src-tauri/build.rs 能将其复制进 resources/cli 一并打包。
 * 对带 --target 的交叉构建，传入相同 triple，保证产物路径与 build.rs 推导一致。
 */
const buildBundledCli = (command) => {
  if (command.category !== "desktop" || !command.args.includes("build")) {
    return;
  }
  const cargoArgs = [
    "build",
    "--release",
    "-p",
    "omninova-core",
    "--bin",
    "omninova",
  ];
  if (command.target) {
    cargoArgs.push("--target", command.target);
  }
  console.log(`> cargo ${cargoArgs.join(" ")}`);
  const result = spawnSync("cargo", cargoArgs, {
    stdio: "inherit",
    cwd: srcTauriDir,
    shell: process.platform === "win32",
    env: normalizedEnv,
  });
  if (result.error) {
    console.error(`构建随包 CLI 失败：${result.error.message}`);
    process.exit(1);
  }
  if ((result.status ?? 0) !== 0) {
    console.error("构建随包 CLI 失败：cargo 退出码非 0");
    process.exit(result.status ?? 1);
  }
};

const listCommands = () => {
  console.log("Available OmniNova Tauri build commands:\n");

  for (const [name, config] of Object.entries(platformCommands)) {
    console.log(`- ${name.padEnd(14)} ${config.description}`);
  }

  console.log(
    "\nYou can append extra Tauri CLI flags after the platform name, for example:"
  );
  console.log(
    "node ./scripts/build-platform.mjs windows --bundles nsis,msi"
  );
};

const platform = process.argv[2] ?? "list";
const extraArgs = process.argv.slice(3);

if (platform === "list" || platform === "--help" || platform === "-h") {
  listCommands();
  process.exit(0);
}

const command = platformCommands[platform];

if (!command) {
  console.error(`Unknown platform command: ${platform}\n`);
  listCommands();
  process.exit(1);
}

buildBundledCli(command);

const tauriArgs = ["run", "tauri", "--", ...command.args, ...extraArgs];

console.log(`> npm ${tauriArgs.join(" ")}`);

const result = spawnSync("npm", tauriArgs, {
  stdio: "inherit",
  shell: process.platform === "win32",
  env: normalizedEnv,
});

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}

process.exit(result.status ?? 0);
