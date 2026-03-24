# Story 2.10: AI 代理删除功能

Status: done

## Story

As a 用户,
I want 删除不再需要的 AI 代理,
so that 我可以清理代理列表保持整洁.

## Acceptance Criteria

1. **Given** 已存在的 AI 代理, **When** 我点击删除按钮, **Then** 显示确认对话框警告删除不可恢复

2. **Given** 确认对话框已显示, **When** 我确认删除, **Then** 代理记录从数据库中删除

3. **Given** 删除操作完成, **When** 我查看代理列表, **Then** 已删除的代理从列表中移除

4. **Given** 代理被删除, **When** 相关数据（会话历史引用）存在, **Then** 相关数据被适当处理或归档

5. **Given** 删除操作失败, **When** 错误发生, **Then** 显示错误通知，代理不受影响

## Tasks / Subtasks

- [x] Task 1: 验证后端 delete_agent 支持 (AC: 2, 3)
  - [x] 确认 `AgentStore::delete` 方法已存在
  - [x] 确认 `delete_agent` Tauri 命令已暴露
  - [x] 添加测试验证删除功能（后端测试已存在于 store.rs）

- [x] Task 2: 添加 AlertDialog 组件 (AC: 1)
  - [x] 使用 Shadcn/UI AlertDialog 组件
  - [x] 配置确认对话框样式和文案
  - [x] 添加删除警告文案："此操作不可撤销，该代理将被永久删除。"
  - [x] 确认按钮使用 destructive 样式

- [x] Task 3: 更新 AgentCard 组件添加删除按钮 (AC: 1)
  - [x] 添加 `showDeleteButton` prop（可选，默认 false）
  - [x] 添加 `onDelete` 回调 prop
  - [x] 使用 `Trash2` 图标（lucide-react）
  - [x] 删除按钮点击时阻止事件冒泡
  - [x] 删除按钮使用红色/destructive 样式
  - [x] 添加 `aria-label` 可访问性支持

- [x] Task 4: 实现删除功能业务逻辑 (AC: 1, 2, 3, 5)
  - [x] 创建 `handleDeleteAgent` 和 `handleConfirmDelete` 函数
  - [x] 调用 `delete_agent` Tauri 命令
  - [x] 显示确认对话框
  - [x] 确认后执行删除
  - [x] 成功后刷新代理列表
  - [x] 显示成功/失败通知

- [x] Task 5: 更新 AgentList 集成删除功能 (AC: 3)
  - [x] 在 AgentList 中传递 `showDeleteButton` 和 `onDelete` props
  - [x] 实现删除后的列表刷新逻辑（通过 loadAgents 回调）

- [x] Task 6: 添加单元测试 (AC: All)
  - [x] 测试 `delete` Rust 函数（已存在于 store.rs:428-454）
  - [x] 测试 AgentCard 删除按钮渲染
  - [x] 测试删除按钮点击调用 onDelete 回调
  - [x] 测试删除按钮不触发卡片 onClick
  - [x] 测试删除按钮 destructive 样式
  - [x] 测试所有按钮同时显示
  - [x] 共新增 7 个测试用例

- [x] Task 7: 文档和导出 (AC: All)
  - [x] 组件 Props 已有 JSDoc 注释
  - [x] 运行 `npm run lint` - 修改的文件无错误
  - [x] 运行 `cargo clippy` - 无新增警告

## Dev Notes

### 前置依赖（已完成）

**Story 2-1 Agent 数据模型：**
- `AgentModel` 结构体已定义
- `AgentStore` 已实现 `delete` 方法
- Tauri 命令已暴露：`delete_agent(uuid: String) -> ()`

**Story 2-6 AgentCard 组件：**
- AgentCard 已有状态指示器
- AgentStatusBadge 组件已实现

**Story 2-8 Agent Duplicate：**
- AgentCard 已有编辑按钮和复制按钮的模式
- 事件冒泡处理模式已建立
- 测试模式和 Mock 配置已完善

### 现有后端实现

**AgentStore::delete 方法（已存在）：**
```rust
/// Delete an agent by UUID
pub fn delete(&self, uuid: &str) -> Result<(), AgentStoreError> {
    let conn = self.get_conn()?;
    let rows_affected = conn.execute("DELETE FROM agents WHERE agent_uuid = ?1", params![uuid])?;

    if rows_affected == 0 {
        return Err(AgentStoreError::NotFound(uuid.to_string()));
    }

    Ok(())
}
```

**delete_agent Tauri 命令（已存在）：**
```rust
/// Delete an agent
#[tauri::command]
async fn delete_agent(
    uuid: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let app_state = state.lock().await;
    // ... implementation
}
```

**前端调用方式：**
```typescript
import { invoke } from '@tauri-apps/api/core';

const deleteAgent = async (uuid: string): Promise<void> => {
  await invoke('delete_agent', { uuid });
};
```

### 数据类型定义

**AgentModel 接口（已存在）：**
```typescript
interface AgentModel {
  id: number;
  agent_uuid: string;
  name: string;
  description?: string;
  domain?: string;
  mbti_type?: MBTIType;
  status: 'active' | 'inactive' | 'archived';
  system_prompt?: string;
  created_at: number;
  updated_at: number;
}
```

### 组件设计规范

**AgentCard 删除按钮位置（与编辑、复制按钮并列）：**
```
┌─────────────────────────────────────────────────────────┐
│ ┌──┐  代理名称      [删除] [复制] [编辑] [状态徽章]    │
│ │代│  描述文本（截断）                                  │
│ │理│                                                    │
│ │图│  [INTJ] 人格类型徽章                               │
│ └──┘  专业领域（可选）                                  │
└─────────────────────────────────────────────────────────┘
```

**确认对话框设计：**
```
┌─────────────────────────────────────────────┐
│  ⚠️ 确认删除                                 │
├─────────────────────────────────────────────┤
│                                             │
│  确定要删除代理 "代理名称" 吗？              │
│                                             │
│  此操作不可撤销，该代理将被永久删除。        │
│                                             │
│         [取消]    [删除] (红色)              │
└─────────────────────────────────────────────┘
```

**删除按钮样式：**
- 图标：`Trash2`（lucide-react）
- 颜色：`text-destructive` 或 hover 时变红
- 点击时阻止事件冒泡

### 项目架构约束

- **组件位置**: `apps/omninova-tauri/src/components/agent/`
- **样式系统**: Tailwind CSS + Shadcn/UI
- **状态管理**: React useState（组件内部状态）
- **命名约定**:
  - 组件文件: PascalCase (`AgentCard.tsx`)
  - Props 接口: 组件名 + Props (`AgentCardProps`)
  - Tauri 命令: snake_case (`delete_agent`)

### AlertDialog 组件安装

如果 AlertDialog 组件尚未安装，需要添加：
```bash
cd apps/omninova-tauri
npx shadcn@latest add alert-dialog
```

### 可访问性要求

- 删除按钮有明确的 aria-label（如 `aria-label="删除代理: ${agent.name}"`）
- 确认对话框支持键盘操作（Escape 取消，Enter 确认）
- 焦点状态清晰可见
- 删除按钮有 destructive 样式提示危险操作

### 测试策略

**单元测试（Vitest）：**
- 组件渲染测试
- 点击交互测试
- 事件冒泡阻止测试
- 确认对话框测试
- Mock Tauri API 调用

**测试文件位置：**
- `apps/omninova-tauri/src/test/components/AgentCard.test.tsx`（扩展现有测试）

**Mock 模式：**
```typescript
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockInvoke = vi.mocked(invoke);

// Mock delete_agent
mockInvoke.mockResolvedValueOnce(undefined);
```

### 依赖项

无需添加新的依赖项，现有依赖已足够：
- React 19
- Tailwind CSS
- Shadcn/UI 组件（AlertDialog, Button）
- @tauri-apps/api
- react-router-dom
- sonner（toast 通知）
- lucide-react（Trash2 图标）

### 注意事项

1. **删除按钮与其他按钮的关系**：
   - 四个按钮并列显示在 AgentCard 右上角：删除、切换、复制、编辑
   - 点击删除后显示确认对话框
   - 确认后才执行删除操作

2. **事件冒泡处理**：
   - 删除按钮点击时必须调用 `e.stopPropagation()`
   - 避免触发卡片的 onClick 事件

3. **确认对话框**：
   - 必须显示警告信息说明删除不可恢复
   - 确认按钮使用 destructive 样式（红色）
   - 取消按钮使用默认样式

4. **用户体验**：
   - 删除操作应该有确认步骤防止误操作
   - 显示加载状态
   - 成功后显示简短通知并刷新列表
   - 失败时显示错误通知

5. **数据完整性**：
   - 当前后端 `delete` 方法直接删除记录
   - 后续可考虑级联删除或归档相关数据
   - MVP 阶段简化处理，仅删除代理记录

### References

- [Source: epics.md#Story 2.10] - 验收标准
- [Source: architecture.md#前端架构] - 组件位置规范
- [Source: architecture.md#Tauri Commands API] - 后端命令设计
- [Source: ux-design-specification.md#核心组件] - AgentCard 组件设计
- [Source: 2-1-agent-data-model.md] - AgentModel 类型定义
- [Source: 2-6-agent-list-card.md] - AgentCard 基础组件
- [Source: 2-8-agent-duplicate.md] - 按钮添加模式和测试模式
- [Source: crates/omninova-core/src/agent/store.rs] - AgentStore::delete 方法

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

None

### Completion Notes List

1. ✅ 后端 delete_agent 支持已验证存在（store.rs:234-243, lib.rs:800-810）
2. ✅ AlertDialog 组件已通过 `npx shadcn@latest add alert-dialog` 安装
3. ✅ AgentCard 组件已添加 `showDeleteButton` 和 `onDelete` props
4. ✅ AgentList 组件已集成删除 props 传递
5. ✅ AgentListPage 已实现删除业务逻辑：
   - `agentToDelete` state 跟踪待删除代理
   - `isDeleting` state 防止重复提交
   - `handleDeleteAgent` 打开确认对话框
   - `handleConfirmDelete` 执行删除操作
   - AlertDialog 组件显示确认对话框
6. ✅ 新增 7 个删除按钮测试用例，总计 210 个测试全部通过
7. ✅ 修改的文件无 lint 错误

### File List

**新增文件：**
- `apps/omninova-tauri/src/components/ui/alert-dialog.tsx` - Shadcn/UI AlertDialog 组件

**修改文件：**
- `apps/omninova-tauri/src/components/agent/AgentCard.tsx` - 添加删除按钮和 props
- `apps/omninova-tauri/src/components/agent/AgentList.tsx` - 传递删除 props
- `apps/omninova-tauri/src/pages/AgentListPage.tsx` - 实现删除业务逻辑和确认对话框
- `apps/omninova-tauri/src/test/components/AgentCard.test.tsx` - 新增 7 个删除按钮测试
- `_bmad-output/implementation-artifacts/sprint-status.yaml` - 更新状态为 in-progress