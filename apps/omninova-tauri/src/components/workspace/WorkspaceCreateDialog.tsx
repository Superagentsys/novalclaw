/**
 * Workspace Create Dialog Component
 *
 * Dialog for creating new workspaces.
 *
 * [Source: Story 10.5 - 工作空间管理]
 */

import * as React from 'react';
import { memo, useState, useCallback } from 'react';
import { cn } from '@/lib/utils';
import { useWorkspaceStore } from '@/stores/workspaceStore';
import { WORKSPACE_ICONS } from '@/types/workspace';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';

// ============================================================================
// Types
// ============================================================================

export interface WorkspaceCreateDialogProps {
  /** Whether dialog is open */
  open: boolean;
  /** Called when open state changes */
  onOpenChange: (open: boolean) => void;
  /** Called when workspace is created */
  onCreated?: (workspaceId: string) => void;
}

// ============================================================================
// Component
// ============================================================================

/**
 * Workspace create dialog component
 *
 * Provides UI for:
 * - Entering workspace name
 * - Selecting workspace icon
 * - Creating the workspace
 */
export const WorkspaceCreateDialog = memo(function WorkspaceCreateDialog({
  open,
  onOpenChange,
  onCreated,
}: WorkspaceCreateDialogProps): React.ReactElement {
  // Store state
  const createWorkspace = useWorkspaceStore((s) => s.createWorkspace);

  // Local state
  const [name, setName] = useState('');
  const [icon, setIcon] = useState<string>(WORKSPACE_ICONS[0]);

  const handleCreate = useCallback(() => {
    if (name.trim()) {
      const id = createWorkspace(name.trim(), icon);
      setName('');
      setIcon(WORKSPACE_ICONS[0]);
      onOpenChange(false);
      onCreated?.(id);
    }
  }, [name, icon, createWorkspace, onOpenChange, onCreated]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && name.trim()) {
        handleCreate();
      }
    },
    [name, handleCreate]
  );

  // Reset form when dialog closes
  const handleOpenChange = useCallback(
    (newOpen: boolean) => {
      if (!newOpen) {
        setName('');
        setIcon(WORKSPACE_ICONS[0]);
      }
      onOpenChange(newOpen);
    },
    [onOpenChange]
  );

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>新建工作空间</DialogTitle>
        </DialogHeader>
        <div className="space-y-4 mt-4">
          <div>
            <label className="text-sm font-medium mb-2 block">名称</label>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="输入工作空间名称..."
              autoFocus
            />
          </div>
          <div>
            <label className="text-sm font-medium mb-2 block">图标</label>
            <div className="grid grid-cols-8 gap-1">
              {WORKSPACE_ICONS.map((iconOption) => (
                <button
                  key={iconOption}
                  type="button"
                  onClick={() => setIcon(iconOption)}
                  className={cn(
                    'w-8 h-8 flex items-center justify-center rounded-md',
                    'text-lg transition-colors',
                    icon === iconOption
                      ? 'bg-primary text-primary-foreground'
                      : 'hover:bg-accent'
                  )}
                >
                  {iconOption}
                </button>
              ))}
            </div>
          </div>
        </div>
        <DialogFooter className="mt-4">
          <Button variant="outline" onClick={() => handleOpenChange(false)}>
            取消
          </Button>
          <Button onClick={handleCreate} disabled={!name.trim()}>
            创建
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
});

export default WorkspaceCreateDialog;