/**
 * Quiet Hours Picker
 *
 * Component for selecting quiet hours time range.
 *
 * [Source: Story 9.3 - 系统通知管理]
 */

import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { useNotificationStore } from '@/stores/notificationStore';

const HOUR_OPTIONS = Array.from({ length: 24 }, (_, i) => ({
  value: i.toString(),
  label: `${i.toString().padStart(2, '0')}:00`,
}));

export function QuietHoursPicker() {
  const { config, setQuietHours, clearQuietHours, isLoading } = useNotificationStore();

  const hasQuietHours =
    config.quietHoursStart !== undefined && config.quietHoursEnd !== undefined;

  const handleToggle = (enabled: boolean) => {
    if (enabled) {
      setQuietHours(22, 8); // Default: 22:00 - 08:00
    } else {
      clearQuietHours();
    }
  };

  return (
    <div className="space-y-4">
      {/* Toggle */}
      <div className="flex items-center justify-between">
        <div className="space-y-0.5">
          <Label htmlFor="quiet-hours-enabled">启用免打扰时段</Label>
          <p className="text-sm text-muted-foreground">
            在指定时段内静音所有通知
          </p>
        </div>
        <Switch
          id="quiet-hours-enabled"
          checked={hasQuietHours}
          onCheckedChange={handleToggle}
          disabled={isLoading}
        />
      </div>

      {/* Time Selectors */}
      {hasQuietHours && (
        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-2">
            <Label htmlFor="quiet-hours-start">开始时间</Label>
            <Select
              value={config.quietHoursStart?.toString() || '22'}
              onValueChange={(value) =>
                setQuietHours(parseInt(value, 10), config.quietHoursEnd || 8)
              }
              disabled={isLoading}
            >
              <SelectTrigger id="quiet-hours-start">
                <SelectValue placeholder="选择开始时间" />
              </SelectTrigger>
              <SelectContent>
                {HOUR_OPTIONS.map((option) => (
                  <SelectItem key={option.value} value={option.value}>
                    {option.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-2">
            <Label htmlFor="quiet-hours-end">结束时间</Label>
            <Select
              value={config.quietHoursEnd?.toString() || '8'}
              onValueChange={(value) =>
                setQuietHours(config.quietHoursStart || 22, parseInt(value, 10))
              }
              disabled={isLoading}
            >
              <SelectTrigger id="quiet-hours-end">
                <SelectValue placeholder="选择结束时间" />
              </SelectTrigger>
              <SelectContent>
                {HOUR_OPTIONS.map((option) => (
                  <SelectItem key={option.value} value={option.value}>
                    {option.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>
      )}

      {/* Info */}
      {hasQuietHours && config.quietHoursStart !== undefined && config.quietHoursEnd !== undefined && (
        <p className="text-sm text-muted-foreground">
          {config.quietHoursStart >= config.quietHoursEnd ? (
            <>
              免打扰时段为 {config.quietHoursStart.toString().padStart(2, '0')}:00 到次日{' '}
              {config.quietHoursEnd.toString().padStart(2, '0')}:00
            </>
          ) : (
            <>
              免打扰时段为 {config.quietHoursStart.toString().padStart(2, '0')}:00 到{' '}
              {config.quietHoursEnd.toString().padStart(2, '0')}:00
            </>
          )}
        </p>
      )}
    </div>
  );
}

export default QuietHoursPicker;