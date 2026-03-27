/**
 * Agent Performance Panel - Main component for performance monitoring
 *
 * Displays agent performance metrics including:
 * - Agent statistics table
 * - Provider comparison
 * - Time range filtering
 *
 * [Source: Story 9.2 - 代理性能监控]
 */

import { useEffect } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useMetricsStore } from '@/stores/metricsStore';
import { AgentStatsTable } from './AgentStatsTable';
import { ProviderComparison } from './ProviderComparison';
import { TimeRangeSelector } from './TimeRangeSelector';

export function AgentPerformancePanel() {
  const {
    isLoading,
    error,
    fetchAgentStats,
    fetchProviderStats,
    clearError,
  } = useMetricsStore();

  // Fetch data on mount
  useEffect(() => {
    fetchAgentStats();
    fetchProviderStats();
  }, [fetchAgentStats, fetchProviderStats]);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold tracking-tight">代理性能监控</h2>
          <p className="text-muted-foreground">
            监控 AI 代理的响应时间、成功率和性能趋势
          </p>
        </div>
        <TimeRangeSelector />
      </div>

      {error && (
        <Card className="border-destructive">
          <CardContent className="pt-6">
            <div className="flex items-center justify-between">
              <p className="text-destructive">{error}</p>
              <button
                onClick={clearError}
                className="text-sm text-muted-foreground hover:text-foreground"
              >
                关闭
              </button>
            </div>
          </CardContent>
        </Card>
      )}

      <Tabs defaultValue="agents" className="space-y-4">
        <TabsList>
          <TabsTrigger value="agents">代理统计</TabsTrigger>
          <TabsTrigger value="providers">提供商对比</TabsTrigger>
        </TabsList>

        <TabsContent value="agents" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>代理性能统计</CardTitle>
              <CardDescription>
                查看每个代理的平均响应时间、成功率和请求统计
              </CardDescription>
            </CardHeader>
            <CardContent>
              <AgentStatsTable isLoading={isLoading} />
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="providers" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>提供商性能对比</CardTitle>
              <CardDescription>
                比较不同 LLM 提供商的响应时间和成功率
              </CardDescription>
            </CardHeader>
            <CardContent>
              <ProviderComparison isLoading={isLoading} />
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}

export default AgentPerformancePanel;