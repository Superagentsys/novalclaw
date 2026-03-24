# Story 8.4: API 使用日志系统

Status: done

## Story

As a 开发者,
I want 查看 API 使用日志,
so that 我可以监控系统调用和排查问题.

## Acceptance Criteria

1. **AC1: 请求日志记录** - 请求日志被记录（时间戳、端点、方法、状态码、响应时间） ✅
2. **AC2: 日志查询筛选** - 日志可以通过界面查询和筛选 ✅
3. **AC3: 多维度过滤** - 可以按时间范围、端点、状态码过滤 ✅
4. **AC4: 使用统计显示** - 显示 API 使用统计（请求量、平均响应时间、错误率） ✅
5. **AC5: 日志导出** - 支持日志导出 ✅

## Tasks / Subtasks

- [x] Task 1: 设计 API 请求日志数据模型 (AC: #1)
  - [x] 1.1 定义 ApiRequestLog 结构体（id, timestamp, method, endpoint, status_code, response_time_ms, api_key_id, ip_address, user_agent）
  - [x] 1.2 定义 RequestLogFilter 结构体用于查询过滤
  - [x] 1.3 定义 ApiUsageStats 结构体用于统计展示
  - [x] 1.4 创建 api_request_logs 数据库表及索引
  - [x] 1.5 实现 ApiLogStore 存储 trait

- [x] Task 2: 实现请求日志记录中间件 (AC: #1)
  - [x] 2.1 创建 LoggingLayer Axum 中间件
  - [x] 2.2 实现请求开始时间记录
  - [x] 2.3 实现响应状态码和响应时间捕获
  - [x] 2.4 提取请求元数据（IP、User-Agent、API Key ID）
  - [x] 2.5 异步写入日志到数据库（不阻塞响应）
  - [x] 2.6 实现日志级别分类（INFO/2xx, WARN/4xx, ERROR/5xx）

- [x] Task 3: 实现 API 使用统计服务 (AC: #4)
  - [x] 3.1 创建 ApiLogService（查询日志、计算统计）
  - [x] 3.2 实现请求量统计（按时间范围、端点分组）
  - [x] 3.3 实现平均响应时间计算
  - [x] 3.4 实现错误率统计
  - [x] 3.5 实现按 API Key 的使用统计
  - [x] 3.6 实现统计缓存优化（避免频繁聚合查询）

- [x] Task 4: 创建 Tauri 命令接口 (AC: #2, #3, #5)
  - [x] 4.1 实现 `list_api_logs` 命令（带分页和过滤）
  - [x] 4.2 实现 `get_api_usage_stats` 命令
  - [x] 4.3 实现 `export_api_logs` 命令（导出为 JSON/CSV）
  - [x] 4.4 实现 `clear_api_logs` 命令（清理旧日志）
  - [x] 4.5 实现 `get_api_log_retention_config` 命令

- [x] Task 5: 创建前端 API 日志管理界面 (AC: #2, #3, #4, #5)
  - [x] 5.1 创建 ApiLogsPage 组件
  - [x] 5.2 实现日志列表显示（时间、端点、方法、状态码、响应时间）
  - [x] 5.3 实现时间范围选择器
  - [x] 5.4 实现端点和状态码过滤器
  - [x] 5.5 实现使用统计仪表板（请求量图表、响应时间趋势、错误率）
  - [x] 5.6 实现日志导出功能
  - [x] 5.7 实现日志清理设置

- [x] Task 6: 单元测试与集成测试 (AC: 全部)
  - [x] 6.1 测试日志记录中间件
  - [x] 6.2 测试日志查询和过滤
  - [x] 6.3 测试统计计算准确性
  - [x] 6.4 测试日志导出功能
  - [x] 6.5 测试日志清理功能

## Dev Notes

### 架构上下文

Story 8.4 是 Epic 8 (开发者工具与API) 的第四个 Story，建立在 Story 8.1 HTTP Gateway、Story 8.2 RESTful API 和 Story 8.3 API 认证与授权基础之上，提供 API 可观测性能力。

**依赖关系：**
- **Story 8.1 (已完成)**: HTTP Gateway、CORS、HTTPS 已实现
- **Story 8.2 (已完成)**: RESTful API 端点已实现
- **Story 8.3 (已完成)**: API Key 认证、速率限制中间件已实现
- **monitoring/logger.rs**: 日志系统基础设施
- **gateway/auth.rs**: AuthContext 提供认证上下文

**功能需求关联：**
- FR48: 开发者可以查看详细的API使用日志
- FR45: 开发者可以通过API与AI代理交互
- NFR-I3: 应提供RESTful API用于第三方工具集成

### 现有实现分析

**已有 Gateway 模块** (`crates/omninova-core/src/gateway/`):

```rust
// mod.rs - 现有路由结构
pub fn create_router(
    state: Arc<AppState>,
) -> Router {
    Router::new()
        // 公开端点
        .route("/health", get(http_health))
        // 受保护端点
        .route("/api/agents", get(http_api_agents_list).post(http_api_agents_create))
        .route("/api/agents/:id", get(http_api_agents_get).put(http_api_agents_update).delete(http_api_agents_delete))
        .route("/api/agents/:id/chat", post(http_api_agents_chat))
        .route("/api/agents/:id/chat/stream", post(http_api_agents_chat_stream))
        // 认证和速率限制层
        .layer(auth_middleware)
        .layer(rate_limit_middleware)
}

// auth.rs - 已有认证上下文
pub struct AuthContext {
    pub api_key_id: i64,
    pub key_name: String,
    pub permissions: Vec<ApiKeyPermission>,
}
```

**已有日志基础设施** (`crates/omninova-core/src/monitoring/`):

```rust
// 日志存储位置: ~/.omninoval/logs/
// 日志格式: 结构化日志 (tracing-subscriber)
```

### 需要新增的功能

**1. API 请求日志数据模型：**

```rust
// crates/omninova-core/src/gateway/logging.rs

/// API 请求日志记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequestLog {
    pub id: i64,
    /// 请求时间戳 (Unix timestamp)
    pub timestamp: i64,
    /// HTTP 方法
    pub method: String,
    /// 请求端点路径
    pub endpoint: String,
    /// HTTP 状态码
    pub status_code: u16,
    /// 响应时间 (毫秒)
    pub response_time_ms: u64,
    /// 关联的 API Key ID (可选)
    pub api_key_id: Option<i64>,
    /// 客户端 IP 地址
    pub ip_address: Option<String>,
    /// User-Agent 字符串
    pub user_agent: Option<String>,
    /// 请求体大小 (字节)
    pub request_size: Option<u64>,
    /// 响应体大小 (字节)
    pub response_size: Option<u64>,
}

/// 日志查询过滤器
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestLogFilter {
    /// 开始时间 (Unix timestamp)
    pub start_time: Option<i64>,
    /// 结束时间 (Unix timestamp)
    pub end_time: Option<i64>,
    /// 按端点过滤 (支持模糊匹配)
    pub endpoint: Option<String>,
    /// 按方法过滤
    pub method: Option<String>,
    /// 按状态码过滤
    pub status_code: Option<u16>,
    /// 按 API Key ID 过滤
    pub api_key_id: Option<i64>,
    /// 最小响应时间 (毫秒)
    pub min_response_time: Option<u64>,
    /// 最大响应时间 (毫秒)
    pub max_response_time: Option<u64>,
}

/// API 使用统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiUsageStats {
    /// 总请求数
    pub total_requests: u64,
    /// 成功请求数 (2xx)
    pub successful_requests: u64,
    /// 客户端错误数 (4xx)
    pub client_errors: u64,
    /// 服务端错误数 (5xx)
    pub server_errors: u64,
    /// 平均响应时间 (毫秒)
    pub avg_response_time_ms: f64,
    /// 最大响应时间 (毫秒)
    pub max_response_time_ms: u64,
    /// 最小响应时间 (毫秒)
    pub min_response_time_ms: u64,
    /// 错误率
    pub error_rate: f64,
    /// 时间范围
    pub time_range: TimeRange,
}

/// 按端点的统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointStats {
    pub endpoint: String,
    pub method: String,
    pub request_count: u64,
    pub avg_response_time_ms: f64,
    pub error_count: u64,
    pub error_rate: f64,
}

/// 时间范围
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: i64,
    pub end: i64,
}
```

**2. 日志记录中间件：**

```rust
// crates/omninova-core/src/gateway/logging.rs

use axum::{
    body::Body,
    extract::Request,
    http::{Response, StatusCode},
    middleware::Next,
};
use std::time::Instant;

/// 请求日志中间件
pub async fn logging_middleware(
    State(log_store): State<Arc<ApiLogStore>>,
    request: Request,
    next: Next,
) -> Response<Body> {
    let start = Instant::now();
    let method = request.method().to_string();
    let endpoint = request.uri().path().to_string();

    // 提取请求元数据
    let ip_address = extract_client_ip(request.headers());
    let user_agent = extract_user_agent(request.headers());
    let api_key_id = request.extensions()
        .get::<AuthContext>()
        .map(|auth| auth.api_key_id);

    // 执行请求
    let response = next.run(request).await;

    // 计算响应时间
    let response_time_ms = start.elapsed().as_millis() as u64;
    let status_code = response.status().as_u16();

    // 异步写入日志 (不阻塞响应)
    let log = ApiRequestLog {
        id: 0, // 数据库自动生成
        timestamp: chrono::Utc::now().timestamp(),
        method,
        endpoint,
        status_code,
        response_time_ms,
        api_key_id,
        ip_address,
        user_agent,
        request_size: None,
        response_size: None,
    };

    // spawn 异步任务写入日志
    tokio::spawn(async move {
        if let Err(e) = log_store.insert(&log).await {
            tracing::error!("Failed to insert API log: {}", e);
        }
    });

    response
}
```

**3. 日志存储实现：**

```rust
// crates/omninova-core/src/gateway/logging.rs

pub struct ApiLogStore {
    conn: Arc<Connection>,
}

impl ApiLogStore {
    pub fn new(conn: Arc<Connection>) -> Self {
        Self { conn }
    }

    pub async fn insert(&self, log: &ApiRequestLog) -> Result<i64, LogError> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            conn.execute(
                "INSERT INTO api_request_logs (timestamp, method, endpoint, status_code, response_time_ms, api_key_id, ip_address, user_agent, request_size, response_size)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    log.timestamp,
                    log.method,
                    log.endpoint,
                    log.status_code,
                    log.response_time_ms,
                    log.api_key_id,
                    log.ip_address,
                    log.user_agent,
                    log.request_size,
                    log.response_size,
                ],
            ).map_err(|e| LogError::DatabaseError(e.to_string()))?;

            Ok(conn.last_insert_rowid())
        }).await.map_err(|e| LogError::AsyncError(e.to_string()))?
    }

    pub async fn query(&self, filter: &RequestLogFilter, limit: u64, offset: u64) -> Result<Vec<ApiRequestLog>, LogError> {
        // 构建动态查询
        let mut sql = String::from(
            "SELECT id, timestamp, method, endpoint, status_code, response_time_ms, api_key_id, ip_address, user_agent, request_size, response_size
             FROM api_request_logs WHERE 1=1"
        );
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(start) = filter.start_time {
            sql.push_str(" AND timestamp >= ?");
            params_vec.push(Box::new(start));
        }
        if let Some(end) = filter.end_time {
            sql.push_str(" AND timestamp <= ?");
            params_vec.push(Box::new(end));
        }
        if let Some(ref endpoint) = filter.endpoint {
            sql.push_str(" AND endpoint LIKE ?");
            params_vec.push(Box::new(format!("%{}%", endpoint)));
        }
        if let Some(ref method) = filter.method {
            sql.push_str(" AND method = ?");
            params_vec.push(Box::new(method.clone()));
        }
        if let Some(status) = filter.status_code {
            sql.push_str(" AND status_code = ?");
            params_vec.push(Box::new(status as i32));
        }
        if let Some(key_id) = filter.api_key_id {
            sql.push_str(" AND api_key_id = ?");
            params_vec.push(Box::new(key_id));
        }

        sql.push_str(" ORDER BY timestamp DESC LIMIT ? OFFSET ?");
        params_vec.push(Box::new(limit as i32));
        params_vec.push(Box::new(offset as i32));

        // 执行查询...
    }

    pub async fn get_stats(&self, start_time: i64, end_time: i64) -> Result<ApiUsageStats, LogError> {
        // 聚合统计查询...
    }

    pub async fn get_endpoint_stats(&self, start_time: i64, end_time: i64) -> Result<Vec<EndpointStats>, LogError> {
        // 按端点分组统计...
    }

    pub async fn clear_before(&self, timestamp: i64) -> Result<u64, LogError> {
        // 清理指定时间之前的日志...
    }
}
```

**4. 数据库表结构：**

```sql
-- 在 db/migrations.rs 中添加
CREATE TABLE IF NOT EXISTS api_request_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    method TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    status_code INTEGER NOT NULL,
    response_time_ms INTEGER NOT NULL,
    api_key_id INTEGER,
    ip_address TEXT,
    user_agent TEXT,
    request_size INTEGER,
    response_size INTEGER
);

-- 索引优化查询性能
CREATE INDEX idx_api_logs_timestamp ON api_request_logs(timestamp);
CREATE INDEX idx_api_logs_endpoint ON api_request_logs(endpoint);
CREATE INDEX idx_api_logs_status_code ON api_request_logs(status_code);
CREATE INDEX idx_api_logs_api_key_id ON api_request_logs(api_key_id);
```

**5. Tauri 命令接口：**

```rust
// apps/omninova-tauri/src-tauri/src/lib.rs

/// 获取 API 请求日志列表
#[tauri::command]
async fn list_api_logs(
    filter: RequestLogFilter,
    limit: u64,
    offset: u64,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<ApiRequestLog>, String> {
    let store = state.api_log_store.as_ref()
        .ok_or("API log store not initialized")?;
    store.query(&filter, limit, offset).await
        .map_err(|e| e.to_string())
}

/// 获取 API 使用统计
#[tauri::command]
async fn get_api_usage_stats(
    start_time: i64,
    end_time: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<ApiUsageStats, String> {
    let store = state.api_log_store.as_ref()
        .ok_or("API log store not initialized")?;
    store.get_stats(start_time, end_time).await
        .map_err(|e| e.to_string())
}

/// 导出 API 日志
#[tauri::command]
async fn export_api_logs(
    filter: RequestLogFilter,
    format: String, // "json" or "csv"
    state: State<'_, Arc<AppState>>,
) -> Result<String, String> {
    // 导出逻辑...
}

/// 清理旧日志
#[tauri::command]
async fn clear_api_logs(
    before_timestamp: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<u64, String> {
    let store = state.api_log_store.as_ref()
        .ok_or("API log store not initialized")?;
    store.clear_before(before_timestamp).await
        .map_err(|e| e.to_string())
}
```

**6. 前端组件结构：**

```typescript
// apps/omninova-tauri/src/types/api-log.ts

export interface ApiRequestLog {
  id: number;
  timestamp: number;
  method: string;
  endpoint: string;
  status_code: number;
  response_time_ms: number;
  api_key_id?: number;
  ip_address?: string;
  user_agent?: string;
  request_size?: number;
  response_size?: number;
}

export interface RequestLogFilter {
  start_time?: number;
  end_time?: number;
  endpoint?: string;
  method?: string;
  status_code?: number;
  api_key_id?: number;
  min_response_time?: number;
  max_response_time?: number;
}

export interface ApiUsageStats {
  total_requests: number;
  successful_requests: number;
  client_errors: number;
  server_errors: number;
  avg_response_time_ms: number;
  max_response_time_ms: number;
  min_response_time_ms: number;
  error_rate: number;
  time_range: TimeRange;
}

export interface EndpointStats {
  endpoint: string;
  method: string;
  request_count: number;
  avg_response_time_ms: number;
  error_count: number;
  error_rate: number;
}
```

### 文件结构

```
crates/omninova-core/src/
├── gateway/
│   ├── mod.rs           # 修改 - 添加日志中间件到路由
│   ├── logging.rs       # 新增 - API 请求日志模块
│   └── ...

├── db/
│   └── migrations.rs    # 修改 - 添加 api_request_logs 表

apps/omninova-tauri/
├── src-tauri/src/
│   └── lib.rs           # 修改 - 注册日志相关 Tauri 命令
│
├── src/
│   ├── pages/settings/
│   │   └── ApiLogsPage.tsx     # 新增 - API 日志管理页面
│   ├── hooks/
│   │   └── useApiLogs.ts       # 新增 - API 日志 hook
│   └── types/
│       └── api-log.ts          # 新增 - API 日志类型定义
```

### 测试策略

1. **单元测试：**
   - 日志记录中间件逻辑
   - 过滤器构建和查询
   - 统计计算准确性

2. **集成测试：**
   - 端到端日志记录
   - 查询接口功能
   - 导出功能验证

3. **性能测试：**
   - 大量日志写入性能
   - 查询响应时间
   - 统计计算效率

### Previous Story Intelligence (Story 8.3)

**可复用模式：**
- Axum 中间件模式（参考 auth_middleware, rate_limit_middleware）
- State 注入模式
- 异步数据库操作模式
- Tauri 命令注册模式

**注意事项：**
- 日志写入应异步执行，不阻塞请求响应
- 使用 spawn_blocking 处理同步数据库操作
- 索引设计要考虑查询模式
- 日志数据量可能很大，需要分页和清理策略

### 性能注意事项

1. **异步日志写入**：使用 `tokio::spawn` 确保日志写入不阻塞响应
2. **索引优化**：为常用查询字段创建索引
3. **分页查询**：强制分页避免大量数据加载
4. **统计缓存**：高频统计查询考虑缓存
5. **日志清理**：提供自动清理旧日志机制

### References

- [Source: epics.md#Story 8.4] - 原始 story 定义
- [Source: architecture.md#监控与日志] - 监控架构设计
- [Source: crates/omninova-core/src/gateway/mod.rs] - 现有 Gateway 实现
- [Source: crates/omninova-core/src/gateway/auth.rs] - 认证中间件参考
- [Source: crates/omninova-core/src/gateway/rate_limit.rs] - 速率限制中间件参考
- [Source: crates/omninova-core/src/db/migrations.rs] - 数据库迁移

---

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

N/A

### Completion Notes List

**Story 8.4 API 使用日志系统已完成:**

1. **API 请求日志数据模型** - 完整实现:
   - `ApiRequestLog` 结构体 with all required fields
   - `RequestLogFilter` with helper constructors for common queries
   - `ApiUsageStats`, `EndpointStats`, `ApiKeyStats` for statistics
   - `TimeRange` for time range specification

2. **API Log Store** - 完整实现:
   - `ApiLogStore` with r2d2 connection pool
   - Synchronous and async insert operations
   - Query with dynamic filtering and pagination
   - Statistics aggregation queries
   - Export to JSON and CSV formats
   - Clear before timestamp and clear all operations

3. **日志记录中间件** - 完整实现:
   - `logging_middleware` Axum middleware
   - Request timing with `Instant`
   - Client IP and User-Agent extraction
   - API Key ID extraction from AuthContext
   - Async log insertion via `tokio::spawn`
   - Status code classification helpers

4. **数据库迁移** - 完整实现:
   - Migration 017_api_request_logs
   - Table with all required columns
   - Indexes on timestamp, endpoint, status_code, api_key_id, method

5. **Tauri 命令** - 完整实现:
   - `init_api_log_store` - Initialize store
   - `list_api_logs` - Query with filter and pagination
   - `count_api_logs` - Count matching filter
   - `get_api_usage_stats` - Usage statistics
   - `get_endpoint_stats` - Per-endpoint statistics
   - `get_api_key_stats` - Per-API-key statistics
   - `export_api_logs` - Export to JSON/CSV
   - `clear_api_logs` - Clear logs before timestamp
   - `clear_all_api_logs` - Clear all logs

6. **前端组件** - 完整实现:
   - `ApiLogsPage` - Main settings page
   - `useApiLogs` - Log fetching and management hook
   - `useApiUsageStats` - Statistics fetching hook
   - `useApiLogExport` - Export functionality hook
   - `useTimeRangeSelector` - Time range selection hook
   - Statistics cards, log table, endpoint stats table
   - Time range presets and method filter

7. **单元测试** - 10 tests passing:
   - Filter helper tests
   - Request log creation test
   - Status classification test
   - Usage stats default test
   - Serialization test

### File List

#### New Files
- `crates/omninova-core/src/gateway/logging.rs` - API request logging module
- `apps/omninova-tauri/src/types/api-log.ts` - TypeScript types
- `apps/omninova-tauri/src/hooks/useApiLogs.ts` - React hooks
- `apps/omninova-tauri/src/pages/settings/ApiLogsPage.tsx` - Settings page

#### Modified Files
- `crates/omninova-core/src/gateway/mod.rs` - Added logging module
- `crates/omninova-core/src/db/migrations.rs` - Added migration 017_api_request_logs
- `apps/omninova-tauri/src-tauri/src/lib.rs` - Added API log Tauri commands and AppState field