/**
 * Provider Selector Component
 *
 * A dropdown selector for choosing LLM providers with:
 * - Connection status indicators
 * - Provider type badges (cloud/local)
 * - Test connection button
 * - Empty state with link to settings
 *
 * [Source: Story 3.7 - Provider 切换与代理默认提供商]
 */

import * as React from 'react';
import { useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  Cloud,
  HardDrive,
  CheckCircle2,
  XCircle,
  Clock,
  Loader2,
  RefreshCw,
  Settings,
  AlertCircle,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  type ConnectionStatus,
  type ProviderCategory,
} from '@/types/provider';
import { useProviders } from '@/hooks/useProviders';

// ============================================================================
// Types
// ============================================================================

export interface ProviderSelectorProps {
  /** Currently selected provider ID */
  value?: string;
  /** Called when provider selection changes */
  onChange: (providerId: string | undefined) => void;
  /** Show test connection button */
  showTestButton?: boolean;
  /** Show manage providers link when empty */
  showEmptyStateLink?: boolean;
  /** Disabled state */
  disabled?: boolean;
  /** Custom class name */
  className?: string;
  /** Placeholder text */
  placeholder?: string;
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Get provider category (cloud/local)
 */
function getProviderCategory(providerType: string): ProviderCategory {
  const localProviders = ['ollama', 'lmstudio', 'llamacpp', 'vllm', 'sglang'];
  return localProviders.includes(providerType) ? 'local' : 'cloud';
}

/**
 * Get connection status icon
 */
function getConnectionIcon(status: ConnectionStatus): React.ReactNode {
  switch (status) {
    case 'connected':
      return <CheckCircle2 className="h-3.5 w-3.5 text-green-600" />;
    case 'failed':
      return <XCircle className="h-3.5 w-3.5 text-destructive" />;
    case 'testing':
      return <Loader2 className="h-3.5 w-3.5 animate-spin text-blue-500" />;
    default:
      return <Clock className="h-3.5 w-3.5 text-muted-foreground" />;
  }
}

/**
 * Get connection status tooltip text
 */
function getConnectionTooltip(status: ConnectionStatus, lastTested?: number): string {
  switch (status) {
    case 'connected':
      return '连接正常';
    case 'failed':
      return '连接失败';
    case 'testing':
      return '测试中...';
    default:
      return lastTested ? '未测试' : '尚未测试连接';
  }
}

// ============================================================================
// Sub-Components
// ============================================================================

/**
 * Provider category badge
 */
function ProviderCategoryBadge({ category }: { category: ProviderCategory }) {
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium',
        category === 'cloud'
          ? 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400'
          : 'bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400'
      )}
    >
      {category === 'cloud' ? (
        <>
          <Cloud className="h-3 w-3" />
          云端
        </>
      ) : (
        <>
          <HardDrive className="h-3 w-3" />
          本地
        </>
      )}
    </span>
  );
}

/**
 * Empty state component
 */
function EmptyState({ onNavigate }: { onNavigate: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center py-6 px-4 text-center">
      <AlertCircle className="h-8 w-8 text-muted-foreground/50 mb-3" />
      <p className="text-sm text-muted-foreground mb-3">
        暂无可用的提供商
      </p>
      <Button
        variant="outline"
        size="sm"
        onClick={onNavigate}
        className="gap-2"
      >
        <Settings className="h-4 w-4" />
        前往设置
      </Button>
    </div>
  );
}

// ============================================================================
// Main Component
// ============================================================================

/**
 * Provider Selector Component
 *
 * Allows selecting a provider from a dropdown with status indicators
 * and optional test connection functionality.
 *
 * @example
 * ```tsx
 * <ProviderSelector
 *   value={formData.defaultProviderId}
 *   onChange={(id) => setFormData({ ...formData, defaultProviderId: id })}
 *   showTestButton
 *   showEmptyStateLink
 * />
 * ```
 */
export function ProviderSelector({
  value,
  onChange,
  showTestButton = true,
  showEmptyStateLink = true,
  disabled = false,
  className,
  placeholder = '选择默认提供商',
}: ProviderSelectorProps): React.ReactElement {
  const navigate = useNavigate();
  const { providers, isLoading, testConnection, testingStates } = useProviders();

  // Find selected provider
  const selectedProvider = useMemo(
    () => providers.find((p) => p.id === value),
    [providers, value]
  );

  // Handle selection change
  const handleSelect = (providerId: string | null) => {
    if (providerId === '__none__' || providerId === null) {
      onChange(undefined);
    } else {
      onChange(providerId);
    }
  };

  // Handle test connection
  const handleTest = async (e: React.MouseEvent, providerId: string) => {
    e.stopPropagation();
    await testConnection(providerId);
  };

  // Navigate to settings
  const handleNavigateSettings = () => {
    navigate('/settings/providers');
  };

  // Loading state
  if (isLoading) {
    return (
      <div className={cn('flex items-center gap-2', className)}>
        <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
        <span className="text-sm text-muted-foreground">加载提供商...</span>
      </div>
    );
  }

  // Empty state
  if (providers.length === 0) {
    if (showEmptyStateLink) {
      return (
        <div className={className}>
          <EmptyState onNavigate={handleNavigateSettings} />
        </div>
      );
    }
    return (
      <div className={cn('text-sm text-muted-foreground', className)}>
        暂无可用的提供商
      </div>
    );
  }

  return (
    <div className={cn('space-y-2', className)}>
      <Select
        value={value || '__none__'}
        onValueChange={handleSelect}
        disabled={disabled}
      >
        <SelectTrigger className="w-full">
          <SelectValue placeholder={placeholder}>
            {selectedProvider ? (
              <div className="flex items-center gap-2">
                <span className="truncate">{selectedProvider.name}</span>
                <ProviderCategoryBadge
                  category={getProviderCategory(selectedProvider.providerType)}
                />
              </div>
            ) : (
              <span className="text-muted-foreground">{placeholder}</span>
            )}
          </SelectValue>
        </SelectTrigger>

        <SelectContent>
          {/* None option */}
          <SelectItem value="__none__">
            <span className="text-muted-foreground">不设置默认提供商</span>
          </SelectItem>

          {/* Provider options */}
          {providers.map((provider) => {
            const category = getProviderCategory(provider.providerType);
            const isTesting = testingStates[provider.id];
            const status = isTesting ? 'testing' : provider.connectionStatus;

            return (
              <SelectItem
                key={provider.id}
                value={provider.id}
                className="cursor-pointer"
              >
                <div className="flex items-center justify-between w-full gap-2 py-0.5">
                  <div className="flex items-center gap-2 min-w-0">
                    {/* Provider name */}
                    <span className="truncate font-medium">{provider.name}</span>

                    {/* Category badge */}
                    <ProviderCategoryBadge category={category} />

                    {/* Default model */}
                    {provider.defaultModel && (
                      <span className="text-xs text-muted-foreground truncate max-w-[120px]">
                        {provider.defaultModel}
                      </span>
                    )}
                  </div>

                  {/* Status indicator with title for tooltip */}
                  <div
                    className="flex items-center gap-1 shrink-0 cursor-help"
                    title={getConnectionTooltip(status, provider.lastTested)}
                  >
                    {getConnectionIcon(status)}
                  </div>
                </div>
              </SelectItem>
            );
          })}
        </SelectContent>
      </Select>

      {/* Selected provider details */}
      {selectedProvider && (
        <div className="flex items-center justify-between text-xs text-muted-foreground">
          <div className="flex items-center gap-2">
            {/* Connection status */}
            <div className="flex items-center gap-1">
              {getConnectionIcon(
                testingStates[selectedProvider.id]
                  ? 'testing'
                  : selectedProvider.connectionStatus
              )}
              <span>
                {getConnectionTooltip(
                  testingStates[selectedProvider.id]
                    ? 'testing'
                    : selectedProvider.connectionStatus,
                  selectedProvider.lastTested
                )}
              </span>
            </div>

            {/* Model info */}
            {selectedProvider.defaultModel && (
              <>
                <span className="text-border">•</span>
                <span>模型: {selectedProvider.defaultModel}</span>
              </>
            )}
          </div>

          {/* Test connection button */}
          {showTestButton && (
            <Button
              variant="ghost"
              size="sm"
              className="h-6 px-2 text-xs"
              onClick={(e) => handleTest(e, selectedProvider.id)}
              disabled={testingStates[selectedProvider.id] || disabled}
            >
              {testingStates[selectedProvider.id] ? (
                <>
                  <Loader2 className="h-3 w-3 mr-1 animate-spin" />
                  测试中
                </>
              ) : (
                <>
                  <RefreshCw className="h-3 w-3 mr-1" />
                  测试连接
                </>
              )}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}

export default ProviderSelector;