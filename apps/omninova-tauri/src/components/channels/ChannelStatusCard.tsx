/**
 * ChannelStatusCard 组件
 *
 * 显示单个渠道的连接状态和活动信息
 *
 * [Source: Story 6.7 - ChannelStatus 组件与渠道监控]
 */

import { type FC } from 'react';
import {
  Wifi,
  WifiOff,
  Loader2,
  AlertCircle,
  RefreshCw,
  MessageSquare,
  ArrowUpRight,
  ArrowDownLeft,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Card } from '@/components/ui/card';
import {
  type ChannelInfo,
  type ChannelStatus,
  CHANNEL_STATUS_COLORS,
  CHANNEL_STATUS_LABELS,
  getChannelKindLabel,
  formatTimeAgo,
} from '@/types/channel';

export interface ChannelStatusCardProps {
  /** Channel information */
  channel: ChannelInfo;
  /** Whether an operation is in progress for this channel */
  isOperating?: boolean;
  /** Callback when connect button is clicked */
  onConnect?: (channelId: string) => void;
  /** Callback when disconnect button is clicked */
  onDisconnect?: (channelId: string) => void;
  /** Callback when retry button is clicked */
  onRetry?: (channelId: string) => void;
}

/**
 * Get status icon
 */
function getStatusIcon(status: ChannelStatus, isOperating: boolean) {
  if (isOperating) {
    return <Loader2 className="h-4 w-4 animate-spin" />;
  }

  switch (status) {
    case 'connected':
      return <Wifi className="h-4 w-4" />;
    case 'disconnected':
      return <WifiOff className="h-4 w-4" />;
    case 'connecting':
      return <Loader2 className="h-4 w-4 animate-spin" />;
    case 'error':
      return <AlertCircle className="h-4 w-4" />;
    default:
      return null;
  }
}

/**
 * Get badge variant for status
 */
function getStatusBadgeVariant(status: ChannelStatus): 'success' | 'secondary' | 'warning' | 'error' {
  switch (status) {
    case 'connected':
      return 'success';
    case 'disconnected':
      return 'secondary';
    case 'connecting':
      return 'warning';
    case 'error':
      return 'error';
    default:
      return 'secondary';
  }
}

/**
 * ChannelStatusCard component
 */
export const ChannelStatusCard: FC<ChannelStatusCardProps> = ({
  channel,
  isOperating = false,
  onConnect,
  onDisconnect,
  onRetry,
}) => {
  const { id, name, kind, status, messagesSent, messagesReceived, lastActivity, errorMessage } = channel;

  const statusLabel = CHANNEL_STATUS_LABELS[status];
  const kindLabel = getChannelKindLabel(kind);
  const lastActivityText = formatTimeAgo(lastActivity);

  const handleConnect = () => {
    if (onConnect && !isOperating) {
      onConnect(id);
    }
  };

  const handleDisconnect = () => {
    if (onDisconnect && !isOperating) {
      onDisconnect(id);
    }
  };

  const handleRetry = () => {
    if (onRetry && !isOperating) {
      onRetry(id);
    }
  };

  const colors = CHANNEL_STATUS_COLORS[status];

  return (
    <Card className="p-4">
      {/* Header */}
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <div className={`${colors.text}`}>
            {getStatusIcon(status, isOperating)}
          </div>
          <span className="font-medium">{name}</span>
        </div>
        <Badge variant={getStatusBadgeVariant(status)}>
          {statusLabel}
          {status === 'connecting' && (
            <Loader2 className="ml-1 h-3 w-3 animate-spin" />
          )}
        </Badge>
      </div>

      {/* Info */}
      <div className="space-y-2 text-sm text-muted-foreground">
        <div className="flex items-center gap-2">
          <span>类型:</span>
          <span className="font-medium text-foreground">{kindLabel}</span>
        </div>

        <div className="flex items-center gap-4">
          <div className="flex items-center gap-1.5">
            <ArrowUpRight className="h-3.5 w-3.5 text-green-500" />
            <span>发送 {messagesSent}</span>
          </div>
          <div className="flex items-center gap-1.5">
            <ArrowDownLeft className="h-3.5 w-3.5 text-blue-500" />
            <span>接收 {messagesReceived}</span>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <span>最后活动:</span>
          <span>{lastActivityText}</span>
        </div>
      </div>

      {/* Error message */}
      {status === 'error' && errorMessage && (
        <div className="mt-3 p-2 rounded bg-red-50 dark:bg-red-950/30 text-sm text-red-600 dark:text-red-400">
          {errorMessage}
        </div>
      )}

      {/* Actions */}
      <div className="mt-4 flex items-center gap-2">
        {status === 'disconnected' && (
          <Button
            size="sm"
            variant="default"
            onClick={handleConnect}
            disabled={isOperating}
          >
            {isOperating ? (
              <Loader2 className="h-3.5 w-3.5 mr-1.5 animate-spin" />
            ) : (
              <Wifi className="h-3.5 w-3.5 mr-1.5" />
            )}
            连接
          </Button>
        )}

        {status === 'connected' && (
          <Button
            size="sm"
            variant="outline"
            onClick={handleDisconnect}
            disabled={isOperating}
          >
            {isOperating ? (
              <Loader2 className="h-3.5 w-3.5 mr-1.5 animate-spin" />
            ) : (
              <WifiOff className="h-3.5 w-3.5 mr-1.5" />
            )}
            断开
          </Button>
        )}

        {status === 'error' && (
          <Button
            size="sm"
            variant="destructive"
            onClick={handleRetry}
            disabled={isOperating}
          >
            {isOperating ? (
              <Loader2 className="h-3.5 w-3.5 mr-1.5 animate-spin" />
            ) : (
              <RefreshCw className="h-3.5 w-3.5 mr-1.5" />
            )}
            重试
          </Button>
        )}

        {status === 'connecting' && (
          <Button size="sm" variant="outline" disabled>
            <Loader2 className="h-3.5 w-3.5 mr-1.5 animate-spin" />
            连接中...
          </Button>
        )}
      </div>
    </Card>
  );
};

export default ChannelStatusCard;