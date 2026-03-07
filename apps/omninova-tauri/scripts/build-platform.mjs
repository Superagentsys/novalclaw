import { spawnSync } from "node:child_process";
import process from "node:process";
import { platformCommands } from "./platforms.mjs";

const normalizedEnv = {
  ...process.env,
  // Tauri expects CI to be the literal string true/false.
  CI: process.env.CI === "1" ? "true" : process.env.CI,
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
