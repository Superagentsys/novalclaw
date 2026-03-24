/**
 * StylePreviewCard 组件
 *
 * 风格配置预览卡片
 *
 * [Source: Story 7.1 - 代理响应风格配置]
 */

import { type FC, useState, useEffect, useCallback } from 'react';
import { Loader2, Sparkles } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { type AgentStyleConfig } from '@/types/agent';

export interface StylePreviewCardProps {
  /** Current style config */
  config: AgentStyleConfig;
  /** Preview function from hook */
  previewEffect: (sampleText: string) => Promise<string>;
  /** Whether preview is disabled */
  disabled?: boolean;
}

/** Sample text for preview */
const SAMPLE_TEXTS = [
  {
    label: '问候',
    text: '你好！我是你的AI助手，很高兴为你服务。有什么我可以帮助你的吗？',
  },
  {
    label: '技术解释',
    text: 'React是一个用于构建用户界面的JavaScript库。它采用组件化的开发方式，让代码更加模块化和可复用。虚拟DOM技术使得页面更新更加高效。',
  },
  {
    label: '问题回答',
    text: '关于这个问题，我认为可以从几个方面来考虑。首先，我们需要理解问题的本质。其次，要分析可能的解决方案。最后，选择最适合的方法来实施。',
  },
];

/**
 * StylePreviewCard component
 */
export const StylePreviewCard: FC<StylePreviewCardProps> = ({
  config,
  previewEffect,
  disabled = false,
}) => {
  const [selectedSample, setSelectedSample] = useState(0);
  const [originalText] = useState(SAMPLE_TEXTS[0].text);
  const [previewText, setPreviewText] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Generate preview when config changes
  const generatePreview = useCallback(async () => {
    if (disabled) return;

    setIsLoading(true);
    setError(null);

    try {
      const result = await previewEffect(originalText);
      setPreviewText(result);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      setPreviewText(null);
    } finally {
      setIsLoading(false);
    }
  }, [originalText, previewEffect, disabled]);

  // Generate preview on mount and when config changes
  useEffect(() => {
    generatePreview();
  }, [config, generatePreview]);

  // Handle sample selection
  const handleSampleSelect = async (index: number) => {
    setSelectedSample(index);
    setIsLoading(true);
    setError(null);

    try {
      const result = await previewEffect(SAMPLE_TEXTS[index].text);
      setPreviewText(result);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      setPreviewText(null);
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h4 className="font-medium text-sm flex items-center gap-2">
          <Sparkles className="h-4 w-4" />
          预览效果
        </h4>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={generatePreview}
          disabled={disabled || isLoading}
        >
          {isLoading ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            '刷新'
          )}
        </Button>
      </div>

      {/* Sample selector */}
      <div className="flex gap-2">
        {SAMPLE_TEXTS.map((sample, index) => (
          <Button
            key={index}
            type="button"
            variant={selectedSample === index ? 'secondary' : 'ghost'}
            size="sm"
            onClick={() => handleSampleSelect(index)}
            disabled={disabled || isLoading}
          >
            {sample.label}
          </Button>
        ))}
      </div>

      {/* Preview content */}
      <div className="rounded-lg border bg-muted/30 p-4">
        {isLoading ? (
          <div className="flex items-center justify-center py-8">
            <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
          </div>
        ) : error ? (
          <div className="text-sm text-red-500 py-4">{error}</div>
        ) : previewText ? (
          <div className="space-y-3">
            <div className="text-xs text-muted-foreground uppercase tracking-wider">
              处理后
            </div>
            <p className="text-sm leading-relaxed">{previewText}</p>
          </div>
        ) : (
          <div className="text-sm text-muted-foreground py-4">
            点击刷新生成预览
          </div>
        )}
      </div>

      {/* Config summary */}
      <div className="flex flex-wrap gap-2 text-xs text-muted-foreground">
        <span>风格: {config.responseStyle}</span>
        <span>•</span>
        <span>详细度: {Math.round(config.verbosity * 100)}%</span>
        {config.maxResponseLength > 0 && (
          <>
            <span>•</span>
            <span>最大长度: {config.maxResponseLength}</span>
          </>
        )}
      </div>
    </div>
  );
};

export default StylePreviewCard;