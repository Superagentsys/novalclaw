/**
 * Search Shortcut Hook
 *
 * Provides keyboard shortcut handling for opening global search.
 * Uses Ctrl+K on Windows/Linux and Cmd+K on macOS.
 *
 * [Source: Story 10.3 - 全局搜索功能]
 */

import { useEffect, useCallback } from 'react';

// ============================================================================
// Types
// ============================================================================

export interface UseSearchShortcutOptions {
  /** Whether the shortcut is enabled */
  enabled?: boolean;
}

// ============================================================================
// Constants
// ============================================================================

/**
 * Get the modifier key for the current platform
 */
const getModifierKey = (): 'metaKey' | 'ctrlKey' => {
  if (typeof navigator === 'undefined') return 'ctrlKey';
  return navigator.platform.toUpperCase().indexOf('MAC') >= 0
    ? 'metaKey'
    : 'ctrlKey';
};

// ============================================================================
// Hook
// ============================================================================

/**
 * Hook for handling search keyboard shortcut
 *
 * Listens for Ctrl+K (Windows/Linux) or Cmd+K (macOS) and triggers
 * the provided callback when pressed.
 *
 * @example
 * ```tsx
 * function App() {
 *   const { isOpen, openSearch } = useGlobalSearch();
 *
 *   // Enable search shortcut globally
 *   useSearchShortcut({ onTrigger: openSearch });
 *
 *   return <MainLayout />;
 * }
 * ```
 */
export function useSearchShortcut(
  onTrigger: () => void,
  options: UseSearchShortcutOptions = {}
): void {
  const { enabled = true } = options;

  // Handle keyboard event
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (!enabled) return;

      // Check for Ctrl+K (Windows/Linux) or Cmd+K (macOS)
      const modifierKey = getModifierKey();

      if (e[modifierKey] && e.key === 'k') {
        e.preventDefault();
        onTrigger();
      }
    },
    [onTrigger, enabled]
  );

  // Register event listener
  useEffect(() => {
    if (!enabled) return;

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown, enabled]);
}

export default useSearchShortcut;