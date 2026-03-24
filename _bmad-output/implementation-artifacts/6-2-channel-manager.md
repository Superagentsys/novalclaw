# Story 6.2: Channel Manager 实现

Status: done

## Story

As a 系统,
I want 有一个中央管理器协调所有渠道连接,
So that 消息可以正确路由到 AI 代理.

## Acceptance Criteria

1. **AC1: 多渠道实例管理** - Manager 可以管理多个渠道实例，支持动态添加/移除
2. **AC2: 连接状态跟踪** - 跟踪每个渠道的连接状态（Disconnected, Connecting, Connected, Error）
3. **AC3: 消息路由 - 入站** - 将传入消息路由到正确的 AI 代理（通过 MessageHandler）
4. **AC4: 消息路由 - 出站** - 将代理响应发送回正确的渠道
5. **AC5: 断线重连** - 处理渠道连接断开和自动重连机制
6. **AC6: 生命周期事件** - 提供渠道生命周期事件通知（连接、断开、错误等）
7. **AC7: 工厂注册** - 支持注册 ChannelFactory 用于创建渠道实例
8. **AC8: 单元测试** - 完整的单元测试覆盖核心功能

## Tasks / Subtasks

- [x] Task 1: 创建 ChannelManager 结构体 (AC: #1, #2)
  - [x] 1.1 创建 `crates/omninova-core/src/channels/manager.rs`
  - [x] 1.2 定义 `ChannelManager` 结构体：
    - `channels: HashMap<ChannelId, ChannelEntry>` - 渠道实例映射
    - `factories: HashMap<ChannelKind, Arc<dyn ChannelFactory>>` - 工厂注册表
    - `default_handler: Option<Arc<dyn MessageHandler>>` - 默认消息处理器
    - `event_tx: broadcast::Sender<ChannelEvent>` - 事件广播通道
  - [x] 1.3 定义 `ChannelEntry` 结构体：
    - `channel: Box<dyn Channel>` - 渠道实例
    - `config: ChannelConfig` - 渠道配置
    - `agent_binding: Option<AgentId>` - 绑定的代理ID
  - [x] 1.4 实现 `new()` 和 `default()` 方法

- [x] Task 2: 实现渠道注册与生命周期 (AC: #1, #7)
  - [x] 2.1 实现 `register_factory(&mut self, factory: Arc<dyn ChannelFactory>)` - 注册工厂
  - [x] 2.2 实现 `create_channel(&mut self, config: ChannelConfig) -> Result<ChannelId>` - 创建渠道
  - [x] 2.3 实现 `remove_channel(&mut self, id: &ChannelId) -> Result<()>` - 移除渠道
  - [x] 2.4 实现 `get_channel(&self, id: &ChannelId) -> Option<&dyn Channel>` - 获取渠道
  - [x] 2.5 实现 `list_channels(&self) -> Vec<ChannelInfo>` - 列出所有渠道信息

- [x] Task 3: 实现连接管理 (AC: #2, #5)
  - [x] 3.1 实现 `connect_channel(&mut self, id: &ChannelId) -> Result<()>` - 连接单个渠道
  - [x] 3.2 实现 `disconnect_channel(&mut self, id: &ChannelId) -> Result<()>` - 断开单个渠道
  - [x] 3.3 实现 `connect_all(&mut self) -> Vec<(ChannelId, Result<()>)>` - 连接所有渠道
  - [x] 3.4 实现 `disconnect_all(&mut self) -> Vec<(ChannelId, Result<()>)>` - 断开所有渠道
  - [x] 3.5 实现自动重连逻辑：
    - 定义 `ReconnectPolicy` 结构体：`max_attempts: u32`, `delay_ms: u64`
    - 实现 `try_reconnect(&mut self, id: &ChannelId) -> Result<()>`

- [x] Task 4: 实现消息路由 (AC: #3, #4)
  - [x] 4.1 定义 `AgentId` 类型别名
  - [x] 4.2 实现 `bind_agent(&mut self, channel_id: &ChannelId, agent_id: AgentId)` - 绑定代理到渠道
  - [x] 4.3 实现 `unbind_agent(&mut self, channel_id: &ChannelId)` - 解绑代理
  - [x] 4.4 实现 `set_default_handler(&mut self, handler: Arc<dyn MessageHandler>)` - 设置默认处理器
  - [x] 4.5 实现 `send_to_channel(&self, channel_id: &ChannelId, message: OutgoingMessage) -> Result<MessageId>` - 发送消息到渠道
  - [x] 4.6 实现 `broadcast_message(&self, message: OutgoingMessage) -> Vec<(ChannelId, Result<MessageId>)>` - 广播消息到所有渠道

- [x] Task 5: 实现事件系统 (AC: #6)
  - [x] 5.1 定义 `ChannelEvent` 枚举：
    - `Connected { channel_id, channel_kind }`
    - `Disconnected { channel_id, reason }`
    - `Error { channel_id, error }`
    - `Reconnecting { channel_id, attempt }`
    - `MessageReceived { channel_id, message_id }`
    - `MessageSent { channel_id, message_id }`
  - [x] 5.2 实现 `subscribe_events(&self) -> broadcast::Receiver<ChannelEvent>` - 订阅事件
  - [x] 5.3 在各操作中发送相应事件

- [x] Task 6: 实现状态查询 (AC: #2)
  - [x] 6.1 实现 `get_channel_status(&self, id: &ChannelId) -> Option<ChannelStatus>` - 获取渠道状态
  - [x] 6.2 实现 `get_all_statuses(&self) -> HashMap<ChannelId, ChannelStatus>` - 获取所有状态
  - [x] 6.3 实现 `is_channel_connected(&self, id: &ChannelId) -> bool` - 检查连接状态
  - [x] 6.4 实现 `get_connected_count(&self) -> usize` - 获取已连接渠道数量

- [x] Task 7: 持久化支持 (AC: #1)
  - [x] 7.1 实现 `save_config(&self, path: &Path) -> Result<()>` - 保存渠道配置
  - [x] 7.2 实现 `load_config(&mut self, path: &Path) -> Result<()>` - 加载渠道配置
  - [x] 7.3 支持从 TOML 文件加载渠道配置

- [x] Task 8: 单元测试 (AC: #8)
  - [x] 8.1 测试工厂注册和渠道创建
  - [x] 8.2 测试渠道添加/移除
  - [x] 8.3 测试连接状态跟踪
  - [x] 8.4 测试消息路由
  - [x] 8.5 测试事件发布/订阅
  - [x] 8.6 测试重连逻辑

## Dev Notes

### 架构上下文

Story 6.2 基于 Story 6.1 完成的 Channel trait、ChannelFactory、ChannelConfig 等基础设施，实现中央管理器。

**依赖关系：**
- **Story 6.1 (已完成)**: 提供了 `Channel` trait, `ChannelFactory` trait, `ChannelConfig`, `ChannelError`, `ChannelStatus` 等核心类型

**后续 Stories：**
- Story 6.3: Slack 适配器 - 将使用 ChannelManager 注册 Slack 工厂
- Story 6.4: Discord 适配器 - 将使用 ChannelManager 注册 Discord 工厂
- Story 6.5: Email 适配器 - 将使用 ChannelManager 注册 Email 工厂

### ChannelManager 架构

```
┌─────────────────────────────────────────────────────────────────┐
│                      ChannelManager                              │
├─────────────────────────────────────────────────────────────────┤
│  - channels: HashMap<ChannelId, ChannelEntry>                   │
│  - factories: HashMap<ChannelKind, Arc<dyn ChannelFactory>>     │
│  - default_handler: Option<Arc<dyn MessageHandler>>             │
│  - event_tx: broadcast::Sender<ChannelEvent>                    │
├─────────────────────────────────────────────────────────────────┤
│  + register_factory(factory)                                     │
│  + create_channel(config) -> ChannelId                          │
│  + remove_channel(id)                                            │
│  + connect_channel(id) / disconnect_channel(id)                 │
│  + bind_agent(channel_id, agent_id)                             │
│  + send_to_channel(channel_id, message)                         │
│  + subscribe_events() -> Receiver<ChannelEvent>                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                     ChannelEntry                                  │
├─────────────────────────────────────────────────────────────────┤
│  - channel: Box<dyn Channel>                                     │
│  - config: ChannelConfig                                         │
│  - agent_binding: Option<AgentId>                               │
│  - reconnect_policy: ReconnectPolicy                            │
└─────────────────────────────────────────────────────────────────┘
```

### 与 Agent 模块集成

ChannelManager 需要与 Agent Dispatcher 集成：

```rust
// 在 AgentService 中使用 ChannelManager
impl AgentService {
    pub async fn handle_channel_message(&self, channel_id: &str, message: IncomingMessage) {
        // 1. 根据 channel_id 找到绑定的 agent
        // 2. 调用 agent 处理消息
        // 3. 获取 agent 响应
        // 4. 通过 ChannelManager 发送响应回渠道
    }
}
```

### 消息流

```
外部渠道 (Slack/Discord/Email)
        │
        ▼ IncomingMessage
┌───────────────────┐
│  Channel Adapter  │
└────────┬──────────┘
         │
         ▼ handle()
┌───────────────────┐
│  MessageHandler   │ (由 ChannelManager 设置)
└────────┬──────────┘
         │
         ▼
┌───────────────────┐
│   AgentService    │
└────────┬──────────┘
         │
         ▼ OutgoingMessage
┌───────────────────┐
│  ChannelManager   │.send_to_channel()
└────────┬──────────┘
         │
         ▼
┌───────────────────┐
│  Channel Adapter  │
└────────┬──────────┘
         │
         ▼
外部渠道 (Slack/Discord/Email)
```

### 文件结构

```
crates/omninova-core/src/channels/
├── mod.rs           # 已存在 - 更新导出
├── traits.rs        # 已存在 - Channel trait
├── types.rs         # 已存在 - 消息类型
├── error.rs         # 已存在 - 错误类型
├── manager.rs       # 新增 - ChannelManager 实现
└── event.rs         # 新增 - 事件类型定义
```

### 关键设计决策

1. **线程安全**: 使用 `Arc<Mutex<ChannelManager>>` 或 `Arc<RwLock<ChannelManager>>` 共享管理器
2. **事件广播**: 使用 `tokio::sync::broadcast` 实现事件发布/订阅
3. **异步友好**: 所有 I/O 操作使用 async/await
4. **错误恢复**: 提供重连策略配置

### 重连策略

```rust
pub struct ReconnectPolicy {
    /// 最大重试次数 (0 = 无限重试)
    pub max_attempts: u32,
    /// 初始延迟 (毫秒)
    pub initial_delay_ms: u64,
    /// 最大延迟 (毫秒)
    pub max_delay_ms: u64,
    /// 乘数因子 (指数退避)
    pub multiplier: f64,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            initial_delay_ms: 1000,
            max_delay_ms: 60000,
            multiplier: 2.0,
        }
    }
}
```

### 事件类型

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChannelEvent {
    Connected {
        channel_id: ChannelId,
        channel_kind: ChannelKind,
    },
    Disconnected {
        channel_id: ChannelId,
        reason: Option<String>,
    },
    Error {
        channel_id: ChannelId,
        error: String,
    },
    Reconnecting {
        channel_id: ChannelId,
        attempt: u32,
    },
    MessageReceived {
        channel_id: ChannelId,
        message_id: String,
    },
    MessageSent {
        channel_id: ChannelId,
        message_id: String,
    },
}
```

### 命名约定

遵循项目 Rust 命名约定：
- **函数**: snake_case (`create_channel`, `send_to_channel`)
- **结构体**: PascalCase (`ChannelManager`, `ChannelEntry`)
- **枚举**: PascalCase (`ChannelEvent`, `ReconnectPolicy`)
- **类型别名**: PascalCase (`ChannelId`, `AgentId`)

### Previous Story Intelligence (Story 6.1)

**已实现：**
1. `Channel` trait - 渠道适配器核心接口
2. `ChannelFactory` trait - 工厂模式创建渠道
3. `MessageHandler` trait - 消息处理接口
4. `ChannelConfig` - 渠道配置结构
5. `ChannelSettings` - 渠道设置
6. `ChannelCapabilities` - 渠道能力位标志
7. `ChannelError` - 错误类型
8. `ChannelStatus` - 状态枚举
9. `IncomingMessage` / `OutgoingMessage` - 消息类型

**关键决策：**
- 使用 `ChannelKind` 而非 `ChannelType`（与现有代码兼容）
- bitflags 启用 serde feature
- 手动实现 `Default` for `ChannelSettings`

### References

- [Source: epics.md#Story 6.2] - 原始 story 定义
- [Source: architecture.md#渠道适配器架构] - 架构设计图
- [Source: architecture.md#FR27-FR32] - 多渠道连接功能需求
- [Source: Story 6.1] - Channel trait 和基础类型实现

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

No critical issues encountered. All 103 tests pass successfully.

### Completion Notes List

1. **Module Structure**: Created `event.rs` with `ChannelEvent` enum, `ReconnectPolicy` struct, and `AgentId` type alias. Created `manager.rs` with `ChannelManager` and `ChannelEntry` structs.

2. **ChannelKind Hash**: Added `Hash` derive to `ChannelKind` enum in `mod.rs` to enable use as HashMap key for the factories registry.

3. **ReconnectPolicy**: Implemented exponential backoff with configurable `max_attempts`, `initial_delay_ms`, `max_delay_ms`, and `multiplier`. Supports unlimited retries when `max_attempts = 0`.

4. **ChannelEvent**: Added additional events beyond spec: `Created`, `Removed`, `AgentBound`, `AgentUnbound` for better lifecycle tracking.

5. **Error Types**: Added `DuplicateChannel`, `NoFactory`, and `ReconnectExhausted` error variants to `ChannelError` enum.

6. **Event Broadcasting**: Used `tokio::sync::broadcast` channel for event pub/sub with 256 capacity.

7. **Persistence**: Implemented `save_config` and `load_config` methods using TOML serialization.

### File List

- `crates/omninova-core/src/channels/event.rs` - **Created**: ChannelEvent enum, ReconnectPolicy struct, AgentId type alias
- `crates/omninova-core/src/channels/manager.rs` - **Created**: ChannelManager struct, ChannelEntry struct, all manager methods
- `crates/omninova-core/src/channels/mod.rs` - **Modified**: Added Hash derive to ChannelKind, added event and manager module exports
- `crates/omninova-core/src/channels/error.rs` - **Modified**: Added DuplicateChannel, NoFactory, ReconnectExhausted error variants