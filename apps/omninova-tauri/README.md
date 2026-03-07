# OmniNova Tauri

Tauri desktop and mobile shell for OmniNova Claw.

## Development

```bash
npm run dev
npm run tauri dev
```

## Frontend Build

```bash
npm run build
```

## Environment Checks

```bash
npm run check:build-env
npm run check:build-env:desktop
npm run check:build-env:mobile
```

## Cross-Platform Build Commands

List all available build commands:

```bash
npm run build:list
```

Desktop targets:

```bash
npm run build:desktop
npm run build:all:desktop
npm run build:linux
npm run build:linux:arm64
npm run build:macos
npm run build:macos:intel
npm run build:macos:apple
npm run build:windows
npm run build:windows:arm64
```

Mobile targets:

```bash
npm run mobile:init:android
npm run mobile:init:ios
npm run build:android
npm run build:ios
```

## GitHub Release

- Push a tag such as `v0.1.0` to trigger the desktop release workflow.
- The workflow publishes staged desktop assets to GitHub Releases automatically.
- Release asset names follow this pattern:

```text
omninova-claw_<version>_<platform>_<original-bundle-name>.<ext>
```

## Signing Secrets Template

- Reference: `../../.github/omninova-tauri-secrets.example.md`
- Use that file as the checklist for Android/iOS/macOS signing variables and GitHub Actions secrets.
- The GitHub workflow enables signing/notarization steps automatically when the corresponding secrets are configured.

## Notes

- Cross-platform desktop builds require the matching Rust target toolchain and platform-specific native dependencies.
- Mobile builds require Android Studio or Xcode and the corresponding Tauri mobile toolchain setup.
- `npm run build:all:desktop` will attempt all configured desktop targets sequentially and print a success/failure summary.
- Extra Tauri CLI arguments can be passed through the helper script:

```bash
node ./scripts/build-platform.mjs windows --bundles nsis,msi
```
