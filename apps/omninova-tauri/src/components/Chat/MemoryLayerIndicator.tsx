/**
 * Memory Layer Indicator Component
 *
 * Displays the status of the three-layer memory system (L1/L2/L3)
 * with capacity usage, activity indicators, and retrieval animations.
 *
 * [Source: Story 5.6 - MemoryLayerIndicator 组件]
 */

import { memo } from 'react';
import { cn } from '@/lib/utils';
import type { MemoryManagerStats } from '@/types/memory';

// ============================================================================
// Types
// ============================================================================

/**
 * Memory layer identifier
 */
export type MemoryLayer = 'L1' | 'L2' | 'L3';

/**
 * Props for MemoryLayerIndicator component
 */
export interface MemoryLayerIndicatorProps {
  /** Memory statistics */
  stats?: MemoryManagerStats | null;
  /** Currently active memory layer */
  activeLayer?: MemoryLayer | null;
  /** Whether memory retrieval is in progress */
  isRetrieving?: boolean;
  /** Compact mode (minimal display) */
  compact?: boolean;
  /** Additional CSS classes */
  className?: string;
}

// ============================================================================
// Sub-Components
// ============================================================================

/**
 * L1 Layer indicator with progress bar
 */
interface LayerL1Props {
  used: number;
  capacity: number;
  isActive: boolean;
  isRetrieving: boolean;
  compact: boolean;
}

const LayerL1 = memo(function LayerL1({
  used,
  capacity,
  isActive,
  isRetrieving,
  compact,
}: LayerL1Props) {
  const percentage = capacity > 0 ? Math.round((used / capacity) * 100) : 0;

  return (
    <div
      className={cn(
        'flex items-center gap-2 px-2 py-1 rounded transition-all',
        isActive && 'bg-blue-100 dark:bg-blue-900/30 ring-1 ring-blue-400',
        isRetrieving && isActive && 'animate-pulse'
      )}
      title={`L1 工作记忆: ${used}/${capacity} 条目已使用`}
    >
      <span className="text-xs font-medium text-blue-600 dark:text-blue-400">
        L1
      </span>
      {!compact && (
        <>
          <div className="w-16 h-2 bg-blue-100 dark:bg-blue-900/50 rounded overflow-hidden">
            <div
              className="h-full bg-blue-500 transition-all duration-300"
              style={{ width: `${percentage}%` }}
            />
          </div>
          <span className="text-xs text-muted-foreground tabular-nums">
            {used}/{capacity}
          </span>
        </>
      )}
    </div>
  );
});

/**
 * L2 Layer indicator with count display
 */
interface LayerL2Props {
  total: number;
  avgImportance: number;
  isActive: boolean;
  isRetrieving: boolean;
  compact: boolean;
}

const LayerL2 = memo(function LayerL2({
  total,
  avgImportance,
  isActive,
  isRetrieving,
  compact,
}: LayerL2Props) {
  return (
    <div
      className={cn(
        'flex items-center gap-2 px-2 py-1 rounded transition-all',
        isActive && 'bg-green-100 dark:bg-green-900/30 ring-1 ring-green-400',
        isRetrieving && isActive && 'animate-pulse'
      )}
      title={`L2 情景记忆: ${total} 条记忆, 平均重要性 ${avgImportance.toFixed(1)}`}
    >
      <span className="text-xs font-medium text-green-600 dark:text-green-400">
        L2
      </span>
      {!compact && (
        <>
          <span className="text-xs text-muted-foreground tabular-nums">
            {total}
          </span>
          {avgImportance > 0 && (
            <span className="text-xs text-green-500/70 tabular-nums">
              ({avgImportance.toFixed(0)})
            </span>
          )}
        </>
      )}
    </div>
  );
});

/**
 * L3 Layer indicator with count display
 */
interface LayerL3Props {
  total: number;
  available: boolean;
  isActive: boolean;
  isRetrieving: boolean;
  compact: boolean;
}

const LayerL3 = memo(function LayerL3({
  total,
  available,
  isActive,
  isRetrieving,
  compact,
}: LayerL3Props) {
  return (
    <div
      className={cn(
        'flex items-center gap-2 px-2 py-1 rounded transition-all',
        isActive && 'bg-purple-100 dark:bg-purple-900/30 ring-1 ring-purple-400',
        isRetrieving && isActive && 'animate-pulse',
        !available && 'opacity-50'
      )}
      title={`L3 语义记忆: ${total} 条索引, ${available ? '可用' : '不可用'}`}
    >
      <span className="text-xs font-medium text-purple-600 dark:text-purple-400">
        L3
      </span>
      {!compact && (
        <span className="text-xs text-muted-foreground tabular-nums">
          {total}
        </span>
      )}
      {!compact && !available && (
        <span className="text-xs text-muted-foreground/50">(离线)</span>
      )}
    </div>
  );
});

/**
 * Retrieval indicator animation
 */
interface RetrievalIndicatorProps {
  activeLayer: MemoryLayer | null;
}

const RetrievalIndicator = memo(function RetrievalIndicator({
  activeLayer,
}: RetrievalIndicatorProps) {
  if (!activeLayer) return null;

  return (
    <div className="flex items-center gap-1 px-2 py-0.5 bg-muted/50 rounded animate-pulse">
      <div className="w-1.5 h-1.5 bg-primary rounded-full animate-bounce" />
      <span className="text-xs text-muted-foreground">
        {activeLayer} 检索中...
      </span>
    </div>
  );
});

// ============================================================================
// Main Component
// ============================================================================

/**
 * MemoryLayerIndicator component
 *
 * Displays a compact status bar showing all three memory layers
 * with capacity/usage indicators and activity highlighting.
 *
 * @example
 * ```tsx
 * function ChatHeader() {
 *   const { stats } = useMemoryStats();
 *   const isStreaming = useChatStore((s) => s.isStreaming);
 *
 *   return (
 *     <MemoryLayerIndicator
 *       stats={stats}
 *       isRetrieving={isStreaming}
 *       activeLayer={isStreaming ? 'L1' : null}
 *     />
 *   );
 * }
 * ```
 */
export const MemoryLayerIndicator = memo(function MemoryLayerIndicator({
  stats,
  activeLayer = null,
  isRetrieving = false,
  compact = false,
  className,
}: MemoryLayerIndicatorProps) {
  // Default values when stats not available
  const l1Used = stats?.l1Used ?? 0;
  const l1Capacity = stats?.l1Capacity ?? 10;
  const l2Total = stats?.l2Total ?? 0;
  const l2AvgImportance = stats?.l2AvgImportance ?? 0;
  const l3Total = stats?.l3Total ?? 0;

  // L3 is available if stats exist (backend will report 0 if not initialized)
  const l3Available = stats !== null;

  return (
    <div
      className={cn(
        'flex items-center gap-1 text-xs',
        className
      )}
      role="status"
      aria-label="记忆系统状态"
    >
      {/* L1: Working Memory */}
      <LayerL1
        used={l1Used}
        capacity={l1Capacity}
        isActive={activeLayer === 'L1'}
        isRetrieving={isRetrieving}
        compact={compact}
      />

      {/* L2: Episodic Memory */}
      <LayerL2
        total={l2Total}
        avgImportance={l2AvgImportance}
        isActive={activeLayer === 'L2'}
        isRetrieving={isRetrieving}
        compact={compact}
      />

      {/* L3: Semantic Memory */}
      <LayerL3
        total={l3Total}
        available={l3Available}
        isActive={activeLayer === 'L3'}
        isRetrieving={isRetrieving}
        compact={compact}
      />

      {/* Retrieval animation indicator */}
      {isRetrieving && activeLayer && (
        <RetrievalIndicator activeLayer={activeLayer} />
      )}
    </div>
  );
});

export default MemoryLayerIndicator;