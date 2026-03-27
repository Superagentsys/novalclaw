/**
 * Workspace Types
 *
 * Type definitions for workspace management.
 *
 * [Source: Story 10.5 - 工作空间管理]
 */

// ============================================================================
// Types
// ============================================================================

/**
 * 工作空间配置
 */
export interface Workspace {
  /** 工作空间 ID */
  id: string;
  /** 工作空间名称 */
  name: string;
  /** 工作空间图标 (emoji) */
  icon: string;
  /** 创建时间 */
  createdAt: string;
  /** 最后访问时间 */
  lastAccessedAt: string;
}

// ============================================================================
// Constants
// ============================================================================

/**
 * 默认工作空间
 */
export const DEFAULT_WORKSPACE: Workspace = {
  id: 'default',
  name: '默认工作空间',
  icon: '🏠',
  createdAt: new Date().toISOString(),
  lastAccessedAt: new Date().toISOString(),
};

/**
 * 预设图标
 */
export const WORKSPACE_ICONS = [
  '🏠',
  '💼',
  '🚀',
  '📚',
  '🎮',
  '🎨',
  '🔧',
  '📊',
  '🌟',
  '🎯',
  '💻',
  '🔬',
  '📱',
  '🎵',
  '✈️',
  '📝',
] as const;

export type WorkspaceIcon = (typeof WORKSPACE_ICONS)[number];