# OmniNova Claw 架构概述

本文档介绍 OmniNova Claw 的系统架构设计。

## 系统架构

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Desktop App (Tauri)                          │
│                    React 19 + TypeScript UI                         │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                         Gateway (HTTP/WS)                           │
│                      axum + tower-http                               │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                        Core Runtime                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────┐   │
│  │  Agent   │  │  Memory  │  │  Tools   │  │     Skills       │   │
│  │Dispatcher│  │  System  │  │ Registry │  │    Manager       │   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────────────┘   │
│                                                                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────┐   │
│  │ Providers│  │ Channels │  │  Config  │  │     Session      │   │
│  │Registry  │  │ Manager  │  │  Manager │  │     Manager      │   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                        Storage Layer                                │
│     SQLite (Data)     │     FileSystem (Skills/Config)              │
└─────────────────────────────────────────────────────────────────────┘
```

## 核心模块

### 1. Agent System (`agent/`)

Agent 系统是 OmniNova 的核心，负责处理对话和调用工具。

```rust
// Agent 核心结构
pub struct Agent {
    pub id: String,
    pub name: String,
    pub personality: Personality,      // MBTI 性格
    pub provider: ProviderConfig,       // LLM 提供商
    pub tools: Vec<Tool>,               // 可用工具
    pub memory: MemoryConfig,           // 记忆配置
}
```

**关键组件：**
- **Dispatcher**: 消息分发，路由到对应 Agent
- **Personality**: 基于 MBTI 的性格系统
- **Tool Executor**: 工具调用执行器

### 2. Memory System (`memory/`)

三层记忆系统，模拟人类认知架构：

```rust
pub enum MemoryLayer {
    /// 工作记忆 - 当前对话上下文
    Working {
        context: Vec<Message>,
        max_tokens: usize,
    },
    /// 情景记忆 - 历史会话记录
    Episodic {
        sessions: Vec<SessionSummary>,
        retention_days: u32,
    },
    /// 语义记忆 - 知识库
    Semantic {
        skills: Vec<SkillKnowledge>,
        documents: Vec<Document>,
    },
}
```

**记忆压缩策略：**
- Sliding Window: 滑动窗口保留最近 N 条消息
- Summarization: 对旧消息进行摘要压缩
- Relevance Scoring: 基于相关性评分决定保留

### 3. Skills System (`skills/`)

可扩展的技能系统，兼容 OpenClaw Skills 格式。

```rust
pub struct Skill {
    pub name: String,
    pub version: String,
    pub description: String,
    pub trigger: SkillTrigger,          // 触发条件
    pub tools: Vec<ToolDefinition>,     // 提供的工具
    pub personality_mod: PersonalityMod, // 性格修正
}
```

**Skill 生命周期：**
1. **Install**: 从目录或 Git 安装
2. **Validate**: 验证 SKILL.md 结构
3. **Load**: 加载到运行时
4. **Activate**: 激活并注册工具
5. **Unload**: 卸载并清理

### 4. Providers (`providers/`)

统一的 LLM 提供商接口：

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    async fn complete(&self, request: CompletionRequest) 
        -> Result<CompletionResponse>;
    
    async fn stream(&self, request: CompletionRequest) 
        -> Result<BoxStream<CompletionChunk>>;
    
    fn models(&self) -> Vec<ModelInfo>;
}
```

**已支持的提供商：**
- OpenAI (GPT-4, GPT-4o, o1, o3)
- Anthropic (Claude 3.5/4)
- Google (Gemini 2.0)
- DeepSeek (V3, R1)
- Qwen (通义千问)
- Ollama (本地模型)

### 5. Channels (`channels/`)

多渠道消息接入系统：

```rust
pub trait Channel: Send + Sync {
    async fn connect(&mut self) -> Result<()>;
    async fn send(&self, message: OutgoingMessage) -> Result<()>;
    fn channel_type(&self) -> ChannelType;
}
```

**已支持的渠道：**
- Slack (Webhook + Socket Mode)
- Discord
- Telegram
- WeChat (企业微信)
- Feishu / Lark
- DingTalk
- WhatsApp
- Email (SMTP/IMAP)
- Webhook (通用)

### 6. Gateway (`gateway/`)

HTTP API 网关，提供 REST 和 WebSocket 接口。

```rust
// 主要路由
pub fn routes() -> Router {
    Router::new()
        // Agent API
        .route("/api/v1/agents", get(list_agents).post(create_agent))
        .route("/api/v1/agents/:id", get(get_agent).put(update_agent).delete(delete_agent))
        
        // Chat API
        .route("/api/v1/sessions", post(create_session))
        .route("/api/v1/sessions/:id/messages", post(send_message).get(get_history))
        
        // Skills API
        .route("/api/v1/skills", get(list_skills).post(install_skill))
        .route("/api/v1/skills/:name", get(get_skill).delete(uninstall_skill))
        
        // System API
        .route("/api/v1/status", get(get_status))
        .route("/api/v1/health", get(health_check))
}
```

## 数据流

### 消息处理流程

```
用户消息 → Channel → Dispatcher → Agent
                                    │
                                    ▼
                              Memory Load (Working + Episodic + Semantic)
                                    │
                                    ▼
                              LLM Provider (Completion/Stream)
                                    │
                                    ▼
                              Tool Execution (如果需要)
                                    │
                                    ▼
                              Response Generation
                                    │
                                    ▼
                              Memory Store
                                    │
                                    ▼
                              Channel → 用户
```

### Agent 路由策略

```rust
pub struct RoutingRule {
    pub channel: Option<ChannelType>,
    pub user_pattern: Option<Regex>,
    pub metadata_match: Option<HashMap<String, Value>>,
    pub agent_id: String,
}
```

路由示例：
- Slack 消息 → slack-agent
- 来自特定用户的请求 → personal-agent
- 带有 `skill: code` 元数据 → coder-agent

## 安全机制

### 1. E-Stop (紧急停止)

```rust
pub struct EStop {
    enabled: AtomicBool,
    reason: Option<String>,
}

impl EStop {
    pub fn trigger(&self, reason: String) {
        self.enabled.store(true, Ordering::SeqCst);
        // 立即停止所有操作
    }
}
```

### 2. 工具策略

```rust
pub struct ToolPolicy {
    pub allowed_tools: HashSet<String>,
    pub denied_tools: HashSet<String>,
    pub sandbox_mode: bool,           // 沙箱模式
    pub require_confirmation: bool,   // 需要确认
}
```

### 3. 敏感数据过滤

- 自动检测 API Key、密码等敏感信息
- 日志脱敏
- 响应过滤

## 配置系统

配置采用 TOML 格式，分层加载：

```rust
pub struct Config {
    pub server: ServerConfig,
    pub providers: HashMap<String, ProviderConfig>,
    pub channels: HashMap<String, ChannelConfig>,
    pub agents: Vec<AgentConfig>,
    pub skills: SkillsConfig,
    pub security: SecurityConfig,
}
```

**加载优先级：**
1. 环境变量 (`OMNINOVA_*`)
2. 命令行参数 (`--config`, `--server`)
3. 用户配置 (`~/.omninoval/config.toml`)
4. 默认配置

## 扩展点

### 添加新 Provider

```rust
pub struct MyProvider {
    client: reqwest::Client,
    config: MyConfig,
}

#[async_trait]
impl Provider for MyProvider {
    async fn complete(&self, request: CompletionRequest) 
        -> Result<CompletionResponse> {
        // 实现完成逻辑
    }
}

// 注册
registry.register("my-provider", Box::new(MyProvider::new));
```

### 添加新 Channel

```rust
pub struct MyChannel {
    // ...
}

#[async_trait]
impl Channel for MyChannel {
    async fn connect(&mut self) -> Result<()> {
        // 实现连接逻辑
    }
}

// 注册
channel_manager.register(ChannelType::MyChannel, Box::new(MyChannel::new));
```

### 添加新 Skill

创建 `SKILL.md`：

```markdown
# My Skill

name: my-skill
version: 1.0.0
description: A custom skill

## Tools

### my_tool
Description: Does something useful
Parameters:
  - input (string, required): Input data

## Triggers
- pattern: "use my-skill"
```

安装：
```bash
omninova skills install /path/to/my-skill
```

## 性能考量

### 并发模型

- 基于 Tokio 异步运行时
- 每个 Agent 独立 task
- Channel 连接池管理

### 内存管理

- 工作记忆 Token 限制
- 历史消息压缩
- Skill 按需加载

### 数据库

- SQLite 连接池 (r2d2)
- WAL 模式提升并发
- 定期 vacuum

## 版本

- 架构版本: 1.0
- 最后更新: 2026-03-25