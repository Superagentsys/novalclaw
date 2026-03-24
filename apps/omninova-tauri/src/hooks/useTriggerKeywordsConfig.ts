/**
 * useTriggerKeywordsConfig Hook
 *
 * Manages agent trigger keywords configuration state and operations.
 *
 * [Source: Story 7.3 - 触发关键词配置]
 */

import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  type AgentTriggerConfig,
  type TriggerKeyword,
  type TriggerTestResult,
  DEFAULT_TRIGGER_CONFIG,
} from '@/types/agent';

interface UseTriggerKeywordsConfigResult {
  /** Current trigger keywords config */
  config: AgentTriggerConfig;
  /** Loading state */
  isLoading: boolean;
  /** Error message */
  error: string | null;
  /** Update trigger keywords config */
  updateConfig: (newConfig: Partial<AgentTriggerConfig>) => Promise<void>;
  /** Add a trigger keyword */
  addKeyword: (keyword: TriggerKeyword) => Promise<void>;
  /** Remove a trigger keyword by index */
  removeKeyword: (index: number) => Promise<void>;
  /** Toggle enabled state */
  toggleEnabled: () => Promise<void>;
  /** Reset to defaults */
  resetToDefaults: () => Promise<void>;
  /** Refresh config from server */
  refresh: () => Promise<void>;
  /** Test trigger keywords against sample text */
  testTriggers: (testText: string) => Promise<TriggerTestResult>;
  /** Test a single trigger keyword */
  testSingleTrigger: (keyword: TriggerKeyword, testText: string) => Promise<TriggerTestResult>;
  /** Validate a trigger keyword */
  validateKeyword: (keyword: TriggerKeyword) => Promise<boolean>;
}

/**
 * Hook for managing agent trigger keywords configuration
 */
export function useTriggerKeywordsConfig(agentUuid: string | null): UseTriggerKeywordsConfigResult {
  const [config, setConfig] = useState<AgentTriggerConfig>(DEFAULT_TRIGGER_CONFIG);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Fetch trigger keywords config from server
  const refresh = useCallback(async () => {
    if (!agentUuid) {
      setConfig(DEFAULT_TRIGGER_CONFIG);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const configJson = await invoke<string>('get_trigger_keywords_config', {
        uuid: agentUuid,
      });
      const parsedConfig = JSON.parse(configJson) as AgentTriggerConfig;
      setConfig(parsedConfig);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(`获取触发关键词配置失败: ${message}`);
    } finally {
      setIsLoading(false);
    }
  }, [agentUuid]);

  // Update trigger keywords config
  const updateConfig = useCallback(async (newConfig: Partial<AgentTriggerConfig>) => {
    if (!agentUuid) {
      setError('No agent selected');
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const mergedConfig = { ...config, ...newConfig };
      const configJson = JSON.stringify(mergedConfig);

      await invoke<string>('update_trigger_keywords_config', {
        uuid: agentUuid,
        configJson,
      });

      setConfig(mergedConfig);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(`更新触发关键词配置失败: ${message}`);
    } finally {
      setIsLoading(false);
    }
  }, [agentUuid, config]);

  // Add a trigger keyword
  const addKeyword = useCallback(async (keyword: TriggerKeyword) => {
    const newKeywords = [...config.keywords, keyword];
    await updateConfig({ keywords: newKeywords });
  }, [config.keywords, updateConfig]);

  // Remove a trigger keyword by index
  const removeKeyword = useCallback(async (index: number) => {
    const newKeywords = config.keywords.filter((_, i) => i !== index);
    await updateConfig({ keywords: newKeywords });
  }, [config.keywords, updateConfig]);

  // Toggle enabled state
  const toggleEnabled = useCallback(async () => {
    await updateConfig({ enabled: !config.enabled });
  }, [config.enabled, updateConfig]);

  // Reset to defaults
  const resetToDefaults = useCallback(async () => {
    await updateConfig(DEFAULT_TRIGGER_CONFIG);
  }, [updateConfig]);

  // Test trigger keywords against sample text
  const testTriggers = useCallback(async (testText: string): Promise<TriggerTestResult> => {
    try {
      const configJson = JSON.stringify(config);
      const resultJson = await invoke<string>('test_trigger_match', {
        configJson,
        testText,
      });
      return JSON.parse(resultJson) as TriggerTestResult;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      throw new Error(`测试触发词失败: ${message}`);
    }
  }, [config]);

  // Test a single trigger keyword
  const testSingleTrigger = useCallback(async (
    keyword: TriggerKeyword,
    testText: string
  ): Promise<TriggerTestResult> => {
    try {
      const keywordJson = JSON.stringify(keyword);
      const resultJson = await invoke<string>('test_single_trigger', {
        keywordJson,
        testText,
      });
      return JSON.parse(resultJson) as TriggerTestResult;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      throw new Error(`测试触发词失败: ${message}`);
    }
  }, []);

  // Validate a trigger keyword
  const validateKeyword = useCallback(async (keyword: TriggerKeyword): Promise<boolean> => {
    try {
      const keywordJson = JSON.stringify(keyword);
      await invoke<void>('validate_trigger_keyword', { keywordJson });
      return true;
    } catch {
      return false;
    }
  }, []);

  // Load config when agent changes
  useEffect(() => {
    if (agentUuid) {
      refresh();
    } else {
      setConfig(DEFAULT_TRIGGER_CONFIG);
    }
  }, [agentUuid, refresh]);

  return {
    config,
    isLoading,
    error,
    updateConfig,
    addKeyword,
    removeKeyword,
    toggleEnabled,
    resetToDefaults,
    refresh,
    testTriggers,
    testSingleTrigger,
    validateKeyword,
  };
}

export default useTriggerKeywordsConfig;