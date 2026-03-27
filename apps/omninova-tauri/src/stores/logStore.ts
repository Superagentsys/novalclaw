/**
 * Log Store - Zustand State Management
 *
 * Manages log entries, filtering, and pagination.
 *
 * [Source: Story 9.4 - 日志查看器实现]
 */

import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';
import type { LogEntry, LogLevel, LogQuery, LogStats } from '@/types/log';
import { DEFAULT_PAGE_SIZE } from '@/types/log';

// ============================================================================
// Store Types
// ============================================================================

export interface LogState {
  /** Log entries */
  entries: LogEntry[];
  /** Log statistics */
  stats: LogStats | null;
  /** Current query parameters */
  query: LogQuery;
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: string | null;
}

export interface LogActions {
  /** Load log entries */
  loadLogs: (query?: Partial<LogQuery>) => Promise<void>;
  /** Load more entries (append) */
  loadMore: () => Promise<void>;
  /** Refresh current query */
  refresh: () => Promise<void>;
  /** Set level filter */
  setLevelFilter: (levels: LogLevel[]) => void;
  /** Set keyword search */
  setKeyword: (keyword: string) => void;
  /** Set time range */
  setTimeRange: (start?: number, end?: number) => void;
  /** Clear all filters */
  clearFilters: () => void;
  /** Clear error */
  clearError: () => void;
  /** Load log statistics */
  loadStats: () => Promise<void>;
  /** Clear logs */
  clearLogs: (before?: number) => Promise<void>;
}

export type LogStore = LogState & LogActions;

// ============================================================================
// Store Implementation
// ============================================================================

const DEFAULT_QUERY: LogQuery = {
  limit: DEFAULT_PAGE_SIZE,
  offset: 0,
};

/**
 * Log store with Zustand
 *
 * Uses subscribeWithSelector for fine-grained subscriptions
 */
export const useLogStore = create<LogStore>()(
  subscribeWithSelector((set, get) => ({
    // Initial state
    entries: [],
    stats: null,
    query: { ...DEFAULT_QUERY },
    isLoading: false,
    error: null,

    // Actions
    loadLogs: async (query?: Partial<LogQuery>) => {
      set({ isLoading: true, error: null });
      try {
        const newQuery = query
          ? { ...DEFAULT_QUERY, ...query }
          : { ...DEFAULT_QUERY };

        // Use localStorage as fallback (no Tauri backend integration yet)
        const stored = localStorage.getItem('log-entries');
        let entries: LogEntry[] = stored ? JSON.parse(stored) : [];

        // Apply filters
        if (newQuery.levels && newQuery.levels.length > 0) {
          entries = entries.filter((e) => newQuery.levels!.includes(e.level));
        }

        if (newQuery.keyword) {
          const kw = newQuery.keyword.toLowerCase();
          entries = entries.filter(
            (e) =>
              e.message.toLowerCase().includes(kw) ||
              e.target.toLowerCase().includes(kw)
          );
        }

        if (newQuery.startTime) {
          entries = entries.filter((e) => e.timestamp >= newQuery.startTime!);
        }

        if (newQuery.endTime) {
          entries = entries.filter((e) => e.timestamp <= newQuery.endTime!);
        }

        // Sort by timestamp (newest first)
        entries.sort((a, b) => b.timestamp - a.timestamp);

        // Apply pagination
        const offset = newQuery.offset || 0;
        const limit = newQuery.limit || DEFAULT_PAGE_SIZE;
        entries = entries.slice(offset, offset + limit);

        set({ entries, query: newQuery, isLoading: false });
      } catch (error) {
        console.error('Failed to load logs:', error);
        set({ error: '加载日志失败', isLoading: false });
      }
    },

    loadMore: async () => {
      const { query, entries } = get();
      const offset = (query.offset || 0) + entries.length;

      set({ isLoading: true, error: null });
      try {
        const stored = localStorage.getItem('log-entries');
        let allEntries: LogEntry[] = stored ? JSON.parse(stored) : [];

        // Apply filters
        if (query.levels && query.levels.length > 0) {
          allEntries = allEntries.filter((e) => query.levels!.includes(e.level));
        }

        if (query.keyword) {
          const kw = query.keyword.toLowerCase();
          allEntries = allEntries.filter(
            (e) =>
              e.message.toLowerCase().includes(kw) ||
              e.target.toLowerCase().includes(kw)
          );
        }

        if (query.startTime) {
          allEntries = allEntries.filter((e) => e.timestamp >= query.startTime!);
        }

        if (query.endTime) {
          allEntries = allEntries.filter((e) => e.timestamp <= query.endTime!);
        }

        allEntries.sort((a, b) => b.timestamp - a.timestamp);

        const limit = query.limit || DEFAULT_PAGE_SIZE;
        const newEntries = allEntries.slice(offset, offset + limit);

        set({
          entries: [...entries, ...newEntries],
          query: { ...query, offset },
          isLoading: false,
        });
      } catch (error) {
        console.error('Failed to load more logs:', error);
        set({ error: '加载更多日志失败', isLoading: false });
      }
    },

    refresh: async () => {
      const { query } = get();
      await get().loadLogs(query);
    },

    setLevelFilter: (levels: LogLevel[]) => {
      const { query } = get();
      set({ query: { ...query, levels, offset: 0 } });
      get().loadLogs({ ...query, levels, offset: 0 });
    },

    setKeyword: (keyword: string) => {
      const { query } = get();
      set({ query: { ...query, keyword, offset: 0 } });
      get().loadLogs({ ...query, keyword, offset: 0 });
    },

    setTimeRange: (start?: number, end?: number) => {
      const { query } = get();
      set({ query: { ...query, startTime: start, endTime: end, offset: 0 } });
      get().loadLogs({ ...query, startTime: start, endTime: end, offset: 0 });
    },

    clearFilters: () => {
      set({ query: { ...DEFAULT_QUERY } });
      get().loadLogs(DEFAULT_QUERY);
    },

    clearError: () => {
      set({ error: null });
    },

    loadStats: async () => {
      try {
        const stored = localStorage.getItem('log-entries');
        const entries: LogEntry[] = stored ? JSON.parse(stored) : [];

        const stats: LogStats = {
          fileSize: stored ? stored.length : 0,
          entryCount: entries.length,
          oldestEntry: entries.length > 0
            ? Math.min(...entries.map((e) => e.timestamp))
            : undefined,
          newestEntry: entries.length > 0
            ? Math.max(...entries.map((e) => e.timestamp))
            : undefined,
        };

        set({ stats });
      } catch (error) {
        console.error('Failed to load log stats:', error);
      }
    },

    clearLogs: async (before?: number) => {
      try {
        if (before) {
          const stored = localStorage.getItem('log-entries');
          let entries: LogEntry[] = stored ? JSON.parse(stored) : [];
          entries = entries.filter((e) => e.timestamp >= before);
          localStorage.setItem('log-entries', JSON.stringify(entries));
        } else {
          localStorage.removeItem('log-entries');
        }
        await get().refresh();
        await get().loadStats();
      } catch (error) {
        console.error('Failed to clear logs:', error);
        set({ error: '清除日志失败' });
      }
    },
  }))
);

export default useLogStore;