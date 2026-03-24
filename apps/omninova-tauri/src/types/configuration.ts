/**
 * Configuration Types
 *
 * Aggregated types for agent configuration management.
 * [Source: Story 7.7 - ConfigurationPanel 组件]
 */

import {
  type AgentStyleConfig,
  type ContextWindowConfig,
  type AgentTriggerConfig,
  type AgentPrivacyConfig,
  DEFAULT_STYLE_CONFIG,
  DEFAULT_CONTEXT_WINDOW_CONFIG,
  DEFAULT_TRIGGER_CONFIG,
  DEFAULT_PRIVACY_CONFIG,
} from './agent';
import { type AgentSkillConfig } from './skill';

/**
 * Agent configuration - aggregates all config types
 */
export interface AgentConfiguration {
  /** Agent ID */
  agentId: string;
  /** Response style configuration */
  styleConfig: AgentStyleConfig;
  /** Context window configuration */
  contextConfig: ContextWindowConfig;
  /** Trigger keywords configuration */
  triggerConfig: AgentTriggerConfig;
  /** Privacy configuration */
  privacyConfig: AgentPrivacyConfig;
  /** Skill configuration */
  skillConfig: AgentSkillConfig;
}

/**
 * Configuration change record
 */
export interface ConfigChange {
  /** Path to the changed field (dot notation) */
  path: string;
  /** Previous value */
  oldValue: unknown;
  /** New value */
  newValue: unknown;
}

/**
 * Configuration validation error
 */
export interface ConfigValidationError {
  /** Path to the invalid field */
  path: string;
  /** Error message */
  message: string;
}

/**
 * Configuration validation result
 */
export interface ConfigValidationResult {
  /** Whether configuration is valid */
  isValid: boolean;
  /** List of validation errors */
  errors: ConfigValidationError[];
}

/**
 * Default agent configuration
 */
export function createDefaultAgentConfiguration(agentId: string): AgentConfiguration {
  return {
    agentId,
    styleConfig: { ...DEFAULT_STYLE_CONFIG },
    contextConfig: { ...DEFAULT_CONTEXT_WINDOW_CONFIG },
    triggerConfig: { ...DEFAULT_TRIGGER_CONFIG },
    privacyConfig: { ...DEFAULT_PRIVACY_CONFIG, dataRetention: { ...DEFAULT_PRIVACY_CONFIG.dataRetention }, sensitiveFilter: { ...DEFAULT_PRIVACY_CONFIG.sensitiveFilter } },
    skillConfig: {
      agentId,
      enabledSkills: [],
      skillConfigs: {},
    },
  };
}

/**
 * Deep compare two configurations and return changes
 */
export function getConfigChanges(
  original: AgentConfiguration,
  current: AgentConfiguration
): ConfigChange[] {
  const changes: ConfigChange[] = [];

  // Compare style config
  compareObjects('styleConfig', original.styleConfig, current.styleConfig, changes);

  // Compare context config
  compareObjects('contextConfig', original.contextConfig, current.contextConfig, changes);

  // Compare trigger config
  compareObjects('triggerConfig', original.triggerConfig, current.triggerConfig, changes);

  // Compare privacy config
  compareObjects('privacyConfig', original.privacyConfig, current.privacyConfig, changes);

  // Compare skill config
  compareObjects('skillConfig', original.skillConfig, current.skillConfig, changes);

  return changes;
}

/**
 * Helper function to compare two objects recursively
 */
function compareObjects(
  prefix: string,
  original: Record<string, unknown>,
  current: Record<string, unknown>,
  changes: ConfigChange[]
): void {
  const allKeys = new Set([...Object.keys(original), ...Object.keys(current)]);

  for (const key of allKeys) {
    const path = `${prefix}.${key}`;
    const oldValue = original[key];
    const newValue = current[key];

    if (oldValue === undefined && newValue !== undefined) {
      changes.push({ path, oldValue: undefined, newValue });
    } else if (oldValue !== undefined && newValue === undefined) {
      changes.push({ path, oldValue, newValue: undefined });
    } else if (typeof oldValue === 'object' && oldValue !== null &&
               typeof newValue === 'object' && newValue !== null &&
               !Array.isArray(oldValue) && !Array.isArray(newValue)) {
      compareObjects(path, oldValue as Record<string, unknown>, newValue as Record<string, unknown>, changes);
    } else if (JSON.stringify(oldValue) !== JSON.stringify(newValue)) {
      changes.push({ path, oldValue, newValue });
    }
  }
}

/**
 * Validate agent configuration
 */
export function validateConfiguration(config: AgentConfiguration): ConfigValidationResult {
  const errors: ConfigValidationError[] = [];

  // Validate style config
  if (config.styleConfig.verbosity < 0 || config.styleConfig.verbosity > 1) {
    errors.push({
      path: 'styleConfig.verbosity',
      message: '详细程度必须在 0-1 之间',
    });
  }

  if (config.styleConfig.maxResponseLength < 0) {
    errors.push({
      path: 'styleConfig.maxResponseLength',
      message: '最大响应长度不能为负数',
    });
  }

  // Validate context config
  if (config.contextConfig.maxTokens < 0) {
    errors.push({
      path: 'contextConfig.maxTokens',
      message: '上下文窗口大小不能为负数',
    });
  }

  if (config.contextConfig.responseReserve < 0) {
    errors.push({
      path: 'contextConfig.responseReserve',
      message: '响应预留空间不能为负数',
    });
  }

  // Validate privacy config
  if (config.privacyConfig.dataRetention.episodicMemoryDays < 0) {
    errors.push({
      path: 'privacyConfig.dataRetention.episodicMemoryDays',
      message: '情景记忆保留天数不能为负数',
    });
  }

  if (config.privacyConfig.dataRetention.workingMemoryHours < 0) {
    errors.push({
      path: 'privacyConfig.dataRetention.workingMemoryHours',
      message: '工作记忆保留小时数不能为负数',
    });
  }

  return {
    isValid: errors.length === 0,
    errors,
  };
}