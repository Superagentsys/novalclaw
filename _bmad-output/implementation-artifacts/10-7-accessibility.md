# Story 10.7: 无障碍访问

**Story ID:** 10.7
**Status:** done
**Created:** 2026-03-27
**Epic:** Epic 10 - 界面与导航体验

---

## 用户故事

**As a** 有视觉或运动障碍的用户,
**I want** 应用支持无障碍访问,
**So that** 我可以使用辅助技术操作应用.

---

## 验收标准

### 功能验收标准

1. **Given** 用户使用屏幕阅读器, **When** 操作界面, **Then** 所有交互元素有正确的 aria 标签
2. **Given** 用户只能使用键盘, **When** 导航应用, **Then** 可以通过 Tab 键访问所有功能
3. **Given** 用户有视觉障碍, **When** 使用高对比度模式, **Then** 界面显示清晰可见
4. **Given** 用户需要更大字体, **When** 调整字体大小, **Then** 界面元素自适应缩放
5. **Given** 用户使用缩放功能, **When** 界面缩放, **Then** 布局保持可读性

### 非功能验收标准

- WCAG 2.1 AA 级别合规
- 键盘导航焦点可见
- 支持系统级高对比度设置

---

## 技术需求

### 前端实现 (React + TypeScript)

#### 1. 无障碍配置类型

**位置:** `apps/omninova-tauri/src/types/accessibility.ts`

```typescript
/**
 * 无障碍设置
 */
export interface AccessibilitySettings {
  /** 启用高对比度模式 */
  highContrast: boolean;
  /** 启用大字体模式 */
  largeText: boolean;
  /** 启用减少动画 */
  reduceMotion: boolean;
  /** 界面缩放比例 */
  zoomLevel: number;
  /** 启用屏幕阅读器优化 */
  screenReaderMode: boolean;
}

/**
 * 默认无障碍设置
 */
export const DEFAULT_ACCESSIBILITY: AccessibilitySettings = {
  highContrast: false,
  largeText: false,
  reduceMotion: false,
  zoomLevel: 100,
  screenReaderMode: false,
};

/**
 * 缩放级别选项
 */
export const ZOOM_LEVELS = [75, 90, 100, 110, 125, 150, 175, 200] as const;
```

#### 2. 无障碍 Store

**位置:** `apps/omninova-tauri/src/stores/accessibilityStore.ts`

```typescript
import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { AccessibilitySettings } from '@/types/accessibility';
import { DEFAULT_ACCESSIBILITY, ZOOM_LEVELS } from '@/types/accessibility';

interface AccessibilityState extends AccessibilitySettings {}

interface AccessibilityActions {
  toggleHighContrast: () => void;
  toggleLargeText: () => void;
  toggleReduceMotion: () => void;
  toggleScreenReaderMode: () => void;
  setZoomLevel: (level: number) => void;
  zoomIn: () => void;
  zoomOut: () => void;
  resetToDefaults: () => void;
}

export const useAccessibilityStore = create<AccessibilityState & AccessibilityActions>()(
  persist(
    (set, get) => ({
      ...DEFAULT_ACCESSIBILITY,

      toggleHighContrast: () => {
        set((state) => ({ highContrast: !state.highContrast }));
      },

      toggleLargeText: () => {
        set((state) => ({ largeText: !state.largeText }));
      },

      toggleReduceMotion: () => {
        set((state) => ({ reduceMotion: !state.reduceMotion }));
      },

      toggleScreenReaderMode: () => {
        set((state) => ({ screenReaderMode: !state.screenReaderMode }));
      },

      setZoomLevel: (level) => {
        if (ZOOM_LEVELS.includes(level as never)) {
          set({ zoomLevel: level });
        }
      },

      zoomIn: () => {
        const { zoomLevel } = get();
        const currentIndex = ZOOM_LEVELS.indexOf(zoomLevel as never);
        if (currentIndex < ZOOM_LEVELS.length - 1) {
          set({ zoomLevel: ZOOM_LEVELS[currentIndex + 1] });
        }
      },

      zoomOut: () => {
        const { zoomLevel } = get();
        const currentIndex = ZOOM_LEVELS.indexOf(zoomLevel as never);
        if (currentIndex > 0) {
          set({ zoomLevel: ZOOM_LEVELS[currentIndex - 1] });
        }
      },

      resetToDefaults: () => {
        set(DEFAULT_ACCESSIBILITY);
      },
    }),
    {
      name: 'omninova-accessibility-storage',
    }
  )
);
```

#### 3. 焦点管理 Hook

**位置:** `apps/omninova-tauri/src/hooks/useFocusManagement.ts`

```typescript
import { useRef, useCallback, useEffect } from 'react';

export function useFocusManagement<T extends HTMLElement>() {
  const ref = useRef<T>(null);

  const focus = useCallback(() => {
    ref.current?.focus();
  }, []);

  const blur = useCallback(() => {
    ref.current?.blur();
  }, []);

  return { ref, focus, blur };
}

export function useFocusTrap(containerRef: React.RefObject<HTMLElement>) {
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const focusableElements = container.querySelectorAll<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
    );

    const firstElement = focusableElements[0];
    const lastElement = focusableElements[focusableElements.length - 1];

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key !== 'Tab') return;

      if (e.shiftKey) {
        if (document.activeElement === firstElement) {
          e.preventDefault();
          lastElement?.focus();
        }
      } else {
        if (document.activeElement === lastElement) {
          e.preventDefault();
          firstElement?.focus();
        }
      }
    };

    container.addEventListener('keydown', handleKeyDown);
    return () => container.removeEventListener('keydown', handleKeyDown);
  }, [containerRef]);
}
```

---

## 架构合规要求

### 文件组织

```
apps/omninova-tauri/src/
├── types/accessibility.ts
├── stores/accessibilityStore.ts
├── hooks/useFocusManagement.ts
└── components/accessibility/
    ├── AccessibilitySettings.tsx
    └── index.ts
```

---

## 完成标准

- [x] 创建 `types/accessibility.ts` 类型定义
- [x] 创建 `stores/accessibilityStore.ts` 状态管理
- [x] 创建 `hooks/useFocusManagement.ts` 焦点管理
- [x] 创建 `components/accessibility/AccessibilitySettings.tsx` 设置面板
- [x] 实现高对比度模式
- [x] 实现字体缩放
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
- Created `types/accessibility.ts` with AccessibilitySettings interface and zoom levels
- Created `stores/accessibilityStore.ts` with toggle/set operations for all settings
- Created `hooks/useFocusManagement.ts` with useFocusManagement, useFocusTrap, useAutoFocus
- Created `components/accessibility/AccessibilitySettings.tsx` for settings dialog
- All 12 unit tests pass

**Technical Decisions:**
- 5 accessibility options: highContrast, largeText, reduceMotion, zoomLevel, screenReaderMode
- 8 zoom levels: 75%, 90%, 100%, 110%, 125%, 150%, 175%, 200%
- Focus trap implementation for modal dialogs
- Auto-focus hook for initial focus management
- Settings persisted to localStorage

### File List

**新增文件:**
- `apps/omninova-tauri/src/types/accessibility.ts`
- `apps/omninova-tauri/src/stores/accessibilityStore.ts`
- `apps/omninova-tauri/src/stores/accessibilityStore.test.ts`
- `apps/omninova-tauri/src/hooks/useFocusManagement.ts`
- `apps/omninova-tauri/src/components/accessibility/AccessibilitySettings.tsx`
- `apps/omninova-tauri/src/components/accessibility/index.ts`

### Completion Notes List

(待填写)

### File List

(待填写)