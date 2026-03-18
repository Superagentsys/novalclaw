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