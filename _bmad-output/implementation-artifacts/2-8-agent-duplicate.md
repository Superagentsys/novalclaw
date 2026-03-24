# Story 2.8: AI 代理复制功能

Status: done

## Story

As a 用户,
I want 复制现有的 AI 代理配置,
so that 我可以基于已有代理快速创建类似的新代理.

## Acceptance Criteria

1. **Given** 已存在的 AI 代理, **When** 我点击代理的复制按钮, **Then** 创建一个新代理副本，自动生成新的 UUID

2. **Given** 复制操作已完成, **When** 我查看新代理, **Then** 副本名称为 "原名称 (副本)"

3. **Given** 复制操作已完成, **When** 我查看新代理配置, **Then** 所有配置（人格类型、描述、领域、系统提示词）被复制

4. **Given** 复制操作已完成, **When** 系统完成创建, **Then** 自动打开编辑页面允许修改副本

5. **Given** 复制操作失败, **When** 错误发生, **Then** 显示错误通知，原有代理不受影响

## Tasks / Subtasks

- [x] Task 1: 添加 duplicate_agent Tauri 命令 (AC: 1, 2, 3)
  - [x] 在 `crates/omninova-core/src/agent/store.rs` 添加 `duplicate` 方法
  - [x] 生成新的 UUID
  - [x] 复制所有配置字段（name, description, domain, mbti_type, system_prompt）
  - [x] 修改名称为 "原名称 (副本)"
  - [x] 设置 status 为 'active'
  - [x] 设置新的 created_at 和 updated_at
  - [x] 在 `apps/omninova-tauri/src-tauri/src/lib.rs` 暴露 Tauri 命令

- [x] Task 2: 更新 AgentCard 组件添加复制按钮 (AC: 1)
  - [x] 添加 `showDuplicateButton` prop（可选，默认 false）
  - [x] 添加 `onDuplicate` 回调 prop
  - [x] 使用 `Copy` 图标（lucide-react）
  - [x] 复制按钮点击时阻止事件冒泡
  - [x] 复制按钮使用人格主题色
  - [x] 添加 `aria-label` 可访问性支持

- [x] Task 3: 实现复制功能业务逻辑 (AC: 1, 2, 3, 4, 5)
  - [x] 创建 `duplicateAgent` 前端 API 函数
  - [x] 调用 `duplicate_agent` Tauri 命令
  - [x] 显示加载状态
  - [x] 成功后导航到编辑页面
  - [x] 显示成功/失败通知

- [x] Task 4: 更新 AgentList 集成复制功能 (AC: 1)
  - [x] 在 AgentList 中传递 `showDuplicateButton` 和 `onDuplicate` props
  - [x] 实现复制后的导航逻辑

- [x] Task 5: 添加单元测试 (AC: All)
  - [x] 测试 `duplicate_agent` Rust 函数
  - [x] 测试 AgentCard 复制按钮渲染
  - [x] 测试复制按钮点击调用 onDuplicate 回调
  - [x] 测试复制按钮不触发卡片 onClick
  - [x] 测试复制成功后导航到编辑页面
  - [x] 测试复制失败显示错误通知
  - [x] Mock Tauri API 调用

- [x] Task 6: 文档和导出 (AC: All)
  - [x] 添加组件 JSDoc 注释
  - [x] 运行 `npm run lint` 确保无警告
  - [x] 运行 `cargo clippy` 确保无警告

## Dev Notes

### 前置依赖（已完成）

**Story 2-1 Agent 数据模型：**
- `AgentModel` 结构体已定义
- `AgentStore` 已实现 CRUD 操作
- Tauri 命令已暴露：
  - `get_agents() -> Vec<AgentModel>`
  - `get_agent_by_id(uuid: String) -> Option<AgentModel>`
  - `create_agent(config_json: String) -> AgentModel`
  - `update_agent(uuid: String, updates_json: String) -> AgentModel`
  - `delete_agent(uuid: String) -> ()`

**Story 2-7 AgentEditPage：**
- AgentEditForm 组件已实现
- AgentCard 已有编辑按钮功能
- 编辑页面路由 `/agents/:uuid/edit` 已配置
- 路由导航模式已建立

### Tauri 命令调用

**复制代理：**
```typescript
import { invoke } from '@tauri-apps/api/core';

const duplicateAgent = async (uuid: string): Promise<AgentModel> => {
  const jsonStr = await invoke<string>('duplicate_agent', { uuid });
  return JSON.parse(jsonStr);
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

**AgentCard 复制按钮位置（与编辑按钮并列）：**
```
┌─────────────────────────────────────────┐
│ ┌──┐  代理名称      [复制] [编辑] [状态]│
│ │代│  描述文本（截断）                   │
│ │理│                                      │
│ │图│  [INTJ] 人格类型徽章                 │
│ └──┘  专业领域（可选）                    │
└─────────────────────────────────────────┘
```

### 项目架构约束

- **组件位置**: `apps/omninova-tauri/src/components/agent/`
- **样式系统**: Tailwind CSS + Shadcn/UI
- **状态管理**: React useState（组件内部状态）
- **命名约定**:
  - 组件文件: PascalCase (`AgentCard.tsx`)
  - Props 接口: 组件名 + Props (`AgentCardProps`)
  - Tauri 命令: snake_case (`duplicate_agent`)

### Rust 后端实现

**duplicate_agent 方法签名：**
```rust
impl AgentStore {
    /// 复制代理，创建一个具有新 UUID 的副本
    ///
    /// # Arguments
    /// * `uuid` - 原代理的 UUID
    ///
    /// # Returns
    /// * `Result<AgentModel>` - 新创建的代理副本
    pub fn duplicate_agent(&self, uuid: &str) -> Result<AgentModel> {
        // 1. 获取原代理
        let original = self.get_by_uuid(uuid)?;

        // 2. 创建新代理，复制所有配置
        let mut duplicate = original.clone();
        duplicate.agent_uuid = Uuid::new_v4().to_string();
        duplicate.name = format!("{} (副本)", original.name);
        duplicate.status = AgentStatus::Active;
        duplicate.created_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
        duplicate.updated_at = duplicate.created_at;
        duplicate.id = 0; // 让数据库自动生成新 ID

        // 3. 插入数据库
        self.insert(&duplicate)
    }
}
```

### 可访问性要求

- 复制按钮有明确的 aria-label（如 `aria-label="复制代理: ${agent.name}"`）
- 键盘导航支持（Tab 遍历，Enter/Space 触发）
- 焦点状态清晰可见

### 测试策略

**单元测试（Vitest）：**
- 组件渲染测试
- 点击交互测试
- 事件冒泡阻止测试
- Mock Tauri API 调用

**测试文件位置：**
- `apps/omninova-tauri/src/test/components/AgentCard.test.tsx`（扩展现有测试）

**Mock 模式：**
```typescript
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockInvoke = vi.mocked(invoke);

// Mock duplicate_agent
mockInvoke.mockResolvedValueOnce(JSON.stringify(duplicatedAgent));
```

### 依赖项

无需添加新的依赖项，现有依赖已足够：
- React 19
- Tailwind CSS
- Shadcn/UI 组件（Button）
- @tauri-apps/api
- react-router-dom
- sonner（toast 通知）
- lucide-react（Copy 图标）

### 注意事项

1. **复制按钮与编辑按钮的关系**：
   - 两个按钮并列显示在 AgentCard 右上角
   - 点击复制后直接创建副本并导航到编辑页面
   - 用户可以在编辑页面进一步修改副本

2. **事件冒泡处理**：
   - 复制按钮点击时必须调用 `e.stopPropagation()`
   - 避免触发卡片的 onClick 事件

3. **名称处理**：
   - 副本名称格式固定为 "原名称 (副本)"
   - 如果原名称已经很长，考虑截断但保留 "(副本)" 后缀

4. **状态处理**：
   - 副本的 status 总是设置为 'active'
   - 不继承原代理的 'inactive' 或 'archived' 状态

5. **用户体验**：
   - 复制操作应该快速完成
   - 显示加载指示器
   - 成功后自动导航，无需用户额外操作

### References

- [Source: epics.md#Story 2.8] - 验收标准
- [Source: architecture.md#前端架构] - 组件位置规范
- [Source: architecture.md#Tauri Commands API] - 后端命令设计
- [Source: ux-design-specification.md#核心组件] - AgentCard 组件设计
- [Source: 2-1-agent-data-model.md] - AgentModel 类型定义
- [Source: 2-7-agent-edit.md] - AgentEditPage 路由和导航模式
- [Source: crates/omninova-core/src/agent/store.rs] - AgentStore 现有方法

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6

### Debug Log References

None

### Completion Notes List

1. 所有任务已完成，所有测试通过
2. Rust 后端 `duplicate` 方法实现了完整的复制逻辑，包括新 UUID 生成、名称格式化、状态重置
3. 前端 AgentCard 组件添加了复制按钮，支持事件冒泡阻止和可访问性
4. AgentListPage 实现了完整的复制业务逻辑，包括成功/失败通知和自动导航
5. 测试覆盖全面：Rust 5 个测试用例，前端 8 个新测试用例
6. **Code Review 修复 (2026-03-16):**
   - [MEDIUM] 修复复制成功后未刷新代理列表的问题，现在用户使用浏览器返回按钮能看到新代理
   - [LOW] 测试增加 `toast.success` 验证，确保成功通知被正确调用

### File List

**Rust Backend:**
- `crates/omninova-core/src/agent/store.rs` - 添加 `duplicate` 方法和 5 个测试

**Tauri Commands:**
- `apps/omninova-tauri/src-tauri/src/lib.rs` - 添加 `duplicate_agent` 命令

**React Components:**
- `apps/omninova-tauri/src/components/agent/AgentCard.tsx` - 添加复制按钮 UI 和 props
- `apps/omninova-tauri/src/components/agent/AgentList.tsx` - 添加复制功能 props 传递

**Pages:**
- `apps/omninova-tauri/src/pages/AgentListPage.tsx` - 添加 `handleDuplicateAgent` 业务逻辑

**Tests:**
- `apps/omninova-tauri/src/test/components/AgentCard.test.tsx` - 添加复制按钮测试
- `apps/omninova-tauri/src/test/pages/AgentListPage.test.tsx` - 添加复制功能集成测试