/**
 * TriggerKeywordsConfigForm 组件
 *
 * 代理触发关键词配置表单
 *
 * [Source: Story 7.3 - 触发关键词配置]
 */

import { type FC, useState, useCallback } from 'react';
import { Plus, Trash2, RefreshCw, TestTube } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import {
  type AgentTriggerConfig,
  type TriggerKeyword,
  type MatchType,
  type TriggerTestResult,
  MATCH_TYPE_LABELS,
  MATCH_TYPE_DESCRIPTIONS,
  DEFAULT_TRIGGER_CONFIG,
} from '@/types/agent';

export interface TriggerKeywordsConfigFormProps {
  /** Current trigger keywords config */
  config: AgentTriggerConfig;
  /** Callback when config changes */
  onChange: (config: AgentTriggerConfig) => void;
  /** Callback to test triggers */
  onTestTriggers?: (testText: string) => Promise<TriggerTestResult>;
  /** Whether the form is disabled */
  disabled?: boolean;
}

/**
 * Create a new empty trigger keyword
 */
function createEmptyKeyword(): TriggerKeyword {
  return {
    keyword: '',
    matchType: 'exact',
    caseSensitive: false,
  };
}

/**
 * TriggerKeywordsConfigForm component
 */
export const TriggerKeywordsConfigForm: FC<TriggerKeywordsConfigFormProps> = ({
  config,
  onChange,
  onTestTriggers,
  disabled = false,
}) => {
  const [isAddDialogOpen, setIsAddDialogOpen] = useState(false);
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [newKeyword, setNewKeyword] = useState<TriggerKeyword>(createEmptyKeyword());
  const [testText, setTestText] = useState('');
  const [testResult, setTestResult] = useState<TriggerTestResult | null>(null);
  const [isTesting, setIsTesting] = useState(false);

  // Handle enabled toggle
  const handleEnabledChange = useCallback((checked: boolean) => {
    onChange({ ...config, enabled: checked });
  }, [config, onChange]);

  // Handle default match type change
  const handleDefaultMatchTypeChange = useCallback((value: MatchType) => {
    onChange({ ...config, defaultMatchType: value });
  }, [config, onChange]);

  // Handle default case sensitive toggle
  const handleDefaultCaseSensitiveChange = useCallback((checked: boolean) => {
    onChange({ ...config, defaultCaseSensitive: checked });
  }, [config, onChange]);

  // Open add dialog
  const handleOpenAddDialog = useCallback(() => {
    setNewKeyword({
      keyword: '',
      matchType: config.defaultMatchType,
      caseSensitive: config.defaultCaseSensitive,
    });
    setEditingIndex(null);
    setIsAddDialogOpen(true);
  }, [config.defaultMatchType, config.defaultCaseSensitive]);

  // Open edit dialog
  const handleOpenEditDialog = useCallback((index: number) => {
    setNewKeyword({ ...config.keywords[index] });
    setEditingIndex(index);
    setIsAddDialogOpen(true);
  }, [config.keywords]);

  // Save keyword (add or edit)
  const handleSaveKeyword = useCallback(() => {
    if (!newKeyword.keyword.trim()) return;

    let newKeywords: TriggerKeyword[];
    if (editingIndex !== null) {
      // Edit existing
      newKeywords = [...config.keywords];
      newKeywords[editingIndex] = newKeyword;
    } else {
      // Add new
      newKeywords = [...config.keywords, newKeyword];
    }

    onChange({ ...config, keywords: newKeywords });
    setIsAddDialogOpen(false);
    setNewKeyword(createEmptyKeyword());
    setEditingIndex(null);
  }, [config, newKeyword, editingIndex, onChange]);

  // Remove keyword
  const handleRemoveKeyword = useCallback((index: number) => {
    const newKeywords = config.keywords.filter((_, i) => i !== index);
    onChange({ ...config, keywords: newKeywords });
  }, [config, onChange]);

  // Handle reset to defaults
  const handleReset = useCallback(() => {
    onChange(DEFAULT_TRIGGER_CONFIG);
  }, [onChange]);

  // Test triggers
  const handleTestTriggers = useCallback(async () => {
    if (!onTestTriggers || !testText.trim()) return;

    setIsTesting(true);
    try {
      const result = await onTestTriggers(testText);
      setTestResult(result);
    } catch (error) {
      console.error('Test failed:', error);
    } finally {
      setIsTesting(false);
    }
  }, [onTestTriggers, testText]);

  return (
    <div className="space-y-6">
      {/* Enable toggle */}
      <div className="flex items-center justify-between">
        <div className="space-y-0.5">
          <Label htmlFor="trigger-enabled">启用触发关键词</Label>
          <p className="text-xs text-muted-foreground">
            只有匹配触发词的消息才会触发代理响应
          </p>
        </div>
        <Switch
          id="trigger-enabled"
          checked={config.enabled}
          onCheckedChange={handleEnabledChange}
          disabled={disabled}
        />
      </div>

      {/* Keywords list */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <Label>
            已配置的关键词 ({config.keywords.length})
          </Label>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={handleOpenAddDialog}
            disabled={disabled}
          >
            <Plus className="h-4 w-4 mr-1" />
            添加关键词
          </Button>
        </div>

        {config.keywords.length > 0 ? (
          <div className="border rounded-md divide-y">
            {config.keywords.map((keyword, index) => (
              <div
                key={index}
                className="flex items-center justify-between p-3"
              >
                <div className="flex items-center gap-3">
                  <code className="px-2 py-1 bg-muted rounded text-sm">
                    {keyword.keyword}
                  </code>
                  <span className="text-sm text-muted-foreground">
                    {MATCH_TYPE_LABELS[keyword.matchType]}
                  </span>
                  {keyword.caseSensitive && (
                    <span className="text-xs px-1.5 py-0.5 bg-secondary rounded">
                      区分大小写
                    </span>
                  )}
                </div>
                <div className="flex items-center gap-1">
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => handleOpenEditDialog(index)}
                    disabled={disabled}
                  >
                    编辑
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => handleRemoveKeyword(index)}
                    disabled={disabled}
                    className="text-destructive hover:text-destructive"
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              </div>
            ))}
          </div>
        ) : (
          <div className="text-sm text-muted-foreground py-4 text-center border rounded-md">
            {config.enabled
              ? '未配置触发关键词，所有消息都会触发代理响应'
              : '触发关键词已禁用'}
          </div>
        )}
      </div>

      {/* Default settings for new keywords */}
      <div className="grid grid-cols-2 gap-4">
        <div className="space-y-2">
          <Label>默认匹配模式</Label>
          <Select
            value={config.defaultMatchType}
            onValueChange={(value) => handleDefaultMatchTypeChange(value as MatchType)}
            disabled={disabled}
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {(Object.entries(MATCH_TYPE_LABELS) as [MatchType, string][]).map(
                ([value, label]) => (
                  <SelectItem key={value} value={value}>
                    {label}
                  </SelectItem>
                )
              )}
            </SelectContent>
          </Select>
        </div>

        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label>默认区分大小写</Label>
          </div>
          <Switch
            checked={config.defaultCaseSensitive}
            onCheckedChange={handleDefaultCaseSensitiveChange}
            disabled={disabled}
          />
        </div>
      </div>

      {/* Test panel */}
      {onTestTriggers && config.keywords.length > 0 && (
        <div className="space-y-3 border-t pt-4">
          <Label className="flex items-center gap-2">
            <TestTube className="h-4 w-4" />
            触发词测试
          </Label>
          <div className="flex gap-2">
            <Input
              placeholder="输入测试消息..."
              value={testText}
              onChange={(e) => setTestText(e.target.value)}
              disabled={disabled || isTesting}
              className="flex-1"
            />
            <Button
              type="button"
              variant="secondary"
              onClick={handleTestTriggers}
              disabled={disabled || isTesting || !testText.trim()}
            >
              {isTesting ? '测试中...' : '测试'}
            </Button>
          </div>

          {testResult && (
            <div className={`p-3 rounded-md ${testResult.matched ? 'bg-green-50 border border-green-200' : 'bg-muted'}`}>
              <div className="flex items-center gap-2 mb-2">
                <span className={testResult.matched ? 'text-green-600' : 'text-muted-foreground'}>
                  {testResult.matched ? '✓ 匹配成功' : '✗ 未匹配'}
                </span>
              </div>
              {testResult.matchedKeywords.length > 0 && (
                <div className="text-sm">
                  <span className="text-muted-foreground">匹配的关键词：</span>
                  {testResult.matchedKeywords.map((kw, i) => (
                    <span key={i} className="inline-block mr-2">
                      <code className="px-1 bg-green-100 rounded">{kw.keyword}</code>
                      <span className="text-muted-foreground text-xs ml-1">
                        ({MATCH_TYPE_LABELS[kw.matchType]})
                      </span>
                    </span>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      )}

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

      {/* Add/Edit Keyword Dialog */}
      <Dialog open={isAddDialogOpen} onOpenChange={setIsAddDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {editingIndex !== null ? '编辑触发关键词' : '添加触发关键词'}
            </DialogTitle>
          </DialogHeader>

          <div className="space-y-4 py-4">
            {/* Keyword input */}
            <div className="space-y-2">
              <Label htmlFor="keyword">关键词/模式</Label>
              <Input
                id="keyword"
                value={newKeyword.keyword}
                onChange={(e) => setNewKeyword({ ...newKeyword, keyword: e.target.value })}
                placeholder={newKeyword.matchType === 'regex' ? '正则表达式，如: help\\s+\\w+' : '输入关键词'}
                disabled={disabled}
              />
            </div>

            {/* Match type selection */}
            <div className="space-y-2">
              <Label>匹配模式</Label>
              <div className="space-y-2">
                {(Object.entries(MATCH_TYPE_LABELS) as [MatchType, string][]).map(
                  ([value, label]) => (
                    <div
                      key={value}
                      className={`flex items-start gap-3 p-2 rounded-md cursor-pointer border ${
                        newKeyword.matchType === value
                          ? 'border-primary bg-primary/5'
                          : 'border-transparent hover:bg-muted'
                      }`}
                      onClick={() => setNewKeyword({ ...newKeyword, matchType: value })}
                    >
                      <input
                        type="radio"
                        name="matchType"
                        value={value}
                        checked={newKeyword.matchType === value}
                        onChange={() => setNewKeyword({ ...newKeyword, matchType: value })}
                        className="mt-1"
                      />
                      <div>
                        <div className="font-medium text-sm">{label}</div>
                        <div className="text-xs text-muted-foreground">
                          {MATCH_TYPE_DESCRIPTIONS[value]}
                        </div>
                      </div>
                    </div>
                  )
                )}
              </div>
            </div>

            {/* Case sensitive toggle */}
            <div className="flex items-center justify-between">
              <Label htmlFor="case-sensitive">区分大小写</Label>
              <Switch
                id="case-sensitive"
                checked={newKeyword.caseSensitive}
                onCheckedChange={(checked) => setNewKeyword({ ...newKeyword, caseSensitive: checked })}
                disabled={disabled}
              />
            </div>
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setIsAddDialogOpen(false)}
            >
              取消
            </Button>
            <Button
              type="button"
              onClick={handleSaveKeyword}
              disabled={!newKeyword.keyword.trim()}
            >
              {editingIndex !== null ? '保存' : '添加关键词'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
};

export default TriggerKeywordsConfigForm;