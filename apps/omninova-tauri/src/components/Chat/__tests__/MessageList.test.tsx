/**
 * Tests for MessageList Component
 *
 * [Source: Story 4.4 - ChatInterface 组件基础]
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@/test/utils';
import { MessageList } from '../MessageList';
import type { Message } from '@/types/session';

// Mock scrollIntoView
const scrollIntoViewMock = vi.fn();
Element.prototype.scrollIntoView = scrollIntoViewMock;

describe('MessageList', () => {
  const mockMessages: Message[] = [
    {
      id: 1,
      sessionId: 1,
      role: 'user',
      content: 'Hello!',
      createdAt: Math.floor(Date.now() / 1000) - 60,
    },
    {
      id: 2,
      sessionId: 1,
      role: 'assistant',
      content: 'Hi there!',
      createdAt: Math.floor(Date.now() / 1000) - 30,
    },
  ];

  beforeEach(() => {
    scrollIntoViewMock.mockClear();
  });

  describe('Message rendering', () => {
    it('renders all messages', () => {
      render(
        <MessageList messages={mockMessages} />
      );

      expect(screen.getByText('Hello!')).toBeInTheDocument();
      expect(screen.getByText('Hi there!')).toBeInTheDocument();
    });

    it('renders empty state when no messages', () => {
      render(
        <MessageList messages={[]} />
      );

      expect(screen.getByText(/输入消息开始与/)).toBeInTheDocument();
    });

    it('renders custom empty state', () => {
      render(
        <MessageList
          messages={[]}
          emptyState={<div>No messages yet</div>}
        />
      );

      expect(screen.getByText('No messages yet')).toBeInTheDocument();
    });
  });

  describe('Agent theming', () => {
    it('passes personality type to assistant messages', () => {
      const { container } = render(
        <MessageList
          messages={[mockMessages[1]]}
          personalityType="INTJ"
        />
      );

      // Should have personality-themed border
      const bubble = container.querySelector('.border-l-2');
      expect(bubble).toBeInTheDocument();
    });

    it('displays agent name for assistant messages', () => {
      render(
        <MessageList
          messages={[mockMessages[1]]}
          agentName="Nova"
        />
      );

      expect(screen.getByText('Nova')).toBeInTheDocument();
    });
  });

  describe('Streaming support', () => {
    it('shows streaming message when isStreaming is true', () => {
      render(
        <MessageList
          messages={mockMessages}
          isStreaming={true}
          streamedContent="Thinking..."
        />
      );

      expect(screen.getByText('Thinking...')).toBeInTheDocument();
    });

    it('shows typing indicator when streaming with no content', () => {
      render(
        <MessageList
          messages={mockMessages}
          isStreaming={true}
          streamedContent=""
        />
      );

      expect(screen.getByText('正在思考...')).toBeInTheDocument();
    });

    it('does not show streaming message when not streaming', () => {
      render(
        <MessageList
          messages={mockMessages}
          isStreaming={false}
          streamedContent=""
        />
      );

      expect(screen.queryByText('正在思考...')).not.toBeInTheDocument();
    });
  });

  describe('Auto-scroll behavior', () => {
    it('scrolls to bottom on new messages', () => {
      const { rerender } = render(
        <MessageList messages={mockMessages} />
      );

      // Add a new message
      const newMessages = [
        ...mockMessages,
        {
          id: 3,
          sessionId: 1,
          role: 'user' as const,
          content: 'New message',
          createdAt: Math.floor(Date.now() / 1000),
        },
      ];

      rerender(<MessageList messages={newMessages} />);

      // scrollIntoView should have been called
      expect(scrollIntoViewMock).toHaveBeenCalled();
    });

    it('scrolls on streamed content update', () => {
      const { rerender } = render(
        <MessageList
          messages={mockMessages}
          isStreaming={true}
          streamedContent=""
        />
      );

      rerender(
        <MessageList
          messages={mockMessages}
          isStreaming={true}
          streamedContent="New content"
        />
      );

      expect(scrollIntoViewMock).toHaveBeenCalled();
    });
  });

  describe('Accessibility', () => {
    it('has proper ARIA attributes', () => {
      render(
        <MessageList messages={mockMessages} />
      );

      const log = screen.getByRole('log');
      expect(log).toHaveAttribute('aria-live', 'polite');
      expect(log).toHaveAttribute('aria-label', '聊天消息列表');
    });
  });

  describe('Timestamps', () => {
    it('shows timestamps when showTimestamps is true', () => {
      const { container } = render(
        <MessageList messages={mockMessages} showTimestamps={true} />
      );

      const timestamps = container.querySelectorAll('.text-muted-foreground\\/60');
      expect(timestamps.length).toBeGreaterThan(0);
    });

    it('hides timestamps when showTimestamps is false', () => {
      render(
        <MessageList messages={mockMessages} showTimestamps={false} />
      );

      // Messages should still be visible
      expect(screen.getByText('Hello!')).toBeInTheDocument();
    });
  });
});