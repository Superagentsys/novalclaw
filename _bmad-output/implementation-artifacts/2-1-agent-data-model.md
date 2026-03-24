# Story 2.1: Agent 数据模型与数据库 Schema

Status: done

## Story

As a 用户,
I want 系统能够存储和管理 AI 代理的数据,
so that 我创建的代理可以被持久化保存并在后续使用中访问.

## Acceptance Criteria

1. **Given** SQLite 数据库迁移系统已建立, **When** 我运行 Agent schema 迁移, **Then** agents 表已创建，包含字段：id (UUID), name, description, domain, mbti_type, status, created_at, updated_at

2. **Given** agents 表已存在, **When** 我定义 Rust 结构体, **Then** Agent 结构体已在 Rust 中定义并实现 Serialize/Deserialize

3. **Given** Agent 结构体已定义, **When** 我实现数据访问层, **Then** 基础 CRUD 操作函数已实现（create, read, update, delete, list）

4. **Given** CRUD 操作已实现, **When** 我创建 Tauri 命令, **Then** Tauri commands 已暴露这些操作给前端

## Tasks / Subtasks

- [x] Task 1: 创建 Agent 数据库迁移 (AC: 1)
  - [x] 创建新迁移 `002_agent_enhancements` 在 `migrations.rs`
  - [x] 添加 `agent_uuid` 列 (TEXT, 可为 NULL 兼容现有数据)
  - [x] 添加 `domain` 列 (TEXT)
  - [x] 重命名 `is_active` 为 `status` (TEXT: 'active'/'inactive'/'archived')
  - [x] 创建迁移的 down_sql 用于回滚
  - [x] 添加单元测试验证迁移正确执行

- [x] Task 2: 定义 Agent 数据模型结构体 (AC: 2)
  - [x] 创建 `crates/omninova-core/src/agent/model.rs` 模块
  - [x] 定义 `AgentStatus` 枚举 (Active, Inactive, Archived)
  - [x] 定义 `AgentModel` 结构体，包含所有数据库字段
  - [x] 实现 `Serialize`/`Deserialize` for JSON 序列化
  - [x] 实现 `FromRow` trait 用于 rusqlite 映射
  - [x] 更新 `agent/mod.rs` 导出新模块

- [x] Task 3: 实现 Agent CRUD 操作 (AC: 3)
  - [x] 创建 `crates/omninova-core/src/agent/store.rs` 模块
  - [x] 实现 `AgentStore` 结构体，持有 `DbPool`
  - [x] 实现 `AgentStore::create(agent: &NewAgent) -> Result<AgentModel>`
  - [x] 实现 `AgentStore::find_by_id(id: &str) -> Result<Option<AgentModel>>`
  - [x] 实现 `AgentStore::find_all() -> Result<Vec<AgentModel>>`
  - [x] 实现 `AgentStore::update(id: &str, updates: &AgentUpdate) -> Result<AgentModel>`
  - [x] 实现 `AgentStore::delete(id: &str) -> Result<()>`
  - [x] 添加单元测试覆盖所有 CRUD 操作

- [x] Task 4: 暴露 Tauri Commands (AC: 4)
  - [x] 在 `lib.rs` 中实现 `get_agents` 命令
  - [x] 实现 `create_agent` 命令
  - [x] 实现 `update_agent` 命令
  - [x] 实现 `delete_agent` 命令
  - [x] 实现 `get_agent_by_id` 命令
  - [x] 注册所有命令到 `invoke_handler`
  - [x] 添加状态管理，使用 `State<'_, Arc<AgentStore>>`

- [x] Task 5: 集成测试和验证 (AC: All)
  - [x] 编写集成测试验证完整流程
  - [x] 验证迁移可重复执行不报错
  - [x] 运行 `cargo test` 确保所有测试通过
  - [x] 验证 Tauri 命令可通过前端调用

## Dev Notes

### 现有架构发现

**重要**: 现有 `agents` 表已在初始迁移中创建，结构如下:

```sql
CREATE TABLE IF NOT EXISTS agents (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    description TEXT,
    mbti_type TEXT,
    system_prompt TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);
```

**差异分析**:

| 字段 | 现有 | Story 要求 | 处理方式 |
|------|------|-----------|----------|
| id | INTEGER AUTOINCREMENT | UUID | 新增 agent_uuid 列 |
| domain | 无 | 需要 | 新增列 |
| status | is_active (INTEGER) | status (TEXT) | 重命名+类型转换 |
| system_prompt | 有 | 未提及 | 保留 |

### 推荐的迁移策略

```sql
-- Migration 002_agent_enhancements
ALTER TABLE agents ADD COLUMN agent_uuid TEXT;
ALTER TABLE agents ADD COLUMN domain TEXT;
ALTER TABLE agents ADD COLUMN status TEXT;

-- 迁移现有数据
UPDATE agents SET
    agent_uuid = lower(hex(randomblob(16))),
    status = CASE WHEN is_active = 1 THEN 'active' ELSE 'inactive' END;

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_agents_uuid ON agents(agent_uuid);
CREATE INDEX IF NOT EXISTS idx_agents_status ON agents(status);
```

### 项目架构约束

- **工作目录**: Rust 后端代码在 `crates/omninova-core/` 目录下
- **Tauri 入口**: `apps/omninova-tauri/src-tauri/src/` 调用 core 模块
- **数据库位置**: 通过 `resolve_config_dir()` 获取配置目录下的 SQLite 文件

### 已有模块可复用

**迁移系统** (`db/migrations.rs`):
```rust
// 创建迁移
Migration::new("002_agent_enhancements", "Add agent UUID and domain fields")
    .up(MIGRATION_SQL)
    .down(DOWN_SQL)

// 运行迁移
let runner = create_builtin_runner();
runner.run(&conn)?;
```

**连接池** (`db/pool.rs`):
```rust
let pool = create_pool(&config)?;
let conn = pool.get()?;
```

### AgentModel 设计

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentStatus {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "inactive")]
    Inactive,
    #[serde(rename = "archived")]
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentModel {
    pub id: i64,                    // 数据库自增 ID
    pub agent_uuid: String,         // UUID 字符串
    pub name: String,
    pub description: Option<String>,
    pub domain: Option<String>,
    pub mbti_type: Option<String>,
    pub system_prompt: Option<String>,
    pub status: AgentStatus,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewAgent {
    pub name: String,
    pub description: Option<String>,
    pub domain: Option<String>,
    pub mbti_type: Option<String>,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub domain: Option<String>,
    pub mbti_type: Option<String>,
    pub system_prompt: Option<String>,
    pub status: Option<AgentStatus>,
}
```

### AgentStore 设计

```rust
use crate::db::DbPool;
use anyhow::Result;

pub struct AgentStore {
    pool: DbPool,
}

impl AgentStore {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub fn create(&self, agent: &NewAgent) -> Result<AgentModel> { ... }
    pub fn find_by_uuid(&self, uuid: &str) -> Result<Option<AgentModel>> { ... }
    pub fn find_all(&self) -> Result<Vec<AgentModel>> { ... }
    pub fn update(&self, uuid: &str, updates: &AgentUpdate) -> Result<AgentModel> { ... }
    pub fn delete(&self, uuid: &str) -> Result<()> { ... }
}
```

### Tauri 命令设计

```rust
#[tauri::command]
async fn get_agents(store: State<'_, Arc<AgentStore>>) -> Result<String, String> {
    let agents = store.find_all().map_err(|e| e.to_string())?;
    serde_json::to_string(&agents).map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_agent(
    store: State<'_, Arc<AgentStore>>,
    agent_json: String,
) -> Result<String, String> {
    let agent: NewAgent = serde_json::from_str(&agent_json).map_err(|e| e.to_string())?;
    let created = store.create(&agent).map_err(|e| e.to_string())?;
    serde_json::to_string(&created).map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_agent(
    store: State<'_, Arc<AgentStore>>,
    uuid: String,
    updates_json: String,
) -> Result<String, String> {
    let updates: AgentUpdate = serde_json::from_str(&updates_json).map_err(|e| e.to_string())?;
    let updated = store.update(&uuid, &updates).map_err(|e| e.to_string())?;
    serde_json::to_string(&updated).map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_agent(
    store: State<'_, Arc<AgentStore>>,
    uuid: String,
) -> Result<(), String> {
    store.delete(&uuid).map_err(|e| e.to_string())
}
```

### 命名约定

遵循 architecture.md 约定:
- **数据库**: snake_case (agents, agent_uuid, mbti_type)
- **Rust**: snake_case (agent_uuid, mbti_type)
- **JSON/API**: camelCase (agentUuid, mbtiType)
- **Tauri Commands**: camelCase (getAgents, createAgent)

### 错误处理模式

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Agent not found: {0}")]
    NotFound(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Invalid UUID: {0}")]
    InvalidUuid(String),
}
```

### 测试策略

**单元测试**:
- 使用 `Connection::open_in_memory()` 创建测试数据库
- 测试迁移执行和回滚
- 测试 CRUD 操作正确性
- 测试边界条件（空列表、不存在的 UUID）

**测试辅助**:
```rust
fn create_test_store() -> AgentStore {
    let conn = Connection::open_in_memory().unwrap();
    let runner = create_builtin_runner();
    runner.run(&conn).unwrap();
    // 创建测试用的 pool 包装器...
}
```

### 项目目录结构（更新后）

**新建文件:**
```
crates/omninova-core/src/agent/
├── model.rs        # AgentModel, AgentStatus, NewAgent, AgentUpdate
└── store.rs        # AgentStore CRUD 操作
```

**修改文件:**
```
crates/omninova-core/
├── Cargo.toml                   # 添加 uuid crate (如果需要)
└── src/
    ├── agent/mod.rs             # 导出 model, store 模块
    └── db/migrations.rs         # 添加 002_agent_enhancements 迁移

apps/omninova-tauri/src-tauri/
└── src/lib.rs                   # 添加 Agent 相关 Tauri 命令
```

### 前一个故事的发现 (Story 1.6)

1. **Tauri 2.x 兼容**: 需要导入 `tauri::Emitter` trait
2. **共享状态模式**: 使用 `Arc<T>` 和 `State<'_, Arc<T>>`
3. **测试验证**: 使用 `cargo test` 确保所有测试通过
4. **编译验证**: 确保无编译警告

### Dependencies

可能需要添加到 `Cargo.toml`:
```toml
[dependencies]
uuid = { version = "1", features = ["v4", "serde"] }
```

### References

- [Source: db/migrations.rs] - 现有迁移系统和 agents 表定义
- [Source: db/pool.rs] - 连接池实现
- [Source: agent/agent.rs] - 现有 Agent 运行时结构体
- [Source: config/schema.rs] - AgentConfig 配置定义
- [Source: architecture.md#数据库架构] - 数据库设计原则
- [Source: architecture.md#命名模式] - 命名约定
- [Source: epics.md#Story 2.1] - 验收标准
- [Source: 1-6-config-hot-reload.md] - 前一个故事的实现记录

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

N/A - 无重大调试问题

### Completion Notes List

1. **迁移系统增强**: 在 `db/migrations.rs` 中添加了 `002_agent_enhancements` 迁移，包含 up/down SQL 和 5 个新测试
2. **AgentModel 实现**: 创建了完整的 `model.rs` 模块，包含 `AgentStatus`、`AgentModel`、`NewAgent`、`AgentUpdate` 结构体
3. **AgentStore CRUD**: 实现了完整的 CRUD 操作，使用 `DbPool` 连接池，包含 9 个单元测试
4. **Tauri Commands**: 添加了 6 个命令 (`init_agent_store`, `get_agents`, `get_agent_by_id`, `create_agent`, `update_agent`, `delete_agent`)
5. **测试验证**: 所有 90 个测试通过，无编译警告

### Code Review Fixes (2026-03-16)

**代码审查发现并修复的问题:**

1. **[HIGH] UUID 格式不一致**: 迁移 SQL 使用 `hex(randomblob(16))` 生成 32 字符无连字符格式，而 Rust 使用标准 UUID 格式 (8-4-4-4-12)。修复：更新迁移 SQL 使用标准 UUID 格式生成。

2. **[MEDIUM] FromRow 错误转换重复**: 三处相同的错误转换代码。修复：添加 `AgentStatus::from_db_string()` 辅助方法减少重复。

3. **[MEDIUM] timestamp 生成静默失败**: `unwrap_or_default()` 在系统时钟异常时返回 0 (1970-01-01)。修复：改为 `expect()` panic，因为这是严重的环境问题。

4. **[LOW] 缺少 name 字段验证**: NewAgent 的 name 字段没有长度或空白检查。修复：添加 `validate()` 方法和 `AgentValidationError` 错误类型，检查 name 非空且不超过 100 字符。

5. **[LOW] Story 文档说明**: AC 要求 "id (UUID)" 但实现保留 INTEGER id + agent_uuid。这是合理的向后兼容设计，在 Dev Notes 中已说明。

### File List

**新建文件:**
- `crates/omninova-core/src/agent/model.rs` - Agent 数据模型结构体
- `crates/omninova-core/src/agent/store.rs` - Agent CRUD 存储层

**修改文件:**
- `crates/omninova-core/src/agent/mod.rs` - 导出 model 和 store 模块
- `crates/omninova-core/src/db/migrations.rs` - 添加 002_agent_enhancements 迁移
- `apps/omninova-tauri/src-tauri/src/lib.rs` - 添加 Agent 相关 Tauri 命令