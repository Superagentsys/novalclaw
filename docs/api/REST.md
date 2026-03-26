# OmniNova Claw REST API 参考

## 概述

OmniNova Claw 提供完整的 REST API，用于管理 Agents、Skills 和对话。

- **Base URL**: `http://localhost:8080/api/v1`
- **Content-Type**: `application/json`
- **认证**: Bearer Token (可选)

## 认证

### 获取 Token

```http
POST /auth/token
Content-Type: application/json

{
  "username": "admin",
  "password": "your-password"
}
```

**响应**:
```json
{
  "token": "eyJhbGciOiJIUzI1NiIs...",
  "expires_at": "2026-03-26T00:00:00Z"
}
```

### 使用 Token

```http
Authorization: Bearer eyJhbGciOiJIUzI1NiIs...
```

---

## Agents API

### 列出所有 Agents

```http
GET /agents
```

**响应**:
```json
{
  "agents": [
    {
      "id": "agent-001",
      "name": "小助手",
      "description": "通用助手 Agent",
      "personality": {
        "mbti": "ENFP",
        "traits": ["friendly", "helpful"]
      },
      "provider": "openai",
      "model": "gpt-4",
      "enabled": true,
      "created_at": "2026-03-25T10:00:00Z"
    }
  ],
  "total": 1
}
```

### 获取单个 Agent

```http
GET /agents/{id}
```

### 创建 Agent

```http
POST /agents
Content-Type: application/json

{
  "name": "新助手",
  "description": "助手描述",
  "personality": {
    "mbti": "INFP"
  },
  "provider": "openai",
  "model": "gpt-4"
}
```

### 更新 Agent

```http
PUT /agents/{id}
Content-Type: application/json

{
  "name": "更新的名称",
  "description": "更新的描述"
}
```

### 删除 Agent

```http
DELETE /agents/{id}
```

### 切换 Agent 状态

```http
PATCH /agents/{id}/toggle
```

---

## Chat API

### 创建会话

```http
POST /sessions
Content-Type: application/json

{
  "agent_id": "agent-001"
}
```

**响应**:
```json
{
  "session_id": "session-123",
  "agent_id": "agent-001",
  "created_at": "2026-03-25T10:00:00Z"
}
```

### 发送消息

```http
POST /sessions/{session_id}/messages
Content-Type: application/json

{
  "content": "你好！",
  "role": "user"
}
```

**响应 (流式)**:
```
data: {"type": "token", "content": "你"}
data: {"type": "token", "content": "好"}
data: {"type": "done"}
```

### 获取会话历史

```http
GET /sessions/{session_id}/messages
```

---

## Skills API

### 列出 Skills

```http
GET /skills
```

### 安装 Skill

```http
POST /skills/install
Content-Type: application/json

{
  "source": "/path/to/skill",
  "name": "my-skill"
}
```

### 卸载 Skill

```http
DELETE /skills/{name}
```

---

## Channels API

### 列出连接渠道

```http
GET /channels
```

### 配置渠道

```http
PUT /channels/{channel_type}
Content-Type: application/json

{
  "enabled": true,
  "config": {
    "webhook_url": "https://..."
  }
}
```

---

## 系统状态

### 获取系统状态

```http
GET /status
```

**响应**:
```json
{
  "version": "0.1.0",
  "uptime": "2h 30m",
  "agents": {
    "total": 5,
    "active": 3
  },
  "memory": {
    "used_mb": 256,
    "available_mb": 768
  }
}
```

---

## 错误响应

所有错误响应遵循统一格式：

```json
{
  "error": {
    "code": "AGENT_NOT_FOUND",
    "message": "Agent with id 'agent-001' not found",
    "details": {}
  }
}
```

### 常见错误码

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `INVALID_REQUEST` | 400 | 请求参数无效 |
| `UNAUTHORIZED` | 401 | 未授权 |
| `AGENT_NOT_FOUND` | 404 | Agent 不存在 |
| `INTERNAL_ERROR` | 500 | 内部服务器错误 |

---

## 速率限制

API 默认速率限制：100 请求/分钟

响应头包含限制信息：
```
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 95
X-RateLimit-Reset: 1648214400
```

---

## 版本

- API 版本: v1
- 最后更新: 2026-03-25