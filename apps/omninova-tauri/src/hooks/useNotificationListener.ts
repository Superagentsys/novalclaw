/**
 * Notification Listener Hook
 *
 * Listens for notification events and updates the store.
 *
 * [Source: Story 9.3 - 系统通知管理]
 */

import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useNotificationStore } from '@/stores/notificationStore';
import type { Notification } from '@/types/notification';

/**
 * Hook to listen for new notification events
 */
export function useNotificationListener() {
  const { loadHistory } = useNotificationStore();

  useEffect(() => {
    let unlisten: (() => void) | null = null;

    const setupListener = async () => {
      try {
        unlisten = await listen<Notification>('notification:new', () => {
          // Refresh history when new notification arrives
          loadHistory();
        });
      } catch (error) {
        // Tauri may not be available in all contexts
        console.debug('Notification listener not available:', error);
      }
    };

    setupListener();

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, [loadHistory]);
}

export default useNotificationListener;