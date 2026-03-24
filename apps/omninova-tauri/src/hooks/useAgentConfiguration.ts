/**
 * useAgentConfiguration Hook
 *
 * Hook for managing agent configuration state with change tracking.
 * [Source: Story 7.7 - ConfigurationPanel 组件]
 */

import { useState, useCallback, useEffect, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import {
  type AgentConfiguration,
  type ConfigChange,
  type ConfigValidationResult,
  createDefaultAgentConfiguration,
  getConfigChanges,
  validateConfiguration,
} from '@/types/configuration';
import {
  type AgentStyleConfig,
  type ContextWindowConfig,
  type AgentTriggerConfig,
  type AgentPrivacyConfig,
} from '@/types/agent';
import { type AgentSkillConfig } from '@/types/skill';

export interface UseAgentConfigurationOptions {
  /** Agent ID */
  agentId: string;
  /** Initial configuration (if available) */
  initialConfig?: AgentConfiguration;
  /** Auto-save on change */
  autoSave?: boolean;
}

export interface UseAgentConfigurationReturn {
  /** Current configuration */
  config: AgentConfiguration;
  /** Original configuration (for comparison) */
  originalConfig: AgentConfiguration;
  /** Whether there are unsaved changes */
  isDirty: boolean;
  /** List of changes from original */
  changes: ConfigChange[];
  /** Whether configuration is valid */
  isValid: boolean;
  /** Validation result */
  validationResult: ConfigValidationResult;
  /** Whether currently loading */
  isLoading: boolean;
  /** Whether currently saving */
  isSaving: boolean;
  /** Error message */
  error: string | null;

  /** Update style config */
  setStyleConfig: (config: AgentStyleConfig) => void;
  /** Update context config */
  setContextConfig: (config: ContextWindowConfig) => void;
  /** Update trigger config */
  setTriggerConfig: (config: AgentTriggerConfig) => void;
  /** Update privacy config */
  setPrivacyConfig: (config: AgentPrivacyConfig) => void;
  /** Update skill config */
  setSkillConfig: (config: AgentSkillConfig) => void;

  /** Save configuration */
  save: () => Promise<boolean>;
  /** Cancel changes and revert to original */
  cancel: () => void;
  /** Reset to default configuration */
  resetToDefaults: () => void;
  /** Reload configuration from server */
  reload: () => Promise<void>;
}

/**
 * Hook for managing agent configuration
 */
export function useAgentConfiguration({
  agentId,
  initialConfig,
  autoSave = false,
}: UseAgentConfigurationOptions): UseAgentConfigurationReturn {
  // Create default config if not provided
  const defaultConfig = useMemo(
    () => initialConfig || createDefaultAgentConfiguration(agentId),
    [initialConfig, agentId]
  );

  // State
  const [config, setConfig] = useState<AgentConfiguration>(defaultConfig);
  const [originalConfig, setOriginalConfig] = useState<AgentConfiguration>(defaultConfig);
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Computed values
  const changes = useMemo(
    () => getConfigChanges(originalConfig, config),
    [originalConfig, config]
  );

  const isDirty = changes.length > 0;

  const validationResult = useMemo(
    () => validateConfiguration(config),
    [config]
  );

  const isValid = validationResult.isValid;

  // Load configuration from server if not provided
  useEffect(() => {
    if (!initialConfig && agentId) {
      reload();
    }
  }, [agentId]);

  // Update config when initialConfig changes
  useEffect(() => {
    if (initialConfig) {
      setConfig(initialConfig);
      setOriginalConfig(initialConfig);
    }
  }, [initialConfig]);

  // Auto-save on change
  useEffect(() => {
    if (autoSave && isDirty && isValid) {
      save();
    }
  }, [config, autoSave]);

  // Update handlers
  const setStyleConfig = useCallback((newStyleConfig: AgentStyleConfig) => {
    setConfig(prev => ({ ...prev, styleConfig: newStyleConfig }));
    setError(null);
  }, []);

  const setContextConfig = useCallback((newContextConfig: ContextWindowConfig) => {
    setConfig(prev => ({ ...prev, contextConfig: newContextConfig }));
    setError(null);
  }, []);

  const setTriggerConfig = useCallback((newTriggerConfig: AgentTriggerConfig) => {
    setConfig(prev => ({ ...prev, triggerConfig: newTriggerConfig }));
    setError(null);
  }, []);

  const setPrivacyConfig = useCallback((newPrivacyConfig: AgentPrivacyConfig) => {
    setConfig(prev => ({ ...prev, privacyConfig: newPrivacyConfig }));
    setError(null);
  }, []);

  const setSkillConfig = useCallback((newSkillConfig: AgentSkillConfig) => {
    setConfig(prev => ({ ...prev, skillConfig: newSkillConfig }));
    setError(null);
  }, []);

  // Save configuration
  const save = useCallback(async (): Promise<boolean> => {
    if (!isValid) {
      toast.error('配置验证失败，请检查输入');
      return false;
    }

    setIsSaving(true);
    setError(null);

    try {
      // Call Tauri command to save configuration
      await invoke('update_agent_configuration', {
        agentId,
        styleConfig: JSON.stringify(config.styleConfig),
        contextConfig: JSON.stringify(config.contextConfig),
        triggerConfig: JSON.stringify(config.triggerConfig),
        privacyConfig: JSON.stringify(config.privacyConfig),
      });

      // Update original config to reflect saved state
      setOriginalConfig(config);
      toast.success('配置已保存');
      return true;
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : '保存配置失败';
      setError(errorMessage);
      toast.error(errorMessage);
      return false;
    } finally {
      setIsSaving(false);
    }
  }, [agentId, config, isValid]);

  // Cancel changes
  const cancel = useCallback(() => {
    setConfig(originalConfig);
    setError(null);
    toast.info('已撤销更改');
  }, [originalConfig]);

  // Reset to defaults
  const resetToDefaults = useCallback(() => {
    const defaults = createDefaultAgentConfiguration(agentId);
    setConfig(defaults);
    setError(null);
  }, [agentId]);

  // Reload from server
  const reload = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      const result = await invoke<{
        style_config: string | null;
        context_window_config: string | null;
        trigger_keywords_config: string | null;
        privacy_config: string | null;
      }>('get_agent_configuration', { agentId });

      const loadedConfig: AgentConfiguration = {
        agentId,
        styleConfig: result.style_config
          ? JSON.parse(result.style_config)
          : createDefaultAgentConfiguration(agentId).styleConfig,
        contextConfig: result.context_window_config
          ? JSON.parse(result.context_window_config)
          : createDefaultAgentConfiguration(agentId).contextConfig,
        triggerConfig: result.trigger_keywords_config
          ? JSON.parse(result.trigger_keywords_config)
          : createDefaultAgentConfiguration(agentId).triggerConfig,
        privacyConfig: result.privacy_config
          ? JSON.parse(result.privacy_config)
          : createDefaultAgentConfiguration(agentId).privacyConfig,
        skillConfig: {
          agentId,
          enabledSkills: [],
          skillConfigs: {},
        },
      };

      setConfig(loadedConfig);
      setOriginalConfig(loadedConfig);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : '加载配置失败';
      setError(errorMessage);
      // Fall back to defaults
      const defaults = createDefaultAgentConfiguration(agentId);
      setConfig(defaults);
      setOriginalConfig(defaults);
    } finally {
      setIsLoading(false);
    }
  }, [agentId]);

  return {
    config,
    originalConfig,
    isDirty,
    changes,
    isValid,
    validationResult,
    isLoading,
    isSaving,
    error,

    setStyleConfig,
    setContextConfig,
    setTriggerConfig,
    setPrivacyConfig,
    setSkillConfig,

    save,
    cancel,
    resetToDefaults,
    reload,
  };
}

export default useAgentConfiguration;