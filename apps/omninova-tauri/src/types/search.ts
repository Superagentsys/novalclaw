/**
 * Search Types
 *
 * Type definitions for global search functionality.
 *
 * [Source: Story 10.3 - 全局搜索功能]
 */

// ============================================================================
// Types
// ============================================================================

/**
 * Search result type categories
 */
export type SearchResultType = 'session' | 'agent' | 'message' | 'memory';

/**
 * Individual search result item
 */
export interface SearchResult {
  /** Unique identifier */
  id: string;
  /** Result type */
  type: SearchResultType;
  /** Display title */
  title: string;
  /** Description/summary */
  description?: string;
  /** Matching text snippet */
  snippet?: string;
  /** Source ID (e.g., agentId, sessionId) */
  sourceId?: number;
  /** Optional secondary ID (e.g., messageId) */
  secondaryId?: number;
  /** Match position in original text */
  matchStart?: number;
  matchEnd?: number;
}

/**
 * Grouped search results
 */
export interface SearchResultsGroup {
  /** Group type */
  type: SearchResultType;
  /** Display label */
  label: string;
  /** Results in this group */
  results: SearchResult[];
}

/**
 * Search state
 */
export interface SearchState {
  /** Search query */
  query: string;
  /** Is search in progress */
  isSearching: boolean;
  /** Search results */
  results: SearchResult[];
  /** Is search panel open */
  isOpen: boolean;
}

/**
 * Search actions
 */
export interface SearchActions {
  /** Set search query */
  setQuery: (query: string) => void;
  /** Open search panel */
  openSearch: () => void;
  /** Close search panel */
  closeSearch: () => void;
  /** Clear search */
  clearSearch: () => void;
}

/**
 * Combined search store type
 */
export type SearchStore = SearchState & SearchActions;

// ============================================================================
// Constants
// ============================================================================

/**
 * Labels for each result type
 */
export const SEARCH_TYPE_LABELS: Record<SearchResultType, string> = {
  session: '对话',
  agent: '代理',
  message: '消息',
  memory: '记忆',
};

/**
 * Order of result types in display
 */
export const SEARCH_TYPE_ORDER: SearchResultType[] = [
  'session',
  'agent',
  'message',
  'memory',
];