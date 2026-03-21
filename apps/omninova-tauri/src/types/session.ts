/**
 * 会话与消息类型定义
 *
 * 包含会话和消息相关的数据模型和类型
 *
 * [Source: architecture.md#数据模型]
 */

// ============================================================================
// 类型定义
// ============================================================================

/**
 * 消息角色类型
 */
export type MessageRole = 'user' | 'assistant' | 'system';

/**
 * 会话模型（与后端 Session 一致）
 */
export interface Session {
  /** 自增主键 */
  id: number;
  /** 所属代理 ID */
  agentId: number;
  /** 会话标题 */
  title?: string;
  /** 创建时间（Unix 时间戳） */
  createdAt: number;
  /** 更新时间 */
  updatedAt: number;
}

/**
 * 新会话数据（发送给后端创建会话）
 */
export interface NewSession {
  /** 所属代理 ID（必填） */
  agentId: number;
  /** 会话标题 */
  title?: string;
}

/**
 * 会话更新数据（发送给后端更新会话）
 *
 * 所有字段均为可选，仅更新提供的字段
 */
export interface SessionUpdate {
  /** 会话标题 */
  title?: string;
}

/**
 * 消息模型（与后端 Message 一致）
 *
 * 支持消息引用功能：
 * - 通过 quoteMessageId 字段引用之前的消息
 * - 引用消息会在回复中显示上下文预览
 * - 引用关系用于构建对话线程结构
 *
 * [Source: Story 5.8 - 重要片段标记功能]
 */
export interface Message {
  /** 自增主键 */
  id: number;
  /** 所属会话 ID */
  sessionId: number;
  /** 消息角色 */
  role: MessageRole;
  /** 消息内容 */
  content: string;
  /** 创建时间（Unix 时间戳） */
  createdAt: number;
  /**
   * 引用的消息 ID（可选，用于回复功能）
   *
   * 当此字段存在时，表示该消息是对另一条消息的引用回复。
   * 引用的消息应属于同一会话。
   */
  quoteMessageId?: number;
  /**
   * 是否被用户标记为重要
   *
   * [Source: Story 5.8 - 重要片段标记功能]
   */
  isMarked?: boolean;
}

/**
 * 新消息数据（发送给后端创建消息）
 */
export interface NewMessage {
  /** 所属会话 ID（必填） */
  sessionId: number;
  /** 消息角色 */
  role: MessageRole;
  /** 消息内容（必填） */
  content: string;
}

/**
 * 引用消息的简化视图（用于显示引用预览）
 *
 * 包含引用消息的关键信息，用于在 UI 中显示引用预览卡片
 */
export interface QuoteMessage {
  /** 原消息 ID */
  id: number;
  /** 消息内容预览（截断后的内容） */
  contentPreview: string;
  /** 消息角色 */
  role: MessageRole;
  /** 发送者名称（用户或 Agent 名称） */
  senderName?: string;
}