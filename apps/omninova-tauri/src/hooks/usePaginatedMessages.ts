/**
 * Paginated Messages Hook
 *
 * Provides paginated message loading for chat sessions with
 * infinite scroll support and "load more" functionality.
 *
 * [Source: Story 4.7 - 对话历史持久化与导航]
 */

import { useState, useCallback, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { type Message } from '@/types/session';

// ============================================================================
// Types
// ============================================================================

/**
 * Options for usePaginatedMessages hook
 */
export interface UsePaginatedMessagesOptions {
  /** Session ID to load messages for */
  sessionId: number | null;
  /** Number of messages per page (default: 50) */
  pageSize?: number;
  /** Auto-load initial messages on mount/session change */
  autoLoad?: boolean;
}

/**
 * Return type for usePaginatedMessages hook
 */
export interface UsePaginatedMessagesReturn {
  /** Loaded messages (chronological order - oldest first) */
  messages: Message[];
  /** Whether messages are currently loading */
  isLoading: boolean;
  /** Whether there are more messages to load */
  hasMore: boolean;
  /** Total message count for the session */
  totalCount: number;
  /** Load more messages (prepends older messages) */
  loadMore: () => Promise<void>;
  /** Add a new message to the end (for new outgoing/incoming messages) */
  appendMessage: (message: Message) => void;
  /** Reset state and optionally reload */
  reset: (reload?: boolean) => void;
  /** Reload all messages from scratch */
  reload: () => Promise<void>;
  /** Error message if any */
  error: string | null;
}

// ============================================================================
// Constants
// ============================================================================

const DEFAULT_PAGE_SIZE = 50;

// ============================================================================
// Tauri Commands
// ============================================================================

/**
 * Fetch all messages for a session
 *
 * Note: When backend pagination is implemented, this will be replaced with
 * a paginated version that accepts offset and limit parameters.
 */
async function fetchMessages(sessionId: number): Promise<Message[]> {
  return invoke<Message[]>('list_messages_by_session', { sessionId });
}

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * usePaginatedMessages hook
 *
 * Manages paginated message loading for a chat session with support for:
 * - Initial message loading
 * - "Load more" for older messages
 * - Appending new messages
 * - State management and error handling
 *
 * **Current Implementation Note:**
 * This hook currently fetches all messages at once since the backend
 * pagination API (`list_messages_paginated`) is not yet implemented.
 * When pagination is added, this hook will be updated to use it.
 * The hook is designed to be backward-compatible - consumers don't need
 * to change when pagination is added.
 *
 * @param options - Hook configuration options
 * @returns Message state and control functions
 *
 * @example
 * ```tsx
 * function ChatMessageList({ sessionId }: { sessionId: number }) {
 *   const {
 *     messages,
 *     isLoading,
 *     hasMore,
 *     loadMore,
 *     appendMessage,
 *   } = usePaginatedMessages({ sessionId, pageSize: 50 });
 *
 *   // Handle scroll to top to load more
 *   const handleScroll = (e: React.UIEvent<HTMLDivElement>) => {
 *     const { scrollTop } = e.currentTarget;
 *     if (scrollTop < 50 && hasMore && !isLoading) {
 *       loadMore();
 *     }
 *   };
 *
 *   return (
 *     <div onScroll={handleScroll}>
 *       {hasMore && <button onClick={loadMore}>Load more</button>}
 *       {messages.map(msg => <MessageBubble key={msg.id} message={msg} />)}
 *     </div>
 *   );
 * }
 * ```
 */
export function usePaginatedMessages(
  options: UsePaginatedMessagesOptions
): UsePaginatedMessagesReturn {
  const {
    sessionId,
    pageSize = DEFAULT_PAGE_SIZE,
    autoLoad = true,
  } = options;

  // State
  const [messages, setMessages] = useState<Message[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const [totalCount, setTotalCount] = useState(0);
  const [error, setError] = useState<string | null>(null);

  // Track current session to detect changes
  const currentSessionIdRef = useRef<number | null>(null);

  // Track if initial load has been done for this session
  const initialLoadDoneRef = useRef(false);

  /**
   * Load all messages for a session
   *
   * Note: Currently fetches all messages. When backend pagination is
   * implemented, this will load only the most recent page.
   */
  const loadMessages = useCallback(async (sid: number) => {
    setIsLoading(true);
    setError(null);

    try {
      const fetched = await fetchMessages(sid);

      // Messages are returned in chronological order (oldest first)
      setMessages(fetched);
      setTotalCount(fetched.length);

      // For now, we don't have true pagination, so hasMore is based on
      // whether we have more than pageSize messages
      // When pagination is implemented, this will check if fetched < pageSize
      setHasMore(false); // No pagination support yet

      initialLoadDoneRef.current = true;
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      setError(errorMessage);
      setMessages([]);
      setTotalCount(0);
      setHasMore(false);
    } finally {
      setIsLoading(false);
    }
  }, []);

  /**
   * Load more messages (older ones)
   *
   * Note: Currently this is a no-op since we fetch all messages at once.
   * When pagination is implemented, this will prepend older messages.
   */
  const loadMore = useCallback(async () => {
    // No pagination support yet - all messages are loaded at once
    // When implemented, this will:
    // 1. Fetch the next page of older messages
    // 2. Prepend them to the existing messages array
    // 3. Update hasMore based on whether we got a full page
    if (!sessionId || isLoading) {
      return;
    }

    // Placeholder for when pagination is implemented:
    // const offset = messages.length;
    // const olderMessages = await fetchPaginatedMessages(sessionId, offset, pageSize);
    // setMessages(prev => [...olderMessages.reverse(), ...prev]);
    // setHasMore(olderMessages.length === pageSize);
  }, [sessionId, isLoading, pageSize]);

  /**
   * Append a new message (for real-time updates)
   */
  const appendMessage = useCallback((message: Message) => {
    setMessages((prev) => [...prev, message]);
    setTotalCount((prev) => prev + 1);
  }, []);

  /**
   * Reset state and optionally reload
   */
  const reset = useCallback((reload = false) => {
    setMessages([]);
    setTotalCount(0);
    setHasMore(false);
    setError(null);
    initialLoadDoneRef.current = false;

    if (reload && sessionId) {
      loadMessages(sessionId);
    }
  }, [sessionId, loadMessages]);

  /**
   * Reload all messages from scratch
   */
  const reload = useCallback(async () => {
    if (sessionId) {
      initialLoadDoneRef.current = false;
      await loadMessages(sessionId);
    }
  }, [sessionId, loadMessages]);

  /**
   * Auto-load messages when session changes
   */
  useEffect(() => {
    // Detect session change
    const sessionChanged = currentSessionIdRef.current !== sessionId;
    currentSessionIdRef.current = sessionId;

    if (sessionId && autoLoad && (sessionChanged || !initialLoadDoneRef.current)) {
      loadMessages(sessionId);
    } else if (!sessionId) {
      // Clear messages when no session
      setMessages([]);
      setTotalCount(0);
      setHasMore(false);
      setError(null);
      initialLoadDoneRef.current = false;
    }
  }, [sessionId, autoLoad, loadMessages]);

  return {
    messages,
    isLoading,
    hasMore,
    totalCount,
    loadMore,
    appendMessage,
    reset,
    reload,
    error,
  };
}

export default usePaginatedMessages;