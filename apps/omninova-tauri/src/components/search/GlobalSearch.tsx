/**
 * Global Search Component
 *
 * Modal search panel for searching across sessions, agents, messages, and memories.
 * Supports keyboard navigation and quick selection.
 *
 * [Source: Story 10.3 - 全局搜索功能]
 */

import * as React from 'react';
import { useEffect, useRef, useState, useCallback } from 'react';
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';
import { useGlobalSearch } from '@/hooks/useGlobalSearch';
import { Search, MessageSquare, User, Brain, FileText, X } from 'lucide-react';
import type { SearchResult, SearchResultType } from '@/types/search';
import { SEARCH_TYPE_LABELS } from '@/types/search';

// ============================================================================
// Types
// ============================================================================

export interface GlobalSearchProps {
  /** Handler when a result is selected */
  onSelectResult?: (result: SearchResult) => void;
  /** Additional CSS classes */
  className?: string;
}

// ============================================================================
// Constants
// ============================================================================

/**
 * Icons for each result type
 */
const RESULT_TYPE_ICONS: Record<SearchResultType, React.ComponentType<{ className?: string }>> = {
  session: FileText,
  agent: User,
  message: MessageSquare,
  memory: Brain,
};

// ============================================================================
// Component
// ============================================================================

/**
 * Global search panel component
 *
 * Features:
 * - Modal search panel triggered by Ctrl/Cmd+K
 * - Search across sessions, agents, messages, memories
 * - Keyboard navigation (arrows + enter)
 * - Grouped results by type
 */
export function GlobalSearch({
  onSelectResult,
  className,
}: GlobalSearchProps): React.ReactElement {
  // Search state
  const {
    query,
    setQuery,
    isOpen,
    groupedResults,
    totalResults,
    closeSearch,
  } = useGlobalSearch();

  // Selection state
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const resultsRef = useRef<HTMLDivElement>(null);

  // Flatten results for keyboard navigation
  const flatResults = groupedResults.flatMap((g) => g.results);

  // Reset selection when results change
  useEffect(() => {
    setSelectedIndex(0);
  }, [query, groupedResults]);

  // Focus input on open
  useEffect(() => {
    if (isOpen) {
      // Small delay to ensure dialog is mounted
      requestAnimationFrame(() => {
        inputRef.current?.focus();
      });
    }
  }, [isOpen]);

  // Handle result selection
  const handleSelect = useCallback(
    (result: SearchResult) => {
      onSelectResult?.(result);
      closeSearch();
    },
    [onSelectResult, closeSearch]
  );

  // Handle keyboard navigation
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      switch (e.key) {
        case 'ArrowDown':
          e.preventDefault();
          setSelectedIndex((i) => Math.min(i + 1, flatResults.length - 1));
          break;
        case 'ArrowUp':
          e.preventDefault();
          setSelectedIndex((i) => Math.max(i - 1, 0));
          break;
        case 'Enter':
          e.preventDefault();
          if (flatResults[selectedIndex]) {
            handleSelect(flatResults[selectedIndex]);
          }
          break;
        case 'Escape':
          e.preventDefault();
          closeSearch();
          break;
      }
    },
    [flatResults, selectedIndex, handleSelect, closeSearch]
  );

  // Clear search
  const handleClear = useCallback(() => {
    setQuery('');
    inputRef.current?.focus();
  }, [setQuery]);

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && closeSearch()}>
      <DialogContent
        className={cn(
          'max-w-xl p-0 gap-0',
          'max-h-[70vh] flex flex-col',
          className
        )}
        onOpenAutoFocus={(e) => e.preventDefault()}
      >
        {/* Visually hidden title for accessibility */}
        <DialogTitle className="sr-only">全局搜索</DialogTitle>

        {/* Search input */}
        <div className="flex items-center border-b px-3 py-2 gap-2">
          <Search className="w-4 h-4 text-muted-foreground flex-shrink-0" />
          <Input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="搜索对话、代理、记忆..."
            className="border-0 focus-visible:ring-0 h-8"
          />
          {query && (
            <button
              type="button"
              onClick={handleClear}
              className="p-1 hover:bg-muted rounded"
            >
              <X className="w-4 h-4 text-muted-foreground" />
            </button>
          )}
        </div>

        {/* Results */}
        <div ref={resultsRef} className="flex-1 overflow-y-auto">
          {groupedResults.length === 0 ? (
            query.trim().length >= 2 ? (
              <div className="px-4 py-8 text-center text-sm text-muted-foreground">
                没有找到 "{query}" 相关结果
              </div>
            ) : (
              <div className="px-4 py-8 text-center text-sm text-muted-foreground">
                输入至少 2 个字符开始搜索
              </div>
            )
          ) : (
            groupedResults.map((group) => {
              const Icon = RESULT_TYPE_ICONS[group.type];
              let globalIndex = flatResults.findIndex(
                (r) => r.id === group.results[0]?.id
              );

              return (
                <div key={group.type}>
                  {/* Group header */}
                  <div className="px-4 py-1.5 text-xs font-medium text-muted-foreground bg-muted/50 flex items-center gap-2">
                    <Icon className="w-3 h-3" />
                    {group.label}
                  </div>

                  {/* Group results */}
                  <ul role="listbox">
                    {group.results.map((result) => {
                      const currentIndex = globalIndex++;
                      const isSelected = currentIndex === selectedIndex;

                      return (
                        <li key={result.id} role="option" aria-selected={isSelected}>
                          <button
                            type="button"
                            onClick={() => handleSelect(result)}
                            onMouseEnter={() => setSelectedIndex(currentIndex)}
                            className={cn(
                              'w-full px-4 py-2 text-left flex items-start gap-3',
                              'transition-colors',
                              isSelected && 'bg-accent'
                            )}
                          >
                            <Icon className="w-4 h-4 mt-0.5 text-muted-foreground flex-shrink-0" />
                            <div className="flex-1 min-w-0">
                              <div className="text-sm font-medium truncate">
                                {result.title}
                              </div>
                              {result.snippet && (
                                <div className="text-xs text-muted-foreground line-clamp-2 mt-0.5">
                                  {result.snippet}
                                </div>
                              )}
                            </div>
                          </button>
                        </li>
                      );
                    })}
                  </ul>
                </div>
              );
            })
          )}
        </div>

        {/* Footer with hints */}
        <div className="border-t px-3 py-1.5 flex items-center justify-between text-xs text-muted-foreground">
          <span>共 {totalResults} 条结果</span>
          <div className="flex items-center gap-2">
            <kbd className="px-1.5 py-0.5 bg-muted rounded text-[10px]">↑↓</kbd>
            <span>导航</span>
            <kbd className="px-1.5 py-0.5 bg-muted rounded text-[10px]">Enter</kbd>
            <span>选择</span>
            <kbd className="px-1.5 py-0.5 bg-muted rounded text-[10px]">Esc</kbd>
            <span>关闭</span>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

export default GlobalSearch;