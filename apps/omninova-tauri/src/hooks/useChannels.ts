/**
 * 渠道状态管理 Hook
 *
 * 提供渠道连接状态查询、连接/断开操作
 *
 * [Source: Story 6.7 - ChannelStatus 组件与渠道监控]
 */

import { useState, useCallback, useEffect } from 'react';
import { toast } from 'sonner';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  type ChannelInfo,
  type ChannelEvent,
  type ChannelStatus,
} from '@/types/channel';

/**
 * Hook return type
 */
export interface UseChannelsReturn {
  /** List of channels with status */
  channels: ChannelInfo[];
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: string | null;
  /** Refresh channel list */
  refresh: () => Promise<void>;
  /** Connect a channel */
  connect: (channelId: string) => Promise<boolean>;
  /** Disconnect a channel */
  disconnect: (channelId: string) => Promise<boolean>;
  /** Retry connection for error state channel */
  retry: (channelId: string) => Promise<boolean>;
  /** Operation in progress states by channel ID */
  operationStates: Record<string, boolean>;
}

/**
 * Initialize channel manager
 */
async function initChannelManager(): Promise<void> {
  await invoke('init_channel_manager');
}

/**
 * Get all channels
 */
async function getAllChannels(): Promise<ChannelInfo[]> {
  return invoke<ChannelInfo[]>('get_all_channels');
}

/**
 * Connect a channel
 */
async function connectChannel(channelId: string): Promise<void> {
  await invoke('connect_channel', { channelId });
}

/**
 * Disconnect a channel
 */
async function disconnectChannel(channelId: string): Promise<void> {
  await invoke('disconnect_channel', { channelId });
}

/**
 * Retry channel connection
 */
async function retryChannelConnection(channelId: string): Promise<void> {
  await invoke('retry_channel_connection', { channelId });
}

/**
 * Hook for managing channel status
 */
export function useChannels(): UseChannelsReturn {
  const [channels, setChannels] = useState<ChannelInfo[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [operationStates, setOperationStates] = useState<Record<string, boolean>>({});

  /**
   * Refresh channel list
   */
  const refresh = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      // Ensure channel manager is initialized
      await initChannelManager();
      const channelList = await getAllChannels();
      setChannels(channelList);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      console.error('Failed to load channels:', err);
    } finally {
      setIsLoading(false);
    }
  }, []);

  /**
   * Connect a channel
   */
  const connect = useCallback(async (channelId: string): Promise<boolean> => {
    setOperationStates((prev) => ({ ...prev, [channelId]: true }));

    try {
      await connectChannel(channelId);

      // Update channel status locally
      setChannels((prev) =>
        prev.map((ch) =>
          ch.id === channelId ? { ...ch, status: 'connected' as ChannelStatus } : ch
        )
      );

      toast.success(`渠道已连接`);
      return true;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      toast.error(`连接失败: ${message}`);

      // Update channel with error
      setChannels((prev) =>
        prev.map((ch) =>
          ch.id === channelId
            ? { ...ch, status: 'error' as ChannelStatus, errorMessage: message }
            : ch
        )
      );

      return false;
    } finally {
      setOperationStates((prev) => {
        const next = { ...prev };
        delete next[channelId];
        return next;
      });
    }
  }, []);

  /**
   * Disconnect a channel
   */
  const disconnect = useCallback(async (channelId: string): Promise<boolean> => {
    setOperationStates((prev) => ({ ...prev, [channelId]: true }));

    try {
      await disconnectChannel(channelId);

      // Update channel status locally
      setChannels((prev) =>
        prev.map((ch) =>
          ch.id === channelId ? { ...ch, status: 'disconnected' as ChannelStatus } : ch
        )
      );

      toast.success(`渠道已断开`);
      return true;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      toast.error(`断开失败: ${message}`);
      return false;
    } finally {
      setOperationStates((prev) => {
        const next = { ...prev };
        delete next[channelId];
        return next;
      });
    }
  }, []);

  /**
   * Retry connection for error state channel
   */
  const retry = useCallback(async (channelId: string): Promise<boolean> => {
    setOperationStates((prev) => ({ ...prev, [channelId]: true }));

    // Set status to connecting
    setChannels((prev) =>
      prev.map((ch) =>
        ch.id === channelId ? { ...ch, status: 'connecting' as ChannelStatus } : ch
      )
    );

    try {
      await retryChannelConnection(channelId);

      // Update channel status locally
      setChannels((prev) =>
        prev.map((ch) =>
          ch.id === channelId ? { ...ch, status: 'connected' as ChannelStatus, errorMessage: null } : ch
        )
      );

      toast.success(`渠道重连成功`);
      return true;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      toast.error(`重连失败: ${message}`);

      // Update channel with error
      setChannels((prev) =>
        prev.map((ch) =>
          ch.id === channelId
            ? { ...ch, status: 'error' as ChannelStatus, errorMessage: message }
            : ch
        )
      );

      return false;
    } finally {
      setOperationStates((prev) => {
        const next = { ...prev };
        delete next[channelId];
        return next;
      });
    }
  }, []);

  // Load channels on mount and listen for events
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;

    const setup = async () => {
      // Initial load
      await refresh();

      // Listen for channel events
      unlisten = await listen<ChannelEvent>('channel-event', (event) => {
        const channelEvent = event.payload;

        setChannels((prev) => {
          const channelId = 'channelId' in channelEvent ? channelEvent.channelId : '';

          switch (channelEvent.type) {
            case 'connected':
              return prev.map((ch) =>
                ch.id === channelId ? { ...ch, status: 'connected' as ChannelStatus } : ch
              );

            case 'disconnected':
              return prev.map((ch) =>
                ch.id === channelId
                  ? { ...ch, status: 'disconnected' as ChannelStatus }
                  : ch
              );

            case 'error':
              return prev.map((ch) =>
                ch.id === channelId
                  ? {
                      ...ch,
                      status: 'error' as ChannelStatus,
                      errorMessage: channelEvent.error,
                    }
                  : ch
              );

            case 'reconnecting':
              return prev.map((ch) =>
                ch.id === channelId
                  ? { ...ch, status: 'connecting' as ChannelStatus }
                  : ch
              );

            case 'message_received':
              return prev.map((ch) =>
                ch.id === channelId
                  ? {
                      ...ch,
                      messagesReceived: ch.messagesReceived + 1,
                      lastActivity: Math.floor(Date.now() / 1000),
                    }
                  : ch
              );

            case 'message_sent':
              return prev.map((ch) =>
                ch.id === channelId
                  ? {
                      ...ch,
                      messagesSent: ch.messagesSent + 1,
                      lastActivity: Math.floor(Date.now() / 1000),
                    }
                  : ch
              );

            default:
              return prev;
          }
        });
      });
    };

    setup();

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, [refresh]);

  return {
    channels,
    isLoading,
    error,
    refresh,
    connect,
    disconnect,
    retry,
    operationStates,
  };
}