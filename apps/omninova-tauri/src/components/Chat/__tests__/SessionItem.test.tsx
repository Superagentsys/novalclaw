/**
 * Tests for SessionItem Component
 *
 * [Source: Story 4.7 - 对话历史持久化与导航]
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { SessionItem } from '../SessionItem';
import type { Session } from '@/types/session';

describe('SessionItem', () => {
  const mockSession: Session = {
    id: 1,
    agentId: 1,
    title: 'Test Session',
    createdAt: Math.floor(Date.now() / 1000) - 3600, // 1 hour ago
    updatedAt: Math.floor(Date.now() / 1000) - 3600,
  };

  describe('Rendering', () => {
    it('renders session title', () => {
      render(
        <SessionItem
          session={mockSession}
          isActive={false}
          onClick={() => {}}
        />
      );

      expect(screen.getByText('Test Session')).toBeInTheDocument();
    });

    it('shows "新对话" for untitled sessions', () => {
      const untitledSession = { ...mockSession, title: undefined };
      render(
        <SessionItem
          session={untitledSession}
          isActive={false}
          onClick={() => {}}
        />
      );

      expect(screen.getByText('新对话')).toBeInTheDocument();
    });

    it('displays relative time', () => {
      render(
        <SessionItem
          session={mockSession}
          isActive={false}
          onClick={() => {}}
        />
      );

      // Should show time ago
      expect(screen.getByText(/小时前/)).toBeInTheDocument();
    });

    it('shows preview text when provided', () => {
      render(
        <SessionItem
          session={mockSession}
          isActive={false}
          onClick={() => {}}
          preview="Last message preview..."
        />
      );

      expect(screen.getByText('Last message preview...')).toBeInTheDocument();
    });

    it('does not show preview when not provided', () => {
      render(
        <SessionItem
          session={mockSession}
          isActive={false}
          onClick={() => {}}
        />
      );

      // No preview should be rendered
      const button = screen.getByRole('option');
      expect(button.textContent).not.toContain('preview');
    });
  });

  describe('Active State', () => {
    it('applies active styling when active', () => {
      render(
        <SessionItem
          session={mockSession}
          isActive={true}
          onClick={() => {}}
        />
      );

      const button = screen.getByRole('option');
      expect(button).toHaveAttribute('aria-selected', 'true');
      expect(button).toHaveClass('bg-primary/10');
    });

    it('applies inactive styling when not active', () => {
      render(
        <SessionItem
          session={mockSession}
          isActive={false}
          onClick={() => {}}
        />
      );

      const button = screen.getByRole('option');
      expect(button).toHaveAttribute('aria-selected', 'false');
      expect(button).toHaveClass('hover:bg-muted');
    });
  });

  describe('Interactions', () => {
    it('calls onClick when clicked', () => {
      const handleClick = vi.fn();
      render(
        <SessionItem
          session={mockSession}
          isActive={false}
          onClick={handleClick}
        />
      );

      fireEvent.click(screen.getByRole('option'));
      expect(handleClick).toHaveBeenCalledTimes(1);
    });
  });

  describe('Accessibility', () => {
    it('has role="option"', () => {
      render(
        <SessionItem
          session={mockSession}
          isActive={false}
          onClick={() => {}}
        />
      );

      expect(screen.getByRole('option')).toBeInTheDocument();
    });

    it('has aria-selected attribute', () => {
      render(
        <SessionItem
          session={mockSession}
          isActive={false}
          onClick={() => {}}
        />
      );

      expect(screen.getByRole('option')).toHaveAttribute('aria-selected');
    });
  });

  describe('Custom Styling', () => {
    it('accepts custom className', () => {
      render(
        <SessionItem
          session={mockSession}
          isActive={false}
          onClick={() => {}}
          className="custom-class"
        />
      );

      expect(screen.getByRole('option')).toHaveClass('custom-class');
    });
  });

  describe('Time Formatting', () => {
    it('shows "刚刚" for recent sessions', () => {
      const recentSession = {
        ...mockSession,
        updatedAt: Math.floor(Date.now() / 1000),
      };
      render(
        <SessionItem
          session={recentSession}
          isActive={false}
          onClick={() => {}}
        />
      );

      expect(screen.getByText('刚刚')).toBeInTheDocument();
    });

    it('shows minutes ago for sessions within an hour', () => {
      const minutesAgoSession = {
        ...mockSession,
        updatedAt: Math.floor(Date.now() / 1000) - 1800, // 30 minutes ago
      };
      render(
        <SessionItem
          session={minutesAgoSession}
          isActive={false}
          onClick={() => {}}
        />
      );

      expect(screen.getByText(/分钟前/)).toBeInTheDocument();
    });

    it('shows days ago for sessions within a week', () => {
      const daysAgoSession = {
        ...mockSession,
        updatedAt: Math.floor(Date.now() / 1000) - 86400 * 3, // 3 days ago
      };
      render(
        <SessionItem
          session={daysAgoSession}
          isActive={false}
          onClick={() => {}}
        />
      );

      expect(screen.getByText(/天前/)).toBeInTheDocument();
    });
  });
});