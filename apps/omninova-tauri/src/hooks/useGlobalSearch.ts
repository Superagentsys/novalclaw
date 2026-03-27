/**
 * Global Search Hook
 *
 * Provides global search functionality across sessions, agents, messages, and memories.
 *
 * [Source: Story 10.3 - 全局搜索功能]
 */

import { useState, useCallback, useMemo } from 'react';
import { useSessionStore } from '@/stores/sessionStore';
import { useNavigationStore } from '@/stores/navigationStore';
import type {
  SearchResult,
  SearchResultsGroup,
  SearchResultType,
} from '@/types/search';
import {
  SEARCH_TYPE_LABELS,
  SEARCH_TYPE_ORDER,
} from '@/types/search';

// ============================================================================
// Types
// ============================================================================

export interface UseGlobalSearchOptions {
  /** Minimum query length to trigger search */
  minQueryLength?: number;
  /** Maximum results per category */
  maxResultsPerCategory?: number;
}

export interface UseGlobalSearchReturn {
  /** Current search query */
  query: string;
  /** Set search query */
  setQuery: (query: string) => void;
  /** Is search panel open */
  isOpen: boolean;
  /** Is search in progress */
  isSearching: boolean;
  /** Flat search results */
  results: SearchResult[];
  /** Grouped search results */
  groupedResults: SearchResultsGroup[];
  /** Total result count */
  totalResults: number;
  /** Open search panel */
  openSearch: () => void;
  /** Close search panel */
  closeSearch: () => void;
  /** Clear search query */
  clearSearch: () => void;
}

// ============================================================================
// Hook
// ============================================================================

/**
 * Global search hook
 *
 * Provides search functionality across multiple data sources:
 * - Sessions (conversations)
 * - Agents
 * - Messages
 * - Memories
 *
 * @example
 * ```tsx
 * function SearchComponent() {
 *   const { query, setQuery, groupedResults, isOpen, openSearch, closeSearch } = useGlobalSearch();
 *
 *   return (
 *     <Dialog open={isOpen} onOpenChange={(open) => !open && closeSearch()}>
 *       <Input value={query} onChange={(e) => setQuery(e.target.value)} />
 *       // ... render results
 *     </Dialog>
 *   );
 * }
 * ```
 */
export function useGlobalSearch({
  minQueryLength = 2,
  maxResultsPerCategory = 5,
}: UseGlobalSearchOptions = {}): UseGlobalSearchReturn {
  // State
  const [query, setQuery] = useState('');
  const [isOpen, setIsOpen] = useState(false);
  const [isSearching, setIsSearching] = useState(false);

  // Data sources
  const sessions = useSessionStore((s) => s.sessions);

  // Perform search
  const results = useMemo((): SearchResult[] => {
    // Check minimum query length
    if (query.trim().length < minQueryLength) {
      return [];
    }

    const searchResults: SearchResult[] = [];
    const lowerQuery = query.toLowerCase().trim();

    // Search sessions
    const sessionResults = sessions
      .filter((session) => {
        const title = session.title?.toLowerCase() ?? '';
        return title.includes(lowerQuery);
      })
      .slice(0, maxResultsPerCategory)
      .map((session): SearchResult => ({
        id: `session-${session.id}`,
        type: 'session',
        title: session.title || '新对话',
        sourceId: session.id,
        snippet: session.title
          ? createSnippet(session.title, lowerQuery)
          : undefined,
      }));

    searchResults.push(...sessionResults);

    // TODO: Add agent search when agent store is available
    // TODO: Add message search when message search API is available
    // TODO: Add memory search when memory store is available

    return searchResults;
  }, [query, sessions, minQueryLength, maxResultsPerCategory]);

  // Group results by type
  const groupedResults = useMemo((): SearchResultsGroup[] => {
    const groups: Record<SearchResultType, SearchResultsGroup> = {
      session: { type: 'session', label: SEARCH_TYPE_LABELS.session, results: [] },
      agent: { type: 'agent', label: SEARCH_TYPE_LABELS.agent, results: [] },
      message: { type: 'message', label: SEARCH_TYPE_LABELS.message, results: [] },
      memory: { type: 'memory', label: SEARCH_TYPE_LABELS.memory, results: [] },
    };

    results.forEach((result) => {
      const group = groups[result.type];
      if (group) {
        group.results.push(result);
      }
    });

    // Return groups in display order, filtering empty ones
    return SEARCH_TYPE_ORDER
      .map((type) => groups[type])
      .filter((group) => group.results.length > 0);
  }, [results]);

  // Total results count
  const totalResults = results.length;

  // Open search panel
  const openSearch = useCallback(() => {
    setIsOpen(true);
  }, []);

  // Close search panel
  const closeSearch = useCallback(() => {
    setIsOpen(false);
    setQuery('');
  }, []);

  // Clear search (keep panel open)
  const clearSearch = useCallback(() => {
    setQuery('');
  }, []);

  return {
    query,
    setQuery,
    isOpen,
    isSearching,
    results,
    groupedResults,
    totalResults,
    openSearch,
    closeSearch,
    clearSearch,
  };
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Create a snippet with context around the match
 */
function createSnippet(text: string, query: string, contextChars = 30): string {
  const lowerText = text.toLowerCase();
  const matchIndex = lowerText.indexOf(query);

  if (matchIndex === -1) {
    return text.slice(0, 50) + (text.length > 50 ? '...' : '');
  }

  const start = Math.max(0, matchIndex - contextChars);
  const end = Math.min(text.length, matchIndex + query.length + contextChars);

  let snippet = text.slice(start, end);

  if (start > 0) {
    snippet = '...' + snippet;
  }
  if (end < text.length) {
    snippet = snippet + '...';
  }

  return snippet;
}

export default useGlobalSearch;