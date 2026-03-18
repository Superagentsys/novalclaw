/**
 * useConversationProvider Hook 单元测试
 *
 * 测试覆盖:
 * - 初始化和状态
 * - 提供商选择
 * - 临时切换
 * - 错误处理
 * - 代理同步
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useConversationProvider, type ProviderError } from '@/hooks/useConversationProvider';
import { useProviderStore } from '@/stores/providerStore';
import type { ProviderWithStatus } from '@/types/provider';
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

describe('useConversationProvider', () => {
  const mockProviders = [
    createMockProvider({ id: 'provider-1', name: 'Provider 1', isDefault: true }),
    createMockProvider({ id: 'provider-2', name: 'Provider 2' }),
    createMockProvider({ id: 'provider-3', name: 'Provider 3' }),
  ];

  const mockInitialize = vi.fn();
  const mockTestConnection = vi.fn();

  const defaultStoreReturn = {
    providers: mockProviders,
    isLoading: false,
    error: null,
    testingStates: {},
    storeType: null,
    isInitialized: true,
    initialize: mockInitialize,
    testConnection: mockTestConnection,
    refresh: vi.fn(),
    addProvider: vi.fn(),
    editProvider: vi.fn(),
    removeProvider: vi.fn(),
    setAsDefault: vi.fn(),
    setAgentProvider: vi.fn(),
    getAgentProvider: vi.fn(),
    validateForAgent: vi.fn(),
    getProviderById: vi.fn((id: string) => mockProviders.find(p => p.id === id)),
    getDefaultProvider: vi.fn(() => mockProviders.find(p => p.isDefault)),
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
    it('应该初始化 store', () => {
      renderHook(() => useConversationProvider());

      expect(mockInitialize).toHaveBeenCalled();
    });

    it('应该返回加载状态', () => {
      mockUseProviderStore.mockImplementation((selector) => {
        if (typeof selector === 'function') {
          return selector({ ...defaultStoreReturn, isLoading: true });
        }
        return { ...defaultStoreReturn, isLoading: true };
      });

      const { result } = renderHook(() => useConversationProvider());

      expect(result.current.isLoading).toBe(true);
    });
  });

  describe('默认提供商', () => {
    it('没有代理时应该返回全局默认提供商', () => {
      const { result } = renderHook(() => useConversationProvider());

      expect(result.current.defaultProvider).toEqual(mockProviders[0]);
      expect(result.current.defaultProvider?.isDefault).toBe(true);
    });

    it('代理没有默认提供商时应该返回全局默认', () => {
      const agent = createMockAgent();

      const { result } = renderHook(() => useConversationProvider(agent));

      expect(result.current.defaultProvider).toEqual(mockProviders[0]);
    });

    it('代理有默认提供商时应该返回代理的提供商', () => {
      const agent = createMockAgent({ default_provider_id: 'provider-2' });

      const { result } = renderHook(() => useConversationProvider(agent));

      expect(result.current.defaultProvider).toEqual(mockProviders[1]);
      expect(result.current.defaultProvider?.name).toBe('Provider 2');
    });

    it('代理的提供商不存在时应该回退到全局默认', () => {
      const agent = createMockAgent({ default_provider_id: 'non-existent' });

      const { result } = renderHook(() => useConversationProvider(agent));

      expect(result.current.defaultProvider).toEqual(mockProviders[0]);
    });
  });

  describe('当前提供商', () => {
    it('当前提供商应该等于默认提供商', () => {
      const { result } = renderHook(() => useConversationProvider());

      expect(result.current.currentProvider).toEqual(result.current.defaultProvider);
    });

    it('临时切换后当前提供商应该改变', () => {
      const { result } = renderHook(() => useConversationProvider());

      act(() => {
        result.current.switchProvider('provider-2');
      });

      expect(result.current.currentProvider).toEqual(mockProviders[1]);
      expect(result.current.isTemporarySwitch).toBe(true);
    });
  });

  describe('临时切换', () => {
    it('switchProvider 应该切换提供商', () => {
      const { result } = renderHook(() => useConversationProvider());

      act(() => {
        result.current.switchProvider('provider-3');
      });

      expect(result.current.currentProvider).toEqual(mockProviders[2]);
      expect(result.current.isTemporarySwitch).toBe(true);
    });

    it('switchProvider 使用无效 ID 不应该改变状态', () => {
      const { result } = renderHook(() => useConversationProvider());
      const originalProvider = result.current.currentProvider;

      act(() => {
        result.current.switchProvider('non-existent');
      });

      expect(result.current.currentProvider).toEqual(originalProvider);
      expect(result.current.isTemporarySwitch).toBe(false);
    });

    it('resetToDefault 应该重置到默认提供商', () => {
      const { result } = renderHook(() => useConversationProvider());

      // 先切换
      act(() => {
        result.current.switchProvider('provider-2');
      });
      expect(result.current.isTemporarySwitch).toBe(true);

      // 再重置
      act(() => {
        result.current.resetToDefault();
      });

      expect(result.current.currentProvider).toEqual(mockProviders[0]);
      expect(result.current.isTemporarySwitch).toBe(false);
    });

    it('重置后错误应该被清除', () => {
      const { result } = renderHook(() => useConversationProvider());

      // 设置一个错误
      act(() => {
        result.current.handleProviderError(new Error('Test error'));
      });
      expect(result.current.lastError).not.toBeNull();

      // 重置
      act(() => {
        result.current.resetToDefault();
      });

      expect(result.current.lastError).toBeNull();
    });
  });

  describe('可用提供商', () => {
    it('应该返回所有提供商', () => {
      const { result } = renderHook(() => useConversationProvider());

      expect(result.current.availableProviders).toEqual(mockProviders);
    });
  });

  describe('错误处理', () => {
    it('handleProviderError 应该解析 API 密钥错误', () => {
      const { result } = renderHook(() => useConversationProvider());

      let error: ProviderError | null = null;
      act(() => {
        error = result.current.handleProviderError(new Error('API key is invalid'));
      });

      expect(error?.type).toBe('api_key_missing');
      expect(error?.message).toContain('API 密钥');
    });

    it('handleProviderError 应该解析连接失败错误', () => {
      const { result } = renderHook(() => useConversationProvider());

      let error: ProviderError | null = null;
      act(() => {
        error = result.current.handleProviderError(new Error('Connection timeout'));
      });

      expect(error?.type).toBe('connection_failed');
      expect(error?.message).toContain('网络');
    });

    it('handleProviderError 应该解析频率限制错误', () => {
      const { result } = renderHook(() => useConversationProvider());

      let error: ProviderError | null = null;
      act(() => {
        error = result.current.handleProviderError(new Error('Rate limit exceeded (429)'));
      });

      expect(error?.type).toBe('rate_limited');
      expect(error?.message).toContain('频率');
    });

    it('handleProviderError 应该解析服务不可用错误', () => {
      const { result } = renderHook(() => useConversationProvider());

      let error: ProviderError | null = null;
      act(() => {
        error = result.current.handleProviderError(new Error('Service unavailable (503)'));
      });

      expect(error?.type).toBe('service_unavailable');
      expect(error?.message).toContain('服务');
    });

    it('handleProviderError 应该解析提供商未找到错误', () => {
      const { result } = renderHook(() => useConversationProvider());

      let error: ProviderError | null = null;
      act(() => {
        error = result.current.handleProviderError(new Error('Provider not found'));
      });

      expect(error?.type).toBe('provider_not_found');
    });

    it('handleProviderError 应该处理未知错误', () => {
      const { result } = renderHook(() => useConversationProvider());

      let error: ProviderError | null = null;
      act(() => {
        error = result.current.handleProviderError(new Error('Some random error'));
      });

      expect(error?.type).toBe('unknown');
    });

    it('handleProviderError 应该添加当前提供商信息', () => {
      const { result } = renderHook(() => useConversationProvider());

      let error: ProviderError | null = null;
      act(() => {
        error = result.current.handleProviderError(new Error('Test error'));
      });

      expect(error?.providerId).toBe('provider-1');
      expect(error?.providerName).toBe('Provider 1');
    });

    it('clearError 应该清除错误', () => {
      const { result } = renderHook(() => useConversationProvider());

      // 设置错误
      act(() => {
        result.current.handleProviderError(new Error('Test error'));
      });
      expect(result.current.lastError).not.toBeNull();

      // 清除错误
      act(() => {
        result.current.clearError();
      });

      expect(result.current.lastError).toBeNull();
    });
  });

  describe('代理管理', () => {
    it('setAgent 应该更新当前代理', () => {
      const { result } = renderHook(() => useConversationProvider());

      const newAgent = createMockAgent({
        agent_uuid: 'new-agent',
        default_provider_id: 'provider-2'
      });

      act(() => {
        result.current.setAgent(newAgent);
      });

      expect(result.current.currentAgent).toEqual(newAgent);
      expect(result.current.currentProvider).toEqual(mockProviders[1]);
    });

    it('setAgent 应该重置临时提供商', () => {
      const { result } = renderHook(() => useConversationProvider());

      // 先切换
      act(() => {
        result.current.switchProvider('provider-3');
      });
      expect(result.current.isTemporarySwitch).toBe(true);

      // 设置新代理
      act(() => {
        result.current.setAgent(createMockAgent());
      });

      expect(result.current.isTemporarySwitch).toBe(false);
    });

    it('setAgent(null) 应该清除代理', () => {
      const agent = createMockAgent({ default_provider_id: 'provider-2' });

      const { result } = renderHook(() => useConversationProvider(agent));

      expect(result.current.currentAgent).toEqual(agent);

      act(() => {
        result.current.setAgent(null);
      });

      expect(result.current.currentAgent).toBeNull();
    });
  });

  describe('连接测试', () => {
    it('testConnection 应该调用 store 的 testConnection', async () => {
      mockTestConnection.mockResolvedValueOnce({ healthy: true });

      const { result } = renderHook(() => useConversationProvider());

      await act(async () => {
        const res = await result.current.testConnection('provider-1');
        expect(res).toEqual({ healthy: true });
      });

      expect(mockTestConnection).toHaveBeenCalledWith('provider-1');
    });

    it('testConnection 失败时应该返回 null', async () => {
      mockTestConnection.mockResolvedValueOnce(null);

      const { result } = renderHook(() => useConversationProvider());

      await act(async () => {
        const res = await result.current.testConnection('provider-1');
        expect(res).toBeNull();
      });
    });

    it('应该返回测试状态', () => {
      mockUseProviderStore.mockImplementation((selector) => {
        if (typeof selector === 'function') {
          return selector({
            ...defaultStoreReturn,
            testingStates: { 'provider-1': true }
          });
        }
        return { ...defaultStoreReturn, testingStates: { 'provider-1': true } };
      });

      const { result } = renderHook(() => useConversationProvider());

      expect(result.current.testingStates).toEqual({ 'provider-1': true });
    });
  });

  describe('initialAgent 参数', () => {
    it('应该使用 initialAgent 作为初始代理', () => {
      const agent = createMockAgent({ default_provider_id: 'provider-2' });

      const { result } = renderHook(() => useConversationProvider(agent));

      expect(result.current.currentAgent).toEqual(agent);
      expect(result.current.currentProvider).toEqual(mockProviders[1]);
    });

    it('initialAgent 变化时应该更新代理', () => {
      const agent1 = createMockAgent({ agent_uuid: 'agent-1' });
      const agent2 = createMockAgent({ agent_uuid: 'agent-2', default_provider_id: 'provider-2' });

      const { result, rerender } = renderHook(
        ({ agent }) => useConversationProvider(agent),
        { initialProps: { agent: agent1 } }
      );

      expect(result.current.currentAgent).toEqual(agent1);

      rerender({ agent: agent2 });

      expect(result.current.currentAgent).toEqual(agent2);
    });
  });
});