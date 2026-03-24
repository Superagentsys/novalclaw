/**
 * useConfigImportExport Hook Tests
 *
 * Tests for the configuration import/export hook.
 * [Source: Story 7.8 - 配置导入导出功能]
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useConfigImportExport } from './useConfigImportExport';
import type { AgentModel } from '@/types/agent';

// Mock Tauri APIs
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: vi.fn(),
  open: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  readFile: vi.fn(),
  writeFile: vi.fn(),
}));

const mockAgent: AgentModel = {
  id: 1,
  agent_uuid: 'test-uuid-123',
  name: 'Test Agent',
  description: 'Test description',
  domain: 'Testing',
  mbti_type: 'INTJ',
  status: 'active',
  created_at: Date.now(),
  updated_at: Date.now(),
  style_config: JSON.stringify({
    responseStyle: 'detailed',
    verbosity: 0.5,
    maxResponseLength: 0,
    friendlyTone: true,
  }),
  context_window_config: JSON.stringify({
    maxTokens: 4096,
    overflowStrategy: 'truncate',
    includeSystemPrompt: true,
    responseReserve: 1024,
  }),
  trigger_keywords_config: JSON.stringify({
    keywords: [],
    enabled: true,
    defaultMatchType: 'exact',
    defaultCaseSensitive: false,
  }),
  privacy_config: JSON.stringify({
    dataRetention: {
      episodicMemoryDays: 90,
      workingMemoryHours: 24,
      autoCleanup: true,
    },
    sensitiveFilter: {
      enabled: false,
      filterEmail: true,
      filterPhone: true,
      filterIdCard: true,
      filterBankCard: true,
      filterIpAddress: false,
      customPatterns: [],
    },
    memorySharingScope: 'singleSession',
    exclusionRules: [],
    verboseLogging: false,
  }),
};

describe('useConfigImportExport', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Initial State', () => {
    it('should have correct initial export state', () => {
      const { result } = renderHook(() => useConfigImportExport());

      expect(result.current.exportState.isExporting).toBe(false);
      expect(result.current.exportState.error).toBeNull();
    });

    it('should have correct initial import state', () => {
      const { result } = renderHook(() => useConfigImportExport());

      expect(result.current.importState.isValidating).toBe(false);
      expect(result.current.importState.isImporting).toBe(false);
      expect(result.current.importState.error).toBeNull();
      expect(result.current.importState.validationResult).toBeNull();
    });
  });

  describe('Reset Functions', () => {
    it('should reset export state', () => {
      const { result } = renderHook(() => useConfigImportExport());

      act(() => {
        result.current.resetExportState();
      });

      expect(result.current.exportState.isExporting).toBe(false);
      expect(result.current.exportState.error).toBeNull();
    });

    it('should reset import state', () => {
      const { result } = renderHook(() => useConfigImportExport());

      act(() => {
        result.current.resetImportState();
      });

      expect(result.current.importState.isValidating).toBe(false);
      expect(result.current.importState.isImporting).toBe(false);
      expect(result.current.importState.error).toBeNull();
      expect(result.current.importState.validationResult).toBeNull();
    });
  });
});