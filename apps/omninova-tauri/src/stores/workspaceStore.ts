/**
 * Workspace Store
 *
 * State management for workspace management.
 *
 * [Source: Story 10.5 - 工作空间管理]
 */

import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { Workspace } from '@/types/workspace';
import { DEFAULT_WORKSPACE } from '@/types/workspace';

// ============================================================================
// Types
// ============================================================================

interface WorkspaceState {
  workspaces: Workspace[];
  activeWorkspaceId: string;
}

interface WorkspaceActions {
  createWorkspace: (name: string, icon: string) => string;
  updateWorkspace: (
    id: string,
    updates: Partial<Pick<Workspace, 'name' | 'icon'>>
  ) => void;
  deleteWorkspace: (id: string) => boolean;
  switchWorkspace: (id: string) => void;
  getActiveWorkspace: () => Workspace | undefined;
}

export type WorkspaceStore = WorkspaceState & WorkspaceActions;

// ============================================================================
// Store
// ============================================================================

export const useWorkspaceStore = create<WorkspaceStore>()(
  persist(
    (set, get) => ({
      workspaces: [DEFAULT_WORKSPACE],
      activeWorkspaceId: DEFAULT_WORKSPACE.id,

      createWorkspace: (name, icon) => {
        const id = `workspace-${Date.now()}`;
        const now = new Date().toISOString();
        const newWorkspace: Workspace = {
          id,
          name,
          icon,
          createdAt: now,
          lastAccessedAt: now,
        };
        set((state) => ({
          workspaces: [...state.workspaces, newWorkspace],
        }));
        return id;
      },

      updateWorkspace: (id, updates) => {
        set((state) => ({
          workspaces: state.workspaces.map((ws) =>
            ws.id === id ? { ...ws, ...updates } : ws
          ),
        }));
      },

      deleteWorkspace: (id) => {
        const { workspaces, activeWorkspaceId } = get();
        // Cannot delete the last workspace
        if (workspaces.length <= 1) return false;
        // Cannot delete active workspace
        if (id === activeWorkspaceId) return false;

        set((state) => ({
          workspaces: state.workspaces.filter((ws) => ws.id !== id),
        }));
        return true;
      },

      switchWorkspace: (id) => {
        const { workspaces } = get();
        if (!workspaces.find((ws) => ws.id === id)) return;

        const now = new Date().toISOString();
        set((state) => ({
          activeWorkspaceId: id,
          workspaces: state.workspaces.map((ws) =>
            ws.id === id ? { ...ws, lastAccessedAt: now } : ws
          ),
        }));
      },

      getActiveWorkspace: () => {
        const { workspaces, activeWorkspaceId } = get();
        return workspaces.find((ws) => ws.id === activeWorkspaceId);
      },
    }),
    {
      name: 'omninova-workspace-storage',
    }
  )
);