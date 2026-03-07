# OmniNova Tauri Secrets Template

Use this document as a checklist when configuring GitHub Actions secrets and variables for desktop/mobile signing and release.

## Release Trigger

- Push a git tag like `v0.1.0` to trigger the desktop release workflow automatically.
- Desktop artifacts are staged with this naming pattern:

```text
omninova-claw_<version>_<platform>_<original-bundle-name>.<ext>
```

Examples:

```text
omninova-claw_0.1.0_linux-x64_omninova-claw_0.1.0_amd64.deb
omninova-claw_0.1.0_macos-arm64_omninova-claw_0.1.0_aarch64.dmg
omninova-claw_0.1.0_windows-x64_omninova-claw_0.1.0_x64_en-us.msi
```

## GitHub Release

- `GITHUB_TOKEN`
  - Provided automatically by GitHub Actions.
  - Used by the release job to publish assets to GitHub Releases.

## macOS Signing / Notarization

Set these when you want signed and notarized macOS bundles:

- `APPLE_CERTIFICATE`
  - Base64-encoded `.p12` signing certificate.
- `APPLE_CERTIFICATE_PASSWORD`
  - Password for the `.p12` certificate.
- `APPLE_SIGNING_IDENTITY`
  - Example: `Developer ID Application: Your Company (TEAMID)`
- `APPLE_ID`
  - Apple developer account email.
- `APPLE_PASSWORD`
  - App-specific password used for notarization.
- `APPLE_TEAM_ID`
  - Apple developer team ID.

Optional API-key-based notarization alternative:

- `APPLE_API_ISSUER`
- `APPLE_API_KEY`
  - App Store Connect API key ID.
- `APPLE_API_PRIVATE_KEY`
  - The contents of the `.p8` private key.

## Android Signing

Set these when you want signed `.apk` or `.aab` outputs:

- `ANDROID_KEYSTORE_BASE64`
  - Base64-encoded keystore file.
- `ANDROID_KEYSTORE_PASSWORD`
- `ANDROID_KEY_ALIAS`
- `ANDROID_KEY_PASSWORD`
- `ANDROID_HOME` or `ANDROID_SDK_ROOT`
  - Usually configured as environment variables on the runner instead of secrets.

## iOS Signing

Set these when you want signed iOS archives:

- `IOS_CERTIFICATE`
  - Base64-encoded iOS signing certificate (`.p12`).
- `IOS_CERTIFICATE_PASSWORD`
- `IOS_MOBILE_PROVISION`
  - Base64-encoded provisioning profile (`.mobileprovision`).
- `IOS_TEAM_ID`
- `IOS_BUNDLE_IDENTIFIER`
  - Example: `com.omninova.claw`

Optional App Store Connect API credentials:

- `APPLE_API_ISSUER`
- `APPLE_API_KEY`
- `APPLE_API_PRIVATE_KEY`

## Workflow Behavior

- macOS signing steps run only when the corresponding Apple secrets are present.
- Android signing configuration runs only when all four Android keystore secrets are present.
- iOS certificate import and provisioning profile installation run only when the matching iOS secrets are present.
- Without signing secrets, the workflows still build unsigned artifacts when the platform toolchains are available.

## Optional Tauri Updater Signing

Only needed if you later enable Tauri updater packages:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

## Suggested Repository Variables

Store non-secret reusable values as GitHub Actions variables:

- `OMNINOVA_PRODUCT_NAME=OmniNova Claw`
- `OMNINOVA_BUNDLE_ID=com.omninova.claw`
- `ANDROID_APPLICATION_ID=com.omninova.claw`

## Setup Tips

Base64 helper examples:

```bash
# macOS / Linux
base64 -i certificate.p12 | pbcopy

# keystore
base64 -i release.keystore | pbcopy
```
