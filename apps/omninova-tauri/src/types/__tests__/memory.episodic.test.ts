/**
 * Episodic Memory API Tests (Story 5.2)
 *
 * Tests for L2 episodic memory TypeScript API functions.
 * These functions call Tauri backend commands for long-term memory storage.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
  storeEpisodicMemory,
  getEpisodicMemories,
  getEpisodicMemoriesBySession,
  getEpisodicMemoriesByImportance,
  deleteEpisodicMemory,
  getEpisodicMemoryStats,
  exportEpisodicMemories,
  importEpisodicMemories,
  endSession,
  type EpisodicMemory,
  type EpisodicMemoryStats,
} from '../memory'

// Mock Tauri API
const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
}))

describe('Episodic Memory API (L2)', () => {
  beforeEach(() => {
    mockInvoke.mockReset()
  })

  // ============================================================================
  // storeEpisodicMemory
  // ============================================================================

  describe('storeEpisodicMemory', () => {
    it('should store a memory with required parameters', async () => {
      mockInvoke.mockResolvedValueOnce(1)

      const result = await storeEpisodicMemory(1, 'Test memory content', 7)

      expect(mockInvoke).toHaveBeenCalledWith('store_episodic_memory', {
        agentId: 1,
        sessionId: null,
        content: 'Test memory content',
        importance: 7,
        metadata: null,
      })
      expect(result).toBe(1)
    })

    it('should store a memory with session ID', async () => {
      mockInvoke.mockResolvedValueOnce(2)

      const result = await storeEpisodicMemory(1, 'Memory with session', 5, 42)

      expect(mockInvoke).toHaveBeenCalledWith('store_episodic_memory', {
        agentId: 1,
        sessionId: 42,
        content: 'Memory with session',
        importance: 5,
        metadata: null,
      })
      expect(result).toBe(2)
    })

    it('should store a memory with metadata', async () => {
      mockInvoke.mockResolvedValueOnce(3)
      const metadata = JSON.stringify({ tags: ['important', 'work'] })

      const result = await storeEpisodicMemory(
        1,
        'Memory with metadata',
        8,
        null,
        metadata
      )

      expect(mockInvoke).toHaveBeenCalledWith('store_episodic_memory', {
        agentId: 1,
        sessionId: null,
        content: 'Memory with metadata',
        importance: 8,
        metadata,
      })
      expect(result).toBe(3)
    })

    it('should store a memory with all parameters', async () => {
      mockInvoke.mockResolvedValueOnce(4)

      const result = await storeEpisodicMemory(
        5,
        'Complete memory',
        10,
        100,
        '{"key":"value"}'
      )

      expect(mockInvoke).toHaveBeenCalledWith('store_episodic_memory', {
        agentId: 5,
        sessionId: 100,
        content: 'Complete memory',
        importance: 10,
        metadata: '{"key":"value"}',
      })
      expect(result).toBe(4)
    })
  })

  // ============================================================================
  // getEpisodicMemories
  // ============================================================================

  describe('getEpisodicMemories', () => {
    const mockMemories: EpisodicMemory[] = [
      {
        id: 1,
        agentId: 1,
        sessionId: 10,
        content: 'First memory',
        importance: 5,
        metadata: null,
        createdAt: 1700000000,
      },
      {
        id: 2,
        agentId: 1,
        sessionId: 10,
        content: 'Second memory',
        importance: 7,
        metadata: '{"tag":"important"}',
        createdAt: 1700000001,
      },
    ]

    it('should get memories with default pagination', async () => {
      mockInvoke.mockResolvedValueOnce(mockMemories)

      const result = await getEpisodicMemories(1)

      expect(mockInvoke).toHaveBeenCalledWith('get_episodic_memories', {
        agentId: 1,
        limit: 100,
        offset: 0,
      })
      expect(result).toEqual(mockMemories)
    })

    it('should get memories with custom limit', async () => {
      mockInvoke.mockResolvedValueOnce(mockMemories)

      await getEpisodicMemories(1, 50)

      expect(mockInvoke).toHaveBeenCalledWith('get_episodic_memories', {
        agentId: 1,
        limit: 50,
        offset: 0,
      })
    })

    it('should get memories with custom limit and offset', async () => {
      mockInvoke.mockResolvedValueOnce(mockMemories)

      await getEpisodicMemories(1, 20, 10)

      expect(mockInvoke).toHaveBeenCalledWith('get_episodic_memories', {
        agentId: 1,
        limit: 20,
        offset: 10,
      })
    })

    it('should return empty array when no memories found', async () => {
      mockInvoke.mockResolvedValueOnce([])

      const result = await getEpisodicMemories(999)

      expect(result).toEqual([])
    })
  })

  // ============================================================================
  // getEpisodicMemoriesBySession
  // ============================================================================

  describe('getEpisodicMemoriesBySession', () => {
    it('should get memories by session ID', async () => {
      const mockMemories: EpisodicMemory[] = [
        {
          id: 1,
          agentId: 1,
          sessionId: 42,
          content: 'Session memory',
          importance: 5,
          metadata: null,
          createdAt: 1700000000,
        },
      ]
      mockInvoke.mockResolvedValueOnce(mockMemories)

      const result = await getEpisodicMemoriesBySession(42)

      expect(mockInvoke).toHaveBeenCalledWith('get_episodic_memories_by_session', {
        sessionId: 42,
      })
      expect(result).toEqual(mockMemories)
    })
  })

  // ============================================================================
  // getEpisodicMemoriesByImportance
  // ============================================================================

  describe('getEpisodicMemoriesByImportance', () => {
    it('should get important memories with default limit', async () => {
      mockInvoke.mockResolvedValueOnce([])

      await getEpisodicMemoriesByImportance(8)

      expect(mockInvoke).toHaveBeenCalledWith('get_episodic_memories_by_importance', {
        minImportance: 8,
        limit: 100,
      })
    })

    it('should get important memories with custom limit', async () => {
      mockInvoke.mockResolvedValueOnce([])

      await getEpisodicMemoriesByImportance(5, 50)

      expect(mockInvoke).toHaveBeenCalledWith('get_episodic_memories_by_importance', {
        minImportance: 5,
        limit: 50,
      })
    })
  })

  // ============================================================================
  // deleteEpisodicMemory
  // ============================================================================

  describe('deleteEpisodicMemory', () => {
    it('should delete a memory and return true', async () => {
      mockInvoke.mockResolvedValueOnce(true)

      const result = await deleteEpisodicMemory(1)

      expect(mockInvoke).toHaveBeenCalledWith('delete_episodic_memory', { id: 1 })
      expect(result).toBe(true)
    })

    it('should return false when memory not found', async () => {
      mockInvoke.mockResolvedValueOnce(false)

      const result = await deleteEpisodicMemory(999)

      expect(result).toBe(false)
    })
  })

  // ============================================================================
  // getEpisodicMemoryStats
  // ============================================================================

  describe('getEpisodicMemoryStats', () => {
    it('should return memory statistics', async () => {
      const mockStats: EpisodicMemoryStats = {
        totalCount: 100,
        avgImportance: 6.5,
        byAgent: {
          '1': 50,
          '2': 30,
          '3': 20,
        },
      }
      mockInvoke.mockResolvedValueOnce(mockStats)

      const result = await getEpisodicMemoryStats()

      expect(mockInvoke).toHaveBeenCalledWith('get_episodic_memory_stats')
      expect(result).toEqual(mockStats)
    })

    it('should return zero stats when no memories', async () => {
      const emptyStats: EpisodicMemoryStats = {
        totalCount: 0,
        avgImportance: 0,
        byAgent: {},
      }
      mockInvoke.mockResolvedValueOnce(emptyStats)

      const result = await getEpisodicMemoryStats()

      expect(result.totalCount).toBe(0)
    })
  })

  // ============================================================================
  // exportEpisodicMemories
  // ============================================================================

  describe('exportEpisodicMemories', () => {
    it('should export memories as JSON string', async () => {
      const mockJson = JSON.stringify([
        { id: 1, content: 'Memory 1' },
        { id: 2, content: 'Memory 2' },
      ])
      mockInvoke.mockResolvedValueOnce(mockJson)

      const result = await exportEpisodicMemories(1)

      expect(mockInvoke).toHaveBeenCalledWith('export_episodic_memories', { agentId: 1 })
      expect(result).toBe(mockJson)
    })
  })

  // ============================================================================
  // importEpisodicMemories
  // ============================================================================

  describe('importEpisodicMemories', () => {
    it('should import memories without skipping duplicates', async () => {
      const json = '[{"content":"Imported memory","importance":5}]'
      mockInvoke.mockResolvedValueOnce(1)

      const result = await importEpisodicMemories(json)

      expect(mockInvoke).toHaveBeenCalledWith('import_episodic_memories', {
        json,
        skipDuplicates: false,
      })
      expect(result).toBe(1)
    })

    it('should import memories with skipDuplicates option', async () => {
      const json = '[{"content":"Imported memory","importance":5}]'
      mockInvoke.mockResolvedValueOnce(1)

      const result = await importEpisodicMemories(json, true)

      expect(mockInvoke).toHaveBeenCalledWith('import_episodic_memories', {
        json,
        skipDuplicates: true,
      })
      expect(result).toBe(1)
    })
  })

  // ============================================================================
  // endSession
  // ============================================================================

  describe('endSession', () => {
    it('should end session and return count of persisted memories', async () => {
      mockInvoke.mockResolvedValueOnce(5)

      const result = await endSession(1, 42)

      expect(mockInvoke).toHaveBeenCalledWith('end_session', {
        agentId: 1,
        sessionId: 42,
      })
      expect(result).toBe(5)
    })

    it('should return zero when no memories to persist', async () => {
      mockInvoke.mockResolvedValueOnce(0)

      const result = await endSession(1, 999)

      expect(result).toBe(0)
    })
  })
})