# Story 2.12: 配置备份与恢复

Status: ready-for-dev

## Story

As a 用户,
I want 备份和恢复我的配置数据,
so that 我可以在不同设备间迁移或防止数据丢失.

## Acceptance Criteria

1. **Given** 已有 AI 代理和配置, **When** 我访问备份设置, **Then** 可以导出所有配置为 JSON 或 YAML 文件

2. **Given** 导出配置时, **When** 选择导出, **Then** 导出包含代理配置、人格设置、偏好设置

3. **Given** 备份文件已创建, **When** 我选择导入, **Then** 可以导入备份文件恢复配置

4. **Given** 导入备份时, **When** 选择导入选项, **Then** 提供：完全覆盖或选择性合并

5. **Given** 选择备份文件, **When** 导入前, **Then** 验证文件格式有效性，无效文件显示错误提示

## Tasks / Subtasks

- [x] Task 1: 定义备份数据模型 (AC: 1, 2)
  - [x] 创建 `BackupData` 结构体（config, agents, version, created_at）
  - [x] 创建 `BackupMeta` 结构体（version, app_version, created_at, checksum）
  - [x] 定义 `ImportOptions` 枚举（overwrite, merge）
  - [x] 添加 serde 序列化/反序列化支持
  - [x] 添加 Rust 单元测试

- [x] Task 2: 实现后端备份导出功能 (AC: 1, 2)
  - [x] 创建 `crates/omninova-core/src/backup/mod.rs` 模块
  - [x] 实现 `export_backup() -> Result<BackupData>` 函数
  - [x] 从 ConfigManager 获取当前配置
  - [x] 从 AgentStore 获取所有代理配置
  - [x] 收集账户设置（不含密码哈希）
  - [x] 添加版本信息和时间戳
  - [x] 实现 JSON/YAML 格式导出
  - [x] 添加 Rust 单元测试

- [x] Task 3: 实现后端备份导入功能 (AC: 3, 4, 5)
  - [x] 实现 `validate_backup(data: &str) -> Result<BackupMeta>` 验证函数
  - [x] 实现 `import_backup(data: &str, options: ImportOptions) -> Result<()>` 函数
  - [x] 实现完全覆盖逻辑（清空现有数据，导入备份）
  - [x] 实现选择性合并逻辑（保留现有，合并备份）
  - [x] 处理版本兼容性检查
  - [x] 添加错误处理和回滚机制
  - [x] 添加 Rust 单元测试

- [x] Task 4: 实现 Tauri 命令 API (AC: All)
  - [x] `export_config_backup(format: String) -> Result<String>` - 导出配置为 JSON/YAML
  - [x] `validate_backup_file(content: String) -> Result<BackupMeta>` - 验证备份文件
  - [x] `import_config_backup(content: String, options: ImportOptionsJson) -> Result<()>` - 导入备份
  - [ ] `get_backup_history() -> Result<Vec<BackupRecord>>` - 获取备份历史（可选，暂不实现）
  - [x] 添加命令到 Tauri invoke_handler

- [x] Task 5: 创建前端备份设置组件 (AC: 1, 3, 4, 5)
  - [x] 创建 `src/components/backup/BackupSettings.tsx` 组件
  - [x] 实现导出按钮（选择 JSON/YAML 格式）
  - [x] 实现导入文件选择器（使用 Tauri dialog API）
  - [x] 实现导入选项对话框（覆盖/合并选择）
  - [x] 添加格式验证和错误提示
  - [x] 添加成功/失败通知

- [x] Task 6: 创建前端类型定义 (AC: All)
  - [x] 创建 `src/types/backup.ts` 定义 BackupData 接口
  - [x] 定义 ImportOptions 类型
  - [x] 定义 BackupMeta 接口
  - [x] 导出类型供其他组件使用

- [x] Task 7: 集成到设置页面 (AC: All)
  - [x] 在 Setup 组件添加"备份与恢复"标签页
  - [x] 添加路由配置
  - [x] 更新侧边栏导航

- [x] Task 8: 添加单元测试 (AC: All)
  - [x] 测试 BackupData 序列化/反序列化
  - [x] 测试备份验证逻辑
  - [x] 测试导入选项处理
  - [x] 前端组件测试

- [x] Task 9: 文档和导出 (AC: All)
  - [x] 添加 JSDoc 注释到所有公开 API
  - [x] 运行 `npm run lint` - 确保无错误（backup 相关代码无新增错误）
  - [x] 运行 `cargo clippy` - 确保无新增警告（已修复 backup 模块的 clippy 警告）

## Dev Notes

### 前置依赖（已完成）

**Story 1-5 SQLite 迁移系统：**
- `MigrationRunner` 已实现
- 数据库路径：`~/.omninoval/omninoval.db`

**Story 1-6 配置热重载：**
- `ConfigManager` 已实现
- 配置文件路径：`~/.omninoval/config.toml`

**Story 2-1 ~ 2-10 代理管理：**
- `AgentStore` 已实现
- 代理数据存储在 SQLite

**Story 2-11 账户管理：**
- `AccountStore` 已实现
- 账户数据存储在 SQLite

### 架构设计

**备份数据模型：**
```rust
/// 备份数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupData {
    /// 备份元数据
    pub meta: BackupMeta,
    /// 配置数据
    pub config: ConfigBackup,
    /// 代理列表
    pub agents: Vec<AgentBackup>,
    /// 账户信息（不含敏感数据）
    pub account: Option<AccountBackup>,
}

/// 备份元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMeta {
    /// 备份格式版本
    pub version: String,
    /// 应用版本
    pub app_version: String,
    /// 创建时间 (ISO 8601)
    pub created_at: String,
    /// 数据校验和（可选）
    pub checksum: Option<String>,
}

/// 配置备份
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigBackup {
    /// 提供商配置
    pub providers: Vec<ProviderConfig>,
    /// 渠道配置
    pub channels: ChannelsConfig,
    /// 技能配置
    pub skills: SkillsConfig,
    /// 代理人格配置
    pub agent: AgentPersonaConfig,
    /// 其他偏好设置
    pub preferences: HashMap<String, Value>,
}

/// 代理备份
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBackup {
    /// 代理 UUID
    pub uuid: String,
    /// 代理名称
    pub name: String,
    /// 描述
    pub description: Option<String>,
    /// 专业领域
    pub domain: Option<String>,
    /// MBTI 类型
    pub mbti_type: Option<String>,
    /// 系统提示词
    pub system_prompt: Option<String>,
    /// 状态
    pub status: String,
    /// 创建时间
    pub created_at: i64,
    /// 更新时间
    pub updated_at: i64,
}

/// 账户备份（不含密码）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountBackup {
    /// 用户名
    pub username: String,
    /// 是否启动时要求密码
    pub require_password_on_startup: bool,
    /// 创建时间
    pub created_at: i64,
    /// 更新时间
    pub updated_at: i64,
}

/// 导入选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImportMode {
    /// 完全覆盖：清空现有数据，导入备份
    Overwrite,
    /// 选择性合并：保留现有，合并备份（备份优先）
    Merge,
}

/// 导入选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportOptions {
    /// 导入模式
    pub mode: ImportMode,
    /// 是否导入代理配置
    pub include_agents: bool,
    /// 是否导入提供商配置
    pub include_providers: bool,
    /// 是否导入渠道配置
    pub include_channels: bool,
    /// 是否导入技能配置
    pub include_skills: bool,
    /// 是否导入账户设置
    pub include_account: bool,
}
```

**备份导出流程：**
```
用户点击导出
    ↓
选择格式（JSON/YAML）
    ↓
调用 export_config_backup(format)
    ↓
收集配置数据（ConfigManager）
收集代理数据（AgentStore）
收集账户数据（AccountStore）
    ↓
构建 BackupData
    ↓
序列化为指定格式
    ↓
返回给前端
    ↓
前端触发文件下载
```

**备份导入流程：**
```
用户选择备份文件
    ↓
读取文件内容
    ↓
调用 validate_backup_file(content)
    ↓
显示预览信息（元数据）
    ↓
用户选择导入选项
    ↓
调用 import_config_backup(content, options)
    ↓
验证备份格式
    ↓
根据 ImportMode 执行导入
    ↓
返回结果（成功/失败）
```

### 前端组件设计

**备份设置页面布局：**
```
┌─────────────────────────────────────────────────────────────┐
│  备份与恢复                                                  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  导出配置                                            │   │
│  │                                                     │   │
│  │  选择导出格式：                                      │   │
│  │  ○ JSON    ○ YAML                                   │   │
│  │                                                     │   │
│  │  [导出配置]                                         │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  导入配置                                            │   │
│  │                                                     │   │
│  │  [选择备份文件]                                     │   │
│  │                                                     │   │
│  │  当前文件: backup-2026-03-17.json                   │   │
│  │  创建时间: 2026-03-17 10:30:00                      │   │
│  │  应用版本: v1.0.0                                   │   │
│  │                                                     │   │
│  │  导入选项：                                         │   │
│  │  ○ 完全覆盖（清空现有配置）                         │   │
│  │  ○ 选择性合并（保留现有配置）                       │   │
│  │                                                     │   │
│  │  选择导入内容：                                     │   │
│  │  ☑ 代理配置    ☑ 提供商配置                        │   │
│  │  ☑ 渠道配置    ☑ 技能配置                          │   │
│  │  ☑ 账户设置                                         │   │
│  │                                                     │   │
│  │  [导入配置]                                         │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  ⚠️ 警告                                            │   │
│  │  导入配置将覆盖现有设置，建议先导出当前配置备份。     │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**导入选项对话框：**
```
┌─────────────────────────────────────────────┐
│  确认导入配置                                │
├─────────────────────────────────────────────┤
│                                             │
│  即将导入备份文件：                          │
│  - 5 个代理                                 │
│  - 3 个提供商                               │
│  - 2 个渠道                                 │
│                                             │
│  导入模式：完全覆盖                          │
│                                             │
│  ⚠️ 此操作将清空现有配置并替换为备份内容。    │
│                                             │
│         [取消]    [确认导入]                 │
└─────────────────────────────────────────────┘
```

### 项目架构约束

- **后端模块位置**: `crates/omninova-core/src/backup/mod.rs`
- **前端组件位置**: `apps/omninova-tauri/src/components/backup/`
- **前端类型位置**: `apps/omninova-tauri/src/types/backup.ts`
- **样式系统**: Tailwind CSS + Shadcn/UI
- **命名约定**:
  - 组件文件: PascalCase (`BackupSettings.tsx`)
  - Props 接口: 组件名 + Props (`BackupSettingsProps`)
  - Tauri 命令: snake_case (`export_config_backup`)

### 依赖项

**需要添加到 `crates/omninova-core/Cargo.toml`：**
```toml
# 已有依赖，无需新增
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"  # 可选，用于 YAML 格式支持
```

**前端依赖（已存在）：**
- React 19
- Tailwind CSS
- Shadcn/UI 组件（Button, Dialog, RadioGroup, Checkbox）
- @tauri-apps/api（invoke, dialog, fs）
- sonner（toast 通知）
- lucide-react（图标）

### 安全注意事项

1. **敏感数据排除**：
   - 导出不包含密码哈希
   - 导出不包含 API 密钥明文（仅引用环境变量名）
   - 导出不包含渠道 Token 明文

2. **备份文件安全**：
   - 提醒用户妥善保管备份文件
   - 建议备份文件加密存储（Phase 2）

3. **导入验证**：
   - 验证备份格式有效性
   - 验证版本兼容性
   - 显示导入预览，让用户确认

4. **错误处理**：
   - 导入失败时提供回滚选项（完全覆盖模式）
   - 显示详细错误信息

### MVP 范围限制

根据 architecture.md，MVP 阶段的备份恢复限制：

- **本地备份**：仅支持本地文件导出/导入
- **无云同步**：Phase 2 考虑
- **无自动备份**：手动触发
- **无加密备份**：Phase 2 考虑

### 版本兼容性

**备份版本控制：**
- 当前版本: `1.0`
- 版本检查: 导入时验证 `meta.version`
- 向后兼容: 支持导入 `1.x` 版本备份
- 不兼容处理: 提示用户版本不兼容

**应用版本检查：**
- 记录 `app_version` 用于诊断
- 主要版本差异警告（如 v1.x 备份导入到 v2.x 应用）

### 测试策略

**后端测试（Rust）：**
- BackupData 序列化/反序列化测试
- 备份验证测试（有效/无效格式）
- 导入选项测试（覆盖/合并）
- 版本兼容性测试
- 边界条件测试（空备份、大数据量）

**前端测试（Vitest）：**
- 组件渲染测试
- 文件选择器交互测试
- 导入选项状态管理测试
- 错误提示显示测试

**测试文件位置：**
- `crates/omninova-core/src/backup/mod.rs` - 内联测试
- `apps/omninova-tauri/src/test/components/BackupSettings.test.tsx`

### 注意事项

1. **数据一致性**：
   - 导入前验证所有依赖数据存在
   - 导入失败时回滚到导入前状态

2. **用户体验**：
   - 导出时显示进度
   - 导入前显示预览
   - 操作确认对话框
   - 成功/失败通知

3. **文件格式**：
   - JSON 作为默认格式
   - YAML 作为可选格式（更易读）
   - 文件名包含时间戳：`omninoval-backup-YYYY-MM-DD-HHmmss.json`

4. **跨设备迁移**：
   - API 密钥通过环境变量引用，需用户在新设备配置
   - 代理 UUID 保持不变，确保数据一致性

### References

- [Source: epics.md#Story 2.12] - 验收标准
- [Source: architecture.md#数据架构] - 配置存储设计
- [Source: architecture.md#基础设施] - 配置文件路径
- [Source: prd.md#FR42] - 备份和恢复功能需求
- [Source: ux-design-specification.md#体验原则] - 通过控制建立信任
- [Source: crates/omninova-core/src/config/mod.rs] - ConfigManager 实现
- [Source: crates/omninova-core/src/agent/store.rs] - AgentStore 实现模式
- [Source: crates/omninova-core/src/account/store.rs] - AccountStore 实现模式
- [Source: apps/omninova-tauri/src/types/config.ts] - 前端配置类型定义

## Dev Agent Record

### Agent Model Used

{{agent_model_name_version}}

### Debug Log References

### Completion Notes List

### File List