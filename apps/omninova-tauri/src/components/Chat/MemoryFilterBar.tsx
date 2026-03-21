/**
 * Memory Filter Bar Component
 *
 * Provides filtering controls for memory visualization including
 * layer selection, time range, importance, and search.
 *
 * [Source: Story 5.7 - MemoryVisualization 组件]
 */

import { memo, useCallback } from 'react';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { MemoryLayer } from '@/types/memory';

// ============================================================================
// Types
// ============================================================================

/**
 * Time range filter options
 */
export type TimeRange = 'today' | 'week' | 'month' | 'all';

/**
 * Props for MemoryFilterBar component
 */
export interface MemoryFilterBarProps {
  /** Current layer filter */
  layer: MemoryLayer;
  /** Layer change callback */
  onLayerChange: (layer: MemoryLayer) => void;
  /** Time range filter */
  timeRange?: TimeRange;
  /** Time range change callback */
  onTimeRangeChange?: (range: TimeRange) => void;
  /** Minimum importance filter */
  minImportance?: number;
  /** Importance change callback */
  onImportanceChange?: (value: number) => void;
  /** Search query */
  searchQuery?: string;
  /** Search change callback */
  onSearchChange?: (query: string) => void;
  /** Show marked only filter */
  showMarkedOnly?: boolean;
  /** Show marked only change callback */
  onShowMarkedOnlyChange?: (value: boolean) => void;
  /** Total count for display */
  total?: number;
  /** Additional CSS classes */
  className?: string;
}

// ============================================================================
// Constants
// ============================================================================

const IMPORTANCE_OPTIONS = [
  { value: '1', label: '全部' },
  { value: '5', label: '中等及以上' },
  { value: '7', label: '较高及以上' },
  { value: '8', label: '高重要性' },
];

const TIME_RANGE_OPTIONS = [
  { value: 'all', label: '全部时间' },
  { value: 'today', label: '今天' },
  { value: 'week', label: '本周' },
  { value: 'month', label: '本月' },
];

// ============================================================================
// Sub-Components
// ============================================================================

/**
 * Search icon
 */
const SearchIcon = () => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width="16"
    height="16"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
    className="text-muted-foreground"
  >
    <circle cx="11" cy="11" r="8" />
    <path d="m21 21-4.3-4.3" />
  </svg>
);

/**
 * Filter icon
 */
const FilterIcon = () => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width="16"
    height="16"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <polygon points="22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3" />
  </svg>
);

// ============================================================================
// Main Component
// ============================================================================

/**
 * MemoryFilterBar component
 *
 * Filter controls for memory visualization.
 *
 * @example
 * ```tsx
 * <MemoryFilterBar
 *   layer="L2"
 *   onLayerChange={setLayer}
 *   searchQuery={search}
 *   onSearchChange={setSearch}
 * />
 * ```
 */
export const MemoryFilterBar = memo(function MemoryFilterBar({
  layer,
  onLayerChange,
  timeRange = 'all',
  onTimeRangeChange,
  minImportance = 1,
  onImportanceChange,
  searchQuery = '',
  onSearchChange,
  showMarkedOnly = false,
  onShowMarkedOnlyChange,
  total,
  className,
}: MemoryFilterBarProps) {
  // Handle layer tab click
  const handleLayerClick = useCallback(
    (selectedLayer: MemoryLayer) => {
      onLayerChange(selectedLayer);
    },
    [onLayerChange]
  );

  return (
    <div className={cn('border-b border-border px-4 py-3 space-y-3', className)}>
      {/* Layer tabs */}
      <div className="flex items-center gap-1">
        <LayerTab
          label="L1 工作记忆"
          value="L1"
          active={layer === 'L1'}
          onClick={() => handleLayerClick('L1')}
          color="blue"
        />
        <LayerTab
          label="L2 情景记忆"
          value="L2"
          active={layer === 'L2'}
          onClick={() => handleLayerClick('L2')}
          color="green"
        />
        <LayerTab
          label="L3 语义记忆"
          value="L3"
          active={layer === 'L3'}
          onClick={() => handleLayerClick('L3')}
          color="purple"
        />
      </div>

      {/* Filter row */}
      <div className="flex items-center gap-3 flex-wrap">
        {/* Search input */}
        <div className="relative flex-1 min-w-[180px]">
          <div className="absolute left-3 top-1/2 -translate-y-1/2">
            <SearchIcon />
          </div>
          <Input
            type="text"
            placeholder={
              layer === 'L3' ? '输入关键词搜索...' : '搜索记忆内容...'
            }
            value={searchQuery}
            onChange={(e) => onSearchChange?.(e.target.value)}
            className="pl-9 h-9 text-sm"
          />
        </div>

        {/* Time range filter (only for L2) */}
        {layer === 'L2' && onTimeRangeChange && (
          <Select value={timeRange} onValueChange={onTimeRangeChange}>
            <SelectTrigger className="w-[100px] h-9 text-sm">
              <SelectValue placeholder="时间" />
            </SelectTrigger>
            <SelectContent>
              {TIME_RANGE_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}

        {/* Importance filter (only for L2) */}
        {layer === 'L2' && onImportanceChange && (
          <Select
            value={String(minImportance)}
            onValueChange={(v) => onImportanceChange(parseInt(v, 10))}
          >
            <SelectTrigger className="w-[120px] h-9 text-sm">
              <SelectValue placeholder="重要性" />
            </SelectTrigger>
            <SelectContent>
              {IMPORTANCE_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}

        {/* Marked only filter (only for L2) */}
        {layer === 'L2' && onShowMarkedOnlyChange && (
          <Button
            variant={showMarkedOnly ? 'default' : 'outline'}
            size="sm"
            onClick={() => onShowMarkedOnlyChange(!showMarkedOnly)}
            className={cn(
              'h-9 text-xs',
              showMarkedOnly && 'bg-amber-500 hover:bg-amber-600 text-white'
            )}
          >
            ⭐ 仅标记
          </Button>
        )}

        {/* Total count */}
        {total !== undefined && (
          <span className="text-xs text-muted-foreground ml-auto">
            共 {total} 条
          </span>
        )}
      </div>

      {/* L3 hint */}
      {layer === 'L3' && (
        <p className="text-xs text-muted-foreground">
          💡 语义记忆使用向量相似性搜索，输入关键词查找相关内容
        </p>
      )}
    </div>
  );
});

// ============================================================================
// Helper Components
// ============================================================================

/**
 * Layer tab button
 */
interface LayerTabProps {
  label: string;
  value: MemoryLayer;
  active: boolean;
  onClick: () => void;
  color: 'blue' | 'green' | 'purple';
}

const LayerTab = memo(function LayerTab({
  label,
  active,
  onClick,
  color,
}: LayerTabProps) {
  const colorClasses = {
    blue: {
      active: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400 border-blue-300 dark:border-blue-700',
      inactive: 'text-muted-foreground hover:bg-muted',
    },
    green: {
      active: 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400 border-green-300 dark:border-green-700',
      inactive: 'text-muted-foreground hover:bg-muted',
    },
    purple: {
      active: 'bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400 border-purple-300 dark:border-purple-700',
      inactive: 'text-muted-foreground hover:bg-muted',
    },
  };

  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        'px-3 py-1.5 text-xs font-medium rounded-md transition-colors border border-transparent',
        active ? colorClasses[color].active : colorClasses[color].inactive
      )}
    >
      {label}
    </button>
  );
});

export default MemoryFilterBar;