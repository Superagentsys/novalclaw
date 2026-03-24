# Story 2.13: 数据加密与隐私设置

Status: ready-for-dev

## Story

As a 用户,
I want 设置数据加密和隐私保护选项,
so that 我可以控制我的数据如何被存储和处理.

## Acceptance Criteria

1. **Given** 本地账户已创建, **When** 我访问隐私设置, **Then** 可以启用/禁用本地数据加密

2. **Given** 数据加密已启用, **When** 应用存储用户数据时, **Then** 敏感数据使用 AES-256-GCM 加密后存储

3. **Given** 隐私设置页面, **When** 我查看数据存储信息, **Then** 显示数据存储位置和占用大小

4. **Given** 隐私设置页面, **When** 我选择清除对话历史, **Then** 可以选择清除范围（全部/指定代理/指定时间范围）并确认执行

5. **Given** 隐私设置变更, **When** 我保存设置, **Then** 变更立即生效无需重启应用

6. **Given** 云端同步功能（如果可用）, **When** 我访问隐私设置, **Then** 可以查看同步状态和控制同步选项

## Tasks / Subtasks

- [x] Task 1: 实现后端加密服务模块 (AC: 1, 2)
  - [x] 创建 `crates/omninova-core/src/security/crypto.rs` 加密服务
  - [x] 实现 `EncryptionService` trait（encrypt, decrypt, is_encrypted）
  - [x] 实现 AES-256-GCM 加密算法
  - [x] 实现密钥派生（PBKDF2 或 Argon2）
  - [x] 创建 `EncryptionKeyManager` 管理加密密钥
  - [x] 实现密钥的安全存储（使用 OS Keychain）
  - [x] 添加 Rust 单元测试

- [x] Task 2: 扩展数据库加密支持 (AC: 2)
  - [x] 在 `crates/omninova-core/src/db/` 添加加密层
  - [x] 实现 `EncryptedDb` 提供透明加密/解密
  - [x] 实现敏感字段的透明加密/解密
  - [x] 定义需要加密的数据字段（messages.content, agents.system_prompt 等）
  - [x] 实现加密元数据表（encrypted_fields）
  - [x] 添加数据库迁移脚本（004_encrypted_fields）
  - [x] 添加 Rust 单元测试

- [ ] Task 3: 实现隐私设置数据模型 (AC: All)
  - [x] 创建 `PrivacySettings` 结构体
  - [x] 定义加密启用/禁用标志
  - [x] 定义数据保留策略配置
  - [x] 定义云端同步设置（预留）
  - [x] 添加 serde 序列化支持
  - [x] 添加 Rust 单元测试

- [ ] Task 4: 实现 Tauri 命令 API (AC: All)
  - [ ] `get_privacy_settings() -> Result<PrivacySettings>` - 获取隐私设置
  - [ ] `update_privacy_settings(settings: PrivacySettings) -> Result<()>` - 更新隐私设置
  - [ ] `get_data_storage_info() -> Result<StorageInfo>` - 获取存储信息
  - [ ] `clear_conversation_history(options: ClearOptions) -> Result<ClearResult>` - 清除对话历史
  - [ ] `toggle_encryption(enabled: bool) -> Result<()>` - 启用/禁用加密
  - [ ] `is_encryption_available() -> Result<bool>` - 检查加密功能可用性
  - [ ] 添加命令到 Tauri invoke_handler

- [ ] Task 5: 实现存储信息查询 (AC: 3)
  - [ ] 实现 `StorageInfo` 结构体（config_path, data_path, total_size, breakdown）
  - [ ] 计算数据库文件大小
  - [ ] 计算配置文件大小
  - [ ] 计算日志文件大小
  - [ ] 计算缓存大小
  - [ ] 格式化大小显示（B/KB/MB/GB）
  - [ ] 添加 Rust 单元测试

- [ ] Task 6: 实现对话历史清除功能 (AC: 4)
  - [ ] 创建 `ClearOptions` 结构体（scope, agent_ids, date_range）
  - [ ] 实现 `ClearScope` 枚举（All, SpecificAgents, DateRange）
  - [ ] 实现级联删除逻辑（messages → conversations）
  - [ ] 实现按时间范围删除
  - [ ] 实现删除确认和结果统计
  - [ ] 更新相关记忆数据
  - [ ] 添加 Rust 单元测试

- [ ] Task 7: 创建前端隐私设置组件 (AC: 1, 3, 4, 5)
  - [ ] 创建 `src/components/settings/PrivacySettings.tsx` 组件
  - [ ] 实现加密开关组件（带状态指示）
  - [ ] 实现存储信息显示组件
  - [ ] 实现清除历史对话框（范围选择、确认）
  - [ ] 实现设置保存和即时生效
  - [ ] 添加加载和错误状态处理

- [ ] Task 8: 创建前端类型定义 (AC: All)
  - [ ] 创建 `src/types/privacy.ts` 定义 PrivacySettings 接口
  - [ ] 定义 StorageInfo 接口
  - [ ] 定义 ClearOptions 和 ClearResult 接口
  - [ ] 导出类型供其他组件使用

- [ ] Task 9: 集成到设置页面 (AC: All)
  - [ ] 在 Setup 组件添加"隐私与安全"标签页
  - [ ] 添加路由配置
  - [ ] 更新侧边栏导航

- [ ] Task 10: 添加单元测试 (AC: All)
  - [ ] 测试加密/解密功能
  - [ ] 测试密钥管理
  - [ ] 测试存储信息计算
  - [ ] 测试历史清除逻辑
  - [ ] 前端组件测试

- [ ] Task 11: 文档和代码质量 (AC: All)
  - [ ] 添加 JSDoc 注释到所有公开 API
  - [ ] 运行 `npm run lint` - 确保无错误
  - [ ] 运行 `cargo clippy` - 确保无新增警告

## Dev Notes

### 前置依赖（已完成）

**Story 1-5 SQLite 迁移系统：**
- `MigrationRunner` 已实现
- 数据库路径：`~/.omninoval/omninoval.db`

**Story 1-6 配置热重载：**
- `ConfigManager` 已实现
- 配置文件路径：`~/.omninoval/config.toml`

**Story 2-11 账户管理：**
- `AccountStore` 已实现
- 密码哈希存储机制已实现

**Story 2-12 配置备份与恢复：**
- 备份导出/导入功能已实现
- 敏感数据排除机制已建立

### 架构设计

**加密服务架构：**

```rust
/// 加密服务 trait
pub trait EncryptionService: Send + Sync {
    /// 加密数据
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>>;

    /// 解密数据
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>>;

    /// 检查数据是否已加密
    fn is_encrypted(&self, data: &[u8]) -> bool;

    /// 重新加密（密钥更换时）
    fn re_encrypt(&self, data: &[u8], old_key: &[u8]) -> Result<Vec<u8>>;
}

/// AES-256-GCM 加密实现
pub struct AesGcmEncryption {
    key_manager: Arc<EncryptionKeyManager>,
}

/// 加密密钥管理器
pub struct EncryptionKeyManager {
    /// 密钥存储在 OS Keychain
    keychain_service: KeychainService,
    /// 当前密钥 ID
    current_key_id: String,
}
```

**隐私设置数据模型：**

```rust
/// 隐私设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacySettings {
    /// 是否启用本地数据加密
    pub encryption_enabled: bool,

    /// 数据保留策略
    pub data_retention: DataRetentionPolicy,

    /// 云端同步设置（预留）
    pub cloud_sync: CloudSyncSettings,

    /// 最后更新时间
    pub updated_at: i64,
}

/// 数据保留策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataRetentionPolicy {
    /// 对话历史保留天数（0 = 永久保留）
    pub conversation_retention_days: u32,

    /// 是否自动清理过期数据
    pub auto_cleanup_enabled: bool,
}

/// 云端同步设置（预留）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSyncSettings {
    /// 是否启用同步
    pub enabled: bool,

    /// 同步范围
    pub sync_scope: SyncScope,

    /// 上次同步时间
    pub last_sync_at: Option<i64>,
}

/// 存储信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageInfo {
    /// 配置目录路径
    pub config_path: String,

    /// 数据目录路径
    pub data_path: String,

    /// 总占用大小（字节）
    pub total_size: u64,

    /// 分类大小
    pub breakdown: StorageBreakdown,
}

/// 存储大小分类
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageBreakdown {
    /// 数据库大小
    pub database: u64,

    /// 配置文件大小
    pub config: u64,

    /// 日志大小
    pub logs: u64,

    /// 缓存大小
    pub cache: u64,
}

/// 清除选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClearOptions {
    /// 清除范围
    pub scope: ClearScope,

    /// 指定代理 ID（当 scope 为 SpecificAgents 时）
    pub agent_ids: Option<Vec<String>>,

    /// 时间范围（当 scope 为 DateRange 时）
    pub date_range: Option<DateRange>,
}

/// 清除范围
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClearScope {
    /// 清除全部
    All,
    /// 指定代理
    SpecificAgents,
    /// 指定时间范围
    DateRange,
}

/// 清除结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClearResult {
    /// 删除的消息数
    pub messages_deleted: u64,

    /// 删除的会话数
    pub conversations_deleted: u64,

    /// 释放的空间（字节）
    pub space_freed: u64,
}
```

**加密数据流程：**

```
用户启用加密
    ↓
生成/获取加密密钥
    ↓
密钥存储到 OS Keychain
    ↓
标记数据库为加密模式
    ↓
对敏感字段进行透明加密/解密
    ↓
应用运行时自动处理加解密
```

**对话历史清除流程：**

```
用户选择清除范围
    ↓
显示确认对话框（清除内容预览）
    ↓
用户确认
    ↓
执行删除操作
    ↓
返回删除统计
    ↓
更新存储信息
```

### 前端组件设计

**隐私设置页面布局：**

```
┌─────────────────────────────────────────────────────────────┐
│  隐私与安全                                                  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  数据加密                                            │   │
│  │                                                     │   │
│  │  本地数据加密：[开关]                                │   │
│  │                                                     │   │
│  │  ℹ️ 启用后，敏感数据将使用 AES-256-GCM 加密存储      │   │
│  │                                                     │   │
│  │  ⚠️ 注意：禁用加密将解密所有数据，请确保环境安全      │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  数据存储                                            │   │
│  │                                                     │   │
│  │  配置目录：~/.omninoval/                            │   │
│  │  数据库：~/.omninoval/omninoval.db                  │   │
│  │                                                     │   │
│  │  存储占用：                                          │   │
│  │  ┌────────────────────────────────────┐            │   │
│  │  │ 数据库    ████████████  128 MB     │            │   │
│  │  │ 配置      ██          2 MB         │            │   │
│  │  │ 日志      ████        16 MB        │            │   │
│  │  │ 缓存      █           4 MB         │            │   │
│  │  └────────────────────────────────────┘            │   │
│  │  总计：150 MB                                       │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  清除数据                                            │   │
│  │                                                     │   │
│  │  清除范围：                                          │   │
│  │  ○ 全部对话历史                                      │   │
│  │  ○ 指定代理的对话                                    │   │
│  │  ○ 指定时间范围的对话                                │   │
│  │                                                     │   │
│  │  [清除对话历史]                                     │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  云端同步（即将推出）                                │   │
│  │                                                     │   │
│  │  云端同步功能将在后续版本中提供                      │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**清除确认对话框：**

```
┌─────────────────────────────────────────────┐
│  确认清除对话历史                            │
├─────────────────────────────────────────────┤
│                                             │
│  即将清除：                                  │
│  - 全部代理的对话历史                        │
│                                             │
│  ⚠️ 此操作不可恢复，已删除的对话将永久丢失。  │
│                                             │
│  建议先导出配置备份。                        │
│                                             │
│         [取消]    [确认清除]                 │
└─────────────────────────────────────────────┘
```

### 项目架构约束

- **后端模块位置**: `crates/omninova-core/src/security/`
- **前端组件位置**: `apps/omninova-tauri/src/components/settings/`
- **前端类型位置**: `apps/omninova-tauri/src/types/privacy.ts`
- **样式系统**: Tailwind CSS + Shadcn/UI
- **命名约定**:
  - 组件文件: PascalCase (`PrivacySettings.tsx`)
  - Props 接口: 组件名 + Props (`PrivacySettingsProps`)
  - Tauri 命令: snake_case (`get_privacy_settings`)

### 依赖项

**需要添加到 `crates/omninova-core/Cargo.toml`：**
```toml
# 加密相关
aes-gcm = "0.10"           # AES-256-GCM 加密
argon2 = "0.5"             # 密钥派生函数
rand = "0.8"               # 随机数生成
base64 = "0.22"            # Base64 编码
```

**前端依赖（已存在）：**
- React 19
- Tailwind CSS
- Shadcn/UI 组件
- @tauri-apps/api
- sonner（toast 通知）
- lucide-react（图标）

### 安全注意事项

1. **密钥管理**：
   - 加密密钥使用 OS Keychain 存储
   - 密钥派生使用 Argon2id（抗暴力破解）
   - 密钥不在内存中明文保存超过必要时间

2. **加密范围**：
   - 敏感字段：messages.content, agents.system_prompt
   - 非敏感字段：timestamps, ids, status
   - 配置文件：可选加密

3. **数据清理**：
   - 清除操作需要二次确认
   - 删除前显示将要删除的内容统计
   - 物理删除而非软删除

4. **用户体验**：
   - 首次启用加密时显示说明
   - 禁用加密前警告风险
   - 加密/解密过程显示进度

### MVP 范围限制

根据 architecture.md，MVP 阶段的加密隐私限制：

- **本地加密**：仅支持敏感字段加密
- **无云端同步**：Phase 2 考虑
- **无端到端加密**：Phase 2 考虑
- **无自动备份**：手动触发

### 性能考虑

1. **加密性能**：
   - 使用 AES-GCM 硬件加速（如果可用）
   - 大数据分块加密
   - 加密操作异步执行

2. **存储信息计算**：
   - 缓存计算结果（5分钟）
   - 后台计算，不阻塞 UI

3. **历史清除**：
   - 批量删除，事务保护
   - 进度显示
   - 完成后执行 VACUUM 回收空间

### 测试策略

**后端测试（Rust）：**
- 加密/解密单元测试
- 密钥派生测试
- 密钥存储测试
- 数据库加密层测试
- 存储信息计算测试
- 历史清除逻辑测试

**前端测试（Vitest）：**
- 组件渲染测试
- 加密开关交互测试
- 清除对话框交互测试
- 错误状态处理测试

**测试文件位置：**
- `crates/omninova-core/src/security/crypto.rs` - 内联测试
- `apps/omninova-tauri/src/test/components/PrivacySettings.test.tsx`

### 注意事项

1. **向后兼容**：
   - 未加密数据应能正常读取
   - 加密启用时自动迁移现有数据

2. **错误处理**：
   - 加密失败时的回滚机制
   - 密钥丢失的数据恢复（无解）

3. **用户通知**：
   - 加密状态变化时通知用户
   - 清除操作完成时显示结果

4. **跨平台**：
   - macOS: Keychain
   - Windows: Credential Manager
   - Linux: Secret Service (libsecret)

### References

- [Source: epics.md#Story 2.13] - 验收标准
- [Source: architecture.md#认证与安全架构] - 安全架构设计
- [Source: architecture.md#安全性要求] - 安全需求
- [Source: architecture.md#security/模块] - 安全模块结构
- [Source: prd.md#FR44] - 数据加密和隐私保护选项
- [Source: prd.md#FR43] - 本地存储和云端同步控制
- [Source: prd.md#NFR-S1] - 本地加密存储要求
- [Source: prd.md#NFR-S3] - TLS 1.3 通信要求
- [Source: ux-design-specification.md#隐私控制] - 隐私 UI 设计原则
- [Source: crates/omninova-core/src/security/keychain.rs] - Keychain 服务实现
- [Source: crates/omninova-core/src/db/] - 数据库层实现
- [Source: apps/omninova-tauri/src/components/backup/BackupSettings.tsx] - 参考组件结构

## Dev Agent Record

### Agent Model Used

{{agent_model_name_version}}

### Debug Log References

### Completion Notes List

### File List