/**
 * 渠道配置管理 Hook
 *
 * 提供渠道配置的创建、更新、删除和测试连接功能
 *
 * [Source: Story 6.8 - 渠道配置界面]
 */

import { useState, useCallback } from 'react';
import { toast } from 'sonner';
import { invoke } from '@tauri-apps/api/core';
import {
  type ChannelConfig,
  type ChannelInfo,
  type ChannelKind,
  type ChannelBehaviorConfig,
  type SlackCredentials,
  type DiscordCredentials,
  type EmailCredentials,
  type TelegramCredentials,
} from '@/types/channel';

/**
 * Hook return type
 */
export interface UseChannelConfigReturn {
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: string | null;
  /** Create a new channel */
  createChannel: (config: ChannelConfig, credentials: ChannelCredentialsData) => Promise<ChannelInfo | null>;
  /** Update an existing channel */
  updateChannel: (channelId: string, config: ChannelConfig, credentials?: ChannelCredentialsData) => Promise<boolean>;
  /** Delete a channel */
  deleteChannel: (channelId: string) => Promise<boolean>;
  /** Test channel connection */
  testConnection: (channelId: string) => Promise<boolean>;
  /** Get channel config */
  getChannelConfig: (channelId: string) => Promise<ChannelConfig | null>;
  /** Save channel behavior config */
  saveBehaviorConfig: (channelId: string, behavior: ChannelBehaviorConfig) => Promise<boolean>;
}

/**
 * Channel credentials data (union of all types)
 */
export type ChannelCredentialsData =
  | { kind: 'slack'; data: SlackCredentials }
  | { kind: 'discord'; data: DiscordCredentials }
  | { kind: 'email'; data: EmailCredentials }
  | { kind: 'telegram'; data: TelegramCredentials }
  | { kind: string; data: Record<string, unknown> };

/**
 * Create channel request
 */
interface CreateChannelRequest {
  name: string;
  kind: ChannelKind;
  enabled: boolean;
  behavior: ChannelBehaviorConfig;
  agentId?: string;
}

/**
 * Create channel response
 */
interface CreateChannelResponse {
  id: string;
  name: string;
  kind: ChannelKind;
  status: string;
}

/**
 * Hook for managing channel configuration
 */
export function useChannelConfig(): UseChannelConfigReturn {
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /**
   * Create a new channel
   */
  const createChannel = useCallback(async (
    config: ChannelConfig,
    credentials: ChannelCredentialsData
  ): Promise<ChannelInfo | null> => {
    setIsLoading(true);
    setError(null);

    try {
      // Create the channel first
      const request: CreateChannelRequest = {
        name: config.name,
        kind: config.kind,
        enabled: config.enabled,
        behavior: config.behavior,
        agentId: config.agentId,
      };

      const response = await invoke<CreateChannelResponse>('create_channel', {
        config: request,
      });

      // Save credentials
      await invoke('save_channel_credentials', {
        channelId: response.id,
        credentials: credentials.data,
      });

      toast.success(`渠道 "${config.name}" 创建成功`);

      // Return as ChannelInfo
      return {
        id: response.id,
        name: response.name,
        kind: response.kind,
        status: response.status as ChannelInfo['status'],
        capabilities: 0,
        messagesSent: 0,
        messagesReceived: 0,
        lastActivity: null,
        errorMessage: null,
      };
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      toast.error(`创建渠道失败: ${message}`);
      return null;
    } finally {
      setIsLoading(false);
    }
  }, []);

  /**
   * Update an existing channel
   */
  const updateChannel = useCallback(async (
    channelId: string,
    config: ChannelConfig,
    credentials?: ChannelCredentialsData
  ): Promise<boolean> => {
    setIsLoading(true);
    setError(null);

    try {
      await invoke('update_channel', {
        channelId,
        config: {
          name: config.name,
          enabled: config.enabled,
          behavior: config.behavior,
          agentId: config.agentId,
        },
      });

      // Update credentials if provided
      if (credentials) {
        await invoke('save_channel_credentials', {
          channelId,
          credentials: credentials.data,
        });
      }

      toast.success(`渠道 "${config.name}" 更新成功`);
      return true;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      toast.error(`更新渠道失败: ${message}`);
      return false;
    } finally {
      setIsLoading(false);
    }
  }, []);

  /**
   * Delete a channel
   */
  const deleteChannel = useCallback(async (channelId: string): Promise<boolean> => {
    setIsLoading(true);
    setError(null);

    try {
      await invoke('delete_channel', { channelId });
      toast.success('渠道已删除');
      return true;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      toast.error(`删除渠道失败: ${message}`);
      return false;
    } finally {
      setIsLoading(false);
    }
  }, []);

  /**
   * Test channel connection
   */
  const testConnection = useCallback(async (channelId: string): Promise<boolean> => {
    setIsLoading(true);
    setError(null);

    try {
      const result = await invoke<boolean>('test_channel_connection', { channelId });

      if (result) {
        toast.success('连接测试成功');
      } else {
        toast.error('连接测试失败');
      }

      return result;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      toast.error(`连接测试失败: ${message}`);
      return false;
    } finally {
      setIsLoading(false);
    }
  }, []);

  /**
   * Get channel config
   */
  const getChannelConfig = useCallback(async (channelId: string): Promise<ChannelConfig | null> => {
    setIsLoading(true);
    setError(null);

    try {
      const config = await invoke<ChannelConfig>('get_channel_config', { channelId });
      return config;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      console.error('Failed to get channel config:', err);
      return null;
    } finally {
      setIsLoading(false);
    }
  }, []);

  /**
   * Save channel behavior config
   */
  const saveBehaviorConfig = useCallback(async (
    channelId: string,
    behavior: ChannelBehaviorConfig
  ): Promise<boolean> => {
    setIsLoading(true);
    setError(null);

    try {
      await invoke('save_channel_behavior', { channelId, behavior });
      toast.success('行为配置已保存');
      return true;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      toast.error(`保存行为配置失败: ${message}`);
      return false;
    } finally {
      setIsLoading(false);
    }
  }, []);

  return {
    isLoading,
    error,
    createChannel,
    updateChannel,
    deleteChannel,
    testConnection,
    getChannelConfig,
    saveBehaviorConfig,
  };
}

export default useChannelConfig;