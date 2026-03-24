# Story 6.6: 渠道行为配置

Status: done

## Story

As a 用户,
I want 为不同渠道配置不同的代理行为,
So that 代理可以根据渠道特点调整响应方式.

## Acceptance Criteria

1. **AC1: 响应风格配置** - 可以为每个渠道设置不同的响应风格（正式/非正式/简洁/详细） ✅
2. **AC2: 触发关键词配置** - 可以配置渠道特定的触发关键词 ✅
3. **AC3: 响应长度限制** - 可以设置响应长度限制（字符数） ✅
4. **AC4: 响应延迟配置** - 可以设置响应延迟（模拟人工输入效果） ✅
5. **AC5: 工作时段配置** - 可以配置工作时段（仅在特定时间响应） ✅
6. **AC6: 行为配置持久化** - 行为配置可以持久化存储 ✅
7. **AC7: 运行时更新** - 支持运行时更新行为配置无需重启渠道 ✅

## Tasks / Subtasks

- [x] Task 1: 定义渠道行为配置数据结构 (AC: #1, #2, #3, #4, #5)
  - [x] 1.1 创建 `ChannelBehaviorConfig` 结构体
  - [x] 1.2 定义 `ResponseStyle` 枚举（Formal, Casual, Concise, Detailed）
  - [x] 1.3 定义 `TriggerKeyword` 结构体（keyword, match_type, case_sensitive）
  - [x] 1.4 定义 `WorkingHours` 结构体（timezone, time_slots, enabled_days）
  - [x] 1.5 实现 Serialize/Deserialize trait

- [x] Task 2: 扩展 ChannelSettings (AC: #6)
  - [x] 2.1 在 `ChannelSettings` 中添加 `behavior: ChannelBehaviorConfig` 字段
  - [x] 2.2 实现 Default trait for ChannelBehaviorConfig
  - [x] 2.3 实现 Builder pattern for ChannelBehaviorConfig
  - [x] 2.4 更新现有测试适配新的 ChannelSettings 结构

- [x] Task 3: 实现响应风格处理 (AC: #1)
  - [x] 3.1 创建 `ResponseStyleProcessor` 处理器
  - [x] 3.2 实现 `apply_style(content: &str, style: ResponseStyle) -> String` 方法
  - [x] 3.3 Formal 风格：正式语言、完整句子、无表情符号
  - [x] 3.4 Casual 风格：轻松语言、可以包含表情符号
  - [x] 3.5 Concise 风格：简短回复、要点列表
  - [x] 3.6 Detailed 风格：详细解释、包含上下文

- [x] Task 4: 实现触发关键词匹配 (AC: #2)
  - [x] 4.1 创建 `TriggerKeywordMatcher` 匹配器
  - [x] 4.2 实现精确匹配模式 (`MatchType::Exact`)
  - [x] 4.3 实现前缀匹配模式 (`MatchType::Prefix`)
  - [x] 4.4 实现正则匹配模式 (`MatchType::Regex`)
  - [x] 4.5 实现大小写敏感/不敏感选项
  - [x] 4.6 实现 `check_triggers(message: &str, keywords: &[TriggerKeyword]) -> bool`

- [x] Task 5: 实现响应长度限制 (AC: #3)
  - [x] 5.1 在发送消息前检查长度限制
  - [x] 5.2 实现消息截断逻辑（按句子边界截断）
  - [x] 5.3 添加省略号或"..."后缀表示截断
  - [x] 5.4 记录截断日志用于监控

- [x] Task 6: 实现响应延迟模拟 (AC: #4)
  - [x] 6.1 定义 `ResponseDelay` 配置结构
  - [x] 6.2 实现固定延迟模式 (`DelayMode::Fixed`)
  - [x] 6.3 实现随机延迟范围 (`DelayMode::Random { min, max }`)
  - [x] 6.4 实现模拟打字延迟 (`DelayMode::Typing { chars_per_second }`)
  - [x] 6.5 在 send_message 流程中集成延迟逻辑

- [x] Task 7: 实现工作时段控制 (AC: #5)
  - [x] 7.1 创建 `WorkingHoursChecker` 检查器
  - [x] 7.2 实现时区支持
  - [x] 7.3 实现时间段匹配逻辑
  - [x] 7.4 实现工作日过滤（周一至周日选择）
  - [x] 7.5 非工作时段消息处理（队列或忽略）
  - [x] 7.6 实现 `is_within_working_hours(config: &WorkingHours) -> bool`

- [x] Task 8: 实现配置持久化 (AC: #6)
  - [x] 8.1 创建数据库迁移脚本添加 behavior_config 表
  - [x] 8.2 实现 `ChannelBehaviorStore` trait
  - [x] 8.3 实现 `save_behavior_config` 方法
  - [x] 8.4 实现 `load_behavior_config` 方法
  - [x] 8.5 在 ChannelManager 中集成行为配置加载

- [x] Task 9: 实现运行时配置更新 (AC: #7)
  - [x] 9.1 添加 `update_behavior_config` 方法到 ChannelManager
  - [x] 9.2 实现配置变更事件通知
  - [x] 9.3 在 Channel trait 中添加 `on_behavior_changed` 回调
  - [x] 9.4 确保运行时更新不影响正在进行的消息处理

- [x] Task 10: 单元测试 (AC: 全部)
  - [x] 10.1 测试 ResponseStyle 处理器
  - [x] 10.2 测试 TriggerKeywordMatcher 匹配逻辑
  - [x] 10.3 测试响应长度截断
  - [x] 10.4 测试响应延迟逻辑
  - [x] 10.5 测试工作时段检查
  - [x] 10.6 测试配置序列化/反序列化
  - [x] 10.7 测试运行时配置更新

## Dev Notes

### 架构上下文

Story 6.6 基于 Epic 6 已完成的基础设施（Story 6.1-6.5），实现渠道行为配置功能。

**依赖关系：**
- **Story 6.1 (已完成)**: `ChannelSettings` 基础结构
- **Story 6.2 (已完成)**: `ChannelManager` 管理渠道实例
- **Story 6.3-6.5 (已完成)**: Slack/Discord/Email 适配器实现模式

**功能需求关联：**
- FR30: 用户可以配置AI代理在不同渠道的行为差异

### 数据结构设计

```rust
/// 渠道行为配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelBehaviorConfig {
    /// 响应风格
    pub response_style: ResponseStyle,

    /// 触发关键词列表
    #[serde(default)]
    pub trigger_keywords: Vec<TriggerKeyword>,

    /// 响应长度限制（0 = 无限制）
    #[serde(default)]
    pub max_response_length: usize,

    /// 响应延迟配置
    #[serde(default)]
    pub response_delay: ResponseDelay,

    /// 工作时段配置
    #[serde(default)]
    pub working_hours: Option<WorkingHours>,
}

/// 响应风格
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStyle {
    /// 正式风格
    Formal,
    /// 非正式/轻松风格
    Casual,
    /// 简洁风格
    Concise,
    /// 详细风格
    #[default]
    Detailed,
}

/// 触发关键词
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerKeyword {
    /// 关键词内容
    pub keyword: String,
    /// 匹配类型
    #[serde(default)]
    pub match_type: MatchType,
    /// 是否大小写敏感
    #[serde(default)]
    pub case_sensitive: bool,
}

/// 匹配类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MatchType {
    /// 精确匹配
    #[default]
    Exact,
    /// 前缀匹配
    Prefix,
    /// 包含匹配
    Contains,
    /// 正则匹配
    Regex,
}

/// 响应延迟配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ResponseDelay {
    /// 无延迟
    None,
    /// 固定延迟（毫秒）
    Fixed { delay_ms: u64 },
    /// 随机延迟范围
    Random { min_ms: u64, max_ms: u64 },
    /// 模拟打字
    Typing { chars_per_second: f64 },
}

impl Default for ResponseDelay {
    fn default() -> Self {
        Self::None
    }
}

/// 工作时段配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingHours {
    /// 时区（如 "Asia/Shanghai"）
    #[serde(default = "default_timezone")]
    pub timezone: String,

    /// 时间段列表
    pub time_slots: Vec<TimeSlot>,

    /// 启用的星期（1=周一, 7=周日）
    #[serde(default = "default_workdays")]
    pub enabled_days: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSlot {
    /// 开始时间 "HH:MM"
    pub start: String,
    /// 结束时间 "HH:MM"
    pub end: String,
}

fn default_timezone() -> String {
    "UTC".to_string()
}

fn default_workdays() -> Vec<u8> {
    vec![1, 2, 3, 4, 5] // 周一到周五
}
```

### 文件结构

```
crates/omninova-core/src/channels/
├── behavior/                  # 新增目录
│   ├── mod.rs                # 模块导出
│   ├── config.rs             # ChannelBehaviorConfig 定义
│   ├── style.rs              # ResponseStyle 处理器
│   ├── trigger.rs            # TriggerKeywordMatcher 实现
│   ├── delay.rs              # ResponseDelay 实现
│   └── working_hours.rs      # WorkingHoursChecker 实现
├── traits.rs                  # 修改 - 扩展 ChannelSettings
├── event.rs                   # 修改 - 添加 BehaviorChanged 事件
└── manager.rs                 # 修改 - 集成行为配置

apps/omninova-tauri/src-tauri/src/
└── lib.rs                     # 修改 - 添加 Tauri commands
```

### 数据库迁移

```sql
-- 新增 behavior_config 表
CREATE TABLE IF NOT EXISTS channel_behavior_config (
    channel_id TEXT PRIMARY KEY,
    config TEXT NOT NULL,  -- JSON 序列化的 ChannelBehaviorConfig
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT DEFAULT CURRENT_TIMESTAMP
);
```

### Tauri Commands

```rust
#[tauri::command]
async fn get_channel_behavior_config(channel_id: String) -> Result<ChannelBehaviorConfig, String>;

#[tauri::command]
async fn update_channel_behavior_config(
    channel_id: String,
    config: ChannelBehaviorConfig,
) -> Result<(), String>;
```

### 测试策略

1. **单元测试**：
   - 使用 mock 数据测试各处理器
   - 测试边界条件（空输入、极限值）

2. **集成测试**：
   - 测试完整的行为配置流程
   - 测试与现有渠道的集成

### Previous Story Intelligence (Story 6.5)

**学习要点：**
1. 使用 `serde` 的 `#[serde(default)]` 提供默认值
2. Builder pattern 用于复杂配置结构
3. 数据库迁移使用 `src/db/migrations.rs`
4. Tauri commands 使用 `#[tauri::command]` 宏

**可复用模式：**
- 配置结构体的 Builder pattern
- 错误处理使用 `thiserror`
- 异步方法使用 `async_trait`
- 测试覆盖核心逻辑

### 命名约定

遵循项目 Rust 命名约定：
- **函数**: snake_case (`apply_style`, `check_triggers`)
- **结构体**: PascalCase (`ChannelBehaviorConfig`, `TriggerKeyword`)
- **枚举**: PascalCase (`ResponseStyle`, `MatchType`)
- **常量**: SCREAMING_SNAKE_CASE (`DEFAULT_TIMEZONE`)

### References

- [Source: epics.md#Story 6.6] - 原始 story 定义
- [Source: architecture.md#channels] - 渠道模块架构设计
- [Source: architecture.md#FR30] - 渠道行为差异配置需求
- [Source: channels/traits.rs] - ChannelSettings 现有实现
- [Source: Story 6.1-6.5] - 渠道适配器实现模式参考

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

- Fixed lifetime parameters in `trigger.rs` for `find_matching` and `find_first` functions
- Fixed DateTime type annotations in `working_hours.rs` tests
- Fixed `chrono_tz::Tz` parsing to use `str.parse::<chrono_tz::Tz>()` instead of `from_str()`
- Added `DatabaseError` variant to `ChannelError` enum
- Fixed `MessageContent.len()` to use `text_content().map(|t| t.len()).unwrap_or(0)`

### Completion Notes List

1. **Task 1 Complete**: Created all core data structures in `crates/omninova-core/src/channels/behavior/`:
   - `config.rs` - `ChannelBehaviorConfig` with builder pattern
   - `style.rs` - `ResponseStyle` enum and `ResponseStyleProcessor`
   - `trigger.rs` - `TriggerKeyword`, `MatchType`, and `TriggerKeywordMatcher`
   - `delay.rs` - `ResponseDelay` and `TypingDelay`
   - `working_hours.rs` - `WorkingHours`, `TimeSlot`, and `WorkingHoursChecker`

2. **Task 2 Complete**: Extended `ChannelSettings` in `traits.rs` with `behavior: ChannelBehaviorConfig` field

3. **Task 6.5 Complete**: Integrated delay logic in `send_to_channel` method - applies `ResponseDelay.calculate_delay()` before sending

4. **Task 7.5 Complete**: Implemented `send_to_channel_queued` method that queues messages outside working hours

5. **Task 8 Complete**:
   - Added migration `011_channel_behavior_config`
   - Implemented `SqliteBehaviorStore`
   - Integrated behavior config loading in `ChannelManager`

6. **Task 9 Complete**: Runtime configuration updates
   - Added `update_behavior_config` method to `ChannelManager`
   - Added `BehaviorChanged` event to `ChannelEvent` enum
   - Added `on_behavior_changed` callback to `Channel` trait
   - Runtime updates broadcast events without interrupting ongoing operations

7. **Task 10.7 Complete**: Added tests for runtime config updates in `manager.rs`:
   - `test_get_behavior_config_default`
   - `test_get_behavior_config_not_found`
   - `test_update_behavior_config`
   - `test_update_behavior_config_not_found`
   - `test_update_behavior_config_broadcasts_event`
   - `test_is_within_working_hours_default`
   - `test_is_within_working_hours_nonexistent`

### File List

**Created Files:**
- `crates/omninova-core/src/channels/behavior/mod.rs`
- `crates/omninova-core/src/channels/behavior/config.rs`
- `crates/omninova-core/src/channels/behavior/style.rs`
- `crates/omninova-core/src/channels/behavior/trigger.rs`
- `crates/omninova-core/src/channels/behavior/delay.rs`
- `crates/omninova-core/src/channels/behavior/working_hours.rs`

**Modified Files:**
- `crates/omninova-core/Cargo.toml` - Added `chrono-tz` and `regex` dependencies
- `crates/omninova-core/src/channels/mod.rs` - Added `pub mod behavior;`
- `crates/omninova-core/src/db/migrations.rs` - Added migration `011_channel_behavior_config`

**Files with Story 6.6 Additions (currently untracked in git):**
- `crates/omninova-core/src/channels/traits.rs` - Added `behavior` field to `ChannelSettings`, added `on_behavior_changed` callback
- `crates/omninova-core/src/channels/error.rs` - Added `DatabaseError` variant
- `crates/omninova-core/src/channels/event.rs` - Added `BehaviorChanged` event
- `crates/omninova-core/src/channels/manager.rs` - Integrated behavior config, added delay/working hours checks, runtime update methods
- `crates/omninova-core/src/channels/types.rs` - Channel types (from earlier stories)

## Change Log

- 2026-03-22: Completed Story 6.6 implementation
  - All acceptance criteria met
  - 747 tests passing
  - Ready for code review