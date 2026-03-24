/**
 * SkillConfigDialog 组件
 *
 * 技能配置对话框，根据 JSON Schema 渲染配置表单
 *
 * [Source: Story 7.6 - 技能管理界面]
 */

import { type FC, useState, useCallback, useMemo } from 'react';
import { Loader2, AlertCircle } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import { type SkillMetadata } from '@/types/skill';

export interface SkillConfigDialogProps {
  /** Skill to configure */
  skill: SkillMetadata;
  /** Current configuration */
  config: Record<string, unknown>;
  /** Dialog open state */
  open: boolean;
  /** Callback when open state changes */
  onOpenChange: (open: boolean) => void;
  /** Callback when config is saved */
  onSave: (config: Record<string, unknown>) => void;
}

/**
 * JSON Schema property definition
 */
interface SchemaProperty {
  type: string;
  title?: string;
  description?: string;
  default?: unknown;
  enum?: string[];
  minimum?: number;
  maximum?: number;
  minLength?: number;
  maxLength?: number;
  pattern?: string;
  items?: SchemaProperty;
  properties?: Record<string, SchemaProperty>;
  required?: string[];
}

/**
 * Render a single form field based on schema property
 */
interface FormFieldProps {
  name: string;
  property: SchemaProperty;
  value: unknown;
  onChange: (name: string, value: unknown) => void;
  required?: boolean;
}

const FormField: FC<FormFieldProps> = ({
  name,
  property,
  value,
  onChange,
  required,
}) => {
  const inputId = `skill-config-${name}`;
  const label = property.title || name;
  const description = property.description;

  const handleChange = (newValue: unknown) => {
    onChange(name, newValue);
  };

  // Render based on type
  switch (property.type) {
    case 'string':
      if (property.enum && property.enum.length > 0) {
        return (
          <div className="space-y-2">
            <Label htmlFor={inputId}>
              {label}
              {required && <span className="text-red-500 ml-1">*</span>}
            </Label>
            <select
              id={inputId}
              value={String(value || '')}
              onChange={(e) => handleChange(e.target.value)}
              className="flex h-8 w-full rounded-lg border border-input bg-transparent px-2.5 py-1 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
            >
              <option value="">请选择...</option>
              {property.enum.map(opt => (
                <option key={opt} value={opt}>{opt}</option>
              ))}
            </select>
            {description && (
              <p className="text-xs text-muted-foreground">{description}</p>
            )}
          </div>
        );
      }

      if (property.maxLength && property.maxLength > 200) {
        return (
          <div className="space-y-2">
            <Label htmlFor={inputId}>
              {label}
              {required && <span className="text-red-500 ml-1">*</span>}
            </Label>
            <textarea
              id={inputId}
              value={String(value || '')}
              onChange={(e) => handleChange(e.target.value)}
              placeholder={description}
              rows={4}
              className="flex w-full rounded-lg border border-input bg-transparent px-2.5 py-1.5 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 resize-none"
            />
            {description && (
              <p className="text-xs text-muted-foreground">{description}</p>
            )}
          </div>
        );
      }

      return (
        <div className="space-y-2">
          <Label htmlFor={inputId}>
            {label}
            {required && <span className="text-red-500 ml-1">*</span>}
          </Label>
          <Input
            id={inputId}
            type="text"
            value={String(value || '')}
            onChange={(e) => handleChange(e.target.value)}
            placeholder={description}
          />
          {description && property.maxLength && property.maxLength <= 100 && (
            <p className="text-xs text-muted-foreground">{description}</p>
          )}
        </div>
      );

    case 'number':
    case 'integer':
      return (
        <div className="space-y-2">
          <Label htmlFor={inputId}>
            {label}
            {required && <span className="text-red-500 ml-1">*</span>}
          </Label>
          <Input
            id={inputId}
            type="number"
            value={String(value ?? property.default ?? '')}
            onChange={(e) => {
              const val = property.type === 'integer'
                ? parseInt(e.target.value, 10)
                : parseFloat(e.target.value);
              handleChange(isNaN(val) ? undefined : val);
            }}
            min={property.minimum}
            max={property.maximum}
            placeholder={description}
          />
          {description && (
            <p className="text-xs text-muted-foreground">{description}</p>
          )}
        </div>
      );

    case 'boolean':
      return (
        <div className="flex items-center justify-between">
          <div>
            <Label htmlFor={inputId}>{label}</Label>
            {description && (
              <p className="text-xs text-muted-foreground">{description}</p>
            )}
          </div>
          <input
            id={inputId}
            type="checkbox"
            checked={Boolean(value ?? property.default ?? false)}
            onChange={(e) => handleChange(e.target.checked)}
            className="h-4 w-4 rounded border-input"
          />
        </div>
      );

    case 'array':
      // Simple array handling - comma-separated values
      return (
        <div className="space-y-2">
          <Label htmlFor={inputId}>
            {label}
            {required && <span className="text-red-500 ml-1">*</span>}
          </Label>
          <Input
            id={inputId}
            type="text"
            value={Array.isArray(value) ? value.join(', ') : ''}
            onChange={(e) => {
              const values = e.target.value.split(',').map(s => s.trim()).filter(Boolean);
              handleChange(values.length > 0 ? values : undefined);
            }}
            placeholder={description || '逗号分隔的值'}
          />
          {description && (
            <p className="text-xs text-muted-foreground">{description}</p>
          )}
        </div>
      );

    default:
      return (
        <div className="space-y-2">
          <Label htmlFor={inputId}>
            {label}
            {required && <span className="text-red-500 ml-1">*</span>}
          </Label>
          <Input
            id={inputId}
            type="text"
            value={String(value || '')}
            onChange={(e) => handleChange(e.target.value)}
            placeholder={description}
          />
          <Badge variant="outline" className="text-xs">
            {property.type}
          </Badge>
        </div>
      );
  }
};

/**
 * SkillConfigDialog component
 */
export const SkillConfigDialog: FC<SkillConfigDialogProps> = ({
  skill,
  config,
  open,
  onOpenChange,
  onSave,
}) => {
  const [localConfig, setLocalConfig] = useState<Record<string, unknown>>(() => ({ ...config }));
  const [isSaving, setIsSaving] = useState(false);
  const [errors, setErrors] = useState<Record<string, string>>({});

  // Parse schema properties
  const schemaProperties = useMemo(() => {
    if (!skill.configSchema) return {};
    return (skill.configSchema as { properties?: Record<string, SchemaProperty> }).properties || {};
  }, [skill.configSchema]);

  const requiredFields = useMemo(() => {
    if (!skill.configSchema) return [];
    return (skill.configSchema as { required?: string[] }).required || [];
  }, [skill.configSchema]);

  // Reset local config when dialog opens
  useMemo(() => {
    if (open) {
      setLocalConfig({ ...config });
      setErrors({});
    }
  }, [open, config]);

  // Handle field change
  const handleFieldChange = useCallback((name: string, value: unknown) => {
    setLocalConfig(prev => ({
      ...prev,
      [name]: value,
    }));
    // Clear error for this field
    setErrors(prev => {
      const next = { ...prev };
      delete next[name];
      return next;
    });
  }, []);

  // Validate form
  const validateForm = useCallback((): boolean => {
    const newErrors: Record<string, string> = {};

    // Check required fields
    requiredFields.forEach(field => {
      if (localConfig[field] === undefined || localConfig[field] === '') {
        newErrors[field] = '此字段为必填项';
      }
    });

    setErrors(newErrors);
    return Object.keys(newErrors).length === 0;
  }, [localConfig, requiredFields]);

  // Handle save
  const handleSave = async () => {
    if (!validateForm()) {
      return;
    }

    setIsSaving(true);
    try {
      onSave(localConfig);
    } finally {
      setIsSaving(false);
    }
  };

  const hasConfig = Object.keys(schemaProperties).length > 0;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>配置技能: {skill.name}</DialogTitle>
          <DialogDescription>
            {skill.description}
          </DialogDescription>
        </DialogHeader>

        {hasConfig ? (
          <div className="space-y-4 py-4">
            {Object.entries(schemaProperties).map(([name, property]) => (
              <div key={name}>
                <FormField
                  name={name}
                  property={property}
                  value={localConfig[name]}
                  onChange={handleFieldChange}
                  required={requiredFields.includes(name)}
                />
                {errors[name] && (
                  <div className="flex items-center gap-1.5 mt-1 text-xs text-red-500">
                    <AlertCircle className="h-3 w-3" />
                    {errors[name]}
                  </div>
                )}
              </div>
            ))}
          </div>
        ) : (
          <div className="py-8 text-center text-muted-foreground">
            此技能无需配置
          </div>
        )}

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
          >
            取消
          </Button>
          {hasConfig && (
            <Button
              onClick={handleSave}
              disabled={isSaving}
            >
              {isSaving && <Loader2 className="h-4 w-4 mr-1.5 animate-spin" />}
              保存
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

export default SkillConfigDialog;