/**
 * useContextWindowConfig Hook
 *
 * Manages agent context window configuration state and operations.
 *
 * [Source: Story 7.2 - 上下文窗口配置]
 */

import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  type ContextWindowConfig,
  DEFAULT_CONTEXT_WINDOW_CONFIG,
} from '@/types/agent';

interface UseContextWindowConfigResult {
  /** Current context window config */
  config: ContextWindowConfig;
  /** Loading state */
  isLoading: boolean;
  /** Error message */
  error: string | null;
  /** Update context window config */
  updateConfig: (newConfig: Partial<ContextWindowConfig>) => Promise<void>;
  /** Reset to defaults */
  resetToDefaults: () => Promise<void>;
  /** Refresh config from server */
  refresh: () => Promise<void>;
  /** Estimate tokens for text */
  estimateTokens: (text: string) => Promise<number>;
  /** Get model context recommendations */
  getModelRecommendations: (modelName: string) => Promise<{ recommended: number; max: number } | null>;
  /** Effective message tokens (max - reserve) */
  effectiveMessageTokens: number;
}

/**
 * Hook for managing agent context window configuration
 */
export function useContextWindowConfig(agentUuid: string | null): UseContextWindowConfigResult {
  const [config, setConfig] = useState<ContextWindowConfig>(DEFAULT_CONTEXT_WINDOW_CONFIG);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Calculate effective message tokens
  const effectiveMessageTokens = config.maxTokens > config.responseReserve
    ? config.maxTokens - config.responseReserve
    : 0;

  // Fetch context window config from server
  const refresh = useCallback(async () => {
    if (!agentUuid) {
      setConfig(DEFAULT_CONTEXT_WINDOW_CONFIG);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const configJson = await invoke<string>('get_context_window_config', {
        uuid: agentUuid,
      });
      const parsedConfig = JSON.parse(configJson) as ContextWindowConfig;
      setConfig(parsedConfig);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(`获取上下文窗口配置失败: ${message}`);
    } finally {
      setIsLoading(false);
    }
  }, [agentUuid]);

  // Update context window config
  const updateConfig = useCallback(async (newConfig: Partial<ContextWindowConfig>) => {
    if (!agentUuid) {
      setError('No agent selected');
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const mergedConfig = { ...config, ...newConfig };
      const configJson = JSON.stringify(mergedConfig);

      await invoke<string>('update_context_window_config', {
        uuid: agentUuid,
        configJson,
      });

      setConfig(mergedConfig);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(`更新上下文窗口配置失败: ${message}`);
    } finally {
      setIsLoading(false);
    }
  }, [agentUuid, config]);

  // Reset to defaults
  const resetToDefaults = useCallback(async () => {
    await updateConfig(DEFAULT_CONTEXT_WINDOW_CONFIG);
  }, [updateConfig]);

  // Estimate tokens for text
  const estimateTokens = useCallback(async (text: string): Promise<number> => {
    try {
      const count = await invoke<number>('estimate_tokens', { text });
      return count;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      throw new Error(`Token 估算失败: ${message}`);
    }
  }, []);

  // Get model context recommendations
  const getModelRecommendations = useCallback(async (
    modelName: string
  ): Promise<{ recommended: number; max: number } | null> => {
    try {
      const result = await invoke<[number, number] | null>('get_model_context_recommendations', {
        modelName,
      });
      if (result) {
        return { recommended: result[0], max: result[1] };
      }
      return null;
    } catch (err) {
      console.error('Failed to get model recommendations:', err);
      return null;
    }
  }, []);

  // Load config when agent changes
  useEffect(() => {
    if (agentUuid) {
      refresh();
    } else {
      setConfig(DEFAULT_CONTEXT_WINDOW_CONFIG);
    }
  }, [agentUuid, refresh]);

  return {
    config,
    isLoading,
    error,
    updateConfig,
    resetToDefaults,
    refresh,
    estimateTokens,
    getModelRecommendations,
    effectiveMessageTokens,
  };
}

export default useContextWindowConfig;