/**
 * ProviderCard 组件单元测试
 *
 * 测试覆盖:
 * - 组件渲染
 * - 连接状态显示
 * - 编辑/删除操作
 * - 设为默认
 * - 测试连接
 * - 删除确认对话框
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '../utils';
import { ProviderCard } from '@/components/settings/ProviderCard';
import { invoke } from '@tauri-apps/api/core';
import type { ProviderWithStatus } from '@/types/provider';

// Mock Tauri invoke
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockInvoke = vi.mocked(invoke);

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

describe('ProviderCard', () => {
  const mockOnEdit = vi.fn();
  const mockOnDelete = vi.fn();
  const mockOnSetDefault = vi.fn();
  const mockOnTestConnection = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('组件渲染', () => {
    it('应该渲染提供商名称', () => {
      const provider = createMockProvider();
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      expect(screen.getByText('Test Provider')).toBeInTheDocument();
    });

    it('应该渲染提供商类型', () => {
      const provider = createMockProvider({ providerType: 'anthropic' });
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      expect(screen.getByText('anthropic')).toBeInTheDocument();
    });

    it('应该渲染 Base URL 如果存在', () => {
      const provider = createMockProvider({ baseUrl: 'https://api.openai.com/v1' });
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      expect(screen.getByText('https://api.openai.com/v1')).toBeInTheDocument();
    });

    it('应该渲染默认模型如果存在', () => {
      const provider = createMockProvider({ defaultModel: 'gpt-4' });
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      expect(screen.getByText(/模型：/)).toBeInTheDocument();
      expect(screen.getByText('gpt-4')).toBeInTheDocument();
    });

    it('应该显示默认标签对于默认提供商', () => {
      const provider = createMockProvider({ isDefault: true });
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      // 查找默认标签 (带有 rounded-full 样式的 span)
      const defaultBadge = document.querySelector('.rounded-full.bg-primary\\/10');
      expect(defaultBadge).toHaveTextContent('默认');
    });
  });

  describe('连接状态显示', () => {
    it('应该显示未测试状态', () => {
      const provider = createMockProvider({ connectionStatus: 'untested' });
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      expect(screen.getByText('未测试')).toBeInTheDocument();
    });

    it('应该显示已连接状态', () => {
      const provider = createMockProvider({ connectionStatus: 'connected' });
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      expect(screen.getByText('已连接')).toBeInTheDocument();
    });

    it('应该显示连接失败状态', () => {
      const provider = createMockProvider({ connectionStatus: 'failed' });
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      expect(screen.getByText('连接失败')).toBeInTheDocument();
    });

    it('应该显示测试中状态', () => {
      const provider = createMockProvider({ connectionStatus: 'untested' });
      render(
        <ProviderCard
          provider={provider}
          isTesting={true}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      expect(screen.getByText('测试中...')).toBeInTheDocument();
    });
  });

  describe('API 密钥状态', () => {
    it('应该显示密钥已存储状态', () => {
      const provider = createMockProvider({ keyExists: true });
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      expect(screen.getByText(/API 密钥已存储/)).toBeInTheDocument();
    });

    it('应该显示密钥未存储状态', () => {
      const provider = createMockProvider({ keyExists: false });
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      expect(screen.getByText('未存储 API 密钥')).toBeInTheDocument();
    });

    it('应该显示系统密钥链存储类型', () => {
      const provider = createMockProvider({ keyExists: true, storeType: 'os-keyring' });
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      expect(screen.getByText(/系统密钥链/)).toBeInTheDocument();
    });

    it('应该显示加密文件存储类型', () => {
      const provider = createMockProvider({ keyExists: true, storeType: 'encrypted-file' });
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      expect(screen.getByText(/加密文件/)).toBeInTheDocument();
    });
  });

  describe('操作按钮', () => {
    it('点击测试连接应该调用回调', async () => {
      const provider = createMockProvider();
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      const testButton = screen.getByRole('button', { name: /测试连接/i });
      fireEvent.click(testButton);

      expect(mockOnTestConnection).toHaveBeenCalledWith('test-provider-id');
    });

    it('测试中时测试连接按钮应该被禁用', () => {
      const provider = createMockProvider();
      render(
        <ProviderCard
          provider={provider}
          isTesting={true}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      const testButton = screen.getByRole('button', { name: /测试中/i });
      expect(testButton).toBeDisabled();
    });

    it('点击设为默认应该调用回调', async () => {
      const provider = createMockProvider({ isDefault: false });
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      const defaultButton = screen.getByRole('button', { name: /设为默认/i });
      fireEvent.click(defaultButton);

      expect(mockOnSetDefault).toHaveBeenCalledWith('test-provider-id', true);
    });

    it('默认提供商的设为默认按钮应该被禁用', () => {
      const provider = createMockProvider({ isDefault: true });
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      const defaultButton = screen.getByRole('button', { name: /默认/i });
      expect(defaultButton).toBeDisabled();
    });

    it('点击编辑按钮应该调用回调', async () => {
      const provider = createMockProvider();
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      // 找到编辑按钮 (Pencil 图标)
      const editButtons = screen.getAllByRole('button');
      const editButton = editButtons.find(btn => btn.querySelector('svg.lucide-pencil'));
      expect(editButton).toBeDefined();
      fireEvent.click(editButton!);

      expect(mockOnEdit).toHaveBeenCalledWith(provider);
    });
  });

  describe('删除确认对话框', () => {
    it('点击删除按钮应该显示确认对话框', async () => {
      const provider = createMockProvider();
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      // 找到删除按钮 (Trash2 图标)
      const deleteButtons = screen.getAllByRole('button');
      const deleteButton = deleteButtons.find(btn => btn.querySelector('svg.lucide-trash-2'));
      expect(deleteButton).toBeDefined();
      fireEvent.click(deleteButton!);

      await waitFor(() => {
        expect(screen.getByText('确认删除提供商')).toBeInTheDocument();
        expect(screen.getByText(/确定要删除/)).toBeInTheDocument();
      });
    });

    it('确认删除应该调用删除回调', async () => {
      const provider = createMockProvider();
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      // 打开对话框
      const deleteButtons = screen.getAllByRole('button');
      const deleteButton = deleteButtons.find(btn => btn.querySelector('svg.lucide-trash-2'));
      fireEvent.click(deleteButton!);

      await waitFor(() => {
        expect(screen.getByText('确认删除')).toBeInTheDocument();
      });

      // 确认删除
      const confirmButton = screen.getByRole('button', { name: '确认删除' });
      fireEvent.click(confirmButton);

      expect(mockOnDelete).toHaveBeenCalledWith('test-provider-id');
    });

    it('取消删除应该关闭对话框', async () => {
      const provider = createMockProvider();
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      // 打开对话框
      const deleteButtons = screen.getAllByRole('button');
      const deleteButton = deleteButtons.find(btn => btn.querySelector('svg.lucide-trash-2'));
      fireEvent.click(deleteButton!);

      await waitFor(() => {
        expect(screen.getByText('取消')).toBeInTheDocument();
      });

      // 取消删除
      const cancelButton = screen.getByRole('button', { name: '取消' });
      fireEvent.click(cancelButton);

      await waitFor(() => {
        expect(screen.queryByText('确认删除提供商')).not.toBeInTheDocument();
      });
    });
  });

  describe('提供商图标', () => {
    it('云端提供商应该显示云图标', () => {
      const provider = createMockProvider({ providerType: 'openai' });
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      const cloudIcon = document.querySelector('svg.lucide-cloud');
      expect(cloudIcon).toBeInTheDocument();
    });

    it('本地提供商应该显示硬盘图标', () => {
      const provider = createMockProvider({ providerType: 'ollama' });
      render(
        <ProviderCard
          provider={provider}
          onEdit={mockOnEdit}
          onDelete={mockOnDelete}
          onSetDefault={mockOnSetDefault}
          onTestConnection={mockOnTestConnection}
        />
      );

      const hardDriveIcon = document.querySelector('svg.lucide-hard-drive');
      expect(hardDriveIcon).toBeInTheDocument();
    });
  });
});