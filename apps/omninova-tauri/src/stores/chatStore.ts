/**
 * Chat Store
 *
 * Zustand store for managing chat state including messages,
 * sessions, loading states, and streaming status.
 *
 * [Source: Story 4.4 - ChatInterface 组件基础]
 */

import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';
import { type Message, type MessageRole } from '@/types/session';

// ============================================================================
// Types
// ============================================================================

/**
 * Chat store state
 */
export interface ChatState {
  /** Messages for the active session */
  messages: Message[];
  /** Currently active session ID */
  activeSessionId: number | null;
  /** Currently active agent ID */
  activeAgentId: number | null;
  /** Loading state for API calls */
  isLoading: boolean;
  /** Error message if any */
  error: string | null;
  /** Streaming state */
  isStreaming: boolean;
  /** Streamed content during active stream */
  streamedContent: string;
  /** Reasoning content during streaming (for thinking models) */
  reasoningContent: string;
  /**
   * Currently quoted message for reply
   *
   * When set, the message will be displayed in QuoteCard above the input
   * and included in the context when sending a reply.
   */
  quoteMessage: Message | null;
}

/**
 * Chat store actions
 */
export interface ChatActions {
  /** Set messages for the active session */
  setMessages: (messages: Message[]) => void;
  /** Add a new message to the list */
  addMessage: (message: Message) => void;
  /** Update an existing message by ID */
  updateMessage: (id: number, content: string) => void;
  /** Clear all messages */
  clearMessages: () => void;
  /** Set active session */
  setActiveSession: (sessionId: number | null, agentId?: number | null) => void;
  /** Set loading state */
  setLoading: (isLoading: boolean) => void;
  /** Set error state */
  setError: (error: string | null) => void;
  /** Start streaming */
  startStreaming: () => void;
  /** Append to streamed content */
  appendStreamedContent: (content: string, reasoning?: string) => void;
  /** Set full streamed content */
  setStreamedContent: (content: string, reasoning?: string) => void;
  /** Stop streaming and optionally save as message */
  stopStreaming: (saveAsMessage?: boolean) => void;
  /**
   * Set the message to be quoted in reply
   *
   * When a message is quoted, it will appear in QuoteCard above the input
   * and be included as context when sending the reply.
   */
  setQuoteMessage: (message: Message | null) => void;
  /**
   * Clear the quoted message
   *
   * Resets quoteMessage to null, removing the QuoteCard display.
   */
  clearQuoteMessage: () => void;
  /** Reset store state */
  reset: () => void;
}

/**
 * Combined store type
 */
export type ChatStore = ChatState & ChatActions;

// ============================================================================
// Initial State
// ============================================================================

const initialState: ChatState = {
  messages: [],
  activeSessionId: null,
  activeAgentId: null,
  isLoading: false,
  error: null,
  isStreaming: false,
  streamedContent: '',
  reasoningContent: '',
  quoteMessage: null,
};

// ============================================================================
// Store
// ============================================================================

/**
 * Chat store hook
 *
 * Provides state management for chat functionality including:
 * - Message management (add, update, clear)
 * - Session tracking
 * - Loading and error states
 * - Streaming content management
 * - Persistence to localStorage
 *
 * @example
 * ```tsx
 * function ChatContainer() {
 *   const { messages, addMessage, isStreaming, startStreaming } = useChatStore();
 *
 *   const handleSend = async (content: string) => {
 *     addMessage({ id: Date.now(), role: 'user', content, createdAt: Date.now() / 1000 });
 *     startStreaming();
 *     // ... API call
 *   };
 * }
 * ```
 */
export const useChatStore = create<ChatStore>()(
  persist(
    (set, get) => ({
      ...initialState,

      setMessages: (messages) => set({ messages }),

      addMessage: (message) =>
        set((state) => ({
          messages: [...state.messages, message],
        })),

      updateMessage: (id, content) =>
        set((state) => ({
          messages: state.messages.map((msg) =>
            msg.id === id ? { ...msg, content } : msg
          ),
        })),

      clearMessages: () => set({ messages: [] }),

      setActiveSession: (sessionId, agentId) =>
        set({
          activeSessionId: sessionId,
          activeAgentId: agentId ?? null,
        }),

      setLoading: (isLoading) => set({ isLoading }),

      setError: (error) => set({ error }),

      startStreaming: () =>
        set({
          isStreaming: true,
          streamedContent: '',
          reasoningContent: '',
        }),

      appendStreamedContent: (content, reasoning) =>
        set((state) => ({
          streamedContent: state.streamedContent + content,
          reasoningContent: reasoning
            ? state.reasoningContent + reasoning
            : state.reasoningContent,
        })),

      setStreamedContent: (content, reasoning) =>
        set({
          streamedContent: content,
          reasoningContent: reasoning || '',
        }),

      stopStreaming: (saveAsMessage = false) => {
        const state = get();
        if (saveAsMessage && state.streamedContent && state.activeSessionId) {
          const newMessage: Message = {
            id: Date.now(),
            sessionId: state.activeSessionId,
            role: 'assistant',
            content: state.streamedContent,
            createdAt: Math.floor(Date.now() / 1000),
          };
          set({
            isStreaming: false,
            streamedContent: '',
            reasoningContent: '',
            messages: [...state.messages, newMessage],
          });
        } else {
          set({
            isStreaming: false,
            streamedContent: '',
            reasoningContent: '',
          });
        }
      },

      setQuoteMessage: (message) => set({ quoteMessage: message }),

      clearQuoteMessage: () => set({ quoteMessage: null }),

      reset: () => set(initialState),
    }),
    {
      name: 'omninova-chat-storage',
      storage: createJSONStorage(() => localStorage),
      // Only persist certain fields
      partialize: (state) => ({
        messages: state.messages,
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
 * Select messages count
 */
export const useMessagesCount = () =>
  useChatStore((state) => state.messages.length);

/**
 * Select last message
 */
export const useLastMessage = () =>
  useChatStore((state) =>
    state.messages.length > 0 ? state.messages[state.messages.length - 1] : null
  );

/**
 * Select if currently streaming
 */
export const useIsStreaming = () =>
  useChatStore((state) => state.isStreaming);

/**
 * Select active session info
 */
export const useActiveSession = () =>
  useChatStore((state) => ({
    sessionId: state.activeSessionId,
    agentId: state.activeAgentId,
  }));

/**
 * Select error state
 */
export const useError = () =>
  useChatStore((state) => state.error);

export default useChatStore;