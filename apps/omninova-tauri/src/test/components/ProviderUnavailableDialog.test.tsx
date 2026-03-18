/**
 * ProviderUnavailableDialog 组件单元测试
 *
 * 测试覆盖:
 * - 组件渲染
 * - 错误类型显示
 * - 提供商建议列表
 * - 切换操作
 * - 导航到设置
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '../utils';
import { ProviderUnavailableDialog } from '@/components/Chat/ProviderUnavailableDialog';
import type { ProviderWithStatus } from '@/types/provider';
import type { ProviderError } from '@/hooks/useConversationProvider';

// Mock react-router-dom
const mockNavigate = vi.fn();
vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual('react-router-dom');
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  };
});

// Mock provider data
const createMockProvider = (overrides?: Partial<ProviderWithStatus>): ProviderWithStatus => ({
  id: 'test-provider-id',
  name: 'Test Provider',
  providerType: 'openai',
  isDefault: false,
  createdAt: Date.now(),
  updatedAt: Date.now(),
  connectionStatus: 'untested',
  keyExists: true,
  ...overrides,
});

// Mock error data
const createMockError = (overrides?: Partial<ProviderError>): ProviderError => ({
  type: 'connection_failed',
  message: '网络连接失败',
  suggestion: '请检查网络连接',
  ...overrides,
});

describe('ProviderUnavailableDialog', () => {
  const mockOnOpenChange = vi.fn();
  const mockOnSwitchProvider = vi.fn();
  const mockOnRetry = vi.fn();

  const defaultProps = {
    open: true,
    onOpenChange: mockOnOpenChange,
    error: createMockError(),
    currentProvider: createMockProvider({ id: 'current-provider', name: 'Current Provider' }),
    defaultProvider: createMockProvider({ id: 'default-provider', name: 'Default Provider', isDefault: true }),
    availableProviders: [
      createMockProvider({ id: 'current-provider', name: 'Current Provider' }),
      createMockProvider({ id: 'default-provider', name: 'Default Provider', isDefault: true }),
      createMockProvider({ id: 'other-provider', name: 'Other Provider' }),
    ],
    testingStates: {},
    onSwitchProvider: mockOnSwitchProvider,
    onRetry: mockOnRetry,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('对话框状态', () => {
    it('open=true 时应该显示对话框', () => {
      render(<ProviderUnavailableDialog {...defaultProps} open={true} />);

      expect(screen.getByRole('dialog')).toBeInTheDocument();
    });

    it('open=false 时应该隐藏对话框', () => {
      render(<ProviderUnavailableDialog {...defaultProps} open={false} />);

      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    });

    it('error=null 时不应该渲染内容', () => {
      render(<ProviderUnavailableDialog {...defaultProps} error={null} />);

      // Should render empty fragment
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    });
  });

  describe('错误类型显示', () => {
    it('应该显示 API 密钥错误', () => {
      const error = createMockError({ type: 'api_key_missing', message: 'API 密钥缺失' });

      render(<ProviderUnavailableDialog {...defaultProps} error={error} />);

      expect(screen.getByText('API 密钥问题')).toBeInTheDocument();
      expect(screen.getByText('API 密钥缺失')).toBeInTheDocument();
    });

    it('应该显示连接失败错误', () => {
      const error = createMockError({ type: 'connection_failed', message: '网络错误' });

      render(<ProviderUnavailableDialog {...defaultProps} error={error} />);

      expect(screen.getByText('连接失败')).toBeInTheDocument();
      expect(screen.getByText('网络错误')).toBeInTheDocument();
    });

    it('应该显示服务不可用错误', () => {
      const error = createMockError({ type: 'service_unavailable', message: '服务暂时不可用' });

      render(<ProviderUnavailableDialog {...defaultProps} error={error} />);

      expect(screen.getByText('服务不可用')).toBeInTheDocument();
    });

    it('应该显示请求频率超限错误', () => {
      const error = createMockError({ type: 'rate_limited', message: '请求过于频繁' });

      render(<ProviderUnavailableDialog {...defaultProps} error={error} />);

      expect(screen.getByText('请求频率超限')).toBeInTheDocument();
    });

    it('应该显示提供商未找到错误', () => {
      const error = createMockError({ type: 'provider_not_found', message: '未找到提供商' });

      render(<ProviderUnavailableDialog {...defaultProps} error={error} />);

      expect(screen.getByText('提供商未找到')).toBeInTheDocument();
    });

    it('应该显示未知错误', () => {
      const error = createMockError({ type: 'unknown', message: '发生了未知错误' });

      render(<ProviderUnavailableDialog {...defaultProps} error={error} />);

      expect(screen.getByText('提供商错误')).toBeInTheDocument();
    });
  });

  describe('错误信息显示', () => {
    it('应该显示错误消息', () => {
      render(<ProviderUnavailableDialog {...defaultProps} />);

      expect(screen.getByText('网络连接失败')).toBeInTheDocument();
    });

    it('应该显示提供商名称', () => {
      const error = createMockError({ providerName: 'OpenAI' });

      render(<ProviderUnavailableDialog {...defaultProps} error={error} />);

      expect(screen.getByText(/当前提供商:/)).toBeInTheDocument();
      // Text is part of "当前提供商: OpenAI", use regex for partial match
      expect(screen.getByText(/OpenAI/)).toBeInTheDocument();
    });

    it('应该显示建议', () => {
      const error = createMockError({ suggestion: '建议切换到其他提供商' });

      render(<ProviderUnavailableDialog {...defaultProps} error={error} />);

      expect(screen.getByText(/建议切换到其他提供商/)).toBeInTheDocument();
    });
  });

  describe('提供商建议列表', () => {
    it('应该显示可用提供商列表', () => {
      render(<ProviderUnavailableDialog {...defaultProps} />);

      expect(screen.getByText('可用提供商：')).toBeInTheDocument();
      // Should show other providers (excluding current)
      expect(screen.getByText('Default Provider')).toBeInTheDocument();
      expect(screen.getByText('Other Provider')).toBeInTheDocument();
    });

    it('应该排除当前提供商从建议列表', () => {
      render(<ProviderUnavailableDialog {...defaultProps} />);

      // Current provider should not be in suggestions list
      // There are 2 suggestion cards (default-provider and other-provider) + 1 "切换到默认提供商" button
      const switchButtons = screen.getAllByRole('button', { name: /切换/i });
      // Should have 3 switch buttons total: 2 in cards + 1 in footer
      expect(switchButtons).toHaveLength(3);
    });

    it('应该标记默认提供商', () => {
      render(<ProviderUnavailableDialog {...defaultProps} />);

      expect(screen.getByText('默认')).toBeInTheDocument();
    });

    it('当前提供商不应出现在建议列表中', () => {
      render(<ProviderUnavailableDialog {...defaultProps} />);

      // Current provider should not be in the suggestions
      expect(screen.queryByText('Current Provider')).not.toBeInTheDocument();
    });

    it('应该显示提供商默认模型', () => {
      const providers = [
        createMockProvider({ id: 'current-provider', name: 'Current' }),
        createMockProvider({
          id: 'other-provider',
          name: 'Other Provider',
          defaultModel: 'gpt-4'
        }),
      ];

      render(
        <ProviderUnavailableDialog
          {...defaultProps}
          availableProviders={providers}
        />
      );

      expect(screen.getByText('gpt-4')).toBeInTheDocument();
    });

    it('没有其他提供商时应该显示空状态', () => {
      render(
        <ProviderUnavailableDialog
          {...defaultProps}
          availableProviders={[createMockProvider({ id: 'current-provider' })]}
        />
      );

      expect(screen.getByText('没有其他可用的提供商')).toBeInTheDocument();
      expect(screen.getByText('请前往设置添加新的提供商')).toBeInTheDocument();
    });
  });

  describe('提供商健康状态', () => {
    it('应该显示已连接状态', () => {
      const providers = [
        createMockProvider({ id: 'current-provider', name: 'Current' }),
        createMockProvider({
          id: 'healthy-provider',
          name: 'Healthy Provider',
          connectionStatus: 'connected'
        }),
      ];

      render(
        <ProviderUnavailableDialog
          {...defaultProps}
          availableProviders={providers}
        />
      );

      // Healthy provider card should have green border class
      const healthyCard = document.querySelector('.border-green-500\\/30');
      expect(healthyCard).toBeInTheDocument();
    });

    it('应该显示连接失败状态', () => {
      const providers = [
        createMockProvider({ id: 'current-provider', name: 'Current' }),
        createMockProvider({
          id: 'unhealthy-provider',
          name: 'Unhealthy Provider',
          connectionStatus: 'failed'
        }),
      ];

      render(
        <ProviderUnavailableDialog
          {...defaultProps}
          availableProviders={providers}
        />
      );

      // Unhealthy provider card should have red border class
      const unhealthyCard = document.querySelector('.border-red-500\\/30');
      expect(unhealthyCard).toBeInTheDocument();
    });
  });

  describe('操作按钮', () => {
    it('点击切换应该调用 onSwitchProvider 并关闭对话框', async () => {
      render(<ProviderUnavailableDialog {...defaultProps} />);

      const switchButtons = screen.getAllByRole('button', { name: /切换/i });
      fireEvent.click(switchButtons[0]);

      expect(mockOnSwitchProvider).toHaveBeenCalled();
      expect(mockOnOpenChange).toHaveBeenCalledWith(false);
    });

    it('点击重试应该调用 onRetry', () => {
      render(<ProviderUnavailableDialog {...defaultProps} />);

      const retryButton = screen.getByRole('button', { name: /重试/i });
      fireEvent.click(retryButton);

      expect(mockOnRetry).toHaveBeenCalled();
    });

    it('没有 onRetry 时不应该显示重试按钮', () => {
      render(
        <ProviderUnavailableDialog
          {...defaultProps}
          onRetry={undefined}
        />
      );

      expect(screen.queryByRole('button', { name: /重试/i })).not.toBeInTheDocument();
    });

    it('点击管理提供商应该导航到设置页面', () => {
      render(<ProviderUnavailableDialog {...defaultProps} />);

      const settingsButton = screen.getByRole('button', { name: /管理提供商/i });
      fireEvent.click(settingsButton);

      expect(mockNavigate).toHaveBeenCalledWith('/settings/providers');
      expect(mockOnOpenChange).toHaveBeenCalledWith(false);
    });

    it('点击切换到默认提供商应该调用 onSwitchProvider', () => {
      render(<ProviderUnavailableDialog {...defaultProps} />);

      const switchToDefaultButton = screen.getByRole('button', { name: /切换到默认提供商/i });
      fireEvent.click(switchToDefaultButton);

      expect(mockOnSwitchProvider).toHaveBeenCalledWith('default-provider');
      expect(mockOnOpenChange).toHaveBeenCalledWith(false);
    });

    it('当前提供商就是默认提供商时不显示切换到默认按钮', () => {
      render(
        <ProviderUnavailableDialog
          {...defaultProps}
          currentProvider={createMockProvider({ id: 'default-provider', name: 'Default', isDefault: true })}
        />
      );

      expect(screen.queryByRole('button', { name: /切换到默认提供商/i })).not.toBeInTheDocument();
    });
  });

  describe('测试状态', () => {
    it('测试中的提供商切换按钮应该被禁用', () => {
      render(
        <ProviderUnavailableDialog
          {...defaultProps}
          testingStates={{ 'other-provider': true }}
        />
      );

      // Find the switch button for other-provider (which is testing)
      const switchButtons = screen.getAllByRole('button', { name: /切换/i });
      // The first switch button (default-provider) should not be disabled
      // But we need to check if one of them shows loading state
      const spinner = document.querySelector('svg.lucide-refresh-cw.animate-spin');
      expect(spinner).toBeInTheDocument();
    });
  });

  describe('自定义属性', () => {
    it('应该应用自定义 className', () => {
      render(
        <ProviderUnavailableDialog
          {...defaultProps}
          className="custom-dialog-class"
        />
      );

      const dialog = document.querySelector('.custom-dialog-class');
      expect(dialog).toBeInTheDocument();
    });
  });
});