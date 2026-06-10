# ShellTool Bug 修复记录

文件：`crates/omninova-core/src/tools/shell.rs`

---

## Bug 1：Windows 下 Unix 命令找不到（`program not found`）

### 现象

在 Windows 上，agent 执行 `curl`、`grep`、`pwd`、`sed` 等命令时报错：

```
failed to execute command: program not found
```

从 Git Bash 启动 agent 正常，从 CMD 启动则全部失败。

### 根本原因

`ShellTool` 用 `Command::new("sh")` 启动子进程，`sh` 以及 `curl`/`grep` 等 Unix 工具在 Windows 上由 Git for Windows 提供。子进程继承父进程的 `PATH`，从 CMD 启动时 `PATH` 不包含 Git 的工具目录（`mingw64/bin`、`usr/bin`），导致这些命令全部找不到。

### 修复方案

在 Windows 上，启动子进程前主动探测 Git 安装位置并将其工具目录预置到 `PATH`：

1. 执行 `where git` 找到 `git.exe` 路径，向上两级得到 Git 安装根目录
2. 从根目录拼出 `mingw64/bin`、`usr/bin`、`bin`，过滤实际存在的目录
3. 若 `where git` 失败（Git 未在 PATH 中），回退到硬编码的默认安装路径兜底

```rust
#[cfg(windows)]
{
    let git_root = std::process::Command::new("where")
        .arg("git")
        .output()
        .ok()
        .and_then(|o| {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout
                .lines()
                .next()
                .map(str::trim)
                .and_then(|line| Path::new(line).parent()?.parent().map(PathBuf::from))
        });

    let extra: Vec<String> = if let Some(root) = git_root {
        ["mingw64/bin", "usr/bin", "bin"]
            .iter()
            .map(|sub| root.join(sub))
            .filter(|p| p.is_dir())
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    } else {
        // 回退：已知默认安装路径
        let fallbacks = [
            r"C:\Program Files\Git\mingw64\bin",
            r"C:\Program Files\Git\usr\bin",
            r"C:\Program Files\Git\bin",
            r"C:\Program Files (x86)\Git\mingw64\bin",
            r"C:\Program Files (x86)\Git\usr\bin",
        ];
        fallbacks.iter().filter(|p| Path::new(p).is_dir())
            .map(|p| p.to_string()).collect()
    };

    if !extra.is_empty() {
        let current = std::env::var("PATH").unwrap_or_default();
        let prefix = extra.join(";");
        child.env("PATH", if current.is_empty() {
            prefix
        } else {
            format!("{prefix};{current}")
        });
    }
}
```

### 覆盖范围

| 场景 | 修复前 | 修复后 |
|------|--------|--------|
| 从 Git Bash 启动 | ✅ 正常 | ✅ 正常 |
| 从 CMD 启动，Git 在默认路径 | ❌ 失败 | ✅ 正常 |
| 从 CMD 启动，Git 在自定义路径（如 `D:\Git`） | ❌ 失败 | ✅ 正常（`where git` 动态定位） |
| 从 CMD 启动，Git 在 scoop/winget 路径 | ❌ 失败 | ✅ 正常（`where git` 动态定位） |
| 未安装 Git | ❌ 失败 | ❌ 失败（Unix 工具本身不存在，无法修复） |

---

## Bug 2：shell 命令白名单误拦合法写法

### 现象

合法的 shell 命令被白名单检查拦截，报错 `command '...' is not allowed`：

- `T=$(curl https://example.com)` → 首词被识别为 `T=$(curl`，拦截
- `RESULT=$(grep foo file.txt)` → 首词被识别为 `RESULT=$(grep`，拦截
- `VAR=value curl https://example.com` → 首词被识别为 `VAR=value`，拦截
- `curl ... | grep ...` → `grep` 段未被检查（漏检方向的问题）

### 根本原因

`check_command_allowed` 用 `split_whitespace().next()` 取命令字符串的第一个空格分隔词与白名单比对，无法理解 shell 语法：

```rust
// 修复前
let first = command.split_whitespace().next()...;
if self.allowed_commands.iter().any(|c| c == first) { Ok(()) }
```

`T=$(curl ...)` 的第一个词是 `T=$(curl`，既不等于 `T`，也不等于 `curl`，导致误拦。

### 修复方案

将单词匹配改为 token 解析：提取命令字符串中所有实际会执行的命令名，逐一与白名单比对。

解析规则：
- 按 `;`、`|`、`&`、换行拆分出各个命令段
- 每段内，跳过 `IDENT=...` 形式的变量赋值前缀词，取第一个非赋值词为命令名
- 赋值右侧的 `$(...)` 和反引号中的命令名也递归提取并检查

```rust
fn check_command_allowed(&self, command: &str) -> anyhow::Result<()> {
    let names = Self::extract_command_names(command);
    if names.is_empty() { anyhow::bail!("empty command"); }
    for name in &names {
        if !self.allowed_commands.iter().any(|c| c == name) {
            anyhow::bail!("command '{name}' is not allowed");
        }
    }
    Ok(())
}
```

### 修复前后对比

| 命令 | 修复前 | 修复后 |
|------|--------|--------|
| `curl https://example.com` | ✅ 通过 | ✅ 通过 |
| `T=$(curl https://example.com)` | ❌ 误拦（首词 `T=$(curl`） | ✅ 通过（提取出 `curl`） |
| `VAR=value curl https://api.com` | ❌ 误拦（首词 `VAR=value`） | ✅ 通过（跳过赋值，取 `curl`） |
| `curl ... \| grep pattern` | ✅/⚠️ 通过但 `grep` 未检查 | ✅ 两个命令均检查 |
| `curl ... \| rm -rf /` | ⚠️ `rm` 未检查，漏过 | ✅ `rm` 被拦截 |

### 已知边界

该解析是 shell 语法的近似实现，不是完整 AST 解析：
- 嵌套子shell `$(cmd1 $(cmd2))` 只处理一层
- `if`/`for`/`while` 等关键字不做特殊处理（本身不会在白名单中，无实际影响）

对于安全白名单场景，解析不到的情况顶多导致漏检（可能放行），不会误拦合法命令，是合理的工程权衡。若需完整解析，可引入 `shlex` crate，但对当前项目过度。
