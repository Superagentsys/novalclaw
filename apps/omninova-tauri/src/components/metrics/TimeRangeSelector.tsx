/**
 * Time Range Selector - Select time range for metrics
 *
 * Allows selection of:
 * - Preset time ranges (15min, 1h, 6h, 24h, 7d)
 * - Custom time range
 *
 * [Source: Story 9.2 - 代理性能监控]
 */

import { useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { useMetricsStore } from '@/stores/metricsStore';
import { TIME_RANGE_PRESETS, type TimeRange } from '@/types/metrics';

type PresetValue = (typeof TIME_RANGE_PRESETS)[number]['value'];

export function TimeRangeSelector() {
  const { timeRange, setTimeRange } = useMetricsStore();
  const [selectedPreset, setSelectedPreset] = useState<string>('3600'); // Default: 1 hour

  const handlePresetChange = (value: string) => {
    setSelectedPreset(value);
    const seconds = parseInt(value, 10);
    const now = Math.floor(Date.now() / 1000);
    setTimeRange({
      from: now - seconds,
      to: now,
    });
  };

  return (
    <div className="flex items-center gap-2">
      <span className="text-sm text-muted-foreground">时间范围:</span>
      <Select value={selectedPreset} onValueChange={handlePresetChange}>
        <SelectTrigger className="w-[140px]">
          <SelectValue placeholder="选择时间范围" />
        </SelectTrigger>
        <SelectContent>
          {TIME_RANGE_PRESETS.map((preset) => (
            <SelectItem key={preset.value} value={preset.value.toString()}>
              {preset.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

export default TimeRangeSelector;