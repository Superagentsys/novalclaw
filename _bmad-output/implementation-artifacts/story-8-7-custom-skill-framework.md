---
story_key: 8-7-custom-skill-framework
epic_key: epic-8
epic_name: 开发者工具与API
status: done
priority: high
created_date: 2026-03-25
---

# Story 8.7: Custom Skill Framework CLI

## Implementation Status
- [x] Research & Analysis
- [x] Implementation - all commands (list, install, uninstall, show, validate, package)
- [x] Testing

## User Story

As a 开发者,
I want 通过 CLI 管理自定义 Skills,
So that 我可以创建、安装和管理 Agent 的技能.

## Acceptance Criteria

**Given** CLI 已安装
**When** 我运行 skill 命令
**Then** 支持以下功能:
- 列出已安装的 skills
- 安装 skill 从本地目录或 Git 仓库
- 卸载 skill
- 查看 skill 详情
- 验证 skill 配置
- 打包 skill 用于分发

## Technical Context

### Architecture Notes
- Skills 存储在 `~/.omninova/skills/` 目录
- 每个 skill 是一个包含 `SKILL.md` 的目录
- CLI 需要解析 SKILL.md 的 YAML frontmatter

### Dependencies
- `walkdir` - 目录遍历
- `git2` - Git 操作 (可选)
- `tar` / `flate2` - 打包压缩

### File Structure
```
crates/omninova-cli/src/
├── commands/
│   └── skills.rs         # Skill 管理命令
```

## Tasks / Subtasks

- [ ] 实现 skill list 命令
- [ ] 实现 skill install 命令 (本地目录)
- [ ] 实现 skill install 命令 (Git 仓库)
- [ ] 实现 skill uninstall 命令
- [ ] 实现 skill show 命令
- [ ] 实现 skill validate 命令
- [ ] 实现 skill package 命令
- [ ] 添加单元测试
- [ ] 验证所有 AC 通过

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-03-25: 创建 Story 8-7 文档
