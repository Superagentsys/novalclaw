/**
 * Startup Types
 *
 * Type definitions for startup performance tracking.
 *
 * [Source: Story 9.6 - 应用启动优化]
 */

// ============================================================================
// Types
// ============================================================================

/**
 * 启动阶段
 */
export type StartupPhase = 'initializing' | 'loading-config' | 'loading-ui' | 'ready';

/**
 * 启动里程碑
 */
export interface StartupMilestone {
  /** 里程碑名称 */
  name: string;
  /** 相对于启动的时间（秒） */
  elapsed_seconds: number;
}

/**
 * 启动报告
 */
export interface StartupReport {
  /** 总启动时间（秒） */
  total_seconds: number;
  /** 各阶段里程碑 */
  milestones: StartupMilestone[];
  /** 是否已完成启动 */
  is_ready: boolean;
}

/**
 * 启动进度状态
 */
export interface StartupProgress {
  /** 当前阶段 */
  phase: StartupPhase;
  /** 进度消息 */
  message: string;
  /** 进度百分比 (0-100) */
  progress: number;
}

// ============================================================================
// Constants
// ============================================================================

/**
 * 启动阶段标签
 */
export const STARTUP_PHASE_LABELS: Record<StartupPhase, string> = {
  initializing: '初始化中',
  'loading-config': '加载配置',
  'loading-ui': '加载界面',
  ready: '准备就绪',
};

/**
 * 启动阶段默认消息
 */
export const STARTUP_PHASE_MESSAGES: Record<StartupPhase, string> = {
  initializing: '正在初始化应用...',
  'loading-config': '正在加载配置文件...',
  'loading-ui': '正在加载用户界面...',
  ready: '应用已准备就绪',
};

/**
 * 启动阶段进度映射
 */
export const STARTUP_PHASE_PROGRESS: Record<StartupPhase, number> = {
  initializing: 25,
  'loading-config': 50,
  'loading-ui': 75,
  ready: 100,
};