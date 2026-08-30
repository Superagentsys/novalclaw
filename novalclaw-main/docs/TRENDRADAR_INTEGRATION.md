# TrendRadar 集成到 OmniNova Claw 技术文档

> 版本: 1.0  
> 日期: 2026-06-14  
> 状态: 第一阶段已完成，第二/三阶段待实施

---

## 目录

1. [项目概述](#1-项目概述)
2. [两项目对比分析](#2-两项目对比分析)
3. [集成架构设计](#3-集成架构设计)
4. [分阶段实施计划](#4-分阶段实施计划)
5. [第一阶段成果](#5-第一阶段成果)
6. [第二阶段：MCP 协议桥接](#6-第二阶段mcp-协议桥接)
7. [第三阶段：Rust 原生重写（可选）](#7-第三阶段rust-原生重写可选)
8. [部署与运维指南](#8-部署与运维指南)
9. [风险与注意事项](#9-风险与注意事项)
10. [附录](#10-附录)

---

## 1. 项目概述

### 1.1 TrendRadar

| 属性 | 值 |
|------|-----|
| 全称 | TrendRadar - 热点新闻聚合与分析工具 |
| 版本 | v6.9.1 |
| 仓库 | <https://github.com/sansan0/TrendRadar> |
| 语言 | Python 3.12+ |
| 许可证 | GPL-3.0 |
| 核心能力 | 多平台热榜爬取、关键词/AI 筛选、多渠道推送、MCP Server |

**架构**：
```
TrendRadar-master/
├── trendradar/              # 核心引擎
│   ├── crawler/             #   爬虫（热榜 + RSS + Twitter）
│   ├── ai/                  #   AI 分析/筛选/翻译（LiteLLM）
│   ├── core/                #   配置/调度/频率分析
│   ├── notification/        #   通知分发（9 渠道）
│   ├── storage/             #   存储（SQLite + S3）
│   └── report/              #   HTML 报告生成
├── mcp_server/              # MCP Server（FastMCP 2.0）
│   ├── server.py            #   主入口，26 工具注册
│   ├── tools/               #   工具实现
│   │   ├── data_query.py    #     基础查询（4 工具）
│   │   ├── analytics.py     #     高级分析（6 工具）
│   │   ├── search_tools.py  #     智能检索（2 工具）
│   │   ├── config_mgmt.py   #     配置管理（1 工具）
│   │   ├── system.py        #     系统管理（3 工具）
│   │   ├── storage_sync.py  #     存储同步（3 工具）
│   │   ├── article_reader.py #    文章读取（2 工具）
│   │   └── notification.py  #     通知推送（3 工具）
│   └── services/            #   业务服务层
├── config/                  # 配置文件
│   ├── config.yaml          #   主配置（热榜/通知/AI/存储）
│   ├── frequency_words.txt  #   关注词
│   └── timeline.yaml        #   调度时间线
└── output/                  # 数据输出
    ├── news/*.db            #   SQLite 数据库
    └── html/                #   HTML 报告
```

### 1.2 OmniNova Claw

| 属性 | 值 |
|------|-----|
| 全称 | OmniNova Claw - 下一代 AI Agent 平台 |
| 版本 | v0.1.0 |
| 语言 | Rust (核心) + TypeScript (桌面) + Swift/Kotlin (移动) |
| 许可证 | MIT / Apache-2.0 |
| 核心能力 | Agent 运行时、技能系统、三层记忆、多渠道、桌面控制中心 |

**相关架构**：
```
novalclaw-main/
├── crates/omninova-core/src/
│   ├── agent/           # Agent 调度与提示词
│   ├── skills/mod.rs    # 技能加载（SKILL.md 格式）
│   ├── tools/           # 内置工具（Shell/File/Web/Browser 等）
│   ├── acp/             # Agent Control Protocol
│   ├── channels/        # 渠道适配器
│   ├── cron/            # 定时调度
│   ├── gateway/         # HTTP API 网关
│   ├── memory/          # 三层记忆系统
│   ├── providers/       # LLM 供应商
│   └── security/        # 安全沙箱
├── skills/              # 技能包目录
│   ├── phone-call-assistant/
│   ├── financial-analysis/
│   └── trendradar-news/     # 【新增】
└── apps/                # 桌面/移动应用
```

---

## 2. 两项目对比分析

### 2.1 功能重合与互补

| 维度 | TrendRadar | OmniNova Claw | 关系 |
|------|-----------|---------------|------|
| 新闻采集 | ✅ 11+ 热榜 + RSS | ❌ | **互补** |
| AI 分析 | ✅ 趋势/情感/聚合 | ✅ Agent 通用推理 | **互补** |
| Agent 运行时 | ❌ | ✅ | **互补** |
| 技能系统 | ❌ | ✅ SKILL.md | **互补** |
| MCP 协议 | ✅ FastMCP 2.0 | ✅ ACP 协议 | **可桥接** |
| 通知推送 | ✅ 9 渠道 | ✅ 8 渠道 | **可合并** |
| 定时调度 | ✅ timeline.yaml | ✅ cron 模块 | **可统一** |
| 桌面应用 | ❌ | ✅ Tauri + React | **增强** |
| 移动客户端 | ❌ | ✅ iOS + Android | **增强** |
| 记忆系统 | ❌ | ✅ 三层记忆 | **增强** |
| 安全沙箱 | ❌ | ✅ E-Stop + 审计 | **增强** |

### 2.2 技术栈对比

| 维度 | TrendRadar | OmniNova Claw |
|------|-----------|---------------|
| 核心语言 | Python 3.12+ | Rust 2021 |
| 前端 | 无（HTML 报告） | React 19 + TypeScript |
| 包管理 | pip / uv | Cargo + npm |
| 配置格式 | YAML | TOML |
| 数据存储 | SQLite + S3 | 文件系统 + 记忆后端 |
| LLM 调用 | LiteLLM（多供应商） | 自建 provider 模块 |
| 协议 | MCP (JSON-RPC) | ACP (类 MCP) |

---

## 3. 集成架构设计

### 3.1 总体架构

```
┌──────────────────────────────────────────────────────────────┐
│                     OmniNova Claw                            │
│                                                              │
│  ┌──────────┐   ┌──────────┐   ┌──────────────────────┐     │
│  │ Desktop  │   │  Gateway │   │   Agent Runtime      │     │
│  │ (Tauri)  │◄──┤ (HTTP)   │◄──┤                       │     │
│  └──────────┘   └────┬─────┘   │  ┌────────────────┐  │     │
│                      │         │  │ Skill Loader   │  │     │
│                      │         │  │ skills/        │  │     │
│                      │         │  │ trendradar-news│  │     │
│                      │         │  └───────┬────────┘  │     │
│                      │         └──────────┼───────────┘     │
│                      │                    │                  │
│                      │         ┌──────────▼───────────┐     │
│                      │         │   ACP/MCP Bridge      │     │
│                      │         │   (acp/mcp_adapter)   │     │
│                      │         └──────────┬───────────┘     │
└──────────────────────┼────────────────────┼─────────────────┘
                       │                    │
                ┌──────▼──────┐    ┌────────▼──────────┐
                │  OmniNova   │    │  TrendRadar        │
                │  Channels   │    │  MCP Server        │
                │  (推送)     │    │  (FastMCP 2.0)     │
                └─────────────┘    └────────┬───────────┘
                                            │
                                   ┌────────▼───────────┐
                                   │  TrendRadar Core   │
                                   │  ├─ Crawler        │
                                   │  ├─ AI Analyzer    │
                                   │  ├─ Notifications  │
                                   │  └─ Storage        │
                                   └────────────────────┘
```

### 3.2 五条集成路径

| 路径 | 名称 | 工作量 | 深度 | 推荐度 |
|------|------|--------|------|--------|
| 1 | 技能包封装 | 低 | 浅 | ⭐⭐⭐ |
| 2 | MCP 协议桥接 | 中 | 深 | ⭐⭐⭐ |
| 3 | Cron 定时任务 | 低 | 浅 | ⭐⭐ |
| 4 | 通知渠道合并 | 低 | 中 | ⭐⭐ |
| 5 | Rust 原生重写 | 高 | 最深 | ⭐ |

---

## 4. 分阶段实施计划

### 路线图

```
Phase 1 (已完成)          Phase 2 (1周)              Phase 3 (可选,2-4周)
┌─────────────────┐     ┌──────────────────┐      ┌──────────────────┐
│ 技能包创建       │     │ MCP 协议桥接      │      │ Rust 原生重写     │
│                 │     │                  │      │                  │
│ • SKILL.md     │ ──► │ • ACP↔MCP 适配   │ ──►  │ • crawler crate  │
│ • bridge.py    │     │ • 工具注册        │      │ • analyzer crate │
│ • bridge-config│     │ • 进程管理        │      │ • 深度记忆集成    │
└─────────────────┘     └──────────────────┘      └──────────────────┘
        │                        │                         │
  收益: Agent 可感知       收益: Agent 可调用         收益: 原生性能
  TrendRadar 能力          全部26个工具              无需 Python 依赖
```

---

## 5. 第一阶段成果

### 5.1 已创建文件

```
novalclaw-main/skills/trendradar-news/
├── SKILL.md              # 技能定义文档（完整26工具说明）
├── bridge-config.toml     # OmniNova 集成配置模板
└── bridge.py              # MCP Server 管理脚本
```

### 5.2 SKILL.md 内容概览

- **26 个 MCP 工具**的完整参数、返回值、使用场景文档
- **5 个典型使用流程**：
  1. 日常热点速览 → `get_latest_news`
  2. 话题深度分析 → `resolve_date_range` → `analyze_topic_trend` → `analyze_sentiment` → `aggregate_news`
  3. 时期对比 → `compare_periods`
  4. 深度阅读 → `search_news` → `read_articles_batch`
  5. 推送订阅 → `get_trending_topics` → `generate_summary_report` → `send_notification`
- **11 个热榜平台** ID 和名称映射表
- **架构图**和 MCP Server 启动命令
- OmniNova 配置示例

### 5.3 bridge.py 使用方式

```bash
# 启动 HTTP 模式 MCP Server
python skills/trendradar-news/bridge.py start --port 3333

# 健康检查
python skills/trendradar-news/bridge.py health

# 手动触发爬取
python skills/trendradar-news/bridge.py crawl

# 获取热点摘要
python skills/trendradar-news/bridge.py summary --top-n 15

# 停止服务
python skills/trendradar-news/bridge.py stop
```

### 5.4 OmniNova 配置集成

将 `bridge-config.toml` 的内容合并到 `~/.omninova/config.toml`：

```toml
[skills.trendradar]
enabled = true

[skills.trendradar.mcp]
transport = "http"
host = "127.0.0.1"
port = 3333

[skills.trendradar.cron]
enabled = false
schedule = "*/30 * * * *"
```

### 5.5 环境依赖状态

| 依赖 | 状态 | 备注 |
|------|------|------|
| Python 3.12+ | ✅ | 3.12.4 (Anaconda) |
| fastmcp 2.12.5 | ✅ | MCP Server 框架 |
| mcp 1.16.0 | ✅ | MCP 协议库 |
| PyYAML | ✅ | 配置解析 |
| requests | ✅ | HTTP 请求 |
| feedparser | ✅ | RSS 解析 |
| json-repair | ✅ | JSON 修复 |
| tenacity | ✅ | 重试机制 |
| websockets | ✅ | WebSocket |
| litellm | ❌ | tiktoken 编译问题 (Windows) |
| boto3 | ❌ | 可选（S3 远程存储） |

> **litellm 缺失影响范围**：仅影响 AI 分析（`analyze_sentiment`）、AI 翻译、AI 筛选功能。新闻搜索、热点统计、趋势检测、跨平台聚合等核心工具均不受影响。

---

## 6. 第二阶段：MCP 协议桥接

### 6.1 目标

让 OmniNova Agent 能够**直接调用** TrendRadar 的全部 26 个 MCP 工具，就像调用内置工具（shell、web_search 等）一样。

### 6.2 技术方案

#### 6.2.1 ACP ↔ MCP 协议适配

OmniNova 的 `acp/` 模块与 TrendRadar 的 MCP Server 都使用 **JSON-RPC 2.0** 协议，主要差异在消息格式：

| 层面 | ACP (OmniNova) | MCP (TrendRadar) |
|------|----------------|-------------------|
| 传输 | HTTP / WebSocket | HTTP / stdio |
| 初始化 | `initialize` | `initialize` |
| 工具列表 | `tools/list` | `tools/list` |
| 工具调用 | `tools/call` | `tools/call` |
| 资源 | 无 | `resources/read` |
| 通知 | 自定义 | `notifications/*` |

#### 6.2.2 新增文件

```
crates/omninova-core/src/acp/
├── mod.rs              # 现有
├── client.rs           # 现有 ACP 客户端
├── server.rs           # 现有 ACP 服务端
├── types.rs            # 现有类型定义
└── mcp_adapter.rs      # 【新增】MCP 适配器
```

#### 6.2.3 mcp_adapter.rs 核心逻辑

```rust
// 伪代码结构
pub struct McpAdapter {
    transport: McpTransport,  // Http or Stdio
    tools_cache: Vec<ToolDef>,
    session_id: String,
}

impl McpAdapter {
    // 连接到 TrendRadar MCP Server
    pub async fn connect(&mut self) -> Result<()> { ... }

    // 初始化 MCP 会话
    pub async fn initialize(&mut self) -> Result<ServerCapabilities> { ... }

    // 获取工具列表
    pub async fn list_tools(&mut self) -> Result<Vec<ToolDef>> { ... }

    // 调用工具
    pub async fn call_tool(&self, name: &str, args: Value) -> Result<Value> { ... }

    // 将 TrendRadar 工具注册为 OmniNova 内置工具
    pub fn register_as_tools(&self, tool_registry: &mut ToolRegistry) { ... }
}
```

#### 6.2.4 进程生命周期管理

需要在 OmniNova 中管理 TrendRadar MCP Server 进程：

```rust
// 新增 crates/omninova-core/src/skills/process_manager.rs
pub struct SkillProcess {
    name: String,
    command: String,
    child: Option<Child>,
    health_check_url: Option<String>,
}

impl SkillProcess {
    pub fn start(&mut self) -> Result<()> { ... }
    pub fn stop(&mut self) -> Result<()> { ... }
    pub fn health_check(&self) -> Result<bool> { ... }
    pub fn restart(&mut self) -> Result<()> { ... }
}
```

#### 6.2.5 工具注册映射

将 TrendRadar 的 26 个工具自动注册到 OmniNova 工具系统：

```
TrendRadar MCP Tool              OmniNova Internal Tool
─────────────────────────────────────────────────────
resolve_date_range           →   trendradar.resolve_date_range
get_latest_news              →   trendradar.get_latest_news
get_trending_topics          →   trendradar.get_trending_topics
get_news_by_date             →   trendradar.get_news_by_date
get_latest_rss               →   trendradar.get_latest_rss
search_rss                   →   trendradar.search_rss
get_rss_feeds_status         →   trendradar.get_rss_feeds_status
search_news                  →   trendradar.search_news
find_related_news            →   trendradar.find_related_news
analyze_topic_trend          →   trendradar.analyze_topic_trend
analyze_data_insights        →   trendradar.analyze_data_insights
analyze_sentiment            →   trendradar.analyze_sentiment
aggregate_news               →   trendradar.aggregate_news
compare_periods              →   trendradar.compare_periods
generate_summary_report      →   trendradar.generate_summary_report
read_article                 →   trendradar.read_article
read_articles_batch          →   trendradar.read_articles_batch
send_notification            →   trendradar.send_notification
... (其余 8 个管理类工具)
```

### 6.3 预估工作量

| 任务 | 预计时间 | 难度 |
|------|---------|------|
| MCP JSON-RPC 客户端实现 | 4 小时 | 中 |
| 进程管理器 | 2 小时 | 低 |
| 工具注册集成 | 3 小时 | 中 |
| Gateway API 端点 | 2 小时 | 低 |
| 错误处理与重连 | 2 小时 | 中 |
| 测试与调试 | 3 小时 | 中 |
| **合计** | **约 16 小时** | **2-3 天** |

---

## 7. 第三阶段：Rust 原生重写（可选）

### 7.1 目标

将 TrendRadar 的核心功能用 Rust 重写为 OmniNova 的原生 crate，消除 Python 依赖。

### 7.2 新增 crate 结构

```
crates/omninova-trendradar/
├── Cargo.toml
├── src/
│   ├── lib.rs                 # crate 入口
│   ├── crawler/               # 爬虫模块
│   │   ├── mod.rs
│   │   ├── hotlist.rs         #   热榜爬取（替换 newsnow API 调用）
│   │   ├── rss.rs             #   RSS 聚合
│   │   └── twitter.rs         #   Twitter 趋势
│   ├── analyzer/              # 分析模块
│   │   ├── mod.rs
│   │   ├── frequency.rs       #   关键词频率统计
│   │   ├── trend.rs           #   趋势检测
│   │   ├── sentiment.rs       #   情感分析（调用 LLM）
│   │   └── aggregation.rs     #   跨平台聚合去重
│   ├── notification/          # 通知模块
│   │   ├── mod.rs
│   │   ├── dispatch.rs        #   分发器
│   │   └── templates.rs       #   各渠道模板
│   ├── storage/               # 存储模块
│   │   ├── mod.rs
│   │   ├── sqlite.rs          #   SQLite 操作
│   │   └── remote.rs          #   S3 兼容存储
│   └── tools/                 # 工具注册
│       ├── mod.rs
│       └── registry.rs        #   注册到 OmniNova 工具系统
```

### 7.3 优缺点

| 优点 | 缺点 |
|------|------|
| 原生性能（零 FFI 开销） | 开发周期长（2-4 周） |
| 直接访问 Agent 记忆系统 | 需持续维护两套代码 |
| 无需 Python 运行时 | GPL-3.0 许可证传染风险 |
| 更好的错误处理 | Python 生态替代成本高 |

### 7.4 许可证风险提示

TrendRadar 使用 **GPL-3.0**，OmniNova Claw 使用 **MIT/Apache-2.0**。如果 TrendRadar 代码以 crate 形式编译进 OmniNova 二进制，将触发 GPL 传染条款，整个项目需以 GPL-3.0 分发。

**安全做法**：保持独立进程通信（如 MCP 协议），避免同二进制链接。

---

## 8. 部署与运维指南

### 8.1 开发环境部署

```bash
# 1. 克隆两个项目
git clone https://github.com/omninova/claw.git
git clone https://github.com/sansan0/TrendRadar.git

# 2. 安装 TrendRadar Python 依赖
cd TrendRadar
pip install -r requirements.txt

# 3. 配置 TrendRadar
cp config/config.yaml.example config/config.yaml
# 编辑 config.yaml: 设置热榜平台、通知渠道、AI API Key

# 4. 启动 MCP Server
python -m mcp_server --transport http --port 3333 &

# 5. 配置 OmniNova（合并 bridge-config.toml 到 ~/.omninova/config.toml）

# 6. 启动 OmniNova
cd omniovalclaw
cargo build -p omninova-core --release --bin omninova
./target/release/omninova gateway run
```

### 8.2 Docker 部署

```dockerfile
# docker-compose.yml
version: '3.8'
services:
  omninova:
    build: ./novalclaw-main
    ports:
      - "8080:8080"
    volumes:
      - ./config:/root/.omninova
    depends_on:
      - trendradar-mcp

  trendradar-mcp:
    image: wantcat/trendradar-mcp:latest
    ports:
      - "3333:3333"
    volumes:
      - ./trendradar-config:/app/config
      - ./trendradar-output:/app/output
    environment:
      - AI_API_KEY=${AI_API_KEY}
    command: ["--transport", "http", "--port", "3333"]
```

### 8.3 健康监控

```bash
# OmniNova doctor（含 TrendRadar 检查）
omninova doctor

# TrendRadar 独立检查
python bridge.py health

# 检查 MCP 端点
curl http://127.0.0.1:3333/mcp
```

---

## 9. 风险与注意事项

### 9.1 技术风险

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| Python 进程崩溃 | Agent 工具不可用 | 进程管理器自动重启 |
| MCP 协议版本不兼容 | 工具调用失败 | 版本锁定 + 兼容性测试 |
| API 限流（newsnow） | 爬取失败 | 合理设置请求间隔 |
| LLM Token 消耗 | 成本过高 | 设置 max_news_for_analysis |
| Windows GBK 编码 | 中文输出乱码 | 统一使用 UTF-8 |

### 9.2 法律风险

| 风险 | 说明 |
|------|------|
| GPL-3.0 传染 | 仅当 Rust 重写且编译链接时触发，MCP 进程通信方式不受影响 |
| 数据合规 | 爬取的新闻内容可能涉及版权，仅用于个人学习分析 |
| API Key 安全 | 不要在代码中硬编码，使用环境变量或 secrets 管理 |

### 9.3 运维建议

- **定时爬取**：建议 30 分钟间隔，避免对 newsnow API 造成压力
- **数据清理**：配置 `storage.retention_days` 避免 SQLite 无限增长
- **AI 成本控制**：`ai_analysis.max_news_for_analysis` 设置合理上限（默认 150）
- **日志监控**：关注 MCP Server 的 stderr 输出，捕获异常

---

## 10. 附录

### 10.1 TrendRadar 全部 26 个 MCP 工具速查表

| # | 工具名 | 类别 | 说明 |
|---|--------|------|------|
| 1 | `resolve_date_range` | 日期解析 | 自然语言转标准日期范围 |
| 2 | `get_latest_news` | 数据查询 | 获取最新一批新闻 |
| 3 | `get_trending_topics` | 数据查询 | 热点话题频率统计 |
| 4 | `get_news_by_date` | 数据查询 | 按日期范围查询新闻 |
| 5 | `get_latest_rss` | RSS 查询 | 获取最新 RSS 订阅 |
| 6 | `search_rss` | RSS 查询 | 搜索 RSS 数据 |
| 7 | `get_rss_feeds_status` | RSS 查询 | RSS 源状态 |
| 8 | `search_news` | 智能检索 | 统一新闻搜索（关键词/模糊/实体） |
| 9 | `find_related_news` | 智能检索 | 相关新闻查找 |
| 10 | `analyze_topic_trend` | 高级分析 | 话题趋势分析（4模式） |
| 11 | `analyze_data_insights` | 高级分析 | 数据洞察（3模式） |
| 12 | `analyze_sentiment` | 高级分析 | 情感倾向分析 |
| 13 | `aggregate_news` | 高级分析 | 跨平台聚合去重 |
| 14 | `compare_periods` | 高级分析 | 时期对比分析 |
| 15 | `generate_summary_report` | 高级分析 | 每日/每周摘要 |
| 16 | `get_current_config` | 配置管理 | 获取系统配置 |
| 17 | `get_system_status` | 系统管理 | 系统运行状态 |
| 18 | `check_version` | 系统管理 | 版本更新检查 |
| 19 | `trigger_crawl` | 系统管理 | 手动触发爬取 |
| 20 | `sync_from_remote` | 存储同步 | 远程拉取数据 |
| 21 | `get_storage_status` | 存储同步 | 存储配置状态 |
| 22 | `list_available_dates` | 存储同步 | 可用日期列表 |
| 23 | `read_article` | 文章读取 | 读取单篇文章（Markdown） |
| 24 | `read_articles_batch` | 文章读取 | 批量读取（≤5篇） |
| 25 | `get_channel_format_guide` | 通知推送 | 渠道格式化策略 |
| 26 | `send_notification` | 通知推送 | 发送通知到渠道 |

### 10.2 关键配置文件

#### TrendRadar 最小配置 (`config/config.yaml`)

需要至少配置以下内容才能正常运行：

```yaml
platforms:
  enabled: true
  sources:
    - id: "zhihu"
      name: "知乎"

notification:
  enabled: false  # 可先关闭，通过 MCP 工具手动推送

ai:
  model: "deepseek/deepseek-v4-flash"
  api_key: ""     # 使用环境变量 AI_API_KEY

ai_analysis:
  enabled: false  # 可先关闭，节省 token

ai_translation:
  enabled: false  # 可先关闭
```

#### 关注词配置 (`config/frequency_words.txt`)

```
# 格式: 词组名: 关键词1, 关键词2, ...
AI: AI, 人工智能, ChatGPT, GPT, 大模型, LLM
科技: 芯片, 半导体, 5G, 量子
金融: A股, 港股, 美股, 降息, 加息
```

### 10.3 故障排查

| 问题 | 可能原因 | 解决方案 |
|------|----------|----------|
| MCP Server 启动失败 | fastmcp 版本不兼容 | `pip install fastmcp==2.12.5` |
| 爬取无数据 | newsnow API 不可达 | 检查网络，尝试配置代理 |
| 工具调用超时 | 数据库锁定 | 减少并发调用，增加超时 |
| 中文乱码 | Windows GBK 编码 | 设置 `PYTHONUTF8=1` 或 `chcp 65001` |
| AI 分析报错 | API Key 未配置 | 设置 `AI_API_KEY` 环境变量 |

### 10.4 参考链接

- TrendRadar 仓库：<https://github.com/sansan0/TrendRadar>
- OmniNova Claw 仓库：<https://github.com/omninova/claw>
- FastMCP 文档：<https://github.com/jlowin/fastmcp>
- MCP 协议规范：<https://modelcontextprotocol.io/>
- LiteLLM 供应商列表：<https://docs.litellm.ai/docs/providers>
- newsnow 项目：<https://github.com/ourongxing/newsnow>

---

> 文档维护者：Claude Code  
> 最后更新：2026-06-14
