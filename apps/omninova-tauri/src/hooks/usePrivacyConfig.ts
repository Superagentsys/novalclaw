/**
 * usePrivacyConfig Hook
 *
 * Manages agent privacy configuration state and operations.
 *
 * [Source: Story 7.4 - 数据处理与隐私设置]
 */

import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  type AgentPrivacyConfig,
  type SensitiveDataFilter,
  type ExclusionRule,
  type DataRetentionPolicy,
  type MemorySharingScope,
  DEFAULT_PRIVACY_CONFIG,
  DEFAULT_SENSITIVE_DATA_FILTER,
  DEFAULT_DATA_RETENTION_POLICY,
} from '@/types/agent';

interface UsePrivacyConfigResult {
  /** Current privacy config */
  config: AgentPrivacyConfig;
  /** Loading state */
  isLoading: boolean;
  /** Error message */
  error: string | null;
  /** Update privacy config */
  updateConfig: (newConfig: Partial<AgentPrivacyConfig>) => Promise<void>;
  /** Update data retention policy */
  updateDataRetention: (policy: Partial<DataRetentionPolicy>) => Promise<void>;
  /** Update sensitive data filter */
  updateSensitiveFilter: (filter: Partial<SensitiveDataFilter>) => Promise<void>;
  /** Update memory sharing scope */
  updateMemorySharingScope: (scope: MemorySharingScope) => Promise<void>;
  /** Add exclusion rule */
  addExclusionRule: (rule: ExclusionRule) => Promise<void>;
  /** Remove exclusion rule by index */
  removeExclusionRule: (index: number) => Promise<void>;
  /** Toggle sensitive filter enabled */
  toggleSensitiveFilterEnabled: () => Promise<void>;
  /** Reset to defaults */
  resetToDefaults: () => Promise<void>;
  /** Refresh config from server */
  refresh: () => Promise<void>;
  /** Test sensitive filter against sample content */
  testSensitiveFilter: (content: string) => Promise<string>;
  /** Validate exclusion rule pattern */
  validateExclusionPattern: (pattern: string) => Promise<boolean>;
  /** Validate custom filter pattern */
  validateFilterPattern: (pattern: string) => Promise<boolean>;
}

/**
 * Hook for managing agent privacy configuration
 */
export function usePrivacyConfig(agentUuid: string | null): UsePrivacyConfigResult {
  const [config, setConfig] = useState<AgentPrivacyConfig>(DEFAULT_PRIVACY_CONFIG);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Fetch privacy config from server
  const refresh = useCallback(async () => {
    if (!agentUuid) {
      setConfig(DEFAULT_PRIVACY_CONFIG);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const configJson = await invoke<string>('get_privacy_config', {
        uuid: agentUuid,
      });
      const parsedConfig = JSON.parse(configJson) as AgentPrivacyConfig;
      setConfig(parsedConfig);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(`获取隐私配置失败: ${message}`);
    } finally {
      setIsLoading(false);
    }
  }, [agentUuid]);

  // Update privacy config
  const updateConfig = useCallback(async (newConfig: Partial<AgentPrivacyConfig>) => {
    if (!agentUuid) {
      setError('No agent selected');
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const mergedConfig = { ...config, ...newConfig };
      const configJson = JSON.stringify(mergedConfig);

      await invoke<string>('update_privacy_config', {
        uuid: agentUuid,
        configJson,
      });

      setConfig(mergedConfig);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(`更新隐私配置失败: ${message}`);
    } finally {
      setIsLoading(false);
    }
  }, [agentUuid, config]);

  // Update data retention policy
  const updateDataRetention = useCallback(async (policy: Partial<DataRetentionPolicy>) => {
    const newPolicy = { ...config.dataRetention, ...policy };
    await updateConfig({ dataRetention: newPolicy });
  }, [config.dataRetention, updateConfig]);

  // Update sensitive data filter
  const updateSensitiveFilter = useCallback(async (filter: Partial<SensitiveDataFilter>) => {
    const newFilter = { ...config.sensitiveFilter, ...filter };
    await updateConfig({ sensitiveFilter: newFilter });
  }, [config.sensitiveFilter, updateConfig]);

  // Update memory sharing scope
  const updateMemorySharingScope = useCallback(async (scope: MemorySharingScope) => {
    await updateConfig({ memorySharingScope: scope });
  }, [updateConfig]);

  // Add exclusion rule
  const addExclusionRule = useCallback(async (rule: ExclusionRule) => {
    const newRules = [...config.exclusionRules, rule];
    await updateConfig({ exclusionRules: newRules });
  }, [config.exclusionRules, updateConfig]);

  // Remove exclusion rule by index
  const removeExclusionRule = useCallback(async (index: number) => {
    const newRules = config.exclusionRules.filter((_, i) => i !== index);
    await updateConfig({ exclusionRules: newRules });
  }, [config.exclusionRules, updateConfig]);

  // Toggle sensitive filter enabled
  const toggleSensitiveFilterEnabled = useCallback(async () => {
    await updateSensitiveFilter({ enabled: !config.sensitiveFilter.enabled });
  }, [config.sensitiveFilter.enabled, updateSensitiveFilter]);

  // Reset to defaults
  const resetToDefaults = useCallback(async () => {
    await updateConfig(DEFAULT_PRIVACY_CONFIG);
  }, [updateConfig]);

  // Test sensitive filter against sample content
  const testSensitiveFilter = useCallback(async (content: string): Promise<string> => {
    try {
      const filterJson = JSON.stringify(config.sensitiveFilter);
      const result = await invoke<string>('test_sensitive_filter', {
        filterJson,
        content,
      });
      return result;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      throw new Error(`测试敏感信息过滤失败: ${message}`);
    }
  }, [config.sensitiveFilter]);

  // Validate exclusion rule pattern
  const validateExclusionPattern = useCallback(async (pattern: string): Promise<boolean> => {
    try {
      await invoke<void>('validate_exclusion_pattern', { pattern });
      return true;
    } catch {
      return false;
    }
  }, []);

  // Validate custom filter pattern
  const validateFilterPattern = useCallback(async (pattern: string): Promise<boolean> => {
    try {
      await invoke<void>('validate_filter_pattern', { pattern });
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
      setConfig(DEFAULT_PRIVACY_CONFIG);
    }
  }, [agentUuid, refresh]);

  return {
    config,
    isLoading,
    error,
    updateConfig,
    updateDataRetention,
    updateSensitiveFilter,
    updateMemorySharingScope,
    addExclusionRule,
    removeExclusionRule,
    toggleSensitiveFilterEnabled,
    resetToDefaults,
    refresh,
    testSensitiveFilter,
    validateExclusionPattern,
    validateFilterPattern,
  };
}

export default usePrivacyConfig;