# OmniNova Claw — 本次变更日志

> 日期: 2026-06-15
> 分支建议: `feat/trendradar-integration`

---

## 变更概要

集成 TrendRadar MCP 热点新闻服务到 OmniNova Claw，修复 MCP SSE 协议适配器的 8 个兼容性问题，使 Agent 能够通过 Chat 界面自动发现和调用 27 个新闻工具。

---

## 文件变更清单

### 1. `crates/omninova-core/src/acp/mcp_adapter.rs` (核心修复)

**8 项修改：**

| # | 修改点 | 说明 |
|---|--------|------|
| 1 | Accept 请求头 | 添加 `Accept: application/json, text/event-stream`，MCP SSE 传输规范要求 |
| 2 | SSE 响应解析 | 新增 `extract_sse_data()` 函数，从 `event: message\ndata: {json}` 格式提取 JSON |
| 3 | Session ID 管理 | 新增 `session_id: RwLock<Option<String>>` 字段，从 `Mcp-Session-Id` 响应头自动提取并附加到后续请求 |
| 4 | HTTP/1.1 强制 | `.http1_only()` + `.pool_max_idle_per_host(0)` 禁用连接池避免 SSE 阻塞 |
| 5 | Connection: close | 添加 `Connection: close` 请求头，防止 reqwest 等待 keep-alive |
| 6 | bytes() 读取 | `.text()` 改为 `.bytes()` + `String::from_utf8_lossy()`，30s 超时保护 |
| 7 | 通知不含 id | `JsonRpcRequest.id` 改为 `Option<u64>`，通知消息不序列化 `id` 字段 |
| 8 | UTF-8 安全截断 | `body_truncated()` 使用 `is_char_boundary()` 避免中文多字节字符 panic (根因) |

### 2. `crates/omninova-core/src/tools/trendradar.rs`

- 工具命名从 `trendradar.xxx` 改为 `trendradar_xxx`（DeepSeek API 要求 `^[a-zA-Z0-9_-]+$` 不允许 `.`）

### 3. `crates/omninova-core/src/gateway/mod.rs`

- `create_tools_for_route()` 改为 `async`，接入 `trendradar_bridge`，调用 `create_all_tools_async`
- `http_api_tools` 改用 `create_all_tools_async`，使 `/api/tools` 返回 TrendRadar 工具
- `/api/trendradar/call` 前缀解析改为 `trendradar_`
- 测试函数 `delegate_allowed_tools_filter_default_toolset` 适配新签名

### 4. `~/.omninova/config.toml` (用户配置)

- 网关端口: `10809` -> `9090` (10809 被僵尸进程占用)
- `model_providers.deepseek.api_key_env` -> `api_key` (修复 Key 配置字段名)
- `[[providers]].api_key_env` -> `api_key` (同上)
- 新增 `agent.system_prompt`，告知 Agent 可用的 TrendRadar 工具
- `approvals.auto_approve` 新增 14 个 TrendRadar 工具自动批准

---

## 环境依赖

- **TrendRadar MCP Server**: `D:\caozuo\TrendRadar-master`，Python 3.12，通过 `uv sync` 部署
- **启动命令**: `uv run python -m mcp_server.server --transport http --host 0.0.0.0 --port 3333`
- **OmniNova 启动**: `cd apps/omninova-tauri && npm run tauri dev`

---

## 验证结果

| 端点 | 状态 |
|------|------|
| `GET /health` | OK |
| `GET /api/trendradar/health` | `healthy: true` |
| `GET /api/trendradar/tools` | 27 工具返回 |
| `POST /api/trendradar/call` | 功能正常 |
| `GET /api/tools` | 38 工具 (11 内置 + 27 TrendRadar) |
| Chat 界面 | Agent 调用 TrendRadar 工具，自动批准生效 |

---

## 提交建议

```bash
git checkout -b feat/trendradar-integration
git add crates/omninova-core/src/acp/mcp_adapter.rs
git add crates/omninova-core/src/tools/trendradar.rs
git add crates/omninova-core/src/gateway/mod.rs
git commit -m "feat: 集成 TrendRadar MCP 新闻工具，修复 MCP SSE 协议适配器

- MCP SSE 协议: Accept/SSE解析/SessionID/通知格式/连接管理
- UTF-8 边界安全截断修复中文恐慌
- 工具命名改为下划线符合 DeepSeek API 规范
- create_tools_for_route 接入 TrendRadar 桥
- http_api_tools 返回全部 38 工具 (含 27 TrendRadar)"
```

---

## 待后续跟进

1. `body_truncated` 应该用字符数而非字节数截断（目前 500 字节边界对中文仍会过早截断）
2. 清理 `mcp_adapter.rs` 中的 `eprintln!` 调试日志
3. 10809 端口僵尸进程需要系统重启释放
4. Windows 终端中文字符 GBK/UTF-8 编码问题（不影响功能）
