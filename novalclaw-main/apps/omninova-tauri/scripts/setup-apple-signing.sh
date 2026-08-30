#!/usr/bin/env bash
# Import Apple signing certificate into a temporary keychain and export env vars
# for Tauri macOS code signing + notarization.
#
# Required for signing (pick one path):
#   - APPLE_CERTIFICATE + APPLE_CERTIFICATE_PASSWORD (+ optional APPLE_SIGNING_IDENTITY)
#   - Signing identity already in login keychain (local dev only)
#
# Required for notarization (pick one path):
#   - APPLE_API_ISSUER + APPLE_API_KEY + APPLE_API_PRIVATE_KEY (or APPLE_API_KEY_PATH)
#   - APPLE_ID + APPLE_PASSWORD + APPLE_TEAM_ID
#
# Usage (CI):
#   bash ./scripts/setup-apple-signing.sh
# Usage (local, after exporting vars):
#   source <(bash ./scripts/setup-apple-signing.sh --print-env)
set -euo pipefail

emit_env() {
  local key="$1"
  local value="$2"
  if [[ -n "${GITHUB_ENV:-}" ]]; then
    {
      echo "${key}<<EOF"
      echo "${value}"
      echo "EOF"
    } >> "${GITHUB_ENV}"
  fi
  export "${key}=${value}"
}

print_env=false
if [[ "${1:-}" == "--print-env" ]]; then
  print_env=true
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "[signing] Not macOS — skipping Apple signing setup."
  exit 0
fi

# --- App Store Connect API key (notarization) ---
if [[ -n "${APPLE_API_PRIVATE_KEY:-}" && -n "${APPLE_API_ISSUER:-}" && -n "${APPLE_API_KEY:-}" ]]; then
  api_path="${APPLE_API_KEY_PATH:-${RUNNER_TEMP:-/tmp}/AuthKey_${APPLE_API_KEY}.p8}"
  if [[ ! -f "${api_path}" ]]; then
    printf '%s' "${APPLE_API_PRIVATE_KEY}" > "${api_path}"
  fi
  emit_env "APPLE_API_KEY_PATH" "${api_path}"
  emit_env "APPLE_API_ISSUER" "${APPLE_API_ISSUER}"
  emit_env "APPLE_API_KEY" "${APPLE_API_KEY}"
  echo "[signing] App Store Connect API key configured for notarization."
fi

# --- Import .p12 (CI) ---
if [[ -n "${APPLE_CERTIFICATE:-}" && -n "${APPLE_CERTIFICATE_PASSWORD:-}" ]]; then
  KEYCHAIN_PATH="${RUNNER_TEMP:-/tmp}/omninova-signing.keychain-db"
  CERT_PATH="${RUNNER_TEMP:-/tmp}/omninova-signing.p12"

  printf '%s' "${APPLE_CERTIFICATE}" | base64 --decode > "${CERT_PATH}"

  security create-keychain -p "" "${KEYCHAIN_PATH}" 2>/dev/null || true
  security set-keychain-settings -t 3600 -u "${KEYCHAIN_PATH}"
  security unlock-keychain -p "" "${KEYCHAIN_PATH}"
  security import "${CERT_PATH}" -P "${APPLE_CERTIFICATE_PASSWORD}" -A -f pkcs12 -k "${KEYCHAIN_PATH}"
  security list-keychains -d user -s "${KEYCHAIN_PATH}" $(security list-keychains -d user | tr -d '"')
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "" "${KEYCHAIN_PATH}"

  echo "[signing] Certificate imported into ${KEYCHAIN_PATH}"
  security find-identity -v -p codesigning "${KEYCHAIN_PATH}" || true

  if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  detected=$(
    security find-identity -v -p codesigning "${KEYCHAIN_PATH}" 2>/dev/null \
      | grep -E 'Developer ID Application|Apple Distribution|Apple Development' \
      | head -1 \
      | sed -E 's/^[[:space:]]*[0-9]+[[:space:]]+[0-9A-F]+[[:space:]]+"([^"]+)".*/\1/' \
      || true
  )
    if [[ -n "${detected}" ]]; then
      emit_env "APPLE_SIGNING_IDENTITY" "${detected}"
      echo "[signing] Auto-detected APPLE_SIGNING_IDENTITY=${detected}"
    else
      echo "::warning::Could not auto-detect signing identity. Set APPLE_SIGNING_IDENTITY secret explicitly."
    fi
  else
    emit_env "APPLE_SIGNING_IDENTITY" "${APPLE_SIGNING_IDENTITY}"
  fi
elif [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  emit_env "APPLE_SIGNING_IDENTITY" "${APPLE_SIGNING_IDENTITY}"
  echo "[signing] Using existing APPLE_SIGNING_IDENTITY from environment."
else
  # Local: try login keychain
  detected=$(
    security find-identity -v -p codesigning 2>/dev/null \
      | grep 'Developer ID Application' \
      | head -1 \
      | sed -E 's/^[[:space:]]*[0-9]+[[:space:]]+[0-9A-F]+[[:space:]]+"([^"]+)".*/\1/' \
      || true
  )
  if [[ -n "${detected}" ]]; then
    emit_env "APPLE_SIGNING_IDENTITY" "${detected}"
    echo "[signing] Using login keychain identity: ${detected}"
  elif [[ "${CI:-}" == "true" || "${GITHUB_ACTIONS:-}" == "true" ]]; then
    # Ad-hoc sign so Apple Silicon builds are not completely unsigned.
    # Users still need Developer ID + notarization to avoid Gatekeeper
    # "damaged" warnings on browser downloads — see docs/SIGNING_CN.md.
    emit_env "APPLE_SIGNING_IDENTITY" "-"
    echo "[signing] No Developer ID cert — using ad-hoc identity '-' for CI build."
  else
    echo "[signing] No signing certificate configured (unsigned build)."
  fi
fi

# Notarization account password path
for var in APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID; do
  if [[ -n "${!var:-}" ]]; then
    emit_env "${var}" "${!var}"
  fi
done

if [[ "${print_env}" == "true" && -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  echo "export APPLE_SIGNING_IDENTITY='${APPLE_SIGNING_IDENTITY}'"
  [[ -n "${APPLE_ID:-}" ]] && echo "export APPLE_ID='${APPLE_ID}'"
  [[ -n "${APPLE_PASSWORD:-}" ]] && echo "export APPLE_PASSWORD='${APPLE_PASSWORD}'"
  [[ -n "${APPLE_TEAM_ID:-}" ]] && echo "export APPLE_TEAM_ID='${APPLE_TEAM_ID}'"
  [[ -n "${APPLE_API_KEY_PATH:-}" ]] && echo "export APPLE_API_KEY_PATH='${APPLE_API_KEY_PATH}'"
  [[ -n "${APPLE_API_ISSUER:-}" ]] && echo "export APPLE_API_ISSUER='${APPLE_API_ISSUER}'"
  [[ -n "${APPLE_API_KEY:-}" ]] && echo "export APPLE_API_KEY='${APPLE_API_KEY}'"
fi
