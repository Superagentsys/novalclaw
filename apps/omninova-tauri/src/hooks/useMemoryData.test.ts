/**
 * Tests for useMemoryData hook
 *
 * [Source: Story 5.7 - MemoryVisualization 组件]
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useMemoryData } from '@/hooks/useMemoryData';

// Mock memory APIs
vi.mock('@/types/memory', () => ({
  getWorkingMemory: vi.fn(),
  getEpisodicMemories: vi.fn(),
  searchSemanticMemories: vi.fn(),
  deleteEpisodicMemory: vi.fn(),
  deleteSemanticMemory: vi.fn(),
  clearWorkingMemory: vi.fn(),
}));

import {
  getWorkingMemory,
  getEpisodicMemories,
  searchSemanticMemories,
  deleteEpisodicMemory,
  deleteSemanticMemory,
} from '@/types/memory';

const mockGetWorkingMemory = vi.mocked(getWorkingMemory);
const mockGetEpisodicMemories = vi.mocked(getEpisodicMemories);
const mockSearchSemanticMemories = vi.mocked(searchSemanticMemories);
const mockDeleteEpisodicMemory = vi.mocked(deleteEpisodicMemory);
const mockDeleteSemanticMemory = vi.mocked(deleteSemanticMemory);

describe('useMemoryData', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    // Default mock implementations
    mockGetWorkingMemory.mockResolvedValue([]);
    mockGetEpisodicMemories.mockResolvedValue([]);
    mockSearchSemanticMemories.mockResolvedValue([]);
    mockDeleteEpisodicMemory.mockResolvedValue(true);
    mockDeleteSemanticMemory.mockResolvedValue(true);
  });

  const defaultOptions = {
    agentId: 1,
    layer: 'L2' as const,
    pageSize: 20,
    autoRefresh: false,
  };

  describe('initialization', () => {
    it('returns initial state correctly', () => {
      const { result } = renderHook(() => useMemoryData(defaultOptions));

      expect(result.current.memories).toEqual([]);
      expect(result.current.error).toBeNull();
      expect(result.current.hasMore).toBe(false);
      expect(result.current.total).toBe(0);
    });
  });

  describe('L1 working memory', () => {
    it('fetches working memory when layer is L1', async () => {
      const mockEntries = [
        { id: '1', role: 'user', content: 'Hello', timestamp: '1000' },
        { id: '2', role: 'assistant', content: 'Hi there', timestamp: '2000' },
      ];
      mockGetWorkingMemory.mockResolvedValue(mockEntries);

      const { result } = renderHook(() =>
        useMemoryData({ ...defaultOptions, layer: 'L1', autoRefresh: true })
      );

      await waitFor(() => {
        expect(mockGetWorkingMemory).toHaveBeenCalledWith(0);
      });

      await waitFor(() => {
        expect(result.current.memories.length).toBe(2);
        expect(result.current.memories[0].sourceLayer).toBe('L1');
      });
    });

    it('converts working memory entries to unified format', async () => {
      mockGetWorkingMemory.mockResolvedValue([
        { id: '1', role: 'user', content: 'Test', timestamp: '1000' },
      ]);

      const { result } = renderHook(() =>
        useMemoryData({ ...defaultOptions, layer: 'L1', autoRefresh: true })
      );

      await waitFor(() => {
        expect(result.current.memories[0]).toMatchObject({
          id: 'l1-1',
          content: 'Test',
          role: 'user',
          sourceLayer: 'L1',
          similarityScore: null,
        });
      });
    });
  });

  describe('L2 episodic memory', () => {
    it('fetches episodic memories when layer is L2', async () => {
      const mockEpisodic = [
        {
          id: 1,
          agentId: 1,
          sessionId: 100,
          content: 'Memory 1',
          importance: 8,
          metadata: null,
          createdAt: Date.now() / 1000,
        },
      ];
      mockGetEpisodicMemories.mockResolvedValue(mockEpisodic);

      const { result } = renderHook(() =>
        useMemoryData({ ...defaultOptions, layer: 'L2', autoRefresh: true })
      );

      await waitFor(() => {
        expect(mockGetEpisodicMemories).toHaveBeenCalledWith(1, 21, 0);
      });

      await waitFor(() => {
        expect(result.current.memories.length).toBe(1);
        expect(result.current.memories[0].sourceLayer).toBe('L2');
      });
    });

    it('filters by session ID when provided', async () => {
      const mockEpisodic = [
        { id: 1, agentId: 1, sessionId: 100, content: 'Memory 1', importance: 5, metadata: null, createdAt: 1000 },
        { id: 2, agentId: 1, sessionId: 200, content: 'Memory 2', importance: 5, metadata: null, createdAt: 1000 },
      ];
      mockGetEpisodicMemories.mockResolvedValue(mockEpisodic);

      const { result } = renderHook(() =>
        useMemoryData({
          ...defaultOptions,
          layer: 'L2',
          sessionId: 100,
          autoRefresh: true,
        })
      );

      await waitFor(() => {
        expect(result.current.memories.length).toBe(1);
        expect(result.current.memories[0].sessionId).toBe(100);
      });
    });

    it('converts episodic memory entries to unified format', async () => {
      mockGetEpisodicMemories.mockResolvedValue([
        {
          id: 1,
          agentId: 1,
          sessionId: 100,
          content: 'Test memory',
          importance: 7,
          metadata: null,
          createdAt: 1000000,
        },
      ]);

      const { result } = renderHook(() =>
        useMemoryData({ ...defaultOptions, layer: 'L2', autoRefresh: true })
      );

      await waitFor(() => {
        expect(result.current.memories[0]).toMatchObject({
          id: 'l2-1',
          content: 'Test memory',
          importance: 7,
          sessionId: 100,
          sourceLayer: 'L2',
          similarityScore: null,
        });
      });
    });
  });

  describe('L3 semantic memory', () => {
    it('does not fetch without search query', async () => {
      renderHook(() =>
        useMemoryData({ ...defaultOptions, layer: 'L3', autoRefresh: true })
      );

      // Should not call search without query
      expect(mockSearchSemanticMemories).not.toHaveBeenCalled();
    });

    it('searches semantic memories when query is provided', async () => {
      mockSearchSemanticMemories.mockResolvedValue([
        {
          memory: { id: 1, episodicMemoryId: 1, embedding: [], embeddingDim: 1536, embeddingModel: 'test', createdAt: 1000, updatedAt: 1000 },
          score: 0.85,
          content: 'Semantic result',
        },
      ]);

      const { result } = renderHook(() =>
        useMemoryData({ ...defaultOptions, layer: 'L3', autoRefresh: true })
      );

      // Set search query
      act(() => {
        result.current.setSearchQuery('test query');
      });

      await waitFor(() => {
        expect(mockSearchSemanticMemories).toHaveBeenCalled();
      });
    });

    it('converts semantic search results to unified format', async () => {
      mockSearchSemanticMemories.mockResolvedValue([
        {
          memory: { id: 1, episodicMemoryId: 1, embedding: [], embeddingDim: 1536, embeddingModel: 'test', createdAt: 1000, updatedAt: 1000 },
          score: 0.9,
          content: 'Semantic memory content',
        },
      ]);

      const { result } = renderHook(() =>
        useMemoryData({ ...defaultOptions, layer: 'L3', autoRefresh: true })
      );

      act(() => {
        result.current.setSearchQuery('test');
      });

      await waitFor(() => {
        expect(result.current.memories[0]).toMatchObject({
          id: 'l3-1',
          content: 'Semantic memory content',
          sourceLayer: 'L3',
          similarityScore: 0.9,
        });
      });
    });
  });

  describe('filtering', () => {
    it('filters by time range', async () => {
      const now = Date.now() / 1000;
      const oneHourAgo = now - 3600;
      const oneWeekAgo = now - 7 * 24 * 3600;

      mockGetEpisodicMemories.mockResolvedValue([
        { id: 1, agentId: 1, sessionId: null, content: 'Recent', importance: 5, metadata: null, createdAt: oneHourAgo },
        { id: 2, agentId: 1, sessionId: null, content: 'Old', importance: 5, metadata: null, createdAt: oneWeekAgo },
      ]);

      const { result } = renderHook(() =>
        useMemoryData({ ...defaultOptions, layer: 'L2', autoRefresh: true })
      );

      await waitFor(() => {
        expect(result.current.memories.length).toBe(2);
      });

      // Filter by today
      act(() => {
        result.current.setTimeRange('today');
      });

      await waitFor(() => {
        expect(result.current.memories.length).toBe(1);
        expect(result.current.memories[0].content).toBe('Recent');
      });
    });

    it('filters by minimum importance', async () => {
      mockGetEpisodicMemories.mockResolvedValue([
        { id: 1, agentId: 1, sessionId: null, content: 'Important', importance: 9, metadata: null, createdAt: 1000 },
        { id: 2, agentId: 1, sessionId: null, content: 'Normal', importance: 5, metadata: null, createdAt: 1000 },
      ]);

      const { result } = renderHook(() =>
        useMemoryData({ ...defaultOptions, layer: 'L2', autoRefresh: true })
      );

      await waitFor(() => {
        expect(result.current.memories.length).toBe(2);
      });

      // Filter by importance >= 8
      act(() => {
        result.current.setMinImportance(8);
      });

      await waitFor(() => {
        expect(result.current.memories.length).toBe(1);
        expect(result.current.memories[0].importance).toBe(9);
      });
    });

    it('filters by search query for L1/L2', async () => {
      mockGetEpisodicMemories.mockResolvedValue([
        { id: 1, agentId: 1, sessionId: null, content: 'Hello world', importance: 5, metadata: null, createdAt: 1000 },
        { id: 2, agentId: 1, sessionId: null, content: 'Goodbye', importance: 5, metadata: null, createdAt: 1000 },
      ]);

      const { result } = renderHook(() =>
        useMemoryData({ ...defaultOptions, layer: 'L2', autoRefresh: true })
      );

      await waitFor(() => {
        expect(result.current.memories.length).toBe(2);
      });

      // Filter by search query
      act(() => {
        result.current.setSearchQuery('hello');
      });

      await waitFor(() => {
        expect(result.current.memories.length).toBe(1);
        expect(result.current.memories[0].content).toBe('Hello world');
      });
    });
  });

  describe('delete functionality', () => {
    it('deletes L2 memory', async () => {
      mockGetEpisodicMemories.mockResolvedValue([
        { id: 123, agentId: 1, sessionId: null, content: 'Test', importance: 5, metadata: null, createdAt: 1000 },
      ]);

      const { result } = renderHook(() =>
        useMemoryData({ ...defaultOptions, layer: 'L2', autoRefresh: true })
      );

      await waitFor(() => {
        expect(result.current.memories.length).toBe(1);
      });

      let success: boolean | undefined;
      await act(async () => {
        success = await result.current.deleteMemory('l2-123');
      });

      expect(mockDeleteEpisodicMemory).toHaveBeenCalledWith(123);
      expect(success).toBe(true);
    });

    it('deletes L3 memory', async () => {
      mockSearchSemanticMemories.mockResolvedValue([
        {
          memory: { id: 456, episodicMemoryId: 1, embedding: [], embeddingDim: 1536, embeddingModel: 'test', createdAt: 1000, updatedAt: 1000 },
          score: 0.9,
          content: 'Test',
        },
      ]);

      const { result } = renderHook(() =>
        useMemoryData({ ...defaultOptions, layer: 'L3', autoRefresh: true })
      );

      act(() => {
        result.current.setSearchQuery('test');
      });

      await waitFor(() => {
        expect(result.current.memories.length).toBe(1);
      });

      let success: boolean | undefined;
      await act(async () => {
        success = await result.current.deleteMemory('l3-456');
      });

      expect(mockDeleteSemanticMemory).toHaveBeenCalledWith(456);
      expect(success).toBe(true);
    });

    it('removes deleted memory from local state', async () => {
      mockGetEpisodicMemories.mockResolvedValue([
        { id: 1, agentId: 1, sessionId: null, content: 'Memory 1', importance: 5, metadata: null, createdAt: 1000 },
        { id: 2, agentId: 1, sessionId: null, content: 'Memory 2', importance: 5, metadata: null, createdAt: 1000 },
      ]);

      const { result } = renderHook(() =>
        useMemoryData({ ...defaultOptions, layer: 'L2', autoRefresh: true })
      );

      await waitFor(() => {
        expect(result.current.memories.length).toBe(2);
      });

      await act(async () => {
        await result.current.deleteMemory('l2-1');
      });

      expect(result.current.memories.length).toBe(1);
      expect(result.current.memories[0].id).toBe('l2-2');
    });
  });

  describe('refresh and pagination', () => {
    it('refresh reloads data', async () => {
      mockGetEpisodicMemories.mockResolvedValue([]);

      const { result } = renderHook(() =>
        useMemoryData({ ...defaultOptions, layer: 'L2', autoRefresh: false })
      );

      await act(async () => {
        await result.current.refresh();
      });

      expect(mockGetEpisodicMemories).toHaveBeenCalled();
    });
  });

  describe('state setters', () => {
    it('setSearchQuery updates search query', () => {
      const { result } = renderHook(() => useMemoryData(defaultOptions));

      act(() => {
        result.current.setSearchQuery('new query');
      });

      expect(result.current.searchQuery).toBe('new query');
    });

    it('setTimeRange updates time range', () => {
      const { result } = renderHook(() => useMemoryData(defaultOptions));

      act(() => {
        result.current.setTimeRange('week');
      });

      expect(result.current.timeRange).toBe('week');
    });

    it('setMinImportance updates min importance', () => {
      const { result } = renderHook(() => useMemoryData(defaultOptions));

      act(() => {
        result.current.setMinImportance(8);
      });

      expect(result.current.minImportance).toBe(8);
    });
  });
});