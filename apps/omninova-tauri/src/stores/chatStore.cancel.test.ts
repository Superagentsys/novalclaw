/**
 * Tests for chatStore cancellation functionality
 *
 * Story 4.9: 响应中断功能
 *
 * Tests for:
 * - cancelActiveStream action
 * - interruptedContent state
 * - stopStreaming with partial content preservation
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { act } from '@testing-library/react';
import { useChatStore } from './chatStore';

// Mock Tauri invoke
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(true),
}));

describe('chatStore - Cancellation (Story 4.9)', () => {
  beforeEach(() => {
    // Reset store before each test
    useChatStore.setState({
      messages: [],
      activeSessionId: null,
      activeAgentId: null,
      isLoading: false,
      error: null,
      isStreaming: false,
      streamedContent: '',
      reasoningContent: '',
      quoteMessage: null,
    });
  });

  describe('cancelActiveStream action', () => {
    it('should cancel active stream and preserve streamed content', async () => {
      const sessionId = 123;

      // Setup streaming state
      useChatStore.setState({
        activeSessionId: sessionId,
        isStreaming: true,
        streamedContent: 'Partial response content...',
      });

      // Cancel the stream
      await act(async () => {
        await useChatStore.getState().cancelActiveStream(sessionId);
      });

      const state = useChatStore.getState();

      // Verify streaming stopped
      expect(state.isStreaming).toBe(false);

      // Verify partial content was preserved as a message
      expect(state.messages).toHaveLength(1);
      expect(state.messages[0].content).toBe('Partial response content...');
      expect(state.messages[0].role).toBe('assistant');

      // Verify streamed content cleared
      expect(state.streamedContent).toBe('');
    });

    it('should not create message when no streamed content', async () => {
      const sessionId = 123;

      useChatStore.setState({
        activeSessionId: sessionId,
        isStreaming: true,
        streamedContent: '', // Empty content
      });

      await act(async () => {
        await useChatStore.getState().cancelActiveStream(sessionId);
      });

      const state = useChatStore.getState();

      expect(state.messages).toHaveLength(0);
      expect(state.isStreaming).toBe(false);
    });

    it('should do nothing when not streaming', async () => {
      useChatStore.setState({
        activeSessionId: 123,
        isStreaming: false,
        streamedContent: 'Some content',
      });

      const initialState = useChatStore.getState();

      await act(async () => {
        await useChatStore.getState().cancelActiveStream(123);
      });

      const state = useChatStore.getState();

      // State should remain unchanged
      expect(state.isStreaming).toBe(false);
      expect(state.streamedContent).toBe('Some content');
      expect(state.messages).toEqual(initialState.messages);
    });

    it('should do nothing when session ID does not match', async () => {
      useChatStore.setState({
        activeSessionId: 123,
        isStreaming: true,
        streamedContent: 'Content',
      });

      await act(async () => {
        // Try to cancel with wrong session ID
        await useChatStore.getState().cancelActiveStream(456);
      });

      const state = useChatStore.getState();

      // Should still be streaming since session ID didn't match
      expect(state.isStreaming).toBe(true);
    });
  });

  describe('interruptedContent state', () => {
    it('should track interrupted content separately', () => {
      const { setInterruptedContent, clearInterruptedContent } = useChatStore.getState();

      // Set interrupted content
      act(() => {
        setInterruptedContent?.('Interrupted partial content');
      });

      expect(useChatStore.getState().interruptedContent).toBe('Interrupted partial content');

      // Clear it
      act(() => {
        clearInterruptedContent?.();
      });

      expect(useChatStore.getState().interruptedContent).toBeNull();
    });
  });

  describe('stopStreaming with saveAsMessage', () => {
    it('should save streamed content as assistant message when saveAsMessage is true', () => {
      const sessionId = 123;

      useChatStore.setState({
        activeSessionId: sessionId,
        isStreaming: true,
        streamedContent: 'Final response content',
      });

      act(() => {
        useChatStore.getState().stopStreaming(true);
      });

      const state = useChatStore.getState();

      expect(state.isStreaming).toBe(false);
      expect(state.messages).toHaveLength(1);
      expect(state.messages[0]).toMatchObject({
        role: 'assistant',
        content: 'Final response content',
        sessionId: sessionId,
      });
      expect(state.streamedContent).toBe('');
    });

    it('should discard content when saveAsMessage is false', () => {
      useChatStore.setState({
        activeSessionId: 123,
        isStreaming: true,
        streamedContent: 'Discarded content',
        messages: [],
      });

      act(() => {
        useChatStore.getState().stopStreaming(false);
      });

      const state = useChatStore.getState();

      expect(state.isStreaming).toBe(false);
      expect(state.messages).toHaveLength(0);
      expect(state.streamedContent).toBe('');
    });
  });
});