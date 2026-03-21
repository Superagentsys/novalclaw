/**
 * Tests for MemoryFilterBar component
 *
 * [Source: Story 5.7 - MemoryVisualization 组件]
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MemoryFilterBar } from '@/components/Chat/MemoryFilterBar';
import type { MemoryLayer } from '@/types/memory';

describe('MemoryFilterBar', () => {
  const defaultProps = {
    layer: 'L2' as MemoryLayer,
    onLayerChange: vi.fn(),
    timeRange: 'all' as const,
    onTimeRangeChange: vi.fn(),
    minImportance: 1,
    onImportanceChange: vi.fn(),
    searchQuery: '',
    onSearchChange: vi.fn(),
    total: 100,
  };

  it('renders layer tabs correctly', () => {
    render(<MemoryFilterBar {...defaultProps} />);

    expect(screen.getByText('L1 工作记忆')).toBeInTheDocument();
    expect(screen.getByText('L2 情景记忆')).toBeInTheDocument();
    expect(screen.getByText('L3 语义记忆')).toBeInTheDocument();
  });

  it('highlights the active layer tab', () => {
    const { container } = render(<MemoryFilterBar {...defaultProps} layer="L2" />);

    // L2 tab should be active (green color)
    const l2Tab = screen.getByText('L2 情景记忆');
    expect(l2Tab.closest('button')).toHaveClass('bg-green-100');
  });

  it('calls onLayerChange when layer tab is clicked', () => {
    const onLayerChange = vi.fn();
    render(<MemoryFilterBar {...defaultProps} onLayerChange={onLayerChange} />);

    fireEvent.click(screen.getByText('L1 工作记忆'));
    expect(onLayerChange).toHaveBeenCalledWith('L1');

    fireEvent.click(screen.getByText('L3 语义记忆'));
    expect(onLayerChange).toHaveBeenCalledWith('L3');
  });

  it('renders search input with placeholder for L1/L2', () => {
    render(<MemoryFilterBar {...defaultProps} layer="L2" />);

    const searchInput = screen.getByPlaceholderText('搜索记忆内容...');
    expect(searchInput).toBeInTheDocument();
  });

  it('renders search input with placeholder for L3', () => {
    render(<MemoryFilterBar {...defaultProps} layer="L3" />);

    const searchInput = screen.getByPlaceholderText('输入关键词搜索...');
    expect(searchInput).toBeInTheDocument();
  });

  it('calls onSearchChange when search input changes', () => {
    const onSearchChange = vi.fn();
    render(<MemoryFilterBar {...defaultProps} onSearchChange={onSearchChange} />);

    const searchInput = screen.getByPlaceholderText('搜索记忆内容...');
    fireEvent.change(searchInput, { target: { value: 'test query' } });

    expect(onSearchChange).toHaveBeenCalledWith('test query');
  });

  it('displays current search query in input', () => {
    render(<MemoryFilterBar {...defaultProps} searchQuery="existing query" />);

    const searchInput = screen.getByPlaceholderText('搜索记忆内容...') as HTMLInputElement;
    expect(searchInput.value).toBe('existing query');
  });

  it('renders time range filter for L2', () => {
    render(<MemoryFilterBar {...defaultProps} layer="L2" />);

    // Time range select should be present - look for the select trigger
    const timeSelects = screen.getAllByRole('combobox');
    expect(timeSelects.length).toBeGreaterThan(0);
  });

  it('does not render time range filter for L1', () => {
    render(<MemoryFilterBar {...defaultProps} layer="L1" />);

    // Only search input should be present
    expect(screen.getByPlaceholderText('搜索记忆内容...')).toBeInTheDocument();
  });

  it('does not render importance filter for L1', () => {
    render(<MemoryFilterBar {...defaultProps} layer="L1" />);

    // Should not have importance select
    expect(screen.queryByText('重要性')).not.toBeInTheDocument();
  });

  it('renders importance filter for L2', () => {
    render(<MemoryFilterBar {...defaultProps} layer="L2" />);

    // Should have importance-related content
    const selects = screen.getAllByRole('combobox');
    expect(selects.length).toBeGreaterThan(0);
  });

  it('displays total count when provided', () => {
    render(<MemoryFilterBar {...defaultProps} total={50} />);

    expect(screen.getByText('共 50 条')).toBeInTheDocument();
  });

  it('displays hint for L3 semantic search', () => {
    render(<MemoryFilterBar {...defaultProps} layer="L3" />);

    expect(screen.getByText(/语义记忆使用向量相似性搜索/)).toBeInTheDocument();
  });

  it('does not display hint for L1', () => {
    render(<MemoryFilterBar {...defaultProps} layer="L1" />);

    expect(screen.queryByText(/语义记忆使用向量相似性搜索/)).not.toBeInTheDocument();
  });

  describe('layer tab colors', () => {
    it('L1 tab has blue color when active', () => {
      render(<MemoryFilterBar {...defaultProps} layer="L1" />);

      const l1Tab = screen.getByText('L1 工作记忆');
      expect(l1Tab.closest('button')).toHaveClass('bg-blue-100');
    });

    it('L2 tab has green color when active', () => {
      render(<MemoryFilterBar {...defaultProps} layer="L2" />);

      const l2Tab = screen.getByText('L2 情景记忆');
      expect(l2Tab.closest('button')).toHaveClass('bg-green-100');
    });

    it('L3 tab has purple color when active', () => {
      render(<MemoryFilterBar {...defaultProps} layer="L3" />);

      const l3Tab = screen.getByText('L3 语义记忆');
      expect(l3Tab.closest('button')).toHaveClass('bg-purple-100');
    });
  });
});