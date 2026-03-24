/**
 * Skill management hook for OmniNova Claw
 *
 * Provides skill listing, execution, and management functionality
 *
 * [Source: Story 7.5 - 技能系统框架]
 * [Source: Story 7.6 - 技能管理界面]
 */

import { useState, useCallback, useEffect } from 'react';
import { toast } from 'sonner';
import { invoke } from '@tauri-apps/api/core';
import {
  type SkillMetadata,
  type SkillResult,
  type SkillExecutionRequest,
  type ExecutionLog,
  type CacheStats,
  type AgentSkillConfig,
  type SkillUsageStatistics,
  DEFAULT_SKILL_TAGS,
  type SkillTag,
} from '@/types/skill';

/**
 * Hook return type
 */
export interface UseSkillsReturn {
  /** List of available skills */
  skills: SkillMetadata[];
  /** Available skill tags */
  tags: SkillTag[];
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: string | null;
  /** Refresh skill list */
  refresh: () => Promise<void>;
  /** Get skill info by ID */
  getSkillInfo: (skillId: string) => Promise<SkillMetadata | null>;
  /** Execute a skill */
  executeSkill: (request: SkillExecutionRequest) => Promise<SkillResult | null>;
  /** Validate skill configuration */
  validateConfig: (skillId: string, config: Record<string, unknown>) => Promise<boolean>;
  /** Register a custom skill from YAML */
  registerCustomSkill: (yaml: string) => Promise<SkillMetadata | null>;
  /** List skills by tag */
  listByTag: (tag: string) => Promise<SkillMetadata[]>;
  /** Initialize the skill registry */
  initRegistry: () => Promise<boolean>;
}

/**
 * Parse skill metadata from JSON string
 */
function parseSkillMetadata(json: string): SkillMetadata {
  return JSON.parse(json) as SkillMetadata;
}

/**
 * Parse skill result from JSON string
 */
function parseSkillResult(json: string): SkillResult {
  const raw = JSON.parse(json);
  return {
    success: raw.success,
    content: raw.content,
    data: raw.data,
    error: raw.error,
    durationMs: raw.durationMs ?? raw.duration_ms ?? 0,
    metadata: raw.metadata ?? {},
  };
}

/**
 * Hook for skill management
 */
export function useSkills(): UseSkillsReturn {
  const [skills, setSkills] = useState<SkillMetadata[]>([]);
  const [tags, setTags] = useState<SkillTag[]>([...DEFAULT_SKILL_TAGS]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isInitialized, setIsInitialized] = useState(false);

  /**
   * Initialize the skill registry
   */
  const initRegistry = useCallback(async (): Promise<boolean> => {
    try {
      await invoke('init_skill_registry');
      setIsInitialized(true);
      return true;
    } catch (err) {
      console.error('Failed to initialize skill registry:', err);
      setError(`Failed to initialize skill registry: ${err}`);
      return false;
    }
  }, []);

  /**
   * Refresh the skill list
   */
  const refresh = useCallback(async () => {
    if (!isInitialized) {
      const initialized = await initRegistry();
      if (!initialized) return;
    }

    setIsLoading(true);
    setError(null);

    try {
      // Fetch skills
      const skillsJson = await invoke<string>('list_available_skills');
      const skillList = JSON.parse(skillsJson) as SkillMetadata[];
      setSkills(skillList);

      // Fetch tags
      const tagsJson = await invoke<string>('list_skill_tags');
      const tagList = JSON.parse(tagsJson) as string[];
      setTags([...new Set([...DEFAULT_SKILL_TAGS, ...tagList])] as SkillTag[]);
    } catch (err) {
      console.error('Failed to refresh skills:', err);
      setError(`Failed to load skills: ${err}`);
      toast.error('Failed to load skills');
    } finally {
      setIsLoading(false);
    }
  }, [isInitialized, initRegistry]);

  /**
   * Initialize on mount
   */
  useEffect(() => {
    refresh();
  }, [refresh]);

  /**
   * Get skill info by ID
   */
  const getSkillInfo = useCallback(async (skillId: string): Promise<SkillMetadata | null> => {
    try {
      const json = await invoke<string>('get_skill_info', { skillId });
      return parseSkillMetadata(json);
    } catch (err) {
      console.error(`Failed to get skill info for ${skillId}:`, err);
      toast.error(`Failed to get skill info: ${err}`);
      return null;
    }
  }, []);

  /**
   * Execute a skill
   */
  const executeSkill = useCallback(async (request: SkillExecutionRequest): Promise<SkillResult | null> => {
    setIsLoading(true);
    try {
      const json = await invoke<string>('execute_skill', {
        request: JSON.stringify({
          skill_id: request.skillId,
          agent_id: request.agentId,
          session_id: request.sessionId,
          user_input: request.userInput,
          config: request.config,
        }),
      });
      const result = parseSkillResult(json);

      if (!result.success && result.error) {
        toast.error(`Skill execution failed: ${result.error}`);
      }

      return result;
    } catch (err) {
      console.error('Failed to execute skill:', err);
      toast.error(`Failed to execute skill: ${err}`);
      return null;
    } finally {
      setIsLoading(false);
    }
  }, []);

  /**
   * Validate skill configuration
   */
  const validateConfig = useCallback(async (
    skillId: string,
    config: Record<string, unknown>
  ): Promise<boolean> => {
    try {
      await invoke('validate_skill_config', {
        skillId,
        config: JSON.stringify(config),
      });
      return true;
    } catch (err) {
      console.error('Config validation failed:', err);
      return false;
    }
  }, []);

  /**
   * Register a custom skill from YAML
   */
  const registerCustomSkill = useCallback(async (yaml: string): Promise<SkillMetadata | null> => {
    setIsLoading(true);
    try {
      const json = await invoke<string>('register_custom_skill', { skillYaml: yaml });
      const metadata = parseSkillMetadata(json);
      toast.success(`Skill "${metadata.name}" registered successfully`);

      // Refresh to include the new skill
      await refresh();

      return metadata;
    } catch (err) {
      console.error('Failed to register skill:', err);
      toast.error(`Failed to register skill: ${err}`);
      return null;
    } finally {
      setIsLoading(false);
    }
  }, [refresh]);

  /**
   * List skills by tag
   */
  const listByTag = useCallback(async (tag: string): Promise<SkillMetadata[]> => {
    try {
      const json = await invoke<string>('list_skills_by_tag', { tag });
      return JSON.parse(json) as SkillMetadata[];
    } catch (err) {
      console.error(`Failed to list skills by tag ${tag}:`, err);
      return [];
    }
  }, []);

  return {
    skills,
    tags,
    isLoading,
    error,
    refresh,
    getSkillInfo,
    executeSkill,
    validateConfig,
    registerCustomSkill,
    listByTag,
    initRegistry,
  };
}

/**
 * Hook for skill execution with tracking
 */
export interface UseSkillExecutionReturn {
  /** Execute a skill */
  execute: (request: SkillExecutionRequest) => Promise<SkillResult | null>;
  /** Last execution result */
  lastResult: SkillResult | null;
  /** Execution history */
  history: ExecutionLog[];
  /** Loading state */
  isExecuting: boolean;
  /** Clear history */
  clearHistory: () => void;
}

/**
 * Hook for skill execution with history tracking
 */
export function useSkillExecution(): UseSkillExecutionReturn {
  const [lastResult, setLastResult] = useState<SkillResult | null>(null);
  const [history, setHistory] = useState<ExecutionLog[]>([]);
  const [isExecuting, setIsExecuting] = useState(false);

  const execute = useCallback(async (request: SkillExecutionRequest): Promise<SkillResult | null> => {
    setIsExecuting(true);
    try {
      const json = await invoke<string>('execute_skill', {
        request: JSON.stringify({
          skill_id: request.skillId,
          agent_id: request.agentId,
          session_id: request.sessionId,
          user_input: request.userInput,
          config: request.config,
        }),
      });
      const result = parseSkillResult(json);
      setLastResult(result);

      // Add to history
      const log: ExecutionLog = {
        skillId: request.skillId,
        agentId: request.agentId,
        sessionId: request.sessionId,
        success: result.success,
        durationMs: result.durationMs,
        error: result.error,
        timestamp: new Date().toISOString(),
      };
      setHistory(prev => [...prev.slice(-99), log]);

      return result;
    } catch (err) {
      console.error('Skill execution failed:', err);

      // Add failed execution to history
      const log: ExecutionLog = {
        skillId: request.skillId,
        agentId: request.agentId,
        sessionId: request.sessionId,
        success: false,
        durationMs: 0,
        error: String(err),
        timestamp: new Date().toISOString(),
      };
      setHistory(prev => [...prev.slice(-99), log]);

      return null;
    } finally {
      setIsExecuting(false);
    }
  }, []);

  const clearHistory = useCallback(() => {
    setHistory([]);
    setLastResult(null);
  }, []);

  return {
    execute,
    lastResult,
    history,
    isExecuting,
    clearHistory,
  };
}

/**
 * Hook return type for agent skill configuration
 */
export interface UseAgentSkillConfigReturn {
  /** Current skill configuration */
  config: AgentSkillConfig | null;
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: string | null;
  /** Load config for an agent */
  loadConfig: (agentId: string) => Promise<void>;
  /** Update config */
  updateConfig: (config: AgentSkillConfig) => Promise<boolean>;
  /** Toggle a skill */
  toggleSkill: (agentId: string, skillId: string, enabled: boolean) => Promise<boolean>;
}

/**
 * Hook for agent skill configuration management
 */
export function useAgentSkillConfig(): UseAgentSkillConfigReturn {
  const [config, setConfig] = useState<AgentSkillConfig | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadConfig = useCallback(async (agentId: string) => {
    setIsLoading(true);
    setError(null);
    try {
      const json = await invoke<string>('get_agent_skill_config', { agentId });
      const loadedConfig = JSON.parse(json) as AgentSkillConfig;
      setConfig(loadedConfig);
    } catch (err) {
      console.error('Failed to load agent skill config:', err);
      setError(`Failed to load skill config: ${err}`);
    } finally {
      setIsLoading(false);
    }
  }, []);

  const updateConfig = useCallback(async (newConfig: AgentSkillConfig): Promise<boolean> => {
    setIsLoading(true);
    try {
      await invoke('update_agent_skill_config', {
        config: JSON.stringify({
          agent_id: newConfig.agentId,
          enabled_skills: newConfig.enabledSkills,
          skill_configs: newConfig.skillConfigs,
        }),
      });
      setConfig(newConfig);
      toast.success('技能配置已保存');
      return true;
    } catch (err) {
      console.error('Failed to update agent skill config:', err);
      setError(`Failed to save skill config: ${err}`);
      toast.error('保存技能配置失败');
      return false;
    } finally {
      setIsLoading(false);
    }
  }, []);

  const toggleSkill = useCallback(async (
    agentId: string,
    skillId: string,
    enabled: boolean
  ): Promise<boolean> => {
    setIsLoading(true);
    try {
      const json = await invoke<string>('toggle_agent_skill', {
        agentId,
        skillId,
        enabled,
      });
      const updatedConfig = JSON.parse(json) as AgentSkillConfig;
      setConfig(updatedConfig);
      return true;
    } catch (err) {
      console.error('Failed to toggle skill:', err);
      setError(`Failed to toggle skill: ${err}`);
      return false;
    } finally {
      setIsLoading(false);
    }
  }, []);

  return {
    config,
    isLoading,
    error,
    loadConfig,
    updateConfig,
    toggleSkill,
  };
}

/**
 * Hook return type for skill usage statistics
 */
export interface UseSkillUsageStatsReturn {
  /** Usage statistics map */
  stats: Map<string, SkillUsageStatistics>;
  /** Execution logs */
  logs: ExecutionLog[];
  /** Loading state */
  isLoading: boolean;
  /** Refresh statistics */
  refresh: () => Promise<void>;
}

/**
 * Hook for skill usage statistics
 */
export function useSkillUsageStats(): UseSkillUsageStatsReturn {
  const [stats, setStats] = useState<Map<string, SkillUsageStatistics>>(new Map());
  const [logs, setLogs] = useState<ExecutionLog[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    try {
      // Fetch logs
      const logsJson = await invoke<string>('get_skill_execution_logs', { limit: 1000 });
      const logsList = JSON.parse(logsJson) as ExecutionLog[];
      setLogs(logsList);

      // Calculate statistics from logs
      const statsMap = new Map<string, SkillUsageStatistics>();

      // Group by skillId and calculate stats
      const skillLogs = new Map<string, ExecutionLog[]>();
      logsList.forEach(log => {
        const existing = skillLogs.get(log.skillId) || [];
        existing.push(log);
        skillLogs.set(log.skillId, existing);
      });

      skillLogs.forEach((skillLogList, skillId) => {
        const totalExecutions = skillLogList.length;
        const successCount = skillLogList.filter(l => l.success).length;
        const failureCount = totalExecutions - successCount;
        const totalDuration = skillLogList.reduce((sum, l) => sum + l.durationMs, 0);
        const avgDurationMs = totalExecutions > 0 ? totalDuration / totalExecutions : 0;

        // Find last execution time
        const sortedLogs = [...skillLogList].sort((a, b) =>
          new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()
        );
        const lastExecutedAt = sortedLogs[0]?.timestamp;

        statsMap.set(skillId, {
          skillId,
          totalExecutions,
          successCount,
          failureCount,
          avgDurationMs,
          lastExecutedAt,
        });
      });

      setStats(statsMap);
    } catch (err) {
      console.error('Failed to load skill usage stats:', err);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return {
    stats,
    logs,
    isLoading,
    refresh,
  };
}