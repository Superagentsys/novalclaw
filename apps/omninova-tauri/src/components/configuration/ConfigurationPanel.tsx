/**
 * ConfigurationPanel Component
 *
 * Main configuration panel that integrates all agent settings with tabs
 * for basic settings, advanced settings, and skill management.
 *
 * [Source: Story 7.7 - ConfigurationPanel 组件]
 */

import { type FC, useState, useCallback } from 'react';
import {
  Settings2,
  Sliders,
  Wrench,
  Eye,
  RotateCcw,
  Save,
  X,
  AlertCircle,
  ChevronDown,
  ChevronRight,
} from 'lucide-react';
import { toast } from 'sonner';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription } from '@/components/ui/alert';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { AgentStyleConfigForm } from '@/components/agent/AgentStyleConfigForm';
import { ContextWindowConfigForm } from '@/components/agent/ContextWindowConfigForm';
import { TriggerKeywordsConfigForm } from '@/components/agent/TriggerKeywordsConfigForm';
import { PrivacyConfigForm } from '@/components/agent/PrivacyConfigForm';
import { SkillManagementPanel } from '@/components/skills/SkillManagementPanel';
import { useAgentConfiguration } from '@/hooks/useAgentConfiguration';
import {
  type AgentConfiguration,
  type ConfigChange,
} from '@/types/configuration';

export interface ConfigurationPanelProps {
  /** Agent ID */
  agentId: string;
  /** Initial configuration */
  initialConfig?: AgentConfiguration;
  /** Configuration save callback */
  onSave?: (config: AgentConfiguration) => Promise<void>;
  /** Configuration change callback */
  onChange?: (config: AgentConfiguration, changes: ConfigChange[]) => void;
  /** Whether the panel is disabled */
  disabled?: boolean;
}

/**
 * ConfigurationPanel component
 */
export const ConfigurationPanel: FC<ConfigurationPanelProps> = ({
  agentId,
  initialConfig,
  onSave,
  onChange,
  disabled = false,
}) => {
  // Advanced settings expanded state
  const [advancedExpanded, setAdvancedExpanded] = useState(false);

  // Reset confirmation dialog state
  const [showResetDialog, setShowResetDialog] = useState(false);

  // Preview dialog state
  const [showPreviewDialog, setShowPreviewDialog] = useState(false);

  // Use the configuration hook
  const {
    config,
    isDirty,
    changes,
    isValid,
    validationResult,
    isLoading,
    isSaving,
    error,

    setStyleConfig,
    setContextConfig,
    setTriggerConfig,
    setPrivacyConfig,
    setSkillConfig,

    save,
    cancel,
    resetToDefaults,
  } = useAgentConfiguration({
    agentId,
    initialConfig,
  });

  // Handle save
  const handleSave = useCallback(async () => {
    const success = await save();
    if (success && onSave) {
      await onSave(config);
    }
  }, [save, onSave, config]);

  // Handle cancel
  const handleCancel = useCallback(() => {
    cancel();
  }, [cancel]);

  // Handle reset
  const handleReset = useCallback(() => {
    resetToDefaults();
    setShowResetDialog(false);
    toast.success('配置已重置为默认值');
  }, [resetToDefaults]);

  // Handle change notifications
  const handleChange = useCallback(
    (newConfig: AgentConfiguration, newChanges: ConfigChange[]) => {
      if (onChange) {
        onChange(newConfig, newChanges);
      }
    },
    [onChange]
  );

  // Loading state
  if (isLoading) {
    return (
      <Card>
        <CardContent className="py-12">
          <div className="flex items-center justify-center">
            <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary" />
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-4">
      {/* Error Alert */}
      {error && (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      {/* Validation Errors Summary */}
      {!isValid && isDirty && (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>
            <span className="font-medium">配置验证失败：</span>
            <ul className="mt-1 list-disc list-inside text-sm">
              {validationResult.errors.slice(0, 3).map((err, i) => (
                <li key={i}>{err.message}</li>
              ))}
              {validationResult.errors.length > 3 && (
                <li>还有 {validationResult.errors.length - 3} 个错误</li>
              )}
            </ul>
          </AlertDescription>
        </Alert>
      )}

      {/* Main Tabs */}
      <Tabs defaultValue="basic" className="w-full">
        <TabsList className="grid w-full grid-cols-3">
          <TabsTrigger value="basic" className="gap-1.5">
            <Settings2 className="h-4 w-4" />
            基础设置
          </TabsTrigger>
          <TabsTrigger value="advanced" className="gap-1.5">
            <Sliders className="h-4 w-4" />
            高级设置
          </TabsTrigger>
          <TabsTrigger value="skills" className="gap-1.5">
            <Wrench className="h-4 w-4" />
            技能管理
          </TabsTrigger>
        </TabsList>

        {/* Basic Settings Tab */}
        <TabsContent value="basic" className="mt-4">
          <Card>
            <CardHeader>
              <CardTitle className="text-lg">响应风格</CardTitle>
            </CardHeader>
            <CardContent>
              <AgentStyleConfigForm
                config={config.styleConfig}
                onChange={setStyleConfig}
                disabled={disabled || isSaving}
              />
            </CardContent>
          </Card>

          {/* Quick Access to Advanced Settings */}
          <div className="mt-4">
            <button
              type="button"
              onClick={() => setAdvancedExpanded(!advancedExpanded)}
              className="w-full flex items-center justify-between p-4 border rounded-lg hover:bg-muted/50 transition-colors"
            >
              <div className="flex items-center gap-2">
                <Sliders className="h-5 w-5 text-muted-foreground" />
                <span className="font-medium">高级设置</span>
                <Badge variant="secondary" className="text-xs">
                  上下文窗口 · 触发关键词 · 隐私设置
                </Badge>
              </div>
              {advancedExpanded ? (
                <ChevronDown className="h-4 w-4 text-muted-foreground" />
              ) : (
                <ChevronRight className="h-4 w-4 text-muted-foreground" />
              )}
            </button>

            {advancedExpanded && (
              <div className="mt-4 space-y-4">
                {/* Context Window Config */}
                <Card>
                  <CardHeader>
                    <CardTitle className="text-lg">上下文窗口</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <ContextWindowConfigForm
                      config={config.contextConfig}
                      onChange={setContextConfig}
                      disabled={disabled || isSaving}
                    />
                  </CardContent>
                </Card>

                {/* Trigger Keywords Config */}
                <Card>
                  <CardHeader>
                    <CardTitle className="text-lg">触发关键词</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <TriggerKeywordsConfigForm
                      config={config.triggerConfig}
                      onChange={setTriggerConfig}
                      disabled={disabled || isSaving}
                    />
                  </CardContent>
                </Card>

                {/* Privacy Config */}
                <Card>
                  <CardHeader>
                    <CardTitle className="text-lg">隐私设置</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <PrivacyConfigForm
                      config={config.privacyConfig}
                      onChange={setPrivacyConfig}
                      disabled={disabled || isSaving}
                    />
                  </CardContent>
                </Card>
              </div>
            )}
          </div>
        </TabsContent>

        {/* Advanced Settings Tab */}
        <TabsContent value="advanced" className="mt-4 space-y-4">
          {/* Context Window Config */}
          <Card>
            <CardHeader>
              <CardTitle className="text-lg">上下文窗口</CardTitle>
            </CardHeader>
            <CardContent>
              <ContextWindowConfigForm
                config={config.contextConfig}
                onChange={setContextConfig}
                disabled={disabled || isSaving}
              />
            </CardContent>
          </Card>

          {/* Trigger Keywords Config */}
          <Card>
            <CardHeader>
              <CardTitle className="text-lg">触发关键词</CardTitle>
            </CardHeader>
            <CardContent>
              <TriggerKeywordsConfigForm
                config={config.triggerConfig}
                onChange={setTriggerConfig}
                disabled={disabled || isSaving}
              />
            </CardContent>
          </Card>

          {/* Privacy Config */}
          <Card>
            <CardHeader>
              <CardTitle className="text-lg">隐私设置</CardTitle>
            </CardHeader>
            <CardContent>
              <PrivacyConfigForm
                config={config.privacyConfig}
                onChange={setPrivacyConfig}
                disabled={disabled || isSaving}
              />
            </CardContent>
          </Card>
        </TabsContent>

        {/* Skills Tab */}
        <TabsContent value="skills" className="mt-4">
          <SkillManagementPanel
            agentId={agentId}
            initialConfig={config.skillConfig}
            onConfigChange={setSkillConfig}
          />
        </TabsContent>
      </Tabs>

      {/* Action Bar */}
      <div className="flex items-center justify-between pt-4 border-t">
        <div className="flex items-center gap-2">
          {/* Preview Button */}
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => setShowPreviewDialog(true)}
            disabled={disabled || !isDirty}
          >
            <Eye className="h-4 w-4 mr-2" />
            预览更改
          </Button>

          {/* Reset Button */}
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => setShowResetDialog(true)}
            disabled={disabled}
          >
            <RotateCcw className="h-4 w-4 mr-2" />
            重置默认
          </Button>
        </div>

        <div className="flex items-center gap-2">
          {/* Dirty indicator */}
          {isDirty && (
            <Badge variant="secondary" className="mr-2">
              有未保存的更改
            </Badge>
          )}

          {/* Cancel Button */}
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={handleCancel}
            disabled={disabled || !isDirty}
          >
            <X className="h-4 w-4 mr-2" />
            取消
          </Button>

          {/* Save Button */}
          <Button
            type="button"
            size="sm"
            onClick={handleSave}
            disabled={disabled || !isDirty || !isValid}
            loading={isSaving}
          >
            <Save className="h-4 w-4 mr-2" />
            保存
          </Button>
        </div>
      </div>

      {/* Reset Confirmation Dialog */}
      <AlertDialog open={showResetDialog} onOpenChange={setShowResetDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>重置配置</AlertDialogTitle>
            <AlertDialogDescription>
              确定要将所有配置重置为默认值吗？此操作不可撤销。
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction onClick={handleReset}>
              确认重置
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Preview Dialog */}
      <AlertDialog open={showPreviewDialog} onOpenChange={setShowPreviewDialog}>
        <AlertDialogContent className="max-w-lg">
          <AlertDialogHeader>
            <AlertDialogTitle>配置变更预览</AlertDialogTitle>
            <AlertDialogDescription>
              以下是将要保存的配置更改：
            </AlertDialogDescription>
          </AlertDialogHeader>
          <div className="max-h-64 overflow-y-auto">
            {changes.length > 0 ? (
              <div className="space-y-2">
                {changes.map((change, index) => (
                  <div
                    key={index}
                    className="p-2 bg-muted rounded text-sm"
                  >
                    <div className="font-mono text-xs text-muted-foreground mb-1">
                      {change.path}
                    </div>
                    <div className="flex items-center gap-2">
                      <span className="text-red-500 line-through">
                        {JSON.stringify(change.oldValue)}
                      </span>
                      <span>→</span>
                      <span className="text-green-500">
                        {JSON.stringify(change.newValue)}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <p className="text-sm text-muted-foreground">没有配置更改</p>
            )}
          </div>
          <AlertDialogFooter>
            <AlertDialogCancel>关闭</AlertDialogCancel>
            <AlertDialogAction
              onClick={async () => {
                setShowPreviewDialog(false);
                await handleSave();
              }}
              disabled={!isValid}
            >
              确认保存
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
};

export default ConfigurationPanel;