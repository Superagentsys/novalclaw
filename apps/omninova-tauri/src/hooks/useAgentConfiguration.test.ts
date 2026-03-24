/**
 * useAgentConfiguration Hook Tests
 *
 * Tests for the agent configuration state management hook.
 * [Source: Story 7.7 - ConfigurationPanel 组件]
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useAgentConfiguration } from './useAgentConfiguration';
import { invoke } from '@tauri-apps/api/core';

// Mock Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

describe('useAgentConfiguration', () => {
  const mockInvoke = vi.mocked(invoke);
  const defaultAgentId = 'test-agent-id';

  const mockConfigResponse = {
    style_config: JSON.stringify({
      responseStyle: 'detailed',
      verbosity: 0.5,
      maxResponseLength: 0,
    }),
    context_window_config: JSON.stringify({
      maxTokens: 4096,
      responseReserve: 1024,
    }),
    trigger_keywords_config: JSON.stringify({
      keywords: ['hello', 'help'],
      defaultMatchType: 'exact',
    }),
    privacy_config: JSON.stringify({
      dataRetention: {
        episodicMemoryDays: 90,
        workingMemoryHours: 24,
      },
      sensitiveFilter: {
        enabled: true,
      },
    }),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockResolvedValue(mockConfigResponse);
  });

  describe('initialization', () => {
    it('should initialize with default config when no initialConfig provided', async () => {
      const { result } = renderHook(() =>
        useAgentConfiguration({ agentId: defaultAgentId })
      );

      // Wait for the initial reload to complete
      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      expect(result.current.config).toBeDefined();
      expect(result.current.config.agentId).toBe(defaultAgentId);
      expect(result.current.isDirty).toBe(false);
    });

    it('should use provided initialConfig', () => {
      const initialConfig = {
        agentId: defaultAgentId,
        styleConfig: { responseStyle: 'concise' as const, verbosity: 0.8, maxResponseLength: 1000, friendlyTone: true },
        contextConfig: { maxTokens: 2048, responseReserve: 512, overflowStrategy: 'truncate' as const, includeSystemPrompt: true },
        triggerConfig: { keywords: ['test'], defaultMatchType: 'exact' as const, enabled: true, defaultCaseSensitive: false },
        privacyConfig: {
          dataRetention: { episodicMemoryDays: 7, workingMemoryHours: 12, autoCleanup: true },
          sensitiveFilter: { enabled: false, patterns: [], exclusions: [] },
          memorySharingScope: 'singleSession' as const,
        },
        skillConfig: { agentId: defaultAgentId, enabledSkills: [], skillConfigs: {} },
      };

      const { result } = renderHook(() =>
        useAgentConfiguration({ agentId: defaultAgentId, initialConfig })
      );

      expect(result.current.config).toEqual(initialConfig);
      expect(result.current.isLoading).toBe(false);
    });
  });

  describe('config updates', () => {
    it('should update style config', () => {
      const { result } = renderHook(() =>
        useAgentConfiguration({ agentId: defaultAgentId })
      );

      act(() => {
        result.current.setStyleConfig({
          responseStyle: 'concise',
          verbosity: 0.8,
          maxResponseLength: 1000,
          friendlyTone: true,
        });
      });

      expect(result.current.config.styleConfig.responseStyle).toBe('concise');
      expect(result.current.isDirty).toBe(true);
    });

    it('should update context config', () => {
      const { result } = renderHook(() =>
        useAgentConfiguration({ agentId: defaultAgentId })
      );

      act(() => {
        result.current.setContextConfig({
          maxTokens: 8192,
          responseReserve: 2048,
          overflowStrategy: 'truncate',
          includeSystemPrompt: true,
        });
      });

      expect(result.current.config.contextConfig.maxTokens).toBe(8192);
      expect(result.current.isDirty).toBe(true);
    });

    it('should update trigger config', () => {
      const { result } = renderHook(() =>
        useAgentConfiguration({ agentId: defaultAgentId })
      );

      act(() => {
        result.current.setTriggerConfig({
          keywords: ['new', 'keywords'],
          defaultMatchType: 'prefix',
          enabled: true,
          defaultCaseSensitive: false,
        });
      });

      expect(result.current.config.triggerConfig.keywords).toEqual(['new', 'keywords']);
      expect(result.current.isDirty).toBe(true);
    });

    it('should update privacy config', () => {
      const { result } = renderHook(() =>
        useAgentConfiguration({ agentId: defaultAgentId })
      );

      act(() => {
        result.current.setPrivacyConfig({
          dataRetention: {
            episodicMemoryDays: 7,
            workingMemoryHours: 12,
            autoCleanup: true,
          },
          sensitiveFilter: {
            enabled: true,
            patterns: [],
            exclusions: [],
          },
          memorySharingScope: 'singleSession',
        });
      });

      expect(result.current.config.privacyConfig.dataRetention.episodicMemoryDays).toBe(7);
      expect(result.current.isDirty).toBe(true);
    });

    it('should track multiple changes', () => {
      const { result } = renderHook(() =>
        useAgentConfiguration({ agentId: defaultAgentId })
      );

      act(() => {
        result.current.setStyleConfig({
          responseStyle: 'concise',
          verbosity: 0.8,
          maxResponseLength: 1000,
          friendlyTone: true,
        });
        result.current.setContextConfig({
          maxTokens: 8192,
          responseReserve: 2048,
          overflowStrategy: 'truncate',
          includeSystemPrompt: true,
        });
      });

      expect(result.current.changes.length).toBeGreaterThanOrEqual(2);
    });
  });

  describe('save', () => {
    it('should save configuration successfully', async () => {
      mockInvoke.mockResolvedValueOnce(undefined);

      const { result } = renderHook(() =>
        useAgentConfiguration({ agentId: defaultAgentId })
      );

      // Wait for initial load
      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      // Make a change
      act(() => {
        result.current.setStyleConfig({
          responseStyle: 'concise',
          verbosity: 0.8,
          maxResponseLength: 1000,
          friendlyTone: true,
        });
      });

      expect(result.current.isDirty).toBe(true);

      // Save
      let saveResult: boolean | undefined;
      await act(async () => {
        saveResult = await result.current.save();
      });

      expect(saveResult).toBe(true);
      expect(mockInvoke).toHaveBeenCalledWith('update_agent_configuration', expect.any(Object));

      // Wait for state update
      await waitFor(() => {
        expect(result.current.isDirty).toBe(false);
      });
    });

    it('should handle save error', async () => {
      mockInvoke.mockRejectedValue(new Error('Save failed'));

      const { result } = renderHook(() =>
        useAgentConfiguration({ agentId: defaultAgentId })
      );

      // Make a change
      act(() => {
        result.current.setStyleConfig({
          responseStyle: 'concise',
          verbosity: 0.8,
          maxResponseLength: 1000,
          friendlyTone: true,
        });
      });

      // Save
      let saveResult: boolean | undefined;
      await act(async () => {
        saveResult = await result.current.save();
      });

      expect(saveResult).toBe(false);
      expect(result.current.error).toBe('Save failed');
    });
  });

  describe('cancel', () => {
    it('should revert changes on cancel', () => {
      const { result } = renderHook(() =>
        useAgentConfiguration({ agentId: defaultAgentId })
      );

      const originalConfig = { ...result.current.config };

      // Make a change
      act(() => {
        result.current.setStyleConfig({
          responseStyle: 'concise',
          verbosity: 0.8,
          maxResponseLength: 1000,
          friendlyTone: true,
        });
      });

      expect(result.current.isDirty).toBe(true);

      // Cancel
      act(() => {
        result.current.cancel();
      });

      expect(result.current.config).toEqual(originalConfig);
      expect(result.current.isDirty).toBe(false);
    });
  });

  describe('resetToDefaults', () => {
    it('should reset all config to defaults', () => {
      const { result } = renderHook(() =>
        useAgentConfiguration({ agentId: defaultAgentId })
      );

      // Make changes
      act(() => {
        result.current.setStyleConfig({
          responseStyle: 'concise',
          verbosity: 0.8,
          maxResponseLength: 1000,
          friendlyTone: true,
        });
      });

      expect(result.current.isDirty).toBe(true);

      // Reset
      act(() => {
        result.current.resetToDefaults();
      });

      // Should have default values
      expect(result.current.config.styleConfig.responseStyle).toBe('detailed');
    });
  });

  describe('validation', () => {
    it('should validate configuration correctly', () => {
      const { result } = renderHook(() =>
        useAgentConfiguration({ agentId: defaultAgentId })
      );

      // Set invalid verbosity
      act(() => {
        result.current.setStyleConfig({
          responseStyle: 'detailed',
          verbosity: -1, // Invalid
          maxResponseLength: 1000,
          friendlyTone: true,
        });
      });

      expect(result.current.isValid).toBe(false);
      expect(result.current.validationResult.errors.length).toBeGreaterThan(0);
    });

    it('should validate maxTokens', () => {
      const { result } = renderHook(() =>
        useAgentConfiguration({ agentId: defaultAgentId })
      );

      // Set invalid maxTokens
      act(() => {
        result.current.setContextConfig({
          maxTokens: -100, // Invalid
          responseReserve: 512,
          overflowStrategy: 'truncate',
          includeSystemPrompt: true,
        });
      });

      expect(result.current.isValid).toBe(false);
    });
  });

  describe('reload', () => {
    it('should reload configuration from server', async () => {
      const { result } = renderHook(() =>
        useAgentConfiguration({ agentId: defaultAgentId })
      );

      await act(async () => {
        await result.current.reload();
      });

      expect(mockInvoke).toHaveBeenCalledWith('get_agent_configuration', {
        agentId: defaultAgentId,
      });
    });
  });
});