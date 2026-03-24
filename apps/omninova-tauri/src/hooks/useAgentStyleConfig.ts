/**
 * useAgentStyleConfig Hook
 *
 * Manages agent style configuration state and operations.
 *
 * [Source: Story 7.1 - 代理响应风格配置]
 */

import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  type AgentStyleConfig,
  DEFAULT_STYLE_CONFIG,
} from '@/types/agent';

interface UseAgentStyleConfigResult {
  /** Current style config */
  config: AgentStyleConfig;
  /** Loading state */
  isLoading: boolean;
  /** Error message */
  error: string | null;
  /** Update style config */
  updateConfig: (newConfig: Partial<AgentStyleConfig>) => Promise<void>;
  /** Reset to defaults */
  resetToDefaults: () => Promise<void>;
  /** Refresh config from server */
  refresh: () => Promise<void>;
  /** Preview style effect */
  previewEffect: (sampleText: string) => Promise<string>;
}

/**
 * Hook for managing agent style configuration
 */
export function useAgentStyleConfig(agentUuid: string | null): UseAgentStyleConfigResult {
  const [config, setConfig] = useState<AgentStyleConfig>(DEFAULT_STYLE_CONFIG);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Fetch style config from server
  const refresh = useCallback(async () => {
    if (!agentUuid) {
      setConfig(DEFAULT_STYLE_CONFIG);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const configJson = await invoke<string>('get_agent_style_config', {
        uuid: agentUuid,
      });
      const parsedConfig = JSON.parse(configJson) as AgentStyleConfig;
      setConfig(parsedConfig);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(`获取风格配置失败: ${message}`);
    } finally {
      setIsLoading(false);
    }
  }, [agentUuid]);

  // Update style config
  const updateConfig = useCallback(async (newConfig: Partial<AgentStyleConfig>) => {
    if (!agentUuid) {
      setError('No agent selected');
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const mergedConfig = { ...config, ...newConfig };
      const configJson = JSON.stringify(mergedConfig);

      await invoke<string>('update_agent_style_config', {
        uuid: agentUuid,
        styleConfigJson: configJson,
      });

      setConfig(mergedConfig);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(`更新风格配置失败: ${message}`);
    } finally {
      setIsLoading(false);
    }
  }, [agentUuid, config]);

  // Reset to defaults
  const resetToDefaults = useCallback(async () => {
    await updateConfig(DEFAULT_STYLE_CONFIG);
  }, [updateConfig]);

  // Preview style effect
  const previewEffect = useCallback(async (sampleText: string): Promise<string> => {
    try {
      const configJson = JSON.stringify(config);
      const result = await invoke<string>('preview_style_effect', {
        styleConfigJson: configJson,
        sampleText,
      });
      return result;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      throw new Error(`预览失败: ${message}`);
    }
  }, [config]);

  // Load config when agent changes
  useEffect(() => {
    if (agentUuid) {
      refresh();
    } else {
      setConfig(DEFAULT_STYLE_CONFIG);
    }
  }, [agentUuid, refresh]);

  return {
    config,
    isLoading,
    error,
    updateConfig,
    resetToDefaults,
    refresh,
    previewEffect,
  };
}

export default useAgentStyleConfig;