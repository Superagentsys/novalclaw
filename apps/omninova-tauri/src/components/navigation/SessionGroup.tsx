/**
 * Session Group Component
 *
 * Displays a group of sessions with a collapsible header.
 *
 * [Source: Story 10.2 - 历史对话导航]
 */

import * as React from 'react';
import { memo, useCallback } from 'react';
import { cn } from '@/lib/utils';
import { SessionListItem } from './SessionListItem';
import { SESSION_GROUP_LABELS, type SessionTimeGroup } from '@/utils/sessionGrouping';
import type { Session } from '@/types/session';
import { ChevronDown, ChevronRight } from 'lucide-react';

// ============================================================================
// Types
// ============================================================================

export interface SessionGroupProps {
  /** Group identifier */
  group: SessionTimeGroup;
  /** Sessions in this group */
  sessions: Session[];
  /** Whether the group is collapsed */
  isCollapsed: boolean;
  /** Toggle collapse handler */
  onToggle: () => void;
  /** Session selection handler */
  onSelectSession: (session: Session) => void;
  /** Session deletion handler */
  onDeleteSession?: (session: Session) => void;
  /** Session archive handler */
  onArchiveSession?: (session: Session) => void;
  /** Additional CSS classes */
  className?: string;
}

// ============================================================================
// Component
// ============================================================================

/**
 * Session group component
 *
 * Renders a collapsible group of sessions with a header.
 */
export const SessionGroup = memo(function SessionGroup({
  group,
  sessions,
  isCollapsed,
  onToggle,
  onSelectSession,
  onDeleteSession,
  onArchiveSession,
  className,
}: SessionGroupProps): React.ReactElement {
  const label = SESSION_GROUP_LABELS[group];
  const count = sessions.length;

  // Handle keyboard navigation on header
  const handleHeaderKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        onToggle();
      }
    },
    [onToggle]
  );

  return (
    <div className={cn('border-b border-border last:border-b-0', className)}>
      {/* Group header */}
      <button
        type="button"
        onClick={onToggle}
        onKeyDown={handleHeaderKeyDown}
        className={cn(
          'w-full flex items-center gap-2 px-3 py-2',
          'text-xs font-medium text-muted-foreground uppercase tracking-wide',
          'hover:bg-accent/50 transition-colors',
          'focus:outline-none focus:ring-2 focus:ring-primary/30'
        )}
        aria-expanded={!isCollapsed}
      >
        {isCollapsed ? (
          <ChevronRight className="w-3 h-3" />
        ) : (
          <ChevronDown className="w-3 h-3" />
        )}
        <span>{label}</span>
        <span className="text-muted-foreground/60">({count})</span>
      </button>

      {/* Session list */}
      {!isCollapsed && (
        <ul className="py-1" role="list">
          {sessions.map((session) => (
            <li key={session.id}>
              <SessionListItem
                session={session}
                onClick={() => onSelectSession(session)}
                onDelete={onDeleteSession ? () => onDeleteSession(session) : undefined}
                onArchive={onArchiveSession ? () => onArchiveSession(session) : undefined}
              />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
});

export default SessionGroup;