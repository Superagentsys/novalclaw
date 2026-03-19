/**
 * Tests for SessionStore
 *
 * [Source: Story 4.7 - 对话历史持久化与导航]
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useSessionStore } from '../sessionStore';

// Mock Tauri invoke
const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

// Mock localStorage
const localStorageMock = (() => {
  let store: Record<string, string> = {};
  return {
    getItem: vi.fn((key: string) => store[key] || null),
    setItem: vi.fn((key: string, value: string) => {
      store[key] = value;
    }),
    removeItem: vi.fn((key: string) => {
      delete store[key];
    }),
    clear: vi.fn(() => {
      store = {};
    }),
  };
})();

Object.defineProperty(window, 'localStorage', {
  value: localStorageMock,
});

describe('SessionStore', () => {
  beforeEach(() => {
    // Reset store state before each test
    useSessionStore.setState({
      sessions: [],
      activeSessionId: null,
      activeAgentId: null,
      isLoading: false,
      error: null,
    });
    localStorageMock.clear();
    mockInvoke.mockReset();
  });

  describe('Initial state', () => {
    it('has correct initial state', () => {
      const state = useSessionStore.getState();

      expect(state.sessions).toEqual([]);
      expect(state.activeSessionId).toBeNull();
      expect(state.activeAgentId).toBeNull();
      expect(state.isLoading).toBe(false);
      expect(state.error).toBeNull();
    });
  });

  describe('loadSessions', () => {
    it('loads sessions for an agent', async () => {
      const mockSessions = [
        { id: 1, agentId: 1, title: 'Session 1', createdAt: 1000, updatedAt: 2000 },
        { id: 2, agentId: 1, title: 'Session 2', createdAt: 1001, updatedAt: 2001 },
      ];

      mockInvoke.mockResolvedValueOnce(mockSessions);

      await act(async () => {
        await useSessionStore.getState().loadSessions(1);
      });

      expect(mockInvoke).toHaveBeenCalledWith('list_sessions_by_agent', { agentId: 1 });
      const state = useSessionStore.getState();
      expect(state.sessions).toEqual(mockSessions);
      expect(state.activeAgentId).toBe(1);
      expect(state.isLoading).toBe(false);
    });

    it('sets loading state during load', async () => {
      mockInvoke.mockImplementation(() => new Promise((resolve) => setTimeout(resolve, 100)));

      act(() => {
        useSessionStore.getState().loadSessions(1);
      });

      expect(useSessionStore.getState().isLoading).toBe(true);
    });

    it('handles errors when loading sessions', async () => {
      const errorMessage = 'Failed to load sessions';
      mockInvoke.mockRejectedValueOnce(new Error(errorMessage));

      await act(async () => {
        await useSessionStore.getState().loadSessions(1);
      });

      const state = useSessionStore.getState();
      expect(state.error).toBe(errorMessage);
      expect(state.isLoading).toBe(false);
    });

    it('handles non-Error errors', async () => {
      mockInvoke.mockRejectedValueOnce('String error');

      await act(async () => {
        await useSessionStore.getState().loadSessions(1);
      });

      expect(useSessionStore.getState().error).toBe('String error');
    });
  });

  describe('loadSession', () => {
    it('loads a single session by ID', async () => {
      const mockSession = { id: 1, agentId: 1, title: 'Test Session', createdAt: 1000, updatedAt: 2000 };

      mockInvoke.mockResolvedValueOnce(mockSession);

      let loadedSession;
      await act(async () => {
        loadedSession = await useSessionStore.getState().loadSession(1);
      });

      expect(mockInvoke).toHaveBeenCalledWith('get_session', { sessionId: 1 });
      expect(loadedSession).toEqual(mockSession);
    });

    it('returns null if session not found', async () => {
      mockInvoke.mockResolvedValueOnce(null);

      let loadedSession;
      await act(async () => {
        loadedSession = await useSessionStore.getState().loadSession(999);
      });

      expect(loadedSession).toBeNull();
    });

    it('sets error on loadSession failure', async () => {
      const errorMessage = 'Session not found';
      mockInvoke.mockRejectedValueOnce(new Error(errorMessage));

      await act(async () => {
        await useSessionStore.getState().loadSession(999);
      });

      expect(useSessionStore.getState().error).toBe(errorMessage);
    });
  });

  describe('createSession', () => {
    it('creates a new session', async () => {
      const newSession = { id: 3, agentId: 1, title: 'New Session', createdAt: 3000, updatedAt: 3000 };

      mockInvoke.mockResolvedValueOnce(newSession);

      let createdSession;
      await act(async () => {
        createdSession = await useSessionStore.getState().createSession(1, 'New Session');
      });

      expect(mockInvoke).toHaveBeenCalledWith('create_session', {
        newSession: { agentId: 1, title: 'New Session' },
      });
      expect(createdSession).toEqual(newSession);
      const state = useSessionStore.getState();
      expect(state.sessions).toContainEqual(newSession);
      expect(state.activeSessionId).toBe(3);
    });

    it('creates session without title', async () => {
      const newSession = { id: 4, agentId: 1, title: undefined, createdAt: 4000, updatedAt: 4000 };

      mockInvoke.mockResolvedValueOnce(newSession);

      await act(async () => {
        await useSessionStore.getState().createSession(1);
      });

      expect(mockInvoke).toHaveBeenCalledWith('create_session', {
        newSession: { agentId: 1, title: undefined },
      });
    });

    it('prepends new session to list', async () => {
      const existingSession = { id: 1, agentId: 1, title: 'Existing', createdAt: 1000, updatedAt: 1000 };
      const newSession = { id: 2, agentId: 1, title: 'New', createdAt: 2000, updatedAt: 2000 };

      useSessionStore.setState({ sessions: [existingSession] });
      mockInvoke.mockResolvedValueOnce(newSession);

      await act(async () => {
        await useSessionStore.getState().createSession(1, 'New');
      });

      const state = useSessionStore.getState();
      expect(state.sessions[0]).toEqual(newSession);
      expect(state.sessions[1]).toEqual(existingSession);
    });

    it('sets loading state during creation', async () => {
      mockInvoke.mockImplementation(() => new Promise((resolve) => setTimeout(resolve, 100)));

      act(() => {
        useSessionStore.getState().createSession(1, 'Test');
      });

      expect(useSessionStore.getState().isLoading).toBe(true);
    });

    it('handles errors during creation', async () => {
      const errorMessage = 'Failed to create session';
      mockInvoke.mockRejectedValueOnce(new Error(errorMessage));

      await act(async () => {
        try {
          await useSessionStore.getState().createSession(1, 'Test');
        } catch {
          // Expected to throw
        }
      });

      const state = useSessionStore.getState();
      expect(state.error).toBe(errorMessage);
      expect(state.isLoading).toBe(false);
    });

    it('throws error on creation failure', async () => {
      const errorMessage = 'Database error';
      mockInvoke.mockRejectedValueOnce(new Error(errorMessage));

      await expect(
        useSessionStore.getState().createSession(1, 'Test')
      ).rejects.toThrow(errorMessage);
    });
  });

  describe('switchSession', () => {
    it('switches to an existing session', () => {
      const sessions = [
        { id: 1, agentId: 1, title: 'Session 1', createdAt: 1000, updatedAt: 1000 },
        { id: 2, agentId: 1, title: 'Session 2', createdAt: 2000, updatedAt: 2000 },
      ];

      useSessionStore.setState({ sessions });

      act(() => {
        useSessionStore.getState().switchSession(2);
      });

      expect(useSessionStore.getState().activeSessionId).toBe(2);
    });

    it('does not switch if session does not exist', () => {
      const sessions = [
        { id: 1, agentId: 1, title: 'Session 1', createdAt: 1000, updatedAt: 1000 },
      ];

      useSessionStore.setState({ sessions, activeSessionId: 1 });

      act(() => {
        useSessionStore.getState().switchSession(999);
      });

      expect(useSessionStore.getState().activeSessionId).toBe(1); // Unchanged
    });

    it('updates activeAgentId when switching session', () => {
      const sessions = [
        { id: 1, agentId: 1, title: 'Session 1', createdAt: 1000, updatedAt: 1000 },
        { id: 2, agentId: 2, title: 'Session 2', createdAt: 2000, updatedAt: 2000 },
      ];

      useSessionStore.setState({ sessions, activeAgentId: 1 });

      act(() => {
        useSessionStore.getState().switchSession(2);
      });

      expect(useSessionStore.getState().activeAgentId).toBe(2);
    });
  });

  describe('deleteSession', () => {
    it('deletes a session', async () => {
      const sessions = [
        { id: 1, agentId: 1, title: 'Session 1', createdAt: 1000, updatedAt: 1000 },
        { id: 2, agentId: 1, title: 'Session 2', createdAt: 2000, updatedAt: 2000 },
      ];

      useSessionStore.setState({ sessions, activeSessionId: 2 });
      mockInvoke.mockResolvedValueOnce(undefined);

      await act(async () => {
        await useSessionStore.getState().deleteSession(2);
      });

      expect(mockInvoke).toHaveBeenCalledWith('delete_session', { sessionId: 2 });
      const state = useSessionStore.getState();
      expect(state.sessions).toHaveLength(1);
      expect(state.sessions[0].id).toBe(1);
    });

    it('switches to first session if active session is deleted', async () => {
      const sessions = [
        { id: 1, agentId: 1, title: 'Session 1', createdAt: 1000, updatedAt: 1000 },
        { id: 2, agentId: 1, title: 'Session 2', createdAt: 2000, updatedAt: 2000 },
      ];

      useSessionStore.setState({ sessions, activeSessionId: 2 });
      mockInvoke.mockResolvedValueOnce(undefined);

      await act(async () => {
        await useSessionStore.getState().deleteSession(2);
      });

      expect(useSessionStore.getState().activeSessionId).toBe(1);
    });

    it('sets activeSessionId to null if last session is deleted', async () => {
      const sessions = [
        { id: 1, agentId: 1, title: 'Session 1', createdAt: 1000, updatedAt: 1000 },
      ];

      useSessionStore.setState({ sessions, activeSessionId: 1 });
      mockInvoke.mockResolvedValueOnce(undefined);

      await act(async () => {
        await useSessionStore.getState().deleteSession(1);
      });

      expect(useSessionStore.getState().activeSessionId).toBeNull();
    });

    it('does not change activeSessionId if deleting non-active session', async () => {
      const sessions = [
        { id: 1, agentId: 1, title: 'Session 1', createdAt: 1000, updatedAt: 1000 },
        { id: 2, agentId: 1, title: 'Session 2', createdAt: 2000, updatedAt: 2000 },
      ];

      useSessionStore.setState({ sessions, activeSessionId: 1 });
      mockInvoke.mockResolvedValueOnce(undefined);

      await act(async () => {
        await useSessionStore.getState().deleteSession(2);
      });

      expect(useSessionStore.getState().activeSessionId).toBe(1);
    });

    it('handles errors during deletion', async () => {
      const errorMessage = 'Failed to delete session';
      mockInvoke.mockRejectedValueOnce(new Error(errorMessage));

      await act(async () => {
        try {
          await useSessionStore.getState().deleteSession(1);
        } catch {
          // Expected to throw
        }
      });

      expect(useSessionStore.getState().error).toBe(errorMessage);
    });

    it('throws error on deletion failure', async () => {
      const errorMessage = 'Database error';
      mockInvoke.mockRejectedValueOnce(new Error(errorMessage));

      await expect(
        useSessionStore.getState().deleteSession(1)
      ).rejects.toThrow(errorMessage);
    });
  });

  describe('updateSessionTitle', () => {
    it('updates session title', async () => {
      const sessions = [
        { id: 1, agentId: 1, title: 'Old Title', createdAt: 1000, updatedAt: 1000 },
      ];

      const updatedSession = { id: 1, agentId: 1, title: 'New Title', createdAt: 1000, updatedAt: 2000 };

      useSessionStore.setState({ sessions });
      mockInvoke.mockResolvedValueOnce(updatedSession);

      await act(async () => {
        await useSessionStore.getState().updateSessionTitle(1, 'New Title');
      });

      expect(mockInvoke).toHaveBeenCalledWith('update_session', { sessionId: 1, updates: { title: 'New Title' } });
      expect(useSessionStore.getState().sessions[0].title).toBe('New Title');
    });

    it('handles errors during title update', async () => {
      const errorMessage = 'Failed to update session';
      mockInvoke.mockRejectedValueOnce(new Error(errorMessage));

      await act(async () => {
        try {
          await useSessionStore.getState().updateSessionTitle(1, 'New Title');
        } catch {
          // Expected to throw
        }
      });

      expect(useSessionStore.getState().error).toBe(errorMessage);
    });

    it('throws error on update failure', async () => {
      const errorMessage = 'Database error';
      mockInvoke.mockRejectedValueOnce(new Error(errorMessage));

      await expect(
        useSessionStore.getState().updateSessionTitle(1, 'New Title')
      ).rejects.toThrow(errorMessage);
    });
  });

  describe('setActiveSession', () => {
    it('sets active session ID', () => {
      act(() => {
        useSessionStore.getState().setActiveSession(5);
      });

      expect(useSessionStore.getState().activeSessionId).toBe(5);
    });

    it('allows setting null', () => {
      useSessionStore.setState({ activeSessionId: 1 });

      act(() => {
        useSessionStore.getState().setActiveSession(null);
      });

      expect(useSessionStore.getState().activeSessionId).toBeNull();
    });
  });

  describe('clearSessions', () => {
    it('clears all sessions', () => {
      const sessions = [
        { id: 1, agentId: 1, title: 'Session 1', createdAt: 1000, updatedAt: 1000 },
      ];

      useSessionStore.setState({ sessions, activeSessionId: 1, activeAgentId: 1 });

      act(() => {
        useSessionStore.getState().clearSessions();
      });

      const state = useSessionStore.getState();
      expect(state.sessions).toEqual([]);
      expect(state.activeSessionId).toBeNull();
      expect(state.activeAgentId).toBeNull();
    });
  });

  describe('setLoading', () => {
    it('sets loading state', () => {
      act(() => {
        useSessionStore.getState().setLoading(true);
      });

      expect(useSessionStore.getState().isLoading).toBe(true);

      act(() => {
        useSessionStore.getState().setLoading(false);
      });

      expect(useSessionStore.getState().isLoading).toBe(false);
    });
  });

  describe('setError', () => {
    it('sets error state', () => {
      act(() => {
        useSessionStore.getState().setError('Test error');
      });

      expect(useSessionStore.getState().error).toBe('Test error');

      act(() => {
        useSessionStore.getState().setError(null);
      });

      expect(useSessionStore.getState().error).toBeNull();
    });
  });

  describe('reset', () => {
    it('resets state to initial values', () => {
      const sessions = [
        { id: 1, agentId: 1, title: 'Session 1', createdAt: 1000, updatedAt: 1000 },
      ];

      useSessionStore.setState({
        sessions,
        activeSessionId: 1,
        activeAgentId: 1,
        isLoading: true,
        error: 'Some error',
      });

      act(() => {
        useSessionStore.getState().reset();
      });

      const state = useSessionStore.getState();
      expect(state.sessions).toEqual([]);
      expect(state.activeSessionId).toBeNull();
      expect(state.activeAgentId).toBeNull();
      expect(state.isLoading).toBe(false);
      expect(state.error).toBeNull();
    });
  });

  describe('Selector hooks', () => {
    it('useSessionsCount returns correct count', () => {
      const sessions = [
        { id: 1, agentId: 1, title: 'S1', createdAt: 1000, updatedAt: 1000 },
        { id: 2, agentId: 1, title: 'S2', createdAt: 2000, updatedAt: 2000 },
      ];

      useSessionStore.setState({ sessions });

      const { result } = renderHook(() => useSessionStore((s) => s.sessions.length));

      expect(result.current).toBe(2);
    });

    it('useActiveSession returns active session', () => {
      const sessions = [
        { id: 1, agentId: 1, title: 'S1', createdAt: 1000, updatedAt: 1000 },
        { id: 2, agentId: 1, title: 'S2', createdAt: 2000, updatedAt: 2000 },
      ];

      useSessionStore.setState({ sessions, activeSessionId: 2 });

      const { result } = renderHook(() => {
        const { sessions, activeSessionId } = useSessionStore();
        return activeSessionId ? sessions.find((s) => s.id === activeSessionId) || null : null;
      });

      expect(result.current).toEqual(sessions[1]);
    });

    it('useActiveSession returns null if no active session', () => {
      const { result } = renderHook(() => {
        const { sessions, activeSessionId } = useSessionStore();
        return activeSessionId ? sessions.find((s) => s.id === activeSessionId) || null : null;
      });

      expect(result.current).toBeNull();
    });
  });
});