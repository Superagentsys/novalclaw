/**
 * Agent Performance Metrics Types
 *
 * Defines types for agent performance monitoring, including:
 * - Response time tracking
 * - Success rate statistics
 * - Provider performance comparison
 * - Time series data for charts
 *
 * [Source: Story 9.2 - 代理性能监控]
 */

// ============================================================================
// Core Types
// ============================================================================

/**
 * Single request performance record
 */
export interface AgentRequestMetric {
  /** Agent identifier */
  agentId: string;
  /** Provider name (e.g., "openai", "anthropic") */
  provider: string;
  /** Model name */
  model: string;
  /** Unix timestamp in seconds */
  timestamp: number;
  /** Response time in milliseconds */
  responseTimeMs: number;
  /** Whether the request succeeded */
  success: boolean;
  /** Error code if failed */
  errorCode?: string;
  /** Input token count (if available) */
  tokensInput?: number;
  /** Output token count (if available) */
  tokensOutput?: number;
}

/**
 * Aggregated agent performance statistics
 */
export interface AgentPerformanceStats {
  /** Agent identifier */
  agentId: string;
  /** Agent name (for display) */
  agentName?: string;
  /** Total number of requests */
  totalRequests: number;
  /** Successful requests count */
  successfulRequests: number;
  /** Failed requests count */
  failedRequests: number;
  /** Average response time in milliseconds */
  avgResponseTimeMs: number;
  /** Minimum response time in milliseconds */
  minResponseTimeMs: number;
  /** Maximum response time in milliseconds */
  maxResponseTimeMs: number;
  /** Success rate (0-100) */
  successRate: number;
  /** P50 (median) response time in milliseconds */
  p50ResponseTimeMs: number;
  /** P95 response time in milliseconds */
  p95ResponseTimeMs: number;
  /** P99 response time in milliseconds */
  p99ResponseTimeMs: number;
}

/**
 * Provider performance statistics for comparison
 */
export interface ProviderPerformanceStats {
  /** Provider name */
  provider: string;
  /** Total number of requests */
  totalRequests: number;
  /** Average response time in milliseconds */
  avgResponseTimeMs: number;
  /** Success rate (0-100) */
  successRate: number;
}

/**
 * Time series data point for charts
 */
export interface MetricDataPoint {
  /** Unix timestamp in seconds */
  timestamp: number;
  /** Metric value */
  value: number;
}

/**
 * Time range for queries
 */
export interface TimeRange {
  /** Start timestamp (Unix seconds) */
  from: number;
  /** End timestamp (Unix seconds) */
  to: number;
}

/**
 * Metric type for time series queries
 */
export type MetricType = 'response_time' | 'success_rate' | 'request_count';

// ============================================================================
// API Response Types
// ============================================================================

/**
 * API response wrapper for metrics data
 */
export interface MetricsApiResponse<T> {
  /** Response data */
  data: T;
  /** Response timestamp */
  timestamp: string;
}

/**
 * Time series API response
 */
export interface TimeSeriesApiResponse {
  /** Data points */
  data: MetricDataPoint[];
  /** Agent ID */
  agentId: string;
  /** Metric type */
  metricType: MetricType;
  /** Interval in seconds */
  intervalSeconds: number;
  /** Response timestamp */
  timestamp: string;
}

// ============================================================================
// Constants
// ============================================================================

/**
 * Response time warning threshold in milliseconds
 */
export const RESPONSE_TIME_WARNING_THRESHOLD_MS = 3000;

/**
 * Success rate warning threshold (percentage)
 */
export const SUCCESS_RATE_WARNING_THRESHOLD = 95;

/**
 * Preset time ranges for quick selection
 */
export const TIME_RANGE_PRESETS = [
  { label: '最近 15 分钟', value: 15 * 60 },
  { label: '最近 1 小时', value: 60 * 60 },
  { label: '最近 6 小时', value: 6 * 60 * 60 },
  { label: '最近 24 小时', value: 24 * 60 * 60 },
  { label: '最近 7 天', value: 7 * 24 * 60 * 60 },
] as const;

/**
 * Metric type labels
 */
export const METRIC_TYPE_LABELS: Record<MetricType, string> = {
  response_time: '响应时间',
  success_rate: '成功率',
  request_count: '请求数量',
};

/**
 * Provider display names
 */
export const PROVIDER_DISPLAY_NAMES: Record<string, string> = {
  openai: 'OpenAI',
  anthropic: 'Anthropic',
  ollama: 'Ollama',
  deepseek: 'DeepSeek',
  google: 'Google AI',
  azure: 'Azure OpenAI',
};

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Get default time range (last 1 hour)
 */
export function getDefaultTimeRange(): TimeRange {
  const now = Math.floor(Date.now() / 1000);
  return {
    from: now - 3600,
    to: now,
  };
}

/**
 * Check if response time is above warning threshold
 */
export function isResponseTimeWarning(responseTimeMs: number): boolean {
  return responseTimeMs > RESPONSE_TIME_WARNING_THRESHOLD_MS;
}

/**
 * Check if success rate is below warning threshold
 */
export function isSuccessRateWarning(successRate: number): boolean {
  return successRate < SUCCESS_RATE_WARNING_THRESHOLD;
}

/**
 * Format response time for display
 */
export function formatResponseTime(ms: number): string {
  if (ms < 1000) {
    return `${ms}ms`;
  }
  return `${(ms / 1000).toFixed(2)}s`;
}

/**
 * Format success rate for display
 */
export function formatSuccessRate(rate: number): string {
  return `${rate.toFixed(1)}%`;
}

/**
 * Get provider display name
 */
export function getProviderDisplayName(provider: string): string {
  return PROVIDER_DISPLAY_NAMES[provider] || provider;
}