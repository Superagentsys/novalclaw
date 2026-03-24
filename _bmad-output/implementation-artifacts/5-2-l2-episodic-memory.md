# Story 5.2: L2 情景记忆层实现

Status: done

## Story

As a AI 代理,
I want 存储长期情景记忆到持久化存储,
so that 我可以记住跨会话的重要对话和事件.

## Acceptance Criteria

1. **AC1: 数据库表创建** - SQLite 数据库已配置 WAL 模式，episodic_memories 表已创建，包含字段：id, agent_id, session_id, content, importance, created_at
2. **AC2: 多维度查询** - 支持按代理、会话、时间范围查询
3. **AC3: 重要性排序** - 支持按重要性排序
4. **AC4: 批量操作** - 实现批量插入和检索操作
5. **AC5: 数据导出备份** - 记忆数据支持导出和备份

## Tasks / Subtasks

- [x] Task 1: 数据库迁移与表结构 (AC: #1)
  - [x] 1.1 创建 `008_episodic_memories` 迁移，定义 `episodic_memories` 表
  - [x] 1.2 添加索引：`idx_episodic_agent_id`, `idx_episodic_session_id`, `idx_episodic_importance`, `idx_episodic_created_at`
  - [x] 1.3 实现 `down_sql` 回滚脚本

- [x] Task 2: EpisodicMemory 结构体与 Store (AC: #1, #2, #3)
  - [x] 2.1 定义 `EpisodicMemory` 结构体（id, agent_id, session_id, content, importance, created_at）
  - [x] 2.2 定义 `NewEpisodicMemory` 和 `EpisodicMemoryUpdate` 结构体
  - [x] 2.3 创建 `EpisodicMemoryStore` 封装数据库操作
  - [x] 2.4 实现 Memory-like interface with CRUD operations

- [x] Task 3: 查询与批量操作 (AC: #2, #3, #4)
  - [x] 3.1 实现 `find_by_agent(agent_id, limit, offset)` 按代理查询
  - [x] 3.2 实现 `find_by_session(session_id)` 按会话查询
  - [x] 3.3 实现 `find_by_time_range(start, end, agent_id)` 时间范围查询
  - [x] 3.4 实现 `find_by_importance(min_importance, limit)` 重要性筛选
  - [x] 3.5 实现 `batch_insert(entries)` 批量插入
  - [x] 3.6 实现 `batch_get(ids)` 批量检索

- [x] Task 4: 导出与备份功能 (AC: #5)
  - [x] 4.1 实现 `export_to_json(agent_id)` 导出单个代理的记忆
  - [x] 4.2 实现 `import_from_json(json_data)` 导入记忆
  - [x] 4.3 添加 Tauri 命令暴露导出/导入功能

- [x] Task 5: Tauri Commands API (AC: #1, #4)
  - [x] 5.1 添加 `store_episodic_memory` Tauri 命令
  - [x] 5.2 添加 `get_episodic_memories` Tauri 命令
  - [x] 5.3 添加 `export_episodic_memories` Tauri 命令
  - [x] 5.4 添加 `import_episodic_memories` Tauri 命令
  - [x] 5.5 定义 TypeScript 类型 `EpisodicMemory`, `EpisodicMemoryStats`

- [x] Task 6: AgentService 集成 (AC: #1)
  - [x] 6.1 在 `AgentService` 中添加 `EpisodicMemoryStore` 实例
  - [x] 6.2 实现会话结束时自动从 L1 持久化到 L2
  - [x] 6.3 添加配置项到 `MemoryConfig`

- [x] Task 7: 单元测试 (All ACs)
  - [x] 7.1 测试数据库迁移
  - [x] 7.2 测试 CRUD 操作
  - [x] 7.3 测试多维度查询
  - [x] 7.4 测试批量操作
  - [x] 7.5 测试导出/导入功能

## Dev Notes

### 现有基础设施分析

**已有数据库系统：**

1. **Migration 系统** (`crates/omninova-core/src/db/migrations.rs`):
   ```rust
   pub struct Migration {
       pub id: String,
       pub description: String,
       pub up_sql: String,
       pub down_sql: Option<String>,
   }

   pub fn get_builtin_migrations() -> Vec<Migration>  // 当前有 7 个迁移
   ```

2. **DbPool** (`crates/omninova-core/src/db/pool.rs`):
   - SQLite 连接池，支持 WAL 模式
   - `create_pool()` 函数创建连接池

3. **Memory trait** (`crates/omninova-core/src/memory/traits.rs`):
   ```rust
   #[async_trait]
   pub trait Memory: Send + Sync {
       fn name(&self) -> &str;
       async fn store(&self, key: &str, content: &str, category: MemoryCategory, session_id: Option<&str>) -> anyhow::Result<()>;
       async fn recall(&self, query: &str, limit: usize, session_id: Option<&str>) -> anyhow::Result<Vec<MemoryEntry>>;
       // ...
   }
   ```

4. **已有后端实现**:
   - `InMemoryMemory` - 内存 HashMap
   - `JsonFileMemory` - JSON 文件持久化
   - `LruMemory` - LRU 缓存 (Story 5.1)
   - `WorkingMemory` - 工作记忆管理器 (Story 5.1)

### 前序 Story 学习 (5.1 L1 工作记忆层)

1. **WorkingMemory 与 L2 集成接口**:
   ```rust
   pub async fn persist_to_l2(&self, l2: &dyn Memory) -> anyhow::Result<usize>
   ```

2. **AgentService 集成模式**:
   ```rust
   pub struct AgentService {
       // ...
       working_memory: Arc<Mutex<WorkingMemory>>,
       memory: Arc<dyn Memory>,  // 这是 L2/L3，需要替换为 EpisodicMemoryStore
   }
   ```

3. **测试模式** - 内联 `#[cfg(test)] mod tests`，使用 `tokio::test`

### 技术实现方案

**episodic_memories 表结构：**

```sql
CREATE TABLE IF NOT EXISTS episodic_memories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id INTEGER NOT NULL,
    session_id INTEGER,
    content TEXT NOT NULL,
    importance INTEGER NOT NULL DEFAULT 5 CHECK(importance BETWEEN 1 AND 10),
    metadata TEXT,  -- JSON for additional data
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE SET NULL
);

-- Indexes for query patterns
CREATE INDEX idx_episodic_agent_id ON episodic_memories(agent_id);
CREATE INDEX idx_episodic_session_id ON episodic_memories(session_id);
CREATE INDEX idx_episodic_importance ON episodic_memories(importance DESC);
CREATE INDEX idx_episodic_created_at ON episodic_memories(created_at DESC);
CREATE INDEX idx_episodic_agent_created ON episodic_memories(agent_id, created_at DESC);
```

**EpisodicMemory 结构体设计：**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicMemory {
    pub id: i64,
    pub agent_id: i64,
    pub session_id: Option<i64>,
    pub content: String,
    pub importance: u8,  // 1-10
    pub metadata: Option<String>,  // JSON
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewEpisodicMemory {
    pub agent_id: i64,
    pub session_id: Option<i64>,
    pub content: String,
    pub importance: u8,
    pub metadata: Option<String>,
}

pub struct EpisodicMemoryStore {
    db: DbPool,
}

impl EpisodicMemoryStore {
    pub fn new(db: DbPool) -> Self { ... }

    pub async fn create(&self, entry: &NewEpisodicMemory) -> anyhow::Result<i64>;
    pub async fn find_by_agent(&self, agent_id: i64, limit: usize, offset: usize) -> anyhow::Result<Vec<EpisodicMemory>>;
    pub async fn find_by_session(&self, session_id: i64) -> anyhow::Result<Vec<EpisodicMemory>>;
    pub async fn find_by_time_range(&self, start: i64, end: i64, agent_id: Option<i64>) -> anyhow::Result<Vec<EpisodicMemory>>;
    pub async fn find_by_importance(&self, min_importance: u8, limit: usize) -> anyhow::Result<Vec<EpisodicMemory>>;
    pub async fn batch_insert(&self, entries: &[NewEpisodicMemory]) -> anyhow::Result<usize>;
    pub async fn delete(&self, id: i64) -> anyhow::Result<bool>;
    pub async fn count_by_agent(&self, agent_id: i64) -> anyhow::Result<usize>;
    pub async fn export_to_json(&self, agent_id: i64) -> anyhow::Result<String>;
    pub async fn import_from_json(&self, json: &str) -> anyhow::Result<usize>;
}
```

### 配置扩展

在 `MemoryConfig` 中添加：
```rust
pub struct MemoryConfig {
    // ... existing fields
    pub episodic_memory_enabled: bool,  // default: true
}
```

### 文件结构

| 文件 | 作用 | 类型 |
|------|------|------|
| `crates/omninova-core/src/db/migrations.rs` | 添加迁移 | 修改 |
| `crates/omninova-core/src/memory/episodic.rs` | 情景记忆存储 | 新建 |
| `crates/omninova-core/src/memory/mod.rs` | 模块导出 | 修改 |
| `crates/omninova-core/src/agent/service.rs` | 集成情景记忆 | 修改 |
| `crates/omninova-core/src/config/schema.rs` | 添加配置项 | 修改 |
| `apps/omninova-tauri/src-tauri/src/lib.rs` | Tauri commands | 修改 |
| `apps/omninova-tauri/src/types/memory.ts` | TypeScript 类型 | 修改 |

### 架构模式遵循

**命名约定：**
- Rust 结构体: `PascalCase` (如 `EpisodicMemory`, `EpisodicMemoryStore`)
- Rust 函数: `snake_case` (如 `find_by_agent`, `batch_insert`)
- Tauri Commands: `camelCase` (如 `getEpisodicMemories`, `storeEpisodicMemory`)
- TypeScript 类型: `PascalCase` (如 `EpisodicMemory`, `EpisodicMemoryStats`)

**数据库模式：**
- 使用现有的 `DbPool` 和迁移系统
- 外键关联到 `agents` 和 `sessions` 表
- WAL 模式已由 pool.rs 配置

### 性能要求 (NFR-P2)

- L2 数据库查询响应时间 < 200ms
- 批量插入 100 条记录 < 500ms
- 索引覆盖常用查询模式

### 测试标准

1. **单元测试** - Rust 测试使用 `cargo test`
2. **迁移测试** - 验证表结构和索引
3. **CRUD 测试** - 完整的增删改查测试
4. **查询测试** - 各种查询条件的正确性
5. **导出/导入测试** - JSON 序列化正确性

## References

- [Source: _bmad-output/planning-artifacts/epics.md#L942-L956] - Story 5.2 requirements
- [Source: crates/omninova-core/src/db/migrations.rs] - Migration system
- [Source: crates/omninova-core/src/memory/traits.rs] - Memory trait definition
- [Source: crates/omninova-core/src/memory/backend.rs] - Existing memory backends
- [Source: crates/omninova-core/src/memory/working.rs] - WorkingMemory with persist_to_l2
- [Source: _bmad-output/implementation-artifacts/5-1-l1-working-memory.md] - 前序 Story 学习
- [Source: _bmad-output/planning-artifacts/architecture.md#L1066] - L2 情景记忆架构

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

N/A

### Completion Notes List

Story 5.2 implementation completed:

1. **Database Migration** (`crates/omninova-core/src/db/migrations.rs`):
   - Added migration 008 for episodic_memories table
   - Created indexes: idx_episodic_agent_id, idx_episodic_session_id, idx_episodic_importance, idx_episodic_created_at, idx_episodic_agent_created
   - Implemented rollback SQL for clean migration reversal
   - 17 tests passing for migrations (including new episodic_memories test)

2. **EpisodicMemoryStore** (`crates/omninova-core/src/memory/episodic.rs`):
   - Defined EpisodicMemory, NewEpisodicMemory, EpisodicMemoryUpdate, EpisodicMemoryStats structs
   - Implemented CRUD operations: create, get, update, delete
   - Multi-dimensional queries: find_by_agent, find_by_session, find_by_time_range, find_by_importance
   - Batch operations: batch_insert, batch_get
   - Export/Import: export_to_json, import_from_json
   - Statistics: count, count_by_agent, stats
   - 13 tests passing for EpisodicMemoryStore

3. **Tauri Commands API** (`apps/omninova-tauri/src-tauri/src/lib.rs`):
   - store_episodic_memory - create new episodic memory
   - get_episodic_memories - query by agent with pagination
   - get_episodic_memories_by_session - query by session
   - get_episodic_memories_by_importance - query by importance
   - delete_episodic_memory - delete memory
   - get_episodic_memory_stats - get statistics
   - export_episodic_memories - export to JSON
   - import_episodic_memories - import from JSON
   - Added episodic_memory_store to AppState with initialization

4. **TypeScript Types** (`apps/omninova-tauri/src/types/memory.ts`):
   - EpisodicMemory, NewEpisodicMemory, EpisodicMemoryStats interfaces
   - Async functions for all Tauri commands

5. **AgentService Integration** (`crates/omninova-core/src/agent/service.rs`):
   - Added episodic_memory_store: Option<Arc<EpisodicMemoryStore>> field
   - New constructor: with_episodic_memory()
   - Added set_episodic_memory_store() method
   - Implemented persist_session_to_l2() for L1→L2 persistence
   - Implemented store_important_memory() for direct L2 storage

6. **Configuration** (`crates/omninova-core/src/config/schema.rs`):
   - Added episodic_memory_enabled field to MemoryConfig (default: true)

**Total: 528 tests passing (13 new episodic memory tests + 17 migration tests updated)**

### File List

**Created:**
- `crates/omninova-core/src/memory/episodic.rs` - Episodic memory store with 13 tests

**Modified:**
- `crates/omninova-core/src/db/migrations.rs` - Added migration 008 + tests
- `crates/omninova-core/src/memory/mod.rs` - Export new module
- `crates/omninova-core/src/config/schema.rs` - Add episodic_memory_enabled config
- `crates/omninova-core/src/agent/service.rs` - Integrate episodic memory store
- `apps/omninova-tauri/src-tauri/src/lib.rs` - Add 8 Tauri commands
- `apps/omninova-tauri/src/types/memory.ts` - Add TypeScript types and API functions

**Tests (inline):**
- `crates/omninova-core/src/memory/episodic.rs` - 13 tests for EpisodicMemoryStore
- `crates/omninova-core/src/db/migrations.rs` - 17 tests including episodic_memories validation

## Change Log

| Date | Change |
|------|--------|
| 2026-03-14 | Story 5.2 context created - ready for implementation |
| 2026-03-20 | Story 5.2 implementation completed - all tasks done, 528 tests passing |
| 2026-03-20 | Code review fixes applied - importance validation, duplicate handling, L1→L2 persistence |

## Code Review Fixes (2026-03-20)

### Issues Fixed

1. **HIGH: Automatic L1→L2 Persistence** (`crates/omninova-core/src/agent/service.rs:695`)
   - **Issue**: `persist_session_to_l2()` method existed but was never called automatically
   - **Fix**: Added `end_session` Tauri command that triggers L1→L2 persistence when a session ends
   - **Files**: `lib.rs` (new command), `memory.ts` (new API function)

2. **MEDIUM: Duplicate Data in Import** (`crates/omninova-core/src/memory/episodic.rs:458`)
   - **Issue**: `import_from_json` could create duplicate entries
   - **Fix**: Added `skip_duplicates` parameter with duplicate detection logic
   - **Files**: `episodic.rs` (new `import_with_duplicate_handling` method), `lib.rs`, `memory.ts`

3. **MEDIUM: Memory Exhaustion Risk** (`crates/omninova-core/src/memory/episodic.rs:452`)
   - **Issue**: `usize::MAX` limit could cause memory issues
   - **Fix**: Added `DEFAULT_EXPORT_LIMIT = 10,000` with warning when limit reached
   - **Files**: `episodic.rs` (new constant and warning log)

4. **LOW: Importance Value Validation** (`crates/omninova-core/src/memory/episodic.rs:35`)
   - **Issue**: No validation for importance range (1-10)
   - **Fix**: Added validation in `create()`, `batch_insert()`, and `import_from_json()`
   - **Files**: `episodic.rs` (validation in all entry points)

### New Tests Added

- `test_import_with_skip_duplicates` - Verifies duplicate detection works
- `test_importance_validation` - Verifies importance range validation

### Updated File List

**Modified:**
- `crates/omninova-core/src/memory/episodic.rs` - Added validation, export limit, duplicate handling
- `apps/omninova-tauri/src-tauri/src/lib.rs` - Updated import command, added end_session command
- `apps/omninova-tauri/src/types/memory.ts` - Updated API signatures

**Total: 530 tests passing (16 episodic memory tests)**