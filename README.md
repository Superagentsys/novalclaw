# OmniNova Claw

<div align="center">
  <img src="apps/omninova-tauri/public/omninoval-logo.png" alt="OmniNova Claw Logo" width="200" height="200" />
  <p><strong>A Next-Gen AI Agent Platform & Desktop Control Plane</strong></p>
  <p>
    <a href="README_zh.md">中文文档</a> | 
    <a href="#features">Features</a> | 
    <a href="#getting-started">Getting Started</a> | 
    <a href="#architecture">Architecture</a>
  </p>
</div>

---

**OmniNova Claw** is a powerful, local-first AI agent platform built on the **Novalclaw** architecture. It combines a high-performance Rust core runtime with a modern Tauri + React desktop interface, giving you complete control over your AI agents, skills, and model providers.

Whether you're building complex agent workflows, managing multiple LLM providers (OpenAI, Anthropic, Gemini, DeepSeek, etc.), or deploying bots across various channels (Slack, Discord, WeChat, etc.), OmniNova Claw provides a unified, secure, and extensible foundation.

## ✨ Features

### 👻 Soul System (Agent Persona & MBTI)
The **Soul System** gives your agent a unique identity and behavioral framework, deeply integrated with **MBTI** psychology.
- **MBTI-Driven Personality**: Architect your agent's cognition using MBTI types (e.g., **INTJ** for logical strategy, **ENFP** for creative empathy). The system translates these types into distinct reasoning patterns and communication styles.
- **System Prompt**: Define the core personality, tone, and constraints of your agent.
- **Behavioral Control**: Fine-tune interaction styles, context handling (`compact_context`), and tool usage limits.
- **Adaptive Persona**: Switch between different "Souls" (e.g., Coder, Researcher, Assistant) based on the task or channel.

### 🧠 Three-Layer Memory System
OmniNova Claw implements a sophisticated cognitive architecture with three distinct memory layers:
1.  **Working Memory (Short-term)**: Manages the immediate conversation context with intelligent token compression and sliding windows to maintain focus.
2.  **Episodic Memory (Long-term)**: Stores and retrieves past interaction history, preserving the lineage of sessions and enabling the agent to recall previous contexts.
3.  **Semantic/Skill Memory (Knowledge)**: A persistent knowledge base derived from loaded Skills (`SKILL.md`) and external documents, allowing the agent to utilize specialized domain knowledge.

### 🛠️ Powerful Tools & Capabilities
- **Built-in Tools**: File operations, Web Search, PDF reading, Git operations, Shell execution (sandboxed).
- **Skills System**: Extensible capability system compatible with OpenClaw skills. Load skills from `SKILL.md` or local directories.
- **ACP Protocol**: Implements the Agent Control Protocol for standardized agent-tool interaction.
- **Safety First**: E-stop mechanism, tool policy enforcement, and dangerous command filtering.

### 🔌 Universal Connectivity
- **Multi-Provider Support**: Seamlessly switch between OpenAI, Anthropic, Gemini, DeepSeek, Qwen, Ollama, and more.
- **Omni-Channel**: Connect your agents to Slack, Discord, Telegram, WeChat, Feishu, Lark, DingTalk, WhatsApp, Email, and Webhooks.
- **Declarative Routing**: Route messages to specific agents based on channel, user, or metadata without writing code.

### 🖥️ Modern Desktop Experience
- **Cross-Platform**: Native apps for **macOS** (Apple Silicon/Intel), **Windows**, and **Linux**.
- **Visual Configuration**: Configure providers, channels, and skills through an intuitive React-based UI.
- **Local Gateway**: Run the entire stack locally with a built-in HTTP gateway and daemon management.

## 🚀 Getting Started

### Prerequisites
- **Rust**: Latest stable version (`rustup update`)
- **Node.js**: Version 22+ (`node -v`)
- **System Dependencies**:
  - **Linux**: `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`
  - **Windows**: Microsoft Visual Studio C++ Build Tools

### Installation

1.  **Clone the repository**
    ```bash
    git clone https://github.com/omninova/claw.git
    cd claw/omninovalclaw
    ```

2.  **Install dependencies**
    ```bash
    # Install frontend dependencies
    cd apps/omninova-tauri
    npm install
    ```

3.  **Run in Development Mode**
    ```bash
    # Run Tauri app (Frontend + Rust Backend)
    npm run tauri dev
    ```

4.  **Build for Production**
    ```bash
    # Build the application for your OS
    npm run tauri build
    ```
    Artifacts will be generated in `apps/omninova-tauri/src-tauri/target/release/bundle/`.

## 🏗️ Architecture

OmniNova Claw follows a modular workspace structure:

```text
omninovalclaw/
├── apps/
│   └── omninova-tauri/      # Desktop Frontend (React 19 + TypeScript) & Tauri Config
│       ├── src/             # UI Components (Setup, Chat, Console)
│       ├── src-tauri/       # Tauri Backend Entrypoint
│       └── public/          # Static Assets
├── crates/
│   └── omninova-core/       # Core Runtime Library
│       ├── agent/           # Agent Logic & Dispatcher
│       ├── skills/          # Skills System Implementation
│       ├── tools/           # Native Tools (PDF, Web, File, etc.)
│       ├── providers/       # LLM Provider Integrations
│       ├── channels/        # IM & Webhook Adapters
│       └── gateway/         # HTTP API Gateway
└── .github/workflows/       # CI/CD Pipelines (release.yml)
```

## ⚙️ Configuration

OmniNova Claw uses a `config.toml` file for configuration, which can be managed via the Desktop UI or edited manually.

- **Config Location**: `~/.omninoval/config.toml` (default)
- **Environment Variables**: Can override config settings (e.g., `OMNINOVA_OPENAI_API_KEY`).

The Desktop App provides a **Setup Wizard** to easily configure:
- **Providers**: API Keys and Base URLs.
- **Channels**: Bot tokens and Webhook secrets.
- **Skills**: Enable/Disable Open Skills and set import paths.
- **Persona**: Define your agent's system prompt and behavior.

## 📦 Releases

We use GitHub Actions for automated cross-platform builds.
- **Stable Releases**: Tagged with `v*` (e.g., `v0.1.0`).
- **Platform Support**:
  - macOS (Universal/Apple Silicon) `.dmg`
  - Windows (x64) `.msi`
  - Linux (x64) `.AppImage` / `.deb`

## 📄 License

This project is licensed under the [MIT License](LICENSE).

---

<div align="center">
  <sub>Built with ❤️ by the OmniNova Team</sub>
</div>
