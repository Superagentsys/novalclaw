#!/usr/bin/env bash
# OmniNova Android build helper.
# 依赖：JDK 17、Android SDK 34（`ANDROID_SDK_ROOT` 或 `ANDROID_HOME`）
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AND_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${AND_DIR}"

BUILD_TYPE="${BUILD_TYPE:-Debug}"

SDK_HINT="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}"
if [[ -z "${SDK_HINT}" ]] && [[ ! -f local.properties ]]; then
    echo "WARN: ANDROID_SDK_ROOT/ANDROID_HOME 未设置且缺少 local.properties" >&2
    echo "      Gradle 可能无法定位 Android SDK" >&2
fi

if [[ ! -x gradlew ]]; then
    chmod +x gradlew || true
fi

echo "[android] working directory: ${AND_DIR}"
echo "[android] build type: ${BUILD_TYPE}"

case "${BUILD_TYPE,,}" in
    debug)   TASK="assembleDebug" ;;
    release) TASK="assembleRelease" ;;
    bundle)  TASK="bundleRelease" ;;
    *)       TASK="${BUILD_TYPE}" ;;
esac

./gradlew --no-daemon "${TASK}"

echo "[android] done. outputs:"
find app/build/outputs -type f \( -name "*.apk" -o -name "*.aab" \) -print 2>/dev/null || true
