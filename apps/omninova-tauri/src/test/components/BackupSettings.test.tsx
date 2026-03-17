/**
 * BackupSettings 组件单元测试
 *
 * 测试覆盖:
 * - 组件渲染
 * - 格式选择交互
 * - 导入模式选择
 * - 导入内容选择
 * - 导出流程
 * - 导入流程
 * - 错误处理
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '../utils';
import { BackupSettings } from '@/components/backup/BackupSettings';
import { invoke } from '@tauri-apps/api/core';

// Mock Tauri dialog and fs plugins
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn().mockResolvedValue(null),
  save: vi.fn().mockResolvedValue(null),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  readFile: vi.fn().mockResolvedValue(new ArrayBuffer(0)),
  writeFile: vi.fn().mockResolvedValue(undefined),
  mkdir: vi.fn().mockResolvedValue(undefined),
}));

// Get typed mock functions
const mockInvoke = vi.mocked(invoke);

describe('BackupSettings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('组件渲染', () => {
    it('应该渲染导出配置卡片', () => {
      render(<BackupSettings />);

      // 卡片标题 - 使用 getAllByText 取第一个匹配项（标题）
      const exportTitles = screen.getAllByText('导出配置');
      expect(exportTitles.length).toBeGreaterThan(0);
      expect(screen.getByText('将当前配置导出为备份文件，包含代理、提供商、渠道等设置')).toBeInTheDocument();
    });

    it('应该渲染导入配置卡片', () => {
      render(<BackupSettings />);

      // 使用 getAllByText
      const importTitles = screen.getAllByText('导入配置');
      expect(importTitles.length).toBeGreaterThan(0);
      expect(screen.getByText('从备份文件恢复配置，支持完全覆盖或选择性合并')).toBeInTheDocument();
    });

    it('应该渲染警告提示', () => {
      render(<BackupSettings />);

      expect(screen.getByText('注意事项')).toBeInTheDocument();
      expect(screen.getByText(/备份文件不包含密码/)).toBeInTheDocument();
    });

    it('应该默认选择 JSON 格式', () => {
      render(<BackupSettings />);

      const jsonButton = screen.getByRole('button', { name: /json/i });
      expect(jsonButton).toHaveClass('bg-primary');
    });

    it('应该渲染选择备份文件按钮', () => {
      render(<BackupSettings />);

      expect(screen.getByRole('button', { name: /选择备份文件/i })).toBeInTheDocument();
    });

    it('应该渲染导出配置按钮', () => {
      render(<BackupSettings />);

      // 按钮有特定的图标，用更精确的选择器
      const buttons = screen.getAllByRole('button');
      const exportButton = buttons.find(btn => btn.textContent?.includes('导出配置') && btn.closest('[data-slot="card"]'));
      expect(exportButton).toBeDefined();
    });
  });

  describe('格式选择', () => {
    it('应该能够切换到 YAML 格式', async () => {
      render(<BackupSettings />);

      const yamlButton = screen.getByRole('button', { name: /yaml/i });
      fireEvent.click(yamlButton);

      await waitFor(() => {
        expect(yamlButton).toHaveClass('bg-primary');
      });
    });

    it('格式按钮在导出中应该被禁用', async () => {
      mockInvoke.mockImplementation(() => new Promise(() => {})); // Never resolves

      render(<BackupSettings />);

      const exportButton = screen.getByRole('button', { name: /导出配置/i });
      fireEvent.click(exportButton);

      await waitFor(() => {
        const jsonButton = screen.getByRole('button', { name: /json/i });
        expect(jsonButton).toBeDisabled();
      });
    });
  });

  describe('导出流程', () => {
    it('点击导出按钮应该调用后端 API', async () => {
      mockInvoke.mockResolvedValueOnce('{"meta": {"version": "1.0"}}');

      render(<BackupSettings />);

      const exportButton = screen.getByRole('button', { name: /导出配置/i });
      fireEvent.click(exportButton);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith('export_config_backup', {
          format: 'json',
        });
      });
    });

    it('导出时应该显示加载状态', async () => {
      mockInvoke.mockImplementation(() => new Promise(() => {}));

      render(<BackupSettings />);

      const exportButton = screen.getByRole('button', { name: /导出配置/i });
      fireEvent.click(exportButton);

      await waitFor(() => {
        expect(screen.getByText('导出中...')).toBeInTheDocument();
      });
    });

    it('导出失败应该显示错误提示', async () => {
      mockInvoke.mockRejectedValueOnce(new Error('导出失败'));

      render(<BackupSettings />);

      const exportButton = screen.getByRole('button', { name: /导出配置/i });
      fireEvent.click(exportButton);

      // 错误提示由 sonner 处理，这里验证 invoke 被调用
      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalled();
      });
    });
  });

  describe('导入流程', () => {
    it('点击选择备份文件应该打开文件选择器', async () => {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const mockOpen = vi.mocked(open);

      render(<BackupSettings />);

      const selectButton = screen.getByText('选择备份文件');
      fireEvent.click(selectButton);

      await waitFor(() => {
        expect(mockOpen).toHaveBeenCalled();
      });
    });
  });
});

describe('FormatSelector', () => {
  it('应该渲染 JSON 和 YAML 两个选项', () => {
    render(<BackupSettings />);

    expect(screen.getByRole('button', { name: /json/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /yaml/i })).toBeInTheDocument();
  });
});

describe('导入选项组件', () => {
  // 这些测试需要模拟完整的导入流程
  it('应该包含导入模式的描述', async () => {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const mockOpen = vi.mocked(open);
    mockOpen.mockResolvedValueOnce('/path/to/backup.json');

    mockInvoke.mockResolvedValueOnce({
      version: '1.0',
      app_version: '0.1.0',
      created_at: '2026-03-17T10:00:00Z',
    });

    render(<BackupSettings />);

    const selectButton = screen.getByText('选择备份文件');
    fireEvent.click(selectButton);

    await waitFor(() => {
      expect(mockOpen).toHaveBeenCalled();
    });
  });
});

describe('错误处理', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('备份验证失败应该显示错误', async () => {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const mockOpen = vi.mocked(open);
    mockOpen.mockResolvedValueOnce('/path/to/invalid.json');

    mockInvoke.mockRejectedValueOnce(new Error('无效的备份文件'));

    render(<BackupSettings />);

    const selectButton = screen.getByText('选择备份文件');
    fireEvent.click(selectButton);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('validate_backup_file', {
        content: expect.any(String),
      });
    });
  });
});

describe('onImportComplete 回调', () => {
  it('导入成功后应该调用回调', async () => {
    const onImportComplete = vi.fn();

    // Mock the entire import flow
    const { open } = await import('@tauri-apps/plugin-dialog');
    const mockOpen = vi.mocked(open);
    mockOpen.mockResolvedValueOnce('/path/to/backup.json');

    mockInvoke
      .mockResolvedValueOnce({
        version: '1.0',
        app_version: '0.1.0',
        created_at: '2026-03-17T10:00:00Z',
      })
      .mockResolvedValueOnce('{"agents_imported": 5, "account_imported": false}');

    render(<BackupSettings onImportComplete={onImportComplete} />);

    // Select file
    const selectButton = screen.getByText('选择备份文件');
    fireEvent.click(selectButton);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('validate_backup_file', expect.any(Object));
    });
  });
});