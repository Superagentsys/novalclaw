/**
 * Working Memory Types
 *
 * Types for L1 working memory (short-term session context).
 *
 * [Source: Story 5.1 - L1 工作记忆层实现]
 */

// ============================================================================
// Types
// ============================================================================

/**
 * Working memory entry representing a conversation message
 */
export interface WorkingMemoryEntry {
  /** Entry ID */
  id: string;
  /** Role (user, assistant, system) */
  role: string;
  /** Message content */
  content: string;
  /** Timestamp (Unix timestamp as string) */
  timestamp: string;
}

/**
 * Statistics about working memory usage
 */
export interface MemoryStats {
  /** Maximum capacity (number of entries) */
  capacity: number;
  /** Current number of entries in use */
  used: number;
  /** Current session ID if active */
  sessionId: number | null;
  /** Associated agent ID if set */
  agentId: number | null;
}

/**
 * Role type for working memory entries
 */
export type WorkingMemoryRole = 'user' | 'assistant' | 'system';

// ============================================================================
// API Functions
// ============================================================================

/**
 * Get all working memory entries for the current session
 *
 * @param limit - Maximum number of entries to return (0 = all)
 * @returns Array of working memory entries in chronological order
 */
export async function getWorkingMemory(limit: number = 0): Promise<WorkingMemoryEntry[]> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<WorkingMemoryEntry[]>('get_working_memory', { limit });
}

/**
 * Clear all working memory entries
 */
export async function clearWorkingMemory(): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke('clear_working_memory');
}

/**
 * Get working memory statistics
 *
 * @returns Memory statistics including capacity and usage
 */
export async function getMemoryStats(): Promise<MemoryStats> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<MemoryStats>('get_memory_stats');
}

/**
 * Set the session context for working memory
 *
 * @param sessionId - The session ID
 * @param agentId - The agent ID
 */
export async function setWorkingMemorySession(
  sessionId: number,
  agentId: number
): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke('set_working_memory_session', { sessionId, agentId });
}

/**
 * Push a context entry to working memory
 *
 * @param role - The role (user, assistant, system)
 * @param content - The message content
 */
export async function pushWorkingMemoryContext(
  role: WorkingMemoryRole,
  content: string
): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke('push_working_memory_context', { role, content });
}

// ============================================================================
// Episodic Memory Types (Story 5.2)
// ============================================================================

/**
 * Episodic memory entry representing a long-term memory
 */
export interface EpisodicMemory {
  /** Unique identifier */
  id: number;
  /** Agent this memory belongs to */
  agentId: number;
  /** Session this memory originated from (optional) */
  sessionId: number | null;
  /** Memory content */
  content: string;
  /** Importance score (1-10, higher is more important) */
  importance: number;
  /** Whether this memory is marked as important by user */
  isMarked: boolean;
  /** Additional metadata as JSON string */
  metadata: string | null;
  /** Unix timestamp of creation */
  createdAt: number;
}

/**
 * Data for creating a new episodic memory
 */
export interface NewEpisodicMemory {
  /** Agent this memory belongs to */
  agentId: number;
  /** Session this memory originated from (optional) */
  sessionId?: number | null;
  /** Memory content */
  content: string;
  /** Importance score (1-10) */
  importance: number;
  /** Whether this memory is marked as important by user */
  isMarked?: boolean;
  /** Additional metadata as JSON string (optional) */
  metadata?: string | null;
}

/**
 * Statistics about episodic memories
 */
export interface EpisodicMemoryStats {
  /** Total number of memories */
  totalCount: number;
  /** Average importance score */
  avgImportance: number;
  /** Memory count by agent ID */
  byAgent: Record<number, number>;
}

// ============================================================================
// Episodic Memory API Functions
// ============================================================================

/**
 * Store a new episodic memory
 *
 * @param agentId - The agent ID
 * @param content - The memory content
 * @param importance - Importance score (1-10)
 * @param sessionId - Optional session ID
 * @param metadata - Optional metadata as JSON string
 * @returns The ID of the created memory
 */
export async function storeEpisodicMemory(
  agentId: number,
  content: string,
  importance: number,
  sessionId?: number | null,
  metadata?: string | null
): Promise<number> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<number>('store_episodic_memory', {
    agentId,
    sessionId: sessionId ?? null,
    content,
    importance,
    metadata: metadata ?? null,
  });
}

/**
 * Get episodic memories by agent ID with pagination
 *
 * @param agentId - The agent ID
 * @param limit - Maximum number of entries to return
 * @param offset - Number of entries to skip
 * @returns Array of episodic memories
 */
export async function getEpisodicMemories(
  agentId: number,
  limit: number = 100,
  offset: number = 0
): Promise<EpisodicMemory[]> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<EpisodicMemory[]>('get_episodic_memories', { agentId, limit, offset });
}

/**
 * Get episodic memories by session ID
 *
 * @param sessionId - The session ID
 * @returns Array of episodic memories
 */
export async function getEpisodicMemoriesBySession(
  sessionId: number
): Promise<EpisodicMemory[]> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<EpisodicMemory[]>('get_episodic_memories_by_session', { sessionId });
}

/**
 * Get episodic memories by minimum importance
 *
 * @param minImportance - Minimum importance score
 * @param limit - Maximum number of entries to return
 * @returns Array of important episodic memories
 */
export async function getEpisodicMemoriesByImportance(
  minImportance: number,
  limit: number = 100
): Promise<EpisodicMemory[]> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<EpisodicMemory[]>('get_episodic_memories_by_importance', { minImportance, limit });
}

/**
 * Delete an episodic memory
 *
 * @param id - The memory ID to delete
 * @returns True if deleted, false if not found
 */
export async function deleteEpisodicMemory(id: number): Promise<boolean> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<boolean>('delete_episodic_memory', { id });
}

/**
 * Get episodic memory statistics
 *
 * @returns Statistics about episodic memories
 */
export async function getEpisodicMemoryStats(): Promise<EpisodicMemoryStats> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<EpisodicMemoryStats>('get_episodic_memory_stats');
}

/**
 * Export episodic memories for an agent to JSON
 *
 * @param agentId - The agent ID
 * @returns JSON string of episodic memories
 */
export async function exportEpisodicMemories(agentId: number): Promise<string> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<string>('export_episodic_memories', { agentId });
}

/**
 * Import episodic memories from JSON
 *
 * @param json - JSON string of episodic memories
 * @param skipDuplicates - If true, skip entries that would create duplicates
 * @returns Number of memories imported
 */
export async function importEpisodicMemories(
  json: string,
  skipDuplicates: boolean = false
): Promise<number> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<number>('import_episodic_memories', { json, skipDuplicates });
}

/**
 * End a session and persist working memory to L2 episodic memory
 *
 * This should be called when a user closes a session to ensure
 * important context is saved to long-term storage.
 *
 * @param agentId - The agent ID
 * @param sessionId - The session ID to end
 * @returns Number of memories persisted to L2
 */
export async function endSession(
  agentId: number,
  sessionId: number
): Promise<number> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<number>('end_session', { agentId, sessionId });
}

// ============================================================================
// Semantic Memory Types (Story 5.3 - L3 Semantic Memory Layer)
// ============================================================================

/**
 * Semantic memory entry with vector embedding
 */
export interface SemanticMemory {
  /** Unique identifier */
  id: number;
  /** Reference to the source episodic memory */
  episodicMemoryId: number;
  /** The embedding vector (for display purposes, usually not used) */
  embedding: number[];
  /** Dimension of the embedding vector */
  embeddingDim: number;
  /** Model used to generate the embedding */
  embeddingModel: string;
  /** Unix timestamp of creation */
  createdAt: number;
  /** Unix timestamp of last update */
  updatedAt: number;
}

/**
 * Result of a semantic similarity search
 */
export interface SemanticSearchResult {
  /** The semantic memory entry */
  memory: SemanticMemory;
  /** Similarity score (0.0 to 1.0) */
  score: number;
  /** The original content from episodic memory */
  content: string | null;
}

/**
 * Statistics about semantic memories
 */
export interface SemanticMemoryStats {
  /** Total number of indexed memories */
  totalCount: number;
  /** Number of embeddings by model */
  byModel: Record<string, number>;
  /** Average embedding dimension */
  avgDimension: number;
}

// ============================================================================
// Semantic Memory API Functions
// ============================================================================

/**
 * Index an episodic memory to the semantic layer
 *
 * This generates an embedding for the episodic memory content
 * and stores it in the semantic memory store for similarity search.
 *
 * @param episodicMemoryId - The episodic memory ID to index
 * @returns The ID of the created semantic memory entry
 */
export async function indexEpisodicMemory(
  episodicMemoryId: number
): Promise<number> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<number>('index_episodic_memory', { episodicMemoryId });
}

/**
 * Search semantic memories by similarity
 *
 * @param query - The search query text
 * @param k - Maximum number of results to return (default: 10)
 * @param agentId - Optional filter by agent ID
 * @param threshold - Minimum similarity threshold (default: 0.7)
 * @returns Array of search results sorted by similarity
 */
export async function searchSemanticMemories(
  query: string,
  k: number = 10,
  agentId?: number,
  threshold?: number
): Promise<SemanticSearchResult[]> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<SemanticSearchResult[]>('search_semantic_memories', {
    query,
    k,
    agentId: agentId ?? null,
    threshold: threshold ?? null,
  });
}

/**
 * Get semantic memory statistics
 *
 * @returns Statistics about semantic memories
 */
export async function getSemanticMemoryStats(): Promise<SemanticMemoryStats> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<SemanticMemoryStats>('get_semantic_memory_stats');
}

/**
 * Delete a semantic memory embedding
 *
 * @param id - The semantic memory ID to delete
 * @returns True if deleted, false if not found
 */
export async function deleteSemanticMemory(id: number): Promise<boolean> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<boolean>('delete_semantic_memory', { id });
}

/**
 * Rebuild semantic index for an agent
 *
 * This regenerates all embeddings from episodic memories for the given agent.
 * Useful when the embedding model changes or to fix corrupted embeddings.
 *
 * @param agentId - The agent ID to rebuild index for
 * @param model - Optional embedding model (defaults to text-embedding-3-small)
 * @returns Number of memories re-indexed
 */
export async function rebuildSemanticIndex(
  agentId: number,
  model?: string
): Promise<number> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<number>('rebuild_semantic_index', {
    agentId,
    model: model ?? null,
  });
}

// ============================================================================
// Unified Memory Manager Types (Story 5.4 - 记忆管理 API 统一封装)
// ============================================================================

/**
 * Memory layer enum for targeting specific storage
 */
export type MemoryLayer = 'L1' | 'L2' | 'L3' | 'ALL';

/**
 * Unified memory entry across all layers
 */
export interface UnifiedMemoryEntry {
  /** Unique identifier */
  id: string;
  /** Memory content */
  content: string;
  /** Role (user, assistant, system) for conversation entries */
  role: string | null;
  /** Importance score (1-10) */
  importance: number;
  /** Whether this memory is marked as important by user */
  isMarked: boolean;
  /** Session ID this memory belongs to */
  sessionId: number | null;
  /** Creation timestamp (Unix) */
  createdAt: number;
  /** Source layer (L1, L2, or L3) */
  sourceLayer: MemoryLayer;
  /** Similarity score (only for L3 semantic search results) */
  similarityScore: number | null;
}

/**
 * Result of a memory query
 */
export interface MemoryQueryResult {
  /** Retrieved memory entries */
  entries: UnifiedMemoryEntry[];
  /** Source layer */
  layer: MemoryLayer;
  /** Total count (before pagination) */
  totalCount: number;
}

/**
 * Statistics for all memory layers
 */
export interface MemoryManagerStats {
  /** L1 capacity */
  l1Capacity: number;
  /** L1 used slots */
  l1Used: number;
  /** L1 session ID (if active) */
  l1SessionId: number | null;
  /** L2 total count */
  l2Total: number;
  /** L2 average importance */
  l2AvgImportance: number;
  /** L3 total indexed memories */
  l3Total: number;
}

// ============================================================================
// Unified Memory Manager API Functions
// ============================================================================

/**
 * Store a memory using the unified MemoryManager
 *
 * - Always stores to L1 (working memory)
 * - Optionally persists to L2 if persistToL2 is true
 * - Optionally indexes to L3 if indexToL3 is true
 *
 * @param content - The memory content
 * @param role - The role (user, assistant, system)
 * @param importance - Importance score (1-10)
 * @param persistToL2 - Whether to persist to L2 (episodic memory)
 * @param indexToL3 - Whether to index to L3 (semantic memory)
 * @returns Memory ID (L2 ID if persisted, otherwise "l1-only")
 */
export async function storeMemory(
  content: string,
  role: string,
  importance: number,
  persistToL2: boolean = false,
  indexToL3: boolean = false
): Promise<string> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<string>('memory_store', {
    content,
    role,
    importance,
    persistToL2,
    indexToL3,
  });
}

/**
 * Retrieve memories using the unified MemoryManager
 *
 * Queries layers based on the specified layer parameter:
 * - "L1": Only working memory
 * - "L2": Only episodic memory
 * - "L3": Only semantic memory
 * - "All" or other: Try L1 first, then L2, then L3
 *
 * @param agentId - The agent ID
 * @param sessionId - Optional session ID filter
 * @param layer - Target layer (L1, L2, L3, or All)
 * @param limit - Maximum number of results
 * @returns Memory query result
 */
export async function retrieveMemory(
  agentId: number,
  sessionId?: number | null,
  layer: MemoryLayer = 'ALL',
  limit: number = 100
): Promise<MemoryQueryResult> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<MemoryQueryResult>('memory_retrieve', {
    agentId,
    sessionId: sessionId ?? null,
    layer,
    limit,
  });
}

/**
 * Search memories using semantic similarity (L3)
 *
 * Returns memories sorted by similarity score (descending).
 *
 * @param query - The search query text
 * @param k - Maximum number of results (default: 10)
 * @param threshold - Minimum similarity threshold (default: 0.7)
 * @returns Array of memory entries with similarity scores
 */
export async function searchMemory(
  query: string,
  k: number = 10,
  threshold: number = 0.7
): Promise<UnifiedMemoryEntry[]> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<UnifiedMemoryEntry[]>('memory_search', { query, k, threshold });
}

/**
 * Delete a memory from the specified layer(s)
 *
 * - "L1": Not supported (L1 doesn't support direct deletion)
 * - "L2": Delete from episodic memory
 * - "L3": Delete from semantic memory only
 * - "All" or other: Delete from L2 and L3
 *
 * @param id - The memory ID
 * @param layer - Target layer
 * @returns True if deletion was successful
 */
export async function deleteMemory(
  id: string,
  layer: MemoryLayer = 'ALL'
): Promise<boolean> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<boolean>('memory_delete', { id, layer });
}

/**
 * Get statistics for all memory layers
 *
 * @returns Statistics for L1, L2, and L3
 */
export async function getMemoryManagerStats(): Promise<MemoryManagerStats> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<MemoryManagerStats>('memory_get_stats');
}

/**
 * Set the current session context for the MemoryManager
 *
 * @param sessionId - The session ID
 * @param agentId - Optional agent ID
 */
export async function setMemorySession(
  sessionId: number,
  agentId?: number
): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke('memory_set_session', {
    sessionId,
    agentId: agentId ?? null,
  });
}

/**
 * Persist session memories from L1 to L2
 *
 * Call this when a session ends to save working memory to long-term storage.
 *
 * @returns Number of memories persisted
 */
export async function persistMemorySession(): Promise<number> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<number>('memory_persist_session');
}

// ============================================================================
// Performance Metrics Types (Story 5.5 - 记忆检索性能优化)
// ============================================================================

/**
 * Performance statistics for memory operations
 */
export interface PerformanceStats {
  /** L1 total queries */
  l1_total_queries: number;
  /** L1 cache hits */
  l1_cache_hits: number;
  /** L1 average latency (ms) */
  l1_avg_latency_ms: number;
  /** L1 max latency (ms) */
  l1_max_latency_ms: number;
  /** L2 total queries */
  l2_total_queries: number;
  /** L2 average latency (ms) */
  l2_avg_latency_ms: number;
  /** L2 max latency (ms) */
  l2_max_latency_ms: number;
  /** L3 total queries */
  l3_total_queries: number;
  /** L3 average latency (ms) */
  l3_avg_latency_ms: number;
  /** L3 max latency (ms) */
  l3_max_latency_ms: number;
  /** Total queries across all layers */
  total_queries: number;
  /** Overall cache hit rate */
  overall_cache_hit_rate: number;
  /** Overall average latency (ms) */
  overall_avg_latency_ms: number;
  /** Window size in seconds */
  window_secs: number;
}

/**
 * Benchmark results for memory operations
 */
export interface BenchmarkResults {
  /** L1 retrieve time in milliseconds */
  l1_retrieve_ms: number;
  /** L2 retrieve time in milliseconds */
  l2_retrieve_ms: number;
  /** L3 search time in milliseconds */
  l3_search_ms: number;
  /** Whether L3 is available */
  l3_available: boolean;
  /** Combined retrieve time in milliseconds */
  combined_retrieve_ms: number;
}

// ============================================================================
// Performance Metrics API Functions
// ============================================================================

/**
 * Get memory performance statistics
 *
 * @returns Performance statistics for all memory layers
 */
export async function getMemoryPerformanceStats(): Promise<PerformanceStats> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<PerformanceStats>('memory_get_performance_stats');
}

/**
 * Run memory performance benchmark
 *
 * @returns Benchmark results for L1, L2, and L3 operations
 */
export async function runMemoryBenchmark(): Promise<BenchmarkResults> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<BenchmarkResults>('memory_benchmark');
}

// ============================================================================
// Memory Mark API Functions (Story 5.8 - 重要片段标记功能)
// ============================================================================

/**
 * Mark an episodic memory as important
 *
 * @param id - The episodic memory ID to mark
 * @returns True if successfully marked
 */
export async function markEpisodicMemoryImportant(id: number): Promise<boolean> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<boolean>('mark_episodic_memory_important', { id });
}

/**
 * Unmark an episodic memory (remove important flag)
 *
 * @param id - The episodic memory ID to unmark
 * @returns True if successfully unmarked
 */
export async function unmarkEpisodicMemoryImportant(id: number): Promise<boolean> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<boolean>('unmark_episodic_memory_important', { id });
}

/**
 * Get marked episodic memories for an agent
 *
 * @param agentId - The agent ID
 * @param limit - Maximum number of memories to return
 * @returns Array of marked episodic memories
 */
export async function getMarkedEpisodicMemories(
  agentId: number,
  limit: number = 100
): Promise<EpisodicMemory[]> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<EpisodicMemory[]>('get_marked_episodic_memories', { agentId, limit });
}

// ============================================================================
// Memory Context Types (Story 5.9 - 上下文增强响应)
// ============================================================================

/**
 * A single memory context entry returned in chat response
 */
export interface MemoryContextEntry {
  /** Memory ID */
  id: string;
  /** Memory content */
  content: string;
  /** Similarity score (0.0 to 1.0) */
  similarityScore: number | null;
  /** Importance score (1-10) */
  importance: number;
  /** Source layer (L1, L2, or L3) */
  sourceLayer: string;
  /** Creation timestamp (Unix) */
  createdAt: number;
}

/**
 * Memory context information returned with chat responses
 */
export interface MemoryContextInfo {
  /** Retrieved memory entries */
  entries: MemoryContextEntry[];
  /** Total character count of all memories */
  totalChars: number;
  /** Time taken for retrieval in milliseconds */
  retrievalTimeMs: number;
}

// ============================================================================
// System Memory Monitor Types (Story 9.7 - 内存使用优化)
// ============================================================================

/**
 * System memory statistics
 */
export interface SystemMemoryStats {
  /** Used memory in bytes */
  usedBytes: number;
  /** Available memory in bytes */
  availableBytes: number;
  /** Memory usage percentage */
  usagePercent: number;
  /** Timestamp of measurement */
  timestamp: number;
}

/**
 * Cache eviction policy
 */
export type CacheEvictionPolicy = 'lru' | 'fifo' | 'lfu';

/**
 * Cache configuration
 */
export interface SystemCacheConfig {
  /** Maximum cache size in bytes */
  maxSize: number;
  /** Eviction policy */
  evictionPolicy: CacheEvictionPolicy;
  /** Check interval in seconds */
  checkIntervalSecs: number;
  /** Warning threshold percentage */
  warningThresholdPercent: number;
}

/**
 * Default cache configuration
 */
export const DEFAULT_SYSTEM_CACHE_CONFIG: SystemCacheConfig = {
  maxSize: 100 * 1024 * 1024, // 100MB
  evictionPolicy: 'lru',
  checkIntervalSecs: 30,
  warningThresholdPercent: 80,
};

/**
 * Cache eviction policy labels
 */
export const CACHE_EVICTION_POLICY_LABELS: Record<CacheEvictionPolicy, string> = {
  lru: '最近最少使用 (LRU)',
  fifo: '先进先出 (FIFO)',
  lfu: '最不常用 (LFU)',
};

/**
 * Format bytes to human readable string
 */
export function formatSystemBytes(bytes: number): string {
  if (bytes === 0) return '0 B';

  const units = ['B', 'KB', 'MB', 'GB'];
  const k = 1024;
  const i = Math.floor(Math.log(bytes) / Math.log(k));

  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${units[i]}`;
}

/**
 * Get memory status color
 */
export function getSystemMemoryStatusColor(percent: number): string {
  if (percent < 50) return 'text-green-600';
  if (percent < 80) return 'text-amber-600';
  return 'text-red-600';
}