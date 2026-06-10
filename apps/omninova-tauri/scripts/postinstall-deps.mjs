#!/usr/bin/env node

/**
 * Post-install script for OmniNova Claw.
 * Ensures optional runtime dependencies are available.
 *
 * Usage:
 *   node scripts/postinstall-deps.mjs          # check only
 *   node scripts/postinstall-deps.mjs --install # auto-install missing deps
 */

import { execSync } from "node:child_process";

const INSTALL_MODE = process.argv.includes("--install");

const DEPS = [
  {
    name: "agent-browser",
    check: "agent-browser --version",
    install: [
      "npm install -g agent-browser",
      "agent-browser install",
    ],
    description: "Headless browser automation for AI agents",
    required: false,
  },
];

let allOk = true;

for (const dep of DEPS) {
  process.stdout.write(`  Checking ${dep.name}... `);

  try {
    const version = execSync(dep.check, { encoding: "utf-8", timeout: 15_000 }).trim();
    console.log(`OK (${version})`);
  } catch {
    if (INSTALL_MODE) {
      console.log("not found, installing...");
      try {
        for (const cmd of dep.install) {
          console.log(`    $ ${cmd}`);
          execSync(cmd, { stdio: "inherit", timeout: 300_000 });
        }
        const version = execSync(dep.check, { encoding: "utf-8", timeout: 15_000 }).trim();
        console.log(`    Installed: ${version}`);
      } catch (installErr) {
        console.error(`    FAILED: ${installErr.message}`);
        if (dep.required) {
          allOk = false;
        }
      }
    } else {
      console.log(dep.required ? "MISSING (required)" : "MISSING (optional)");
      console.log(`    ${dep.description}`);
      console.log(`    Install: ${dep.install.join(" && ")}`);
      console.log(`    Or run:  node scripts/postinstall-deps.mjs --install`);
      if (dep.required) {
        allOk = false;
      }
    }
  }
}

if (!allOk) {
  console.error("\nSome required dependencies are missing.");
  process.exit(1);
}
console.log("\nDependency check complete.");
