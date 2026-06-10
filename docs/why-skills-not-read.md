# 为什么 Agent 不读 Skill —— 原因按可能性排序

> 环境：OmniNova Claw / omninova-core 0.1.0 / headless CLI 与 gateway 路径
> 方法：通读 `skills`、`config`、`gateway`、`agent`、`tools`、`security` 全链路后，按"是某次实测中 skill 没被读取的真正原因"的概率从高到低排序。
> 结论先说：这不是单一 bug，而是 **5 个独立原因叠加**，任意一个都足以让 skill 系统空转；越靠前的越可能是"你这次没读到"的直接原因。

---

## 排序总览

| # | 原因 | 性质 | 触发条件 | 单独是否致命 |
|---|------|------|----------|--------------|
| 1 | `open_skills_enabled` 默认 `false` | 配置默认值 | 用户未显式开启 | 是（根本没注入） |
| 2 | skills 目录在 workspace 外，`file_read` 够不到 | 架构死锁 | skills 已注入、模型想读 | 是（必然读失败） |
| 3 | summary 模式只给目录、无检索路由、纯靠模型自觉 | 设计缺陷 | skill 已启用且注入 | 是（模型不主动读） |
| 4 | 发现类工具（glob/content_search）在 supervised 下被审批拦截 | 自治策略 | CLI 无审批通道 | 是（无法定位 skill 文件） |
| 5 | 未配 mode 时默认 Full，754 个 skill 撑爆上下文 | 设计缺陷 | 启用但没设 summary | 部分（噪声/溢出） |

---

## 原因 1【最可能】`open_skills_enabled` 默认为 `false`，skill 根本没被加载

**证据**：`crates/omninova-core/src/config/schema.rs:1134-1140`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillsConfig {
    #[serde(default)]
    pub open_skills_enabled: bool,   // ← bool 的 Default 是 false
    pub open_skills_dir: Option<String>,
    pub prompt_injection_mode: Option<String>,
}
```

注入逻辑在 `gateway/mod.rs:129` 和 `:231` 两处都被一个 if 包着：

```rust
if cfg.skills.open_skills_enabled {        // ← false 时整段跳过
    let skills_dir = ...;
    if let Ok(skills) = load_skills_from_dir(&skills_dir) { ... }
}
```

**推断**：只要用户没在 `config.toml` 里显式写 `open_skills_enabled = true`，这段加载+注入逻辑**完全不执行**——system prompt 里连 skill 目录都没有，模型当然"不读 skill"，因为它根本不知道 skill 存在。

`config.template.toml:86` 里这一行还是注释掉的（`# open_skills_enabled = true`），新用户拿到默认配置就是关闭状态。

> 这是对"全新用户/默认配置"场景下概率最高的原因。如果你的实测里 system prompt 里**连 skill 目录都没出现**，那就是这一条。

---

## 原因 2【确定致命】skills 目录在 workspace 之外，`file_read` 架构上够不到

即使原因 1 解决了（skill 已启用、目录已注入 prompt），模型想读也读不到。

**证据**：
- `file_read` 锁死在 workspace：`gateway/mod.rs:2491` → `FileReadTool::new(workspace.clone())`
- 路径双重闸门：`tools/file_read.rs:19-30`

```rust
if rel.is_absolute() {
    anyhow::bail!("absolute paths are not allowed");   // 闸门1：拒绝绝对路径
}
let full_path = self.workspace_dir.join(rel);
let resolved = tokio::fs::canonicalize(&full_path).await?;
let workspace = tokio::fs::canonicalize(&self.workspace_dir).await?;
if !resolved.starts_with(&workspace) {
    anyhow::bail!("path escapes workspace");           // 闸门2：禁止逃出 workspace
}
```

- 路径分歧：
  - `workspace_dir` 默认 `~/.omninova/workspace`（`schema.rs:177`）
  - `skills_dir` 默认 `~/.omninova/skills`，且 `skills/cybersecurity/README.md:23` 推荐配成仓库绝对路径
  - 两者默认就**不在同一棵子树**，skills 在 workspace 外

**死锁**：prompt 让模型"去 skills 目录读 SKILL.md"，但
- 传绝对路径 → 闸门1 拒绝
- 传 `../skills/...` 相对路径 → `canonicalize` 后逃出 workspace → 闸门2 拒绝

模型两条路都被拒，试几次就放弃。**这与模型能力无关，是确定性的逻辑死锁**——换任何模型结果都一样。

> 唯一的例外：用户把 `open_skills_dir` 设成 `~/.omninova/workspace/skills`（workspace 内）。但 754 个 skill 几乎不会这么放，README 也不这么推荐。

---

## 原因 3【已知设计缺陷 / BUG-7】无相关性检索，summary 模式纯靠模型自觉

即使原因 1、2 都解决，注入策略本身也不奏效。

**证据**：`skills/mod.rs:116-128`（Summary 模式）

```rust
SkillPromptMode::Summary => {
    let mut prompt = String::from(
        "...When a skill is relevant, read its full `SKILL.md` ... before acting.\n\n",
    );
    for skill in skills {
        prompt.push_str(&format!("- **{}**: {}\n", name, description));  // 只有名字+描述
    }
}
```

- 注入的只是一句**软提示** + 一份 754 行的名字目录
- **全项目无任何相关性检索 / 关键词路由 / 自动按需注入机制**（已 grep 确认，没有任何调用按任务挑 Top-N skill）
- 读不读完全由模型自行决定

**实测对照**：`BUG清单.md` BUG-7 记录 DeepSeek / GLM-4.6 在自主渗透任务里 `file_read` 调用数为 0，754 个 skill 全程未被触碰。

> 这是"功能能跑、不报错，但形同虚设"的设计缺陷，不是代码错误。

---

## 原因 4【自治策略连带】发现类工具在 supervised 下被审批拦截，模型无法定位 skill 文件

模型要读 skill，通常得先"列目录/搜文件"确认有哪些 SKILL.md。但这些发现类工具在默认自治级别下被拦。

**证据**：
- 默认自治级别 `supervised`：`schema.rs:520-522`
- 默认自动批准只含 `file_read` 和 `memory_recall`：`schema.rs:539-541`

```rust
fn default_auto_approve() -> Vec<String> {
    vec!["file_read".into(), "memory_recall".into()]   // glob_search / content_search 不在内
}
```

- supervised 下非自动批准的工具一律要审批：`tool_policy.rs:171-176`

```rust
_ => ToolPolicyDecision::RequireApproval {
    reason: format!("tool '{tool_name}' requires approval under supervised autonomy"),
}
```

- CLI `agent -m` 无审批入口（`BUG清单.md` 补充观察已记录 `glob_search` 被 `requires approval under supervised autonomy` 拦截）

**后果**：`glob_search` / `content_search` 被拦 → 模型无法枚举 skills 目录里有哪些文件 → 即使想读也不知道读哪个路径。`file_read` 虽自动批准，但缺了发现步骤，等于有钥匙没有地址。

---

## 原因 5【次要】未配 mode 时默认 Full，754 个 skill 撑爆上下文

**证据**：`skills/mod.rs:96-102`

```rust
pub fn from_config(value: Option<&str>) -> Self {
    match value... {
        Some("summary") => SkillPromptMode::Summary,
        Some("disabled") | ... => SkillPromptMode::Disabled,
        _ => SkillPromptMode::Full,        // ← None 时落到 Full
    }
}
```

**后果**：用户启用了 skill 但没设 `prompt_injection_mode`，默认走 **Full 模式**——把全部 754 个 SKILL.md 完整内容一次性塞进 system prompt。结果要么上下文溢出、要么 prompt 被截断、要么淹没真正的任务指令。`config.template.toml:89` 和 cybersecurity README 都专门警告"754 skills 要用 summary 避免溢出"，说明这是已知坑。

> 这条多数情况下表现为"读了但没用好/响应异常"，而非"完全不读"，所以排在最后。

---

## 综合结论

按"某次实测中 skill 没被读取"的直接原因概率排序：

1. **没开开关**（原因 1）——默认 `false`，最常见，新用户必踩。
2. **够不到文件**（原因 2）——开了也读不了，架构死锁，确定性失败。
3. **不主动读**（原因 3）——能读也不读，无检索路由，靠自觉。
4. **找不到文件**（原因 4）——想读但发现类工具被审批拦，定位不到路径。
5. **读了用不好**（原因 5）——Full 模式溢出/噪声。

**关键点**：这 5 条是**串联的关卡**，必须从前往后逐个打通。只修后面不修前面没有意义——

- 只加检索路由（修 3）→ 撞原因 2 的墙，照样读失败
- 只开开关（修 1）→ 撞原因 2 + 3 + 4，依然空转
- 完整修复需要：默认启用或引导启用（1）+ 打通 skills 目录读取通路（2，给 skills 目录开受控白名单或做专用 `skill_read` 工具）+ 相关性检索/自动注入（3）+ CLI 审批通道或放宽发现类工具（4）+ 默认 summary 模式（5）。
