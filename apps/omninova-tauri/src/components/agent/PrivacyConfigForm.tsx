/**
 * PrivacyConfigForm 组件
 *
 * 代理隐私配置表单，包含数据保留、敏感信息过滤、记忆共享范围等设置
 *
 * [Source: Story 7.4 - 数据处理与隐私设置]
 */

import { type FC, useState, useCallback } from 'react';
import {
  RefreshCw,
  ChevronDown,
  ChevronRight,
  Plus,
  Trash2,
  TestTube,
  Clock,
  Eye,
  Share2,
  Filter,
  AlertCircle,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import {
  type AgentPrivacyConfig,
  type SensitiveDataFilter,
  type ExclusionRule,
  type MemorySharingScope,
  DEFAULT_PRIVACY_CONFIG,
  MEMORY_SHARING_SCOPE_LABELS,
  MEMORY_SHARING_SCOPE_DESCRIPTIONS,
} from '@/types/agent';

export interface PrivacyConfigFormProps {
  /** Current privacy config */
  config: AgentPrivacyConfig;
  /** Callback when config changes */
  onChange: (config: AgentPrivacyConfig) => void;
  /** Callback to test sensitive filter */
  onTestFilter?: (content: string) => Promise<string>;
  /** Callback to validate exclusion pattern */
  onValidatePattern?: (pattern: string) => Promise<boolean>;
  /** Whether the form is disabled */
  disabled?: boolean;
}

/**
 * Create a new empty exclusion rule
 */
function createEmptyRule(): ExclusionRule {
  return {
    name: '',
    description: '',
    pattern: '',
    enabled: true,
  };
}

/**
 * PrivacyConfigForm component
 */
export const PrivacyConfigForm: FC<PrivacyConfigFormProps> = ({
  config,
  onChange,
  onTestFilter,
  onValidatePattern,
  disabled = false,
}) => {
  // Collapsible sections state
  const [dataRetentionExpanded, setDataRetentionExpanded] = useState(true);
  const [sensitiveFilterExpanded, setSensitiveFilterExpanded] = useState(true);
  const [memorySharingExpanded, setMemorySharingExpanded] = useState(true);
  const [exclusionRulesExpanded, setExclusionRulesExpanded] = useState(true);

  // Exclusion rule dialog state
  const [isAddRuleDialogOpen, setIsAddRuleDialogOpen] = useState(false);
  const [editingRuleIndex, setEditingRuleIndex] = useState<number | null>(null);
  const [newRule, setNewRule] = useState<ExclusionRule>(createEmptyRule());
  const [patternError, setPatternError] = useState<string | null>(null);
  const [isValidatingPattern, setIsValidatingPattern] = useState(false);

  // Filter test state
  const [testContent, setTestContent] = useState('');
  const [testResult, setTestResult] = useState<string | null>(null);
  const [isTesting, setIsTesting] = useState(false);

  // ============================================================================
  // Data Retention Handlers
  // ============================================================================

  const handleEpisodicDaysChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const value = parseInt(e.target.value, 10) || 0;
    onChange({
      ...config,
      dataRetention: { ...config.dataRetention, episodicMemoryDays: Math.max(0, value) },
    });
  }, [config, onChange]);

  const handleWorkingHoursChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const value = parseInt(e.target.value, 10) || 0;
    onChange({
      ...config,
      dataRetention: { ...config.dataRetention, workingMemoryHours: Math.max(0, value) },
    });
  }, [config, onChange]);

  const handleAutoCleanupChange = useCallback((checked: boolean) => {
    onChange({
      ...config,
      dataRetention: { ...config.dataRetention, autoCleanup: checked },
    });
  }, [config, onChange]);

  // ============================================================================
  // Sensitive Filter Handlers
  // ============================================================================

  const handleFilterEnabledChange = useCallback((checked: boolean) => {
    onChange({
      ...config,
      sensitiveFilter: { ...config.sensitiveFilter, enabled: checked },
    });
  }, [config, onChange]);

  const handleFilterTypeChange = useCallback(
    (key: keyof SensitiveDataFilter, checked: boolean) => {
      if (key === 'enabled' || key === 'customPatterns') return;
      onChange({
        ...config,
        sensitiveFilter: { ...config.sensitiveFilter, [key]: checked },
      });
    },
    [config, onChange]
  );

  const handleTestFilter = useCallback(async () => {
    if (!onTestFilter || !testContent.trim()) return;
    setIsTesting(true);
    try {
      const result = await onTestFilter(testContent);
      setTestResult(result);
    } catch (error) {
      console.error('Filter test failed:', error);
    } finally {
      setIsTesting(false);
    }
  }, [onTestFilter, testContent]);

  // ============================================================================
  // Memory Sharing Scope Handlers
  // ============================================================================

  const handleMemorySharingScopeChange = useCallback((scope: MemorySharingScope) => {
    onChange({ ...config, memorySharingScope: scope });
  }, [config, onChange]);

  // ============================================================================
  // Exclusion Rules Handlers
  // ============================================================================

  const handleOpenAddRuleDialog = useCallback(() => {
    setNewRule(createEmptyRule());
    setEditingRuleIndex(null);
    setPatternError(null);
    setIsAddRuleDialogOpen(true);
  }, []);

  const handleOpenEditRuleDialog = useCallback((index: number) => {
    setNewRule({ ...config.exclusionRules[index] });
    setEditingRuleIndex(index);
    setPatternError(null);
    setIsAddRuleDialogOpen(true);
  }, [config.exclusionRules]);

  const handleValidateRulePattern = useCallback(async () => {
    if (!newRule.pattern.trim()) {
      setPatternError('请输入正则表达式模式');
      return false;
    }

    if (onValidatePattern) {
      setIsValidatingPattern(true);
      try {
        const isValid = await onValidatePattern(newRule.pattern);
        if (!isValid) {
          setPatternError('无效的正则表达式');
          return false;
        }
      } catch {
        setPatternError('无效的正则表达式');
        return false;
      } finally {
        setIsValidatingPattern(false);
      }
    }

    setPatternError(null);
    return true;
  }, [newRule.pattern, onValidatePattern]);

  const handleSaveRule = useCallback(async () => {
    if (!newRule.name.trim() || !newRule.pattern.trim()) return;

    const isValid = await handleValidateRulePattern();
    if (!isValid) return;

    let newRules: ExclusionRule[];
    if (editingRuleIndex !== null) {
      newRules = [...config.exclusionRules];
      newRules[editingRuleIndex] = newRule;
    } else {
      newRules = [...config.exclusionRules, newRule];
    }

    onChange({ ...config, exclusionRules: newRules });
    setIsAddRuleDialogOpen(false);
    setNewRule(createEmptyRule());
    setEditingRuleIndex(null);
  }, [config, newRule, editingRuleIndex, handleValidateRulePattern, onChange]);

  const handleRemoveRule = useCallback((index: number) => {
    const newRules = config.exclusionRules.filter((_, i) => i !== index);
    onChange({ ...config, exclusionRules: newRules });
  }, [config, onChange]);

  const handleToggleRule = useCallback((index: number, enabled: boolean) => {
    const newRules = [...config.exclusionRules];
    newRules[index] = { ...newRules[index], enabled };
    onChange({ ...config, exclusionRules: newRules });
  }, [config, onChange]);

  // ============================================================================
  // Reset Handler
  // ============================================================================

  const handleReset = useCallback(() => {
    onChange(DEFAULT_PRIVACY_CONFIG);
  }, [onChange]);

  // ============================================================================
  // Render
  // ============================================================================

  return (
    <div className="space-y-4">
      {/* Data Retention Section */}
      <div className="border rounded-lg">
        <button
          type="button"
          className="w-full flex items-center justify-between p-3 hover:bg-muted/50 transition-colors"
          onClick={() => setDataRetentionExpanded(!dataRetentionExpanded)}
        >
          <div className="flex items-center gap-2">
            <Clock className="h-4 w-4 text-muted-foreground" />
            <span className="font-medium">数据保留策略</span>
            {config.dataRetention.autoCleanup && (
              <Badge variant="secondary" className="text-xs">自动清理</Badge>
            )}
          </div>
          {dataRetentionExpanded ? (
            <ChevronDown className="h-4 w-4 text-muted-foreground" />
          ) : (
            <ChevronRight className="h-4 w-4 text-muted-foreground" />
          )}
        </button>
        {dataRetentionExpanded && (
          <div className="p-4 pt-0 space-y-4 border-t">
            {/* Episodic Memory Days */}
            <div className="space-y-2">
              <Label htmlFor="episodic-days">情景记忆保留天数</Label>
              <div className="flex items-center gap-2">
                <Input
                  id="episodic-days"
                  type="number"
                  value={config.dataRetention.episodicMemoryDays}
                  onChange={handleEpisodicDaysChange}
                  min={0}
                  disabled={disabled}
                  className="w-32"
                />
                <span className="text-sm text-muted-foreground">天</span>
              </div>
              <p className="text-xs text-muted-foreground">
                0 表示永久保留。超过此天数的情景记忆将被自动清理。
              </p>
            </div>

            {/* Working Memory Hours */}
            <div className="space-y-2">
              <Label htmlFor="working-hours">工作记忆保留小时数</Label>
              <div className="flex items-center gap-2">
                <Input
                  id="working-hours"
                  type="number"
                  value={config.dataRetention.workingMemoryHours}
                  onChange={handleWorkingHoursChange}
                  min={0}
                  disabled={disabled}
                  className="w-32"
                />
                <span className="text-sm text-muted-foreground">小时</span>
              </div>
              <p className="text-xs text-muted-foreground">
                工作记忆用于存储临时上下文，过期后自动清理。
              </p>
            </div>

            {/* Auto Cleanup */}
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label htmlFor="auto-cleanup">自动清理过期数据</Label>
                <p className="text-xs text-muted-foreground">
                  启用后将定期清理过期的记忆数据
                </p>
              </div>
              <Switch
                id="auto-cleanup"
                checked={config.dataRetention.autoCleanup}
                onCheckedChange={handleAutoCleanupChange}
                disabled={disabled}
              />
            </div>
          </div>
        )}
      </div>

      {/* Sensitive Filter Section */}
      <div className="border rounded-lg">
        <button
          type="button"
          className="w-full flex items-center justify-between p-3 hover:bg-muted/50 transition-colors"
          onClick={() => setSensitiveFilterExpanded(!sensitiveFilterExpanded)}
        >
          <div className="flex items-center gap-2">
            <Filter className="h-4 w-4 text-muted-foreground" />
            <span className="font-medium">敏感信息过滤</span>
            {config.sensitiveFilter.enabled && (
              <Badge variant="default" className="text-xs bg-green-600">已启用</Badge>
            )}
          </div>
          {sensitiveFilterExpanded ? (
            <ChevronDown className="h-4 w-4 text-muted-foreground" />
          ) : (
            <ChevronRight className="h-4 w-4 text-muted-foreground" />
          )}
        </button>
        {sensitiveFilterExpanded && (
          <div className="p-4 pt-0 space-y-4 border-t">
            {/* Enable Toggle */}
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label htmlFor="filter-enabled">启用敏感信息自动过滤</Label>
                <p className="text-xs text-muted-foreground">
                  存储消息前自动脱敏敏感信息
                </p>
              </div>
              <Switch
                id="filter-enabled"
                checked={config.sensitiveFilter.enabled}
                onCheckedChange={handleFilterEnabledChange}
                disabled={disabled}
              />
            </div>

            {/* Filter Types */}
            {config.sensitiveFilter.enabled && (
              <div className="space-y-3">
                <Label>过滤类型</Label>
                <div className="grid grid-cols-2 gap-2">
                  {[
                    { key: 'filterEmail', label: '邮箱地址' },
                    { key: 'filterPhone', label: '电话号码' },
                    { key: 'filterIdCard', label: '身份证号' },
                    { key: 'filterBankCard', label: '银行卡号' },
                    { key: 'filterIpAddress', label: 'IP 地址' },
                  ].map(({ key, label }) => (
                    <label
                      key={key}
                      className="flex items-center gap-2 p-2 rounded border hover:bg-muted/50 cursor-pointer"
                    >
                      <input
                        type="checkbox"
                        checked={config.sensitiveFilter[key as keyof SensitiveDataFilter] as boolean}
                        onChange={(e) => handleFilterTypeChange(key as keyof SensitiveDataFilter, e.target.checked)}
                        disabled={disabled}
                        className="rounded"
                      />
                      <span className="text-sm">{label}</span>
                    </label>
                  ))}
                </div>
              </div>
            )}

            {/* Filter Test */}
            {onTestFilter && config.sensitiveFilter.enabled && (
              <div className="space-y-3 border-t pt-4">
                <Label className="flex items-center gap-2">
                  <TestTube className="h-4 w-4" />
                  过滤测试
                </Label>
                <div className="space-y-2">
                  <Input
                    placeholder="输入测试内容，如: 邮箱 test@example.com，电话 13812345678"
                    value={testContent}
                    onChange={(e) => setTestContent(e.target.value)}
                    disabled={disabled || isTesting}
                  />
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    onClick={handleTestFilter}
                    disabled={disabled || isTesting || !testContent.trim()}
                  >
                    {isTesting ? '测试中...' : '测试过滤'}
                  </Button>
                </div>
                {testResult && (
                  <div className="p-3 rounded-md bg-muted text-sm">
                    <p className="font-medium mb-1">过滤结果：</p>
                    <p className="whitespace-pre-wrap break-all">{testResult}</p>
                  </div>
                )}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Memory Sharing Scope Section */}
      <div className="border rounded-lg">
        <button
          type="button"
          className="w-full flex items-center justify-between p-3 hover:bg-muted/50 transition-colors"
          onClick={() => setMemorySharingExpanded(!memorySharingExpanded)}
        >
          <div className="flex items-center gap-2">
            <Share2 className="h-4 w-4 text-muted-foreground" />
            <span className="font-medium">记忆共享范围</span>
            <Badge variant="outline" className="text-xs">
              {MEMORY_SHARING_SCOPE_LABELS[config.memorySharingScope]}
            </Badge>
          </div>
          {memorySharingExpanded ? (
            <ChevronDown className="h-4 w-4 text-muted-foreground" />
          ) : (
            <ChevronRight className="h-4 w-4 text-muted-foreground" />
          )}
        </button>
        {memorySharingExpanded && (
          <div className="p-4 pt-0 space-y-3 border-t">
            {(Object.entries(MEMORY_SHARING_SCOPE_LABELS) as [MemorySharingScope, string][]).map(
              ([value, label]) => (
                <label
                  key={value}
                  className={`flex items-start gap-3 p-3 rounded-md cursor-pointer border ${
                    config.memorySharingScope === value
                      ? 'border-primary bg-primary/5'
                      : 'border-transparent hover:bg-muted'
                  }`}
                  onClick={() => handleMemorySharingScopeChange(value)}
                >
                  <input
                    type="radio"
                    name="memorySharingScope"
                    value={value}
                    checked={config.memorySharingScope === value}
                    onChange={() => handleMemorySharingScopeChange(value)}
                    disabled={disabled}
                    className="mt-1"
                  />
                  <div>
                    <div className="font-medium text-sm">{label}</div>
                    <div className="text-xs text-muted-foreground">
                      {MEMORY_SHARING_SCOPE_DESCRIPTIONS[value]}
                    </div>
                  </div>
                </label>
              )
            )}
          </div>
        )}
      </div>

      {/* Exclusion Rules Section */}
      <div className="border rounded-lg">
        <button
          type="button"
          className="w-full flex items-center justify-between p-3 hover:bg-muted/50 transition-colors"
          onClick={() => setExclusionRulesExpanded(!exclusionRulesExpanded)}
        >
          <div className="flex items-center gap-2">
            <Eye className="h-4 w-4 text-muted-foreground" />
            <span className="font-medium">排除数据规则</span>
            {config.exclusionRules.length > 0 && (
              <Badge variant="secondary" className="text-xs">
                {config.exclusionRules.filter(r => r.enabled).length} 条规则
              </Badge>
            )}
          </div>
          {exclusionRulesExpanded ? (
            <ChevronDown className="h-4 w-4 text-muted-foreground" />
          ) : (
            <ChevronRight className="h-4 w-4 text-muted-foreground" />
          )}
        </button>
        {exclusionRulesExpanded && (
          <div className="p-4 pt-0 space-y-4 border-t">
            {/* Rules List */}
            <div className="flex items-center justify-between">
              <Label>
                已配置的排除规则 ({config.exclusionRules.length})
              </Label>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={handleOpenAddRuleDialog}
                disabled={disabled}
              >
                <Plus className="h-4 w-4 mr-1" />
                添加规则
              </Button>
            </div>

            {config.exclusionRules.length > 0 ? (
              <div className="border rounded-md divide-y">
                {config.exclusionRules.map((rule, index) => (
                  <div
                    key={index}
                    className="flex items-center justify-between p-3"
                  >
                    <div className="flex items-center gap-3 flex-1 min-w-0">
                      <Switch
                        checked={rule.enabled}
                        onCheckedChange={(checked) => handleToggleRule(index, checked)}
                        disabled={disabled}
                      />
                      <div className="min-w-0">
                        <div className="font-medium text-sm truncate">{rule.name}</div>
                        <code className="text-xs text-muted-foreground truncate block">
                          {rule.pattern}
                        </code>
                      </div>
                    </div>
                    <div className="flex items-center gap-1 ml-2">
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={() => handleOpenEditRuleDialog(index)}
                        disabled={disabled}
                      >
                        编辑
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={() => handleRemoveRule(index)}
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
                暂无排除规则。匹配规则的内容将不会被存储到长期记忆。
              </div>
            )}

            <p className="text-xs text-muted-foreground">
              排除规则使用正则表达式匹配，匹配成功的内容将不会被存储到记忆中。
            </p>
          </div>
        )}
      </div>

      {/* Reset Button */}
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

      {/* Add/Edit Rule Dialog */}
      <Dialog open={isAddRuleDialogOpen} onOpenChange={setIsAddRuleDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {editingRuleIndex !== null ? '编辑排除规则' : '添加排除规则'}
            </DialogTitle>
          </DialogHeader>

          <div className="space-y-4 py-4">
            {/* Rule Name */}
            <div className="space-y-2">
              <Label htmlFor="rule-name">规则名称 *</Label>
              <Input
                id="rule-name"
                value={newRule.name}
                onChange={(e) => setNewRule({ ...newRule, name: e.target.value })}
                placeholder="例如: 排除密码"
                disabled={disabled}
              />
            </div>

            {/* Rule Description */}
            <div className="space-y-2">
              <Label htmlFor="rule-description">规则描述</Label>
              <Input
                id="rule-description"
                value={newRule.description || ''}
                onChange={(e) => setNewRule({ ...newRule, description: e.target.value })}
                placeholder="可选的规则描述"
                disabled={disabled}
              />
            </div>

            {/* Rule Pattern */}
            <div className="space-y-2">
              <Label htmlFor="rule-pattern">正则表达式 *</Label>
              <Input
                id="rule-pattern"
                value={newRule.pattern}
                onChange={(e) => {
                  setNewRule({ ...newRule, pattern: e.target.value });
                  setPatternError(null);
                }}
                placeholder="例如: password\s*[:=]\s*\S+"
                disabled={disabled}
                className={patternError ? 'border-destructive' : ''}
              />
              {patternError && (
                <p className="text-sm text-destructive flex items-center gap-1">
                  <AlertCircle className="h-4 w-4" />
                  {patternError}
                </p>
              )}
              <p className="text-xs text-muted-foreground">
                匹配此模式的内容将不会被存储到记忆中
              </p>
            </div>

            {/* Enable Toggle */}
            <div className="flex items-center justify-between">
              <Label htmlFor="rule-enabled">启用规则</Label>
              <Switch
                id="rule-enabled"
                checked={newRule.enabled}
                onCheckedChange={(checked) => setNewRule({ ...newRule, enabled: checked })}
                disabled={disabled}
              />
            </div>
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setIsAddRuleDialogOpen(false)}
            >
              取消
            </Button>
            <Button
              type="button"
              onClick={handleSaveRule}
              disabled={!newRule.name.trim() || !newRule.pattern.trim() || isValidatingPattern}
            >
              {editingRuleIndex !== null ? '保存' : '添加规则'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
};

export default PrivacyConfigForm;