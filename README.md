# OmniNova Claw

**English** | [中文](./README_zh.md)

**OmniNova Claw** is a local-first AI agent platform: core runtime, HTTP gateway, multi-provider LLM support, and a desktop control plane. You run the gateway and agents on your own machine; the desktop app (Tauri + React) gives you setup, config, and gateway control in one place.

If you want a single control plane for routing, sessions, tools, and multiple model providers (OpenAI, Anthropic, Gemini, DeepSeek, Qwen, Ollama, OpenRouter, etc.) with a native desktop UI, this is it.

OmniNova Claw is built on the **Novalclaw** architecture — a next-gen design that puts declarative routing, session governance, and a unified control plane at the center.

---

## Why Novalclaw? (What’s advanced)

Novalclaw is the architecture behind OmniNova Claw. Here’s what makes it stand out:

- **Declarative routing** — Route by **channel**, **account_id**, **guild_id**, **team_id**, **peer**, or **roles** via TOML `[bindings]`. No code: a single config entry can send “Slack #eng” to one agent and “Discord DMs” to another. Explicit `metadata.agent` overrides bindings for debugging or APIs.
- **Session tree & lineage** — Every request is tied to a **session** with **parent-child lineage**, **spawn_depth**, and **TTL**. The gateway enforces depth and concurrency limits, and exposes **`/sessions/tree`** with pagination (cursor/limit), filters (channel, agent, depth), and stats. You get observability and governance (who spawned whom, how deep) out of the box.
- **Single control plane** — One HTTP gateway serves **chat**, **route**, **ingress**, **webhook**, **session tree**, **e-stop**, and **config**. No separate “router” and “runtime” services; one process, one bind address, one config.
- **Provider-agnostic agent loop** — The same **Agent** loop (messages → provider → tool calls → memory) runs over **OpenAI**, **Anthropic**, **Gemini**, **Ollama**, **OpenRouter**, or any OpenAI-compatible endpoint. Swap provider/model in config or via routing; no code change.
- **Delegate agents** — Multiple **named agents** (e.g. `researcher`, `coder`) with their own **provider**, **model**, **system_prompt**, and **tool allowlists**. Routing picks the agent; the gateway builds the right provider and tools per request. Enables “right model for the right job” without separate services.
- **Security by default** — **E-stop** (pause/resume at gateway level, optional OTP), **webhook signature** verification and **nonce** replay protection, and **tool policy** (allowlist/denylist, dangerous-tool checks) are first-class in the schema and runtime.
- **Local-first & config-driven** — **TOML** + **env overrides**; config dir from `OMNINOVA_CONFIG_DIR` or workspace. No vendor lock-in; run the gateway and desktop app entirely on your own machine.
- **Production-ready ops** — **Daemon** (install/start/stop/status/check) with **launchd**, **systemd**, and **schtasks** adapters; **health** and **config** APIs for monitoring and hot config.

---

## Highlights

- **Local-first gateway** — HTTP API for chat, route, ingress, webhook, session tree, e-stop, and config; default bind `http://127.0.0.1:42617`.
- **Channels** — WhatsApp (Baileys), Telegram (grammY), Slack (Bolt), Discord (discord.js), Google Chat, Signal (signal-cli), BlueBubbles (iMessage), iMessage (legacy), IRC, Microsoft Teams, Matrix, Feishu, LINE, Mattermost, Nextcloud Talk, Nostr, Synology Chat, Tlon, Twitch, WeChat, Zalo, Zalo Personal, Lark, 钉钉 (DingTalk), WebChat, Email, Webhook. Route by channel in `[bindings]`; platform webhooks at `/webhook/wechat`, `/webhook/feishu`, `/webhook/lark`, `/webhook/dingtalk`.
- **Multi-provider** — OpenAI, Anthropic, Gemini, DeepSeek, Qwen, Moonshot, xAI, Mistral, Groq, OpenRouter, Ollama, LM Studio; extensible via TOML and env.
- **Routing & agents** — Route by channel, metadata, or bindings to different agents/models; delegate agents with custom prompts and tool allowlists.
- **Session tree** — Parent-child sessions, depth/concurrency limits, pagination, cursor, and source distribution for governance.
- **Tools** — File read/edit, shell (with policy), and pluggable tool trait; memory (conversation/store) and security (e-stop, webhook signature, nonce).
- **Desktop app** — Tauri 2 + React 19 setup UI: providers, default model, gateway URL, save config, start/stop gateway; build to .app / .dmg / Windows / Linux.
- **Daemon** — Install/start/stop/status/check for gateway as a background service (launchd, systemd, schtasks).

---

## How it works (short)

```
CLI / Web / Webhook / (future: Telegram, Discord, Slack, …)
                    │
                    ▼
┌─────────────────────────────────────────────────────────┐
│                  OmniNova Gateway                        │
│              http://127.0.0.1:42617                      │
│   /health  /chat  /route  /ingress  /webhook            │
│   /sessions/tree  /estop  /config                        │
└──────────────────────────┬──────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        ▼                   ▼                   ▼
   Routing           Agent (omninova /        Provider
   (channel,             delegate agents)     (OpenAI, Anthropic,
   metadata,                                   Gemini, Ollama, …)
   bindings)
                            │
                            ├── Tools (file_read, file_edit, shell)
                            ├── Memory (conversation, store)
                            └── Security (e-stop, webhook verify)
```

---

## Project structure

```text
omninovalclaw/
├── crates/
│   └── omninova-core/          # Core library + CLI + Gateway
│       ├── agent/               # Agent loop, dispatcher, prompt
│       ├── channels/            # ChannelKind, InboundMessage, adapters (cli, webhook)
│       ├── config/              # Schema, loader, validation, env
│       ├── daemon/              # launchd, systemd, schtasks
│       ├── gateway/             # HTTP server, routes, session tree, e-stop
│       ├── memory/              # Traits, backend, factory
│       ├── providers/           # Provider trait, OpenAI, Anthropic, Gemini, factory
│       ├── routing/             # resolve_agent_route, bindings
│       ├── security/            # e-stop, tool policy, webhook auth
│       ├── tools/               # file_read, file_edit, shell
│       └── util/                # auth, crypto, network
├── apps/
│   └── omninova-tauri/          # Desktop app (Tauri 2 + React 19)
│       ├── src/                 # Setup UI, config types, tauri invoke
│       └── src-tauri/            # Rust shell, gateway runtime bridge
├── config.template.toml         # Full config template
├── Cargo.toml                   # Workspace
└── .github/workflows/           # CI (e.g. omninova-tauri-build.yml)
```

---

## Architecture (detailed)

### 1. Configuration

- **Source**: `Config::load_or_init()` — resolves config directory (env `OMNINOVA_CONFIG_DIR`, `OMNINOVA_WORKSPACE`, or `~/.omninova/`), then loads `config.toml`.
- **Schema**: Single TOML with `api_key`, `default_provider`, `default_model`, `[model_providers.*]`, `[agent]`, `[agents.<name>]` (delegate agents), `[gateway]`, `[bindings]`, `[security]`, `[memory]`, `[autonomy]`, etc. See `config.template.toml`.
- **Override**: Environment variables (`OMNINOVA_*`) override file values; used for API keys and gateway host/port.

### 2. Gateway (HTTP control plane)

- **Role**: Single HTTP server binding (default `127.0.0.1:42617`) that accepts chat, routing, ingress, webhooks, session tree queries, e-stop, and config get/set.
- **Routes**:
  - `GET /` — Service info and links to `/health`, `/chat`, `/config`.
  - `GET /health` — Provider health and gateway status.
  - `POST /chat` — User message → routing → agent → provider/tools → reply.
  - `POST /route` — Returns `RouteDecision` (agent_name, provider, model) for an inbound payload.
  - `POST /ingress` — Inbound message with session validation and lineage.
  - `POST /webhook` — Webhook payload with signature/nonce verification.
  - `GET /sessions/tree` — Paginated session tree (filters: channel, agent, depth, cursor, etc.).
  - `GET /estop/status`, `POST /estop/pause`, `POST /estop/resume` — Emergency stop.
  - `GET/POST /config` — Read or update config.
- **Lifecycle**: Built from `GatewayRuntime::new(config)`. In the Tauri app, `start_gateway` spawns `serve_http()` in a tokio task; `stop_gateway` aborts it.

### 3. Routing

- **Input**: `InboundMessage` (channel, user_id, session_id, text, metadata).
- **Logic**: `resolve_agent_route(config, inbound)`:
  - If `metadata.agent` is set → use that agent.
  - Else match `config.bindings` (channel, account_id, guild_id, team_id, etc.) → bound agent.
  - Else use `acp.default_agent` or main `agent.name`.
- **Output**: `RouteDecision { agent_name, provider, model }`. Delegate agents get provider/model from `config.agents.<name>`; otherwise from default provider/model.

### 4. Agent

- **Single agent run**: Load provider (from factory by provider/model), tools (file_read, file_edit, shell), memory backend; build `Agent` with system prompt and limits.
- **Loop**: `process_message` → append user message, call `AgentDispatcher::run` (provider chat + tool calls, max_tool_iterations), return assistant reply.
- **Delegate agents**: Defined in `config.agents`; same pipeline with their own provider/model and optional tool allowlists.

### 5. Providers

- **Trait**: `Provider::chat()`, `health_check()`, optional `models()`.
- **Implementations**: `OpenAiProvider` (OpenAI-compatible: OpenAI, DeepSeek, Qwen, Moonshot, Groq, xAI, Mistral, Ollama, LM Studio, OpenRouter), `AnthropicProvider`, `GeminiProvider`, `MockProvider`.
- **Factory**: `build_provider_with_selection(config, selection)` resolves api_key (profile or env), base_url (profile or default per provider), and builds the right boxed provider.

### 6. Tools & memory

- **Tools**: `FileReadTool`, `FileEditTool`, `ShellTool`; each implements `Tool` (spec, execute). Security layer can restrict shell and paths.
- **Memory**: `Memory` trait (store, recall); backends (e.g. in-memory, SQLite) and categories (e.g. conversation). Used by agent for context and persistence.

### 7. Security

- **E-stop**: Pause/resume at gateway level; optional OTP to resume.
- **Webhook**: Signature verification (e.g. x-hub-signature-256), optional nonce and timestamp replay protection.
- **Tool policy**: Allowlist/denylist and dangerous-tool checks for shell and file access.

### 8. Channels

- **Types**: `ChannelKind`: Cli, Web, Telegram, Discord, Slack, Whatsapp, Matrix, Email, Webhook, Other.
- **Adapters**: CLI and webhook turn raw input into `InboundMessage`; routing then picks agent/provider/model.

### 9. Desktop app (omninova-tauri)

- **Stack**: Tauri 2, React 19, Vite, TypeScript.
- **Responsibilities**: Setup UI (workspace, gateway URL, provider/model dropdowns, robot config), load/save config via Tauri commands, start/stop gateway in-process, show gateway status and last error.
- **Bridge**: `get_setup_config`, `save_setup_config`, `gateway_status`, `start_gateway`, `stop_gateway` in `src-tauri/src/lib.rs` map UI to `Config` and `GatewayRuntime`.

### 10. Daemon

- **Commands**: `omninova daemon install|uninstall|start|stop|status|check` — manage gateway as a background service.
- **Adapters**: launchd (macOS), systemd (Linux), schtasks (Windows).

---

## Requirements

- **Rust** (stable); **Node.js** + npm (for desktop app).
- **Tauri 2** system deps for desktop (see [Tauri docs](https://v2.tauri.app/start/prerequisites/)).

---

## Quick start

### 1. Clone and build core

```bash
cd omninovalclaw
cargo check
cargo test -p omninova-core
```

### 2. Run CLI

```bash
# Health
cargo run -p omninova-core --bin omninova -- health

# Single message (uses default provider/model from config)
cargo run -p omninova-core --bin omninova -- agent -m "Hello"

# Start gateway (default 127.0.0.1:42617)
cargo run -p omninova-core --bin omninova -- gateway

# Route debug
cargo run -p omninova-core --bin omninova -- route --channel cli -t "Summarize this"
```

### 3. Run desktop app (recommended for setup)

```bash
cd apps/omninova-tauri
npm install
npm run tauri dev
```

In the Setup UI: set workspace, add providers/models, set gateway URL, then **Save config** and **Save and start gateway**. Open the gateway link in the browser to see `GET /` service info and `GET /health` for health.

### 4. Call the gateway (e.g. after starting it)

```bash
curl http://127.0.0.1:42617/health
curl -X POST http://127.0.0.1:42617/chat \
  -H "Content-Type: application/json" \
  -d '{"user_id":"user1","session_id":"s1","message":"Hello"}'
```

---

## Configuration

- **Config dir**: `OMNINOVA_CONFIG_DIR`, or derived from `OMNINOVA_WORKSPACE`, or `~/.omninova/` (see pointer file `~/.omninova/active_workspace.toml`).
- **Template**: Copy and edit `config.template.toml` to `~/.omninova/config.toml` (or your resolved dir).
- **Important keys**: `api_key`, `default_provider`, `default_model`, `[gateway]` (host, port), `[agent]`, `[agents.<name>]`, `[model_providers.*]`, `[bindings]`.

---

## HTTP Gateway APIs (reference)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | Service info and API links |
| GET | `/health` | Gateway and provider health |
| POST | `/chat` | Send message, get reply |
| POST | `/route` | Get route decision for an inbound message |
| POST | `/ingress` | Inbound with session lineage validation |
| POST | `/webhook` | Webhook with signature/nonce support |
| GET | `/sessions/tree` | Session tree (pagination: cursor, limit, filters) |
| GET | `/estop/status` | E-stop status |
| POST | `/estop/pause` | Pause (optional reason) |
| POST | `/estop/resume` | Resume |
| GET/POST | `/config` | Read or update config |

---

## Session tree (`/sessions/tree`)

Query params: `session_id`, `session_key`, `parent_session_id`, `agent_name`, `channel`, `min_spawn_depth`, `max_spawn_depth`, `contains`, `cursor`, `offset`, `limit`, `sort_by`, `sort_order`. Use `limit` and `next_cursor` for paging.

---

## Build (desktop / release)

From `apps/omninova-tauri`:

```bash
npm run build:list          # List targets
npm run check:build-env     # Check prerequisites
npm run build:desktop       # Current host
npm run build:macos         # macOS (current arch)
npm run build:macos:apple   # Apple Silicon
npm run build:macos:intel   # Intel Mac
npm run build:windows
npm run build:linux
```

Outputs (example macOS): `target/release/bundle/macos/OmniNova Claw.app`, `target/release/bundle/dmg/OmniNova Claw_0.1.0_aarch64.dmg`. Push a tag (e.g. `v0.1.0`) to trigger the GitHub workflow; signing/secrets: `.github/omninova-tauri-secrets.example.md`.

---

## Code navigation

| Area | Path |
|------|------|
| Core exports | `crates/omninova-core/src/lib.rs` |
| CLI | `crates/omninova-core/src/cli/mod.rs` |
| Gateway routes & session tree | `crates/omninova-core/src/gateway/mod.rs` |
| Config schema | `crates/omninova-core/src/config/schema.rs` |
| Config load | `crates/omninova-core/src/config/loader.rs` |
| Routing | `crates/omninova-core/src/routing/mod.rs` |
| Agent loop | `crates/omninova-core/src/agent/agent.rs`, `agent/dispatcher.rs` |
| Providers | `crates/omninova-core/src/providers/factory.rs`, `openai.rs`, `anthropic.rs`, `gemini.rs` |
| Tools | `crates/omninova-core/src/tools/` |
| Tauri bridge | `apps/omninova-tauri/src-tauri/src/lib.rs` |

---

## License

**MIT OR Apache-2.0**
