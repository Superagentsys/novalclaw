/**
 * AgentStyleConfigForm 组件
 *
 * 代理响应风格配置表单
 *
 * [Source: Story 7.1 - 代理响应风格配置]
 */

import { type FC, useState, useEffect, useCallback } from 'react';
import { RefreshCw } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { Input } from '@/components/ui/input';
import { Slider } from '@/components/ui/slider';
import {
  type AgentStyleConfig,
  type ResponseStyle,
  RESPONSE_STYLE_LABELS,
  VERBOSITY_LABELS,
  VERBOSITY_PRESETS,
  DEFAULT_STYLE_CONFIG,
} from '@/types/agent';

export interface AgentStyleConfigFormProps {
  /** Current style config */
  config: AgentStyleConfig;
  /** Callback when config changes */
  onChange: (config: AgentStyleConfig) => void;
  /** Whether the form is disabled */
  disabled?: boolean;
}

/**
 * Get verbosity preset key from value
 */
function getVerbosityPreset(value: number): keyof typeof VERBOSITY_PRESETS {
  if (value <= 0.35) return 'brief';
  if (value >= 0.65) return 'detailed';
  return 'normal';
}

/**
 * AgentStyleConfigForm component
 */
export const AgentStyleConfigForm: FC<AgentStyleConfigFormProps> = ({
  config,
  onChange,
  disabled = false,
}) => {
  const [verbosityPreset, setVerbosityPreset] = useState<keyof typeof VERBOSITY_PRESETS>(
    getVerbosityPreset(config.verbosity)
  );

  // Update verbosity preset when config changes
  useEffect(() => {
    setVerbosityPreset(getVerbosityPreset(config.verbosity));
  }, [config.verbosity]);

  // Handle response style change
  const handleResponseStyleChange = useCallback((value: ResponseStyle) => {
    onChange({ ...config, responseStyle: value });
  }, [config, onChange]);

  // Handle verbosity slider change
  const handleVerbosityChange = useCallback((values: number[]) => {
    const newVerbosity = values[0] / 100;
    onChange({ ...config, verbosity: newVerbosity });
  }, [config, onChange]);

  // Handle verbosity preset change
  const handleVerbosityPresetChange = useCallback((preset: keyof typeof VERBOSITY_PRESETS) => {
    setVerbosityPreset(preset);
    onChange({ ...config, verbosity: VERBOSITY_PRESETS[preset] });
  }, [config, onChange]);

  // Handle max response length change
  const handleMaxLengthChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const value = parseInt(e.target.value, 10) || 0;
    onChange({ ...config, maxResponseLength: Math.max(0, value) });
  }, [config, onChange]);

  // Handle friendly tone toggle
  const handleFriendlyToneChange = useCallback((checked: boolean) => {
    onChange({ ...config, friendlyTone: checked });
  }, [config, onChange]);

  // Handle reset to defaults
  const handleReset = useCallback(() => {
    onChange(DEFAULT_STYLE_CONFIG);
  }, [onChange]);

  return (
    <div className="space-y-6">
      {/* Response style selection */}
      <div className="space-y-2">
        <Label>风格预设</Label>
        <div className="grid grid-cols-4 gap-2">
          {(Object.entries(RESPONSE_STYLE_LABELS) as [ResponseStyle, string][]).map(
            ([value, label]) => (
              <Button
                key={value}
                type="button"
                variant={config.responseStyle === value ? 'default' : 'outline'}
                size="sm"
                onClick={() => handleResponseStyleChange(value)}
                disabled={disabled}
                className="w-full"
              >
                {label}
              </Button>
            )
          )}
        </div>
      </div>

      {/* Verbosity slider */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <Label>详细程度</Label>
          <span className="text-sm text-muted-foreground">
            {VERBOSITY_LABELS[verbosityPreset]}
          </span>
        </div>
        <Slider
          value={[config.verbosity * 100]}
          onValueChange={handleVerbosityChange}
          min={0}
          max={100}
          step={1}
          disabled={disabled}
        />
        <div className="flex justify-between text-xs text-muted-foreground">
          <span>简短</span>
          <span>详细</span>
        </div>
        {/* Verbosity presets */}
        <div className="flex gap-2 mt-2">
          {(Object.entries(VERBOSITY_LABELS) as [keyof typeof VERBOSITY_PRESETS, string][]).map(
            ([preset, label]) => (
              <Button
                key={preset}
                type="button"
                variant={verbosityPreset === preset ? 'secondary' : 'ghost'}
                size="sm"
                onClick={() => handleVerbosityPresetChange(preset)}
                disabled={disabled}
              >
                {label}
              </Button>
            )
          )}
        </div>
      </div>

      {/* Max response length */}
      <div className="space-y-2">
        <Label htmlFor="max-length">最大响应长度</Label>
        <div className="flex items-center gap-2">
          <Input
            id="max-length"
            type="number"
            value={config.maxResponseLength}
            onChange={handleMaxLengthChange}
            min={0}
            placeholder="0"
            disabled={disabled}
            className="flex-1"
          />
          <span className="text-sm text-muted-foreground whitespace-nowrap">
            (0 = 无限制)
          </span>
        </div>
        <p className="text-xs text-muted-foreground">
          设置响应的最大字符数，超过将被截断
        </p>
      </div>

      {/* Friendly tone toggle */}
      <div className="flex items-center justify-between">
        <div className="space-y-0.5">
          <Label htmlFor="friendly-tone">添加友好问候语</Label>
          <p className="text-xs text-muted-foreground">
            在响应中添加问候语和结束语
          </p>
        </div>
        <Switch
          id="friendly-tone"
          checked={config.friendlyTone}
          onCheckedChange={handleFriendlyToneChange}
          disabled={disabled}
        />
      </div>

      {/* Reset button */}
      <div className="pt-4 border-t">
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={handleReset}
          disabled={disabled}
        >
          <RefreshCw className="h-4 w-4 mr-2" />
          重置为默认
        </Button>
      </div>
    </div>
  );
};

export default AgentStyleConfigForm;