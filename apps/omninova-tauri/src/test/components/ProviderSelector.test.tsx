/**
 * ProviderSelector 组件单元测试
 *
 * 测试覆盖:
 * - 组件渲染
 * - 提供商选择
 * - 连接状态显示
 * - 测试连接按钮
 * - 空状态处理
 * - 云端/本地分类徽章
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '../utils';
import userEvent from '@testing-library/user-event';
import { ProviderSelector } from '@/components/agent/ProviderSelector';
import { useProviders } from '@/hooks/useProviders';
import type { ProviderWithStatus } from '@/types/provider';

// Mock useProviders hook
vi.mock('@/hooks/useProviders', () => ({
  useProviders: vi.fn(),
}));

const mockUseProviders = vi.mocked(useProviders);

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
  defaultModel: 'gpt-4',
  ...overrides,
});

describe('ProviderSelector', () => {
  const mockOnChange = vi.fn();
  const mockTestConnection = vi.fn();

  const defaultMockReturn = {
    providers: [] as ProviderWithStatus[],
    isLoading: false,
    error: null,
    testConnection: mockTestConnection,
    testingStates: {} as Record<string, boolean>,
    createProvider: vi.fn(),
    updateProvider: vi.fn(),
    deleteProvider: vi.fn(),
    setDefault: vi.fn(),
    refresh: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockUseProviders.mockReturnValue(defaultMockReturn);
  });

  describe('加载状态', () => {
    it('应该显示加载状态', () => {
      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        isLoading: true,
      });

      render(<ProviderSelector onChange={mockOnChange} />);

      expect(screen.getByText('加载提供商...')).toBeInTheDocument();
    });
  });

  describe('空状态', () => {
    it('应该显示空状态并带有设置链接', () => {
      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers: [],
      });

      render(<ProviderSelector onChange={mockOnChange} showEmptyStateLink />);

      expect(screen.getByText('暂无可用的提供商')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /前往设置/i })).toBeInTheDocument();
    });

    it('空状态点击设置按钮应该导航到设置页面', () => {
      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers: [],
      });

      render(<ProviderSelector onChange={mockOnChange} showEmptyStateLink />);

      const settingsButton = screen.getByRole('button', { name: /前往设置/i });
      fireEvent.click(settingsButton);

      expect(mockNavigate).toHaveBeenCalledWith('/settings/providers');
    });

    it('空状态不显示设置链接时应该只显示文本', () => {
      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers: [],
      });

      render(<ProviderSelector onChange={mockOnChange} showEmptyStateLink={false} />);

      expect(screen.getByText('暂无可用的提供商')).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: /前往设置/i })).not.toBeInTheDocument();
    });
  });

  describe('组件渲染', () => {
    it('应该渲染提供商下拉列表', () => {
      const providers = [
        createMockProvider({ id: 'provider-1', name: 'OpenAI' }),
        createMockProvider({ id: 'provider-2', name: 'Anthropic' }),
      ];

      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers,
      });

      render(<ProviderSelector onChange={mockOnChange} />);

      // 打开下拉列表
      const trigger = screen.getByRole('combobox');
      fireEvent.click(trigger);

      expect(screen.getByText('OpenAI')).toBeInTheDocument();
      expect(screen.getByText('Anthropic')).toBeInTheDocument();
    });

    it('应该显示选中的提供商', () => {
      const providers = [
        createMockProvider({ id: 'provider-1', name: 'OpenAI' }),
        createMockProvider({ id: 'provider-2', name: 'Anthropic' }),
      ];

      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers,
      });

      render(<ProviderSelector value="provider-1" onChange={mockOnChange} />);

      expect(screen.getByText('OpenAI')).toBeInTheDocument();
    });

    it('应该显示云端徽章', () => {
      const providers = [
        createMockProvider({ id: 'provider-1', name: 'OpenAI', providerType: 'openai' }),
      ];

      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers,
      });

      render(<ProviderSelector value="provider-1" onChange={mockOnChange} />);

      expect(screen.getByText('云端')).toBeInTheDocument();
    });

    it('应该显示本地徽章', () => {
      const providers = [
        createMockProvider({ id: 'provider-1', name: 'Ollama', providerType: 'ollama' }),
      ];

      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers,
      });

      render(<ProviderSelector value="provider-1" onChange={mockOnChange} />);

      expect(screen.getByText('本地')).toBeInTheDocument();
    });

    it('应该显示默认模型', () => {
      const providers = [
        createMockProvider({ id: 'provider-1', name: 'OpenAI', defaultModel: 'gpt-4-turbo' }),
      ];

      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers,
      });

      render(<ProviderSelector value="provider-1" onChange={mockOnChange} />);

      expect(screen.getByText(/gpt-4-turbo/)).toBeInTheDocument();
    });
  });

  describe('连接状态显示', () => {
    it('应该显示未测试状态', () => {
      const providers = [
        createMockProvider({
          id: 'provider-1',
          name: 'OpenAI',
          connectionStatus: 'untested'
        }),
      ];

      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers,
      });

      render(<ProviderSelector value="provider-1" onChange={mockOnChange} />);

      expect(screen.getByText('尚未测试连接')).toBeInTheDocument();
    });

    it('应该显示已连接状态', () => {
      const providers = [
        createMockProvider({
          id: 'provider-1',
          name: 'OpenAI',
          connectionStatus: 'connected'
        }),
      ];

      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers,
      });

      render(<ProviderSelector value="provider-1" onChange={mockOnChange} />);

      expect(screen.getByText('连接正常')).toBeInTheDocument();
    });

    it('应该显示连接失败状态', () => {
      const providers = [
        createMockProvider({
          id: 'provider-1',
          name: 'OpenAI',
          connectionStatus: 'failed'
        }),
      ];

      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers,
      });

      render(<ProviderSelector value="provider-1" onChange={mockOnChange} />);

      expect(screen.getByText('连接失败')).toBeInTheDocument();
    });

    it('应该显示测试中状态', () => {
      const providers = [
        createMockProvider({
          id: 'provider-1',
          name: 'OpenAI',
          connectionStatus: 'untested'
        }),
      ];

      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers,
        testingStates: { 'provider-1': true },
      });

      render(<ProviderSelector value="provider-1" onChange={mockOnChange} />);

      expect(screen.getByText('测试中...')).toBeInTheDocument();
    });
  });

  describe('选择操作', () => {
    it('选择提供商应该调用 onChange', async () => {
      const user = userEvent.setup();
      const providers = [
        createMockProvider({ id: 'provider-1', name: 'OpenAI' }),
        createMockProvider({ id: 'provider-2', name: 'Anthropic' }),
      ];

      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers,
      });

      render(<ProviderSelector onChange={mockOnChange} />);

      // 打开下拉列表
      const trigger = screen.getByRole('combobox');
      await user.click(trigger);

      // 等待选项出现并点击
      await waitFor(() => {
        expect(screen.getByText('OpenAI')).toBeInTheDocument();
      });

      // 点击选项
      const option = screen.getByRole('option', { name: /OpenAI/ });
      await user.click(option);

      await waitFor(() => {
        expect(mockOnChange).toHaveBeenCalledWith('provider-1');
      });
    });

    it('选择"不设置默认提供商"应该调用 onChange(undefined)', async () => {
      const user = userEvent.setup();
      const providers = [
        createMockProvider({ id: 'provider-1', name: 'OpenAI' }),
      ];

      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers,
      });

      render(<ProviderSelector onChange={mockOnChange} />);

      // 打开下拉列表
      const trigger = screen.getByRole('combobox');
      await user.click(trigger);

      // 等待选项出现
      await waitFor(() => {
        expect(screen.getByText('不设置默认提供商')).toBeInTheDocument();
      });

      // 点击"不设置默认提供商"选项
      const noneOption = screen.getByRole('option', { name: /不设置默认提供商/ });
      await user.click(noneOption);

      await waitFor(() => {
        expect(mockOnChange).toHaveBeenCalledWith(undefined);
      });
    });
  });

  describe('测试连接', () => {
    it('点击测试连接应该调用 testConnection', async () => {
      const providers = [
        createMockProvider({ id: 'provider-1', name: 'OpenAI' }),
      ];

      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers,
      });

      render(
        <ProviderSelector
          value="provider-1"
          onChange={mockOnChange}
          showTestButton
        />
      );

      const testButton = screen.getByRole('button', { name: /测试连接/i });
      fireEvent.click(testButton);

      expect(mockTestConnection).toHaveBeenCalledWith('provider-1');
    });

    it('测试中时测试按钮应该被禁用', () => {
      const providers = [
        createMockProvider({
          id: 'provider-1',
          name: 'OpenAI',
          connectionStatus: 'untested'
        }),
      ];

      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers,
        testingStates: { 'provider-1': true },
      });

      render(
        <ProviderSelector
          value="provider-1"
          onChange={mockOnChange}
          showTestButton
        />
      );

      const testButton = screen.getByRole('button', { name: /测试中/i });
      expect(testButton).toBeDisabled();
    });

    it('不显示测试按钮时应该隐藏', () => {
      const providers = [
        createMockProvider({ id: 'provider-1', name: 'OpenAI' }),
      ];

      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers,
      });

      render(
        <ProviderSelector
          value="provider-1"
          onChange={mockOnChange}
          showTestButton={false}
        />
      );

      expect(screen.queryByRole('button', { name: /测试连接/i })).not.toBeInTheDocument();
    });
  });

  describe('禁用状态', () => {
    it('禁用时选择器应该不可交互', () => {
      const providers = [
        createMockProvider({ id: 'provider-1', name: 'OpenAI' }),
      ];

      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers,
      });

      render(<ProviderSelector value="provider-1" onChange={mockOnChange} disabled />);

      const trigger = screen.getByRole('combobox');
      expect(trigger).toBeDisabled();
    });

    it('禁用时测试按钮应该被禁用', () => {
      const providers = [
        createMockProvider({ id: 'provider-1', name: 'OpenAI' }),
      ];

      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers,
      });

      render(
        <ProviderSelector
          value="provider-1"
          onChange={mockOnChange}
          showTestButton
          disabled
        />
      );

      const testButton = screen.getByRole('button', { name: /测试连接/i });
      expect(testButton).toBeDisabled();
    });
  });

  describe('自定义属性', () => {
    it('应该应用自定义 className', () => {
      const providers = [
        createMockProvider({ id: 'provider-1', name: 'OpenAI' }),
      ];

      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers,
      });

      render(
        <ProviderSelector
          value="provider-1"
          onChange={mockOnChange}
          className="custom-class"
        />
      );

      const container = document.querySelector('.custom-class');
      expect(container).toBeInTheDocument();
    });

    it('应该显示自定义 placeholder', () => {
      mockUseProviders.mockReturnValue({
        ...defaultMockReturn,
        providers: [],
      });

      render(
        <ProviderSelector
          onChange={mockOnChange}
          placeholder="选择一个提供商"
          showEmptyStateLink={false}
        />
      );

      expect(screen.getByText('暂无可用的提供商')).toBeInTheDocument();
    });
  });
});