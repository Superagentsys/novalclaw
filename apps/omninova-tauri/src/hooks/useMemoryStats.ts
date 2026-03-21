/**
 * Memory Stats Hook
 *
 * Provides periodic polling of memory system statistics for the
 * three-layer memory system (L1/L2/L3).
 *
 * [Source: Story 5.6 - MemoryLayerIndicator 组件]
 */

import { useState, useCallback, useEffect, useRef } from 'react';
import { getMemoryManagerStats, type MemoryManagerStats } from '@/types/memory';

// ============================================================================
// Types
// ============================================================================

/**
 * Options for useMemoryStats hook
 */
export interface UseMemoryStatsOptions {
  /** Polling interval in milliseconds (default: 5000) */
  interval?: number;
  /** Enable auto-refresh polling (default: true) */
  autoRefresh?: boolean;
}

/**
 * Return type for useMemoryStats hook
 */
export interface UseMemoryStatsReturn {
  /** Current memory statistics */
  stats: MemoryManagerStats | null;
  /** Whether stats are currently loading */
  isLoading: boolean;
  /** Error message if any */
  error: Error | null;
  /** Manually refresh stats */
  refresh: () => Promise<void>;
}

// ============================================================================
// Constants
// ============================================================================

const DEFAULT_INTERVAL = 5000; // 5 seconds

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * useMemoryStats hook
 *
 * Periodically fetches memory system statistics for all three layers.
 *
 * @example
 * ```tsx
 * function MemoryDisplay() {
 *   const { stats, isLoading, error } = useMemoryStats({ interval: 3000 });
 *
 *   if (isLoading) return <div>Loading...</div>;
 *   if (error) return <div>Error: {error.message}</div>;
 *   if (!stats) return <div>No stats available</div>;
 *
 *   return (
 *     <div>
 *       <div>L1: {stats.l1Used}/{stats.l1Capacity}</div>
 *       <div>L2: {stats.l2Total}</div>
 *       <div>L3: {stats.l3Total}</div>
 *     </div>
 *   );
 * }
 * ```
 */
export function useMemoryStats(options: UseMemoryStatsOptions = {}): UseMemoryStatsReturn {
  const { interval = DEFAULT_INTERVAL, autoRefresh = true } = options;

  const [stats, setStats] = useState<MemoryManagerStats | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  // Track mounted state to avoid state updates after unmount
  const mountedRef = useRef(true);
  // Track if this is the initial load (for showing loading indicator)
  const hasLoadedRef = useRef(false);

  /**
   * Fetch memory stats from backend
   */
  const refresh = useCallback(async () => {
    // Only show loading indicator on initial load
    if (!hasLoadedRef.current) {
      setIsLoading(true);
    }

    try {
      const result = await getMemoryManagerStats();
      if (mountedRef.current) {
        setStats(result);
        setError(null);
        hasLoadedRef.current = true;
      }
    } catch (err) {
      if (mountedRef.current) {
        setError(err instanceof Error ? err : new Error(String(err)));
      }
    } finally {
      if (mountedRef.current) {
        setIsLoading(false);
      }
    }
  }, []); // No dependencies - stable reference

  // Initial load
  useEffect(() => {
    mountedRef.current = true;
    hasLoadedRef.current = false;
    refresh();

    return () => {
      mountedRef.current = false;
    };
  }, [refresh]);

  // Set up polling interval
  useEffect(() => {
    if (!autoRefresh || interval <= 0) {
      return;
    }

    const timerId = setInterval(() => {
      refresh();
    }, interval);

    return () => {
      clearInterval(timerId);
    };
  }, [autoRefresh, interval, refresh]);

  return {
    stats,
    isLoading,
    error,
    refresh,
  };
}

export default useMemoryStats;