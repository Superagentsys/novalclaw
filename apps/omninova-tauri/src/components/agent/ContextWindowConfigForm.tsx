/**
 * ContextWindowConfigForm 组件
 *
 * 代理上下文窗口配置表单
 *
 * [Source: Story 7.2 - 上下文窗口配置]
 */

import { type FC, useState, useEffect, useCallback } from 'react';
import { AlertCircle, RefreshCw, Info } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { Input } from '@/components/ui/input';
import { Slider } from '@/components/ui/slider';
import { Alert, AlertDescription } from '@/components/ui/alert';
import {
  type ContextWindowConfig,
  type OverflowStrategy,
  OVERFLOW_STRATEGY_LABELS,
  CONTEXT_WINDOW_PRESETS,
  DEFAULT_CONTEXT_WINDOW_CONFIG,
} from '@/types/agent';

export interface ContextWindowConfigFormProps {
  /** Current context window config */
  config: ContextWindowConfig;
  /** Callback when config changes */
  onChange: (config: ContextWindowConfig) => void;
  /** Model name for recommendations */
  modelName?: string;
  /** Whether the form is disabled */
  disabled?: boolean;
}

/**
 * ContextWindowConfigForm component
 */
export const ContextWindowConfigForm: FC<ContextWindowConfigFormProps> = ({
  config,
  onChange,
  modelName,
  disabled = false,
}) => {
  const [modelRecommendation, setModelRecommendation] = useState<{ recommended: number; max: number } | null>(null);
  const [showWarning, setShowWarning] = useState(false);

  // Fetch model recommendations when model changes
  useEffect(() => {
    if (modelName) {
      // Import the invoke function to call the backend
      import('@tauri-apps/api/core').then(({ invoke }) => {
        invoke<[number, number] | null>('get_model_context_recommendations', { modelName })
          .then((result) => {
            if (result) {
              setModelRecommendation({ recommended: result[0], max: result[1] });
            } else {
              setModelRecommendation(null);
            }
          })
          .catch(() => {
            setModelRecommendation(null);
          });
      });
    } else {
      setModelRecommendation(null);
    }
  }, [modelName]);

  // Show warning if max tokens exceeds model recommendation
  useEffect(() => {
    if (modelRecommendation && config.maxTokens > modelRecommendation.recommended) {
      setShowWarning(true);
    } else {
      setShowWarning(false);
    }
  }, [config.maxTokens, modelRecommendation]);

  // Handle max tokens change
  const handleMaxTokensChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const value = parseInt(e.target.value, 10) || 0;
    onChange({ ...config, maxTokens: Math.max(0, value) });
  }, [config, onChange]);

  // Handle slider change (value is in thousands)
  const handleSliderChange = useCallback((values: number[]) => {
    onChange({ ...config, maxTokens: values[0] });
  }, [config, onChange]);

  // Handle preset click
  const handlePresetClick = useCallback((value: number) => {
    onChange({ ...config, maxTokens: value });
  }, [config, onChange]);

  // Handle overflow strategy change
  const handleOverflowStrategyChange = useCallback((value: OverflowStrategy) => {
    onChange({ ...config, overflowStrategy: value });
  }, [config, onChange]);

  // Handle include system prompt toggle
  const handleIncludeSystemPromptChange = useCallback((checked: boolean) => {
    onChange({ ...config, includeSystemPrompt: checked });
  }, [config, onChange]);

  // Handle response reserve change
  const handleResponseReserveChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const value = parseInt(e.target.value, 10) || 0;
    onChange({ ...config, responseReserve: Math.max(0, value) });
  }, [config, onChange]);

  // Handle reset to defaults
  const handleReset = useCallback(() => {
    onChange(DEFAULT_CONTEXT_WINDOW_CONFIG);
  }, [onChange]);

  return (
    <div className="space-y-6">
      {/* Model recommendation */}
      {modelRecommendation && (
        <Alert>
          <Info className="h-4 w-4" />
          <AlertDescription>
            推荐值: {modelName} 建议 {modelRecommendation.recommended / 1024}K tokens
            (最大 {modelRecommendation.max / 1024}K)
          </AlertDescription>
        </Alert>
      )}

      {/* Max tokens slider */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <Label>上下文窗口大小</Label>
          <span className="text-sm text-muted-foreground">
            {config.maxTokens >= 1024
              ? `${(config.maxTokens / 1024).toFixed(1)}K`
              : config.maxTokens} tokens
          </span>
        </div>
        <Slider
          value={[config.maxTokens]}
          onValueChange={handleSliderChange}
          min={512}
          max={modelRecommendation?.max || 128000}
          step={512}
          disabled={disabled}
        />
        <div className="flex justify-between text-xs text-muted-foreground">
          <span>512</span>
          <span>{modelRecommendation ? `${(modelRecommendation.max / 1024).toFixed(0)}K` : '128K'}</span>
        </div>
        {/* Presets */}
        <div className="flex flex-wrap gap-2 mt-2">
          {CONTEXT_WINDOW_PRESETS.map((preset) => (
            <Button
              key={preset.value}
              type="button"
              variant={config.maxTokens === preset.value ? 'secondary' : 'ghost'}
              size="sm"
              onClick={() => handlePresetClick(preset.value)}
              disabled={disabled || (modelRecommendation !== null && preset.value > modelRecommendation.max)}
            >
              {preset.label}
            </Button>
          ))}
        </div>
      </div>

      {/* Custom max tokens input */}
      <div className="space-y-2">
        <Label htmlFor="max-tokens">自定义上下文窗口大小</Label>
        <div className="flex items-center gap-2">
          <Input
            id="max-tokens"
            type="number"
            value={config.maxTokens}
            onChange={handleMaxTokensChange}
            min={0}
            placeholder="4096"
            disabled={disabled}
            className="flex-1"
          />
          <span className="text-sm text-muted-foreground whitespace-nowrap">
            tokens
          </span>
        </div>
      </div>

      {/* Warning for oversized context */}
      {showWarning && modelRecommendation && (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>
            当前设置 ({(config.maxTokens / 1024).toFixed(1)}K) 超过推荐值
            ({(modelRecommendation.recommended / 1024).toFixed(1)}K)，可能导致性能下降或额外费用
          </AlertDescription>
        </Alert>
      )}

      {/* Overflow strategy */}
      <div className="space-y-2">
        <Label>溢出处理策略</Label>
        <div className="grid grid-cols-3 gap-2">
          {(Object.entries(OVERFLOW_STRATEGY_LABELS) as [OverflowStrategy, string][]).map(
            ([value, label]) => (
              <Button
                key={value}
                type="button"
                variant={config.overflowStrategy === value ? 'default' : 'outline'}
                size="sm"
                onClick={() => handleOverflowStrategyChange(value)}
                disabled={disabled}
                className="w-full"
              >
                {label}
              </Button>
            )
          )}
        </div>
        <p className="text-xs text-muted-foreground">
          当上下文超过限制时的处理方式
        </p>
      </div>

      {/* Include system prompt toggle */}
      <div className="flex items-center justify-between">
        <div className="space-y-0.5">
          <Label htmlFor="include-system">包含系统提示词在 Token 计数中</Label>
          <p className="text-xs text-muted-foreground">
            系统提示词将占用部分上下文窗口
          </p>
        </div>
        <Switch
          id="include-system"
          checked={config.includeSystemPrompt}
          onCheckedChange={handleIncludeSystemPromptChange}
          disabled={disabled}
        />
      </div>

      {/* Response reserve */}
      <div className="space-y-2">
        <Label htmlFor="response-reserve">响应预留空间</Label>
        <div className="flex items-center gap-2">
          <Input
            id="response-reserve"
            type="number"
            value={config.responseReserve}
            onChange={handleResponseReserveChange}
            min={0}
            placeholder="1024"
            disabled={disabled}
            className="flex-1"
          />
          <span className="text-sm text-muted-foreground whitespace-nowrap">
            tokens
          </span>
        </div>
        <p className="text-xs text-muted-foreground">
          为模型响应预留的 Token 数量，实际可用上下文 = 上下文窗口 - 响应预留
        </p>
        <p className="text-xs text-muted-foreground">
          当前有效上下文: <strong>{Math.max(0, config.maxTokens - config.responseReserve)}</strong> tokens
        </p>
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

export default ContextWindowConfigForm;