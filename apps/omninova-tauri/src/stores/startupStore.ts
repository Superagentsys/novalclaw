/**
 * Startup Store - Zustand State Management
 *
 * Manages startup progress and performance tracking.
 *
 * [Source: Story 9.6 - 应用启动优化]
 */

import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';
import type {
  StartupPhase,
  StartupProgress,
  StartupReport,
} from '@/types/startup';
import {
  STARTUP_PHASE_MESSAGES,
  STARTUP_PHASE_PROGRESS,
} from '@/types/startup';

// ============================================================================
// Store Types
// ============================================================================

export interface StartupState {
  /** 当前启动进度 */
  progress: StartupProgress;
  /** 启动报告 */
  report: StartupReport | null;
  /** 是否正在加载 */
  isLoading: boolean;
}

export interface StartupActions {
  /** 设置启动阶段 */
  setPhase: (phase: StartupPhase) => void;
  /** 加载启动报告 */
  loadReport: () => Promise<void>;
  /** 记录里程碑 */
  recordMilestone: (name: string) => Promise<void>;
}

export type StartupStore = StartupState & StartupActions;

// ============================================================================
// Helper Functions
// ============================================================================

function createProgress(phase: StartupPhase): StartupProgress {
  return {
    phase,
    message: STARTUP_PHASE_MESSAGES[phase],
    progress: STARTUP_PHASE_PROGRESS[phase],
  };
}

// ============================================================================
// Store Implementation
// ============================================================================

/**
 * Startup store with Zustand
 */
export const useStartupStore = create<StartupStore>()(
  subscribeWithSelector((set) => ({
    // Initial state
    progress: createProgress('initializing'),
    report: null,
    isLoading: false,

    // Actions
    setPhase: (phase: StartupPhase) => {
      set({ progress: createProgress(phase) });
    },

    loadReport: async () => {
      set({ isLoading: true });
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const report = await invoke<StartupReport>('get_startup_report');
        set({ report, isLoading: false });

        // Update phase based on report
        if (report.is_ready) {
          set({ progress: createProgress('ready') });
        }
      } catch (error) {
        console.error('Failed to load startup report:', error);
        set({ isLoading: false });
      }
    },

    recordMilestone: async (name: string) => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        await invoke('record_startup_milestone', { name });
      } catch (error) {
        console.error('Failed to record milestone:', error);
      }
    },
  }))
);

export default useStartupStore;