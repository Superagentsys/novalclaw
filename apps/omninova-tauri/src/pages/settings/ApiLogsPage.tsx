/**
 * API Logs Settings Page (Story 8.4)
 *
 * Provides UI for viewing API request logs, statistics, and managing log retention.
 */

import { type FC, useState, useCallback, useEffect } from 'react';
import {
  Activity,
  Clock,
  Download,
  Trash2,
  RefreshCw,
  AlertCircle,
  TrendingUp,
  BarChart3,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription } from '@/components/ui/alert';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  useApiLogs,
  useApiUsageStats,
  useApiLogExport,
  useTimeRangeSelector,
} from '@/hooks/useApiLogs';
import type {
  ApiRequestLog,
  RequestLogFilter,
  EndpointStats,
} from '@/types/api-log';
import {
  formatLogTimestamp,
  formatResponseTime,
  getStatusCodeColor,
  getMethodColor,
  getErrorRateColor,
  TIME_RANGE_PRESETS,
} from '@/types/api-log';

/**
 * Statistics card component
 */
const StatCard: FC<{
  title: string;
  value: string | number;
  subtitle?: string;
  icon: FC<{ className?: string }>;
  valueClassName?: string;
}> = ({ title, value, subtitle, icon: Icon, valueClassName }) => (
  <Card>
    <CardContent className="pt-6">
      <div className="flex items-center justify-between">
        <div>
          <p className="text-sm font-medium text-muted-foreground">{title}</p>
          <p className={`text-2xl font-bold ${valueClassName || ''}`}>{value}</p>
          {subtitle && <p className="text-xs text-muted-foreground">{subtitle}</p>}
        </div>
        <Icon className="h-8 w-8 text-muted-foreground" />
      </div>
    </CardContent>
  </Card>
);

/**
 * Log entry row component
 */
const LogEntryRow: FC<{ log: ApiRequestLog }> = ({ log }) => (
  <tr className="border-b hover:bg-muted/50">
    <td className="py-2 px-3 text-sm font-mono text-muted-foreground">
      {formatLogTimestamp(log.timestamp)}
    </td>
    <td className="py-2 px-3">
      <Badge className={getMethodColor(log.method)} variant="outline">
        {log.method}
      </Badge>
    </td>
    <td className="py-2 px-3 text-sm font-mono truncate max-w-[200px]" title={log.endpoint}>
      {log.endpoint}
    </td>
    <td className="py-2 px-3">
      <Badge className={`${getStatusCodeColor(log.status_code)} text-white`}>
        {log.status_code}
      </Badge>
    </td>
    <td className="py-2 px-3 text-sm text-muted-foreground">
      {formatResponseTime(log.response_time_ms)}
    </td>
    <td className="py-2 px-3 text-sm text-muted-foreground">
      {log.api_key_id ? `Key #${log.api_key_id}` : '-'}
    </td>
    <td className="py-2 px-3 text-sm text-muted-foreground truncate max-w-[150px]" title={log.ip_address}>
      {log.ip_address || '-'}
    </td>
  </tr>
);

/**
 * Endpoint stats row component
 */
const EndpointStatsRow: FC<{ stats: EndpointStats }> = ({ stats }) => (
  <tr className="border-b hover:bg-muted/50">
    <td className="py-2 px-3">
      <Badge className={getMethodColor(stats.method)} variant="outline">
        {stats.method}
      </Badge>
    </td>
    <td className="py-2 px-3 text-sm font-mono truncate max-w-[200px]" title={stats.endpoint}>
      {stats.endpoint}
    </td>
    <td className="py-2 px-3 text-sm">{stats.request_count}</td>
    <td className="py-2 px-3 text-sm text-muted-foreground">
      {formatResponseTime(Math.round(stats.avg_response_time_ms))}
    </td>
    <td className="py-2 px-3 text-sm">{stats.error_count}</td>
    <td className={`py-2 px-3 text-sm font-medium ${getErrorRateColor(stats.error_rate)}`}>
      {(stats.error_rate * 100).toFixed(1)}%
    </td>
  </tr>
);

/**
 * API Logs Page Component
 */
export const ApiLogsPage: FC = () => {
  const {
    logs,
    totalCount,
    loading,
    error,
    initialized,
    fetchLogs,
    clearLogsBefore,
    clearAllLogs,
  } = useApiLogs();

  const {
    stats,
    endpointStats,
    loading: statsLoading,
    fetchStats,
  } = useApiUsageStats();

  const { exporting, downloadExport } = useApiLogExport();

  const { startTime, endTime, preset, applyPreset } = useTimeRangeSelector();

  // Filter state
  const [filter] = useState<RequestLogFilter>({});
  const [methodFilter, setMethodFilter] = useState<string>('all');
  const [statusFilter] = useState<string>('all');

  // Pagination
  const [page, setPage] = useState(0);
  const pageSize = 50;

  // Active tab: 'logs' | 'stats'
  const [activeTab, setActiveTab] = useState<'logs' | 'stats'>('logs');

  // Fetch logs when filter or page changes
  useEffect(() => {
    if (!initialized) return;

    const currentFilter: RequestLogFilter = {
      ...filter,
      start_time: startTime,
      end_time: endTime,
    };

    if (methodFilter !== 'all') {
      currentFilter.method = methodFilter;
    }
    if (statusFilter !== 'all') {
      const statusCode = parseInt(statusFilter, 10);
      if (!isNaN(statusCode)) {
        currentFilter.status_code = statusCode;
      }
    }

    fetchLogs(currentFilter, pageSize, page * pageSize);
  }, [initialized, filter, startTime, endTime, methodFilter, statusFilter, page, fetchLogs]);

  // Fetch stats when time range changes
  useEffect(() => {
    if (!initialized) return;
    fetchStats(startTime, endTime);
  }, [initialized, startTime, endTime, fetchStats]);

  // Handle time range preset change
  const handlePresetChange = useCallback((value: string) => {
    if (value in TIME_RANGE_PRESETS) {
      applyPreset(value as keyof typeof TIME_RANGE_PRESETS);
      setPage(0);
    }
  }, [applyPreset]);

  // Handle export
  const handleExport = useCallback(async (format: 'json' | 'csv') => {
    const exportFilter: RequestLogFilter = {
      ...filter,
      start_time: startTime,
      end_time: endTime,
    };
    await downloadExport(exportFilter, format, `api_logs_${preset}`);
  }, [filter, startTime, endTime, preset, downloadExport]);

  // Handle clear old logs
  const handleClearOldLogs = useCallback(async () => {
    if (!confirm('确定要清理所选时间范围之前的所有日志吗？此操作无法撤销。')) {
      return;
    }
    try {
      const count = await clearLogsBefore(startTime);
      alert(`已清理 ${count} 条日志`);
      // Refresh
      fetchLogs({ ...filter, start_time: startTime, end_time: endTime }, pageSize, page * pageSize);
      fetchStats(startTime, endTime);
    } catch (err) {
      alert(`清理失败: ${err}`);
    }
  }, [clearLogsBefore, startTime, filter, fetchLogs, fetchStats, endTime, page]);

  // Handle clear all logs
  const handleClearAllLogs = useCallback(async () => {
    if (!confirm('确定要清理所有日志吗？此操作无法撤销。')) {
      return;
    }
    try {
      const count = await clearAllLogs();
      alert(`已清理 ${count} 条日志`);
    } catch (err) {
      alert(`清理失败: ${err}`);
    }
  }, [clearAllLogs]);

  if (!initialized) {
    return (
      <div className="container mx-auto py-6 px-4 max-w-6xl">
        <div className="flex items-center justify-center py-12">
          <RefreshCw className="h-6 w-6 animate-spin mr-2" />
          <span>正在初始化...</span>
        </div>
      </div>
    );
  }

  return (
    <div className="container mx-auto py-6 px-4 max-w-6xl">
      <div className="space-y-6">
        {/* Header */}
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-bold">API 使用日志</h1>
            <p className="text-muted-foreground">
              查看 API 请求记录、统计分析和日志管理
            </p>
          </div>
          <div className="flex items-center gap-2">
            <Select value={preset} onValueChange={(value) => value && handlePresetChange(value)}>
              <SelectTrigger className="w-[180px]">
                <Clock className="h-4 w-4 mr-2" />
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="last_hour">最近 1 小时</SelectItem>
                <SelectItem value="last_24_hours">最近 24 小时</SelectItem>
                <SelectItem value="last_7_days">最近 7 天</SelectItem>
                <SelectItem value="last_30_days">最近 30 天</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>

        {/* Error Alert */}
        {error && (
          <Alert variant="destructive">
            <AlertCircle className="h-4 w-4" />
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}

        {/* Statistics Summary */}
        {stats && (
          <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
            <StatCard
              title="总请求数"
              value={stats.total_requests.toLocaleString()}
              icon={Activity}
            />
            <StatCard
              title="平均响应时间"
              value={formatResponseTime(Math.round(stats.avg_response_time_ms))}
              icon={Clock}
            />
            <StatCard
              title="错误率"
              value={`${(stats.error_rate * 100).toFixed(1)}%`}
              icon={TrendingUp}
              valueClassName={getErrorRateColor(stats.error_rate)}
            />
            <StatCard
              title="成功率"
              value={`${((1 - stats.error_rate) * 100).toFixed(1)}%`}
              icon={BarChart3}
              valueClassName="text-green-600"
            />
          </div>
        )}

        {/* Tab Navigation */}
        <div className="flex gap-2 border-b">
          <Button
            variant={activeTab === 'logs' ? 'default' : 'ghost'}
            onClick={() => setActiveTab('logs')}
          >
            请求日志
          </Button>
          <Button
            variant={activeTab === 'stats' ? 'default' : 'ghost'}
            onClick={() => setActiveTab('stats')}
          >
            端点统计
          </Button>
        </div>

        {activeTab === 'logs' && (
          <Card>
            <CardHeader>
              <div className="flex items-center justify-between">
                <div>
                  <CardTitle>请求日志</CardTitle>
                  <CardDescription>
                    共 {totalCount.toLocaleString()} 条记录
                  </CardDescription>
                </div>
                <div className="flex items-center gap-2">
                  <Select value={methodFilter} onValueChange={(value) => value && setMethodFilter(value)}>
                    <SelectTrigger className="w-[100px]">
                      <SelectValue placeholder="方法" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="all">全部方法</SelectItem>
                      <SelectItem value="GET">GET</SelectItem>
                      <SelectItem value="POST">POST</SelectItem>
                      <SelectItem value="PUT">PUT</SelectItem>
                      <SelectItem value="DELETE">DELETE</SelectItem>
                    </SelectContent>
                  </Select>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => handleExport('json')}
                    disabled={exporting}
                  >
                    <Download className="h-4 w-4 mr-1" />
                    导出
                  </Button>
                </div>
              </div>
            </CardHeader>
            <CardContent>
              {loading ? (
                <div className="flex items-center justify-center py-8">
                  <RefreshCw className="h-6 w-6 animate-spin mr-2" />
                  <span>加载中...</span>
                </div>
              ) : logs.length === 0 ? (
                <div className="text-center py-8 text-muted-foreground">
                  没有找到日志记录
                </div>
              ) : (
                <div className="overflow-x-auto">
                  <table className="w-full">
                    <thead>
                      <tr className="border-b">
                        <th className="py-2 px-3 text-left text-sm font-medium">时间</th>
                        <th className="py-2 px-3 text-left text-sm font-medium">方法</th>
                        <th className="py-2 px-3 text-left text-sm font-medium">端点</th>
                        <th className="py-2 px-3 text-left text-sm font-medium">状态</th>
                        <th className="py-2 px-3 text-left text-sm font-medium">响应时间</th>
                        <th className="py-2 px-3 text-left text-sm font-medium">API Key</th>
                        <th className="py-2 px-3 text-left text-sm font-medium">IP</th>
                      </tr>
                    </thead>
                    <tbody>
                      {logs.map((log) => (
                        <LogEntryRow key={log.id} log={log} />
                      ))}
                    </tbody>
                  </table>
                </div>
              )}

              {/* Pagination */}
              {totalCount > pageSize && (
                <div className="flex items-center justify-between mt-4">
                  <p className="text-sm text-muted-foreground">
                    显示 {page * pageSize + 1} - {Math.min((page + 1) * pageSize, totalCount)} / {totalCount}
                  </p>
                  <div className="flex gap-2">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => setPage(Math.max(0, page - 1))}
                      disabled={page === 0}
                    >
                      上一页
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => setPage(page + 1)}
                      disabled={(page + 1) * pageSize >= totalCount}
                    >
                      下一页
                    </Button>
                  </div>
                </div>
              )}
            </CardContent>
          </Card>
        )}

        {activeTab === 'stats' && (
          <Card>
            <CardHeader>
              <CardTitle>端点统计</CardTitle>
              <CardDescription>
                按端点分组的请求统计
              </CardDescription>
            </CardHeader>
            <CardContent>
              {statsLoading ? (
                <div className="flex items-center justify-center py-8">
                  <RefreshCw className="h-6 w-6 animate-spin mr-2" />
                  <span>加载中...</span>
                </div>
              ) : endpointStats.length === 0 ? (
                <div className="text-center py-8 text-muted-foreground">
                  没有统计数据
                </div>
              ) : (
                <div className="overflow-x-auto">
                  <table className="w-full">
                    <thead>
                      <tr className="border-b">
                        <th className="py-2 px-3 text-left text-sm font-medium">方法</th>
                        <th className="py-2 px-3 text-left text-sm font-medium">端点</th>
                        <th className="py-2 px-3 text-left text-sm font-medium">请求数</th>
                        <th className="py-2 px-3 text-left text-sm font-medium">平均响应时间</th>
                        <th className="py-2 px-3 text-left text-sm font-medium">错误数</th>
                        <th className="py-2 px-3 text-left text-sm font-medium">错误率</th>
                      </tr>
                    </thead>
                    <tbody>
                      {endpointStats.map((stat, index) => (
                        <EndpointStatsRow key={`${stat.method}-${stat.endpoint}-${index}`} stats={stat} />
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </CardContent>
          </Card>
        )}

        {/* Log Management */}
        <Card>
          <CardHeader>
            <CardTitle>日志管理</CardTitle>
            <CardDescription>
              清理和导出 API 日志
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="flex flex-wrap gap-2">
              <Button
                variant="outline"
                onClick={handleClearOldLogs}
                disabled={loading}
              >
                <Trash2 className="h-4 w-4 mr-1" />
                清理旧日志
              </Button>
              <Button
                variant="destructive"
                onClick={handleClearAllLogs}
                disabled={loading}
              >
                <Trash2 className="h-4 w-4 mr-1" />
                清理所有日志
              </Button>
              <Button
                variant="outline"
                onClick={() => handleExport('json')}
                disabled={exporting}
              >
                <Download className="h-4 w-4 mr-1" />
                导出 JSON
              </Button>
              <Button
                variant="outline"
                onClick={() => handleExport('csv')}
                disabled={exporting}
              >
                <Download className="h-4 w-4 mr-1" />
                导出 CSV
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
};

export default ApiLogsPage;