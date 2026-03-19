/**
 * Message Skeleton Component
 *
 * Displays skeleton loading placeholder for chat messages.
 * Supports different variants for user and assistant messages.
 *
 * [Source: Story 4.5 - 打字指示器与加载状态]
 */

import { memo } from 'react';
import { Skeleton } from '@/components/ui/skeleton';
import { cn } from '@/lib/utils';

/**
 * Props for MessageSkeleton component
 */
export interface MessageSkeletonProps {
  /** Message role variant */
  role?: 'user' | 'assistant';
  /** Number of skeleton lines */
  lines?: number;
  /** Show agent name skeleton (assistant only) */
  showAgentName?: boolean;
  /** Show timestamp skeleton */
  showTimestamp?: boolean;
  /** Additional CSS classes */
  className?: string;
  /** CSS styles (for staggered animation delay) */
  style?: React.CSSProperties;
}

/**
 * MessageSkeleton component
 *
 * Displays a skeleton placeholder while messages are loading.
 * Provides visual feedback for users during initial load.
 *
 * @example
 * ```tsx
 * // Assistant message skeleton
 * <MessageSkeleton role="assistant" lines={3} showAgentName />
 *
 * // User message skeleton
 * <MessageSkeleton role="user" lines={2} />
 *
 * // Multiple skeletons for loading state
 * {isLoading && Array.from({ length: 3 }).map((_, i) => (
 *   <MessageSkeleton key={i} role={i % 2 === 0 ? 'assistant' : 'user'} />
 * ))}
 * ```
 */
export const MessageSkeleton = memo(function MessageSkeleton({
  role = 'assistant',
  lines = 2,
  showAgentName = true,
  showTimestamp = true,
  className,
  style,
}: MessageSkeletonProps) {
  const isUser = role === 'user';

  // Generate deterministic widths for consistent rendering
  // Use position-based calculation to avoid Math.random() during render
  const lineWidths = Array.from({ length: lines }, (_, i) => {
    // Create pseudo-random but deterministic widths based on index
    const pseudoRandom = ((i * 37 + 13) % 20); // Gives values 0-19
    const baseWidth = 60 + pseudoRandom;
    // Last line is typically shorter
    const isLast = i === lines - 1;
    return isLast ? Math.max(40, baseWidth - 20) : baseWidth;
  });

  return (
    <div
      className={cn(
        'flex flex-col gap-1.5 max-w-[80%]',
        isUser ? 'ml-auto items-end' : 'mr-auto items-start',
        className
      )}
      role="status"
      aria-label="加载中"
      aria-busy="true"
      style={style}
    >
      {/* Agent name skeleton (assistant only) */}
      {!isUser && showAgentName && (
        <Skeleton className="h-3 w-16 rounded" />
      )}

      {/* Message content skeleton */}
      <div
        className={cn(
          'flex flex-col gap-1.5 rounded-lg px-4 py-3',
          isUser ? 'bg-primary/10' : 'bg-muted/50'
        )}
      >
        {lineWidths.map((width, i) => (
          <Skeleton
            key={i}
            className="h-4 rounded"
            style={{ width: `${width}%` }}
          />
        ))}
      </div>

      {/* Timestamp skeleton */}
      {showTimestamp && (
        <Skeleton className="h-3 w-10 rounded" />
      )}
    </div>
  );
});

/**
 * Props for MessageSkeletonList component
 */
export interface MessageSkeletonListProps {
  /** Number of skeleton messages to display */
  count?: number;
  /** Additional CSS classes */
  className?: string;
}

/**
 * MessageSkeletonList component
 *
 * Renders a list of alternating message skeletons with staggered animation.
 *
 * @example
 * ```tsx
 * function ChatLoadingState() {
 *   return (
 *     <div className="flex flex-col gap-3 p-4">
 *       <MessageSkeletonList count={4} />
 *     </div>
 *   );
 * }
 * ```
 */
export const MessageSkeletonList = memo(function MessageSkeletonList({
  count = 3,
  className,
}: MessageSkeletonListProps) {
  return (
    <div className={cn('flex flex-col gap-3', className)}>
      {Array.from({ length: count }).map((_, i) => (
        <MessageSkeleton
          key={i}
          role={i % 2 === 0 ? 'assistant' : 'user'}
          // Deterministic line count based on index
          lines={2 + (i % 2)}
          showAgentName={i % 2 === 0}
          style={{ animationDelay: `${i * 100}ms` }}
        />
      ))}
    </div>
  );
});

export default MessageSkeleton;