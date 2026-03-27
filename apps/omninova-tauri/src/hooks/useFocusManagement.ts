/**
 * Focus Management Hook
 *
 * Hooks for managing keyboard focus and focus traps.
 *
 * [Source: Story 10.7 - 无障碍访问]
 */

import { useRef, useCallback, useEffect } from 'react';
import type { RefObject } from 'react';

// ============================================================================
// Types
// ============================================================================

/**
 * Focus management return type
 */
export interface FocusManagementReturn<T extends HTMLElement> {
  /** Ref to attach to the element */
  ref: RefObject<T | null>;
  /** Focus the element */
  focus: () => void;
  /** Blur the element */
  blur: () => void;
}

// ============================================================================
// Hooks
// ============================================================================

/**
 * Hook for managing focus on an element
 *
 * @example
 * ```tsx
 * const { ref, focus } = useFocusManagement<HTMLInputElement>();
 * // Later: focus() to focus the input
 * ```
 */
export function useFocusManagement<
  T extends HTMLElement,
>(): FocusManagementReturn<T> {
  const ref = useRef<T>(null);

  const focus = useCallback(() => {
    ref.current?.focus();
  }, []);

  const blur = useCallback(() => {
    ref.current?.blur();
  }, []);

  return { ref, focus, blur };
}

/**
 * Hook for trapping focus within a container (for modals, dialogs, etc.)
 *
 * @param containerRef - Ref to the container element
 *
 * @example
 * ```tsx
 * const containerRef = useRef<HTMLDivElement>(null);
 * useFocusTrap(containerRef);
 * return <div ref={containerRef}>...</div>;
 * ```
 */
export function useFocusTrap(
  containerRef: RefObject<HTMLElement | null>
): void {
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const focusableSelector =
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])';

    const getFocusableElements = () =>
      container.querySelectorAll<HTMLElement>(focusableSelector);

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key !== 'Tab') return;

      const focusableElements = getFocusableElements();
      if (focusableElements.length === 0) return;

      const firstElement = focusableElements[0];
      const lastElement = focusableElements[focusableElements.length - 1];

      if (e.shiftKey) {
        // Shift+Tab - go to last element if on first
        if (document.activeElement === firstElement) {
          e.preventDefault();
          lastElement?.focus();
        }
      } else {
        // Tab - go to first element if on last
        if (document.activeElement === lastElement) {
          e.preventDefault();
          firstElement?.focus();
        }
      }
    };

    container.addEventListener('keydown', handleKeyDown);
    return () => container.removeEventListener('keydown', handleKeyDown);
  }, [containerRef]);
}

/**
 * Hook for auto-focusing an element on mount
 *
 * @param options - Focus options
 *
 * @example
 * ```tsx
 * const inputRef = useAutoFocus<HTMLInputElement>();
 * return <input ref={inputRef} />;
 * ```
 */
export function useAutoFocus<T extends HTMLElement>(
  options?: FocusOptions
): RefObject<T | null> {
  const ref = useRef<T>(null);

  useEffect(() => {
    // Small delay to ensure element is rendered
    const timer = setTimeout(() => {
      ref.current?.focus(options);
    }, 0);
    return () => clearTimeout(timer);
  }, [options]);

  return ref;
}