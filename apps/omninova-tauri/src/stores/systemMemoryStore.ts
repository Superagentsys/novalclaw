/**
 * System Memory Store - Zustand State Management
 *
 * Manages system memory stats and cache operations.
 *
 * [Source: Story 9.7 - 内存使用优化]
 */

import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';
import type {
  SystemMemoryStats,
  SystemCacheConfig,
} from '@/types/memory';

// ============================================================================
// Store Types
// ============================================================================

export interface SystemMemoryState {
  /** Current memory statistics */
  stats: SystemMemoryStats | null;
  /** Cache configuration */
  config: SystemCacheConfig | null;
  /** Loading state */
  isLoading: boolean;
  /** Error message */
  error: string | null;
}

export interface SystemMemoryActions {
  /** Load memory statistics */
  loadStats: () => Promise<void>;
  /** Load cache configuration */
  loadConfig: () => Promise<void>;
  /** Clear cache */
  clearCache: () => Promise<number>;
  /** Set cache configuration */
  setConfig: (config: SystemCacheConfig) => Promise<void>;
  /** Clear error */
  clearError: () => void;
}

export type SystemMemoryStore = SystemMemoryState & SystemMemoryActions;

// ============================================================================
// Store Implementation
// ============================================================================

/**
 * System memory store with Zustand
 */
export const useSystemMemoryStore = create<SystemMemoryStore>()(
  subscribeWithSelector((set) => ({
    // Initial state
    stats: null,
    config: null,
    isLoading: false,
    error: null,

    // Actions
    loadStats: async () => {
      set({ isLoading: true, error: null });
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const stats = await invoke<SystemMemoryStats>('get_app_memory_stats');
        set({ stats, isLoading: false });
      } catch (error) {
        console.error('Failed to load memory stats:', error);
        set({ error: '加载内存统计失败', isLoading: false });
      }
    },

    loadConfig: async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const config = await invoke<SystemCacheConfig>('get_cache_config_command');
        set({ config });
      } catch (error) {
        console.error('Failed to load cache config:', error);
      }
    },

    clearCache: async () => {
      set({ isLoading: true, error: null });
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const bytesFreed = await invoke<number>('clear_app_cache');
        // Reload stats after clearing
        const { invoke: invoke2 } = await import('@tauri-apps/api/core');
        const stats = await invoke2<SystemMemoryStats>('get_app_memory_stats');
        set({ stats, isLoading: false });
        return bytesFreed;
      } catch (error) {
        console.error('Failed to clear cache:', error);
        set({ error: '清理缓存失败', isLoading: false });
        return 0;
      }
    },

    setConfig: async (config: SystemCacheConfig) => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        await invoke('set_cache_config_command', { config });
        set({ config });
      } catch (error) {
        console.error('Failed to set cache config:', error);
        set({ error: '设置缓存配置失败' });
      }
    },

    clearError: () => {
      set({ error: null });
    },
  }))
);

export default useSystemMemoryStore;