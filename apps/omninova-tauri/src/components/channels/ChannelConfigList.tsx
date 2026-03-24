/**
 * ChannelConfigList 组件
 *
 * 显示已配置渠道的列表，支持编辑和删除操作
 *
 * [Source: Story 6.8 - 渠道配置界面]
 */

import { type FC, useState } from 'react';
import {
  MoreVertical,
  Pencil,
  Trash2,
  Wifi,
  WifiOff,
  Loader2,
  AlertCircle,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
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
import {
  type ChannelInfo,
  CHANNEL_STATUS_COLORS,
  CHANNEL_STATUS_LABELS,
  getChannelKindLabel,
} from '@/types/channel';

export interface ChannelConfigListProps {
  /** List of configured channels */
  channels: ChannelInfo[];
  /** Callback when edit is requested */
  onEdit: (channel: ChannelInfo) => void;
  /** Callback when delete is requested */
  onDelete: (channelId: string) => Promise<void>;
  /** Callback when connect is requested */
  onConnect?: (channelId: string) => Promise<void>;
  /** Callback when disconnect is requested */
  onDisconnect?: (channelId: string) => Promise<void>;
  /** Operation states by channel ID */
  operationStates?: Record<string, { loading: boolean; error?: string }>;
}

/**
 * Get status icon
 */
function getStatusIcon(status: ChannelInfo['status']) {
  switch (status) {
    case 'connected':
      return <Wifi className="h-4 w-4 text-green-500" />;
    case 'disconnected':
      return <WifiOff className="h-4 w-4 text-gray-400" />;
    case 'connecting':
      return <Loader2 className="h-4 w-4 text-yellow-500 animate-spin" />;
    case 'error':
      return <AlertCircle className="h-4 w-4 text-red-500" />;
    default:
      return null;
  }
}

/**
 * ChannelConfigCard - Individual channel card with actions
 */
interface ChannelConfigCardProps {
  channel: ChannelInfo;
  onEdit: () => void;
  onDelete: () => void;
  onConnect?: () => void;
  onDisconnect?: () => void;
  isLoading?: boolean;
  error?: string;
}

const ChannelConfigCard: FC<ChannelConfigCardProps> = ({
  channel,
  onEdit,
  onDelete,
  onConnect,
  onDisconnect,
  isLoading,
  error,
}) => {
  const kindLabel = getChannelKindLabel(channel.kind);
  const statusLabel = CHANNEL_STATUS_LABELS[channel.status];
  const statusColors = CHANNEL_STATUS_COLORS[channel.status];

  return (
    <Card className="p-4">
      <div className="flex items-start justify-between">
        {/* Left side: info */}
        <div className="flex items-start gap-3">
          {getStatusIcon(channel.status)}
          <div>
            <div className="flex items-center gap-2">
              <span className="font-medium">{channel.name}</span>
              <Badge variant="outline" className="text-xs">
                {kindLabel}
              </Badge>
            </div>
            <div className="flex items-center gap-2 mt-1">
              <Badge className={statusColors.badge}>
                {statusLabel}
              </Badge>
            </div>
            {error && (
              <p className="text-xs text-red-500 mt-1">{error}</p>
            )}
          </div>
        </div>

        {/* Right side: actions */}
        <div className="flex items-center gap-2">
          {channel.status === 'disconnected' && onConnect && (
            <Button
              size="sm"
              variant="ghost"
              onClick={onConnect}
              disabled={isLoading}
            >
              {isLoading ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Wifi className="h-4 w-4" />
              )}
            </Button>
          )}

          {channel.status === 'connected' && onDisconnect && (
            <Button
              size="sm"
              variant="ghost"
              onClick={onDisconnect}
              disabled={isLoading}
            >
              {isLoading ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <WifiOff className="h-4 w-4" />
              )}
            </Button>
          )}

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button size="sm" variant="ghost" disabled={isLoading}>
                <MoreVertical className="h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onClick={onEdit}>
                <Pencil className="h-4 w-4 mr-2" />
                编辑
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={onDelete}
                className="text-red-600 focus:text-red-600"
              >
                <Trash2 className="h-4 w-4 mr-2" />
                删除
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>

      {/* Activity stats */}
      <div className="mt-3 pt-3 border-t text-sm text-muted-foreground">
        <div className="flex items-center gap-4">
          <span>发送: {channel.messagesSent}</span>
          <span>接收: {channel.messagesReceived}</span>
        </div>
      </div>
    </Card>
  );
};

/**
 * ChannelConfigList component
 */
export const ChannelConfigList: FC<ChannelConfigListProps> = ({
  channels,
  onEdit,
  onDelete,
  onConnect,
  onDisconnect,
  operationStates = {},
}) => {
  const [deleteChannel, setDeleteChannel] = useState<ChannelInfo | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);

  const handleConfirmDelete = async () => {
    if (!deleteChannel) return;

    setIsDeleting(true);
    try {
      await onDelete(deleteChannel.id);
      setDeleteChannel(null);
    } catch (error) {
      console.error('Failed to delete channel:', error);
    } finally {
      setIsDeleting(false);
    }
  };

  if (channels.length === 0) {
    return (
      <div className="text-center py-8 text-muted-foreground">
        <p>暂无已配置的渠道</p>
        <p className="text-sm mt-1">点击上方按钮添加新渠道</p>
      </div>
    );
  }

  return (
    <>
      <div className="space-y-3">
        {channels.map((channel) => {
          const opState = operationStates[channel.id] || {};
          return (
            <ChannelConfigCard
              key={channel.id}
              channel={channel}
              onEdit={() => onEdit(channel)}
              onDelete={() => setDeleteChannel(channel)}
              onConnect={onConnect ? () => onConnect(channel.id) : undefined}
              onDisconnect={onDisconnect ? () => onDisconnect(channel.id) : undefined}
              isLoading={opState.loading}
              error={opState.error}
            />
          );
        })}
      </div>

      {/* Delete confirmation dialog */}
      <AlertDialog
        open={!!deleteChannel}
        onOpenChange={(open) => !open && setDeleteChannel(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>确认删除</AlertDialogTitle>
            <AlertDialogDescription>
              确定要删除渠道 "{deleteChannel?.name}" 吗？此操作无法撤销。
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={isDeleting}>取消</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleConfirmDelete}
              disabled={isDeleting}
              className="bg-red-600 hover:bg-red-700"
            >
              {isDeleting ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  删除中...
                </>
              ) : (
                '删除'
              )}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
};

export default ChannelConfigList;