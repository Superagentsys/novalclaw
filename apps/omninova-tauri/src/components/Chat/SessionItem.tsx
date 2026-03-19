/**
 * Session Item Component
 *
 * Individual session item in the session list. Displays session title,
 * last updated time, and optional preview text.
 *
 * [Source: Story 4.7 - 对话历史持久化与导航]
 */

import { memo } from 'react';
import { cn } from '@/lib/utils';
import { type Session } from '@/types/session';

// ============================================================================
// Types
// ============================================================================

/**
 * Props for SessionItem component
 */
export interface SessionItemProps {
  /** Session data */
  session: Session;
  /** Whether this session is currently active */
  isActive: boolean;
  /** Callback when session is clicked */
  onClick: () => void;
  /** Optional preview text (e.g., last message) */
  preview?: string;
  /** Additional CSS classes */
  className?: string;
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Format a timestamp as relative time
 *
 * @param timestamp - Unix timestamp in seconds
 * @returns Human-readable relative time string
 */
function formatRelativeTime(timestamp: number): string {
  const now = Date.now();
  const diff = now - timestamp * 1000; // Convert to milliseconds

  const seconds = Math.floor(diff / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);

  if (seconds < 60) {
    return '刚刚';
  }
  if (minutes < 60) {
    return `${minutes} 分钟前`;
  }
  if (hours < 24) {
    return `${hours} 小时前`;
  }
  if (days < 7) {
    return `${days} 天前`;
  }

  // Format as date for older sessions
  const date = new Date(timestamp * 1000);
  return date.toLocaleDateString('zh-CN', {
    month: 'short',
    day: 'numeric',
  });
}

// ============================================================================
// Component
// ============================================================================

/**
 * SessionItem component
 *
 * Displays a single session in the session list with:
 * - Session title (or "新对话" if untitled)
 * - Last updated time
 * - Optional preview text
 * - Active state styling
 *
 * @example
 * ```tsx
 * <SessionItem
 *   session={session}
 *   isActive={session.id === activeSessionId}
 *   onClick={() => switchSession(session.id)}
 *   preview="最后一条消息预览..."
 * />
 * ```
 */
export const SessionItem = memo(function SessionItem({
  session,
  isActive,
  onClick,
  preview,
  className,
}: SessionItemProps) {
  return (
    <button
      type="button"
      role="option"
      aria-selected={isActive}
      className={cn(
        'w-full text-left px-3 py-2 rounded-lg transition-colors',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
        isActive
          ? 'bg-primary/10 text-primary'
          : 'hover:bg-muted text-foreground',
        className
      )}
      onClick={onClick}
    >
      {/* Title */}
      <div className="font-medium truncate">
        {session.title || '新对话'}
      </div>

      {/* Time */}
      <div className="text-xs text-muted-foreground mt-0.5">
        {formatRelativeTime(session.updatedAt)}
      </div>

      {/* Preview (optional) */}
      {preview && (
        <div className="text-xs text-muted-foreground truncate mt-1">
          {preview}
        </div>
      )}
    </button>
  );
});

export default SessionItem;