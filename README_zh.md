# OmniNova Claw

[English](./README.md) | 中文

OmniNova Claw 是一个基于 Rust + Tauri + React 的智能代理项目，包含：

- `omninova-core`：核心运行时、网关、路由、配置、工具链与安全控制
- `omninova-tauri`：桌面端 UI（Tauri 2 + React 19）
- 统一的 TOML 配置模型与可扩展的多 Provider 架构

---

## 项目结构

```text
omninovalclaw/
├─ crates/
│  └─ omninova-core/        # 核心库 + CLI + 网关
├─ apps/
│  ├─ omninova-tauri/       # 桌面应用（前端 + Tauri Rust）
│  └─ omninova-ui/          # 预留 UI 工作区
├─ config.template.toml     # 配置模板
├─ Cargo.toml               # Rust 工作区
└─ Cargo.lock
```

---

## 核心能力

- Agent 执行：消息处理、工具调用、记忆集成
- 路由决策：按渠道/元数据/绑定规则路由到不同 agent/model
- HTTP 网关：健康检查、路由、入口、webhook 处理、会话树、e-stop、配置 API
- 会话树治理：
  - 父子会话关系验证
  - 深度和并发限制
  - 分页/游标/统计/来源分布
- 安全控制：
  - e-stop（暂停/恢复）
  - webhook 签名验证和 nonce 重放保护
- 服务运维：
  - `daemon install/start/stop/status/check`
  - 跨平台适配器（launchd/systemd/schtasks）

---

## 环境要求

- Rust（推荐稳定版，与当前工作区依赖兼容）
- Node.js + npm
- Tauri 2 运行时依赖（本地桌面开发必需）

---

## 快速开始

### 1) 进入仓库

```bash
cd /omninovalclaw
```

### 2) 构建和测试核心

```bash
cargo check
cargo test -p omninova-core
```

### 3) 运行 CLI

```bash
cargo run -p omninova-core --bin omninova -- health
```

常用命令示例：

```bash
# 单条消息
cargo run -p omninova-core --bin omninova -- agent -m "你好"

# 启动网关
cargo run -p omninova-core --bin omninova -- gateway --host 127.0.0.1 --port 42617

# 路由调试
cargo run -p omninova-core --bin omninova -- route --channel cli -t "总结这个目录"

# e-stop
cargo run -p omninova-core --bin omninova -- estop status
cargo run -p omninova-core --bin omninova -- estop pause --reason "维护"
cargo run -p omninova-core --bin omninova -- estop resume
```

### 4) 运行桌面应用（Tauri）

```bash
cd apps/omninova-tauri
npm install
npm run tauri dev
```

常用前端命令：

```bash
npm run lint
npm run build
npm run dev
```

---

## 配置系统

配置由 `Config::load_or_init()` 管理，缺失时自动初始化配置文件。

配置目录解析优先级：

1. `OMNINOVA_CONFIG_DIR`
2. 从 `OMNINOVA_WORKSPACE` 推断
3. 指针文件 `~/.omninova/active_workspace.toml`
4. 默认 `~/.omninova/`

模板参考：

- `config.template.toml`

常用关键设置：

- Provider/model：`api_key`、`default_provider`、`default_model`
- 网关：`gateway.host`、`gateway.port`
- 会话：`gateway.session_ttl_secs`
- 子 agent 默认值：`agents.defaults.*` 和 `agents.defaults.subagents.*`

---

## HTTP 网关 API

默认绑定地址：`http://127.0.0.1:42617`

- `GET /health`
- `POST /chat`
- `POST /route`
- `POST /ingress`
- `POST /webhook`
- `GET /sessions/tree`
- `GET /estop/status`
- `POST /estop/pause`
- `POST /estop/resume`
- `GET/POST /config`

---

## 会话树查询（`/sessions/tree`）

支持的查询参数：

- 身份：`session_id` `session_key` `parent_session_id` `parent_session_key`
- Agent：`agent_name` `parent_agent_id`
- 维度：`channel` `source` `min_spawn_depth` `max_spawn_depth`
- 模糊查询：`contains` `case_insensitive`
- 分页/排序：`cursor` `offset` `limit` `sort_by` `sort_order`

推荐分页模式：

- 首次请求发送 `limit`
- 使用响应中的 `next_cursor` 获取下一页

示例：

```bash
curl "http://127.0.0.1:42617/sessions/tree?limit=20"
curl "http://127.0.0.1:42617/sessions/tree?limit=20&cursor=20"
curl "http://127.0.0.1:42617/sessions/tree?parent_agent_id=omninova&sort_by=spawn_depth&sort_order=asc"
```

---

## 开发与质量检查

常用验证流程：

```bash
# Rust
cargo test -p omninova-core
cargo check

# Tauri 前端
cd apps/omninova-tauri
npm run lint
npm run build
```

## 编译命令

`apps/omninova-tauri` 支持以下桌面端和移动端编译命令：

```bash
cd apps/omninova-tauri

# 查看全部可用目标
npm run build:list

# 检查本机构建环境
npm run check:build-env
npm run check:build-env:desktop
npm run check:build-env:mobile

# 仅构建前端
npm run build

# 桌面端
npm run build:desktop
npm run build:all:desktop
npm run build:linux
npm run build:linux:arm64
npm run build:macos
npm run build:macos:intel
npm run build:macos:apple
npm run build:windows
npm run build:windows:arm64

# 移动端
npm run mobile:init:android
npm run build:android
npm run mobile:init:ios
npm run build:ios
```

常见产物路径示例：

- Apple Silicon macOS 应用：`target/aarch64-apple-darwin/release/bundle/macos/OmniNova Claw.app`
- Apple Silicon macOS 安装包：`target/aarch64-apple-darwin/release/bundle/dmg/OmniNova Claw_0.1.0_aarch64.dmg`

---

## 代码导航

- 核心导出：`crates/omninova-core/src/lib.rs`
- CLI 命令定义：`crates/omninova-core/src/cli/mod.rs`
- 网关路由和会话树：`crates/omninova-core/src/gateway/mod.rs`
- 配置模式：`crates/omninova-core/src/config/schema.rs`
- 配置加载器：`crates/omninova-core/src/config/loader.rs`
- 桌面命令桥接：`apps/omninova-tauri/src-tauri/src/lib.rs`

---

## 许可证

工作区许可证：

- `MIT OR Apache-2.0`
