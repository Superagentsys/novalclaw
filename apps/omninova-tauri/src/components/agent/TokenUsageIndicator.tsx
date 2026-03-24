/**
 * TokenUsageIndicator 组件
 *
 * 显示当前对话的 Token 使用量
 *
 * [Source: Story 7.2 - 上下文窗口配置]
 */

import { type FC, useState, useEffect, useCallback } from 'react';
import { Progress } from '@/components/ui/progress';
import { cn } from '@/lib/utils';

export interface TokenUsageIndicatorProps {
  /** Current token count */
  currentTokens: number;
  /** Maximum token limit */
  maxTokens: number;
  /** Reserved tokens for response */
  responseReserve?: number;
  /** Whether to show detailed info */
  showDetails?: boolean;
  /** Additional CSS classes */
  className?: string;
  /** Size variant */
  size?: 'sm' | 'md' | 'lg';
}

/**
 * TokenUsageIndicator component
 *
 * Displays a progress bar showing token usage relative to the context window limit.
 */
export const TokenUsageIndicator: FC<TokenUsageIndicatorProps> = ({
  currentTokens,
  maxTokens,
  responseReserve = 1024,
  showDetails = true,
  className,
  size = 'md',
}) => {
  const [percentage, setPercentage] = useState(0);

  // Calculate effective max tokens (accounting for response reserve)
  const effectiveMax = maxTokens > responseReserve ? maxTokens - responseReserve : maxTokens;

  // Update percentage
  useEffect(() => {
    if (effectiveMax > 0) {
      const pct = Math.min(100, (currentTokens / effectiveMax) * 100);
      setPercentage(Math.round(pct));
    } else {
      setPercentage(0);
    }
  }, [currentTokens, effectiveMax]);

  // Determine color based on usage
  const getColor = useCallback(() => {
    if (percentage >= 90) return 'bg-destructive';
    if (percentage >= 70) return 'bg-yellow-500';
    return 'bg-primary';
  }, [percentage]);

  // Get status text
  const getStatusText = useCallback(() => {
    if (percentage >= 90) return '即将达到上限';
    if (percentage >= 70) return '使用量较高';
    return '正常';
  }, [percentage]);

  // Size-based styling
  const sizeStyles = {
    sm: {
      text: 'text-xs',
      height: 'h-1',
    },
    md: {
      text: 'text-sm',
      height: 'h-2',
    },
    lg: {
      text: 'text-base',
      height: 'h-3',
    },
  };

  const styles = sizeStyles[size];

  // Format token count for display
  const formatTokens = (tokens: number): string => {
    if (tokens >= 1000) {
      return `${(tokens / 1000).toFixed(1)}K`;
    }
    return tokens.toString();
  };

  return (
    <div className={cn('space-y-1', className)}>
      {/* Token count display */}
      <div className={cn('flex items-center justify-between', styles.text)}>
        <span className="text-muted-foreground">
          Token 使用量
        </span>
        <span className="font-medium">
          {formatTokens(currentTokens)} / {formatTokens(effectiveMax)}
        </span>
      </div>

      {/* Progress bar */}
      <div className={cn('relative w-full overflow-hidden rounded-full bg-secondary', styles.height)}>
        <div
          className={cn(
            'h-full transition-all duration-300 ease-in-out',
            getColor()
          )}
          style={{ width: `${percentage}%` }}
        />
      </div>

      {/* Detailed info */}
      {showDetails && (
        <div className={cn('flex items-center justify-between', styles.text)}>
          <span className="text-muted-foreground">
            {getStatusText()}
          </span>
          <span className="text-muted-foreground">
            {percentage}%
          </span>
        </div>
      )}

      {/* Warning when close to limit */}
      {percentage >= 90 && (
        <div className={cn('text-destructive', styles.text)}>
          ⚠️ 上下文窗口即将达到上限，旧消息可能被{responseReserve > 0 ? '截断' : '处理'}
        </div>
      )}
    </div>
  );
};

export default TokenUsageIndicator;