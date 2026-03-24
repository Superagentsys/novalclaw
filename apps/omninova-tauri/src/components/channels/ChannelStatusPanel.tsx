/**
 * ChannelStatusPanel 组件
 *
 * 渠道状态管理面板，集成 useChannels hook 和 ChannelStatusList
 *
 * [Source: Story 6.7 - ChannelStatus 组件与渠道监控]
 */

import { type FC, useCallback } from 'react';
import { useChannels } from '@/hooks/useChannels';
import { ChannelStatusList } from './ChannelStatusList';

export interface ChannelStatusPanelProps {
  /** Callback when add channel button is clicked */
  onAddChannel?: () => void;
}

/**
 * ChannelStatusPanel component
 */
export const ChannelStatusPanel: FC<ChannelStatusPanelProps> = ({
  onAddChannel,
}) => {
  const {
    channels,
    isLoading,
    error,
    refresh,
    connect,
    disconnect,
    retry,
    operationStates,
  } = useChannels();

  const handleConnect = useCallback(
    async (channelId: string) => {
      await connect(channelId);
    },
    [connect]
  );

  const handleDisconnect = useCallback(
    async (channelId: string) => {
      await disconnect(channelId);
    },
    [disconnect]
  );

  const handleRetry = useCallback(
    async (channelId: string) => {
      await retry(channelId);
    },
    [retry]
  );

  return (
    <div className="space-y-4">
      {error && (
        <div className="p-4 rounded-lg bg-red-50 dark:bg-red-950/30 text-red-600 dark:text-red-400">
          {error}
        </div>
      )}

      <ChannelStatusList
        channels={channels}
        isLoading={isLoading}
        operationStates={operationStates}
        onRefresh={refresh}
        onAdd={onAddChannel}
        onConnect={handleConnect}
        onDisconnect={handleDisconnect}
        onRetry={handleRetry}
      />
    </div>
  );
};

export default ChannelStatusPanel;