/**
 * Global Shortcuts Hook
 *
 * Hook for handling global keyboard shortcuts.
 *
 * [Source: Story 10.6 - 键盘快捷键]
 */

import { useEffect, useCallback } from 'react';
import { useShortcutStore } from '@/stores/shortcutStore';
import { getModifierKey } from '@/types/navigation';

// ============================================================================
// Types
// ============================================================================

export type ShortcutHandlers = Record<string, () => void>;

// ============================================================================
// Hook
// ============================================================================

/**
 * Hook for handling global keyboard shortcuts
 *
 * @param handlers - Map of action names to handler functions
 *
 * @example
 * ```tsx
 * useGlobalShortcuts({
 *   globalSearch: () => setSearchOpen(true),
 *   toggleSidebar: () => toggleSidebar(),
 *   newSession: () => createSession(),
 * });
 * ```
 */
export function useGlobalShortcuts(handlers: ShortcutHandlers): void {
  const shortcuts = useShortcutStore((s) => s.shortcuts);
  const enabled = useShortcutStore((s) => s.enabled);

  const handleKeyDown = useCallback(
    (event: KeyboardEvent) => {
      if (!enabled) return;

      // Don't trigger shortcuts when typing in input fields
      const target = event.target as HTMLElement;
      if (
        target.tagName === 'INPUT' ||
        target.tagName === 'TEXTAREA' ||
        target.isContentEditable
      ) {
        // Allow specific shortcuts in input fields
        const isInputAllowed =
          event.key === 'Escape' ||
          (event.key === 'Enter' && !event.shiftKey);
        if (!isInputAllowed) return;
      }

      const modKey = getModifierKey();
      const metaPressed = event[modKey];
      const shiftPressed = event.shiftKey;
      const altPressed = event.altKey;

      for (const shortcut of shortcuts) {
        const { keys, action } = shortcut;
        const keyMatches = event.key.toLowerCase() === keys.key.toLowerCase();
        const metaMatches = keys.meta ? metaPressed : !metaPressed;
        const shiftMatches = keys.shift ? shiftPressed : !shiftPressed;
        const altMatches = keys.alt ? altPressed : !altPressed;

        if (keyMatches && metaMatches && shiftMatches && altMatches) {
          const handler = handlers[action];
          if (handler) {
            event.preventDefault();
            handler();
          }
          break;
        }
      }
    },
    [shortcuts, enabled, handlers]
  );

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);
}