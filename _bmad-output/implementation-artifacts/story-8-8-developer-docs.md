---
story_key: 8-8-developer-docs
epic_key: epic-8
epic_name: 开发者工具与API
status: in-progress
priority: high
created_date: 2026-03-25
---

# Story 8.8: Developer Documentation

## User Story

As a 开发者,
I want 有完整的开发者文档,
So that 我可以快速理解和使用 OmniNova API 和 CLI.

## Acceptance Criteria

**Given** 项目文档目录
**When** 我查看文档
**Then** 包含以下内容:
- API 参考文档 (RESTful API)
- CLI 使用指南
- 架构概述
- 开发环境设置指南
- 贡献指南

## Technical Context

### Architecture Notes
- 使用 Markdown 格式
- 存放在 `docs/` 目录
- 可选：集成 `rustdoc` 自动生成 API 文档

### File Structure
```
docs/
├── README.md              # 文档首页
├── api/
│   └── REST.md           # REST API 文档
├── cli/
│   └── GUIDE.md          # CLI 使用指南
├── architecture/
│   └── OVERVIEW.md       # 架构概述
└── development/
    ├── SETUP.md          # 开发环境设置
    └── CONTRIBUTING.md   # 贡献指南
```

## Tasks / Subtasks

- [ ] 创建 docs 目录结构
- [ ] 编写 API 参考文档
- [ ] 编写 CLI 使用指南
- [ ] 编写架构概述
- [ ] 编写开发环境设置指南
- [ ] 编写贡献指南
- [ ] 更新项目根 README

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-03-25: 创建 Story 8-8 文档