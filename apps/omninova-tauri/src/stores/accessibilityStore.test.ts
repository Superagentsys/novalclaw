/**
 * Accessibility Store Tests
 *
 * Unit tests for accessibility settings management.
 *
 * [Source: Story 10.7 - 无障碍访问]
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { useAccessibilityStore } from './accessibilityStore';
import { DEFAULT_ACCESSIBILITY, ZOOM_LEVELS } from '@/types/accessibility';

describe('AccessibilityStore', () => {
  beforeEach(() => {
    // Reset store before each test
    useAccessibilityStore.setState(DEFAULT_ACCESSIBILITY);
  });

  describe('initial state', () => {
    it('should have default settings', () => {
      const state = useAccessibilityStore.getState();
      expect(state.highContrast).toBe(false);
      expect(state.largeText).toBe(false);
      expect(state.reduceMotion).toBe(false);
      expect(state.zoomLevel).toBe(100);
      expect(state.screenReaderMode).toBe(false);
    });
  });

  describe('toggleHighContrast', () => {
    it('should toggle high contrast mode', () => {
      const { toggleHighContrast } = useAccessibilityStore.getState();

      toggleHighContrast();
      expect(useAccessibilityStore.getState().highContrast).toBe(true);

      toggleHighContrast();
      expect(useAccessibilityStore.getState().highContrast).toBe(false);
    });
  });

  describe('toggleLargeText', () => {
    it('should toggle large text mode', () => {
      const { toggleLargeText } = useAccessibilityStore.getState();

      toggleLargeText();
      expect(useAccessibilityStore.getState().largeText).toBe(true);
    });
  });

  describe('toggleReduceMotion', () => {
    it('should toggle reduce motion mode', () => {
      const { toggleReduceMotion } = useAccessibilityStore.getState();

      toggleReduceMotion();
      expect(useAccessibilityStore.getState().reduceMotion).toBe(true);
    });
  });

  describe('toggleScreenReaderMode', () => {
    it('should toggle screen reader mode', () => {
      const { toggleScreenReaderMode } = useAccessibilityStore.getState();

      toggleScreenReaderMode();
      expect(useAccessibilityStore.getState().screenReaderMode).toBe(true);
    });
  });

  describe('setZoomLevel', () => {
    it('should set valid zoom level', () => {
      const { setZoomLevel } = useAccessibilityStore.getState();

      setZoomLevel(150);
      expect(useAccessibilityStore.getState().zoomLevel).toBe(150);
    });

    it('should ignore invalid zoom level', () => {
      const { setZoomLevel } = useAccessibilityStore.getState();

      setZoomLevel(123);
      expect(useAccessibilityStore.getState().zoomLevel).toBe(100);
    });
  });

  describe('zoomIn', () => {
    it('should increase zoom level', () => {
      const { zoomIn } = useAccessibilityStore.getState();

      zoomIn();
      expect(useAccessibilityStore.getState().zoomLevel).toBe(110);

      zoomIn();
      expect(useAccessibilityStore.getState().zoomLevel).toBe(125);
    });

    it('should not exceed max zoom level', () => {
      useAccessibilityStore.setState({ zoomLevel: 200 });
      const { zoomIn } = useAccessibilityStore.getState();

      zoomIn();
      expect(useAccessibilityStore.getState().zoomLevel).toBe(200);
    });
  });

  describe('zoomOut', () => {
    it('should decrease zoom level', () => {
      useAccessibilityStore.setState({ zoomLevel: 125 });
      const { zoomOut } = useAccessibilityStore.getState();

      zoomOut();
      expect(useAccessibilityStore.getState().zoomLevel).toBe(110);
    });

    it('should not go below min zoom level', () => {
      useAccessibilityStore.setState({ zoomLevel: 75 });
      const { zoomOut } = useAccessibilityStore.getState();

      zoomOut();
      expect(useAccessibilityStore.getState().zoomLevel).toBe(75);
    });
  });

  describe('resetToDefaults', () => {
    it('should reset all settings to defaults', () => {
      const store = useAccessibilityStore.getState();
      store.toggleHighContrast();
      store.toggleLargeText();
      store.setZoomLevel(150);

      store.resetToDefaults();

      const state = useAccessibilityStore.getState();
      expect(state.highContrast).toBe(false);
      expect(state.largeText).toBe(false);
      expect(state.zoomLevel).toBe(100);
    });
  });
});