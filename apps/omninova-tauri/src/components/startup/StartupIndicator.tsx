/**
 * Startup Indicator Component
 *
 * Displays startup progress during application initialization.
 *
 * [Source: Story 9.6 - 应用启动优化]
 */

import { useEffect } from 'react';
import { useStartupStore } from '@/stores/startupStore';
import {
  STARTUP_PHASE_LABELS,
  type StartupPhase,
} from '@/types/startup';
import { Progress } from '@/components/ui/progress';
import { cn } from '@/lib/utils';

interface StartupIndicatorProps {
  /** Additional CSS classes */
  className?: string;
  /** Callback when startup is complete */
  onComplete?: () => void;
}

// 启动阶段序列
const STARTUP_SEQUENCE: StartupPhase[] = [
  'initializing',
  'loading-config',
  'loading-ui',
  'ready',
];

/**
 * Startup indicator component
 */
export function StartupIndicator({ className, onComplete }: StartupIndicatorProps) {
  const { progress, loadReport } = useStartupStore();

  // Load startup report on mount
  useEffect(() => {
    void loadReport();
  }, [loadReport]);

  // Notify when ready
  useEffect(() => {
    if (progress.phase === 'ready' && onComplete) {
      onComplete();
    }
  }, [progress.phase, onComplete]);

  return (
    <div
      className={cn(
        'flex flex-col items-center justify-center min-h-[200px] p-8',
        className
      )}
    >
      {/* Logo or Icon */}
      <div className="mb-6 text-4xl font-bold text-primary">
        OmniNova
      </div>

      {/* Progress Bar */}
      <div className="w-full max-w-xs mb-4">
        <Progress value={progress.progress} className="h-2" />
      </div>

      {/* Status Text */}
      <div className="text-sm text-muted-foreground">
        {STARTUP_PHASE_LABELS[progress.phase]}
      </div>

      {/* Progress Percentage */}
      <div className="text-xs text-muted-foreground/60 mt-1">
        {Math.round(progress.progress)}%
      </div>
    </div>
  );
}

/**
 * Minimal startup indicator for inline use
 */
export function StartupIndicatorMinimal({ className }: { className?: string }) {
  const { progress } = useStartupStore();

  return (
    <div className={cn('flex items-center gap-2', className)}>
      <Progress value={progress.progress} className="h-1 flex-1" />
      <span className="text-xs text-muted-foreground">
        {Math.round(progress.progress)}%
      </span>
    </div>
  );
}

export default StartupIndicator;