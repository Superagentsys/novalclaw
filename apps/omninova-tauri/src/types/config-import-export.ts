/**
 * Config Import/Export Types
 *
 * Types for agent configuration import and export functionality.
 * [Source: Story 7.8 - 配置导入导出功能]
 */

import {
  type AgentStyleConfig,
  type ContextWindowConfig,
  type AgentTriggerConfig,
  type AgentPrivacyConfig,
} from './agent';
import { type AgentSkillConfig } from './skill';

/**
 * Export format
 */
export type ExportFormat = 'json' | 'yaml';

/**
 * Export format labels
 */
export const EXPORT_FORMAT_LABELS: Record<ExportFormat, string> = {
  json: 'JSON',
  yaml: 'YAML',
};

/**
 * Export options
 */
export interface ExportOptions {
  /** Export format */
  format: ExportFormat;
  /** Include skill configurations */
  includeSkills: boolean;
  /** Include conversation history */
  includeHistory: boolean;
  /** Include memory data */
  includeMemory: boolean;
}

/**
 * Default export options
 */
export const DEFAULT_EXPORT_OPTIONS: ExportOptions = {
  format: 'json',
  includeSkills: true,
  includeHistory: false,
  includeMemory: false,
};

/**
 * Import strategy
 */
export type ImportStrategy = 'overwrite' | 'merge';

/**
 * Import strategy labels
 */
export const IMPORT_STRATEGY_LABELS: Record<ImportStrategy, string> = {
  overwrite: '覆盖',
  merge: '合并',
};

/**
 * Import strategy descriptions
 */
export const IMPORT_STRATEGY_DESCRIPTIONS: Record<ImportStrategy, string> = {
  overwrite: '替换现有配置',
  merge: '保留现有配置，添加新配置',
};

/**
 * Import options
 */
export interface ImportOptions {
  /** Import strategy */
  strategy: ImportStrategy;
  /** Overwrite existing agents */
  overwriteExisting: boolean;
  /** Import skill configurations */
  importSkills: boolean;
  /** Import history data */
  importHistory: boolean;
}

/**
 * Default import options
 */
export const DEFAULT_IMPORT_OPTIONS: ImportOptions = {
  strategy: 'merge',
  overwriteExisting: false,
  importSkills: true,
  importHistory: false,
};

/**
 * Exported agent configuration
 */
export interface ExportedAgentConfig {
  /** Agent basic info */
  id: string;
  name: string;
  description?: string;
  domain?: string;
  mbtiType: string;
  /** Configuration */
  styleConfig: AgentStyleConfig;
  contextConfig: ContextWindowConfig;
  triggerConfig: AgentTriggerConfig;
  privacyConfig: AgentPrivacyConfig;
  skillConfig?: AgentSkillConfig;
}

/**
 * Agent configuration export package
 */
export interface AgentConfigExport {
  /** Export version */
  version: string;
  /** Export timestamp (ISO 8601) */
  exportedAt: string;
  /** Application version */
  appVersion: string;
  /** Agent configurations */
  agents: ExportedAgentConfig[];
  /** Global settings (optional) */
  globalSettings?: Record<string, unknown>;
}

/**
 * Current export version
 */
export const CURRENT_EXPORT_VERSION = '1.0.0';

/**
 * Import validation result
 */
export interface ImportValidationResult {
  /** Whether the file is valid */
  valid: boolean;
  /** List of errors */
  errors: string[];
  /** List of warnings */
  warnings: string[];
  /** Number of agents detected */
  agentCount: number;
  /** Version compatibility */
  versionCompatible: boolean;
  /** File format */
  format: ExportFormat;
}

/**
 * Import result
 */
export interface ImportResult {
  /** Whether import was successful */
  success: boolean;
  /** Number of imported agents */
  importedCount: number;
  /** Number of skipped agents */
  skippedCount: number;
  /** List of errors */
  errors: string[];
  /** List of imported agent names */
  importedAgents: string[];
}

/**
 * Create an empty import validation result
 */
export function createEmptyValidationResult(format: ExportFormat): ImportValidationResult {
  return {
    valid: false,
    errors: [],
    warnings: [],
    agentCount: 0,
    versionCompatible: false,
    format,
  };
}

/**
 * Check version compatibility
 */
export function checkVersionCompatibility(fileVersion: string): boolean {
  const [major] = fileVersion.split('.');
  const [currentMajor] = CURRENT_EXPORT_VERSION.split('.');

  // Major version must match
  return major === currentMajor;
}

/**
 * Detect file format from content
 */
export function detectFormat(content: string): ExportFormat | null {
  const trimmed = content.trim();

  // JSON starts with { or [
  if (trimmed.startsWith('{') || trimmed.startsWith('[')) {
    try {
      JSON.parse(trimmed);
      return 'json';
    } catch {
      // Not valid JSON
    }
  }

  // YAML detection: check for common YAML patterns
  if (trimmed.includes(':\n') || trimmed.includes(': ')) {
    // Could be YAML
    return 'yaml';
  }

  return null;
}

/**
 * Sensitive field names that should not be exported
 */
export const SENSITIVE_FIELDS = [
  'apiKey',
  'api_key',
  'apiSecret',
  'api_secret',
  'token',
  'accessToken',
  'access_token',
  'refreshToken',
  'refresh_token',
  'password',
  'secret',
  'credential',
] as const;

/**
 * Check if a field name is sensitive
 */
export function isSensitiveField(fieldName: string): boolean {
  const lowerName = fieldName.toLowerCase();
  return SENSITIVE_FIELDS.some(sensitive =>
    lowerName === sensitive.toLowerCase() ||
    lowerName.includes(sensitive.toLowerCase())
  );
}

/**
 * Filter sensitive data from an object
 */
export function filterSensitiveData<T extends Record<string, unknown>>(
  obj: T,
  depth: number = 0
): T {
  if (depth > 10) {
    // Prevent infinite recursion
    return obj;
  }

  const result: Record<string, unknown> = {};

  for (const [key, value] of Object.entries(obj)) {
    // Skip sensitive fields
    if (isSensitiveField(key)) {
      continue;
    }

    // Recursively filter nested objects
    if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
      result[key] = filterSensitiveData(value as Record<string, unknown>, depth + 1);
    } else if (Array.isArray(value)) {
      result[key] = value.map(item =>
        item !== null && typeof item === 'object'
          ? filterSensitiveData(item as Record<string, unknown>, depth + 1)
          : item
      );
    } else {
      result[key] = value;
    }
  }

  return result as T;
}