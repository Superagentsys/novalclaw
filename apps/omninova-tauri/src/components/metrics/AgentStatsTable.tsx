/**
 * Agent Stats Table - Display agent performance statistics
 *
 * Shows a table with:
 * - Agent name
 * - Total/successful/failed requests
 * - Average, P50, P95, P99 response times
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
import { useMetricsStore } from '@/stores/metricsStore';
import {
  formatResponseTime,
  formatSuccessRate,
  isResponseTimeWarning,
  isSuccessRateWarning,
} from '@/types/metrics';

interface AgentStatsTableProps {
  isLoading?: boolean;
}

export function AgentStatsTable({ isLoading }: AgentStatsTableProps) {
  const { agentStats } = useMetricsStore();

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-8">
        <div className="text-muted-foreground">加载中...</div>
      </div>
    );
  }

  if (agentStats.length === 0) {
    return (
      <div className="flex items-center justify-center py-8">
        <div className="text-muted-foreground">暂无性能数据</div>
      </div>
    );
  }

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>代理</TableHead>
          <TableHead className="text-right">请求数</TableHead>
          <TableHead className="text-right">成功</TableHead>
          <TableHead className="text-right">失败</TableHead>
          <TableHead className="text-right">平均响应</TableHead>
          <TableHead className="text-right">P50</TableHead>
          <TableHead className="text-right">P95</TableHead>
          <TableHead className="text-right">P99</TableHead>
          <TableHead className="text-right">成功率</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {agentStats.map((stat) => {
          const responseWarning = isResponseTimeWarning(stat.avgResponseTimeMs);
          const successWarning = isSuccessRateWarning(stat.successRate);

          return (
            <TableRow key={stat.agentId}>
              <TableCell className="font-medium">
                {stat.agentName || stat.agentId}
              </TableCell>
              <TableCell className="text-right">
                {stat.totalRequests.toLocaleString()}
              </TableCell>
              <TableCell className="text-right text-green-600">
                {stat.successfulRequests.toLocaleString()}
              </TableCell>
              <TableCell className="text-right text-red-600">
                {stat.failedRequests.toLocaleString()}
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
                {formatResponseTime(stat.p50ResponseTimeMs)}
              </TableCell>
              <TableCell className="text-right">
                {formatResponseTime(stat.p95ResponseTimeMs)}
              </TableCell>
              <TableCell className="text-right">
                {formatResponseTime(stat.p99ResponseTimeMs)}
              </TableCell>
              <TableCell className="text-right">
                <span className={successWarning ? 'text-red-600 font-medium' : 'text-green-600'}>
                  {formatSuccessRate(stat.successRate)}
                </span>
                {successWarning && (
                  <Badge variant="outline" className="ml-2 text-red-600 border-red-600">
                    低
                  </Badge>
                )}
              </TableCell>
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
}

export default AgentStatsTable;