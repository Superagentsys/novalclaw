# Story 7.3: 触发关键词配置

Status: done

## Story

As a 用户,
I want 自定义 AI 代理的触发关键词,
So that 代理只在特定条件下响应.

## Acceptance Criteria

1. **AC1: 多关键词支持** - 可以添加多个触发关键词或短语
2. **AC2: 正则表达式匹配** - 支持正则表达式匹配
3. **AC3: 匹配模式选择** - 可以设置匹配模式（精确匹配、前缀匹配、包含匹配）
4. **AC4: 触发测试功能** - 提供触发词测试功能验证匹配效果
5. **AC5: 渠道消息触发** - 渠道消息只有匹配触发词时才触发代理响应

## Tasks / Subtasks

- [x] Task 1: 扩展后端代理模型 (AC: #1, #2, #3)
  - [x] 1.1 在 AgentModel 中添加 trigger_keywords_config 字段
  - [x] 1.2 创建 AgentTriggerConfig 结构体（关键词列表、默认匹配模式）
  - [x] 1.3 添加数据库迁移脚本
  - [x] 1.4 更新 AgentStore 的 CRUD 方法

- [x] Task 2: 创建 TriggerConfigService 服务 (AC: #4, #5)
  - [x] 2.1 创建 TriggerConfigService 封装触发词匹配逻辑
  - [x] 2.2 实现 test_trigger 方法用于测试匹配效果
  - [x] 2.3 集成已有的 TriggerKeywordMatcher
  - [x] 2.4 添加 Tauri command 用于测试触发词

- [x] Task 3: 实现 Tauri Commands (AC: #1-#5)
  - [x] 3.1 添加 `get_trigger_keywords_config` 命令
  - [x] 3.2 添加 `update_trigger_keywords_config` 命令
  - [x] 3.3 添加 `test_trigger_match` 命令
  - [x] 3.4 添加 `test_single_trigger` 和 `validate_trigger_keyword` 命令

- [x] Task 4: 实现前端类型定义 (AC: #1-#3)
  - [x] 4.1 扩展 Agent 类型添加 triggerKeywordsConfig 字段
  - [x] 4.2 创建 TriggerKeyword TypeScript 类型（复用 channels 类型）
  - [x] 4.3 创建 AgentTriggerConfig TypeScript 类型
  - [x] 4.4 创建 useTriggerKeywordsConfig hook

- [x] Task 5: 实现 TriggerKeywordsConfigForm 组件 (AC: #1, #2, #3)
  - [x] 5.1 创建关键词列表显示和编辑
  - [x] 5.2 创建添加关键词对话框（关键词、匹配模式、大小写敏感）
  - [x] 5.3 创建匹配模式选择器
  - [x] 5.4 创建正则表达式输入支持
  - [x] 5.5 集成到 AgentEditForm 高级设置区域

- [x] Task 6: 实现触发词测试功能 (AC: #4)
  - [x] 6.1 创建 TriggerTestPanel 组件
  - [x] 6.2 实现测试输入框和匹配结果显示
  - [x] 6.3 高亮显示匹配的关键词
  - [x] 6.4 显示匹配详情（匹配类型、匹配位置）

- [x] Task 7: 集成到渠道消息处理 (AC: #5)
  - [x] 7.1 在 ChannelManager 中集成代理触发词检查
  - [x] 7.2 实现代理触发词与渠道触发词的优先级逻辑
  - [x] 7.3 添加触发日志记录

- [x] Task 8: 单元测试 (AC: 全部)
  - [x] 8.1 测试 AgentTriggerConfig 数据模型
  - [x] 8.2 测试 TriggerConfigService
  - [x] 8.3 测试前端组件
  - [x] 8.4 测试触发词匹配逻辑

## Dev Notes

### 架构上下文

Story 7.3 基于 Epic 2 已完成的代理系统和 Epic 6 的渠道触发词实现，为代理添加代理级别的触发关键词配置能力。

**依赖关系：**
- **Epic 2 (已完成)**: AgentModel, AgentStore, AgentService 实现
- **Story 6.6 (已完成)**: `TriggerKeyword`, `MatchType`, `TriggerKeywordMatcher` 已在 channels/behavior 中实现
- **Story 7.1 (已完成)**: AgentStyleConfig 数据模型和表单模式
- **Story 7.2 (已完成)**: AgentModel 扩展模式（context_window_config）

**功能需求关联：**
- FR35: 用户可以自定义AI代理的触发关键词

**现有实现可复用：**

```rust
// From channels/behavior/trigger.rs (Story 6.6 已实现)
pub enum MatchType {
    Exact,     // 精确匹配
    Prefix,    // 前缀匹配
    Contains,  // 包含匹配
    Regex,     // 正则表达式匹配
}

pub struct TriggerKeyword {
    pub keyword: String,
    pub match_type: MatchType,
    pub case_sensitive: bool,
}

impl TriggerKeyword {
    pub fn matches(&self, text: &str) -> bool { /* ... */ }
}

pub struct TriggerKeywordMatcher;
impl TriggerKeywordMatcher {
    pub fn check_triggers(message: &str, keywords: &[TriggerKeyword]) -> bool { /* ... */ }
    pub fn find_matching<'a>(message: &str, keywords: &'a [TriggerKeyword]) -> Vec<&'a TriggerKeyword> { /* ... */ }
}
```

### 后端数据模型扩展

```rust
// 新增: crates/omninova-core/src/agent/trigger_config.rs

use crate::channels::behavior::{TriggerKeyword, MatchType, TriggerKeywordMatcher};

/// Agent-level trigger keywords configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentTriggerConfig {
    /// List of trigger keywords for this agent
    pub keywords: Vec<TriggerKeyword>,
    /// Whether triggers are enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Default match type for new keywords
    #[serde(default)]
    pub default_match_type: MatchType,
    /// Whether to use case-sensitive matching by default
    #[serde(default)]
    pub default_case_sensitive: bool,
}

fn default_enabled() -> bool { true }

impl AgentTriggerConfig {
    /// Check if a message matches any trigger keyword
    pub fn matches(&self, message: &str) -> bool {
        if !self.enabled || self.keywords.is_empty() {
            return true; // No filter when disabled or empty
        }
        TriggerKeywordMatcher::check_triggers(message, &self.keywords)
    }

    /// Find all matching keywords
    pub fn find_matches<'a>(&'a self, message: &str) -> Vec<&'a TriggerKeyword> {
        TriggerKeywordMatcher::find_matching(message, &self.keywords)
    }

    /// Add a new trigger keyword
    pub fn add_keyword(&mut self, keyword: TriggerKeyword) {
        if !self.keywords.iter().any(|k| k.keyword == keyword.keyword && k.match_type == keyword.match_type) {
            self.keywords.push(keyword);
        }
    }

    /// Remove a trigger keyword by index
    pub fn remove_keyword(&mut self, index: usize) -> Option<TriggerKeyword> {
        if index < self.keywords.len() {
            Some(self.keywords.remove(index))
        } else {
            None
        }
    }
}
```

### 数据库迁移

```sql
-- Migration: Add trigger keywords config to agents
ALTER TABLE agents ADD COLUMN trigger_keywords_config TEXT;
-- Default: {"keywords": [], "enabled": true, "defaultMatchType": "exact", "defaultCaseSensitive": false}
```

### AgentModel 扩展

```rust
// 在 agent/model.rs 中添加
pub struct AgentModel {
    // ... existing fields including style_config and context_window_config
    /// Trigger keywords configuration (JSON serialized)
    pub trigger_keywords_config: Option<String>,
}

impl AgentModel {
    pub fn get_trigger_keywords_config(&self) -> AgentTriggerConfig {
        self.trigger_keywords_config
            .as_ref()
            .and_then(|s| AgentTriggerConfig::from_json(s).ok())
            .unwrap_or_default()
    }

    pub fn set_trigger_keywords_config(&mut self, config: &AgentTriggerConfig) {
        self.trigger_keywords_config = config.to_json().ok();
    }
}
```

### TriggerConfigService

```rust
// 新增: crates/omninova-core/src/agent/trigger_service.rs

use super::trigger_config::AgentTriggerConfig;
use crate::channels::behavior::{TriggerKeyword, MatchType, TriggerKeywordMatcher};

/// Service for managing trigger keyword configuration
pub struct TriggerConfigService;

impl TriggerConfigService {
    /// Test a trigger keyword against sample text
    pub fn test_trigger(keyword: &TriggerKeyword, text: &str) -> TriggerTestResult {
        let is_match = keyword.matches(text);
        let matches = if is_match {
            TriggerKeywordMatcher::find_matching(text, std::slice::from_ref(keyword))
        } else {
            vec![]
        };

        TriggerTestResult {
            matched: is_match,
            matched_keywords: matches.into_iter().map(|k| k.keyword.clone()).collect(),
            match_positions: Self::find_match_positions(keyword, text),
        }
    }

    /// Test all keywords against sample text
    pub fn test_all_triggers(keywords: &[TriggerKeyword], text: &str) -> Vec<TriggerTestResult> {
        keywords.iter()
            .map(|k| Self::test_trigger(k, text))
            .collect()
    }

    fn find_match_positions(keyword: &TriggerKeyword, text: &str) -> Vec<(usize, usize)> {
        // Find all positions where the keyword matches
        // Implementation depends on match type
        // ...
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerTestResult {
    pub matched: bool,
    pub matched_keywords: Vec<String>,
    pub match_positions: Vec<(usize, usize)>,
}
```

### 前端类型定义

```typescript
// src/types/agent.ts (扩展)

// 复用 channels 中的类型
export type MatchType = 'exact' | 'prefix' | 'contains' | 'regex';

export interface TriggerKeyword {
  keyword: string;
  matchType: MatchType;
  caseSensitive: boolean;
}

export interface AgentTriggerConfig {
  keywords: TriggerKeyword[];
  enabled: boolean;
  defaultMatchType: MatchType;
  defaultCaseSensitive: boolean;
}

export const DEFAULT_TRIGGER_CONFIG: AgentTriggerConfig = {
  keywords: [],
  enabled: true,
  defaultMatchType: 'exact',
  defaultCaseSensitive: false,
};

export const MATCH_TYPE_LABELS: Record<MatchType, string> = {
  exact: '精确匹配',
  prefix: '前缀匹配',
  contains: '包含匹配',
  regex: '正则表达式',
};

export const MATCH_TYPE_DESCRIPTIONS: Record<MatchType, string> = {
  exact: '完整匹配整个词语',
  prefix: '匹配以关键词开头的内容',
  contains: '只要包含关键词即可匹配',
  regex: '使用正则表达式进行复杂匹配',
};
```

### 组件设计

#### TriggerKeywordsConfigForm

```
┌─────────────────────────────────────────────────────────────┐
│ 触发关键词配置                                                │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│ ☑ 启用触发关键词                                              │
│                                                             │
│ 已配置的关键词 (3)                                           │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ @help     精确匹配    [编辑] [删除]                       │ │
│ │ assist*   前缀匹配    [编辑] [删除]                       │ │
│ │ /bot\s+.+/ 正则表达式 [编辑] [删除]                       │ │
│ └─────────────────────────────────────────────────────────┘ │
│                                                             │
│ [+ 添加关键词]                                               │
│                                                             │
│ 默认匹配模式: [精确匹配 ▼]                                   │
│ ☐ 默认区分大小写                                             │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

#### AddTriggerKeywordDialog

```
┌─────────────────────────────────────────────────────────────┐
│ 添加触发关键词                                         [×]   │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│ 关键词/模式                                                  │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ @help                                                   │ │
│ └─────────────────────────────────────────────────────────┘ │
│                                                             │
│ 匹配模式                                                     │
│ ○ 精确匹配 - 完整匹配整个词语                                │
│ ○ 前缀匹配 - 匹配以关键词开头的内容                          │
│ ○ 包含匹配 - 只要包含关键词即可匹配                          │
│ ● 正则表达式 - 使用正则表达式进行复杂匹配                    │
│                                                             │
│ ☐ 区分大小写                                                 │
│                                                             │
│ [取消]                              [添加关键词]            │
└─────────────────────────────────────────────────────────────┘
```

#### TriggerTestPanel

```
┌─────────────────────────────────────────────────────────────┐
│ 触发词测试                                                   │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│ 测试消息                                                     │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ @help me with this problem                              │ │
│ └─────────────────────────────────────────────────────────┘ │
│                                                             │
│ [测试匹配]                                                   │
│                                                             │
│ 匹配结果: ✅ 匹配成功                                        │
│                                                             │
│ 匹配的关键词:                                                │
│ • "@help" (精确匹配) 位置: 0-5                              │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 文件结构

```
crates/omninova-core/src/agent/
├── trigger_config.rs         # 新增 - AgentTriggerConfig 定义
├── trigger_service.rs        # 新增 - TriggerConfigService
├── mod.rs                    # 修改 - 添加模块导出
├── model.rs                  # 修改 - 添加 trigger_keywords_config 字段
├── store.rs                  # 修改 - 更新 CRUD 方法
└── service.rs                # 修改 - 集成触发词检查

apps/omninova-tauri/src/
├── types/
│   └── agent.ts              # 扩展 - 添加 TriggerKeyword 类型
├── hooks/
│   └── useTriggerKeywordsConfig.ts # 新增 - 触发词配置 hook
├── components/agent/
│   ├── TriggerKeywordsConfigForm.tsx # 新增 - 触发词配置表单
│   ├── AddTriggerKeywordDialog.tsx   # 新增 - 添加关键词对话框
│   ├── TriggerTestPanel.tsx          # 新增 - 触发词测试面板
│   ├── AgentEditForm.tsx      # 修改 - 集成触发词配置
│   └── __tests__/
│       └── TriggerKeywordsConfigForm.test.tsx # 新增
└── components/chat/
    └── ChatInterface.tsx      # 可选 - 显示触发状态

apps/omninova-tauri/src-tauri/src/
└── lib.rs                    # 修改 - 添加触发词配置 Tauri commands

crates/omninova-core/src/db/
└── migrations.rs             # 修改 - 添加 trigger_keywords_config 列
```

### 与 Channel BehaviorConfig 的关系

**ChannelBehaviorConfig.trigger_keywords (Story 6.6)** 用于渠道级别的触发词配置：
- 影响特定渠道的消息触发
- 在渠道级别管理

**AgentTriggerConfig (Story 7.3)** 用于代理级别的触发词配置：
- 影响该代理在所有渠道的默认触发行为
- 在代理级别管理

**优先级逻辑：**
1. 如果渠道配置了触发词，使用渠道触发词
2. 如果渠道没有配置触发词，使用代理触发词
3. 如果两者都没有配置，所有消息都会触发代理响应

```rust
// 在 ChannelManager 或消息处理中的逻辑
fn should_respond(agent: &AgentModel, channel_config: &ChannelBehaviorConfig, message: &str) -> bool {
    // 优先使用渠道触发词
    if !channel_config.trigger_keywords.is_empty() {
        return TriggerKeywordMatcher::check_triggers(message, &channel_config.trigger_keywords);
    }

    // 回退到代理触发词
    let agent_config = agent.get_trigger_keywords_config();
    agent_config.matches(message)
}
```

### 命名约定

遵循 Story 7.1 和 7.2 确立的命名约定：

**Rust:**
- 结构体: PascalCase (`AgentTriggerConfig`)
- 字段: snake_case (`trigger_keywords_config`)
- 方法: snake_case (`test_trigger`)

**TypeScript/React:**
- 接口: PascalCase (`TriggerKeyword`, `AgentTriggerConfig`)
- 属性: camelCase (`matchType`, `caseSensitive`)
- 组件: PascalCase (`TriggerKeywordsConfigForm`)

### 测试策略

1. **单元测试**：
   - AgentTriggerConfig 序列化/反序列化
   - 触发词匹配逻辑（各种匹配模式）
   - TriggerConfigService 测试方法
   - AgentStore CRUD 操作

2. **组件测试**：
   - TriggerKeywordsConfigForm 渲染和交互
   - 添加/删除关键词逻辑
   - 触发词测试功能
   - 表单验证

### Previous Story Intelligence (Story 7.2)

**可复用模式：**
- AgentModel 扩展模式（添加 JSON 序列化字段）
- 数据库迁移方式（添加新列）
- Tauri commands 结构（get/update 命令对）
- 前端 hook 模式（useContextWindowConfig）

**注意事项：**
- 触发词配置应放在"高级设置"折叠区域
- 正则表达式需要验证有效性
- 触发词测试需要实时反馈
- 空触发词列表意味着不过滤（所有消息都触发）

### References

- [Source: epics.md#Story 7.3] - 原始 story 定义
- [Source: architecture.md#FR35] - 触发关键词配置需求
- [Source: channels/behavior/trigger.rs] - TriggerKeyword 和 TriggerKeywordMatcher 实现
- [Source: channels/behavior/config.rs] - ChannelBehaviorConfig 定义
- [Source: agent/model.rs] - AgentModel 定义
- [Source: Story 6.6] - 渠道行为配置实现
- [Source: Story 7.1] - AgentStyleConfig 实现模式
- [Source: Story 7.2] - ContextWindowConfig 实现模式

## Dev Agent Record

### Agent Model Used

{{agent_model_name_version}}

### Debug Log References

### Completion Notes List

### File List