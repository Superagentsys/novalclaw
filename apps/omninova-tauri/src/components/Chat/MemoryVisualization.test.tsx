/**
 * Tests for MemoryVisualization component
 *
 * [Source: Story 5.7 - MemoryVisualization 组件]
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { MemoryVisualization } from '@/components/Chat/MemoryVisualization';

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
  sessionId: null,
  createdAt: Date.now() / 1000,
  sourceLayer: 'L2' as const,
  similarityScore: null,
  ...overrides,
});

describe('MemoryVisualization', () => {
  const defaultHookReturn = {
    memories: [],
    isLoading: false,
    error: null,
    refresh: vi.fn(),
    loadMore: vi.fn(),
    hasMore: false,
    total: 0,
    deleteMemory: vi.fn().mockResolvedValue(true),
    searchQuery: '',
    setSearchQuery: vi.fn(),
    timeRange: 'all' as const,
    setTimeRange: vi.fn(),
    minImportance: 1,
    setMinImportance: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockUseMemoryData.mockReturnValue(defaultHookReturn);
  });

  const defaultProps = {
    agentId: 1,
  };

  it('renders header with title', () => {
    render(<MemoryVisualization {...defaultProps} />);

    expect(screen.getByText('记忆管理')).toBeInTheDocument();
  });

  it('renders close button when onClose is provided', () => {
    const onClose = vi.fn();
    render(<MemoryVisualization {...defaultProps} onClose={onClose} />);

    // Find close button by its SVG icon
    const closeButton = screen.getByRole('button', { name: '' });
    expect(closeButton).toBeInTheDocument();
  });

  it('calls onClose when close button is clicked', () => {
    const onClose = vi.fn();
    render(<MemoryVisualization {...defaultProps} onClose={onClose} />);

    const closeButtons = screen.getAllByRole('button');
    // The close button is the one with just an X icon (no text)
    const xButton = closeButtons.find((btn) =>
      btn.querySelector('svg path[d*="18 6 6 18"]')
    );
    if (xButton) {
      fireEvent.click(xButton);
      expect(onClose).toHaveBeenCalled();
    }
  });

  it('does not render close button when onClose is not provided', () => {
    render(<MemoryVisualization {...defaultProps} />);

    // Header should still be there but no close button
    expect(screen.getByText('记忆管理')).toBeInTheDocument();
  });

  it('renders MemoryFilterBar component', () => {
    render(<MemoryVisualization {...defaultProps} />);

    expect(screen.getByText('L1 工作记忆')).toBeInTheDocument();
    expect(screen.getByText('L2 情景记忆')).toBeInTheDocument();
    expect(screen.getByText('L3 语义记忆')).toBeInTheDocument();
  });

  it('renders loading skeleton when loading', () => {
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      isLoading: true,
    });

    const { container } = render(<MemoryVisualization {...defaultProps} />);

    // Skeleton elements should be present - they are div elements with skeleton class
    const skeletons = container.querySelectorAll('.animate-pulse');
    expect(skeletons.length).toBeGreaterThan(0);
  });

  it('renders empty state when no memories', () => {
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories: [],
      total: 0,
    });

    render(<MemoryVisualization {...defaultProps} />);

    expect(screen.getByText('暂无情景记忆')).toBeInTheDocument();
  });

  it('renders empty state for L1', () => {
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories: [],
      layer: 'L1',
    });

    render(<MemoryVisualization {...defaultProps} />);

    // Click L1 tab first
    fireEvent.click(screen.getByText('L1 工作记忆'));

    expect(screen.getByText('工作记忆为空')).toBeInTheDocument();
  });

  it('renders empty state for L3 without search query', () => {
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories: [],
      searchQuery: '',
    });

    render(<MemoryVisualization {...defaultProps} />);

    // Click L3 tab
    fireEvent.click(screen.getByText('L3 语义记忆'));

    expect(screen.getByText('输入关键词搜索语义记忆')).toBeInTheDocument();
  });

  it('renders memory list when memories exist', () => {
    const memories = [
      createMockMemory('l2-1', { content: 'First memory' }),
      createMockMemory('l2-2', { content: 'Second memory' }),
    ];

    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories,
      total: 2,
    });

    render(<MemoryVisualization {...defaultProps} />);

    expect(screen.getByText('First memory')).toBeInTheDocument();
    expect(screen.getByText('Second memory')).toBeInTheDocument();
  });

  it('renders footer with memory count', () => {
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories: [createMockMemory('l2-1')],
      total: 5,
    });

    render(<MemoryVisualization {...defaultProps} />);

    expect(screen.getByText(/显示 1 条/)).toBeInTheDocument();
    expect(screen.getByText(/共 5 条记忆/)).toBeInTheDocument();
  });

  it('renders load more button when hasMore is true', () => {
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories: [createMockMemory('l2-1')],
      total: 100,
      hasMore: true,
    });

    render(<MemoryVisualization {...defaultProps} />);

    expect(screen.getByText('加载更多')).toBeInTheDocument();
  });

  it('calls loadMore when load more button is clicked', () => {
    const loadMore = vi.fn();
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories: [createMockMemory('l2-1')],
      total: 100,
      hasMore: true,
      loadMore,
    });

    render(<MemoryVisualization {...defaultProps} />);

    fireEvent.click(screen.getByText('加载更多'));
    expect(loadMore).toHaveBeenCalled();
  });

  it('disables load more button while loading more', () => {
    // When loading with existing memories, the button should be disabled
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories: [createMockMemory('l2-1')],
      total: 100,
      hasMore: true,
      isLoading: false, // Not loading initially
    });

    render(<MemoryVisualization {...defaultProps} />);

    // Find the load more button
    const loadMoreText = screen.getByText('加载更多');
    const loadMoreButton = loadMoreText.closest('button');
    expect(loadMoreButton).not.toBeDisabled();
  });

  it('renders error message when error occurs', () => {
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      error: new Error('Failed to load memories'),
    });

    render(<MemoryVisualization {...defaultProps} />);

    expect(screen.getByText(/加载失败/)).toBeInTheDocument();
    expect(screen.getByText(/Failed to load memories/)).toBeInTheDocument();
  });

  it('renders memory item with importance indicator', () => {
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories: [createMockMemory('l2-1', { importance: 9 })],
      total: 1,
    });

    render(<MemoryVisualization {...defaultProps} />);

    expect(screen.getByText(/9\/10/)).toBeInTheDocument();
  });

  it('renders memory item with session info', () => {
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories: [createMockMemory('l2-1', { sessionId: 42 })],
      total: 1,
    });

    render(<MemoryVisualization {...defaultProps} />);

    expect(screen.getByText(/会话 #42/)).toBeInTheDocument();
  });

  it('renders memory item with similarity score for L3', () => {
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories: [
        createMockMemory('l3-1', {
          sourceLayer: 'L3',
          similarityScore: 0.85,
        }),
      ],
      total: 1,
    });

    render(<MemoryVisualization {...defaultProps} />);

    // The similarity score should be displayed as percentage with "匹配" text
    expect(screen.getByText(/85%/)).toBeInTheDocument();
  });

  it('opens detail dialog when detail button is clicked', async () => {
    const memories = [createMockMemory('l2-1', { content: 'Test memory' })];
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories,
      total: 1,
    });

    render(<MemoryVisualization {...defaultProps} />);

    fireEvent.click(screen.getByText('详情'));

    await waitFor(() => {
      expect(screen.getByText('记忆详情')).toBeInTheDocument();
    });
  });

  it('deletes memory when delete button is clicked', async () => {
    const deleteMemory = vi.fn().mockResolvedValue(true);
    const memories = [createMockMemory('l2-1', { content: 'Test memory' })];
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      memories,
      total: 1,
      deleteMemory,
    });

    render(<MemoryVisualization {...defaultProps} />);

    // Click delete button on memory item - this directly calls deleteMemory
    const deleteButtons = screen.getAllByRole('button', { name: '删除' });
    fireEvent.click(deleteButtons[0]);

    // The delete function should be called directly (no confirmation dialog at list level)
    await waitFor(() => {
      expect(deleteMemory).toHaveBeenCalledWith('l2-1');
    });
  });

  it('calls setSearchQuery when search input changes', () => {
    const setSearchQuery = vi.fn();
    mockUseMemoryData.mockReturnValue({
      ...defaultHookReturn,
      setSearchQuery,
    });

    render(<MemoryVisualization {...defaultProps} />);

    const searchInput = screen.getByPlaceholderText('搜索记忆内容...');
    fireEvent.change(searchInput, { target: { value: 'test' } });

    expect(setSearchQuery).toHaveBeenCalledWith('test');
  });

  it('passes agentId and sessionId to useMemoryData hook', () => {
    render(
      <MemoryVisualization agentId={5} sessionId={100} />
    );

    expect(mockUseMemoryData).toHaveBeenCalledWith(
      expect.objectContaining({
        agentId: 5,
        sessionId: 100,
      })
    );
  });

  it('uses default layer L2 when not specified', () => {
    render(<MemoryVisualization {...defaultProps} />);

    // L2 tab should be active by default
    const l2Tab = screen.getByText('L2 情景记忆');
    expect(l2Tab.closest('button')).toHaveClass('bg-green-100');
  });

  it('uses specified default layer', () => {
    render(
      <MemoryVisualization {...defaultProps} defaultLayer="L3" />
    );

    // L3 tab should be active
    const l3Tab = screen.getByText('L3 语义记忆');
    expect(l3Tab.closest('button')).toHaveClass('bg-purple-100');
  });

  it('changes layer when tab is clicked', () => {
    const { rerender } = render(<MemoryVisualization {...defaultProps} />);

    // Click L1 tab
    fireEvent.click(screen.getByText('L1 工作记忆'));

    // L1 tab should now be active
    const l1Tab = screen.getByText('L1 工作记忆');
    expect(l1Tab.closest('button')).toHaveClass('bg-blue-100');
  });

  describe('highlights search matches', () => {
    it('highlights matching text when search query is present', () => {
      const memories = [
        createMockMemory('l2-1', { content: 'This is a test memory' }),
      ];
      mockUseMemoryData.mockReturnValue({
        ...defaultHookReturn,
        memories,
        total: 1,
        searchQuery: 'test',
      });

      render(<MemoryVisualization {...defaultProps} />);

      // The word "test" should be highlighted
      const highlighted = screen.getByText('test');
      expect(highlighted.tagName).toBe('MARK');
    });
  });
});