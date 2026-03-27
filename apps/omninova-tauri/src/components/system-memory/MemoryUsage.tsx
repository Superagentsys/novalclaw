/**
 * Memory Usage Component
 *
 * Displays system memory usage and provides cache management.
 *
 * [Source: Story 9.7 - 内存使用优化]
 */

import { useEffect } from 'react';
import { useSystemMemoryStore } from '@/stores/systemMemoryStore';
import {
  formatSystemBytes,
  getSystemMemoryStatusColor,
} from '@/types/memory';
import { Progress } from '@/components/ui/progress';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

interface MemoryUsageProps {
  /** Additional CSS classes */
  className?: string;
}

/**
 * Memory usage display component
 */
export function MemoryUsage({ className }: MemoryUsageProps) {
  const { stats, isLoading, loadStats, clearCache } = useSystemMemoryStore();

  // Load stats on mount and periodically
  useEffect(() => {
    void loadStats();
    const interval = setInterval(() => void loadStats(), 30000); // Every 30s
    return () => clearInterval(interval);
  }, [loadStats]);

  const handleClearCache = async () => {
    await clearCache();
  };

  if (!stats) {
    return (
      <div className={cn('p-4', className)}>
        <p className="text-sm text-muted-foreground">加载中...</p>
      </div>
    );
  }

  return (
    <div className={cn('space-y-4', className)}>
      <h3 className="text-sm font-medium text-foreground">内存使用</h3>

      {/* Memory Usage Bar */}
      <div className="space-y-2">
        <div className="flex justify-between text-sm">
          <span className="text-muted-foreground">已使用</span>
          <span className={getSystemMemoryStatusColor(stats.usagePercent)}>
            {formatSystemBytes(stats.usedBytes)}
          </span>
        </div>
        <Progress
          value={stats.usagePercent}
          className="h-2"
        />
        <div className="flex justify-between text-xs text-muted-foreground">
          <span>{stats.usagePercent.toFixed(1)}%</span>
          <span>可用: {formatSystemBytes(stats.availableBytes)}</span>
        </div>
      </div>

      {/* Clear Cache Button */}
      <Button
        variant="outline"
        size="sm"
        onClick={handleClearCache}
        disabled={isLoading}
        className="w-full"
      >
        {isLoading ? '清理中...' : '清理缓存'}
      </Button>
    </div>
  );
}

export default MemoryUsage;