/**
 * AI 代理创建表单组件
 *
 * 提供创建新 AI 代理的表单界面，包含:
 * - 名称、描述、专业领域输入
 * - MBTI 人格类型选择
 * - 人格预览
 * - 表单验证
 * - 提交和取消操作
 *
 * [Source: ux-design-specification.md#核心组件]
 * [Source: 2-5-agent-creation-ui.md]
 */

import * as React from 'react';
import { useState, useCallback, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { cn } from '@/lib/utils';
import { personalityColors, type MBTIType } from '@/lib/personality-colors';
import { MBTISelector } from './MBTISelector';
import { PersonalityPreview } from './PersonalityPreview';
import { ProviderSelector } from './ProviderSelector';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Loader2, Sparkles, UserCircle, Zap } from 'lucide-react';
import {
  type AgentModel,
  type NewAgent,
} from '@/types/agent';

// ============================================================================
// 类型定义
// ============================================================================

/**
 * 表单状态
 */
interface FormState {
  name: string;
  description: string;
  domain: string;
  mbtiType?: MBTIType;
  defaultProviderId?: string;
}

/**
 * 表单验证错误
 */
interface FormErrors {
  name?: string;
  description?: string;
  domain?: string;
  mbtiType?: string;
  defaultProviderId?: string;
}

/**
 * AgentCreateForm 组件属性
 */
export interface AgentCreateFormProps {
  /** 创建成功回调 */
  onSuccess?: (agent: AgentModel) => void;
  /** 取消回调 */
  onCancel?: () => void;
  /** 自定义类名 */
  className?: string;
}

// ============================================================================
// 常量
// ============================================================================

/** 字段最大长度 */
const MAX_LENGTHS = {
  name: 50,
  description: 500,
  domain: 100,
} as const;

/** 初始表单状态 */
const INITIAL_FORM_STATE: FormState = {
  name: '',
  description: '',
  domain: '',
  mbtiType: undefined,
  defaultProviderId: undefined,
};

// ============================================================================
// 工具函数
// ============================================================================

/**
 * 验证表单
 */
function validateForm(state: FormState): { valid: boolean; errors: FormErrors } {
  const errors: FormErrors = {};

  const trimmedName = state.name.trim();
  if (!trimmedName) {
    errors.name = '请输入代理名称';
  } else if (trimmedName.length > MAX_LENGTHS.name) {
    errors.name = `名称不能超过${MAX_LENGTHS.name}个字符`;
  }

  if (state.description.length > MAX_LENGTHS.description) {
    errors.description = `描述不能超过${MAX_LENGTHS.description}个字符`;
  }

  if (state.domain.length > MAX_LENGTHS.domain) {
    errors.domain = `专业领域不能超过${MAX_LENGTHS.domain}个字符`;
  }

  return {
    valid: Object.keys(errors).length === 0,
    errors,
  };
}

// ============================================================================
// 主组件
// ============================================================================

/**
 * AI 代理创建表单
 *
 * @example
 * ```tsx
 * // 基础用法
 * <AgentCreateForm
 *   onSuccess={(agent) => console.log('Created:', agent)}
 *   onCancel={() => navigate('/agents')}
 * />
 *
 * // 自定义样式
 * <AgentCreateForm className="max-w-4xl mx-auto" />
 * ```
 */
export function AgentCreateForm({
  onSuccess,
  onCancel,
  className,
}: AgentCreateFormProps): React.ReactElement {
  // ============================================================================
  // 状态
  // ============================================================================

  const [formState, setFormState] = useState<FormState>(INITIAL_FORM_STATE);
  const [errors, setErrors] = useState<FormErrors>({});
  const [touched, setTouched] = useState<Record<string, boolean>>({});
  const [isSubmitting, setIsSubmitting] = useState(false);

  // ============================================================================
  // 计算属性
  // ============================================================================

  /** 当前选中人格的主题色 */
  const themeColor = useMemo(() => {
    if (formState.mbtiType) {
      return personalityColors[formState.mbtiType].primary;
    }
    return undefined;
  }, [formState.mbtiType]);

  /** 表单是否有效（用于按钮禁用状态） */
  const isFormValid = useMemo(() => {
    const { valid } = validateForm(formState);
    return valid;
  }, [formState]);

  // ============================================================================
  // 事件处理
  // ============================================================================

  /** 验证单个字段 */
  const validateField = useCallback(
    <K extends keyof FormState>(field: K, value: FormState[K]): FormErrors => {
      const errors: FormErrors = {};

      if (field === 'name') {
        const trimmedName = (value as string).trim();
        if (!trimmedName) {
          errors.name = '请输入代理名称';
        } else if (trimmedName.length > MAX_LENGTHS.name) {
          errors.name = `名称不能超过${MAX_LENGTHS.name}个字符`;
        }
      }

      if (field === 'description' && (value as string).length > MAX_LENGTHS.description) {
        errors.description = `描述不能超过${MAX_LENGTHS.description}个字符`;
      }

      if (field === 'domain' && (value as string).length > MAX_LENGTHS.domain) {
        errors.domain = `专业领域不能超过${MAX_LENGTHS.domain}个字符`;
      }

      return errors;
    },
    []
  );

  /** 更新表单字段 */
  const updateField = useCallback(
    <K extends keyof FormState>(field: K, value: FormState[K]) => {
      setFormState((prev) => ({ ...prev, [field]: value }));
      // 如果字段已经被触碰过，实时验证
      if (touched[field]) {
        const newErrors = validateField(field, value);
        setErrors((prev) => {
          const updated = { ...prev };
          if (newErrors[field]) {
            updated[field] = newErrors[field];
          } else {
            delete updated[field];
          }
          return updated;
        });
      }
    },
    [touched, validateField]
  );

  /** 处理字段失去焦点 */
  const handleBlur = useCallback(
    <K extends keyof FormState>(field: K) => {
      setTouched((prev) => ({ ...prev, [field]: true }));
      // 验证该字段
      const newErrors = validateField(field, formState[field]);
      setErrors((prev) => {
        const updated = { ...prev };
        if (newErrors[field]) {
          updated[field] = newErrors[field];
        } else {
          delete updated[field];
        }
        return updated;
      });
    },
    [formState, validateField]
  );

  /** 处理 MBTI 类型选择 */
  const handleMBTIChange = useCallback(
    (type: MBTIType) => {
      updateField('mbtiType', type);
    },
    [updateField]
  );

  /** 处理默认提供商选择 */
  const handleProviderChange = useCallback(
    (providerId: string | undefined) => {
      updateField('defaultProviderId', providerId);
    },
    [updateField]
  );

  /** 处理表单提交 */
  const handleSubmit = useCallback(async () => {
    // 验证表单
    const { valid, errors: validationErrors } = validateForm(formState);
    if (!valid) {
      setErrors(validationErrors);
      return;
    }

    setIsSubmitting(true);

    try {
      // 构建请求数据
      const newAgent: NewAgent = {
        name: formState.name.trim(),
        description: formState.description.trim() || undefined,
        domain: formState.domain.trim() || undefined,
        mbti_type: formState.mbtiType,
        default_provider_id: formState.defaultProviderId,
      };

      // 调用 Tauri 命令
      const agent = await invoke<AgentModel>('create_agent', { config: newAgent });

      // 显示成功通知
      toast.success(`代理 "${agent.name}" 创建成功！`);

      // 重置表单
      setFormState(INITIAL_FORM_STATE);
      setErrors({});

      // 调用成功回调
      onSuccess?.(agent);
    } catch (error) {
      // 显示错误通知
      const message = error instanceof Error ? error.message : '创建代理失败';
      toast.error('创建代理失败', {
        description: message,
      });
    } finally {
      setIsSubmitting(false);
    }
  }, [formState, onSuccess]);

  /** 处理取消 */
  const handleCancel = useCallback(() => {
    onCancel?.();
  }, [onCancel]);

  // ============================================================================
  // 渲染
  // ============================================================================

  return (
    <div className={cn('space-y-6', className)}>
      {/* 响应式布局：表单左侧，预览右侧 */}
      <div className="grid grid-cols-1 md:grid-cols-5 gap-6">
        {/* 左侧：表单区域 */}
        <div className="md:col-span-3 space-y-6">
          {/* 名称输入 */}
          <div className="space-y-2">
            <label
              htmlFor="agent-name"
              className="block text-sm font-medium text-foreground/70"
            >
              名称 <span className="text-destructive">*</span>
            </label>
            <Input
              id="agent-name"
              type="text"
              value={formState.name}
              onChange={(e) => updateField('name', e.target.value)}
              onBlur={() => handleBlur('name')}
              placeholder="输入代理名称"
              maxLength={MAX_LENGTHS.name}
              disabled={isSubmitting}
              aria-describedby={errors.name ? 'name-error' : undefined}
              aria-invalid={!!errors.name}
              className={cn(
                'bg-background/50 border-border/50',
                'focus:border-primary/50 focus:ring-primary/20',
                errors.name && 'border-destructive'
              )}
            />
            {errors.name && (
              <p
                id="name-error"
                className="text-sm text-destructive"
                role="alert"
              >
                {errors.name}
              </p>
            )}
            <p className="text-xs text-muted-foreground">
              {formState.name.length}/{MAX_LENGTHS.name}
            </p>
          </div>

          {/* 描述输入 */}
          <div className="space-y-2">
            <label
              htmlFor="agent-description"
              className="block text-sm font-medium text-foreground/70"
            >
              描述
            </label>
            <textarea
              id="agent-description"
              value={formState.description}
              onChange={(e) => updateField('description', e.target.value)}
              onBlur={() => handleBlur('description')}
              placeholder="描述这个代理的用途和特点..."
              rows={3}
              maxLength={MAX_LENGTHS.description}
              disabled={isSubmitting}
              aria-describedby={errors.description ? 'description-error' : undefined}
              aria-invalid={!!errors.description}
              className={cn(
                'w-full bg-background/50 border border-border/50 rounded-md px-4 py-2',
                'text-foreground placeholder:text-muted-foreground/50',
                'focus:outline-none focus:border-primary/50 focus:ring-2 focus:ring-primary/20',
                'disabled:opacity-50 disabled:cursor-not-allowed',
                'resize-none',
                errors.description && 'border-destructive'
              )}
            />
            {errors.description && (
              <p
                id="description-error"
                className="text-sm text-destructive"
                role="alert"
              >
                {errors.description}
              </p>
            )}
            <p className="text-xs text-muted-foreground">
              {formState.description.length}/{MAX_LENGTHS.description}
            </p>
          </div>

          {/* 专业领域输入 */}
          <div className="space-y-2">
            <label
              htmlFor="agent-domain"
              className="block text-sm font-medium text-foreground/70"
            >
              专业领域
            </label>
            <Input
              id="agent-domain"
              type="text"
              value={formState.domain}
              onChange={(e) => updateField('domain', e.target.value)}
              onBlur={() => handleBlur('domain')}
              placeholder="例如：代码审查、文档写作、数据分析"
              maxLength={MAX_LENGTHS.domain}
              disabled={isSubmitting}
              aria-describedby={errors.domain ? 'domain-error' : undefined}
              aria-invalid={!!errors.domain}
              className={cn(
                'bg-background/50 border-border/50',
                'focus:border-primary/50 focus:ring-primary/20',
                errors.domain && 'border-destructive'
              )}
            />
            {errors.domain && (
              <p
                id="domain-error"
                className="text-sm text-destructive"
                role="alert"
              >
                {errors.domain}
              </p>
            )}
          </div>

          {/* MBTI 人格类型选择 */}
          <div className="space-y-2">
            <label className="block text-sm font-medium text-foreground/70 flex items-center gap-2">
              <Sparkles className="w-4 h-4" style={{ color: themeColor }} />
              人格类型
            </label>
            <MBTISelector
              value={formState.mbtiType}
              onChange={handleMBTIChange}
              disabled={isSubmitting}
            />
          </div>

          {/* 默认提供商选择 */}
          <div className="space-y-2">
            <label className="block text-sm font-medium text-foreground/70 flex items-center gap-2">
              <Zap className="w-4 h-4" />
              默认提供商
            </label>
            <p className="text-xs text-muted-foreground">
              为此代理指定默认的 LLM 提供商。如不指定，将使用全局默认提供商。
            </p>
            <ProviderSelector
              value={formState.defaultProviderId}
              onChange={handleProviderChange}
              disabled={isSubmitting}
              placeholder="选择默认提供商（可选）"
            />
          </div>
        </div>

        {/* 右侧：预览区域 */}
        <div className="md:col-span-2">
          <div className="sticky top-4">
            {formState.mbtiType ? (
              <PersonalityPreview
                mbtiType={formState.mbtiType}
                className="border border-border/50 rounded-lg"
              />
            ) : (
              <div className="flex flex-col items-center justify-center p-8 rounded-lg border border-dashed border-border/50 text-center min-h-[300px]">
                <UserCircle className="w-12 h-12 text-muted-foreground/30 mb-4" />
                <p className="text-muted-foreground text-sm">
                  选择人格类型后
                </p>
                <p className="text-muted-foreground text-sm">
                  将显示预览
                </p>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* 操作按钮 */}
      <div className="flex items-center justify-end gap-3 pt-4 border-t border-border/50">
        {onCancel && (
          <Button
            type="button"
            variant="outline"
            onClick={handleCancel}
            disabled={isSubmitting}
          >
            取消
          </Button>
        )}
        <Button
          type="button"
          onClick={handleSubmit}
          disabled={isSubmitting || !isFormValid}
          style={themeColor ? { backgroundColor: themeColor } : undefined}
          className={cn(
            'min-w-[120px]',
            themeColor && 'hover:opacity-90'
          )}
        >
          {isSubmitting ? (
            <>
              <Loader2 className="w-4 h-4 mr-2 animate-spin" />
              创建中...
            </>
          ) : (
            '创建代理'
          )}
        </Button>
      </div>
    </div>
  );
}

export default AgentCreateForm;