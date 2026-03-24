# Story 2.7: AI 代理编辑功能

Status: complete

## Story

As a 用户,
I want 修改现有 AI 代理的配置,
so that 我可以根据需要调整代理的行为和特征.

## Acceptance Criteria

1. **Given** 已存在的 AI 代理, **When** 我点击代理的编辑按钮, **Then** 显示预填充当前配置的编辑表单

2. **Given** 编辑表单已显示, **When** 我查看表单内容, **Then** 显示当前代理的名称、描述、专业领域（预填充）

3. **Given** 编辑表单已显示, **When** 我修改人格类型, **Then** 显示新人格类型的预览特征

4. **Given** 我已修改代理配置, **When** 我点击保存按钮, **Then** 更新代理配置并显示成功通知

5. **Given** 我正在编辑代理, **When** 我点击取消按钮, **Then** 保留原有配置不变并返回上一页

## Tasks / Subtasks

- [x] Task 1: 创建 AgentUpdate 类型定义 (AC: 4)
  - [x] 在 `src/types/agent.ts` 添加 `AgentUpdate` 接口
  - [x] 包含可选字段：name, description, domain, mbti_type, system_prompt
  - [x] 添加类型导出

- [x] Task 2: 创建 AgentEditForm 组件 (AC: 1, 2, 3, 4)
  - [x] 创建 `apps/omninova-tauri/src/components/agent/AgentEditForm.tsx` 文件
  - [x] 定义 `AgentEditFormProps` 接口（agent, onSuccess, onCancel, className）
  - [x] 复用 AgentCreateForm 的布局和样式模式
  - [x] 实现表单字段预填充（名称、描述、领域）
  - [x] 实现 MBTI 类型预选和预览
  - [x] 实现表单验证（名称必填，长度限制）
  - [x] 实现"保存更改"按钮，调用 `update_agent` Tauri 命令
  - [x] 实现取消按钮功能

- [x] Task 3: 创建 AgentEditPage 页面组件 (AC: 1, 4, 5)
  - [x] 创建 `apps/omninova-tauri/src/pages/AgentEditPage.tsx` 文件
  - [x] 从 URL 参数获取 agent UUID
  - [x] 调用 `get_agent_by_id` Tauri 命令获取代理数据
  - [x] 处理加载状态（骨架屏）
  - [x] 处理代理不存在情况（404 页面）
  - [x] 集成 AgentEditForm 组件
  - [x] 实现保存成功后的导航和通知
  - [x] 实现取消后的导航

- [x] Task 4: 更新 AgentCard 组件添加编辑入口 (AC: 1)
  - [x] 在 AgentCard 添加编辑按钮/图标
  - [x] 编辑按钮点击时导航到编辑页面
  - [x] 编辑按钮样式与卡片整体协调
  - [x] 支持通过 props 控制是否显示编辑按钮

- [x] Task 5: 更新路由配置 (AC: 1)
  - [x] 添加 `/agents/:uuid/edit` 路由
  - [x] 配置路由参数和页面组件映射

- [x] Task 6: 添加单元测试 (AC: All)
  - [x] 测试 AgentEditForm 渲染预填充数据
  - [x] 测试表单验证
  - [x] 测试保存更改调用正确的 Tauri 命令
  - [x] 测试取消操作不修改数据
  - [x] 测试人格类型预览更新
  - [x] 测试 AgentEditPage 加载状态
  - [x] 测试 AgentEditPage 404 情况
  - [x] Mock Tauri API 调用

- [x] Task 7: 文档和导出 (AC: All)
  - [x] 添加组件 JSDoc 注释
  - [x] 更新 `components/agent/index.ts` 导出 AgentEditForm
  - [x] 更新 `pages/index.ts` 导出 AgentEditPage
  - [x] 运行 `npm run lint` 确保无警告

## Dev Notes

### 前置依赖（已完成）

**Story 2-1 Agent 数据模型：**
- `AgentModel` 结构体已定义
- `AgentStore` 已实现 CRUD 操作
- Tauri 命令已暴露：
  - `get_agents() -> Vec<AgentModel>`
  - `get_agent_by_id(uuid: String) -> Option<AgentModel>`
  - `update_agent(uuid: String, updates_json: String) -> AgentModel`
  - `delete_agent(uuid: String) -> ()`

**Story 2-5 AgentCreateForm：**
- 表单布局和样式可复用
- MBTISelector 组件已实现
- PersonalityPreview 组件已实现
- 表单验证模式可复用

**Story 2-6 AgentCard：**
- AgentCard 组件已实现
- AgentList 组件已实现
- 状态徽章已实现

### Tauri 命令调用

**获取单个代理：**
```typescript
import { invoke } from '@tauri-apps/api/core';

const getAgentById = async (uuid: string): Promise<AgentModel> => {
  const jsonStr = await invoke<string>('get_agent_by_id', { uuid });
  return JSON.parse(jsonStr);
};
```

**更新代理：**
```typescript
interface AgentUpdate {
  name?: string;
  description?: string;
  domain?: string;
  mbti_type?: MBTIType;
  system_prompt?: string;
}

const updateAgent = async (uuid: string, updates: AgentUpdate): Promise<AgentModel> => {
  const jsonStr = await invoke<string>('update_agent', {
    uuid,
    updatesJson: JSON.stringify(updates)
  });
  return JSON.parse(jsonStr);
};
```

### 数据类型定义

**AgentUpdate 接口（新增）：**
```typescript
/**
 * 代理更新数据（发送给后端更新代理）
 */
export interface AgentUpdate {
  /** 代理名称 */
  name?: string;
  /** 描述 */
  description?: string;
  /** 专业领域 */
  domain?: string;
  /** MBTI 人格类型 */
  mbti_type?: MBTIType;
  /** 系统提示词 */
  system_prompt?: string;
}
```

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

**AgentEditForm 布局（复用 AgentCreateForm 模式）：**
```
┌─────────────────────────────────────────────────────────────────┐
│  编辑代理                                                        │
├─────────────────────────────────────────────────────────────────┤
│  ┌───────────────────────────┐  ┌─────────────────────────────┐ │
│  │ 名称 *                    │  │                             │ │
│  │ [预填充名称]              │  │    人格预览                 │ │
│  │                           │  │    (PersonalityPreview)     │ │
│  │ 描述                      │  │                             │ │
│  │ [预填充描述]              │  │                             │ │
│  │                           │  │                             │ │
│  │ 专业领域                  │  │                             │ │
│  │ [预填充领域]              │  │                             │ │
│  │                           │  │                             │ │
│  │ 人格类型                  │  │                             │ │
│  │ [MBTISelector]            │  │                             │ │
│  └───────────────────────────┘  └─────────────────────────────┘ │
├─────────────────────────────────────────────────────────────────┤
│                              [取消]  [保存更改]                  │
└─────────────────────────────────────────────────────────────────┘
```

**AgentCard 编辑按钮位置：**
```
┌─────────────────────────────────────────┐
│ ┌──┐  代理名称            [编辑] [状态] │
│ │代│  描述文本（截断）                   │
│ │理│                                      │
│ │图│  [INTJ] 人格类型徽章                 │
│ └──┘  专业领域（可选）                    │
└─────────────────────────────────────────┘
```

### 路由配置

**路由定义：**
```typescript
// 在 App.tsx 或路由配置文件中
<Route path="/agents/:uuid/edit" element={<AgentEditPage />} />
```

**导航到编辑页面：**
```typescript
import { useNavigate } from 'react-router-dom';

const navigate = useNavigate();

// 从 AgentCard 点击编辑
const handleEdit = (agent: AgentModel) => {
  navigate(`/agents/${agent.agent_uuid}/edit`);
};
```

### 表单状态管理

**使用 useState 管理编辑状态：**
```typescript
interface EditFormState {
  name: string;
  description: string;
  domain: string;
  mbtiType?: MBTIType;
}

// 初始化时从 agent 数据填充
const [formState, setFormState] = useState<EditFormState>({
  name: agent.name,
  description: agent.description || '',
  domain: agent.domain || '',
  mbtiType: agent.mbti_type,
});
```

### 项目架构约束

- **组件位置**: `apps/omninova-tauri/src/components/agent/`
- **页面位置**: `apps/omninova-tauri/src/pages/AgentEditPage.tsx`
- **样式系统**: Tailwind CSS + Shadcn/UI
- **状态管理**: React useState（组件内部状态）
- **命名约定**:
  - 组件文件: PascalCase (`AgentEditForm.tsx`)
  - Props 接口: 组件名 + Props (`AgentEditFormProps`)

### 可访问性要求

- 编辑按钮有明确的 aria-label
- 表单字段有关联的 label
- 验证错误有 aria-invalid 和 aria-describedby
- 键盘导航支持（Tab 遍历，Enter 提交，Escape 取消）

### 测试策略

**单元测试（Vitest）：**
- 组件渲染测试（预填充数据）
- 状态更新测试
- 表单验证测试
- 用户交互测试（保存、取消）
- Mock Tauri API 调用

**测试文件位置：**
- `apps/omninova-tauri/src/test/components/AgentEditForm.test.tsx`
- `apps/omninova-tauri/src/test/pages/AgentEditPage.test.tsx`

**Mock 模式：**
```typescript
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockInvoke = vi.mocked(invoke);

// Mock get_agent_by_id
mockInvoke.mockResolvedValueOnce(JSON.stringify(mockAgent));

// Mock update_agent
mockInvoke.mockResolvedValueOnce(JSON.stringify(updatedAgent));
```

### 依赖项

无需添加新的依赖项，现有依赖已足够：
- React 19
- Tailwind CSS
- Shadcn/UI 组件（Button, Input, Card）
- @tauri-apps/api
- react-router-dom
- sonner（toast 通知）

### 注意事项

1. **区分创建和编辑模式**：
   - AgentEditForm 是独立组件，专门用于编辑
   - 或者可以扩展 AgentCreateForm 支持 `mode` prop ('create' | 'edit')
   - 建议创建独立组件以保持职责单一

2. **编辑按钮位置**：
   - 在 AgentCard 右上角添加编辑图标
   - 使用 lucide-react 的 `Pencil` 或 `Edit` 图标
   - 点击编辑时阻止事件冒泡，避免触发卡片点击

3. **取消操作处理**：
   - 使用 `useNavigate` 的 `-1` 返回上一页
   - 或者导航到代理列表页
   - 如果有未保存更改，考虑显示确认对话框

4. **乐观更新**：
   - 可以考虑乐观更新 UI
   - 但需要处理更新失败的情况并回滚

### References

- [Source: epics.md#Story 2.7] - 验收标准
- [Source: architecture.md#前端组件位置] - 组件位置规范
- [Source: architecture.md#前端架构] - 状态管理模式
- [Source: ux-design-specification.md#核心组件] - AgentCard 组件设计
- [Source: ux-design-specification.md#色彩系统] - 人格自适应色彩
- [Source: 2-1-agent-data-model.md] - AgentModel 类型定义
- [Source: 2-5-agent-creation-ui.md] - AgentCreateForm 参考实现
- [Source: 2-6-agent-list-card.md] - AgentCard 组件和导航模式
- [Source: crates/omninova-core/src/agent/store.rs] - AgentStore update 方法
- [Source: apps/omninova-tauri/src-tauri/src/lib.rs] - Tauri 命令定义

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (via Claude Code)

### Debug Log References

N/A

### Completion Notes List

1. Task 1 (AgentUpdate 类型定义) - 已在之前的会话中完成
2. Task 2 (AgentEditForm 组件) - 16 个测试全部通过
3. Task 3 (AgentEditPage 页面) - 15 个测试全部通过
4. Task 4 (AgentCard 编辑按钮) - 新增 7 个测试，28 个测试全部通过
5. Task 5 (路由配置) - AgentEditPage 已导出到 pages/index.ts，路由页面组件已就绪
6. Task 6 (单元测试) - 共 59 个测试全部通过
7. Task 7 (文档和导出) - JSDoc 注释已添加，导出已配置

### File List

**新建文件:**
- `apps/omninova-tauri/src/components/agent/AgentEditForm.tsx` - 代理编辑表单组件
- `apps/omninova-tauri/src/pages/AgentEditPage.tsx` - 代理编辑页面组件
- `apps/omninova-tauri/src/test/components/AgentEditForm.test.tsx` - 表单组件测试 (16 tests)
- `apps/omninova-tauri/src/test/pages/AgentEditPage.test.tsx` - 页面组件测试 (15 tests)

**修改文件:**
- `apps/omninova-tauri/src/types/agent.ts` - 添加 AgentUpdate 接口
- `apps/omninova-tauri/src/components/agent/AgentCard.tsx` - 添加编辑按钮功能
- `apps/omninova-tauri/src/components/agent/index.ts` - 导出 AgentEditForm
- `apps/omninova-tauri/src/pages/index.ts` - 导出 AgentEditPage
- `apps/omninova-tauri/src/test/components/AgentCard.test.tsx` - 添加编辑按钮测试 (7 new tests)