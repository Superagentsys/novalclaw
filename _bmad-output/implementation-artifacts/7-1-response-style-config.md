# Story 7.1: 代理响应风格配置

Status: done

## Story

As a 用户,
I want 调整 AI 代理的响应风格和行为,
So that 代理可以以符合我期望的方式与我交流.

## Acceptance Criteria

1. **AC1: 风格预设选择** - 可以选择响应风格预设（正式、随意、简洁、详细）
2. **AC2: 语气参数调整** - 可以调整语气参数（简洁程度、详细程度）
3. **AC3: 响应长度偏好** - 可以设置响应长度偏好（简短、中等、详细）
4. **AC4: 实时生效** - 风格变更实时反映在后续对话中
5. **AC5: 预览功能** - 可以预览风格设置效果

## Tasks / Subtasks

- [x] Task 1: 扩展后端代理模型 (AC: #1, #2, #3)
  - [x] 1.1 在 AgentModel 中添加 response_style 字段
  - [x] 1.2 创建 AgentStyleConfig 结构体（风格、语气、长度偏好）
  - [x] 1.3 添加数据库迁移脚本
  - [x] 1.4 更新 AgentStore 的 CRUD 方法

- [x] Task 2: 集成 ResponseStyleProcessor 到 AgentService (AC: #4)
  - [x] 2.1 在 AgentService 中应用响应风格处理
  - [x] 2.2 在流式响应中应用风格转换
  - [x] 2.3 确保 style processor 与 LLM 生成内容协调

- [x] Task 3: 实现 Tauri Commands (AC: #1-#5)
  - [x] 3.1 添加 `get_agent_style_config` 命令
  - [x] 3.2 添加 `update_agent_style_config` 命令
  - [x] 3.3 添加 `preview_style_effect` 命令

- [x] Task 4: 实现前端类型定义 (AC: #1-#3)
  - [x] 4.1 扩展 Agent 类型添加 styleConfig 字段
  - [x] 4.2 创建 AgentStyleConfig TypeScript 类型
  - [x] 4.3 创建 useAgentStyleConfig hook

- [x] Task 5: 实现 AgentStyleConfigForm 组件 (AC: #1-#3)
  - [x] 5.1 创建风格预设选择器
  - [x] 5.2 创建语气参数滑块
  - [x] 5.3 创建响应长度选择器
  - [x] 5.4 集成到 AgentEditForm

- [x] Task 6: 实现风格预览功能 (AC: #5)
  - [x] 6.1 创建 StylePreviewCard 组件
  - [x] 6.2 显示示例响应对比
  - [x] 6.3 实时更新预览

- [ ] Task 7: 单元测试 (AC: 全部)
  - [ ] 7.1 测试 AgentStyleConfig 数据模型
  - [ ] 7.2 测试 ResponseStyleProcessor 集成
  - [ ] 7.3 测试前端组件

## Dev Notes

### 架构上下文

Story 7.1 基于 Epic 2 已完成的代理系统，为代理添加响应风格配置能力。

**依赖关系：**
- **Epic 2 (已完成)**: AgentModel, AgentStore, AgentService 实现
- **Story 6.6 (已完成)**: ResponseStyle 和 ResponseStyleProcessor 已在 channels/behavior 中实现

**功能需求关联：**
- FR33: 用户可以调整代理响应风格和行为

**现有实现可复用：**

```rust
// From channels/behavior/style.rs
pub enum ResponseStyle {
    Formal,    // 正式
    Casual,    // 随意
    Concise,   // 简洁
    Detailed,  // 详细
}

pub struct ResponseStyleProcessor;
// 方法: apply_style(content, style) -> String
// 方法: truncate(content, max_length) -> String
```

### 后端数据模型扩展

```rust
// 新增: crates/omninova-core/src/agent/style_config.rs

/// Agent style configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentStyleConfig {
    /// Primary response style
    pub response_style: ResponseStyle,
    /// Verbosity level (0.0-1.0, affects detail level)
    pub verbosity: f32,
    /// Maximum response length (0 = no limit)
    pub max_response_length: usize,
    /// Enable friendly additions (greetings, sign-offs)
    pub friendly_tone: bool,
}

/// Verbosity presets
pub enum VerbosityPreset {
    Brief,    // 0.2
    Normal,   // 0.5
    Detailed, // 0.8
}
```

### 数据库迁移

```sql
-- Migration: Add style config to agents
ALTER TABLE agents ADD COLUMN style_config TEXT;
-- Default: {"responseStyle": "detailed", "verbosity": 0.5, "maxResponseLength": 0, "friendlyTone": true}
```

### AgentModel 扩展

```rust
// 在 agent/model.rs 中添加
pub struct AgentModel {
    // ... existing fields
    /// Style configuration (JSON serialized)
    pub style_config: Option<String>,
}

impl AgentModel {
    pub fn get_style_config(&self) -> AgentStyleConfig {
        self.style_config
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }
}
```

### AgentService 集成

```rust
// 在 agent/service.rs 中
impl AgentService {
    pub async fn process_response(&self, content: String, style: &AgentStyleConfig) -> String {
        let mut result = ResponseStyleProcessor::apply_style(&content, style.response_style);

        if style.max_response_length > 0 {
            result = ResponseStyleProcessor::truncate(&result, style.max_response_length);
        }

        result
    }
}
```

### 前端类型定义

```typescript
// src/types/agent.ts (扩展)

export interface AgentStyleConfig {
  responseStyle: 'formal' | 'casual' | 'concise' | 'detailed';
  verbosity: number; // 0.0 - 1.0
  maxResponseLength: number; // 0 = no limit
  friendlyTone: boolean;
}

export const VERBOSITY_PRESETS = {
  brief: 0.2,
  normal: 0.5,
  detailed: 0.8,
} as const;

export const RESPONSE_STYLE_LABELS: Record<AgentStyleConfig['responseStyle'], string> = {
  formal: '正式',
  casual: '随意',
  concise: '简洁',
  detailed: '详细',
};
```

### 组件设计

#### AgentStyleConfigForm

```
┌─────────────────────────────────────────────────────┐
│ 响应风格配置                                         │
├─────────────────────────────────────────────────────┤
│                                                     │
│ 风格预设                                             │
│ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐   │
│ │  正式   │ │  随意  │ │  简洁  │ │  详细  │   │
│ └─────────┘ └─────────┘ └─────────┘ └─────────┘   │
│                                                     │
│ 详细程度                                             │
│ [━━━━━━━━━━━━━━●━━━━━━━━━━━━━]  中等               │
│ 简短                                    详细        │
│                                                     │
│ 最大响应长度                                         │
│ ┌─────────────────────────────────────────────────┐ │
│ │ 0 (无限制)                                      │ │
│ └─────────────────────────────────────────────────┘ │
│                                                     │
│ ☑ 添加友好问候语                                     │
│                                                     │
├─────────────────────────────────────────────────────┤
│ 预览效果                                            │
│ ┌─────────────────────────────────────────────────┐ │
│ │ 示例响应：                                      │ │
│ │ "您好！很高兴为您提供帮助。请问有什么我可以      │ │
│ │  为您解答的问题吗？"                            │ │
│ └─────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────┘
```

### 文件结构

```
crates/omninova-core/src/agent/
├── style_config.rs           # 新增 - AgentStyleConfig 定义
├── model.rs                  # 修改 - 添加 style_config 字段
├── store.rs                  # 修改 - 更新 CRUD 方法
└── service.rs                # 修改 - 集成 ResponseStyleProcessor

apps/omninova-tauri/src/
├── types/
│   └── agent.ts              # 扩展 - 添加 AgentStyleConfig 类型
├── hooks/
│   └── useAgentStyleConfig.ts # 新增 - 风格配置 hook
├── components/agent/
│   ├── AgentStyleConfigForm.tsx # 新增 - 风格配置表单
│   ├── AgentEditForm.tsx      # 修改 - 集成风格配置
│   └── __tests__/
│       └── AgentStyleConfigForm.test.tsx # 新增
└── pages/
    └── AgentEditPage.tsx     # 修改 - 显示风格配置

apps/omninova-tauri/src-tauri/src/
└── lib.rs                    # 修改 - 添加风格配置 Tauri commands

crates/omninova-core/src/db/
└── migrations.rs             # 修改 - 添加 style_config 列
```

### 与 Channel BehaviorConfig 的关系

**ChannelBehaviorConfig (Story 6.6)** 用于渠道级别的行为配置：
- 影响特定渠道的响应行为
- 包含 trigger_keywords, working_hours 等渠道特有配置

**AgentStyleConfig (Story 7.1)** 用于代理级别的风格配置：
- 影响所有渠道的响应风格
- 包含 response_style, verbosity, max_length 等通用配置

**优先级**：ChannelBehaviorConfig 的 responseStyle 可覆盖 AgentStyleConfig

### 命名约定

**Rust:**
- 结构体: PascalCase (`AgentStyleConfig`)
- 字段: snake_case (`response_style`)
- 方法: snake_case (`apply_style`)

**TypeScript/React:**
- 接口: PascalCase (`AgentStyleConfig`)
- 属性: camelCase (`responseStyle`)
- 组件: PascalCase (`AgentStyleConfigForm`)

### 测试策略

1. **单元测试**：
   - ResponseStyleProcessor 与 AgentStyleConfig 集成
   - AgentStore CRUD 操作
   - 风格配置序列化/反序列化

2. **组件测试**：
   - AgentStyleConfigForm 渲染和交互
   - 风格预览更新逻辑
   - 表单验证

### Previous Story Intelligence (Story 6.8)

**学习要点：**
1. Tauri commands 使用 `tauri::command` 宏
2. 前端使用 `invoke` 调用后端
3. 使用 `sonner` 库显示 toast 通知
4. 表单使用 React Hook Form 或受控组件

**可复用模式：**
- `useChannelConfig` hook 模式可用于 `useAgentStyleConfig`
- 配置表单组件结构可参考 `ChannelConfigForm`

**注意事项：**
- 数据库迁移需要更新 migrations.rs
- 需要处理默认值和向后兼容
- 前端需要添加加载状态处理

### References

- [Source: epics.md#Story 7.1] - 原始 story 定义
- [Source: architecture.md#FR33] - 响应风格配置需求
- [Source: channels/behavior/style.rs] - ResponseStyleProcessor 实现
- [Source: agent/model.rs] - AgentModel 定义
- [Source: agent/store.rs] - AgentStore 实现
- [Source: agent/service.rs] - AgentService 实现

## Dev Agent Record

### Agent Model Used

{{agent_model_name_version}}

### Debug Log References

### Completion Notes List

### File List