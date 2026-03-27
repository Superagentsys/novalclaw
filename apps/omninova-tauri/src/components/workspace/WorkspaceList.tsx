/**
 * Workspace List Component
 *
 * List view for managing workspaces.
 *
 * [Source: Story 10.5 - 工作空间管理]
 */

import * as React from 'react';
import { memo, useState } from 'react';
import { cn } from '@/lib/utils';
import { useWorkspaceStore } from '@/stores/workspaceStore';
import type { Workspace } from '@/types/workspace';
import { WORKSPACE_ICONS } from '@/types/workspace';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  MoreHorizontal,
  Pencil,
  Trash2,
  Check,
  Settings2,
} from 'lucide-react';
import { WorkspaceCreateDialog } from './WorkspaceCreateDialog';

// ============================================================================
// Types
// ============================================================================

export interface WorkspaceListProps {
  /** Additional CSS classes */
  className?: string;
  /** Called when workspace is selected */
  onSelect?: (workspaceId: string) => void;
}

// ============================================================================
// Component
// ============================================================================

/**
 * Workspace list component
 *
 * Provides UI for:
 * - Viewing all workspaces
 * - Editing workspace name/icon
 * - Deleting workspaces
 */
export const WorkspaceList = memo(function WorkspaceList({
  className,
  onSelect,
}: WorkspaceListProps): React.ReactElement {
  // Store state
  const workspaces = useWorkspaceStore((s) => s.workspaces);
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const switchWorkspace = useWorkspaceStore((s) => s.switchWorkspace);
  const updateWorkspace = useWorkspaceStore((s) => s.updateWorkspace);
  const deleteWorkspace = useWorkspaceStore((s) => s.deleteWorkspace);

  // Local state
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [editingWorkspace, setEditingWorkspace] = useState<Workspace | null>(
    null
  );
  const [editName, setEditName] = useState('');
  const [editIcon, setEditIcon] = useState('');

  const handleStartEdit = (workspace: Workspace) => {
    setEditingWorkspace(workspace);
    setEditName(workspace.name);
    setEditIcon(workspace.icon);
  };

  const handleSaveEdit = () => {
    if (editingWorkspace && editName.trim()) {
      updateWorkspace(editingWorkspace.id, {
        name: editName.trim(),
        icon: editIcon,
      });
      setEditingWorkspace(null);
    }
  };

  const handleDelete = (workspace: Workspace) => {
    if (workspace.id !== activeWorkspaceId) {
      deleteWorkspace(workspace.id);
    }
  };

  const handleSelect = (workspaceId: string) => {
    switchWorkspace(workspaceId);
    onSelect?.(workspaceId);
  };

  return (
    <div className={cn('space-y-4', className)}>
      {/* Header */}
      <div className="flex items-center justify-between">
        <h3 className="text-lg font-semibold">工作空间</h3>
        <Button
          variant="outline"
          size="sm"
          onClick={() => setIsCreateOpen(true)}
        >
          新建
        </Button>
      </div>

      {/* Workspace list */}
      <div className="space-y-1">
        {workspaces.map((workspace) => (
          <div
            key={workspace.id}
            className={cn(
              'flex items-center justify-between px-3 py-2 rounded-lg',
              'text-sm transition-colors cursor-pointer',
              workspace.id === activeWorkspaceId
                ? 'bg-primary/10 text-primary'
                : 'hover:bg-accent'
            )}
            onClick={() => handleSelect(workspace.id)}
          >
            <span className="flex items-center gap-2">
              <span className="text-lg">{workspace.icon}</span>
              <span className="font-medium">{workspace.name}</span>
              {workspace.id === activeWorkspaceId && (
                <Check className="w-4 h-4" />
              )}
            </span>

            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-6 w-6"
                  onClick={(e) => e.stopPropagation()}
                >
                  <MoreHorizontal className="w-4 h-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem onClick={() => handleStartEdit(workspace)}>
                  <Pencil className="w-4 h-4 mr-2" />
                  编辑
                </DropdownMenuItem>
                {workspace.id !== activeWorkspaceId &&
                  workspaces.length > 1 && (
                    <DropdownMenuItem
                      onClick={() => handleDelete(workspace)}
                      className="text-destructive"
                    >
                      <Trash2 className="w-4 h-4 mr-2" />
                      删除
                    </DropdownMenuItem>
                  )}
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        ))}
      </div>

      {/* Create dialog */}
      <WorkspaceCreateDialog
        open={isCreateOpen}
        onOpenChange={setIsCreateOpen}
      />

      {/* Edit dialog */}
      <Dialog
        open={!!editingWorkspace}
        onOpenChange={(open) => !open && setEditingWorkspace(null)}
      >
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>编辑工作空间</DialogTitle>
          </DialogHeader>
          <div className="space-y-4 mt-4">
            <div>
              <label className="text-sm font-medium mb-2 block">名称</label>
              <Input
                value={editName}
                onChange={(e) => setEditName(e.target.value)}
                placeholder="输入工作空间名称..."
              />
            </div>
            <div>
              <label className="text-sm font-medium mb-2 block">图标</label>
              <div className="grid grid-cols-8 gap-1">
                {WORKSPACE_ICONS.map((icon) => (
                  <button
                    key={icon}
                    type="button"
                    onClick={() => setEditIcon(icon)}
                    className={cn(
                      'w-8 h-8 flex items-center justify-center rounded-md',
                      'text-lg transition-colors',
                      editIcon === icon
                        ? 'bg-primary text-primary-foreground'
                        : 'hover:bg-accent'
                    )}
                  >
                    {icon}
                  </button>
                ))}
              </div>
            </div>
            <div className="flex justify-end gap-2">
              <Button variant="outline" onClick={() => setEditingWorkspace(null)}>
                取消
              </Button>
              <Button onClick={handleSaveEdit} disabled={!editName.trim()}>
                保存
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
});

export default WorkspaceList;