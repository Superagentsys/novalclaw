/**
 * ChannelStatusList 组件
 *
 * 显示所有渠道的状态列表
 *
 * [Source: Story 6.7 - ChannelStatus 组件与渠道监控]
 */

import { type FC } from 'react';
import { RefreshCw, Plus, Inbox } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { ChannelStatusCard } from './ChannelStatusCard';
import { type ChannelInfo } from '@/types/channel';

export interface ChannelStatusListProps {
  /** List of channels */
  channels: ChannelInfo[];
  /** Loading state */
  isLoading?: boolean;
  /** Operation in progress states by channel ID */
  operationStates?: Record<string, boolean>;
  /** Callback when refresh button is clicked */
  onRefresh?: () => void;
  /** Callback when add button is clicked */
  onAdd?: () => void;
  /** Callback when connect button is clicked */
  onConnect?: (channelId: string) => void;
  /** Callback when disconnect button is clicked */
  onDisconnect?: (channelId: string) => void;
  /** Callback when retry button is clicked */
  onRetry?: (channelId: string) => void;
}

/**
 * Loading skeleton for channel cards
 */
const ChannelCardSkeleton: FC = () => (
  <div className="rounded-lg border bg-card p-4 animate-pulse">
    <div className="flex items-center justify-between mb-3">
      <div className="flex items-center gap-2">
        <div className="h-4 w-4 bg-muted rounded" />
        <div className="h-5 w-24 bg-muted rounded" />
      </div>
      <div className="h-5 w-16 bg-muted rounded-full" />
    </div>
    <div className="space-y-2">
      <div className="h-4 w-32 bg-muted rounded" />
      <div className="h-4 w-48 bg-muted rounded" />
      <div className="h-4 w-24 bg-muted rounded" />
    </div>
    <div className="mt-4 flex gap-2">
      <div className="h-8 w-20 bg-muted rounded" />
    </div>
  </div>
);

/**
 * Empty state component
 */
const EmptyState: FC<{ onAdd?: () => void }> = ({ onAdd }) => (
  <div className="flex flex-col items-center justify-center py-12 text-center">
    <Inbox className="h-12 w-12 text-muted-foreground mb-4" />
    <h3 className="text-lg font-medium mb-2">暂无渠道</h3>
    <p className="text-muted-foreground mb-4">
      您还没有配置任何通信渠道
    </p>
    {onAdd && (
      <Button onClick={onAdd}>
        <Plus className="h-4 w-4 mr-2" />
        添加渠道
      </Button>
    )}
  </div>
);

/**
 * ChannelStatusList component
 */
export const ChannelStatusList: FC<ChannelStatusListProps> = ({
  channels,
  isLoading = false,
  operationStates = {},
  onRefresh,
  onAdd,
  onConnect,
  onDisconnect,
  onRetry,
}) => {
  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">渠道状态</h2>
        <div className="flex items-center gap-2">
          {onRefresh && (
            <Button
              variant="outline"
              size="sm"
              onClick={onRefresh}
              disabled={isLoading}
            >
              <RefreshCw className={`h-4 w-4 mr-2 ${isLoading ? 'animate-spin' : ''}`} />
              刷新
            </Button>
          )}
          {onAdd && (
            <Button size="sm" onClick={onAdd}>
              <Plus className="h-4 w-4 mr-2" />
              添加
            </Button>
          )}
        </div>
      </div>

      {/* Content */}
      {isLoading ? (
        // Loading skeletons
        <div className="grid gap-4 grid-cols-1 md:grid-cols-2 lg:grid-cols-3">
          {[1, 2, 3].map((i) => (
            <ChannelCardSkeleton key={i} />
          ))}
        </div>
      ) : channels.length === 0 ? (
        // Empty state
        <EmptyState onAdd={onAdd} />
      ) : (
        // Channel grid
        <div className="grid gap-4 grid-cols-1 md:grid-cols-2 lg:grid-cols-3">
          {channels.map((channel) => (
            <ChannelStatusCard
              key={channel.id}
              channel={channel}
              isOperating={operationStates[channel.id] || false}
              onConnect={onConnect}
              onDisconnect={onDisconnect}
              onRetry={onRetry}
            />
          ))}
        </div>
      )}
    </div>
  );
};

export default ChannelStatusList;