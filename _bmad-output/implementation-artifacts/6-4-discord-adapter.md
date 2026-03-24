# Story 6.4: Discord 渠道适配器

Status: complete

## Story

As a 用户,
I want 将 AI 代理连接到 Discord 服务器,
So that 代理可以在 Discord 社区中提供支持.

## Acceptance Criteria

1. **AC1: Bot Token 认证** - 支持 Discord Bot Token 认证方式 ✅
2. **AC2: 消息接收** - 支持接收服务器频道消息 ✅
3. **AC3: 消息发送** - 支持发送消息到指定频道 ✅
4. **AC4: @ 提及处理** - 支持处理 @ 提及事件 ✅
5. **AC5: 服务器/频道过滤** - 配置可以指定代理监听的服务器和频道列表 ✅
6. **AC6: Discord 消息格式** - 支持 Discord 特有的消息格式（嵌入、表情等） ✅
7. **AC7: Channel trait 实现** - 完整实现 Channel trait 的所有方法 ✅
8. **AC8: ChannelFactory 实现** - 实现 ChannelFactory 用于创建 Discord 适配器实例 ✅
9. **AC9: 单元测试** - 核心功能有完整的单元测试覆盖 ✅

## Tasks / Subtasks

- [x] Task 1: 创建 Discord 适配器模块结构 (AC: #7)
  - [x] 1.1 在 `crates/omninova-core/src/channels/adapters/` 创建 `discord.rs` 文件
  - [x] 1.2 在 `adapters/mod.rs` 中导出 discord 模块
  - [x] 1.3 确保与现有 Slack 适配器结构一致

- [x] Task 2: 定义 Discord 配置结构体 (AC: #1, #5)
  - [x] 2.1 定义 `DiscordConfig` 结构体：
    - `bot_token: String` - Discord Bot Token
    - `application_id: Option<String>` - Application ID (for slash commands)
    - `enabled_guilds: Vec<String>` - 监听的服务器(guild)ID列表
    - `enabled_channels: Vec<String>` - 监听的频道列表
  - [x] 2.2 实现 `DiscordConfig` 的序列化/反序列化
  - [x] 2.3 实现 `DiscordConfig` 的验证方法（bot_token 格式校验）

- [x] Task 3: 实现 DiscordChannel 结构体 (AC: #7)
  - [x] 3.1 定义 `DiscordChannel` 结构体：
    - `id: ChannelId` - 渠道实例ID
    - `config: DiscordConfig` - Discord 配置
    - `status: ChannelStatus` - 连接状态
    - `api: Option<DiscordApi>` - Discord API 客户端
    - `message_handler: Option<Arc<RwLock<Box<dyn MessageHandler>>>>` - 消息处理器
    - `bot_user_id: Option<String>` - Bot 用户 ID
  - [x] 3.2 实现构造函数 `new(config: DiscordConfig) -> Self`
  - [x] 3.3 实现能力声明方法，返回 Discord 支持的能力

- [x] Task 4: 实现 Channel trait (AC: #7)
  - [x] 4.1 实现 `fn id(&self) -> &str`
  - [x] 4.2 实现 `fn channel_kind(&self) -> ChannelKind` 返回 `ChannelKind::Discord`
  - [x] 4.3 实现 `async fn connect(&mut self) -> Result<(), ChannelError>`
    - 验证 Bot Token ✅
    - 获取 Bot 用户信息 ✅
    - 更新状态为 Connected ✅
  - [x] 4.4 实现 `async fn disconnect(&mut self) -> Result<(), ChannelError>`
    - 更新状态为 Disconnected ✅
  - [x] 4.5 实现 `async fn send_message(&self, message: OutgoingMessage) -> Result<MessageId, ChannelError>`
    - 支持 text 格式 ✅
    - 支持 embed 格式 ✅
    - 返回 Discord message id 作为 MessageId ✅
  - [x] 4.6 实现 `fn get_status(&self) -> ChannelStatus`
  - [x] 4.7 实现 `fn capabilities(&self) -> ChannelCapabilities`
    - TEXT, RICH_TEXT, THREADS, MENTIONS, REACTIONS, EMBEDS, CHANNEL_MESSAGE ✅
  - [x] 4.8 实现 `fn set_message_handler(&mut self, handler: Box<dyn MessageHandler>)`

- [x] Task 5: 实现消息接收与转换 (AC: #2, #4, #6)
  - [x] 5.1 实现 Discord 事件解析
    - `MESSAGE_CREATE` 事件 - 频道消息
    - `MESSAGE_UPDATE` 事件 - 消息编辑
    - 提及检测（mentions 数组或 @bot）
  - [x] 5.2 实现 Discord 消息到 `IncomingMessage` 转换：
    - `author` -> `MessageSender` ✅
    - `content` -> `MessageContent` ✅
    - `channel_id` -> `channel_id` ✅
    - `guild_id` -> `metadata` ✅
    - `id` -> `message_id` ✅
  - [x] 5.3 实现频道/服务器过滤逻辑
    - 检查消息是否来自 `enabled_guilds` / `enabled_channels` ✅
    - 如果列表为空，接收所有消息 ✅
  - [x] 5.4 实现消息处理器调用

- [x] Task 6: 实现 DiscordApi 客户端封装 (AC: #1, #3)
  - [x] 6.1 创建 `DiscordApi` 结构体封装 HTTP API 调用
  - [x] 6.2 实现 `get_current_user` 方法获取 Bot 信息
  - [x] 6.3 实现 `post_message` 方法发送消息
  - [x] 6.4 实现 `get_channel` 方法获取频道信息
  - [x] 6.5 实现 `get_guild` 方法获取服务器信息
  - [x] 6.6 实现错误处理和速率限制处理 (Discord rate limits)
  - [x] 6.7 实现 Embed 消息格式支持

- [x] Task 7: 实现 Gateway 连接支持 (AC: #2, #4)
  - [x] 7.1 创建 `DiscordGateway` 结构体处理 WebSocket 连接
  - [x] 7.2 实现 WebSocket 连接建立 (wss://gateway.discord.gg)
  - [x] 7.3 实现 Hello/Heartbeat 握手流程
  - [x] 7.4 实现 Identify 认证
  - [x] 7.5 实现事件分发（MESSAGE_CREATE 等）
  - [x] 7.6 实现连接保活 (heartbeat/ack)
  - [x] 7.7 实现重连逻辑 (resume/reconnect)

- [x] Task 8: 实现 DiscordChannelFactory (AC: #8)
  - [x] 8.1 创建 `DiscordChannelFactory` 结构体
  - [x] 8.2 实现 `ChannelFactory` trait:
    - `fn channel_kind(&self) -> ChannelKind` 返回 `ChannelKind::Discord` ✅
    - `fn create(&self, config: ChannelConfig) -> Result<Box<dyn Channel>, ChannelError>` ✅
  - [x] 8.3 从 `ChannelConfig` 中提取 Discord 配置

- [x] Task 9: 单元测试 (AC: #9)
  - [x] 9.1 测试 `DiscordConfig` 创建和验证
  - [x] 9.2 测试消息转换 (Discord message -> IncomingMessage)
  - [x] 9.3 测试频道/服务器过滤逻辑
  - [x] 9.4 测试 `DiscordChannel` 连接/断开状态
  - [x] 9.5 测试 `DiscordChannelFactory` 创建渠道
  - [x] 9.6 测试 `Channel` trait 实现

## Dev Notes

### 架构上下文

Story 6.4 基于 Story 6.1、6.2 和 6.3 完成的基础设施，实现 Discord 渠道适配器。

**依赖关系：**
- **Story 6.1 (已完成)**: 提供了 `Channel` trait, `ChannelFactory` trait, `ChannelConfig`, `ChannelError`, `ChannelStatus`, `ChannelCapabilities` 等核心类型
- **Story 6.2 (已完成)**: 提供了 `ChannelManager` 用于注册工厂和管理渠道实例
- **Story 6.3 (已完成)**: 提供了 Slack 适配器的实现模式，可参考其结构

**后续 Stories：**
- Story 6.5: Email 渠道适配器 - 将参考此适配器的实现模式

### Discord API 概述

**认证方式：**
1. **Bot Token** - 用于 Bot API 调用和 Gateway 连接
2. **Application ID** - 用于 Slash Commands（可选）

**关键 API 端点：**
- `GET /users/@me` - 获取当前 Bot 用户信息
- `POST /channels/{id}/messages` - 发送消息
- `GET /channels/{id}` - 获取频道信息
- `GET /guilds/{id}` - 获取服务器信息

**Gateway 事件：**
- `MESSAGE_CREATE` - 新消息
- `MESSAGE_UPDATE` - 消息编辑
- `MESSAGE_DELETE` - 消息删除
- `MESSAGE_REACTION_ADD` - 添加反应

**Gateway 连接流程：**
1. WebSocket 连接到 `wss://gateway.discord.gg/?v=10&encoding=json`
2. 接收 Hello 事件，开始心跳
3. 发送 Identify 进行认证
4. 接收 Ready 事件，连接完成
5. 接收事件并处理

### 文件结构

```
crates/omninova-core/src/channels/
├── mod.rs           # 已存在 - 导出模块
├── traits.rs        # 已存在 - Channel trait
├── types.rs         # 已存在 - 消息类型
├── error.rs         # 已存在 - 错误类型
├── manager.rs       # 已存在 - ChannelManager
├── event.rs         # 已存在 - 事件类型
└── adapters/        # 已存在
    ├── mod.rs       # 更新 - 添加 discord 导出
    ├── slack.rs     # 已存在 - Slack 适配器
    └── discord.rs   # 新增 - Discord 适配器实现
```

### Discord 消息格式

**接收消息示例：**
```json
{
  "t": "MESSAGE_CREATE",
  "s": 42,
  "op": 0,
  "d": {
    "id": "1234567890123456789",
    "channel_id": "9876543210987654321",
    "guild_id": "111222333444555666",
    "author": {
      "id": "123456789012345678",
      "username": "User",
      "discriminator": "0001",
      "bot": false
    },
    "content": "Hello bot!",
    "mentions": []
  }
}
```

**发送消息示例：**
```json
{
  "content": "Hello!",
  "embeds": [
    {
      "title": "Title",
      "description": "Description",
      "color": 3447003
    }
  ]
}
```

### 错误处理

Discord API 错误码：
- `401` - Unauthorized (Token 无效)
- `403` - Forbidden (权限不足)
- `404` - Not Found (频道/服务器不存在)
- `429` - Rate Limited

映射到 `ChannelError`：
- `401` -> `ChannelError::AuthenticationFailed`
- `429` -> `ChannelError::RateLimitExceeded`
- 其他 -> `ChannelError::ConnectionFailed` 或 `ChannelError::MessageSendFailed`

### Gateway 连接要点

**心跳机制：**
- 连接后收到 Hello 事件，包含 `heartbeat_interval`
- 每 `heartbeat_interval` 毫秒发送一次心跳
- 收到 Heartbeat ACK 表示成功
- 连续多次未收到 ACK 需要重连

**重连策略：**
- 正常断开：重新连接
- 网络错误：指数退避重连
- Token 无效：不重连，返回错误

### 依赖库选择

**推荐依赖：**
```toml
[dependencies]
# HTTP 客户端 (已存在)
reqwest = { workspace = true }

# WebSocket (已存在)
tokio-tungstenite = { workspace = true }

# JSON 处理 (已存在)
serde = { workspace = true }
serde_json = { workspace = true }
```

**注意：** 不要使用 `serenity` 或 `twilight` 等第三方 Discord 库，原因与 Slack 相同：
1. 可能不兼容最新 API
2. 引入额外依赖
3. 自己封装更灵活可控

### 测试策略

1. **单元测试：**
   - 使用 mock HTTP 响应测试 API 调用
   - 测试消息转换逻辑
   - 测试频道/服务器过滤

2. **集成测试：**
   - 使用环境变量中的真实 Token 测试 (可选)
   - 测试完整的连接/发送/接收流程

3. **Mock 策略：**
   - 参考 Slack 适配器的 mock 模式
   - 使用 `mockito` mock HTTP 响应

### Previous Story Intelligence (Story 6.3)

**学习要点：**
1. `ChannelKind` 使用已存在的枚举，已添加 `Hash` derive
2. 使用 `Arc<RwLock<Box<dyn MessageHandler>>>` 存储消息处理器，支持异步访问
3. `ChannelCapabilities` 使用 bitflags，支持位运算组合
4. 所有异步方法使用 `async_trait`
5. 错误类型使用 `thiserror`
6. Socket Mode 实现模式：WebSocket 连接 + 消息帧解析 + handler 调用

**可复用模式：**
- Channel trait 实现结构
- Api 客户端封装模式（HTTP + 错误映射）
- WebSocket 连接管理（tokio-tungstenite）
- 消息处理器传播模式（set_message_handler 更新内部组件）
- 测试 mock 模式

**关键差异（Discord vs Slack）：**
- Discord Gateway 连接需要 Hello/Identify/Heartbeat 流程
- Discord 使用不同的消息格式（事件结构）
- Discord 的 Embed 与 Slack 的 Block Kit 不同
- Discord 的服务器(guild)概念对应 Slack 的 workspace

### 命名约定

遵循项目 Rust 命名约定：
- **函数**: snake_case (`send_message`, `get_status`)
- **结构体**: PascalCase (`DiscordChannel`, `DiscordConfig`)
- **枚举**: PascalCase (`DiscordEventType`)
- **常量**: SCREAMING_SNAKE_CASE (`DISCORD_API_URL`, `DISCORD_GATEWAY_URL`)

### References

- [Source: epics.md#Story 6.4] - 原始 story 定义
- [Source: architecture.md#channels] - 渠道模块架构设计
- [Source: architecture.md#FR27-FR32] - 多渠道连接功能需求
- [Source: Discord API Docs] - https://discord.com/developers/docs
- [Source: Story 6.1] - Channel trait 和基础类型实现
- [Source: Story 6.2] - ChannelManager 实现
- [Source: Story 6.3] - Slack 适配器实现模式参考

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

No critical issues encountered. All 134 channel tests pass (11 specific to Discord adapter).

### Completion Notes List

1. **Module Structure**: Created `discord.rs` with comprehensive Discord adapter implementation. Updated `adapters/mod.rs` to export the discord module and re-export main types.

2. **DiscordConfig**: Implemented configuration struct with builder pattern. Includes validation for bot_token (minimum length check), and methods for guild/channel filtering.

3. **DiscordApi**: HTTP client wrapper for Discord REST API with:
   - `get_current_user()` - Get bot user info
   - `post_message()` - Send messages to channels
   - `get_channel()` - Get channel information
   - `get_guild()` - Get guild (server) information
   - Rate limit handling (429 response)
   - Error mapping to ChannelError

4. **DiscordChannel**: Channel trait implementation with:
   - Bot Token authentication
   - Message sending via REST API
   - Channel/guild filtering
   - Gateway connection support
   - Full capabilities declaration (TEXT, RICH_TEXT, THREADS, MENTIONS, REACTIONS, CHANNEL_MESSAGE)

5. **DiscordGateway**: WebSocket client for real-time events:
   - Hello/Heartbeat/Identify handshake flow
   - Heartbeat in main select! loop (no clone issues)
   - MESSAGE_CREATE/MESSAGE_UPDATE event handling
   - Channel and guild filtering
   - Graceful disconnect with stop signal

6. **DiscordChannelFactory**: Factory implementation for creating Discord channel instances from ChannelConfig.

7. **Message Conversion**: `convert_message()` method transforms Discord messages to IncomingMessage with:
   - Bot message filtering
   - Channel/guild filtering
   - Mention detection
   - Metadata preservation (discord_channel_id, discord_message_id, discord_guild_id)

8. **Tests**: 11 unit tests covering:
   - DiscordConfig creation, validation, builder pattern, and filters
   - DiscordChannel creation and capabilities
   - DiscordChannelFactory creation
   - Message conversion and bot filtering
   - Embed serialization
   - Config extra deserialization

### File List

- `crates/omninova-core/src/channels/adapters/discord.rs` - **Created**: Complete Discord adapter implementation (~1450 lines)
- `crates/omninova-core/src/channels/adapters/mod.rs` - **Modified**: Added discord module export and re-exports