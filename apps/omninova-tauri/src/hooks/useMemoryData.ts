/**
 * Memory Data Hook
 *
 * Provides data fetching and management for the memory visualization
 * component with support for L1, L2, and L3 memory layers.
 *
 * [Source: Story 5.7 - MemoryVisualization 组件]
 */

import { useState, useCallback, useEffect, useRef } from 'react';
import {
  getWorkingMemory,
  getEpisodicMemories,
  searchSemanticMemories,
  deleteEpisodicMemory,
  deleteSemanticMemory,
  clearWorkingMemory,
  markEpisodicMemoryImportant,
  unmarkEpisodicMemoryImportant,
  type WorkingMemoryEntry,
  type EpisodicMemory,
  type SemanticSearchResult,
  type MemoryLayer,
  type UnifiedMemoryEntry,
} from '@/types/memory';

// ============================================================================
// Types
// ============================================================================

/**
 * Options for useMemoryData hook
 */
export interface UseMemoryDataOptions {
  /** Agent ID */
  agentId: number;
  /** Session ID (optional) */
  sessionId?: number | null;
  /** Target layer */
  layer?: MemoryLayer;
  /** Page size for pagination */
  pageSize?: number;
  /** Auto refresh on mount */
  autoRefresh?: boolean;
}

/**
 * Return type for useMemoryData hook
 */
export interface UseMemoryDataReturn {
  /** Memory entries */
  memories: UnifiedMemoryEntry[];
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: Error | null;
  /** Refresh data */
  refresh: () => Promise<void>;
  /** Load more entries */
  loadMore: () => Promise<void>;
  /** Whether more entries exist */
  hasMore: boolean;
  /** Total count */
  total: number;
  /** Delete a memory */
  deleteMemory: (id: string) => Promise<boolean>;
  /** Mark a memory as important */
  markMemory: (id: string) => Promise<boolean>;
  /** Unmark a memory (remove important flag) */
  unmarkMemory: (id: string) => Promise<boolean>;
  /** Current search query */
  searchQuery: string;
  /** Set search query */
  setSearchQuery: (query: string) => void;
  /** Time range filter */
  timeRange: TimeRange;
  /** Set time range filter */
  setTimeRange: (range: TimeRange) => void;
  /** Minimum importance filter */
  minImportance: number;
  /** Set minimum importance filter */
  setMinImportance: (value: number) => void;
  /** Show only marked memories */
  showMarkedOnly: boolean;
  /** Set show only marked memories */
  setShowMarkedOnly: (value: boolean) => void;
}

/**
 * Time range filter options
 */
export type TimeRange = 'today' | 'week' | 'month' | 'all';

// ============================================================================
// Constants
// ============================================================================

const DEFAULT_PAGE_SIZE = 20;

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Convert L1 working memory entry to unified format
 */
function workingToUnified(entry: WorkingMemoryEntry): UnifiedMemoryEntry {
  return {
    id: `l1-${entry.id}`,
    content: entry.content,
    role: entry.role,
    importance: 5, // Default importance for L1
    isMarked: false, // L1 doesn't support marking
    sessionId: null,
    createdAt: parseInt(entry.timestamp, 10) || Date.now(),
    sourceLayer: 'L1',
    similarityScore: null,
  };
}

/**
 * Convert L2 episodic memory entry to unified format
 */
function episodicToUnified(entry: EpisodicMemory): UnifiedMemoryEntry {
  return {
    id: `l2-${entry.id}`,
    content: entry.content,
    role: null,
    importance: entry.importance,
    isMarked: entry.isMarked,
    sessionId: entry.sessionId,
    createdAt: entry.createdAt,
    sourceLayer: 'L2',
    similarityScore: null,
  };
}

/**
 * Convert L3 semantic search result to unified format
 */
function semanticToUnified(result: SemanticSearchResult): UnifiedMemoryEntry {
  return {
    id: `l3-${result.memory.id}`,
    content: result.content || '',
    role: null,
    importance: 5, // Default importance for L3
    isMarked: false, // L3 gets isMarked from episodic, but we don't have it here
    sessionId: null,
    createdAt: result.memory.createdAt,
    sourceLayer: 'L3',
    similarityScore: result.score,
  };
}

/**
 * Filter entries by time range
 */
function filterByTimeRange(
  entries: UnifiedMemoryEntry[],
  timeRange: TimeRange
): UnifiedMemoryEntry[] {
  if (timeRange === 'all') return entries;

  const now = Date.now();
  const oneDayMs = 24 * 60 * 60 * 1000;

  let cutoff: number;
  switch (timeRange) {
    case 'today':
      cutoff = now - oneDayMs;
      break;
    case 'week':
      cutoff = now - 7 * oneDayMs;
      break;
    case 'month':
      cutoff = now - 30 * oneDayMs;
      break;
    default:
      return entries;
  }

  // Convert Unix timestamp (seconds) to milliseconds for comparison
  return entries.filter((entry) => {
    const entryTime = entry.createdAt > 1e10 ? entry.createdAt : entry.createdAt * 1000;
    return entryTime >= cutoff;
  });
}

/**
 * Filter entries by minimum importance
 */
function filterByImportance(
  entries: UnifiedMemoryEntry[],
  minImportance: number
): UnifiedMemoryEntry[] {
  if (minImportance <= 1) return entries;
  return entries.filter((entry) => entry.importance >= minImportance);
}

/**
 * Filter entries by marked status
 * [Source: Story 5.8 - 重要片段标记功能]
 */
function filterByMarked(
  entries: UnifiedMemoryEntry[],
  showMarkedOnly: boolean
): UnifiedMemoryEntry[] {
  if (!showMarkedOnly) return entries;
  return entries.filter((entry) => entry.isMarked);
}

/**
 * Filter entries by search query (local text matching)
 */
function filterBySearchQuery(
  entries: UnifiedMemoryEntry[],
  query: string
): UnifiedMemoryEntry[] {
  if (!query.trim()) return entries;
  const lowerQuery = query.toLowerCase();
  return entries.filter((entry) =>
    entry.content.toLowerCase().includes(lowerQuery)
  );
}

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * useMemoryData hook
 *
 * Manages memory data fetching and filtering for the memory visualization component.
 *
 * @example
 * ```tsx
 * function MemoryPanel({ agentId }) {
 *   const { memories, isLoading, refresh, deleteMemory } = useMemoryData({
 *     agentId,
 *     layer: 'L2',
 *   });
 *
 *   if (isLoading) return <div>Loading...</div>;
 *
 *   return (
 *     <div>
 *       {memories.map((memory) => (
 *         <div key={memory.id}>{memory.content}</div>
 *       ))}
 *     </div>
 *   );
 * }
 * ```
 */
export function useMemoryData(options: UseMemoryDataOptions): UseMemoryDataReturn {
  const {
    agentId,
    sessionId,
    layer = 'L2',
    pageSize = DEFAULT_PAGE_SIZE,
    autoRefresh = true,
  } = options;

  // State
  const [memories, setMemories] = useState<UnifiedMemoryEntry[]>([]);
  const [rawMemories, setRawMemories] = useState<UnifiedMemoryEntry[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  const [searchQuery, setSearchQuery] = useState('');
  const [timeRange, setTimeRange] = useState<TimeRange>('all');
  const [minImportance, setMinImportance] = useState(1);
  const [showMarkedOnly, setShowMarkedOnly] = useState(false);

  // Refs
  const mountedRef = useRef(true);
  const searchTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  /**
   * Fetch memories from the appropriate layer
   */
  const fetchMemories = useCallback(async (resetOffset = false) => {
    if (!mountedRef.current) return;

    setIsLoading(true);
    setError(null);

    const currentOffset = resetOffset ? 0 : offset;

    try {
      let entries: UnifiedMemoryEntry[] = [];
      let totalCount = 0;

      if (layer === 'L1') {
        // L1: Working memory
        const workingMemories = await getWorkingMemory(0);
        entries = workingMemories.map(workingToUnified);
        totalCount = entries.length;
      } else if (layer === 'L2') {
        // L2: Episodic memory
        const episodicMemories = await getEpisodicMemories(
          agentId,
          pageSize + currentOffset + 1, // Fetch extra to check hasMore
          currentOffset
        );

        // If session ID is specified, filter by session
        const filtered = sessionId
          ? episodicMemories.filter((m) => m.sessionId === sessionId)
          : episodicMemories;

        entries = filtered.slice(0, pageSize).map(episodicToUnified);
        totalCount = filtered.length;
        setHasMore(filtered.length > pageSize);
      } else if (layer === 'L3') {
        // L3: Semantic memory (search-based)
        if (searchQuery.trim()) {
          const results = await searchSemanticMemories(searchQuery, pageSize + 1, agentId);
          entries = results.slice(0, pageSize).map(semanticToUnified);
          totalCount = results.length;
          setHasMore(results.length > pageSize);
        } else {
          // No query, show empty for L3
          entries = [];
          totalCount = 0;
          setHasMore(false);
        }
      }

      if (mountedRef.current) {
        setRawMemories(entries);
        setTotal(totalCount);
        if (resetOffset) {
          setOffset(0);
        }
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
  }, [agentId, sessionId, layer, pageSize, offset, searchQuery]);

  /**
   * Apply filters to raw memories
   */
  useEffect(() => {
    let filtered = [...rawMemories];

    // Apply time range filter
    filtered = filterByTimeRange(filtered, timeRange);

    // Apply importance filter
    filtered = filterByImportance(filtered, minImportance);

    // Apply marked filter
    filtered = filterByMarked(filtered, showMarkedOnly);

    // Apply search filter (for L1 and L2, L3 uses API search)
    if (layer !== 'L3') {
      filtered = filterBySearchQuery(filtered, searchQuery);
    }

    setMemories(filtered);
  }, [rawMemories, timeRange, minImportance, searchQuery, layer, showMarkedOnly]);

  /**
   * Refresh data
   */
  const refresh = useCallback(async () => {
    await fetchMemories(true);
  }, [fetchMemories]);

  /**
   * Load more entries
   */
  const loadMore = useCallback(async () => {
    if (!hasMore || isLoading) return;
    setOffset((prev) => prev + pageSize);
    await fetchMemories(false);
  }, [hasMore, isLoading, pageSize, fetchMemories]);

  /**
   * Delete a memory entry
   */
  const handleDeleteMemory = useCallback(async (id: string): Promise<boolean> => {
    try {
      const [layerPrefix, memoryId] = id.split('-');
      const numericId = parseInt(memoryId, 10);

      if (layerPrefix === 'l1') {
        // L1: Clear individual entry not supported, clear all
        // For now, just remove from local state
        setRawMemories((prev) => prev.filter((m) => m.id !== id));
        return true;
      } else if (layerPrefix === 'l2') {
        const success = await deleteEpisodicMemory(numericId);
        if (success) {
          setRawMemories((prev) => prev.filter((m) => m.id !== id));
        }
        return success;
      } else if (layerPrefix === 'l3') {
        const success = await deleteSemanticMemory(numericId);
        if (success) {
          setRawMemories((prev) => prev.filter((m) => m.id !== id));
        }
        return success;
      }
      return false;
    } catch (err) {
      console.error('Failed to delete memory:', err);
      return false;
    }
  }, []);

  /**
   * Mark a memory as important
   * [Source: Story 5.8 - 重要片段标记功能]
   */
  const handleMarkMemory = useCallback(async (id: string): Promise<boolean> => {
    try {
      const [layerPrefix, memoryId] = id.split('-');
      const numericId = parseInt(memoryId, 10);

      if (layerPrefix === 'l2') {
        const success = await markEpisodicMemoryImportant(numericId);
        if (success) {
          setRawMemories((prev) =>
            prev.map((m) => (m.id === id ? { ...m, isMarked: true } : m))
          );
        }
        return success;
      }
      return false;
    } catch (err) {
      console.error('Failed to mark memory:', err);
      return false;
    }
  }, []);

  /**
   * Unmark a memory (remove important flag)
   * [Source: Story 5.8 - 重要片段标记功能]
   */
  const handleUnmarkMemory = useCallback(async (id: string): Promise<boolean> => {
    try {
      const [layerPrefix, memoryId] = id.split('-');
      const numericId = parseInt(memoryId, 10);

      if (layerPrefix === 'l2') {
        const success = await unmarkEpisodicMemoryImportant(numericId);
        if (success) {
          setRawMemories((prev) =>
            prev.map((m) => (m.id === id ? { ...m, isMarked: false } : m))
          );
        }
        return success;
      }
      return false;
    } catch (err) {
      console.error('Failed to unmark memory:', err);
      return false;
    }
  }, []);

  /**
   * Debounced search for L3
   */
  useEffect(() => {
    if (layer !== 'L3') return;

    // Clear existing timeout
    if (searchTimeoutRef.current) {
      clearTimeout(searchTimeoutRef.current);
    }

    // Debounce search
    searchTimeoutRef.current = setTimeout(() => {
      fetchMemories(true);
    }, 300);

    return () => {
      if (searchTimeoutRef.current) {
        clearTimeout(searchTimeoutRef.current);
      }
    };
  }, [searchQuery, layer, fetchMemories]);

  /**
   * Initial load and auto-refresh setup
   */
  useEffect(() => {
    mountedRef.current = true;

    if (autoRefresh) {
      fetchMemories(true);
    }

    return () => {
      mountedRef.current = false;
      if (searchTimeoutRef.current) {
        clearTimeout(searchTimeoutRef.current);
      }
    };
  }, [autoRefresh, fetchMemories]);

  /**
   * Re-fetch when filters change (except search which is debounced)
   */
  useEffect(() => {
    if (layer !== 'L3') {
      fetchMemories(true);
    }
  }, [layer, agentId, sessionId]);

  return {
    memories,
    isLoading,
    error,
    refresh,
    loadMore,
    hasMore,
    total,
    deleteMemory: handleDeleteMemory,
    markMemory: handleMarkMemory,
    unmarkMemory: handleUnmarkMemory,
    searchQuery,
    setSearchQuery,
    timeRange,
    setTimeRange,
    minImportance,
    setMinImportance,
    showMarkedOnly,
    setShowMarkedOnly,
  };
}

export default useMemoryData;