/**
 * System monitoring hook
 *
 * Provides real-time system resource monitoring data
 *
 * [Source: Story 9-1 - System Resource Monitor]
 */

import { useState, useEffect, useCallback } from 'react';

// ============================================================================
// Types
// ============================================================================

export interface DiskUsage {
  name: string;
  total_gb: number;
  used_gb: number;
  available_gb: number;
  usage_percent: number;
}

export interface SystemResources {
  timestamp: number;
  cpu: {
    usage_percent: number;
  };
  memory: {
    used_mb: number;
    total_mb: number;
    usage_percent: number;
    warning: boolean;
    warning_threshold_mb: number;
  };
  disks: DiskUsage[];
}

export interface HistoryEntry {
  timestamp: number;
  value: number;
}

export interface SystemHistory {
  cpu: HistoryEntry[];
  memory: HistoryEntry[];
}

// ============================================================================
// Hook
// ============================================================================

/**
 * Hook to fetch system resource data
 */
export function useSystemResources(refreshInterval = 5000) {
  const [resources, setResources] = useState<SystemResources | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchResources = useCallback(async () => {
    try {
      const response = await fetch('/api/system/resources');
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }
      const data = await response.json();
      setResources(data);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to fetch resources');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchResources();
    const interval = setInterval(fetchResources, refreshInterval);
    return () => clearInterval(interval);
  }, [fetchResources, refreshInterval]);

  return { resources, loading, error, refetch: fetchResources };
}

/**
 * Hook to fetch system resource history
 */
export function useSystemHistory(refreshInterval = 30000) {
  const [history, setHistory] = useState<SystemHistory | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchHistory = useCallback(async () => {
    try {
      const response = await fetch('/api/system/history');
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }
      const data = await response.json();
      setHistory(data);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to fetch history');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchHistory();
    const interval = setInterval(fetchHistory, refreshInterval);
    return () => clearInterval(interval);
  }, [fetchHistory, refreshInterval]);

  return { history, loading, error, refetch: fetchHistory };
}

/**
 * Hook to export system data
 */
export function useSystemExport() {
  const [exporting, setExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const exportData = useCallback(async (format: 'json' | 'csv' = 'json') => {
    setExporting(true);
    setError(null);
    try {
      const response = await fetch(`/api/system/export?format=${format}`);
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }
      const data = await response.text();
      
      // Create download
      const blob = new Blob([data], { 
        type: format === 'json' ? 'application/json' : 'text/csv' 
      });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `system-resources.${format}`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      
      return data;
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Export failed';
      setError(message);
      throw err;
    } finally {
      setExporting(false);
    }
  }, []);

  return { exportData, exporting, error };
}