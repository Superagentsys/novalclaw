/**
 * Layout Store Tests
 *
 * Unit tests for layout state management.
 *
 * [Source: Story 10.4 - 界面布局自定义]
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { useLayoutStore } from './layoutStore';
import { DEFAULT_LAYOUT } from '@/types/layout';

describe('LayoutStore', () => {
  beforeEach(() => {
    // Reset store before each test
    useLayoutStore.setState({
      currentLayout: DEFAULT_LAYOUT,
      presets: [DEFAULT_LAYOUT],
    });
  });

  describe('initial state', () => {
    it('should have default layout', () => {
      const { currentLayout } = useLayoutStore.getState();
      expect(currentLayout.id).toBe('default');
      expect(currentLayout.visibility.sidebar).toBe(true);
    });

    it('should have default preset in presets', () => {
      const { presets } = useLayoutStore.getState();
      expect(presets).toHaveLength(1);
      expect(presets[0].id).toBe('default');
    });
  });

  describe('setVisibility', () => {
    it('should set panel visibility', () => {
      const { setVisibility } = useLayoutStore.getState();
      setVisibility('sidebar', false);

      const { currentLayout } = useLayoutStore.getState();
      expect(currentLayout.visibility.sidebar).toBe(false);
    });

    it('should not affect other panels', () => {
      const { setVisibility } = useLayoutStore.getState();
      setVisibility('rightPanel', true);

      const { currentLayout } = useLayoutStore.getState();
      expect(currentLayout.visibility.rightPanel).toBe(true);
      expect(currentLayout.visibility.sidebar).toBe(true);
    });
  });

  describe('togglePanel', () => {
    it('should toggle panel visibility', () => {
      const { togglePanel } = useLayoutStore.getState();

      togglePanel('sidebar');
      expect(useLayoutStore.getState().currentLayout.visibility.sidebar).toBe(false);

      togglePanel('sidebar');
      expect(useLayoutStore.getState().currentLayout.visibility.sidebar).toBe(true);
    });
  });

  describe('setSize', () => {
    it('should set panel size', () => {
      const { setSize } = useLayoutStore.getState();
      setSize('sidebarWidth', 300);

      const { currentLayout } = useLayoutStore.getState();
      expect(currentLayout.sizes.sidebarWidth).toBe(300);
    });
  });

  describe('savePreset', () => {
    it('should save current layout as preset', () => {
      const { savePreset } = useLayoutStore.getState();
      savePreset('我的布局');

      const { presets } = useLayoutStore.getState();
      expect(presets).toHaveLength(2);
      expect(presets[1].name).toBe('我的布局');
    });

    it('should generate unique id for each preset', async () => {
      const { savePreset } = useLayoutStore.getState();
      savePreset('布局1');

      // Small delay to ensure different timestamp
      await new Promise((r) => setTimeout(r, 10));
      savePreset('布局2');

      const { presets } = useLayoutStore.getState();
      expect(presets[1].id).not.toBe(presets[2].id);
    });
  });

  describe('loadPreset', () => {
    it('should load a preset', () => {
      const { setVisibility, savePreset, loadPreset } = useLayoutStore.getState();

      // Modify and save
      setVisibility('sidebar', false);
      savePreset('隐藏侧边栏');

      // Reset
      useLayoutStore.setState({ currentLayout: DEFAULT_LAYOUT });

      // Load saved preset
      const savedPresetId = useLayoutStore.getState().presets[1].id;
      loadPreset(savedPresetId);

      const { currentLayout } = useLayoutStore.getState();
      expect(currentLayout.visibility.sidebar).toBe(false);
    });

    it('should not change if preset not found', () => {
      const { loadPreset } = useLayoutStore.getState();
      loadPreset('nonexistent');

      const { currentLayout } = useLayoutStore.getState();
      expect(currentLayout.id).toBe('default');
    });
  });

  describe('deletePreset', () => {
    it('should delete a preset', () => {
      const { savePreset, deletePreset } = useLayoutStore.getState();
      savePreset('测试预设');

      const presetId = useLayoutStore.getState().presets[1].id;
      deletePreset(presetId);

      const { presets } = useLayoutStore.getState();
      expect(presets).toHaveLength(1);
    });

    it('should not delete default preset', () => {
      const { deletePreset } = useLayoutStore.getState();
      deletePreset('default');

      const { presets } = useLayoutStore.getState();
      expect(presets).toHaveLength(1);
      expect(presets[0].id).toBe('default');
    });
  });

  describe('resetToDefault', () => {
    it('should reset to default layout', () => {
      const { setVisibility, resetToDefault } = useLayoutStore.getState();

      setVisibility('sidebar', false);
      expect(useLayoutStore.getState().currentLayout.visibility.sidebar).toBe(false);

      resetToDefault();
      expect(useLayoutStore.getState().currentLayout.visibility.sidebar).toBe(true);
    });
  });
});