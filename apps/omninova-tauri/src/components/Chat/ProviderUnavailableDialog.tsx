/**
 * Provider Unavailable Dialog Component
 *
 * A dialog that shows when a provider becomes unavailable during conversation,
 * with suggestions for alternative providers and quick switch actions.
 *
 * [Source: Story 3.7 - Provider 切换与代理默认提供商]
 */

import * as React from 'react';
import { useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  AlertTriangle,
  CheckCircle2,
  XCircle,
  Key,
  Wifi,
  Settings,
  ArrowRight,
  RefreshCw,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  type ProviderWithStatus,
} from '@/types/provider';
import { type ProviderError, type ProviderErrorType } from '@/hooks/useConversationProvider';

// ============================================================================
// Types
// ============================================================================

export interface ProviderUnavailableDialogProps {
  /** Whether dialog is open */
  open: boolean;
  /** Close callback */
  onOpenChange: (open: boolean) => void;
  /** Provider error information */
  error: ProviderError | null;
  /** Current provider that failed */
  currentProvider: ProviderWithStatus | null;
  /** Default provider for fallback */
  defaultProvider: ProviderWithStatus | null;
  /** Available providers for switching */
  availableProviders: ProviderWithStatus[];
  /** Connection testing states */
  testingStates?: Record<string, boolean>;
  /** Switch provider callback */
  onSwitchProvider: (providerId: string) => void;
  /** Retry callback */
  onRetry?: () => void;
  /** Custom class name */
  className?: string;
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Get icon for error type
 */
function getErrorIcon(type: ProviderErrorType): React.ReactNode {
  switch (type) {
    case 'api_key_missing':
      return <Key className="h-5 w-5 text-amber-500" />;
    case 'connection_failed':
      return <Wifi className="h-5 w-5 text-red-500" />;
    case 'service_unavailable':
      return <AlertTriangle className="h-5 w-5 text-orange-500" />;
    case 'rate_limited':
      return <RefreshCw className="h-5 w-5 text-blue-500" />;
    default:
      return <AlertTriangle className="h-5 w-5 text-destructive" />;
  }
}

/**
 * Get title for error type
 */
function getErrorTitle(type: ProviderErrorType): string {
  switch (type) {
    case 'api_key_missing':
      return 'API 密钥问题';
    case 'connection_failed':
      return '连接失败';
    case 'service_unavailable':
      return '服务不可用';
    case 'rate_limited':
      return '请求频率超限';
    case 'provider_not_found':
      return '提供商未找到';
    default:
      return '提供商错误';
  }
}

/**
 * Get provider status for suggestions
 */
function getProviderHealthStatus(provider: ProviderWithStatus): 'healthy' | 'warning' | 'unhealthy' {
  switch (provider.connectionStatus) {
    case 'connected':
      return 'healthy';
    case 'failed':
      return 'unhealthy';
    default:
      return 'warning';
  }
}

// ============================================================================
// Sub-Components
// ============================================================================

/**
 * Provider suggestion card
 */
function ProviderSuggestionCard({
  provider,
  isDefault,
  isCurrent,
  testingState,
  onSelect,
}: {
  provider: ProviderWithStatus;
  isDefault: boolean;
  isCurrent: boolean;
  testingState: boolean;
  onSelect: () => void;
}) {
  const healthStatus = getProviderHealthStatus(provider);

  return (
    <div
      className={cn(
        'flex items-center justify-between p-3 rounded-lg border transition-colors',
        'hover:bg-muted/50',
        isCurrent && 'border-destructive/50 bg-destructive/5',
        healthStatus === 'healthy' && 'border-green-500/30',
        healthStatus === 'unhealthy' && 'border-red-500/30'
      )}
    >
      <div className="flex items-center gap-3">
        {/* Status indicator */}
        <div className="shrink-0">
          {healthStatus === 'healthy' && (
            <CheckCircle2 className="h-5 w-5 text-green-500" />
          )}
          {healthStatus === 'unhealthy' && (
            <XCircle className="h-5 w-5 text-red-500" />
          )}
          {healthStatus === 'warning' && (
            <div className="h-5 w-5 rounded-full bg-muted-foreground/30" />
          )}
        </div>

        {/* Provider info */}
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="font-medium truncate">{provider.name}</span>
            {isDefault && (
              <span className="text-[10px] text-blue-600 font-medium px-1.5 py-0.5 rounded bg-blue-100 dark:bg-blue-900/30">
                默认
              </span>
            )}
            {isCurrent && (
              <span className="text-[10px] text-destructive font-medium px-1.5 py-0.5 rounded bg-destructive/10">
                当前
              </span>
            )}
          </div>
          {provider.defaultModel && (
            <p className="text-xs text-muted-foreground truncate">
              {provider.defaultModel}
            </p>
          )}
        </div>
      </div>

      {/* Switch button */}
      {!isCurrent && (
        <Button
          variant="outline"
          size="sm"
          onClick={onSelect}
          disabled={testingState}
          className="shrink-0"
        >
          {testingState ? (
            <RefreshCw className="h-4 w-4 animate-spin" />
          ) : (
            <>
              切换
              <ArrowRight className="h-4 w-4 ml-1" />
            </>
          )}
        </Button>
      )}
    </div>
  );
}

// ============================================================================
// Main Component
// ============================================================================

/**
 * Provider Unavailable Dialog
 *
 * Shows when a provider error occurs, with suggestions for switching
 * to alternative providers.
 *
 * @example
 * ```tsx
 * <ProviderUnavailableDialog
 *   open={showError}
 *   onOpenChange={setShowError}
 *   error={lastError}
 *   currentProvider={currentProvider}
 *   defaultProvider={defaultProvider}
 *   availableProviders={providers}
 *   onSwitchProvider={handleSwitch}
 *   onRetry={handleRetry}
 * />
 * ```
 */
export function ProviderUnavailableDialog({
  open,
  onOpenChange,
  error,
  currentProvider,
  defaultProvider,
  availableProviders,
  testingStates = {},
  onSwitchProvider,
  onRetry,
  className,
}: ProviderUnavailableDialogProps): React.ReactElement {
  const navigate = useNavigate();

  // Get suggested providers (healthy ones first, exclude current)
  const suggestedProviders = useMemo(() => {
    const others = availableProviders.filter((p) => p.id !== currentProvider?.id);

    // Sort: healthy first, then warning, then unhealthy
    return others.sort((a, b) => {
      const aHealth = getProviderHealthStatus(a);
      const bHealth = getProviderHealthStatus(b);
      const order = { healthy: 0, warning: 1, unhealthy: 2 };
      return order[aHealth] - order[bHealth];
    });
  }, [availableProviders, currentProvider]);

  // Handle switch
  const handleSwitch = (providerId: string) => {
    onSwitchProvider(providerId);
    onOpenChange(false);
  };

  // Handle go to settings
  const handleGoToSettings = () => {
    onOpenChange(false);
    navigate('/settings/providers');
  };

  // If no error, don't render
  if (!error) return <></>;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className={cn('sm:max-w-[500px]', className)}>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {getErrorIcon(error.type)}
            {getErrorTitle(error.type)}
          </DialogTitle>
          <DialogDescription className="text-base pt-2">
            {error.message}
            {error.providerName && (
              <span className="block mt-1 text-sm text-muted-foreground">
                当前提供商: {error.providerName}
              </span>
            )}
          </DialogDescription>
        </DialogHeader>

        {/* Suggestion */}
        {error.suggestion && (
          <div className="px-1 py-2 text-sm text-muted-foreground">
            💡 {error.suggestion}
          </div>
        )}

        {/* Provider suggestions */}
        {suggestedProviders.length > 0 && (
          <div className="py-4 space-y-3">
            <p className="text-sm font-medium">可用提供商：</p>
            <div className="space-y-2 max-h-[200px] overflow-y-auto">
              {suggestedProviders.map((provider) => (
                <ProviderSuggestionCard
                  key={provider.id}
                  provider={provider}
                  isDefault={provider.id === defaultProvider?.id}
                  isCurrent={provider.id === currentProvider?.id}
                  testingState={testingStates[provider.id] ?? false}
                  onSelect={() => handleSwitch(provider.id)}
                />
              ))}
            </div>
          </div>
        )}

        {/* No providers available */}
        {suggestedProviders.length === 0 && (
          <div className="py-4 text-center text-muted-foreground">
            <p className="text-sm">没有其他可用的提供商</p>
            <p className="text-xs mt-1">请前往设置添加新的提供商</p>
          </div>
        )}

        <DialogFooter className="flex-col sm:flex-row gap-2">
          {/* Retry button */}
          {onRetry && (
            <Button variant="outline" onClick={onRetry}>
              <RefreshCw className="h-4 w-4 mr-2" />
              重试
            </Button>
          )}

          {/* Settings button */}
          <Button variant="outline" onClick={handleGoToSettings}>
            <Settings className="h-4 w-4 mr-2" />
            管理提供商
          </Button>

          {/* Default action */}
          {defaultProvider && defaultProvider.id !== currentProvider?.id && (
            <Button onClick={() => handleSwitch(defaultProvider.id)}>
              切换到默认提供商
              <ArrowRight className="h-4 w-4 ml-2" />
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export default ProviderUnavailableDialog;