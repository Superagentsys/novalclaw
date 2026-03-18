/**
 * Agent Provider Hook
 *
 * Combines agent and provider data for provider assignment functionality.
 * Provides:
 * - Agent's default provider lookup
 * - Provider validation for agent assignment
 * - Provider assignment operations
 *
 * [Source: Story 3.7 - Provider 切换与代理默认提供商]
 */

import { useState, useCallback, useEffect, useMemo } from 'react';
import { useProviderStore } from '@/stores/providerStore';
import {
  type ProviderWithStatus,
  type AgentProviderValidation,
} from '@/types/provider';
import { type AgentModel } from '@/types/agent';

// ============================================================================
// Types
// ============================================================================

export interface UseAgentProviderOptions {
  /** Agent to get provider for */
  agent?: AgentModel | null;
  /** Auto-load provider on mount */
  autoLoad?: boolean;
}

export interface UseAgentProviderReturn {
  /** Agent's default provider */
  provider: ProviderWithStatus | null;
  /** Whether provider is loading */
  isLoading: boolean;
  /** Validation result for current provider */
  validation: AgentProviderValidation | null;
  /** Set agent's default provider */
  setProvider: (providerId: string) => Promise<boolean>;
  /** Validate a provider for this agent */
  validateProvider: (providerId: string) => Promise<AgentProviderValidation>;
  /** Available providers for assignment */
  availableProviders: ProviderWithStatus[];
  /** Refresh provider data */
  refresh: () => Promise<void>;
  /** Whether agent has a provider configured */
  hasProvider: boolean;
  /** Whether agent's provider is the global default */
  isGlobalDefault: boolean;
  /** Error state */
  error: string | null;
}

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * Agent Provider Hook
 *
 * Manages provider assignment for agents, combining agent and provider data.
 *
 * @param options - Configuration options
 * @returns Agent provider state and operations
 *
 * @example
 * ```tsx
 * function AgentProviderSelector({ agent }) {
 *   const {
 *     provider,
 *     setProvider,
 *     validateProvider,
 *     availableProviders,
 *     isLoading,
 *   } = useAgentProvider({ agent });
 *
 *   return (
 *     <Select
 *       value={provider?.id}
 *       onValueChange={setProvider}
 *       disabled={isLoading}
 *     >
 *       {availableProviders.map((p) => (
 *         <SelectItem key={p.id} value={p.id}>
 *           {p.name}
 *         </SelectItem>
 *       ))}
 *     </Select>
 *   );
 * }
 * ```
 */
export function useAgentProvider(
  options: UseAgentProviderOptions = {}
): UseAgentProviderReturn {
  const { agent, autoLoad = true } = options;

  // Store state
  const providers = useProviderStore((state) => state.providers);
  const isStoreLoading = useProviderStore((state) => state.isLoading);
  const getProviderById = useProviderStore((state) => state.getProviderById);
  const getDefaultProvider = useProviderStore((state) => state.getDefaultProvider);
  const setAgentProvider = useProviderStore((state) => state.setAgentProvider);
  const validateForAgent = useProviderStore((state) => state.validateForAgent);
  const initialize = useProviderStore((state) => state.initialize);

  // Local state
  const [isLoading, setIsLoading] = useState(false);
  const [validation, setValidation] = useState<AgentProviderValidation | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Initialize store on mount
  useEffect(() => {
    if (autoLoad) {
      initialize();
    }
  }, [autoLoad, initialize]);

  // Get agent's provider
  const provider = useMemo<ProviderWithStatus | null>(() => {
    if (!agent) return null;

    // If agent has a default_provider_id, find that provider
    if (agent.default_provider_id) {
      const found = getProviderById(agent.default_provider_id);
      if (found) return found;
      // Fall through to global default if provider not found
    }

    // Fall back to global default
    return getDefaultProvider() ?? null;
  }, [agent, getProviderById, getDefaultProvider]);

  // Whether agent has a provider configured
  const hasProvider = useMemo(() => {
    return provider !== null;
  }, [provider]);

  // Whether agent's provider is the global default
  const isGlobalDefault = useMemo(() => {
    if (!provider || !agent) return false;
    // Agent doesn't have explicit provider and is using global default
    return !agent.default_provider_id && provider.isDefault;
  }, [provider, agent]);

  // Available providers (all providers for selection)
  const availableProviders = useMemo(() => {
    return providers;
  }, [providers]);

  // Set agent's provider
  const setProvider = useCallback(
    async (providerId: string): Promise<boolean> => {
      if (!agent) {
        setError('未选择代理');
        return false;
      }

      setIsLoading(true);
      setError(null);

      try {
        // Validate first
        const result = await validateForAgent(providerId);
        setValidation(result);

        if (!result.isValid) {
          setError(result.errors[0] ?? '提供商验证失败');
          return false;
        }

        // Set provider
        const success = await setAgentProvider(agent.agent_uuid, providerId);
        if (!success) {
          setError('设置提供商失败');
          return false;
        }

        return true;
      } catch (err) {
        const message = err instanceof Error ? err.message : '设置提供商失败';
        setError(message);
        return false;
      } finally {
        setIsLoading(false);
      }
    },
    [agent, setAgentProvider, validateForAgent]
  );

  // Validate provider
  const validateProvider = useCallback(
    async (providerId: string): Promise<AgentProviderValidation> => {
      try {
        const result = await validateForAgent(providerId);
        setValidation(result);
        return result;
      } catch (err) {
        const message = err instanceof Error ? err.message : '验证失败';
        const errorResult: AgentProviderValidation = {
          isValid: false,
          errors: [message],
          warnings: [],
          suggestions: [],
        };
        setValidation(errorResult);
        return errorResult;
      }
    },
    [validateForAgent]
  );

  // Refresh provider data
  const refresh = useCallback(async () => {
    setIsLoading(true);
    try {
      await initialize();
    } finally {
      setIsLoading(false);
    }
  }, [initialize]);

  return {
    provider,
    isLoading: isLoading || isStoreLoading,
    validation,
    setProvider,
    validateProvider,
    availableProviders,
    refresh,
    hasProvider,
    isGlobalDefault,
    error,
  };
}

export default useAgentProvider;