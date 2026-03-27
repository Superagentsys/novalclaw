/**
 * Session Search Input Component
 *
 * Search input for filtering sessions by title.
 *
 * [Source: Story 10.2 - 历史对话导航]
 */

import * as React from 'react';
import { memo, useCallback } from 'react';
import { cn } from '@/lib/utils';
import { Input } from '@/components/ui/input';
import { Search, X } from 'lucide-react';
import { Button } from '@/components/ui/button';

// ============================================================================
// Types
// ============================================================================

export interface SessionSearchInputProps {
  /** Current search value */
  value: string;
  /** Change handler */
  onChange: (value: string) => void;
  /** Placeholder text */
  placeholder?: string;
  /** Additional CSS classes */
  className?: string;
}

// ============================================================================
// Component
// ============================================================================

/**
 * Session search input component
 *
 * Provides a search input with clear button for filtering sessions.
 */
export const SessionSearchInput = memo(function SessionSearchInput({
  value,
  onChange,
  placeholder = '搜索...',
  className,
}: SessionSearchInputProps): React.ReactElement {
  // Handle input change
  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      onChange(e.target.value);
    },
    [onChange]
  );

  // Handle clear
  const handleClear = useCallback(() => {
    onChange('');
  }, [onChange]);

  return (
    <div className={cn('relative', className)}>
      {/* Search icon */}
      <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />

      {/* Input */}
      <Input
        type="text"
        value={value}
        onChange={handleChange}
        placeholder={placeholder}
        className="pl-8 pr-8 h-8 text-sm"
      />

      {/* Clear button */}
      {value && (
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="absolute right-0.5 top-1/2 -translate-y-1/2 h-7 w-7"
          onClick={handleClear}
        >
          <X className="w-3 h-3" />
        </Button>
      )}
    </div>
  );
});

export default SessionSearchInput;