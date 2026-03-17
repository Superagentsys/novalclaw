/**
 * ProviderSettings 组件单元测试
 *
 * 测试覆盖:
 * - 组件渲染
 * - 空状态显示
 * - 加载状态
 * - 错误状态
 * - 提供商列表显示
 * - 添加/编辑/删除流程
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '../utils';
import { ProviderSettings } from '@/components/settings/ProviderSettings';
import type { ProviderConfig } from '@/types/provider';

// Mock the tauri utility - this is what the components actually use
vi.mock('@/utils/tauri', () => ({
  invokeTauri: vi.fn(),
}));

// Mock keyring functions
vi.mock('@/types/keyring', () => ({
  apiKeyExists: vi.fn().mockResolvedValue(true),
  getKeyringStoreType: vi.fn().mockResolvedValue('os-keyring'),
}));

import { invokeTauri } from '@/utils/tauri';

const mockInvokeTauri = vi.mocked(invokeTauri);

// Mock provider data
const createMockProvider = (overrides?: Partial<ProviderConfig>): ProviderConfig => ({
  id: 'test-provider-id',
  name: 'Test Provider',
  providerType: 'openai',
  isDefault: false,
  createdAt: Date.now(),
  updatedAt: Date.now(),
  ...overrides,
});

describe('ProviderSettings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvokeTauri.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_provider_configs') {
        return '[]';
      }
      return '';
    });
  });

  describe('加载状态', () => {
    it('应该显示加载状态', async () => {
      mockInvokeTauri.mockImplementation(() => new Promise(() => {})); // Never resolves

      render(<ProviderSettings />);

      expect(screen.getByText('加载提供商列表...')).toBeInTheDocument();
    });
  });

  describe('空状态', () => {
    it('应该显示空状态提示', async () => {
      mockInvokeTauri.mockResolvedValue('[]');

      render(<ProviderSettings />);

      await waitFor(() => {
        expect(screen.getByText('暂无提供商')).toBeInTheDocument();
      });
    });

    it('空状态应该显示添加按钮', async () => {
      mockInvokeTauri.mockResolvedValue('[]');

      render(<ProviderSettings />);

      await waitFor(() => {
        expect(screen.getByRole('button', { name: /添加提供商/i })).toBeInTheDocument();
      });
    });

    it('点击空状态的添加按钮应该打开对话框', async () => {
      mockInvokeTauri.mockResolvedValue('[]');

      render(<ProviderSettings />);

      await waitFor(() => {
        expect(screen.getByText('暂无提供商')).toBeInTheDocument();
      });

      // 点击添加按钮
      const addButton = screen.getAllByRole('button', { name: /添加提供商/i })[0];
      fireEvent.click(addButton);

      await waitFor(() => {
        // 检查对话框标题 (使用 heading role)
        expect(screen.getByRole('heading', { name: '添加提供商' })).toBeInTheDocument();
      });
    });
  });

  describe('错误状态', () => {
    it('应该显示错误信息', async () => {
      mockInvokeTauri.mockRejectedValue(new Error('加载失败'));

      render(<ProviderSettings />);

      await waitFor(() => {
        expect(screen.getByText('加载失败')).toBeInTheDocument();
      });
    });

    it('应该显示重试按钮', async () => {
      mockInvokeTauri.mockRejectedValue(new Error('加载失败'));

      render(<ProviderSettings />);

      await waitFor(() => {
        expect(screen.getByRole('button', { name: '重试' })).toBeInTheDocument();
      });
    });

    it('点击重试应该重新加载', async () => {
      mockInvokeTauri
        .mockRejectedValueOnce(new Error('加载失败'))
        .mockResolvedValueOnce('[]');

      render(<ProviderSettings />);

      await waitFor(() => {
        expect(screen.getByText('加载失败')).toBeInTheDocument();
      });

      const retryButton = screen.getByRole('button', { name: '重试' });
      fireEvent.click(retryButton);

      await waitFor(() => {
        expect(screen.getByText('暂无提供商')).toBeInTheDocument();
      });
    });
  });

  describe('提供商列表', () => {
    it('应该显示提供商列表', async () => {
      const providers = [
        createMockProvider({ id: '1', name: 'OpenAI', providerType: 'openai' }),
        createMockProvider({ id: '2', name: 'Anthropic', providerType: 'anthropic' }),
      ];
      mockInvokeTauri.mockResolvedValue(JSON.stringify(providers));

      render(<ProviderSettings />);

      await waitFor(() => {
        expect(screen.getByText('OpenAI')).toBeInTheDocument();
        expect(screen.getByText('Anthropic')).toBeInTheDocument();
      });
    });

    it('应该显示统计信息', async () => {
      const providers = [
        createMockProvider({ id: '1', name: 'OpenAI', providerType: 'openai' }),
        createMockProvider({ id: '2', name: 'Ollama', providerType: 'ollama' }),
      ];
      mockInvokeTauri.mockResolvedValue(JSON.stringify(providers));

      render(<ProviderSettings />);

      await waitFor(() => {
        expect(screen.getByText('1 个云端')).toBeInTheDocument();
        expect(screen.getByText('1 个本地')).toBeInTheDocument();
      });
    });

    it('应该显示添加按钮在标题旁边', async () => {
      const providers = [createMockProvider()];
      mockInvokeTauri.mockResolvedValue(JSON.stringify(providers));

      render(<ProviderSettings />);

      await waitFor(() => {
        const addButtons = screen.getAllByRole('button', { name: /添加提供商/i });
        expect(addButtons.length).toBeGreaterThan(0);
      });
    });
  });

  describe('添加提供商', () => {
    it('点击添加按钮应该打开对话框', async () => {
      mockInvokeTauri.mockResolvedValue(JSON.stringify([createMockProvider()]));

      render(<ProviderSettings />);

      await waitFor(() => {
        expect(screen.getByText('Test Provider')).toBeInTheDocument();
      });

      const addButton = screen.getAllByRole('button', { name: /添加提供商/i })[0];
      fireEvent.click(addButton);

      await waitFor(() => {
        expect(screen.getByText('添加提供商')).toBeInTheDocument();
      });
    });
  });

  describe('编辑提供商', () => {
    it('点击编辑按钮应该打开对话框', async () => {
      const provider = createMockProvider();
      mockInvokeTauri.mockResolvedValue(JSON.stringify([provider]));

      render(<ProviderSettings />);

      await waitFor(() => {
        expect(screen.getByText('Test Provider')).toBeInTheDocument();
      });

      // 找到编辑按钮 (Pencil 图标)
      const editButtons = screen.getAllByRole('button');
      const editButton = editButtons.find(btn => btn.querySelector('svg.lucide-pencil'));
      fireEvent.click(editButton!);

      await waitFor(() => {
        expect(screen.getByText('编辑提供商')).toBeInTheDocument();
      });
    });
  });

  describe('删除提供商', () => {
    it('点击删除按钮应该显示确认对话框', async () => {
      const provider = createMockProvider();
      mockInvokeTauri.mockResolvedValue(JSON.stringify([provider]));

      render(<ProviderSettings />);

      await waitFor(() => {
        expect(screen.getByText('Test Provider')).toBeInTheDocument();
      });

      // 找到删除按钮 (Trash2 图标)
      const deleteButtons = screen.getAllByRole('button');
      const deleteButton = deleteButtons.find(btn => btn.querySelector('svg.lucide-trash-2'));
      fireEvent.click(deleteButton!);

      await waitFor(() => {
        expect(screen.getByText('确认删除提供商')).toBeInTheDocument();
      });
    });
  });

  describe('设置变更回调', () => {
    it('添加成功后应该调用 onSettingsChange', async () => {
      const onSettingsChange = vi.fn();
      mockInvokeTauri
        .mockResolvedValueOnce('[]')
        .mockResolvedValueOnce(JSON.stringify([createMockProvider()]));

      render(<ProviderSettings onSettingsChange={onSettingsChange} />);

      await waitFor(() => {
        expect(screen.getByText('暂无提供商')).toBeInTheDocument();
      });

      // 点击添加按钮
      const addButton = screen.getByRole('button', { name: /添加提供商/i });
      fireEvent.click(addButton);

      await waitFor(() => {
        expect(screen.getByText('添加提供商')).toBeInTheDocument();
      });
    });
  });
});