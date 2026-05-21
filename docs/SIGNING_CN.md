# OmniNova Claw 发布签名指南

## macOS 桌面版（OmniNova Claw）

### 为什么会出现「已损坏，无法打开」？

从浏览器（Chrome 等）下载的 **未公证** `.dmg` / `.app` 会被 macOS Gatekeeper 拦截，并显示「**已损坏，无法打开**」。**应用通常没有坏**，只是系统拒绝启动。

**立刻能打开（复制到终端执行）：**

```bash
xattr -dr com.apple.quarantine "/Applications/OmniNova Claw.app"
open "/Applications/OmniNova Claw.app"
```

若还在 DMG 里未拖入应用程序：

```bash
xattr -dr com.apple.quarantine "/Volumes/OmniNova Claw"*/OmniNova\ Claw.app
open "/Volumes/OmniNova Claw"*/OmniNova\ Claw.app
```

或使用仓库脚本：

```bash
cd apps/omninova-tauri
bash ./scripts/strip-quarantine-mac.sh "/Applications/OmniNova Claw.app"
```

Release 包内也会附带 `macOS-安装与修复.txt`（同上说明）。

**正式发布：** 必须在 CI 或本机完成 **Developer ID 签名 + Apple 公证（Notarization）**。

---

### 1. 准备 Apple 证书

1. 登录 [Apple Developer](https://developer.apple.com/account/)
2. **Certificates** → 创建 **Developer ID Application**（对外分发 DMG，非 App Store）
3. 在 Mac 上安装 `.cer`，用「钥匙串访问」导出为 `.p12`（记住密码）

查看本机签名身份：

```bash
security find-identity -v -p codesigning
```

记下类似：`Developer ID Application: 公司名 (TEAMID)` → 用作 `APPLE_SIGNING_IDENTITY`。

---

### 2. 配置 GitHub Actions Secrets

在仓库 **Settings → Secrets and variables → Actions** 添加：

| Secret | 说明 |
|--------|------|
| `APPLE_CERTIFICATE` | `base64 -i cert.p12 \| pbcopy` |
| `APPLE_CERTIFICATE_PASSWORD` | 导出 .p12 时的密码 |
| `APPLE_SIGNING_IDENTITY` | 可选；不填则 CI 自动从证书检测 |
| `APPLE_ID` | Apple ID 邮箱（公证用） |
| `APPLE_PASSWORD` | [App 专用密码](https://appleid.apple.com) |
| `APPLE_TEAM_ID` | 10 位 Team ID |

**或使用 API Key 公证（推荐，可代替 APPLE_ID/PASSWORD）：**

| Secret | 说明 |
|--------|------|
| `APPLE_API_ISSUER` | App Store Connect Issuer ID |
| `APPLE_API_KEY` | Key ID |
| `APPLE_API_PRIVATE_KEY` | `.p8` 文件全文 |

完整清单：`.github/omninova-tauri-secrets.example.md`

---

### 3. 触发已签名 Release

```bash
git tag v0.1.5.10
git push origin v0.1.5.10
```

Workflow 会：

1. `scripts/setup-apple-signing.sh` — 导入证书、设置 `APPLE_SIGNING_IDENTITY` 与公证环境变量  
2. `npm run build:macos:*` — Tauri 自动 **签名 + 公证 + staple**  
3. `scripts/verify-macos-signature.sh` — 校验 tag 构建的 `.app` / `.dmg`  

Release 页应出现已签名的 `omninova-claw_*_macos-*.dmg`。

---

### 4. 本机签名构建

```bash
cd apps/omninova-tauri
npm ci

# 方式 A：证书已在钥匙串
export APPLE_SIGNING_IDENTITY="Developer ID Application: 公司名 (TEAMID)"
export APPLE_ID="your@apple.id"
export APPLE_PASSWORD="xxxx-xxxx-xxxx-xxxx"
export APPLE_TEAM_ID="XXXXXXXXXX"
npm run build:macos:apple

# 方式 B：使用 .p12
export APPLE_CERTIFICATE="$(base64 -i ~/Downloads/cert.p12)"
export APPLE_CERTIFICATE_PASSWORD="your-p12-password"
bash ./scripts/setup-apple-signing.sh
npm run build:macos:apple
```

---

## Android

配置 `ANDROID_KEYSTORE_*` 四个 Secret 后，CI 自动打签名 `release` APK。见 `.github/omninova-tauri-secrets.example.md`。

---

## iOS

当前 CI 默认产出 **未签名** `OmniNovaPhoneAgent-*-unsigned.ipa`（需自行重签名）。

配置 `IOS_CERTIFICATE` 等 Secret 的完整 iOS 签名流程尚未接入 CI；见 `apps/omninova-ios/README.md`。
