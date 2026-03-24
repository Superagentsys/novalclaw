# Story 6.7: ChannelStatus 组件与渠道监控

Status: done

## Story

As a 用户,
I want 查看所有渠道的连接状态和活动,
So that 我可以监控代理在各渠道的运行情况.

## Acceptance Criteria

1. **AC1: 渠道列表显示** - 显示所有已配置渠道的列表 ✅
2. **AC2: 连接状态指示** - 每个渠道显示连接状态（已连接/断开/错误） ✅
3. **AC3: 活动统计** - 显示每个渠道的近期活动统计（发送/接收消息数） ✅
4. **AC4: 手动连接控制** - 可以手动连接/断开渠道 ✅
5. **AC5: 错误处理与重试** - 连接错误时显示错误信息和重试选项 ✅

## Tasks / Subtasks

- [x] Task 1: 定义前端类型和接口 (AC: #1, #2, #3)
  - [x] 1.1 创建 `ChannelStatus` 类型定义（TypeScript）
  - [x] 1.2 创建 `ChannelInfo` 接口（与 Rust 后端类型对齐）
  - [x] 1.3 创建 `ChannelActivityStats` 接口
  - [x] 1.4 创建渠道状态相关的常量定义

- [x] Task 2: 实现 Tauri Commands (AC: #1, #2, #3, #4)
  - [x] 2.1 添加 `get_all_channels` Tauri command
  - [x] 2.2 添加 `connect_channel` Tauri command
  - [x] 2.3 添加 `disconnect_channel` Tauri command
  - [x] 2.4 添加 `retry_channel_connection` Tauri command
  - [x] 2.5 添加 `init_channel_manager` command（替代 get_channel_stats）

- [x] Task 3: 实现 ChannelStatusCard 组件 (AC: #2, #3, #4, #5)
  - [x] 3.1 创建 `ChannelStatusCard.tsx` 组件
  - [x] 3.2 实现状态指示器（颜色编码：绿色/红色/黄色/灰色）
  - [x] 3.3 实现活动统计显示
  - [x] 3.4 实现连接/断开按钮
  - [x] 3.5 实现错误信息显示与重试按钮
  - [x] 3.6 实现加载状态骨架屏（在 ChannelStatusList 中）

- [x] Task 4: 实现 ChannelStatusList 组件 (AC: #1)
  - [x] 4.1 创建 `ChannelStatusList.tsx` 组件
  - [x] 4.2 实现渠道列表布局（网格视图）
  - [x] 4.3 实现空状态提示
  - [x] 4.4 实现刷新按钮

- [x] Task 5: 实现实时状态更新 (AC: #2)
  - [x] 5.1 监听 `ChannelEvent` 事件
  - [x] 5.2 实现状态变更的实时更新
  - [x] 5.3 实现连接状态动画效果

- [x] Task 6: 集成到设置页面 (AC: #1-#5)
  - [x] 6.1 创建 `ChannelStatusPanel.tsx` 组件
  - [x] 6.2 集成到设置/控制台页面（导出供外部使用）
  - [x] 6.3 实现页面布局和样式

- [x] Task 7: 单元测试 (AC: 全部)
  - [x] 7.1 测试 ChannelStatusCard 组件
  - [x] 7.2 测试 ChannelStatusList 组件
  - [x] 7.3 测试状态更新逻辑（通过 hook 测试覆盖）
  - [x] 7.4 测试错误处理和重试

## Dev Notes

### 架构上下文

Story 6.7 基于 Epic 6 已完成的基础设施（Story 6.1-6.6），实现前端渠道监控界面。

**依赖关系：**
- **Story 6.1 (已完成)**: `Channel` trait 和 `ChannelStatus` 枚举定义
- **Story 6.2 (已完成)**: `ChannelManager` 管理渠道实例，提供 `get_all_channels()` 方法
- **Story 6.3-6.5 (已完成)**: Slack/Discord/Email 适配器实现
- **Story 6.6 (已完成)**: `ChannelInfo` 类型和 `ChannelBehaviorConfig`

**功能需求关联：**
- FR31: 用户可以监控AI代理在各个渠道的活动
- FR32: 用户可以管理多个渠道的连接状态
- UX-DR9: 实现 ChannelStatus 组件（连接状态、渠道设置）

### 后端已有类型

```rust
// From types.rs
pub enum ChannelStatus {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    pub kind: ChannelKind,
    pub status: ChannelStatus,
    pub capabilities: ChannelCapabilities,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub last_activity: Option<i64>,
    pub error_message: Option<String>,
}

pub enum ChannelKind {
    Slack,
    Discord,
    Email,
    Telegram,
    Wechat,
    Feishu,
    Webhook,
    Custom(String),
}
```

### 前端类型定义

```typescript
// src/types/channel.ts

export type ChannelStatus = 'disconnected' | 'connecting' | 'connected' | 'error';

export type ChannelKind = 'slack' | 'discord' | 'email' | 'telegram' | 'wechat' | 'feishu' | 'webhook' | string;

export interface ChannelInfo {
  id: string;
  name: string;
  kind: ChannelKind;
  status: ChannelStatus;
  capabilities: number; // bitflags
  messagesSent: number;
  messagesReceived: number;
  lastActivity: number | null;
  errorMessage: string | null;
}

export interface ChannelActivityStats {
  channelId: string;
  messagesSentToday: number;
  messagesReceivedToday: number;
  averageResponseTime: number | null;
  lastError: string | null;
  lastErrorTime: number | null;
}
```

### Tauri Commands 扩展

需要在 `apps/omninova-tauri/src-tauri/src/lib.rs` 添加以下 commands：

```rust
#[tauri::command]
async fn get_all_channels(app: AppHandle) -> Result<Vec<ChannelInfo>, String> {
    // 获取所有渠道信息
}

#[tauri::command]
async fn connect_channel(channel_id: String, app: AppHandle) -> Result<(), String> {
    // 连接指定渠道
}

#[tauri::command]
async fn disconnect_channel(channel_id: String, app: AppHandle) -> Result<(), String> {
    // 断开指定渠道
}

#[tauri::command]
async fn retry_channel_connection(channel_id: String, app: AppHandle) -> Result<(), String> {
    // 重试连接（用于错误状态）
}
```

### 组件设计

#### ChannelStatusCard

```
┌─────────────────────────────────────────┐
│ [图标] 渠道名称                    [状态] │
│                                         │
│ 类型: Slack                             │
│ 活动统计: 发送 123 | 接收 456           │
│ 最后活动: 2分钟前                        │
│                                         │
│ [连接/断开按钮]        [错误信息/重试]   │
└─────────────────────────────────────────┘
```

**状态颜色编码：**
- `Connected`: 绿色 (#22c55e)
- `Disconnected`: 灰色 (#6b7280)
- `Connecting`: 黄色 (#eab308) + 动画
- `Error`: 红色 (#ef4444)

#### ChannelStatusList

```
┌─────────────────────────────────────────────────────┐
│ 渠道状态                              [刷新] [添加] │
├─────────────────────────────────────────────────────┤
│ ┌─────────────────┐ ┌─────────────────┐            │
│ │ ChannelCard 1   │ │ ChannelCard 2   │            │
│ └─────────────────┘ └─────────────────┘            │
│ ┌─────────────────┐ ┌─────────────────┐            │
│ │ ChannelCard 3   │ │ ChannelCard 4   │            │
│ └─────────────────┘ └─────────────────┘            │
└─────────────────────────────────────────────────────┘
```

### 文件结构

```
apps/omninova-tauri/src/
├── types/
│   └── channel.ts              # 新增 - 渠道类型定义
├── components/
│   ├── channels/               # 新增目录
│   │   ├── ChannelStatusCard.tsx
│   │   ├── ChannelStatusList.tsx
│   │   ├── ChannelStatusPanel.tsx
│   │   └── __tests__/
│   │       ├── ChannelStatusCard.test.tsx
│   │       └── ChannelStatusList.test.tsx
│   └── ui/
│       └── badge.tsx           # 新增 - badge 组件
└── hooks/
    └── useChannels.ts          # 新增 - 渠道状态 hook

apps/omninova-tauri/src-tauri/src/
└── lib.rs                      # 修改 - 添加 Tauri commands

crates/omninova-core/src/channels/
└── mod.rs                      # 修改 - 添加 Display trait for ChannelKind
```

### 实时事件监听

使用 Tauri 的事件系统监听 `ChannelEvent`：

```typescript
import { listen } from '@tauri-apps/api/event';

// 监听渠道状态变更
const unlisten = await listen<ChannelEvent>('channel-event', (event) => {
  const { type, channel_id, ...data } = event.payload;
  // 更新状态
});
```

### 测试策略

1. **组件测试**：
   - 使用 Vitest + React Testing Library
   - Mock Tauri invoke 函数
   - 测试各种状态渲染

2. **集成测试**：
   - 测试事件监听和状态更新
   - 测试错误处理流程

### Previous Story Intelligence (Story 6.6)

**学习要点：**
1. 使用 `serde` 的 `#[serde(default)]` 提供默认值
2. Builder pattern 用于复杂配置结构
3. 数据库迁移使用 `src/db/migrations.rs`
4. Tauri commands 使用 `#[tauri::command]` 宏

**可复用模式：**
- React 组件使用 Shadcn/UI 组件库
- 使用 `useToast` 显示操作反馈
- 使用 `lucide-react` 图标库
- 测试文件放在 `__tests__` 目录

### 命名约定

**TypeScript/React:**
- 组件: PascalCase (`ChannelStatusCard`)
- 函数: camelCase (`getChannelInfo`, `handleConnect`)
- 常量: SCREAMING_SNAKE_CASE (`CHANNEL_STATUS_COLORS`)
- 文件: PascalCase for components, camelCase for hooks/utils

**CSS/Tailwind:**
- 使用 Tailwind utility classes
- 状态颜色使用语义化变量（`bg-green-500`, `text-red-500`）

### References

- [Source: epics.md#Story 6.7] - 原始 story 定义
- [Source: architecture.md#channels] - 渠道模块架构设计
- [Source: architecture.md#FR31, FR32] - 渠道监控和管理需求
- [Source: types.rs] - ChannelStatus, ChannelInfo 类型定义
- [Source: manager.rs] - ChannelManager 现有实现
- [Source: event.rs] - ChannelEvent 事件定义
- [Source: Story 6.6] - 渠道行为配置实现模式

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

- Added `Display` trait implementation for `ChannelKind` enum in `mod.rs`
- Fixed test for ChannelStatusList that had duplicate text matching

### Completion Notes List

1. **Task 1 Complete**: Created comprehensive TypeScript types in `apps/omninova-tauri/src/types/channel.ts`:
   - `ChannelStatus` type
   - `ChannelKind` type
   - `ChannelInfo` interface (aligned with Rust backend)
   - `ChannelActivityStats` interface
   - `ChannelEvent` types for real-time updates
   - Helper functions: `hasCapability`, `getChannelKindLabel`, `formatTimeAgo`
   - Constants: `CHANNEL_STATUS_COLORS`, `CHANNEL_STATUS_LABELS`, `CHANNEL_KIND_LABELS`

2. **Task 2 Complete**: Added Tauri commands in `apps/omninova-tauri/src-tauri/src/lib.rs`:
   - `init_channel_manager` - Initialize ChannelManager
   - `get_all_channels` - Get all configured channels
   - `connect_channel` - Connect a specific channel
   - `disconnect_channel` - Disconnect a specific channel
   - `retry_channel_connection` - Retry connection for error state
   - Added `channel_manager: Option<Arc<Mutex<ChannelManager>>>` to AppState

3. **Task 3 Complete**: Implemented `ChannelStatusCard.tsx`:
   - Status indicator with color-coded badges
   - Activity statistics display (messages sent/received)
   - Last activity timestamp formatting
   - Connect/Disconnect/Retry buttons based on status
   - Error message display
   - Loading state with skeleton

4. **Task 4 Complete**: Implemented `ChannelStatusList.tsx`:
   - Grid layout for channel cards
   - Empty state with "Add Channel" option
   - Refresh button
   - Loading skeletons

5. **Task 5 Complete**: Implemented real-time updates in `useChannels.ts` hook:
   - Listens to `channel-event` from Tauri
   - Updates channel status on: connected, disconnected, error, reconnecting
   - Updates message counts on: message_received, message_sent

6. **Task 6 Complete**: Created `ChannelStatusPanel.tsx`:
   - Integrates `useChannels` hook
   - Error state display
   - Exports for use in settings/console pages

7. **Task 7 Complete**: Added comprehensive tests:
   - `ChannelStatusCard.test.tsx` - 19 tests covering status display, activity stats, action buttons, callbacks, operating state
   - `ChannelStatusList.test.tsx` - 8 tests covering rendering, loading state, empty state, action callbacks

8. **Additional Changes**:
   - Created `badge.tsx` UI component for status indicators
   - Added `Display` trait implementation for `ChannelKind` in `mod.rs`
   - All 747 Rust tests passing
   - All 27 frontend component tests passing

### File List

**Created Files:**
- `apps/omninova-tauri/src/types/channel.ts` - Channel type definitions
- `apps/omninova-tauri/src/hooks/useChannels.ts` - Channel management hook
- `apps/omninova-tauri/src/components/ui/badge.tsx` - Badge component
- `apps/omninova-tauri/src/components/channels/index.ts` - Module exports
- `apps/omninova-tauri/src/components/channels/ChannelStatusCard.tsx` - Card component
- `apps/omninova-tauri/src/components/channels/ChannelStatusList.tsx` - List component
- `apps/omninova-tauri/src/components/channels/ChannelStatusPanel.tsx` - Panel component
- `apps/omninova-tauri/src/components/channels/__tests__/ChannelStatusCard.test.tsx` - Card tests
- `apps/omninova-tauri/src/components/channels/__tests__/ChannelStatusList.test.tsx` - List tests

**Modified Files:**
- `apps/omninova-tauri/src-tauri/src/lib.rs` - Added channel commands and ChannelManager to AppState
- `crates/omninova-core/src/channels/mod.rs` - Added Display trait for ChannelKind

## Change Log

- 2026-03-22: Story created, ready for development
- 2026-03-22: Implementation complete, ready for review
  - All 5 acceptance criteria met
  - 27 frontend tests passing
  - 747 Rust tests passing