/**
 * Memory Visualization Component
 *
 * Displays and manages AI agent memory content across all three layers
 * (L1/L2/L3) with filtering, search, and deletion capabilities.
 *
 * [Source: Story 5.7 - MemoryVisualization 组件]
 */

import { memo, useState, useCallback } from 'react';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Skeleton } from '@/components/ui/skeleton';
import { MemoryFilterBar, type TimeRange } from './MemoryFilterBar';
import { MemoryDetailDialog } from './MemoryDetailDialog';
import { useMemoryData } from '@/hooks/useMemoryData';
import type { MemoryLayer, UnifiedMemoryEntry } from '@/types/memory';

// ============================================================================
// Types
// ============================================================================

/**
 * Props for MemoryVisualization component
 */
export interface MemoryVisualizationProps {
  /** Current agent ID */
  agentId: number;
  /** Current session ID (optional) */
  sessionId?: number | null;
  /** Close callback */
  onClose?: () => void;
  /** Default layer to display */
  defaultLayer?: MemoryLayer;
  /** Additional CSS classes */
  className?: string;
}

// ============================================================================
// Sub-Components
// ============================================================================

/**
 * Individual memory item card
 */
interface MemoryItemProps {
  memory: UnifiedMemoryEntry;
  onViewDetails: (memory: UnifiedMemoryEntry) => void;
  onDelete: (id: string) => void;
  onToggleMark?: (id: string) => void;
  searchQuery?: string;
}

const MemoryItem = memo(function MemoryItem({
  memory,
  onViewDetails,
  onDelete,
  onToggleMark,
  searchQuery,
}: MemoryItemProps) {
  // Format timestamp
  const timeStr = new Date(
    memory.createdAt > 1e10 ? memory.createdAt : memory.createdAt * 1000
  ).toLocaleString('zh-CN', {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });

  // Highlight search matches
  const highlightedContent = searchQuery?.trim()
    ? highlightText(memory.content, searchQuery)
    : memory.content;

  // Layer badge colors
  const layerColors = {
    L1: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400',
    L2: 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400',
    L3: 'bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400',
  };

  return (
    <div className={cn(
      'p-3 rounded-lg border border-border bg-card hover:bg-accent/50 transition-colors',
      memory.isMarked && 'border-amber-300 dark:border-amber-700 bg-amber-50/30 dark:bg-amber-950/10'
    )}>
      <div className="flex items-start justify-between gap-2 mb-2">
        <div className="flex items-center gap-2 flex-wrap">
          {/* Marked indicator */}
          {memory.isMarked && (
            <span className="text-amber-500" title="已标记为重要">⭐</span>
          )}
          {/* Layer badge */}
          <span
            className={cn(
              'text-xs font-medium px-2 py-0.5 rounded',
              layerColors[memory.sourceLayer]
            )}
          >
            {memory.sourceLayer}
          </span>
          {/* Time */}
          <span className="text-xs text-muted-foreground">{timeStr}</span>
          {/* Importance */}
          {memory.importance >= 8 && (
            <span className="text-xs text-amber-600 dark:text-amber-500 font-medium">
              {memory.importance}/10
            </span>
          )}
        </div>
        {/* Similarity score for L3 */}
        {memory.similarityScore !== null && (
          <span className="text-xs text-purple-600 dark:text-purple-400">
            {(memory.similarityScore * 100).toFixed(0)}% 匹配
          </span>
        )}
      </div>

      {/* Content */}
      <p className="text-sm text-foreground line-clamp-3 mb-2">
        {highlightedContent}
      </p>

      {/* Session info */}
      {memory.sessionId && (
        <p className="text-xs text-muted-foreground mb-2">
          会话 #{memory.sessionId}
        </p>
      )}

      {/* Actions */}
      <div className="flex items-center gap-2">
        <Button
          variant="ghost"
          size="sm"
          onClick={() => onViewDetails(memory)}
          className="h-7 text-xs"
        >
          详情
        </Button>
        {/* Mark/Unmark button (only for L2) */}
        {memory.sourceLayer === 'L2' && onToggleMark && (
          <Button
            variant="ghost"
            size="sm"
            onClick={() => onToggleMark(memory.id)}
            className={cn(
              'h-7 text-xs',
              memory.isMarked
                ? 'text-amber-600 hover:text-amber-700'
                : 'text-muted-foreground hover:text-foreground'
            )}
          >
            {memory.isMarked ? '取消标记' : '标记重要'}
          </Button>
        )}
        <Button
          variant="ghost"
          size="sm"
          onClick={() => onDelete(memory.id)}
          className="h-7 text-xs text-destructive hover:text-destructive"
        >
          删除
        </Button>
      </div>
    </div>
  );
});

/**
 * Highlight matching text in content
 */
function highlightText(text: string, query: string): React.ReactNode {
  if (!query.trim()) return text;

  const lowerText = text.toLowerCase();
  const lowerQuery = query.toLowerCase();
  const index = lowerText.indexOf(lowerQuery);

  if (index === -1) return text;

  const before = text.slice(0, index);
  const match = text.slice(index, index + query.length);
  const after = text.slice(index + query.length);

  return (
    <>
      {before}
      <mark className="bg-yellow-200 dark:bg-yellow-800 rounded px-0.5">
        {match}
      </mark>
      {after}
    </>
  );
}

/**
 * Empty state component
 */
interface EmptyStateProps {
  layer: MemoryLayer;
  searchQuery?: string;
}

const EmptyState = memo(function EmptyState({ layer, searchQuery }: EmptyStateProps) {
  const messages: Record<MemoryLayer, string> = {
    L1: '工作记忆为空',
    L2: '暂无情景记忆',
    L3: searchQuery ? '未找到匹配的记忆' : '输入关键词搜索语义记忆',
  };

  return (
    <div className="flex flex-col items-center justify-center py-12 text-center">
      <div className="w-16 h-16 mb-4 rounded-full bg-muted flex items-center justify-center">
        <BrainIcon className="w-8 h-8 text-muted-foreground" />
      </div>
      <p className="text-muted-foreground">{messages[layer]}</p>
      {layer === 'L3' && !searchQuery && (
        <p className="text-xs text-muted-foreground mt-2">
          语义搜索需要在 L3 向量索引中查找相似内容
        </p>
      )}
    </div>
  );
});

/**
 * Brain icon component
 */
const BrainIcon = ({ className }: { className?: string }) => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
  >
    <path d="M12 5a3 3 0 1 0-5.997.125 4 4 0 0 0-2.526 5.77 4 4 0 0 0 .556 6.588A4 4 0 1 0 12 18Z" />
    <path d="M12 5a3 3 0 1 1 5.997.125 4 4 0 0 1 2.526 5.77 4 4 0 0 1-.556 6.588A4 4 0 1 1 12 18Z" />
    <path d="M15 13a4.5 4.5 0 0 1-3-4 4.5 4.5 0 0 1-3 4" />
    <path d="M17.599 6.5a3 3 0 0 0 .399-1.375" />
    <path d="M6.003 5.125A3 3 0 0 0 6.401 6.5" />
    <path d="M3.477 10.896a4 4 0 0 1 .585-.396" />
    <path d="M19.938 10.5a4 4 0 0 1 .585.396" />
    <path d="M6 18a4 4 0 0 1-1.967-.516" />
    <path d="M19.967 17.484A4 4 0 0 1 18 18" />
  </svg>
);

/**
 * Loading skeleton
 */
const LoadingSkeleton = memo(function LoadingSkeleton() {
  return (
    <div className="space-y-3">
      {[1, 2, 3].map((i) => (
        <div key={i} className="p-3 rounded-lg border border-border">
          <div className="flex items-center gap-2 mb-2">
            <Skeleton className="h-5 w-10" />
            <Skeleton className="h-4 w-24" />
          </div>
          <Skeleton className="h-4 w-full mb-1" />
          <Skeleton className="h-4 w-3/4 mb-2" />
          <div className="flex gap-2">
            <Skeleton className="h-7 w-12" />
            <Skeleton className="h-7 w-12" />
          </div>
        </div>
      ))}
    </div>
  );
});

// ============================================================================
// Main Component
// ============================================================================

/**
 * MemoryVisualization component
 *
 * Main container for viewing and managing agent memory content.
 *
 * @example
 * ```tsx
 * function MemoryPanel() {
 *   const [open, setOpen] = useState(false);
 *
 *   return (
 *     <>
 *       <Button onClick={() => setOpen(true)}>查看记忆</Button>
 *       <Sheet open={open} onOpenChange={setOpen}>
 *         <SheetContent>
 *           <MemoryVisualization
 *             agentId={1}
 *             onClose={() => setOpen(false)}
 *           />
 *         </SheetContent>
 *       </Sheet>
 *     </>
 *   );
 * }
 * ```
 */
export const MemoryVisualization = memo(function MemoryVisualization({
  agentId,
  sessionId,
  onClose,
  defaultLayer = 'L2',
  className,
}: MemoryVisualizationProps) {
  // State
  const [activeLayer, setActiveLayer] = useState<MemoryLayer>(defaultLayer);
  const [selectedMemory, setSelectedMemory] = useState<UnifiedMemoryEntry | null>(null);
  const [showDetailDialog, setShowDetailDialog] = useState(false);

  // Data hook
  const {
    memories,
    isLoading,
    error,
    refresh,
    loadMore,
    hasMore,
    total,
    deleteMemory,
    markMemory,
    unmarkMemory,
    searchQuery,
    setSearchQuery,
    timeRange,
    setTimeRange,
    minImportance,
    setMinImportance,
    showMarkedOnly,
    setShowMarkedOnly,
  } = useMemoryData({
    agentId,
    sessionId,
    layer: activeLayer,
    pageSize: 20,
    autoRefresh: true,
  });

  // Handlers
  const handleViewDetails = useCallback((memory: UnifiedMemoryEntry) => {
    setSelectedMemory(memory);
    setShowDetailDialog(true);
  }, []);

  const handleDeleteMemory = useCallback(
    async (id: string) => {
      const success = await deleteMemory(id);
      if (!success) {
        console.error('Failed to delete memory:', id);
      }
    },
    [deleteMemory]
  );

  const handleToggleMark = useCallback(
    async (id: string) => {
      // Find the memory to check current marked state
      const memory = memories.find((m) => m.id === id);
      if (!memory) return;

      const success = memory.isMarked
        ? await unmarkMemory(id)
        : await markMemory(id);

      if (!success) {
        console.error('Failed to toggle mark:', id);
      }
    },
    [memories, markMemory, unmarkMemory]
  );

  const handleLayerChange = useCallback((layer: string) => {
    setActiveLayer(layer as MemoryLayer);
  }, []);

  const handleTimeRangeChange = useCallback((range: string) => {
    setTimeRange(range as TimeRange);
  }, [setTimeRange]);

  const handleImportanceChange = useCallback((value: number) => {
    setMinImportance(value);
  }, [setMinImportance]);

  return (
    <div className={cn('flex flex-col h-full', className)}>
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <h2 className="text-lg font-semibold">记忆管理</h2>
        {onClose && (
          <Button variant="ghost" size="icon" onClick={onClose}>
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="M18 6 6 18" />
              <path d="m6 6 12 12" />
            </svg>
          </Button>
        )}
      </div>

      {/* Filter bar */}
      <MemoryFilterBar
        layer={activeLayer}
        onLayerChange={handleLayerChange}
        timeRange={timeRange}
        onTimeRangeChange={handleTimeRangeChange}
        minImportance={minImportance}
        onImportanceChange={handleImportanceChange}
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        showMarkedOnly={showMarkedOnly}
        onShowMarkedOnlyChange={setShowMarkedOnly}
        total={total}
      />

      {/* Content */}
      <div className="flex-1 overflow-auto px-4 py-3">
        {error && (
          <div className="p-4 rounded-lg bg-destructive/10 text-destructive text-sm">
            加载失败: {error.message}
          </div>
        )}

        {isLoading ? (
          <LoadingSkeleton />
        ) : memories.length === 0 ? (
          <EmptyState layer={activeLayer} searchQuery={searchQuery} />
        ) : (
          <div className="space-y-3">
            {memories.map((memory) => (
              <MemoryItem
                key={memory.id}
                memory={memory}
                onViewDetails={handleViewDetails}
                onDelete={handleDeleteMemory}
                onToggleMark={handleToggleMark}
                searchQuery={activeLayer !== 'L3' ? searchQuery : undefined}
              />
            ))}

            {/* Load more button */}
            {hasMore && (
              <div className="flex justify-center py-4">
                <Button variant="outline" onClick={loadMore} disabled={isLoading}>
                  加载更多
                </Button>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Footer stats */}
      <div className="px-4 py-2 border-t border-border text-xs text-muted-foreground">
        显示 {memories.length} 条 / 共 {total} 条记忆
      </div>

      {/* Detail dialog */}
      <MemoryDetailDialog
        memory={selectedMemory}
        open={showDetailDialog}
        onClose={() => setShowDetailDialog(false)}
        onDelete={handleDeleteMemory}
      />
    </div>
  );
});

export default MemoryVisualization;