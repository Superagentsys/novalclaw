/**
 * Navigation Store Tests
 *
 * Unit tests for navigation store functionality.
 *
 * [Source: Story 10.1 - 代理快速切换功能]
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { useNavigationStore } from './navigationStore';
import { MAX_RECENT_AGENTS } from '@/types/navigation';

describe('NavigationStore', () => {
  beforeEach(() => {
    // Reset store before each test
    useNavigationStore.setState({
      activeAgentId: null,
      recentAgentIds: [],
      agentShortcuts: {},
    });
  });

  describe('setActiveAgent', () => {
    it('should set active agent', () => {
      const { setActiveAgent } = useNavigationStore.getState();
      setActiveAgent(1);

      expect(useNavigationStore.getState().activeAgentId).toBe(1);
    });

    it('should record agent access when setting active', () => {
      const { setActiveAgent } = useNavigationStore.getState();
      setActiveAgent(1);

      expect(useNavigationStore.getState().recentAgentIds).toContain(1);
    });

    it('should allow setting null', () => {
      const { setActiveAgent } = useNavigationStore.getState();
      setActiveAgent(1);
      setActiveAgent(null);

      expect(useNavigationStore.getState().activeAgentId).toBeNull();
    });
  });

  describe('recordAgentAccess', () => {
    it('should add agent to recent list', () => {
      const { recordAgentAccess } = useNavigationStore.getState();
      recordAgentAccess(1);

      expect(useNavigationStore.getState().recentAgentIds).toEqual([1]);
    });

    it('should move existing agent to front', () => {
      const { recordAgentAccess } = useNavigationStore.getState();
      recordAgentAccess(1);
      recordAgentAccess(2);
      recordAgentAccess(1);

      expect(useNavigationStore.getState().recentAgentIds).toEqual([1, 2]);
    });

    it('should limit recent agents to MAX_RECENT_AGENTS', () => {
      const { recordAgentAccess } = useNavigationStore.getState();

      // Add more than max
      for (let i = 1; i <= MAX_RECENT_AGENTS + 2; i++) {
        recordAgentAccess(i);
      }

      const { recentAgentIds } = useNavigationStore.getState();
      expect(recentAgentIds).toHaveLength(MAX_RECENT_AGENTS);
      // Oldest should be removed
      expect(recentAgentIds).not.toContain(1);
      expect(recentAgentIds).not.toContain(2);
    });

    it('should not have duplicates', () => {
      const { recordAgentAccess } = useNavigationStore.getState();
      recordAgentAccess(1);
      recordAgentAccess(1);
      recordAgentAccess(1);

      const { recentAgentIds } = useNavigationStore.getState();
      expect(recentAgentIds).toEqual([1]);
    });
  });

  describe('setAgentShortcut', () => {
    it('should set agent shortcut', () => {
      const { setAgentShortcut } = useNavigationStore.getState();
      setAgentShortcut(1, 42);

      expect(useNavigationStore.getState().agentShortcuts[1]).toBe(42);
    });

    it('should remove shortcut when agentId is null', () => {
      const { setAgentShortcut } = useNavigationStore.getState();
      setAgentShortcut(1, 42);
      setAgentShortcut(1, null);

      expect(useNavigationStore.getState().agentShortcuts[1]).toBeUndefined();
    });
  });

  describe('getAgentShortcutIndex', () => {
    it('should return shortcut index for agent', () => {
      const { setAgentShortcut, getAgentShortcutIndex } =
        useNavigationStore.getState();
      setAgentShortcut(3, 42);

      expect(getAgentShortcutIndex(42)).toBe(3);
    });

    it('should return undefined for unbound agent', () => {
      const { getAgentShortcutIndex } = useNavigationStore.getState();
      expect(getAgentShortcutIndex(999)).toBeUndefined();
    });
  });

  describe('reset', () => {
    it('should reset all state to initial values', () => {
      const { setActiveAgent, setAgentShortcut, reset } =
        useNavigationStore.getState();

      setActiveAgent(1);
      setAgentShortcut(1, 42);
      reset();

      const state = useNavigationStore.getState();
      expect(state.activeAgentId).toBeNull();
      expect(state.recentAgentIds).toEqual([]);
      expect(state.agentShortcuts).toEqual({});
    });
  });
});