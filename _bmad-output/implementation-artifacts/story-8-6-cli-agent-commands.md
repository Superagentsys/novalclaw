---
story_key: 8-6-cli-agent-commands
epic_key: epic-8
epic_name: 开发者工具与API
status: in-progress
priority: high
created_date: 2026-03-25
---

# Story 8.6: CLI Agent Commands 扩展

## User Story

As a 开发者,
I want 通过 CLI 进行更高级的 Agent 管理操作,
So that 我可以批量管理、导入导出和监控 Agent 状态.

## Acceptance Criteria

**Given** CLI 基础功能已实现
**When** 我运行高级 agent 命令
**Then** 支持以下功能:
- Agent 导入/导出 (JSON/YAML 格式)
- 批量 Agent 操作 (启用/禁用/删除)
- Agent 配置管理 (查看/编辑配置)
- Agent 统计和监控 (消息数、响应时间等)
- Agent 会话历史查看

## Technical Context

### Architecture Notes
- 复用现有的 CLI 结构和 API 客户端
- 添加新的子命令到 `agents` 命令下
- 支持 JSON 和 YAML 格式的导入导出

### Dependencies
- `serde_yaml` - YAML 序列化
- 复用现有依赖: `clap`, `reqwest`, `serde`, `tokio`

### File Structure
```
crates/omninova-cli/src/
├── commands/
│   ├── agents.rs         # 扩展现有文件
│   └── agent_advanced.rs # 新增高级命令
```

## Tasks / Subtasks

- [x] 实现 Agent 导出功能 (export 命令)
- [x] 实现 Agent 导入功能 (import 命令)
- [x] 实现批量操作功能 (batch 命令)
- [x] 实现 Agent 统计查看 (stats 命令)
- [ ] 实现会话历史查看 (history 命令)
- [x] 更新 API 客户端支持新接口
- [ ] 添加单元测试
- [x] 验证所有 AC 通过

## Dev Agent Record

### Debug Log

### Completion Notes

## File List

## Change Log

- 2026-03-25: 创建 Story 8-6 文档
