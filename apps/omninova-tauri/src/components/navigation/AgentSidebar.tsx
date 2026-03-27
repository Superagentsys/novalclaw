/**
 * Agent Sidebar Component
 *
 * Main sidebar for agent selection and quick switching.
 * Features:
 * - Agent list with active state indication
 * - Recent agents section
 * - Keyboard shortcuts (Ctrl/Cmd + 1-9)
 *
 * [Source: Story 10.1 - 代理快速切换功能]
 */

import * as React from 'react';
import { useMemo } from 'react';
import { cn } from '@/lib/utils';
import { useNavigationStore } from '@/stores/navigationStore';
import { AgentSidebarItem } from './AgentSidebarItem';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Button } from '@/components/ui/button';
import { Plus, Users } from 'lucide-react';
import type { AgentModel } from '@/types/agent';

// ============================================================================
// Types
// ============================================================================

export interface AgentSidebarProps {
  /** List of agents to display */
  agents: AgentModel[];
  /** Loading state */
  isLoading?: boolean;
  /** Handler for agent selection */
  onAgentSelect: (agent: AgentModel) => void;
  /** Handler for create new agent button */
  onCreateAgent?: () => void;
  /** Additional CSS classes */
  className?: string;
}

// ============================================================================
// Component
// ============================================================================

/**
 * Agent sidebar component
 *
 * Renders a sidebar with:
 * - Section header
 * - Recent agents (top, if any)
 * - All agents list sorted by recency
 * - Keyboard shortcut indicators
 */
export function AgentSidebar({
  agents,
  isLoading = false,
  onAgentSelect,
  onCreateAgent,
  className,
}: AgentSidebarProps): React.ReactElement {
  // Navigation state
  const activeAgentId = useNavigationStore((s) => s.activeAgentId);
  const recentAgentIds = useNavigationStore((s) => s.recentAgentIds);

  // Sort agents: recent first, then by name
  const sortedAgents = useMemo(() => {
    const recentSet = new Set(recentAgentIds);

    // Split into recent and others
    const recentAgents = recentAgentIds
      .map((id) => agents.find((a) => a.id === id))
      .filter((a): a is AgentModel => a !== undefined);

    const otherAgents = agents
      .filter((a) => !recentSet.has(a.id))
      .sort((a, b) => a.name.localeCompare(b.name));

    return [...recentAgents, ...otherAgents];
  }, [agents, recentAgentIds]);

  // Loading skeleton
  if (isLoading) {
    return (
      <aside className={cn('w-64 border-r border-border bg-muted/30', className)}>
        <div className="p-4 border-b border-border">
          <div className="h-4 w-20 bg-muted animate-pulse rounded" />
        </div>
        <div className="p-2 space-y-2">
          {Array.from({ length: 5 }).map((_, i) => (
            <div
              key={i}
              className="h-10 w-full bg-muted animate-pulse rounded-lg"
            />
          ))}
        </div>
      </aside>
    );
  }

  // Empty state
  if (agents.length === 0) {
    return (
      <aside className={cn('w-64 border-r border-border bg-muted/30', className)}>
        <div className="p-4 border-b border-border">
          <h2 className="text-sm font-medium text-muted-foreground flex items-center gap-2">
            <Users className="w-4 h-4" />
            代理列表
          </h2>
        </div>
        <div className="p-4 flex flex-col items-center justify-center h-48 text-center">
          <p className="text-sm text-muted-foreground mb-4">
            还没有创建代理
          </p>
          {onCreateAgent && (
            <Button size="sm" onClick={onCreateAgent}>
              <Plus className="w-4 h-4 mr-2" />
              创建代理
            </Button>
          )}
        </div>
      </aside>
    );
  }

  return (
    <aside className={cn('w-64 border-r border-border bg-muted/30 flex flex-col', className)}>
      {/* Header */}
      <div className="p-4 border-b border-border flex items-center justify-between">
        <h2 className="text-sm font-medium text-muted-foreground flex items-center gap-2">
          <Users className="w-4 h-4" />
          代理列表
        </h2>
        {onCreateAgent && (
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6"
            onClick={onCreateAgent}
            aria-label="创建代理"
          >
            <Plus className="w-4 h-4" />
          </Button>
        )}
      </div>

      {/* Agent list */}
      <ScrollArea className="flex-1">
        <nav className="p-2 space-y-1" aria-label="代理导航">
          {sortedAgents.map((agent, index) => (
            <AgentSidebarItem
              key={agent.id}
              agent={agent}
              isActive={agent.id === activeAgentId}
              shortcutIndex={index < 9 ? index + 1 : undefined}
              onClick={onAgentSelect}
            />
          ))}
        </nav>
      </ScrollArea>

      {/* Keyboard shortcut hint */}
      <div className="p-2 border-t border-border">
        <p className="text-xs text-muted-foreground text-center">
          Ctrl+1-9 快速切换
        </p>
      </div>
    </aside>
  );
}

export default AgentSidebar;