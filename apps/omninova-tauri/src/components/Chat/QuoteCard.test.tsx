/**
 * Tests for QuoteCard component
 *
 * [Source: Story 4.8 - 消息引用功能]
 */

import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { QuoteCard } from '@/components/Chat/QuoteCard'
import type { QuoteMessage } from '@/types/session'

// Mock personality-colors module
vi.mock('@/lib/personality-colors', () => ({
  getPersonalityColors: () => ({ primary: '#3b82f6', secondary: '#60a5fa' }),
}))

describe('QuoteCard', () => {
  const mockQuote: QuoteMessage = {
    id: 1,
    role: 'user',
    content: 'This is a quoted message that is being replied to.',
  }

  const defaultProps = {
    quote: mockQuote,
    onCancel: vi.fn(),
  }

  it('renders quote content', () => {
    render(<QuoteCard {...defaultProps} />)

    expect(screen.getByText(/This is a quoted message/)).toBeInTheDocument()
  })

  it('shows sender label for user messages', () => {
    render(<QuoteCard {...defaultProps} />)

    // Component shows "引用回复" · "用户" separately
    expect(screen.getByText('引用回复')).toBeInTheDocument()
    expect(screen.getByText('用户')).toBeInTheDocument()
  })

  it('shows agent name for assistant messages', () => {
    const assistantQuote: QuoteMessage = {
      ...mockQuote,
      role: 'assistant',
    }
    render(<QuoteCard quote={assistantQuote} onCancel={defaultProps.onCancel} agentName="Nova" />)

    expect(screen.getByText('引用回复')).toBeInTheDocument()
    expect(screen.getByText('Nova')).toBeInTheDocument()
  })

  it('truncates long messages', () => {
    const longQuote: QuoteMessage = {
      id: 1,
      role: 'user',
      content: 'A'.repeat(200),
    }
    render(<QuoteCard quote={longQuote} onCancel={defaultProps.onCancel} />)

    // Should show truncated content with ellipsis
    const content = screen.getByText(/A+\.\.\./)
    expect(content).toBeInTheDocument()
  })

  it('calls onCancel when cancel button is clicked', () => {
    const onCancel = vi.fn()
    render(<QuoteCard {...defaultProps} onCancel={onCancel} />)

    const cancelButton = screen.getByRole('button', { name: /取消引用/i })
    fireEvent.click(cancelButton)

    expect(onCancel).toHaveBeenCalledTimes(1)
  })

  it('has correct accessibility attributes', () => {
    render(<QuoteCard {...defaultProps} />)

    const card = screen.getByRole('region', { name: /引用的消息/i })
    expect(card).toBeInTheDocument()
  })

  it('applies custom className', () => {
    const { container } = render(
      <QuoteCard {...defaultProps} className="custom-class" />
    )

    expect(container.firstChild).toHaveClass('custom-class')
  })
})