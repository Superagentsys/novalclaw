/**
 * Tests for MemoryItem mark functionality
 *
 * [Source: Story 5.8 - 重要片段标记功能]
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MemoryVisualization } from '../MemoryVisualization';

// Mock useMemoryData hook
vi.mock('@/hooks/useMemoryData', () => ({
  useMemoryData: vi.fn(),
}));

import { useMemoryData } from '@/hooks/useMemoryData';

const mockUseMemoryData = vi.mocked(useMemoryData);

// Mock memory data
const createMockMemory = (id: string, overrides = {}) => ({
  id,
  content: `Memory content ${id}`,
  role: null,
  importance: 5,
  isMarked: false,
  sessionId: null,
  createdAt: Date.now() / 1000,
  sourceLayer: 'L2' as const,
  similarityScore: null,
  ...overrides,
});

describe('MemoryItem Mark Functionality', () => {
  const defaultHookReturn = {
    memories: [],
    isLoading: false,
    error: null,
    refresh: vi.fn(),
    loadMore: vi.fn(),
    hasMore: false,
    total: 0,
    deleteMemory: vi.fn().mockResolvedValue(true),
    markMemory: vi.fn().mockResolvedValue(true),
    unmarkMemory: vi.fn().mockResolvedValue(true),
    searchQuery: '',
    setSearchQuery: vi.fn(),
    timeRange: 'all' as const,
    setTimeRange: vi.fn(),
    minImportance: 1,
    setMinImportance: vi.fn(),
    showMarkedOnly: false,
    setShowMarkedOnly: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockUseMemoryData.mockReturnValue(defaultHookReturn);
  });

  const defaultProps = {
    agentId: 1,
  };

  it('shows star indicator for marked memories', () => {
    const markedMemory = createMockMemory('l2-1', { isMarked: true });
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories: [markedMemory],
      total: 1,
    });

    render(<MemoryVisualization {...defaultProps} />);

    // Should show star for marked memory
    expect(screen.getByText('⭐')).toBeInTheDocument();
  });

  it('does not show star for unmarked memories', () => {
    const unmarkedMemory = createMockMemory('l2-1', { isMarked: false });
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories: [unmarkedMemory],
      total: 1,
    });

    render(<MemoryVisualization {...defaultProps} />);

    // Should not show star
    expect(screen.queryByText('⭐')).not.toBeInTheDocument();
  });

  it('shows mark button for L2 memories', () => {
    const memory = createMockMemory('l2-1');
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories: [memory],
      total: 1,
    });

    render(<MemoryVisualization {...defaultProps} />);

    expect(screen.getByText('标记重要')).toBeInTheDocument();
  });

  it('shows unmark button for marked L2 memories', () => {
    const markedMemory = createMockMemory('l2-1', { isMarked: true });
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories: [markedMemory],
      total: 1,
    });

    render(<MemoryVisualization {...defaultProps} />);

    expect(screen.getByText('取消标记')).toBeInTheDocument();
  });

  it('calls markMemory when mark button is clicked', async () => {
    const markMemory = vi.fn().mockResolvedValue(true);
    const memory = createMockMemory('l2-1');
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories: [memory],
      total: 1,
      markMemory,
    });

    render(<MemoryVisualization {...defaultProps} />);

    fireEvent.click(screen.getByText('标记重要'));

    expect(markMemory).toHaveBeenCalledWith('l2-1');
  });

  it('calls unmarkMemory when unmark button is clicked', async () => {
    const unmarkMemory = vi.fn().mockResolvedValue(true);
    const markedMemory = createMockMemory('l2-1', { isMarked: true });
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories: [markedMemory],
      total: 1,
      unmarkMemory,
    });

    render(<MemoryVisualization {...defaultProps} />);

    fireEvent.click(screen.getByText('取消标记'));

    expect(unmarkMemory).toHaveBeenCalledWith('l2-1');
  });

  it('applies marked highlight style to marked memories', () => {
    const markedMemory = createMockMemory('l2-1', { isMarked: true });
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories: [markedMemory],
      total: 1,
    });

    const { container } = render(<MemoryVisualization {...defaultProps} />);

    // Check for amber border class on marked memory
    const memoryCard = container.querySelector('.border-amber-300');
    expect(memoryCard).toBeInTheDocument();
  });

  it('does not show mark button for L1 memories', () => {
    const l1Memory = createMockMemory('l1-1', { sourceLayer: 'L1' });
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories: [l1Memory],
      total: 1,
      layer: 'L1',
    });

    render(<MemoryVisualization {...defaultProps} />);

    // L1 doesn't support marking
    expect(screen.queryByText('标记重要')).not.toBeInTheDocument();
    expect(screen.queryByText('取消标记')).not.toBeInTheDocument();
  });
});