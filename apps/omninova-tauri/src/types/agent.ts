/**
 * AI 代理类型定义
 *
 * 包含代理相关的数据模型和类型
 *
 * [Source: architecture.md#数据模型]
 */

import { type MBTIType } from '@/lib/personality-colors';

// Re-export MBTIType for convenience
export type { MBTIType } from '@/lib/personality-colors';

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