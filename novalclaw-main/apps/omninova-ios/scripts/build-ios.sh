#!/usr/bin/env bash
# OmniNova iOS build helper.
# 1) 生成 Xcode 工程（XcodeGen）
# 2) 构建 Simulator 或 Device，并可打包 .ipa
#
# 依赖：macOS、Xcode 15+、xcodegen（`brew install xcodegen`）。
#
# 用法：
#   ./scripts/build-ios.sh                    # Simulator Debug
#   BUILD_DEVICE=1 ./scripts/build-ios.sh   # 真机 Release + 未签名 .ipa
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IOS_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${IOS_DIR}"

CONFIGURATION="${CONFIGURATION:-Debug}"
SCHEME="${SCHEME:-OmniNovaPhoneAgent}"
BUILD_DEVICE="${BUILD_DEVICE:-0}"

echo "[ios] working directory: ${IOS_DIR}"

if ! command -v xcodegen >/dev/null 2>&1; then
    echo "ERROR: xcodegen is required. Install via: brew install xcodegen" >&2
    exit 1
fi

echo "[ios] generating Xcode project from project.yml"
xcodegen generate --spec project.yml

if [ "${BUILD_DEVICE}" = "1" ]; then
    CONFIGURATION="${CONFIGURATION:-Release}"
    DERIVED="build-device"
    DESTINATION="generic/platform=iOS"
    echo "[ios] device build (unsigned) -configuration ${CONFIGURATION}"
    xcodebuild \
        -project OmniNovaPhoneAgent.xcodeproj \
        -scheme "${SCHEME}" \
        -configuration "${CONFIGURATION}" \
        -destination "${DESTINATION}" \
        -derivedDataPath "${DERIVED}" \
        CODE_SIGNING_ALLOWED=NO \
        CODE_SIGNING_REQUIRED=NO \
        CODE_SIGN_IDENTITY="" \
        CODE_SIGN_ENTITLEMENTS="" \
        AD_HOC_CODE_SIGNING_ALLOWED=NO \
        ENABLE_BITCODE=NO \
        VALIDATE_PRODUCT=NO \
        build

    APP="${DERIVED}/Build/Products/Release-iphoneos/OmniNovaPhoneAgent.app"
    if [ ! -d "${APP}" ]; then
        echo "ERROR: expected ${APP}" >&2
        exit 1
    fi
    OUT="release-assets/OmniNovaPhoneAgent-local-unsigned.ipa"
    mkdir -p release-assets ipa-staging/Payload
    rm -rf ipa-staging/Payload/OmniNovaPhoneAgent.app
    cp -R "${APP}" ipa-staging/Payload/
    (cd ipa-staging && zip -ry "../${OUT}" Payload)
    rm -rf ipa-staging
    echo "[ios] unsigned IPA: ${IOS_DIR}/${OUT}"
    echo "[ios] 真机安装需自行重签名（AltStore / Sideloadly / Xcode Archive）"
else
    DESTINATION="${DESTINATION:-generic/platform=iOS Simulator}"
    DERIVED="build-sim"
    echo "[ios] simulator build -scheme ${SCHEME} -configuration ${CONFIGURATION}"
    xcodebuild \
        -project OmniNovaPhoneAgent.xcodeproj \
        -scheme "${SCHEME}" \
        -configuration "${CONFIGURATION}" \
        -destination "${DESTINATION}" \
        -derivedDataPath "${DERIVED}" \
        CODE_SIGNING_ALLOWED=NO \
        CODE_SIGNING_REQUIRED=NO \
        CODE_SIGN_IDENTITY="" \
        build
    echo "[ios] simulator app: ${IOS_DIR}/${DERIVED}/Build/Products/${CONFIGURATION}-iphonesimulator/OmniNovaPhoneAgent.app"
fi
