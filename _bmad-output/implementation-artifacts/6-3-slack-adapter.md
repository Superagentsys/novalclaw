# Story 6.3: Slack 渠道适配器

Status: complete

## Story

As a 用户,
I want 将 AI 代理连接到 Slack 频道,
So that 代理可以在 Slack 中响应团队成员的问题.

## Acceptance Criteria

1. **AC1: Bot Token 认证** - 支持 Slack Bot Token 认证方式 ✅
2. **AC2: 消息接收** - 支持接收频道消息和直接消息 (DM) ✅
3. **AC3: 消息发送** - 支持发送消息到指定频道或用户 ✅
4. **AC4: 事件订阅** - 支持 Slack 事件订阅 (Events API) ✅
5. **AC5: 线程回复** - 处理消息线程回复 ✅
6. **AC6: 频道过滤** - 配置可以指定代理监听的频道列表 ✅
7. **AC7: Channel trait 实现** - 完整实现 Channel trait 的所有方法 ✅
8. **AC8: ChannelFactory 实现** - 实现 ChannelFactory 用于创建 Slack 适配器实例 ✅
9. **AC9: 单元测试** - 核心功能有完整的单元测试覆盖 ✅

## Tasks / Subtasks

- [x] Task 1: 创建 Slack 适配器模块结构 (AC: #7)
  - [x] 1.1 创建 `crates/omninova-core/src/channels/adapters/mod.rs` 更新导出
  - [x] 1.2 创建 `crates/omninova-core/src/channels/adapters/slack.rs` 文件
  - [x] 1.3 在 `channels/mod.rs` 中导出 slack 模块

- [x] Task 2: 定义 Slack 配置结构体 (AC: #1, #6)
  - [x] 2.1 定义 `SlackConfig` 结构体：
    - `bot_token: String` - Slack Bot Token
    - `app_token: Option<String>` - App-level Token (for Socket Mode)
    - `signing_secret: Option<String>` - 签名密钥
    - `enabled_channels: Vec<String>` - 监听的频道列表
    - `socket_mode: bool` - 是否使用 Socket Mode
  - [x] 2.2 实现 `SlackConfig` 的序列化/反序列化
  - [x] 2.3 实现 `SlackConfig` 的验证方法

- [x] Task 3: 实现 SlackChannel 结构体 (AC: #7)
  - [x] 3.1 定义 `SlackChannel` 结构体：
    - `id: String` - 渠道实例ID
    - `config: SlackConfig` - Slack 配置
    - `status: ChannelStatus` - 连接状态
    - `api: Option<SlackApi>` - Slack API 客户端
    - `message_handler: Option<Box<dyn MessageHandler>>` - 消息处理器
  - [x] 3.2 实现构造函数 `new(config: SlackConfig) -> Self`
  - [x] 3.3 实现能力声明方法，返回 Slack 支持的能力

- [x] Task 4: 实现 Channel trait (AC: #7)
  - [x] 4.1 实现 `fn id(&self) -> &str`
  - [x] 4.2 实现 `fn channel_kind(&self) -> ChannelKind` 返回 `ChannelKind::Slack`
  - [x] 4.3 实现 `async fn connect(&mut self) -> Result<(), ChannelError>`
    - 验证 Bot Token ✅
    - 建立 WebSocket 连接 (Socket Mode) ✅
    - 更新状态为 Connected ✅
  - [x] 4.4 实现 `async fn disconnect(&mut self) -> Result<(), ChannelError>`
    - 关闭 WebSocket 连接 ✅
    - 更新状态为 Disconnected ✅
  - [x] 4.5 实现 `async fn send_message(&self, message: OutgoingMessage) -> Result<MessageId, ChannelError>`
    - 支持 text 格式 ✅
    - 支持线程回复 (thread_ts) ✅
    - 返回 Slack message ts 作为 MessageId ✅
  - [x] 4.6 实现 `fn get_status(&self) -> ChannelStatus`
  - [x] 4.7 实现 `fn capabilities(&self) -> ChannelCapabilities`
    - TEXT, RICH_TEXT, THREADS, MENTIONS, REACTIONS, FILES, IMAGES, CHANNEL_MESSAGE, DIRECT_MESSAGE ✅
  - [x] 4.8 实现 `fn set_message_handler(&mut self, handler: Box<dyn MessageHandler>)`

- [x] Task 5: 实现消息接收与转换 (AC: #2, #5)
  - [x] 5.1 实现 Slack 事件解析
    - `message` 事件 - 频道消息 (SlackMessage 结构体)
    - `message.im` 事件 - 直接消息 (通过 channel_type 区分)
  - [x] 5.2 实现 Slack 消息到 `IncomingMessage` 转换：
    - `user` -> `MessageSender` ✅
    - `text` / `blocks` -> `MessageContent` ✅
    - `thread_ts` -> `thread_id` ✅
    - `ts` -> `timestamp` ✅
  - [x] 5.3 实现频道过滤逻辑
    - 检查消息是否来自 `enabled_channels` ✅
    - 如果列表为空，接收所有消息 ✅
  - [x] 5.4 实现消息处理器调用
    - 解析事件后调用 `message_handler.handle(message)` ✅

- [x] Task 6: 实现 SlackApi 客户端封装 (AC: #1, #3)
  - [x] 6.1 创建 `SlackApi` 结构体封装 HTTP API 调用
  - [x] 6.2 实现 `auth_test` 方法验证 Token
  - [x] 6.3 实现 `post_message` 方法发送消息
  - [x] 6.4 实现 `conversations_list` 方法获取频道列表
  - [x] 6.5 实现 `users_info` 方法获取用户信息
  - [x] 6.6 实现错误处理和重试逻辑
  - [x] 6.7 实现速率限制处理 (Slack rate limits)

- [x] Task 7: 实现 Socket Mode 支持 (AC: #4)
  - [x] 7.1 创建 `SlackSocketMode` 结构体处理 WebSocket 连接
  - [x] 7.2 实现WebSocket 连接建立
  - [x] 7.3 实现消息帧解析
  - [x] 7.4 实现连接保活 (ping/pong)
  - [x] 7.5 实现重连逻辑 (通过 state 状态管理)

- [x] Task 8: 实现 SlackChannelFactory (AC: #8)
  - [x] 8.1 创建 `SlackChannelFactory` 结构体
  - [x] 8.2 实现 `ChannelFactory` trait:
    - `fn channel_kind(&self) -> ChannelKind` 返回 `ChannelKind::Slack` ✅
    - `fn create(&self, config: ChannelConfig) -> Result<Box<dyn Channel>, ChannelError>` ✅
  - [x] 8.3 从 `ChannelConfig` 中提取 Slack 配置

- [x] Task 9: 单元测试 (AC: #9)
  - [x] 9.1 测试 `SlackConfig` 创建和验证
  - [x] 9.2 测试消息转换 (Slack message -> IncomingMessage)
  - [x] 9.3 测试频道过滤逻辑
  - [x] 9.4 测试 `SlackChannel` 连接/断开状态
  - [x] 9.5 测试 `SlackChannelFactory` 创建渠道
  - [x] 9.6 测试 `Channel` trait 实现

## Dev Notes

### 架构上下文

Story 6.3 基于 Story 6.1 和 Story 6.2 完成的基础设施，实现 Slack 渠道适配器。

**依赖关系：**
- **Story 6.1 (已完成)**: 提供了 `Channel` trait, `ChannelFactory` trait, `ChannelConfig`, `ChannelError`, `ChannelStatus`, `ChannelCapabilities` 等核心类型
- **Story 6.2 (已完成)**: 提供了 `ChannelManager` 用于注册工厂和管理渠道实例

**后续 Stories：**
- Story 6.4: Discord 渠道适配器 - 将参考 Slack 适配器的实现模式
- Story 6.5: Email 渠道适配器 - 将参考 Slack 适配器的实现模式

### Slack API 概述

**认证方式：**
1. **Bot Token** (`xoxb-`) - 用于 API 调用，代表 Bot 用户
2. **App-Level Token** (`xapp-`) - 用于 Socket Mode
3. **Signing Secret** - 用于验证 Webhook 请求

**关键 API 端点：**
- `POST /api/auth.test` - 验证 Token 有效性
- `POST /api/chat.postMessage` - 发送消息
- `POST /api/conversations.list` - 获取频道列表
- `GET /api/users.info` - 获取用户信息

**事件类型：**
- `message` - 频道消息
- `message.im` - 直接消息
- `app_mention` - @ 提及

### 文件结构

```
crates/omninova-core/src/channels/
├── mod.rs           # 已存在 - 更新导出
├── traits.rs        # 已存在 - Channel trait
├── types.rs         # 已存在 - 消息类型
├── error.rs         # 已存在 - 错误类型
├── manager.rs       # 已存在 - ChannelManager
├── event.rs         # 已存在 - 事件类型
└── adapters/        # 新增目录
    ├── mod.rs       # 新增 - 适配器模块入口
    └── slack.rs     # 新增 - Slack 适配器实现
```

### Slack 消息格式

**接收消息示例：**
```json
{
  "type": "message",
  "channel": "C0123456789",
  "user": "U0123456789",
  "text": "Hello bot!",
  "ts": "1234567890.123456",
  "thread_ts": "1234567890.000000",
  "channel_type": "channel"
}
```

**发送消息示例：**
```json
{
  "channel": "C0123456789",
  "text": "Hello!",
  "thread_ts": "1234567890.123456",
  "blocks": [...]
}
```

### 错误处理

Slack API 错误码：
- `invalid_auth` - Token 无效
- `channel_not_found` - 频道不存在
- `not_in_channel` - Bot 未加入频道
- `rate_limited` - 速率限制

映射到 `ChannelError`：
- `invalid_auth` -> `ChannelError::AuthenticationFailed`
- `rate_limited` -> `ChannelError::RateLimitExceeded`
- 其他 -> `ChannelError::ConnectionFailed` 或 `ChannelError::MessageSendFailed`

### Socket Mode vs Webhook

**推荐使用 Socket Mode：**
- 无需公网 IP 和 HTTPS 证书
- 通过 WebSocket 连接 Slack 服务器
- 更适合桌面应用场景

**WebSocket 连接流程：**
1. 使用 App-Level Token 调用 `apps.connections.open`
2. 获取 WebSocket URL
3. 建立 WebSocket 连接
4. 接收并处理消息帧

### 依赖库选择

**推荐依赖：**
```toml
[dependencies]
# HTTP 客户端
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }

# WebSocket (for Socket Mode)
tokio-tungstenite = { version = "0.21", features = ["rustls-tls-webpki-roots"] }

# JSON 处理
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# URL 处理
url = "2.5"
```

**注意：** 不要使用 `slack-api` 或 `slack-rust` 等第三方库，因为：
1. 可能不兼容最新 API
2. 引入额外依赖
3. 自己封装更灵活可控

### 测试策略

1. **单元测试：**
   - 使用 mock HTTP 响应测试 API 调用
   - 测试消息转换逻辑
   - 测试频道过滤

2. **集成测试：**
   - 使用环境变量中的真实 Token 测试 (可选)
   - 测试完整的连接/发送/接收流程

3. **Mock 策略：**
   - 创建 `MockSlackApi` trait 用于测试
   - 使用 `mockito` 或类似库 mock HTTP 响应

### Previous Story Intelligence (Story 6.2)

**学习要点：**
1. `ChannelKind` 使用已存在的枚举，已添加 `Hash` derive
2. 使用 `Arc<dyn ChannelFactory>` 注册工厂
3. `ChannelCapabilities` 使用 bitflags，支持位运算组合
4. 所有异步方法使用 `async_trait`
5. 错误类型使用 `thiserror`

**可复用模式：**
- Mock Channel 实现用于测试 (在 traits.rs 中)
- ChannelFactory 模式创建渠道实例
- 事件广播使用 `tokio::sync::broadcast`

### 命名约定

遵循项目 Rust 命名约定：
- **函数**: snake_case (`send_message`, `get_status`)
- **结构体**: PascalCase (`SlackChannel`, `SlackConfig`)
- **枚举**: PascalCase (`SlackEventType`)
- **常量**: SCREAMING_SNAKE_CASE (`SLACK_API_URL`)

### References

- [Source: epics.md#Story 6.3] - 原始 story 定义
- [Source: architecture.md#channels] - 渠道模块架构设计
- [Source: architecture.md#FR27-FR32] - 多渠道连接功能需求
- [Source: Slack API Docs] - https://api.slack.com/apis
- [Source: Story 6.1] - Channel trait 和基础类型实现
- [Source: Story 6.2] - ChannelManager 实现

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

No critical issues encountered. All 123 channel tests pass (20 specific to Slack adapter).

### Completion Notes List

1. **Module Structure**: Created `slack.rs` with comprehensive Slack adapter implementation. Updated `adapters/mod.rs` to export the slack module and re-export main types.

2. **SlackConfig**: Implemented configuration struct with builder pattern. Includes validation for bot_token format (must start with `xoxb-`) and app_token format for Socket Mode (must start with `xapp-`).

3. **SlackApi**: HTTP client wrapper for Slack Web API with:
   - `auth_test()` - Token validation
   - `post_message()` - Send messages with thread support
   - `conversations_list()` - List channels with pagination
   - `users_info()` - Get user information
   - Rate limit handling (429 response)
   - Error mapping to ChannelError

4. **SlackChannel**: Channel trait implementation with:
   - HTTP-based authentication via auth.test
   - Message sending via chat.postMessage
   - Thread reply support (thread_ts)
   - Channel filtering
   - Full capabilities declaration

5. **SlackChannelFactory**: Factory implementation for creating Slack channel instances from ChannelConfig.

6. **Message Conversion**: `convert_message()` method transforms Slack messages to IncomingMessage with:
   - Bot message filtering
   - Channel filtering
   - Thread support
   - Timestamp parsing
   - Metadata preservation

7. **Socket Mode**: Implemented with `tokio-tungstenite` for real-time event handling:
   - `SlackSocketMode` struct with connect/disconnect methods
   - WebSocket connection via `apps.connections.open` API
   - Message frame parsing (hello, events_api, disconnect)
   - Automatic ping/pong keepalive handling
   - Message handler invocation for incoming Slack events
   - Channel filtering in Socket Mode reader
   - Graceful disconnect with stop signal

### File List

- `crates/omninova-core/src/channels/adapters/slack.rs` - **Created/Updated**: Complete Slack adapter implementation with Socket Mode
- `crates/omninova-core/src/channels/adapters/mod.rs` - **Modified**: Added slack module export and re-exports
- `crates/omninova-core/Cargo.toml` - **Modified**: Added `tokio-tungstenite` dependency
- `Cargo.toml` - **Modified**: Added workspace `tokio-tungstenite` dependency

### Remaining Work

None - Story 6.3 is complete. All acceptance criteria have been met.