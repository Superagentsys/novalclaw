/**
 * Provider Comparison - Display provider performance comparison
 *
 * Shows a comparison view with:
 * - Provider name
 * - Total requests
 * - Average response time
 * - Success rate
 *
 * [Source: Story 9.2 - 代理性能监控]
 */

import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { useMetricsStore } from '@/stores/metricsStore';
import {
  formatResponseTime,
  formatSuccessRate,
  isResponseTimeWarning,
  isSuccessRateWarning,
  getProviderDisplayName,
} from '@/types/metrics';

interface ProviderComparisonProps {
  isLoading?: boolean;
}

export function ProviderComparison({ isLoading }: ProviderComparisonProps) {
  const { providerStats } = useMetricsStore();

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-8">
        <div className="text-muted-foreground">加载中...</div>
      </div>
    );
  }

  if (providerStats.length === 0) {
    return (
      <div className="flex items-center justify-center py-8">
        <div className="text-muted-foreground">暂无提供商数据</div>
      </div>
    );
  }

  // Sort by average response time (fastest first)
  const sortedStats = [...providerStats].sort(
    (a, b) => a.avgResponseTimeMs - b.avgResponseTimeMs
  );

  return (
    <div className="space-y-4">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>提供商</TableHead>
            <TableHead className="text-right">请求数</TableHead>
            <TableHead className="text-right">平均响应时间</TableHead>
            <TableHead className="text-right">成功率</TableHead>
            <TableHead className="w-32">性能</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {sortedStats.map((stat) => {
            const responseWarning = isResponseTimeWarning(stat.avgResponseTimeMs);
            const successWarning = isSuccessRateWarning(stat.successRate);

            // Calculate performance score (0-100)
            // Higher success rate and lower response time = better score
            const responseScore = Math.max(
              0,
              100 - (stat.avgResponseTimeMs / 3000) * 50
            );
            const performanceScore = (stat.successRate * 0.7 + responseScore * 0.3);

            return (
              <TableRow key={stat.provider}>
                <TableCell className="font-medium">
                  {getProviderDisplayName(stat.provider)}
                </TableCell>
                <TableCell className="text-right">
                  {stat.totalRequests.toLocaleString()}
                </TableCell>
                <TableCell className="text-right">
                  <span className={responseWarning ? 'text-amber-600 font-medium' : ''}>
                    {formatResponseTime(stat.avgResponseTimeMs)}
                  </span>
                  {responseWarning && (
                    <Badge variant="outline" className="ml-2 text-amber-600 border-amber-600">
                      慢
                    </Badge>
                  )}
                </TableCell>
                <TableCell className="text-right">
                  <span className={successWarning ? 'text-red-600 font-medium' : 'text-green-600'}>
                    {formatSuccessRate(stat.successRate)}
                  </span>
                </TableCell>
                <TableCell>
                  <div className="flex items-center gap-2">
                    <Progress
                      value={performanceScore}
                      className="h-2"
                    />
                    <span className="text-xs text-muted-foreground w-8">
                      {Math.round(performanceScore)}
                    </span>
                  </div>
                </TableCell>
              </TableRow>
            );
          })}
        </TableBody>
      </Table>

      {/* Summary Cards */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mt-6">
        {sortedStats.slice(0, 3).map((stat, index) => (
          <div
            key={stat.provider}
            className="p-4 border rounded-lg bg-card"
          >
            <div className="flex items-center justify-between mb-2">
              <span className="font-medium">
                {getProviderDisplayName(stat.provider)}
              </span>
              {index === 0 && (
                <Badge className="bg-green-600">最快</Badge>
              )}
            </div>
            <div className="text-sm text-muted-foreground">
              平均响应: {formatResponseTime(stat.avgResponseTimeMs)}
            </div>
            <div className="text-sm text-muted-foreground">
              成功率: {formatSuccessRate(stat.successRate)}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

export default ProviderComparison;