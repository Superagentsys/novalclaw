/**
 * Session History Component
 *
 * Displays session history grouped by time with search and filtering.
 *
 * [Source: Story 10.2 - 历史对话导航]
 */

import * as React from 'react';
import { useState, useMemo, useCallback } from 'react';
import { cn } from '@/lib/utils';
import { useSessionStore } from '@/stores/sessionStore';
import { useNavigationStore } from '@/stores/navigationStore';
import { SessionGroup } from './SessionGroup';
import { SessionSearchInput } from './SessionSearchInput';
import {
  groupSessionsByTime,
  SESSION_GROUP_ORDER,
} from '@/utils/sessionGrouping';
import type { Session } from '@/types/session';

// ============================================================================
// Types
// ============================================================================

export interface SessionHistoryProps {
  /** Handler for session selection */
  onSelectSession: (sessionId: number) => void;
  /** Handler for session deletion */
  onDeleteSession?: (sessionId: number) => void;
  /** Handler for session archive */
  onArchiveSession?: (sessionId: number) => void;
  /** Additional CSS classes */
  className?: string;
}

// ============================================================================
// Component
// ============================================================================

/**
 * Session history sidebar component
 *
 * Features:
 * - Time-based grouping (today, yesterday, this week, older)
 * - Search by title
 * - Filter by active agent
 * - Collapsible groups
 */
export function SessionHistory({
  onSelectSession,
  onDeleteSession,
  onArchiveSession,
  className,
}: SessionHistoryProps): React.ReactElement {
  // State
  const [searchQuery, setSearchQuery] = useState('');
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set());

  // Store state
  const sessions = useSessionStore((s) => s.sessions);
  const activeAgentId = useNavigationStore((s) => s.activeAgentId);

  // Filter sessions by agent and search
  const filteredSessions = useMemo(() => {
    let result = sessions;

    // Filter by active agent
    if (activeAgentId !== null) {
      result = result.filter((s) => s.agentId === activeAgentId);
    }

    // Filter by search query
    if (searchQuery.trim()) {
      const query = searchQuery.toLowerCase();
      result = result.filter((s) => {
        const title = s.title?.toLowerCase() ?? '';
        return title.includes(query);
      });
    }

    return result;
  }, [sessions, activeAgentId, searchQuery]);

  // Group by time
  const groupedSessions = useMemo(
    () => groupSessionsByTime(filteredSessions),
    [filteredSessions]
  );

  // Toggle group collapse
  const toggleGroup = useCallback((group: string) => {
    setCollapsedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(group)) {
        next.delete(group);
      } else {
        next.add(group);
      }
      return next;
    });
  }, []);

  // Handle session selection
  const handleSelectSession = useCallback(
    (session: Session) => {
      onSelectSession(session.id);
    },
    [onSelectSession]
  );

  // Handle session deletion
  const handleDeleteSession = useCallback(
    (session: Session) => {
      onDeleteSession?.(session.id);
    },
    [onDeleteSession]
  );

  // Handle session archive
  const handleArchiveSession = useCallback(
    (session: Session) => {
      onArchiveSession?.(session.id);
    },
    [onArchiveSession]
  );

  return (
    <div className={cn('flex flex-col h-full', className)}>
      {/* Search input */}
      <div className="p-2 border-b border-border">
        <SessionSearchInput
          value={searchQuery}
          onChange={setSearchQuery}
          placeholder="搜索对话..."
        />
      </div>

      {/* Session list */}
      <div className="flex-1 overflow-y-auto">
        {filteredSessions.length === 0 ? (
          <div className="p-4 text-center text-sm text-muted-foreground">
            {searchQuery ? '没有找到匹配的对话' : '暂无历史对话'}
          </div>
        ) : (
          SESSION_GROUP_ORDER.map((group) => {
            const groupSessions = groupedSessions[group];
            if (groupSessions.length === 0) return null;

            return (
              <SessionGroup
                key={group}
                group={group}
                sessions={groupSessions}
                isCollapsed={collapsedGroups.has(group)}
                onToggle={() => toggleGroup(group)}
                onSelectSession={handleSelectSession}
                onDeleteSession={handleDeleteSession}
                onArchiveSession={handleArchiveSession}
              />
            );
          })
        )}
      </div>
    </div>
  );
}

export default SessionHistory;