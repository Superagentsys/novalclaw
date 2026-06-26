#!/usr/bin/env bash
# Verify that a macOS .app or .dmg is code-signed (and optionally notarized).
set -euo pipefail

ARTIFACT="${1:-}"
STRICT="${STRICT_MACOS_SIGNING:-0}"

if [[ -z "${ARTIFACT}" ]]; then
  echo "Usage: $0 <path-to-.app-or-.dmg>" >&2
  exit 1
fi

if [[ ! -e "${ARTIFACT}" ]]; then
  echo "::error::Artifact not found: ${ARTIFACT}" >&2
  exit 1
fi

echo "[verify] Checking: ${ARTIFACT}"
codesign -dv --verbose=2 "${ARTIFACT}" 2>&1 || true

if codesign --verify --deep --strict "${ARTIFACT}" 2>/dev/null; then
  echo "[verify] codesign --verify: OK"
else
  msg="Not signed or signature invalid: ${ARTIFACT}"
  if [[ "${STRICT}" == "1" ]]; then
    echo "::error::${msg}" >&2
    exit 1
  fi
  echo "::warning::${msg}"
  exit 0
fi

# Stapled notarization (optional; only for dmg/pkg)
if [[ "${ARTIFACT}" == *.dmg || "${ARTIFACT}" == *.pkg ]]; then
  if xcrun stapler validate "${ARTIFACT}" 2>/dev/null; then
    echo "[verify] stapler validate: OK (notarized)"
  else
    echo "[verify] stapler: not stapled or not notarized (may still be OK if ticket is online)"
  fi
fi
