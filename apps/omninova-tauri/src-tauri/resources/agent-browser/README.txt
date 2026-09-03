Place the agent-browser CLI binary in the OS folder so Tauri can bundle it:

  macos/agent-browser
  linux/agent-browser
  windows/agent-browser.exe

Do not copy binaries from a developer home directory and do not commit them.
Stage from the pinned npm package (exact version in apps/omninova-tauri/package.json):

  cd apps/omninova-tauri
  npm install
  npm run prepare:browser-runtime

That extracts node_modules/agent-browser/bin/<platform-native> into this tree.
`tauri dev` / `tauri build` run prepare automatically (beforeDevCommand / beforeBuildCommand).

Resolution order (unchanged):

  OMNINOVA_AGENT_BROWSER_BIN
  {resource_dir}/agent-browser/<os>/...
  {resource_dir}/resources/agent-browser/<os>/...
  {exe_dir}/resources/agent-browser/<os>/...
  PATH / unwrapped native npm exe (never spawn agent-browser.cmd)

The native CLI does **not** include Chromium. First launch uses system Chrome/Brave
if present; otherwise run the official installer (downloads Chrome for Testing):

  npm run setup:browser
  (prepare + staged `agent-browser.exe install`)

Clean Windows with a proper OmniNova installer:
  CLI: bundled in resources (no global npm, no PATH).
  Chromium: system Chrome, or `agent-browser install` / Setup "install browser"
  (uses the bundled CLI; does not require Node for the daemon).

agent-browser is Apache-2.0 (Vercel). LICENSE is copied next to this README at
prepare time and listed in THIRD_PARTY_NOTICES.md. Chromium/Chrome for Testing
is **not** redistributed in the OmniNova git tree or by this prepare step.
