/**
 * Layout Types
 *
 * Type definitions for layout customization functionality.
 *
 * [Source: Story 10.4 - 界面布局自定义]
 */

// ============================================================================
// Types
// ============================================================================

/**
 * Panel visibility configuration
 */
export interface PanelVisibility {
  /** Left sidebar */
  sidebar: boolean;
  /** Right panel */
  rightPanel: boolean;
  /** Bottom panel */
  bottomPanel: boolean;
}

/**
 * Panel size configuration (in pixels)
 */
export interface PanelSizes {
  /** Sidebar width */
  sidebarWidth: number;
  /** Right panel width */
  rightPanelWidth: number;
  /** Bottom panel height */
  bottomPanelHeight: number;
}

/**
 * Layout preset
 */
export interface LayoutPreset {
  /** Preset ID */
  id: string;
  /** Preset name */
  name: string;
  /** Panel visibility */
  visibility: PanelVisibility;
  /** Panel sizes */
  sizes: PanelSizes;
}

/**
 * Layout state
 */
export interface LayoutState {
  /** Current active layout */
  currentLayout: LayoutPreset;
  /** Saved presets */
  presets: LayoutPreset[];
}

/**
 * Layout actions
 */
export interface LayoutActions {
  /** Set panel visibility */
  setVisibility: (panel: keyof PanelVisibility, visible: boolean) => void;
  /** Set panel size */
  setSize: (panel: keyof PanelSizes, size: number) => void;
  /** Toggle panel visibility */
  togglePanel: (panel: keyof PanelVisibility) => void;
  /** Save current layout as preset */
  savePreset: (name: string) => void;
  /** Load a preset */
  loadPreset: (id: string) => void;
  /** Delete a preset */
  deletePreset: (id: string) => void;
  /** Reset to default layout */
  resetToDefault: () => void;
  /** Update current layout name */
  setLayoutName: (name: string) => void;
}

/**
 * Combined layout store type
 */
export type LayoutStore = LayoutState & LayoutActions;

// ============================================================================
// Constants
// ============================================================================

/**
 * Default layout configuration
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
 * Layout constraints for panel sizing
 */
export const LAYOUT_CONSTRAINTS = {
  sidebar: { minWidth: 180, maxWidth: 400 },
  rightPanel: { minWidth: 200, maxWidth: 500 },
  bottomPanel: { minHeight: 100, maxHeight: 400 },
} as const;

/**
 * Panel display names
 */
export const PANEL_NAMES: Record<keyof PanelVisibility, string> = {
  sidebar: '侧边栏',
  rightPanel: '右侧面板',
  bottomPanel: '底部面板',
};