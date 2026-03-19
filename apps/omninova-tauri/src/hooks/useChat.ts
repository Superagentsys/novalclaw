/**
 * Chat Hook - Integrates streaming with chat store
 *
 * Provides a unified interface for chat functionality that combines:
 * - Message management via chatStore
 * - Streaming via useStreamChat
 * - Error handling and state synchronization
 *
 * [Source: Story 4.4 - ChatInterface 组件基础]
 */

import { useCallback, useEffect } from 'react';
import { useChatStore } from '@/stores/chatStore';
import { useStreamChat, type UseStreamChatOptions } from './useStreamChat';
import { type StreamChatRequest, type TokenUsage } from '@/types/agent';
import { type Message } from '@/types/session';

// ============================================================================
// Types
// ============================================================================

/**
 * Options for useChat hook
 */
export interface UseChatOptions extends UseStreamChatOptions {
  /** Agent ID for messages */
  agentId: number;
  /** Auto-add user message to store when sending */
  autoAddUserMessage?: boolean;
  /** Auto-add assistant message to store on stream done */
  autoAddAssistantMessage?: boolean;
}

/**
 * Return type for useChat hook
 */
export interface UseChatReturn {
  /** Messages from the store */
  messages: Message[];
  /** Whether a stream is active */
  isStreaming: boolean;
  /** Current streamed content */
  streamedContent: string;
  /** Reasoning content (for thinking models) */
  reasoningContent: string;
  /** Loading state */
  isLoading: boolean;
  /** Error message */
  error: string | null;
  /** Active session ID */
  sessionId: number | null;
  /** Token usage stats */
  usage: TokenUsage | null;
  /** Send a message (starts streaming) */
  sendMessage: (content: string, providerId?: string, model?: string) => Promise<void>;
  /** Cancel active stream */
  cancelStream: () => Promise<void>;
  /** Clear all messages */
  clearMessages: () => void;
  /** Reset all state */
  reset: () => void;
}

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * useChat hook
 *
 * Unified chat interface combining message management and streaming.
 * Automatically syncs streaming state to chat store.
 *
 * @param options - Hook configuration options
 * @returns Chat state and control functions
 *
 * @example
 * ```tsx
 * function ChatContainer({ agentId }: { agentId: number }) {
 *   const {
 *     messages,
 *     isStreaming,
 *     streamedContent,
 *     sendMessage,
 *     cancelStream,
 *   } = useChat({
 *     agentId,
 *     autoAddUserMessage: true,
 *     autoAddAssistantMessage: true,
 *   });
 *
 *   return (
 *     <div>
 *       <MessageList messages={messages} />
 *       {isStreaming && (
 *         <StreamingMessage content={streamedContent} />
 *       )}
 *       <ChatInput onSend={sendMessage} />
 *     </div>
 *   );
 * }
 * ```
 */
export function useChat(options: UseChatOptions): UseChatReturn {
  const {
    agentId,
    autoAddUserMessage = true,
    autoAddAssistantMessage = true,
    onStreamStart,
    onStreamDelta,
    onStreamToolCall,
    onStreamDone,
    onStreamError,
  } = options;

  // Get store actions
  const messages = useChatStore((state) => state.messages);
  const activeSessionId = useChatStore((state) => state.activeSessionId);
  const isLoading = useChatStore((state) => state.isLoading);
  const storeError = useChatStore((state) => state.error);
  const addMessage = useChatStore((state) => state.addMessage);
  const clearMessages = useChatStore((state) => state.clearMessages);
  const setLoading = useChatStore((state) => state.setLoading);
  const setError = useChatStore((state) => state.setError);
  const startStreaming = useChatStore((state) => state.startStreaming);
  const appendStreamedContent = useChatStore((state) => state.appendStreamedContent);
  const stopStreaming = useChatStore((state) => state.stopStreaming);
  const resetStore = useChatStore((state) => state.reset);

  // Create wrapped callbacks for streaming hook
  const handleStreamStart = useCallback(
    (sessionId: number, requestId: string) => {
      startStreaming();
      onStreamStart?.(sessionId, requestId);
    },
    [startStreaming, onStreamStart]
  );

  const handleStreamDelta = useCallback(
    (delta: string, reasoning?: string) => {
      appendStreamedContent(delta, reasoning);
      onStreamDelta?.(delta, reasoning);
    },
    [appendStreamedContent, onStreamDelta]
  );

  const handleStreamDone = useCallback(
    (sessionId: number, messageId: number, usage?: TokenUsage) => {
      // Auto-add assistant message if enabled
      if (autoAddAssistantMessage) {
        const state = useChatStore.getState();
        const content = state.streamedContent;
        if (content) {
          const assistantMessage: Message = {
            id: messageId || Date.now(),
            sessionId,
            role: 'assistant',
            content,
            createdAt: Math.floor(Date.now() / 1000),
          };
          addMessage(assistantMessage);
        }
      }

      stopStreaming(autoAddAssistantMessage);
      onStreamDone?.(sessionId, messageId, usage);
    },
    [autoAddAssistantMessage, addMessage, stopStreaming, onStreamDone]
  );

  const handleStreamError = useCallback(
    (code: string, message: string, partialContent?: string) => {
      setError(message);
      stopStreaming(false);
      onStreamError?.(code, message, partialContent);
    },
    [setError, stopStreaming, onStreamError]
  );

  // Use streaming hook with wrapped callbacks
  const {
    isStreaming,
    streamedContent,
    reasoningContent,
    sessionId,
    usage,
    sendMessage: streamSendMessage,
    cancelStream: streamCancelStream,
    reset: streamReset,
  } = useStreamChat({
    onStreamStart: handleStreamStart,
    onStreamDelta: handleStreamDelta,
    onStreamToolCall,
    onStreamDone: handleStreamDone,
    onStreamError: handleStreamError,
  });

  /**
   * Send a message
   */
  const sendMessage = useCallback(
    async (content: string, providerId?: string, model?: string) => {
      if (!content.trim()) return;

      // Clear previous error
      setError(null);
      setLoading(true);

      // Auto-add user message to store
      if (autoAddUserMessage && activeSessionId) {
        const userMessage: Message = {
          id: Date.now(),
          sessionId: activeSessionId,
          role: 'user',
          content: content.trim(),
          createdAt: Math.floor(Date.now() / 1000),
        };
        addMessage(userMessage);
      }

      try {
        const request: StreamChatRequest = {
          agentId,
          sessionId: activeSessionId || undefined,
          message: content.trim(),
          providerId,
          model,
        };

        await streamSendMessage(request);
      } catch (err) {
        const errorMessage = err instanceof Error ? err.message : String(err);
        setError(errorMessage);
      } finally {
        setLoading(false);
      }
    },
    [
      agentId,
      activeSessionId,
      autoAddUserMessage,
      addMessage,
      setError,
      setLoading,
      streamSendMessage,
    ]
  );

  /**
   * Cancel active stream
   */
  const cancelStream = useCallback(async () => {
    await streamCancelStream();
    stopStreaming(false);
  }, [streamCancelStream, stopStreaming]);

  /**
   * Reset all state
   */
  const reset = useCallback(() => {
    streamReset();
    resetStore();
  }, [streamReset, resetStore]);

  return {
    messages,
    isStreaming,
    streamedContent,
    reasoningContent,
    isLoading,
    error: storeError,
    sessionId,
    usage,
    sendMessage,
    cancelStream,
    clearMessages,
    reset,
  };
}

export default useChat;