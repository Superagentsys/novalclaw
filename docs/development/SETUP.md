# OmniNova Claw 开发环境设置

本文档帮助你搭建 OmniNova Claw 的开发环境。

## 系统要求

### 操作系统
- **macOS**: 12.0 (Monterey) 或更高版本
- **Windows**: Windows 10/11
- **Linux**: Ubuntu 22.04+ / Debian 12+ / Fedora 38+

### 硬件要求
- **内存**: 最低 8GB，推荐 16GB+
- **存储**: 至少 10GB 可用空间
- **CPU**: 支持 AVX2 指令集（大部分现代 CPU）

## 安装步骤

### 1. 安装 Rust

OmniNova Claw 需要 Rust 1.75 或更高版本。

```bash
# 安装 rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 添加到 PATH
source $HOME/.cargo/env

# 确保使用最新版本
rustup update stable

# 验证安装
rustc --version
cargo --version
```

### 2. 安装 Node.js

需要 Node.js 22 或更高版本。

```bash
# 使用 nvm 安装 (推荐)
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
source ~/.bashrc  # 或 ~/.zshrc

nvm install 22
nvm use 22

# 验证安装
node --version
npm --version
```

### 3. 安装系统依赖

#### macOS

```bash
# 安装 Xcode Command Line Tools
xcode-select --install

# 安装 Homebrew (如果未安装)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# 安装其他依赖
brew install pkg-config
```

#### Linux (Ubuntu/Debian)

```bash
sudo apt update
sudo apt install -y \
    build-essential \
    libwebkit2gtk-4.1-dev \
    libappindicator3-dev \
    librsvg2-dev \
    libgtk-3-dev \
    libssl-dev \
    pkg-config
```

#### Windows

1. 安装 [Microsoft Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
2. 安装 [Microsoft Edge WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)

### 4. 克隆仓库

```bash
git clone https://github.com/omninova/claw.git
cd claw/omninovalclaw
```

### 5. 安装前端依赖

```bash
cd apps/omninova-tauri
npm install
cd ../..
```

## 开发模式

### 运行桌面应用

```bash
cd apps/omninova-tauri
npm run tauri dev
```

这将同时启动：
- 前端开发服务器 (Vite)
- Rust 后端 (Tauri)
- 热重载支持

### 只构建 Rust

```bash
# 检查编译
cargo check

# 运行测试
cargo test

# 构建 release
cargo build --release
```

### 只运行前端

```bash
cd apps/omninova-tauri
npm run dev
```

## 项目结构

```
omninovalclaw/
├── apps/
│   └── omninova-tauri/          # Tauri 桌面应用
│       ├── src/                 # React 前端源码
│       │   ├── components/      # UI 组件
│       │   ├── pages/          # 页面
│       │   ├── hooks/          # React Hooks
│       │   └── stores/         # 状态管理
│       ├── src-tauri/          # Tauri 后端
│       │   ├── src/           # Rust 源码
│       │   └── tauri.conf.json # Tauri 配置
│       └── public/             # 静态资源
│
├── crates/
│   └── omninova-core/          # 核心运行时库
│       ├── src/
│       │   ├── agent/         # Agent 系统
│       │   ├── memory/        # 记忆系统
│       │   ├── skills/        # Skills 系统
│       │   ├── tools/         # 内置工具
│       │   ├── providers/     # LLM 提供商
│       │   ├── channels/      # 渠道适配器
│       │   ├── gateway/       # HTTP API
│       │   ├── config/       # 配置管理
│       │   └── db/            # 数据库
│       └── Cargo.toml
│
├── docs/                       # 文档
│   ├── api/                   # API 文档
│   ├── cli/                   # CLI 文档
│   ├── architecture/          # 架构文档
│   └── development/           # 开发文档
│
└── Cargo.toml                  # Workspace 配置
```

## IDE 配置

### VS Code

推荐安装以下扩展：

```json
{
  "recommendations": [
    "rust-lang.rust-analyzer",
    "tauri-apps.tauri-vscode",
    "dbaeumer.vscode-eslint",
    "esbenp.prettier-vscode"
  ]
}
```

### Rust Analyzer 配置

`.vscode/settings.json`:
```json
{
  "rust-analyzer.checkOnSave.command": "clippy",
  "rust-analyzer.cargo.features": "all"
}
```

### IntelliJ IDEA

1. 安装 Rust 插件
2. 配置 `cargo` 路径
3. 启用 `clippy` 检查

## 环境变量

创建 `.env` 文件（开发环境）：

```bash
# LLM API Keys
OMNINOVA_OPENAI_API_KEY=sk-xxx
OMNINOVA_ANTHROPIC_API_KEY=sk-ant-xxx
OMNINOVA_DEEPSEEK_API_KEY=sk-xxx

# 配置路径
OMNINOVA_CONFIG=~/.omninoval/config.toml

# 日志级别
RUST_LOG=debug

# 开发模式
TAURI_DEV=1
```

## 调试技巧

### 启用详细日志

```bash
RUST_LOG=omninova=debug,agent=trace cargo run
```

### 查看数据库

```bash
# SQLite 数据库位置
sqlite3 ~/.omninoval/data.db

# 常用查询
.tables
SELECT * FROM agents;
SELECT * FROM sessions ORDER BY created_at DESC LIMIT 10;
```

### 前端调试

React DevTools 和 Redux DevTools 浏览器扩展可用。

### 检查内存使用

```bash
# 使用 valgrind (Linux)
cargo build && valgrind --leak-check=full ./target/debug/omninova

# 使用 Instruments (macOS)
instruments -t Leaks ./target/debug/omninova
```

## 常见问题

### 编译错误: `linker 'cc' not found`

**Linux 解决方案：**
```bash
sudo apt install build-essential
```

### 编译错误: `cannot find -lwebkit2gtk-4.1`

**Linux 解决方案：**
```bash
sudo apt install libwebkit2gtk-4.1-dev
```

### 前端错误: `node-gyp rebuild failed`

**macOS 解决方案：**
```bash
xcode-select --install
npm rebuild
```

### Tauri 窗口空白

检查控制台错误：
```bash
# macOS: View -> Toggle Developer Tools
# 或在 tauri.conf.json 中启用 devtools
{
  "build": {
    "devtools": true
  }
}
```

### Rust 编译慢

使用 `sccache` 加速：
```bash
cargo install sccache
export RUSTC_WRAPPER=sccache
```

## 下一步

- 阅读 [架构概述](../architecture/OVERVIEW.md) 了解系统设计
- 阅读 [贡献指南](./CONTRIBUTING.md) 了解如何贡献代码
- 查看 [API 文档](../api/REST.md) 了解 API 接口

## 版本

- 文档版本: 1.0
- 最后更新: 2026-03-25