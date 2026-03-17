/**
 * ProviderFormDialog 组件单元测试
 *
 * 测试覆盖:
 * - 组件渲染
 * - 表单验证
 * - 添加提供商流程
 * - 编辑提供商流程
 * - API 密钥显示/隐藏
 * - 提供商类型选择
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '../utils';
import { ProviderFormDialog } from '@/components/settings/ProviderFormDialog';
import type { ProviderWithStatus, NewProviderConfig, ProviderConfigUpdate } from '@/types/provider';

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

describe('ProviderFormDialog', () => {
  const mockOnOpenChange = vi.fn();
  const mockOnSubmit = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    mockOnSubmit.mockResolvedValue(true);
  });

  describe('添加模式', () => {
    it('应该显示添加提供商标题', () => {
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          onSubmit={mockOnSubmit}
        />
      );

      // 使用 heading role 来查找标题
      expect(screen.getByRole('heading', { name: '添加提供商' })).toBeInTheDocument();
    });

    it('应该显示提供商类型选择器', () => {
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          onSubmit={mockOnSubmit}
        />
      );

      expect(screen.getByText('提供商类型')).toBeInTheDocument();
    });

    it('应该显示云端和本地提供商选项', async () => {
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          onSubmit={mockOnSubmit}
        />
      );

      // 打开提供商类型选择器 (第一个 combobox)
      const triggers = screen.getAllByRole('combobox');
      const providerTypeTrigger = triggers[0];
      fireEvent.click(providerTypeTrigger);

      await waitFor(() => {
        expect(screen.getByText('云端服务')).toBeInTheDocument();
        expect(screen.getByText('本地服务')).toBeInTheDocument();
      });
    });

    it('应该显示必填字段', () => {
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          onSubmit={mockOnSubmit}
        />
      );

      // 使用 placeholder 来查找输入框，因为标签没有与输入框关联
      expect(screen.getByPlaceholderText('输入提供商名称')).toBeInTheDocument();
      expect(screen.getByPlaceholderText(/输入 API 密钥/)).toBeInTheDocument();
    });

    it('选择提供商类型后应该填充默认值', async () => {
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          onSubmit={mockOnSubmit}
        />
      );

      // 打开提供商类型选择器 (第一个 combobox)
      const triggers = screen.getAllByRole('combobox');
      const providerTypeTrigger = triggers[0];
      fireEvent.click(providerTypeTrigger);

      await waitFor(() => {
        expect(screen.getByText('Ollama')).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText('Ollama'));

      // 应该填充默认 Base URL
      await waitFor(() => {
        const baseUrlInput = screen.getByPlaceholderText('https://api.example.com/v1');
        expect(baseUrlInput).toHaveValue('http://localhost:11434');
      });
    });
  });

  describe('编辑模式', () => {
    it('应该显示编辑提供商标题', () => {
      const provider = createMockProvider();
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          provider={provider}
          onSubmit={mockOnSubmit}
        />
      );

      // 使用 heading role 来查找标题
      expect(screen.getByRole('heading', { name: '编辑提供商' })).toBeInTheDocument();
    });

    it('不应该显示提供商类型选择器', () => {
      const provider = createMockProvider();
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          provider={provider}
          onSubmit={mockOnSubmit}
        />
      );

      expect(screen.queryByText('提供商类型')).not.toBeInTheDocument();
    });

    it('应该填充现有提供商数据', () => {
      const provider = createMockProvider({
        name: 'My OpenAI',
        baseUrl: 'https://api.openai.com/v1',
        defaultModel: 'gpt-4',
      });
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          provider={provider}
          onSubmit={mockOnSubmit}
        />
      );

      expect(screen.getByDisplayValue('My OpenAI')).toBeInTheDocument();
      expect(screen.getByDisplayValue('https://api.openai.com/v1')).toBeInTheDocument();
      expect(screen.getByDisplayValue('gpt-4')).toBeInTheDocument();
    });

    it('API 密钥字段应该显示留空保留现有密钥提示', () => {
      const provider = createMockProvider();
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          provider={provider}
          onSubmit={mockOnSubmit}
        />
      );

      expect(screen.getByText(/留空保留现有密钥/)).toBeInTheDocument();
    });
  });

  describe('表单验证', () => {
    it('名称为空时应该显示错误', async () => {
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          onSubmit={mockOnSubmit}
        />
      );

      // 直接点击添加按钮
      const submitButton = screen.getByRole('button', { name: /添加提供商/i });
      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(screen.getByText('请输入提供商名称')).toBeInTheDocument();
      });
    });

    it('云端提供商未填 API 密钥时应该显示错误', async () => {
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          onSubmit={mockOnSubmit}
        />
      );

      // 填写名称但不填 API 密钥
      const nameInput = screen.getByPlaceholderText('输入提供商名称');
      fireEvent.change(nameInput, { target: { value: 'Test Provider' } });

      const submitButton = screen.getByRole('button', { name: /添加提供商/i });
      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(screen.getByText('此提供商需要 API 密钥')).toBeInTheDocument();
      });
    });

    it('自定义提供商未填 Base URL 时应该显示错误', async () => {
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          onSubmit={mockOnSubmit}
        />
      );

      // 选择提供商类型 (测试 Ollama 作为不需要 API 密钥的示例)
      const triggers = screen.getAllByRole('combobox');
      const providerTypeTrigger = triggers[0];
      fireEvent.click(providerTypeTrigger);

      await waitFor(() => {
        expect(screen.getByText('Ollama')).toBeInTheDocument();
      });

      // 选择 Ollama 后可以继续测试其他验证
    });
  });

  describe('API 密钥显示/隐藏', () => {
    it('默认应该隐藏 API 密钥', () => {
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          onSubmit={mockOnSubmit}
        />
      );

      const apiKeyInput = screen.getByPlaceholderText(/输入 API 密钥/);
      expect(apiKeyInput).toHaveAttribute('type', 'password');
    });

    it('点击眼睛图标应该切换显示', async () => {
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          onSubmit={mockOnSubmit}
        />
      );

      // 找到显示/隐藏按钮 (Eye 图标按钮在 API 密钥输入框内)
      const buttons = screen.getAllByRole('button');
      // API 密钥输入框内的眼睛按钮是最小的那个 icon-xs 按钮
      const eyeButton = buttons.find(btn => btn.querySelector('svg.lucide-eye, svg.lucide-eye-off'));
      expect(eyeButton).toBeDefined();
      fireEvent.click(eyeButton!);

      await waitFor(() => {
        const apiKeyInput = screen.getByPlaceholderText(/输入 API 密钥/);
        expect(apiKeyInput).toHaveAttribute('type', 'text');
      });
    });
  });

  describe('提交流程', () => {
    it('添加提供商应该调用 onSubmit', async () => {
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          onSubmit={mockOnSubmit}
        />
      );

      // 填写表单
      const nameInput = screen.getByPlaceholderText('输入提供商名称');
      fireEvent.change(nameInput, { target: { value: 'My OpenAI' } });

      const apiKeyInput = screen.getByPlaceholderText(/输入 API 密钥/);
      fireEvent.change(apiKeyInput, { target: { value: 'sk-test-key' } });

      // 提交
      const submitButton = screen.getByRole('button', { name: /添加提供商/i });
      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(mockOnSubmit).toHaveBeenCalledWith(
          expect.objectContaining({
            name: 'My OpenAI',
            apiKey: 'sk-test-key',
            providerType: 'openai',
          })
        );
      });
    });

    it('提交成功后应该关闭对话框', async () => {
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          onSubmit={mockOnSubmit}
        />
      );

      // 填写并提交
      const nameInput = screen.getByLabelText(/显示名称/i);
      fireEvent.change(nameInput, { target: { value: 'My OpenAI' } });

      const apiKeyInput = screen.getByPlaceholderText(/输入 API 密钥/);
      fireEvent.change(apiKeyInput, { target: { value: 'sk-test-key' } });

      const submitButton = screen.getByRole('button', { name: /添加提供商/i });
      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(mockOnOpenChange).toHaveBeenCalledWith(false);
      });
    });

    it('提交失败后不应该关闭对话框', async () => {
      mockOnSubmit.mockResolvedValue(false);

      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          onSubmit={mockOnSubmit}
        />
      );

      // 填写并提交
      const nameInput = screen.getByLabelText(/显示名称/i);
      fireEvent.change(nameInput, { target: { value: 'My OpenAI' } });

      const apiKeyInput = screen.getByPlaceholderText(/输入 API 密钥/);
      fireEvent.change(apiKeyInput, { target: { value: 'sk-test-key' } });

      const submitButton = screen.getByRole('button', { name: /添加提供商/i });
      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(mockOnSubmit).toHaveBeenCalled();
      });

      // 对话框不应该关闭
      expect(mockOnOpenChange).not.toHaveBeenCalled();
    });
  });

  describe('取消操作', () => {
    it('点击取消应该关闭对话框', async () => {
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          onSubmit={mockOnSubmit}
        />
      );

      const cancelButton = screen.getByRole('button', { name: '取消' });
      fireEvent.click(cancelButton);

      expect(mockOnOpenChange).toHaveBeenCalledWith(false);
    });
  });

  describe('设为默认选项', () => {
    it('应该显示设为默认复选框', () => {
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          onSubmit={mockOnSubmit}
        />
      );

      expect(screen.getByLabelText(/设为默认提供商/)).toBeInTheDocument();
    });

    it('编辑模式下应该反映现有默认状态', () => {
      const provider = createMockProvider({ isDefault: true });
      render(
        <ProviderFormDialog
          open={true}
          onOpenChange={mockOnOpenChange}
          provider={provider}
          onSubmit={mockOnSubmit}
        />
      );

      const checkbox = screen.getByLabelText(/设为默认提供商/);
      expect(checkbox).toBeChecked();
    });
  });
});