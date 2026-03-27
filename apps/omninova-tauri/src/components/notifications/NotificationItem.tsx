/**
 * Notification Item
 *
 * Displays a single notification entry.
 *
 * [Source: Story 9.3 - 系统通知管理]
 */

import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { useNotificationStore } from '@/stores/notificationStore';
import {
  type Notification,
  NOTIFICATION_TYPE_LABELS,
  PRIORITY_LABELS,
  formatNotificationTime,
} from '@/types/notification';

interface NotificationItemProps {
  notification: Notification;
}

export function NotificationItem({ notification }: NotificationItemProps) {
  const { markAsRead } = useNotificationStore();

  const handleClick = () => {
    if (!notification.read) {
      markAsRead(notification.id);
    }
  };

  return (
    <div
      className={cn(
        'p-3 rounded-lg border transition-colors cursor-pointer',
        notification.read
          ? 'bg-background hover:bg-muted/50'
          : 'bg-muted/50 hover:bg-muted border-primary/20'
      )}
      onClick={handleClick}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="flex-1 min-w-0">
          {/* Header */}
          <div className="flex items-center gap-2 mb-1">
            {!notification.read && (
              <span className="w-2 h-2 rounded-full bg-primary flex-shrink-0" />
            )}
            <span className="font-medium text-sm truncate">
              {notification.title}
            </span>
          </div>

          {/* Body */}
          <p className="text-sm text-muted-foreground line-clamp-2 mb-2">
            {notification.body}
          </p>

          {/* Footer */}
          <div className="flex items-center gap-2">
            <Badge variant="outline" className="text-xs">
              {NOTIFICATION_TYPE_LABELS[notification.notificationType]}
            </Badge>
            {notification.priority !== 'normal' && (
              <Badge
                variant={
                  notification.priority === 'urgent'
                    ? 'destructive'
                    : notification.priority === 'high'
                    ? 'default'
                    : 'secondary'
                }
                className="text-xs"
              >
                {PRIORITY_LABELS[notification.priority]}
              </Badge>
            )}
            <span className="text-xs text-muted-foreground">
              {formatNotificationTime(notification.createdAt)}
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}

export default NotificationItem;