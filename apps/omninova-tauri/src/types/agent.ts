/**
 * AI 代理类型定义
 *
 * 包含代理相关的数据模型和类型
 *
 * [Source: architecture.md#数据模型]
 */

import { type MBTIType } from '@/lib/personality-colors';

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
}