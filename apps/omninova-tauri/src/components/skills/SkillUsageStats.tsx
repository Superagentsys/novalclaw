/**
 * SkillUsageStats 组件
 *
 * 显示技能使用统计信息
 *
 * [Source: Story 7.6 - 技能管理界面]
 */

import { type FC, useMemo } from 'react';
import {
  BarChart3,
  TrendingUp,
  TrendingDown,
  Clock,
  CheckCircle2,
  XCircle,
  Zap,
} from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import {
  type SkillMetadata,
  type SkillUsageStatistics,
} from '@/types/skill';

export interface SkillUsageStatsProps {
  /** List of skills */
  skills: SkillMetadata[];
  /** Enabled skill IDs */
  enabledSkillIds?: Set<string>;
  /** Usage statistics map */
  usageStatsMap?: Map<string, SkillUsageStatistics>;
}

/**
 * Format duration for display
 */
function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  return `${(ms / 60000).toFixed(1)}m`;
}

/**
 * Calculate success rate
 */
function calculateSuccessRate(stats: SkillUsageStatistics): number {
  if (stats.totalExecutions === 0) return 0;
  return Math.round((stats.successCount / stats.totalExecutions) * 100);
}

/**
 * SkillUsageStats component
 */
export const SkillUsageStats: FC<SkillUsageStatsProps> = ({
  skills,
  enabledSkillIds = new Set(),
  usageStatsMap = new Map(),
}) => {
  // Calculate overall statistics
  const overallStats = useMemo(() => {
    let totalExecutions = 0;
    let totalSuccess = 0;
    let totalFailures = 0;
    let totalDuration = 0;
    let executionCount = 0;

    usageStatsMap.forEach(stats => {
      totalExecutions += stats.totalExecutions;
      totalSuccess += stats.successCount;
      totalFailures += stats.failureCount;
      if (stats.avgDurationMs > 0) {
        totalDuration += stats.avgDurationMs;
        executionCount++;
      }
    });

    const avgDuration = executionCount > 0 ? totalDuration / executionCount : 0;
    const successRate = totalExecutions > 0
      ? Math.round((totalSuccess / totalExecutions) * 100)
      : 0;

    return {
      totalExecutions,
      totalSuccess,
      totalFailures,
      avgDuration,
      successRate,
    };
  }, [usageStatsMap]);

  // Get top used skills
  const topSkills = useMemo(() => {
    const skillStats: Array<{
      skill: SkillMetadata;
      stats?: SkillUsageStatistics;
    }> = skills.map(skill => ({
      skill,
      stats: usageStatsMap.get(skill.id),
    }));

    return skillStats
      .filter(item => item.stats && item.stats.totalExecutions > 0)
      .sort((a, b) => (b.stats?.totalExecutions || 0) - (a.stats?.totalExecutions || 0))
      .slice(0, 5);
  }, [skills, usageStatsMap]);

  // Get recently executed skills
  const recentSkills = useMemo(() => {
    const skillStats: Array<{
      skill: SkillMetadata;
      stats?: SkillUsageStatistics;
    }> = skills.map(skill => ({
      skill,
      stats: usageStatsMap.get(skill.id),
    }));

    return skillStats
      .filter(item => item.stats?.lastExecutedAt)
      .sort((a, b) => {
        const aTime = a.stats?.lastExecutedAt ? new Date(a.stats.lastExecutedAt).getTime() : 0;
        const bTime = b.stats?.lastExecutedAt ? new Date(b.stats.lastExecutedAt).getTime() : 0;
        return bTime - aTime;
      })
      .slice(0, 5);
  }, [skills, usageStatsMap]);

  if (usageStatsMap.size === 0) {
    return (
      <div className="text-center py-12 text-muted-foreground">
        <BarChart3 className="h-12 w-12 mx-auto mb-4 opacity-50" />
        <p>暂无使用统计数据</p>
        <p className="text-sm mt-2">启用并使用技能后将显示统计信息</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Overview Stats */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <Card>
          <CardContent className="pt-4">
            <div className="flex items-center gap-3">
              <div className="p-2 bg-blue-100 dark:bg-blue-900/30 rounded-lg">
                <Zap className="h-4 w-4 text-blue-600 dark:text-blue-400" />
              </div>
              <div>
                <p className="text-2xl font-bold">{overallStats.totalExecutions}</p>
                <p className="text-xs text-muted-foreground">总执行次数</p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-4">
            <div className="flex items-center gap-3">
              <div className="p-2 bg-green-100 dark:bg-green-900/30 rounded-lg">
                <CheckCircle2 className="h-4 w-4 text-green-600 dark:text-green-400" />
              </div>
              <div>
                <p className="text-2xl font-bold">{overallStats.totalSuccess}</p>
                <p className="text-xs text-muted-foreground">成功次数</p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-4">
            <div className="flex items-center gap-3">
              <div className="p-2 bg-red-100 dark:bg-red-900/30 rounded-lg">
                <XCircle className="h-4 w-4 text-red-600 dark:text-red-400" />
              </div>
              <div>
                <p className="text-2xl font-bold">{overallStats.totalFailures}</p>
                <p className="text-xs text-muted-foreground">失败次数</p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-4">
            <div className="flex items-center gap-3">
              <div className="p-2 bg-purple-100 dark:bg-purple-900/30 rounded-lg">
                <Clock className="h-4 w-4 text-purple-600 dark:text-purple-400" />
              </div>
              <div>
                <p className="text-2xl font-bold">{formatDuration(overallStats.avgDuration)}</p>
                <p className="text-xs text-muted-foreground">平均耗时</p>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Success Rate */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-medium">整体成功率</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center gap-4">
            <div className="flex-1">
              <div className="h-4 bg-muted rounded-full overflow-hidden">
                <div
                  className={`h-full transition-all ${
                    overallStats.successRate >= 80
                      ? 'bg-green-500'
                      : overallStats.successRate >= 50
                      ? 'bg-yellow-500'
                      : 'bg-red-500'
                  }`}
                  style={{ width: `${overallStats.successRate}%` }}
                />
              </div>
            </div>
            <div className="flex items-center gap-2">
              {overallStats.successRate >= 80 ? (
                <TrendingUp className="h-4 w-4 text-green-500" />
              ) : (
                <TrendingDown className="h-4 w-4 text-red-500" />
              )}
              <span className="font-bold">{overallStats.successRate}%</span>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Top Used Skills */}
      {topSkills.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium">最常用技能</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-3">
              {topSkills.map((item, index) => {
                const successRate = item.stats ? calculateSuccessRate(item.stats) : 0;
                return (
                  <div
                    key={item.skill.id}
                    className="flex items-center justify-between"
                  >
                    <div className="flex items-center gap-3">
                      <span className="text-muted-foreground w-6">#{index + 1}</span>
                      <div>
                        <p className="font-medium">{item.skill.name}</p>
                        <p className="text-xs text-muted-foreground">
                          {item.stats?.totalExecutions} 次执行
                        </p>
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      <Badge
                        variant={successRate >= 80 ? 'success' : successRate >= 50 ? 'warning' : 'error'}
                      >
                        {successRate}%
                      </Badge>
                      {enabledSkillIds.has(item.skill.id) && (
                        <Badge variant="outline">已启用</Badge>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Recently Executed */}
      {recentSkills.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium">最近执行</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-3">
              {recentSkills.map(item => (
                <div
                  key={item.skill.id}
                  className="flex items-center justify-between"
                >
                  <div>
                    <p className="font-medium">{item.skill.name}</p>
                    <p className="text-xs text-muted-foreground">
                      {item.stats?.lastExecutedAt && (
                        <>
                          {new Date(item.stats.lastExecutedAt).toLocaleString('zh-CN')}
                          {' · '}
                          {formatDuration(item.stats.avgDurationMs)}
                        </>
                      )}
                    </p>
                  </div>
                  <Badge variant={item.stats?.success ? 'success' : 'error'}>
                    {item.stats?.success ? '成功' : '失败'}
                  </Badge>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
};

export default SkillUsageStats;