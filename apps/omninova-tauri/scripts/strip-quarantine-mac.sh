#!/usr/bin/env bash
# Remove Gatekeeper quarantine from a downloaded OmniNova Claw .app (dev/test only).
# Official releases should be signed + notarized instead of using this script.
set -euo pipefail

APP_PATH="${1:-/Applications/OmniNova Claw.app}"

if [[ ! -d "${APP_PATH}" ]]; then
  echo "App not found: ${APP_PATH}" >&2
  echo "Usage: $0 [/path/to/OmniNova Claw.app]" >&2
  exit 1
fi

xattr -dr com.apple.quarantine "${APP_PATH}" 2>/dev/null || true
echo "Removed quarantine from: ${APP_PATH}"
echo "You can now: open \"${APP_PATH}\""
