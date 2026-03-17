/**
 * Provider Settings Page Component
 *
 * Main settings page for managing LLM providers
 *
 * [Source: Story 3.6 - Provider 配置界面]
 */

import * as React from 'react';
import { useState, useCallback } from 'react';
import {
  Plus,
  Cloud,
  HardDrive,
  Settings,
  AlertCircle,
  Loader2,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
} from '@/components/ui/card';
import { ProviderCard } from './ProviderCard';
import { ProviderFormDialog } from './ProviderFormDialog';
import { useProviders } from '@/hooks/useProviders';
import type {
  NewProviderConfig,
  ProviderConfigUpdate,
  ProviderWithStatus,
} from '@/types/provider';

// ============================================================================
// Types
// ============================================================================

interface ProviderSettingsProps {
  /** Settings change callback */
  onSettingsChange?: () => void;
}

// ============================================================================
// Sub-Components
// ============================================================================

/**
 * Empty state component
 */
function EmptyState({
  onAddClick,
}: {
  onAddClick: () => void;
}): React.ReactElement {
  return (
    <Card className="border-dashed">
      <CardContent className="flex flex-col items-center justify-center py-12">
        <div className="flex h-16 w-16 items-center justify-center rounded-full bg-muted mb-4">
          <Cloud className="h-8 w-8 text-muted-foreground" />
        </div>
        <h3 className="text-lg font-medium mb-1">暂无提供商</h3>
        <p className="text-sm text-muted-foreground text-center max-w-sm mb-4">
          添加您的第一个 LLM 提供商以开始使用 AI 代理。
          支持云端服务如 OpenAI、Anthropic，或本地服务如 Ollama。
        </p>
        <Button onClick={onAddClick}>
          <Plus className="h-4 w-4 mr-1.5" />
          添加提供商
        </Button>
      </CardContent>
    </Card>
  );
}

/**
 * Loading state component
 */
function LoadingState(): React.ReactElement {
  return (
    <Card>
      <CardContent className="flex items-center justify-center py-12">
        <Loader2 className="h-6 w-6 animate-spin mr-2" />
        <span className="text-muted-foreground">加载提供商列表...</span>
      </CardContent>
    </Card>
  );
}

/**
 * Error state component
 */
function ErrorState({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}): React.ReactElement {
  return (
    <Card className="border-destructive/50">
      <CardContent className="flex flex-col items-center justify-center py-12">
        <AlertCircle className="h-10 w-10 text-destructive mb-4" />
        <h3 className="text-lg font-medium mb-1">加载失败</h3>
        <p className="text-sm text-muted-foreground mb-4">{message}</p>
        <Button variant="outline" onClick={onRetry}>
          重试
        </Button>
      </CardContent>
    </Card>
  );
}

/**
 * Stats header component
 */
function StatsHeader({
  total,
  connected,
  local,
}: {
  total: number;
  connected: number;
  local: number;
}): React.ReactElement {
  return (
    <div className="flex gap-4 text-sm text-muted-foreground">
      <div className="flex items-center gap-1.5">
        <Cloud className="h-4 w-4" />
        <span>{total - local} 个云端</span>
      </div>
      <div className="flex items-center gap-1.5">
        <HardDrive className="h-4 w-4" />
        <span>{local} 个本地</span>
      </div>
      {connected > 0 && (
        <div className="flex items-center gap-1.5 text-green-600">
          <span>{connected} 个已连接</span>
        </div>
      )}
    </div>
  );
}

// ============================================================================
// Main Component
// ============================================================================

/**
 * Provider Settings Page Component
 *
 * Displays and manages LLM provider configurations with:
 * - Provider list with connection status
 * - Add/Edit/Delete operations
 * - Connection testing
 * - Keychain integration
 */
export function ProviderSettings({
  onSettingsChange,
}: ProviderSettingsProps): React.ReactElement {
  // State
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editingProvider, setEditingProvider] = useState<ProviderWithStatus | null>(null);

  // Hooks
  const {
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
  } = useProviders();

  // Dialog handlers
  const handleOpenAddDialog = useCallback(() => {
    setEditingProvider(null);
    setDialogOpen(true);
  }, []);

  const handleOpenEditDialog = useCallback((provider: ProviderWithStatus) => {
    setEditingProvider(provider);
    setDialogOpen(true);
  }, []);

  const handleCloseDialog = useCallback(() => {
    setDialogOpen(false);
    setEditingProvider(null);
  }, []);

  // CRUD handlers
  const handleSubmit = useCallback(
    async (config: NewProviderConfig | ProviderConfigUpdate): Promise<boolean> => {
      if (editingProvider) {
        const result = await editProvider(editingProvider.id, config as ProviderConfigUpdate);
        if (result) {
          onSettingsChange?.();
          return true;
        }
        return false;
      } else {
        const result = await addProvider(config as NewProviderConfig);
        if (result) {
          onSettingsChange?.();
          return true;
        }
        return false;
      }
    },
    [editingProvider, addProvider, editProvider, onSettingsChange]
  );

  const handleDelete = useCallback(
    async (id: string) => {
      const success = await removeProvider(id);
      if (success) {
        onSettingsChange?.();
      }
    },
    [removeProvider, onSettingsChange]
  );

  const handleSetDefault = useCallback(
    async (id: string, _isDefault: boolean) => {
      const success = await setAsDefault(id);
      if (success) {
        onSettingsChange?.();
      }
    },
    [setAsDefault, onSettingsChange]
  );

  const handleTestConnection = useCallback(
    async (id: string) => {
      await testConnection(id);
    },
    [testConnection]
  );

  // Handle dialog test connection
  const handleDialogTestConnection = useCallback(async (): Promise<boolean> => {
    if (!editingProvider) return false;
    const result = await testConnection(editingProvider.id);
    return result?.healthy ?? false;
  }, [editingProvider, testConnection]);

  // Calculate stats
  const localProviders = ['ollama', 'lmstudio', 'llamacpp', 'vllm', 'sglang'];
  const stats = {
    total: providers.length,
    connected: providers.filter((p) => p.connectionStatus === 'connected').length,
    local: providers.filter((p) => localProviders.includes(p.providerType)).length,
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-start justify-between">
        <div>
          <h2 className="text-lg font-semibold flex items-center gap-2">
            <Settings className="h-5 w-5" />
            提供商设置
          </h2>
          <p className="text-sm text-muted-foreground mt-1">
            管理 LLM 提供商配置和 API 密钥
          </p>
        </div>

        {providers.length > 0 && (
          <Button onClick={handleOpenAddDialog}>
            <Plus className="h-4 w-4 mr-1.5" />
            添加提供商
          </Button>
        )}
      </div>

      {/* Stats */}
      {providers.length > 0 && (
        <StatsHeader
          total={stats.total}
          connected={stats.connected}
          local={stats.local}
        />
      )}

      {/* Content */}
      {isLoading ? (
        <LoadingState />
      ) : error ? (
        <ErrorState message={error} onRetry={refresh} />
      ) : providers.length === 0 ? (
        <EmptyState onAddClick={handleOpenAddDialog} />
      ) : (
        <div className="grid gap-4 md:grid-cols-2">
          {providers.map((provider) => (
            <ProviderCard
              key={provider.id}
              provider={provider}
              isTesting={testingStates[provider.id]}
              onEdit={handleOpenEditDialog}
              onDelete={handleDelete}
              onSetDefault={handleSetDefault}
              onTestConnection={handleTestConnection}
            />
          ))}
        </div>
      )}

      {/* Add/Edit Dialog */}
      <ProviderFormDialog
        open={dialogOpen}
        onOpenChange={handleCloseDialog}
        provider={editingProvider}
        onSubmit={handleSubmit}
        onTestConnection={editingProvider ? handleDialogTestConnection : undefined}
      />
    </div>
  );
}

export default ProviderSettings;