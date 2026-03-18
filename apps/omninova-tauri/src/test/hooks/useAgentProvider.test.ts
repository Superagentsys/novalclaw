/**
 * useAgentProvider Hook 单元测试
 *
 * 测试覆盖:
 * - 初始化和状态
 * - 提供商查找
 * - 提供商设置
 * - 验证功能
 * - 错误处理
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useAgentProvider } from '@/hooks/useAgentProvider';
import { useProviderStore } from '@/stores/providerStore';
import type { ProviderWithStatus, AgentProviderValidation } from '@/types/provider';
import type { AgentModel } from '@/types/agent';

// Mock useProviderStore
vi.mock('@/stores/providerStore', () => ({
  useProviderStore: vi.fn(),
}));

const mockUseProviderStore = vi.mocked(useProviderStore);

// Mock provider data
const createMockProvider = (overrides?: Partial<ProviderWithStatus>): ProviderWithStatus => ({
  id: 'test-provider-id',
  name: 'Test Provider',
  providerType: 'openai',
  isDefault: false,
  createdAt: Date.now(),
  updatedAt: Date.now(),
  connectionStatus: 'untested',
  keyExists: true,
  ...overrides,
});

// Mock agent data
const createMockAgent = (overrides?: Partial<AgentModel>): AgentModel => ({
  id: 1,
  agent_uuid: 'test-agent-uuid',
  name: 'Test Agent',
  status: 'active',
  created_at: Date.now(),
  updated_at: Date.now(),
  ...overrides,
});

describe('useAgentProvider', () => {
  const mockProviders = [
    createMockProvider({ id: 'provider-1', name: 'Provider 1', isDefault: true }),
    createMockProvider({ id: 'provider-2', name: 'Provider 2' }),
  ];

  const mockInitialize = vi.fn();
  const mockSetAgentProvider = vi.fn();
  const mockValidateForAgent = vi.fn();

  const defaultStoreReturn = {
    providers: mockProviders,
    isLoading: false,
    error: null,
    testingStates: {},
    storeType: null,
    isInitialized: true,
    initialize: mockInitialize,
    setAgentProvider: mockSetAgentProvider,
    validateForAgent: mockValidateForAgent,
    getProviderById: vi.fn((id: string) => mockProviders.find(p => p.id === id)),
    getDefaultProvider: vi.fn(() => mockProviders.find(p => p.isDefault)),
    refresh: vi.fn(),
    addProvider: vi.fn(),
    editProvider: vi.fn(),
    removeProvider: vi.fn(),
    setAsDefault: vi.fn(),
    testConnection: vi.fn(),
    getAgentProvider: vi.fn(),
    updateProviderStatus: vi.fn(),
    clearError: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockUseProviderStore.mockImplementation((selector) => {
      if (typeof selector === 'function') {
        return selector(defaultStoreReturn);
      }
      return defaultStoreReturn;
    });
  });

  describe('初始化', () => {
    it('autoLoad=true 时应该初始化 store', () => {
      renderHook(() => useAgentProvider({ autoLoad: true }));

      expect(mockInitialize).toHaveBeenCalled();
    });

    it('autoLoad=false 时不应该初始化 store', () => {
      renderHook(() => useAgentProvider({ autoLoad: false }));

      expect(mockInitialize).not.toHaveBeenCalled();
    });

    it('应该返回加载状态', () => {
      mockUseProviderStore.mockImplementation((selector) => {
        if (typeof selector === 'function') {
          return selector({ ...defaultStoreReturn, isLoading: true });
        }
        return { ...defaultStoreReturn, isLoading: true };
      });

      const { result } = renderHook(() => useAgentProvider());

      expect(result.current.isLoading).toBe(true);
    });
  });

  describe('提供商查找', () => {
    it('没有代理时应该返回 null', () => {
      const { result } = renderHook(() => useAgentProvider());

      expect(result.current.provider).toBeNull();
    });

    it('代理没有默认提供商时应该返回全局默认', () => {
      const agent = createMockAgent();

      const { result } = renderHook(() => useAgentProvider({ agent }));

      expect(result.current.provider).toEqual(mockProviders[0]);
      expect(result.current.isGlobalDefault).toBe(true);
    });

    it('代理有默认提供商时应该返回代理的提供商', () => {
      const agent = createMockAgent({ default_provider_id: 'provider-2' });

      const { result } = renderHook(() => useAgentProvider({ agent }));

      expect(result.current.provider).toEqual(mockProviders[1]);
      expect(result.current.isGlobalDefault).toBe(false);
    });

    it('代理的提供商不存在时应该返回全局默认', () => {
      const agent = createMockAgent({ default_provider_id: 'non-existent' });

      const { result } = renderHook(() => useAgentProvider({ agent }));

      expect(result.current.provider).toEqual(mockProviders[0]);
    });
  });

  describe('hasProvider', () => {
    it('没有代理时 should be false', () => {
      const { result } = renderHook(() => useAgentProvider());

      expect(result.current.hasProvider).toBe(false);
    });

    it('有提供商时 should be true', () => {
      const agent = createMockAgent();

      const { result } = renderHook(() => useAgentProvider({ agent }));

      expect(result.current.hasProvider).toBe(true);
    });
  });

  describe('isGlobalDefault', () => {
    it('没有代理时 should be false', () => {
      const { result } = renderHook(() => useAgentProvider());

      expect(result.current.isGlobalDefault).toBe(false);
    });

    it('使用全局默认时 should be true', () => {
      const agent = createMockAgent();

      const { result } = renderHook(() => useAgentProvider({ agent }));

      expect(result.current.isGlobalDefault).toBe(true);
    });

    it('使用代理专属提供商时 should be false', () => {
      const agent = createMockAgent({ default_provider_id: 'provider-2' });

      const { result } = renderHook(() => useAgentProvider({ agent }));

      expect(result.current.isGlobalDefault).toBe(false);
    });
  });

  describe('availableProviders', () => {
    it('应该返回所有提供商', () => {
      const { result } = renderHook(() => useAgentProvider());

      expect(result.current.availableProviders).toEqual(mockProviders);
    });
  });

  describe('setProvider', () => {
    it('没有代理时应该返回 false 并设置错误', async () => {
      const { result } = renderHook(() => useAgentProvider());

      let success = false;
      await act(async () => {
        success = await result.current.setProvider('provider-1');
      });

      expect(success).toBe(false);
      expect(result.current.error).toBe('未选择代理');
    });

    it('验证失败时应该返回 false', async () => {
      const agent = createMockAgent();
      mockValidateForAgent.mockResolvedValueOnce({
        isValid: false,
        errors: ['提供商不可用'],
        warnings: [],
        suggestions: [],
      });

      const { result } = renderHook(() => useAgentProvider({ agent }));

      let success = false;
      await act(async () => {
        success = await result.current.setProvider('provider-1');
      });

      expect(success).toBe(false);
      expect(result.current.error).toBe('提供商不可用');
    });

    it('设置提供商失败时应该返回 false', async () => {
      const agent = createMockAgent();
      mockValidateForAgent.mockResolvedValueOnce({
        isValid: true,
        errors: [],
        warnings: [],
        suggestions: [],
      });
      mockSetAgentProvider.mockResolvedValueOnce(false);

      const { result } = renderHook(() => useAgentProvider({ agent }));

      let success = false;
      await act(async () => {
        success = await result.current.setProvider('provider-2');
      });

      expect(success).toBe(false);
      expect(result.current.error).toBe('设置提供商失败');
    });

    it('成功设置提供商时应该返回 true', async () => {
      const agent = createMockAgent();
      mockValidateForAgent.mockResolvedValueOnce({
        isValid: true,
        errors: [],
        warnings: [],
        suggestions: [],
      });
      mockSetAgentProvider.mockResolvedValueOnce(true);

      const { result } = renderHook(() => useAgentProvider({ agent }));

      let success = false;
      await act(async () => {
        success = await result.current.setProvider('provider-2');
      });

      expect(success).toBe(true);
      expect(mockSetAgentProvider).toHaveBeenCalledWith('test-agent-uuid', 'provider-2');
    });

    it('设置提供商时应该验证并存储结果', async () => {
      const agent = createMockAgent();
      const validation: AgentProviderValidation = {
        isValid: true,
        errors: [],
        warnings: ['建议测试连接'],
        suggestions: ['检查 API 密钥'],
      };
      mockValidateForAgent.mockResolvedValueOnce(validation);
      mockSetAgentProvider.mockResolvedValueOnce(true);

      const { result } = renderHook(() => useAgentProvider({ agent }));

      await act(async () => {
        await result.current.setProvider('provider-1');
      });

      expect(result.current.validation).toEqual(validation);
    });
  });

  describe('validateProvider', () => {
    it('应该调用验证并返回结果', async () => {
      const validation: AgentProviderValidation = {
        isValid: true,
        errors: [],
        warnings: [],
        suggestions: [],
      };
      mockValidateForAgent.mockResolvedValueOnce(validation);

      const { result } = renderHook(() => useAgentProvider());

      let res: AgentProviderValidation | null = null;
      await act(async () => {
        res = await result.current.validateProvider('provider-1');
      });

      expect(res).toEqual(validation);
      expect(mockValidateForAgent).toHaveBeenCalledWith('provider-1');
    });

    it('应该存储验证结果', async () => {
      const validation: AgentProviderValidation = {
        isValid: false,
        errors: ['错误'],
        warnings: [],
        suggestions: [],
      };
      mockValidateForAgent.mockResolvedValueOnce(validation);

      const { result } = renderHook(() => useAgentProvider());

      await act(async () => {
        await result.current.validateProvider('provider-1');
      });

      expect(result.current.validation).toEqual(validation);
    });
  });

  describe('refresh', () => {
    it('应该调用 initialize', async () => {
      const { result } = renderHook(() => useAgentProvider());

      await act(async () => {
        await result.current.refresh();
      });

      expect(mockInitialize).toHaveBeenCalled();
    });
  });

  describe('错误处理', () => {
    it('setProvider 异常时应该设置错误消息', async () => {
      const agent = createMockAgent();
      mockValidateForAgent.mockRejectedValueOnce(new Error('验证异常'));

      const { result } = renderHook(() => useAgentProvider({ agent }));

      await act(async () => {
        await result.current.setProvider('provider-1');
      });

      expect(result.current.error).toBe('验证异常');
    });

    it('validateProvider 异常时应该返回错误验证结果', async () => {
      mockValidateForAgent.mockRejectedValueOnce(new Error('验证失败'));

      const { result } = renderHook(() => useAgentProvider());

      let res: AgentProviderValidation | null = null;
      await act(async () => {
        res = await result.current.validateProvider('provider-1');
      });

      expect(res?.isValid).toBe(false);
      expect(res?.errors).toContain('验证失败');
    });
  });

  describe('代理变化', () => {
    it('代理变化时应该更新提供商', () => {
      const agent1 = createMockAgent();
      const agent2 = createMockAgent({
        agent_uuid: 'agent-2',
        default_provider_id: 'provider-2'
      });

      const { result, rerender } = renderHook(
        ({ agent }) => useAgentProvider({ agent }),
        { initialProps: { agent: agent1 } }
      );

      expect(result.current.provider).toEqual(mockProviders[0]);

      rerender({ agent: agent2 });

      expect(result.current.provider).toEqual(mockProviders[1]);
    });

    it('代理变为 null 时应该清除提供商', () => {
      const agent = createMockAgent();

      const { result, rerender } = renderHook(
        ({ agent }) => useAgentProvider({ agent }),
        { initialProps: { agent } }
      );

      expect(result.current.provider).not.toBeNull();

      rerender({ agent: null });

      expect(result.current.provider).toBeNull();
    });
  });
});