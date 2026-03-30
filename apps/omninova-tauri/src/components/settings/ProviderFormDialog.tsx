/**
 * Provider Form Dialog Component
 *
 * Dialog for adding and editing provider configurations
 *
 * [Source: Story 3.6 - Provider 配置界面]
 */

import * as React from 'react';
import { useState, useEffect, useCallback } from 'react';
import {
  Eye,
  EyeOff,
  Loader2,
  Plus,
  Cloud,
  HardDrive,
  Settings2,
  RefreshCw,
  Cpu,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type {
  ProviderType,
  NewProviderConfig,
  ProviderConfigUpdate,
  ProviderPreset,
  ProviderWithStatus,
  ApiProtocol,
} from '@/types/provider';
import { PROVIDER_PRESETS, getProviderPreset } from '@/types/provider';

// ============================================================================
// Types
// ============================================================================

interface ProviderFormDialogProps {
  /** Dialog open state */
  open: boolean;
  /** Open change callback */
  onOpenChange: (open: boolean) => void;
  /** Provider to edit (null for new) */
  provider?: ProviderWithStatus | null;
  /** Submit callback */
  onSubmit: (config: NewProviderConfig | ProviderConfigUpdate) => Promise<boolean>;
  /** Test connection callback (for edit mode) */
  onTestConnection?: () => Promise<boolean>;
  /** Is loading state */
  isLoading?: boolean;
}

interface FormData {
  name: string;
  providerType: ProviderType;
  apiKey: string;
  baseUrl: string;
  defaultModel: string;
  isDefault: boolean;
  apiProtocol: ApiProtocol;
}

interface FormErrors {
  name?: string;
  apiKey?: string;
  baseUrl?: string;
  defaultModel?: string;
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Validate form data
 */
function validateForm(data: FormData, preset: ProviderPreset | undefined): FormErrors {
  const errors: FormErrors = {};

  // Name validation
  if (!data.name.trim()) {
    errors.name = '请输入提供商名称';
  } else if (data.name.length > 100) {
    errors.name = '名称不能超过 100 个字符';
  }

  // API key validation
  if (preset?.requiresApiKey && !data.apiKey.trim()) {
    errors.apiKey = '此提供商需要 API 密钥';
  }

  // Base URL validation for custom providers
  if (data.providerType === 'custom' && !data.baseUrl.trim()) {
    errors.baseUrl = '自定义提供商需要指定 Base URL';
  }

  return errors;
}

// ============================================================================
// Component
// ============================================================================

/**
 * Provider Form Dialog Component
 *
 * Provides a form for adding new providers or editing existing ones.
 * Includes provider type selection, API key input, and connection testing.
 */
export function ProviderFormDialog({
  open,
  onOpenChange,
  provider,
  onSubmit,
  onTestConnection,
  isLoading = false,
}: ProviderFormDialogProps): React.ReactElement {
  const isEditMode = !!provider;

  // Form state
  const [formData, setFormData] = useState<FormData>({
    name: '',
    providerType: 'openai',
    apiKey: '',
    baseUrl: '',
    defaultModel: '',
    isDefault: false,
    apiProtocol: 'openai',
  });
  const [errors, setErrors] = useState<FormErrors>({});
  const [showApiKey, setShowApiKey] = useState(false);
  const [isTestingConnection, setIsTestingConnection] = useState(false);

  // Get current preset
  const currentPreset = getProviderPreset(formData.providerType);

  // Initialize form when provider changes
  useEffect(() => {
    if (provider) {
      setFormData({
        name: provider.name,
        providerType: provider.providerType,
        apiKey: '', // Don't populate API key for security
        baseUrl: provider.baseUrl || '',
        defaultModel: provider.defaultModel || '',
        isDefault: provider.isDefault,
        apiProtocol: provider.apiProtocol || 'openai',
      });
    } else {
      // Reset for new provider
      setFormData({
        name: '',
        providerType: 'openai',
        apiKey: '',
        baseUrl: '',
        defaultModel: '',
        isDefault: false,
        apiProtocol: 'openai',
      });
    }
    setErrors({});
    setShowApiKey(false);
  }, [provider, open]);

  // Update base URL when provider type changes (only for new providers)
  useEffect(() => {
    if (!isEditMode && currentPreset?.defaultBaseUrl) {
      setFormData((prev) => ({
        ...prev,
        baseUrl: currentPreset.defaultBaseUrl || '',
        defaultModel: currentPreset.popularModels[0] || '',
      }));
    }
  }, [formData.providerType, isEditMode, currentPreset]);

  // Handle input change
  const handleInputChange = useCallback(
    (field: keyof FormData, value: string | boolean) => {
      setFormData((prev) => ({ ...prev, [field]: value }));
      // Clear error when user types
      if (errors[field as keyof FormErrors]) {
        setErrors((prev) => ({ ...prev, [field]: undefined }));
      }
    },
    [errors]
  );

  // Handle provider type change
  const handleProviderTypeChange = useCallback((value: ProviderType) => {
    const preset = getProviderPreset(value);
    setFormData((prev) => ({
      ...prev,
      providerType: value,
      baseUrl: preset?.defaultBaseUrl || '',
      defaultModel: preset?.popularModels[0] || '',
    }));
    setErrors({});
  }, []);

  // Handle test connection
  const handleTestConnection = useCallback(async () => {
    if (!onTestConnection) return;

    setIsTestingConnection(true);
    try {
      await onTestConnection();
    } finally {
      setIsTestingConnection(false);
    }
  }, [onTestConnection]);

  // Handle form submit
  const handleSubmit = useCallback(async () => {
    // Validate
    const validationErrors = validateForm(formData, currentPreset);
    if (Object.keys(validationErrors).length > 0) {
      setErrors(validationErrors);
      return;
    }

    // Build config
    if (isEditMode) {
      // Update existing provider
      const update: ProviderConfigUpdate = {
        name: formData.name,
        baseUrl: formData.baseUrl || undefined,
        defaultModel: formData.defaultModel || undefined,
        isDefault: formData.isDefault,
        apiProtocol: formData.providerType === 'custom' ? formData.apiProtocol : undefined,
      };

      // Only include API key if changed
      if (formData.apiKey.trim()) {
        update.apiKey = formData.apiKey;
      }

      const success = await onSubmit(update);
      if (success) {
        onOpenChange(false);
      }
    } else {
      // Create new provider
      const newConfig: NewProviderConfig = {
        name: formData.name,
        providerType: formData.providerType,
        apiKey: formData.apiKey || undefined,
        baseUrl: formData.baseUrl || undefined,
        defaultModel: formData.defaultModel || undefined,
        isDefault: formData.isDefault,
        apiProtocol: formData.providerType === 'custom' ? formData.apiProtocol : undefined,
      };

      const success = await onSubmit(newConfig);
      if (success) {
        onOpenChange(false);
      }
    }
  }, [formData, currentPreset, isEditMode, onSubmit, onOpenChange]);

  // Get cloud and local presets for select groups
  const cloudPresets = PROVIDER_PRESETS.filter((p) => p.category === 'cloud');
  const localPresets = PROVIDER_PRESETS.filter((p) => p.category === 'local');
  const customPresets = PROVIDER_PRESETS.filter((p) => p.category === 'custom');

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Settings2 className="h-5 w-5" />
            {isEditMode ? '编辑提供商' : '添加提供商'}
          </DialogTitle>
          <DialogDescription>
            {isEditMode
              ? '修改提供商配置。留空 API 密钥字段将保留现有密钥。'
              : '选择提供商类型并配置连接参数。'}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          {/* Provider Type (only for new) */}
          {!isEditMode && (
            <div className="space-y-2">
              <label className="text-sm font-medium">提供商类型</label>
              <Select
                value={formData.providerType}
                onValueChange={(value) =>
                  handleProviderTypeChange(value as ProviderType)
                }
              >
                <SelectTrigger className="w-full">
                  <SelectValue placeholder="选择提供商类型" />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    <SelectLabel className="flex items-center gap-2">
                      <Cloud className="h-4 w-4" />
                      云端服务
                    </SelectLabel>
                    {cloudPresets.map((preset) => (
                      <SelectItem key={preset.id} value={preset.id}>
                        {preset.name}
                      </SelectItem>
                    ))}
                  </SelectGroup>

                  <SelectGroup>
                    <SelectLabel className="flex items-center gap-2">
                      <HardDrive className="h-4 w-4" />
                      本地服务
                    </SelectLabel>
                    {localPresets.map((preset) => (
                      <SelectItem key={preset.id} value={preset.id}>
                        {preset.name}
                      </SelectItem>
                    ))}
                  </SelectGroup>

                  <SelectGroup>
                    <SelectLabel className="flex items-center gap-2">
                      <Cpu className="h-4 w-4" />
                      自定义服务
                    </SelectLabel>
                    {customPresets.map((preset) => (
                      <SelectItem key={preset.id} value={preset.id}>
                        {preset.name}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </div>
          )}

          {/* Display Name */}
          <div className="space-y-2">
            <label className="text-sm font-medium">显示名称</label>
            <Input
              value={formData.name}
              onChange={(e) => handleInputChange('name', e.target.value)}
              placeholder="输入提供商名称"
              className={errors.name ? 'border-destructive' : ''}
            />
            {errors.name && (
              <p className="text-xs text-destructive">{errors.name}</p>
            )}
          </div>

          {/* API Key */}
          <div className="space-y-2">
            <label className="text-sm font-medium">
              API 密钥
              {isEditMode && (
                <span className="text-muted-foreground font-normal ml-1">
                  (留空保留现有密钥)
                </span>
              )}
            </label>
            <div className="relative">
              <Input
                type={showApiKey ? 'text' : 'password'}
                value={formData.apiKey}
                onChange={(e) => handleInputChange('apiKey', e.target.value)}
                placeholder={
                  currentPreset?.requiresApiKey
                    ? '输入 API 密钥'
                    : '可选：输入 API 密钥'
                }
                className={`pr-10 ${errors.apiKey ? 'border-destructive' : ''}`}
              />
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                className="absolute right-1 top-1/2 -translate-y-1/2"
                onClick={() => setShowApiKey(!showApiKey)}
              >
                {showApiKey ? (
                  <EyeOff className="h-4 w-4" />
                ) : (
                  <Eye className="h-4 w-4" />
                )}
              </Button>
            </div>
            {errors.apiKey && (
              <p className="text-xs text-destructive">{errors.apiKey}</p>
            )}
          </div>

          {/* Base URL */}
          <div className="space-y-2">
            <label className="text-sm font-medium">
              Base URL
              {formData.providerType !== 'custom' && (
                <span className="text-muted-foreground font-normal ml-1">
                  (可选)
                </span>
              )}
            </label>
            <Input
              value={formData.baseUrl}
              onChange={(e) => handleInputChange('baseUrl', e.target.value)}
              placeholder="https://api.example.com/v1"
              className={errors.baseUrl ? 'border-destructive' : ''}
            />
            {errors.baseUrl && (
              <p className="text-xs text-destructive">{errors.baseUrl}</p>
            )}
            {currentPreset?.defaultBaseUrl && (
              <p className="text-xs text-muted-foreground">
                默认: {currentPreset.defaultBaseUrl}
              </p>
            )}
          </div>

          {/* API Protocol (only for custom providers) */}
          {formData.providerType === 'custom' && (
            <div className="space-y-2">
              <label className="text-sm font-medium">API 协议</label>
              <Select
                value={formData.apiProtocol}
                onValueChange={(value) =>
                  handleInputChange('apiProtocol', value as ApiProtocol)
                }
              >
                <SelectTrigger className="w-full">
                  <SelectValue placeholder="选择 API 协议" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="openai">OpenAI 兼容协议</SelectItem>
                  <SelectItem value="anthropic">Anthropic 兼容协议</SelectItem>
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">
                选择您的自定义服务使用的 API 协议类型
              </p>
            </div>
          )}

          {/* Default Model */}
          <div className="space-y-2">
            <label className="text-sm font-medium">
              默认模型
              <span className="text-muted-foreground font-normal ml-1">
                (可选)
              </span>
            </label>
            {currentPreset && currentPreset.popularModels.length > 0 ? (
              <Select
                value={formData.defaultModel}
                onValueChange={(value) =>
                  handleInputChange('defaultModel', value ?? '')
                }
              >
                <SelectTrigger className="w-full">
                  <SelectValue placeholder="选择默认模型" />
                </SelectTrigger>
                <SelectContent>
                  {currentPreset.popularModels.map((model) => (
                    <SelectItem key={model} value={model}>
                      {model}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : (
              <Input
                value={formData.defaultModel}
                onChange={(e) => handleInputChange('defaultModel', e.target.value)}
                placeholder="输入模型名称"
              />
            )}
          </div>

          {/* Set as Default */}
          <div className="flex items-center gap-2">
            <input
              type="checkbox"
              id="isDefault"
              checked={formData.isDefault}
              onChange={(e) => handleInputChange('isDefault', e.target.checked)}
              className="h-4 w-4 rounded border-input"
            />
            <label htmlFor="isDefault" className="text-sm">
              设为默认提供商
            </label>
          </div>
        </div>

        <DialogFooter>
          {isEditMode && onTestConnection && (
            <Button
              variant="outline"
              onClick={handleTestConnection}
              disabled={isTestingConnection || isLoading}
            >
              {isTestingConnection ? (
                <>
                  <Loader2 className="h-4 w-4 mr-1.5 animate-spin" />
                  测试中
                </>
              ) : (
                <>
                  <RefreshCw className="h-4 w-4 mr-1.5" />
                  测试连接
                </>
              )}
            </Button>
          )}
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            取消
          </Button>
          <Button onClick={handleSubmit} disabled={isLoading}>
            {isLoading ? (
              <>
                <Loader2 className="h-4 w-4 mr-1.5 animate-spin" />
                保存中
              </>
            ) : (
              <>
                {isEditMode ? (
                  '保存更改'
                ) : (
                  <>
                    <Plus className="h-4 w-4 mr-1.5" />
                    添加提供商
                  </>
                )}
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export default ProviderFormDialog;