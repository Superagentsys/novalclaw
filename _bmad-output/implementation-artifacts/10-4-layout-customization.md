# Story 10.4: 界面布局自定义

**Story ID:** 10.4
**Status:** done
**Created:** 2026-03-27
**Epic:** Epic 10 - 界面与导航体验

---

## 用户故事

**As a** 用户,
**I want** 自定义应用界面布局,
**So that** 我可以按个人偏好调整工作空间.

---

## 验收标准

### 功能验收标准

1. **Given** 桌面应用已运行, **When** 我调整界面布局, **Then** 可以显示/隐藏侧边栏
2. **Given** 界面布局支持调整, **When** 我拖拽分割线, **Then** 可以调整面板大小
3. **Given** 界面有多区域, **When** 我操作界面, **Then** 可以折叠/展开不同区域
4. **Given** 我修改布局, **When** 我重启应用, **Then** 布局设置被持久化保存
5. **Given** 我想要默认布局, **When** 我点击重置, **Then** 可以重置为默认布局
6. **Given** 我有多种使用场景, **When** 我保存布局, **Then** 支持保存多个布局预设

### 非功能验收标准

- 布局调整响应流畅（无卡顿）
- 最小/最大面板宽度限制
- 支持键盘操作调整面板

---

## 技术需求

### 前端实现 (React + TypeScript)

#### 1. 布局状态类型

**位置:** `apps/omninova-tauri/src/types/layout.ts`

```typescript
/**
 * 面板可见性配置
 */
export interface PanelVisibility {
  /** 左侧边栏 */
  sidebar: boolean;
  /** 右侧面板 */
  rightPanel: boolean;
  /** 底部面板 */
  bottomPanel: boolean;
}

/**
 * 面板尺寸配置
 */
export interface PanelSizes {
  /** 侧边栏宽度 */
  sidebarWidth: number;
  /** 右侧面板宽度 */
  rightPanelWidth: number;
  /** 底部面板高度 */
  bottomPanelHeight: number;
}

/**
 * 布局预设
 */
export interface LayoutPreset {
  /** 预设 ID */
  id: string;
  /** 预设名称 */
  name: string;
  /** 面板可见性 */
  visibility: PanelVisibility;
  /** 面板尺寸 */
  sizes: PanelSizes;
}

/**
 * 默认布局配置
 */
export const DEFAULT_LAYOUT: LayoutPreset = {
  id: 'default',
  name: '默认布局',
  visibility: {
    sidebar: true,
    rightPanel: false,
    bottomPanel: false,
  },
  sizes: {
    sidebarWidth: 256,
    rightPanelWidth: 300,
    bottomPanelHeight: 200,
  },
};

/**
 * 布局约束
 */
export const LAYOUT_CONSTRAINTS = {
  sidebar: { minWidth: 180, maxWidth: 400 },
  rightPanel: { minWidth: 200, maxWidth: 500 },
  bottomPanel: { minHeight: 100, maxHeight: 400 },
};
```

#### 2. 布局 Store

**位置:** `apps/omninova-tauri/src/stores/layoutStore.ts`

```typescript
import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { LayoutPreset, PanelVisibility, PanelSizes } from '@/types/layout';
import { DEFAULT_LAYOUT } from '@/types/layout';

interface LayoutState {
  currentLayout: LayoutPreset;
  presets: LayoutPreset[];
}

interface LayoutActions {
  setVisibility: (panel: keyof PanelVisibility, visible: boolean) => void;
  setSize: (panel: keyof PanelSizes, size: number) => void;
  togglePanel: (panel: keyof PanelVisibility) => void;
  savePreset: (name: string) => void;
  loadPreset: (id: string) => void;
  deletePreset: (id: string) => void;
  resetToDefault: () => void;
}

export const useLayoutStore = create<LayoutState & LayoutActions>()(
  persist(
    (set, get) => ({
      currentLayout: DEFAULT_LAYOUT,
      presets: [DEFAULT_LAYOUT],

      setVisibility: (panel, visible) => {
        set((state) => ({
          currentLayout: {
            ...state.currentLayout,
            visibility: {
              ...state.currentLayout.visibility,
              [panel]: visible,
            },
          },
        }));
      },

      setSize: (panel, size) => {
        set((state) => ({
          currentLayout: {
            ...state.currentLayout,
            sizes: {
              ...state.currentLayout.sizes,
              [panel]: size,
            },
          },
        }));
      },

      togglePanel: (panel) => {
        const { currentLayout } = get();
        get().setVisibility(panel, !currentLayout.visibility[panel]);
      },

      savePreset: (name) => {
        const { currentLayout, presets } = get();
        const newPreset: LayoutPreset = {
          ...currentLayout,
          id: `preset-${Date.now()}`,
          name,
        };
        set({ presets: [...presets, newPreset] });
      },

      loadPreset: (id) => {
        const { presets } = get();
        const preset = presets.find((p) => p.id === id);
        if (preset) {
          set({ currentLayout: preset });
        }
      },

      deletePreset: (id) => {
        set((state) => ({
          presets: state.presets.filter((p) => p.id !== id),
        }));
      },

      resetToDefault: () => {
        set({ currentLayout: DEFAULT_LAYOUT });
      },
    }),
    {
      name: 'omninova-layout-storage',
    }
  )
);
```

#### 3. 可调整大小面板组件

**位置:** `apps/omninova-tauri/src/components/layout/ResizablePanel.tsx`

```tsx
import { useState, useCallback, useRef, useEffect } from 'react';
import { cn } from '@/lib/utils';
import { GripVertical } from 'lucide-react';

interface ResizablePanelProps {
  children: React.ReactNode;
  width: number;
  minWidth: number;
  maxWidth: number;
  onResize: (width: number) => void;
  side?: 'left' | 'right';
  className?: string;
}

export function ResizablePanel({
  children,
  width,
  minWidth,
  maxWidth,
  onResize,
  side = 'left',
  className,
}: ResizablePanelProps) {
  const [isResizing, setIsResizing] = useState(false);
  const panelRef = useRef<HTMLDivElement>(null);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setIsResizing(true);
  }, []);

  useEffect(() => {
    if (!isResizing) return;

    const handleMouseMove = (e: MouseEvent) => {
      if (!panelRef.current) return;

      const rect = panelRef.current.getBoundingClientRect();
      const newWidth = side === 'left'
        ? e.clientX - rect.left
        : rect.right - e.clientX;

      const clampedWidth = Math.max(minWidth, Math.min(maxWidth, newWidth));
      onResize(clampedWidth);
    };

    const handleMouseUp = () => {
      setIsResizing(false);
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isResizing, minWidth, maxWidth, onResize, side]);

  return (
    <div
      ref={panelRef}
      className={cn('relative flex', className)}
      style={{ width }}
    >
      {children}

      {/* Resize handle */}
      <div
        className={cn(
          'absolute top-0 bottom-0 w-1 cursor-col-resize',
          'hover:bg-primary/20 transition-colors',
          isResizing && 'bg-primary/30',
          side === 'left' ? 'right-0' : 'left-0'
        )}
        onMouseDown={handleMouseDown}
      >
        <GripVertical className="w-1 h-4 absolute top-1/2 -translate-y-1/2 opacity-0 hover:opacity-100" />
      </div>
    </div>
  );
}
```

---

## 架构合规要求

### 文件组织

```
apps/omninova-tauri/src/
├── types/layout.ts
├── stores/layoutStore.ts
└── components/layout/
    ├── ResizablePanel.tsx
    ├── LayoutPresets.tsx
    └── index.ts
```

---

## 完成标准

- [ ] 创建 `types/layout.ts` 类型定义
- [ ] 创建 `stores/layoutStore.ts` 状态管理
- [ ] 创建 `components/layout/ResizablePanel.tsx` 可调整面板
- [ ] 实现侧边栏显示/隐藏
- [ ] 实现面板大小调整
- [ ] 实现布局预设保存/加载
- [ ] 布局持久化
- [ ] 单元测试
- [ ] 更新 sprint-status.yaml

---

## Dev Agent Record

### Agent Model Used

claude-sonnet-4.6

### Debug Log References

None

### Completion Notes List

**Implementation Summary:**
- Created `types/layout.ts` with PanelVisibility, PanelSizes, LayoutPreset types
- Created `stores/layoutStore.ts` with panel visibility and size management
- Created `components/layout/ResizablePanel.tsx` for draggable resize
- Created `components/layout/LayoutPresets.tsx` for preset management
- All 13 unit tests pass

**Technical Decisions:**
- Used localStorage for layout persistence
- Default sidebar width: 256px (range: 180-400)
- Presets stored in store, not separate API
- Resize handle with visual feedback

### File List

**新增文件:**
- `apps/omninova-tauri/src/types/layout.ts`
- `apps/omninova-tauri/src/stores/layoutStore.ts`
- `apps/omninova-tauri/src/stores/layoutStore.test.ts`
- `apps/omninova-tauri/src/components/layout/ResizablePanel.tsx`
- `apps/omninova-tauri/src/components/layout/LayoutPresets.tsx`
- `apps/omninova-tauri/src/components/layout/index.ts`