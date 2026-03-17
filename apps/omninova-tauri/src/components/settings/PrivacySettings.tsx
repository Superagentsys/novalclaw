/**
 * 隐私与安全设置组件
 *
 * 提供隐私和安全相关功能:
 * - 本地数据加密开关
 * - 存储信息查看
 * - 对话历史清除
 * - 云端同步设置（预留）
 *
 * [Source: 2-13-data-encryption-privacy.md]
 */

import * as React from 'react';
import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  Shield,
  ShieldCheck,
  HardDrive,
  Trash2,
  Cloud,
  Loader2,
  AlertCircle,
  Database,
  FileText,
  FileCode,
  FolderCog,
  Calendar,
  Users,
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
import { Switch } from '@/components/ui/switch';
import type {
  PrivacySettings,
  StorageInfo,
  ClearOptions,
  ClearResult,
  ClearScope,
} from '@/types/privacy';
import { defaultPrivacySettings, createDateRangeLastDays } from '@/types/privacy';

// ============================================================================
// 类型定义
// ============================================================================

interface PrivacySettingsProps {
  /** 设置变更后的回调 */
  onSettingsChange?: () => void;
}

// ============================================================================
// 辅助函数
// ============================================================================

/**
 * 格式化字节大小为人类可读字符串
 */
function formatSize(bytes: number): string {
  const KB = 1024;
  const MB = KB * 1024;
  const GB = MB * 1024;

  if (bytes >= GB) {
    return `${(bytes / GB).toFixed(2)} GB`;
  } else if (bytes >= MB) {
    return `${(bytes / MB).toFixed(2)} MB`;
  } else if (bytes >= KB) {
    return `${(bytes / KB).toFixed(2)} KB`;
  } else {
    return `${bytes} B`;
  }
}

/**
 * 存储大小进度条
 */
function StorageBar({
  label,
  size,
  total,
  icon: Icon,
  color,
}: {
  label: string;
  size: number;
  total: number;
  icon: React.ElementType;
  color: string;
}) {
  const percentage = total > 0 ? (size / total) * 100 : 0;

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between text-sm">
        <div className="flex items-center gap-2">
          <Icon className="h-4 w-4 text-muted-foreground" />
          <span>{label}</span>
        </div>
        <span className="text-muted-foreground">{formatSize(size)}</span>
      </div>
      <div className="h-2 bg-muted rounded-full overflow-hidden">
        <div
          className={`h-full rounded-full transition-all ${color}`}
          style={{ width: `${Math.min(percentage, 100)}%` }}
        />
      </div>
    </div>
  );
}

// ============================================================================
// 子组件
// ============================================================================

/**
 * 加密设置卡片
 */
function EncryptionCard({
  encryptionEnabled,
  onToggle,
  isLoading,
  disabled,
}: {
  encryptionEnabled: boolean;
  onToggle: (enabled: boolean) => void;
  isLoading: boolean;
  disabled?: boolean;
}) {
  const [showDisableWarning, setShowDisableWarning] = useState(false);

  const handleToggle = (checked: boolean) => {
    if (!checked && encryptionEnabled) {
      // 显示禁用警告
      setShowDisableWarning(true);
    } else {
      onToggle(checked);
    }
  };

  const confirmDisable = () => {
    setShowDisableWarning(false);
    onToggle(false);
  };

  return (
    <>
      <Card className="border-border/50">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            {encryptionEnabled ? (
              <ShieldCheck className="h-5 w-5 text-green-600" />
            ) : (
              <Shield className="h-5 w-5" />
            )}
            数据加密
          </CardTitle>
          <CardDescription>
            启用后，敏感数据将使用 AES-256-GCM 加密存储
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <div className="text-sm font-medium">本地数据加密</div>
              <div className="text-xs text-muted-foreground">
                {encryptionEnabled
                  ? '加密已启用，敏感数据受到保护'
                  : '加密未启用，数据以明文存储'}
              </div>
            </div>
            <Switch
              checked={encryptionEnabled}
              onCheckedChange={handleToggle}
              disabled={isLoading || disabled}
            />
          </div>

          {isLoading && (
            <div className="flex items-center justify-center py-2">
              <Loader2 className="h-4 w-4 animate-spin mr-2" />
              <span className="text-sm text-muted-foreground">
                {encryptionEnabled ? '正在启用加密...' : '正在禁用加密...'}
              </span>
            </div>
          )}
        </CardContent>
      </Card>

      {/* 禁用加密确认对话框 */}
      <AlertDialog open={showDisableWarning} onOpenChange={setShowDisableWarning}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle className="flex items-center gap-2">
              <AlertCircle className="h-5 w-5 text-amber-500" />
              确认禁用加密
            </AlertDialogTitle>
            <AlertDialogDescription>
              禁用加密将解密所有存储的敏感数据。请确保您的环境安全，此操作可能需要一些时间完成。
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction onClick={confirmDisable}>
              确认禁用
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

/**
 * 存储信息卡片
 */
function StorageCard({
  storageInfo,
  isLoading,
  onRefresh,
}: {
  storageInfo: StorageInfo | null;
  isLoading: boolean;
  onRefresh: () => void;
}) {
  return (
    <Card className="border-border/50">
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle className="flex items-center gap-2">
            <HardDrive className="h-5 w-5" />
            数据存储
          </CardTitle>
          <Button variant="ghost" size="sm" onClick={onRefresh} disabled={isLoading}>
            {isLoading ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              '刷新'
            )}
          </Button>
        </div>
        <CardDescription>
          查看应用数据存储位置和占用空间
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {storageInfo ? (
          <>
            {/* 存储路径 */}
            <div className="space-y-1 text-sm">
              <div className="flex items-center gap-2">
                <span className="text-muted-foreground">配置目录：</span>
                <code className="text-xs bg-muted px-1.5 py-0.5 rounded">
                  {storageInfo.config_path}
                </code>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-muted-foreground">数据目录：</span>
                <code className="text-xs bg-muted px-1.5 py-0.5 rounded">
                  {storageInfo.data_path}
                </code>
              </div>
            </div>

            {/* 存储占用 */}
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium">存储占用</span>
                <span className="text-sm text-muted-foreground">
                  总计 {formatSize(storageInfo.total_size)}
                </span>
              </div>

              <div className="space-y-3">
                <StorageBar
                  label="数据库"
                  size={storageInfo.breakdown.database}
                  total={storageInfo.total_size}
                  icon={Database}
                  color="bg-blue-500"
                />
                <StorageBar
                  label="配置"
                  size={storageInfo.breakdown.config}
                  total={storageInfo.total_size}
                  icon={FileText}
                  color="bg-green-500"
                />
                <StorageBar
                  label="日志"
                  size={storageInfo.breakdown.logs}
                  total={storageInfo.total_size}
                  icon={FileCode}
                  color="bg-amber-500"
                />
                <StorageBar
                  label="缓存"
                  size={storageInfo.breakdown.cache}
                  total={storageInfo.total_size}
                  icon={FolderCog}
                  color="bg-purple-500"
                />
              </div>
            </div>
          </>
        ) : (
          <div className="flex items-center justify-center py-8">
            {isLoading ? (
              <>
                <Loader2 className="h-5 w-5 animate-spin mr-2" />
                <span className="text-muted-foreground">正在计算存储信息...</span>
              </>
            ) : (
              <span className="text-muted-foreground">点击刷新查看存储信息</span>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

/**
 * 清除历史范围选择
 */
function ClearScopeSelector({
  value,
  onChange,
  disabled,
}: {
  value: ClearScope;
  onChange: (scope: ClearScope) => void;
  disabled?: boolean;
}) {
  const options: { value: ClearScope; label: string; icon: React.ElementType; description: string }[] = [
    {
      value: 'all',
      label: '全部对话历史',
      icon: Trash2,
      description: '清除所有代理的全部对话历史',
    },
    {
      value: 'specific_agents',
      label: '指定代理的对话',
      icon: Users,
      description: '仅清除选定代理的对话历史',
    },
    {
      value: 'date_range',
      label: '指定时间范围',
      icon: Calendar,
      description: '清除指定时间范围内的对话',
    },
  ];

  return (
    <div className="space-y-2">
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          onClick={() => onChange(option.value)}
          disabled={disabled}
          className={`w-full flex items-start gap-3 p-3 rounded-lg border text-left transition-colors ${
            value === option.value
              ? 'border-primary bg-primary/5'
              : 'border-border hover:border-border/80'
          } ${disabled ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer'}`}
        >
          <option.icon className="h-5 w-5 mt-0.5 flex-shrink-0" />
          <div>
            <div className="font-medium">{option.label}</div>
            <div className="text-xs text-muted-foreground">{option.description}</div>
          </div>
        </button>
      ))}
    </div>
  );
}

/**
 * 时间范围快捷选择
 */
function DateRangePresets({
  onSelect,
  disabled,
}: {
  onSelect: (days: number) => void;
  disabled?: boolean;
}) {
  const presets = [
    { days: 1, label: '今天' },
    { days: 7, label: '最近 7 天' },
    { days: 30, label: '最近 30 天' },
    { days: 90, label: '最近 90 天' },
  ];

  return (
    <div className="flex flex-wrap gap-2">
      {presets.map((preset) => (
        <Button
          key={preset.days}
          variant="outline"
          size="sm"
          onClick={() => onSelect(preset.days)}
          disabled={disabled}
        >
          {preset.label}
        </Button>
      ))}
    </div>
  );
}

/**
 * 清除历史卡片
 */
function ClearHistoryCard({
  onClear,
  isClearing,
  disabled,
}: {
  onClear: (options: ClearOptions) => void;
  isClearing: boolean;
  disabled?: boolean;
}) {
  const [scope, setScope] = useState<ClearScope>('all');
  const [showConfirm, setShowConfirm] = useState(false);

  const handleClear = () => {
    setShowConfirm(true);
  };

  const confirmClear = () => {
    const options: ClearOptions = { scope };

    if (scope === 'date_range') {
      options.date_range = createDateRangeLastDays(30); // 默认最近30天
    }

    onClear(options);
    setShowConfirm(false);
  };

  const getScopeDescription = () => {
    switch (scope) {
      case 'all':
        return '所有代理的全部对话历史';
      case 'specific_agents':
        return '选定代理的对话历史';
      case 'date_range':
        return '指定时间范围内的对话';
    }
  };

  return (
    <>
      <Card className="border-border/50">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Trash2 className="h-5 w-5" />
            清除数据
          </CardTitle>
          <CardDescription>
            清除对话历史数据，释放存储空间
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <ClearScopeSelector
            value={scope}
            onChange={setScope}
            disabled={isClearing || disabled}
          />

          {scope === 'date_range' && (
            <div className="space-y-2">
              <div className="text-sm font-medium">快捷选择</div>
              <DateRangePresets
                onSelect={(days) => {
                  // For MVP, we just show the presets but actual implementation
                  // would update the date range
                  console.log(`Selected ${days} days`);
                }}
                disabled={isClearing || disabled}
              />
            </div>
          )}

          <Button
            variant="destructive"
            onClick={handleClear}
            disabled={isClearing || disabled}
            className="w-full sm:w-auto"
          >
            {isClearing ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                清除中...
              </>
            ) : (
              <>
                <Trash2 className="h-4 w-4 mr-2" />
                清除对话历史
              </>
            )}
          </Button>
        </CardContent>
      </Card>

      {/* 确认对话框 */}
      <AlertDialog open={showConfirm} onOpenChange={setShowConfirm}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle className="flex items-center gap-2">
              <AlertCircle className="h-5 w-5 text-destructive" />
              确认清除对话历史
            </AlertDialogTitle>
            <AlertDialogDescription>
              <div className="space-y-2">
                <p>即将清除：</p>
                <p className="font-medium">{getScopeDescription()}</p>
                <p className="text-destructive">
                  此操作不可恢复，已删除的对话将永久丢失。
                </p>
                <p className="text-muted-foreground">
                  建议先导出配置备份。
                </p>
              </div>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction
              onClick={confirmClear}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              确认清除
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

/**
 * 云端同步卡片（预留）
 */
function CloudSyncCard() {
  return (
    <Card className="border-border/50 opacity-60">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Cloud className="h-5 w-5" />
          云端同步
          <span className="text-xs font-normal text-muted-foreground">(即将推出)</span>
        </CardTitle>
        <CardDescription>
          云端同步功能将在后续版本中提供
        </CardDescription>
      </CardHeader>
      <CardContent>
        <p className="text-sm text-muted-foreground">
          通过云端同步，您可以在多设备间同步您的代理配置和对话历史。
        </p>
      </CardContent>
    </Card>
  );
}

// ============================================================================
// 主组件
// ============================================================================

/**
 * 隐私与安全设置组件
 *
 * 提供加密、存储、清除历史等功能的用户界面
 */
export function PrivacySettings({
  onSettingsChange,
}: PrivacySettingsProps): React.ReactElement {
  // 状态
  const [settings, setSettings] = useState<PrivacySettings>(defaultPrivacySettings());
  const [storageInfo, setStorageInfo] = useState<StorageInfo | null>(null);
  const [isLoadingSettings, setIsLoadingSettings] = useState(true);
  const [isLoadingStorage, setIsLoadingStorage] = useState(false);
  const [isTogglingEncryption, setIsTogglingEncryption] = useState(false);
  const [isClearing, setIsClearing] = useState(false);

  // 加载设置
  const loadSettings = useCallback(async () => {
    setIsLoadingSettings(true);
    try {
      const settingsJson = await invoke<string>('get_privacy_settings');
      const loadedSettings = JSON.parse(settingsJson) as PrivacySettings;
      setSettings(loadedSettings);
    } catch (error) {
      console.error('Failed to load privacy settings:', error);
      // 使用默认设置
      setSettings(defaultPrivacySettings());
    } finally {
      setIsLoadingSettings(false);
    }
  }, []);

  // 加载存储信息
  const loadStorageInfo = useCallback(async () => {
    setIsLoadingStorage(true);
    try {
      const storageJson = await invoke<string>('get_data_storage_info');
      const info = JSON.parse(storageJson) as StorageInfo;
      setStorageInfo(info);
    } catch (error) {
      console.error('Failed to load storage info:', error);
      toast.error('加载存储信息失败', {
        description: error instanceof Error ? error.message : '未知错误',
      });
    } finally {
      setIsLoadingStorage(false);
    }
  }, []);

  // 保存设置
  const saveSettings = useCallback(async (newSettings: PrivacySettings) => {
    try {
      await invoke('update_privacy_settings', {
        settingsJson: JSON.stringify(newSettings),
      });
      setSettings(newSettings);
      onSettingsChange?.();
    } catch (error) {
      console.error('Failed to save privacy settings:', error);
      toast.error('保存设置失败', {
        description: error instanceof Error ? error.message : '未知错误',
      });
    }
  }, [onSettingsChange]);

  // 切换加密
  const handleToggleEncryption = useCallback(async (enabled: boolean) => {
    setIsTogglingEncryption(true);
    try {
      await invoke('toggle_encryption', { enabled });

      // 更新设置
      const newSettings: PrivacySettings = {
        ...settings,
        encryption_enabled: enabled,
        updated_at: Math.floor(Date.now() / 1000),
      };
      await saveSettings(newSettings);

      toast.success(enabled ? '加密已启用' : '加密已禁用', {
        description: enabled
          ? '敏感数据将使用 AES-256-GCM 加密存储'
          : '所有数据已解密',
      });
    } catch (error) {
      console.error('Failed to toggle encryption:', error);
      toast.error('操作失败', {
        description: error instanceof Error ? error.message : '未知错误',
      });
    } finally {
      setIsTogglingEncryption(false);
    }
  }, [settings, saveSettings]);

  // 清除历史
  const handleClearHistory = useCallback(async (options: ClearOptions) => {
    setIsClearing(true);
    try {
      const resultJson = await invoke<string>('clear_conversation_history', {
        optionsJson: JSON.stringify(options),
      });
      const result = JSON.parse(resultJson) as ClearResult;

      toast.success('清除完成', {
        description: `已删除 ${result.messages_deleted} 条消息，释放 ${formatSize(result.space_freed)}`,
      });

      // 刷新存储信息
      await loadStorageInfo();
    } catch (error) {
      console.error('Failed to clear history:', error);
      toast.error('清除失败', {
        description: error instanceof Error ? error.message : '未知错误',
      });
    } finally {
      setIsClearing(false);
    }
  }, [loadStorageInfo]);

  // 初始化
  useEffect(() => {
    loadSettings();
    loadStorageInfo();
  }, [loadSettings, loadStorageInfo]);

  return (
    <div className="space-y-6">
      {/* 加密设置 */}
      <EncryptionCard
        encryptionEnabled={settings.encryption_enabled}
        onToggle={handleToggleEncryption}
        isLoading={isTogglingEncryption}
        disabled={isLoadingSettings}
      />

      {/* 存储信息 */}
      <StorageCard
        storageInfo={storageInfo}
        isLoading={isLoadingStorage}
        onRefresh={loadStorageInfo}
      />

      {/* 清除历史 */}
      <ClearHistoryCard
        onClear={handleClearHistory}
        isClearing={isClearing}
        disabled={isLoadingSettings}
      />

      {/* 云端同步（预留） */}
      <CloudSyncCard />
    </div>
  );
}

export default PrivacySettings;