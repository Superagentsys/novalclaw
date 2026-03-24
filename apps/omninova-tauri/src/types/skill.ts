/**
 * Skill System Types
 *
 * Types for the skill system including metadata, results, and errors.
 * [Source: Story 7.5 - 技能系统框架]
 */

/**
 * Skill metadata describing a skill.
 */
export interface SkillMetadata {
  /** Unique identifier for the skill */
  id: string;
  /** Human-readable name */
  name: string;
  /** Version string (semver recommended) */
  version: string;
  /** Detailed description */
  description: string;
  /** Author or creator */
  author?: string;
  /** Tags for categorization */
  tags: string[];
  /** IDs of dependent skills */
  dependencies: string[];
  /** Whether this is a built-in skill */
  isBuiltin: boolean;
  /** JSON Schema for configuration */
  configSchema?: Record<string, unknown>;
  /** Homepage or documentation URL */
  homepage?: string;
}

/**
 * Skill execution result.
 */
export interface SkillResult {
  /** Whether execution was successful */
  success: boolean;
  /** Text content of the result */
  content?: string;
  /** Structured data returned by the skill */
  data?: Record<string, unknown>;
  /** Error message if not successful */
  error?: string;
  /** Execution duration in milliseconds */
  durationMs: number;
  /** Additional metadata */
  metadata: Record<string, string>;
}

/**
 * Skill error types.
 */
export type SkillErrorType =
  | 'configuration'
  | 'execution'
  | 'timeout'
  | 'permission'
  | 'dependency'
  | 'validation'
  | 'notFound'
  | 'notRegistered';

/**
 * Skill error response.
 */
export interface SkillError {
  type: SkillErrorType;
  message: string;
  details?: Record<string, unknown>;
}

/**
 * Skill execution request.
 */
export interface SkillExecutionRequest {
  /** Skill ID to execute */
  skillId: string;
  /** Agent ID executing the skill */
  agentId: string;
  /** Session ID (optional) */
  sessionId?: string;
  /** User input that triggered the skill */
  userInput: string;
  /** Skill configuration parameters */
  config: Record<string, unknown>;
}

/**
 * Permission types for skills.
 */
export type Permission =
  | 'memory_read'
  | 'memory_write'
  | 'file_read'
  | 'file_write'
  | 'execute_command'
  | 'network_access'
  | 'external_api_access';

/**
 * Permission set for a skill.
 */
export interface PermissionSet {
  permissions: Permission[];
}

/**
 * Default skill tags available in the system.
 */
export const DEFAULT_SKILL_TAGS = [
  'productivity',
  'analysis',
  'creative',
  'automation',
  'integration',
  'openclaw',
] as const;

export type SkillTag = (typeof DEFAULT_SKILL_TAGS)[number];

/**
 * Cache statistics for the skill executor.
 */
export interface CacheStats {
  totalEntries: number;
  expiredEntries: number;
  activeEntries: number;
  maxEntries: number;
}

/**
 * Execution log entry.
 */
export interface ExecutionLog {
  /** Skill ID that was executed */
  skillId: string;
  /** Agent ID that executed the skill */
  agentId: string;
  /** Session ID if available */
  sessionId?: string;
  /** Whether execution was successful */
  success: boolean;
  /** Execution duration in milliseconds */
  durationMs: number;
  /** Error message if failed */
  error?: string;
  /** Timestamp of execution */
  timestamp: string;
}

/**
 * Agent skill configuration.
 * [Source: Story 7.6 - 技能管理界面]
 */
export interface AgentSkillConfig {
  /** Agent ID */
  agentId: string;
  /** List of enabled skill IDs */
  enabledSkills: string[];
  /** Skill configuration parameters */
  skillConfigs: Record<string, Record<string, unknown>>;
}

/**
 * Skill usage statistics.
 * [Source: Story 7.6 - 技能管理界面]
 */
export interface SkillUsageStatistics {
  /** Skill ID */
  skillId: string;
  /** Total execution count */
  totalExecutions: number;
  /** Success count */
  successCount: number;
  /** Failure count */
  failureCount: number;
  /** Average execution duration (ms) */
  avgDurationMs: number;
  /** Last execution time */
  lastExecutedAt?: string;
}