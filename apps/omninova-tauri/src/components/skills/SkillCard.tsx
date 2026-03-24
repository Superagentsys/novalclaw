/**
 * SkillCard 组件
 *
 * 显示单个技能的信息卡片，包括名称、描述、版本和启用/禁用开关
 *
 * [Source: Story 7.6 - 技能管理界面]
 */

import { type FC, useMemo } from 'react';
import {
  Settings,
  ExternalLink,
  Clock,
  CheckCircle2,
  XCircle,
  Zap,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Switch } from '@/components/ui/switch';
import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from '@/components/ui/card';
import {
  type SkillMetadata,
  type SkillUsageStatistics,
} from '@/types/skill';

export interface SkillCardProps {
  /** Skill metadata */
  skill: SkillMetadata;
  /** Whether the skill is enabled */
  enabled?: boolean;
  /** Callback when enabled state changes */
  onToggle?: (enabled: boolean) => void;
  /** Callback when configure button is clicked */
  onConfigure?: () => void;
  /** Whether to show usage statistics */
  showStats?: boolean;
  /** Usage statistics data */
  usageStats?: SkillUsageStatistics;
  /** Whether an operation is in progress */
  isLoading?: boolean;
}

/**
 * Format duration in milliseconds to human readable string
 */
function formatDuration(ms: number): string {
  if (ms < 1000) {
    return `${ms}ms`;
  }
  return `${(ms / 1000).toFixed(2)}s`;
}

/**
 * Calculate success rate percentage
 */
function calculateSuccessRate(stats: SkillUsageStatistics): number {
  if (stats.totalExecutions === 0) return 0;
  return Math.round((stats.successCount / stats.totalExecutions) * 100);
}

/**
 * SkillCard component
 */
export const SkillCard: FC<SkillCardProps> = ({
  skill,
  enabled = false,
  onToggle,
  onConfigure,
  showStats = false,
  usageStats,
  isLoading = false,
}) => {
  const {
    id,
    name,
    version,
    description,
    author,
    tags,
    dependencies,
    isBuiltin,
    homepage,
  } = skill;

  const successRate = usageStats ? calculateSuccessRate(usageStats) : 0;
  const hasConfig = skill.configSchema && Object.keys(skill.configSchema).length > 0;

  // Group tags by category for display
  const tagGroups = useMemo(() => {
    const groups: Record<string, string[]> = {};
    tags.forEach(tag => {
      groups[tag] = groups[tag] || [];
      groups[tag].push(tag);
    });
    return groups;
  }, [tags]);

  return (
    <Card className="group relative">
      <CardHeader>
        <div className="flex items-start justify-between gap-2">
          <div className="flex-1 min-w-0">
            <CardTitle className="flex items-center gap-2">
              <Zap className="h-4 w-4 text-primary shrink-0" />
              <span className="truncate">{name}</span>
            </CardTitle>
            <CardDescription className="mt-1 line-clamp-2">
              {description}
            </CardDescription>
          </div>
          {onToggle && (
            <Switch
              checked={enabled}
              onCheckedChange={onToggle}
              disabled={isLoading}
            />
          )}
        </div>
      </CardHeader>

      <CardContent className="space-y-3">
        {/* Tags */}
        <div className="flex flex-wrap gap-1.5">
          {isBuiltin && (
            <Badge variant="secondary" className="text-xs">
              内置
            </Badge>
          )}
          {tags.slice(0, 4).map(tag => (
            <Badge key={tag} variant="outline" className="text-xs">
              {tag}
            </Badge>
          ))}
          {tags.length > 4 && (
            <Badge variant="outline" className="text-xs">
              +{tags.length - 4}
            </Badge>
          )}
        </div>

        {/* Version and Author */}
        <div className="flex items-center gap-4 text-xs text-muted-foreground">
          <span>v{version}</span>
          {author && (
            <>
              <span className="text-border">|</span>
              <span>{author}</span>
            </>
          )}
        </div>

        {/* Dependencies indicator */}
        {dependencies.length > 0 && (
          <div className="text-xs text-muted-foreground">
            依赖: {dependencies.length} 个技能
          </div>
        )}

        {/* Usage Statistics */}
        {showStats && usageStats && (
          <div className="pt-2 border-t space-y-2">
            <div className="flex items-center justify-between text-sm">
              <span className="text-muted-foreground">执行次数</span>
              <span className="font-medium">{usageStats.totalExecutions}</span>
            </div>
            <div className="flex items-center justify-between text-sm">
              <span className="text-muted-foreground">成功率</span>
              <div className="flex items-center gap-1.5">
                {usageStats.totalExecutions > 0 ? (
                  <>
                    {successRate >= 80 ? (
                      <CheckCircle2 className="h-3.5 w-3.5 text-green-500" />
                    ) : successRate >= 50 ? (
                      <CheckCircle2 className="h-3.5 w-3.5 text-yellow-500" />
                    ) : (
                      <XCircle className="h-3.5 w-3.5 text-red-500" />
                    )}
                    <span className="font-medium">{successRate}%</span>
                  </>
                ) : (
                  <span className="text-muted-foreground">-</span>
                )}
              </div>
            </div>
            {usageStats.avgDurationMs > 0 && (
              <div className="flex items-center justify-between text-sm">
                <span className="text-muted-foreground">平均耗时</span>
                <div className="flex items-center gap-1.5">
                  <Clock className="h-3.5 w-3.5 text-muted-foreground" />
                  <span className="font-medium">{formatDuration(usageStats.avgDurationMs)}</span>
                </div>
              </div>
            )}
            {usageStats.lastExecutedAt && (
              <div className="text-xs text-muted-foreground">
                最后执行: {new Date(usageStats.lastExecutedAt).toLocaleString('zh-CN')}
              </div>
            )}
          </div>
        )}
      </CardContent>

      {/* Actions */}
      <CardFooter className="gap-2">
        {hasConfig && onConfigure && (
          <Button
            variant="outline"
            size="sm"
            onClick={onConfigure}
            disabled={!enabled || isLoading}
          >
            <Settings className="h-3.5 w-3.5 mr-1.5" />
            配置
          </Button>
        )}
        {homepage && (
          <Button
            variant="ghost"
            size="sm"
            asChild
          >
            <a href={homepage} target="_blank" rel="noopener noreferrer">
              <ExternalLink className="h-3.5 w-3.5 mr-1.5" />
              文档
            </a>
          </Button>
        )}
      </CardFooter>
    </Card>
  );
};

export default SkillCard;