/**
 * ChannelSettingsPage 组件
 *
 * 渠道设置页面，集成渠道类型选择器、配置列表和对话框
 *
 * [Source: Story 6.8 - 渠道配置界面]
 */

import { type FC, useState, useCallback, useEffect } from 'react';
import { Plus, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { ChannelTypeSelector } from '@/components/channels/ChannelTypeSelector';
import { ChannelConfigList } from '@/components/channels/ChannelConfigList';
import { ChannelConfigDialog } from '@/components/channels/ChannelConfigDialog';
import {
  type ChannelInfo,
  type ChannelKind,
} from '@/types/channel';
import { useChannelConfig } from '@/hooks/useChannelConfig';
import { useChannels } from '@/hooks/useChannels';

/**
 * ChannelSettingsPage component
 */
export const ChannelSettingsPage: FC = () => {
  // State
  const [showTypeSelector, setShowTypeSelector] = useState(true);
  const [selectedKind, setSelectedKind] = useState<ChannelKind | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editingChannel, setEditingChannel] = useState<ChannelInfo | null>(null);
  const [operationStates, setOperationStates] = useState<Record<string, { loading: boolean; error?: string }>>({});

  // Hooks
  const {
    channels,
    isLoading: isLoadingChannels,
    error: channelsError,
    refresh,
    connect,
    disconnect,
  } = useChannels();

  const {
    deleteChannel,
    error: configError,
  } = useChannelConfig();

  // Compute channel counts by kind
  const channelCounts = channels.reduce<Record<string, number>>((acc, channel) => {
    acc[channel.kind] = (acc[channel.kind] || 0) + 1;
    return acc;
  }, {});

  // Handle channel type selection
  const handleSelectKind = useCallback((kind: ChannelKind) => {
    setSelectedKind(kind);
    setEditingChannel(null);
    setDialogOpen(true);
  }, []);

  // Handle edit channel
  const handleEditChannel = useCallback((channel: ChannelInfo) => {
    setEditingChannel(channel);
    setSelectedKind(null);
    setDialogOpen(true);
  }, []);

  // Handle delete channel
  const handleDeleteChannel = useCallback(async (channelId: string) => {
    setOperationStates((prev) => ({
      ...prev,
      [channelId]: { loading: true },
    }));

    try {
      await deleteChannel(channelId);
      await refresh();
      setOperationStates((prev) => {
        const { [channelId]: _, ...rest } = prev;
        return rest;
      });
    } catch (error) {
      setOperationStates((prev) => ({
        ...prev,
        [channelId]: {
          loading: false,
          error: error instanceof Error ? error.message : '删除失败',
        },
      }));
    }
  }, [deleteChannel, refresh]);

  // Handle connect channel
  const handleConnectChannel = useCallback(async (channelId: string) => {
    setOperationStates((prev) => ({
      ...prev,
      [channelId]: { loading: true },
    }));

    try {
      await connect(channelId);
      setOperationStates((prev) => ({
        ...prev,
        [channelId]: { loading: false },
      }));
    } catch (error) {
      setOperationStates((prev) => ({
        ...prev,
        [channelId]: {
          loading: false,
          error: error instanceof Error ? error.message : '连接失败',
        },
      }));
    }
  }, [connect]);

  // Handle disconnect channel
  const handleDisconnectChannel = useCallback(async (channelId: string) => {
    setOperationStates((prev) => ({
      ...prev,
      [channelId]: { loading: true },
    }));

    try {
      await disconnect(channelId);
      setOperationStates((prev) => ({
        ...prev,
        [channelId]: { loading: false },
      }));
    } catch (error) {
      setOperationStates((prev) => ({
        ...prev,
        [channelId]: {
          loading: false,
          error: error instanceof Error ? error.message : '断开失败',
        },
      }));
    }
  }, [disconnect]);

  // Handle dialog success
  const handleDialogSuccess = useCallback(async () => {
    await refresh();
    setDialogOpen(false);
    setEditingChannel(null);
    setSelectedKind(null);
  }, [refresh]);

  // Handle dialog close
  const handleDialogOpenChange = useCallback((open: boolean) => {
    setDialogOpen(open);
    if (!open) {
      setEditingChannel(null);
      setSelectedKind(null);
    }
  }, []);

  // Handle add new channel
  const handleAddNew = useCallback(() => {
    setShowTypeSelector(true);
  }, []);

  // Initial load
  useEffect(() => {
    refresh();
  }, [refresh]);

  return (
    <div className="container mx-auto py-6 px-4 max-w-4xl">
      <div className="space-y-6">
        {/* Header */}
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-bold">渠道配置</h1>
            <p className="text-muted-foreground">
              配置 AI 代理连接的通信渠道
            </p>
          </div>
          <Button onClick={handleAddNew}>
            <Plus className="h-4 w-4 mr-2" />
            添加渠道
          </Button>
        </div>

        {/* Loading state */}
        {isLoadingChannels && channels.length === 0 && (
          <div className="flex items-center justify-center py-12">
            <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
          </div>
        )}

        {/* Error state */}
        {(channelsError || configError) && (
          <div className="p-4 rounded bg-red-50 dark:bg-red-950/30 text-red-600 dark:text-red-400">
            {channelsError || configError}
          </div>
        )}

        {/* Type selector (when adding new) */}
        {showTypeSelector && channels.length === 0 && (
          <ChannelTypeSelector
            onSelect={handleSelectKind}
            channelCounts={channelCounts}
          />
        )}

        {/* Channel list */}
        {channels.length > 0 && (
          <>
            {/* Quick add buttons */}
            {showTypeSelector && (
              <ChannelTypeSelector
                onSelect={handleSelectKind}
                channelCounts={channelCounts}
              />
            )}

            <div>
              <h2 className="text-lg font-semibold mb-4">已配置渠道</h2>
              <ChannelConfigList
                channels={channels}
                onEdit={handleEditChannel}
                onDelete={handleDeleteChannel}
                onConnect={handleConnectChannel}
                onDisconnect={handleDisconnectChannel}
                operationStates={operationStates}
              />
            </div>
          </>
        )}

        {/* Empty state */}
        {!isLoadingChannels && channels.length === 0 && !showTypeSelector && (
          <div className="text-center py-12">
            <p className="text-muted-foreground mb-4">
              还没有配置任何渠道
            </p>
            <Button onClick={() => setShowTypeSelector(true)}>
              <Plus className="h-4 w-4 mr-2" />
              添加第一个渠道
            </Button>
          </div>
        )}
      </div>

      {/* Configuration dialog */}
      <ChannelConfigDialog
        open={dialogOpen}
        onOpenChange={handleDialogOpenChange}
        channelKind={selectedKind || undefined}
        channel={editingChannel || undefined}
        onSuccess={handleDialogSuccess}
      />
    </div>
  );
};

export default ChannelSettingsPage;