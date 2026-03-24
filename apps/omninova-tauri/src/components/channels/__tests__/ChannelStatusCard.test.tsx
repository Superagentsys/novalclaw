/**
 * ChannelStatusCard 组件测试
 *
 * [Source: Story 6.7 - ChannelStatus 组件与渠道监控]
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ChannelStatusCard } from '../ChannelStatusCard';
import type { ChannelInfo } from '@/types/channel';

// Mock lucide-react icons
vi.mock('lucide-react', () => ({
  Wifi: () => <span data-testid="wifi-icon">Wifi</span>,
  WifiOff: () => <span data-testid="wifi-off-icon">WifiOff</span>,
  Loader2: () => <span data-testid="loader-icon">Loader2</span>,
  AlertCircle: () => <span data-testid="alert-icon">AlertCircle</span>,
  RefreshCw: () => <span data-testid="refresh-icon">RefreshCw</span>,
  MessageSquare: () => <span data-testid="message-icon">MessageSquare</span>,
  ArrowUpRight: () => <span data-testid="arrow-up-icon">ArrowUpRight</span>,
  ArrowDownLeft: () => <span data-testid="arrow-down-icon">ArrowDownLeft</span>,
}));

const mockConnectedChannel: ChannelInfo = {
  id: 'slack-1',
  name: 'My Slack Channel',
  kind: 'slack',
  status: 'connected',
  capabilities: 1,
  messagesSent: 100,
  messagesReceived: 200,
  lastActivity: Math.floor(Date.now() / 1000) - 120, // 2 minutes ago
  errorMessage: null,
};

const mockDisconnectedChannel: ChannelInfo = {
  id: 'discord-1',
  name: 'Discord Server',
  kind: 'discord',
  status: 'disconnected',
  capabilities: 1,
  messagesSent: 50,
  messagesReceived: 75,
  lastActivity: null,
  errorMessage: null,
};

const mockErrorChannel: ChannelInfo = {
  id: 'telegram-1',
  name: 'Telegram Bot',
  kind: 'telegram',
  status: 'error',
  capabilities: 1,
  messagesSent: 10,
  messagesReceived: 20,
  lastActivity: Math.floor(Date.now() / 1000) - 3600,
  errorMessage: 'Connection refused: invalid token',
};

const mockConnectingChannel: ChannelInfo = {
  id: 'email-1',
  name: 'Email Account',
  kind: 'email',
  status: 'connecting',
  capabilities: 1,
  messagesSent: 0,
  messagesReceived: 0,
  lastActivity: null,
  errorMessage: null,
};

describe('ChannelStatusCard', () => {
  describe('Status display', () => {
    it('should render connected status correctly', () => {
      render(<ChannelStatusCard channel={mockConnectedChannel} />);

      expect(screen.getByText('My Slack Channel')).toBeInTheDocument();
      expect(screen.getByText('已连接')).toBeInTheDocument();
      expect(screen.getByText('Slack')).toBeInTheDocument();
    });

    it('should render disconnected status correctly', () => {
      render(<ChannelStatusCard channel={mockDisconnectedChannel} />);

      expect(screen.getByText('Discord Server')).toBeInTheDocument();
      expect(screen.getByText('已断开')).toBeInTheDocument();
    });

    it('should render error status with error message', () => {
      render(<ChannelStatusCard channel={mockErrorChannel} />);

      expect(screen.getByText('Telegram Bot')).toBeInTheDocument();
      expect(screen.getByText('错误')).toBeInTheDocument();
      expect(screen.getByText('Connection refused: invalid token')).toBeInTheDocument();
    });

    it('should render connecting status with animation', () => {
      render(<ChannelStatusCard channel={mockConnectingChannel} />);

      expect(screen.getByText('Email Account')).toBeInTheDocument();
      expect(screen.getByText('连接中')).toBeInTheDocument();
    });
  });

  describe('Activity stats', () => {
    it('should display message counts', () => {
      render(<ChannelStatusCard channel={mockConnectedChannel} />);

      expect(screen.getByText('发送 100')).toBeInTheDocument();
      expect(screen.getByText('接收 200')).toBeInTheDocument();
    });

    it('should display last activity time', () => {
      render(<ChannelStatusCard channel={mockConnectedChannel} />);

      expect(screen.getByText(/最后活动:/)).toBeInTheDocument();
    });

    it('should show "无" for no last activity', () => {
      render(<ChannelStatusCard channel={mockDisconnectedChannel} />);

      expect(screen.getByText('无')).toBeInTheDocument();
    });
  });

  describe('Action buttons', () => {
    it('should show connect button for disconnected channel', () => {
      render(<ChannelStatusCard channel={mockDisconnectedChannel} />);

      expect(screen.getByRole('button', { name: /连接/ })).toBeInTheDocument();
    });

    it('should show disconnect button for connected channel', () => {
      render(<ChannelStatusCard channel={mockConnectedChannel} />);

      expect(screen.getByRole('button', { name: /断开/ })).toBeInTheDocument();
    });

    it('should show retry button for error channel', () => {
      render(<ChannelStatusCard channel={mockErrorChannel} />);

      expect(screen.getByRole('button', { name: /重试/ })).toBeInTheDocument();
    });

    it('should show disabled button for connecting channel', () => {
      render(<ChannelStatusCard channel={mockConnectingChannel} />);

      const button = screen.getByRole('button', { name: /连接中/ });
      expect(button).toBeDisabled();
    });
  });

  describe('Callbacks', () => {
    it('should call onConnect when connect button is clicked', () => {
      const onConnect = vi.fn();
      render(<ChannelStatusCard channel={mockDisconnectedChannel} onConnect={onConnect} />);

      fireEvent.click(screen.getByRole('button', { name: /连接/ }));
      expect(onConnect).toHaveBeenCalledWith('discord-1');
    });

    it('should call onDisconnect when disconnect button is clicked', () => {
      const onDisconnect = vi.fn();
      render(<ChannelStatusCard channel={mockConnectedChannel} onDisconnect={onDisconnect} />);

      fireEvent.click(screen.getByRole('button', { name: /断开/ }));
      expect(onDisconnect).toHaveBeenCalledWith('slack-1');
    });

    it('should call onRetry when retry button is clicked', () => {
      const onRetry = vi.fn();
      render(<ChannelStatusCard channel={mockErrorChannel} onRetry={onRetry} />);

      fireEvent.click(screen.getByRole('button', { name: /重试/ }));
      expect(onRetry).toHaveBeenCalledWith('telegram-1');
    });
  });

  describe('Operating state', () => {
    it('should disable buttons when operating', () => {
      render(<ChannelStatusCard channel={mockDisconnectedChannel} isOperating />);

      const button = screen.getByRole('button', { name: /连接/ });
      expect(button).toBeDisabled();
    });

    it('should not call callbacks when operating', () => {
      const onConnect = vi.fn();
      render(<ChannelStatusCard channel={mockDisconnectedChannel} isOperating onConnect={onConnect} />);

      fireEvent.click(screen.getByRole('button', { name: /连接/ }));
      expect(onConnect).not.toHaveBeenCalled();
    });
  });
});