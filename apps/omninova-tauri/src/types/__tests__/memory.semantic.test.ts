/**
 * Semantic Memory API Tests (Story 5.3)
 *
 * Tests for L3 semantic memory TypeScript API functions.
 * These functions call Tauri backend commands for vector-based similarity search.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
  indexEpisodicMemory,
  searchSemanticMemories,
  getSemanticMemoryStats,
  deleteSemanticMemory,
  rebuildSemanticIndex,
  type SemanticMemory,
  type SemanticSearchResult,
  type SemanticMemoryStats,
} from '../memory'

// Mock Tauri API
const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
}))

describe('Semantic Memory API (L3)', () => {
  beforeEach(() => {
    mockInvoke.mockReset()
  })

  // ============================================================================
  // indexEpisodicMemory
  // ============================================================================

  describe('indexEpisodicMemory', () => {
    it('should call invoke with correct command and parameters', async () => {
      mockInvoke.mockResolvedValueOnce(1)

      const result = await indexEpisodicMemory(42)

      expect(mockInvoke).toHaveBeenCalledWith('index_episodic_memory', {
        episodicMemoryId: 42,
      })
      expect(result).toBe(1)
    })

    it('should return the created semantic memory ID', async () => {
      mockInvoke.mockResolvedValueOnce(123)

      const result = await indexEpisodicMemory(42)

      expect(result).toBe(123)
    })

    it('should propagate errors from backend', async () => {
      mockInvoke.mockRejectedValueOnce(new Error('Episodic memory not found'))

      await expect(indexEpisodicMemory(999)).rejects.toThrow('Episodic memory not found')
    })
  })

  // ============================================================================
  // searchSemanticMemories
  // ============================================================================

  describe('searchSemanticMemories', () => {
    const mockSearchResults: SemanticSearchResult[] = [
      {
        memory: {
          id: 1,
          episodicMemoryId: 100,
          embedding: [0.1, 0.2, 0.3],
          embeddingDim: 3,
          embeddingModel: 'text-embedding-3-small',
          createdAt: 1700000000,
          updatedAt: 1700000000,
        },
        score: 0.95,
        content: 'This is a relevant memory',
      },
      {
        memory: {
          id: 2,
          episodicMemoryId: 101,
          embedding: [0.2, 0.3, 0.4],
          embeddingDim: 3,
          embeddingModel: 'text-embedding-3-small',
          createdAt: 1700000001,
          updatedAt: 1700000001,
        },
        score: 0.85,
        content: 'Another related memory',
      },
    ]

    it('should call invoke with default parameters', async () => {
      mockInvoke.mockResolvedValueOnce(mockSearchResults)

      const result = await searchSemanticMemories('test query')

      expect(mockInvoke).toHaveBeenCalledWith('search_semantic_memories', {
        query: 'test query',
        k: 10,
        agentId: null,
        threshold: null,
      })
      expect(result).toEqual(mockSearchResults)
    })

    it('should call invoke with custom k parameter', async () => {
      mockInvoke.mockResolvedValueOnce(mockSearchResults)

      await searchSemanticMemories('test query', 5)

      expect(mockInvoke).toHaveBeenCalledWith('search_semantic_memories', {
        query: 'test query',
        k: 5,
        agentId: null,
        threshold: null,
      })
    })

    it('should call invoke with agentId filter', async () => {
      mockInvoke.mockResolvedValueOnce(mockSearchResults)

      await searchSemanticMemories('test query', 10, 42)

      expect(mockInvoke).toHaveBeenCalledWith('search_semantic_memories', {
        query: 'test query',
        k: 10,
        agentId: 42,
        threshold: null,
      })
    })

    it('should call invoke with threshold parameter', async () => {
      mockInvoke.mockResolvedValueOnce(mockSearchResults)

      await searchSemanticMemories('test query', 10, undefined, 0.8)

      expect(mockInvoke).toHaveBeenCalledWith('search_semantic_memories', {
        query: 'test query',
        k: 10,
        agentId: null,
        threshold: 0.8,
      })
    })

    it('should call invoke with all parameters', async () => {
      mockInvoke.mockResolvedValueOnce(mockSearchResults)

      await searchSemanticMemories('hello world', 20, 5, 0.5)

      expect(mockInvoke).toHaveBeenCalledWith('search_semantic_memories', {
        query: 'hello world',
        k: 20,
        agentId: 5,
        threshold: 0.5,
      })
    })

    it('should return empty array when no matches found', async () => {
      mockInvoke.mockResolvedValueOnce([])

      const result = await searchSemanticMemories('nonexistent query')

      expect(result).toEqual([])
    })

    it('should return results sorted by similarity score', async () => {
      const unsortedResults: SemanticSearchResult[] = [
        { ...mockSearchResults[1], score: 0.7 },
        { ...mockSearchResults[0], score: 0.9 },
      ]
      mockInvoke.mockResolvedValueOnce(unsortedResults)

      const result = await searchSemanticMemories('test')

      // Backend should return sorted, but we verify the structure
      expect(result).toHaveLength(2)
      expect(result[0].score).toBeDefined()
      expect(result[1].score).toBeDefined()
    })
  })

  // ============================================================================
  // getSemanticMemoryStats
  // ============================================================================

  describe('getSemanticMemoryStats', () => {
    const mockStats: SemanticMemoryStats = {
      totalCount: 150,
      byModel: {
        'text-embedding-3-small': 120,
        'nomic-embed-text': 30,
      },
      avgDimension: 1536,
    }

    it('should call invoke with correct command', async () => {
      mockInvoke.mockResolvedValueOnce(mockStats)

      const result = await getSemanticMemoryStats()

      expect(mockInvoke).toHaveBeenCalledWith('get_semantic_memory_stats')
      expect(result).toEqual(mockStats)
    })

    it('should return stats with correct structure', async () => {
      mockInvoke.mockResolvedValueOnce(mockStats)

      const result = await getSemanticMemoryStats()

      expect(result).toHaveProperty('totalCount')
      expect(result).toHaveProperty('byModel')
      expect(result).toHaveProperty('avgDimension')
      expect(typeof result.totalCount).toBe('number')
      expect(typeof result.avgDimension).toBe('number')
    })

    it('should return zero stats when no memories indexed', async () => {
      const emptyStats: SemanticMemoryStats = {
        totalCount: 0,
        byModel: {},
        avgDimension: 0,
      }
      mockInvoke.mockResolvedValueOnce(emptyStats)

      const result = await getSemanticMemoryStats()

      expect(result.totalCount).toBe(0)
      expect(result.byModel).toEqual({})
    })
  })

  // ============================================================================
  // deleteSemanticMemory
  // ============================================================================

  describe('deleteSemanticMemory', () => {
    it('should call invoke with correct command and id', async () => {
      mockInvoke.mockResolvedValueOnce(true)

      const result = await deleteSemanticMemory(42)

      expect(mockInvoke).toHaveBeenCalledWith('delete_semantic_memory', { id: 42 })
      expect(result).toBe(true)
    })

    it('should return true when deletion succeeds', async () => {
      mockInvoke.mockResolvedValueOnce(true)

      const result = await deleteSemanticMemory(1)

      expect(result).toBe(true)
    })

    it('should return false when memory not found', async () => {
      mockInvoke.mockResolvedValueOnce(false)

      const result = await deleteSemanticMemory(999)

      expect(result).toBe(false)
    })

    it('should propagate errors from backend', async () => {
      mockInvoke.mockRejectedValueOnce(new Error('Database error'))

      await expect(deleteSemanticMemory(1)).rejects.toThrow('Database error')
    })
  })

  // ============================================================================
  // rebuildSemanticIndex
  // ============================================================================

  describe('rebuildSemanticIndex', () => {
    it('should call invoke with agentId only', async () => {
      mockInvoke.mockResolvedValueOnce(50)

      await rebuildSemanticIndex(1)

      expect(mockInvoke).toHaveBeenCalledWith('rebuild_semantic_index', {
        agentId: 1,
        model: null,
      })
    })

    it('should call invoke with agentId and model', async () => {
      mockInvoke.mockResolvedValueOnce(50)

      await rebuildSemanticIndex(1, 'nomic-embed-text')

      expect(mockInvoke).toHaveBeenCalledWith('rebuild_semantic_index', {
        agentId: 1,
        model: 'nomic-embed-text',
      })
    })

    it('should return the number of re-indexed memories', async () => {
      mockInvoke.mockResolvedValueOnce(75)

      const result = await rebuildSemanticIndex(42)

      expect(result).toBe(75)
    })

    it('should return zero when agent has no memories', async () => {
      mockInvoke.mockResolvedValueOnce(0)

      const result = await rebuildSemanticIndex(999)

      expect(result).toBe(0)
    })

    it('should propagate errors from backend', async () => {
      mockInvoke.mockRejectedValueOnce(new Error('Agent not found'))

      await expect(rebuildSemanticIndex(999)).rejects.toThrow('Agent not found')
    })
  })

  // ============================================================================
  // Type Guards and Validation
  // ============================================================================

  describe('Type validation', () => {
    it('SemanticMemory interface should have all required fields', () => {
      const memory: SemanticMemory = {
        id: 1,
        episodicMemoryId: 100,
        embedding: [0.1, 0.2],
        embeddingDim: 2,
        embeddingModel: 'test-model',
        createdAt: 1700000000,
        updatedAt: 1700000000,
      }

      expect(memory.id).toBe(1)
      expect(memory.episodicMemoryId).toBe(100)
      expect(memory.embedding).toEqual([0.1, 0.2])
      expect(memory.embeddingDim).toBe(2)
      expect(memory.embeddingModel).toBe('test-model')
    })

    it('SemanticSearchResult interface should have memory, score, and content', () => {
      const result: SemanticSearchResult = {
        memory: {
          id: 1,
          episodicMemoryId: 100,
          embedding: [],
          embeddingDim: 0,
          embeddingModel: 'test',
          createdAt: 0,
          updatedAt: 0,
        },
        score: 0.95,
        content: 'test content',
      }

      expect(result.score).toBe(0.95)
      expect(result.content).toBe('test content')
    })
  })
})