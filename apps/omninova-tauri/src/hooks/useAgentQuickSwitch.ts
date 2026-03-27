/**
 * Agent Quick Switch Hook
 *
 * Provides keyboard shortcut handling for agent switching.
 * Uses Ctrl/Cmd + 1-9 to switch between agents.
 *
 * [Source: Story 10.1 - 代理快速切换功能]
 */

import { useEffect, useCallback } from 'react';
import { useNavigationStore } from '@/stores/navigationStore';
import { getModifierKey } from '@/types/navigation';
import type { AgentModel } from '@/types/agent';

// ============================================================================
// Types
// ============================================================================

export interface UseAgentQuickSwitchOptions {
  /** List of agents available for switching */
  agents: AgentModel[];
  /** Whether the hook is enabled */
  enabled?: boolean;
}

// ============================================================================
// Hook
// ============================================================================

/**
 * Hook for handling agent quick switch keyboard shortcuts
 *
 * Listens for Ctrl (Windows/Linux) or Cmd (macOS) + 1-9 key combinations
 * and switches to the corresponding agent by index.
 *
 * @example
 * ```tsx
 * function App() {
 *   const agents = useAgentStore((s) => s.agents);
 *
 *   // Enable quick switch globally
 *   useAgentQuickSwitch({ agents });
 *
 *   return <MainLayout />;
 * }
 * ```
 */
export function useAgentQuickSwitch({
  agents,
  enabled = true,
}: UseAgentQuickSwitchOptions): void {
  const setActiveAgent = useNavigationStore((s) => s.setActiveAgent);

  // Handle keyboard events
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      // Skip if disabled
      if (!enabled) return;

      // Get the modifier key for this platform
      const modifierKey = getModifierKey();

      // Check if modifier key is pressed
      if (!e[modifierKey]) return;

      // Parse the number key (1-9)
      const num = parseInt(e.key, 10);
      if (isNaN(num) || num < 1 || num > 9) return;

      // Get agent at index (1-based to 0-based)
      const agentIndex = num - 1;
      const agent = agents[agentIndex];

      if (agent) {
        e.preventDefault();
        setActiveAgent(agent.id);
      }
    },
    [agents, enabled, setActiveAgent]
  );

  // Register event listener
  useEffect(() => {
    if (!enabled) return;

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown, enabled]);
}

export default useAgentQuickSwitch;