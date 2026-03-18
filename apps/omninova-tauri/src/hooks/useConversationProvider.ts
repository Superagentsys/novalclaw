/**
 * Conversation Provider Management Hook
 *
 * Manages provider selection for conversations, including:
 * - Tracking current provider (temporary or default)
 * - Switching providers during conversation
 * - Handling provider unavailability
 * - Persisting provider choice per session
 *
 * Uses the global Zustand provider store for state management.
 *
 * [Source: Story 3.7 - Provider 切换与代理默认提供商]
 */

import { useState, useCallback, useMemo, useEffect } from 'react';
import { type ProviderWithStatus } from '@/types/provider';
import { type AgentModel } from '@/types/agent';
import { useProviderStore } from '@/stores/providerStore';

// ============================================================================
// Types
// ============================================================================

/**
 * Provider error types
 */
export type ProviderErrorType =
  | 'provider_not_found'
  | 'api_key_missing'
  | 'connection_failed'
  | 'rate_limited'
  | 'service_unavailable'
  | 'unknown';

/**
 * Provider error information
 */
export interface ProviderError {
  type: ProviderErrorType;
  message: string;
  providerId?: string;
  providerName?: string;
  suggestion?: string;
}

/**
 * Hook return type
 */
export interface UseConversationProviderReturn {
  /** Current provider for this conversation */
  currentProvider: ProviderWithStatus | null;
  /** Agent's default provider */
  defaultProvider: ProviderWithStatus | null;
  /** Whether using temporary switch */
  isTemporarySwitch: boolean;
  /** Available providers for switching */
  availableProviders: ProviderWithStatus[];
  /** Switch provider temporarily */
  switchProvider: (providerId: string) => void;
  /** Reset to default provider */
  resetToDefault: () => void;
  /** Handle provider unavailable error */
  handleProviderError: (error: Error) => ProviderError | null;
  /** Last provider error */
  lastError: ProviderError | null;
  /** Clear error */
  clearError: () => void;
  /** Set agent for conversation */
  setAgent: (agent: AgentModel | null) => void;
  /** Current agent */
  currentAgent: AgentModel | null;
  /** Loading state */
  isLoading: boolean;
  /** Connection testing states by provider ID */
  testingStates: Record<string, boolean>;
  /** Test provider connection */
  testConnection: (id: string) => Promise<{ healthy: boolean } | null>;
}

// ============================================================================
// Error Detection
// ============================================================================

/**
 * Parse error to determine provider error type
 */
function parseProviderError(error: Error): ProviderError {
  const message = error.message.toLowerCase();

  // API Key related errors
  if (
    message.includes('api key') ||
    message.includes('apikey') ||
    message.includes('authentication') ||
    message.includes('unauthorized') ||
    message.includes('401')
  ) {
    return {
      type: 'api_key_missing',
      message: '提供商缺少 API 密钥或密钥无效',
      suggestion: '请检查 API 密钥配置，或切换到其他提供商',
    };
  }

  // Rate limiting
  if (
    message.includes('rate limit') ||
    message.includes('too many requests') ||
    message.includes('429')
  ) {
    return {
      type: 'rate_limited',
      message: '请求频率超限',
      suggestion: '请稍后重试，或切换到其他提供商',
    };
  }

  // Connection errors
  if (
    message.includes('connection') ||
    message.includes('network') ||
    message.includes('timeout') ||
    message.includes('econnrefused') ||
    message.includes('enotfound')
  ) {
    return {
      type: 'connection_failed',
      message: '网络连接失败',
      suggestion: '请检查网络连接，或切换到其他提供商',
    };
  }

  // Service unavailable
  if (
    message.includes('service unavailable') ||
    message.includes('503') ||
    message.includes('overloaded')
  ) {
    return {
      type: 'service_unavailable',
      message: '服务暂时不可用',
      suggestion: '服务可能正在维护，建议切换到其他提供商',
    };
  }

  // Provider not found
  if (message.includes('provider not found') || message.includes('未找到提供商')) {
    return {
      type: 'provider_not_found',
      message: '未找到提供商配置',
      suggestion: '请检查提供商设置，或选择其他提供商',
    };
  }

  // Unknown error
  return {
    type: 'unknown',
    message: error.message || '未知错误',
    suggestion: '请尝试切换到其他提供商',
  };
}

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * Conversation provider management hook
 *
 * @param initialAgent - Optional initial agent to use
 * @returns Provider state and management functions
 *
 * @example
 * ```tsx
 * function ChatHeader({ agent }) {
 *   const {
 *     currentProvider,
 *     isTemporarySwitch,
 *     switchProvider,
 *     resetToDefault,
 *     availableProviders,
 *   } = useConversationProvider(agent);
 *
 *   return (
 *     <div>
 *       <span>{currentProvider?.name ?? '未选择提供商'}</span>
 *       {isTemporarySwitch && <Badge>临时切换</Badge>}
 *       <ProviderDropdown
 *         providers={availableProviders}
 *         onSelect={switchProvider}
 *       />
 *     </div>
 *   );
 * }
 * ```
 */
export function useConversationProvider(
  initialAgent?: AgentModel | null
): UseConversationProviderReturn {
  // Use Zustand store for global provider state
  const providers = useProviderStore((state) => state.providers);
  const isStoreLoading = useProviderStore((state) => state.isLoading);
  const testingStates = useProviderStore((state) => state.testingStates);
  const testConnection = useProviderStore((state) => state.testConnection);
  const initialize = useProviderStore((state) => state.initialize);

  // State
  const [internalAgent, setInternalAgent] = useState<AgentModel | null | undefined>(undefined);
  const [temporaryProviderId, setTemporaryProviderId] = useState<string | null>(null);
  const [lastError, setLastError] = useState<ProviderError | null>(null);

  // Initialize store on mount
  useEffect(() => {
    initialize();
  }, [initialize]);

  // Determine current agent - if internalAgent is explicitly set (including null), use it
  // Otherwise use the initialAgent
  const currentAgent = useMemo(() => {
    if (internalAgent !== undefined) {
      return internalAgent; // Could be null if explicitly set
    }
    return initialAgent ?? null;
  }, [internalAgent, initialAgent]);

  // Find default provider for current agent
  const defaultProvider = useMemo<ProviderWithStatus | null>(() => {
    if (!currentAgent?.default_provider_id) {
      // Fall back to global default
      return providers.find((p) => p.isDefault) ?? null;
    }
    // Try to find agent's specific provider
    const agentProvider = providers.find((p) => p.id === currentAgent.default_provider_id);
    if (agentProvider) {
      return agentProvider;
    }
    // Fall back to global default if agent's provider not found
    return providers.find((p) => p.isDefault) ?? null;
  }, [currentAgent, providers]);

  // Current provider (temporary or default)
  const currentProvider = useMemo<ProviderWithStatus | null>(() => {
    if (temporaryProviderId) {
      return providers.find((p) => p.id === temporaryProviderId) ?? defaultProvider;
    }
    return defaultProvider;
  }, [temporaryProviderId, providers, defaultProvider]);

  // Is using temporary switch
  const isTemporarySwitch = useMemo(() => {
    return temporaryProviderId !== null;
  }, [temporaryProviderId]);

  // Available providers (exclude current from suggestions, but include all for selection)
  const availableProviders = useMemo<ProviderWithStatus[]>(() => {
    return providers;
  }, [providers]);

  // Switch provider temporarily
  const switchProvider = useCallback((providerId: string) => {
    const provider = providers.find((p) => p.id === providerId);
    if (provider) {
      setTemporaryProviderId(providerId);
      setLastError(null);
    }
  }, [providers]);

  // Reset to default provider
  const resetToDefault = useCallback(() => {
    setTemporaryProviderId(null);
    setLastError(null);
  }, []);

  // Handle provider error
  const handleProviderError = useCallback((error: Error): ProviderError | null => {
    const providerError = parseProviderError(error);

    // Add provider info
    if (currentProvider) {
      providerError.providerId = currentProvider.id;
      providerError.providerName = currentProvider.name;
    }

    setLastError(providerError);
    return providerError;
  }, [currentProvider]);

  // Clear error
  const clearError = useCallback(() => {
    setLastError(null);
  }, []);

  // Set agent
  const setAgent = useCallback((agent: AgentModel | null) => {
    setInternalAgent(agent);
    // Reset temporary provider when switching agents
    setTemporaryProviderId(null);
    setLastError(null);
  }, []);

  return {
    currentProvider,
    defaultProvider,
    isTemporarySwitch,
    availableProviders,
    switchProvider,
    resetToDefault,
    handleProviderError,
    lastError,
    clearError,
    setAgent,
    currentAgent,
    isLoading: isStoreLoading,
    testingStates,
    testConnection: async (id: string) => {
      const result = await testConnection(id);
      return result ? { healthy: result.healthy } : null;
    },
  };
}

export default useConversationProvider;