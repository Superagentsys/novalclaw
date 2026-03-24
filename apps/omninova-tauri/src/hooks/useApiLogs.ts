/**
 * API Logs Hook (Story 8.4)
 *
 * Provides state management for API request logs.
 */

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type {
  ApiRequestLog,
  RequestLogFilter,
  ApiUsageStats,
  EndpointStats,
  ApiKeyStats,
} from '../types/api-log';
import { TIME_RANGE_PRESETS } from '../types/api-log';

/**
 * API Logs hook state
 */
export function useApiLogs() {
  const [logs, setLogs] = useState<ApiRequestLog[]>([]);
  const [totalCount, setTotalCount] = useState<number>(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [initialized, setInitialized] = useState(false);

  /**
   * Initialize the API log store
   */
  const init = useCallback(async () => {
    try {
      await invoke('init_api_log_store');
      setInitialized(true);
    } catch (err) {
      console.error('Failed to initialize API log store:', err);
      setError(`Failed to initialize: ${err}`);
    }
  }, []);

  // Initialize on mount
  useEffect(() => {
    init();
  }, [init]);

  /**
   * Fetch logs with filter and pagination
   */
  const fetchLogs = useCallback(async (
    filter: RequestLogFilter = {},
    limit: number = 100,
    offset: number = 0
  ) => {
    if (!initialized) return;

    setLoading(true);
    setError(null);

    try {
      const [logsResult, countResult] = await Promise.all([
        invoke<ApiRequestLog[]>('list_api_logs', {
          filter,
          limit,
          offset,
        }),
        invoke<number>('count_api_logs', { filter }),
      ]);

      setLogs(logsResult);
      setTotalCount(countResult);
    } catch (err) {
      console.error('Failed to fetch API logs:', err);
      setError(`Failed to fetch logs: ${err}`);
    } finally {
      setLoading(false);
    }
  }, [initialized]);

  /**
   * Clear logs before a timestamp
   */
  const clearLogsBefore = useCallback(async (beforeTimestamp: number): Promise<number> => {
    try {
      const count = await invoke<number>('clear_api_logs', { beforeTimestamp });
      return count;
    } catch (err) {
      console.error('Failed to clear API logs:', err);
      throw err;
    }
  }, []);

  /**
   * Clear all logs
   */
  const clearAllLogs = useCallback(async (): Promise<number> => {
    try {
      const count = await invoke<number>('clear_all_api_logs');
      setLogs([]);
      setTotalCount(0);
      return count;
    } catch (err) {
      console.error('Failed to clear all API logs:', err);
      throw err;
    }
  }, []);

  return {
    logs,
    totalCount,
    loading,
    error,
    initialized,
    fetchLogs,
    clearLogsBefore,
    clearAllLogs,
  };
}

/**
 * API Usage Statistics hook
 */
export function useApiUsageStats() {
  const [stats, setStats] = useState<ApiUsageStats | null>(null);
  const [endpointStats, setEndpointStats] = useState<EndpointStats[]>([]);
  const [apiKeyStats, setApiKeyStats] = useState<ApiKeyStats[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /**
   * Fetch usage statistics for a time range
   */
  const fetchStats = useCallback(async (startTime: number, endTime: number) => {
    setLoading(true);
    setError(null);

    try {
      const [usageStats, endpoints, keys] = await Promise.all([
        invoke<ApiUsageStats>('get_api_usage_stats', { startTime, endTime }),
        invoke<EndpointStats[]>('get_endpoint_stats', { startTime, endTime, limit: 20 }),
        invoke<ApiKeyStats[]>('get_api_key_stats', { startTime, endTime, limit: 20 }),
      ]);

      setStats(usageStats);
      setEndpointStats(endpoints);
      setApiKeyStats(keys);
    } catch (err) {
      console.error('Failed to fetch API usage stats:', err);
      setError(`Failed to fetch stats: ${err}`);
    } finally {
      setLoading(false);
    }
  }, []);

  return {
    stats,
    endpointStats,
    apiKeyStats,
    loading,
    error,
    fetchStats,
  };
}

/**
 * API Log Export hook
 */
export function useApiLogExport() {
  const [exporting, setExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /**
   * Export logs to JSON or CSV
   */
  const exportLogs = useCallback(async (
    filter: RequestLogFilter = {},
    format: 'json' | 'csv' = 'json'
  ): Promise<string> => {
    setExporting(true);
    setError(null);

    try {
      const result = await invoke<string>('export_api_logs', {
        filter,
        format,
      });
      return result;
    } catch (err) {
      console.error('Failed to export API logs:', err);
      setError(`Failed to export: ${err}`);
      throw err;
    } finally {
      setExporting(false);
    }
  }, []);

  /**
   * Download exported logs as a file
   */
  const downloadExport = useCallback(async (
    filter: RequestLogFilter = {},
    format: 'json' | 'csv' = 'json',
    filename: string = 'api_logs'
  ) => {
    const content = await exportLogs(filter, format);
    const mimeType = format === 'json' ? 'application/json' : 'text/csv';
    const blob = new Blob([content], { type: mimeType });
    const url = URL.createObjectURL(blob);

    const a = document.createElement('a');
    a.href = url;
    a.download = `${filename}.${format}`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }, [exportLogs]);

  return {
    exporting,
    error,
    exportLogs,
    downloadExport,
  };
}

/**
 * Time range selector hook
 */
export function useTimeRangeSelector() {
  const [startTime, setStartTime] = useState<number>(() => {
    const end = Math.floor(Date.now() / 1000);
    return end - 86400; // Default: last 24 hours
  });
  const [endTime, setEndTime] = useState<number>(() => {
    return Math.floor(Date.now() / 1000);
  });
  const [preset, setPreset] = useState<string>('last_24_hours');

  /**
   * Apply a preset time range
   */
  const applyPreset = useCallback((presetKey: keyof typeof TIME_RANGE_PRESETS) => {
    const { start, end } = TIME_RANGE_PRESETS[presetKey]();
    setStartTime(start);
    setEndTime(end);
    setPreset(presetKey);
  }, []);

  /**
   * Set custom time range
   */
  const setCustomRange = useCallback((start: number, end: number) => {
    setStartTime(start);
    setEndTime(end);
    setPreset('custom');
  }, []);

  return {
    startTime,
    endTime,
    preset,
    applyPreset,
    setCustomRange,
  };
}

export default useApiLogs;