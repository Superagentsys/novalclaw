/**
 * Unified Memory Manager API Tests (Story 5.4)
 *
 * Tests for the unified memory manager TypeScript API functions.
 * These functions call Tauri backend commands for unified memory operations.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
  storeMemory,
  retrieveMemory,
  searchMemory,
  deleteMemory,
  getMemoryManagerStats,
  setMemorySession,
  persistMemorySession,
  type UnifiedMemoryEntry,
  type MemoryQueryResult,
  type MemoryManagerStats,
  type MemoryLayer,
} from '../memory'

// Mock Tauri API
const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
}))

describe('Unified Memory Manager API (Story 5.4)', () => {
  beforeEach(() => {
    mockInvoke.mockReset()
  })

  // ============================================================================
  // storeMemory
  // ============================================================================

  describe('storeMemory', () => {
    it('should store memory with default parameters', async () => {
      mockInvoke.mockResolvedValueOnce('l1-only')

      const result = await storeMemory('Test content', 'user', 5)

      expect(mockInvoke).toHaveBeenCalledWith('memory_store', {
        content: 'Test content',
        role: 'user',
        importance: 5,
        persistToL2: false,
        indexToL3: false,
      })
      expect(result).toBe('l1-only')
    })

    it('should store memory with persist to L2', async () => {
      mockInvoke.mockResolvedValueOnce('42')

      const result = await storeMemory('Important content', 'user', 8, true, false)

      expect(mockInvoke).toHaveBeenCalledWith('memory_store', {
        content: 'Important content',
        role: 'user',
        importance: 8,
        persistToL2: true,
        indexToL3: false,
      })
      expect(result).toBe('42')
    })

    it('should store memory with index to L3', async () => {
      mockInvoke.mockResolvedValueOnce('123')

      const result = await storeMemory('Critical content', 'system', 10, true, true)

      expect(mockInvoke).toHaveBeenCalledWith('memory_store', {
        content: 'Critical content',
        role: 'system',
        importance: 10,
        persistToL2: true,
        indexToL3: true,
      })
      expect(result).toBe('123')
    })

    it('should propagate errors from backend', async () => {
      mockInvoke.mockRejectedValueOnce(new Error('Failed to store'))

      await expect(storeMemory('Test', 'user', 5)).rejects.toThrow('Failed to store')
    })
  })

  // ============================================================================
  // retrieveMemory
  // ============================================================================

  describe('retrieveMemory', () => {
    const mockResult: MemoryQueryResult = {
      entries: [
        {
          id: '1',
          content: 'Memory 1',
          role: 'user',
          importance: 5,
          sessionId: 42,
          createdAt: 1700000000,
          sourceLayer: 'L2',
          similarityScore: null,
        },
      ],
      layer: 'L2',
      totalCount: 1,
    }

    it('should retrieve memories with default parameters', async () => {
      mockInvoke.mockResolvedValueOnce(mockResult)

      const result = await retrieveMemory(1)

      expect(mockInvoke).toHaveBeenCalledWith('memory_retrieve', {
        agentId: 1,
        sessionId: null,
        layer: 'ALL',
        limit: 100,
      })
      expect(result).toEqual(mockResult)
    })

    it('should retrieve memories with session ID filter', async () => {
      mockInvoke.mockResolvedValueOnce(mockResult)

      const result = await retrieveMemory(1, 42, 'L2', 50)

      expect(mockInvoke).toHaveBeenCalledWith('memory_retrieve', {
        agentId: 1,
        sessionId: 42,
        layer: 'L2',
        limit: 50,
      })
      expect(result).toEqual(mockResult)
    })

    it('should retrieve memories from specific layer', async () => {
      const l1Result: MemoryQueryResult = {
        entries: [
          {
            id: 'l1-1',
            content: 'Working memory entry',
            role: 'user',
            importance: 5,
            sessionId: null,
            createdAt: 1700000000,
            sourceLayer: 'L1',
            similarityScore: null,
          },
        ],
        layer: 'L1',
        totalCount: 1,
      }
      mockInvoke.mockResolvedValueOnce(l1Result)

      const result = await retrieveMemory(1, null, 'L1', 10)

      expect(mockInvoke).toHaveBeenCalledWith('memory_retrieve', {
        agentId: 1,
        sessionId: null,
        layer: 'L1',
        limit: 10,
      })
      expect(result.layer).toBe('L1')
    })

    it('should return empty result when no memories found', async () => {
      const emptyResult: MemoryQueryResult = {
        entries: [],
        layer: 'ALL',
        totalCount: 0,
      }
      mockInvoke.mockResolvedValueOnce(emptyResult)

      const result = await retrieveMemory(999)

      expect(result.entries).toEqual([])
      expect(result.totalCount).toBe(0)
    })
  })

  // ============================================================================
  // searchMemory
  // ============================================================================

  describe('searchMemory', () => {
    const mockSearchResults: UnifiedMemoryEntry[] = [
      {
        id: '1',
        content: 'Relevant memory',
        role: null,
        importance: 5,
        sessionId: null,
        createdAt: 1700000000,
        sourceLayer: 'L3',
        similarityScore: 0.95,
      },
      {
        id: '2',
        content: 'Another relevant memory',
        role: null,
        importance: 7,
        sessionId: null,
        createdAt: 1700000001,
        sourceLayer: 'L3',
        similarityScore: 0.85,
      },
    ]

    it('should search memories with default parameters', async () => {
      mockInvoke.mockResolvedValueOnce(mockSearchResults)

      const result = await searchMemory('test query')

      expect(mockInvoke).toHaveBeenCalledWith('memory_search', {
        query: 'test query',
        k: 10,
        threshold: 0.7,
      })
      expect(result).toEqual(mockSearchResults)
    })

    it('should search memories with custom k and threshold', async () => {
      mockInvoke.mockResolvedValueOnce(mockSearchResults)

      const result = await searchMemory('test query', 5, 0.8)

      expect(mockInvoke).toHaveBeenCalledWith('memory_search', {
        query: 'test query',
        k: 5,
        threshold: 0.8,
      })
    })

    it('should return empty array when no matches found', async () => {
      mockInvoke.mockResolvedValueOnce([])

      const result = await searchMemory('nonexistent query')

      expect(result).toEqual([])
    })

    it('should return results sorted by similarity score', async () => {
      mockInvoke.mockResolvedValueOnce(mockSearchResults)

      const result = await searchMemory('test')

      expect(result[0].similarityScore).toBeGreaterThan(result[1].similarityScore!)
    })
  })

  // ============================================================================
  // deleteMemory
  // ============================================================================

  describe('deleteMemory', () => {
    it('should delete memory from all layers by default', async () => {
      mockInvoke.mockResolvedValueOnce(true)

      const result = await deleteMemory('42')

      expect(mockInvoke).toHaveBeenCalledWith('memory_delete', {
        id: '42',
        layer: 'ALL',
      })
      expect(result).toBe(true)
    })

    it('should delete memory from specific layer', async () => {
      mockInvoke.mockResolvedValueOnce(true)

      const result = await deleteMemory('42', 'L2')

      expect(mockInvoke).toHaveBeenCalledWith('memory_delete', {
        id: '42',
        layer: 'L2',
      })
    })

    it('should return false when memory not found', async () => {
      mockInvoke.mockResolvedValueOnce(false)

      const result = await deleteMemory('999')

      expect(result).toBe(false)
    })

    it('should propagate errors from backend', async () => {
      mockInvoke.mockRejectedValueOnce(new Error('Invalid ID'))

      await expect(deleteMemory('invalid')).rejects.toThrow('Invalid ID')
    })
  })

  // ============================================================================
  // getMemoryManagerStats
  // ============================================================================

  describe('getMemoryManagerStats', () => {
    const mockStats: MemoryManagerStats = {
      l1Capacity: 100,
      l1Used: 25,
      l1SessionId: 42,
      l2Total: 150,
      l2AvgImportance: 6.5,
      l3Total: 75,
    }

    it('should return memory manager statistics', async () => {
      mockInvoke.mockResolvedValueOnce(mockStats)

      const result = await getMemoryManagerStats()

      expect(mockInvoke).toHaveBeenCalledWith('memory_get_stats')
      expect(result).toEqual(mockStats)
    })

    it('should return stats with null session when not set', async () => {
      const statsNoSession: MemoryManagerStats = {
        l1Capacity: 100,
        l1Used: 0,
        l1SessionId: null,
        l2Total: 0,
        l2AvgImportance: 0,
        l3Total: 0,
      }
      mockInvoke.mockResolvedValueOnce(statsNoSession)

      const result = await getMemoryManagerStats()

      expect(result.l1SessionId).toBeNull()
    })

    it('should have correct structure', async () => {
      mockInvoke.mockResolvedValueOnce(mockStats)

      const result = await getMemoryManagerStats()

      expect(typeof result.l1Capacity).toBe('number')
      expect(typeof result.l1Used).toBe('number')
      expect(typeof result.l2Total).toBe('number')
      expect(typeof result.l2AvgImportance).toBe('number')
      expect(typeof result.l3Total).toBe('number')
    })
  })

  // ============================================================================
  // setMemorySession
  // ============================================================================

  describe('setMemorySession', () => {
    it('should set session with both IDs', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      await setMemorySession(42, 1)

      expect(mockInvoke).toHaveBeenCalledWith('memory_set_session', {
        sessionId: 42,
        agentId: 1,
      })
    })

    it('should set session without agent ID', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      await setMemorySession(100)

      expect(mockInvoke).toHaveBeenCalledWith('memory_set_session', {
        sessionId: 100,
        agentId: null,
      })
    })
  })

  // ============================================================================
  // persistMemorySession
  // ============================================================================

  describe('persistMemorySession', () => {
    it('should persist session and return count', async () => {
      mockInvoke.mockResolvedValueOnce(5)

      const result = await persistMemorySession()

      expect(mockInvoke).toHaveBeenCalledWith('memory_persist_session')
      expect(result).toBe(5)
    })

    it('should return zero when no memories to persist', async () => {
      mockInvoke.mockResolvedValueOnce(0)

      const result = await persistMemorySession()

      expect(result).toBe(0)
    })
  })

  // ============================================================================
  // Type Validation
  // ============================================================================

  describe('Type validation', () => {
    it('MemoryLayer should accept valid values', () => {
      const l1: MemoryLayer = 'L1'
      const l2: MemoryLayer = 'L2'
      const l3: MemoryLayer = 'L3'
      const all: MemoryLayer = 'ALL'

      expect(l1).toBe('L1')
      expect(l2).toBe('L2')
      expect(l3).toBe('L3')
      expect(all).toBe('ALL')
    })

    it('UnifiedMemoryEntry should have all required fields', () => {
      const entry: UnifiedMemoryEntry = {
        id: '1',
        content: 'Test',
        role: 'user',
        importance: 5,
        sessionId: null,
        createdAt: 1700000000,
        sourceLayer: 'L2',
        similarityScore: null,
      }

      expect(entry.id).toBeDefined()
      expect(entry.content).toBeDefined()
      expect(entry.importance).toBeDefined()
      expect(entry.sourceLayer).toBeDefined()
    })

    it('MemoryQueryResult should have all required fields', () => {
      const result: MemoryQueryResult = {
        entries: [],
        layer: 'ALL',
        totalCount: 0,
      }

      expect(result.entries).toBeDefined()
      expect(result.layer).toBeDefined()
      expect(result.totalCount).toBeDefined()
    })

    it('MemoryManagerStats should have all required fields', () => {
      const stats: MemoryManagerStats = {
        l1Capacity: 100,
        l1Used: 50,
        l1SessionId: null,
        l2Total: 200,
        l2AvgImportance: 6.5,
        l3Total: 150,
      }

      expect(stats.l1Capacity).toBeDefined()
      expect(stats.l1Used).toBeDefined()
      expect(stats.l2Total).toBeDefined()
      expect(stats.l3Total).toBeDefined()
    })
  })
})