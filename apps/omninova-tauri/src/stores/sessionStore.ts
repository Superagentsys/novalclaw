/**
 * Session Store
 *
 * Zustand store for managing conversation sessions including
 * session list, active session, and session CRUD operations.
 *
 * [Source: Story 4.7 - 对话历史持久化与导航]
 */

import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';
import { invoke } from '@tauri-apps/api/core';
import { type Session, type NewSession } from '@/types/session';

// ============================================================================
// Types
// ============================================================================

/**
 * Session store state
 */
export interface SessionState {
  /** List of sessions for the current agent */
  sessions: Session[];
  /** Currently active session ID */
  activeSessionId: number | null;
  /** Currently active agent ID (for which sessions are loaded) */
  activeAgentId: number | null;
  /** Loading state for session operations */
  isLoading: boolean;
  /** Error message if any */
  error: string | null;
}

/**
 * Session store actions
 */
export interface SessionActions {
  /** Load sessions for a specific agent */
  loadSessions: (agentId: number) => Promise<void>;
  /** Load a single session by ID */
  loadSession: (sessionId: number) => Promise<Session | null>;
  /** Create a new session */
  createSession: (agentId: number, title?: string) => Promise<Session>;
  /** Switch to a different session */
  switchSession: (sessionId: number) => void;
  /** Delete a session */
  deleteSession: (sessionId: number) => Promise<void>;
  /** Update session title */
  updateSessionTitle: (sessionId: number, title: string) => Promise<void>;
  /** Set active session directly */
  setActiveSession: (sessionId: number | null) => void;
  /** Clear all session data */
  clearSessions: () => void;
  /** Set loading state */
  setLoading: (isLoading: boolean) => void;
  /** Set error state */
  setError: (error: string | null) => void;
  /** Reset store state */
  reset: () => void;
}

/**
 * Combined store type
 */
export type SessionStore = SessionState & SessionActions;

// ============================================================================
// Initial State
// ============================================================================

const initialState: SessionState = {
  sessions: [],
  activeSessionId: null,
  activeAgentId: null,
  isLoading: false,
  error: null,
};

// ============================================================================
// Tauri Command Invokers
// ============================================================================

/**
 * Invoke Tauri command to list sessions by agent
 */
async function invokeListSessions(agentId: number): Promise<Session[]> {
  return invoke<Session[]>('list_sessions_by_agent', { agentId });
}

/**
 * Invoke Tauri command to create a session
 */
async function invokeCreateSession(newSession: NewSession): Promise<Session> {
  return invoke<Session>('create_session', { newSession });
}

/**
 * Invoke Tauri command to delete a session
 */
async function invokeDeleteSession(sessionId: number): Promise<void> {
  return invoke('delete_session', { sessionId });
}

/**
 * Invoke Tauri command to update a session
 */
async function invokeUpdateSession(sessionId: number, title: string): Promise<Session> {
  return invoke<Session>('update_session', { sessionId, updates: { title } });
}

/**
 * Invoke Tauri command to get a session by ID
 */
async function invokeGetSession(sessionId: number): Promise<Session | null> {
  return invoke<Session | null>('get_session', { sessionId });
}

// ============================================================================
// Store
// ============================================================================

/**
 * Session store hook
 *
 * Provides state management for session functionality including:
 * - Session list management (load, create, delete)
 * - Active session tracking
 * - Loading and error states
 * - Persistence to localStorage for caching
 *
 * @example
 * ```tsx
 * function SessionListContainer() {
 *   const { sessions, activeSessionId, loadSessions, switchSession } = useSessionStore();
 *
 *   useEffect(() => {
 *     loadSessions(agentId);
 *   }, [agentId]);
 *
 *   return (
 *     <ul>
 *       {sessions.map(session => (
 *         <li key={session.id} onClick={() => switchSession(session.id)}>
 *           {session.title || '新对话'}
 *         </li>
 *       ))}
 *     </ul>
 *   );
 * }
 * ```
 */
export const useSessionStore = create<SessionStore>()(
  persist(
    (set, get) => ({
      ...initialState,

      loadSessions: async (agentId: number) => {
        set({ isLoading: true, error: null });
        try {
          const sessions = await invokeListSessions(agentId);
          set({
            sessions,
            activeAgentId: agentId,
            isLoading: false,
          });
        } catch (error) {
          set({
            error: error instanceof Error ? error.message : String(error),
            isLoading: false,
          });
        }
      },

      loadSession: async (sessionId: number) => {
        try {
          const session = await invokeGetSession(sessionId);
          return session;
        } catch (error) {
          set({
            error: error instanceof Error ? error.message : String(error),
          });
          return null;
        }
      },

      createSession: async (agentId: number, title?: string) => {
        set({ isLoading: true, error: null });
        try {
          const newSession: NewSession = { agentId, title };
          const session = await invokeCreateSession(newSession);

          // Add to sessions list
          set((state) => ({
            sessions: [session, ...state.sessions],
            activeSessionId: session.id,
            activeAgentId: agentId,
            isLoading: false,
          }));

          return session;
        } catch (error) {
          set({
            error: error instanceof Error ? error.message : String(error),
            isLoading: false,
          });
          throw error;
        }
      },

      switchSession: (sessionId: number) => {
        const { sessions } = get();
        const session = sessions.find((s) => s.id === sessionId);
        if (session) {
          set({
            activeSessionId: sessionId,
            activeAgentId: session.agentId,
          });
        }
      },

      deleteSession: async (sessionId: number) => {
        set({ isLoading: true, error: null });
        try {
          await invokeDeleteSession(sessionId);

          // Remove from sessions list
          set((state) => {
            const newSessions = state.sessions.filter((s) => s.id !== sessionId);
            const newActiveSessionId =
              state.activeSessionId === sessionId
                ? newSessions.length > 0
                  ? newSessions[0].id
                  : null
                : state.activeSessionId;

            return {
              sessions: newSessions,
              activeSessionId: newActiveSessionId,
              isLoading: false,
            };
          });
        } catch (error) {
          set({
            error: error instanceof Error ? error.message : String(error),
            isLoading: false,
          });
          throw error;
        }
      },

      updateSessionTitle: async (sessionId: number, title: string) => {
        try {
          const updatedSession = await invokeUpdateSession(sessionId, title);

          // Update in sessions list
          set((state) => ({
            sessions: state.sessions.map((s) =>
              s.id === sessionId ? updatedSession : s
            ),
          }));
        } catch (error) {
          set({
            error: error instanceof Error ? error.message : String(error),
          });
          throw error;
        }
      },

      setActiveSession: (sessionId: number | null) => {
        set({ activeSessionId: sessionId });
      },

      clearSessions: () => {
        set({
          sessions: [],
          activeSessionId: null,
          activeAgentId: null,
        });
      },

      setLoading: (isLoading: boolean) => set({ isLoading }),

      setError: (error: string | null) => set({ error }),

      reset: () => set(initialState),
    }),
    {
      name: 'omninova-session-storage',
      storage: createJSONStorage(() => localStorage),
      // Only persist certain fields for caching
      partialize: (state) => ({
        sessions: state.sessions,
        activeSessionId: state.activeSessionId,
        activeAgentId: state.activeAgentId,
      }),
    }
  )
);

// ============================================================================
// Selector Hooks
// ============================================================================

/**
 * Select sessions count
 */
export const useSessionsCount = () =>
  useSessionStore((state) => state.sessions.length);

/**
 * Select active session
 */
export const useActiveSession = () =>
  useSessionStore((state) => {
    const { sessions, activeSessionId } = state;
    return activeSessionId
      ? sessions.find((s) => s.id === activeSessionId) || null
      : null;
  });

/**
 * Select if currently loading
 */
export const useIsLoadingSessions = () =>
  useSessionStore((state) => state.isLoading);

/**
 * Select session error
 */
export const useSessionError = () =>
  useSessionStore((state) => state.error);

/**
 * Select sessions for active agent
 */
export const useActiveAgentSessions = () =>
  useSessionStore((state) => state.sessions);

export default useSessionStore;