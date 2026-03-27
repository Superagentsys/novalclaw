/**
 * Notification History
 *
 * Displays a list of recent notifications.
 *
 * [Source: Story 9.3 - 系统通知管理]
 */

import { useEffect } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useNotificationStore } from '@/stores/notificationStore';
import { NotificationItem } from './NotificationItem';

export function NotificationHistory() {
  const { history, isLoading, loadHistory, markAllRead, clearHistory } =
    useNotificationStore();

  // Load history on mount
  useEffect(() => {
    loadHistory();
  }, [loadHistory]);

  const unreadCount = history.filter((n) => !n.read).length;

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <div>
            <CardTitle>通知历史</CardTitle>
            <CardDescription>
              {unreadCount > 0 ? `${unreadCount} 条未读` : '全部已读'}
            </CardDescription>
          </div>
          <div className="flex gap-2">
            {unreadCount > 0 && (
              <Button variant="outline" size="sm" onClick={markAllRead}>
                全部已读
              </Button>
            )}
            {history.length > 0 && (
              <Button variant="outline" size="sm" onClick={clearHistory}>
                清除历史
              </Button>
            )}
          </div>
        </div>
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <div className="flex items-center justify-center py-8">
            <p className="text-muted-foreground">加载中...</p>
          </div>
        ) : history.length === 0 ? (
          <div className="flex items-center justify-center py-8">
            <p className="text-muted-foreground">暂无通知</p>
          </div>
        ) : (
          <ScrollArea className="h-[400px]">
            <div className="space-y-2">
              {history.map((notification) => (
                <NotificationItem
                  key={notification.id}
                  notification={notification}
                />
              ))}
            </div>
          </ScrollArea>
        )}
      </CardContent>
    </Card>
  );
}

export default NotificationHistory;