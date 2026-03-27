/**
 * Metrics Store - Agent Performance Monitoring State Management
 *
 * Centralizes metrics state for performance monitoring, enabling:
 * - Agent performance statistics
 * - Provider performance comparison
 * - Time series data for charts
 * - Time range filtering
 *
 * [Source: Story 9.2 - 代理性能监控]
 */

import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';
import type {
  AgentPerformanceStats,
  ProviderPerformanceStats,
  MetricDataPoint,
  TimeRange,
  MetricType,
  MetricsApiResponse,
  TimeSeriesApiResponse,
  getDefaultTimeRange,
} from '@/types/metrics';

// ============================================================================
// API Functions
// ============================================================================

const GATEWAY_BASE = 'http://localhost:3000';

/**
 * Fetch all agent performance statistics
 */
async function fetchAgentStats(
  timeRange?: TimeRange
): Promise<AgentPerformanceStats[]> {
  const params = new URLSearchParams();
  if (timeRange) {
    params.set('from', timeRange.from.toString());
    params.set('to', timeRange.to.toString());
  }

  const url = `${GATEWAY_BASE}/api/metrics/agents${params.toString() ? `?${params}` : ''}`;
  const response = await fetch(url);

  if (!response.ok) {
    throw new Error(`Failed to fetch agent stats: ${response.statusText}`);
  }

  const data: MetricsApiResponse<AgentPerformanceStats[]> = await response.json();
  return data.data;
}

/**
 * Fetch single agent performance statistics
 */
async function fetchAgentStatsById(
  agentId: string,
  timeRange?: TimeRange
): Promise<AgentPerformanceStats> {
  const params = new URLSearchParams();
  if (timeRange) {
    params.set('from', timeRange.from.toString());
    params.set('to', timeRange.to.toString());
  }

  const url = `${GATEWAY_BASE}/api/metrics/agents/${agentId}${params.toString() ? `?${params}` : ''}`;
  const response = await fetch(url);

  if (!response.ok) {
    throw new Error(`Failed to fetch agent stats: ${response.statusText}`);
  }

  const data: MetricsApiResponse<AgentPerformanceStats> = await response.json();
  return data.data;
}

/**
 * Fetch provider performance statistics
 */
async function fetchProviderStats(
  timeRange?: TimeRange
): Promise<ProviderPerformanceStats[]> {
  const params = new URLSearchParams();
  if (timeRange) {
    params.set('from', timeRange.from.toString());
    params.set('to', timeRange.to.toString());
  }

  const url = `${GATEWAY_BASE}/api/metrics/providers${params.toString() ? `?${params}` : ''}`;
  const response = await fetch(url);

  if (!response.ok) {
    throw new Error(`Failed to fetch provider stats: ${response.statusText}`);
  }

  const data: MetricsApiResponse<ProviderPerformanceStats[]> = await response.json();
  return data.data;
}

/**
 * Fetch time series data for an agent
 */
async function fetchTimeSeries(
  agentId: string,
  metricType: MetricType,
  timeRange?: TimeRange,
  intervalSeconds?: number
): Promise<MetricDataPoint[]> {
  const params = new URLSearchParams();
  params.set('metric', metricType);
  if (timeRange) {
    params.set('from', timeRange.from.toString());
    params.set('to', timeRange.to.toString());
  }
  if (intervalSeconds) {
    params.set('interval', intervalSeconds.toString());
  }

  const url = `${GATEWAY_BASE}/api/metrics/agents/${agentId}/timeseries?${params}`;
  const response = await fetch(url);

  if (!response.ok) {
    throw new Error(`Failed to fetch time series: ${response.statusText}`);
  }

  const data: TimeSeriesApiResponse = await response.json();
  return data.data;
}

// ============================================================================
// Store Types
// ============================================================================

export interface MetricsState {
  /** Agent performance statistics */
  agentStats: AgentPerformanceStats[];
  /** Provider performance statistics */
  providerStats: ProviderPerformanceStats[];
  /** Time series data */
  timeSeries: MetricDataPoint[];
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: string | null;
  /** Selected time range */
  timeRange: TimeRange;
  /** Selected agent ID for detailed view */
  selectedAgentId: string | null;
  /** Selected metric type for time series */
  selectedMetricType: MetricType;
}

export interface MetricsActions {
  /** Fetch all agent statistics */
  fetchAgentStats: () => Promise<void>;
  /** Fetch single agent statistics */
  fetchAgentStatsById: (agentId: string) => Promise<AgentPerformanceStats | null>;
  /** Fetch provider statistics */
  fetchProviderStats: () => Promise<void>;
  /** Fetch time series data */
  fetchTimeSeries: (agentId: string, metricType?: MetricType) => Promise<void>;
  /** Set time range */
  setTimeRange: (range: TimeRange) => void;
  /** Set selected agent */
  setSelectedAgent: (agentId: string | null) => void;
  /** Set selected metric type */
  setMetricType: (type: MetricType) => void;
  /** Clear error */
  clearError: () => void;
  /** Refresh all data */
  refresh: () => Promise<void>;
}

export type MetricsStore = MetricsState & MetricsActions;

// ============================================================================
// Store Creation
// ============================================================================

/**
 * Get default time range (last 1 hour)
 */
function getDefaultTimeRange(): TimeRange {
  const now = Math.floor(Date.now() / 1000);
  return {
    from: now - 3600,
    to: now,
  };
}

/**
 * Metrics store with Zustand
 *
 * Uses subscribeWithSelector for fine-grained subscriptions
 *
 * @example
 * ```tsx
 * // In a component
 * const agentStats = useMetricsStore((state) => state.agentStats);
 * const fetchAgentStats = useMetricsStore((state) => state.fetchAgentStats);
 *
 * // Or get entire store
 * const { agentStats, fetchAgentStats, isLoading } = useMetricsStore();
 * ```
 */
export const useMetricsStore = create<MetricsStore>()(
  subscribeWithSelector((set, get) => ({
    // Initial state
    agentStats: [],
    providerStats: [],
    timeSeries: [],
    isLoading: false,
    error: null,
    timeRange: getDefaultTimeRange(),
    selectedAgentId: null,
    selectedMetricType: 'response_time',

    // Actions
    fetchAgentStats: async () => {
      const { timeRange } = get();
      set({ isLoading: true, error: null });

      try {
        const stats = await fetchAgentStats(timeRange);
        set({ agentStats: stats, isLoading: false });
      } catch (err) {
        const message = err instanceof Error ? err.message : '获取代理统计失败';
        set({ error: message, isLoading: false });
        console.error('Failed to fetch agent stats:', err);
      }
    },

    fetchAgentStatsById: async (agentId: string) => {
      const { timeRange } = get();
      set({ isLoading: true, error: null });

      try {
        const stats = await fetchAgentStatsById(agentId, timeRange);
        set({ isLoading: false });
        return stats;
      } catch (err) {
        const message = err instanceof Error ? err.message : '获取代理统计失败';
        set({ error: message, isLoading: false });
        console.error('Failed to fetch agent stats by ID:', err);
        return null;
      }
    },

    fetchProviderStats: async () => {
      const { timeRange } = get();
      set({ isLoading: true, error: null });

      try {
        const stats = await fetchProviderStats(timeRange);
        set({ providerStats: stats, isLoading: false });
      } catch (err) {
        const message = err instanceof Error ? err.message : '获取提供商统计失败';
        set({ error: message, isLoading: false });
        console.error('Failed to fetch provider stats:', err);
      }
    },

    fetchTimeSeries: async (agentId: string, metricType?: MetricType) => {
      const { timeRange, selectedMetricType } = get();
      const type = metricType || selectedMetricType;

      set({ isLoading: true, error: null, selectedAgentId: agentId });

      try {
        const data = await fetchTimeSeries(agentId, type, timeRange);
        set({ timeSeries: data, selectedMetricType: type, isLoading: false });
      } catch (err) {
        const message = err instanceof Error ? err.message : '获取时间序列数据失败';
        set({ error: message, isLoading: false });
        console.error('Failed to fetch time series:', err);
      }
    },

    setTimeRange: (range: TimeRange) => {
      set({ timeRange: range });
      // Auto-refresh data when time range changes
      get().refresh();
    },

    setSelectedAgent: (agentId: string | null) => {
      set({ selectedAgentId: agentId });
    },

    setMetricType: (type: MetricType) => {
      set({ selectedMetricType: type });
    },

    clearError: () => {
      set({ error: null });
    },

    refresh: async () => {
      const { fetchAgentStats, fetchProviderStats, selectedAgentId, selectedMetricType } = get();

      // Fetch all stats in parallel
      await Promise.all([
        fetchAgentStats(),
        fetchProviderStats(),
        selectedAgentId ? get().fetchTimeSeries(selectedAgentId, selectedMetricType) : Promise.resolve(),
      ]);
    },
  }))
);

export default useMetricsStore;