/**
 * Notification Settings Panel
 *
 * Main component for managing notification preferences.
 *
 * [Source: Story 9.3 - 系统通知管理]
 */

import { useEffect } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Switch } from '@/components/ui/switch';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import { useNotificationStore } from '@/stores/notificationStore';
import { NotificationTypeSelector } from './NotificationTypeSelector';
import { QuietHoursPicker } from './QuietHoursPicker';
import { isInQuietHours } from '@/types/notification';

export function NotificationSettings() {
  const {
    config,
    isLoading,
    error,
    loadConfig,
    setEnabled,
    setSoundEnabled,
    sendTestNotification,
    clearError,
  } = useNotificationStore();

  // Load config on mount
  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  const inQuietHours = isInQuietHours(config);

  return (
    <div className="space-y-6">
      {error && (
        <Card className="border-destructive">
          <CardContent className="pt-6">
            <div className="flex items-center justify-between">
              <p className="text-destructive">{error}</p>
              <button
                onClick={clearError}
                className="text-sm text-muted-foreground hover:text-foreground"
              >
                关闭
              </button>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Main Toggle */}
      <Card>
        <CardHeader>
          <CardTitle>桌面通知</CardTitle>
          <CardDescription>
            控制应用是否发送桌面通知
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label htmlFor="notifications-enabled">启用桌面通知</Label>
              <p className="text-sm text-muted-foreground">
                {inQuietHours && config.enabled && (
                  <span className="text-amber-600">当前处于免打扰时段</span>
                )}
              </p>
            </div>
            <Switch
              id="notifications-enabled"
              checked={config.enabled}
              onCheckedChange={setEnabled}
              disabled={isLoading}
            />
          </div>
        </CardContent>
      </Card>

      {/* Notification Types */}
      {config.enabled && (
        <Card>
          <CardHeader>
            <CardTitle>通知类型</CardTitle>
            <CardDescription>
              选择要接收的通知类型
            </CardDescription>
          </CardHeader>
          <CardContent>
            <NotificationTypeSelector />
          </CardContent>
        </Card>
      )}

      {/* Sound Settings */}
      {config.enabled && (
        <Card>
          <CardHeader>
            <CardTitle>声音设置</CardTitle>
            <CardDescription>
              控制通知声音
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label htmlFor="sound-enabled">启用通知声音</Label>
                <p className="text-sm text-muted-foreground">
                  收到通知时播放提示音
                </p>
              </div>
              <Switch
                id="sound-enabled"
                checked={config.soundEnabled}
                onCheckedChange={setSoundEnabled}
                disabled={isLoading}
              />
            </div>
          </CardContent>
        </Card>
      )}

      {/* Quiet Hours */}
      {config.enabled && (
        <Card>
          <CardHeader>
            <CardTitle>免打扰时段</CardTitle>
            <CardDescription>
              在指定时段内不显示通知
            </CardDescription>
          </CardHeader>
          <CardContent>
            <QuietHoursPicker />
          </CardContent>
        </Card>
      )}

      {/* Test Notification */}
      <Card>
        <CardHeader>
          <CardTitle>测试通知</CardTitle>
          <CardDescription>
            发送一条测试通知以验证设置
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Button
            onClick={sendTestNotification}
            disabled={isLoading || !config.enabled}
            variant="outline"
          >
            发送测试通知
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}

export default NotificationSettings;