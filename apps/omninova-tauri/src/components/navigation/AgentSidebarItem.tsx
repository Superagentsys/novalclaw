/**
 * Agent Sidebar Item Component
 *
 * Displays a single agent item in the sidebar with:
 * - Agent name and status
 * - MBTI personality indicator
 * - Active state styling
 * - Keyboard shortcut badge
 *
 * [Source: Story 10.1 - 代理快速切换功能]
 */

import * as React from 'react';
import { memo, useCallback } from 'react';
import { cn } from '@/lib/utils';
import { personalityColors } from '@/lib/personality-colors';
import { AgentStatusBadge } from '@/components/agent/AgentStatusBadge';
import { UserCircle } from 'lucide-react';
import type { AgentModel } from '@/types/agent';

// ============================================================================
// Types
// ============================================================================

export interface AgentSidebarItemProps {
  /** Agent data */
  agent: AgentModel;
  /** Whether this agent is currently active */
  isActive: boolean;
  /** Keyboard shortcut index (1-9), if assigned */
  shortcutIndex?: number;
  /** Click handler */
  onClick: (agent: AgentModel) => void;
  /** Additional CSS classes */
  className?: string;
}

// ============================================================================
// Component
// ============================================================================

/**
 * Agent sidebar item component
 *
 * Renders a clickable agent item with:
 * - Visual indication of active state
 * - Personality-themed border color
 * - Optional keyboard shortcut badge
 */
export const AgentSidebarItem = memo(function AgentSidebarItem({
  agent,
  isActive,
  shortcutIndex,
  onClick,
  className,
}: AgentSidebarItemProps): React.ReactElement {
  // Get personality theme color
  const themeColor = agent.mbti_type
    ? personalityColors[agent.mbti_type]?.primary
    : undefined;

  // Handle click
  const handleClick = useCallback(() => {
    onClick(agent);
  }, [agent, onClick]);

  // Handle keyboard navigation
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        onClick(agent);
      }
    },
    [agent, onClick]
  );

  // Inactive state
  const isInactive = agent.status === 'inactive';

  return (
    <button
      type="button"
      className={cn(
        // Base styles
        'w-full text-left px-3 py-2.5 rounded-lg',
        'flex items-center gap-3',
        'transition-all duration-150 ease-in-out',
        'focus:outline-none focus:ring-2 focus:ring-primary/30',
        // Border
        'border-l-2',
        // Hover state
        'hover:bg-accent/50',
        // Active state
        isActive && 'bg-accent font-medium',
        // Inactive state
        isInactive && 'opacity-60',
        className
      )}
      style={{
        borderLeftColor: isActive && themeColor ? themeColor : 'transparent',
      }}
      onClick={handleClick}
      onKeyDown={handleKeyDown}
      aria-label={`选择代理: ${agent.name}`}
      aria-current={isActive ? 'page' : undefined}
    >
      {/* Avatar */}
      <div
        className={cn(
          'flex-shrink-0 w-8 h-8 rounded-full flex items-center justify-center',
          isActive ? 'bg-primary/10' : 'bg-muted/50'
        )}
        style={{
          color: themeColor,
        }}
      >
        <UserCircle className="w-5 h-5" />
      </div>

      {/* Info */}
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-sm truncate">{agent.name}</span>
          <AgentStatusBadge status={agent.status} size="sm" />
        </div>
        {agent.mbti_type && (
          <span
            className="text-xs"
            style={{ color: themeColor }}
          >
            {agent.mbti_type}
          </span>
        )}
      </div>

      {/* Shortcut badge */}
      {shortcutIndex !== undefined && (
        <span className="flex-shrink-0 text-xs text-muted-foreground bg-muted/50 px-1.5 py-0.5 rounded">
          {shortcutIndex}
        </span>
      )}
    </button>
  );
});

export default AgentSidebarItem;