/**
 * API Key Types for Gateway Authentication (Story 8.3)
 *
 * Provides type definitions for API key management in the frontend.
 */

export type ApiKeyPermission = 'read' | 'write' | 'admin';

export interface ApiKeyInfo {
  id: number;
  key_prefix: string;
  name: string;
  permissions: ApiKeyPermission[];
  created_at: number;
  expires_at: number | null;
  last_used_at: number | null;
  is_revoked: boolean;
  is_expired: boolean;
}

export interface ApiKeyCreated {
  id: number;
  key: string; // Full key - only shown once!
  key_prefix: string;
  name: string;
  permissions: ApiKeyPermission[];
  created_at: number;
  expires_at: number | null;
}

export interface CreateApiKeyRequest {
  name: string;
  permissions: ApiKeyPermission[];
  expires_in_days?: number;
}

/**
 * Permission display names
 */
export const PERMISSION_LABELS: Record<ApiKeyPermission, string> = {
  read: '只读 (Read)',
  write: '读写 (Write)',
  admin: '管理员 (Admin)',
};

/**
 * Permission descriptions
 */
export const PERMISSION_DESCRIPTIONS: Record<ApiKeyPermission, string> = {
  read: '可以查看代理和会话',
  write: '可以创建/修改代理和发送消息',
  admin: '可以管理 API Keys 和系统配置',
};

/**
 * Format date from Unix timestamp
 */
export function formatApiKeyDate(timestamp: number | null): string {
  if (!timestamp) return '永不';
  const date = new Date(timestamp * 1000);
  return date.toLocaleDateString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

/**
 * Get status badge color
 */
export function getApiKeyStatusColor(key: ApiKeyInfo): string {
  if (key.is_revoked) return 'bg-gray-500';
  if (key.is_expired) return 'bg-red-500';
  return 'bg-green-500';
}

/**
 * Get status label
 */
export function getApiKeyStatusLabel(key: ApiKeyInfo): string {
  if (key.is_revoked) return '已撤销';
  if (key.is_expired) return '已过期';
  return '有效';
}