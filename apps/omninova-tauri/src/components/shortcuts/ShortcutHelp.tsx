/**
 * Shortcut Help Component
 *
 * Displays keyboard shortcut reference.
 *
 * [Source: Story 10.6 - 键盘快捷键]
 */

import * as React from 'react';
import { memo } from 'react';
import { cn } from '@/lib/utils';
import { useShortcutStore } from '@/stores/shortcutStore';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import { Keyboard, RotateCcw } from 'lucide-react';
import { getModifierKey } from '@/types/navigation';

// ============================================================================
// Types
// ============================================================================

export interface ShortcutHelpProps {
  /** Additional CSS classes */
  className?: string;
  /** Whether dialog is open (controlled) */
  open?: boolean;
  /** Called when open state changes */
  onOpenChange?: (open: boolean) => void;
}

// ============================================================================
// Component
// ============================================================================

/**
 * Shortcut help component
 *
 * Displays a dialog with all keyboard shortcuts.
 */
export const ShortcutHelp = memo(function ShortcutHelp({
  className,
  open,
  onOpenChange,
}: ShortcutHelpProps): React.ReactElement {
  // Store state
  const shortcuts = useShortcutStore((s) => s.shortcuts);
  const resetShortcuts = useShortcutStore((s) => s.resetShortcuts);

  const modKey = getModifierKey();
  const modLabel = modKey === 'metaKey' ? '⌘' : 'Ctrl';

  const formatKey = (key: string): string => {
    // Capitalize single letters
    if (key.length === 1) {
      return key.toUpperCase();
    }
    return key;
  };

  const formatShortcut = (keys: {
    key: string;
    meta?: boolean;
    shift?: boolean;
    alt?: boolean;
  }): string => {
    const parts: string[] = [];
    if (keys.meta) parts.push(modLabel);
    if (keys.shift) parts.push('⇧');
    if (keys.alt) parts.push('⌥');
    parts.push(formatKey(keys.key));
    return parts.join(' + ');
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className={cn('gap-2', className)}
        >
          <Keyboard className="w-4 h-4" />
          快捷键
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>键盘快捷键</DialogTitle>
        </DialogHeader>
        <div className="mt-4 space-y-1">
          {shortcuts.map((shortcut) => (
            <div
              key={shortcut.action}
              className="flex items-center justify-between py-2 px-3 rounded-lg hover:bg-accent"
            >
              <span className="text-sm">{shortcut.description}</span>
              <kbd className="px-2 py-1 text-xs font-mono bg-muted rounded">
                {formatShortcut(shortcut.keys)}
              </kbd>
            </div>
          ))}
        </div>
        <div className="mt-4 flex justify-end">
          <Button
            variant="outline"
            size="sm"
            onClick={resetShortcuts}
            className="gap-2"
          >
            <RotateCcw className="w-4 h-4" />
            重置为默认
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
});

export default ShortcutHelp;