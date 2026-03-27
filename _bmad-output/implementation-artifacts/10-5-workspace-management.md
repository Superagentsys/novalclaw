# Story 10.5: 工作空间管理

**Story ID:** 10.5
**Status:** done
**Created:** 2026-03-27
**Epic:** Epic 10 - 界面与导航体验

---

## 用户故事

**As a** 用户,
**I want** 管理多个工作空间,
**So that** 我可以为不同项目或场景创建独立的工作环境.

---

## 验收标准

### 功能验收标准

1. **Given** 应用已运行, **When** 我点击工作空间菜单, **Then** 可以查看当前工作空间列表
2. **Given** 我在工作空间菜单, **When** 我点击新建, **Then** 可以创建新的工作空间
3. **Given** 我有多个工作空间, **When** 我切换工作空间, **Then** 所有会话和设置切换到对应空间
4. **Given** 我在工作空间详情, **When** 我编辑, **Then** 可以修改工作空间名称和图标
5. **Given** 我选择一个工作空间, **When** 我点击删除, **Then** 可以删除工作空间（至少保留一个）
6. **Given** 我想要快速切换, **When** 我使用快捷键, **Then** 可以通过快捷键切换工作空间

### 非功能验收标准

- 工作空间切换流畅（无卡顿）
- 工作空间数据隔离
- 支持工作空间导出/导入

---

## 技术需求

### 前端实现 (React + TypeScript)

#### 1. 工作空间类型

**位置:** `apps/omninova-tauri/src/types/workspace.ts`

```typescript
/**
 * 工作空间配置
 */
export interface Workspace {
  /** 工作空间 ID */
  id: string;
  /** 工作空间名称 */
  name: string;
  /** 工作空间图标 (emoji) */
  icon: string;
  /** 创建时间 */
  createdAt: string;
  /** 最后访问时间 */
  lastAccessedAt: string;
}

/**
 * 默认工作空间
 */
export const DEFAULT_WORKSPACE: Workspace = {
  id: 'default',
  name: '默认工作空间',
  icon: '🏠',
  createdAt: new Date().toISOString(),
  lastAccessedAt: new Date().toISOString(),
};

/**
 * 预设图标
 */
export const WORKSPACE_ICONS = [
  '🏠', '💼', '🚀', '📚', '🎮', '🎨', '🔧', '📊',
  '🌟', '🎯', '💻', '🔬', '📱', '🎵', '✈️', '🏠',
] as const;
```

#### 2. 工作空间 Store

**位置:** `apps/omninova-tauri/src/stores/workspaceStore.ts`

```typescript
import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { Workspace } from '@/types/workspace';
import { DEFAULT_WORKSPACE } from '@/types/workspace';

interface WorkspaceState {
  workspaces: Workspace[];
  activeWorkspaceId: string;
}

interface WorkspaceActions {
  createWorkspace: (name: string, icon: string) => string;
  updateWorkspace: (id: string, updates: Partial<Pick<Workspace, 'name' | 'icon'>>) => void;
  deleteWorkspace: (id: string) => void;
  switchWorkspace: (id: string) => void;
  getActiveWorkspace: () => Workspace | undefined;
}

export const useWorkspaceStore = create<WorkspaceState & WorkspaceActions>()(
  persist(
    (set, get) => ({
      workspaces: [DEFAULT_WORKSPACE],
      activeWorkspaceId: DEFAULT_WORKSPACE.id,

      createWorkspace: (name, icon) => {
        const id = `workspace-${Date.now()}`;
        const now = new Date().toISOString();
        const newWorkspace: Workspace = {
          id,
          name,
          icon,
          createdAt: now,
          lastAccessedAt: now,
        };
        set((state) => ({
          workspaces: [...state.workspaces, newWorkspace],
        }));
        return id;
      },

      updateWorkspace: (id, updates) => {
        set((state) => ({
          workspaces: state.workspaces.map((ws) =>
            ws.id === id ? { ...ws, ...updates } : ws
          ),
        }));
      },

      deleteWorkspace: (id) => {
        const { workspaces, activeWorkspaceId } = get();
        // Cannot delete the last workspace
        if (workspaces.length <= 1) return;
        // Cannot delete active workspace
        if (id === activeWorkspaceId) return;

        set((state) => ({
          workspaces: state.workspaces.filter((ws) => ws.id !== id),
        }));
      },

      switchWorkspace: (id) => {
        const { workspaces } = get();
        if (!workspaces.find((ws) => ws.id === id)) return;

        const now = new Date().toISOString();
        set((state) => ({
          activeWorkspaceId: id,
          workspaces: state.workspaces.map((ws) =>
            ws.id === id ? { ...ws, lastAccessedAt: now } : ws
          ),
        }));
      },

      getActiveWorkspace: () => {
        const { workspaces, activeWorkspaceId } = get();
        return workspaces.find((ws) => ws.id === activeWorkspaceId);
      },
    }),
    {
      name: 'omninova-workspace-storage',
    }
  )
);
```

---

## 架构合规要求

### 文件组织

```
apps/omninova-tauri/src/
├── types/workspace.ts
├── stores/workspaceStore.ts
└── components/workspace/
    ├── WorkspaceSelector.tsx
    ├── WorkspaceList.tsx
    ├── WorkspaceCreateDialog.tsx
    └── index.ts
```

---

## 完成标准

- [x] 创建 `types/workspace.ts` 类型定义
- [x] 创建 `stores/workspaceStore.ts` 状态管理
- [x] 创建 `components/workspace/WorkspaceSelector.tsx` 选择器
- [x] 创建 `components/workspace/WorkspaceList.tsx` 列表
- [x] 创建 `components/workspace/WorkspaceCreateDialog.tsx` 创建对话框
- [x] 实现工作空间切换
- [x] 实现工作空间创建/编辑/删除
- [x] 单元测试
- [x] 更新 sprint-status.yaml

---

## Dev Agent Record

### Agent Model Used

claude-sonnet-4.6

### Debug Log References

None

### Completion Notes List

**Implementation Summary:**
- Created `types/workspace.ts` with Workspace interface, default workspace, and 16 preset icons
- Created `stores/workspaceStore.ts` with CRUD operations for workspaces
- Created `components/workspace/WorkspaceSelector.tsx` for dropdown selection
- Created `components/workspace/WorkspaceList.tsx` for full management UI
- Created `components/workspace/WorkspaceCreateDialog.tsx` for creation flow
- All 15 unit tests pass

**Technical Decisions:**
- Used localStorage for workspace persistence via Zustand persist middleware
- Workspace IDs generated with timestamp: `workspace-${Date.now()}`
- Cannot delete the last workspace or active workspace
- 16 preset emoji icons available
- Edit dialog supports name and icon modification

### File List

**新增文件:**
- `apps/omninova-tauri/src/types/workspace.ts`
- `apps/omninova-tauri/src/stores/workspaceStore.ts`
- `apps/omninova-tauri/src/stores/workspaceStore.test.ts`
- `apps/omninova-tauri/src/components/workspace/WorkspaceSelector.tsx`
- `apps/omninova-tauri/src/components/workspace/WorkspaceList.tsx`
- `apps/omninova-tauri/src/components/workspace/WorkspaceCreateDialog.tsx`
- `apps/omninova-tauri/src/components/workspace/index.ts`