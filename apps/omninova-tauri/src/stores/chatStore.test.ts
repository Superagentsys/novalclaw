/**
 * Tests for chatStore quote state
 *
 * [Source: Story 4.8 - 消息引用功能]
 */

import { describe, it, expect, beforeEach } from 'vitest'
import { act } from '@testing-library/react'
import { useChatStore } from '@/stores/chatStore'
import type { QuoteMessage } from '@/types/session'

describe('chatStore quote state', () => {
  beforeEach(() => {
    // Reset store before each test
    act(() => {
      useChatStore.setState({
        messages: [],
        isStreaming: false,
        streamedContent: '',
        reasoningContent: '',
        isLoading: false,
        error: null,
        quoteMessage: null,
      })
    })
  })

  describe('setQuoteMessage', () => {
    it('sets quote message', () => {
      const quote: QuoteMessage = {
        id: 1,
        role: 'user',
        content: 'Test message',
      }

      act(() => {
        useChatStore.getState().setQuoteMessage(quote)
      })

      expect(useChatStore.getState().quoteMessage).toEqual(quote)
    })

    it('replaces existing quote message', () => {
      const quote1: QuoteMessage = {
        id: 1,
        role: 'user',
        content: 'First message',
      }
      const quote2: QuoteMessage = {
        id: 2,
        role: 'assistant',
        content: 'Second message',
      }

      act(() => {
        useChatStore.getState().setQuoteMessage(quote1)
      })
      expect(useChatStore.getState().quoteMessage).toEqual(quote1)

      act(() => {
        useChatStore.getState().setQuoteMessage(quote2)
      })
      expect(useChatStore.getState().quoteMessage).toEqual(quote2)
    })
  })

  describe('clearQuoteMessage', () => {
    it('clears quote message', () => {
      const quote: QuoteMessage = {
        id: 1,
        role: 'user',
        content: 'Test message',
      }

      act(() => {
        useChatStore.getState().setQuoteMessage(quote)
      })
      expect(useChatStore.getState().quoteMessage).not.toBeNull()

      act(() => {
        useChatStore.getState().clearQuoteMessage()
      })
      expect(useChatStore.getState().quoteMessage).toBeNull()
    })

    it('is idempotent - calling when no quote exists does not error', () => {
      expect(() => {
        act(() => {
          useChatStore.getState().clearQuoteMessage()
        })
      }).not.toThrow()

      expect(useChatStore.getState().quoteMessage).toBeNull()
    })
  })

  describe('quote message integration with messages', () => {
    it('quote message persists independently of messages', () => {
      const quote: QuoteMessage = {
        id: 1,
        role: 'user',
        content: 'Quote test',
      }

      act(() => {
        useChatStore.getState().setQuoteMessage(quote)
        useChatStore.getState().setMessages([
          {
            id: 1,
            sessionId: 1,
            role: 'user',
            content: 'New message',
            createdAt: Date.now() / 1000,
            quoteMessageId: null,
          },
        ])
      })

      // Quote should still be set
      expect(useChatStore.getState().quoteMessage).toEqual(quote)
      // Messages should also be set
      expect(useChatStore.getState().messages).toHaveLength(1)
    })
  })
})