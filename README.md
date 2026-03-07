# OmniNova Claw

English | [中文](./README_zh.md)

OmniNova Claw is an intelligent agent project built with Rust + Tauri + React, including:

- `omninova-core`: core runtime, gateway, routing, config, tools, and security controls
- `omninova-tauri`: desktop UI (Tauri 2 + React 19)
- A unified TOML configuration model with an extensible multi-provider architecture

---

## Project Structure

```text
omninovalclaw/
├─ crates/
│  └─ omninova-core/        # Core library + CLI + Gateway
├─ apps/
│  ├─ omninova-tauri/       # Desktop app (frontend + Tauri Rust)
│  └─ omninova-ui/          # Reserved UI workspace
├─ config.template.toml     # Config template
├─ Cargo.toml               # Rust workspace
└─ Cargo.lock
```

---

## Core Capabilities

- Agent execution: message processing, tool calling, and memory integration
- Routing decisions: route by channel / metadata / binding rules to different agents/models
- HTTP gateway: health checks, routing, ingress, webhook handling, session tree, e-stop, config APIs
- Session tree governance:
  - Parent-child session relationship validation
  - Depth and concurrency limits
  - Pagination / cursor / stats / source distribution
- Security controls:
  - e-stop (pause/resume)
  - webhook signature verification and nonce replay protection
- Service operations:
  - `daemon install/start/stop/status/check`
  - Cross-platform adapters (launchd/systemd/schtasks)

---

## Requirements

- Rust (stable recommended, compatible with current workspace dependencies)
- Node.js + npm
- Tauri 2 runtime dependencies (required for local desktop development)

---

## Quick Start

### 1) Enter the repository

```bash
cd omninovalclaw
```

### 2) Build and test core

```bash
cargo check
cargo test -p omninova-core
```

### 3) Run CLI

```bash
cargo run -p omninova-core --bin omninova -- health
```

Common command examples:

```bash
# Single message
cargo run -p omninova-core --bin omninova -- agent -m "Hello"

# Start gateway
cargo run -p omninova-core --bin omninova -- gateway --host 127.0.0.1 --port 42617

# Route debug
cargo run -p omninova-core --bin omninova -- route --channel cli -t "Summarize this directory"

# e-stop
cargo run -p omninova-core --bin omninova -- estop status
cargo run -p omninova-core --bin omninova -- estop pause --reason "maintenance"
cargo run -p omninova-core --bin omninova -- estop resume
```

### 4) Run desktop app (Tauri)

```bash
cd apps/omninova-tauri
npm install
npm run tauri dev
```

Common frontend commands:

```bash
npm run lint
npm run build
npm run dev
```

---

## Configuration System

Configuration is managed by `Config::load_or_init()`, which auto-initializes config files when missing.

Config directory resolution priority:

1. `OMNINOVA_CONFIG_DIR`
2. inferred from `OMNINOVA_WORKSPACE`
3. pointer file `~/.omninova/active_workspace.toml`
4. default `~/.omninova/`

Template reference:

- `config.template.toml`

Common key settings:

- Provider/model: `api_key`, `default_provider`, `default_model`
- Gateway: `gateway.host`, `gateway.port`
- Sessions: `gateway.session_ttl_secs`
- Subagent defaults: `agents.defaults.*` and `agents.defaults.subagents.*`

---

## HTTP Gateway APIs

Default bind address: `http://127.0.0.1:42617`

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

## Session Tree Query (`/sessions/tree`)

Supported query parameters:

- Identity: `session_id` `session_key` `parent_session_id` `parent_session_key`
- Agent: `agent_name` `parent_agent_id`
- Dimensions: `channel` `source` `min_spawn_depth` `max_spawn_depth`
- Fuzzy query: `contains` `case_insensitive`
- Pagination/sort: `cursor` `offset` `limit` `sort_by` `sort_order`

Recommended pagination pattern:

- Send `limit` for the first request
- Use `next_cursor` from the response to fetch the next page

Examples:

```bash
curl "http://127.0.0.1:42617/sessions/tree?limit=20"
curl "http://127.0.0.1:42617/sessions/tree?limit=20&cursor=20"
curl "http://127.0.0.1:42617/sessions/tree?parent_agent_id=omninova&sort_by=spawn_depth&sort_order=asc"
```

---

## Development & Quality Checks

Common verification flow:

```bash
# Rust
cargo test -p omninova-core
cargo check

# Tauri frontend
cd apps/omninova-tauri
npm run lint
npm run build
```

---

## Code Navigation

- Core exports: `crates/omninova-core/src/lib.rs`
- CLI command definitions: `crates/omninova-core/src/cli/mod.rs`
- Gateway routes and session tree: `crates/omninova-core/src/gateway/mod.rs`
- Config schema: `crates/omninova-core/src/config/schema.rs`
- Config loader: `crates/omninova-core/src/config/loader.rs`
- Desktop command bridge: `apps/omninova-tauri/src-tauri/src/lib.rs`

---

## License

Workspace license:

- `MIT OR Apache-2.0`
