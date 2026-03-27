/**
 * Session List Item Component
 *
 * Displays a single session item with actions.
 *
 * [Source: Story 10.2 - 历史对话导航]
 */

import * as React from 'react';
import { memo, useState, useCallback } from 'react';
import { cn } from '@/lib/utils';
import type { Session } from '@/types/session';
import { MessageSquare, Trash2, Archive, MoreHorizontal } from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Button } from '@/components/ui/button';
import {
  formatDistanceToNow,
  parseISO,
} from 'date-fns';
import { zhCN } from 'date-fns/locale';

// ============================================================================
// Types
// ============================================================================

export interface SessionListItemProps {
  /** Session data */
  session: Session;
  /** Click handler */
  onClick: () => void;
  /** Delete handler */
  onDelete?: () => void;
  /** Archive handler */
  onArchive?: () => void;
  /** Additional CSS classes */
  className?: string;
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Format session date for display
 */
function formatSessionDate(date: string | Date): string {
  const d = date instanceof Date ? date : parseISO(date);
  return formatDistanceToNow(d, { addSuffix: true, locale: zhCN });
}

// ============================================================================
// Component
// ============================================================================

/**
 * Session list item component
 *
 * Renders a clickable session item with:
 * - Title or placeholder
 * - Time ago
 * - Action menu (delete, archive)
 */
export const SessionListItem = memo(function SessionListItem({
  session,
  onClick,
  onDelete,
  onArchive,
  className,
}: SessionListItemProps): React.ReactElement {
  const [isMenuOpen, setIsMenuOpen] = useState(false);

  // Handle click
  const handleClick = useCallback(() => {
    if (!isMenuOpen) {
      onClick();
    }
  }, [onClick, isMenuOpen]);

  // Handle keyboard
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        onClick();
      }
    },
    [onClick]
  );

  // Get display title
  const title = session.title || '新对话';
  const timeAgo = formatSessionDate(session.createdAt);

  return (
    <button
      type="button"
      onClick={handleClick}
      onKeyDown={handleKeyDown}
      className={cn(
        'w-full flex items-center gap-3 px-3 py-2',
        'text-left transition-colors',
        'hover:bg-accent/50',
        'focus:outline-none focus:ring-2 focus:ring-primary/30 focus:ring-inset',
        isMenuOpen && 'bg-accent/50',
        className
      )}
    >
      {/* Icon */}
      <MessageSquare className="w-4 h-4 text-muted-foreground flex-shrink-0" />

      {/* Content */}
      <div className="flex-1 min-w-0">
        <div className="text-sm truncate">{title}</div>
        <div className="text-xs text-muted-foreground">{timeAgo}</div>
      </div>

      {/* Actions */}
      {(onDelete || onArchive) && (
        <DropdownMenu open={isMenuOpen} onOpenChange={setIsMenuOpen}>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="h-6 w-6 opacity-0 group-hover:opacity-100 focus:opacity-100"
              onClick={(e) => e.stopPropagation()}
            >
              <MoreHorizontal className="w-4 h-4" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-40">
            {onArchive && (
              <DropdownMenuItem onClick={onArchive}>
                <Archive className="w-4 h-4 mr-2" />
                归档
              </DropdownMenuItem>
            )}
            {onDelete && (
              <DropdownMenuItem
                onClick={onDelete}
                className="text-destructive focus:text-destructive"
              >
                <Trash2 className="w-4 h-4 mr-2" />
                删除
              </DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </button>
  );
});

export default SessionListItem;