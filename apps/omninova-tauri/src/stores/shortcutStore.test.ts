/**
 * Shortcut Store Tests
 *
 * Unit tests for keyboard shortcut management.
 *
 * [Source: Story 10.6 - 键盘快捷键]
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { useShortcutStore } from './shortcutStore';
import { DEFAULT_SHORTCUTS } from '@/types/shortcut';

describe('ShortcutStore', () => {
  beforeEach(() => {
    // Reset store before each test
    useShortcutStore.setState({
      shortcuts: DEFAULT_SHORTCUTS,
      enabled: true,
    });
  });

  describe('initial state', () => {
    it('should have default shortcuts', () => {
      const { shortcuts } = useShortcutStore.getState();
      expect(shortcuts).toHaveLength(8);
      expect(shortcuts[0].action).toBe('globalSearch');
    });

    it('should be enabled by default', () => {
      const { enabled } = useShortcutStore.getState();
      expect(enabled).toBe(true);
    });
  });

  describe('updateShortcut', () => {
    it('should update shortcut keys', () => {
      const { updateShortcut, getShortcut } = useShortcutStore.getState();

      updateShortcut('globalSearch', { key: 'f', meta: true, shift: true });

      const shortcut = getShortcut('globalSearch');
      expect(shortcut?.keys.key).toBe('f');
      expect(shortcut?.keys.shift).toBe(true);
    });

    it('should not affect other shortcuts', () => {
      const { updateShortcut, getShortcut } = useShortcutStore.getState();

      updateShortcut('globalSearch', { key: 'x', meta: true });

      const toggle = getShortcut('toggleSidebar');
      expect(toggle?.keys.key).toBe('b');
    });
  });

  describe('resetShortcuts', () => {
    it('should reset to default shortcuts', () => {
      const { updateShortcut, resetShortcuts, shortcuts } =
        useShortcutStore.getState();

      updateShortcut('globalSearch', { key: 'x', meta: true });
      resetShortcuts();

      const after = useShortcutStore.getState().shortcuts;
      expect(after[0].keys.key).toBe('k');
    });
  });

  describe('toggleEnabled', () => {
    it('should toggle enabled state', () => {
      const { toggleEnabled } = useShortcutStore.getState();

      toggleEnabled();
      expect(useShortcutStore.getState().enabled).toBe(false);

      toggleEnabled();
      expect(useShortcutStore.getState().enabled).toBe(true);
    });
  });

  describe('getShortcut', () => {
    it('should return shortcut by action', () => {
      const { getShortcut } = useShortcutStore.getState();
      const shortcut = getShortcut('globalSearch');

      expect(shortcut?.action).toBe('globalSearch');
      expect(shortcut?.description).toBe('全局搜索');
    });

    it('should return undefined for unknown action', () => {
      const { getShortcut } = useShortcutStore.getState();
      const shortcut = getShortcut('unknownAction' as never);

      expect(shortcut).toBeUndefined();
    });
  });
});