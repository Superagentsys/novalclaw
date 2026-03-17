/**
 * Provider Card Component
 *
 * Displays a single provider with status, actions, and connection info
 *
 * [Source: Story 3.6 - Provider 配置界面]
 */

import * as React from 'react';
import { useState } from 'react';
import {
  Server,
  Cloud,
  HardDrive,
  Pencil,
  Trash2,
  Star,
  StarOff,
  RefreshCw,
  CheckCircle2,
  XCircle,
  Clock,
  ShieldCheck,
  Shield,
  Loader2,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import type {
  ProviderWithStatus,
  ConnectionStatus,
} from '@/types/provider';

// ============================================================================
// Types
// ============================================================================

interface ProviderCardProps {
  /** Provider data with status */
  provider: ProviderWithStatus;
  /** Is connection test in progress */
  isTesting?: boolean;
  /** Edit callback */
  onEdit: (provider: ProviderWithStatus) => void;
  /** Delete callback */
  onDelete: (id: string) => void;
  /** Set as default callback */
  onSetDefault: (id: string, isDefault: boolean) => void;
  /** Test connection callback */
  onTestConnection: (id: string) => void;
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Get provider icon based on type
 */
function getProviderIcon(providerType: string): React.ReactNode {
  // Local providers
  const localProviders = ['ollama', 'lmstudio', 'llamacpp', 'vllm', 'sglang'];
  if (localProviders.includes(providerType)) {
    return <HardDrive className="h-5 w-5" />;
  }
  return <Cloud className="h-5 w-5" />;
}

/**
 * Get connection status badge
 */
function ConnectionStatusBadge({
  status,
}: {
  status: ConnectionStatus;
}) {
  const statusConfig: Record<
    ConnectionStatus,
    { icon: React.ReactNode; label: string; className: string }
  > = {
    untested: {
      icon: <Clock className="h-3 w-3" />,
      label: '未测试',
      className: 'bg-muted text-muted-foreground',
    },
    testing: {
      icon: <Loader2 className="h-3 w-3 animate-spin" />,
      label: '测试中...',
      className: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400',
    },
    connected: {
      icon: <CheckCircle2 className="h-3 w-3" />,
      label: '已连接',
      className: 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400',
    },
    failed: {
      icon: <XCircle className="h-3 w-3" />,
      label: '连接失败',
      className: 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400',
    },
  };

  const config = statusConfig[status];

  return (
    <div
      className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs ${config.className}`}
    >
      {config.icon}
      <span>{config.label}</span>
    </div>
  );
}

/**
 * Format last tested time
 */
function formatLastTested(timestamp?: number): string {
  if (!timestamp) return '';

  const now = Date.now();
  const diff = now - timestamp;

  if (diff < 60000) return '刚刚';
  if (diff < 3600000) return `${Math.floor(diff / 60000)} 分钟前`;
  if (diff < 86400000) return `${Math.floor(diff / 3600000)} 小时前`;
  return `${Math.floor(diff / 86400000)} 天前`;
}

// ============================================================================
// Component
// ============================================================================

/**
 * Provider Card Component
 *
 * Displays provider information with actions for editing, deleting,
 * testing connection, and setting as default.
 */
export function ProviderCard({
  provider,
  isTesting = false,
  onEdit,
  onDelete,
  onSetDefault,
  onTestConnection,
}: ProviderCardProps): React.ReactElement {
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);

  const handleDelete = () => {
    setShowDeleteConfirm(false);
    onDelete(provider.id);
  };

  const handleToggleDefault = () => {
    onSetDefault(provider.id, !provider.isDefault);
  };

  return (
    <>
      <Card className="border-border/50 transition-colors hover:border-border">
        <CardHeader className="pb-2">
          <div className="flex items-start justify-between">
            <div className="flex items-center gap-3">
              <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-muted">
                {getProviderIcon(provider.providerType)}
              </div>
              <div>
                <CardTitle className="flex items-center gap-2">
                  {provider.name}
                  {provider.isDefault && (
                    <span className="inline-flex items-center gap-1 rounded-full bg-primary/10 px-2 py-0.5 text-xs text-primary">
                      <Star className="h-3 w-3" />
                      默认
                    </span>
                  )}
                </CardTitle>
                <CardDescription className="mt-0.5">
                  {provider.providerType}
                </CardDescription>
              </div>
            </div>

            {/* Connection Status */}
            <ConnectionStatusBadge
              status={isTesting ? 'testing' : provider.connectionStatus}
            />
          </div>
        </CardHeader>

        <CardContent className="space-y-4">
          {/* Provider Details */}
          <div className="grid gap-2 text-sm">
            {provider.baseUrl && (
              <div className="flex items-center gap-2 text-muted-foreground">
                <Server className="h-4 w-4" />
                <code className="text-xs bg-muted px-1.5 py-0.5 rounded">
                  {provider.baseUrl}
                </code>
              </div>
            )}

            {provider.defaultModel && (
              <div className="text-muted-foreground">
                <span className="font-medium">模型：</span>
                {provider.defaultModel}
              </div>
            )}

            {/* Key Status */}
            <div className="flex items-center gap-2">
              {provider.keyExists ? (
                <>
                  <ShieldCheck className="h-4 w-4 text-green-600" />
                  <span className="text-xs text-muted-foreground">
                    API 密钥已存储
                    {provider.storeType === 'os-keyring' && ' (系统密钥链)'}
                    {provider.storeType === 'encrypted-file' && ' (加密文件)'}
                  </span>
                </>
              ) : (
                <>
                  <Shield className="h-4 w-4 text-muted-foreground" />
                  <span className="text-xs text-muted-foreground">
                    未存储 API 密钥
                  </span>
                </>
              )}
            </div>

            {/* Last Tested */}
            {provider.lastTested && (
              <div className="text-xs text-muted-foreground">
                上次测试：{formatLastTested(provider.lastTested)}
              </div>
            )}
          </div>

          {/* Actions */}
          <div className="flex flex-wrap items-center gap-2 pt-2 border-t">
            <Button
              variant="outline"
              size="sm"
              onClick={() => onTestConnection(provider.id)}
              disabled={isTesting}
            >
              {isTesting ? (
                <>
                  <Loader2 className="h-4 w-4 mr-1.5 animate-spin" />
                  测试中
                </>
              ) : (
                <>
                  <RefreshCw className="h-4 w-4 mr-1.5" />
                  测试连接
                </>
              )}
            </Button>

            <Button
              variant="ghost"
              size="sm"
              onClick={handleToggleDefault}
              disabled={provider.isDefault}
            >
              {provider.isDefault ? (
                <>
                  <StarOff className="h-4 w-4 mr-1.5" />
                  默认
                </>
              ) : (
                <>
                  <Star className="h-4 w-4 mr-1.5" />
                  设为默认
                </>
              )}
            </Button>

            <div className="flex-1" />

            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => onEdit(provider)}
            >
              <Pencil className="h-4 w-4" />
            </Button>

            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => setShowDeleteConfirm(true)}
            >
              <Trash2 className="h-4 w-4 text-destructive" />
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Delete Confirmation Dialog */}
      <AlertDialog open={showDeleteConfirm} onOpenChange={setShowDeleteConfirm}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>确认删除提供商</AlertDialogTitle>
            <AlertDialogDescription>
              <div className="space-y-2">
                <p>
                  确定要删除 <strong>{provider.name}</strong> 吗？
                </p>
                <p className="text-destructive">
                  此操作将同时删除存储的 API 密钥，无法恢复。
                </p>
              </div>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDelete}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              确认删除
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

export default ProviderCard;