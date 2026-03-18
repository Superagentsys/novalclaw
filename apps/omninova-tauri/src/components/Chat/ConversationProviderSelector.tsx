/**
 * Conversation Provider Selector Component
 *
 * A compact provider selector for the chat interface that:
 * - Shows current provider with status indicator
 * - Allows temporary provider switching
 * - Indicates when using a non-default provider
 * - Provides quick reset to default
 *
 * [Source: Story 3.7 - Provider 切换与代理默认提供商]
 */

import * as React from 'react';
import { useMemo } from 'react';
import {
  CheckCircle2,
  XCircle,
  Clock,
  Loader2,
  RotateCcw,
  Zap,
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
  type ProviderWithStatus,
  type ConnectionStatus,
} from '@/types/provider';

// ============================================================================
// Types
// ============================================================================

export interface ConversationProviderSelectorProps {
  /** Current provider */
  currentProvider: ProviderWithStatus | null;
  /** Default provider for reset */
  defaultProvider: ProviderWithStatus | null;
  /** Whether using temporary switch */
  isTemporarySwitch: boolean;
  /** Available providers */
  availableProviders: ProviderWithStatus[];
  /** Connection testing states */
  testingStates?: Record<string, boolean>;
  /** Switch provider callback */
  onSwitchProvider: (providerId: string) => void;
  /** Reset to default callback */
  onResetToDefault: () => void;
  /** Disabled state */
  disabled?: boolean;
  /** Custom class name */
  className?: string;
  /** Compact mode for smaller UI */
  compact?: boolean;
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Get connection status icon (compact version)
 */
function getStatusIcon(status: ConnectionStatus, size: 'sm' | 'xs' = 'xs'): React.ReactNode {
  const sizeClass = size === 'sm' ? 'h-3.5 w-3.5' : 'h-3 w-3';

  switch (status) {
    case 'connected':
      return <CheckCircle2 className={cn(sizeClass, 'text-green-600')} />;
    case 'failed':
      return <XCircle className={cn(sizeClass, 'text-destructive')} />;
    case 'testing':
      return <Loader2 className={cn(sizeClass, 'animate-spin text-blue-500')} />;
    default:
      return <Clock className={cn(sizeClass, 'text-muted-foreground')} />;
  }
}

/**
 * Get status text
 */
function getStatusText(status: ConnectionStatus): string {
  switch (status) {
    case 'connected':
      return '正常';
    case 'failed':
      return '异常';
    case 'testing':
      return '测试中';
    default:
      return '未测试';
  }
}

// ============================================================================
// Main Component
// ============================================================================

/**
 * Conversation Provider Selector
 *
 * A compact provider selector for chat headers that shows current provider
 * and allows temporary switching.
 *
 * @example
 * ```tsx
 * <ConversationProviderSelector
 *   currentProvider={currentProvider}
 *   defaultProvider={defaultProvider}
 *   isTemporarySwitch={isTemporarySwitch}
 *   availableProviders={providers}
 *   onSwitchProvider={switchProvider}
 *   onResetToDefault={resetToDefault}
 * />
 * ```
 */
export function ConversationProviderSelector({
  currentProvider,
  defaultProvider,
  isTemporarySwitch,
  availableProviders,
  testingStates = {},
  onSwitchProvider,
  onResetToDefault,
  disabled = false,
  className,
  compact = false,
}: ConversationProviderSelectorProps): React.ReactElement {
  // Get current status
  const currentStatus = currentProvider?.connectionStatus ?? 'untested';
  const isTesting = currentProvider ? testingStates[currentProvider.id] : false;
  const displayStatus = isTesting ? 'testing' : currentStatus;

  // Handle selection
  const handleSelect = (value: string | null) => {
    if (value && value !== '__current__') {
      onSwitchProvider(value);
    }
  };

  // Get provider name for display
  const displayName = useMemo(() => {
    if (!currentProvider) return '未选择提供商';
    return currentProvider.name;
  }, [currentProvider]);

  // Render compact version
  if (compact) {
    return (
      <div className={cn('flex items-center gap-1.5', className)}>
        <Select
          value={currentProvider?.id ?? '__none__'}
          onValueChange={handleSelect}
          disabled={disabled || availableProviders.length === 0}
        >
          <SelectTrigger
            className={cn(
              'h-7 w-auto min-w-[120px] max-w-[200px] gap-1 border-0 bg-muted/50 px-2',
              'hover:bg-muted focus:ring-1'
            )}
          >
            <div className="flex items-center gap-1.5">
              {getStatusIcon(displayStatus)}
              <span className="truncate text-xs">{displayName}</span>
              {isTemporarySwitch && (
                <span className="text-[10px] text-amber-600 font-medium">临时</span>
              )}
            </div>
          </SelectTrigger>

          <SelectContent className="min-w-[180px]">
            {availableProviders.map((provider) => (
              <SelectItem key={provider.id} value={provider.id}>
                <div className="flex items-center gap-2">
                  <span className="truncate">{provider.name}</span>
                  {getStatusIcon(provider.connectionStatus)}
                </div>
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        {isTemporarySwitch && (
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6"
            onClick={onResetToDefault}
            disabled={disabled}
            title="恢复默认提供商"
          >
            <RotateCcw className="h-3 w-3" />
          </Button>
        )}
      </div>
    );
  }

  // Render full version
  return (
    <div className={cn('flex items-center gap-2', className)}>
      {/* Provider selector */}
      <Select
        value={currentProvider?.id ?? '__none__'}
        onValueChange={handleSelect}
        disabled={disabled || availableProviders.length === 0}
      >
        <SelectTrigger
          className={cn(
            'w-auto min-w-[180px] max-w-[280px]',
            isTemporarySwitch && 'border-amber-500/50 bg-amber-50/50 dark:bg-amber-950/20'
          )}
        >
          <SelectValue placeholder="选择提供商">
            <div className="flex items-center gap-2">
              <Zap className="h-4 w-4 text-muted-foreground" />
              <span className="truncate">{displayName}</span>
              {currentProvider?.defaultModel && (
                <span className="text-xs text-muted-foreground truncate max-w-[80px]">
                  {currentProvider.defaultModel}
                </span>
              )}
            </div>
          </SelectValue>
        </SelectTrigger>

        <SelectContent className="min-w-[220px]">
          {availableProviders.map((provider) => {
            const isCurrent = provider.id === currentProvider?.id;
            const isTestingThis = testingStates[provider.id];
            const status = isTestingThis ? 'testing' : provider.connectionStatus;

            return (
              <SelectItem
                key={provider.id}
                value={provider.id}
                disabled={isCurrent}
                className="cursor-pointer"
              >
                <div className="flex items-center justify-between w-full gap-2">
                  <div className="flex items-center gap-2 min-w-0">
                    <span className="truncate font-medium">{provider.name}</span>
                    {provider.isDefault && (
                      <span className="text-[10px] text-blue-600 font-medium">默认</span>
                    )}
                  </div>
                  <div
                    className="flex items-center gap-1 shrink-0 cursor-help"
                    title={`${getStatusText(status)}`}
                  >
                    {getStatusIcon(status)}
                  </div>
                </div>
              </SelectItem>
            );
          })}
        </SelectContent>
      </Select>

      {/* Status indicator */}
      <div
        className="flex items-center gap-1 px-2 py-1 rounded-md bg-muted/50 cursor-help"
        title={`状态: ${getStatusText(displayStatus)}`}
      >
        {getStatusIcon(displayStatus, 'sm')}
        <span className="text-xs text-muted-foreground">
          {getStatusText(displayStatus)}
        </span>
      </div>

      {/* Temporary switch indicator & reset button */}
      {isTemporarySwitch && (
        <div className="flex items-center gap-1">
          <span className="text-xs text-amber-600 font-medium px-2 py-0.5 rounded bg-amber-100 dark:bg-amber-900/30">
            临时切换
          </span>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2"
            onClick={onResetToDefault}
            disabled={disabled}
            title={`恢复默认: ${defaultProvider?.name ?? '未设置'}`}
          >
            <RotateCcw className="h-3 w-3 mr-1" />
            恢复
          </Button>
        </div>
      )}
    </div>
  );
}

export default ConversationProviderSelector;