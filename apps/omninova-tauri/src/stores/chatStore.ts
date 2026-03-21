/**
 * Chat Store
 *
 * Zustand store for managing chat state including messages,
 * sessions, loading states, and streaming status.
 *
 * [Source: Story 4.4 - ChatInterface 组件基础]
 * [Source: Story 4.9 - 响应中断功能]
 * [Source: Story 5.8 - 重要片段标记功能]
 */

import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';
import { type Message, type MessageRole } from '@/types/session';
import type { CommandResult, CommandData } from '@/types/command';
import { parseCommandData } from '@/types/command';
import { invoke } from '@tauri-apps/api/core';

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
  /**
   * Interrupted content from cancelled stream
   *
   * When a stream is cancelled, the partial content is stored here
   * for display purposes. Cleared when a new stream starts.
   *
   * [Source: Story 4.9 - 响应中断功能]
   */
  interruptedContent: string | null;
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
  /**
   * Cancel the active stream and preserve partial content
   *
   * This action:
   * 1. Calls the backend cancel_stream command
   * 2. Preserves streamed content as an assistant message
   * 3. Updates streaming state to false
   *
   * @param sessionId - The session ID to cancel the stream for
   * @returns Promise that resolves when cancellation is complete
   *
   * [Source: Story 4.9 - 响应中断功能]
   */
  cancelActiveStream: (sessionId: number) => Promise<void>;
  /**
   * Set interrupted content for display
   *
   * Stores partial content from a cancelled stream.
   *
   * @param content - The interrupted/partial content
   *
   * [Source: Story 4.9 - 响应中断功能]
   */
  setInterruptedContent: (content: string | null) => void;
  /**
   * Clear interrupted content
   *
   * [Source: Story 4.9 - 响应中断功能]
   */
  clearInterruptedContent: () => void;
  /**
   * Execute a chat command (e.g., /help, /clear, /export)
   *
   * This action:
   * 1. Sends the command to the backend
   * 2. Processes the result and performs frontend actions
   * 3. Returns the result for display
   *
   * @param input - The raw command input (e.g., "/help", "/clear")
   * @returns The command result from the backend
   *
   * [Source: Story 4.10 - 指令执行框架]
   */
  executeCommand: (input: string) => Promise<CommandResult>;
  /**
   * Toggle the mark status of a message
   *
   * This action:
   * 1. Calls the backend to mark/unmark the message
   * 2. Updates the message's isMarked status in the store
   *
   * @param messageId - The ID of the message to toggle
   * @returns Promise that resolves when the operation is complete
   *
   * [Source: Story 5.8 - 重要片段标记功能]
   */
  toggleMessageMark: (messageId: number) => Promise<void>;
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
  interruptedContent: null,
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
          interruptedContent: null,
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

      /**
       * Cancel the active stream
       *
       * This action cancels the active streaming response by:
       * 1. Calling the backend cancel_stream command
       * 2. Preserving the partial content as an assistant message
       * 3. Updating streaming state
       *
       * [Source: Story 4.9 - 响应中断功能]
       */
      cancelActiveStream: async (sessionId: number) => {
        const state = get();

        // Do nothing if not streaming
        if (!state.isStreaming) {
          return;
        }

        // Do nothing if session ID doesn't match
        if (state.activeSessionId !== sessionId) {
          return;
        }

        try {
          // Call backend to cancel the stream
          await invoke<boolean>('cancel_stream', { sessionId });
        } catch (error) {
          console.error('Failed to cancel stream:', error);
        }

        // Preserve partial content as a message if there's content
        if (state.streamedContent && state.activeSessionId) {
          const partialMessage: Message = {
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
            interruptedContent: state.streamedContent,
            messages: [...state.messages, partialMessage],
          });
        } else {
          set({
            isStreaming: false,
            streamedContent: '',
            reasoningContent: '',
          });
        }
      },

      /**
       * Set interrupted content for display
       *
       * [Source: Story 4.9 - 响应中断功能]
       */
      setInterruptedContent: (content) => set({ interruptedContent: content }),

      /**
       * Clear interrupted content
       *
       * [Source: Story 4.9 - 响应中断功能]
       */
      clearInterruptedContent: () => set({ interruptedContent: null }),

      /**
       * Execute a chat command
       *
       * This action handles user commands like /help, /clear, /export
       *
       * [Source: Story 4.10 - 指令执行框架]
       */
      executeCommand: async (input: string): Promise<CommandResult> => {
        const state = get();

        // Validate session context
        if (!state.activeSessionId || !state.activeAgentId) {
          return {
            success: false,
            message: '无活动会话，无法执行指令',
          };
        }

        try {
          // Call backend to execute the command
          const result = await invoke<CommandResult>('execute_command', {
            input,
            sessionId: state.activeSessionId,
            agentId: state.activeAgentId,
          });

          // Handle frontend actions from command result
          const commandData = parseCommandData(result);
          if (commandData) {
            switch (commandData.action) {
              case 'clear_messages':
                // Clear messages in the store
                set({ messages: [], interruptedContent: null });
                break;
              case 'export_session':
                // Export is handled by the UI component
                // The result.data contains the format info
                break;
            }
          }

          return result;
        } catch (error) {
          console.error('Failed to execute command:', error);
          return {
            success: false,
            message: error instanceof Error ? error.message : '指令执行失败',
          };
        }
      },

      /**
       * Toggle the mark status of a message
       *
       * This action toggles the isMarked status of a message
       * by calling the backend API and updating the local state.
       *
       * [Source: Story 5.8 - 重要片段标记功能]
       */
      toggleMessageMark: async (messageId: number) => {
        const state = get();

        // Find the message to get current mark status
        const message = state.messages.find((m) => m.id === messageId);
        if (!message) {
          console.error('Message not found:', messageId);
          return;
        }

        try {
          // Toggle the mark status via backend
          const newMarkStatus = !message.isMarked;
          await invoke<boolean>('mark_message_important', {
            messageId,
            isMarked: newMarkStatus,
          });

          // Update local state
          set((state) => ({
            messages: state.messages.map((msg) =>
              msg.id === messageId ? { ...msg, isMarked: newMarkStatus } : msg
            ),
          }));
        } catch (error) {
          console.error('Failed to toggle message mark:', error);
        }
      },

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