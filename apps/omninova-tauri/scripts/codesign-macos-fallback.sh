#!/usr/bin/env bash
# Ad-hoc codesign macOS .app bundles when Developer ID cert is not configured.
# Does NOT replace notarization — users may still need to remove quarantine
# (see strip-quarantine-mac.sh). Tauri also accepts APPLE_SIGNING_IDENTITY="-".
set -euo pipefail

BUNDLE_ROOT="${1:-}"

if [[ -z "${BUNDLE_ROOT}" || ! -d "${BUNDLE_ROOT}" ]]; then
  echo "Usage: $0 <bundle-directory>" >&2
  exit 1
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  exit 0
fi

sign_app() {
  local app="$1"
  echo "[codesign] Ad-hoc signing: ${app}"
  # Sign nested binaries first, then the bundle
  find "${app}" -type f \( -perm +111 -o -name "*.dylib" \) 2>/dev/null | while read -r f; do
    codesign --force --sign - "${f}" 2>/dev/null || true
  done
  codesign --force --deep --sign - "${app}"
}

# macOS CI uses Bash 3.2 (no globstar); use find instead of **/*.app
signed=0
while IFS= read -r app; do
  [[ -d "${app}" ]] || continue
  if codesign --verify --deep --strict "${app}" 2>/dev/null; then
    echo "[codesign] Already signed: ${app}"
    continue
  fi
  sign_app "${app}"
  signed=$((signed + 1))
done <<EOF
$(find "${BUNDLE_ROOT}" -type d -name '*.app')
EOF

if [[ "${signed}" -eq 0 ]]; then
  echo "[codesign] No unsigned .app found under ${BUNDLE_ROOT}"
else
  echo "[codesign] Ad-hoc signed ${signed} bundle(s)"
fi
