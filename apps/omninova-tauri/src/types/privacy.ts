/**
 * Privacy settings types for OmniNova Claw
 *
 * These types correspond to the Rust types in omninova-core/src/privacy/
 */

/**
 * Privacy settings
 */
export interface PrivacySettings {
  /** Whether encryption is enabled */
  encryption_enabled: boolean;
  /** Data retention policy */
  data_retention: DataRetentionPolicy;
  /** Cloud sync settings (reserved for future) */
  cloud_sync: CloudSyncSettings;
  /** Last updated timestamp */
  updated_at: number;
}

/**
 * Data retention policy
 */
export interface DataRetentionPolicy {
  /** Conversation history retention days (0 = keep forever) */
  conversation_retention_days: number;
  /** Whether to automatically clean up expired data */
  auto_cleanup_enabled: boolean;
  /** Maximum storage size in MB (0 = no limit) */
  max_storage_mb: number;
}

/**
 * Cloud sync settings (reserved for future implementation)
 */
export interface CloudSyncSettings {
  /** Whether cloud sync is enabled */
  enabled: boolean;
  /** Sync scope */
  sync_scope: SyncScope;
  /** Last sync timestamp */
  last_sync_at: number | null;
  /** Sync endpoint (if custom) */
  sync_endpoint: string | null;
}

/**
 * Sync scope for cloud synchronization
 */
export type SyncScope = 'all' | 'agents_and_settings' | 'conversations_only' | 'none';

/**
 * Storage information
 */
export interface StorageInfo {
  /** Config directory path */
  config_path: string;
  /** Data directory path */
  data_path: string;
  /** Total storage size in bytes */
  total_size: number;
  /** Storage breakdown by category */
  breakdown: StorageBreakdown;
}

/**
 * Storage breakdown by category
 */
export interface StorageBreakdown {
  /** Database size in bytes */
  database: number;
  /** Config file size in bytes */
  config: number;
  /** Log files size in bytes */
  logs: number;
  /** Cache size in bytes */
  cache: number;
}

/**
 * Clear options for conversation history
 */
export interface ClearOptions {
  /** Clear scope */
  scope: ClearScope;
  /** Agent IDs to clear (when scope is SpecificAgents) */
  agent_ids?: string[];
  /** Date range to clear (when scope is DateRange) */
  date_range?: DateRange;
}

/**
 * Clear scope
 */
export type ClearScope = 'all' | 'specific_agents' | 'date_range';

/**
 * Date range for clearing
 */
export interface DateRange {
  /** Start timestamp (inclusive) */
  start: number;
  /** End timestamp (inclusive) */
  end: number;
}

/**
 * Clear result
 */
export interface ClearResult {
  /** Number of messages deleted */
  messages_deleted: number;
  /** Number of sessions deleted */
  sessions_deleted: number;
  /** Space freed in bytes */
  space_freed: number;
}

/**
 * Default privacy settings
 */
export function defaultPrivacySettings(): PrivacySettings {
  return {
    encryption_enabled: false,
    data_retention: {
      conversation_retention_days: 0,
      auto_cleanup_enabled: false,
      max_storage_mb: 0,
    },
    cloud_sync: {
      enabled: false,
      sync_scope: 'none',
      last_sync_at: null,
      sync_endpoint: null,
    },
    updated_at: Math.floor(Date.now() / 1000),
  };
}

/**
 * Helper to create a date range for the last N days
 */
export function createDateRangeLastDays(days: number): DateRange {
  const now = Math.floor(Date.now() / 1000);
  const start = now - days * 86400;
  return { start, end: now };
}