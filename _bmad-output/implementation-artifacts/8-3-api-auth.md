# Story 8.3: API 认证与授权

Status: done

## Story

As a 开发者,
I want 安全地访问 API,
So that 只有授权的请求才能与系统交互.

## Acceptance Criteria

1. **AC1: API Key 认证** - 支持 API Key 认证机制 ✅
2. **AC2: API Key 管理** - 可以创建和管理多个 API Key ✅
3. **AC3: 权限范围** - 可以设置 API Key 的权限范围（只读、读写等） ✅
4. **AC4: 速率限制** - 支持请求速率限制防止滥用 ✅
5. **AC5: 认证失败响应** - 认证失败返回 401 错误 ✅
6. **AC6: 权限不足响应** - 权限不足返回 403 错误 ✅

## Tasks / Subtasks

- [x] Task 1: 设计 API Key 数据模型 (AC: #1, #2, #3)
  - [x] 1.1 定义 ApiKey 结构体（id, key, name, permissions, created_at, expires_at, last_used_at）
  - [x] 1.2 定义 ApiKeyPermission 枚举（Read, Write, Admin）
  - [x] 1.3 创建 api_keys 数据库表
  - [x] 1.4 实现 ApiKeyStore 存储 trait

- [x] Task 2: 实现 API Key 认证中间件 (AC: #1, #5, #6)
  - [x] 2.1 创建 AuthLayer Axum 中间件
  - [x] 2.2 实现 Bearer Token 提取和验证
  - [x] 2.3 实现 X-API-Key Header 提取和验证
  - [x] 2.4 将认证信息注入请求扩展（AuthenticatedRequest）
  - [x] 2.5 返回 401 未认证错误
  - [x] 2.6 返回 403 权限不足错误

- [x] Task 3: 实现 API Key 管理服务 (AC: #2, #3)
  - [x] 3.1 创建 ApiKeyService（生成、列出、撤销、删除 API Key）
  - [x] 3.2 实现安全的 API Key 生成（使用 crypto_secure_random）
  - [x] 3.3 实现 API Key 前缀显示（只显示前8位，隐藏完整密钥）
  - [x] 3.4 实现 API Key 过期机制
  - [x] 3.5 实现 API Key 使用追踪（last_used_at 更新）

- [x] Task 4: 实现速率限制中间件 (AC: #4)
  - [x] 4.1 创建 RateLimitLayer 中间件
  - [x] 4.2 实现基于 IP 的速率限制
  - [x] 4.3 实现基于 API Key 的速率限制
  - [x] 4.4 配置默认速率限制（如 100 请求/分钟）
  - [x] 4.5 返回 429 Too Many Requests 响应
  - [x] 4.6 添加 X-RateLimit-* 响应头

- [x] Task 5: 创建前端 API Key 管理界面 (AC: #2, #3)
  - [x] 5.1 创建 ApiKeySettingsPage 组件
  - [x] 5.2 实现 API Key 列表显示（带掩码）
  - [x] 5.3 实现创建 API Key 对话框
  - [x] 5.4 实现撤销/删除 API Key 功能
  - [x] 5.5 显示 API Key 使用统计

- [x] Task 6: 单元测试与集成测试 (AC: 全部)
  - [x] 6.1 测试 API Key 生成和验证
  - [x] 6.2 测试认证中间件
  - [x] 6.3 测试权限检查
  - [x] 6.4 测试速率限制
  - [x] 6.5 测试 401/403/429 错误响应

## Dev Notes

### 架构上下文

Story 8.3 是 Epic 8 (开发者工具与API) 的第三个 Story，建立在 Story 8.1 HTTP Gateway 和 Story 8.2 RESTful API 基础之上，提供安全的 API 访问控制。

**依赖关系：**
- **Story 8.1 (已完成)**: HTTP Gateway、CORS、HTTPS 已实现
- **Story 8.2 (已完成)**: RESTful API 端点已实现
- **security/keyring.rs**: 已有 OS Keychain 集成用于安全存储
- **security/crypto.rs**: 已有 AES-256-GCM 加密支持

**功能需求关联：**
- FR26: 用户可以管理API密钥和认证凭据
- FR45: 开发者可以通过API与AI代理交互
- NFR-I2: 系统应采用多层安全机制

### 现有实现分析

**已有安全模块** (`crates/omninova-core/src/security/`):

```rust
// keyring.rs - 已有 OS Keychain 集成
pub struct KeyringService {
    store: Arc<dyn SecretStore>,
}

impl KeyringService {
    pub async fn save_provider_key(&self, provider: &str, api_key: &str) -> Result<KeyReference, KeyringError>;
    pub async fn get_provider_key(&self, provider: &str) -> Result<String, KeyringError>;
}

// crypto.rs - 已有加密支持
pub trait EncryptionService {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, EncryptionError>;
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, EncryptionError>;
}
```

**已有 Gateway 路由模式** (`crates/omninova-core/src/gateway/mod.rs`):

```rust
// 现有路由（需要保护的端点）
.route("/api/agents", get(http_api_agents_list).post(http_api_agents_create))
.route("/api/agents/:id", get(http_api_agents_get).put(http_api_agents_update).delete(http_api_agents_delete))
.route("/api/agents/:id/chat", post(http_api_agents_chat))
.route("/api/agents/:id/chat/stream", post(http_api_agents_chat_stream))
```

### 需要新增的功能

**1. API Key 数据模型：**

```rust
// crates/omninova-core/src/gateway/auth.rs

/// API Key 权限级别
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyPermission {
    /// 只读权限 - 可以查看代理和会话
    Read,
    /// 读写权限 - 可以创建/修改代理和发送消息
    Write,
    /// 管理员权限 - 可以管理 API Keys 和系统配置
    Admin,
}

/// API Key 数据模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: i64,
    pub key_hash: String,         // SHA-256 哈希
    pub key_prefix: String,       // 前8位用于显示
    pub name: String,
    pub permissions: Vec<ApiKeyPermission>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub last_used_at: Option<i64>,
    pub is_revoked: bool,
}

/// 创建 API Key 请求
pub struct CreateApiKeyRequest {
    pub name: String,
    pub permissions: Vec<ApiKeyPermission>,
    pub expires_in_days: Option<u32>,
}

/// API Key 创建响应（只显示一次完整密钥）
pub struct ApiKeyCreated {
    pub id: i64,
    pub key: String,              // 完整密钥（只显示一次）
    pub name: String,
    pub permissions: Vec<ApiKeyPermission>,
    pub expires_at: Option<i64>,
}
```

**2. 认证中间件：**

```rust
// crates/omninova-core/src/gateway/auth.rs

/// 认证信息
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub api_key_id: i64,
    pub key_name: String,
    pub permissions: Vec<ApiKeyPermission>,
}

/// 认证中间件
pub struct AuthLayer {
    api_key_store: Arc<ApiKeyStore>,
}

impl AuthLayer {
    pub fn new(api_key_store: Arc<ApiKeyStore>) -> Self {
        Self { api_key_store }
    }
}

// 中间件提取认证信息
async fn extract_api_key(
    headers: &HeaderMap,
    api_key_store: &ApiKeyStore,
) -> Result<AuthContext, AuthError> {
    // 1. 尝试 Bearer Token
    if let Some(bearer) = headers.get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        return validate_key(bearer, api_key_store).await;
    }

    // 2. 尝试 X-API-Key Header
    if let Some(api_key) = headers.get("X-API-Key")
        .and_then(|v| v.to_str().ok())
    {
        return validate_key(api_key, api_key_store).await;
    }

    Err(AuthError::MissingApiKey)
}

async fn validate_key(key: &str, store: &ApiKeyStore) -> Result<AuthContext, AuthError> {
    // 1. 哈希密钥
    let key_hash = sha256(key);

    // 2. 查找密钥
    let api_key = store.find_by_hash(&key_hash).await?
        .ok_or(AuthError::InvalidApiKey)?;

    // 3. 检查是否撤销
    if api_key.is_revoked {
        return Err(AuthError::RevokedKey);
    }

    // 4. 检查是否过期
    if let Some(expires_at) = api_key.expires_at {
        if chrono::Utc::now().timestamp() > expires_at {
            return Err(AuthError::ExpiredKey);
        }
    }

    // 5. 更新最后使用时间
    store.update_last_used(api_key.id).await?;

    Ok(AuthContext {
        api_key_id: api_key.id,
        key_name: api_key.name,
        permissions: api_key.permissions,
    })
}
```

**3. 权限检查：**

```rust
// 权限检查中间件
pub fn require_permission(
    required: ApiKeyPermission,
) -> impl Filter<Extract = (), Error = Rejection> + Clone {
    warp::filters::ext::get::<AuthContext>()
        .and_then(move |auth: AuthContext| {
            let required = required.clone();
            async move {
                if has_permission(&auth.permissions, &required) {
                    Ok(())
                } else {
                    Err(reject::custom(PermissionDenied))
                }
            }
        })
}

fn has_permission(granted: &[ApiKeyPermission], required: &ApiKeyPermission) -> bool {
    match required {
        ApiKeyPermission::Read => granted.contains(&ApiKeyPermission::Read)
            || granted.contains(&ApiKeyPermission::Write)
            || granted.contains(&ApiKeyPermission::Admin),
        ApiKeyPermission::Write => granted.contains(&ApiKeyPermission::Write)
            || granted.contains(&ApiKeyPermission::Admin),
        ApiKeyPermission::Admin => granted.contains(&ApiKeyPermission::Admin),
    }
}
```

**4. 速率限制：**

```rust
// crates/omninova-core/src/gateway/rate_limit.rs

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// 速率限制配置
pub struct RateLimitConfig {
    pub requests_per_minute: u32,
    pub burst_size: u32,
}

/// 内存速率限制器
pub struct RateLimiter {
    limits: Arc<RwLock<HashMap<String, RateLimitEntry>>>,
    config: RateLimitConfig,
}

struct RateLimitEntry {
    count: AtomicU32,
    reset_at: Instant,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            limits: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    pub async fn check(&self, key: &str) -> Result<RateLimitStatus, RateLimitError> {
        let mut limits = self.limits.write().await;
        let now = Instant::now();

        let entry = limits.entry(key.to_string()).or_insert_with(|| RateLimitEntry {
            count: AtomicU32::new(0),
            reset_at: now + Duration::from_secs(60),
        });

        // 检查是否需要重置窗口
        if now > entry.reset_at {
            entry.count.store(0, Ordering::Relaxed);
            entry.reset_at = now + Duration::from_secs(60);
        }

        let count = entry.count.fetch_add(1, Ordering::Relaxed);

        if count >= self.config.requests_per_minute {
            return Err(RateLimitError::LimitExceeded {
                retry_after: (entry.reset_at - now).as_secs(),
            });
        }

        Ok(RateLimitStatus {
            remaining: self.config.requests_per_minute - count - 1,
            limit: self.config.requests_per_minute,
            reset_at: entry.reset_at,
        })
    }
}
```

**5. 数据库表结构：**

```sql
-- 在 db/migrations.rs 中添加
CREATE TABLE IF NOT EXISTS api_keys (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key_hash TEXT NOT NULL UNIQUE,      -- SHA-256 哈希
    key_prefix TEXT NOT NULL,           -- 前8位
    name TEXT NOT NULL,
    permissions TEXT NOT NULL,          -- JSON 数组
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    last_used_at INTEGER,
    is_revoked INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_api_keys_key_hash ON api_keys(key_hash);
CREATE INDEX idx_api_keys_is_revoked ON api_keys(is_revoked);
```

**6. 路由更新：**

```rust
// 在 serve_http 和 serve_https 中更新路由

// 公开端点（无需认证）
.route("/health", get(http_health))
.route("/api/docs", get(http_api_docs))
.route("/api/docs.json", get(http_api_docs_json))

// 受保护端点（需要认证）
.layer(AuthLayer::new(api_key_store))
.layer(RateLimitLayer::new(rate_limiter))
.route("/api/agents", get(http_api_agents_list).post(http_api_agents_create))
.route("/api/agents/:id", ...)
.route("/api/keys", get(http_api_keys_list).post(http_api_keys_create))
.route("/api/keys/:id", delete(http_api_keys_revoke))
```

### 文件结构

```
crates/omninova-core/src/
├── gateway/
│   ├── mod.rs           # 修改 - 添加认证中间件到路由
│   ├── auth.rs          # 新增 - API Key 认证中间件
│   ├── rate_limit.rs    # 新增 - 速率限制中间件
│   └── openapi.rs       # 修改 - 添加认证相关 schema

├── db/
│   └── migrations.rs    # 修改 - 添加 api_keys 表

apps/omninova-tauri/
├── src-tauri/src/
│   ├── commands/
│   │   ├── api_key.rs   # 新增 - API Key Tauri 命令
│   │   └── mod.rs       # 修改 - 导出 api_key 模块
│   └── lib.rs           # 修改 - 注册命令
│
├── src/
│   ├── pages/settings/
│   │   └── ApiKeySettingsPage.tsx  # 新增 - API Key 管理页面
│   ├── hooks/
│   │   └── useApiKeys.ts            # 新增 - API Key hook
│   └── types/
│       └── api-key.ts               # 新增 - API Key 类型定义
```

### 测试策略

1. **单元测试：**
   - API Key 生成和哈希
   - 权限检查逻辑
   - 速率限制算法

2. **集成测试：**
   - 认证中间件端到端
   - 401/403/429 错误响应
   - API Key 生命周期

3. **安全测试：**
   - 密钥泄露防护
   - 时序攻击防护（使用 constant_time_compare）
   - 速率限制绕过尝试

### Previous Story Intelligence (Story 8.2)

**可复用模式：**
- API 响应格式（ApiResponse, AgentApiError）
- HTTP handler 函数签名
- OpenAPI 文档结构
- 测试组织模式

**注意事项：**
- 使用现有的 AgentApiError 类型
- 错误码扩展：UNAUTHORIZED, FORBIDDEN, RATE_LIMITED
- 保持 JSON 响应格式一致性

### 安全注意事项

1. **密钥存储**：永远不要存储明文密钥，只存储 SHA-256 哈希
2. **密钥传输**：创建时只显示一次完整密钥
3. **时序安全**：使用 constant_time_compare 比较密钥
4. **默认限制**：未认证请求严格限制，认证请求适度限制
5. **审计日志**：记录所有认证失败事件

### References

- [Source: epics.md#Story 8.3] - 原始 story 定义
- [Source: architecture.md#认证与安全架构] - 认证架构设计
- [Source: crates/omninova-core/src/security/keyring.rs] - 现有 Keyring 服务
- [Source: crates/omninova-core/src/security/crypto.rs] - 现有加密服务
- [Source: crates/omninova-core/src/gateway/mod.rs] - 现有 Gateway 实现
- [Source: crates/omninova-core/src/db/migrations.rs] - 数据库迁移

---

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

N/A

### Completion Notes List

**Story 8.3 API 认证与授权已完成:**

1. **API Key 数据模型** - 完整实现:
   - `ApiKeyPermission` 枚举 (Read, Write, Admin) with hierarchy
   - `ApiKey` 结构体 with validation methods
   - `CreateApiKeyRequest`, `ApiKeyCreated`, `ApiKeyInfo` types
   - Database migration 016_api_keys with indexes

2. **API Key Store** - 完整实现:
   - SHA-256 hashing for secure key storage
   - Secure random key generation (32 bytes)
   - Key prefix display (8 chars)
   - Expiration and revocation support
   - Usage tracking (last_used_at)

3. **认证中间件** - 完整实现:
   - `auth_middleware` Axum middleware
   - Bearer Token and X-API-Key header extraction
   - AuthContext injection into request extensions
   - 401 Unauthorized and 403 Forbidden responses

4. **速率限制中间件** - 完整实现:
   - Token bucket algorithm
   - Per-IP rate limiting (unauthenticated)
   - Per-API-key rate limiting (authenticated)
   - 429 Too Many Requests response
   - X-RateLimit-* headers

5. **Tauri 命令** - 完整实现:
   - `init_api_key_store`
   - `create_api_key`
   - `list_api_keys`
   - `revoke_api_key`
   - `delete_gateway_api_key`
   - `get_gateway_api_key`

6. **前端组件** - 完整实现:
   - `ApiKeySettingsPage` - Main settings page
   - `useApiKeys` hook - State management
   - `useCreateApiKeyDialog` hook - Dialog state
   - Create/Revoke/Delete functionality
   - Permission display and usage statistics

7. **单元测试** - 12 tests passing:
   - Permission hierarchy tests
   - Key hashing and generation tests
   - API key validation tests
   - Rate limiter tests

### File List

#### New Files
- `crates/omninova-core/src/gateway/auth.rs` - API Key authentication module
- `crates/omninova-core/src/gateway/rate_limit.rs` - Rate limiting middleware
- `apps/omninova-tauri/src/types/api-key.ts` - TypeScript types
- `apps/omninova-tauri/src/hooks/useApiKeys.ts` - React hooks
- `apps/omninova-tauri/src/pages/settings/ApiKeySettingsPage.tsx` - Settings page

#### Modified Files
- `crates/omninova-core/src/gateway/mod.rs` - Added auth and rate_limit modules
- `crates/omninova-core/src/db/migrations.rs` - Added migration 016_api_keys
- `apps/omninova-tauri/src-tauri/src/lib.rs` - Added API key Tauri commands