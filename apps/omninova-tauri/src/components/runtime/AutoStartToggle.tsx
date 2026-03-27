/**
 * Auto Start Toggle Component
 *
 * Allows users to enable/disable auto-start on boot.
 *
 * [Source: Story 9.5 - 运行模式管理]
 */

import { useEffect } from 'react';
import { useRuntimeStore } from '@/stores/runtimeStore';
import { Switch } from '@/components/ui/switch';
import { Label } from '@/components/ui/label';
import { cn } from '@/lib/utils';

interface AutoStartToggleProps {
  /** Additional CSS classes */
  className?: string;
}

/**
 * Auto start toggle component
 */
export function AutoStartToggle({ className }: AutoStartToggleProps) {
  const { autoStart, isLoading, setAutoStart, loadConfig } = useRuntimeStore();

  // Load config on mount
  useEffect(() => {
    void loadConfig();
  }, [loadConfig]);

  const handleToggle = (enabled: boolean) => {
    void setAutoStart(enabled);
  };

  return (
    <div className={cn('flex items-center justify-between', className)}>
      <div className="space-y-0.5">
        <Label htmlFor="auto-start" className="text-sm font-medium">
          开机自启动
        </Label>
        <p className="text-xs text-muted-foreground">
          系统启动时自动运行应用
        </p>
      </div>
      <Switch
        id="auto-start"
        checked={autoStart}
        onCheckedChange={handleToggle}
        disabled={isLoading}
      />
    </div>
  );
}

export default AutoStartToggle;