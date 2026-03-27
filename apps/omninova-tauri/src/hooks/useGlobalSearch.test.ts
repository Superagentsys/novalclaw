/**
 * Global Search Hook Tests
 *
 * Unit tests for global search functionality.
 *
 * [Source: Story 10.3 - 全局搜索功能]
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useGlobalSearch } from './useGlobalSearch';
import { useSessionStore } from '@/stores/sessionStore';

// Mock session store
vi.mock('@/stores/sessionStore', () => ({
  useSessionStore: vi.fn(),
}));

describe('useGlobalSearch', () => {
  const mockSessions = [
    { id: 1, title: '关于 React 的讨论', agentId: 1, createdAt: '2026-03-27' },
    { id: 2, title: 'TypeScript 学习笔记', agentId: 1, createdAt: '2026-03-26' },
    { id: 3, title: '项目规划会议', agentId: 2, createdAt: '2026-03-25' },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    (useSessionStore as any).mockImplementation((selector) =>
      selector({ sessions: mockSessions })
    );
  });

  describe('initial state', () => {
    it('should have empty query and closed state', () => {
      const { result } = renderHook(() => useGlobalSearch());

      expect(result.current.query).toBe('');
      expect(result.current.isOpen).toBe(false);
      expect(result.current.results).toEqual([]);
    });
  });

  describe('query handling', () => {
    it('should set query', () => {
      const { result } = renderHook(() => useGlobalSearch());

      act(() => {
        result.current.setQuery('React');
      });

      expect(result.current.query).toBe('React');
    });

    it('should not search with short query', () => {
      const { result } = renderHook(() => useGlobalSearch());

      act(() => {
        result.current.setQuery('R');
      });

      expect(result.current.results).toEqual([]);
    });

    it('should search with minimum query length', () => {
      const { result } = renderHook(() => useGlobalSearch());

      act(() => {
        result.current.setQuery('Re');
      });

      // Should find sessions with "Re" in title
      expect(result.current.results.length).toBeGreaterThan(0);
    });
  });

  describe('session search', () => {
    it('should find sessions by title', () => {
      const { result } = renderHook(() => useGlobalSearch());

      act(() => {
        result.current.setQuery('React');
      });

      expect(result.current.results).toHaveLength(1);
      expect(result.current.results[0].type).toBe('session');
      expect(result.current.results[0].title).toBe('关于 React 的讨论');
    });

    it('should be case-insensitive', () => {
      const { result } = renderHook(() => useGlobalSearch());

      act(() => {
        result.current.setQuery('react');
      });

      expect(result.current.results).toHaveLength(1);
    });

    it('should return empty for no matches', () => {
      const { result } = renderHook(() => useGlobalSearch());

      act(() => {
        result.current.setQuery('nonexistent');
      });

      expect(result.current.results).toHaveLength(0);
    });
  });

  describe('grouped results', () => {
    it('should group results by type', () => {
      const { result } = renderHook(() => useGlobalSearch());

      act(() => {
        result.current.setQuery('项目');
      });

      const sessionGroup = result.current.groupedResults.find(
        (g) => g.type === 'session'
      );
      expect(sessionGroup).toBeDefined();
      expect(sessionGroup?.results).toHaveLength(1);
    });

    it('should exclude empty groups', () => {
      const { result } = renderHook(() => useGlobalSearch());

      act(() => {
        result.current.setQuery('项目');
      });

      const memoryGroup = result.current.groupedResults.find(
        (g) => g.type === 'memory'
      );
      expect(memoryGroup).toBeUndefined();
    });
  });

  describe('panel control', () => {
    it('should open and close search panel', () => {
      const { result } = renderHook(() => useGlobalSearch());

      act(() => {
        result.current.openSearch();
      });
      expect(result.current.isOpen).toBe(true);

      act(() => {
        result.current.closeSearch();
      });
      expect(result.current.isOpen).toBe(false);
      expect(result.current.query).toBe('');
    });

    it('should clear search without closing panel', () => {
      const { result } = renderHook(() => useGlobalSearch());

      act(() => {
        result.current.setQuery('test');
      });
      expect(result.current.query).toBe('test');

      act(() => {
        result.current.clearSearch();
      });
      expect(result.current.query).toBe('');
    });
  });
});