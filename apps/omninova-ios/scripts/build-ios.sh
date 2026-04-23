#!/usr/bin/env bash
# OmniNova iOS build helper.
# 1) 生成 Xcode 工程（XcodeGen）
# 2) 为 Simulator 构建（CI 默认无签名）
#
# 依赖：macOS 主机、xcode 15+、xcodegen（`brew install xcodegen`）。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IOS_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${IOS_DIR}"

CONFIGURATION="${CONFIGURATION:-Debug}"
DESTINATION="${DESTINATION:-generic/platform=iOS Simulator}"
SCHEME="${SCHEME:-OmniNovaPhoneAgent}"

echo "[ios] working directory: ${IOS_DIR}"

if ! command -v xcodegen >/dev/null 2>&1; then
    echo "ERROR: xcodegen is required. Install via: brew install xcodegen" >&2
    exit 1
fi

echo "[ios] generating Xcode project from project.yml"
xcodegen generate --spec project.yml

echo "[ios] xcodebuild -scheme ${SCHEME} -configuration ${CONFIGURATION}"
xcodebuild \
    -project OmniNovaPhoneAgent.xcodeproj \
    -scheme "${SCHEME}" \
    -configuration "${CONFIGURATION}" \
    -destination "${DESTINATION}" \
    -derivedDataPath build \
    CODE_SIGNING_ALLOWED=NO \
    CODE_SIGNING_REQUIRED=NO \
    CODE_SIGN_IDENTITY="" \
    build

echo "[ios] build completed. Artifacts under: ${IOS_DIR}/build/Build/Products/${CONFIGURATION}-iphonesimulator/"
