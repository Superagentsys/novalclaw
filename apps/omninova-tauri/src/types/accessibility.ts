/**
 * Accessibility Types
 *
 * Type definitions for accessibility settings.
 *
 * [Source: Story 10.7 - 无障碍访问]
 */

// ============================================================================
// Types
// ============================================================================

/**
 * 无障碍设置
 */
export interface AccessibilitySettings {
  /** 启用高对比度模式 */
  highContrast: boolean;
  /** 启用大字体模式 */
  largeText: boolean;
  /** 启用减少动画 */
  reduceMotion: boolean;
  /** 界面缩放比例 */
  zoomLevel: number;
  /** 启用屏幕阅读器优化 */
  screenReaderMode: boolean;
}

// ============================================================================
// Constants
// ============================================================================

/**
 * 默认无障碍设置
 */
export const DEFAULT_ACCESSIBILITY: AccessibilitySettings = {
  highContrast: false,
  largeText: false,
  reduceMotion: false,
  zoomLevel: 100,
  screenReaderMode: false,
};

/**
 * 缩放级别选项
 */
export const ZOOM_LEVELS = [75, 90, 100, 110, 125, 150, 175, 200] as const;

export type ZoomLevel = (typeof ZOOM_LEVELS)[number];