/**
 * ChannelStatusList 组件测试
 *
 * [Source: Story 6.7 - ChannelStatus 组件与渠道监控]
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ChannelStatusList } from '../ChannelStatusList';
import type { ChannelInfo } from '@/types/channel';

// Mock lucide-react icons
vi.mock('lucide-react', () => ({
  RefreshCw: () => <span data-testid="refresh-icon">RefreshCw</span>,
  Plus: () => <span data-testid="plus-icon">Plus</span>,
  Inbox: () => <span data-testid="inbox-icon">Inbox</span>,
  Wifi: () => <span data-testid="wifi-icon">Wifi</span>,
  WifiOff: () => <span data-testid="wifi-off-icon">WifiOff</span>,
  Loader2: () => <span data-testid="loader-icon">Loader2</span>,
  AlertCircle: () => <span data-testid="alert-icon">AlertCircle</span>,
  ArrowUpRight: () => <span data-testid="arrow-up-icon">ArrowUpRight</span>,
  ArrowDownLeft: () => <span data-testid="arrow-down-icon">ArrowDownLeft</span>,
}));

const mockChannels: ChannelInfo[] = [
  {
    id: 'slack-1',
    name: 'Slack',
    kind: 'slack',
    status: 'connected',
    capabilities: 1,
    messagesSent: 100,
    messagesReceived: 200,
    lastActivity: Math.floor(Date.now() / 1000),
    errorMessage: null,
  },
  {
    id: 'discord-1',
    name: 'Discord',
    kind: 'discord',
    status: 'disconnected',
    capabilities: 1,
    messagesSent: 50,
    messagesReceived: 75,
    lastActivity: null,
    errorMessage: null,
  },
];

describe('ChannelStatusList', () => {
  describe('Rendering', () => {
    it('should render channel cards', () => {
      render(<ChannelStatusList channels={mockChannels} />);

      // Check for channel names in the header area (font-medium class)
      expect(screen.getByText('渠道状态')).toBeInTheDocument();
      // Check that both channels are rendered
      expect(screen.getAllByText('Slack').length).toBeGreaterThan(0);
      expect(screen.getAllByText('Discord').length).toBeGreaterThan(0);
    });

    it('should render header with title', () => {
      render(<ChannelStatusList channels={mockChannels} />);

      expect(screen.getByText('渠道状态')).toBeInTheDocument();
    });
  });

  describe('Loading state', () => {
    it('should show loading skeletons when loading', () => {
      render(<ChannelStatusList channels={[]} isLoading />);

      // Should show skeleton placeholders
      const skeletons = screen.getAllByRole('generic').filter(el =>
        el.className.includes('animate-pulse')
      );
      expect(skeletons.length).toBeGreaterThan(0);
    });

    it('should disable refresh button when loading', () => {
      const onRefresh = vi.fn();
      render(<ChannelStatusList channels={mockChannels} isLoading onRefresh={onRefresh} />);

      const refreshButton = screen.getByRole('button', { name: /刷新/ });
      expect(refreshButton).toBeDisabled();
    });
  });

  describe('Empty state', () => {
    it('should show empty state when no channels', () => {
      render(<ChannelStatusList channels={[]} />);

      expect(screen.getByText('暂无渠道')).toBeInTheDocument();
      expect(screen.getByText('您还没有配置任何通信渠道')).toBeInTheDocument();
    });

    it('should show add button in empty state when onAdd is provided', () => {
      const onAdd = vi.fn();
      render(<ChannelStatusList channels={[]} onAdd={onAdd} />);

      const addButton = screen.getByRole('button', { name: /添加渠道/ });
      expect(addButton).toBeInTheDocument();
    });

    it('should not show add button in empty state when onAdd is not provided', () => {
      render(<ChannelStatusList channels={[]} />);

      expect(screen.queryByRole('button', { name: /添加渠道/ })).not.toBeInTheDocument();
    });
  });

  describe('Action callbacks', () => {
    it('should call onRefresh when refresh button is clicked', () => {
      const onRefresh = vi.fn();
      render(<ChannelStatusList channels={mockChannels} onRefresh={onRefresh} />);

      fireEvent.click(screen.getByRole('button', { name: /刷新/ }));
      expect(onRefresh).toHaveBeenCalled();
    });

    it('should call onAdd when add button is clicked', () => {
      const onAdd = vi.fn();
      render(<ChannelStatusList channels={mockChannels} onAdd={onAdd} />);

      fireEvent.click(screen.getByRole('button', { name: /添加/ }));
      expect(onAdd).toHaveBeenCalled();
    });

    it('should pass callbacks to channel cards', () => {
      const onConnect = vi.fn();
      const onDisconnect = vi.fn();
      const onRetry = vi.fn();

      render(
        <ChannelStatusList
          channels={mockChannels}
          onConnect={onConnect}
          onDisconnect={onDisconnect}
          onRetry={onRetry}
        />
      );

      // Click disconnect on connected channel (Slack)
      fireEvent.click(screen.getByRole('button', { name: /断开/ }));
      expect(onDisconnect).toHaveBeenCalledWith('slack-1');

      // Click connect on disconnected channel (Discord)
      fireEvent.click(screen.getByRole('button', { name: /连接/ }));
      expect(onConnect).toHaveBeenCalledWith('discord-1');
    });
  });

  describe('Operation states', () => {
    it('should pass operation state to channel cards', () => {
      const operationStates = { 'slack-1': true };

      render(
        <ChannelStatusList
          channels={mockChannels}
          operationStates={operationStates}
        />
      );

      // The connected channel (Slack) should have its button disabled
      const disconnectButton = screen.getByRole('button', { name: /断开/ });
      expect(disconnectButton).toBeDisabled();
    });
  });
});