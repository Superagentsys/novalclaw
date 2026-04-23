---
name: "phone-call-assistant"
description: "电话自动接听、语音对话与记录技能。当 Agent 通过 VoIP/CallKit 或 App 内语音渠道接听来电时启用，自动进行对话并结构化记录全部内容。"
---

# 电话通话助手（Phone Call Assistant）

当用户或系统通过 **VoIP / CallKit** 或 **App 内语音** 渠道向 Agent 发起来电时，Agent 应自动接听、进行实时对话，并将全部对话内容结构化记录。

## 何时启用

- 来电渠道为 `phone_voip`（VoIP/CallKit）或 `in_app_voice`（App 内语音会话）
- 用户询问「帮我接电话」「自动应答来电」「电话对话记录」等场景
- 需要对已记录的通话内容进行总结、提取关键信息、生成会议纪要

## 接听与对话流程

1. **来电检测**：VoIP 推送 → CallKit 报告来电 → 系统展示原生来电 UI
2. **自动接听**：收到 `CXAnswerCallAction` 后立即接通（`autoAnswer` 模式）
3. **实时转写**：启动 `SpeechPipeline`（iOS `SFSpeechRecognizer`），将来电者语音实时转为文字
4. **Agent 应答**：转写文本发送到 OmniNova 网关 `/api/inbound`，获取 Agent 回复
5. **语音播报**：Agent 回复通过 TTS（`AVSpeechSynthesizer`）播放给来电者
6. **循环对话**：重复 3→4→5 直至通话结束
7. **会话归档**：通话结束后将完整对话 JSON 落盘并同步到网关

## 对话记录格式

遵循 `conversation_log_schema.json`（本目录）。每通电话生成一个 JSON 文件：

```json
{
  "schema_version": "1.0",
  "session_id": "uuid",
  "channel": "voip_callkit",
  "started_at_utc": "ISO8601",
  "ended_at_utc": "ISO8601",
  "locale": "zh-CN",
  "turns": [
    { "t": "ISO8601", "role": "caller", "text": "你好", "confidence": 0.95 },
    { "t": "ISO8601", "role": "agent", "text": "您好，我是 OmniNova 智能助手，请问有什么可以帮您？", "confidence": 1.0 }
  ],
  "metadata": { "app": "OmniNovaPhoneAgent", "source": "omninova-ios" }
}
```

## Agent 应答准则

1. **开场白**：接听后先自报身份——「您好，我是 [Agent 名称]，请问有什么可以帮您？」
2. **语气**：专业、友好、简洁；中文为主，必要时切换英文
3. **信息采集**：主动确认关键信息（姓名、事由、联系方式），不遗漏
4. **超出能力范围**：诚实说明「这个问题我需要转给人工处理」，并记录到 metadata
5. **隐私**：不主动索取身份证号、银行卡号等敏感信息；若来电者主动提供，记录中标注 `[REDACTED]`
6. **通话时长**：单次应答不宜超过 3 分钟，超时主动确认是否继续
7. **结束**：通话结束前复述关键信息并确认

## 文件引用（本仓库）

- 记录 Schema：`skills/phone-call-assistant/conversation_log_schema.json`
- 接听指南：`skills/phone-call-assistant/call_handling_guide.md`
- 骚扰识别规则：`skills/phone-call-assistant/spam_detection_rules.json`
- 关键信息 Schema：`skills/phone-call-assistant/key_info_extraction_schema.json`
- iOS 客户端：`apps/omninova-ios/OmniNovaPhoneAgent/`
- Android 客户端：`apps/omninova-android/app/src/main/java/com/omninova/phoneagent/`

## 平台限制（重要）

- **iOS**
  - 无法通过公开 API 自动接听运营商蜂窝电话
  - 「自动接听」仅限 **VoIP + CallKit** 来电（需 Apple 开发者账号 + VoIP 证书）
  - 骚扰识别只能走 **Call Directory Extension**（静态名单，不可动态拒接）
- **Android**
  - API 24+ 支持 `CallScreeningService`，可在振铃前决定 allow / silence / reject
  - API 26+ 支持 `ANSWER_PHONE_CALLS`，作为默认拨号器时可 `Call#answer()` 自动接听
  - 需要用户手动将 App 设为「默认电话/拨号」应用
- **App 内模拟通话** 无上述限制，可用于测试与演示

## Agent 可调用的工具（Gateway 侧）

技能导入 Agent 后，可通过网关工具链调用以下能力：

| 工具名 | 说明 |
|--------|------|
| `phone_call_assistant.list_sessions` | 列出最近通话会话 |
| `phone_call_assistant.get_transcript` | 获取指定 session 完整转写 |
| `phone_call_assistant.extract_key_info` | 对会话执行关键信息抽取（遵循 `key_info_extraction_schema.json`）|
| `phone_call_assistant.update_spam_rules` | 向移动客户端推送最新骚扰规则 |
