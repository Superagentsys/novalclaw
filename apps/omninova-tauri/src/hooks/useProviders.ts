/**
 * Provider management hook for OmniNova Claw
 *
 * Provides CRUD operations and connection testing for LLM providers
 * Integrates with keychain for secure API key storage
 *
 * [Source: Story 3.6 - Provider 配置界面]
 */

import { useState, useCallback, useEffect } from 'react';
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
} from '@/types/provider';
import {
  apiKeyExists,
  getKeyringStoreType,
  type KeyringStoreType,
} from '@/types/keyring';

/**
 * Hook return type
 */
export interface UseProvidersReturn {
  /** List of providers with status */
  providers: ProviderWithStatus[];
  /** Loading state */
  isLoading: boolean;
  /** Error state */
  error: string | null;
  /** Refresh provider list */
  refresh: () => Promise<void>;
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
  /** Connection test states by provider ID */
  testingStates: Record<string, boolean>;
  /** Keyring store type */
  storeType: KeyringStoreType | null;
}

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
      // Extract provider name from apiKeyRef or use provider.name
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

/**
 * Provider management hook
 *
 * @returns Provider state and CRUD operations
 *
 * @example
 * ```tsx
 * function ProviderSettings() {
 *   const {
 *     providers,
 *     isLoading,
 *     addProvider,
 *     removeProvider,
 *     testConnection,
 *   } = useProviders();
 *
 *   // ... render UI
 * }
 * ```
 */
export function useProviders(): UseProvidersReturn {
  const [providers, setProviders] = useState<ProviderWithStatus[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [testingStates, setTestingStates] = useState<Record<string, boolean>>({});
  const [storeType, setStoreType] = useState<KeyringStoreType | null>(null);

  // Load providers
  const refresh = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      const providerList = await listProviders();
      const enrichedProviders = await Promise.all(
        providerList.map(enrichProviderWithStatus)
      );
      setProviders(enrichedProviders);

      // Get store type
      try {
        const type = await getKeyringStoreType();
        setStoreType(type);
      } catch {
        // Ignore keyring errors during load
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : '获取提供商列表失败';
      setError(message);
      console.error('Failed to load providers:', err);
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Add provider
  const addProvider = useCallback(
    async (config: NewProviderConfig): Promise<ProviderConfig | null> => {
      try {
        const newProvider = await createProvider(config);
        await refresh();
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
    [refresh]
  );

  // Update provider
  const editProvider = useCallback(
    async (id: string, update: ProviderConfigUpdate): Promise<ProviderConfig | null> => {
      try {
        const updatedProvider = await updateProvider(id, update);
        await refresh();
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
    [refresh]
  );

  // Delete provider
  const removeProvider = useCallback(
    async (id: string): Promise<boolean> => {
      try {
        await deleteProvider(id);
        await refresh();
        toast.success('提供商已删除');
        return true;
      } catch (err) {
        const message = err instanceof Error ? err.message : '删除提供商失败';
        toast.error('删除失败', { description: message });
        console.error('Failed to delete provider:', err);
        return false;
      }
    },
    [refresh]
  );

  // Set as default
  const setAsDefault = useCallback(
    async (id: string): Promise<boolean> => {
      try {
        await setDefaultProvider(id);
        await refresh();
        toast.success('默认提供商已更新');
        return true;
      } catch (err) {
        const message = err instanceof Error ? err.message : '设置默认提供商失败';
        toast.error('设置失败', { description: message });
        console.error('Failed to set default provider:', err);
        return false;
      }
    },
    [refresh]
  );

  // Test connection
  const testConnection = useCallback(
    async (id: string): Promise<ProviderTestResult | null> => {
      // Find the provider to get its config
      const provider = providers.find((p) => p.id === id);
      if (!provider) {
        toast.error('连接测试失败', { description: '找不到提供商配置' });
        return null;
      }

      setTestingStates((prev) => ({ ...prev, [id]: true }));

      // Update status to testing
      setProviders((prev) =>
        prev.map((p) =>
          p.id === id ? { ...p, connectionStatus: 'testing' as ConnectionStatus } : p
        )
      );

      try {
        // Pass full provider config to backend
        const result = await testProviderConnection(provider);

        // Update provider status based on result
        setProviders((prev) =>
          prev.map((p) =>
            p.id === id
              ? {
                  ...p,
                  connectionStatus: result.healthy
                    ? ('connected' as ConnectionStatus)
                    : ('failed' as ConnectionStatus),
                  lastTested: Date.now(),
                }
              : p
          )
        );

        if (result.healthy) {
          toast.success('连接成功');
        } else {
          toast.error('连接失败', { description: '无法连接到提供商服务' });
        }

        return result;
      } catch (err) {
        const message = err instanceof Error ? err.message : '连接测试失败';

        // Update status to failed
        setProviders((prev) =>
          prev.map((p) =>
            p.id === id
              ? {
                  ...p,
                  connectionStatus: 'failed' as ConnectionStatus,
                  lastTested: Date.now(),
                }
              : p
          )
        );

        toast.error('连接测试失败', { description: message });
        console.error('Failed to test connection:', err);
        return null;
      } finally {
        setTestingStates((prev) => {
          const next = { ...prev };
          delete next[id];
          return next;
        });
      }
    },
    [providers]
  );

  // Initial load
  useEffect(() => {
    refresh();
  }, [refresh]);

  return {
    providers,
    isLoading,
    error,
    refresh,
    addProvider,
    editProvider,
    removeProvider,
    setAsDefault,
    testConnection,
    testingStates,
    storeType,
  };
}

export default useProviders;