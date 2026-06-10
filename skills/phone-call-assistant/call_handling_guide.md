# Call Handling Guide（电话处理指南）

本指南为 `phone-call-assistant` 技能在 iOS / Android 客户端的实际执行步骤。实施时请遵循本目录下
`spam_detection_rules.json` 与 `key_info_extraction_schema.json` 约束。

---

## 1. 平台能力矩阵

| 能力 | iOS（OmniNovaPhoneAgent） | Android（OmniNovaPhoneAgent） |
|------|---------------------------|-------------------------------|
| 蜂窝通话自动接听 | 不支持（Apple 公开 API 禁止） | 需 `ANSWER_PHONE_CALLS` + 默认拨号器 / 无障碍 |
| VoIP 自动接听 | CallKit + PushKit `CXAnswerCallAction` | `ConnectionService`/`InCallService#answerCall()` |
| 骚扰电话拦截 | Call Directory Extension（静态名单） | `CallScreeningService#onScreenCall`（动态） |
| 实时转写 | `SFSpeechRecognizer` | `SpeechRecognizer` / 云端 ASR |
| TTS | `AVSpeechSynthesizer` | `TextToSpeech` |
| 后台保活 | VoIP Background Mode | 前台服务（`phoneCall|microphone`）|

---

## 2. 会话状态机

```
        +------------+      ring       +-----------+
        |  IDLE      | --------------> | SCREENING |
        +------------+                 +-----------+
                                        |    |    |
                      spam-block <------+    |    +-> allow
                           |                 |         |
                           v                 v         v
                      +---------+     +-----------+  +---------+
                      | BLOCKED |     | RINGING   |  | LISTEN  |
                      +---------+     +-----------+  +---------+
                                         |              |
                                 auto-answer=true       |
                                         v              v
                                    +---------+   +-----------+
                                    | ACTIVE  |-> | TRANSCRIBE|
                                    +---------+   +-----------+
                                         |              |
                                         +--> END <-----+
                                               |
                                               v
                                          +--------+
                                          | ARCHIVE|
                                          +--------+
```

---

## 3. 执行步骤

### 3.1 来电拦截（骚扰识别）

1. 来电触发 Screening（iOS Call Directory / Android `CallScreeningService`）。
2. 加载 `spam_detection_rules.json`：
   - `number_blocklist[]`：硬黑名单直接拒接
   - `prefix_blocklist[]`：常见骚扰号段前缀匹配（如境外一号通）
   - `pattern_heuristics[]`：正则 + 关键词启发（例如短时内多次陌生来电）
3. 返回匹配决策：`allow | silence | reject`。决策写入 `conversation_log_schema.json` 的 `metadata.screening_decision`。

### 3.2 自动接听（仅 VoIP / Android 默认拨号器）

1. 设置项 `autoAnswer=true` 时，`CXAnswerCallAction`（iOS）或 `Call#answer(videoState)`（Android）立即应答。
2. 启动麦克风捕获（iOS: `AVAudioEngine`；Android: `AudioRecord`）。
3. 播放开场白 TTS。

### 3.3 实时对话回合

循环直到挂断：

1. ASR：将音频流送入语音识别，累计 `partial`，遇到静音/标点触发 `final`。
2. 每个 `final` 文本：
   - `turns.append({ role: "caller", text, confidence })`
   - POST `/api/inbound` 到 OmniNova 网关，携带 `session_id`/`channel`/`text`/`metadata`
   - 取到 `reply` 后 `turns.append({ role: "agent", text: reply })`
   - TTS 播报 reply

### 3.4 关键信息抽取

对当前 session 的全部 caller turns 做抽取：

- 客户端本地轻量抽取（正则 / NER），填入 `extracted_fields`（见 `key_info_extraction_schema.json`）
- 通话结束时向网关 POST `/api/skill/phone-call-assistant/extract`，允许大模型做二次补全与归一化。

### 3.5 归档与同步

1. 本地落盘：`Documents/conversations/<session_id>.json`（iOS）或 `filesDir/conversations/<session_id>.json`（Android）
2. HTTPS 同步：`POST /api/webhook`，body 为完整 JSON（符合 `conversation_log_schema.json`）
3. 失败重试：指数回退，最多 5 次；本地保留 `sync_pending=true` 标记

---

## 4. Agent 行为提示（System Prompt 片段）

```
你是 OmniNova 电话通话助手，正在通过 VoIP 与来电者语音对话。
- 回复精简，单轮 ≤ 80 字，便于 TTS 播报；
- 识别骚扰/销售/诈骗意图时礼貌拒绝并挂断；
- 采集关键信息：姓名/公司/事由/联系方式/期望回电时间；
- 敏感信息（身份证、银行卡）只确认后四位，日志中标注 [REDACTED]。
```

---

## 5. 错误与边界

| 场景 | 处理 |
|------|------|
| ASR 超过 10s 无结果 | 提示 "抱歉没听清，请再说一遍" |
| 网关 5xx / 超时 | 本地回退话术："系统繁忙，稍后我让同事回复您" |
| 通话时长 > 5 min | 主动确认是否继续 |
| 检测到敏感词 | 立即挂断并在 `raw_note` 标记 |
