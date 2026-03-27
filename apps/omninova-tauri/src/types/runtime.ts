/**
 * Runtime Types
 *
 * Type definitions for runtime mode management.
 *
 * [Source: Story 9.5 - 运行模式管理]
 */

// ============================================================================
// Types
// ============================================================================

/**
 * 运行模式
 */
export type RunMode = 'desktop' | 'background';

/**
 * 运行模式配置
 */
export interface RunModeConfig {
  /** 当前运行模式 */
  mode: RunMode;
  /** 是否开机自启动 */
  autoStart: boolean;
}

// ============================================================================
// Constants
// ============================================================================

/**
 * 运行模式标签
 */
export const RUN_MODE_LABELS: Record<RunMode, string> = {
  desktop: '桌面模式',
  background: '后台模式',
};

/**
 * 运行模式描述
 */
export const RUN_MODE_DESCRIPTIONS: Record<RunMode, string> = {
  desktop: '显示完整窗口界面',
  background: '最小化到系统托盘，后台运行',
};