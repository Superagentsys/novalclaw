# Story 1.6: 配置文件监听与热重载系统

Status: done

## Story

As a 开发者,
I want 实现配置文件监听和热重载功能,
so that 用户修改配置后应用可以自动响应而无需重启.

## Acceptance Criteria

1. **Given** 配置文件路径已定义 (~/.omninoval/config.toml), **When** 我实现配置监听系统, **Then** 文件系统监听器已创建用于监控配置文件变化

2. 配置变更时自动触发重新加载

3. 环境变量可以覆盖配置文件设置

4. 配置加载失败时有明确的错误处理和默认值回退

5. Tauri command 已暴露用于获取当前配置

6. 前端可以通过事件订阅配置变更通知

## Tasks / Subtasks

- [x] Task 1: 创建文件系统监听器 (AC: 1)
  - [x] 添加 `notify` 依赖到 `crates/omninova-core/Cargo.toml`
  - [x] 创建 `crates/omninova-core/src/config/watcher.rs` 模块
  - [x] 实现 `ConfigWatcher` 结构体，包装 `notify::RecommendedWatcher`
  - [x] 实现 `ConfigWatcher::start()` 异步启动监听
  - [x] 实现 `ConfigWatcher::stop()` 停止监听
  - [x] 使用 `debounce` 机制防止频繁触发（推荐 200ms）

- [x] Task 2: 实现配置热重载机制 (AC: 2)
  - [x] 创建 `ConfigManager` 结构体，持有当前配置和监听器
  - [x] 使用 `Arc<RwLock<Config>>` 实现线程安全的配置共享
  - [x] 实现 `ConfigManager::reload()` 方法重新加载配置
  - [x] 在文件变更回调中触发重新加载
  - [x] 添加配置变更回调注册机制 (`on_change`)

- [x] Task 3: 验证并增强环境变量覆盖 (AC: 3)
  - [x] 确认 `config/env.rs` 的 `apply_env_overrides()` 已在加载流程中调用
  - [x] 验证环境变量在热重载后仍然生效
  - [x] 添加单元测试验证环境变量覆盖顺序

- [x] Task 4: 实现错误处理和默认值回退 (AC: 4)
  - [x] 实现 `ConfigManager::reload_safe()` 方法，失败时保留当前配置
  - [x] 添加配置验证和错误日志记录
  - [x] 返回 `Result<ConfigChangeStatus, ConfigError>` 通知调用方结果
  - [x] 创建 `ConfigWatcherError` 枚举定义所有可能的错误情况

- [x] Task 5: 实现 Tauri 配置命令 (AC: 5)
  - [x] 在 `apps/omninova-tauri/src-tauri/src/lib.rs` 中实现 `get_config` 命令
  - [x] 实现 `save_config` 命令保存配置到文件
  - [x] 实现 `reload_config` 命令手动触发重载
  - [x] 实现 `get_config_path` 命令获取配置文件路径
  - [x] 注册命令到 Tauri invoke_handler

- [x] Task 6: 实现前端事件订阅机制 (AC: 6)
  - [x] 定义 Tauri 事件 `config:changed`，载荷包含新配置 JSON
  - [x] 定义 Tauri 事件 `config:error`，载荷包含错误信息
  - [x] 在 `ConfigManager` 中集成事件发送（通过 `tauri::AppHandle`）
  - [x] 添加 `subscribe_config_changes` 命令用于管理订阅

- [x] Task 7: 集成测试和验证 (AC: All)
  - [x] 编写单元测试验证文件监听触发
  - [x] 编写单元测试验证热重载正确性
  - [x] 编写单元测试验证错误回退机制
  - [x] 运行 `cargo test` 确保所有测试通过

## Dev Notes

### 项目架构约束

- **工作目录**: Rust 后端代码在 `crates/omninova-core/` 目录下
- **Tauri 入口**: `apps/omninova-tauri/src-tauri/src/` 调用 core 模块
- **配置位置**: `~/.omninoval/config.toml`（通过 `resolve_config_path()` 获取）
- **配置模块**: `crates/omninova-core/src/config/`

### 前一个故事的发现 (Story 1.5)

**重要**: Story 1.5 已完成 SQLite 数据库迁移系统，有以下发现：

1. **Tauri 命令位置**: 直接在 `lib.rs` 中实现，未创建单独的 commands 目录
2. **错误处理**: 使用 `anyhow::Result` 和 `thiserror` 结合
3. **测试策略**: 使用 `tempfile` crate 创建临时文件进行测试
4. **构建验证**: 使用 `cargo test` 验证所有测试通过

### 现有配置模块结构

```
crates/omninova-core/src/config/
├── mod.rs           # 模块入口，导出各子模块
├── loader.rs        # 配置加载逻辑（已实现）
├── env.rs           # 环境变量覆盖（已实现）
├── schema.rs        # 配置结构定义
├── traits.rs        # 配置 trait 定义
└── validation.rs    # 配置验证
```

**需要新建**: `watcher.rs` - 配置文件监听模块

### 已实现的功能（可直接使用）

**配置路径解析** (`loader.rs`):
```rust
pub fn resolve_config_dir() -> PathBuf { ... }
pub fn resolve_config_path() -> PathBuf { ... }
```

**配置加载** (`loader.rs`):
```rust
impl Config {
    pub fn load_or_init() -> Result<Self> { ... }
    pub fn load_from(path: &Path) -> Result<Self> { ... }
    pub fn save(&self) -> Result<()> { ... }
}
```

**环境变量覆盖** (`env.rs`):
```rust
pub fn apply_env_overrides(cfg: &mut Config) { ... }
```

已支持的环境变量：
- `OMNINOVA_API_KEY`, `API_KEY`, `OPENAI_API_KEY`
- `OMNINOVA_PROVIDER`, `OMNINOVA_MODEL`
- `PORT`, `HOST`
- 以及更多代理、网关、存储相关变量

### 推荐的依赖

```toml
[dependencies]
notify = { version = "6.1", features = ["macos_kqueue"] }
parking_lot = "0.12"  # 已在 workspace 中
tokio = { version = "1", features = ["sync", "time"] }  # 已存在
```

### ConfigWatcher 设计

```rust
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Event};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ConfigWatcher {
    watcher: Option<RecommendedWatcher>,
    config_path: PathBuf,
    debounce_ms: u64,
}

impl ConfigWatcher {
    pub fn new(config_path: PathBuf) -> Self { ... }

    pub fn start<F>(&mut self, on_change: F) -> Result<()>
    where
        F: Fn() + Send + 'static
    { ... }

    pub fn stop(&mut self) { ... }
}
```

### ConfigManager 设计

```rust
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ConfigManager {
    config: Arc<RwLock<Config>>,
    watcher: Option<ConfigWatcher>,
    change_callbacks: Vec<Box<dyn Fn(&Config) + Send + Sync>>,
}

impl ConfigManager {
    pub async fn new() -> Result<Self> { ... }

    pub async fn get(&self) -> Arc<RwLock<Config>> { ... }

    pub async fn reload(&self) -> Result<()> { ... }

    pub fn on_change<F>(&mut self, callback: F)
    where
        F: Fn(&Config) + Send + Sync + 'static
    { ... }
}
```

### Tauri 命令设计

**新增命令**（添加到 `lib.rs`）:
```rust
#[tauri::command]
async fn get_config(manager: State<'_, Arc<ConfigManager>>) -> Result<String, String> {
    let config = manager.get().await.read().await.clone();
    serde_json::to_string(&config).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_config(
    manager: State<'_, Arc<ConfigManager>>,
    config_json: String,
) -> Result<(), String> {
    let mut config: Config = serde_json::from_str(&config_json)
        .map_err(|e| e.to_string())?;
    config.save().map_err(|e| e.to_string())?;
    manager.reload().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn reload_config(manager: State<'_, Arc<ConfigManager>>) -> Result<String, String> {
    manager.reload().await.map_err(|e| e.to_string())?;
    let config = manager.get().await.read().await.clone();
    serde_json::to_string(&config).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_config_path() -> String {
    resolve_config_path().to_string_lossy().to_string()
}
```

### Tauri 事件定义

**事件名称**（遵循 architecture.md 命名约定）:
- `config:changed` - 配置成功更新
- `config:error` - 配置加载出错
- `config:reloaded` - 手动重载完成

**事件载荷结构**:
```rust
// config:changed
{
    "config": { /* Config JSON */ },
    "timestamp": "2026-03-15T10:30:00Z"
}

// config:error
{
    "error": "Failed to parse config.toml",
    "timestamp": "2026-03-15T10:30:00Z"
}
```

### 错误处理模式

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigWatcherError {
    #[error("Failed to create file watcher: {0}")]
    WatcherCreate(#[from] notify::Error),

    #[error("Failed to load config: {0}")]
    LoadError(#[from] anyhow::Error),

    #[error("Config file not found: {0}")]
    NotFound(PathBuf),

    #[error("Config parse error: {0}")]
    ParseError(String),
}
```

### 测试策略

**单元测试**:
- 配置文件变更检测
- 防抖机制验证
- 环境变量覆盖生效
- 错误回退机制

**集成测试**:
- 完整热重载流程
- Tauri 命令调用
- 事件发送验证

**测试辅助**:
```rust
// 使用 tempfile 创建临时配置文件
let temp_dir = tempfile::tempdir()?;
let config_path = temp_dir.path().join("config.toml");
std::fs::write(&config_path, "api_key = \"test\"\n")?;
```

### 项目目录结构（更新后）

**新建文件:**
```
crates/omninova-core/src/config/
└── watcher.rs       # 配置文件监听模块
```

**修改文件:**
```
crates/omninova-core/
├── Cargo.toml                        # 添加 notify 依赖
└── src/
    ├── config/mod.rs                 # 导出 watcher 模块
    └── lib.rs                        # 导出 ConfigManager（可选）

apps/omninova-tauri/src-tauri/
├── Cargo.toml                        # 添加所需依赖
└── src/lib.rs                        # 添加配置命令和状态管理
```

### References

- [Source: architecture.md#项目结构] - 配置模块位置
- [Source: architecture.md#数据架构] - 配置文件存储位置
- [Source: architecture.md#命名模式] - Tauri 事件命名约定
- [Source: architecture.md#需求到结构映射] - 配置管理模块映射
- [Source: epics.md#Story 1.6] - 验收标准
- [Source: 1-5-sqlite-migration-system.md] - 前一个故事的实现记录

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

- `notify::RecommendedWatcher` 没有 `stop()` 方法，watcher 在 drop 时自动停止
- Tauri 2.x 需要导入 `tauri::Emitter` trait 才能使用 `app.emit()` 方法

### Completion Notes List

- 实现了完整的配置文件监听系统，使用 `notify` crate 进行文件系统监控
- `ConfigWatcher` 使用 200ms debounce 防止频繁触发
- `ConfigManager` 提供 `reload()` 和 `reload_safe()` 方法，支持错误回退
- 环境变量覆盖在 `Config::load_from()` 中自动调用，热重载时保持生效
- Tauri 命令已集成事件发送：`config:changed`, `config:error`, `config:initial`
- 所有 62 个测试通过，包括 12 个新增的 watcher 相关测试

### File List

**新建文件:**
- `crates/omninova-core/src/config/watcher.rs` - 配置文件监听和热重载模块

**修改文件:**
- `crates/omninova-core/Cargo.toml` - 添加 notify 依赖
- `crates/omninova-core/src/config/mod.rs` - 导出 watcher 模块
- `crates/omninova-core/src/config/watcher.rs` - 添加 `with_shared_config()` 构造函数
- `crates/omninova-core/src/gateway/mod.rs` - 添加 `config_ref()` 方法
- `apps/omninova-tauri/src-tauri/Cargo.toml` - 添加 chrono 依赖
- `apps/omninova-tauri/src-tauri/src/lib.rs` - 添加配置命令、事件发送和 ConfigManager 集成

## Change Log

- 2026-03-15: Story 创建，状态设为 ready-for-dev
- 2026-03-15: 完成 Task 1-4 (文件监听、热重载、环境变量覆盖、错误处理)
- 2026-03-15: 完成 Task 5-6 (Tauri 命令和事件机制)
- 2026-03-15: 完成 Task 7 (测试验证)，62 个测试全部通过
- 2026-03-15: Story 状态更新为 review
- 2026-03-15: Code Review 发现 CRITICAL 问题：ConfigWatcher 未集成到 Tauri 应用
- 2026-03-15: 修复集成问题：
  - 添加 `GatewayRuntime::config_ref()` 方法暴露内部 `Arc<RwLock<Config>>`
  - 添加 `ConfigManager::with_shared_config()` 构造函数接受共享配置引用
  - 修改 `run()` 函数让 ConfigManager 和 GatewayRuntime 共享同一个配置引用
  - 所有 62 个测试通过，编译无警告