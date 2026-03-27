# Story 10.6: 键盘快捷键

**Story ID:** 10.6
**Status:** done
**Created:** 2026-03-27
**Epic:** Epic 10 - 界面与导航体验

---

## 用户故事

**As a** 用户,
**I want** 使用键盘快捷键操作应用,
**So that** 我可以更高效地完成任务.

---

## 验收标准

### 功能验收标准

1. **Given** 应用已运行, **When** 我按下 Cmd/Ctrl+K, **Then** 打开全局搜索
2. **Given** 应用已运行, **When** 我按下 Cmd/Ctrl+B, **Then** 切换侧边栏显示
3. **Given** 应用已运行, **When** 我按下 Cmd/Ctrl+数字, **Then** 切换到对应序号的代理
4. **Given** 应用已运行, **When** 我按下 Cmd/Ctrl+N, **Then** 创建新会话
5. **Given** 我在输入框, **When** 我按下 Enter, **Then** 发送消息
6. **Given** 我在输入框, **When** 我按下 Shift+Enter, **Then** 换行而不发送

### 非功能验收标准

- 快捷键响应及时（<100ms）
- 显示快捷键提示
- 支持自定义快捷键

---

## 技术需求

### 前端实现 (React + TypeScript)

#### 1. 快捷键类型

**位置:** `apps/omninova-tauri/src/types/shortcut.ts`

```typescript
/**
 * 快捷键组合
 */
export interface ShortcutKey {
  /** 主键 */
  key: string;
  /** 是否需要 Ctrl/Cmd */
  meta?: boolean;
  /** 是否需要 Shift */
  shift?: boolean;
  /** 是否需要 Alt */
  alt?: boolean;
}

/**
 * 快捷键动作类型
 */
export type ShortcutAction =
  | 'globalSearch'
  | 'toggleSidebar'
  | 'newSession'
  | 'switchAgent1'
  | 'switchAgent2'
  | 'switchAgent3'
  | 'switchAgent4'
  | 'switchAgent5';

/**
 * 快捷键配置
 */
export interface ShortcutConfig {
  action: ShortcutAction;
  keys: ShortcutKey;
  description: string;
}

/**
 * 默认快捷键配置
 */
export const DEFAULT_SHORTCUTS: ShortcutConfig[] = [
  { action: 'globalSearch', keys: { key: 'k', meta: true }, description: '全局搜索' },
  { action: 'toggleSidebar', keys: { key: 'b', meta: true }, description: '切换侧边栏' },
  { action: 'newSession', keys: { key: 'n', meta: true }, description: '新会话' },
  { action: 'switchAgent1', keys: { key: '1', meta: true }, description: '切换代理 1' },
  { action: 'switchAgent2', keys: { key: '2', meta: true }, description: '切换代理 2' },
  { action: 'switchAgent3', keys: { key: '3', meta: true }, description: '切换代理 3' },
  { action: 'switchAgent4', keys: { key: '4', meta: true }, description: '切换代理 4' },
  { action: 'switchAgent5', keys: { key: '5', meta: true }, description: '切换代理 5' },
];
```

#### 2. 快捷键 Store

**位置:** `apps/omninova-tauri/src/stores/shortcutStore.ts`

```typescript
import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { ShortcutConfig, ShortcutAction, ShortcutKey } from '@/types/shortcut';
import { DEFAULT_SHORTCUTS } from '@/types/shortcut';

interface ShortcutState {
  shortcuts: ShortcutConfig[];
  enabled: boolean;
}

interface ShortcutActions {
  updateShortcut: (action: ShortcutAction, keys: ShortcutKey) => void;
  resetShortcuts: () => void;
  toggleEnabled: () => void;
  getShortcut: (action: ShortcutAction) => ShortcutConfig | undefined;
}

export const useShortcutStore = create<ShortcutState & ShortcutActions>()(
  persist(
    (set, get) => ({
      shortcuts: DEFAULT_SHORTCUTS,
      enabled: true,

      updateShortcut: (action, keys) => {
        set((state) => ({
          shortcuts: state.shortcuts.map((s) =>
            s.action === action ? { ...s, keys } : s
          ),
        }));
      },

      resetShortcuts: () => {
        set({ shortcuts: DEFAULT_SHORTCUTS });
      },

      toggleEnabled: () => {
        set((state) => ({ enabled: !state.enabled }));
      },

      getShortcut: (action) => {
        return get().shortcuts.find((s) => s.action === action);
      },
    }),
    {
      name: 'omninova-shortcut-storage',
    }
  )
);
```

#### 3. 全局快捷键 Hook

**位置:** `apps/omninova-tauri/src/hooks/useGlobalShortcuts.ts`

```typescript
import { useEffect, useCallback } from 'react';
import { useShortcutStore } from '@/stores/shortcutStore';
import { getModifierKey } from '@/types/navigation';

export function useGlobalShortcuts(handlers: Record<string, () => void>) {
  const shortcuts = useShortcutStore((s) => s.shortcuts);
  const enabled = useShortcutStore((s) => s.enabled);

  const handleKeyDown = useCallback(
    (event: KeyboardEvent) => {
      if (!enabled) return;

      const modKey = getModifierKey();
      const metaPressed = event[modKey];
      const shiftPressed = event.shiftKey;
      const altPressed = event.altKey;

      for (const shortcut of shortcuts) {
        const { keys, action } = shortcut;
        const keyMatches = event.key.toLowerCase() === keys.key.toLowerCase();
        const metaMatches = keys.meta ? metaPressed : !metaPressed;
        const shiftMatches = keys.shift ? shiftPressed : !shiftPressed;
        const altMatches = keys.alt ? altPressed : !altPressed;

        if (keyMatches && metaMatches && shiftMatches && altMatches) {
          const handler = handlers[action];
          if (handler) {
            event.preventDefault();
            handler();
          }
          break;
        }
      }
    },
    [shortcuts, enabled, handlers]
  );

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);
}
```

---

## 架构合规要求

### 文件组织

```
apps/omninova-tauri/src/
├── types/shortcut.ts
├── stores/shortcutStore.ts
├── hooks/useGlobalShortcuts.ts
└── components/shortcuts/
    ├── ShortcutHelp.tsx
    └── index.ts
```

---

## 完成标准

- [x] 创建 `types/shortcut.ts` 类型定义
- [x] 创建 `stores/shortcutStore.ts` 状态管理
- [x] 创建 `hooks/useGlobalShortcuts.ts` 全局快捷键处理
- [x] 创建 `components/shortcuts/ShortcutHelp.tsx` 帮助面板
- [x] 实现默认快捷键
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
- Created `types/shortcut.ts` with ShortcutKey, ShortcutAction, ShortcutConfig types
- Created `stores/shortcutStore.ts` with update/reset/toggle operations
- Created `hooks/useGlobalShortcuts.ts` for global keyboard event handling
- Created `components/shortcuts/ShortcutHelp.tsx` for shortcut reference dialog
- All 8 unit tests pass

**Technical Decisions:**
- 8 default shortcuts: Cmd/Ctrl + K/B/N/1-5
- Cross-platform modifier key detection (metaKey on macOS, ctrlKey elsewhere)
- Shortcuts disabled in input fields except Escape and Enter
- Shortcut keys are persisted to localStorage
- Display uses ⌘ on macOS, Ctrl on Windows/Linux

### File List

**新增文件:**
- `apps/omninova-tauri/src/types/shortcut.ts`
- `apps/omninova-tauri/src/stores/shortcutStore.ts`
- `apps/omninova-tauri/src/stores/shortcutStore.test.ts`
- `apps/omninova-tauri/src/hooks/useGlobalShortcuts.ts`
- `apps/omninova-tauri/src/components/shortcuts/ShortcutHelp.tsx`
- `apps/omninova-tauri/src/components/shortcuts/index.ts`