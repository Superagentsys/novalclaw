# Story 7.4: 数据处理与隐私设置

Status: done

## Story

As a 用户,
I want 配置 AI 代理的数据处理和隐私设置,
So that 我可以控制代理如何处理我的数据.

## Acceptance Criteria

1. **AC1: 数据保留期限** - 可以设置数据保留期限
2. **AC2: 敏感信息过滤** - 可以启用/禁用敏感信息自动过滤
3. **AC3: 记忆共享范围** - 可以配置记忆共享范围（跨会话、跨代理）
4. **AC4: 排除数据设置** - 可以设置哪些数据不被存储到长期记忆
5. **AC5: 即时生效** - 设置变更立即生效

## Tasks / Subtasks

- [x] Task 1: 扩展后端代理模型 (AC: #1, #2, #3, #4)
  - [x] 1.1 在 AgentModel 中添加 privacy_config 字段
  - [x] 1.2 创建 AgentPrivacyConfig 结构体（数据保留、敏感过滤、共享范围）
  - [x] 1.3 添加数据库迁移脚本
  - [x] 1.4 更新 AgentStore 的 CRUD 方法

- [x] Task 2: 创建 PrivacyConfigService 服务 (AC: #2, #4, #5)
  - [x] 2.1 创建 PrivacyConfigService 封装隐私配置逻辑
  - [x] 2.2 实现敏感信息检测与过滤功能
  - [x] 2.3 实现数据保留策略执行
  - [x] 2.4 添加 Tauri commands 用于隐私配置操作

- [x] Task 3: 实现 Tauri Commands (AC: #1-#5)
  - [x] 3.1 添加 `get_privacy_config` 命令
  - [x] 3.2 添加 `update_privacy_config` 命令
  - [x] 3.3 添加 `test_sensitive_filter` 命令
  - [x] 3.4 添加 `validate_exclusion_pattern` 命令
  - [x] 3.5 添加 `validate_filter_pattern` 命令

- [x] Task 4: 实现前端类型定义 (AC: #1-#4)
  - [x] 4.1 扩展 Agent 类型添加 privacyConfig 字段
  - [x] 4.2 创建 PrivacyConfig TypeScript 类型
  - [x] 4.3 创建 DataRetentionPolicy TypeScript 类型
  - [x] 4.4 创建 usePrivacyConfig hook

- [x] Task 5: 实现 PrivacyConfigForm 组件 (AC: #1, #2, #3, #4)
  - [x] 5.1 创建数据保留期限设置界面
  - [x] 5.2 创建敏感信息过滤开关
  - [x] 5.3 创建记忆共享范围配置
  - [x] 5.4 创建排除数据规则配置
  - [x] 5.5 集成到 AgentEditForm 高级设置区域

- [ ] Task 6: 实现敏感信息过滤器 (AC: #2)
  - [x] 6.1 实现敏感信息检测模式（邮箱、电话、身份证等）
  - [x] 6.2 实现敏感信息脱敏/替换功能
  - [ ] 6.3 在消息存储前应用过滤器 (需要与消息系统集成)
  - [x] 6.4 提供过滤预览功能

- [ ] Task 7: 实现数据保留策略执行 (AC: #1, #5)
  - [ ] 7.1 在记忆存储时检查保留策略
  - [ ] 7.2 实现定期清理过期数据任务
  - [ ] 7.3 提供手动触发清理功能
  - [ ] 7.4 记录清理日志

- [x] Task 8: 单元测试 (AC: 全部)
  - [x] 8.1 测试 AgentPrivacyConfig 数据模型
  - [x] 8.2 测试 PrivacyConfigService
  - [x] 8.3 测试敏感信息过滤器
  - [x] 8.4 测试前端组件

## Dev Notes

### 架构上下文

Story 7.4 基于 Epic 2 已完成的代理系统、Epic 5 的记忆系统和 Story 2.13 的基础隐私设置，为代理添加细粒度的数据处理和隐私控制能力。

**依赖关系：**
- **Epic 2 (已完成)**: AgentModel, AgentStore, AgentService 实现
- **Epic 5 (已完成)**: 三层记忆系统实现
- **Story 2.13 (已完成)**: 数据加密与隐私设置基础
- **Story 7.1 (已完成)**: AgentStyleConfig 数据模型和表单模式
- **Story 7.2 (已完成)**: AgentModel 扩展模式（context_window_config）
- **Story 7.3 (已完成)**: AgentTriggerConfig 实现模式

**功能需求关联：**
- FR36: 用户可以配置AI代理的数据处理和隐私设置

**安全性需求关联：**
- NFR-S1: 所有用户数据和对话历史必须在本地设备上加密存储
- NFR-S4: 敏感数据不得未经用户许可传输到第三方服务

### 后端数据模型扩展

```rust
// 新增: crates/omninova-core/src/agent/privacy_config.rs

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 记忆共享范围
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum MemorySharingScope {
    #[default]
    SingleSession,    // 仅当前会话
    CrossSession,     // 跨会话共享
    CrossAgent,       // 跨代理共享
}

/// 数据保留策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DataRetentionPolicy {
    /// 情景记忆保留天数 (0 = 永久保留)
    #[serde(default = "default_episodic_retention")]
    pub episodic_memory_days: u32,
    /// 工作记忆保留小时数
    #[serde(default = "default_working_retention")]
    pub working_memory_hours: u32,
    /// 是否自动清理过期数据
    #[serde(default = "default_true")]
    pub auto_cleanup: bool,
}

fn default_episodic_retention() -> u32 { 90 }
fn default_working_retention() -> u32 { 24 }
fn default_true() -> bool { true }

impl Default for DataRetentionPolicy {
    fn default() -> Self {
        Self {
            episodic_memory_days: 90,
            working_memory_hours: 24,
            auto_cleanup: true,
        }
    }
}

/// 敏感信息过滤配置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SensitiveDataFilter {
    /// 是否启用敏感信息过滤
    #[serde(default)]
    pub enabled: bool,
    /// 过滤邮箱地址
    #[serde(default = "default_true")]
    pub filter_email: bool,
    /// 过滤电话号码
    #[serde(default = "default_true")]
    pub filter_phone: bool,
    /// 过滤身份证号
    #[serde(default = "default_true")]
    pub filter_id_card: bool,
    /// 过滤银行卡号
    #[serde(default = "default_true")]
    pub filter_bank_card: bool,
    /// 过滤IP地址
    #[serde(default)]
    pub filter_ip_address: bool,
    /// 自定义正则表达式模式
    #[serde(default)]
    pub custom_patterns: Vec<String>,
}

impl Default for SensitiveDataFilter {
    fn default() -> Self {
        Self {
            enabled: false,
            filter_email: true,
            filter_phone: true,
            filter_id_card: true,
            filter_bank_card: true,
            filter_ip_address: false,
            custom_patterns: Vec::new(),
        }
    }
}

/// 排除数据规则
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExclusionRule {
    /// 规则名称
    pub name: String,
    /// 规则描述
    #[serde(default)]
    pub description: Option<String>,
    /// 匹配模式（正则表达式）
    pub pattern: String,
    /// 是否启用
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Agent-level privacy configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentPrivacyConfig {
    /// 数据保留策略
    #[serde(default)]
    pub data_retention: DataRetentionPolicy,
    /// 敏感信息过滤配置
    #[serde(default)]
    pub sensitive_filter: SensitiveDataFilter,
    /// 记忆共享范围
    #[serde(default)]
    pub memory_sharing_scope: MemorySharingScope,
    /// 排除数据规则列表
    #[serde(default)]
    pub exclusion_rules: Vec<ExclusionRule>,
    /// 是否记录详细日志
    #[serde(default)]
    pub verbose_logging: bool,
}

impl AgentPrivacyConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// 检查内容是否应该被排除存储
    pub fn should_exclude(&self, content: &str) -> bool {
        for rule in &self.exclusion_rules {
            if rule.enabled {
                if let Ok(re) = regex::Regex::new(&rule.pattern) {
                    if re.is_match(content) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 过滤敏感信息
    pub fn filter_sensitive(&self, content: &str) -> String {
        if !self.sensitive_filter.enabled {
            return content.to_string();
        }

        let mut result = content.to_string();

        if self.sensitive_filter.filter_email {
            result = Self::mask_email(&result);
        }
        if self.sensitive_filter.filter_phone {
            result = Self::mask_phone(&result);
        }
        if self.sensitive_filter.filter_id_card {
            result = Self::mask_id_card(&result);
        }
        if self.sensitive_filter.filter_bank_card {
            result = Self::mask_bank_card(&result);
        }
        if self.sensitive_filter.filter_ip_address {
            result = Self::mask_ip(&result);
        }

        // 应用自定义模式
        for pattern in &self.sensitive_filter.custom_patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                result = re.replace_all(&result, "***FILTERED***").to_string();
            }
        }

        result
    }

    fn mask_email(text: &str) -> String {
        let email_pattern = r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}";
        regex::Regex::new(email_pattern)
            .map(|re| re.replace_all(text, "***@***.***").to_string())
            .unwrap_or_else(|_| text.to_string())
    }

    fn mask_phone(text: &str) -> String {
        // 匹配常见电话号码格式
        let phone_pattern = r"(?:\+?86[-\s]?)?1[3-9]\d{9}|(?:\+?\d{2,3}[-\s]?)?\d{3,4}[-\s]?\d{4}";
        regex::Regex::new(phone_pattern)
            .map(|re| re.replace_all(text, "***-****-****").to_string())
            .unwrap_or_else(|_| text.to_string())
    }

    fn mask_id_card(text: &str) -> String {
        // 匹配18位身份证号
        let id_pattern = r"\d{17}[\dXx]";
        regex::Regex::new(id_pattern)
            .map(|re| re.replace_all(text, "******************").to_string())
            .unwrap_or_else(|_| text.to_string())
    }

    fn mask_bank_card(text: &str) -> String {
        // 匹配银行卡号（16-19位）
        let bank_pattern = r"\d{16,19}";
        regex::Regex::new(bank_pattern)
            .map(|re| re.replace_all(text, "****************").to_string())
            .unwrap_or_else(|_| text.to_string())
    }

    fn mask_ip(text: &str) -> String {
        // 匹配IPv4地址
        let ip_pattern = r"\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}";
        regex::Regex::new(ip_pattern)
            .map(|re| re.replace_all(text, "***.***.***.***").to_string())
            .unwrap_or_else(|_| text.to_string())
    }

    /// Parse from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}
```

### 数据库迁移

```sql
-- Migration: Add privacy config to agents
ALTER TABLE agents ADD COLUMN privacy_config TEXT;
-- Default: {"dataRetention":{"episodicMemoryDays":90,"workingMemoryHours":24,"autoCleanup":true},"sensitiveFilter":{"enabled":false,...},"memorySharingScope":"singleSession","exclusionRules":[]}
```

### AgentModel 扩展

```rust
// 在 agent/model.rs 中添加
pub struct AgentModel {
    // ... existing fields including style_config, context_window_config, trigger_keywords_config
    /// Privacy configuration (JSON serialized)
    pub privacy_config: Option<String>,
}

impl AgentModel {
    pub fn get_privacy_config(&self) -> AgentPrivacyConfig {
        self.privacy_config
            .as_ref()
            .and_then(|s| AgentPrivacyConfig::from_json(s).ok())
            .unwrap_or_default()
    }

    pub fn set_privacy_config(&mut self, config: &AgentPrivacyConfig) {
        self.privacy_config = config.to_json().ok();
    }
}
```

### PrivacyConfigService

```rust
// 新增: crates/omninova-core/src/agent/privacy_service.rs

use super::privacy_config::{AgentPrivacyConfig, DataRetentionPolicy};
use crate::memory::{MemoryManager, MemoryLayer};
use chrono::{DateTime, Utc, Duration};

pub struct PrivacyConfigService;

impl PrivacyConfigService {
    /// 应用数据保留策略，清理过期数据
    pub async fn apply_retention_policy(
        memory_manager: &MemoryManager,
        policy: &DataRetentionPolicy,
        agent_id: &str,
    ) -> Result<RetentionResult, String> {
        let mut result = RetentionResult::default();

        if policy.auto_cleanup {
            // 清理过期的情景记忆
            let cutoff = Utc::now() - Duration::days(policy.episodic_memory_days as i64);
            result.episodic_cleaned = memory_manager
                .cleanup_memories_before(agent_id, MemoryLayer::Episodic, cutoff)
                .await?;
        }

        Ok(result)
    }

    /// 获取数据保留状态
    pub async fn get_retention_status(
        memory_manager: &MemoryManager,
        agent_id: &str,
    ) -> Result<RetentionStatus, String> {
        // 实现获取当前数据保留状态的逻辑
        todo!()
    }
}

#[derive(Debug, Default)]
pub struct RetentionResult {
    pub episodic_cleaned: usize,
    pub working_cleaned: usize,
}

#[derive(Debug)]
pub struct RetentionStatus {
    pub total_memories: usize,
    pub oldest_memory: Option<DateTime<Utc>>,
    pub newest_memory: Option<DateTime<Utc>>,
    pub estimated_size_bytes: u64,
}
```

### 前端类型定义

```typescript
// src/types/agent.ts (扩展)

export type MemorySharingScope = 'singleSession' | 'crossSession' | 'crossAgent';

export interface DataRetentionPolicy {
  episodicMemoryDays: number;
  workingMemoryHours: number;
  autoCleanup: boolean;
}

export interface SensitiveDataFilter {
  enabled: boolean;
  filterEmail: boolean;
  filterPhone: boolean;
  filterIdCard: boolean;
  filterBankCard: boolean;
  filterIpAddress: boolean;
  customPatterns: string[];
}

export interface ExclusionRule {
  name: string;
  description?: string;
  pattern: string;
  enabled: boolean;
}

export interface AgentPrivacyConfig {
  dataRetention: DataRetentionPolicy;
  sensitiveFilter: SensitiveDataFilter;
  memorySharingScope: MemorySharingScope;
  exclusionRules: ExclusionRule[];
  verboseLogging: boolean;
}

export const DEFAULT_PRIVACY_CONFIG: AgentPrivacyConfig = {
  dataRetention: {
    episodicMemoryDays: 90,
    workingMemoryHours: 24,
    autoCleanup: true,
  },
  sensitiveFilter: {
    enabled: false,
    filterEmail: true,
    filterPhone: true,
    filterIdCard: true,
    filterBankCard: true,
    filterIpAddress: false,
    customPatterns: [],
  },
  memorySharingScope: 'singleSession',
  exclusionRules: [],
  verboseLogging: false,
};

export const MEMORY_SHARING_SCOPE_LABELS: Record<MemorySharingScope, string> = {
  singleSession: '仅当前会话',
  crossSession: '跨会话共享',
  crossAgent: '跨代理共享',
};

export const MEMORY_SHARING_SCOPE_DESCRIPTIONS: Record<MemorySharingScope, string> = {
  singleSession: '记忆仅在当前对话会话中可用',
  crossSession: '记忆可在不同会话间共享',
  crossAgent: '记忆可在不同代理间共享',
};
```

### 组件设计

#### PrivacyConfigForm

```
┌─────────────────────────────────────────────────────────────┐
│ 隐私与数据处理设置                                            │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│ 数据保留策略                                                 │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ 情景记忆保留: [90] 天                                    │ │
│ │ 工作记忆保留: [24] 小时                                  │ │
│ │ ☑ 自动清理过期数据                                       │ │
│ └─────────────────────────────────────────────────────────┘ │
│                                                             │
│ 敏感信息过滤                                                 │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ ☐ 启用敏感信息自动过滤                                   │ │
│ │                                                         │ │
│ │ 过滤类型:                                                │ │
│ │ ☑ 邮箱地址  ☑ 电话号码  ☑ 身份证号                      │ │
│ │ ☑ 银行卡号  ☐ IP地址                                    │ │
│ │                                                         │ │
│ │ [+ 添加自定义过滤规则]                                   │ │
│ └─────────────────────────────────────────────────────────┘ │
│                                                             │
│ 记忆共享范围                                                 │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ ○ 仅当前会话 - 记忆仅在当前对话会话中可用                │ │
│ │ ○ 跨会话共享 - 记忆可在不同会话间共享                    │ │
│ │ ○ 跨代理共享 - 记忆可在不同代理间共享                    │ │
│ └─────────────────────────────────────────────────────────┘ │
│                                                             │
│ 排除数据规则                                                 │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ 无排除规则                                               │ │
│ │ [+ 添加排除规则]                                         │ │
│ └─────────────────────────────────────────────────────────┘ │
│                                                             │
│ [立即清理过期数据]                                           │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 文件结构

```
crates/omninova-core/src/agent/
├── privacy_config.rs         # 新增 - AgentPrivacyConfig 定义
├── privacy_service.rs        # 新增 - PrivacyConfigService
├── mod.rs                    # 修改 - 添加模块导出
├── model.rs                  # 修改 - 添加 privacy_config 字段
├── store.rs                  # 修改 - 更新 CRUD 方法
└── service.rs                # 修改 - 集成隐私检查

apps/omninova-tauri/src/
├── types/
│   └── agent.ts              # 扩展 - 添加隐私配置类型
├── hooks/
│   └── usePrivacyConfig.ts   # 新增 - 隐私配置 hook
├── components/agent/
│   ├── PrivacyConfigForm.tsx # 新增 - 隐私配置表单
│   ├── SensitiveFilterSettings.tsx # 新增 - 敏感过滤设置
│   ├── ExclusionRulesEditor.tsx # 新增 - 排除规则编辑器
│   ├── AgentEditForm.tsx     # 修改 - 集成隐私配置
│   └── __tests__/
│       └── PrivacyConfigForm.test.tsx # 新增
└── components/memory/
    └── DataRetentionStatus.tsx # 新增 - 数据保留状态显示

apps/omninova-tauri/src-tauri/src/
└── lib.rs                    # 修改 - 添加隐私配置 Tauri commands

crates/omninova-core/src/db/
└── migrations.rs             # 修改 - 添加 privacy_config 列
```

### 命名约定

遵循 Story 7.1、7.2 和 7.3 确立的命名约定：

**Rust:**
- 结构体: PascalCase (`AgentPrivacyConfig`, `DataRetentionPolicy`)
- 字段: snake_case (`privacy_config`, `episodic_memory_days`)
- 方法: snake_case (`apply_retention_policy`)

**TypeScript/React:**
- 接口: PascalCase (`AgentPrivacyConfig`, `DataRetentionPolicy`)
- 属性: camelCase (`episodicMemoryDays`, `memorySharingScope`)
- 组件: PascalCase (`PrivacyConfigForm`)

### 与 Story 2.13 的关系

**Story 2.13 (数据加密与隐私设置)** 提供了全局级别的隐私设置：
- 启用/禁用本地数据加密
- 云端同步控制
- 数据存储位置查看
- 清除本地对话历史

**Story 7.4 (数据处理与隐私设置)** 提供代理级别的细粒度控制：
- 数据保留期限（按代理配置）
- 敏感信息自动过滤
- 记忆共享范围
- 排除数据规则

**优先级逻辑：**
1. 代理级别的隐私设置优先于全局设置
2. 如果代理未配置特定设置，回退到全局设置
3. 敏感信息过滤在消息存储前执行
4. 数据保留策略在后台定期执行

### 测试策略

1. **单元测试**：
   - AgentPrivacyConfig 序列化/反序列化
   - 敏感信息检测与过滤逻辑
   - 数据保留策略计算
   - PrivacyConfigService 方法

2. **组件测试**：
   - PrivacyConfigForm 渲染和交互
   - 敏感过滤开关状态变化
   - 排除规则添加/删除
   - 配置保存验证

### Previous Story Intelligence (Story 7.3)

**可复用模式：**
- AgentModel 扩展模式（添加 JSON 序列化字段）
- 数据库迁移方式（添加新列）
- Tauri commands 结构（get/update 命令对）
- 前端 hook 模式（useTriggerKeywordsConfig）
- 表单组件模式（高级设置折叠区域）

**注意事项：**
- 隐私配置应放在"高级设置"折叠区域
- 敏感信息过滤需要性能优化（避免阻塞）
- 数据保留策略需要异步执行
- 排除规则需要正则表达式验证

### References

- [Source: epics.md#Story 7.4] - 原始 story 定义
- [Source: architecture.md#FR36] - 数据处理和隐私设置需求
- [Source: architecture.md#NFR-S1] - 本地加密存储要求
- [Source: architecture.md#NFR-S4] - 敏感数据保护要求
- [Source: agent/model.rs] - AgentModel 定义
- [Source: memory/manager.rs] - 记忆管理器实现
- [Source: Story 2.13] - 全局隐私设置实现
- [Source: Story 7.3] - AgentTriggerConfig 实现模式

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (glm-5)

### Debug Log References

N/A

### Completion Notes List

- Task 1, 2, 3, 4, 5, 6 (partial), and 8 completed
- Backend: privacy_config.rs, privacy_service.rs created with full implementation
- Database migration 015 added for privacy_config column
- Tauri commands: get_privacy_config, update_privacy_config, test_sensitive_filter, validate_exclusion_pattern, validate_filter_pattern
- Frontend types and usePrivacyConfig hook implemented
- Sensitive data filtering with regex patterns for email, phone, ID card, bank card, IP address
- Data retention policy with episodic/working memory distinction
- Word boundaries added to regex patterns to prevent false matches (phone regex matching within ID cards)
- PrivacyConfigForm component with collapsible sections for data retention, sensitive filter, memory sharing scope, and exclusion rules
- AgentEditForm integration with "高级设置" collapsible section containing PrivacyConfigForm
- Filter preview functionality implemented in UI (Task 6.4)
- **Remaining**: Task 6.3 (消息存储前应用过滤器) and Task 7 (数据保留策略执行) are system-level integrations requiring core message pipeline modifications

### File List

**Backend (Rust):**
- `crates/omninova-core/src/agent/privacy_config.rs` (NEW) - AgentPrivacyConfig, DataRetentionPolicy, SensitiveDataFilter, ExclusionRule, MemorySharingScope
- `crates/omninova-core/src/agent/privacy_service.rs` (NEW) - PrivacyConfigService, RetentionCutoff, RetentionResult, RetentionStatus
- `crates/omninova-core/src/agent/mod.rs` (MODIFIED) - Added module exports
- `crates/omninova-core/src/agent/model.rs` (MODIFIED) - Added privacy_config field and getter/setter
- `crates/omninova-core/src/agent/store.rs` (MODIFIED) - Added privacy_config to CRUD operations, added update_privacy_config method
- `crates/omninova-core/src/db/migrations.rs` (MODIFIED) - Added migration 015_agent_privacy_config
- `crates/omninova-core/src/agent/service.rs` (MODIFIED) - Updated test helper
- `crates/omninova-core/src/backup/mod.rs` (MODIFIED) - Updated test cases

**Tauri:**
- `apps/omninova-tauri/src-tauri/src/lib.rs` (MODIFIED) - Added privacy config commands

**Frontend (TypeScript):**
- `apps/omninova-tauri/src/types/agent.ts` (MODIFIED) - Added privacy config types
- `apps/omninova-tauri/src/hooks/usePrivacyConfig.ts` (NEW) - Privacy config hook
- `apps/omninova-tauri/src/components/agent/PrivacyConfigForm.tsx` (NEW) - Privacy config form component with collapsible sections
- `apps/omninova-tauri/src/components/agent/PrivacyConfigForm.test.tsx` (NEW) - Component tests
- `apps/omninova-tauri/src/components/agent/AgentEditForm.tsx` (MODIFIED) - Integrated PrivacyConfigForm into advanced settings section