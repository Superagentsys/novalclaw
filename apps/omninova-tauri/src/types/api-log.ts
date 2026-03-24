/**
 * API Log Types (Story 8.4)
 *
 * Types for API request logging and usage statistics.
 */

/**
 * API Request Log entry
 */
export interface ApiRequestLog {
  id: number;
  timestamp: number;
  method: string;
  endpoint: string;
  status_code: number;
  response_time_ms: number;
  api_key_id?: number;
  ip_address?: string;
  user_agent?: string;
  request_size?: number;
  response_size?: number;
}

/**
 * Request Log Filter for querying logs
 */
export interface RequestLogFilter {
  start_time?: number;
  end_time?: number;
  endpoint?: string;
  method?: string;
  status_code?: number;
  api_key_id?: number;
  min_response_time?: number;
  max_response_time?: number;
}

/**
 * API Usage Statistics
 */
export interface ApiUsageStats {
  total_requests: number;
  successful_requests: number;
  client_errors: number;
  server_errors: number;
  avg_response_time_ms: number;
  max_response_time_ms: number;
  min_response_time_ms: number;
  error_rate: number;
  time_range: TimeRange;
}

/**
 * Time range for statistics
 */
export interface TimeRange {
  start: number;
  end: number;
}

/**
 * Per-endpoint statistics
 */
export interface EndpointStats {
  endpoint: string;
  method: string;
  request_count: number;
  avg_response_time_ms: number;
  error_count: number;
  error_rate: number;
}

/**
 * Per-API-key statistics
 */
export interface ApiKeyStats {
  api_key_id: number;
  key_name: string;
  request_count: number;
  avg_response_time_ms: number;
  error_count: number;
  error_rate: number;
  last_request_at: number | null;
}

/**
 * HTTP Methods for filtering
 */
export type HttpMethod = 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH' | 'HEAD' | 'OPTIONS';

/**
 * Status code categories
 */
export type StatusCodeCategory = '2xx' | '3xx' | '4xx' | '5xx';

/**
 * Helper functions
 */

/**
 * Format timestamp to readable date string
 */
export function formatLogTimestamp(timestamp: number): string {
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
 * Format response time with appropriate unit
 */
export function formatResponseTime(ms: number): string {
  if (ms < 1000) {
    return `${ms}ms`;
  } else if (ms < 60000) {
    return `${(ms / 1000).toFixed(2)}s`;
  } else {
    return `${(ms / 60000).toFixed(2)}m`;
  }
}

/**
 * Get status code category
 */
export function getStatusCodeCategory(code: number): StatusCodeCategory {
  if (code >= 200 && code < 300) return '2xx';
  if (code >= 300 && code < 400) return '3xx';
  if (code >= 400 && code < 500) return '4xx';
  return '5xx';
}

/**
 * Get status code color class
 */
export function getStatusCodeColor(code: number): string {
  const category = getStatusCodeCategory(code);
  switch (category) {
    case '2xx':
      return 'bg-green-500';
    case '3xx':
      return 'bg-blue-500';
    case '4xx':
      return 'bg-yellow-500';
    case '5xx':
      return 'bg-red-500';
  }
}

/**
 * Get method color class
 */
export function getMethodColor(method: string): string {
  switch (method.toUpperCase()) {
    case 'GET':
      return 'bg-green-100 text-green-800';
    case 'POST':
      return 'bg-blue-100 text-blue-800';
    case 'PUT':
      return 'bg-yellow-100 text-yellow-800';
    case 'DELETE':
      return 'bg-red-100 text-red-800';
    case 'PATCH':
      return 'bg-purple-100 text-purple-800';
    default:
      return 'bg-gray-100 text-gray-800';
  }
}

/**
 * Get error rate color class
 */
export function getErrorRateColor(rate: number): string {
  if (rate < 0.01) return 'text-green-600';
  if (rate < 0.05) return 'text-yellow-600';
  if (rate < 0.1) return 'text-orange-600';
  return 'text-red-600';
}

/**
 * Default time range presets (in seconds)
 */
export const TIME_RANGE_PRESETS = {
  last_hour: () => {
    const end = Math.floor(Date.now() / 1000);
    const start = end - 3600;
    return { start, end };
  },
  last_24_hours: () => {
    const end = Math.floor(Date.now() / 1000);
    const start = end - 86400;
    return { start, end };
  },
  last_7_days: () => {
    const end = Math.floor(Date.now() / 1000);
    const start = end - 604800;
    return { start, end };
  },
  last_30_days: () => {
    const end = Math.floor(Date.now() / 1000);
    const start = end - 2592000;
    return { start, end };
  },
};

export type TimeRangePreset = keyof typeof TIME_RANGE_PRESETS;