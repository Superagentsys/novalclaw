/**
 * Layout Store - Zustand State Management
 *
 * Manages layout state for panel visibility, sizes, and presets.
 *
 * [Source: Story 10.4 - 界面布局自定义]
 */

import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';
import type {
  LayoutState,
  LayoutActions,
  LayoutStore,
  PanelVisibility,
  PanelSizes,
  LayoutPreset,
} from '@/types/layout';
import { DEFAULT_LAYOUT } from '@/types/layout';

// ============================================================================
// Initial State
// ============================================================================

const initialState: LayoutState = {
  currentLayout: DEFAULT_LAYOUT,
  presets: [DEFAULT_LAYOUT],
};

// ============================================================================
// Store Implementation
// ============================================================================

/**
 * Layout store with Zustand
 *
 * Provides state management for:
 * - Panel visibility (show/hide sidebar, right panel, bottom panel)
 * - Panel sizes (width/height)
 * - Layout presets (save/load/delete)
 *
 * @example
 * ```tsx
 * function LayoutControls() {
 *   const { currentLayout, togglePanel, setSize } = useLayoutStore();
 *
 *   return (
 *     <div>
 *       <button onClick={() => togglePanel('sidebar')}>
 *         {currentLayout.visibility.sidebar ? '隐藏' : '显示'}侧边栏
 *       </button>
 *     </div>
 *   );
 * }
 * ```
 */
export const useLayoutStore = create<LayoutStore>()(
  persist(
    (set, get) => ({
      ...initialState,

      setVisibility: (panel: keyof PanelVisibility, visible: boolean) => {
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

      setSize: (panel: keyof PanelSizes, size: number) => {
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

      togglePanel: (panel: keyof PanelVisibility) => {
        const { currentLayout } = get();
        get().setVisibility(panel, !currentLayout.visibility[panel]);
      },

      savePreset: (name: string) => {
        const { currentLayout, presets } = get();
        const newPreset: LayoutPreset = {
          ...currentLayout,
          id: `preset-${Date.now()}`,
          name,
        };
        set({ presets: [...presets, newPreset] });
      },

      loadPreset: (id: string) => {
        const { presets } = get();
        const preset = presets.find((p) => p.id === id);
        if (preset) {
          set({ currentLayout: { ...preset } });
        }
      },

      deletePreset: (id: string) => {
        // Don't allow deleting the default preset
        if (id === 'default') return;

        set((state) => ({
          presets: state.presets.filter((p) => p.id !== id),
        }));
      },

      resetToDefault: () => {
        set({ currentLayout: { ...DEFAULT_LAYOUT } });
      },

      setLayoutName: (name: string) => {
        set((state) => ({
          currentLayout: {
            ...state.currentLayout,
            name,
          },
        }));
      },
    }),
    {
      name: 'omninova-layout-storage',
      storage: createJSONStorage(() => localStorage),
      // Persist layout configuration
      partialize: (state) => ({
        currentLayout: state.currentLayout,
        presets: state.presets,
      }),
    }
  )
);

// ============================================================================
// Selector Hooks
// ============================================================================

/**
 * Select current layout
 */
export const useCurrentLayout = () =>
  useLayoutStore((state) => state.currentLayout);

/**
 * Select panel visibility
 */
export const usePanelVisibility = () =>
  useLayoutStore((state) => state.currentLayout.visibility);

/**
 * Select panel sizes
 */
export const usePanelSizes = () =>
  useLayoutStore((state) => state.currentLayout.sizes);

/**
 * Select presets
 */
export const useLayoutPresets = () =>
  useLayoutStore((state) => state.presets);

/**
 * Check if a panel is visible
 */
export const useIsPanelVisible = (panel: keyof PanelVisibility) =>
  useLayoutStore((state) => state.currentLayout.visibility[panel]);

/**
 * Get a panel size
 */
export const usePanelSize = (panel: keyof PanelSizes) =>
  useLayoutStore((state) => state.currentLayout.sizes[panel]);

export default useLayoutStore;