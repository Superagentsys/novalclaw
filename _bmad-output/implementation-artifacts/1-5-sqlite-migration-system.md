# Story 1.5: SQLite 数据库迁移系统实现

Status: done

## Story

As a 开发者,
I want 实现带有 WAL 模式的 SQLite 数据库迁移系统,
so that 应用可以安全地管理数据持久化和 schema 版本控制.

## Acceptance Criteria

1. **Given** Rust 后端项目结构已建立, **When** 我实现数据库迁移系统, **Then** SQLite 连接已配置为 WAL 模式以提高并发性能

2. 迁移系统框架已创建（支持 up/down 迁移）

3. schema 版本表已创建用于跟踪迁移状态

4. 初始迁移脚本模板已创建

5. 数据库连接池已配置

6. Tauri command 已暴露用于数据库初始化

## Tasks / Subtasks

- [x] Task 1: 配置 SQLite WAL 模式和连接池 (AC: 1, 5)
  - [x] 添加 rusqlite 和 r2d2 依赖到 Cargo.toml
  - [x] 创建 `crates/omninova-core/src/db/pool.rs` 连接池模块
  - [x] 配置 WAL 模式和连接参数
  - [x] 实现连接池初始化函数

- [x] Task 2: 创建迁移系统框架 (AC: 2, 3)
  - [x] 创建 `crates/omninova-core/src/db/` 目录结构
  - [x] 创建 `mod.rs` 导出迁移模块
  - [x] 定义 Migration 结构体（id, description, up_sql, down_sql）
  - [x] 实现 MigrationRunner 支持执行 up/down 迁移
  - [x] 创建 `_migrations` 表用于跟踪迁移状态

- [x] Task 3: 创建初始迁移脚本模板 (AC: 4)
  - [x] 创建初始迁移（嵌入在 migrations.rs 中）
  - [x] 定义基础 schema 结构（agents, sessions, messages 表）
  - [x] 验证迁移脚本可以成功执行

- [x] Task 4: 实现 Tauri 数据库初始化命令 (AC: 6)
  - [x] 在 `apps/omninova-tauri/src-tauri/src/lib.rs` 实现数据库命令
  - [x] 实现 `init_database` Tauri command
  - [x] 实现 `get_database_status` Tauri command
  - [x] 注册 commands 到 Tauri app

- [x] Task 5: 集成测试和验证 (AC: All)
  - [x] 编写 Rust 单元测试验证连接池
  - [x] 编写集成测试验证迁移执行
  - [x] 验证 WAL 模式正确启用
  - [x] 运行 `cargo test` 确保所有测试通过

## Dev Notes

### 项目架构约束

- **工作目录**: Rust 后端代码在 `crates/omninova-core/` 目录下
- **Tauri 入口**: `apps/omninova-tauri/src-tauri/src/` 调用 core 模块
- **数据库位置**: `~/.omninoval/data/memory.db`
- **迁移文件位置**: `crates/omninova-core/src/db/migrations/`

### 前一个故事的发现 (Story 1.4)

**重要**: Story 1.4 已完成 Vitest 测试框架配置，有以下发现：

1. **TypeScript verbatimModuleSyntax**: 类型需要使用 `import type { ... }` 语法
2. **Path alias 配置**: `@/*` 映射到 `./src/*`，已在 tsconfig.json 和 vite.config.ts 中配置
3. **Shadcn/UI v4**: 使用 `@base-ui/react` 作为无样式原语，CSS 变量使用 oklch 色彩空间
4. **构建验证**: 使用 `npm run build` 验证构建成功

### SQLite WAL 模式说明

**为什么选择 WAL 模式:**
- 提高并发性能：读操作不阻塞写操作
- 更好的崩溃恢复能力
- 适合桌面应用的单用户多连接场景

**WAL 配置要点:**
```rust
// 启用 WAL 模式
conn.execute_batch("
    PRAGMA journal_mode = WAL;
    PRAGMA synchronous = NORMAL;
    PRAGMA foreign_keys = ON;
    PRAGMA busy_timeout = 5000;
")?;
```

### Rust 数据库依赖

**核心依赖:**
```toml
[dependencies]
rusqlite = { version = "0.32", features = ["bundled"] }
r2d2 = "0.8"
r2d2_sqlite = "0.25"
thiserror = "2.0"
```

### 数据库命名约定 (来自 architecture.md)

| 元素 | 规则 | 示例 |
|------|------|------|
| 表名 | snake_case复数 | `agents`, `memories`, `sessions` |
| 列名 | snake_case | `agent_id`, `created_at`, `mbti_type` |
| 主键 | `id` | `id INTEGER PRIMARY KEY` |
| 外键 | `{table}_id` | `agent_id`, `session_id` |
| 索引 | `idx_{table}_{columns}` | `idx_agents_mbti_type` |
| 时间戳 | `{action}_at` | `created_at`, `updated_at` |

### 迁移系统设计

**Migration 结构体:**
```rust
pub struct Migration {
    pub version: i32,
    pub name: String,
    pub up_sql: String,
    pub down_sql: Option<String>,
}
```

**schema_versions 表:**
```sql
CREATE TABLE IF NOT EXISTS schema_versions (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at INTEGER NOT NULL
);
```

**MigrationRunner:**
```rust
pub struct MigrationRunner<'a> {
    conn: &'a Connection,
    migrations: Vec<Migration>,
}

impl MigrationRunner<'_> {
    pub fn run_pending(&self) -> Result<()>;
    pub fn rollback(&self, version: i32) -> Result<()>;
    pub fn get_current_version(&self) -> Result<i32>;
}
```

### Tauri Command 设计

**commands/database.rs:**
```rust
use tauri::State;
use crate::db::DbPool;

#[tauri::command]
pub async fn init_database(pool: State<'_, DbPool>) -> Result<(), String> {
    pool.initialize()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_database_status(pool: State<'_, DbPool>) -> Result<DatabaseStatus, String> {
    pool.get_status()
        .await
        .map_err(|e| e.to_string())
}
```

### 项目目录结构

**预期创建的文件:**
```
crates/omninova-core/
├── src/
│   └── db/
│       ├── mod.rs              # 数据库模块入口
│       ├── pool.rs             # 连接池管理
│       ├── migrations.rs       # 迁移系统核心
│       └── migrations/
│           ├── mod.rs
│           └── 001_initial.sql # 初始迁移脚本

apps/omninova-tauri/src-tauri/
└── src/
    └── commands/
        └── database.rs         # 数据库 Tauri 命令
```

### 数据库配置路径

**用户数据目录:**
- macOS: `~/.omninoval/`
- Windows: `%APPDATA%/omninoval/`
- Linux: `~/.local/share/omninoval/`

**数据库文件:**
```
~/.omninoval/
├── config.toml
├── data/
│   ├── memory.db       # 主数据库文件
│   ├── memory.db-wal   # WAL 日志文件
│   └── memory.db-shm   # 共享内存文件
└── logs/
    └── omninova.log
```

### 连接池配置

```rust
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;

pub type DbPool = Pool<SqliteConnectionManager>;
pub type DbConnection = PooledConnection<SqliteConnectionManager>;

pub fn create_pool(db_path: &Path) -> Result<DbPool> {
    let manager = SqliteConnectionManager::file(db_path);
    Pool::builder()
        .max_size(10)
        .build(manager)
        .context("Failed to create database pool")
}
```

### 测试策略

**单元测试:**
- 连接池创建和获取连接
- WAL 模式正确启用
- 迁移版本管理

**集成测试:**
- 完整迁移流程（up/down）
- 数据库初始化命令
- 错误处理和恢复

### 错误处理模式

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Failed to connect to database: {0}")]
    ConnectionError(#[from] rusqlite::Error),

    #[error("Migration failed: {0}")]
    MigrationError(String),

    #[error("Pool error: {0}")]
    PoolError(#[from] r2d2::Error),
}
```

### References

- [Source: architecture.md#数据架构] - 三层存储策略，SQLite WAL 模式
- [Source: architecture.md#命名模式] - 数据库命名约定
- [Source: architecture.md#项目结构] - 数据库模块位置
- [Source: epics.md#Story 1.5] - 验收标准
- [Source: 1-4-vitest-setup.md] - 前一个故事的实现记录

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

N/A - 无重大问题需要调试

### Completion Notes List

- 迁移系统采用嵌入式 SQL（const）而非外部文件，简化部署
- 迁移表命名为 `_migrations`，使用 `id TEXT PRIMARY KEY` 而非 `version INTEGER`
- Tauri 命令直接集成在 `lib.rs` 中，未创建单独的 `commands/database.rs`
- 所有 50 个测试通过，包括 12 个数据库相关测试

### File List

**新建文件:**
- `crates/omninova-core/src/db/mod.rs` - 数据库模块入口
- `crates/omninova-core/src/db/pool.rs` - 连接池管理（含 WAL 配置）
- `crates/omninova-core/src/db/migrations.rs` - 迁移系统核心（含初始 schema）

**修改文件:**
- `crates/omninova-core/Cargo.toml` - 添加 rusqlite, r2d2, r2d2_sqlite, tempfile 依赖
- `crates/omninova-core/src/lib.rs` - 导出 db 模块
- `apps/omninova-tauri/src-tauri/src/lib.rs` - 添加 db_pool 状态和数据库命令
- `apps/omninova-tauri/src-tauri/Cargo.toml` - 添加 tracing 依赖

**设计变更说明:**
- 迁移 SQL 嵌入在 `migrations.rs` 中（`INITIAL_SCHEMA_SQL` 常量），而非外部 `.sql` 文件
- Tauri 数据库命令直接在 `lib.rs` 实现，未创建单独的 commands 目录

## Change Log

- 2026-03-15: Story 创建，状态设为 ready-for-dev
- 2026-03-15: 完成所有实现任务，50 个测试通过
- 2026-03-15: Code Review 完成，更新 Tasks 状态和 Dev Agent Record