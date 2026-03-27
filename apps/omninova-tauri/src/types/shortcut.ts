/**
 * Shortcut Types
 *
 * Type definitions for keyboard shortcuts.
 *
 * [Source: Story 10.6 - 键盘快捷键]
 */

// ============================================================================
// Types
// ============================================================================

/**
 * 快捷键组合
 */
export interface ShortcutKey {
  /** 主键 */
  key: string;
  /** 是否需要 Ctrl/Cmd */
  meta?: boolean;
  /** 是否需要 Shift */
  shift?: boolean;
  /** 是否需要 Alt */
  alt?: boolean;
}

/**
 * 快捷键动作类型
 */
export type ShortcutAction =
  | 'globalSearch'
  | 'toggleSidebar'
  | 'newSession'
  | 'switchAgent1'
  | 'switchAgent2'
  | 'switchAgent3'
  | 'switchAgent4'
  | 'switchAgent5';

/**
 * 快捷键配置
 */
export interface ShortcutConfig {
  action: ShortcutAction;
  keys: ShortcutKey;
  description: string;
}

// ============================================================================
// Constants
// ============================================================================

/**
 * 默认快捷键配置
 */
export const DEFAULT_SHORTCUTS: ShortcutConfig[] = [
  {
    action: 'globalSearch',
    keys: { key: 'k', meta: true },
    description: '全局搜索',
  },
  {
    action: 'toggleSidebar',
    keys: { key: 'b', meta: true },
    description: '切换侧边栏',
  },
  {
    action: 'newSession',
    keys: { key: 'n', meta: true },
    description: '新会话',
  },
  {
    action: 'switchAgent1',
    keys: { key: '1', meta: true },
    description: '切换代理 1',
  },
  {
    action: 'switchAgent2',
    keys: { key: '2', meta: true },
    description: '切换代理 2',
  },
  {
    action: 'switchAgent3',
    keys: { key: '3', meta: true },
    description: '切换代理 3',
  },
  {
    action: 'switchAgent4',
    keys: { key: '4', meta: true },
    description: '切换代理 4',
  },
  {
    action: 'switchAgent5',
    keys: { key: '5', meta: true },
    description: '切换代理 5',
  },
];