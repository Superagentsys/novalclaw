/**
 * Provider Store - Global State Management with Zustand
 *
 * Centralizes provider state for the entire application, enabling:
 * - Shared provider state across components
 * - Connection testing with status tracking
 * - Default provider management
 * - Agent-provider assignments
 *
 * [Source: Story 3.7 - Provider 切换与代理默认提供商]
 */

import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';
import { toast } from 'sonner';
import {
  type ProviderConfig,
  type NewProviderConfig,
  type ProviderConfigUpdate,
  type ProviderWithStatus,
  type ProviderTestResult,
  type ConnectionStatus,
  listProviders,
  createProvider,
  updateProvider,
  deleteProvider,
  setDefaultProvider,
  testProviderConnection,
  setAgentDefaultProvider,
  getAgentProvider,
  validateProviderForAgent,
} from '@/types/provider';
import {
  apiKeyExists,
  getKeyringStoreType,
  type KeyringStoreType,
} from '@/types/keyring';

// ============================================================================
// Types
// ============================================================================

export interface ProviderState {
  /** List of providers with status */
  providers: ProviderWithStatus[];
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: string | null;
  /** Connection testing states by provider ID */
  testingStates: Record<string, boolean>;
  /** Keyring store type */
  storeType: KeyringStoreType | null;
  /** Whether store has been initialized */
  isInitialized: boolean;
}

export interface ProviderActions {
  /** Refresh provider list from backend */
  refresh: () => Promise<void>;
  /** Initialize store (load providers) */
  initialize: () => Promise<void>;
  /** Add a new provider */
  addProvider: (config: NewProviderConfig) => Promise<ProviderConfig | null>;
  /** Update a provider */
  editProvider: (id: string, update: ProviderConfigUpdate) => Promise<ProviderConfig | null>;
  /** Remove a provider */
  removeProvider: (id: string) => Promise<boolean>;
  /** Set provider as default */
  setAsDefault: (id: string) => Promise<boolean>;
  /** Test provider connection */
  testConnection: (id: string) => Promise<ProviderTestResult | null>;
  /** Set agent's default provider */
  setAgentProvider: (agentUuid: string, providerId: string) => Promise<boolean>;
  /** Get agent's provider */
  getAgentProvider: (agentUuid: string) => Promise<ProviderConfig | null>;
  /** Validate provider for agent */
  validateForAgent: (providerId: string) => Promise<{
    isValid: boolean;
    errors: string[];
    warnings: string[];
    suggestions: string[];
  }>;
  /** Get provider by ID */
  getProviderById: (id: string) => ProviderWithStatus | undefined;
  /** Get default provider */
  getDefaultProvider: () => ProviderWithStatus | undefined;
  /** Update provider status in place */
  updateProviderStatus: (id: string, status: ConnectionStatus) => void;
  /** Clear error */
  clearError: () => void;
}

export type ProviderStore = ProviderState & ProviderActions;

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Convert ProviderConfig to ProviderWithStatus
 */
async function enrichProviderWithStatus(
  provider: ProviderConfig
): Promise<ProviderWithStatus> {
  let keyExists = false;
  let storeType: KeyringStoreType | undefined;

  if (provider.apiKeyRef) {
    try {
      const providerName = provider.name;
      keyExists = await apiKeyExists(providerName);
      storeType = await getKeyringStoreType();
    } catch (error) {
      console.warn(`Failed to check key status for ${provider.name}:`, error);
    }
  }

  return {
    ...provider,
    connectionStatus: 'untested' as ConnectionStatus,
    keyExists,
    storeType,
  };
}

// ============================================================================
// Store Creation
// ============================================================================

/**
 * Provider store with Zustand
 *
 * Uses subscribeWithSelector for fine-grained subscriptions
 *
 * @example
 * ```tsx
 * // In a component
 * const providers = useProviderStore((state) => state.providers);
 * const testConnection = useProviderStore((state) => state.testConnection);
 *
 * // Or get entire store
 * const { providers, testConnection, isLoading } = useProviderStore();
 * ```
 */
export const useProviderStore = create<ProviderStore>()(
  subscribeWithSelector((set, get) => ({
    // Initial state
    providers: [],
    isLoading: false,
    error: null,
    testingStates: {},
    storeType: null,
    isInitialized: false,

    // Actions
    refresh: async () => {
      set({ isLoading: true, error: null });

      try {
        const providerList = await listProviders();
        const enrichedProviders = await Promise.all(
          providerList.map(enrichProviderWithStatus)
        );

        // Get store type
        let storeType: KeyringStoreType | null = null;
        try {
          storeType = await getKeyringStoreType();
        } catch {
          // Ignore keyring errors during load
        }

        set({
          providers: enrichedProviders,
          storeType,
          isLoading: false,
          isInitialized: true,
        });
      } catch (err) {
        const message = err instanceof Error ? err.message : '获取提供商列表失败';
        set({ error: message, isLoading: false });
        console.error('Failed to load providers:', err);
      }
    },

    initialize: async () => {
      const { isInitialized, isLoading } = get();
      if (isInitialized || isLoading) return;

      await get().refresh();
    },

    addProvider: async (config: NewProviderConfig) => {
      try {
        const newProvider = await createProvider(config);
        await get().refresh();
        toast.success('提供商已添加', {
          description: `${config.name} 已成功添加`,
        });
        return newProvider;
      } catch (err) {
        const message = err instanceof Error ? err.message : '添加提供商失败';
        toast.error('添加失败', { description: message });
        console.error('Failed to add provider:', err);
        return null;
      }
    },

    editProvider: async (id: string, update: ProviderConfigUpdate) => {
      try {
        const updatedProvider = await updateProvider(id, update);
        await get().refresh();
        toast.success('提供商已更新', {
          description: update.name ? `${update.name} 已成功更新` : '配置已更新',
        });
        return updatedProvider;
      } catch (err) {
        const message = err instanceof Error ? err.message : '更新提供商失败';
        toast.error('更新失败', { description: message });
        console.error('Failed to update provider:', err);
        return null;
      }
    },

    removeProvider: async (id: string) => {
      try {
        await deleteProvider(id);
        await get().refresh();
        toast.success('提供商已删除');
        return true;
      } catch (err) {
        const message = err instanceof Error ? err.message : '删除提供商失败';
        toast.error('删除失败', { description: message });
        console.error('Failed to delete provider:', err);
        return false;
      }
    },

    setAsDefault: async (id: string) => {
      try {
        await setDefaultProvider(id);
        await get().refresh();
        toast.success('默认提供商已更新');
        return true;
      } catch (err) {
        const message = err instanceof Error ? err.message : '设置默认提供商失败';
        toast.error('设置失败', { description: message });
        console.error('Failed to set default provider:', err);
        return false;
      }
    },

    testConnection: async (id: string) => {
      const { providers } = get();
      const provider = providers.find((p) => p.id === id);
      if (!provider) {
        toast.error('连接测试失败', { description: '找不到提供商配置' });
        return null;
      }

      // Set testing state
      set((state) => ({
        testingStates: { ...state.testingStates, [id]: true },
        providers: state.providers.map((p) =>
          p.id === id ? { ...p, connectionStatus: 'testing' as ConnectionStatus } : p
        ),
      }));

      try {
        const result = await testProviderConnection(provider);

        // Update provider status
        set((state) => ({
          providers: state.providers.map((p) =>
            p.id === id
              ? {
                  ...p,
                  connectionStatus: result.healthy
                    ? ('connected' as ConnectionStatus)
                    : ('failed' as ConnectionStatus),
                  lastTested: Date.now(),
                }
              : p
          ),
          testingStates: Object.fromEntries(
            Object.entries(state.testingStates).filter(([key]) => key !== id)
          ),
        }));

        if (result.healthy) {
          toast.success('连接成功');
        } else {
          toast.error('连接失败', { description: '无法连接到提供商服务' });
        }

        return result;
      } catch (err) {
        const message = err instanceof Error ? err.message : '连接测试失败';

        set((state) => ({
          providers: state.providers.map((p) =>
            p.id === id
              ? {
                  ...p,
                  connectionStatus: 'failed' as ConnectionStatus,
                  lastTested: Date.now(),
                }
              : p
          ),
          testingStates: Object.fromEntries(
            Object.entries(state.testingStates).filter(([key]) => key !== id)
          ),
        }));

        toast.error('连接测试失败', { description: message });
        console.error('Failed to test connection:', err);
        return null;
      }
    },

    setAgentProvider: async (agentUuid: string, providerId: string) => {
      try {
        await setAgentDefaultProvider(agentUuid, providerId);
        await get().refresh();
        return true;
      } catch (err) {
        const message = err instanceof Error ? err.message : '设置代理提供商失败';
        toast.error('设置失败', { description: message });
        console.error('Failed to set agent provider:', err);
        return false;
      }
    },

    getAgentProvider: async (agentUuid: string) => {
      try {
        return await getAgentProvider(agentUuid);
      } catch (err) {
        console.error('Failed to get agent provider:', err);
        return null;
      }
    },

    validateForAgent: async (providerId: string) => {
      try {
        return await validateProviderForAgent(providerId);
      } catch (err) {
        console.error('Failed to validate provider for agent:', err);
        return {
          isValid: false,
          errors: [err instanceof Error ? err.message : '验证失败'],
          warnings: [],
          suggestions: [],
        };
      }
    },

    getProviderById: (id: string) => {
      return get().providers.find((p) => p.id === id);
    },

    getDefaultProvider: () => {
      return get().providers.find((p) => p.isDefault);
    },

    updateProviderStatus: (id: string, status: ConnectionStatus) => {
      set((state) => ({
        providers: state.providers.map((p) =>
          p.id === id ? { ...p, connectionStatus: status } : p
        ),
      }));
    },

    clearError: () => {
      set({ error: null });
    },
  }))
);

export default useProviderStore;