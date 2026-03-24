/**
 * ChannelConfigDialog 组件
 *
 * 渠道配置对话框，用于创建和编辑渠道配置
 *
 * [Source: Story 6.8 - 渠道配置界面]
 */

import { type FC, useState, useEffect, useCallback } from 'react';
import { Loader2 } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { ChannelConfigForm } from './ChannelConfigForm';
import {
  type ChannelKind,
  type ChannelBehaviorConfig,
  type ChannelInfo,
  type ChannelTypeDefinition,
  CHANNEL_TYPE_DEFINITIONS,
  createDefaultBehaviorConfig,
} from '@/types/channel';
import { useChannelConfig, type ChannelCredentialsData } from '@/hooks/useChannelConfig';

export interface ChannelConfigDialogProps {
  /** Whether dialog is open */
  open: boolean;
  /** Callback when dialog open state changes */
  onOpenChange: (open: boolean) => void;
  /** Channel kind to configure (for create mode) */
  channelKind?: ChannelKind;
  /** Existing channel to edit (for edit mode) */
  channel?: ChannelInfo;
  /** Callback when channel is created/updated */
  onSuccess?: (channel: ChannelInfo) => void;
}

/**
 * Get channel type definition by kind
 */
function getTypeDefinition(kind: ChannelKind): ChannelTypeDefinition | undefined {
  return CHANNEL_TYPE_DEFINITIONS.find((def) => def.kind === kind);
}

/**
 * ChannelConfigDialog component
 */
export const ChannelConfigDialog: FC<ChannelConfigDialogProps> = ({
  open,
  onOpenChange,
  channelKind,
  channel,
  onSuccess,
}) => {
  const isEditMode = !!channel;
  const effectiveKind = channel?.kind || channelKind;

  const {
    createChannel,
    updateChannel,
    testConnection,
    isLoading,
    error,
  } = useChannelConfig();

  // Form state
  const [name, setName] = useState('');
  const [enabled, setEnabled] = useState(true);
  const [behavior, setBehavior] = useState<ChannelBehaviorConfig>(createDefaultBehaviorConfig());
  const [credentials, setCredentials] = useState<Record<string, unknown>>({});
  const [formErrors, setFormErrors] = useState<Record<string, string>>({});
  const [testResult, setTestResult] = useState<'success' | 'error' | null>(null);
  const [isTesting, setIsTesting] = useState(false);

  // Initialize form for edit mode
  useEffect(() => {
    if (channel) {
      setName(channel.name);
      setEnabled(channel.status !== 'disconnected');
      // TODO: Load existing config and credentials
    } else {
      // Reset for create mode
      setName('');
      setEnabled(true);
      setBehavior(createDefaultBehaviorConfig());
      setCredentials({});
    }
    setFormErrors({});
    setTestResult(null);
  }, [channel, open]);

  // Validate form
  const validateForm = useCallback((): boolean => {
    const errors: Record<string, string> = {};

    if (!name.trim()) {
      errors.name = '请输入渠道名称';
    }

    // Validate required credential fields
    if (!isEditMode && effectiveKind) {
      const typeDef = getTypeDefinition(effectiveKind);
      typeDef?.configFields.forEach((field) => {
        if (field.required) {
          const value = credentials[field.name];
          if (value === undefined || value === '' || value === null) {
            errors[field.name] = `请输入${field.label}`;
          }
        }
      });
    }

    setFormErrors(errors);
    return Object.keys(errors).length === 0;
  }, [name, credentials, effectiveKind, isEditMode]);

  // Handle test connection
  const handleTestConnection = useCallback(async () => {
    if (!channel?.id) {
      // Cannot test before creation
      return false;
    }

    setIsTesting(true);
    setTestResult(null);
    try {
      const result = await testConnection(channel.id);
      setTestResult(result ? 'success' : 'error');
      return result;
    } catch {
      setTestResult('error');
      return false;
    } finally {
      setIsTesting(false);
    }
  }, [channel?.id, testConnection]);

  // Handle save
  const handleSave = useCallback(async () => {
    if (!validateForm()) {
      return;
    }

    if (!effectiveKind) {
      return;
    }

    try {
      if (isEditMode && channel) {
        // Update existing channel
        const success = await updateChannel(channel.id, {
          id: channel.id,
          name,
          kind: channel.kind,
          enabled,
          behavior,
        });

        if (success) {
          onSuccess?.(channel);
          onOpenChange(false);
        }
      } else {
        // Create new channel
        const config = {
          id: '', // Will be generated
          name,
          kind: effectiveKind,
          enabled,
          behavior,
        };

        const credData: ChannelCredentialsData = {
          kind: effectiveKind,
          data: credentials,
        };

        const newChannel = await createChannel(config, credData);

        if (newChannel) {
          onSuccess?.(newChannel);
          onOpenChange(false);
        }
      }
    } catch (err) {
      console.error('Failed to save channel:', err);
    }
  }, [
    validateForm,
    effectiveKind,
    isEditMode,
    channel,
    name,
    enabled,
    behavior,
    credentials,
    createChannel,
    updateChannel,
    onSuccess,
    onOpenChange,
  ]);

  // Get dialog title
  const getTitle = () => {
    if (isEditMode) {
      return `编辑渠道 - ${channel?.name}`;
    }
    const typeDef = effectiveKind ? getTypeDefinition(effectiveKind) : undefined;
    return `添加${typeDef?.name || '渠道'}`;
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[500px] max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>{getTitle()}</DialogTitle>
        </DialogHeader>

        {effectiveKind ? (
          <ChannelConfigForm
            kind={effectiveKind}
            name={name}
            onChangeName={setName}
            enabled={enabled}
            onChangeEnabled={setEnabled}
            behavior={behavior}
            onChangeBehavior={setBehavior}
            credentials={credentials}
            onChangeCredentials={setCredentials}
            onTestConnection={isEditMode ? handleTestConnection : undefined}
            isTestingConnection={isTesting}
            testConnectionResult={testResult}
            isEditMode={isEditMode}
            errors={formErrors}
          />
        ) : (
          <p className="text-muted-foreground">请选择渠道类型</p>
        )}

        {error && (
          <div className="p-3 rounded bg-red-50 dark:bg-red-950/30 text-sm text-red-600 dark:text-red-400">
            {error}
          </div>
        )}

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={isLoading}
          >
            取消
          </Button>
          <Button
            onClick={handleSave}
            disabled={isLoading}
          >
            {isLoading ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                保存中...
              </>
            ) : (
              '保存配置'
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

export default ChannelConfigDialog;