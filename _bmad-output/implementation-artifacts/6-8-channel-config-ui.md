# Story 6.8: 渠道配置界面

Status: ready-for-dev

## Story

As a 用户,
I want 通过图形界面配置通信渠道,
So that 我可以轻松添加和管理渠道连接.

## Acceptance Criteria

1. **AC1: 渠道类型列表显示** - 显示可添加的渠道类型列表（Slack、Discord、Email等）
2. **AC2: 添加新渠道** - 可以添加新渠道（选择类型、输入凭据、配置设置）
3. **AC3: 编辑渠道配置** - 可以编辑现有渠道配置
4. **AC4: 删除渠道配置** - 可以删除渠道配置
5. **AC5: 测试连接功能** - 提供"测试连接"按钮验证配置有效性
6. **AC6: 安全存储** - 敏感凭据（如 API Token）安全存储（使用 OS Keychain）

## Tasks / Subtasks

- [ ] Task 1: 定义前端类型和接口 (AC: #1, #2)
  - [ ] 1.1 扩展 `ChannelConfig` 类型定义（TypeScript）
  - [ ] 1.2 创建各渠道类型的配置 Schema（SlackConfig, DiscordConfig, EmailConfig）
  - [ ] 1.3 创建 `ChannelCredentials` 接口（用于凭据管理）
  - [ ] 1.4 创建表单验证规则

- [ ] Task 2: 实现 Tauri Commands (AC: #2, #3, #4, #5, #6)
  - [ ] 2.1 添加 `create_channel` Tauri command
  - [ ] 2.2 添加 `update_channel` Tauri command
  - [ ] 2.3 添加 `delete_channel` Tauri command
  - [ ] 2.4 添加 `test_channel_connection` Tauri command
  - [ ] 2.5 添加 `save_channel_credentials` Tauri command（集成 OS Keychain）
  - [ ] 2.6 添加 `get_channel_config` Tauri command

- [ ] Task 3: 实现渠道类型选择组件 (AC: #1)
  - [ ] 3.1 创建 `ChannelTypeSelector.tsx` 组件
  - [ ] 3.2 显示可用渠道类型网格（图标 + 名称）
  - [ ] 3.3 实现渠道类型描述和功能说明
  - [ ] 3.4 实现已配置数量显示

- [ ] Task 4: 实现渠道配置表单组件 (AC: #2, #3, #6)
  - [ ] 4.1 创建 `ChannelConfigForm.tsx` 基础组件
  - [ ] 4.2 创建 `SlackConfigForm.tsx` 特定配置表单
  - [ ] 4.3 创建 `DiscordConfigForm.tsx` 特定配置表单
  - [ ] 4.4 创建 `EmailConfigForm.tsx` 特定配置表单
  - [ ] 4.5 实现表单验证和错误提示
  - [ ] 4.6 实现敏感字段掩码显示

- [ ] Task 5: 实现渠道配置对话框 (AC: #2, #3, #5)
  - [ ] 5.1 创建 `ChannelConfigDialog.tsx` 组件
  - [ ] 5.2 实现添加/编辑模式切换
  - [ ] 5.3 实现测试连接按钮和状态显示
  - [ ] 5.4 实现保存/取消操作

- [ ] Task 6: 实现渠道列表管理组件 (AC: #3, #4)
  - [ ] 6.1 创建 `ChannelConfigList.tsx` 组件
  - [ ] 6.2 实现渠道配置卡片布局
  - [ ] 6.3 实现编辑/删除操作按钮
  - [ ] 6.4 实现删除确认对话框

- [ ] Task 7: 实现渠道设置页面 (AC: #1-#6)
  - [ ] 7.1 创建 `ChannelSettingsPage.tsx` 页面组件
  - [ ] 7.2 集成渠道类型选择器
  - [ ] 7.3 集成渠道配置列表
  - [ ] 7.4 实现空状态和加载状态

- [ ] Task 8: 单元测试 (AC: 全部)
  - [ ] 8.1 测试渠道配置表单组件
  - [ ] 8.2 测试渠道类型选择组件
  - [ ] 8.3 测试渠道配置对话框
  - [ ] 8.4 测试渠道列表管理组件

## Dev Notes

### 架构上下文

Story 6.8 基于 Epic 6 已完成的基础设施（Story 6.1-6.7），实现前端渠道配置界面。

**依赖关系：**
- **Story 6.1 (已完成)**: `Channel` trait 和 `ChannelKind` 枚举定义
- **Story 6.2 (已完成)**: `ChannelManager` 管理渠道实例
- **Story 6.3-6.5 (已完成)**: Slack/Discord/Email 适配器实现
- **Story 6.6 (已完成)**: `ChannelBehaviorConfig` 渠道行为配置
- **Story 6.7 (已完成)**: `ChannelStatus` 组件和 `useChannels` hook

**功能需求关联：**
- FR27: 用户可以将AI代理连接到Slack频道
- FR28: 用户可以将AI代理连接到Discord服务器
- FR29: 用户可以将AI代理连接到电子邮件账户
- FR30: 用户可以配置AI代理在不同渠道的行为差异

**UX设计要求：**
- UX-DR9: 实现 ChannelStatus 组件（连接状态、渠道设置）

### 后端已有类型

```rust
// From types.rs
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

// From behavior/config.rs
pub struct ChannelBehaviorConfig {
    pub response_style: ResponseStyle,
    pub trigger_keywords: Vec<String>,
    pub response_delay_ms: Option<u64>,
    pub active_hours: Option<ActiveHours>,
}
```

### 前端类型定义

```typescript
// src/types/channel.ts (扩展)

export interface ChannelConfig {
  id: string;
  name: string;
  kind: ChannelKind;
  credentials: ChannelCredentials;
  behavior: ChannelBehaviorConfig;
  enabled: boolean;
}

export interface ChannelCredentials {
  // Slack
  botToken?: string;
  appToken?: string;
  // Discord
  botToken?: string;
  guildId?: string;
  // Email
  imapHost?: string;
  imapPort?: number;
  smtpHost?: string;
  smtpPort?: number;
  username?: string;
  password?: string;
}

export interface ChannelBehaviorConfig {
  responseStyle: 'formal' | 'casual' | 'professional' | 'friendly';
  triggerKeywords: string[];
  responseDelayMs?: number;
  activeHours?: {
    start: string; // "09:00"
    end: string;   // "18:00"
    timezone: string;
  };
}

export interface ChannelType {
  kind: ChannelKind;
  name: string;
  description: string;
  icon: string;
  features: string[];
  configFields: ConfigField[];
}

export interface ConfigField {
  name: string;
  label: string;
  type: 'text' | 'password' | 'number' | 'select';
  required: boolean;
  placeholder?: string;
  helpText?: string;
  options?: { value: string; label: string }[];
}
```

### Tauri Commands 扩展

需要在 `apps/omninova-tauri/src-tauri/src/lib.rs` 添加以下 commands：

```rust
#[tauri::command]
async fn create_channel(config: ChannelConfig, app: AppHandle) -> Result<ChannelInfo, String> {
    // 创建新渠道配置
}

#[tauri::command]
async fn update_channel(channel_id: String, config: ChannelConfig, app: AppHandle) -> Result<(), String> {
    // 更新渠道配置
}

#[tauri::command]
async fn delete_channel(channel_id: String, app: AppHandle) -> Result<(), String> {
    // 删除渠道配置
}

#[tauri::command]
async fn test_channel_connection(channel_id: String, app: AppHandle) -> Result<bool, String> {
    // 测试渠道连接
}

#[tauri::command]
async fn save_channel_credentials(channel_id: String, credentials: ChannelCredentials, app: AppHandle) -> Result<(), String> {
    // 保存凭据到 OS Keychain
}

#[tauri::command]
async fn get_channel_config(channel_id: String, app: AppHandle) -> Result<ChannelConfig, String> {
    // 获取渠道配置（不含敏感凭据）
}
```

### 组件设计

#### ChannelTypeSelector

```
┌─────────────────────────────────────────────────────┐
│ 选择渠道类型                                         │
├─────────────────────────────────────────────────────┤
│ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐    │
│ │  [Slack图标] │ │ [Discord图标]│ │ [Email图标]  │    │
│ │    Slack    │ │   Discord   │ │    Email    │    │
│ │  已配置: 1  │ │  已配置: 0  │ │  已配置: 2  │    │
│ └─────────────┘ └─────────────┘ └─────────────┘    │
│                                                     │
│ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐    │
│ │ [Telegram]  │ │  [WeChat]   │ │  [Feishu]   │    │
│ └─────────────┘ └─────────────┘ └─────────────┘    │
└─────────────────────────────────────────────────────┘
```

#### ChannelConfigDialog

```
┌─────────────────────────────────────────────────────┐
│ 配置 Slack 渠道                              [×]    │
├─────────────────────────────────────────────────────┤
│                                                     │
│ 渠道名称 *                                          │
│ ┌─────────────────────────────────────────────────┐ │
│ │ 我的Slack工作区                                  │ │
│ └─────────────────────────────────────────────────┘ │
│                                                     │
│ Bot Token *                    [显示/隐藏]         │
│ ┌─────────────────────────────────────────────────┐ │
│ │ xoxb-************                               │ │
│ └─────────────────────────────────────────────────┘ │
│                                                     │
│ App Token (可选)                                    │
│ ┌─────────────────────────────────────────────────┐ │
│ │                                                 │ │
│ └─────────────────────────────────────────────────┘ │
│                                                     │
│ 触发关键词                                          │
│ ┌─────────────────────────────────────────────────┐ │
│ │ @assistant, 帮帮我                              │ │
│ └─────────────────────────────────────────────────┘ │
│                                                     │
│ 响应延迟 (毫秒)                                     │
│ ┌─────────────────────────────────────────────────┐ │
│ │ 500                                             │ │
│ └─────────────────────────────────────────────────┘ │
│                                                     │
├─────────────────────────────────────────────────────┤
│ [测试连接]                    [取消]    [保存配置]  │
└─────────────────────────────────────────────────────┘
```

#### ChannelConfigList

```
┌─────────────────────────────────────────────────────┐
│ 已配置渠道                              [+ 添加]    │
├─────────────────────────────────────────────────────┤
│ ┌─────────────────────────────────────────────────┐ │
│ │ [Slack图标] 我的Slack工作区         [已连接]    │ │
│ │                              [编辑] [删除]      │ │
│ └─────────────────────────────────────────────────┘ │
│                                                     │
│ ┌─────────────────────────────────────────────────┐ │
│ │ [Email图标] 工作邮箱                [已断开]    │ │
│ │                              [编辑] [删除]      │ │
│ └─────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────┘
```

### 文件结构

```
apps/omninova-tauri/src/
├── types/
│   └── channel.ts              # 扩展 - 添加配置类型
├── components/
│   └── channels/
│       ├── ChannelTypeSelector.tsx      # 新增
│       ├── ChannelConfigForm.tsx        # 新增
│       ├── SlackConfigForm.tsx          # 新增
│       ├── DiscordConfigForm.tsx        # 新增
│       ├── EmailConfigForm.tsx          # 新增
│       ├── ChannelConfigDialog.tsx      # 新增
│       ├── ChannelConfigList.tsx        # 新增
│       ├── ChannelConfigCard.tsx        # 新增
│       ├── ChannelSettingsPage.tsx      # 新增
│       └── __tests__/
│           ├── ChannelTypeSelector.test.tsx
│           ├── ChannelConfigForm.test.tsx
│           ├── ChannelConfigDialog.test.tsx
│           └── ChannelConfigList.test.tsx
├── hooks/
│   └── useChannelConfig.ts     # 新增 - 渠道配置 hook
└── pages/
    └── settings/
        └── ChannelsPage.tsx    # 新增 - 设置页面路由

apps/omninova-tauri/src-tauri/src/
└── lib.rs                      # 修改 - 添加渠道配置 commands

crates/omninova-core/src/channels/
└── config.rs                   # 新增 - 渠道配置存储
```

### 敏感数据处理

**OS Keychain 集成（已在 Story 3.5 实现）：**
- API Token、密码等敏感数据存储在系统密钥链
- 配置文件仅存储密钥引用（如 `keychain://slack-bot-token`）
- 前端显示时使用掩码（如 `xoxb-************`）

**安全实践：**
1. 传输时使用 Tauri IPC（本地安全）
2. 存储时使用 OS Keychain（系统级加密）
3. 显示时使用掩码（视觉保护）
4. 复制时需要用户确认（防泄露）

### 测试策略

1. **组件测试**：
   - 使用 Vitest + React Testing Library
   - Mock Tauri invoke 函数
   - 测试表单验证逻辑
   - 测试对话框状态切换

2. **集成测试**：
   - 测试配置保存和加载流程
   - 测试连接测试功能

### Previous Story Intelligence (Story 6.7)

**学习要点：**
1. 使用 Tauri 事件系统实现实时状态更新
2. 使用 `sonner` 库显示 toast 通知
3. 状态颜色编码：绿色(已连接)、灰色(断开)、黄色(连接中)、红色(错误)
4. 使用 `lucide-react` 图标库
5. Shadcn/UI 组件：Badge、Card、Button、Dialog

**可复用模式：**
- `useChannels` hook 已实现基本状态管理
- `ChannelStatusCard` 组件可扩展编辑功能
- 测试放在 `__tests__` 目录

**注意事项：**
- Story 6.7 使用 `operationStates` 跟踪操作状态
- 敏感字段需要使用 password 类型输入框
- 删除操作需要确认对话框

### 命名约定

**TypeScript/React:**
- 组件: PascalCase (`ChannelConfigDialog`)
- 函数: camelCase (`handleSaveConfig`, `testConnection`)
- 常量: SCREAMING_SNAKE_CASE (`CHANNEL_TYPES`)
- 文件: PascalCase for components, camelCase for hooks

**CSS/Tailwind:**
- 使用 Tailwind utility classes
- 表单布局使用 `space-y-4`
- 按钮组使用 `flex gap-2`

### References

- [Source: epics.md#Story 6.8] - 原始 story 定义
- [Source: architecture.md#channels] - 渠道模块架构设计
- [Source: architecture.md#FR27-FR32] - 多渠道连接需求
- [Source: types.rs] - ChannelKind, ChannelInfo 类型定义
- [Source: manager.rs] - ChannelManager 现有实现
- [Source: behavior/config.rs] - ChannelBehaviorConfig 定义
- [Source: Story 6.7] - ChannelStatus 组件实现模式
- [Source: Story 3.5] - OS Keychain 集成实现

## Dev Agent Record

### Agent Model Used

{{agent_model_name_version}}

### Debug Log References

### Completion Notes List

### File List