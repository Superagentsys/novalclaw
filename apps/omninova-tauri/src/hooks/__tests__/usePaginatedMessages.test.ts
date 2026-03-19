/**
 * Tests for usePaginatedMessages Hook
 *
 * [Source: Story 4.7 - 对话历史持久化与导航]
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { usePaginatedMessages } from '../usePaginatedMessages';
import type { Message } from '@/types/session';

// Mock Tauri invoke
const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

// Helper to create mock messages
function createMockMessages(
  sessionId: number,
  count: number,
  startId = 1
): Message[] {
  return Array.from({ length: count }, (_, i) => ({
    id: startId + i,
    sessionId,
    role: i % 2 === 0 ? 'user' as const : 'assistant' as const,
    content: `Message ${startId + i}`,
    createdAt: Math.floor(Date.now() / 1000) - (count - i) * 60,
  }));
}

describe('usePaginatedMessages', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  describe('Initial state', () => {
    it('has correct initial state when no session', () => {
      const { result } = renderHook(() =>
        usePaginatedMessages({ sessionId: null })
      );

      expect(result.current.messages).toEqual([]);
      expect(result.current.isLoading).toBe(false);
      expect(result.current.hasMore).toBe(false);
      expect(result.current.totalCount).toBe(0);
      expect(result.current.error).toBeNull();
    });

    it('does not auto-load when autoLoad is false', () => {
      const sessionId = 1;
      const mockMessages = createMockMessages(sessionId, 5);

      mockInvoke.mockResolvedValueOnce(mockMessages);

      renderHook(() =>
        usePaginatedMessages({ sessionId, autoLoad: false })
      );

      expect(mockInvoke).not.toHaveBeenCalled();
    });
  });

  describe('Loading messages', () => {
    it('loads messages on mount when autoLoad is true', async () => {
      const sessionId = 1;
      const mockMessages = createMockMessages(sessionId, 5);

      mockInvoke.mockResolvedValueOnce(mockMessages);

      const { result } = renderHook(() =>
        usePaginatedMessages({ sessionId, autoLoad: true })
      );

      // Initially loading
      await waitFor(() => {
        expect(result.current.isLoading).toBe(true);
      });

      // Then loaded
      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      expect(mockInvoke).toHaveBeenCalledWith('list_messages_by_session', {
        sessionId,
      });
      expect(result.current.messages).toEqual(mockMessages);
      expect(result.current.totalCount).toBe(5);
    });

    it('sets loading state during fetch', async () => {
      const sessionId = 1;

      mockInvoke.mockImplementation(
        () => new Promise((resolve) => setTimeout(() => resolve([]), 100))
      );

      const { result } = renderHook(() =>
        usePaginatedMessages({ sessionId })
      );

      await waitFor(() => {
        expect(result.current.isLoading).toBe(true);
      });

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });
    });

    it('handles errors when loading messages', async () => {
      const sessionId = 1;
      const errorMessage = 'Failed to load messages';

      mockInvoke.mockRejectedValueOnce(new Error(errorMessage));

      const { result } = renderHook(() =>
        usePaginatedMessages({ sessionId })
      );

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      expect(result.current.error).toBe(errorMessage);
      expect(result.current.messages).toEqual([]);
    });

    it('handles non-Error errors', async () => {
      const sessionId = 1;

      mockInvoke.mockRejectedValueOnce('String error');

      const { result } = renderHook(() =>
        usePaginatedMessages({ sessionId })
      );

      await waitFor(() => {
        expect(result.current.error).toBe('String error');
      });
    });

    it('clears error on successful load', async () => {
      const sessionId = 1;
      const mockMessages = createMockMessages(sessionId, 3);

      mockInvoke.mockRejectedValueOnce(new Error('First error'));
      mockInvoke.mockResolvedValueOnce(mockMessages);

      const { result, rerender } = renderHook(
        ({ sid }) => usePaginatedMessages({ sessionId: sid }),
        { initialProps: { sid: sessionId } }
      );

      // First load fails
      await waitFor(() => {
        expect(result.current.error).toBe('First error');
      });

      // Reload
      await act(async () => {
        await result.current.reload();
      });

      await waitFor(() => {
        expect(result.current.error).toBeNull();
        expect(result.current.messages).toEqual(mockMessages);
      });
    });
  });

  describe('Session changes', () => {
    it('reloads messages when session changes', async () => {
      const sessionId1 = 1;
      const sessionId2 = 2;
      const mockMessages1 = createMockMessages(sessionId1, 3);
      const mockMessages2 = createMockMessages(sessionId2, 2);

      mockInvoke.mockResolvedValueOnce(mockMessages1);
      mockInvoke.mockResolvedValueOnce(mockMessages2);

      const { result, rerender } = renderHook(
        ({ sid }) => usePaginatedMessages({ sessionId: sid }),
        { initialProps: { sid: sessionId1 } }
      );

      await waitFor(() => {
        expect(result.current.messages).toEqual(mockMessages1);
      });

      // Change session
      rerender({ sid: sessionId2 });

      await waitFor(() => {
        expect(result.current.messages).toEqual(mockMessages2);
      });

      expect(mockInvoke).toHaveBeenCalledTimes(2);
    });

    it('clears messages when session becomes null', async () => {
      const sessionId = 1;
      const mockMessages = createMockMessages(sessionId, 3);

      mockInvoke.mockResolvedValueOnce(mockMessages);

      const { result, rerender } = renderHook(
        ({ sid }) => usePaginatedMessages({ sessionId: sid }),
        { initialProps: { sid: sessionId } }
      );

      await waitFor(() => {
        expect(result.current.messages).toEqual(mockMessages);
      });

      // Clear session
      rerender({ sid: null });

      await waitFor(() => {
        expect(result.current.messages).toEqual([]);
        expect(result.current.totalCount).toBe(0);
      });
    });
  });

  describe('appendMessage', () => {
    it('appends a new message to the end', async () => {
      const sessionId = 1;
      const initialMessages = createMockMessages(sessionId, 3);

      mockInvoke.mockResolvedValueOnce(initialMessages);

      const { result } = renderHook(() =>
        usePaginatedMessages({ sessionId })
      );

      await waitFor(() => {
        expect(result.current.messages).toHaveLength(3);
      });

      const newMessage: Message = {
        id: 100,
        sessionId,
        role: 'user',
        content: 'New message',
        createdAt: Math.floor(Date.now() / 1000),
      };

      act(() => {
        result.current.appendMessage(newMessage);
      });

      expect(result.current.messages).toHaveLength(4);
      expect(result.current.messages[3]).toEqual(newMessage);
      expect(result.current.totalCount).toBe(4);
    });

    it('can append multiple messages', async () => {
      const sessionId = 1;

      mockInvoke.mockResolvedValueOnce([]);

      const { result } = renderHook(() =>
        usePaginatedMessages({ sessionId })
      );

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      const msg1: Message = {
        id: 1,
        sessionId,
        role: 'user',
        content: 'First',
        createdAt: 1000,
      };
      const msg2: Message = {
        id: 2,
        sessionId,
        role: 'assistant',
        content: 'Second',
        createdAt: 2000,
      };

      act(() => {
        result.current.appendMessage(msg1);
        result.current.appendMessage(msg2);
      });

      expect(result.current.messages).toEqual([msg1, msg2]);
      expect(result.current.totalCount).toBe(2);
    });
  });

  describe('loadMore', () => {
    it('loadMore is a no-op without pagination support', async () => {
      const sessionId = 1;
      const mockMessages = createMockMessages(sessionId, 3);

      mockInvoke.mockResolvedValueOnce(mockMessages);

      const { result } = renderHook(() =>
        usePaginatedMessages({ sessionId })
      );

      await waitFor(() => {
        expect(result.current.messages).toEqual(mockMessages);
      });

      // loadMore should not change anything (no pagination support yet)
      await act(async () => {
        await result.current.loadMore();
      });

      expect(result.current.messages).toEqual(mockMessages);
      expect(result.current.hasMore).toBe(false);
    });

    it('loadMore does nothing when loading', async () => {
      const sessionId = 1;

      mockInvoke.mockImplementation(
        () => new Promise((resolve) => setTimeout(() => resolve([]), 100))
      );

      const { result } = renderHook(() =>
        usePaginatedMessages({ sessionId })
      );

      // Try to call loadMore while loading
      await act(async () => {
        await result.current.loadMore();
      });

      // Should only have been called once (initial load)
      expect(mockInvoke).toHaveBeenCalledTimes(1);
    });
  });

  describe('reset', () => {
    it('resets state without reloading', async () => {
      const sessionId = 1;
      const mockMessages = createMockMessages(sessionId, 3);

      mockInvoke.mockResolvedValueOnce(mockMessages);

      const { result } = renderHook(() =>
        usePaginatedMessages({ sessionId })
      );

      await waitFor(() => {
        expect(result.current.messages).toEqual(mockMessages);
      });

      act(() => {
        result.current.reset();
      });

      expect(result.current.messages).toEqual([]);
      expect(result.current.totalCount).toBe(0);
      expect(result.current.hasMore).toBe(false);
    });

    it('resets state and reloads when requested', async () => {
      const sessionId = 1;
      const mockMessages = createMockMessages(sessionId, 3);

      mockInvoke.mockResolvedValueOnce(mockMessages);
      mockInvoke.mockResolvedValueOnce(mockMessages);

      const { result } = renderHook(() =>
        usePaginatedMessages({ sessionId })
      );

      await waitFor(() => {
        expect(result.current.messages).toEqual(mockMessages);
      });

      await act(async () => {
        result.current.reset(true);
      });

      expect(result.current.messages).toEqual(mockMessages);
      expect(mockInvoke).toHaveBeenCalledTimes(2);
    });
  });

  describe('reload', () => {
    it('reloads messages from scratch', async () => {
      const sessionId = 1;
      const initialMessages = createMockMessages(sessionId, 3);
      const updatedMessages = createMockMessages(sessionId, 5);

      mockInvoke.mockResolvedValueOnce(initialMessages);
      mockInvoke.mockResolvedValueOnce(updatedMessages);

      const { result } = renderHook(() =>
        usePaginatedMessages({ sessionId })
      );

      await waitFor(() => {
        expect(result.current.messages).toEqual(initialMessages);
      });

      await act(async () => {
        await result.current.reload();
      });

      expect(result.current.messages).toEqual(updatedMessages);
      expect(mockInvoke).toHaveBeenCalledTimes(2);
    });

    it('does nothing when sessionId is null', async () => {
      const { result } = renderHook(() =>
        usePaginatedMessages({ sessionId: null })
      );

      await act(async () => {
        await result.current.reload();
      });

      expect(mockInvoke).not.toHaveBeenCalled();
    });
  });

  describe('Custom page size', () => {
    it('accepts custom page size', async () => {
      const sessionId = 1;
      const mockMessages = createMockMessages(sessionId, 100);

      mockInvoke.mockResolvedValueOnce(mockMessages);

      const { result } = renderHook(() =>
        usePaginatedMessages({ sessionId, pageSize: 100 })
      );

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      // All messages should be loaded since no pagination
      expect(result.current.messages).toHaveLength(100);
    });
  });

  describe('Edge cases', () => {
    it('handles empty message list', async () => {
      const sessionId = 1;

      mockInvoke.mockResolvedValueOnce([]);

      const { result } = renderHook(() =>
        usePaginatedMessages({ sessionId })
      );

      await waitFor(() => {
        expect(result.current.messages).toEqual([]);
        expect(result.current.totalCount).toBe(0);
        expect(result.current.hasMore).toBe(false);
      });
    });

    it('handles large message list', async () => {
      const sessionId = 1;
      const mockMessages = createMockMessages(sessionId, 500);

      mockInvoke.mockResolvedValueOnce(mockMessages);

      const { result } = renderHook(() =>
        usePaginatedMessages({ sessionId })
      );

      await waitFor(() => {
        expect(result.current.messages).toHaveLength(500);
        expect(result.current.totalCount).toBe(500);
      });
    });

    it('handles messages in correct order', async () => {
      const sessionId = 1;
      // Create messages with specific timestamps
      const mockMessages: Message[] = [
        {
          id: 1,
          sessionId,
          role: 'user',
          content: 'Oldest message',
          createdAt: 1000,
        },
        {
          id: 2,
          sessionId,
          role: 'assistant',
          content: 'Middle message',
          createdAt: 2000,
        },
        {
          id: 3,
          sessionId,
          role: 'user',
          content: 'Newest message',
          createdAt: 3000,
        },
      ];

      mockInvoke.mockResolvedValueOnce(mockMessages);

      const { result } = renderHook(() =>
        usePaginatedMessages({ sessionId })
      );

      await waitFor(() => {
        expect(result.current.messages).toEqual(mockMessages);
      });

      // Verify order is preserved (oldest first)
      expect(result.current.messages[0].createdAt).toBe(1000);
      expect(result.current.messages[2].createdAt).toBe(3000);
    });
  });
});