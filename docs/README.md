# OmniNova Claw 开发者文档

欢迎来到 OmniNova Claw 开发者文档！

## 📚 文档目录

### API 文档
- [REST API 参考](./api/REST.md) - 完整的 REST API 文档

### CLI 文档
- [CLI 使用指南](./cli/GUIDE.md) - 命令行工具完整指南

### 架构文档
- [架构概述](./architecture/OVERVIEW.md) - 系统架构设计说明

### 开发文档
- [开发环境设置](./development/SETUP.md) - 如何搭建开发环境
- [贡献指南](./development/CONTRIBUTING.md) - 如何为项目贡献代码

## 🚀 快速开始

### 安装 CLI

```bash
# 从源码构建
git clone https://github.com/your-org/omninova-claw.git
cd omninova-claw
cargo build --release

# 二进制文件位于
./target/release/omninova
```

### 基本使用

```bash
# 查看帮助
omninova --help

# 列出所有 agents
omninova agents list

# 与 agent 对话
omninova chat "Hello!"
```

## 🔗 相关链接

- [GitHub 仓库](https://github.com/your-org/omninova-claw)
- [问题反馈](https://github.com/your-org/omninova-claw/issues)

## 📝 版本

- 文档版本: 0.1.0
- 最后更新: 2026-03-25