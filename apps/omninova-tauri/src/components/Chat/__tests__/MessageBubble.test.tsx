/**
 * Tests for MessageBubble Component
 *
 * [Source: Story 4.4 - ChatInterface 组件基础]
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@/test/utils';
import { MessageBubble } from '../MessageBubble';
import type { MessageRole } from '@/types/session';
import type { MBTIType } from '@/lib/personality-colors';

// Mock the personality-colors module
vi.mock('@/lib/personality-colors', () => ({
  getPersonalityColors: (type: MBTIType) => ({
    primary: `hsl(${type === 'INTJ' ? 220 : 180}, 70%, 50%)`,
    accent: `hsl(${type === 'INTJ' ? 220 : 180}, 70%, 70%)`,
    tone: 'analytical' as const,
  }),
}));

describe('MessageBubble', () => {
  const defaultProps = {
    content: 'Hello, world!',
    role: 'user' as MessageRole,
    timestamp: Math.floor(Date.now() / 1000),
  };

  describe('Role-based styling', () => {
    it('renders user messages with right alignment', () => {
      const { container } = render(
        <MessageBubble {...defaultProps} role="user" />
      );

      const wrapper = container.firstChild as HTMLElement;
      expect(wrapper).toHaveClass('ml-auto');
      expect(wrapper).toHaveClass('items-end');
    });

    it('renders assistant messages with left alignment', () => {
      const { container } = render(
        <MessageBubble {...defaultProps} role="assistant" />
      );

      const wrapper = container.firstChild as HTMLElement;
      expect(wrapper).toHaveClass('mr-auto');
      expect(wrapper).toHaveClass('items-start');
    });

    it('renders system messages with centered styling', () => {
      const { container } = render(
        <MessageBubble {...defaultProps} role="system" />
      );

      const bubble = container.querySelector('.bg-muted\\/30');
      expect(bubble).toBeInTheDocument();
    });
  });

  describe('Personality theming', () => {
    it('applies personality colors for assistant messages', () => {
      const { container } = render(
        <MessageBubble
          {...defaultProps}
          role="assistant"
          personalityType="INTJ"
        />
      );

      // Should have the border-l-2 class for personality styling
      const bubble = container.querySelector('.border-l-2');
      expect(bubble).toBeInTheDocument();
    });

    it('does not apply personality colors for user messages', () => {
      const { container } = render(
        <MessageBubble
          {...defaultProps}
          role="user"
          personalityType="INTJ"
        />
      );

      // User messages should not have personality border
      const bubble = container.querySelector('.border-l-2');
      expect(bubble).not.toBeInTheDocument();
    });
  });

  describe('Timestamp display', () => {
    it('shows timestamp when showTimestamp is true', () => {
      render(
        <MessageBubble
          {...defaultProps}
          showTimestamp={true}
        />
      );

      // Timestamp should be visible (format: HH:MM)
      const timePattern = /\d{2}:\d{2}/;
      expect(screen.queryByText(timePattern)).not.toBeNull();
    });

    it('hides timestamp when showTimestamp is false', () => {
      const { container } = render(
        <MessageBubble
          {...defaultProps}
          showTimestamp={false}
        />
      );

      const timestamp = container.querySelector('.text-muted-foreground\\/60');
      expect(timestamp).not.toBeInTheDocument();
    });
  });

  describe('Agent attribution', () => {
    it('shows agent name for assistant messages', () => {
      render(
        <MessageBubble
          {...defaultProps}
          role="assistant"
          agentName="Nova"
        />
      );

      expect(screen.getByText('Nova')).toBeInTheDocument();
    });

    it('does not show agent name for user messages', () => {
      render(
        <MessageBubble
          {...defaultProps}
          role="user"
          agentName="Nova"
        />
      );

      expect(screen.queryByText('Nova')).not.toBeInTheDocument();
    });
  });

  describe('Content rendering', () => {
    it('renders plain text content', () => {
      render(
        <MessageBubble {...defaultProps} content="Plain text message" />
      );

      expect(screen.getByText('Plain text message')).toBeInTheDocument();
    });

    it('renders inline code', () => {
      render(
        <MessageBubble {...defaultProps} content="Use `code` here" />
      );

      const code = screen.getByText('code');
      expect(code.tagName).toBe('CODE');
    });

    it('renders bold text', () => {
      render(
        <MessageBubble {...defaultProps} content="This is **bold** text" />
      );

      const bold = screen.getByText('bold');
      expect(bold.tagName).toBe('STRONG');
    });

    it('renders code blocks', () => {
      render(
        <MessageBubble
          {...defaultProps}
          content="```javascript\nconst x = 1;\n```"
        />
      );

      const pre = document.querySelector('pre');
      expect(pre).toBeInTheDocument();
      expect(screen.getByText('javascript')).toBeInTheDocument();
    });
  });
});