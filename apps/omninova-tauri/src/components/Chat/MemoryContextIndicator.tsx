/**
 * Memory Context Indicator Component
 *
 * Displays information about memory context used in AI responses.
 * Shows count of memories used and allows expansion to see details.
 *
 * [Source: Story 5.9 - 上下文增强响应]
 */

import { memo, useState } from 'react';
import { cn } from '@/lib/utils';
import { MemoryContextInfo } from '@/types/memory';

/**
 * Props for MemoryContextIndicator component
 */
export interface MemoryContextIndicatorProps {
  /** Memory context information from chat response */
  memoryContext: MemoryContextInfo;
  /** Additional CSS classes */
  className?: string;
}

/**
 * Format a Unix timestamp to a human-readable date string
 */
function formatDate(timestamp: number): string {
  const date = new Date(timestamp * 1000);
  return date.toLocaleDateString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  });
}

/**
 * Format a similarity score to a percentage string
 */
function formatSimilarity(score: number | null): string {
  if (score === null) return '';
  return `${Math.round(score * 100)}%`;
}

/**
 * Get badge color based on memory layer
 */
function getLayerColor(layer: string): string {
  switch (layer) {
    case 'L1':
      return 'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-400';
    case 'L2':
      return 'bg-blue-100 text-blue-800 dark:bg-blue-900/30 dark:text-blue-400';
    case 'L3':
      return 'bg-purple-100 text-purple-800 dark:bg-purple-900/30 dark:text-purple-400';
    default:
      return 'bg-gray-100 text-gray-800 dark:bg-gray-800 dark:text-gray-400';
  }
}

/**
 * Memory Context Indicator Component
 *
 * Shows a collapsed indicator with memory count that can be expanded
 * to show detailed list of used memories.
 */
function MemoryContextIndicator({
  memoryContext,
  className,
}: MemoryContextIndicatorProps) {
  const [isExpanded, setIsExpanded] = useState(false);

  if (!memoryContext.entries.length) {
    return null;
  }

  const { entries, totalChars, retrievalTimeMs } = memoryContext;

  return (
    <div className={cn('mt-3', className)}>
      {/* Collapsed indicator */}
      <button
        type="button"
        onClick={() => setIsExpanded(!isExpanded)}
        className={cn(
          'w-full flex items-center gap-2 px-3 py-2 text-xs rounded-lg',
          'bg-muted/50 hover:bg-muted/70 transition-colors',
          'text-muted-foreground hover:text-foreground'
        )}
        aria-expanded={isExpanded}
        aria-controls="memory-context-details"
      >
        <svg
          className="w-4 h-4 text-primary"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          aria-hidden="true"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253"
          />
        </svg>
        <span className="font-medium">
          使用了 {entries.length} 条相关记忆
        </span>
        <span className="text-muted-foreground/60 ml-auto">
          {retrievalTimeMs}ms
        </span>
        <svg
          className={cn(
            'w-4 h-4 transition-transform',
            isExpanded && 'rotate-180'
          )}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          aria-hidden="true"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M19 9l-7 7-7-7"
          />
        </svg>
      </button>

      {/* Expanded details */}
      {isExpanded && (
        <div
          id="memory-context-details"
          className="mt-2 p-3 rounded-lg bg-muted/30 border border-border/50"
        >
          <div className="flex items-center justify-between text-xs text-muted-foreground mb-3">
            <span>相关记忆 ({entries.length} 条)</span>
            <span>共 {totalChars} 字符</span>
          </div>

          <ul className="space-y-2">
            {entries.map((entry, index) => (
              <li
                key={entry.id}
                className="text-sm flex items-start gap-2 p-2 rounded bg-background/50"
              >
                <span className="text-muted-foreground text-xs mt-0.5">
                  {index + 1}.
                </span>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 mb-1">
                    <span
                      className={cn(
                        'px-1.5 py-0.5 text-xs font-medium rounded',
                        getLayerColor(entry.sourceLayer)
                      )}
                    >
                      {entry.sourceLayer}
                    </span>
                    {entry.similarityScore !== null && (
                      <span className="text-xs text-muted-foreground">
                        相似度: {formatSimilarity(entry.similarityScore)}
                      </span>
                    )}
                    <span className="text-xs text-muted-foreground">
                      重要性: {entry.importance}
                    </span>
                    <span className="text-xs text-muted-foreground ml-auto">
                      {formatDate(entry.createdAt)}
                    </span>
                  </div>
                  <p className="text-sm text-foreground/90 line-clamp-2">
                    {entry.content}
                  </p>
                </div>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

export default memo(MemoryContextIndicator);