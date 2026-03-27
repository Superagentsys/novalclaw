/**
 * Workspace Selector Component
 *
 * Dropdown selector for switching between workspaces.
 *
 * [Source: Story 10.5 - 工作空间管理]
 */

import * as React from 'react';
import { memo } from 'react';
import { cn } from '@/lib/utils';
import { useWorkspaceStore } from '@/stores/workspaceStore';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { ChevronsUpDown, Plus, Check } from 'lucide-react';
import { WorkspaceCreateDialog } from './WorkspaceCreateDialog';

// ============================================================================
// Types
// ============================================================================

export interface WorkspaceSelectorProps {
  /** Additional CSS classes */
  className?: string;
  /** Compact mode - only show icon */
  compact?: boolean;
}

// ============================================================================
// Component
// ============================================================================

/**
 * Workspace selector component
 *
 * Provides a dropdown for:
 * - Viewing all workspaces
 * - Switching between workspaces
 * - Creating new workspaces
 */
export const WorkspaceSelector = memo(function WorkspaceSelector({
  className,
  compact = false,
}: WorkspaceSelectorProps): React.ReactElement {
  // Store state
  const workspaces = useWorkspaceStore((s) => s.workspaces);
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const switchWorkspace = useWorkspaceStore((s) => s.switchWorkspace);
  const getActiveWorkspace = useWorkspaceStore((s) => s.getActiveWorkspace);

  // Local state
  const [isCreateOpen, setIsCreateOpen] = React.useState(false);

  const activeWorkspace = getActiveWorkspace();

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            className={cn(
              'flex items-center gap-2 px-2',
              compact && 'w-9 h-9 p-0',
              className
            )}
          >
            {activeWorkspace && (
              <>
                <span className="text-lg">{activeWorkspace.icon}</span>
                {!compact && (
                  <>
                    <span className="font-medium">{activeWorkspace.name}</span>
                    <ChevronsUpDown className="w-4 h-4 text-muted-foreground" />
                  </>
                )}
              </>
            )}
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-56">
          {workspaces.map((workspace) => (
            <DropdownMenuItem
              key={workspace.id}
              onClick={() => switchWorkspace(workspace.id)}
              className="flex items-center justify-between"
            >
              <span className="flex items-center gap-2">
                <span>{workspace.icon}</span>
                <span>{workspace.name}</span>
              </span>
              {workspace.id === activeWorkspaceId && (
                <Check className="w-4 h-4 text-primary" />
              )}
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={() => setIsCreateOpen(true)}>
            <Plus className="w-4 h-4 mr-2" />
            新建工作空间
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <WorkspaceCreateDialog
        open={isCreateOpen}
        onOpenChange={setIsCreateOpen}
      />
    </>
  );
});

export default WorkspaceSelector;