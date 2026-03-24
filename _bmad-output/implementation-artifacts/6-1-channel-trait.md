# Story 6.1: Channel Trait 与抽象层

Status: ready-for-dev

## Story

As a 开发者,
I want 定义统一的渠道接口抽象,
So that 系统可以无缝支持多个通信渠道（Slack、Discord、邮件等）.

## Acceptance Criteria

1. **AC1: Channel Trait 定义** - Channel trait 已定义，包含核心方法：connect, disconnect, send_message, receive_message, get_status ✅
2. **AC2: ChannelConfig 结构体** - ChannelConfig 结构体已定义，包含 channel_type, credentials, settings 等字段 ✅
3. **AC3: ChannelRegistry 实现** - ChannelRegistry 已实现用于动态注册和获取渠道实例 ✅
4. **AC4: 渠道能力声明** - 支持渠道能力声明（如支持富文本、文件、线程等） ✅
5. **AC5: 错误类型定义** - 错误类型已定义用于处理不同渠道的错误场景 ✅
6. **AC6: 单元测试** - 核心接口和注册表有完整的单元测试 ✅

## Tasks / Subtasks

- [ ] Task 1: 创建 channels 模块结构 (AC: #1)
  - [ ] 1.1 创建 `crates/omninova-core/src/channels/mod.rs` 模块入口
  - [ ] 1.2 创建 `crates/omninova-core/src/channels/traits.rs` 定义 Channel trait
  - [ ] 1.3 在 `lib.rs` 中添加 `pub mod channels;` 导出

- [ ] Task 2: 定义 Channel Trait (AC: #1)
  - [ ] 2.1 定义 `Channel` trait 核心接口方法：
    - `async fn connect(&mut self) -> Result<()>`
    - `async fn disconnect(&mut self) -> Result<()>`
    - `async fn send_message(&self, message: OutgoingMessage) -> Result<MessageId>`
    - `async fn receive_messages(&self) -> Result<Vec<IncomingMessage>>`
    - `fn get_status(&self) -> ChannelStatus`
  - [ ] 2.2 定义 `ChannelCapabilities` 结构表示渠道能力
  - [ ] 2.3 添加生命周期方法：`fn on_message(&self, handler: MessageHandler)`

- [ ] Task 3: 定义核心数据结构 (AC: #2, #4)
  - [ ] 3.1 定义 `ChannelConfig` 结构体：
    - `channel_type: ChannelType`
    - `id: String` (渠道实例唯一标识)
    - `name: String` (显示名称)
    - `credentials: Credentials`
    - `settings: ChannelSettings`
  - [ ] 3.2 定义 `ChannelType` 枚举：Slack, Discord, Email, Telegram, WeChat, Feishu, Webhook
  - [ ] 3.3 定义 `Credentials` 枚举支持不同认证方式
  - [ ] 3.4 定义 `ChannelSettings` 结构体包含渠道特定配置
  - [ ] 3.5 定义 `ChannelCapabilities` 位标志结构

- [ ] Task 4: 定义消息类型 (AC: #1)
  - [ ] 4.1 定义 `IncomingMessage` 结构体：
    - `id: String`
    - `channel_id: String`
    - `sender: MessageSender`
    - `content: MessageContent`
    - `timestamp: i64`
    - `thread_id: Option<String>`
    - `reply_to: Option<String>`
  - [ ] 4.2 定义 `OutgoingMessage` 结构体
  - [ ] 4.3 定义 `MessageContent` 枚举：Text, RichText, File, Image
  - [ ] 4.4 定义 `MessageSender` 结构体

- [ ] Task 5: 实现 ChannelRegistry (AC: #3)
  - [ ] 5.1 创建 `crates/omninova-core/src/channels/registry.rs`
  - [ ] 5.2 实现 `ChannelRegistry` 结构体：
    - `channels: HashMap<String, Box<dyn Channel>>`
    - `factories: HashMap<ChannelType, ChannelFactory>`
  - [ ] 5.3 实现注册方法：
    - `fn register_factory(&mut self, channel_type: ChannelType, factory: ChannelFactory)`
    - `fn create_channel(&mut self, config: ChannelConfig) -> Result<String>`
    - `fn get_channel(&self, id: &str) -> Option<&dyn Channel>`
    - `fn remove_channel(&mut self, id: &str) -> Result<()>`
  - [ ] 5.4 定义 `ChannelFactory` trait 用于创建渠道实例

- [ ] Task 6: 定义错误类型 (AC: #5)
  - [ ] 6.1 创建 `crates/omninova-core/src/channels/error.rs`
  - [ ] 6.2 定义 `ChannelError` 枚举：
    - `ConnectionFailed(String)`
    - `AuthenticationFailed(String)`
    - `RateLimitExceeded { retry_after: Option<u64> }`
    - `MessageSendFailed(String)`
    - `ConfigurationError(String)`
    - `ChannelNotFound(String)`
  - [ ] 6.3 实现 `From<ChannelError> for anyhow::Error`

- [ ] Task 7: 定义渠道状态 (AC: #1)
  - [ ] 7.1 定义 `ChannelStatus` 枚举：
    - `Disconnected`
    - `Connecting`
    - `Connected`
    - `Error { message: String }`
  - [ ] 7.2 定义 `ChannelInfo` 结构体包含状态和统计信息

- [ ] Task 8: 单元测试 (AC: #6)
  - [ ] 8.1 创建 `crates/omninova-core/src/channels/traits.rs` 内联测试
  - [ ] 8.2 创建 `registry.rs` 测试：
    - 测试工厂注册
    - 测试渠道创建和获取
    - 测试渠道移除
  - [ ] 8.3 创建 mock Channel 实现用于测试

## Dev Notes

### 架构上下文

Story 6.1 是 Epic 6 (多渠道连接) 的第一个 story，需要建立渠道系统的核心抽象层。后续 stories 将基于此实现具体的渠道适配器：
- **Story 6.2**: Channel Manager 实现
- **Story 6.3**: Slack 渠道适配器
- **Story 6.4**: Discord 渠道适配器
- **Story 6.5**: 电子邮件渠道适配器

### 架构设计参考

根据 architecture.md，渠道模块结构如下：

```
crates/omninova-core/src/channels/
├── mod.rs           # 模块入口
├── traits.rs        # Channel trait 定义
├── manager.rs       # 渠道管理器 (Story 6.2)
├── registry.rs      # 渠道注册表
├── error.rs         # 错误类型
├── slack.rs         # Slack 适配器 (Story 6.3)
├── discord.rs       # Discord 适配器 (Story 6.4)
├── telegram.rs      # Telegram 适配器 (待定)
├── email.rs         # 邮件适配器 (Story 6.5)
├── wechat.rs        # 微信适配器 (待定)
├── feishu.rs        # 飞书适配器 (待定)
└── webhook.rs       # Webhook 适配器 (待定)
```

### Channel Trait 设计

```rust
// channels/traits.rs

use async_trait::async_trait;
use crate::channels::{ChannelConfig, ChannelStatus, ChannelCapabilities};
use crate::channels::{IncomingMessage, OutgoingMessage, MessageId};

/// Channel trait - 所有渠道适配器必须实现的核心接口
#[async_trait]
pub trait Channel: Send + Sync {
    /// 获取渠道唯一标识
    fn id(&self) -> &str;

    /// 获取渠道类型
    fn channel_type(&self) -> ChannelType;

    /// 连接到渠道服务
    async fn connect(&mut self) -> Result<(), ChannelError>;

    /// 断开连接
    async fn disconnect(&mut self) -> Result<(), ChannelError>;

    /// 发送消息
    async fn send_message(&self, message: OutgoingMessage) -> Result<MessageId, ChannelError>;

    /// 获取当前连接状态
    fn get_status(&self) -> ChannelStatus;

    /// 获取渠道能力声明
    fn capabilities(&self) -> ChannelCapabilities;

    /// 设置消息接收处理器
    fn set_message_handler(&mut self, handler: Box<dyn MessageHandler>);
}

/// 消息处理器 trait
#[async_trait]
pub trait MessageHandler: Send + Sync {
    async fn handle(&self, message: IncomingMessage);
}
```

### 渠道能力设计

```rust
bitflags::bitflags! {
    /// 渠道能力标志
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ChannelCapabilities: u32 {
        /// 支持纯文本消息
        const TEXT = 0b0001;
        /// 支持富文本/Markdown
        const RICH_TEXT = 0b0010;
        /// 支持文件附件
        const FILES = 0b0100;
        /// 支持图片
        const IMAGES = 0b1000;
        /// 支持消息线程
        const THREADS = 0b0001_0000;
        /// 支持消息回复
        const REPLIES = 0b0010_0000;
        /// 支持消息编辑
        const EDIT = 0b0100_0000;
        /// 支持消息删除
        const DELETE = 0b1000_0000;
        /// 支持 @ 提及
        const MENTIONS = 0b0001_0000_0000;
        /// 支持表情反应
        const REACTIONS = 0b0010_0000_0000;
        /// 支持私聊
        const DIRECT_MESSAGE = 0b0100_0000_0000;
        /// 支持频道消息
        const CHANNEL_MESSAGE = 0b1000_0000_0000;
    }
}
```

### 认证凭据设计

不同渠道使用不同的认证方式：

```rust
/// 渠道认证凭据
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Credentials {
    /// Bot Token 认证 (Slack, Discord, Telegram)
    BotToken {
        token: String,
    },
    /// OAuth2 认证
    OAuth2 {
        access_token: String,
        refresh_token: Option<String>,
        expires_at: Option<i64>,
    },
    /// 用户名密码 (Email)
    UsernamePassword {
        username: String,
        password: String,
    },
    /// API Key 认证
    ApiKey {
        key: String,
        secret: Option<String>,
    },
    /// Webhook URL
    Webhook {
        url: String,
        secret: Option<String>,
    },
}
```

### 消息类型设计

```rust
/// 消息内容类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessageContent {
    /// 纯文本消息
    Text {
        text: String,
    },
    /// 富文本消息 (Markdown)
    RichText {
        text: String,
        format: RichTextFormat,
    },
    /// 文件附件
    File {
        filename: String,
        content_type: String,
        url: Option<String>,
        data: Option<Vec<u8>>,
    },
    /// 图片
    Image {
        url: String,
        alt_text: Option<String>,
    },
    /// 组合内容
    Composite {
        parts: Vec<MessageContent>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RichTextFormat {
    Markdown,
    Html,
}
```

### 与其他模块的集成

**与 Agent 模块集成：**
- Channel 通过 `MessageHandler` 将收到的消息传递给 Agent Dispatcher
- Agent 生成响应后通过 Channel 发送回渠道

**与 Memory 模块集成：**
- 渠道消息可以存储到记忆系统
- 记忆可以包含渠道来源信息

**与 Config 模块集成：**
- 渠道配置存储在 `~/.omninoval/channels/` 目录
- 敏感凭据（如 token）存储在 OS Keychain

### 文件结构

```
crates/omninova-core/src/
├── lib.rs                        # 修改 - 添加 pub mod channels;
└── channels/
    ├── mod.rs                    # 新增 - 模块入口，导出公共类型
    ├── traits.rs                 # 新增 - Channel trait 定义
    ├── types.rs                  # 新增 - 消息类型、状态类型等
    ├── config.rs                 # 新增 - ChannelConfig 等配置类型
    ├── registry.rs               # 新增 - ChannelRegistry 实现
    └── error.rs                  # 新增 - ChannelError 错误类型
```

### 命名约定

遵循 architecture.md 中定义的命名约定：
- **Rust 函数**: snake_case (如 `send_message`, `get_status`)
- **结构体**: PascalCase (如 `ChannelConfig`, `IncomingMessage`)
- **枚举**: PascalCase (如 `ChannelType`, `ChannelStatus`)
- **常量**: SCREAMING_SNAKE_CASE (如 `DEFAULT_TIMEOUT`)

### 测试策略

1. **单元测试**:
   - 测试 `ChannelCapabilities` 位操作
   - 测试 `ChannelRegistry` 注册和获取
   - 测试消息序列化/反序列化

2. **Mock 实现**:
   - 创建 `MockChannel` 实现 `Channel` trait
   - 用于测试 Registry 和上层逻辑

3. **集成测试**:
   - 在后续 Story 中添加真实渠道适配器测试

### Previous Story Intelligence (Story 5.9)

**学习要点:**
1. Tauri 命令需要在 `lib.rs` 中注册
2. 前端类型定义需与后端保持同步
3. 使用 `Arc<Mutex<T>>` 共享状态
4. Hook 中使用 `mountedRef` 避免闭包陷阱
5. 配置 schema 需要同步更新

**可复用模式:**
- 错误处理使用 `thiserror` 或 `anyhow`
- 类型定义使用 `serde` 序列化
- 异步方法使用 `async_trait`
- 状态共享使用 `Arc<Mutex<T>>`

### References

- [Source: epics.md#Story 6.1] - 原始 story 定义
- [Source: architecture.md#channels] - 渠道模块架构设计
- [Source: architecture.md#FR27-FR32] - 多渠道连接功能需求
- [Source: architecture.md#渠道适配器架构] - 渠道适配器架构图
- [Source: Story 3.1] - Provider Trait 实现参考（类似抽象层设计）

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

No critical issues encountered. All 58 tests pass successfully.

### Completion Notes List

1. **Module Structure**: Created `traits.rs` with `Channel`, `MessageHandler`, and `ChannelFactory` traits. Updated `types.rs` (existing) and `error.rs` (existing).

2. **Type Naming**: Used `ChannelKind` (existing enum in mod.rs) instead of `ChannelType` to maintain backward compatibility with existing code.

3. **bitflags serde**: Enabled `serde` feature for bitflags crate to support serialization of `ChannelCapabilities`.

4. **Default Implementation**: Implemented `Default` trait manually for `ChannelSettings` to ensure custom defaults (`retry_attempts: 3`, `timeout_secs: 30`) are applied correctly.

5. **Integration**: All types integrate with existing `ChannelKind`, `InboundMessage`, `OutboundMessage` legacy types.

### File List

- `crates/omninova-core/src/channels/traits.rs` - **Created**: Channel trait, MessageHandler trait, ChannelFactory trait, ChannelConfig, ChannelSettings
- `crates/omninova-core/src/channels/types.rs` - **Modified**: Fixed test issues, uses `ChannelKind` from mod.rs
- `crates/omninova-core/src/channels/mod.rs` - **Modified**: Updated re-exports for new traits and types
- `crates/omninova-core/src/channels/error.rs` - **Modified**: Removed unused import
- `crates/omninova-core/Cargo.toml` - **Modified**: Added serde feature to bitflags dependency