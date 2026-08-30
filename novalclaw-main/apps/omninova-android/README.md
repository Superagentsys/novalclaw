# OmniNova Phone Agent（Android）

与 iOS 客户端同构的 Android 工程，核心能力：

- `CallScreeningService` 在振铃前按 `skills/phone-call-assistant/spam_detection_rules.json` 识别骚扰
- `InCallService` 在设备将本 App 设为默认拨号器后自动接听（需 `ANSWER_PHONE_CALLS`）
- 前台服务 `CallAgentForegroundService` 驱动 `SpeechRecognizer` 转写并同步到 OmniNova 网关
- `KeyInfoExtractor` 本地抽取关键信息，通话结束后触发网关侧二次抽取

## 目录

```
apps/omninova-android/
├── settings.gradle.kts
├── build.gradle.kts
├── gradle.properties
├── gradle/wrapper/
│   ├── gradle-wrapper.jar
│   └── gradle-wrapper.properties
├── gradlew / gradlew.bat
└── app/
    ├── build.gradle.kts
    ├── proguard-rules.pro
    └── src/main/
        ├── AndroidManifest.xml
        ├── java/com/omninova/phoneagent/
        │   ├── OmniNovaApp.kt               # Application
        │   ├── ui/MainActivity.kt           # Compose UI
        │   ├── data/                        # 会话模型与落盘
        │   ├── net/GatewayClient.kt         # OkHttp 客户端
        │   └── call/                        # 通话相关服务与工具
        │       ├── OmniCallScreeningService.kt
        │       ├── OmniInCallService.kt
        │       ├── CallAgentForegroundService.kt
        │       ├── SpeechPipeline.kt
        │       ├── AgentResponseSynthesizer.kt
        │       ├── SpamDetector.kt
        │       └── KeyInfoExtractor.kt
        └── res/                              # 主题 / 字符串 / 图标
```

## 依赖与构建环境

- JDK 17（推荐 `Temurin 17`）
- Android SDK Platform 34、Build-Tools 34.0.0
- Android Gradle Plugin 8.3.2、Kotlin 1.9.23、Compose BOM 2024.04

`local.properties` 必须指定 SDK 位置：

```properties
sdk.dir=/Users/you/Library/Android/sdk
# 或导出 ANDROID_SDK_ROOT 环境变量
```

## 本地编译

```bash
cd apps/omninova-android
./gradlew assembleDebug
# 产物：app/build/outputs/apk/debug/app-debug.apk

./gradlew assembleRelease  # 需配置 keystore.properties 与签名
```

## 自动接听 / 骚扰拦截 激活步骤

1. 安装 APK 后打开应用，授予：麦克风、电话、通讯录、通知权限。
2. **设为默认拨号器**：系统 设置 → 应用 → 默认应用 → 电话应用 → 选择 OmniNova。
3. **设为默认通话筛选**：系统 设置 → 应用 → 默认应用 → 通话筛选 App → 选择 OmniNova。
4. 在应用设置中启用 "自动接听" 与 "骚扰识别"。

## 与 OmniNova 网关对接

默认网关地址 `http://127.0.0.1:10809`，可在应用设置中修改：

| 接口 | 用途 |
|------|------|
| `GET /api/health` | 健康检查 |
| `POST /api/inbound` | 单轮对话，body 含 session_id/channel/text |
| `POST /api/webhook` | 通话结束后同步完整会话 JSON |
| `POST /api/skill/phone-call-assistant/extract` | 触发关键信息抽取 |
| `GET /api/skill/phone-call-assistant/rules` | 获取最新骚扰规则 |

## 平台限制

- 自动接听需要用户主动将本 App 设为默认拨号器（Android 政策限制）
- Android 系统 `SpeechRecognizer` 仅识别麦克风流；无法直接获取对方通话音频
- 骚扰识别需要将本 App 设为默认通话筛选应用（`BIND_SCREENING_SERVICE`）
