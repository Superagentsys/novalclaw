# OmniNova CLI 使用指南

## 安装

### 从源码构建

```bash
# 克隆仓库
git clone https://github.com/your-org/omninova-claw.git
cd omninova-claw

# 构建 release 版本
cargo build --release

# 二进制文件位置
./target/release/omninova
```

### 全局安装 (推荐)

```bash
# 复制到 PATH
sudo cp ./target/release/omninova /usr/local/bin/

# 或添加别名
echo 'alias omninova="/path/to/omninova-claw/target/release/omninova"' >> ~/.bashrc
```

---

## 全局选项

```bash
omninova [OPTIONS] <COMMAND>

选项:
  -c, --config <CONFIG>  配置文件路径
  -f, --format <FORMAT>  输出格式 (text 或 json) [默认: text]
  -s, --server <SERVER>  服务器 URL (覆盖配置)
  -v, --verbose          详细输出
  -h, --help             显示帮助
  -V, --version          显示版本
```

---

## 命令参考

### Agents 管理

#### 列出所有 Agents

```bash
omninova agents list
omninova list  # 快捷方式
```

**输出示例**:
```
● 小助手 (agent-001)
  通用助手 Agent
  Provider: openai | Model: gpt-4
  Status: enabled

Total: 3 agents
```

**JSON 输出**:
```bash
omninova agents list -f json
```

#### 查看 Agent 详情

```bash
omninova agents show <name-or-id>
```

#### 创建 Agent

```bash
omninova agents create --name "新助手" --provider openai --model gpt-4
```

#### 更新 Agent

```bash
omninova agents update <name-or-id> --name "更新名称"
```

#### 删除 Agent

```bash
omninova agents delete <name-or-id>
```

#### 切换 Agent 状态

```bash
omninova agents toggle <name-or-id>
```

---

### Skills 管理

#### 列出已安装 Skills

```bash
omninova skills list
```

**输出示例**:
```
● weather v1.0.0
  获取天气信息
  
  Tags: utility, weather

Total: 2 skills
```

#### 查看 Skill 详情

```bash
omninova skills show <name>
```

#### 安装 Skill

```bash
# 从本地目录安装
omninova skills install /path/to/skill --name my-skill

# 从 Git 仓库安装 (未来支持)
omninova skills install https://github.com/user/skill-repo.git
```

#### 卸载 Skill

```bash
omninova skills uninstall <name>
```

**强制卸载**:
```bash
omninova skills uninstall <name> --force
```

#### 验证 Skill 配置

```bash
omninova skills validate /path/to/skill
```

**输出示例**:
```
✓ SKILL.md found
✓ Name: weather
✓ Version: 1.0.0
✓ Description: Get weather information
✓ Valid skill structure

Skill is valid!
```

#### 打包 Skill

```bash
omninova skills package /path/to/skill --output ./dist/
```

---

### 配置管理

#### 查看当前配置

```bash
omninova config show
```

#### 设置配置项

```bash
omninova config set server.url http://localhost:8080
omninova config set default.provider openai
```

#### 获取配置项

```bash
omninova config get server.url
```

---

### 快速聊天

```bash
omninova chat "你好！"
```

**指定 Agent**:
```bash
omninova chat "你好！" --agent "小助手"
```

---

### 系统状态

```bash
omninova status
```

**输出示例**:
```
OmniNova Claw Status
━━━━━━━━━━━━━━━━━━━━
Version: 0.1.0
Server: http://localhost:8080 (connected)

Agents: 5 total, 3 active
Skills: 2 installed
Memory: 256 MB used
Uptime: 2h 30m
```

---

## 配置文件

配置文件默认位置: `~/.omninova/config.toml`

```toml
[server]
url = "http://localhost:8080"

[default]
provider = "openai"
model = "gpt-4"

[logging]
level = "info"
```

---

## Skills 目录结构

Skills 安装位置: `~/.omninova/skills/`

```
~/.omninova/skills/
├── weather/
│   ├── SKILL.md
│   └── scripts/
│       └── fetch.sh
└── translator/
    ├── SKILL.md
    └── ...
```

---

## 环境变量

| 变量 | 描述 |
|------|------|
| `OMNINOVA_SERVER` | 服务器 URL |
| `OMNINOVA_CONFIG` | 配置文件路径 |
| `OMNINOVA_LOG` | 日志级别 |

---

## 版本

- CLI 版本: 0.1.0
- 最后更新: 2026-03-25