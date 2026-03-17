/**
 * Privacy types 单元测试
 *
 * 测试覆盖:
 * - defaultPrivacySettings 函数
 * - createDateRangeLastDays 函数
 * - 类型定义验证
 */

import { describe, it, expect } from 'vitest';
import {
  defaultPrivacySettings,
  createDateRangeLastDays,
  type PrivacySettings,
  type DataRetentionPolicy,
  type CloudSyncSettings,
  type StorageInfo,
  type StorageBreakdown,
  type ClearOptions,
  type ClearResult,
  type DateRange,
} from '@/types/privacy';

describe('defaultPrivacySettings', () => {
  it('应该返回默认的隐私设置', () => {
    const settings = defaultPrivacySettings();

    expect(settings.encryption_enabled).toBe(false);
    expect(settings.data_retention.conversation_retention_days).toBe(0);
    expect(settings.data_retention.auto_cleanup_enabled).toBe(false);
    expect(settings.data_retention.max_storage_mb).toBe(0);
    expect(settings.cloud_sync.enabled).toBe(false);
    expect(settings.cloud_sync.sync_scope).toBe('none');
    expect(settings.cloud_sync.last_sync_at).toBeNull();
    expect(settings.cloud_sync.sync_endpoint).toBeNull();
  });

  it('应该返回有效的 updated_at 时间戳', () => {
    const settings = defaultPrivacySettings();
    const now = Math.floor(Date.now() / 1000);

    // 允许 1 秒的差异
    expect(settings.updated_at).toBeGreaterThanOrEqual(now - 1);
    expect(settings.updated_at).toBeLessThanOrEqual(now + 1);
  });
});

describe('createDateRangeLastDays', () => {
  it('应该创建正确的时间范围', () => {
    const days = 7;
    const range = createDateRangeLastDays(days);
    const now = Math.floor(Date.now() / 1000);

    // 开始时间应该是大约 7 天前
    const expectedStart = now - days * 86400;
    expect(range.start).toBeGreaterThanOrEqual(expectedStart - 1);
    expect(range.start).toBeLessThanOrEqual(expectedStart + 1);

    // 结束时间应该是现在
    expect(range.end).toBeGreaterThanOrEqual(now - 1);
    expect(range.end).toBeLessThanOrEqual(now + 1);
  });

  it('应该返回正确的天数差', () => {
    const days = 30;
    const range = createDateRangeLastDays(days);
    const duration = range.end - range.start;
    const expectedDuration = days * 86400;

    // 允许 1 秒的差异
    expect(duration).toBeGreaterThanOrEqual(expectedDuration - 1);
    expect(duration).toBeLessThanOrEqual(expectedDuration + 1);
  });

  it('应该支持不同的天数', () => {
    const testCases = [1, 7, 14, 30, 90, 365];

    testCases.forEach((days) => {
      const range = createDateRangeLastDays(days);
      const duration = range.end - range.start;
      const expectedDuration = days * 86400;

      expect(duration).toBeGreaterThanOrEqual(expectedDuration - 2);
      expect(duration).toBeLessThanOrEqual(expectedDuration + 2);
    });
  });
});

describe('类型定义', () => {
  it('PrivacySettings 类型应该匹配 Rust 后端定义', () => {
    const settings: PrivacySettings = {
      encryption_enabled: true,
      data_retention: {
        conversation_retention_days: 30,
        auto_cleanup_enabled: true,
        max_storage_mb: 1000,
      },
      cloud_sync: {
        enabled: false,
        sync_scope: 'none',
        last_sync_at: null,
        sync_endpoint: null,
      },
      updated_at: Math.floor(Date.now() / 1000),
    };

    expect(settings.encryption_enabled).toBe(true);
    expect(settings.data_retention.conversation_retention_days).toBe(30);
  });

  it('DataRetentionPolicy 类型应该有正确的字段', () => {
    const policy: DataRetentionPolicy = {
      conversation_retention_days: 30,
      auto_cleanup_enabled: true,
      max_storage_mb: 500,
    };

    expect(policy.conversation_retention_days).toBe(30);
    expect(policy.auto_cleanup_enabled).toBe(true);
    expect(policy.max_storage_mb).toBe(500);
  });

  it('CloudSyncSettings 类型应该有正确的字段', () => {
    const settings: CloudSyncSettings = {
      enabled: true,
      sync_scope: 'agents_and_settings',
      last_sync_at: 1710000000,
      sync_endpoint: 'https://sync.example.com',
    };

    expect(settings.enabled).toBe(true);
    expect(settings.sync_scope).toBe('agents_and_settings');
    expect(settings.last_sync_at).toBe(1710000000);
    expect(settings.sync_endpoint).toBe('https://sync.example.com');
  });

  it('StorageInfo 类型应该有正确的字段', () => {
    const info: StorageInfo = {
      config_path: '/path/to/config',
      data_path: '/path/to/data',
      total_size: 1048576,
      breakdown: {
        database: 512000,
        config: 1024,
        logs: 256000,
        cache: 280000,
      },
    };

    expect(info.config_path).toBe('/path/to/config');
    expect(info.total_size).toBe(1048576);
    expect(info.breakdown.database).toBe(512000);
  });

  it('StorageBreakdown 类型应该有正确的字段', () => {
    const breakdown: StorageBreakdown = {
      database: 1000,
      config: 200,
      logs: 300,
      cache: 400,
    };

    expect(breakdown.database).toBe(1000);
    expect(breakdown.config).toBe(200);
    expect(breakdown.logs).toBe(300);
    expect(breakdown.cache).toBe(400);
  });

  it('ClearOptions 类型应该有正确的字段', () => {
    const options: ClearOptions = {
      scope: 'specific_agents',
      agent_ids: ['agent-1', 'agent-2'],
    };

    expect(options.scope).toBe('specific_agents');
    expect(options.agent_ids).toEqual(['agent-1', 'agent-2']);
  });

  it('ClearResult 类型应该有正确的字段', () => {
    const result: ClearResult = {
      messages_deleted: 100,
      sessions_deleted: 5,
      space_freed: 1048576,
    };

    expect(result.messages_deleted).toBe(100);
    expect(result.sessions_deleted).toBe(5);
    expect(result.space_freed).toBe(1048576);
  });

  it('DateRange 类型应该有正确的字段', () => {
    const range: DateRange = {
      start: 1710000000,
      end: 1710086400,
    };

    expect(range.start).toBe(1710000000);
    expect(range.end).toBe(1710086400);
  });
});

describe('SyncScope 类型', () => {
  it('应该支持所有范围值', () => {
    const scopes: Array<'all' | 'agents_and_settings' | 'conversations_only' | 'none'> = [
      'all',
      'agents_and_settings',
      'conversations_only',
      'none',
    ];

    scopes.forEach((scope) => {
      const settings: CloudSyncSettings = {
        enabled: true,
        sync_scope: scope,
        last_sync_at: null,
        sync_endpoint: null,
      };

      expect(settings.sync_scope).toBe(scope);
    });
  });
});

describe('ClearScope 类型', () => {
  it('应该支持所有范围值', () => {
    const scopes: Array<'all' | 'specific_agents' | 'date_range'> = [
      'all',
      'specific_agents',
      'date_range',
    ];

    scopes.forEach((scope) => {
      const options: ClearOptions = {
        scope,
      };

      expect(options.scope).toBe(scope);
    });
  });
});