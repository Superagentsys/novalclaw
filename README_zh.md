# OmniNova Claw

[English](./README.md) | **中文**

**OmniNova Claw** 是一个本地优先的 AI 代理平台：核心运行时、HTTP 网关、多模型接入与桌面控制面。网关与 Agent 运行在你自己的机器上；桌面应用（Tauri + React）提供配置、网关启停与状态一览。

如果你希望有一个统一的控制面来管理路由、会话、工具与多种模型（OpenAI、Anthropic、Gemini、DeepSeek、通义、Ollama、OpenRouter 等），并配有原生桌面 UI，选它即可。

OmniNova Claw 基于 **Novalclaw** 架构构建 —— 一种以声明式路由、会话治理与统一控制面为核心的新一代设计。

---

## Novalclaw 先进在哪里？

Novalclaw 是 OmniNova Claw 所采用的架构名称，其优势主要体现在：

- **声明式路由** — 通过 TOML 的 `[bindings]` 按 **渠道**、**account_id**、**guild_id**、**team_id**、**peer** 或 **roles** 路由，**无需写代码**：一条配置即可让「Slack #eng」走一个 Agent、「Discord 私信」走另一个。支持通过 `metadata.agent` 显式指定 Agent，便于调试或开放 API。
- **会话树与血缘** — 每次请求都归属一个带 **父子血缘**、**spawn_depth** 与 **TTL** 的会话。网关统一做深度与并发限制，并通过 **`/sessions/tree`** 提供分页（cursor/limit）、按渠道/Agent/深度过滤与统计，**开箱即用的可观测性与治理**（谁在什么会话下派生、深度如何）。
- **单一控制面** — 一个 HTTP 网关同时提供 **chat**、**route**、**ingress**、**webhook**、**会话树**、**e-stop** 与 **config**。无需拆成「路由服务」和「运行时服务」；单进程、单地址、单配置。
- **与 Provider 解耦的 Agent 循环** — 同一套 **Agent** 流程（消息 → Provider → 工具调用 → 记忆）可跑在 **OpenAI**、**Anthropic**、**Gemini**、**Ollama**、**OpenRouter** 或任意 OpenAI 兼容端点上。在配置或路由里切换 provider/model 即可，无需改代码。
- **委托 Agent** — 支持多个 **命名 Agent**（如 `researcher`、`coder`），各自配置 **provider**、**model**、**system_prompt** 与 **工具白名单**。由路由决定使用哪个 Agent，网关按请求组装对应的 Provider 与工具，实现「按场景选模型」而无需多套服务。
- **安全内置** — **E-stop**（网关级暂停/恢复，可选 OTP）、**Webhook 签名**校验与 **nonce** 防重放、**工具策略**（白名单/黑名单、危险工具检查）在 schema 与运行时中都是一等公民。
- **本地优先、配置驱动** — **TOML** + **环境变量覆盖**；配置目录由 `OMNINOVA_CONFIG_DIR` 或工作区决定。无厂商锁定，网关与桌面应用均可完全运行在本地。
- **可运维** — **Daemon**（install/start/stop/status/check）提供 **launchd**、**systemd**、**schtasks** 适配；**health** 与 **config** API 便于监控与热更新配置。

---

## 亮点

- **本地优先网关** — 提供 chat、route、ingress、webhook、会话树、e-stop、配置等 HTTP API，默认绑定 `http://127.0.0.1:42617`。
- **渠道** — WhatsApp (Baileys)、Telegram (grammY)、Slack (Bolt)、Discord (discord.js)、Google Chat、Signal (signal-cli)、BlueBubbles (iMessage)、iMessage (legacy)、IRC、Microsoft Teams、Matrix、飞书、LINE、Mattermost、Nextcloud Talk、Nostr、Synology Chat、Tlon、Twitch、微信、Zalo、Zalo Personal、Lark、钉钉、WebChat、Email、Webhook。在 `[bindings]` 中按渠道路由；平台 webhook 路径：`/webhook/wechat`、`/webhook/feishu`、`/webhook/lark`、`/webhook/dingtalk`。
- **多模型接入** — OpenAI、Anthropic、Gemini、DeepSeek、通义、Moonshot、xAI、Mistral、Groq、OpenRouter、Ollama、LM Studio；可通过 TOML 与环境变量扩展。
- **路由与多 Agent** — 按渠道、元数据或绑定规则路由到不同 Agent/模型；委托 Agent 支持自定义提示词与工具白名单。
- **会话树** — 父子会话、深度与并发限制、分页与游标、来源分布等治理能力。
- **工具与记忆** — 文件读/写、Shell（含策略）、可插拔工具抽象；记忆（会话/存储）与安全（e-stop、webhook 签名、nonce）。
- **桌面应用** — Tauri 2 + React 19 配置界面：Provider、默认模型、网关地址、保存配置、启动/停止网关；可构建 .app / .dmg / Windows / Linux。
- **守护进程** — 网关后台服务安装/启动/停止/状态/检查（launchd、systemd、schtasks）。

---

## 工作流程概览

```
CLI / Web / Webhook /（未来：Telegram、Discord、Slack…）
                    │
                    ▼
┌─────────────────────────────────────────────────────────┐
│                  OmniNova Gateway                        │
│              http://127.0.0.1:42617                     │
│   /health  /chat  /route  /ingress  /webhook            │
│   /sessions/tree  /estop  /config                       │
└──────────────────────────┬──────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        ▼                   ▼                   ▼
   路由（渠道、          Agent（omninova /         Provider
   元数据、绑定）          委托 Agent）          （OpenAI、Anthropic、
                                                 Gemini、Ollama…）
                            │
                            ├── 工具（file_read、file_edit、shell）
                            ├── 记忆（会话、存储）
                            └── 安全（e-stop、webhook 校验）
```

---

## 项目结构

```text
omninovalclaw/
├── crates/
│   └── omninova-core/          # 核心库 + CLI + 网关
│       ├── agent/               # Agent 循环、dispatcher、提示词
│       ├── channels/            # ChannelKind、InboundMessage、适配器（cli、webhook）
│       ├── config/              # 配置 schema、加载、校验、环境变量
│       ├── daemon/              # launchd、systemd、schtasks
│       ├── gateway/             # HTTP 服务、路由、会话树、e-stop
│       ├── memory/              # 记忆抽象、后端、工厂
│       ├── providers/           # Provider 抽象、OpenAI、Anthropic、Gemini、工厂
│       ├── routing/             # resolve_agent_route、绑定规则
│       ├── security/            # e-stop、工具策略、webhook 鉴权
│       ├── tools/               # file_read、file_edit、shell
│       └── util/                # 鉴权、加密、网络
├── apps/
│   └── omninova-tauri/          # 桌面应用（Tauri 2 + React 19）
│       ├── src/                 # 配置 UI、类型、tauri invoke
│       └── src-tauri/            # Rust 壳、网关运行时桥接
├── config.template.toml         # 完整配置模板
├── Cargo.toml                   # 工作区
└── .github/workflows/           # CI（如 omninova-tauri-build.yml）
```

---

## 架构说明（详细）

### 1. 配置

- **来源**：`Config::load_or_init()` — 解析配置目录（环境变量 `OMNINOVA_CONFIG_DIR`、`OMNINOVA_WORKSPACE` 或 `~/.omninova/`），再加载 `config.toml`。
- **Schema**：单一 TOML，包含 `api_key`、`default_provider`、`default_model`、`[model_providers.*]`、`[agent]`、`[agents.<name>]`（委托 Agent）、`[gateway]`、`[bindings]`、`[security]`、`[memory]`、`[autonomy]` 等，见 `config.template.toml`。
- **覆盖**：环境变量 `OMNINOVA_*` 覆盖文件中的值，用于 API Key 与网关 host/port。

### 2. 网关（HTTP 控制面）

- **职责**：单一 HTTP 服务，默认绑定 `127.0.0.1:42617`，接收对话、路由、入口、webhook、会话树查询、e-stop 与配置读写。
- **路由**：
  - `GET /` — 服务说明与 `/health`、`/chat`、`/config` 等链接。
  - `GET /health` — 网关与 Provider 健康状态。
  - `POST /chat` — 用户消息 → 路由 → Agent → Provider/工具 → 回复。
  - `POST /route` — 根据入站 payload 返回 `RouteDecision`（agent_name、provider、model）。
  - `POST /ingress` — 入站消息及会话血缘校验。
  - `POST /webhook` — 支持签名/nonce 的 webhook。
  - `GET /sessions/tree` — 会话树分页（channel、agent、深度、cursor 等过滤）。
  - `GET /estop/status`、`POST /estop/pause`、`POST /estop/resume` — 紧急停止。
  - `GET/POST /config` — 读/写配置。
- **生命周期**：由 `GatewayRuntime::new(config)` 构建。在 Tauri 应用中，`start_gateway` 在 tokio 任务中 spawn `serve_http()`；`stop_gateway` 会中止该任务。

### 3. 路由

- **输入**：`InboundMessage`（channel、user_id、session_id、text、metadata）。
- **逻辑**：`resolve_agent_route(config, inbound)`：
  - 若 `metadata.agent` 存在 → 使用该 Agent。
  - 否则匹配 `config.bindings`（channel、account_id、guild_id、team_id 等）→ 绑定 Agent。
  - 否则使用 `acp.default_agent` 或主 `agent.name`。
- **输出**：`RouteDecision { agent_name, provider, model }`。委托 Agent 的 provider/model 来自 `config.agents.<name>`，否则使用默认 provider/model。

### 4. Agent

- **单次运行**：按 provider/model 从工厂加载 Provider，加载工具（file_read、file_edit、shell）、记忆后端，用系统提示词与限制构建 `Agent`。
- **循环**：`process_message` → 追加用户消息，调用 `AgentDispatcher::run`（Provider 对话 + 工具调用，max_tool_iterations），返回助手回复。
- **委托 Agent**：在 `config.agents` 中定义，使用各自的 provider/model 与可选工具白名单。

### 5. Provider

- **抽象**：`Provider::chat()`、`health_check()`，可选 `models()`。
- **实现**：`OpenAiProvider`（兼容 OpenAI 的各类 API）、`AnthropicProvider`、`GeminiProvider`、`MockProvider`。
- **工厂**：`build_provider_with_selection(config, selection)` 解析 api_key、base_url 并构建对应 Provider。

### 6. 工具与记忆

- **工具**：`FileReadTool`、`FileEditTool`、`ShellTool`；实现 `Tool`（spec、execute）。安全层可限制 shell 与路径。
- **记忆**：`Memory` 抽象（store、recall）；后端（如内存、SQLite）与分类（如会话），供 Agent 做上下文与持久化。

### 7. 安全

- **E-stop**：网关级暂停/恢复；可选 OTP 恢复。
- **Webhook**：签名校验（如 x-hub-signature-256）、可选 nonce 与时间戳防重放。
- **工具策略**：Shell 与文件访问的白名单/黑名单与危险工具检查。

### 8. 渠道

- **类型**：`ChannelKind`：Cli、Web、Telegram、Discord、Slack、Whatsapp、Matrix、Email、Webhook、Other。
- **适配器**：CLI 与 webhook 将原始输入转为 `InboundMessage`，再由路由选择 Agent/provider/model。

### 9. 桌面应用（omninova-tauri）

- **技术栈**：Tauri 2、React 19、Vite、TypeScript。
- **功能**：配置 UI（工作区、网关地址、Provider/模型下拉、机器人配置），通过 Tauri 命令加载/保存配置，进程内启动/停止网关，展示网关状态与错误。
- **桥接**：`get_setup_config`、`save_setup_config`、`gateway_status`、`start_gateway`、`stop_gateway` 在 `src-tauri/src/lib.rs` 中把 UI 与 `Config`、`GatewayRuntime` 对接。

### 10. 守护进程

- **命令**：`omninova daemon install|uninstall|start|stop|status|check` — 将网关作为后台服务管理。
- **适配**：launchd（macOS）、systemd（Linux）、schtasks（Windows）。

---

## 环境要求

- **Rust**（stable）；**Node.js** + npm（桌面应用）。
- **Tauri 2** 系统依赖见 [Tauri 文档](https://v2.tauri.app/start/prerequisites/)。

---

## 快速开始

### 1. 克隆并构建核心

```bash
cd omninovalclaw
cargo check
cargo test -p omninova-core
```

### 2. 运行 CLI

```bash
# 健康检查
cargo run -p omninova-core --bin omninova -- health

# 单条消息（使用配置中的默认 provider/model）
cargo run -p omninova-core --bin omninova -- agent -m "你好"

# 启动网关（默认 127.0.0.1:42617）
cargo run -p omninova-core --bin omninova -- gateway

# 路由调试
cargo run -p omninova-core --bin omninova -- route --channel cli -t "总结一下"
```

### 3. 运行桌面应用（推荐用于配置）

```bash
cd apps/omninova-tauri
npm install
npm run tauri dev
```

在配置页：设置工作区、添加 Provider/模型、设置网关地址，然后**保存配置**并**保存并启动网关**。在浏览器中打开网关链接可查看 `GET /` 服务说明与 `GET /health` 健康状态。

### 4. 调用网关（启动后）

```bash
curl http://127.0.0.1:42617/health
curl -X POST http://127.0.0.1:42617/chat \
  -H "Content-Type: application/json" \
  -d '{"user_id":"user1","session_id":"s1","message":"你好"}'
```

---

## 配置

- **配置目录**：`OMNINOVA_CONFIG_DIR`，或由 `OMNINOVA_WORKSPACE` 推导，或 `~/.omninova/`（见指针文件 `~/.omninova/active_workspace.toml`）。
- **模板**：将 `config.template.toml` 复制到 `~/.omninova/config.toml`（或你解析出的目录）并编辑。
- **常用键**：`api_key`、`default_provider`、`default_model`、`[gateway]`（host、port）、`[agent]`、`[agents.<name>]`、`[model_providers.*]`、`[bindings]`。

---

## HTTP 网关 API 一览

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/` | 服务说明与 API 链接 |
| GET | `/health` | 网关与 Provider 健康 |
| POST | `/chat` | 发送消息、获取回复 |
| POST | `/route` | 获取入站消息的路由决策 |
| POST | `/ingress` | 入站及会话血缘校验 |
| POST | `/webhook` | 支持签名/nonce 的 webhook |
| GET | `/sessions/tree` | 会话树（cursor、limit、过滤） |
| GET | `/estop/status` | E-stop 状态 |
| POST | `/estop/pause` | 暂停（可选原因） |
| POST | `/estop/resume` | 恢复 |
| GET/POST | `/config` | 读/写配置 |

---

## 会话树（/sessions/tree）

查询参数：`session_id`、`session_key`、`parent_session_id`、`agent_name`、`channel`、`min_spawn_depth`、`max_spawn_depth`、`contains`、`cursor`、`offset`、`limit`、`sort_by`、`sort_order`。分页使用 `limit` 与响应中的 `next_cursor`。

---

## 构建（桌面 / 发布）

在 `apps/omninova-tauri` 下：

```bash
npm run build:list          # 列出构建目标
npm run check:build-env     # 检查构建环境
npm run build:desktop       # 当前主机
npm run build:macos        # macOS（当前架构）
npm run build:macos:apple   # Apple Silicon
npm run build:macos:intel    # Intel Mac
npm run build:windows
npm run build:linux
```

产物示例（macOS）：`target/release/bundle/macos/OmniNova Claw.app`、`target/release/bundle/dmg/OmniNova Claw_0.1.0_aarch64.dmg`。推送 tag（如 `v0.1.0`）可触发 GitHub 工作流；签名与密钥见 `.github/omninova-tauri-secrets.example.md`。

---

## 代码导航

| 模块 | 路径 |
|------|------|
| 核心导出 | `crates/omninova-core/src/lib.rs` |
| CLI | `crates/omninova-core/src/cli/mod.rs` |
| 网关路由与会话树 | `crates/omninova-core/src/gateway/mod.rs` |
| 配置 Schema | `crates/omninova-core/src/config/schema.rs` |
| 配置加载 | `crates/omninova-core/src/config/loader.rs` |
| 路由 | `crates/omninova-core/src/routing/mod.rs` |
| Agent 循环 | `crates/omninova-core/src/agent/agent.rs`、`agent/dispatcher.rs` |
| Provider | `crates/omninova-core/src/providers/factory.rs`、`openai.rs`、`anthropic.rs`、`gemini.rs` |
| 工具 | `crates/omninova-core/src/tools/` |
| Tauri 桥接 | `apps/omninova-tauri/src-tauri/src/lib.rs` |

---

## 许可证

**MIT OR Apache-2.0**
