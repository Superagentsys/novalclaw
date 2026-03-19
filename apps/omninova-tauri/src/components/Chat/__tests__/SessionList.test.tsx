/**
 * Tests for SessionList Component
 *
 * [Source: Story 4.7 - 对话历史持久化与导航]
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { SessionList } from '../SessionList';
import type { Session } from '@/types/session';

describe('SessionList', () => {
  const mockSessions: Session[] = [
    {
      id: 1,
      agentId: 1,
      title: 'Session 1',
      createdAt: 1000,
      updatedAt: Math.floor(Date.now() / 1000) - 3600,
    },
    {
      id: 2,
      agentId: 1,
      title: 'Session 2',
      createdAt: 2000,
      updatedAt: Math.floor(Date.now() / 1000) - 7200,
    },
    {
      id: 3,
      agentId: 1,
      title: 'Session 3',
      createdAt: 3000,
      updatedAt: Math.floor(Date.now() / 1000) - 1800,
    },
  ];

  const defaultProps = {
    sessions: mockSessions,
    activeSessionId: null,
    onSelectSession: vi.fn(),
    onCreateSession: vi.fn(),
  };

  describe('Rendering', () => {
    it('renders all sessions', () => {
      render(<SessionList {...defaultProps} />);

      expect(screen.getByText('Session 1')).toBeInTheDocument();
      expect(screen.getByText('Session 2')).toBeInTheDocument();
      expect(screen.getByText('Session 3')).toBeInTheDocument();
    });

    it('renders "新建会话" button', () => {
      render(<SessionList {...defaultProps} />);

      expect(screen.getByRole('button', { name: /创建新会话/ })).toBeInTheDocument();
    });
  });

  describe('Active Session', () => {
    it('highlights active session', () => {
      render(<SessionList {...defaultProps} activeSessionId={2} />);

      const activeOption = screen.getByRole('option', { selected: true });
      expect(activeOption).toHaveTextContent('Session 2');
    });

    it('no session is selected when activeSessionId is null', () => {
      render(<SessionList {...defaultProps} activeSessionId={null} />);

      const selectedOptions = screen.queryAllByRole('option', { selected: true });
      expect(selectedOptions).toHaveLength(0);
    });
  });

  describe('Interactions', () => {
    it('calls onSelectSession when session is clicked', () => {
      const handleSelect = vi.fn();
      render(<SessionList {...defaultProps} onSelectSession={handleSelect} />);

      fireEvent.click(screen.getByText('Session 1'));
      expect(handleSelect).toHaveBeenCalledWith(1);
    });

    it('calls onCreateSession when new session button is clicked', () => {
      const handleCreate = vi.fn();
      render(<SessionList {...defaultProps} onCreateSession={handleCreate} />);

      fireEvent.click(screen.getByRole('button', { name: /创建新会话/ }));
      expect(handleCreate).toHaveBeenCalledTimes(1);
    });
  });

  describe('Loading State', () => {
    it('shows loading skeleton when loading with no sessions', () => {
      render(<SessionList {...defaultProps} sessions={[]} isLoading={true} />);

      // Check for aria-busy attribute
      const listbox = screen.getByRole('listbox');
      expect(listbox).toHaveAttribute('aria-busy', 'true');
    });

    it('does not show skeleton when sessions exist', () => {
      render(<SessionList {...defaultProps} isLoading={true} />);

      // Sessions should still be visible
      expect(screen.getByText('Session 1')).toBeInTheDocument();
    });
  });

  describe('Empty State', () => {
    it('shows empty state when no sessions', () => {
      render(<SessionList {...defaultProps} sessions={[]} />);

      expect(screen.getByText('暂无对话记录')).toBeInTheDocument();
    });

    it('shows "开始新对话" link in empty state', () => {
      render(<SessionList {...defaultProps} sessions={[]} />);

      expect(screen.getByText('开始新对话')).toBeInTheDocument();
    });

    it('calls onCreateSession from empty state', () => {
      const handleCreate = vi.fn();
      render(<SessionList {...defaultProps} sessions={[]} onCreateSession={handleCreate} />);

      fireEvent.click(screen.getByText('开始新对话'));
      expect(handleCreate).toHaveBeenCalledTimes(1);
    });
  });

  describe('Error State', () => {
    it('shows error message when error is provided', () => {
      render(<SessionList {...defaultProps} sessions={[]} error="加载失败" />);

      expect(screen.getByText('加载失败')).toBeInTheDocument();
    });

    it('shows sessions even when error exists but sessions are available', () => {
      render(<SessionList {...defaultProps} error="加载失败" />);

      // Sessions should still be visible when they exist
      expect(screen.getByText('Session 1')).toBeInTheDocument();
    });
  });

  describe('Previews', () => {
    it('shows preview text for sessions', () => {
      const previews: Record<number, string> = {
        1: 'Preview for session 1',
        2: 'Preview for session 2',
      };
      render(<SessionList {...defaultProps} previews={previews} />);

      expect(screen.getByText('Preview for session 1')).toBeInTheDocument();
      expect(screen.getByText('Preview for session 2')).toBeInTheDocument();
    });
  });

  describe('Accessibility', () => {
    it('has region with aria-label', () => {
      render(<SessionList {...defaultProps} />);

      expect(screen.getByRole('region', { name: '会话列表' })).toBeInTheDocument();
    });

    it('has listbox with aria-label', () => {
      render(<SessionList {...defaultProps} />);

      expect(screen.getByRole('listbox', { name: '会话列表' })).toBeInTheDocument();
    });

    it('has aria-busy on listbox during loading', () => {
      render(<SessionList {...defaultProps} sessions={[]} isLoading={true} />);

      expect(screen.getByRole('listbox')).toHaveAttribute('aria-busy', 'true');
    });
  });

  describe('Custom Styling', () => {
    it('accepts custom className', () => {
      const { container } = render(<SessionList {...defaultProps} className="custom-class" />);

      expect(container.firstChild).toHaveClass('custom-class');
    });
  });
});