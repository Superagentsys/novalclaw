/**
 * MemoryLayerIndicator Component Tests
 *
 * Tests for the memory layer status indicator component.
 *
 * [Source: Story 5.6 - MemoryLayerIndicator 组件]
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryLayerIndicator, type MemoryLayerIndicatorProps } from '../MemoryLayerIndicator';
import type { MemoryManagerStats } from '@/types/memory';

// Mock the cn utility
vi.mock('@/lib/utils', () => ({
  cn: (...args: (string | boolean | undefined)[]) => args.filter(Boolean).join(' '),
}));

describe('MemoryLayerIndicator (Story 5.6)', () => {
  // ============================================================================
  // Default Rendering
  // ============================================================================

  describe('Default Rendering', () => {
    it('should render without stats', () => {
      render(<MemoryLayerIndicator />);

      // Should show default L1 capacity (0/10)
      expect(screen.getByText('L1')).toBeInTheDocument();
      expect(screen.getByText('L2')).toBeInTheDocument();
      expect(screen.getByText('L3')).toBeInTheDocument();
    });

    it('should render with stats', () => {
      const stats: MemoryManagerStats = {
        l1Capacity: 20,
        l1Used: 8,
        l1SessionId: 1,
        l2Total: 150,
        l2AvgImportance: 7.5,
        l3Total: 42,
      };

      render(<MemoryLayerIndicator stats={stats} />);

      expect(screen.getByText('8/20')).toBeInTheDocument();
      expect(screen.getByText('150')).toBeInTheDocument();
      expect(screen.getByText('42')).toBeInTheDocument();
    });
  });

  // ============================================================================
  // L1 Layer Indicator
  // ============================================================================

  describe('L1 Layer Indicator', () => {
    it('should display L1 usage correctly', () => {
      const stats: MemoryManagerStats = {
        l1Capacity: 10,
        l1Used: 5,
        l1SessionId: null,
        l2Total: 0,
        l2AvgImportance: 0,
        l3Total: 0,
      };

      render(<MemoryLayerIndicator stats={stats} />);

      expect(screen.getByText('5/10')).toBeInTheDocument();
    });

    it('should show active highlight when L1 is active', () => {
      const stats: MemoryManagerStats = {
        l1Capacity: 10,
        l1Used: 5,
        l1SessionId: 1,
        l2Total: 0,
        l2AvgImportance: 0,
        l3Total: 0,
      };

      const { container } = render(
        <MemoryLayerIndicator stats={stats} activeLayer="L1" />
      );

      // L1 container should have active styling
      const l1Container = container.querySelector('.bg-blue-100');
      expect(l1Container).toBeInTheDocument();
    });

    it('should show pulse animation when retrieving L1', () => {
      const stats: MemoryManagerStats = {
        l1Capacity: 10,
        l1Used: 5,
        l1SessionId: 1,
        l2Total: 0,
        l2AvgImportance: 0,
        l3Total: 0,
      };

      const { container } = render(
        <MemoryLayerIndicator
          stats={stats}
          activeLayer="L1"
          isRetrieving={true}
        />
      );

      // Should have animate-pulse class
      const animatedElement = container.querySelector('.animate-pulse');
      expect(animatedElement).toBeInTheDocument();
    });
  });

  // ============================================================================
  // L2 Layer Indicator
  // ============================================================================

  describe('L2 Layer Indicator', () => {
    it('should display L2 count correctly', () => {
      const stats: MemoryManagerStats = {
        l1Capacity: 10,
        l1Used: 0,
        l1SessionId: null,
        l2Total: 156,
        l2AvgImportance: 6.8,
        l3Total: 0,
      };

      render(<MemoryLayerIndicator stats={stats} />);

      expect(screen.getByText('156')).toBeInTheDocument();
      // Should show avg importance
      expect(screen.getByText('(7)')).toBeInTheDocument();
    });

    it('should show active highlight when L2 is active', () => {
      const stats: MemoryManagerStats = {
        l1Capacity: 10,
        l1Used: 0,
        l1SessionId: null,
        l2Total: 100,
        l2AvgImportance: 5,
        l3Total: 0,
      };

      const { container } = render(
        <MemoryLayerIndicator stats={stats} activeLayer="L2" />
      );

      const l2Container = container.querySelector('.bg-green-100');
      expect(l2Container).toBeInTheDocument();
    });
  });

  // ============================================================================
  // L3 Layer Indicator
  // ============================================================================

  describe('L3 Layer Indicator', () => {
    it('should display L3 count correctly', () => {
      const stats: MemoryManagerStats = {
        l1Capacity: 10,
        l1Used: 0,
        l1SessionId: null,
        l2Total: 0,
        l2AvgImportance: 0,
        l3Total: 42,
      };

      render(<MemoryLayerIndicator stats={stats} />);

      expect(screen.getByText('42')).toBeInTheDocument();
    });

    it('should show active highlight when L3 is active', () => {
      const stats: MemoryManagerStats = {
        l1Capacity: 10,
        l1Used: 0,
        l1SessionId: null,
        l2Total: 0,
        l2AvgImportance: 0,
        l3Total: 30,
      };

      const { container } = render(
        <MemoryLayerIndicator stats={stats} activeLayer="L3" />
      );

      const l3Container = container.querySelector('.bg-purple-100');
      expect(l3Container).toBeInTheDocument();
    });

    it('should show offline indicator when stats are null', () => {
      const { container } = render(
        <MemoryLayerIndicator stats={null} />
      );

      // L3 should be dimmed when unavailable
      const l3Container = container.querySelector('.opacity-50');
      expect(l3Container).toBeInTheDocument();
    });
  });

  // ============================================================================
  // Retrieval Animation
  // ============================================================================

  describe('Retrieval Animation', () => {
    it('should show retrieval indicator when isRetrieving is true', () => {
      render(
        <MemoryLayerIndicator
          activeLayer="L2"
          isRetrieving={true}
        />
      );

      expect(screen.getByText(/检索中/)).toBeInTheDocument();
    });

    it('should not show retrieval indicator when isRetrieving is false', () => {
      render(
        <MemoryLayerIndicator
          activeLayer="L1"
          isRetrieving={false}
        />
      );

      expect(screen.queryByText(/检索中/)).not.toBeInTheDocument();
    });

    it('should not show retrieval indicator when activeLayer is null', () => {
      render(
        <MemoryLayerIndicator
          activeLayer={null}
          isRetrieving={true}
        />
      );

      expect(screen.queryByText(/检索中/)).not.toBeInTheDocument();
    });
  });

  // ============================================================================
  // Compact Mode
  // ============================================================================

  describe('Compact Mode', () => {
    it('should hide detailed stats in compact mode', () => {
      const stats: MemoryManagerStats = {
        l1Capacity: 20,
        l1Used: 10,
        l1SessionId: 1,
        l2Total: 150,
        l2AvgImportance: 7.5,
        l3Total: 42,
      };

      render(<MemoryLayerIndicator stats={stats} compact={true} />);

      // Should show layer labels
      expect(screen.getByText('L1')).toBeInTheDocument();
      expect(screen.getByText('L2')).toBeInTheDocument();
      expect(screen.getByText('L3')).toBeInTheDocument();

      // Should NOT show detailed stats
      expect(screen.queryByText('10/20')).not.toBeInTheDocument();
      expect(screen.queryByText('150')).not.toBeInTheDocument();
      expect(screen.queryByText('42')).not.toBeInTheDocument();
    });
  });

  // ============================================================================
  // Accessibility
  // ============================================================================

  describe('Accessibility', () => {
    it('should have role="status"', () => {
      const { container } = render(<MemoryLayerIndicator />);

      const statusElement = container.querySelector('[role="status"]');
      expect(statusElement).toBeInTheDocument();
    });

    it('should have aria-label', () => {
      const { container } = render(<MemoryLayerIndicator />);

      const statusElement = container.querySelector('[aria-label="记忆系统状态"]');
      expect(statusElement).toBeInTheDocument();
    });
  });
});