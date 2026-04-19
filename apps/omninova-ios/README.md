# OmniNova Phone Agent（iOS）

SwiftUI 骨架：展示 **对话记录落盘**（JSON）与 **语音转写占位**（Speech）。**不包含**完整 CallKit / PushKit 生产配置（需 Apple 开发者账号与 VoIP 证书）。

## 平台能力说明（重要）

- **无法**通过公开 API 让第三方 App **自动接听运营商蜂窝电话**。
- **VoIP + CallKit** 可实现系统级来电 UI；「自动接听」依赖系统/审核策略，本仓库仅保留扩展点注释。
- 对话内容记录适用于：**App 内语音会话**、或你方 **VoIP 接通后** 在同一 App 内采集的转写文本。

## 在 Xcode 中创建工程

1. Xcode → **File → New → Project** → **App**，Interface：**SwiftUI**，Language：**Swift**。
2. 最低版本建议 **iOS 17**（可按需下调并替换 `@Observable` 等）。
3. 将本目录 `OmniNovaPhoneAgent/` 下所有 `.swift` 文件拖入工程目标 **OmniNovaPhoneAgent**（勾选 *Copy items if needed*、Target membership）。
4. 删除 Xcode 自动生成的 `*App.swift` / `ContentView.swift` 若与仓库文件重复，**保留**本仓库版本或合并 `@main`。
5. 在 **Signing & Capabilities** 中按需添加：**Background Modes**（Audio、Voice over IP 仅当你实现 VoIP 时）。

## Info.plist 隐私键（示例值请改为产品文案）

```xml
<key>NSMicrophoneUsageDescription</key>
<string>用于语音对话与实时转写。</string>
<key>NSSpeechRecognitionUsageDescription</key>
<string>用于将语音转为文字并生成对话记录。</string>
```

## 目录说明

| 路径 | 作用 |
|------|------|
| `OmniNovaPhoneAgentApp.swift` | 入口 |
| `ContentView.swift` | 会话列表、开始/结束、转写展示 |
| `Models/ConversationModels.swift` | 与技能包 `conversation_log_schema.json` 对齐的 Codable 模型 |
| `Services/ConversationLogStore.swift` | 写入 `Documents/conversations/*.json` |
| `Services/SpeechPipeline.swift` | `SFSpeechRecognizer` 请求授权与识别占位 |

## 与 OmniNova 技能包配合

仓库内技能：`skills/phone-call-assistant/`。导入到 Agent 工作区后，模型应答会遵循蜂窝/Voice 边界说明与记录 schema。

## 后续可扩展

- 接入贵司 **VoIP 信令** + CallKit `CXProvider` 报告来电。
- 将 `ConversationSession` 经 HTTPS 同步到自建 API，再由 `omninova` 网关消费。
