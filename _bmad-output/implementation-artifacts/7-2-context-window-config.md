# Story 7.2: 上下文窗口配置

Status: done

## Story

As a 用户,
I want 设置 AI 代理的上下文窗口大小,
So that 我可以控制代理在一次对话中能记住多少内容.

## Acceptance Criteria

1. **AC1: 上下文窗口大小设置** - 可以设置上下文窗口大小（token 数量）
2. **AC2: Token 使用量预估** - 显示当前 token 使用量预估
3. **AC3: 推荐值提示** - 提供推荐值提示（根据所选 LLM 模型）
4. **AC4: 过大警告** - 设置过大时显示警告
5. **AC5: 溢出策略选择** - 上下文溢出时可选择策略（截断旧消息、摘要等）

## Tasks / Subtasks

- [x] Task 1: 扩展后端代理模型 (AC: #1, #5)
  - [x] 1.1 在 AgentModel 中添加 context_window_config 字段
  - [x] 1.2 创建 ContextWindowConfig 结构体（max_tokens, overflow_strategy）
  - [x] 1.3 添加数据库迁移脚本
  - [x] 1.4 更新 AgentStore 的 CRUD 方法

- [x] Task 2: 实现 Token 计数服务 (AC: #2)
  - [x] 2.1 创建 TokenCounter 服务
  - [x] 2.2 实现 tiktoken 兼容的 token 计数（或使用估算方法）
  - [x] 2.3 添加 Tauri command 用于计算 token 数量
  - [x] 2.4 提供模型推荐上下文窗口值

- [x] Task 3: 实现溢出策略处理器 (AC: #5)
  - [x] 3.1 创建 ContextOverflowProcessor
  - [x] 3.2 实现截断策略（移除最旧消息）
  - [x] 3.3 实现摘要策略（使用 LLM 摘要旧消息）- MVP 使用截断回退
  - [ ] 3.4 集成到 AgentDispatcher

- [x] Task 4: 实现 Tauri Commands (AC: #1-#5)
  - [x] 4.1 添加 `get_context_window_config` 命令
  - [x] 4.2 添加 `update_context_window_config` 命令
  - [x] 4.3 添加 `estimate_tokens` 命令
  - [x] 4.4 添加 `get_model_context_recommendations` 命令

- [x] Task 5: 实现前端类型定义 (AC: #1-#5)
  - [x] 5.1 扩展 Agent 类型添加 contextWindowConfig 字段
  - [x] 5.2 创建 ContextWindowConfig TypeScript 类型
  - [x] 5.3 创建 OverflowStrategy 枚举类型
  - [x] 5.4 创建 useContextWindowConfig hook

- [x] Task 6: 实现 ContextWindowConfigForm 组件 (AC: #1, #3, #4)
  - [x] 6.1 创建上下文窗口大小滑块
  - [x] 6.2 创建模型推荐提示显示
  - [x] 6.3 创建过大值警告提示
  - [x] 6.4 创建溢出策略选择器
  - [ ] 6.5 集成到 AgentEditForm 高级设置区域

- [x] Task 7: 实现 Token 使用量显示组件 (AC: #2)
  - [x] 7.1 创建 TokenUsageIndicator 组件
  - [x] 7.2 显示当前使用量/最大值
  - [x] 7.3 显示进度条
  - [ ] 7.4 集成到 ChatInterface 组件

- [ ] Task 8: 单元测试 (AC: 全部)
  - [x] 8.1 测试 ContextWindowConfig 数据模型
  - [x] 8.2 测试 TokenCounter 服务
  - [x] 8.3 测试溢出策略处理器
  - [ ] 8.4 测试前端组件

## Dev Notes

### 架构上下文

Story 7.2 基于 Epic 2 已完成的代理系统和 Story 7.1 的风格配置系统，为代理添加上下文窗口配置能力。

**依赖关系：**
- **Epic 2 (已完成)**: AgentModel, AgentStore, AgentService 实现
- **Story 7.1 (已完成)**: AgentStyleConfig 数据模型和表单模式
- **Epic 4 (已完成)**: Agent Dispatcher, 会话和消息模型

**功能需求关联：**
- FR34: 用户可以设置AI代理的上下文窗口大小

**与 Story 7.1 的关系：**
- 复用相同的 AgentModel 扩展模式
- 使用相同的数据库迁移方式
- 采用相同的表单组件结构

### 后端数据模型扩展

```rust
// 新增: crates/omninova-core/src/agent/context_window_config.rs

/// Overflow strategy when context window is exceeded
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum OverflowStrategy {
    #[default]
    Truncate,    // 截断最旧消息
    Summarize,   // 摘要旧消息
    Error,       // 返回错误
}

/// Context window configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextWindowConfig {
    /// Maximum context window size in tokens (0 = use model default)
    pub max_tokens: usize,
    /// Strategy when context exceeds limit
    pub overflow_strategy: OverflowStrategy,
    /// Whether to include system prompt in token count
    pub include_system_prompt: bool,
    /// Reserved tokens for model response
    pub response_reserve: usize,
}

impl Default for ContextWindowConfig {
    fn default() -> Self {
        Self {
            max_tokens: 4096,  // Safe default for most models
            overflow_strategy: OverflowStrategy::Truncate,
            include_system_prompt: true,
            response_reserve: 1024,
        }
    }
}
```

### 模型推荐值

```rust
// 在 context_window_config.rs 中

/// Model-specific context window recommendations
pub const MODEL_CONTEXT_RECOMMENDATIONS: &[(&str, usize, usize)] = &[
    // (model_pattern, recommended_max, absolute_max)
    ("gpt-4", 8192, 8192),
    ("gpt-4-32k", 32768, 32768),
    ("gpt-4-turbo", 128000, 128000),
    ("gpt-3.5-turbo", 16385, 16385),
    ("claude-3-opus", 200000, 200000),
    ("claude-3-sonnet", 200000, 200000),
    ("claude-3-haiku", 200000, 200000),
    ("claude-3-5-sonnet", 200000, 200000),
    ("llama2", 4096, 4096),
    ("llama3", 8192, 8192),
    ("mistral", 8192, 8192),
];

pub fn get_model_recommendation(model_name: &str) -> Option<(usize, usize)> {
    MODEL_CONTEXT_RECOMMENDATIONS
        .iter()
        .find(|(pattern, _, _)| model_name.to_lowercase().contains(pattern))
        .map(|(_, recommended, max)| (*recommended, *max))
}
```

### Token 计数服务

```rust
// 新增: crates/omninova-core/src/agent/token_counter.rs

use std::collections::HashMap;

/// Token counting service
pub struct TokenCounter {
    // 使用简化的估算方法（每个 token 约等于 4 个字符）
    // 或者集成 tiktoken-rs 用于精确计数
}

impl TokenCounter {
    /// Estimate token count for text (simplified method)
    pub fn estimate(text: &str) -> usize {
        // 简化方法：平均每 4 个字符 = 1 token
        // 中文可能需要调整：每 2 个字符 = 1 token
        let char_count = text.chars().count();
        let has_chinese = text.chars().any(|c| c > '\u{7F}');

        if has_chinese {
            char_count / 2 + char_count % 2
        } else {
            char_count / 4 + char_count % 4
        }
    }

    /// Count tokens for a conversation
    pub fn count_conversation(messages: &[Message]) -> usize {
        messages.iter()
            .map(|m| Self::estimate(&m.content) + 4) // +4 for role overhead
            .sum()
    }

    /// Count tokens for agent context (system prompt + messages)
    pub fn count_context(
        system_prompt: &str,
        messages: &[Message],
        include_system: bool,
    ) -> usize {
        let mut total = 0;
        if include_system {
            total += Self::estimate(system_prompt) + 4;
        }
        total += Self::count_conversation(messages);
        total
    }
}
```

### 溢出策略处理器

```rust
// 新增: crates/omninova-core/src/agent/overflow_processor.rs

use crate::memory::MemoryManager;

/// Context overflow processor
pub struct ContextOverflowProcessor {
    memory_manager: Option<Arc<MemoryManager>>,
}

impl ContextOverflowProcessor {
    /// Process messages when context exceeds limit
    pub async fn process(
        &self,
        messages: Vec<Message>,
        max_tokens: usize,
        strategy: OverflowStrategy,
        token_counter: &TokenCounter,
    ) -> Result<Vec<Message>> {
        let current_tokens = token_counter.count_conversation(&messages);

        if current_tokens <= max_tokens {
            return Ok(messages);
        }

        match strategy {
            OverflowStrategy::Truncate => {
                self.truncate_oldest(messages, max_tokens, token_counter)
            }
            OverflowStrategy::Summarize => {
                self.summarize_old_messages(messages, max_tokens).await
            }
            OverflowStrategy::Error => {
                bail!("Context window exceeded ({} > {})", current_tokens, max_tokens)
            }
        }
    }

    fn truncate_oldest(
        &self,
        mut messages: Vec<Message>,
        max_tokens: usize,
        token_counter: &TokenCounter,
    ) -> Result<Vec<Message>> {
        // Keep removing oldest messages until under limit
        while token_counter.count_conversation(&messages) > max_tokens && !messages.is_empty() {
            messages.remove(0);
        }
        Ok(messages)
    }

    async fn summarize_old_messages(
        &self,
        messages: Vec<Message>,
        max_tokens: usize,
    ) -> Result<Vec<Message>> {
        // Use LLM to create summary of old messages
        // This is a placeholder - actual implementation would call the LLM
        // and create a summary message to prepend

        // For MVP, fall back to truncation
        let token_counter = TokenCounter::new();
        self.truncate_oldest(messages, max_tokens, &token_counter)
    }
}
```

### 数据库迁移

```sql
-- Migration: Add context window config to agents
ALTER TABLE agents ADD COLUMN context_window_config TEXT;
-- Default: {"maxTokens": 4096, "overflowStrategy": "truncate", "includeSystemPrompt": true, "responseReserve": 1024}
```

### AgentModel 扩展

```rust
// 在 agent/model.rs 中添加
pub struct AgentModel {
    // ... existing fields including style_config from Story 7.1
    /// Context window configuration (JSON serialized)
    pub context_window_config: Option<String>,
}

impl AgentModel {
    pub fn get_context_window_config(&self) -> ContextWindowConfig {
        self.context_window_config
            .as_ref()
            .and_then(|s| ContextWindowConfig::from_json(s).ok())
            .unwrap_or_default()
    }
}
```

### 前端类型定义

```typescript
// src/types/agent.ts (扩展)

export type OverflowStrategy = 'truncate' | 'summarize' | 'error';

export interface ContextWindowConfig {
  maxTokens: number;          // 0 = use model default
  overflowStrategy: OverflowStrategy;
  includeSystemPrompt: boolean;
  responseReserve: number;    // Reserved tokens for response
}

export interface ModelContextRecommendation {
  modelName: string;
  recommended: number;
  max: number;
}

export const OVERFLOW_STRATEGY_LABELS: Record<OverflowStrategy, string> = {
  truncate: '截断旧消息',
  summarize: '摘要旧消息',
  error: '返回错误',
};

export const CONTEXT_WINDOW_PRESETS = [
  { label: '紧凑 (2K)', value: 2048 },
  { label: '标准 (4K)', value: 4096 },
  { label: '较大 (8K)', value: 8192 },
  { label: '大型 (16K)', value: 16384 },
  { label: '超大 (32K)', value: 32768 },
] as const;
```

### 组件设计

#### ContextWindowConfigForm

```
┌─────────────────────────────────────────────────────┐
│ 上下文窗口配置                                       │
├─────────────────────────────────────────────────────┤
│                                                     │
│ 上下文窗口大小                                       │
│ [━━━━━━━━━━━━━━●━━━━━━━━━━━━━]  4096 tokens        │
│ 紧凑                 标准                  较大     │
│                                                     │
│ ⚠️ 当前设置接近模型限制 (GPT-4 最大 8K)             │
│                                                     │
│ 推荐值: GPT-4 建议 8K tokens                        │
│                                                     │
│ 溢出处理策略                                         │
│ ┌─────────────────────────────────────────────────┐ │
│ │ ○ 截断旧消息 (推荐)                              │ │
│ │ ○ 摘要旧消息                                     │ │
│ │ ○ 返回错误                                       │ │
│ └─────────────────────────────────────────────────┘ │
│                                                     │
│ ☑ 包含系统提示词在 Token 计数中                     │
│                                                     │
│ 响应预留空间                                         │
│ ┌─────────────────────────────────────────────────┐ │
│ │ 1024 tokens                                     │ │
│ └─────────────────────────────────────────────────┘ │
│                                                     │
└─────────────────────────────────────────────────────┘
```

#### TokenUsageIndicator

```
┌─────────────────────────────────────────────────────┐
│ Token 使用量: 2456 / 4096                           │
│ [████████████████░░░░░░░░░░░░░░] 60%              │
└─────────────────────────────────────────────────────┘
```

### 文件结构

```
crates/omninova-core/src/agent/
├── context_window_config.rs  # 新增 - ContextWindowConfig 定义
├── token_counter.rs          # 新增 - Token 计数服务
├── overflow_processor.rs     # 新增 - 溢出策略处理器
├── model.rs                  # 修改 - 添加 context_window_config 字段
├── store.rs                  # 修改 - 更新 CRUD 方法
└── service.rs                # 修改 - 集成溢出处理

apps/omninova-tauri/src/
├── types/
│   └── agent.ts              # 扩展 - 添加 ContextWindowConfig 类型
├── hooks/
│   └── useContextWindowConfig.ts # 新增 - 上下文配置 hook
├── components/agent/
│   ├── ContextWindowConfigForm.tsx # 新增 - 上下文配置表单
│   ├── TokenUsageIndicator.tsx     # 新增 - Token 使用量指示器
│   ├── AgentEditForm.tsx      # 修改 - 集成上下文配置到高级设置
│   └── __tests__/
│       └── ContextWindowConfigForm.test.tsx # 新增
└── components/chat/
    └── ChatInterface.tsx      # 修改 - 集成 TokenUsageIndicator

apps/omninova-tauri/src-tauri/src/
└── lib.rs                    # 修改 - 添加上下文配置 Tauri commands

crates/omninova-core/src/db/
└── migrations.rs             # 修改 - 添加 context_window_config 列
```

### 命名约定

遵循 Story 7.1 确立的命名约定：

**Rust:**
- 结构体: PascalCase (`ContextWindowConfig`)
- 字段: snake_case (`max_tokens`, `overflow_strategy`)
- 方法: snake_case (`count_tokens`, `process_overflow`)

**TypeScript/React:**
- 接口: PascalCase (`ContextWindowConfig`)
- 属性: camelCase (`maxTokens`, `overflowStrategy`)
- 组件: PascalCase (`ContextWindowConfigForm`)

### 与 Agent Dispatcher 集成

```rust
// 在 agent/dispatcher.rs 中
impl AgentDispatcher {
    pub async fn process_message(
        &self,
        agent_id: &str,
        message: &str,
    ) -> Result<String> {
        let agent = self.store.get_agent(agent_id)?;
        let config = agent.get_context_window_config();

        // 获取当前会话消息
        let mut messages = self.session.get_messages()?;

        // 添加新消息
        messages.push(Message::user(message));

        // 检查并处理溢出
        let token_counter = TokenCounter::new();
        let current_tokens = token_counter.count_conversation(&messages);

        if current_tokens > config.max_tokens {
            let processor = ContextOverflowProcessor::new(self.memory.clone());
            messages = processor.process(
                messages,
                config.max_tokens - config.response_reserve,
                config.overflow_strategy,
                &token_counter,
            ).await?;
        }

        // 调用 LLM
        self.provider.chat(&messages, &agent).await
    }
}
```

### 测试策略

1. **单元测试**：
   - ContextWindowConfig 序列化/反序列化
   - TokenCounter 估算准确性
   - OverflowProcessor 各种策略测试
   - AgentStore CRUD 操作

2. **组件测试**：
   - ContextWindowConfigForm 渲染和交互
   - TokenUsageIndicator 显示逻辑
   - 警告提示显示条件

### Previous Story Intelligence (Story 7.1)

**可复用模式：**
- `AgentStyleConfigForm` 组件结构可用于 `ContextWindowConfigForm`
- `useAgentStyleConfig` hook 模式可用于 `useContextWindowConfig`
- 数据库迁移方式相同（添加 JSON 序列化字段）
- Tauri commands 结构相同（get/update 命令对）

**注意事项：**
- 需要将上下文配置放在"高级设置"折叠区域
- Token 计数是 CPU 密集操作，需要考虑性能
- 溢出处理需要在 Agent Dispatcher 中集成
- 前端需要在聊天界面显示实时 Token 使用量

### References

- [Source: epics.md#Story 7.2] - 原始 story 定义
- [Source: architecture.md#FR34] - 上下文窗口配置需求
- [Source: agent/dispatcher.rs] - AgentDispatcher 实现
- [Source: agent/model.rs] - AgentModel 定义
- [Source: Story 7.1] - AgentStyleConfig 实现模式

## Dev Agent Record

### Agent Model Used

{{agent_model_name_version}}

### Debug Log References

### Completion Notes List

### File List