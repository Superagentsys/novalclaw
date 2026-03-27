/**
 * Log Types
 *
 * Type definitions for the log viewer system.
 *
 * [Source: Story 9.4 - 日志查看器实现]
 */

// ============================================================================
// Enums
// ============================================================================

/**
 * Log level enumeration
 */
export type LogLevel = 'ERROR' | 'WARN' | 'INFO' | 'DEBUG' | 'TRACE';

// ============================================================================
// Interfaces
// ============================================================================

/**
 * Log entry
 */
export interface LogEntry {
  /** Timestamp (Unix seconds) */
  timestamp: number;
  /** Log level */
  level: LogLevel;
  /** Source module/target */
  target: string;
  /** Log message content */
  message: string;
}

/**
 * Log query parameters
 */
export interface LogQuery {
  /** Start time (Unix seconds) */
  startTime?: number;
  /** End time (Unix seconds) */
  endTime?: number;
  /** Log level filter */
  levels?: LogLevel[];
  /** Keyword search (case-insensitive) */
  keyword?: string;
  /** Pagination offset */
  offset?: number;
  /** Pagination limit */
  limit?: number;
}

/**
 * Log statistics
 */
export interface LogStats {
  /** Log file size in bytes */
  fileSize: number;
  /** Total entry count */
  entryCount: number;
  /** Oldest entry timestamp */
  oldestEntry?: number;
  /** Newest entry timestamp */
  newestEntry?: number;
}

// ============================================================================
// Constants
// ============================================================================

/**
 * Log level labels (Chinese)
 */
export const LOG_LEVEL_LABELS: Record<LogLevel, string> = {
  ERROR: '错误',
  WARN: '警告',
  INFO: '信息',
  DEBUG: '调试',
  TRACE: '追踪',
};

/**
 * Log level colors
 */
export const LOG_LEVEL_COLORS: Record<LogLevel, string> = {
  ERROR: 'text-red-600 bg-red-50',
  WARN: 'text-amber-600 bg-amber-50',
  INFO: 'text-blue-600 bg-blue-50',
  DEBUG: 'text-gray-600 bg-gray-50',
  TRACE: 'text-purple-600 bg-purple-50',
};

/**
 * All log levels
 */
export const ALL_LOG_LEVELS: LogLevel[] = ['ERROR', 'WARN', 'INFO', 'DEBUG', 'TRACE'];

/**
 * Default query limit
 */
export const DEFAULT_PAGE_SIZE = 100;

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Format timestamp to localized string
 */
export function formatLogTime(timestamp: number): string {
  const date = new Date(timestamp * 1000);
  return date.toLocaleString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

/**
 * Format file size to human readable string
 */
export function formatFileSize(bytes: number): string {
  if (bytes === 0) return '0 B';

  const units = ['B', 'KB', 'MB', 'GB'];
  const k = 1024;
  const i = Math.floor(Math.log(bytes) / Math.log(k));

  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${units[i]}`;
}

/**
 * Get relative time string
 */
export function getRelativeTime(timestamp: number): string {
  const now = Date.now() / 1000;
  const diff = now - timestamp;

  if (diff < 60) {
    return '刚刚';
  } else if (diff < 3600) {
    const minutes = Math.floor(diff / 60);
    return `${minutes} 分钟前`;
  } else if (diff < 86400) {
    const hours = Math.floor(diff / 3600);
    return `${hours} 小时前`;
  } else {
    const days = Math.floor(diff / 86400);
    return `${days} 天前`;
  }
}