/**
 * Session List Component
 *
 * Displays a scrollable list of conversation sessions with
 * a "new session" button and loading/empty states.
 *
 * [Source: Story 4.7 - 对话历史持久化与导航]
 */

import { memo, useCallback } from 'react';
import { cn } from '@/lib/utils';
import { type Session } from '@/types/session';
import SessionItem from './SessionItem';

// ============================================================================
// Types
// ============================================================================

/**
 * Props for SessionList component
 */
export interface SessionListProps {
  /** List of sessions to display */
  sessions: Session[];
  /** Currently active session ID */
  activeSessionId: number | null;
  /** Callback when a session is selected */
  onSelectSession: (sessionId: number) => void;
  /** Callback to create a new session */
  onCreateSession: () => void;
  /** Whether sessions are loading */
  isLoading?: boolean;
  /** Error message if any */
  error?: string | null;
  /** Additional CSS classes */
  className?: string;
  /** Preview text for each session (keyed by session ID) */
  previews?: Record<number, string>;
}

// ============================================================================
// Helper Components
// ============================================================================

/**
 * Loading skeleton for session list
 */
const SessionListSkeleton = memo(function SessionListSkeleton() {
  return (
    <div className="space-y-2 p-2" aria-busy="true" role="status">
      {[1, 2, 3, 4].map((i) => (
        <div
          key={i}
          className="animate-pulse px-3 py-2 rounded-lg bg-muted"
        >
          <div className="h-4 bg-muted-foreground/20 rounded w-3/4" />
          <div className="h-3 bg-muted-foreground/20 rounded w-1/3 mt-1.5" />
        </div>
      ))}
    </div>
  );
});

/**
 * Empty state when no sessions exist
 */
const EmptyState = memo(function EmptyState({
  onCreateSession,
}: {
  onCreateSession: () => void;
}) {
  return (
    <div className="flex flex-col items-center justify-center py-8 px-4 text-center">
      <div className="w-12 h-12 rounded-full bg-muted flex items-center justify-center mb-3">
        <svg
          className="w-6 h-6 text-muted-foreground"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1.5}
            d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"
          />
        </svg>
      </div>
      <p className="text-sm text-muted-foreground mb-3">
        暂无对话记录
      </p>
      <button
        type="button"
        onClick={onCreateSession}
        className="text-sm text-primary hover:text-primary/80 underline underline-offset-2"
      >
        开始新对话
      </button>
    </div>
  );
});

/**
 * Error state display
 */
const ErrorState = memo(function ErrorState({
  error,
}: {
  error: string;
}) {
  return (
    <div className="flex items-center gap-2 px-3 py-2 text-destructive text-sm">
      <svg
        className="w-4 h-4 flex-shrink-0"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={2}
          d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
        />
      </svg>
      <span>{error}</span>
    </div>
  );
});

// ============================================================================
// Main Component
// ============================================================================

/**
 * SessionList component
 *
 * Displays a scrollable list of conversation sessions with:
 * - "新建会话" button at top
 * - Scrollable session list
 * - Loading skeleton during fetch
 * - Empty state when no sessions
 * - Error state on failure
 * - Accessible listbox pattern
 *
 * @example
 * ```tsx
 * function SessionSidebar() {
 *   const { sessions, activeSessionId, loadSessions, switchSession, createSession } = useSessionStore();
 *
 *   useEffect(() => {
 *     loadSessions(agentId);
 *   }, [agentId]);
 *
 *   return (
 *     <SessionList
 *       sessions={sessions}
 *       activeSessionId={activeSessionId}
 *       onSelectSession={switchSession}
 *       onCreateSession={() => createSession(agentId)}
 *       isLoading={isLoading}
 *     />
 *   );
 * }
 * ```
 */
export const SessionList = memo(function SessionList({
  sessions,
  activeSessionId,
  onSelectSession,
  onCreateSession,
  isLoading = false,
  error,
  className,
  previews,
}: SessionListProps) {
  // Handle session click
  const handleSessionClick = useCallback(
    (sessionId: number) => {
      onSelectSession(sessionId);
    },
    [onSelectSession]
  );

  return (
    <div
      className={cn('flex flex-col h-full', className)}
      role="region"
      aria-label="会话列表"
    >
      {/* Header with new session button */}
      <div className="flex-shrink-0 p-2 border-b border-border">
        <button
          type="button"
          onClick={onCreateSession}
          className={cn(
            'w-full flex items-center justify-center gap-2 px-3 py-2 rounded-lg',
            'bg-primary text-primary-foreground hover:bg-primary/90',
            'transition-colors text-sm font-medium',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring'
          )}
          aria-label="创建新会话"
        >
          <svg
            className="w-4 h-4"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M12 4v16m8-8H4"
            />
          </svg>
          新建会话
        </button>
      </div>

      {/* Session list container */}
      <div
        className="flex-1 overflow-y-auto"
        role="listbox"
        aria-label="会话列表"
        aria-busy={isLoading}
      >
        {/* Loading state - only show skeleton when no sessions exist */}
        {isLoading && sessions.length === 0 && <SessionListSkeleton />}

        {/* Error state */}
        {error && !isLoading && sessions.length === 0 && <ErrorState error={error} />}

        {/* Empty state */}
        {!isLoading && !error && sessions.length === 0 && (
          <EmptyState onCreateSession={onCreateSession} />
        )}

        {/* Session list - show even during loading if sessions exist */}
        {sessions.length > 0 && (
          <ul className="space-y-1 p-2">
            {sessions.map((session) => (
              <li key={session.id}>
                <SessionItem
                  session={session}
                  isActive={session.id === activeSessionId}
                  onClick={() => handleSessionClick(session.id)}
                  preview={previews?.[session.id]}
                />
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
});

export default SessionList;