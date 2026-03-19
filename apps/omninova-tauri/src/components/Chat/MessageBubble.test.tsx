/**
 * Tests for MessageBubble quote display
 *
 * [Source: Story 4.8 - 消息引用功能]
 */

import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { MessageBubble } from '@/components/Chat/MessageBubble'
import type { MessageRole } from '@/types/session'

// Mock personality-colors module
vi.mock('@/lib/personality-colors', () => ({
  getPersonalityColors: () => ({ primary: '#3b82f6', secondary: '#60a5fa' }),
}))

describe('MessageBubble quote display', () => {
  const defaultProps = {
    content: 'This is a reply message',
    role: 'assistant' as MessageRole,
    timestamp: Date.now() / 1000,
    showTimestamp: true,
  }

  it('renders quote preview when quoteMessageId and quoteContent are provided', () => {
    render(
      <MessageBubble
        {...defaultProps}
        quoteMessageId={1}
        quoteContent="This is the quoted content"
        quoteRole="user"
      />
    )

    expect(screen.getByText(/用户:/)).toBeInTheDocument()
    expect(screen.getByText(/This is the quoted content/)).toBeInTheDocument()
  })

  it('shows "AI" label for assistant quoted messages', () => {
    render(
      <MessageBubble
        {...defaultProps}
        quoteMessageId={1}
        quoteContent="AI response"
        quoteRole="assistant"
      />
    )

    expect(screen.getByText(/AI:/)).toBeInTheDocument()
  })

  it('truncates long quote content', () => {
    const longContent = 'A'.repeat(150)
    render(
      <MessageBubble
        {...defaultProps}
        quoteMessageId={1}
        quoteContent={longContent}
        quoteRole="user"
      />
    )

    // Should show truncated content with ellipsis
    const truncatedText = screen.getByText(/A+\.\.\./)
    expect(truncatedText).toBeInTheDocument()
  })

  it('calls onQuoteClick when quote preview is clicked', () => {
    const onQuoteClick = vi.fn()
    render(
      <MessageBubble
        {...defaultProps}
        quoteMessageId={1}
        quoteContent="Quoted content"
        quoteRole="user"
        onQuoteClick={onQuoteClick}
      />
    )

    const quoteButton = screen.getByRole('button', { name: /跳转到被引用的消息/i })
    fireEvent.click(quoteButton)

    expect(onQuoteClick).toHaveBeenCalledWith(1)
  })

  it('does not render quote preview when quoteContent is missing', () => {
    render(
      <MessageBubble
        {...defaultProps}
        quoteMessageId={1}
        quoteRole="user"
      />
    )

    // Should not have quote preview
    expect(screen.queryByText(/用户:/)).not.toBeInTheDocument()
  })

  it('does not render quote preview when quoteMessageId is missing', () => {
    render(
      <MessageBubble
        {...defaultProps}
        quoteContent="Some content"
        quoteRole="user"
      />
    )

    expect(screen.queryByText(/用户:/)).not.toBeInTheDocument()
  })

  it('quote preview has accessible label', () => {
    render(
      <MessageBubble
        {...defaultProps}
        quoteMessageId={1}
        quoteContent="Quoted"
        quoteRole="user"
      />
    )

    const quoteButton = screen.getByRole('button', { name: /跳转到被引用的消息/i })
    expect(quoteButton).toBeInTheDocument()
  })

  it('applies different border color for user vs assistant quotes', () => {
    const { container: userContainer } = render(
      <MessageBubble
        {...defaultProps}
        quoteMessageId={1}
        quoteContent="User quote"
        quoteRole="user"
      />
    )

    const { container: assistantContainer } = render(
      <MessageBubble
        {...defaultProps}
        quoteMessageId={2}
        quoteContent="AI quote"
        quoteRole="assistant"
      />
    )

    // Both should have quote elements but with different styling classes
    expect(userContainer.querySelector('.border-l-primary\\/60')).toBeTruthy()
    expect(assistantContainer.querySelector('.border-l-primary')).toBeTruthy()
  })
})