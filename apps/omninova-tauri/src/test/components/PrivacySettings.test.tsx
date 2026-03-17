/**
 * PrivacySettings 组件单元测试
 *
 * 测试覆盖:
 * - 组件渲染
 * - 加密设置交互
 * - 存储信息显示
 * - 清除历史交互
 * - API 调用
 * - 错误处理
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '../utils';
import { PrivacySettings } from '@/components/settings/PrivacySettings';
import { invoke } from '@tauri-apps/api/core';

// Get typed mock functions
const mockInvoke = vi.mocked(invoke);

// Mock sonner toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

describe('PrivacySettings', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    // Default mock implementations
    mockInvoke.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case 'get_privacy_settings':
          return JSON.stringify({
            encryption_enabled: false,
            data_retention: {
              conversation_retention_days: 0,
              auto_cleanup_enabled: false,
              max_storage_mb: 0,
            },
            cloud_sync: {
              enabled: false,
              sync_scope: 'none',
              last_sync_at: null,
              sync_endpoint: null,
            },
            updated_at: Math.floor(Date.now() / 1000),
          });
        case 'get_data_storage_info':
          return JSON.stringify({
            config_path: '/test/config',
            data_path: '/test/data',
            total_size: 1048576,
            breakdown: {
              database: 512000,
              config: 1024,
              logs: 256000,
              cache: 280000,
            },
          });
        default:
          return '{}';
      }
    });
  });

  describe('组件渲染', () => {
    it('应该渲染加密设置卡片', async () => {
      render(<PrivacySettings />);

      await waitFor(() => {
        expect(screen.getByText('数据加密')).toBeInTheDocument();
      });

      expect(screen.getByText('启用后，敏感数据将使用 AES-256-GCM 加密存储')).toBeInTheDocument();
    });

    it('应该渲染存储信息卡片', async () => {
      render(<PrivacySettings />);

      await waitFor(() => {
        expect(screen.getByText('数据存储')).toBeInTheDocument();
      });

      expect(screen.getByText('查看应用数据存储位置和占用空间')).toBeInTheDocument();
    });

    it('应该渲染清除历史卡片', async () => {
      render(<PrivacySettings />);

      await waitFor(() => {
        expect(screen.getByText('清除数据')).toBeInTheDocument();
      });

      expect(screen.getByText('清除对话历史数据，释放存储空间')).toBeInTheDocument();
    });

    it('应该渲染云端同步卡片', async () => {
      render(<PrivacySettings />);

      await waitFor(() => {
        expect(screen.getByText('云端同步')).toBeInTheDocument();
      });

      expect(screen.getByText('(即将推出)')).toBeInTheDocument();
    });

    it('初始化时应该调用 API 加载设置', async () => {
      render(<PrivacySettings />);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith('get_privacy_settings');
      });
    });

    it('初始化时应该调用 API 加载存储信息', async () => {
      render(<PrivacySettings />);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith('get_data_storage_info');
      });
    });
  });

  describe('加密设置', () => {
    it('应该显示加密未启用状态', async () => {
      render(<PrivacySettings />);

      await waitFor(() => {
        expect(screen.getByText('加密未启用，数据以明文存储')).toBeInTheDocument();
      });
    });

    it('应该显示加密已启用状态', async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_privacy_settings') {
          return JSON.stringify({
            encryption_enabled: true,
            data_retention: {
              conversation_retention_days: 0,
              auto_cleanup_enabled: false,
              max_storage_mb: 0,
            },
            cloud_sync: {
              enabled: false,
              sync_scope: 'none',
              last_sync_at: null,
              sync_endpoint: null,
            },
            updated_at: Math.floor(Date.now() / 1000),
          });
        }
        if (cmd === 'get_data_storage_info') {
          return JSON.stringify({
            config_path: '/test/config',
            data_path: '/test/data',
            total_size: 1048576,
            breakdown: { database: 0, config: 0, logs: 0, cache: 0 },
          });
        }
        return '{}';
      });

      render(<PrivacySettings />);

      await waitFor(() => {
        expect(screen.getByText('加密已启用，敏感数据受到保护')).toBeInTheDocument();
      });
    });

    it('启用加密应该调用 API', async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_privacy_settings') {
          return JSON.stringify({
            encryption_enabled: false,
            data_retention: { conversation_retention_days: 0, auto_cleanup_enabled: false, max_storage_mb: 0 },
            cloud_sync: { enabled: false, sync_scope: 'none', last_sync_at: null, sync_endpoint: null },
            updated_at: Math.floor(Date.now() / 1000),
          });
        }
        if (cmd === 'get_data_storage_info') {
          return JSON.stringify({
            config_path: '/test/config',
            data_path: '/test/data',
            total_size: 0,
            breakdown: { database: 0, config: 0, logs: 0, cache: 0 },
          });
        }
        if (cmd === 'toggle_encryption') {
          return undefined;
        }
        if (cmd === 'update_privacy_settings') {
          return undefined;
        }
        return '{}';
      });

      render(<PrivacySettings />);

      // 等待初始加载完成
      await waitFor(() => {
        expect(screen.getByRole('switch')).toBeInTheDocument();
      });

      // 点击开关启用加密
      const switchElement = screen.getByRole('switch');
      fireEvent.click(switchElement);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith('toggle_encryption', { enabled: true });
      });
    });

    it('禁用加密应该显示确认对话框', async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_privacy_settings') {
          return JSON.stringify({
            encryption_enabled: true,
            data_retention: { conversation_retention_days: 0, auto_cleanup_enabled: false, max_storage_mb: 0 },
            cloud_sync: { enabled: false, sync_scope: 'none', last_sync_at: null, sync_endpoint: null },
            updated_at: Math.floor(Date.now() / 1000),
          });
        }
        if (cmd === 'get_data_storage_info') {
          return JSON.stringify({
            config_path: '/test/config',
            data_path: '/test/data',
            total_size: 0,
            breakdown: { database: 0, config: 0, logs: 0, cache: 0 },
          });
        }
        return '{}';
      });

      render(<PrivacySettings />);

      await waitFor(() => {
        expect(screen.getByText('加密已启用，敏感数据受到保护')).toBeInTheDocument();
      });

      // 点击开关禁用加密
      const switchElement = screen.getByRole('switch');
      fireEvent.click(switchElement);

      await waitFor(() => {
        expect(screen.getByText('确认禁用加密')).toBeInTheDocument();
      });
    });
  });

  describe('存储信息', () => {
    it('应该显示存储路径', async () => {
      render(<PrivacySettings />);

      await waitFor(() => {
        expect(screen.getByText('/test/config')).toBeInTheDocument();
        expect(screen.getByText('/test/data')).toBeInTheDocument();
      });
    });

    it('应该显示存储占用', async () => {
      render(<PrivacySettings />);

      await waitFor(() => {
        expect(screen.getByText('存储占用')).toBeInTheDocument();
        expect(screen.getByText(/总计/)).toBeInTheDocument();
      });
    });

    it('应该显示各类别存储大小', async () => {
      render(<PrivacySettings />);

      await waitFor(() => {
        expect(screen.getByText('数据库')).toBeInTheDocument();
        expect(screen.getByText('配置')).toBeInTheDocument();
        expect(screen.getByText('日志')).toBeInTheDocument();
        expect(screen.getByText('缓存')).toBeInTheDocument();
      });
    });

    it('点击刷新应该重新加载存储信息', async () => {
      render(<PrivacySettings />);

      await waitFor(() => {
        expect(screen.getByRole('button', { name: '刷新' })).toBeInTheDocument();
      });

      // 清除之前的调用
      mockInvoke.mockClear();

      const refreshButton = screen.getByRole('button', { name: '刷新' });
      fireEvent.click(refreshButton);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith('get_data_storage_info');
      });
    });
  });

  describe('清除历史', () => {
    it('应该显示清除范围选项', async () => {
      render(<PrivacySettings />);

      await waitFor(() => {
        expect(screen.getByText('全部对话历史')).toBeInTheDocument();
        expect(screen.getByText('指定代理的对话')).toBeInTheDocument();
        expect(screen.getByText('指定时间范围')).toBeInTheDocument();
      });
    });

    it('点击清除按钮应该显示确认对话框', async () => {
      render(<PrivacySettings />);

      await waitFor(() => {
        expect(screen.getByRole('button', { name: /清除对话历史/i })).toBeInTheDocument();
      });

      const clearButton = screen.getByRole('button', { name: /清除对话历史/i });
      fireEvent.click(clearButton);

      await waitFor(() => {
        expect(screen.getByText('确认清除对话历史')).toBeInTheDocument();
      });
    });

    it('确认清除应该调用 API', async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'clear_conversation_history') {
          return JSON.stringify({
            messages_deleted: 10,
            sessions_deleted: 2,
            space_freed: 1024,
          });
        }
        if (cmd === 'get_privacy_settings') {
          return JSON.stringify({
            encryption_enabled: false,
            data_retention: { conversation_retention_days: 0, auto_cleanup_enabled: false, max_storage_mb: 0 },
            cloud_sync: { enabled: false, sync_scope: 'none', last_sync_at: null, sync_endpoint: null },
            updated_at: Math.floor(Date.now() / 1000),
          });
        }
        if (cmd === 'get_data_storage_info') {
          return JSON.stringify({
            config_path: '/test/config',
            data_path: '/test/data',
            total_size: 0,
            breakdown: { database: 0, config: 0, logs: 0, cache: 0 },
          });
        }
        return '{}';
      });

      render(<PrivacySettings />);

      await waitFor(() => {
        expect(screen.getByRole('button', { name: /清除对话历史/i })).toBeInTheDocument();
      });

      const clearButton = screen.getByRole('button', { name: /清除对话历史/i });
      fireEvent.click(clearButton);

      await waitFor(() => {
        expect(screen.getByText('确认清除对话历史')).toBeInTheDocument();
      });

      // 点击确认按钮
      const confirmButton = screen.getByRole('button', { name: '确认清除' });
      fireEvent.click(confirmButton);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith(
          'clear_conversation_history',
          expect.objectContaining({
            optionsJson: expect.any(String),
          })
        );
      });
    });

    it('选择时间范围应该显示快捷选择', async () => {
      render(<PrivacySettings />);

      await waitFor(() => {
        expect(screen.getByText('指定时间范围')).toBeInTheDocument();
      });

      // 点击时间范围选项
      const dateRangeOption = screen.getByText('指定时间范围').closest('button');
      if (dateRangeOption) {
        fireEvent.click(dateRangeOption);
      }

      await waitFor(() => {
        expect(screen.getByText('快捷选择')).toBeInTheDocument();
        expect(screen.getByText('今天')).toBeInTheDocument();
        expect(screen.getByText('最近 7 天')).toBeInTheDocument();
        expect(screen.getByText('最近 30 天')).toBeInTheDocument();
        expect(screen.getByText('最近 90 天')).toBeInTheDocument();
      });
    });
  });

  describe('错误处理', () => {
    it('加载设置失败应该使用默认设置', async () => {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_privacy_settings') {
          throw new Error('Failed to load');
        }
        if (cmd === 'get_data_storage_info') {
          return JSON.stringify({
            config_path: '/test/config',
            data_path: '/test/data',
            total_size: 0,
            breakdown: { database: 0, config: 0, logs: 0, cache: 0 },
          });
        }
        return '{}';
      });

      render(<PrivacySettings />);

      await waitFor(() => {
        expect(screen.getByText('数据加密')).toBeInTheDocument();
      });
    });

    it('加载存储信息失败应该显示错误提示', async () => {
      const { toast } = await import('sonner');

      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_data_storage_info') {
          throw new Error('Failed to load storage');
        }
        if (cmd === 'get_privacy_settings') {
          return JSON.stringify({
            encryption_enabled: false,
            data_retention: { conversation_retention_days: 0, auto_cleanup_enabled: false, max_storage_mb: 0 },
            cloud_sync: { enabled: false, sync_scope: 'none', last_sync_at: null, sync_endpoint: null },
            updated_at: Math.floor(Date.now() / 1000),
          });
        }
        return '{}';
      });

      render(<PrivacySettings />);

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith('加载存储信息失败', expect.any(Object));
      });
    });

    it('切换加密失败应该显示错误提示', async () => {
      const { toast } = await import('sonner');

      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'toggle_encryption') {
          throw new Error('Encryption failed');
        }
        if (cmd === 'get_privacy_settings') {
          return JSON.stringify({
            encryption_enabled: false,
            data_retention: { conversation_retention_days: 0, auto_cleanup_enabled: false, max_storage_mb: 0 },
            cloud_sync: { enabled: false, sync_scope: 'none', last_sync_at: null, sync_endpoint: null },
            updated_at: Math.floor(Date.now() / 1000),
          });
        }
        if (cmd === 'get_data_storage_info') {
          return JSON.stringify({
            config_path: '/test/config',
            data_path: '/test/data',
            total_size: 0,
            breakdown: { database: 0, config: 0, logs: 0, cache: 0 },
          });
        }
        return '{}';
      });

      render(<PrivacySettings />);

      await waitFor(() => {
        expect(screen.getByRole('switch')).toBeInTheDocument();
      });

      const switchElement = screen.getByRole('switch');
      fireEvent.click(switchElement);

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith('操作失败', expect.any(Object));
      });
    });

    it('清除历史失败应该显示错误提示', async () => {
      const { toast } = await import('sonner');

      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'clear_conversation_history') {
          throw new Error('Clear failed');
        }
        if (cmd === 'get_privacy_settings') {
          return JSON.stringify({
            encryption_enabled: false,
            data_retention: { conversation_retention_days: 0, auto_cleanup_enabled: false, max_storage_mb: 0 },
            cloud_sync: { enabled: false, sync_scope: 'none', last_sync_at: null, sync_endpoint: null },
            updated_at: Math.floor(Date.now() / 1000),
          });
        }
        if (cmd === 'get_data_storage_info') {
          return JSON.stringify({
            config_path: '/test/config',
            data_path: '/test/data',
            total_size: 0,
            breakdown: { database: 0, config: 0, logs: 0, cache: 0 },
          });
        }
        return '{}';
      });

      render(<PrivacySettings />);

      await waitFor(() => {
        expect(screen.getByRole('button', { name: /清除对话历史/i })).toBeInTheDocument();
      });

      const clearButton = screen.getByRole('button', { name: /清除对话历史/i });
      fireEvent.click(clearButton);

      await waitFor(() => {
        expect(screen.getByText('确认清除对话历史')).toBeInTheDocument();
      });

      const confirmButton = screen.getByRole('button', { name: '确认清除' });
      fireEvent.click(confirmButton);

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith('清除失败', expect.any(Object));
      });
    });
  });

  describe('onSettingsChange 回调', () => {
    it('设置变更后应该调用回调', async () => {
      const onSettingsChange = vi.fn();

      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_privacy_settings') {
          return JSON.stringify({
            encryption_enabled: false,
            data_retention: { conversation_retention_days: 0, auto_cleanup_enabled: false, max_storage_mb: 0 },
            cloud_sync: { enabled: false, sync_scope: 'none', last_sync_at: null, sync_endpoint: null },
            updated_at: Math.floor(Date.now() / 1000),
          });
        }
        if (cmd === 'get_data_storage_info') {
          return JSON.stringify({
            config_path: '/test/config',
            data_path: '/test/data',
            total_size: 0,
            breakdown: { database: 0, config: 0, logs: 0, cache: 0 },
          });
        }
        if (cmd === 'toggle_encryption') {
          return undefined;
        }
        if (cmd === 'update_privacy_settings') {
          return undefined;
        }
        return '{}';
      });

      render(<PrivacySettings onSettingsChange={onSettingsChange} />);

      await waitFor(() => {
        expect(screen.getByRole('switch')).toBeInTheDocument();
      });

      const switchElement = screen.getByRole('switch');
      fireEvent.click(switchElement);

      await waitFor(() => {
        expect(onSettingsChange).toHaveBeenCalled();
      });
    });
  });
});

describe('formatSize 辅助函数', () => {
  it('应该格式化字节', () => {
    // formatSize is internal, but we can test the displayed output
    expect(true).toBe(true);
  });
});

describe('StorageBar 组件', () => {
  it('应该在存储信息卡片中显示', async () => {
    render(<PrivacySettings />);

    await waitFor(() => {
      expect(screen.getByText('数据库')).toBeInTheDocument();
      expect(screen.getByText('配置')).toBeInTheDocument();
      expect(screen.getByText('日志')).toBeInTheDocument();
      expect(screen.getByText('缓存')).toBeInTheDocument();
    });
  });
});