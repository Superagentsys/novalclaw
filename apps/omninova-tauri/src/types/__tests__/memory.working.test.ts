/**
 * Working Memory API Tests (Story 5.1)
 *
 * Tests for L1 working memory TypeScript API functions.
 * These functions call Tauri backend commands for short-term session context.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
  getWorkingMemory,
  clearWorkingMemory,
  getMemoryStats,
  setWorkingMemorySession,
  pushWorkingMemoryContext,
  type WorkingMemoryEntry,
  type MemoryStats,
  type WorkingMemoryRole,
} from '../memory'

// Mock Tauri API
const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
}))

describe('Working Memory API (L1)', () => {
  beforeEach(() => {
    mockInvoke.mockReset()
  })

  // ============================================================================
  // getWorkingMemory
  // ============================================================================

  describe('getWorkingMemory', () => {
    const mockEntries: WorkingMemoryEntry[] = [
      {
        id: '1',
        role: 'user',
        content: 'Hello there!',
        timestamp: '1700000000',
      },
      {
        id: '2',
        role: 'assistant',
        content: 'Hi! How can I help you?',
        timestamp: '1700000001',
      },
    ]

    it('should get working memory entries with default limit', async () => {
      mockInvoke.mockResolvedValueOnce(mockEntries)

      const result = await getWorkingMemory()

      expect(mockInvoke).toHaveBeenCalledWith('get_working_memory', { limit: 0 })
      expect(result).toEqual(mockEntries)
    })

    it('should get working memory entries with custom limit', async () => {
      mockInvoke.mockResolvedValueOnce(mockEntries)

      const result = await getWorkingMemory(10)

      expect(mockInvoke).toHaveBeenCalledWith('get_working_memory', { limit: 10 })
      expect(result).toEqual(mockEntries)
    })

    it('should return empty array when no entries', async () => {
      mockInvoke.mockResolvedValueOnce([])

      const result = await getWorkingMemory()

      expect(result).toEqual([])
    })

    it('should return entries in chronological order', async () => {
      const entries: WorkingMemoryEntry[] = [
        { id: '1', role: 'user', content: 'First', timestamp: '100' },
        { id: '2', role: 'assistant', content: 'Second', timestamp: '200' },
      ]
      mockInvoke.mockResolvedValueOnce(entries)

      const result = await getWorkingMemory()

      expect(result[0].timestamp).toBe('100')
      expect(result[1].timestamp).toBe('200')
    })
  })

  // ============================================================================
  // clearWorkingMemory
  // ============================================================================

  describe('clearWorkingMemory', () => {
    it('should clear all working memory entries', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      await clearWorkingMemory()

      expect(mockInvoke).toHaveBeenCalledWith('clear_working_memory')
    })

    it('should resolve without return value', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const result = await clearWorkingMemory()

      expect(result).toBeUndefined()
    })
  })

  // ============================================================================
  // getMemoryStats
  // ============================================================================

  describe('getMemoryStats', () => {
    const mockStats: MemoryStats = {
      capacity: 100,
      used: 25,
      sessionId: 42,
      agentId: 1,
    }

    it('should return memory statistics', async () => {
      mockInvoke.mockResolvedValueOnce(mockStats)

      const result = await getMemoryStats()

      expect(mockInvoke).toHaveBeenCalledWith('get_memory_stats')
      expect(result).toEqual(mockStats)
    })

    it('should return stats with null session when not set', async () => {
      const statsNoSession: MemoryStats = {
        capacity: 100,
        used: 0,
        sessionId: null,
        agentId: null,
      }
      mockInvoke.mockResolvedValueOnce(statsNoSession)

      const result = await getMemoryStats()

      expect(result.sessionId).toBeNull()
      expect(result.agentId).toBeNull()
    })

    it('should have correct structure', async () => {
      mockInvoke.mockResolvedValueOnce(mockStats)

      const result = await getMemoryStats()

      expect(typeof result.capacity).toBe('number')
      expect(typeof result.used).toBe('number')
      expect(result.sessionId === null || typeof result.sessionId === 'number').toBe(true)
      expect(result.agentId === null || typeof result.agentId === 'number').toBe(true)
    })
  })

  // ============================================================================
  // setWorkingMemorySession
  // ============================================================================

  describe('setWorkingMemorySession', () => {
    it('should set session context with both IDs', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      await setWorkingMemorySession(42, 1)

      expect(mockInvoke).toHaveBeenCalledWith('set_working_memory_session', {
        sessionId: 42,
        agentId: 1,
      })
    })

    it('should resolve without return value', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      const result = await setWorkingMemorySession(100, 5)

      expect(result).toBeUndefined()
    })
  })

  // ============================================================================
  // pushWorkingMemoryContext
  // ============================================================================

  describe('pushWorkingMemoryContext', () => {
    it('should push user message to working memory', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      await pushWorkingMemoryContext('user', 'Hello world')

      expect(mockInvoke).toHaveBeenCalledWith('push_working_memory_context', {
        role: 'user',
        content: 'Hello world',
      })
    })

    it('should push assistant message to working memory', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      await pushWorkingMemoryContext('assistant', 'Hi there!')

      expect(mockInvoke).toHaveBeenCalledWith('push_working_memory_context', {
        role: 'assistant',
        content: 'Hi there!',
      })
    })

    it('should push system message to working memory', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      await pushWorkingMemoryContext('system', 'You are a helpful assistant.')

      expect(mockInvoke).toHaveBeenCalledWith('push_working_memory_context', {
        role: 'system',
        content: 'You are a helpful assistant.',
      })
    })

    it('should handle empty content', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)

      await pushWorkingMemoryContext('user', '')

      expect(mockInvoke).toHaveBeenCalledWith('push_working_memory_context', {
        role: 'user',
        content: '',
      })
    })
  })

  // ============================================================================
  // Type Validation
  // ============================================================================

  describe('Type validation', () => {
    it('WorkingMemoryEntry should have required fields', () => {
      const entry: WorkingMemoryEntry = {
        id: '1',
        role: 'user',
        content: 'Test content',
        timestamp: '1700000000',
      }

      expect(entry.id).toBeDefined()
      expect(entry.role).toBeDefined()
      expect(entry.content).toBeDefined()
      expect(entry.timestamp).toBeDefined()
    })

    it('WorkingMemoryRole should accept valid values', () => {
      const userRole: WorkingMemoryRole = 'user'
      const assistantRole: WorkingMemoryRole = 'assistant'
      const systemRole: WorkingMemoryRole = 'system'

      expect(userRole).toBe('user')
      expect(assistantRole).toBe('assistant')
      expect(systemRole).toBe('system')
    })

    it('MemoryStats should have all fields', () => {
      const stats: MemoryStats = {
        capacity: 100,
        used: 50,
        sessionId: 1,
        agentId: 1,
      }

      expect(stats.capacity).toBe(100)
      expect(stats.used).toBe(50)
      expect(stats.sessionId).toBe(1)
      expect(stats.agentId).toBe(1)
    })
  })
})