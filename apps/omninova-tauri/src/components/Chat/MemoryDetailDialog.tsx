/**
 * Memory Detail Dialog Component
 *
 * Displays detailed information about a memory entry
 * with options to view full content and delete.
 *
 * [Source: Story 5.7 - MemoryVisualization 组件]
 */

import { memo, useState, useCallback } from 'react';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import type { UnifiedMemoryEntry } from '@/types/memory';

// ============================================================================
// Types
// ============================================================================

/**
 * Props for MemoryDetailDialog component
 */
export interface MemoryDetailDialogProps {
  /** Memory entry to display */
  memory: UnifiedMemoryEntry | null;
  /** Whether dialog is open */
  open: boolean;
  /** Close callback */
  onClose: () => void;
  /** Delete callback */
  onDelete: (id: string) => void;
  /** Additional CSS classes */
  className?: string;
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Format Unix timestamp to human-readable string
 */
function formatTimestamp(timestamp: number): string {
  const date = new Date(timestamp > 1e10 ? timestamp : timestamp * 1000);
  return date.toLocaleString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

/**
 * Get layer display name
 */
function getLayerDisplayName(layer: string): string {
  const names: Record<string, string> = {
    L1: 'L1 工作记忆',
    L2: 'L2 情景记忆',
    L3: 'L3 语义记忆',
  };
  return names[layer] || layer;
}

// ============================================================================
// Main Component
// ============================================================================

/**
 * MemoryDetailDialog component
 *
 * Shows detailed information about a memory entry.
 *
 * @example
 * ```tsx
 * const [open, setOpen] = useState(false);
 * const [selected, setSelected] = useState<UnifiedMemoryEntry | null>(null);
 *
 * <MemoryDetailDialog
 *   memory={selected}
 *   open={open}
 *   onClose={() => setOpen(false)}
 *   onDelete={(id) => handleDelete(id)}
 * />
 * ```
 */
export const MemoryDetailDialog = memo(function MemoryDetailDialog({
  memory,
  open,
  onClose,
  onDelete,
  className,
}: MemoryDetailDialogProps) {
  // State for delete confirmation
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);

  // Handle delete click
  const handleDeleteClick = useCallback(() => {
    setShowDeleteConfirm(true);
  }, []);

  // Handle confirm delete
  const handleConfirmDelete = useCallback(() => {
    if (memory) {
      onDelete(memory.id);
      setShowDeleteConfirm(false);
      onClose();
    }
  }, [memory, onDelete, onClose]);

  // Handle cancel delete
  const handleCancelDelete = useCallback(() => {
    setShowDeleteConfirm(false);
  }, []);

  // Layer badge colors
  const layerColors = {
    L1: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400',
    L2: 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400',
    L3: 'bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400',
  };

  if (!memory) return null;

  return (
    <>
      <Dialog open={open} onOpenChange={onClose}>
        <DialogContent className={cn('max-w-lg', className)}>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              记忆详情
              <span
                className={cn(
                  'text-xs font-medium px-2 py-0.5 rounded',
                  layerColors[memory.sourceLayer]
                )}
              >
                {memory.sourceLayer}
              </span>
            </DialogTitle>
            <DialogDescription>
              {getLayerDisplayName(memory.sourceLayer)} 条目
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4">
            {/* Metadata */}
            <div className="grid grid-cols-2 gap-3 text-sm">
              {/* ID */}
              <div>
                <span className="text-muted-foreground">ID:</span>
                <span className="ml-2 font-mono text-xs">{memory.id}</span>
              </div>

              {/* Importance */}
              <div>
                <span className="text-muted-foreground">重要性:</span>
                <span
                  className={cn(
                    'ml-2 font-medium',
                    memory.importance >= 8 && 'text-amber-600 dark:text-amber-500'
                  )}
                >
                  {memory.importance}/10
                  {memory.importance >= 8 && ' ⭐'}
                </span>
              </div>

              {/* Created time */}
              <div>
                <span className="text-muted-foreground">创建时间:</span>
                <span className="ml-2 text-xs">
                  {formatTimestamp(memory.createdAt)}
                </span>
              </div>

              {/* Session */}
              {memory.sessionId && (
                <div>
                  <span className="text-muted-foreground">关联会话:</span>
                  <span className="ml-2">#{memory.sessionId}</span>
                </div>
              )}

              {/* Role */}
              {memory.role && (
                <div>
                  <span className="text-muted-foreground">角色:</span>
                  <span className="ml-2">{memory.role}</span>
                </div>
              )}

              {/* Similarity score for L3 */}
              {memory.similarityScore !== null && (
                <div>
                  <span className="text-muted-foreground">相似度:</span>
                  <span className="ml-2 text-purple-600 dark:text-purple-400">
                    {(memory.similarityScore * 100).toFixed(1)}%
                  </span>
                </div>
              )}
            </div>

            {/* Content */}
            <div>
              <span className="text-muted-foreground text-sm">内容:</span>
              <div className="mt-2 p-3 bg-muted rounded-md text-sm whitespace-pre-wrap max-h-60 overflow-auto">
                {memory.content}
              </div>
            </div>
          </div>

          <DialogFooter className="gap-2 sm:gap-0">
            <Button variant="destructive" onClick={handleDeleteClick}>
              删除此记忆
            </Button>
            <Button variant="outline" onClick={onClose}>
              关闭
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete confirmation alert dialog */}
      <AlertDialog open={showDeleteConfirm} onOpenChange={setShowDeleteConfirm}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>确认删除</AlertDialogTitle>
            <AlertDialogDescription>
              确定要删除这条记忆吗？此操作无法撤销。
              {memory.sourceLayer === 'L1' && (
                <span className="block mt-2 text-yellow-600 dark:text-yellow-500">
                  注意：L1 工作记忆删除后仅从列表移除，实际内容在会话结束前仍保留。
                </span>
              )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel onClick={handleCancelDelete}>
              取消
            </AlertDialogCancel>
            <AlertDialogAction
              onClick={handleConfirmDelete}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              确认删除
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
});

export default MemoryDetailDialog;