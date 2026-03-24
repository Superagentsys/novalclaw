/**
 * ChannelConfigForm 组件
 *
 * 渠道配置表单基础组件，包含通用配置字段和行为配置
 *
 * [Source: Story 6.8 - 渠道配置界面]
 */

import { type FC, useState } from 'react';
import { Eye, EyeOff, Loader2, CheckCircle2, XCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  type ChannelKind,
  type ChannelBehaviorConfig,
  type TriggerKeyword,
  type ConfigField,
  CHANNEL_TYPE_DEFINITIONS,
  RESPONSE_STYLE_LABELS,
  MATCH_TYPE_LABELS,
  createDefaultTriggerKeyword,
  maskSensitiveValue,
} from '@/types/channel';

export interface ChannelConfigFormProps {
  /** Channel kind being configured */
  kind: ChannelKind;
  /** Channel name */
  name: string;
  onChangeName: (name: string) => void;
  /** Whether channel is enabled */
  enabled: boolean;
  onChangeEnabled: (enabled: boolean) => void;
  /** Behavior configuration */
  behavior: ChannelBehaviorConfig;
  onChangeBehavior: (behavior: ChannelBehaviorConfig) => void;
  /** Credentials data (varies by channel type) */
  credentials: Record<string, unknown>;
  onChangeCredentials: (credentials: Record<string, unknown>) => void;
  /** Initial credentials for edit mode (masked) */
  initialCredentials?: Record<string, unknown>;
  /** Test connection callback */
  onTestConnection?: () => Promise<boolean>;
  /** Is testing connection */
  isTestingConnection?: boolean;
  /** Test connection result */
  testConnectionResult?: 'success' | 'error' | null;
  /** Whether form is in edit mode */
  isEditMode?: boolean;
  /** Validation errors */
  errors?: Record<string, string>;
}

/**
 * ConfigFieldInput - Renders a config field based on its type
 */
interface ConfigFieldInputProps {
  field: ConfigField;
  value: unknown;
  onChange: (value: unknown) => void;
  error?: string;
  showPassword: boolean;
  onTogglePassword: () => void;
  isEditMode?: boolean;
}

const ConfigFieldInput: FC<ConfigFieldInputProps> = ({
  field,
  value,
  onChange,
  error,
  showPassword,
  onTogglePassword,
  isEditMode,
}) => {
  const inputId = `field-${field.name}`;

  // For password fields in edit mode, show masked value
  const displayValue =
    field.type === 'password' && isEditMode && value === undefined
      ? maskSensitiveValue(String(value || ''))
      : String(value || '');

  switch (field.type) {
    case 'password':
      return (
        <div className="space-y-2">
          <Label htmlFor={inputId}>
            {field.label}
            {field.required && <span className="text-red-500 ml-1">*</span>}
          </Label>
          <div className="relative">
            <Input
              id={inputId}
              type={showPassword ? 'text' : 'password'}
              value={displayValue || ''}
              onChange={(e) => onChange(e.target.value)}
              placeholder={field.placeholder}
              className={error ? 'border-red-500' : ''}
            />
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="absolute right-0 top-0 h-full px-3"
              onClick={onTogglePassword}
            >
              {showPassword ? (
                <EyeOff className="h-4 w-4" />
              ) : (
                <Eye className="h-4 w-4" />
              )}
            </Button>
          </div>
          {field.helpText && (
            <p className="text-xs text-muted-foreground">{field.helpText}</p>
          )}
          {error && <p className="text-xs text-red-500">{error}</p>}
        </div>
      );

    case 'number':
      return (
        <div className="space-y-2">
          <Label htmlFor={inputId}>
            {field.label}
            {field.required && <span className="text-red-500 ml-1">*</span>}
          </Label>
          <Input
            id={inputId}
            type="number"
            value={(value as number | undefined) ?? (field.defaultValue as number | undefined) ?? ''}
            onChange={(e) => onChange(parseInt(e.target.value, 10))}
            placeholder={field.placeholder}
            min={field.min}
            max={field.max}
            className={error ? 'border-red-500' : ''}
          />
          {field.helpText && (
            <p className="text-xs text-muted-foreground">{field.helpText}</p>
          )}
          {error && <p className="text-xs text-red-500">{error}</p>}
        </div>
      );

    case 'checkbox':
      return (
        <div className="flex items-center justify-between">
          <div>
            <Label htmlFor={inputId}>{field.label}</Label>
            {field.helpText && (
              <p className="text-xs text-muted-foreground">{field.helpText}</p>
            )}
          </div>
          <Switch
            id={inputId}
            checked={value as boolean}
            onCheckedChange={onChange}
          />
        </div>
      );

    case 'select':
      return (
        <div className="space-y-2">
          <Label htmlFor={inputId}>
            {field.label}
            {field.required && <span className="text-red-500 ml-1">*</span>}
          </Label>
          <Select
            value={value as string}
            onValueChange={onChange}
          >
            <SelectTrigger id={inputId}>
              <SelectValue placeholder={field.placeholder} />
            </SelectTrigger>
            <SelectContent>
              {field.options?.map((opt) => (
                <SelectItem key={opt.value} value={opt.value}>
                  {opt.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {field.helpText && (
            <p className="text-xs text-muted-foreground">{field.helpText}</p>
          )}
          {error && <p className="text-xs text-red-500">{error}</p>}
        </div>
      );

    case 'textarea':
      return (
        <div className="space-y-2">
          <Label htmlFor={inputId}>
            {field.label}
            {field.required && <span className="text-red-500 ml-1">*</span>}
          </Label>
          <textarea
            id={inputId}
            value={(value as string) || ''}
            onChange={(e) => onChange(e.target.value)}
            placeholder={field.placeholder}
            className={`flex min-h-[80px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 ${error ? 'border-red-500' : ''}`}
          />
          {field.helpText && (
            <p className="text-xs text-muted-foreground">{field.helpText}</p>
          )}
          {error && <p className="text-xs text-red-500">{error}</p>}
        </div>
      );

    case 'text':
    default:
      return (
        <div className="space-y-2">
          <Label htmlFor={inputId}>
            {field.label}
            {field.required && <span className="text-red-500 ml-1">*</span>}
          </Label>
          <Input
            id={inputId}
            type="text"
            value={(value as string) || ''}
            onChange={(e) => onChange(e.target.value)}
            placeholder={field.placeholder}
            className={error ? 'border-red-500' : ''}
          />
          {field.helpText && (
            <p className="text-xs text-muted-foreground">{field.helpText}</p>
          )}
          {error && <p className="text-xs text-red-500">{error}</p>}
        </div>
      );
  }
};

/**
 * TriggerKeywordInput - Component for managing trigger keywords
 */
interface TriggerKeywordInputProps {
  keywords: TriggerKeyword[];
  onChange: (keywords: TriggerKeyword[]) => void;
}

const TriggerKeywordInput: FC<TriggerKeywordInputProps> = ({
  keywords,
  onChange,
}) => {
  const handleAdd = () => {
    onChange([...keywords, createDefaultTriggerKeyword()]);
  };

  const handleRemove = (index: number) => {
    onChange(keywords.filter((_, i) => i !== index));
  };

  const handleChange = (index: number, updates: Partial<TriggerKeyword>) => {
    onChange(
      keywords.map((keyword, i) =>
        i === index ? { ...keyword, ...updates } : keyword
      )
    );
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <Label>触发关键词</Label>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={handleAdd}
        >
          添加关键词
        </Button>
      </div>

      {keywords.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          暂无触发关键词，代理将响应所有消息
        </p>
      ) : (
        <div className="space-y-3">
          {keywords.map((keyword, index) => (
            <div
              key={index}
              className="flex items-start gap-2 p-3 border rounded-lg"
            >
              <div className="flex-1 grid grid-cols-3 gap-2">
                <Input
                  value={keyword.keyword}
                  onChange={(e) =>
                    handleChange(index, { keyword: e.target.value })
                  }
                  placeholder="关键词"
                  className="col-span-1"
                />
                <Select
                  value={keyword.matchType}
                  onValueChange={(value) =>
                    handleChange(index, {
                      matchType: value as TriggerKeyword['matchType'],
                    })
                  }
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {Object.entries(MATCH_TYPE_LABELS).map(([value, label]) => (
                      <SelectItem key={value} value={value}>
                        {label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <div className="flex items-center gap-2">
                  <Switch
                    checked={keyword.caseSensitive}
                    onCheckedChange={(checked) =>
                      handleChange(index, { caseSensitive: checked })
                    }
                  />
                  <span className="text-sm">区分大小写</span>
                </div>
              </div>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={() => handleRemove(index)}
              >
                移除
              </Button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};

/**
 * ChannelConfigForm component
 */
export const ChannelConfigForm: FC<ChannelConfigFormProps> = ({
  kind,
  name,
  onChangeName,
  enabled,
  onChangeEnabled,
  behavior,
  onChangeBehavior,
  credentials,
  onChangeCredentials,
  onTestConnection,
  isTestingConnection,
  testConnectionResult,
  isEditMode,
  errors = {},
}) => {
  const [showPasswords, setShowPasswords] = useState<Record<string, boolean>>({});
  const [activeTab, setActiveTab] = useState<'credentials' | 'behavior'>('credentials');

  // Get channel type definition
  const typeDefinition = CHANNEL_TYPE_DEFINITIONS.find(
    (def) => def.kind === kind
  );

  const togglePasswordVisibility = (fieldName: string) => {
    setShowPasswords((prev) => ({
      ...prev,
      [fieldName]: !prev[fieldName],
    }));
  };

  const handleCredentialChange = (fieldName: string, value: unknown) => {
    onChangeCredentials({
      ...credentials,
      [fieldName]: value,
    });
  };

  return (
    <div className="space-y-6">
      {/* Tab navigation */}
      <div className="flex border-b">
        <button
          type="button"
          className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
            activeTab === 'credentials'
              ? 'border-primary text-primary'
              : 'border-transparent text-muted-foreground hover:text-foreground'
          }`}
          onClick={() => setActiveTab('credentials')}
        >
          连接配置
        </button>
        <button
          type="button"
          className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
            activeTab === 'behavior'
              ? 'border-primary text-primary'
              : 'border-transparent text-muted-foreground hover:text-foreground'
          }`}
          onClick={() => setActiveTab('behavior')}
        >
          行为配置
        </button>
      </div>

      {/* Credentials tab */}
      {activeTab === 'credentials' && (
        <div className="space-y-4">
          {/* Basic info */}
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="channel-name">
                渠道名称
                <span className="text-red-500 ml-1">*</span>
              </Label>
              <Input
                id="channel-name"
                value={name}
                onChange={(e) => onChangeName(e.target.value)}
                placeholder="输入渠道名称"
                className={errors.name ? 'border-red-500' : ''}
              />
              {errors.name && (
                <p className="text-xs text-red-500">{errors.name}</p>
              )}
            </div>

            <div className="flex items-center justify-between">
              <Label htmlFor="channel-enabled">启用渠道</Label>
              <Switch
                id="channel-enabled"
                checked={enabled}
                onCheckedChange={onChangeEnabled}
              />
            </div>
          </div>

          {/* Type-specific config fields */}
          {typeDefinition && (
            <div className="space-y-4 pt-4 border-t">
              <h4 className="font-medium">连接凭据</h4>
              {typeDefinition.configFields.map((field) => (
                <ConfigFieldInput
                  key={field.name}
                  field={field}
                  value={credentials[field.name]}
                  onChange={(value) => handleCredentialChange(field.name, value)}
                  error={errors[field.name]}
                  showPassword={showPasswords[field.name] || false}
                  onTogglePassword={() => togglePasswordVisibility(field.name)}
                  isEditMode={isEditMode}
                />
              ))}
            </div>
          )}

          {/* Test connection button */}
          {onTestConnection && (
            <div className="pt-4 border-t">
              <Button
                type="button"
                variant="outline"
                onClick={onTestConnection}
                disabled={isTestingConnection}
              >
                {isTestingConnection ? (
                  <>
                    <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                    测试中...
                  </>
                ) : (
                  <>
                    {testConnectionResult === 'success' && (
                      <CheckCircle2 className="h-4 w-4 mr-2 text-green-500" />
                    )}
                    {testConnectionResult === 'error' && (
                      <XCircle className="h-4 w-4 mr-2 text-red-500" />
                    )}
                    测试连接
                  </>
                )}
              </Button>
            </div>
          )}
        </div>
      )}

      {/* Behavior tab */}
      {activeTab === 'behavior' && (
        <div className="space-y-4">
          {/* Response style */}
          <div className="space-y-2">
            <Label>响应风格</Label>
            <Select
              value={behavior.responseStyle}
              onValueChange={(value) =>
                onChangeBehavior({
                  ...behavior,
                  responseStyle: value as typeof behavior.responseStyle,
                })
              }
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {Object.entries(RESPONSE_STYLE_LABELS).map(([value, label]) => (
                  <SelectItem key={value} value={value}>
                    {label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {/* Max response length */}
          <div className="space-y-2">
            <Label htmlFor="max-length">最大响应长度 (0 = 无限制)</Label>
            <Input
              id="max-length"
              type="number"
              value={behavior.maxResponseLength}
              onChange={(e) =>
                onChangeBehavior({
                  ...behavior,
                  maxResponseLength: parseInt(e.target.value, 10) || 0,
                })
              }
              min={0}
            />
          </div>

          {/* Trigger keywords */}
          <TriggerKeywordInput
            keywords={behavior.triggerKeywords}
            onChange={(keywords) =>
              onChangeBehavior({ ...behavior, triggerKeywords: keywords })
            }
          />
        </div>
      )}
    </div>
  );
};

export default ChannelConfigForm;