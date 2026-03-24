# Story 5.1: L1 工作记忆层实现

Status: done

## Story

As a AI 代理,
I want 维护一个短期工作记忆缓存,
so that 我可以在当前会话中快速访问相关上下文.

## Acceptance Criteria

1. **AC1: 内存缓存创建** - 创建内存缓存用于存储当前会话上下文
2. **AC2: 容量配置** - 缓存支持设置最大容量（可配置的上下文窗口大小）
3. **AC3: LRU淘汰策略** - 使用 LRU 淘汰策略管理缓存大小
4. **AC4: 快速检索** - 支持快速键值检索（O(1) 时间复杂度）
5. **AC5: 会话持久化** - 会话结束时缓存可选择性持久化到 L2

## Tasks / Subtasks

- [x] Task 1: 扩展现有 InMemoryMemory 实现 (AC: #1, #3, #4)
  - [x] 1.1 创建 `LruMemory` 结构体，基于 `lru` crate 实现 LRU 淘汰
  - [x] 1.2 实现 `Memory` trait for `LruMemory`
  - [x] 1.3 添加 `WithCapacity` trait 或 builder pattern 设置容量
  - [x] 1.4 确保 O(1) 读写性能

- [x] Task 2: 工作记忆管理器 (AC: #1, #2, #5)
  - [x] 2.1 创建 `WorkingMemory` 结构体封装 L1 缓存
  - [x] 2.2 实现会话上下文管理：`push_context`, `get_context`, `clear_context`
  - [x] 2.3 实现会话持久化接口：`persist_to_l2`
  - [x] 2.4 添加配置项到 `MemoryConfig`: `working_memory_capacity`

- [x] Task 3: Tauri Commands API (AC: #1, #4)
  - [x] 3.1 添加 `get_working_memory` Tauri 命令
  - [x] 3.2 添加 `clear_working_memory` Tauri 命令
  - [x] 3.3 添加 `get_memory_stats` Tauri 命令
  - [x] 3.4 定义 TypeScript 类型 `WorkingMemoryEntry`, `MemoryStats`

- [x] Task 4: AgentDispatcher 集成 (AC: #1)
  - [x] 4.1 在 `AgentService` 中添加 `WorkingMemory` 实例
  - [x] 4.2 自动将对话消息添加到工作记忆
  - [x] 4.3 在生成响应时注入工作记忆上下文

- [x] Task 5: 单元测试 (All ACs)
  - [x] 5.1 测试 LRU 淘汰策略
  - [x] 5.2 测试容量限制
  - [x] 5.3 测试 O(1) 检索性能
  - [x] 5.4 测试会话持久化接口

## Dev Notes

### 现有基础设施分析

**已有内存系统：**

1. **Memory trait** (`crates/omninova-core/src/memory/traits.rs`):
   ```rust
   #[async_trait]
   pub trait Memory: Send + Sync {
       fn name(&self) -> &str;
       async fn store(&self, key: &str, content: &str, category: MemoryCategory, session_id: Option<&str>) -> anyhow::Result<()>;
       async fn recall(&self, query: &str, limit: usize, session_id: Option<&str>) -> anyhow::Result<Vec<MemoryEntry>>;
       async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>>;
       async fn list(&self, category: Option<&MemoryCategory>, session_id: Option<&str>) -> anyhow::Result<Vec<MemoryEntry>>;
       async fn forget(&self, key: &str) -> anyhow::Result<bool>;
       async fn count(&self) -> anyhow::Result<usize>;
       async fn health_check(&self) -> bool;
   }
   ```

2. **InMemoryMemory** (`crates/omninova-core/src/memory/backend.rs`):
   - 使用 `HashMap<String, MemoryEntry>` 存储
   - 使用 `parking_lot::RwLock` 实现并发访问
   - **缺少**: LRU 淘汰、容量限制、O(1) 访问保证

3. **MemoryConfig** (`crates/omninova-core/src/config/schema.rs`):
   ```rust
   pub struct MemoryConfig {
       pub backend: String,
       pub db_path: Option<String>,
       pub qdrant_url: Option<String>,
       pub qdrant_collection: Option<String>,
       pub qdrant_api_key: Option<String>,
       pub embedding: EmbeddingConfig,
   }
   ```

4. **MemoryEntry** 结构:
   ```rust
   pub struct MemoryEntry {
       pub id: String,
       pub key: String,
       pub content: String,
       pub category: MemoryCategory,
       pub timestamp: String,
       pub session_id: Option<String>,
       pub score: Option<f64>,
   }
   ```

### 技术实现方案

**L1 工作记忆特性：**
- 短期存储，会话级别
- 固定容量，LRU 淘汰
- 内存中，无持久化（可选同步到 L2）
- 为 AgentDispatcher 提供上下文

**推荐实现方式：**

使用 `lru` crate 实现 LRU 缓存：
```rust
use lru::LruCache;
use parking_lot::RwLock;
use std::num::NonZeroUsize;

pub struct LruMemory {
    cache: Arc<RwLock<LruCache<String, MemoryEntry>>>,
    capacity: NonZeroUsize,
}
```

**工作记忆管理器设计：**
```rust
pub struct WorkingMemory {
    l1_cache: LruMemory,
    session_id: Option<i64>,
    agent_id: Option<i64>,
}

impl WorkingMemory {
    /// 添加对话上下文
    pub async fn push_context(&self, role: &str, content: &str) -> anyhow::Result<()>;

    /// 获取完整上下文用于 LLM
    pub async fn get_context(&self, limit: usize) -> anyhow::Result<Vec<MemoryEntry>>;

    /// 清除当前会话上下文
    pub async fn clear(&self) -> anyhow::Result<()>;

    /// 持久化到 L2（可选）
    pub async fn persist_to_l2(&self, l2: &dyn Memory) -> anyhow::Result<()>;

    /// 获取统计信息
    pub fn stats(&self) -> MemoryStats;
}

pub struct MemoryStats {
    pub capacity: usize,
    pub used: usize,
    pub hit_rate: f64,
}
```

### 配置扩展

在 `MemoryConfig` 中添加：
```rust
pub struct MemoryConfig {
    // ... existing fields
    pub working_memory_capacity: Option<usize>, // default: 100
}
```

默认容量建议：100 条消息（约等于 4096 tokens 上下文）

### 文件结构

| 文件 | 作用 | 类型 |
|------|------|------|
| `crates/omninova-core/src/memory/lru.rs` | LRU 缓存实现 | 新建 |
| `crates/omninova-core/src/memory/working.rs` | 工作记忆管理器 | 新建 |
| `crates/omninova-core/src/memory/mod.rs` | 模块导出 | 修改 |
| `crates/omninova-core/src/config/schema.rs` | 添加配置项 | 修改 |
| `crates/omninova-core/src/agent/service.rs` | 集成工作记忆 | 修改 |
| `apps/omninova-tauri/src-tauri/src/lib.rs` | Tauri commands | 修改 |
| `apps/omninova-tauri/src/types/memory.ts` | TypeScript 类型 | 新建 |

### 架构模式遵循

**命名约定：**
- Rust 结构体: `PascalCase` (如 `WorkingMemory`, `LruMemory`)
- Rust 函数: `snake_case` (如 `push_context`, `get_context`)
- Tauri Commands: `camelCase` (如 `getWorkingMemory`, `clearWorkingMemory`)
- TypeScript 类型: `PascalCase` (如 `WorkingMemoryEntry`, `MemoryStats`)

**依赖项：**
- `lru` crate (已广泛使用的 LRU 实现)
- `parking_lot` (已在项目中使用)
- `async_trait` (已在项目中使用)

### 前序 Story 学习 (4.10 指令执行框架)

1. **async_trait 宏** - 异步 trait 方法必须使用 `#[async_trait]`
2. **parking_lot::RwLock** - 比 `std::sync::RwLock` 性能更好
3. **Arc 包装** - 共享所有权时使用 `Arc<T>`
4. **测试模式** - 使用 `#[cfg(test)] mod tests` 内联测试
5. **错误处理** - 使用 `anyhow::Result` 和 `thiserror::Error`

### 关键技术决策

1. **为什么选择 `lru` crate:**
   - 纯 Rust 实现，无 unsafe
   - O(1) 访问时间
   - 支持容量限制
   - 广泛使用，维护良好

2. **为什么需要 WorkingMemory 封装层:**
   - 隔离 LRU 实现细节
   - 提供会话级别的上下文管理
   - 支持与 L2 的集成（未来）
   - 统一的 API 接口

3. **容量选择:**
   - 默认 100 条消息
   - 可通过配置调整
   - 平衡内存使用和上下文长度

### 性能要求 (NFR-P2)

- L1 缓存命中响应时间 < 10ms
- 内存占用可控（可配置上限）
- 无阻塞操作（异步 API）

### 测试标准

1. **单元测试** - Rust 测试使用 `cargo test`
2. **性能测试** - 验证 O(1) 访问时间
3. **边界测试** - 容量满时的淘汰行为
4. **并发测试** - 多线程访问安全性

## References

- [Source: _bmad-output/planning-artifacts/epics.md#L926-L940] - Story 5.1 requirements
- [Source: crates/omninova-core/src/memory/traits.rs] - Memory trait definition
- [Source: crates/omninova-core/src/memory/backend.rs] - Existing InMemoryMemory
- [Source: crates/omninova-core/src/config/schema.rs#L750-L776] - MemoryConfig
- [Source: _bmad-output/planning-artifacts/architecture.md] - ARCH-10 Memory System
- [Source: _bmad-output/implementation-artifacts/4-10-command-execution.md] - 前序 Story 学习

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

N/A

### Completion Notes List

Story 5.1 implementation completed:

1. **LruMemory Implementation** (`crates/omninova-core/src/memory/lru.rs`):
   - LRU cache using `lru` crate with O(1) access time
   - Thread-safe with `parking_lot::RwLock`
   - Implements `Memory` trait for backend abstraction
   - 12 tests passing for LRU eviction, capacity, O(1) access

2. **WorkingMemory Manager** (`crates/omninova-core/src/memory/working.rs`):
   - Session-scoped context management with `push_context`, `get_context`, `clear`
   - Key-based ordering for chronological message retrieval
   - Optional persistence to L2 via `persist_to_l2`
   - Memory statistics with capacity, usage, session/agent IDs
   - 12 tests passing for context management, persistence, stats

3. **Tauri Commands API** (`apps/omninova-tauri/src-tauri/src/lib.rs`):
   - `get_working_memory` - retrieve context entries
   - `clear_working_memory` - clear session context
   - `get_memory_stats` - get memory statistics
   - `set_working_memory_session` - set session context
   - `push_working_memory_context` - add context entry

4. **TypeScript Types** (`apps/omninova-tauri/src/types/memory.ts`):
   - `WorkingMemoryEntry`, `MemoryStats` interfaces
   - Async functions wrapping Tauri commands

5. **Configuration** (`crates/omninova-core/src/config/schema.rs`):
   - Added `working_memory_capacity` field to `MemoryConfig`
   - Default capacity: 100 entries (~4096 tokens context)

6. **AgentService Integration** (`crates/omninova-core/src/agent/service.rs`):
   - Added `working_memory: Arc<Mutex<WorkingMemory>>` field
   - User messages automatically pushed to working memory in `chat()` and `chat_stream()`
   - Assistant responses automatically pushed to working memory

**Total: 24 tests passing**

**Code Review Fix (2026-03-20):**
- Fixed Task 4 implementation: Added WorkingMemory integration to AgentService
- Removed unused import in working.rs

### File List

**Created:**
- `crates/omninova-core/src/memory/lru.rs` - LRU memory implementation (new file)
- `crates/omninova-core/src/memory/working.rs` - Working memory manager (new file)
- `apps/omninova-tauri/src/types/memory.ts` - TypeScript types (new file)

**Modified:**
- `Cargo.toml` (workspace root) - Added lru dependency
- `crates/omninova-core/Cargo.toml` - Added lru workspace dependency
- `crates/omninova-core/src/memory/mod.rs` - Export new modules
- `crates/omninova-core/src/config/schema.rs` - Add capacity config
- `crates/omninova-core/src/agent/service.rs` - Integrate WorkingMemory with AgentService
- `apps/omninova-tauri/src-tauri/src/lib.rs` - Add Tauri commands and AppState

**Tests (inline):**
- `crates/omninova-core/src/memory/lru.rs` - 12 tests for LRU behavior
- `crates/omninova-core/src/memory/working.rs` - 12 tests for WorkingMemory

## Change Log

| Date | Change |
|------|--------|
| 2026-03-20 | Story 5.1 context created - ready for implementation |
| 2026-03-20 | Story 5.1 implementation completed - all tasks done, 24 tests passing |
| 2026-03-20 | Code review: Fixed Task 4 (AgentService integration), removed unused imports |