/**
 * AI 代理类型定义
 *
 * 包含代理相关的数据模型和类型
 *
 * [Source: architecture.md#数据模型]
 */

import { type MBTIType } from '@/lib/personality-colors';
import { type MemoryContextInfo } from './memory';

// Re-export MBTIType for convenience
export type { MBTIType } from '@/lib/personality-colors';

// ============================================================================
// 响应风格类型 (Story 7.1)
// ============================================================================

/**
 * 响应风格类型
 */
export type ResponseStyle = 'formal' | 'casual' | 'concise' | 'detailed';

/**
 * 响应风格标签映射
 */
export const RESPONSE_STYLE_LABELS: Record<ResponseStyle, string> = {
  formal: '正式',
  casual: '随意',
  concise: '简洁',
  detailed: '详细',
};

/**
 * 详细程度预设
 */
export const VERBOSITY_PRESETS = {
  brief: 0.2,
  normal: 0.5,
  detailed: 0.8,
} as const;

/**
 * 详细程度标签映射
 */
export const VERBOSITY_LABELS: Record<keyof typeof VERBOSITY_PRESETS, string> = {
  brief: '简短',
  normal: '中等',
  detailed: '详细',
};

/**
 * 代理风格配置
 */
export interface AgentStyleConfig {
  /** 响应风格 */
  responseStyle: ResponseStyle;
  /** 详细程度 (0.0-1.0) */
  verbosity: number;
  /** 最大响应长度 (0 = 无限制) */
  maxResponseLength: number;
  /** 是否添加友好问候语 */
  friendlyTone: boolean;
}

/**
 * 默认风格配置
 */
export const DEFAULT_STYLE_CONFIG: AgentStyleConfig = {
  responseStyle: 'detailed',
  verbosity: 0.5,
  maxResponseLength: 0,
  friendlyTone: true,
};

// ============================================================================
// 上下文窗口配置类型 (Story 7.2)
// ============================================================================

/**
 * 溢出策略类型
 */
export type OverflowStrategy = 'truncate' | 'summarize' | 'error';

/**
 * 溢出策略标签映射
 */
export const OVERFLOW_STRATEGY_LABELS: Record<OverflowStrategy, string> = {
  truncate: '截断旧消息',
  summarize: '摘要旧消息',
  error: '返回错误',
};

/**
 * 上下文窗口配置
 */
export interface ContextWindowConfig {
  /** 最大上下文窗口大小 (tokens, 0 = 使用模型默认) */
  maxTokens: number;
  /** 溢出策略 */
  overflowStrategy: OverflowStrategy;
  /** 是否在 Token 计数中包含系统提示词 */
  includeSystemPrompt: boolean;
  /** 为模型响应预留的 Token 数量 */
  responseReserve: number;
}

/**
 * 默认上下文窗口配置
 */
export const DEFAULT_CONTEXT_WINDOW_CONFIG: ContextWindowConfig = {
  maxTokens: 4096,
  overflowStrategy: 'truncate',
  includeSystemPrompt: true,
  responseReserve: 1024,
};

/**
 * 模型上下文窗口推荐
 */
export interface ModelContextRecommendation {
  modelName: string;
  recommended: number;
  max: number;
}

/**
 * 上下文窗口预设
 */
export const CONTEXT_WINDOW_PRESETS = [
  { label: '紧凑 (2K)', value: 2048 },
  { label: '标准 (4K)', value: 4096 },
  { label: '较大 (8K)', value: 8192 },
  { label: '大型 (16K)', value: 16384 },
  { label: '超大 (32K)', value: 32768 },
] as const;

// ============================================================================
// 触发关键词配置类型 (Story 7.3)
// ============================================================================

/**
 * 匹配类型
 */
export type MatchType = 'exact' | 'prefix' | 'contains' | 'regex';

/**
 * 匹配类型标签映射
 */
export const MATCH_TYPE_LABELS: Record<MatchType, string> = {
  exact: '精确匹配',
  prefix: '前缀匹配',
  contains: '包含匹配',
  regex: '正则表达式',
};

/**
 * 匹配类型描述
 */
export const MATCH_TYPE_DESCRIPTIONS: Record<MatchType, string> = {
  exact: '完整匹配整个词语',
  prefix: '匹配以关键词开头的内容',
  contains: '只要包含关键词即可匹配',
  regex: '使用正则表达式进行复杂匹配',
};

/**
 * 触发关键词
 */
export interface TriggerKeyword {
  /** 关键词或模式 */
  keyword: string;
  /** 匹配类型 */
  matchType: MatchType;
  /** 是否区分大小写 */
  caseSensitive: boolean;
}

/**
 * 匹配的关键词信息（测试结果）
 */
export interface MatchedKeywordInfo {
  /** 匹配的关键词 */
  keyword: string;
  /** 匹配类型 */
  matchType: MatchType;
  /** 是否区分大小写 */
  caseSensitive: boolean;
}

/**
 * 触发词测试结果
 */
export interface TriggerTestResult {
  /** 是否匹配 */
  matched: boolean;
  /** 匹配的关键词列表 */
  matchedKeywords: MatchedKeywordInfo[];
}

/**
 * 代理触发关键词配置
 */
export interface AgentTriggerConfig {
  /** 触发关键词列表 */
  keywords: TriggerKeyword[];
  /** 是否启用触发词 */
  enabled: boolean;
  /** 默认匹配类型 */
  defaultMatchType: MatchType;
  /** 默认是否区分大小写 */
  defaultCaseSensitive: boolean;
}

/**
 * 默认触发关键词配置
 */
export const DEFAULT_TRIGGER_CONFIG: AgentTriggerConfig = {
  keywords: [],
  enabled: true,
  defaultMatchType: 'exact',
  defaultCaseSensitive: false,
};

// ============================================================================
// 隐私配置类型 (Story 7.4)
// ============================================================================

/**
 * 记忆共享范围
 */
export type MemorySharingScope = 'singleSession' | 'crossSession' | 'crossAgent';

/**
 * 记忆共享范围标签映射
 */
export const MEMORY_SHARING_SCOPE_LABELS: Record<MemorySharingScope, string> = {
  singleSession: '仅当前会话',
  crossSession: '跨会话共享',
  crossAgent: '跨代理共享',
};

/**
 * 记忆共享范围描述
 */
export const MEMORY_SHARING_SCOPE_DESCRIPTIONS: Record<MemorySharingScope, string> = {
  singleSession: '记忆仅在当前对话会话中可用',
  crossSession: '记忆可在不同会话间共享',
  crossAgent: '记忆可在不同代理间共享',
};

/**
 * 数据保留策略
 */
export interface DataRetentionPolicy {
  /** 情景记忆保留天数 (0 = 永久保留) */
  episodicMemoryDays: number;
  /** 工作记忆保留小时数 */
  workingMemoryHours: number;
  /** 是否自动清理过期数据 */
  autoCleanup: boolean;
}

/**
 * 默认数据保留策略
 */
export const DEFAULT_DATA_RETENTION_POLICY: DataRetentionPolicy = {
  episodicMemoryDays: 90,
  workingMemoryHours: 24,
  autoCleanup: true,
};

/**
 * 敏感信息过滤配置
 */
export interface SensitiveDataFilter {
  /** 是否启用敏感信息过滤 */
  enabled: boolean;
  /** 过滤邮箱地址 */
  filterEmail: boolean;
  /** 过滤电话号码 */
  filterPhone: boolean;
  /** 过滤身份证号 */
  filterIdCard: boolean;
  /** 过滤银行卡号 */
  filterBankCard: boolean;
  /** 过滤IP地址 */
  filterIpAddress: boolean;
  /** 自定义正则表达式模式 */
  customPatterns: string[];
}

/**
 * 默认敏感信息过滤配置
 */
export const DEFAULT_SENSITIVE_DATA_FILTER: SensitiveDataFilter = {
  enabled: false,
  filterEmail: true,
  filterPhone: true,
  filterIdCard: true,
  filterBankCard: true,
  filterIpAddress: false,
  customPatterns: [],
};

/**
 * 排除数据规则
 */
export interface ExclusionRule {
  /** 规则名称 */
  name: string;
  /** 规则描述 */
  description?: string;
  /** 匹配模式（正则表达式） */
  pattern: string;
  /** 是否启用 */
  enabled: boolean;
}

/**
 * 代理隐私配置
 */
export interface AgentPrivacyConfig {
  /** 数据保留策略 */
  dataRetention: DataRetentionPolicy;
  /** 敏感信息过滤配置 */
  sensitiveFilter: SensitiveDataFilter;
  /** 记忆共享范围 */
  memorySharingScope: MemorySharingScope;
  /** 排除数据规则列表 */
  exclusionRules: ExclusionRule[];
  /** 是否记录详细日志 */
  verboseLogging: boolean;
}

/**
 * 默认隐私配置
 */
export const DEFAULT_PRIVACY_CONFIG: AgentPrivacyConfig = {
  dataRetention: DEFAULT_DATA_RETENTION_POLICY,
  sensitiveFilter: DEFAULT_SENSITIVE_DATA_FILTER,
  memorySharingScope: 'singleSession',
  exclusionRules: [],
  verboseLogging: false,
};

// ============================================================================
// 类型定义
// ============================================================================

/**
 * 代理状态类型
 */
export type AgentStatus = 'active' | 'inactive' | 'archived';

/**
 * 代理模型（与后端 AgentModel 一致）
 */
export interface AgentModel {
  /** 自增主键 */
  id: number;
  /** UUID */
  agent_uuid: string;
  /** 名称 */
  name: string;
  /** 描述 */
  description?: string;
  /** 专业领域 */
  domain?: string;
  /** MBTI 人格类型 */
  mbti_type?: MBTIType;
  /** 状态 */
  status: AgentStatus;
  /** 系统提示词 */
  system_prompt?: string;
  /** 默认 LLM 提供商 ID */
  default_provider_id?: string;
  /** 风格配置 (JSON 字符串) */
  style_config?: string;
  /** 上下文窗口配置 (JSON 字符串) [Story 7.2] */
  context_window_config?: string;
  /** 触发关键词配置 (JSON 字符串) [Story 7.3] */
  trigger_keywords_config?: string;
  /** 隐私配置 (JSON 字符串) [Story 7.4] */
  privacy_config?: string;
  /** 创建时间（Unix 时间戳） */
  created_at: number;
  /** 更新时间 */
  updated_at: number;
}

/**
 * 新代理数据（发送给后端创建代理）
 */
export interface NewAgent {
  /** 代理名称（必填） */
  name: string;
  /** 描述 */
  description?: string;
  /** 专业领域 */
  domain?: string;
  /** MBTI 人格类型 */
  mbti_type?: MBTIType;
  /** 系统提示词 */
  system_prompt?: string;
  /** 默认 LLM 提供商 ID */
  default_provider_id?: string;
  /** 风格配置 */
  style_config?: string;
  /** 上下文窗口配置 [Story 7.2] */
  context_window_config?: string;
  /** 触发关键词配置 [Story 7.3] */
  trigger_keywords_config?: string;
  /** 隐私配置 [Story 7.4] */
  privacy_config?: string;
}

/**
 * 代理更新数据（发送给后端更新代理）
 *
 * 所有字段均为可选，仅更新提供的字段
 */
export interface AgentUpdate {
  /** 代理名称 */
  name?: string;
  /** 描述 */
  description?: string;
  /** 专业领域 */
  domain?: string;
  /** MBTI 人格类型 */
  mbti_type?: MBTIType;
  /** 系统提示词 */
  system_prompt?: string;
  /** 默认 LLM 提供商 ID */
  default_provider_id?: string;
  /** 风格配置 */
  style_config?: string;
  /** 上下文窗口配置 [Story 7.2] */
  context_window_config?: string;
  /** 触发关键词配置 [Story 7.3] */
  trigger_keywords_config?: string;
  /** 隐私配置 [Story 7.4] */
  privacy_config?: string;
}

// ============================================================================
// Chat Types (Story 4.2)
// ============================================================================

/**
 * 发送消息请求
 */
export interface SendMessageRequest {
  /** 代理 ID（数据库 ID） */
  agentId: number;
  /** 消息内容 */
  message: string;
  /** 可选提供商 ID */
  providerId?: string;
  /** 可选模型覆盖 */
  model?: string;
}

/**
 * 发送消息到会话请求
 */
export interface SendMessageToSessionRequest {
  /** 会话 ID */
  sessionId: number;
  /** 消息内容 */
  message: string;
  /** 可选提供商 ID */
  providerId?: string;
  /** 可选模型覆盖 */
  model?: string;
}

/**
 * 创建会话并发送消息请求
 */
export interface CreateSessionAndSendRequest {
  /** 代理 ID（数据库 ID） */
  agentId: number;
  /** 可选会话标题 */
  title?: string;
  /** 消息内容 */
  message: string;
  /** 可选提供商 ID */
  providerId?: string;
  /** 可选模型覆盖 */
  model?: string;
}

/**
 * 聊天响应
 */
export interface ChatResponse {
  /** 助手回复文本 */
  response: string;
  /** 会话 ID（现有或新创建） */
  sessionId: number;
  /** 助手回复的消息 ID */
  messageId: number;
  /** 记忆上下文信息（如果使用了记忆增强） */
  memoryContext?: MemoryContextInfo;
}

// ============================================================================
// Streaming Types (Story 4.3)
// ============================================================================

/**
 * 流式事件类型
 */
export type StreamEventType = 'start' | 'delta' | 'toolCall' | 'done' | 'error';

/**
 * Token 使用统计
 */
export interface TokenUsage {
  /** 输入 token 数 */
  inputTokens?: number;
  /** 输出 token 数 */
  outputTokens?: number;
}

/**
 * 流式事件基础接口
 */
export interface StreamEventBase {
  /** 事件类型 */
  type: StreamEventType;
}

/**
 * 流开始事件
 *
 * 当流式响应开始时发出
 */
export interface StreamStartEvent extends StreamEventBase {
  type: 'start';
  /** 会话 ID */
  sessionId: number;
  /** 请求唯一标识符 */
  requestId: string;
}

/**
 * 流增量事件
 *
 * 每次收到新的内容块时发出
 */
export interface StreamDeltaEvent extends StreamEventBase {
  type: 'delta';
  /** 文本内容增量 */
  delta: string;
  /** 推理内容（适用于 DeepSeek R1 等思考模型） */
  reasoning?: string;
}

/**
 * 流工具调用事件
 *
 * 当模型请求调用工具时发出
 */
export interface StreamToolCallEvent extends StreamEventBase {
  type: 'toolCall';
  /** 工具名称 */
  toolName: string;
  /** 工具参数 */
  toolArgs: unknown;
}

/**
 * 流完成事件
 *
 * 当流式响应成功完成时发出
 */
export interface StreamDoneEvent extends StreamEventBase {
  type: 'done';
  /** 会话 ID */
  sessionId: number;
  /** 消息 ID */
  messageId: number;
  /** Token 使用统计 */
  usage?: TokenUsage;
}

/**
 * 流错误事件
 *
 * 当流式响应遇到错误时发出
 */
export interface StreamErrorEvent extends StreamEventBase {
  type: 'error';
  /** 错误代码 */
  code: string;
  /** 错误消息（中文） */
  message: string;
  /** 部分内容（中断前已接收的内容） */
  partialContent?: string;
}

/**
 * 所有流式事件的联合类型
 */
export type StreamEvent =
  | StreamStartEvent
  | StreamDeltaEvent
  | StreamToolCallEvent
  | StreamDoneEvent
  | StreamErrorEvent;

/**
 * 流式聊天请求
 */
export interface StreamChatRequest {
  /** 代理 ID（数据库 ID） */
  agentId: number;
  /** 可选会话 ID（继续对话时提供） */
  sessionId?: number;
  /** 消息内容 */
  message: string;
  /** 可选提供商 ID */
  providerId?: string;
  /** 可选模型覆盖 */
  model?: string;
}

/**
 * 流式状态
 */
export type StreamingStatus = 'idle' | 'streaming' | 'done' | 'error' | 'cancelled';

/**
 * 流式错误代码
 */
export const StreamErrorCodes = {
  /** 提供商错误 */
  PROVIDER_ERROR: 'PROVIDER_ERROR',
  /** 频率限制 */
  RATE_LIMIT: 'RATE_LIMIT',
  /** 上下文长度超限 */
  CONTEXT_LENGTH: 'CONTEXT_LENGTH',
  /** 连接错误 */
  CONNECTION_ERROR: 'CONNECTION_ERROR',
  /** 用户取消 */
  CANCELLED: 'CANCELLED',
  /** 首字节超时 */
  FIRST_TOKEN_TIMEOUT: 'FIRST_TOKEN_TIMEOUT',
  /** 内部错误 */
  INTERNAL_ERROR: 'INTERNAL_ERROR',
} as const;

export type StreamErrorCode = typeof StreamErrorCodes[keyof typeof StreamErrorCodes];