/**
 * Session Grouping Utility
 *
 * Provides time-based grouping for session history.
 *
 * [Source: Story 10.2 - 历史对话导航]
 */

// ============================================================================
// Types
// ============================================================================

/**
 * Session time group categories
 */
export type SessionTimeGroup = 'today' | 'yesterday' | 'thisWeek' | 'older';

/**
 * Group label mapping
 */
export const SESSION_GROUP_LABELS: Record<SessionTimeGroup, string> = {
  today: '今天',
  yesterday: '昨天',
  thisWeek: '本周',
  older: '更早',
};

/**
 * Ordered list of groups for display
 */
export const SESSION_GROUP_ORDER: SessionTimeGroup[] = [
  'today',
  'yesterday',
  'thisWeek',
  'older',
];

// ============================================================================
// Functions
// ============================================================================

/**
 * Determine which time group a session belongs to
 *
 * @param date - The session date
 * @returns The time group category
 */
export function getSessionGroup(date: Date): SessionTimeGroup {
  const now = new Date();

  // Get today's start (midnight)
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());

  // Get yesterday's start
  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);

  // Get a week ago's start
  const weekAgo = new Date(today);
  weekAgo.setDate(weekAgo.getDate() - 7);

  // Compare dates
  if (date >= today) {
    return 'today';
  }

  if (date >= yesterday) {
    return 'yesterday';
  }

  if (date >= weekAgo) {
    return 'thisWeek';
  }

  return 'older';
}

/**
 * Group sessions by time category
 *
 * @param sessions - List of sessions with createdAt
 * @returns Sessions grouped by time category
 */
export function groupSessionsByTime<T extends { createdAt: string | Date }>(
  sessions: T[]
): Record<SessionTimeGroup, T[]> {
  const groups: Record<SessionTimeGroup, T[]> = {
    today: [],
    yesterday: [],
    thisWeek: [],
    older: [],
  };

  for (const session of sessions) {
    const date =
      session.createdAt instanceof Date
        ? session.createdAt
        : new Date(session.createdAt);

    const group = getSessionGroup(date);
    groups[group].push(session);
  }

  return groups;
}