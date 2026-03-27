# Story 10.1: 代理快速切换功能

**Story ID:** 10.1
**Status:** done
**Created:** 2026-03-27
**Epic:** Epic 10 - 界面与导航体验

---

## 用户故事

**As a** 用户,
**I want** 快速切换不同的 AI 代理,
**So that** 我可以高效地在多个代理之间工作.

---

## 验收标准

### 功能验收标准

1. **Given** 多个 AI 代理已创建, **When** 我查看侧边栏, **Then** 侧边栏显示代理列表供选择
2. **Given** 代理列表可见, **When** 我点击某个代理, **Then** 切换到该代理并更新对话内容
3. **Given** 应用运行中, **When** 我使用快捷键（如 Ctrl+1-9）, **Then** 切换到对应位置的代理
4. **Given** 我切换过代理, **When** 查看代理列表, **Then** 显示最近使用的代理列表（快速访问）
5. **Given** 切换代理后, **When** 查看对话界面, **Then** 当前选中的代理有明确的视觉指示
6. **Given** 切换代理后, **When** 有历史会话, **Then** 自动加载该代理的最近会话

### 非功能验收标准

- 切换响应时间 < 200ms
- 键盘快捷键支持 macOS (Cmd) 和 Windows/Linux (Ctrl)
- 可访问性：支持键盘导航（Tab/Arrow keys）

---

## 技术需求

### 前端实现 (React + TypeScript)

#### 1. 类型定义

**位置:** `apps/omninova-tauri/src/types/navigation.ts`

```typescript
/**
 * 代理切换快捷键配置
 */
export interface AgentShortcut {
  /** 快捷键索引 (1-9) */
  index: number;
  /** 代理 ID */
  agentId: number | null;
}

/**
 * 最近使用的代理记录
 */
export interface RecentAgent {
  /** 代理 ID */
  agentId: number;
  /** 最后访问时间 */
  lastAccessedAt: string;
}

/**
 * 导航状态
 */
export interface NavigationState {
  /** 当前激活的代理 ID */
  activeAgentId: number | null;
  /** 最近使用的代理 ID 列表（最多5个） */
  recentAgentIds: number[];
  /** 代理快捷键映射 */
  agentShortcuts: Map<number, number>; // agentId -> shortcut index
}
```

#### 2. 状态管理

**位置:** `apps/omninova-tauri/src/stores/navigationStore.ts`

```typescript
import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';
import type { NavigationState } from '@/types/navigation';

interface NavigationActions {
  /** 设置当前激活代理 */
  setActiveAgent: (agentId: number | null) => void;
  /** 记录代理访问（更新最近使用） */
  recordAgentAccess: (agentId: number) => void;
  /** 设置代理快捷键 */
  setAgentShortcut: (index: number, agentId: number | null) => void;
  /** 获取代理快捷键索引 */
  getAgentShortcutIndex: (agentId: number) => number | undefined;
}

export const useNavigationStore = create<NavigationState & NavigationActions>()(
  persist(
    (set, get) => ({
      activeAgentId: null,
      recentAgentIds: [],
      agentShortcuts: new Map(),

      setActiveAgent: (agentId) => {
        set({ activeAgentId: agentId });
        if (agentId !== null) {
          get().recordAgentAccess(agentId);
        }
      },

      recordAgentAccess: (agentId) => {
        set((state) => {
          const recentIds = [agentId, ...state.recentAgentIds.filter(id => id !== agentId)];
          return {
            recentAgentIds: recentIds.slice(0, 5), // 保留最近5个
          };
        });
      },

      setAgentShortcut: (index, agentId) => {
        set((state) => {
          const shortcuts = new Map(state.agentShortcuts);
          if (agentId === null) {
            // 移除快捷键
            for (const [aId, idx] of shortcuts) {
              if (idx === index) {
                shortcuts.delete(aId);
                break;
              }
            }
          } else {
            shortcuts.set(agentId, index);
          }
          return { agentShortcuts: shortcuts };
        });
      },

      getAgentShortcutIndex: (agentId) => {
        return get().agentShortcuts.get(agentId);
      },
    }),
    {
      name: 'omninova-navigation-storage',
      storage: createJSONStorage(() => localStorage),
      // Map 需要序列化转换
      partialize: (state) => ({
        activeAgentId: state.activeAgentId,
        recentAgentIds: state.recentAgentIds,
        agentShortcuts: Array.from(state.agentShortcuts.entries()),
      }),
      revive: (state) => ({
        ...state,
        agentShortcuts: new Map(state.agentShortcuts || []),
      }),
    }
  )
);
```

#### 3. 组件结构

**位置:** `apps/omninova-tauri/src/components/navigation/`

```
navigation/
├── AgentSidebar.tsx         # 代理侧边栏容器
├── AgentSidebarItem.tsx     # 单个代理列表项
├── AgentQuickSwitch.tsx     # 快捷键切换处理 Hook
├── RecentAgentsList.tsx     # 最近使用的代理列表
├── index.ts
```

#### 4. AgentSidebar 组件

```tsx
// AgentSidebar.tsx
import { useNavigationStore } from '@/stores/navigationStore';
import { useAgentStore } from '@/stores/agentStore'; // 假设已存在
import { AgentSidebarItem } from './AgentSidebarItem';
import { cn } from '@/lib/utils';

export function AgentSidebar() {
  const agents = useAgentStore((s) => s.agents);
  const activeAgentId = useNavigationStore((s) => s.activeAgentId);
  const setActiveAgent = useNavigationStore((s) => s.setActiveAgent);
  const recentAgentIds = useNavigationStore((s) => s.recentAgentIds);

  // 将最近使用的代理排在前面
  const sortedAgents = React.useMemo(() => {
    const recentSet = new Set(recentAgentIds);
    const recent = agents.filter(a => recentSet.has(a.id));
    const others = agents.filter(a => !recentSet.has(a.id));
    return [...recent, ...others];
  }, [agents, recentAgentIds]);

  return (
    <aside className="w-64 border-r border-border bg-muted/30 flex flex-col">
      <div className="p-4 border-b border-border">
        <h2 className="text-sm font-medium text-muted-foreground">代理列表</h2>
      </div>
      <nav className="flex-1 overflow-y-auto p-2 space-y-1">
        {sortedAgents.map((agent, index) => (
          <AgentSidebarItem
            key={agent.id}
            agent={agent}
            isActive={agent.id === activeAgentId}
            shortcutIndex={index < 9 ? index + 1 : undefined}
            onClick={() => setActiveAgent(agent.id)}
          />
        ))}
      </nav>
    </aside>
  );
}
```

#### 5. 快捷键 Hook

```tsx
// useAgentQuickSwitch.ts
import { useEffect, useCallback } from 'react';
import { useNavigationStore } from '@/stores/navigationStore';
import { useAgentStore } from '@/stores/agentStore';

const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
const MODIFIER_KEY = isMac ? 'metaKey' : 'ctrlKey';

/**
 * Hook for handling agent quick switch keyboard shortcuts
 * Ctrl/Cmd + 1-9 to switch between agents
 */
export function useAgentQuickSwitch() {
  const agents = useAgentStore((s) => s.agents);
  const setActiveAgent = useNavigationStore((s) => s.setActiveAgent);

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    // Check for Ctrl (Windows/Linux) or Cmd (macOS)
    if (!e[MODIFIER_KEY]) return;

    // Only handle 1-9
    const num = parseInt(e.key);
    if (isNaN(num) || num < 1 || num > 9) return;

    // Get agent at index (1-based to 0-based)
    const agentIndex = num - 1;
    const agent = agents[agentIndex];

    if (agent) {
      e.preventDefault();
      setActiveAgent(agent.id);
    }
  }, [agents, setActiveAgent]);

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);
}
```

---

## 架构合规要求

### 命名约定

| 层级 | 规则 | 示例 |
|------|------|------|
| TypeScript 类型 | PascalCase | `NavigationState`, `RecentAgent` |
| TypeScript 函数 | camelCase | `setActiveAgent()`, `recordAgentAccess()` |
| 组件 | PascalCase | `AgentSidebar`, `AgentSidebarItem` |
| Store 文件 | camelCase + Store 后缀 | `navigationStore.ts` |

### 文件组织

```
apps/omninova-tauri/src/
├── types/navigation.ts          # 导航类型定义
├── stores/navigationStore.ts    # 导航状态管理
└── components/navigation/       # 导航组件
    ├── AgentSidebar.tsx
    ├── AgentSidebarItem.tsx
    ├── AgentQuickSwitch.tsx
    ├── RecentAgentsList.tsx
    └── index.ts
```

---

## 测试要求

### 前端单元测试

```typescript
// navigationStore.test.ts
import { describe, it, expect, beforeEach } from 'vitest';
import { useNavigationStore } from './navigationStore';

describe('NavigationStore', () => {
  beforeEach(() => {
    useNavigationStore.setState({
      activeAgentId: null,
      recentAgentIds: [],
      agentShortcuts: new Map(),
    });
  });

  it('should set active agent', () => {
    const { setActiveAgent } = useNavigationStore.getState();
    setActiveAgent(1);
    expect(useNavigationStore.getState().activeAgentId).toBe(1);
  });

  it('should record agent access and maintain recent list', () => {
    const { recordAgentAccess } = useNavigationStore.getState();
    recordAgentAccess(1);
    recordAgentAccess(2);
    recordAgentAccess(1); // 重复访问

    const { recentAgentIds } = useNavigationStore.getState();
    expect(recentAgentIds).toEqual([1, 2]); // 1 在前面，去重
  });

  it('should limit recent agents to 5', () => {
    const { recordAgentAccess } = useNavigationStore.getState();
    for (let i = 1; i <= 6; i++) {
      recordAgentAccess(i);
    }

    const { recentAgentIds } = useNavigationStore.getState();
    expect(recentAgentIds).toHaveLength(5);
    expect(recentAgentIds).not.toContain(1); // 最早的被移除
  });
});
```

### 组件测试

- AgentSidebar 渲染测试
- AgentSidebarItem 点击交互测试
- 快捷键触发切换测试

---

## 依赖关系

### 前置依赖

- ✅ Agent 数据模型和 Store (Epic 2)
- ✅ Session Store (Story 4-7)
- ✅ Shadcn/UI 组件库
- ✅ Zustand 状态管理

### 后置依赖

- Story 10.2: 历史对话导航（需要在侧边栏显示会话）
- Story 10.6: 键盘快捷键支持（全局快捷键注册）

---

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 快捷键与系统冲突 | 中 | 允许自定义快捷键绑定 |
| 代理列表过长性能 | 低 | 虚拟滚动（超过50个代理时） |
| 状态持久化兼容性 | 低 | 使用 localStorage 序列化 Map |

---

## 实施建议

### 推荐实施顺序

1. **类型和状态管理**
   - 创建 `types/navigation.ts`
   - 创建 `stores/navigationStore.ts`
   - 编写单元测试

2. **侧边栏组件**
   - 创建 `AgentSidebar.tsx` 容器
   - 创建 `AgentSidebarItem.tsx` 列表项
   - 集成现有 AgentStore

3. **快捷键功能**
   - 创建 `useAgentQuickSwitch.ts` Hook
   - 在应用根组件挂载 Hook

4. **视觉优化**
   - 当前代理高亮样式
   - 最近使用代理标识
   - 快捷键提示徽章

### 参考现有实现

- `components/agent/AgentList.tsx` - 代理列表渲染模式
- `components/agent/AgentCard.tsx` - 代理卡片样式
- `stores/sessionStore.ts` - 状态管理模式

### 键盘事件处理注意事项

- macOS 使用 `metaKey` (Cmd)，Windows/Linux 使用 `ctrlKey`
- 需要阻止默认行为防止浏览器快捷键冲突
- 快捷键仅在主窗口激活时生效

---

## 完成标准

- [ ] 创建 `types/navigation.ts` 类型定义
- [ ] 创建 `stores/navigationStore.ts` 状态管理
- [ ] 创建 `components/navigation/AgentSidebar.tsx` 侧边栏组件
- [ ] 创建 `components/navigation/AgentSidebarItem.tsx` 列表项组件
- [ ] 创建 `hooks/useAgentQuickSwitch.ts` 快捷键 Hook
- [ ] 实现代理切换后自动加载最近会话
- [ ] 实现最近使用代理排序
- [ ] 当前代理视觉高亮
- [ ] 快捷键提示显示（Ctrl+1-9）
- [ ] 单元测试覆盖核心逻辑
- [ ] 更新 sprint-status.yaml 状态

---

## Dev Agent Record

### Agent Model Used

claude-sonnet-4.6

### Debug Log References

None

### Completion Notes List

**Implementation Summary:**
- Created `types/navigation.ts` with NavigationState, NavigationActions, and helper functions
- Created `stores/navigationStore.ts` with Zustand store for active agent, recent agents, and shortcuts
- Created `components/navigation/AgentSidebar.tsx` - main sidebar component
- Created `components/navigation/AgentSidebarItem.tsx` - individual agent list item
- Created `hooks/useAgentQuickSwitch.ts` - keyboard shortcut handler hook
- All 12 unit tests pass

**Technical Decisions:**
- Used Record<number, number> for shortcuts instead of Map for easier serialization
- MAX_RECENT_AGENTS = 5 to balance usability and UI space
- Keyboard modifier detection at runtime for cross-platform support
- Added `setOnAgentSwitchCallback` for integration with session loading (AC #6)

**Code Review Fixes:**
- Added callback mechanism for auto-loading recent session when agent switches
- Removed unused type definitions (AgentShortcut, RecentAgent)

### File List

**新增文件:**
- `apps/omninova-tauri/src/types/navigation.ts`
- `apps/omninova-tauri/src/stores/navigationStore.ts`
- `apps/omninova-tauri/src/stores/navigationStore.test.ts`
- `apps/omninova-tauri/src/components/navigation/AgentSidebar.tsx`
- `apps/omninova-tauri/src/components/navigation/AgentSidebarItem.tsx`
- `apps/omninova-tauri/src/components/navigation/index.ts`
- `apps/omninova-tauri/src/hooks/useAgentQuickSwitch.ts`