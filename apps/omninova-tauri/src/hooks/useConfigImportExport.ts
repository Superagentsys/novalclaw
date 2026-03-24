/**
 * useConfigImportExport Hook
 *
 * Hook for agent configuration import and export functionality.
 * [Source: Story 7.8 - 配置导入导出功能]
 */

import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { save, open } from '@tauri-apps/plugin-dialog';
import { readFile, writeFile } from '@tauri-apps/plugin-fs';
import {
  type ExportOptions,
  type ImportOptions,
  type AgentConfigExport,
  type ImportValidationResult,
  type ImportResult,
  type ExportFormat,
  DEFAULT_EXPORT_OPTIONS,
  DEFAULT_IMPORT_OPTIONS,
  CURRENT_EXPORT_VERSION,
  checkVersionCompatibility,
  detectFormat,
  filterSensitiveData,
} from '@/types/config-import-export';
import type { ExportedAgentConfig } from '@/types/config-import-export';
import type { AgentModel } from '@/types/agent';
import type { AgentSkillConfig } from '@/types/skill';

/**
 * Get app version from package.json via Tauri or use default
 */
const APP_VERSION = '0.1.0'; // Fallback version

/**
 * Export state
 */
export interface ExportState {
  isExporting: boolean;
  error: string | null;
}

/**
 * Import state
 */
export interface ImportState {
  isValidating: boolean;
  isImporting: boolean;
  error: string | null;
  validationResult: ImportValidationResult | null;
}

/**
 * Hook return type
 */
export interface UseConfigImportExportReturn {
  /** Export state */
  exportState: ExportState;
  /** Import state */
  importState: ImportState;
  /** Export a single agent */
  exportAgent: (agentId: string, options?: Partial<ExportOptions>) => Promise<boolean>;
  /** Export all agents */
  exportAllAgents: (agents: AgentModel[], options?: Partial<ExportOptions>) => Promise<boolean>;
  /** Validate an import file */
  validateImportFile: (filePath: string) => Promise<ImportValidationResult | null>;
  /** Import configuration from file */
  importConfig: (filePath: string, options?: Partial<ImportOptions>) => Promise<ImportResult | null>;
  /** Reset export state */
  resetExportState: () => void;
  /** Reset import state */
  resetImportState: () => void;
}

/**
 * Hook for configuration import/export
 */
export function useConfigImportExport(): UseConfigImportExportReturn {
  const [exportState, setExportState] = useState<ExportState>({
    isExporting: false,
    error: null,
  });

  const [importState, setImportState] = useState<ImportState>({
    isValidating: false,
    isImporting: false,
    error: null,
    validationResult: null,
  });

  /**
   * Convert agent to export format
   */
  const agentToExportConfig = useCallback(
    async (agent: AgentModel, options: ExportOptions): Promise<ExportedAgentConfig> => {
      // Parse JSON configs from agent model
      const styleConfig = agent.style_config
        ? JSON.parse(agent.style_config)
        : {};
      const contextConfig = agent.context_window_config
        ? JSON.parse(agent.context_window_config)
        : {};
      const triggerConfig = agent.trigger_keywords_config
        ? JSON.parse(agent.trigger_keywords_config)
        : {};
      const privacyConfig = agent.privacy_config
        ? JSON.parse(agent.privacy_config)
        : {};

      // Fetch skill config if option is enabled
      // Note: skill_config is stored separately, need to fetch from backend
      let skillConfig: AgentSkillConfig | undefined;
      if (options.includeSkills) {
        try {
          skillConfig = await invoke<AgentSkillConfig>('get_agent_skill_config', {
            agentUuid: agent.agent_uuid,
          });
        } catch {
          // Skill config may not exist for all agents
          skillConfig = undefined;
        }
      }

      return filterSensitiveData({
        id: agent.agent_uuid,
        name: agent.name,
        description: agent.description,
        domain: agent.domain,
        mbtiType: agent.mbti_type || 'INTJ',
        styleConfig,
        contextConfig,
        triggerConfig,
        privacyConfig,
        skillConfig,
      });
    },
    []
  );

  /**
   * Generate export content
   */
  const generateExportContent = useCallback(
    (
      agents: ExportedAgentConfig[],
      options: ExportOptions,
      format: ExportFormat
    ): string => {
      const exportData: AgentConfigExport = {
        version: CURRENT_EXPORT_VERSION,
        exportedAt: new Date().toISOString(),
        appVersion: APP_VERSION,
        agents,
        // Include metadata about export options
        globalSettings: {
          includeSkills: options.includeSkills,
          includeHistory: options.includeHistory,
          includeMemory: options.includeMemory,
        },
      };

      if (format === 'json') {
        return JSON.stringify(exportData, null, 2);
      } else {
        // YAML format - simple implementation
        return objectToYaml(exportData);
      }
    },
    []
  );

  /**
   * Export a single agent
   */
  const exportAgent = useCallback(
    async (agentId: string, options?: Partial<ExportOptions>): Promise<boolean> => {
      const opts = { ...DEFAULT_EXPORT_OPTIONS, ...options };
      setExportState({ isExporting: true, error: null });

      try {
        // Get agent data from backend
        const agent = await invoke<AgentModel>('get_agent_by_uuid', { agentUuid: agentId });
        if (!agent) {
          throw new Error('代理不存在');
        }

        const exportConfig = await agentToExportConfig(agent, opts);
        const content = generateExportContent([exportConfig], opts, opts.format);

        // Open save dialog
        const filePath = await save({
          defaultPath: `agent-${agent.name}-${new Date().toISOString().split('T')[0]}.${opts.format}`,
          filters: [
            {
              name: opts.format.toUpperCase(),
              extensions: [opts.format],
            },
            {
              name: 'All Files',
              extensions: ['*'],
            },
          ],
        });

        if (!filePath) {
          setExportState({ isExporting: false, error: null });
          return false;
        }

        // Write file
        const encoder = new TextEncoder();
        await writeFile(filePath, encoder.encode(content));

        setExportState({ isExporting: false, error: null });
        return true;
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        setExportState({ isExporting: false, error: errorMessage });
        return false;
      }
    },
    [agentToExportConfig, generateExportContent]
  );

  /**
   * Export all agents
   */
  const exportAllAgents = useCallback(
    async (agents: AgentModel[], options?: Partial<ExportOptions>): Promise<boolean> => {
      const opts = { ...DEFAULT_EXPORT_OPTIONS, ...options };
      setExportState({ isExporting: true, error: null });

      try {
        const exportConfigs = await Promise.all(
          agents.map(agent => agentToExportConfig(agent, opts))
        );
        const content = generateExportContent(exportConfigs, opts, opts.format);

        // Open save dialog
        const filePath = await save({
          defaultPath: `agents-export-${new Date().toISOString().split('T')[0]}.${opts.format}`,
          filters: [
            {
              name: opts.format.toUpperCase(),
              extensions: [opts.format],
            },
            {
              name: 'All Files',
              extensions: ['*'],
            },
          ],
        });

        if (!filePath) {
          setExportState({ isExporting: false, error: null });
          return false;
        }

        // Write file
        const encoder = new TextEncoder();
        await writeFile(filePath, encoder.encode(content));

        setExportState({ isExporting: false, error: null });
        return true;
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        setExportState({ isExporting: false, error: errorMessage });
        return false;
      }
    },
    [agentToExportConfig, generateExportContent]
  );

  /**
   * Validate import file
   */
  const validateImportFile = useCallback(
    async (filePath: string): Promise<ImportValidationResult | null> => {
      setImportState(prev => ({ ...prev, isValidating: true, error: null }));

      try {
        // Read file content
        const contents = await readFile(filePath);
        const decoder = new TextDecoder('utf-8');
        const content = decoder.decode(contents);

        // Detect format
        const format = detectFormat(content);
        if (!format) {
          const result: ImportValidationResult = {
            valid: false,
            errors: ['无法识别文件格式，请使用 JSON 或 YAML 格式'],
            warnings: [],
            agentCount: 0,
            versionCompatible: false,
            format: 'json',
          };
          setImportState(prev => ({
            ...prev,
            isValidating: false,
            validationResult: result,
          }));
          return result;
        }

        // Parse content
        let exportData: AgentConfigExport;
        if (format === 'json') {
          exportData = JSON.parse(content) as AgentConfigExport;
        } else {
          exportData = parseYaml(content);
        }

        // Validate structure
        const errors: string[] = [];
        const warnings: string[] = [];

        if (!exportData.version) {
          errors.push('缺少版本信息');
        }

        if (!exportData.agents || !Array.isArray(exportData.agents)) {
          errors.push('缺少代理配置列表');
        } else if (exportData.agents.length === 0) {
          warnings.push('文件中没有代理配置');
        }

        // Check version compatibility
        const versionCompatible = exportData.version
          ? checkVersionCompatibility(exportData.version)
          : false;

        if (!versionCompatible) {
          warnings.push(
            `文件版本 (${exportData.version || '未知'}) 可能与当前版本 (${CURRENT_EXPORT_VERSION}) 不兼容`
          );
        }

        // Validate each agent config
        for (let i = 0; i < (exportData.agents?.length || 0); i++) {
          const agent = exportData.agents[i];
          if (!agent.name) {
            errors.push(`代理 #${i + 1}: 缺少名称`);
          }
          if (!agent.id && !agent.name) {
            errors.push(`代理 #${i + 1}: 缺少ID或名称`);
          }
        }

        const result: ImportValidationResult = {
          valid: errors.length === 0,
          errors,
          warnings,
          agentCount: exportData.agents?.length || 0,
          versionCompatible,
          format,
        };

        setImportState(prev => ({
          ...prev,
          isValidating: false,
          validationResult: result,
        }));

        return result;
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        const result: ImportValidationResult = {
          valid: false,
          errors: [`解析文件失败: ${errorMessage}`],
          warnings: [],
          agentCount: 0,
          versionCompatible: false,
          format: 'json',
        };

        setImportState(prev => ({
          ...prev,
          isValidating: false,
          error: errorMessage,
          validationResult: result,
        }));

        return result;
      }
    },
    []
  );

  /**
   * Import configuration from file
   */
  const importConfig = useCallback(
    async (
      filePath: string,
      options?: Partial<ImportOptions>
    ): Promise<ImportResult | null> => {
      const opts = { ...DEFAULT_IMPORT_OPTIONS, ...options };
      setImportState(prev => ({ ...prev, isImporting: true, error: null }));

      try {
        // Read and parse file
        const contents = await readFile(filePath);
        const decoder = new TextDecoder('utf-8');
        const content = decoder.decode(contents);

        const format = detectFormat(content);
        if (!format) {
          throw new Error('无法识别文件格式');
        }

        let exportData: AgentConfigExport;
        if (format === 'json') {
          exportData = JSON.parse(content) as AgentConfigExport;
        } else {
          exportData = parseYaml(content);
        }

        // Import agents
        const errors: string[] = [];
        const importedAgents: string[] = [];
        let importedCount = 0;
        let skippedCount = 0;

        for (const agentConfig of exportData.agents) {
          try {
            // Create agent via backend
            await invoke('import_agent_from_config', {
              config: JSON.stringify(agentConfig),
              overwrite: opts.overwriteExisting,
              importSkills: opts.importSkills,
            });
            importedCount++;
            importedAgents.push(agentConfig.name);
          } catch (error) {
            const errorMsg = error instanceof Error ? error.message : String(error);
            if (errorMsg.includes('已存在') && !opts.overwriteExisting) {
              skippedCount++;
              errors.push(`跳过 "${agentConfig.name}": 已存在`);
            } else {
              errors.push(`导入 "${agentConfig.name}" 失败: ${errorMsg}`);
            }
          }
        }

        const result: ImportResult = {
          success: importedCount > 0,
          importedCount,
          skippedCount,
          errors,
          importedAgents,
        };

        setImportState(prev => ({
          ...prev,
          isImporting: false,
        }));

        return result;
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        setImportState(prev => ({
          ...prev,
          isImporting: false,
          error: errorMessage,
        }));
        return null;
      }
    },
    []
  );

  const resetExportState = useCallback(() => {
    setExportState({ isExporting: false, error: null });
  }, []);

  const resetImportState = useCallback(() => {
    setImportState({
      isValidating: false,
      isImporting: false,
      error: null,
      validationResult: null,
    });
  }, []);

  return {
    exportState,
    importState,
    exportAgent,
    exportAllAgents,
    validateImportFile,
    importConfig,
    resetExportState,
    resetImportState,
  };
}

/**
 * Simple YAML to object parser
 */
function parseYaml(content: string): AgentConfigExport {
  // This is a simple YAML parser for our specific use case
  // For production, consider using a proper YAML library
  const lines = content.split('\n');
  const result: Record<string, unknown> = {};
  let currentKey = '';
  let currentArray: unknown[] | null = null;
  let currentObj: Record<string, unknown> | null = null;
  let agents: Record<string, unknown>[] = [];

  for (const line of lines) {
    const trimmed = line.trimEnd();
    if (!trimmed || trimmed.startsWith('#')) continue;

    const indent = line.length - trimmed.length;
    const colonIndex = trimmed.indexOf(':');

    if (colonIndex === -1) continue;

    const key = trimmed.substring(0, colonIndex).trim();
    const value = trimmed.substring(colonIndex + 1).trim();

    if (indent === 0) {
      if (key === 'agents') {
        currentArray = agents;
        currentKey = 'agents';
      } else if (value) {
        result[key] = parseYamlValue(value);
      }
    } else if (currentArray && indent === 2 && key === '-') {
      // New agent in array
      currentObj = {};
      agents.push(currentObj);
    } else if (currentObj && indent >= 4) {
      // Property of current agent
      if (value) {
        currentObj[key] = parseYamlValue(value);
      }
    }
  }

  result.agents = agents;
  return result as unknown as AgentConfigExport;
}

/**
 * Parse a YAML value
 */
function parseYamlValue(value: string): unknown {
  if (value === 'true') return true;
  if (value === 'false') return false;
  if (value === 'null') return null;
  if (/^-?\d+$/.test(value)) return parseInt(value, 10);
  if (/^-?\d+\.\d+$/.test(value)) return parseFloat(value);
  if (value.startsWith('"') && value.endsWith('"')) {
    return value.slice(1, -1);
  }
  if (value.startsWith("'") && value.endsWith("'")) {
    return value.slice(1, -1);
  }
  return value;
}

/**
 * Simple object to YAML converter
 */
function objectToYaml(obj: unknown, indent: number = 0): string {
  if (obj === null || obj === undefined) {
    return 'null';
  }

  if (typeof obj !== 'object') {
    if (typeof obj === 'string') {
      return `"${obj.replace(/"/g, '\\"')}"`;
    }
    return String(obj);
  }

  if (Array.isArray(obj)) {
    if (obj.length === 0) return '[]';
    const indentStr = '  '.repeat(indent);
    return obj
      .map(item => `${indentStr}- ${objectToYaml(item, indent + 1)}`)
      .join('\n');
  }

  const entries = Object.entries(obj as Record<string, unknown>);
  if (entries.length === 0) return '{}';

  const indentStr = '  '.repeat(indent);
  return entries
    .map(([key, value]) => {
      if (value === null || value === undefined) {
        return `${indentStr}${key}: null`;
      }
      if (typeof value === 'object') {
        const nested = objectToYaml(value, indent + 1);
        return `${indentStr}${key}:\n${nested}`;
      }
      return `${indentStr}${key}: ${objectToYaml(value, indent + 1)}`;
    })
    .join('\n');
}