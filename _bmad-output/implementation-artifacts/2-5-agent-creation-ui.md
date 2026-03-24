# Story 2.5: AI 代理创建界面

Status: ready-for-review

## Story

As a 用户,
I want 通过图形界面创建新的 AI 代理,
so that 我可以轻松配置代理的基本信息和人格类型.

## Acceptance Criteria

1. **Given** Agent 数据模型和 UI 组件已准备, **When** 我访问创建代理页面, **Then** 显示包含名称、描述、专业领域输入框的表单

2. **Given** 表单已显示, **When** 我选择人格类型, **Then** MBTISelector 组件已集成用于人格选择

3. **Given** 人格类型已选择, **When** 我查看预览区域, **Then** PersonalityPreview 组件显示选中人格的预览

4. **Given** 表单字段已填写, **When** 我尝试提交, **Then** 表单验证确保必填字段已填写

5. **Given** 表单验证通过, **When** 我提交表单, **Then** 调用后端创建代理并导航到代理详情页

6. **Given** 代理创建成功, **When** 我查看界面反馈, **Then** 创建成功显示确认通知

## Tasks / Subtasks

- [x] Task 1: 创建 AgentCreateForm 组件 (AC: 1, 4)
  - [x] 创建 `apps/omninova-tauri/src/components/agent/AgentCreateForm.tsx` 文件
  - [x] 定义 `AgentCreateFormProps` 接口（onSuccess, onCancel, className）
  - [x] 创建表单状态管理（name, description, domain, mbtiType）
  - [x] 实现名称输入框（必填，最大长度50字符）
  - [x] 实现描述文本域（可选，最大长度500字符）
  - [x] 实现专业领域输入框（可选，最大长度100字符）
  - [x] 实现表单验证逻辑（名称必填）
  - [x] 显示验证错误信息

- [x] Task 2: 集成 MBTISelector 组件 (AC: 2)
  - [x] 导入并使用 `MBTISelector` 组件
  - [x] 绑定选中值到表单状态
  - [x] 实现 onChange 回调更新 mbtiType
  - [x] 应用合适的布局和样式

- [x] Task 3: 集成 PersonalityPreview 组件 (AC: 3)
  - [x] 导入并使用 `PersonalityPreview` 组件
  - [x] 根据选中的 mbtiType 显示预览
  - [x] 实现未选择人格时的占位提示
  - [x] 应用响应式布局（表单左侧，预览右侧）

- [x] Task 4: 实现表单提交与后端集成 (AC: 5)
  - [x] 定义 `NewAgent` 接口匹配后端 API
  - [x] 实现提交按钮（禁用状态：加载中/验证失败）
  - [x] 调用 Tauri `create_agent` 命令
  - [x] 处理加载状态（按钮显示加载指示器）
  - [x] 处理创建成功（调用 onSuccess，显示通知）
  - [x] 处理创建失败（显示错误信息）

- [x] Task 5: 创建 AgentCreatePage 页面组件 (AC: 5)
  - [x] 创建 `apps/omninova-tauri/src/pages/AgentCreatePage.tsx` 文件
  - [x] 实现页面布局（标题、表单、返回按钮）
  - [x] 集成 AgentCreateForm 组件
  - [x] 实现成功后导航到代理详情页（或列表页）
  - [x] 添加取消按钮返回上一页

- [x] Task 6: 实现成功通知 (AC: 6)
  - [x] 使用 Shadcn/UI 的 toast 组件（sonner）
  - [x] 创建成功时显示绿色成功提示
  - [x] 失败时显示红色错误提示
  - [x] 提示内容包含代理名称

- [x] Task 7: 增强视觉设计 (AC: All)
  - [x] 使用人格类型对应的主题色
  - [x] 实现表单输入的 focus 状态样式
  - [x] 实现按钮的 hover 和 active 状态
  - [x] 确保响应式布局（移动端堆叠，桌面端并排）
  - [x] 添加适当的间距和动画过渡

- [x] Task 8: 添加单元测试 (AC: All)
  - [x] 测试表单渲染所有字段
  - [x] 测试必填字段验证
  - [x] 测试 MBTISelector 集成
  - [x] 测试 PersonalityPreview 显示条件
  - [x] 测试表单提交成功流程
  - [x] 测试表单提交失败处理
  - [x] 测试取消按钮行为

- [x] Task 9: 文档和导出 (AC: All)
  - [x] 添加组件 JSDoc 注释
  - [x] 更新 `components/agent/index.ts` 导出
  - [x] 运行 `npm run lint` 确保无警告

## Dev Notes

### 前置依赖（已完成）

**Story 2-1 Agent 数据模型：**
- `AgentModel` 结构体已定义
- `AgentStore` 已实现 CRUD 操作
- Tauri 命令已暴露：
  - `create_agent(config: NewAgent) -> AgentModel`
  - `get_agents() -> Vec<AgentModel>`
  - `get_agent_by_id(id: &str) -> Option<AgentModel>`

**Story 2-3 MBTISelector：**
- 组件位置：`apps/omninova-tauri/src/components/agent/MBTISelector.tsx`
- Props：`value`, `onChange`, `disabled`, `className`
- 支持分类筛选、搜索、键盘导航

**Story 2-4 PersonalityPreview：**
- 组件位置：`apps/omninova-tauri/src/components/agent/PersonalityPreview.tsx`
- Props：`mbtiType`, `className`
- 显示认知功能栈、示例对话、优势盲点、应用场景

### 数据类型定义

**NewAgent 接口（前端发送）：**
```typescript
interface NewAgent {
  name: string;           // 必填，最大50字符
  description?: string;   // 可选，最大500字符
  domain?: string;        // 可选，专业领域
  mbti_type?: MBTIType;   // 可选，人格类型
  system_prompt?: string; // 可选，系统提示词
}
```

**AgentModel 接口（后端返回）：**
```typescript
interface AgentModel {
  id: number;             // 自增主键
  agent_uuid: string;     // UUID
  name: string;
  description?: string;
  domain?: string;
  mbti_type?: MBTIType;
  status: 'active' | 'inactive' | 'archived';
  system_prompt?: string;
  created_at: number;     // Unix时间戳
  updated_at: number;
}
```

**MBTIType 类型：**
```typescript
type MBTIType =
  | 'INTJ' | 'INTP' | 'ENTJ' | 'ENTP'
  | 'INFJ' | 'INFP' | 'ENFJ' | 'ENFP'
  | 'ISTJ' | 'ISFJ' | 'ESTJ' | 'ESFJ'
  | 'ISTP' | 'ISFP' | 'ESTP' | 'ESFP';
```

### Tauri 命令调用

**创建代理：**
```typescript
import { invoke } from '@tauri-apps/api/core';

const createAgent = async (newAgent: NewAgent): Promise<AgentModel> => {
  return await invoke<AgentModel>('create_agent', { config: newAgent });
};
```

### 现有表单模式参考

**PersonaConfigForm 样式模式：**
```tsx
// 输入框样式
className="w-full bg-white/5 border border-white/10 rounded-md px-4 py-2 text-white placeholder:text-white/20 focus:outline-none focus:border-blue-500/50"

// 标签样式
className="block text-white/70 text-sm font-medium mb-2"

// 容器间距
className="space-y-6"
```

**表单状态管理模式：**
```tsx
interface FormState {
  name: string;
  description: string;
  domain: string;
  mbtiType?: MBTIType;
}

const [formState, setFormState] = useState<FormState>({
  name: '',
  description: '',
  domain: '',
  mbtiType: undefined,
});

const updateField = (field: keyof FormState, value: string | MBTIType | undefined) => {
  setFormState(prev => ({ ...prev, [field]: value }));
};
```

### 布局设计建议

```
┌─────────────────────────────────────────────────────────────────┐
│  创建新代理                                              [返回] │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌───────────────────────────┐  ┌─────────────────────────────┐│
│  │                           │  │                             ││
│  │  名称 *                   │  │  人格预览                   ││
│  │  [____________________]   │  │                             ││
│  │                           │  │  ┌─────────────────────────┐││
│  │  描述                     │  │  │ PersonalityPreview      │││
│  │  [____________________]   │  │  │ 组件                    │││
│  │  [____________________]   │  │  │                         │││
│  │                           │  │  └─────────────────────────┘││
│  │  专业领域                 │  │                             ││
│  │  [____________________]   │  │  选择人格类型后显示预览     ││
│  │                           │  │                             ││
│  │  人格类型选择             │  │                             ││
│  │  ┌─────────────────────┐  │  │                             ││
│  │  │ MBTISelector        │  │  │                             ││
│  │  │ 组件                │  │  │                             ││
│  │  └─────────────────────┘  │  │                             ││
│  │                           │  │                             ││
│  │  [取消]  [创建代理]       │  │                             ││
│  │                           │  │                             ││
│  └───────────────────────────┘  └─────────────────────────────┘│
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**响应式断点：**
- 移动端（< 768px）：表单和预览垂直堆叠
- 桌面端（>= 768px）：表单左侧（60%宽度），预览右侧（40%宽度）

### 表单验证规则

| 字段 | 必填 | 最大长度 | 验证规则 |
|------|------|----------|----------|
| name | ✅ | 50 | 非空，去除首尾空格后长度 > 0 |
| description | ❌ | 500 | 可选，自动截断超长内容 |
| domain | ❌ | 100 | 可选，专业领域描述 |
| mbti_type | ❌ | - | 可选，有效的 MBTI 类型 |

**验证实现：**
```typescript
const validateForm = (state: FormState): { valid: boolean; errors: Record<string, string> } => {
  const errors: Record<string, string> = {};

  const trimmedName = state.name.trim();
  if (!trimmedName) {
    errors.name = '请输入代理名称';
  } else if (trimmedName.length > 50) {
    errors.name = '名称不能超过50个字符';
  }

  if (state.description && state.description.length > 500) {
    errors.description = '描述不能超过500个字符';
  }

  if (state.domain && state.domain.length > 100) {
    errors.domain = '专业领域不能超过100个字符';
  }

  return {
    valid: Object.keys(errors).length === 0,
    errors,
  };
};
```

### Toast 通知实现

```tsx
import { toast } from 'sonner';

// 成功通知
toast.success(`代理 "${agent.name}" 创建成功！`);

// 失败通知
toast.error('创建代理失败', {
  description: error.message,
});
```

### 项目架构约束

- **组件位置**: `apps/omninova-tauri/src/components/agent/AgentCreateForm.tsx`
- **页面位置**: `apps/omninova-tauri/src/pages/AgentCreatePage.tsx`（如果需要独立页面）
- **样式系统**: Tailwind CSS + Shadcn/UI
- **状态管理**: React useState（组件内部状态）
- **命名约定**:
  - 组件文件: PascalCase (`AgentCreateForm.tsx`)
  - Props 接口: 组件名 + Props (`AgentCreateFormProps`)
  - CSS 类: kebab-case via Tailwind

### 可访问性要求

- 所有输入框有明确的 label 关联
- 表单错误信息使用 `aria-describedby` 关联
- 提交按钮在加载状态时显示 `aria-busy`
- 键盘可访问的表单提交（Enter 键）
- 焦点管理：提交后焦点回到表单或移动到新页面

### 测试策略

**单元测试（Vitest）：**
- 组件渲染测试
- 表单验证测试
- 用户交互测试（输入、选择、提交）
- Mock Tauri API 调用

**测试文件位置：**
- `apps/omninova-tauri/src/test/components/AgentCreateForm.test.tsx`

**Mock 模式：**
```typescript
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockInvoke = vi.mocked(invoke);
mockInvoke.mockResolvedValueOnce(mockAgentModel);
```

### 依赖项

无需添加新的依赖项，现有依赖已足够：
- React 19
- Tailwind CSS
- Shadcn/UI 组件（Button, Input, Card, Toast）
- @tauri-apps/api
- sonner（Toast）

### References

- [Source: epics.md#Story 2.5] - 验收标准
- [Source: architecture.md#前端组件位置] - 组件位置规范
- [Source: architecture.md#前端架构] - 状态管理模式
- [Source: ux-design-specification.md#核心组件] - UX 设计要求
- [Source: ux-design-specification.md#色彩系统] - 人格自适应色彩
- [Source: 2-1-agent-data-model.md] - Agent 数据模型和 Tauri 命令
- [Source: 2-3-mbti-selector-component.md] - MBTISelector 组件
- [Source: 2-4-personality-preview-component.md] - PersonalityPreview 组件
- [Source: PersonaConfigForm.tsx] - 现有表单模式参考

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

N/A

### Completion Notes List

1. **实现完成**: 所有任务已完成，包括 AgentCreateForm 组件、AgentCreatePage 页面、MBTISelector 和 PersonalityPreview 集成。
2. **表单验证**: 实现了 blur-based 验证，用户在字段失焦时触发验证，提供更好的用户体验。
3. **测试覆盖**: 创建了 24 个单元测试，覆盖组件渲染、验证、提交、MBTI 集成等场景，全部通过。
4. **Lint**: 新增文件无 lint 错误或警告。
5. **响应式布局**: 使用 grid 布局实现移动端堆叠、桌面端并排的响应式设计（表单 60%，预览 40%）。
6. **主题色集成**: 创建按钮使用选中人格类型的主题色，增强视觉一致性。

### File List

**新增文件:**
- `apps/omninova-tauri/src/components/agent/AgentCreateForm.tsx` - 代理创建表单组件
- `apps/omninova-tauri/src/pages/AgentCreatePage.tsx` - 代理创建页面组件
- `apps/omninova-tauri/src/pages/index.ts` - 页面组件导出
- `apps/omninova-tauri/src/test/components/AgentCreateForm.test.tsx` - 单元测试

**修改文件:**
- `apps/omninova-tauri/src/components/agent/index.ts` - 添加 AgentCreateForm 导出