/**
 * Notification Types
 *
 * Type definitions for the notification management system.
 *
 * [Source: Story 9.3 - 系统通知管理]
 */

// ============================================================================
// Enums
// ============================================================================

/**
 * Notification type enumeration
 */
export type NotificationType =
  | 'agent_response'
  | 'error'
  | 'system_update'
  | 'channel_message'
  | 'performance_warning'
  | 'custom';

/**
 * Notification priority levels
 */
export type NotificationPriority = 'low' | 'normal' | 'high' | 'urgent';

// ============================================================================
// Interfaces
// ============================================================================

/**
 * Notification record
 */
export interface Notification {
  /** Unique identifier */
  id: string;
  /** Notification type */
  notificationType: NotificationType;
  /** Notification title */
  title: string;
  /** Notification body content */
  body: string;
  /** Priority level */
  priority: NotificationPriority;
  /** Creation timestamp (Unix seconds) */
  createdAt: number;
  /** Whether notification has been read */
  read: boolean;
  /** Associated metadata */
  metadata?: Record<string, string>;
}

/**
 * Notification configuration
 */
export interface NotificationConfig {
  /** Whether desktop notifications are enabled */
  enabled: boolean;
  /** Enabled notification types */
  enabledTypes: NotificationType[];
  /** Whether notification sound is enabled */
  soundEnabled: boolean;
  /** Quiet hours start time (hour 0-23) */
  quietHoursStart?: number;
  /** Quiet hours end time (hour 0-23) */
  quietHoursEnd?: number;
}

// ============================================================================
// Constants
// ============================================================================

/**
 * Notification type labels (Chinese)
 */
export const NOTIFICATION_TYPE_LABELS: Record<NotificationType, string> = {
  agent_response: '代理响应',
  error: '错误通知',
  system_update: '系统更新',
  channel_message: '渠道消息',
  performance_warning: '性能警告',
  custom: '自定义',
};

/**
 * Notification type descriptions
 */
export const NOTIFICATION_TYPE_DESCRIPTIONS: Record<NotificationType, string> = {
  agent_response: 'AI 代理完成响应时通知',
  error: '系统错误时通知',
  system_update: '系统更新可用时通知',
  channel_message: '收到渠道消息时通知',
  performance_warning: '性能问题警告时通知',
  custom: '自定义通知',
};

/**
 * Priority labels (Chinese)
 */
export const PRIORITY_LABELS: Record<NotificationPriority, string> = {
  low: '低',
  normal: '普通',
  high: '高',
  urgent: '紧急',
};

/**
 * All notification types
 */
export const ALL_NOTIFICATION_TYPES: NotificationType[] = [
  'agent_response',
  'error',
  'system_update',
  'channel_message',
  'performance_warning',
  'custom',
];

/**
 * Default notification configuration
 */
export const DEFAULT_NOTIFICATION_CONFIG: NotificationConfig = {
  enabled: true,
  enabledTypes: ['error', 'system_update', 'performance_warning'],
  soundEnabled: true,
  quietHoursStart: 22,
  quietHoursEnd: 8,
};

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Format timestamp to localized string
 */
export function formatNotificationTime(timestamp: number): string {
  const date = new Date(timestamp * 1000);
  const now = new Date();
  const diff = now.getTime() - date.getTime();

  // Less than 1 minute
  if (diff < 60 * 1000) {
    return '刚刚';
  }

  // Less than 1 hour
  if (diff < 60 * 60 * 1000) {
    const minutes = Math.floor(diff / (60 * 1000));
    return `${minutes} 分钟前`;
  }

  // Less than 24 hours
  if (diff < 24 * 60 * 60 * 1000) {
    const hours = Math.floor(diff / (60 * 60 * 1000));
    return `${hours} 小时前`;
  }

  // Format as date
  return date.toLocaleDateString('zh-CN', {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

/**
 * Check if time is within quiet hours
 */
export function isInQuietHours(config: NotificationConfig): boolean {
  if (config.quietHoursStart === undefined || config.quietHoursEnd === undefined) {
    return false;
  }

  const now = new Date();
  const hour = now.getHours();

  const start = config.quietHoursStart;
  const end = config.quietHoursEnd;

  if (start < end) {
    // Same day (e.g., 12:00 - 14:00)
    return hour >= start && hour < end;
  } else {
    // Across midnight (e.g., 22:00 - 08:00)
    return hour >= start || hour < end;
  }
}

/**
 * Format quiet hours range for display
 */
export function formatQuietHours(start?: number, end?: number): string {
  if (start === undefined || end === undefined) {
    return '未设置';
  }
  return `${start.toString().padStart(2, '0')}:00 - ${end.toString().padStart(2, '0')}:00`;
}