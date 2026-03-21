/**
 * Tests for MessageList mark functionality
 *
 * [Source: Story 5.8 - 重要片段标记功能]
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MessageList } from '../MessageList';
import type { Message } from '@/types/session';

describe('MessageList Mark Functionality', () => {
  const mockMessages: Message[] = [
    {
      id: 1,
      sessionId: 1,
      role: 'user',
      content: 'Hello world',
      createdAt: Date.now() / 1000,
      isMarked: false,
    },
    {
      id: 2,
      sessionId: 1,
      role: 'assistant',
      content: 'Hi there!',
      createdAt: Date.now() / 1000 + 1,
      isMarked: true,
    },
  ];

  const defaultProps = {
    messages: mockMessages,
    personalityType: 'INTJ' as const,
    agentName: 'Test Agent',
  };

  it('shows star indicator for marked messages', () => {
    render(<MessageList {...defaultProps} />);

    // Should show star for marked message
    expect(screen.getByTitle('已标记为重要')).toBeInTheDocument();
  });

  it('applies amber highlight to marked messages', () => {
    const { container } = render(<MessageList {...defaultProps} />);

    // Check for amber background class on marked message container
    const markedContainer = container.querySelector('.bg-amber-50');
    expect(markedContainer).toBeInTheDocument();
  });

  it('shows mark button on hover when onToggleMark is provided', async () => {
    const onToggleMark = vi.fn();
    render(<MessageList {...defaultProps} onToggleMark={onToggleMark} />);

    // Hover over first message to show buttons
    const messageContainer = screen.getByText('Hello world').closest('.group');
    if (messageContainer) {
      fireEvent.mouseEnter(messageContainer);
    }

    // Mark button should appear
    expect(screen.getByTitle('标记重要')).toBeInTheDocument();
  });

  it('shows unmark button on hover for marked messages', async () => {
    const onToggleMark = vi.fn();
    render(<MessageList {...defaultProps} onToggleMark={onToggleMark} />);

    // Hover over second (marked) message to show buttons
    const messageContainer = screen.getByText('Hi there!').closest('.group');
    if (messageContainer) {
      fireEvent.mouseEnter(messageContainer);
    }

    // Unmark button should appear
    expect(screen.getByTitle('取消标记')).toBeInTheDocument();
  });

  it('calls onToggleMark when mark button is clicked', async () => {
    const onToggleMark = vi.fn();
    render(<MessageList {...defaultProps} onToggleMark={onToggleMark} />);

    // Hover over first message
    const messageContainer = screen.getByText('Hello world').closest('.group');
    if (messageContainer) {
      fireEvent.mouseEnter(messageContainer);
    }

    // Click mark button
    const markButton = screen.getByTitle('标记重要');
    fireEvent.click(markButton);

    expect(onToggleMark).toHaveBeenCalledWith(1);
  });

  it('does not show mark button when onToggleMark is not provided', () => {
    render(<MessageList {...defaultProps} />);

    // Hover over first message
    const messageContainer = screen.getByText('Hello world').closest('.group');
    if (messageContainer) {
      fireEvent.mouseEnter(messageContainer);
    }

    // Mark button should not appear
    expect(screen.queryByTitle('标记重要')).not.toBeInTheDocument();
  });
});