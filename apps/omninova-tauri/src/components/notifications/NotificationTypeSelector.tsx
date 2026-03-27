/**
 * Notification Type Selector
 *
 * Allows selecting which notification types to enable.
 *
 * [Source: Story 9.3 - 系统通知管理]
 */

import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
import { useNotificationStore } from '@/stores/notificationStore';
import {
  ALL_NOTIFICATION_TYPES,
  NOTIFICATION_TYPE_LABELS,
  NOTIFICATION_TYPE_DESCRIPTIONS,
  type NotificationType,
} from '@/types/notification';

export function NotificationTypeSelector() {
  const { config, toggleNotificationType, isLoading } = useNotificationStore();

  return (
    <div className="space-y-4">
      {ALL_NOTIFICATION_TYPES.map((type) => (
        <div key={type} className="flex items-start space-x-3">
          <Checkbox
            id={`notification-type-${type}`}
            checked={config.enabledTypes.includes(type)}
            onCheckedChange={() => toggleNotificationType(type)}
            disabled={isLoading}
          />
          <div className="grid gap-1.5 leading-none">
            <Label
              htmlFor={`notification-type-${type}`}
              className="text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70"
            >
              {NOTIFICATION_TYPE_LABELS[type]}
            </Label>
            <p className="text-sm text-muted-foreground">
              {NOTIFICATION_TYPE_DESCRIPTIONS[type]}
            </p>
          </div>
        </div>
      ))}
    </div>
  );
}

export default NotificationTypeSelector;