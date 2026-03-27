/**
 * Metric Card - Display a single performance metric
 *
 * Shows:
 * - Metric label
 * - Metric value
 * - Optional trend indicator
 * - Warning state
 *
 * [Source: Story 9.2 - 代理性能监控]
 */

import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';

interface MetricCardProps {
  /** Metric label */
  label: string;
  /** Metric value */
  value: string | number;
  /** Optional description */
  description?: string;
  /** Warning state */
  isWarning?: boolean;
  /** Warning badge text */
  warningText?: string;
  /** Trend direction */
  trend?: 'up' | 'down' | 'neutral';
  /** Trend value (e.g., "+5%") */
  trendValue?: string;
}

export function MetricCard({
  label,
  value,
  description,
  isWarning,
  warningText,
  trend,
  trendValue,
}: MetricCardProps) {
  const getTrendColor = () => {
    switch (trend) {
      case 'up':
        return 'text-green-600';
      case 'down':
        return 'text-red-600';
      default:
        return 'text-muted-foreground';
    }
  };

  return (
    <Card className={isWarning ? 'border-amber-500' : ''}>
      <CardContent className="pt-6">
        <div className="flex items-center justify-between">
          <span className="text-sm font-medium text-muted-foreground">
            {label}
          </span>
          {isWarning && warningText && (
            <Badge variant="outline" className="text-amber-600 border-amber-600">
              {warningText}
            </Badge>
          )}
        </div>
        <div className="mt-2 flex items-baseline gap-2">
          <span className={`text-2xl font-bold ${isWarning ? 'text-amber-600' : ''}`}>
            {value}
          </span>
          {trend && trendValue && (
            <span className={`text-sm ${getTrendColor()}`}>
              {trend === 'up' && '↑'}
              {trend === 'down' && '↓'}
              {trendValue}
            </span>
          )}
        </div>
        {description && (
          <p className="mt-1 text-xs text-muted-foreground">{description}</p>
        )}
      </CardContent>
    </Card>
  );
}

export default MetricCard;