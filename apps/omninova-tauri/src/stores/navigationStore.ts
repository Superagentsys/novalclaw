/**
 * Navigation Store - Zustand State Management
 *
 * Manages navigation state for agent switching, recent agents, and shortcuts.
 *
 * [Source: Story 10.1 - 代理快速切换功能]
 */

import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';
import type {
  NavigationState,
  NavigationActions,
  NavigationStore,
} from '@/types/navigation';
import { MAX_RECENT_AGENTS } from '@/types/navigation';

// ============================================================================
// Types
// ============================================================================

/**
 * Callback type for loading sessions when agent switches
 */
export type OnAgentSwitchCallback = (agentId: number) => void;

// ============================================================================
// Initial State
// ============================================================================

const initialState: NavigationState = {
  activeAgentId: null,
  recentAgentIds: [],
  agentShortcuts: {},
};

// ============================================================================
// Store Implementation
// ============================================================================

// Store the callback for session loading (set by integration layer)
let onAgentSwitchCallback: OnAgentSwitchCallback | null = null;

/**
 * Set the callback to be called when agent switches
 * This allows the session store to load the most recent session
 */
export function setOnAgentSwitchCallback(callback: OnAgentSwitchCallback | null): void {
  onAgentSwitchCallback = callback;
}

/**
 * Navigation store with Zustand
 *
 * Provides state management for:
 * - Active agent tracking
 * - Recent agents list (max 5)
 * - Keyboard shortcut bindings (Ctrl/Cmd + 1-9)
 *
 * @example
 * ```tsx
 * function AgentList() {
 *   const activeAgentId = useNavigationStore((s) => s.activeAgentId);
 *   const setActiveAgent = useNavigationStore((s) => s.setActiveAgent);
 *
 *   return (
 *     <div>
 *       {agents.map(agent => (
 *         <button
 *           key={agent.id}
 *           onClick={() => setActiveAgent(agent.id)}
 *           className={agent.id === activeAgentId ? 'active' : ''}
 *         >
 *           {agent.name}
 *         </button>
 *       ))}
 *     </div>
 *   );
 * }
 * ```
 */
export const useNavigationStore = create<NavigationStore>()(
  persist(
    (set, get) => ({
      ...initialState,

      setActiveAgent: (agentId: number | null) => {
        set({ activeAgentId: agentId });
        // Record access when switching to an agent
        if (agentId !== null) {
          get().recordAgentAccess(agentId);
          // Trigger callback for session loading (if registered)
          if (onAgentSwitchCallback) {
            onAgentSwitchCallback(agentId);
          }
        }
      },

      recordAgentAccess: (agentId: number) => {
        set((state) => {
          // Remove agent from current position and add to front
          const filtered = state.recentAgentIds.filter((id) => id !== agentId);
          const recentAgentIds = [agentId, ...filtered].slice(
            0,
            MAX_RECENT_AGENTS
          );
          return { recentAgentIds };
        });
      },

      setAgentShortcut: (index: number, agentId: number | null) => {
        set((state) => {
          const shortcuts = { ...state.agentShortcuts };

          if (agentId === null) {
            // Remove shortcut binding
            delete shortcuts[index];
          } else {
            // Bind shortcut
            shortcuts[index] = agentId;
          }

          return { agentShortcuts: shortcuts };
        });
      },

      getAgentShortcutIndex: (agentId: number): number | undefined => {
        const { agentShortcuts } = get();
        for (const [index, id] of Object.entries(agentShortcuts)) {
          if (id === agentId) {
            return parseInt(index, 10);
          }
        }
        return undefined;
      },

      reset: () => set(initialState),
    }),
    {
      name: 'omninova-navigation-storage',
      storage: createJSONStorage(() => localStorage),
      // Persist navigation preferences
      partialize: (state) => ({
        activeAgentId: state.activeAgentId,
        recentAgentIds: state.recentAgentIds,
        agentShortcuts: state.agentShortcuts,
      }),
    }
  )
);

// ============================================================================
// Selector Hooks
// ============================================================================

/**
 * Select active agent ID
 */
export const useActiveAgentId = () =>
  useNavigationStore((state) => state.activeAgentId);

/**
 * Select recent agent IDs
 */
export const useRecentAgentIds = () =>
  useNavigationStore((state) => state.recentAgentIds);

/**
 * Select agent shortcuts
 */
export const useAgentShortcuts = () =>
  useNavigationStore((state) => state.agentShortcuts);

export default useNavigationStore;