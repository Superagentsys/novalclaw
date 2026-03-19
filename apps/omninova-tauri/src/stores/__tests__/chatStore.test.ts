/**
 * Tests for Chat Store
 *
 * [Source: Story 4.4 - ChatInterface 组件基础]
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useChatStore } from '../chatStore';
import type { Message } from '@/types/session';

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

describe('Chat Store', () => {
  beforeEach(() => {
    // Reset store state before each test
    useChatStore.setState({
      messages: [],
      activeSessionId: null,
      activeAgentId: null,
      isLoading: false,
      error: null,
      isStreaming: false,
      streamedContent: '',
      reasoningContent: '',
    });
    localStorageMock.clear();
  });

  describe('Initial state', () => {
    it('has correct initial state', () => {
      const state = useChatStore.getState();

      expect(state.messages).toEqual([]);
      expect(state.activeSessionId).toBeNull();
      expect(state.activeAgentId).toBeNull();
      expect(state.isLoading).toBe(false);
      expect(state.error).toBeNull();
      expect(state.isStreaming).toBe(false);
      expect(state.streamedContent).toBe('');
      expect(state.reasoningContent).toBe('');
    });
  });

  describe('Message management', () => {
    const mockMessage: Message = {
      id: 1,
      sessionId: 1,
      role: 'user',
      content: 'Hello!',
      createdAt: Math.floor(Date.now() / 1000),
    };

    it('adds a message', () => {
      const { addMessage } = useChatStore.getState();

      act(() => {
        addMessage(mockMessage);
      });

      const state = useChatStore.getState();
      expect(state.messages).toHaveLength(1);
      expect(state.messages[0]).toEqual(mockMessage);
    });

    it('sets messages', () => {
      const messages: Message[] = [
        mockMessage,
        { ...mockMessage, id: 2, role: 'assistant', content: 'Hi!' },
      ];
      const { setMessages } = useChatStore.getState();

      act(() => {
        setMessages(messages);
      });

      const state = useChatStore.getState();
      expect(state.messages).toHaveLength(2);
    });

    it('updates a message', () => {
      const { addMessage, updateMessage } = useChatStore.getState();

      act(() => {
        addMessage(mockMessage);
      });

      act(() => {
        updateMessage(1, 'Updated content');
      });

      const state = useChatStore.getState();
      expect(state.messages[0].content).toBe('Updated content');
    });

    it('clears all messages', () => {
      const { addMessage, clearMessages } = useChatStore.getState();

      act(() => {
        addMessage(mockMessage);
        addMessage({ ...mockMessage, id: 2 });
      });

      expect(useChatStore.getState().messages).toHaveLength(2);

      act(() => {
        clearMessages();
      });

      expect(useChatStore.getState().messages).toHaveLength(0);
    });
  });

  describe('Session management', () => {
    it('sets active session', () => {
      const { setActiveSession } = useChatStore.getState();

      act(() => {
        setActiveSession(1, 5);
      });

      const state = useChatStore.getState();
      expect(state.activeSessionId).toBe(1);
      expect(state.activeAgentId).toBe(5);
    });

    it('sets session without agent', () => {
      const { setActiveSession } = useChatStore.getState();

      act(() => {
        setActiveSession(2);
      });

      const state = useChatStore.getState();
      expect(state.activeSessionId).toBe(2);
      expect(state.activeAgentId).toBeNull();
    });
  });

  describe('Loading state', () => {
    it('sets loading state', () => {
      const { setLoading } = useChatStore.getState();

      act(() => {
        setLoading(true);
      });

      expect(useChatStore.getState().isLoading).toBe(true);

      act(() => {
        setLoading(false);
      });

      expect(useChatStore.getState().isLoading).toBe(false);
    });
  });

  describe('Error handling', () => {
    it('sets error', () => {
      const { setError } = useChatStore.getState();

      act(() => {
        setError('Something went wrong');
      });

      expect(useChatStore.getState().error).toBe('Something went wrong');
    });

    it('clears error', () => {
      const { setError } = useChatStore.getState();

      act(() => {
        setError('Error');
      });

      act(() => {
        setError(null);
      });

      expect(useChatStore.getState().error).toBeNull();
    });
  });

  describe('Streaming', () => {
    it('starts streaming and clears content', () => {
      const { startStreaming, appendStreamedContent } = useChatStore.getState();

      // Add some content first
      act(() => {
        appendStreamedContent('Previous content');
      });

      // Start streaming should clear
      act(() => {
        startStreaming();
      });

      const state = useChatStore.getState();
      expect(state.isStreaming).toBe(true);
      expect(state.streamedContent).toBe('');
      expect(state.reasoningContent).toBe('');
    });

    it('appends streamed content', () => {
      const { appendStreamedContent } = useChatStore.getState();

      act(() => {
        appendStreamedContent('Hello');
      });

      act(() => {
        appendStreamedContent(' World');
      });

      expect(useChatStore.getState().streamedContent).toBe('Hello World');
    });

    it('appends reasoning content', () => {
      const { appendStreamedContent } = useChatStore.getState();

      act(() => {
        appendStreamedContent('Content', 'Thinking...');
      });

      act(() => {
        appendStreamedContent(' more', ' more thinking');
      });

      const state = useChatStore.getState();
      expect(state.streamedContent).toBe('Content more');
      expect(state.reasoningContent).toBe('Thinking... more thinking');
    });

    it('sets full streamed content', () => {
      const { setStreamedContent } = useChatStore.getState();

      act(() => {
        setStreamedContent('Full content', 'Full reasoning');
      });

      const state = useChatStore.getState();
      expect(state.streamedContent).toBe('Full content');
      expect(state.reasoningContent).toBe('Full reasoning');
    });

    it('stops streaming without saving', () => {
      const { startStreaming, appendStreamedContent, stopStreaming } = useChatStore.getState();

      act(() => {
        startStreaming();
        appendStreamedContent('Content');
      });

      act(() => {
        stopStreaming(false);
      });

      const state = useChatStore.getState();
      expect(state.isStreaming).toBe(false);
      expect(state.streamedContent).toBe('');
      expect(state.messages).toHaveLength(0);
    });

    it('stops streaming and saves as message', () => {
      const { setActiveSession, startStreaming, appendStreamedContent, stopStreaming } = useChatStore.getState();

      act(() => {
        setActiveSession(1);
        startStreaming();
        appendStreamedContent('Assistant content');
      });

      act(() => {
        stopStreaming(true);
      });

      const state = useChatStore.getState();
      expect(state.isStreaming).toBe(false);
      expect(state.streamedContent).toBe('');
      expect(state.messages).toHaveLength(1);
      expect(state.messages[0].role).toBe('assistant');
      expect(state.messages[0].content).toBe('Assistant content');
    });
  });

  describe('Reset', () => {
    it('resets all state to initial', () => {
      const store = useChatStore.getState();

      act(() => {
        store.addMessage({
          id: 1,
          sessionId: 1,
          role: 'user',
          content: 'Test',
          createdAt: Date.now(),
        });
        store.setActiveSession(1, 5);
        store.setError('Error');
        store.startStreaming();
      });

      act(() => {
        useChatStore.getState().reset();
      });

      const state = useChatStore.getState();
      expect(state.messages).toEqual([]);
      expect(state.activeSessionId).toBeNull();
      expect(state.activeAgentId).toBeNull();
      expect(state.isLoading).toBe(false);
      expect(state.error).toBeNull();
      expect(state.isStreaming).toBe(false);
      expect(state.streamedContent).toBe('');
    });
  });

  describe('Selector hooks', () => {
    it('useMessagesCount returns correct count', () => {
      const { result } = renderHook(() => {
        const messages = useChatStore((s) => s.messages);
        return messages.length;
      });

      expect(result.current).toBe(0);

      act(() => {
        useChatStore.getState().addMessage({
          id: 1,
          sessionId: 1,
          role: 'user',
          content: 'Test',
          createdAt: Date.now(),
        });
      });

      expect(result.current).toBe(1);
    });

    it('useIsStreaming returns streaming state', () => {
      const { result } = renderHook(() =>
        useChatStore((state) => state.isStreaming)
      );

      expect(result.current).toBe(false);

      act(() => {
        useChatStore.getState().startStreaming();
      });

      expect(result.current).toBe(true);
    });
  });
});