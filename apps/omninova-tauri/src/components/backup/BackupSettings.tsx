/**
 * 备份设置组件
 *
 * 提供配置备份和恢复功能:
 * - 导出配置为 JSON/YAML 文件
 * - 导入备份文件恢复配置
 * - 支持完全覆盖和选择性合并
 *
 * [Source: 2-12-config-backup-restore.md]
 */

import * as React from 'react';
import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import { readFile, writeFile, mkdir } from '@tauri-apps/plugin-fs';
import {
  Download,
  Upload,
  FileJson,
  FileCode,
  AlertCircle,
  Loader2,
  RefreshCcw,
  Merge,
  Trash2,
} from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import type {
  BackupMeta,
  BackupFormat,
  ImportMode,
  ImportOptions,
} from '@/types/backup';

// ============================================================================
// 类型定义
// ============================================================================

interface BackupSettingsProps {
  /** 导入完成后的回调 */
  onImportComplete?: () => void;
}

type ImportStep = 'idle' | 'preview' | 'importing';

// ============================================================================
// 子组件
// ============================================================================

/**
 * 格式选择按钮组
 */
function FormatSelector({
  value,
  onChange,
  disabled,
}: {
  value: BackupFormat;
  onChange: (format: BackupFormat) => void;
  disabled?: boolean;
}) {
  return (
    <div className="flex gap-2">
      <Button
        type="button"
        variant={value === 'json' ? 'default' : 'outline'}
        size="sm"
        onClick={() => onChange('json')}
        disabled={disabled}
        className="flex items-center gap-2"
      >
        <FileJson className="h-4 w-4" />
        JSON
      </Button>
      <Button
        type="button"
        variant={value === 'yaml' ? 'default' : 'outline'}
        size="sm"
        onClick={() => onChange('yaml')}
        disabled={disabled}
        className="flex items-center gap-2"
      >
        <FileCode className="h-4 w-4" />
        YAML
      </Button>
    </div>
  );
}

/**
 * 导入模式选择
 */
function ImportModeSelector({
  value,
  onChange,
  disabled,
}: {
  value: ImportMode;
  onChange: (mode: ImportMode) => void;
  disabled?: boolean;
}) {
  return (
    <div className="space-y-3">
      <label className="text-sm font-medium">导入模式</label>
      <div className="space-y-2">
        <button
          type="button"
          onClick={() => onChange('merge')}
          disabled={disabled}
          className={`w-full flex items-start gap-3 p-3 rounded-lg border text-left transition-colors ${
            value === 'merge'
              ? 'border-primary bg-primary/5'
              : 'border-border hover:border-border/80'
          } ${disabled ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer'}`}
        >
          <Merge className="h-5 w-5 mt-0.5 flex-shrink-0" />
          <div>
            <div className="font-medium">选择性合并</div>
            <div className="text-xs text-muted-foreground">
              保留现有配置，备份中的配置将覆盖同名项
            </div>
          </div>
        </button>
        <button
          type="button"
          onClick={() => onChange('overwrite')}
          disabled={disabled}
          className={`w-full flex items-start gap-3 p-3 rounded-lg border text-left transition-colors ${
            value === 'overwrite'
              ? 'border-destructive bg-destructive/5'
              : 'border-border hover:border-border/80'
          } ${disabled ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer'}`}
        >
          <Trash2 className="h-5 w-5 mt-0.5 flex-shrink-0 text-destructive" />
          <div>
            <div className="font-medium text-destructive">完全覆盖</div>
            <div className="text-xs text-muted-foreground">
              清空现有配置，完全使用备份内容
            </div>
          </div>
        </button>
      </div>
    </div>
  );
}

/**
 * 导入内容选择
 */
function ImportContentSelector({
  options,
  onChange,
  disabled,
}: {
  options: ImportOptions;
  onChange: (options: Partial<ImportOptions>) => void;
  disabled?: boolean;
}) {
  const toggleOption = (key: keyof ImportOptions) => {
    if (key === 'mode') return;
    onChange({ [key]: !options[key] });
  };

  const items: { key: keyof ImportOptions; label: string }[] = [
    { key: 'include_agents', label: '代理配置' },
    { key: 'include_providers', label: '提供商配置' },
    { key: 'include_channels', label: '渠道配置' },
    { key: 'include_skills', label: '技能配置' },
    { key: 'include_account', label: '账户设置' },
  ];

  return (
    <div className="space-y-3">
      <label className="text-sm font-medium">选择导入内容</label>
      <div className="space-y-2">
        {items.map(({ key, label }) => (
          <label
            key={key}
            className={`flex items-center gap-2 ${
              disabled ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer'
            }`}
          >
            <input
              type="checkbox"
              checked={options[key] as boolean}
              onChange={() => !disabled && toggleOption(key)}
              disabled={disabled}
              className="h-4 w-4 rounded border-border"
            />
            <span className="text-sm">{label}</span>
          </label>
        ))}
      </div>
    </div>
  );
}

/**
 * 备份预览信息
 */
function BackupPreview({
  meta,
  fileName,
}: {
  meta: BackupMeta;
  fileName: string;
}) {
  const formatDate = (isoString: string) => {
    try {
      return new Date(isoString).toLocaleString('zh-CN');
    } catch {
      return isoString;
    }
  };

  return (
    <div className="space-y-3 p-4 bg-muted/50 rounded-lg">
      <div className="flex items-center gap-2 text-sm">
        <FileJson className="h-4 w-4 text-muted-foreground" />
        <span className="font-medium">{fileName}</span>
      </div>
      <div className="grid grid-cols-2 gap-2 text-sm">
        <div>
          <span className="text-muted-foreground">备份版本：</span>
          <span>{meta.version}</span>
        </div>
        <div>
          <span className="text-muted-foreground">应用版本：</span>
          <span>{meta.app_version}</span>
        </div>
        <div className="col-span-2">
          <span className="text-muted-foreground">创建时间：</span>
          <span>{formatDate(meta.created_at)}</span>
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// 主组件
// ============================================================================

/**
 * 备份设置组件
 *
 * 提供导出和导入配置的用户界面
 */
export function BackupSettings({
  onImportComplete,
}: BackupSettingsProps): React.ReactElement {
  // 导出状态
  const [exportFormat, setExportFormat] = useState<BackupFormat>('json');
  const [isExporting, setIsExporting] = useState(false);

  // 导入状态
  const [importStep, setImportStep] = useState<ImportStep>('idle');
  const [backupContent, setBackupContent] = useState<string>('');
  const [backupMeta, setBackupMeta] = useState<BackupMeta | null>(null);
  const [backupFileName, setBackupFileName] = useState<string>('');
  const [importOptions, setImportOptions] = useState<ImportOptions>({
    mode: 'merge',
    include_agents: true,
    include_providers: true,
    include_channels: true,
    include_skills: true,
    include_account: false,
  });
  const [showConfirmDialog, setShowConfirmDialog] = useState(false);

  // 导出备份
  const handleExport = useCallback(async () => {
    setIsExporting(true);

    try {
      // 调用后端导出
      const content = await invoke<string>('export_config_backup', {
        format: exportFormat,
      });

      // 生成文件名
      const timestamp = new Date()
        .toISOString()
        .replace(/[:.]/g, '-')
        .slice(0, 19);
      const ext = exportFormat === 'yaml' ? 'yaml' : 'json';
      const defaultName = `omninoval-backup-${timestamp}.${ext}`;

      // 打开保存对话框
      const filePath = await save({
        defaultPath: defaultName,
        filters: [
          {
            name: exportFormat.toUpperCase(),
            extensions: [ext],
          },
        ],
      });

      if (!filePath) {
        setIsExporting(false);
        return;
      }

      // 确保目录存在
      const dir = filePath.substring(0, filePath.lastIndexOf('/'));
      try {
        await mkdir(dir, { recursive: true });
      } catch {
        // 目录可能已存在
      }

      // 写入文件
      const encoder = new TextEncoder();
      await writeFile(filePath, encoder.encode(content));

      toast.success('导出成功', {
        description: `配置已保存到 ${filePath}`,
      });
    } catch (error) {
      console.error('Export failed:', error);
      toast.error('导出失败', {
        description: error instanceof Error ? error.message : '未知错误',
      });
    } finally {
      setIsExporting(false);
    }
  }, [exportFormat]);

  // 选择并验证备份文件
  const handleSelectBackup = useCallback(async () => {
    try {
      // 打开文件选择对话框
      const filePath = await open({
        multiple: false,
        filters: [
          { name: 'JSON', extensions: ['json'] },
          { name: 'YAML', extensions: ['yaml', 'yml'] },
        ],
      });

      if (!filePath || typeof filePath !== 'string') {
        return;
      }

      // 读取文件内容
      const content = await readFile(filePath);
      const textContent = new TextDecoder().decode(content);

      // 获取文件名
      const fileName = filePath.substring(filePath.lastIndexOf('/') + 1);

      // 验证备份文件
      const meta = await invoke<BackupMeta>('validate_backup_file', {
        content: textContent,
      });

      // 保存状态
      setBackupContent(textContent);
      setBackupMeta(meta);
      setBackupFileName(fileName);
      setImportStep('preview');
    } catch (error) {
      console.error('Validation failed:', error);
      toast.error('文件无效', {
        description:
          error instanceof Error ? error.message : '备份文件格式无效',
      });
    }
  }, []);

  // 更新导入选项
  const updateImportOptions = useCallback(
    (updates: Partial<ImportOptions>) => {
      setImportOptions((prev) => ({ ...prev, ...updates }));
    },
    []
  );

  // 执行导入
  const handleImport = useCallback(async () => {
    if (!backupContent) return;

    setImportStep('importing');

    try {
      const result = await invoke<string>('import_config_backup', {
        content: backupContent,
        optionsJson: JSON.stringify(importOptions),
      });

      const parsedResult = JSON.parse(result);

      toast.success('导入成功', {
        description: `已导入 ${parsedResult.agents_imported} 个代理${
          parsedResult.account_imported ? '和账户设置' : ''
        }`,
      });

      // 重置状态
      setImportStep('idle');
      setBackupContent('');
      setBackupMeta(null);
      setBackupFileName('');

      // 回调
      onImportComplete?.();
    } catch (error) {
      console.error('Import failed:', error);
      toast.error('导入失败', {
        description: error instanceof Error ? error.message : '未知错误',
      });
      setImportStep('preview');
    } finally {
      setShowConfirmDialog(false);
    }
  }, [backupContent, importOptions, onImportComplete]);

  // 取消导入
  const handleCancelImport = useCallback(() => {
    setImportStep('idle');
    setBackupContent('');
    setBackupMeta(null);
    setBackupFileName('');
  }, []);

  // 确认导入
  const handleConfirmImport = useCallback(() => {
    if (importOptions.mode === 'overwrite') {
      setShowConfirmDialog(true);
    } else {
      handleImport();
    }
  }, [importOptions.mode, handleImport]);

  return (
    <div className="space-y-6">
      {/* 导出配置 */}
      <Card className="border-border/50">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Download className="h-5 w-5" />
            导出配置
          </CardTitle>
          <CardDescription>
            将当前配置导出为备份文件，包含代理、提供商、渠道等设置
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-3">
            <label className="text-sm font-medium">选择导出格式</label>
            <FormatSelector
              value={exportFormat}
              onChange={setExportFormat}
              disabled={isExporting}
            />
          </div>

          <Button
            onClick={handleExport}
            disabled={isExporting}
            className="w-full sm:w-auto"
          >
            {isExporting ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                导出中...
              </>
            ) : (
              <>
                <Download className="h-4 w-4 mr-2" />
                导出配置
              </>
            )}
          </Button>
        </CardContent>
      </Card>

      {/* 导入配置 */}
      <Card className="border-border/50">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Upload className="h-5 w-5" />
            导入配置
          </CardTitle>
          <CardDescription>
            从备份文件恢复配置，支持完全覆盖或选择性合并
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {importStep === 'idle' && (
            <Button
              onClick={handleSelectBackup}
              variant="outline"
              className="w-full sm:w-auto"
            >
              <Upload className="h-4 w-4 mr-2" />
              选择备份文件
            </Button>
          )}

          {importStep === 'preview' && backupMeta && (
            <div className="space-y-4">
              <BackupPreview meta={backupMeta} fileName={backupFileName} />

              <ImportModeSelector
                value={importOptions.mode}
                onChange={(mode) => updateImportOptions({ mode })}
              />

              <ImportContentSelector
                options={importOptions}
                onChange={updateImportOptions}
              />

              <div className="flex gap-2 pt-2">
                <Button onClick={handleConfirmImport}>
                  <RefreshCcw className="h-4 w-4 mr-2" />
                  开始导入
                </Button>
                <Button variant="ghost" onClick={handleCancelImport}>
                  取消
                </Button>
              </div>
            </div>
          )}

          {importStep === 'importing' && (
            <div className="flex items-center justify-center py-8">
              <Loader2 className="h-8 w-8 animate-spin text-primary" />
              <span className="ml-3">正在导入配置...</span>
            </div>
          )}
        </CardContent>
      </Card>

      {/* 警告提示 */}
      <Card className="border-amber-500/30 bg-amber-50 dark:bg-amber-950/20">
        <CardContent className="pt-6">
          <div className="flex items-start gap-3">
            <AlertCircle className="h-5 w-5 text-amber-600 dark:text-amber-400 flex-shrink-0 mt-0.5" />
            <div className="space-y-1">
              <p className="text-sm font-medium text-amber-800 dark:text-amber-200">
                注意事项
              </p>
              <ul className="text-xs text-amber-700 dark:text-amber-300 space-y-1">
                <li>备份文件不包含密码，导入后需要重新设置密码</li>
                <li>备份文件不包含 API 密钥明文，需在新设备重新配置</li>
                <li>建议在导入前先导出当前配置作为备份</li>
              </ul>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* 确认对话框 */}
      <AlertDialog open={showConfirmDialog} onOpenChange={setShowConfirmDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>确认完全覆盖</AlertDialogTitle>
            <AlertDialogDescription>
              此操作将清空现有配置并替换为备份内容，无法恢复。确定要继续吗？
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleImport}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              确认覆盖
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

export default BackupSettings;