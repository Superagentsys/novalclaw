---
name: "trendradar-news"
description: "热点新闻聚合与分析技能。提供多平台热榜爬取、关键词/AI智能筛选、跨平台聚合去重、趋势分析、情感分析、相关新闻查找、文章内容读取、通知推送等26个工具。当用户需要获取最新热点、分析新闻趋势、监控特定话题、搜索新闻或对比不同时期的舆论变化时启用。"
homepage: "https://github.com/sansan0/TrendRadar"
metadata:
  version: "6.9.1"
  mcp_version: "4.1.0"
  tools_count: 26
  transport: "stdio|http"
  category: "news-intelligence"
---

# 热点新闻聚合与分析（TrendRadar News）

基于 TrendRadar MCP Server 提供完整的多平台热点新闻采集、智能分析和多渠道推送能力。Agent 可通过 MCP 协议调用全部 26 个工具。

## 何时启用

- 用户询问「今天有什么热点」「最近什么新闻比较火」
- 用户需要搜索特定话题的相关新闻
- 用户要求分析某个话题的热度趋势或情感倾向
- 用户想对比不同时期的舆论变化
- 用户需要跨平台聚合去重新闻
- 用户要求将分析结果推送到飞书/钉钉/企微/Telegram 等渠道
- 用户需要读取某篇新闻的完整正文

## 架构说明

```
┌──────────────┐     MCP Protocol      ┌─────────────────────┐
│ OmniNova     │ ◄──────────────────► │ TrendRadar MCP      │
│ Agent/Gateway│   (stdio or HTTP)     │ Server (FastMCP 2.0)│
└──────────────┘                       └──────────┬──────────┘
                                                   │
                                    ┌──────────────┴──────────────┐
                                    │  Data Layer                 │
                                    │  ├─ SQLite (local)          │
                                    │  ├─ S3/R2 (remote)         │
                                    │  └─ HTML Reports            │
                                    │                             │
                                    │  Sources                    │
                                    │  ├─ 11+ 热榜平台            │
                                    │  ├─ RSS 订阅                │
                                    │  └─ Twitter 趋势 (可选)    │
                                    └────────────────────────────┘
```

## 启动 MCP Server

在 TrendRadar 项目根目录下执行：

```bash
# stdio 模式（默认，适合 AI 客户端直连）
python -m mcp_server

# HTTP 模式（适合 OmniNova Gateway 连接）
python -m mcp_server --transport http --host 0.0.0.0 --port 3333

# 指定项目根目录
python -m mcp_server --project-root /path/to/TrendRadar-master --transport http --port 3333
```

## Agent 可调用的工具

### 🔍 基础数据查询（P0 核心）

#### 1. `resolve_date_range` — 自然语言日期解析
**推荐优先调用**。将「本周」「最近7天」等自然语言转换为标准日期范围，确保所有工具获得一致的日期参数。

**参数**：
- `expression` (必需): 自然语言日期，支持 "今天"/"昨天"/"本周"/"上周"/"本月"/"上月"/"最近N天"

**返回**：`{"start": "YYYY-MM-DD", "end": "YYYY-MM-DD"}`

**使用场景**：用户说「分析AI本周的情感倾向」→ 先调用此工具获取精确日期 → 再调用 `analyze_sentiment`

#### 2. `get_latest_news` — 获取最新新闻
获取最新一批爬取的新闻数据，快速了解当前热点。

**参数**：
- `platforms` (可选): 平台ID列表，如 `["zhihu", "weibo"]`，不指定则全部
- `limit` (默认50, 最大1000): 返回条数
- `include_url` (默认false): 是否包含链接

**数据展示建议**：默认展示全部返回数据，除非用户明确要求「总结」或「挑重点」

#### 3. `get_trending_topics` — 获取热点话题统计
统计热点话题出现频率。

**参数**：
- `top_n` (默认10): 返回TOP N话题
- `mode`: "daily"（当日累计）| "current"（最新一批，默认）
- `extract_mode`: "keywords"（预设关注词，默认）| "auto_extract"（自动提取高频词）

**使用场景**：用户问「最近什么话题最火」→ `get_trending_topics(extract_mode="auto_extract", top_n=20)`

#### 4. `get_news_by_date` — 按日期查询新闻
查询指定日期范围的新闻数据，用于历史分析和对比。

**参数**：
- `date_range` (可选): `{"start": "YYYY-MM-DD", "end": "YYYY-MM-DD"}` 或 "今天"/"昨天"/"最近7天"
- `platforms` (可选): 平台ID过滤
- `limit` (默认50): 返回条数
- `include_url` (默认false): 是否包含链接

### 📡 RSS 数据查询

#### 5. `get_latest_rss` — 获取最新 RSS
**参数**：`feeds`(可选), `days`(默认1, 最大30), `limit`(默认50), `include_summary`(默认false)

#### 6. `search_rss` — 搜索 RSS 数据
**参数**：`keyword`(必需), `feeds`(可选), `days`(默认7), `limit`, `include_summary`

#### 7. `get_rss_feeds_status` — RSS 源状态
无必需参数，返回各 RSS 源的数据统计。

### 🔎 智能检索

#### 8. `search_news` — 统一新闻搜索
支持关键词/模糊/实体三种搜索模式，可同时搜索热榜和RSS。

**参数**：
- `query` (必需): 搜索关键词
- `search_mode`: "keyword"(精确) | "fuzzy"(模糊) | "entity"(实体)
- `date_range` (可选): 日期范围
- `platforms` (可选): 平台过滤
- `limit` (默认50): 返回条数
- `sort_by`: "relevance" | "weight" | "date"
- `threshold` (默认0.6): 模糊模式相似度阈值
- `include_rss` (默认false): 是否同时搜索RSS

**使用场景**：
- `search_news(query="特斯拉", search_mode="entity")` — 搜索特斯拉相关实体新闻
- `search_news(query="AI", include_rss=True)` — 同时搜索热榜和RSS

#### 9. `find_related_news` — 查找相关新闻
根据参考标题找相关报道，支持自定义日期范围。

**参数**：
- `reference_title` (必需): 参考标题（完整或部分）
- `date_range` (可选): 不指定仅查今天
- `threshold` (默认0.5): 相似度阈值
- `include_url` (默认false)

### 📊 高级数据分析

#### 10. `analyze_topic_trend` — 话题趋势分析
统一趋势分析工具，整合四种分析模式。

**参数**：
- `topic` (必需): 话题关键词
- `analysis_type`: "trend"(热度趋势) | "lifecycle"(生命周期) | "viral"(异常热度检测) | "predict"(话题预测)
- `date_range` (可选): 默认最近7天
- `granularity` (默认"day"): 时间粒度
- `spike_threshold` (默认3.0): 突增倍数阈值(viral模式)
- `time_window` (默认24): 检测窗口小时数(viral模式)
- `lookahead_hours` (默认6): 预测未来小时数(predict模式)

**使用场景**：
- 「AI最近热度怎么样」→ `analyze_topic_trend(topic="AI", analysis_type="trend")`
- 「特斯拉是不是突然火了」→ `analyze_topic_trend(topic="特斯拉", analysis_type="viral")`

#### 11. `analyze_data_insights` — 数据洞察分析
**参数**：
- `insight_type`: "platform_compare"(平台对比) | "platform_activity"(平台活跃度) | "keyword_cooccur"(关键词共现)
- `topic` (可选): 话题关键词
- `date_range` (可选)
- `min_frequency` (默认3): 最小共现频次
- `top_n` (默认20)

#### 12. `analyze_sentiment` — 情感倾向分析
分析新闻的情感分布和热度趋势。

**参数**：
- `topic` (可选): 话题关键词
- `platforms` (可选): 平台过滤
- `date_range` (可选): 默认今天
- `limit` (默认50, 最大100): 返回数量
- `sort_by_weight` (默认true): 按热度排序
- `include_url` (默认false)

**使用场景**：「看看大家对苹果发布会的情感反应」→ `analyze_sentiment(topic="苹果发布会")`

#### 13. `aggregate_news` — 跨平台新闻聚合去重
将不同平台报道的同一事件合并，显示跨平台覆盖情况。

**参数**：
- `date_range` (可选)
- `platforms` (可选)
- `similarity_threshold` (默认0.7): 相似度阈值，越高越严格
- `limit` (默认50)
- `include_url` (默认false)

**使用场景**：「把今天所有平台关于'GPT-5发布'的新闻合并一下」

#### 14. `compare_periods` — 时期对比分析
对比两个时间段的新闻数据变化。

**参数**：
- `period1` (必需): 基准期 `{"start": "...", "end": "..."}` 或 "last_week"
- `period2` (必需): 对比期，同上格式
- `topic` (可选): 聚焦特定话题
- `compare_type`: "overview"(概览) | "topic_shift"(话题变化) | "platform_activity"(平台活跃度)
- `platforms` (可选)
- `top_n` (默认10)

**使用场景**：「这周和上周比，AI赛道有什么变化」→ `compare_periods(period1="last_week", period2="this_week", compare_type="topic_shift")`

#### 15. `generate_summary_report` — 每日/每周摘要生成
自动生成热点摘要报告（Markdown 格式）。

**参数**：
- `report_type`: "daily" | "weekly"
- `date_range` (可选): 自定义日期范围

### ⚙️ 配置与系统管理

#### 16. `get_current_config` — 获取系统配置
**参数**：`section`: "all" | "crawler" | "push" | "keywords" | "weights"

#### 17. `get_system_status` — 系统运行状态
返回版本、数据统计、缓存状态。无必需参数。

#### 18. `check_version` — 检查版本更新
**参数**：`proxy_url` (可选)

#### 19. `trigger_crawl` — 手动触发爬取
**参数**：`platforms`(可选), `save_to_local`(默认false), `include_url`(默认false)

### 💾 存储同步

#### 20. `sync_from_remote` — 从远程拉取数据
**参数**：`days` (默认7, 最大30)

#### 21. `get_storage_status` — 存储配置状态
无必需参数。

#### 22. `list_available_dates` — 列出可用日期
**参数**：`source`: "local" | "remote" | "both"(默认)

### 📖 文章内容读取

#### 23. `read_article` — 读取单篇文章
通过 Jina AI Reader 将网页转为干净的 Markdown 格式。

**参数**：
- `url` (必需): 文章链接
- `timeout` (默认30, 最大60): 超时秒数

**典型流程**：`search_news(include_url=True)` → `read_article(url="...")` → Agent 分析/总结

#### 24. `read_articles_batch` — 批量读取文章
最多5篇，自动5秒间隔限速。

**参数**：`urls`(必需, 最多5篇), `timeout`(默认30)

### 📬 通知推送

#### 25. `get_channel_format_guide` — 渠道格式化策略
获取各推送渠道的 Markdown 支持特性和限制。

**参数**：`channel` (可选): "feishu"/"dingtalk"/"wework"/"telegram"/"email"/"ntfy"/"bark"/"slack"/"generic_webhook"

#### 26. `send_notification` — 发送通知
向已配置渠道推送消息，自动适配各渠道格式。

**参数**：
- `message` (必需): Markdown 格式消息
- `title` (默认 "TrendRadar 通知"): 消息标题
- `channels` (可选): 指定渠道列表，不指定则全部已配置渠道

## 支持的平台 ID 列表

配置文件中 `platforms.sources` 支持的热榜平台：

| ID | 名称 | 说明 |
|----|------|------|
| `toutiao` | 今日头条 | 综合资讯 |
| `baidu` | 百度热搜 | 搜索引擎热搜 |
| `weibo` | 微博 | 社交媒体热搜 |
| `zhihu` | 知乎 | 知识社区热榜 |
| `douyin` | 抖音 | 短视频热点 |
| `bilibili-hot-search` | B站热搜 | 视频社区热搜 |
| `wallstreetcn-hot` | 华尔街见闻 | 财经资讯 |
| `cls-hot` | 财联社热门 | 财经快讯 |
| `thepaper` | 澎湃新闻 | 严肃新闻 |
| `ifeng` | 凤凰网 | 综合新闻 |
| `tieba` | 贴吧 | 社区热议 |

## 典型使用流程

### 流程1：日常热点速览
```
用户：「今天有什么热点？」
  ↓
Agent 调用 get_latest_news(limit=30)
  ↓
Agent 展示热点列表
```

### 流程2：话题深度分析
```
用户：「分析一下AI赛道最近一周的趋势和情绪」
  ↓
1. resolve_date_range("最近一周") → 获取标准日期范围
2. analyze_topic_trend(topic="AI", analysis_type="trend", date_range=...) → 热度趋势
3. analyze_sentiment(topic="AI", date_range=...) → 情感分析
4. aggregate_news(date_range=...) → 跨平台去重聚合
  ↓
Agent 综合报告
```

### 流程3：话题对比
```
用户：「对比一下本周和上周关于特斯拉的舆论变化」
  ↓
compare_periods(period1="last_week", period2="this_week", topic="特斯拉", compare_type="topic_shift")
  ↓
Agent 展示变化趋势
```

### 流程4：深度阅读
```
用户：「找找关于GPT-5的最新报道，读一下原文再给我总结」
  ↓
1. search_news(query="GPT-5", search_mode="fuzzy", include_url=True)
2. read_articles_batch(urls=[最相关的3-5篇])
  ↓
Agent 总结多篇原文
```

### 流程5：推送订阅
```
用户：「把今天的AI热点总结推送到飞书」
  ↓
1. get_trending_topics(extract_mode="auto_extract") → 获取热点
2. generate_summary_report(report_type="daily") → 生成摘要
3. send_notification(message=摘要, channels=["feishu"]) → 推送
```

## 配置说明

技能导入后需要在 OmniNova 工作区配置 TrendRadar 的连接信息：

```toml
# OmniNova 配置中新增
[skills.trendradar]
enabled = true
mcp_transport = "http"          # "stdio" 或 "http"
mcp_host = "127.0.0.1"
mcp_port = 3333
project_root = "/path/to/TrendRadar-master"

# 可选：定时自动爬取
[skills.trendradar.cron]
enabled = true
schedule = "*/30 * * * *"       # 每30分钟
```

## 文件引用

- MCP Server 入口：`TrendRadar-master/mcp_server/server.py`
- 数据查询工具：`TrendRadar-master/mcp_server/tools/data_query.py`
- 分析工具：`TrendRadar-master/mcp_server/tools/analytics.py`
- 搜索工具：`TrendRadar-master/mcp_server/tools/search_tools.py`
- 配置管理：`TrendRadar-master/mcp_server/tools/config_mgmt.py`
- 存储同步：`TrendRadar-master/mcp_server/tools/storage_sync.py`
- 文章读取：`TrendRadar-master/mcp_server/tools/article_reader.py`
- 通知推送：`TrendRadar-master/mcp_server/tools/notification.py`

## 注意事项

1. **首次使用需要配置**：至少需要 `config/config.yaml`（热榜平台列表）和 `config/frequency_words.txt`（关键词），否则部分工具返回空数据
2. **AI 功能需要 API Key**：情感分析/翻译/AI筛选依赖 LiteLLM，需在 `config/config.yaml` 的 `ai.api_key` 配置或设置环境变量 `AI_API_KEY`
3. **数据需要爬取**：TrendRadar 的 MCP Server 查询的是已爬取的数据。如果本地没有数据，需先调用 `trigger_crawl` 触发爬取，或通过 GitHub Actions/Docker 定期运行爬虫
4. **RSS 新鲜度**：默认过滤超过1天的文章，可在配置中调整 `rss.freshness_filter.max_age_days`
5. **速率限制**：`read_article` 使用 Jina AI Reader (100 RPM)，批量读取自动间隔5秒
6. **跨项目路径**：MCP Server 和 OmniNova 在不同项目目录，注意路径配置
