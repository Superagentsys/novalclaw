/**
 * Accessibility Store
 *
 * State management for accessibility settings.
 *
 * [Source: Story 10.7 - 无障碍访问]
 */

import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { AccessibilitySettings } from '@/types/accessibility';
import { DEFAULT_ACCESSIBILITY, ZOOM_LEVELS } from '@/types/accessibility';

// ============================================================================
// Types
// ============================================================================

interface AccessibilityState extends AccessibilitySettings {}

interface AccessibilityActions {
  toggleHighContrast: () => void;
  toggleLargeText: () => void;
  toggleReduceMotion: () => void;
  toggleScreenReaderMode: () => void;
  setZoomLevel: (level: number) => void;
  zoomIn: () => void;
  zoomOut: () => void;
  resetToDefaults: () => void;
}

export type AccessibilityStore = AccessibilityState & AccessibilityActions;

// ============================================================================
// Store
// ============================================================================

export const useAccessibilityStore = create<AccessibilityStore>()(
  persist(
    (set, get) => ({
      ...DEFAULT_ACCESSIBILITY,

      toggleHighContrast: () => {
        set((state) => ({ highContrast: !state.highContrast }));
      },

      toggleLargeText: () => {
        set((state) => ({ largeText: !state.largeText }));
      },

      toggleReduceMotion: () => {
        set((state) => ({ reduceMotion: !state.reduceMotion }));
      },

      toggleScreenReaderMode: () => {
        set((state) => ({ screenReaderMode: !state.screenReaderMode }));
      },

      setZoomLevel: (level) => {
        if (ZOOM_LEVELS.includes(level as never)) {
          set({ zoomLevel: level });
        }
      },

      zoomIn: () => {
        const { zoomLevel } = get();
        const currentIndex = ZOOM_LEVELS.indexOf(zoomLevel as never);
        if (currentIndex < ZOOM_LEVELS.length - 1) {
          set({ zoomLevel: ZOOM_LEVELS[currentIndex + 1] });
        }
      },

      zoomOut: () => {
        const { zoomLevel } = get();
        const currentIndex = ZOOM_LEVELS.indexOf(zoomLevel as never);
        if (currentIndex > 0) {
          set({ zoomLevel: ZOOM_LEVELS[currentIndex - 1] });
        }
      },

      resetToDefaults: () => {
        set(DEFAULT_ACCESSIBILITY);
      },
    }),
    {
      name: 'omninova-accessibility-storage',
    }
  )
);