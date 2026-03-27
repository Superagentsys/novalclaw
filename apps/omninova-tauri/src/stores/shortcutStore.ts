/**
 * Shortcut Store
 *
 * State management for keyboard shortcuts.
 *
 * [Source: Story 10.6 - 键盘快捷键]
 */

import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type {
  ShortcutConfig,
  ShortcutAction,
  ShortcutKey,
} from '@/types/shortcut';
import { DEFAULT_SHORTCUTS } from '@/types/shortcut';

// ============================================================================
// Types
// ============================================================================

interface ShortcutState {
  shortcuts: ShortcutConfig[];
  enabled: boolean;
}

interface ShortcutActions {
  updateShortcut: (action: ShortcutAction, keys: ShortcutKey) => void;
  resetShortcuts: () => void;
  toggleEnabled: () => void;
  getShortcut: (action: ShortcutAction) => ShortcutConfig | undefined;
}

export type ShortcutStore = ShortcutState & ShortcutActions;

// ============================================================================
// Store
// ============================================================================

export const useShortcutStore = create<ShortcutStore>()(
  persist(
    (set, get) => ({
      shortcuts: DEFAULT_SHORTCUTS,
      enabled: true,

      updateShortcut: (action, keys) => {
        set((state) => ({
          shortcuts: state.shortcuts.map((s) =>
            s.action === action ? { ...s, keys } : s
          ),
        }));
      },

      resetShortcuts: () => {
        set({ shortcuts: DEFAULT_SHORTCUTS });
      },

      toggleEnabled: () => {
        set((state) => ({ enabled: !state.enabled }));
      },

      getShortcut: (action) => {
        return get().shortcuts.find((s) => s.action === action);
      },
    }),
    {
      name: 'omninova-shortcut-storage',
    }
  )
);