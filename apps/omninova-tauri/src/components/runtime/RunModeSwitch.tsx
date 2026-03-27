/**
 * Run Mode Switch Component
 *
 * Allows users to switch between desktop and background modes.
 *
 * [Source: Story 9.5 - 运行模式管理]
 */

import { useEffect } from 'react';
import { useRuntimeStore } from '@/stores/runtimeStore';
import { RUN_MODE_LABELS, RUN_MODE_DESCRIPTIONS, type RunMode } from '@/types/runtime';
import { cn } from '@/lib/utils';

interface RunModeSwitchProps {
  /** Additional CSS classes */
  className?: string;
}

/**
 * Run mode switch component
 */
export function RunModeSwitch({ className }: RunModeSwitchProps) {
  const { mode, isLoading, setMode, loadConfig } = useRuntimeStore();

  // Load config on mount
  useEffect(() => {
    void loadConfig();
  }, [loadConfig]);

  const modes: RunMode[] = ['desktop', 'background'];

  return (
    <div className={cn('space-y-3', className)}>
      <h3 className="text-sm font-medium text-foreground">运行模式</h3>
      <div className="flex gap-2">
        {modes.map((m) => (
          <button
            key={m}
            onClick={() => void setMode(m)}
            disabled={isLoading}
            className={cn(
              'flex-1 px-4 py-3 rounded-lg border text-sm transition-colors',
              'disabled:opacity-50 disabled:cursor-not-allowed',
              mode === m
                ? 'border-primary bg-primary/10 text-primary'
                : 'border-border bg-background text-muted-foreground hover:border-primary/50 hover:text-foreground'
            )}
          >
            <div className="font-medium">{RUN_MODE_LABELS[m]}</div>
            <div className="text-xs mt-1 opacity-70">
              {RUN_MODE_DESCRIPTIONS[m]}
            </div>
          </button>
        ))}
      </div>
    </div>
  );
}

export default RunModeSwitch;