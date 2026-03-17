/**
 * 备份恢复类型定义
 *
 * 包含备份相关的数据模型和类型
 *
 * [Source: 2-12-config-backup-restore.md]
 */

// ============================================================================
// 备份元数据
// ============================================================================

/**
 * 备份元数据
 *
 * 包含备份文件的版本和时间信息
 */
export interface BackupMeta {
  /** 备份格式版本 (e.g., "1.0") */
  version: string;
  /** 创建备份的应用版本 */
  app_version: string;
  /** 创建时间 (ISO 8601) */
  created_at: string;
  /** 数据校验和 (可选) */
  checksum?: string;
}

// ============================================================================
// 配置备份
// ============================================================================

/**
 * 提供商备份
 */
export interface ProviderBackup {
  /** 提供商名称 */
  name: string;
  /** 提供商类型 */
  provider_type: string;
  /** 默认模型 */
  model?: string;
  /** API URL */
  api_url?: string;
  /** 是否为默认 */
  is_default: boolean;
  /** 其他设置 */
  settings: Record<string, unknown>;
}

/**
 * 渠道备份
 */
export interface ChannelsBackup {
  /** 渠道列表 */
  channels: ChannelBackup[];
}

/**
 * 渠道备份项
 */
export interface ChannelBackup {
  /** 渠道 ID */
  id: string;
  /** 渠道类型 */
  type: string;
  /** 是否启用 */
  enabled: boolean;
}

/**
 * 技能备份
 */
export interface SkillsBackup {
  /** 技能列表 */
  skills: SkillBackup[];
}

/**
 * 技能备份项
 */
export interface SkillBackup {
  /** 技能 ID */
  id: string;
  /** 技能名称 */
  name: string;
  /** 是否启用 */
  enabled: boolean;
}

/**
 * 代理人格备份
 */
export interface AgentPersonaBackup {
  /** MBTI 类型 */
  mbti_type?: string;
  /** 系统提示词模板 */
  system_prompt_template?: string;
}

/**
 * 配置备份
 */
export interface ConfigBackup {
  /** 提供商列表 */
  providers: ProviderBackup[];
  /** 渠道配置 */
  channels: ChannelsBackup;
  /** 技能配置 */
  skills: SkillsBackup;
  /** 代理人格配置 */
  agent_persona: AgentPersonaBackup;
  /** 其他偏好设置 */
  preferences: Record<string, unknown>;
}

// ============================================================================
// 代理备份
// ============================================================================

/**
 * 代理备份数据
 */
export interface AgentBackup {
  /** 代理 UUID */
  uuid: string;
  /** 代理名称 */
  name: string;
  /** 描述 */
  description?: string;
  /** 专业领域 */
  domain?: string;
  /** MBTI 类型 */
  mbti_type?: string;
  /** 系统提示词 */
  system_prompt?: string;
  /** 状态 */
  status: string;
  /** 创建时间 */
  created_at: number;
  /** 更新时间 */
  updated_at: number;
}

// ============================================================================
// 账户备份
// ============================================================================

/**
 * 账户备份数据（不含密码）
 */
export interface AccountBackup {
  /** 用户名 */
  username: string;
  /** 是否启动时要求密码 */
  require_password_on_startup: boolean;
  /** 创建时间 */
  created_at: number;
  /** 更新时间 */
  updated_at: number;
}

// ============================================================================
// 备份数据
// ============================================================================

/**
 * 完整备份数据结构
 */
export interface BackupData {
  /** 备份元数据 */
  meta: BackupMeta;
  /** 配置数据 */
  config: ConfigBackup;
  /** 代理列表 */
  agents: AgentBackup[];
  /** 账户信息 */
  account?: AccountBackup;
}

// ============================================================================
// 导入选项
// ============================================================================

/**
 * 导入模式
 */
export type ImportMode = 'overwrite' | 'merge';

/**
 * 导入选项
 */
export interface ImportOptions {
  /** 导入模式 */
  mode: ImportMode;
  /** 是否导入代理配置 */
  include_agents: boolean;
  /** 是否导入提供商配置 */
  include_providers: boolean;
  /** 是否导入渠道配置 */
  include_channels: boolean;
  /** 是否导入技能配置 */
  include_skills: boolean;
  /** 是否导入账户设置 */
  include_account: boolean;
}

/**
 * 默认导入选项
 */
export const DEFAULT_IMPORT_OPTIONS: ImportOptions = {
  mode: 'merge',
  include_agents: true,
  include_providers: true,
  include_channels: true,
  include_skills: true,
  include_account: false,
};

// ============================================================================
// 导入结果
// ============================================================================

/**
 * 导入结果
 */
export interface ImportResult {
  /** 导入的代理数量 */
  agents_imported: number;
  /** 是否导入账户 */
  account_imported: boolean;
}

// ============================================================================
// 备份格式
// ============================================================================

/**
 * 备份导出格式
 */
export type BackupFormat = 'json' | 'yaml';