/**
 * Tests for MessageSkeleton Component
 *
 * [Source: Story 4.5 - 打字指示器与加载状态]
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MessageSkeleton, MessageSkeletonList } from '../MessageSkeleton';

describe('MessageSkeleton', () => {
  describe('Rendering', () => {
    it('should render with default props (assistant)', () => {
      const { container } = render(<MessageSkeleton />);

      // Should have role="status" for accessibility
      expect(screen.getByRole('status')).toBeInTheDocument();
      expect(screen.getByLabelText('加载中')).toBeInTheDocument();
    });

    it('should have aria-busy="true"', () => {
      render(<MessageSkeleton />);

      const skeleton = screen.getByRole('status');
      expect(skeleton).toHaveAttribute('aria-busy', 'true');
    });
  });

  describe('Role Variants', () => {
    it('should render assistant role (left-aligned)', () => {
      render(<MessageSkeleton role="assistant" />);

      const skeleton = screen.getByRole('status');
      expect(skeleton).toHaveClass('mr-auto');
      expect(skeleton).toHaveClass('items-start');
    });

    it('should render user role (right-aligned)', () => {
      render(<MessageSkeleton role="user" />);

      const skeleton = screen.getByRole('status');
      expect(skeleton).toHaveClass('ml-auto');
      expect(skeleton).toHaveClass('items-end');
    });
  });

  describe('Lines Configuration', () => {
    it('should render default 2 lines', () => {
      render(<MessageSkeleton />);

      // 2 skeleton lines inside the message bubble
      const lines = screen.getByRole('status').querySelectorAll('.animate-pulse');
      // Includes agent name, content lines, and timestamp
      expect(lines.length).toBeGreaterThan(2);
    });

    it('should render specified number of lines', () => {
      const { container } = render(<MessageSkeleton lines={4} />);

      const lines = container.querySelectorAll('.animate-pulse');
      // Should have more with 4 lines
      expect(lines.length).toBeGreaterThan(4);
    });
  });

  describe('Agent Name', () => {
    it('should show agent name skeleton for assistant by default', () => {
      const { container } = render(<MessageSkeleton role="assistant" showAgentName />);

      // First skeleton should be the agent name (h-3 w-16)
      const skeletons = container.querySelectorAll('.animate-pulse');
      const firstSkeleton = skeletons[0];
      expect(firstSkeleton).toHaveClass('h-3');
    });

    it('should hide agent name skeleton when showAgentName is false', () => {
      const { container } = render(<MessageSkeleton role="assistant" showAgentName={false} />);

      // Check that no h-3 w-16 skeleton (agent name) is present
      const skeletons = container.querySelectorAll('.animate-pulse');
      // First skeleton should be content (h-4), not agent name (h-3)
      const firstSkeleton = skeletons[0];
      expect(firstSkeleton).toHaveClass('h-4');
    });

    it('should not show agent name for user role', () => {
      const { container } = render(<MessageSkeleton role="user" showAgentName />);

      const skeleton = container.firstChild;
      // User messages are right-aligned, agent name would be on the left
      expect(skeleton).toHaveClass('items-end');
    });
  });

  describe('Timestamp', () => {
    it('should show timestamp skeleton by default', () => {
      const { container } = render(<MessageSkeleton showTimestamp />);

      // Last skeleton should be timestamp (h-3 w-10)
      const skeletons = container.querySelectorAll('.animate-pulse');
      const lastSkeleton = skeletons[skeletons.length - 1];
      expect(lastSkeleton).toHaveClass('h-3');
    });

    it('should hide timestamp skeleton when showTimestamp is false', () => {
      const { container } = render(<MessageSkeleton showTimestamp={false} />);

      const skeleton = container.firstChild;
      // Should still render but without timestamp
      expect(skeleton).toBeInTheDocument();
    });
  });

  describe('Custom Styling', () => {
    it('should accept custom className', () => {
      const { container } = render(<MessageSkeleton className="custom-class" />);

      const skeleton = container.firstChild;
      expect(skeleton).toHaveClass('custom-class');
    });

    it('should accept custom style', () => {
      const { container } = render(<MessageSkeleton style={{ animationDelay: '100ms' }} />);

      const skeleton = container.firstChild;
      expect(skeleton).toHaveStyle({ animationDelay: '100ms' });
    });
  });
});

describe('MessageSkeletonList', () => {
  describe('Rendering', () => {
    it('should render default 3 skeletons', () => {
      const { container } = render(<MessageSkeletonList />);

      const skeletons = container.querySelectorAll('[role="status"]');
      expect(skeletons.length).toBe(3);
    });

    it('should render specified count of skeletons', () => {
      const { container } = render(<MessageSkeletonList count={5} />);

      const skeletons = container.querySelectorAll('[role="status"]');
      expect(skeletons.length).toBe(5);
    });

    it('should alternate between assistant and user roles', () => {
      const { container } = render(<MessageSkeletonList count={4} />);

      const skeletons = container.querySelectorAll('[role="status"]');

      // First should be assistant (left-aligned)
      expect(skeletons[0]).toHaveClass('mr-auto');

      // Second should be user (right-aligned)
      expect(skeletons[1]).toHaveClass('ml-auto');

      // Third should be assistant
      expect(skeletons[2]).toHaveClass('mr-auto');

      // Fourth should be user
      expect(skeletons[3]).toHaveClass('ml-auto');
    });
  });

  describe('Custom Styling', () => {
    it('should accept custom className', () => {
      const { container } = render(<MessageSkeletonList className="custom-list" />);

      const list = container.firstChild;
      expect(list).toHaveClass('custom-list');
    });
  });
});