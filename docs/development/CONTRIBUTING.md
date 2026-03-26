# OmniNova Claw 贡献指南

感谢你有兴趣为 OmniNova Claw 做贡献！本文档将帮助你了解贡献流程和规范。

## 行为准则

请阅读并遵守我们的行为准则：
- 尊重所有贡献者
- 接受建设性批评
- 关注对社区最有利的事情
- 对他人保持同理心

## 如何贡献

### 报告 Bug

在提交 Bug 报告前，请：

1. **搜索现有 Issues** - 确保问题未被报告
2. **使用最新版本** - 确认问题仍然存在
3. **收集信息**：
   - 操作系统和版本
   - Rust 版本 (`rustc --version`)
   - Node.js 版本 (`node --version`)
   - 复现步骤
   - 预期行为 vs 实际行为
   - 相关日志

提交 Issue 时，使用 Bug 报告模板。

### 提出新功能

1. **搜索现有 Issues** - 确认功能未被请求
2. **描述功能**：
   - 问题是什么？
   - 你期望的解决方案是什么？
   - 有哪些替代方案？
3. **讨论** - 在实现前先讨论设计方案

### 提交代码

#### 1. Fork 并克隆仓库

```bash
# Fork 后克隆你的仓库
git clone https://github.com/YOUR_USERNAME/claw.git
cd claw/omninovalclaw

# 添加上游仓库
git remote add upstream https://github.com/omninova/claw.git
```

#### 2. 创建分支

```bash
# 更新主分支
git fetch upstream
git checkout main
git merge upstream/main

# 创建功能分支
git checkout -b feature/my-feature
# 或修复分支
git checkout -b fix/my-fix
```

#### 3. 进行更改

```bash
# 确保能编译
cargo check

# 运行测试
cargo test

# 运行 lint
cargo clippy -- -D warnings
cargo fmt --check
```

#### 4. 提交更改

我们遵循 [Conventional Commits](https://www.conventionalcommits.org/) 规范：

```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

**类型：**
- `feat`: 新功能
- `fix`: Bug 修复
- `docs`: 文档更新
- `style`: 代码格式（不影响功能）
- `refactor`: 重构
- `perf`: 性能优化
- `test`: 测试相关
- `chore`: 构建/工具链相关

**示例：**
```
feat(agent): add support for Claude 3.5 Sonnet

- Add Claude 3.5 model to provider registry
- Update documentation with new model
- Add integration tests

Closes #123
```

#### 5. 推送并创建 PR

```bash
git push origin feature/my-feature
```

然后在 GitHub 上创建 Pull Request。

## 代码规范

### Rust 代码

```rust
// 使用标准 Rust 风格
cargo fmt

// Clippy 检查必须通过
cargo clippy -- -D warnings

// 文档注释
/// 简短描述。
///
/// 详细描述。
///
/// # Examples
///
/// ```
/// let result = my_function();
/// ```
pub fn my_function() -> Result<()> {
    // ...
}
```

**命名规范：**
- 类型: `PascalCase`
- 函数/变量: `snake_case`
- 常量: `SCREAMING_SNAKE_CASE`
- 模块: `snake_case`

**错误处理：**
```rust
// 使用 thiserror 定义错误
#[derive(Debug, thiserror::Error)]
pub enum MyError {
    #[error("Failed to process: {0}")]
    ProcessingFailed(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

### TypeScript/React 代码

```typescript
// 使用 ESLint 和 Prettier
npm run lint
npm run format

// 组件命名: PascalCase
export function MyComponent({ prop }: MyComponentProps) {
  // ...
}

// 函数命名: camelCase
function handleClick() {
  // ...
}

// 常量: SCREAMING_SNAKE_CASE 或 PascalCase
const MAX_RETRIES = 3;
const ApiEndpoints = {
  users: '/api/users',
} as const;
```

### 文档

- 使用 Markdown 格式
- 保持简洁明了
- 包含代码示例
- 更新相关文档

## 测试规范

### 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_function() {
        let result = my_function();
        assert!(result.is_ok());
    }

    #[test]
    fn test_edge_case() {
        let result = my_function_with_input("");
        assert_eq!(result, Err(MyError::EmptyInput));
    }
}
```

### 集成测试

放在 `tests/` 目录：

```rust
// tests/integration_test.rs
use omninova_core::Agent;

#[tokio::test]
async fn test_agent_creation() {
    let agent = Agent::new("test-agent").await;
    assert!(agent.is_ok());
}
```

### 运行测试

```bash
# 所有测试
cargo test

# 特定测试
cargo test test_my_function

# 特定 crate
cargo test -p omninova-core

# 带输出
cargo test -- --nocapture
```

## PR 审核流程

### 审核标准

1. **代码质量** - 遵循代码规范
2. **测试覆盖** - 新功能需要测试
3. **文档完整** - 更新相关文档
4. **性能影响** - 无性能回退
5. **向后兼容** - 不破坏现有功能

### 审核流程

1. 自动 CI 检查必须通过
2. 至少一名审核者批准
3. 解决所有评论
4. Squash 合并（如适用）

### CI 检查

每次 PR 会运行以下检查：
- `cargo fmt --check` - 格式检查
- `cargo clippy -- -D warnings` - Lint 检查
- `cargo test` - 测试
- `cargo build --release` - Release 构建
- `npm run lint` - 前端 Lint

## 发布流程

### 版本号

遵循 [语义化版本](https://semver.org/)：
- `MAJOR.MINOR.PATCH`
- `MAJOR`: 不兼容的 API 更改
- `MINOR`: 向后兼容的新功能
- `PATCH`: 向后兼容的 Bug 修复

### 发布步骤

1. 更新 `Cargo.toml` 和 `package.json` 版本号
2. 更新 `CHANGELOG.md`
3. 创建 Git Tag: `git tag v0.1.0`
4. 推送 Tag: `git push origin v0.1.0`
5. GitHub Actions 自动构建发布

## 开发技巧

### 保持同步

```bash
# 定期同步上游
git fetch upstream
git checkout main
git merge upstream/main
```

### 清理分支

```bash
# 删除已合并的本地分支
git branch -d feature/my-feature

# 删除远程分支
git push origin --delete feature/my-feature
```

### 调试 CI

本地运行 CI 步骤：
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
cargo build --release
```

## 获取帮助

- **GitHub Issues** - 提问或报告问题
- **Discord** - 实时讨论
- **文档** - 查看现有文档

## 许可证

通过贡献代码，你同意你的代码将在 MIT 许可证下发布。

---

感谢你对 OmniNova Claw 的贡献！ 🎉