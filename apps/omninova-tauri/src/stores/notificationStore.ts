/**
 * Notification Store - Zustand State Management
 *
 * Manages notification configuration and history.
 *
 * [Source: Story 9.3 - 系统通知管理]
 */

import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';
import type {
  Notification,
  NotificationConfig,
  NotificationType,
} from '@/types/notification';
import { DEFAULT_NOTIFICATION_CONFIG } from '@/types/notification';

// ============================================================================
// Store Types
// ============================================================================

export interface NotificationState {
  /** Current notification configuration */
  config: NotificationConfig;
  /** Notification history */
  history: Notification[];
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: string | null;
}

export interface NotificationActions {
  /** Load notification configuration from backend */
  loadConfig: () => Promise<void>;
  /** Update notification configuration */
  updateConfig: (config: NotificationConfig) => Promise<void>;
  /** Load notification history */
  loadHistory: (limit?: number) => Promise<void>;
  /** Mark notification as read */
  markAsRead: (id: string) => Promise<void>;
  /** Mark all notifications as read */
  markAllRead: () => void;
  /** Clear notification history */
  clearHistory: () => Promise<void>;
  /** Send test notification */
  sendTestNotification: () => Promise<void>;
  /** Toggle notification type */
  toggleNotificationType: (type: NotificationType) => void;
  /** Set quiet hours */
  setQuietHours: (start: number, end: number) => void;
  /** Clear quiet hours (disable) */
  clearQuietHours: () => void;
  /** Enable/disable notifications */
  setEnabled: (enabled: boolean) => void;
  /** Enable/disable sound */
  setSoundEnabled: (enabled: boolean) => void;
  /** Clear error */
  clearError: () => void;
}

export type NotificationStore = NotificationState & NotificationActions;

// ============================================================================
// Store Implementation
// ============================================================================

/**
 * Notification store with Zustand
 *
 * Uses subscribeWithSelector for fine-grained subscriptions
 */
export const useNotificationStore = create<NotificationStore>()(
  subscribeWithSelector((set, get) => ({
    // Initial state
    config: {
      enabled: true,
      enabledTypes: ['error', 'system_update', 'performance_warning'],
      soundEnabled: true,
      quietHoursStart: 22,
      quietHoursEnd: 8,
    },
    history: [],
    isLoading: false,
    error: null,

    // Actions
    loadConfig: async () => {
      set({ isLoading: true, error: null });
      try {
        // For now, use local storage as fallback
        const stored = localStorage.getItem('notification-config');
        if (stored) {
          const config = JSON.parse(stored) as NotificationConfig;
          set({ config, isLoading: false });
        } else {
          set({ isLoading: false });
        }
      } catch (error) {
        console.error('Failed to load notification config:', error);
        set({ error: '加载通知配置失败', isLoading: false });
      }
    },

    updateConfig: async (config: NotificationConfig) => {
      set({ isLoading: true });
      try {
        // Save to local storage
        localStorage.setItem('notification-config', JSON.stringify(config));
        set({ config, isLoading: false });
      } catch (error) {
        console.error('Failed to update notification config:', error);
        set({ error: '更新通知配置失败', isLoading: false });
      }
    },

    loadHistory: async (limit = 100) => {
      set({ isLoading: true });
      try {
        // For now, use local storage as fallback
        const stored = localStorage.getItem('notification-history');
        if (stored) {
          const history = JSON.parse(stored) as Notification[];
          set({ history: history.slice(0, limit), isLoading: false });
        } else {
          set({ history: [], isLoading: false });
        }
      } catch (error) {
        console.error('Failed to load notification history:', error);
        set({ error: '加载通知历史失败', isLoading: false });
      }
    },

    markAsRead: async (id: string) => {
      set((state) => {
        const history = state.history.map((n) =>
          n.id === id ? { ...n, read: true } : n
        );
        // Persist to local storage
        localStorage.setItem('notification-history', JSON.stringify(history));
        return { history };
      });
    },

    markAllRead: () => {
      set((state) => {
        const history = state.history.map((n) => ({ ...n, read: true }));
        localStorage.setItem('notification-history', JSON.stringify(history));
        return { history };
      });
    },

    clearHistory: async () => {
      localStorage.removeItem('notification-history');
      set({ history: [] });
    },

    sendTestNotification: async () => {
      try {
        // Add a test notification to history
        const testNotification: Notification = {
          id: `test-${Date.now()}`,
          notificationType: 'custom',
          title: '测试通知',
          body: '这是一条测试通知消息',
          priority: 'normal',
          createdAt: Math.floor(Date.now() / 1000),
          read: false,
        };

        set((state) => {
          const history = [testNotification, ...state.history].slice(0, 100);
          localStorage.setItem('notification-history', JSON.stringify(history));
          return { history };
        });
      } catch (error) {
        console.error('Failed to send test notification:', error);
        set({ error: '发送测试通知失败' });
      }
    },

    toggleNotificationType: (type: NotificationType) => {
      const { config, updateConfig } = get();
      const enabledTypes = config.enabledTypes.includes(type)
        ? config.enabledTypes.filter((t) => t !== type)
        : [...config.enabledTypes, type];
      updateConfig({ ...config, enabledTypes });
    },

    setQuietHours: (start: number, end: number) => {
      const { config, updateConfig } = get();
      updateConfig({
        ...config,
        quietHoursStart: start,
        quietHoursEnd: end,
      });
    },

    clearQuietHours: () => {
      const { config, updateConfig } = get();
      updateConfig({
        ...config,
        quietHoursStart: undefined,
        quietHoursEnd: undefined,
      });
    },

    setEnabled: (enabled: boolean) => {
      const { config, updateConfig } = get();
      updateConfig({ ...config, enabled });
    },

    setSoundEnabled: (soundEnabled: boolean) => {
      const { config, updateConfig } = get();
      updateConfig({ ...config, soundEnabled });
    },

    clearError: () => {
      set({ error: null });
    },
  }))
);

export default useNotificationStore;