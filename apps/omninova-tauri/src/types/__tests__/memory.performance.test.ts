/**
 * Memory Performance Metrics Tests (Story 5.5)
 *
 * Tests for the performance metrics TypeScript API functions.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
  getMemoryPerformanceStats,
  runMemoryBenchmark,
  type PerformanceStats,
  type BenchmarkResults,
} from '../memory'

// Mock Tauri API
const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
}))

describe('Memory Performance Metrics API (Story 5.5)', () => {
  beforeEach(() => {
    mockInvoke.mockReset()
  })

  // ============================================================================
  // getMemoryPerformanceStats
  // ============================================================================

  describe('getMemoryPerformanceStats', () => {
    const mockStats: PerformanceStats = {
      l1_total_queries: 100,
      l1_cache_hits: 75,
      l1_avg_latency_ms: 3.5,
      l1_max_latency_ms: 8.0,
      l2_total_queries: 50,
      l2_avg_latency_ms: 45.0,
      l2_max_latency_ms: 120.0,
      l3_total_queries: 20,
      l3_avg_latency_ms: 150.0,
      l3_max_latency_ms: 300.0,
      total_queries: 170,
      overall_cache_hit_rate: 0.75,
      overall_avg_latency_ms: 35.0,
      window_secs: 300,
    }

    it('should return performance statistics', async () => {
      mockInvoke.mockResolvedValueOnce(mockStats)

      const result = await getMemoryPerformanceStats()

      expect(mockInvoke).toHaveBeenCalledWith('memory_get_performance_stats')
      expect(result).toEqual(mockStats)
    })

    it('should return stats with zero queries', async () => {
      const emptyStats: PerformanceStats = {
        l1_total_queries: 0,
        l1_cache_hits: 0,
        l1_avg_latency_ms: 0,
        l1_max_latency_ms: 0,
        l2_total_queries: 0,
        l2_avg_latency_ms: 0,
        l2_max_latency_ms: 0,
        l3_total_queries: 0,
        l3_avg_latency_ms: 0,
        l3_max_latency_ms: 0,
        total_queries: 0,
        overall_cache_hit_rate: 0,
        overall_avg_latency_ms: 0,
        window_secs: 300,
      }
      mockInvoke.mockResolvedValueOnce(emptyStats)

      const result = await getMemoryPerformanceStats()

      expect(result.total_queries).toBe(0)
      expect(result.l1_cache_hits).toBe(0)
    })

    it('should have correct structure', async () => {
      mockInvoke.mockResolvedValueOnce(mockStats)

      const result = await getMemoryPerformanceStats()

      expect(typeof result.l1_total_queries).toBe('number')
      expect(typeof result.l1_cache_hits).toBe('number')
      expect(typeof result.l1_avg_latency_ms).toBe('number')
      expect(typeof result.l2_total_queries).toBe('number')
      expect(typeof result.l3_total_queries).toBe('number')
      expect(typeof result.overall_cache_hit_rate).toBe('number')
    })
  })

  // ============================================================================
  // runMemoryBenchmark
  // ============================================================================

  describe('runMemoryBenchmark', () => {
    const mockResults: BenchmarkResults = {
      l1_retrieve_ms: 2.5,
      l2_retrieve_ms: 35.0,
      l3_search_ms: 120.0,
      l3_available: true,
      combined_retrieve_ms: 150.0,
    }

    it('should return benchmark results', async () => {
      mockInvoke.mockResolvedValueOnce(mockResults)

      const result = await runMemoryBenchmark()

      expect(mockInvoke).toHaveBeenCalledWith('memory_benchmark')
      expect(result).toEqual(mockResults)
    })

    it('should return results with L3 unavailable', async () => {
      const noL3Results: BenchmarkResults = {
        l1_retrieve_ms: 2.5,
        l2_retrieve_ms: 35.0,
        l3_search_ms: 0,
        l3_available: false,
        combined_retrieve_ms: 40.0,
      }
      mockInvoke.mockResolvedValueOnce(noL3Results)

      const result = await runMemoryBenchmark()

      expect(result.l3_available).toBe(false)
    })

    it('should have correct structure', async () => {
      mockInvoke.mockResolvedValueOnce(mockResults)

      const result = await runMemoryBenchmark()

      expect(typeof result.l1_retrieve_ms).toBe('number')
      expect(typeof result.l2_retrieve_ms).toBe('number')
      expect(typeof result.l3_search_ms).toBe('number')
      expect(typeof result.l3_available).toBe('boolean')
      expect(typeof result.combined_retrieve_ms).toBe('number')
    })

    it('should propagate errors from backend', async () => {
      mockInvoke.mockRejectedValueOnce(new Error('Benchmark failed'))

      await expect(runMemoryBenchmark()).rejects.toThrow('Benchmark failed')
    })
  })

  // ============================================================================
  // Type Validation
  // ============================================================================

  describe('Type validation', () => {
    it('PerformanceStats should have all required fields', () => {
      const stats: PerformanceStats = {
        l1_total_queries: 100,
        l1_cache_hits: 75,
        l1_avg_latency_ms: 3.5,
        l1_max_latency_ms: 8.0,
        l2_total_queries: 50,
        l2_avg_latency_ms: 45.0,
        l2_max_latency_ms: 120.0,
        l3_total_queries: 20,
        l3_avg_latency_ms: 150.0,
        l3_max_latency_ms: 300.0,
        total_queries: 170,
        overall_cache_hit_rate: 0.75,
        overall_avg_latency_ms: 35.0,
        window_secs: 300,
      }

      expect(stats.l1_total_queries).toBeDefined()
      expect(stats.l2_total_queries).toBeDefined()
      expect(stats.l3_total_queries).toBeDefined()
      expect(stats.overall_cache_hit_rate).toBeDefined()
    })

    it('BenchmarkResults should have all required fields', () => {
      const results: BenchmarkResults = {
        l1_retrieve_ms: 2.5,
        l2_retrieve_ms: 35.0,
        l3_search_ms: 120.0,
        l3_available: true,
        combined_retrieve_ms: 150.0,
      }

      expect(results.l1_retrieve_ms).toBeDefined()
      expect(results.l2_retrieve_ms).toBeDefined()
      expect(results.l3_search_ms).toBeDefined()
      expect(results.l3_available).toBeDefined()
      expect(results.combined_retrieve_ms).toBeDefined()
    })
  })
})