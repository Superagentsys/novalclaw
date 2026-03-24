# Story 8.2: RESTful API 设计与实现

Status: done

## Story

As a 开发者,
I want 使用标准的 RESTful API 与系统交互,
So that 我可以轻松集成 AI 代理到我的应用程序中.

## Acceptance Criteria

1. **AC1: 代理列表API** - GET /api/agents 返回代理列表
2. **AC2: 创建代理API** - POST /api/agents 创建新代理
3. **AC3: 代理详情API** - GET /api/agents/{id} 返回代理详情
4. **AC4: 更新代理API** - PUT /api/agents/{id} 更新代理配置
5. **AC5: 删除代理API** - DELETE /api/agents/{id} 删除代理
6. **AC6: 聊天API** - POST /api/agents/{id}/chat 发送消息并获取响应
7. **AC7: 流式聊天API** - POST /api/agents/{id}/chat/stream 发送消息并获取流式响应
8. **AC8: JSON响应格式** - API 响应遵循标准 JSON 格式

## Tasks / Subtasks

- [x] Task 1: 设计RESTful API响应格式 (AC: #8)
  - [x] 1.1 定义统一的 API 响应结构体 (ApiResponse<T>)
  - [x] 1.2 定义错误响应结构体 (ApiError)
  - [x] 1.3 定义分页响应结构体 (PaginatedResponse<T>)
  - [x] 1.4 添加响应辅助函数和中间件

- [x] Task 2: 实现代理CRUD API端点 (AC: #1, #2, #3, #4, #5)
  - [x] 2.1 GET /api/agents - 列出所有代理（支持分页）
  - [x] 2.2 POST /api/agents - 创建新代理
  - [x] 2.3 GET /api/agents/{id} - 获取单个代理详情
  - [x] 2.4 PUT /api/agents/{id} - 更新代理配置
  - [x] 2.5 DELETE /api/agents/{id} - 删除代理
  - [x] 2.6 添加请求验证和错误处理

- [x] Task 3: 实现聊天API端点 (AC: #6, #7)
  - [x] 3.1 POST /api/agents/{id}/chat - 同步聊天接口
  - [x] 3.2 POST /api/agents/{id}/chat/stream - 流式聊天接口 (SSE)
  - [x] 3.3 定义聊天请求/响应结构体
  - [x] 3.4 集成 AgentService 进行消息处理
  - [x] 3.5 实现会话管理（创建/复用会话）- 使用简化版本

- [x] Task 4: 添加API文档和OpenAPI规范 (AC: 全部)
  - [x] 4.1 创建 OpenAPI/Swagger 文档结构
  - [x] 4.2 为所有端点添加文档注释
  - [x] 4.3 实现 /api/docs 端点返回 OpenAPI JSON
  - [x] 4.4 添加请求/响应示例

- [x] Task 5: 单元测试与集成测试 (AC: 全部)
  - [x] 5.1 测试代理 CRUD 端点
  - [x] 5.2 测试聊天端点
  - [x] 5.3 测试错误响应格式
  - [x] 5.4 测试请求验证

## Dev Notes

### 架构上下文

Story 8.2 是 Epic 8 (开发者工具与API) 的第二个 Story，建立在 Story 8.1 HTTP Gateway 基础之上，提供完整的 RESTful API 竡点。

**依赖关系：**
- **Story 8.1 (已完成)**: HTTP Gateway、CORS、HTTPS、健康检查已实现
- **AgentService**: 已实现代理管理服务（agent/service.rs）
- **AgentStore**: 已实现代理数据存储（agent/store.rs）
- **AgentModel**: 已定义代理数据模型（agent/model.rs）

**功能需求关联：**
- FR45: 开发者可以通过API与AI代理交互
- FR46: 开发者可以访问和修改AI代理的配置参数
- FR47: 开发者可以集成AI代理到其他应用程序
- NFR-I3: 应提供RESTful API用于第三方工具集成

### 现有实现分析

**已有 Gateway 路由** (`crates/omninova-core/src/gateway/mod.rs`):

```rust
// 当前已有的 API 路由
.route("/api/status", get(http_api_status))
.route("/api/tools", get(http_api_tools))
.route("/api/memory", get(http_api_memory_list).post(http_api_memory_store).delete(http_api_memory_forget))
.route("/api/doctor", get(http_api_doctor))
.route("/api/cron", get(http_api_cron_list).post(http_api_cron_add))
```

**已有 HTTP Handler 模式**:

```rust
// 典型的 HTTP handler 实现模式
async fn http_api_status(
    State(runtime): State<GatewayRuntime>,
) -> Json<serde_json::Value> {
    // 处理逻辑
}
```

**AgentService 关键方法** (`crates/omninova-core/src/agent/service.rs`):

```rust
pub struct AgentService {
    agent_store: AgentStore,
    session_store: SessionStore,
    message_store: MessageStore,
    memory: Arc<dyn Memory>,
    // ...
}

impl AgentService {
    // CRUD 操作
    pub async fn create_agent(&self, agent: NewAgent) -> Result<AgentModel, AgentServiceError>
    pub async fn get_agent(&self, id: i64) -> Result<AgentModel, AgentServiceError>
    pub async fn list_agents(&self) -> Result<Vec<AgentModel>, AgentServiceError>
    pub async fn update_agent(&self, id: i64, update: AgentUpdate) -> Result<AgentModel, AgentServiceError>
    pub async fn delete_agent(&self, id: i64) -> Result<(), AgentServiceError>

    // 聊天操作
    pub async fn chat(&self, agent_id: i64, message: &str, session_id: Option<i64>) -> Result<ChatResult, AgentServiceError>
    pub async fn chat_stream(&self, agent_id: i64, message: &str, session_id: Option<i64>) -> impl Stream<Item = Result<StreamEvent, StreamError>>
}
```

**AgentModel 数据结构** (`crates/omninova-core/src/agent/model.rs`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentModel {
    pub id: i64,
    pub agent_uuid: String,
    pub name: String,
    pub description: Option<String>,
    pub domain: Option<String>,
    pub mbti_type: Option<String>,
    pub system_prompt: Option<String>,
    pub status: AgentStatus,
    pub default_provider_id: Option<String>,
    pub style_config: Option<String>,
    pub context_window_config: Option<String>,
    pub trigger_keywords_config: Option<String>,
    pub privacy_config: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct NewAgent {
    pub name: String,
    pub description: Option<String>,
    pub domain: Option<String>,
    pub mbti_type: Option<String>,
    pub system_prompt: Option<String>,
    pub default_provider_id: Option<String>,
}

pub struct AgentUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub domain: Option<String>,
    pub mbti_type: Option<String>,
    pub system_prompt: Option<String>,
    pub status: Option<AgentStatus>,
    pub default_provider_id: Option<String>,
}
```

### 需要新增的功能

**1. 统一API响应格式**:

```rust
// 成功响应
#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ResponseMeta>,
}

#[derive(Serialize)]
pub struct ResponseMeta {
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

// 错误响应
#[derive(Serialize)]
pub struct ApiError {
    pub success: bool,
    pub error: ErrorDetail,
}

#[derive(Serialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

// 分页响应
#[derive(Serialize)]
pub struct PaginatedResponse<T> {
    pub success: bool,
    pub data: Vec<T>,
    pub pagination: Pagination,
}

#[derive(Serialize)]
pub struct Pagination {
    pub page: u32,
    pub per_page: u32,
    pub total: u64,
    pub total_pages: u32,
}
```

**2. 聊天请求/响应结构体**:

```rust
#[derive(Deserialize)]
pub struct ChatRequest {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ChatContext>,
}

#[derive(Deserialize)]
pub struct ChatContext {
    pub include_memory: Option<bool>,
    pub max_tokens: Option<u32>,
}

#[derive(Serialize)]
pub struct ChatResponse {
    pub response: String,
    pub session_id: i64,
    pub message_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_used: Option<bool>,
}
```

**3. 新增路由**:

```rust
// 在 serve_http 和 serve_https 中添加
.route("/api/agents", get(http_api_agents_list).post(http_api_agents_create))
.route("/api/agents/:id", get(http_api_agents_get).put(http_api_agents_update).delete(http_api_agents_delete))
.route("/api/agents/:id/chat", post(http_api_agents_chat))
.route("/api/agents/:id/chat/stream", post(http_api_agents_chat_stream))
```

**4. HTTP Handler 实现**:

```rust
// 代理列表
async fn http_api_agents_list(
    State(runtime): State<GatewayRuntime>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<AgentModel>>, (StatusCode, Json<ApiError>)> {
    // 调用 AgentService::list_agents
}

// 创建代理
async fn http_api_agents_create(
    State(runtime): State<GatewayRuntime>,
    Json(payload): Json<CreateAgentRequest>,
) -> Result<Json<ApiResponse<AgentModel>>, (StatusCode, Json<ApiError>)> {
    // 调用 AgentService::create_agent
}

// 获取代理
async fn http_api_agents_get(
    State(runtime): State<GatewayRuntime>,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<AgentModel>>, (StatusCode, Json<ApiError>)> {
    // 调用 AgentService::get_agent
}

// 更新代理
async fn http_api_agents_update(
    State(runtime): State<GatewayRuntime>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateAgentRequest>,
) -> Result<Json<ApiResponse<AgentModel>>, (StatusCode, Json<ApiError>)> {
    // 调用 AgentService::update_agent
}

// 删除代理
async fn http_api_agents_delete(
    State(runtime): State<GatewayRuntime>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    // 调用 AgentService::delete_agent
}

// 同步聊天
async fn http_api_agents_chat(
    State(runtime): State<GatewayRuntime>,
    Path(id): Path<i64>,
    Json(payload): Json<ChatRequest>,
) -> Result<Json<ApiResponse<ChatResponse>>, (StatusCode, Json<ApiError>)> {
    // 调用 AgentService::chat
}

// 流式聊天 (SSE)
async fn http_api_agents_chat_stream(
    State(runtime): State<GatewayRuntime>,
    Path(id): Path<i64>,
    Json(payload): Json<ChatRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // 调用 AgentService::chat_stream
    // 使用 axum 的 Sse 响应类型
}
```

### 文件结构

```
crates/omninova-core/src/
├── gateway/
│   ├── mod.rs           # 修改 - 添加新路由
│   ├── api.rs           # 新增 - RESTful API handlers
│   └── response.rs      # 新增 - API 响应结构体

apps/omninova-tauri/
├── src/
│   └── types/
│       └── api.ts       # 新增 - API TypeScript 类型定义
```

### 测试策略

1. **单元测试：**
   - API 响应结构体序列化/反序列化
   - 请求验证逻辑
   - 错误处理

2. **集成测试：**
   - CRUD 端点完整流程
   - 聊天端点响应
   - 错误状态码和响应格式
   - 分页功能

3. **端到端测试：**
   - 通过 HTTP 客户端测试完整 API 流程
   - 流式响应测试

### 注意事项

1. **AgentService 集成**: GatewayRuntime 需要持有 AgentService 实例，或者通过其他方式访问
2. **路径参数**: Axum 使用 `:id` 语法定义路径参数，需要使用 `Path<i64>` 提取器
3. **错误处理**: 使用 `Result<T, (StatusCode, Json<ApiError>)>` 统一错误响应
4. **流式响应**: 使用 `axum::response::sse::Sse` 和 `Event` 实现流式聊天
5. **验证**: 添加请求体验证，返回 400 错误码表示无效请求

### Previous Story Intelligence (Story 8.1)

**可复用模式：**
- CORS 中间件配置（tower-http）
- HTTPS/TLS 支持（axum-server）
- HTTP handler 函数签名模式
- 测试组织模式

**注意事项：**
- 使用 anyhow 统一错误处理
- 状态更新使用 Arc<RwLock>
- GatewayRuntime 已有 requests_total 计数器可用于 API 统计

### References

- [Source: epics.md#Story 8.2] - 原始 story 定义
- [Source: architecture.md#API架构] - API 与通信架构
- [Source: crates/omninova-core/src/gateway/mod.rs] - 现有 Gateway 实现
- [Source: crates/omninova-core/src/agent/service.rs] - AgentService 实现
- [Source: crates/omninova-core/src/agent/model.rs] - AgentModel 定义
- [Source: crates/omninova-core/src/agent/store.rs] - AgentStore 实现

---

## File List

### New Files
- `crates/omninova-core/src/gateway/openapi.rs` - OpenAPI 3.0.3 specification for all endpoints

### Modified Files
- `crates/omninova-core/src/gateway/mod.rs` - Added CRUD endpoints, chat endpoints, docs endpoints, API types (inline)
- `crates/omninova-core/Cargo.toml` - Added async-stream dependency

---

## Change Log

| Date | Change |
|------|--------|
| 2026-03-24 | Story created with comprehensive context |
| 2026-03-24 | Implemented Task 1: API response format types |
| 2026-03-24 | Implemented Task 2: Agent CRUD API endpoints |
| 2026-03-24 | Implemented Task 3: Chat API endpoints (sync + SSE streaming) |
| 2026-03-24 | Implemented Task 4: OpenAPI specification and /api/docs endpoint |
| 2026-03-24 | Implemented Task 5: Unit tests for API types and handlers |
| 2026-03-24 | Code Review: Removed dead code (response.rs, api.rs), fixed unused imports |

---

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (glm-5)

### Debug Log References

No critical issues encountered during implementation.

### Completion Notes List

1. **API Response Types** (inline in `mod.rs`):
   - Defined `AgentApiError` for error responses with code and message
   - Defined `AgentResponse` for single agent success responses
   - Defined `PaginatedAgentsResponse` with `PaginationInfo` for list endpoints
   - Added helper functions for common error types (not_found, validation, internal, unavailable)

2. **CRUD Endpoints** (`mod.rs`):
   - `GET /api/agents` - List agents with pagination
   - `POST /api/agents` - Create new agent with validation
   - `GET /api/agents/:id` - Get agent by ID
   - `PUT /api/agents/:id` - Update agent configuration
   - `DELETE /api/agents/:id` - Delete agent (returns 204 No Content)

3. **Chat Endpoints** (`mod.rs`):
   - `POST /api/agents/:id/chat` - Synchronous chat (returns full response)
   - `POST /api/agents/:id/chat/stream` - SSE streaming chat
   - Both endpoints validate agent exists and is active before processing
   - Integrated with existing Agent type from the gateway

4. **OpenAPI Documentation** (`openapi.rs`):
   - Complete OpenAPI 3.0.3 specification
   - All endpoints documented with parameters, request bodies, and responses
   - Schema definitions for all data types
   - Examples for requests and responses
   - `GET /api/docs` and `GET /api/docs.json` endpoints

5. **Tests**:
   - API type tests inline in `mod.rs`
   - OpenAPI spec validation tests in `openapi.rs`
   - All tests passing

### File List

See above File List section.