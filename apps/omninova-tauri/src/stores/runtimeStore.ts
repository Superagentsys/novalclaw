/**
 * Runtime Store - Zustand State Management
 *
 * Manages runtime mode and auto-start settings.
 *
 * [Source: Story 9.5 - 运行模式管理]
 */

import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';
import type { RunMode } from '@/types/runtime';

// ============================================================================
// Store Types
// ============================================================================

export interface RuntimeState {
  /** 当前运行模式 */
  mode: RunMode;
  /** 是否开机自启动 */
  autoStart: boolean;
  /** 加载状态 */
  isLoading: boolean;
  /** 错误信息 */
  error: string | null;
}

export interface RuntimeActions {
  /** 设置运行模式 */
  setMode: (mode: RunMode) => Promise<void>;
  /** 设置开机自启动 */
  setAutoStart: (enabled: boolean) => Promise<void>;
  /** 加载配置 */
  loadConfig: () => Promise<void>;
  /** 清除错误 */
  clearError: () => void;
}

export type RuntimeStore = RuntimeState & RuntimeActions;

// ============================================================================
// Store Implementation
// ============================================================================

/**
 * Runtime store with Zustand
 *
 * Uses subscribeWithSelector for fine-grained subscriptions
 */
export const useRuntimeStore = create<RuntimeStore>()(
  subscribeWithSelector((set) => ({
    // Initial state
    mode: 'desktop',
    autoStart: false,
    isLoading: false,
    error: null,

    // Actions
    setMode: async (mode: RunMode) => {
      set({ isLoading: true, error: null });
      try {
        // Dynamic import to avoid bundling issues
        const { invoke } = await import('@tauri-apps/api/core');
        await invoke('set_run_mode', { mode });
        set({ mode, isLoading: false });
      } catch (error) {
        console.error('Failed to set run mode:', error);
        set({ error: '设置运行模式失败', isLoading: false });
      }
    },

    setAutoStart: async (enabled: boolean) => {
      set({ isLoading: true, error: null });
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        await invoke('set_auto_start', { enabled });
        set({ autoStart: enabled, isLoading: false });
      } catch (error) {
        console.error('Failed to set auto start:', error);
        set({ error: '设置开机自启动失败', isLoading: false });
      }
    },

    loadConfig: async () => {
      set({ isLoading: true, error: null });
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const mode = await invoke<RunMode>('get_run_mode');
        const autoStart = await invoke<boolean>('get_auto_start');
        set({ mode, autoStart, isLoading: false });
      } catch (error) {
        console.error('Failed to load runtime config:', error);
        set({ error: '加载配置失败', isLoading: false });
      }
    },

    clearError: () => {
      set({ error: null });
    },
  }))
);

export default useRuntimeStore;