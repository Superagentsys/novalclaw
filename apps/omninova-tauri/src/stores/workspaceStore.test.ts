/**
 * Workspace Store Tests
 *
 * Unit tests for workspace state management.
 *
 * [Source: Story 10.5 - 工作空间管理]
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { useWorkspaceStore } from './workspaceStore';
import { DEFAULT_WORKSPACE } from '@/types/workspace';

describe('WorkspaceStore', () => {
  beforeEach(() => {
    // Reset store before each test
    useWorkspaceStore.setState({
      workspaces: [DEFAULT_WORKSPACE],
      activeWorkspaceId: DEFAULT_WORKSPACE.id,
    });
  });

  describe('initial state', () => {
    it('should have default workspace', () => {
      const { workspaces } = useWorkspaceStore.getState();
      expect(workspaces).toHaveLength(1);
      expect(workspaces[0].id).toBe('default');
    });

    it('should have default workspace as active', () => {
      const { activeWorkspaceId } = useWorkspaceStore.getState();
      expect(activeWorkspaceId).toBe('default');
    });
  });

  describe('createWorkspace', () => {
    it('should create a new workspace', () => {
      const { createWorkspace } = useWorkspaceStore.getState();
      const id = createWorkspace('工作项目', '💼');

      const { workspaces } = useWorkspaceStore.getState();
      expect(workspaces).toHaveLength(2);
      expect(workspaces[1].name).toBe('工作项目');
      expect(workspaces[1].icon).toBe('💼');
      expect(workspaces[1].id).toBe(id);
    });

    it('should generate unique ids', async () => {
      const { createWorkspace } = useWorkspaceStore.getState();
      const id1 = createWorkspace('空间1', '🏠');

      await new Promise((r) => setTimeout(r, 10));
      const id2 = createWorkspace('空间2', '💼');

      expect(id1).not.toBe(id2);
    });
  });

  describe('updateWorkspace', () => {
    it('should update workspace name', () => {
      const { createWorkspace, updateWorkspace } = useWorkspaceStore.getState();
      const id = createWorkspace('旧名称', '🏠');

      updateWorkspace(id, { name: '新名称' });

      const { workspaces } = useWorkspaceStore.getState();
      const updated = workspaces.find((ws) => ws.id === id);
      expect(updated?.name).toBe('新名称');
      expect(updated?.icon).toBe('🏠');
    });

    it('should update workspace icon', () => {
      const { createWorkspace, updateWorkspace } = useWorkspaceStore.getState();
      const id = createWorkspace('测试', '🏠');

      updateWorkspace(id, { icon: '💼' });

      const { workspaces } = useWorkspaceStore.getState();
      const updated = workspaces.find((ws) => ws.id === id);
      expect(updated?.icon).toBe('💼');
    });

    it('should not affect other workspaces', () => {
      const { createWorkspace, updateWorkspace } = useWorkspaceStore.getState();
      const id = createWorkspace('测试', '🏠');

      updateWorkspace(id, { name: '更新' });

      const { workspaces } = useWorkspaceStore.getState();
      expect(workspaces[0].name).toBe('默认工作空间');
    });
  });

  describe('deleteWorkspace', () => {
    it('should delete a workspace', () => {
      const { createWorkspace, deleteWorkspace } = useWorkspaceStore.getState();
      const id = createWorkspace('待删除', '🗑️');

      const result = deleteWorkspace(id);
      expect(result).toBe(true);

      const { workspaces } = useWorkspaceStore.getState();
      expect(workspaces).toHaveLength(1);
    });

    it('should not delete the last workspace', () => {
      const { deleteWorkspace } = useWorkspaceStore.getState();

      const result = deleteWorkspace('default');
      expect(result).toBe(false);

      const { workspaces } = useWorkspaceStore.getState();
      expect(workspaces).toHaveLength(1);
    });

    it('should not delete active workspace', () => {
      const { createWorkspace, switchWorkspace, deleteWorkspace } =
        useWorkspaceStore.getState();
      const id = createWorkspace('活跃空间', '⭐');
      switchWorkspace(id);

      const result = deleteWorkspace(id);
      expect(result).toBe(false);

      const { workspaces } = useWorkspaceStore.getState();
      expect(workspaces).toHaveLength(2);
    });
  });

  describe('switchWorkspace', () => {
    it('should switch active workspace', () => {
      const { createWorkspace, switchWorkspace } = useWorkspaceStore.getState();
      const id = createWorkspace('新空间', '🚀');

      switchWorkspace(id);

      const { activeWorkspaceId } = useWorkspaceStore.getState();
      expect(activeWorkspaceId).toBe(id);
    });

    it('should update lastAccessedAt on switch', async () => {
      const { createWorkspace, switchWorkspace } = useWorkspaceStore.getState();
      const id = createWorkspace('测试', '🏠');

      await new Promise((r) => setTimeout(r, 10));
      switchWorkspace(id);

      const { workspaces } = useWorkspaceStore.getState();
      const ws = workspaces.find((w) => w.id === id);
      expect(ws?.lastAccessedAt).not.toBe(ws?.createdAt);
    });

    it('should not switch to nonexistent workspace', () => {
      const { switchWorkspace } = useWorkspaceStore.getState();

      switchWorkspace('nonexistent');

      const { activeWorkspaceId } = useWorkspaceStore.getState();
      expect(activeWorkspaceId).toBe('default');
    });
  });

  describe('getActiveWorkspace', () => {
    it('should return active workspace', () => {
      const { getActiveWorkspace } = useWorkspaceStore.getState();
      const active = getActiveWorkspace();

      expect(active?.id).toBe('default');
      expect(active?.name).toBe('默认工作空间');
    });

    it('should return undefined if not found', () => {
      useWorkspaceStore.setState({ activeWorkspaceId: 'nonexistent' });

      const { getActiveWorkspace } = useWorkspaceStore.getState();
      const active = getActiveWorkspace();

      expect(active).toBeUndefined();
    });
  });
});