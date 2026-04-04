# OmniNova Claw

<div align="center">
  <img src="apps/omninova-tauri/public/omninoval-logo.png" alt="OmniNova Claw Logo" width="200" height="200" />
  <p><strong>下一代 AI Agent 平台与桌面控制中心</strong></p>
  <p>
    <a href="README.md">English Docs</a> | 
    <a href="#特性">特性</a> | 
    <a href="#快速开始">快速开始</a> | 
    <a href="#架构">架构</a>
  </p>
</div>

---

**OmniNova Claw** 是一个功能强大的、本地优先的 AI Agent 平台，基于 **Novalclaw** 架构构建。它结合了高性能的 Rust 核心运行时与现代化的 Tauri + React 桌面界面，让您能够完全掌控您的 AI Agent、技能 (Skills) 和模型供应商。

无论您是构建复杂的 Agent 工作流，管理多个 LLM 供应商（OpenAI、Anthropic、Gemini、DeepSeek 等），还是在各种渠道（Slack、Discord、微信等）部署机器人，OmniNova Claw 都能为您提供一个统一、安全且可扩展的基础。

## ✨ 特性

### 👻 灵魂系统 (Soul System & MBTI)
**灵魂系统** 赋予您的 Agent 独特的身份和行为框架，深度结合 **MBTI** 心理学模型。
- **MBTI 人格构建**: 利用 MBTI 类型（如 **INTJ** 战略家、**ENFP** 竞选者）来定义 Agent 的认知模式。系统将这些类型转化为独特的推理逻辑和沟通风格，使 Agent 更具“人性”。
- **系统提示词 (System Prompt)**: 定义 Agent 的核心人格、语气和行为约束。
- **行为控制**: 微调交互风格、上下文处理方式（如 `compact_context` 压缩上下文）以及工具使用限制。
- **自适应人设**: 根据任务或渠道的不同，在不同的“灵魂”（如程序员、研究员、助手）之间灵活切换。

### 🧠 三层记忆系统 (Three-Layer Memory System)
OmniNova Claw 实现了精密的三层认知记忆架构：
1.  **工作记忆 (Working Memory - 短期)**: 管理当前的对话上下文，通过智能 Token 压缩和滑动窗口机制，确保 Agent 专注于当下的任务。
2.  **情景记忆 (Episodic Memory - 长期)**: 存储和检索过去的交互历史，保留会话的血缘关系 (Lineage)，使 Agent 能够回忆起之前的上下文。
3.  **语义/技能记忆 (Semantic/Skill Memory - 知识)**: 基于加载的技能 (`SKILL.md`) 和外部文档构建的持久化知识库，允许 Agent 运用特定领域的专业知识。

### 🛠️ 强大的工具与能力
- **内置工具**: 文件操作、Web 搜索、PDF 阅读、Git 操作、Shell 执行（沙箱环境）。
- **技能系统 (Skills System)**: 可扩展的能力系统，兼容 OpenClaw 技能格式。支持从 `SKILL.md` 或本地目录加载技能。
- **ACP 协议**: 实现了 Agent Control Protocol，用于标准化的 Agent-工具交互。
- **安全第一**: 内置 E-stop (紧急停止) 机制、工具策略强制执行以及危险命令过滤。

### 🔌 通用连接性
- **多模型支持**: 无缝切换 OpenAI, Anthropic, Gemini, DeepSeek, Qwen, Ollama 等多种模型。
- **全渠道接入**: 将 Agent 连接到 Slack, Discord, Telegram, 微信, 飞书, Lark, 钉钉, WhatsApp, Email 和 Webhook。
- **声明式路由**: 基于渠道、用户或元数据将消息路由到特定的 Agent，无需编写代码。

### 🖥️ 现代桌面体验
- **跨平台**: 提供 **macOS** (Apple Silicon/Intel), **Windows**, 和 **Linux** 的原生应用。
- **可视化配置**: 通过直观的 React UI 配置供应商、渠道和技能。
- **本地网关**: 在本地运行完整的技术栈，内置 HTTP 网关和守护进程管理。

## 🚀 快速开始

### 前置要求
- **Rust**: 最新稳定版 (`rustup update`)
- **Node.js**: 版本 22+ (`node -v`)
- **系统依赖**:
  - **Linux**: `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`
  - **Windows**: Microsoft Visual Studio C++ Build Tools

### 安装步骤

1.  **克隆仓库**
    ```bash
    git clone https://github.com/omninova/claw.git
    cd claw/omninovalclaw
    ```

2.  **安装依赖**
    ```bash
    # 安装前端依赖
    cd apps/omninova-tauri
    npm install
    ```

3.  **开发模式运行**
    ```bash
    # 运行 Tauri 应用 (前端 + Rust 后端)
    npm run tauri dev
    ```

4.  **构建发布版本**
    ```bash
    # 为当前操作系统构建应用
    npm run tauri build
    ```
    构建产物将生成在 `apps/omninova-tauri/src-tauri/target/release/bundle/` 目录下。

## 🏗️ 架构

OmniNova Claw 采用模块化的工作区结构：

```text
omninovalclaw/
├── skills/                  # 内置 SKILL.md 技能包（可导入工作区）
├── apps/
│   └── omninova-tauri/      # 桌面前端 (React 19 + TypeScript) & Tauri 配置
│       ├── src/             # UI 组件 (Setup, Chat, Console)
│       ├── src-tauri/       # Tauri 后端入口
│       └── public/          # 静态资源
├── crates/
│   └── omninova-core/       # 核心运行时库
│       ├── agent/           # Agent 逻辑 & 调度器
│       ├── skills/          # 技能系统实现
│       ├── tools/           # 原生工具 (PDF, Web, File 等)
│       ├── providers/       # LLM 供应商集成
│       ├── channels/        # IM & Webhook 适配器
│       └── gateway/         # HTTP API 网关
└── .github/workflows/       # CI/CD 流水线 (release.yml)
```

## ⚙️ 配置

OmniNova Claw 使用 `config.toml` 文件进行配置，可以通过桌面 UI 管理或手动编辑。

- **配置位置**: `~/.omninoval/config.toml` (默认)
- **环境变量**: 可以覆盖配置设置 (例如 `OMNINOVA_OPENAI_API_KEY`)。

桌面应用提供了 **设置向导 (Setup Wizard)** 以便轻松配置：
- **供应商 (Providers)**: API Key 和 Base URL。
- **渠道 (Channels)**: 机器人 Token 和 Webhook 密钥。
- **技能 (Skills)**: 启用/禁用 Open Skills 并设置导入路径。仓库内置示例见 `skills/`（含 **金融分析** `financial-analysis`、**估值** `financial-valuation`、**量化研究** `quantitative-research`、**量化回测** `quantitative-backtest`）；可在仓库根目录执行 `omninova skills import --from ./skills` 导入到默认技能目录。
- **人设 (Persona)**: 定义 Agent 的系统提示词和行为。

## 📦 发布

我们使用 GitHub Actions 进行自动化的跨平台构建。
- **稳定版本**: 标记为 `v*` (例如 `v0.1.0`)。
- **平台支持**:
  - macOS (Universal/Apple Silicon) `.dmg`
  - Windows (x64) `.msi`
  - Linux (x64) `.AppImage` / `.deb`

## 📄 许可证

本项目基于 [MIT License](LICENSE) 授权。

---

<div align="center">
  <sub>Built with ❤️ by the OmniNova Team</sub>
</div>
