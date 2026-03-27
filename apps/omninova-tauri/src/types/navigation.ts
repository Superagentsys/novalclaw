/**
 * Navigation Types
 *
 * Type definitions for navigation and agent switching functionality.
 *
 * [Source: Story 10.1 - 代理快速切换功能]
 */

// ============================================================================
// Types
// ============================================================================

/**
 * Navigation store state
 */
export interface NavigationState {
  /** Currently active agent ID */
  activeAgentId: number | null;
  /** Recently used agent IDs (max 5) */
  recentAgentIds: number[];
  /** Agent shortcut mappings (index -> agentId) */
  agentShortcuts: Record<number, number>;
}

/**
 * Navigation store actions
 */
export interface NavigationActions {
  /** Set the currently active agent */
  setActiveAgent: (agentId: number | null) => void;
  /** Record agent access (updates recent list) */
  recordAgentAccess: (agentId: number) => void;
  /** Set agent shortcut binding */
  setAgentShortcut: (index: number, agentId: number | null) => void;
  /** Get shortcut index for an agent */
  getAgentShortcutIndex: (agentId: number) => number | undefined;
  /** Reset navigation state */
  reset: () => void;
}

/**
 * Combined navigation store type
 */
export type NavigationStore = NavigationState & NavigationActions;

// ============================================================================
// Constants
// ============================================================================

/**
 * Maximum number of recent agents to track
 */
export const MAX_RECENT_AGENTS = 5;

/**
 * Maximum shortcut index (1-9)
 */
export const MAX_SHORTCUT_INDEX = 9;

/**
 * Keyboard modifier key for shortcuts
 * Cmd on macOS, Ctrl on Windows/Linux
 */
export const getModifierKey = (): 'metaKey' | 'ctrlKey' => {
  if (typeof navigator === 'undefined') return 'ctrlKey';
  return navigator.platform.toUpperCase().indexOf('MAC') >= 0
    ? 'metaKey'
    : 'ctrlKey';
};