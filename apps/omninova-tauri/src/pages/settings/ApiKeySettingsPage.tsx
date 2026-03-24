/**
 * API Key Settings Page (Story 8.3)
 *
 * Provides UI for managing gateway API keys.
 */

import { useState } from 'react';
import { useApiKeys, useCreateApiKeyDialog } from '../../hooks/useApiKeys';
import type { ApiKeyInfo, ApiKeyCreated, ApiKeyPermission } from '../../types/api-key';
import {
  formatApiKeyDate,
  getApiKeyStatusColor,
  getApiKeyStatusLabel,
  PERMISSION_LABELS,
  PERMISSION_DESCRIPTIONS,
} from '../../types/api-key';

// Simple Card components (assuming these exist or using basic HTML)
const Card = ({ children, className = '' }: { children: React.ReactNode; className?: string }) => (
  <div className={`rounded-lg border bg-card shadow-sm ${className}`}>{children}</div>
);

const CardHeader = ({ children }: { children: React.ReactNode }) => (
  <div className="flex flex-col space-y-1.5 p-6">{children}</div>
);

const CardTitle = ({ children }: { children: React.ReactNode }) => (
  <h3 className="text-lg font-semibold leading-none tracking-tight">{children}</h3>
);

const CardDescription = ({ children }: { children: React.ReactNode }) => (
  <p className="text-sm text-muted-foreground">{children}</p>
);

const CardContent = ({ children }: { children: React.ReactNode }) => (
  <div className="p-6 pt-0">{children}</div>
);

// Simple Button component
const Button = ({
  children,
  onClick,
  variant = 'default',
  size = 'default',
  disabled = false,
  className = '',
}: {
  children: React.ReactNode;
  onClick?: () => void;
  variant?: 'default' | 'destructive' | 'outline' | 'ghost';
  size?: 'default' | 'sm' | 'lg';
  disabled?: boolean;
  className?: string;
}) => {
  const baseStyles = 'inline-flex items-center justify-center rounded-md font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 disabled:pointer-events-none disabled:opacity-50';
  const variantStyles = {
    default: 'bg-primary text-primary-foreground hover:bg-primary/90',
    destructive: 'bg-destructive text-destructive-foreground hover:bg-destructive/90',
    outline: 'border border-input bg-background hover:bg-accent hover:text-accent-foreground',
    ghost: 'hover:bg-accent hover:text-accent-foreground',
  };
  const sizeStyles = {
    default: 'h-10 px-4 py-2',
    sm: 'h-9 rounded-md px-3 text-xs',
    lg: 'h-11 rounded-md px-8',
  };

  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={`${baseStyles} ${variantStyles[variant]} ${sizeStyles[size]} ${className}`}
    >
      {children}
    </button>
  );
};

// Simple Checkbox component
const Checkbox = ({
  checked,
  onCheckedChange,
  label,
  description,
}: {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  label: string;
  description?: string;
}) => (
  <div className="flex items-start space-x-3">
    <input
      type="checkbox"
      checked={checked}
      onChange={(e) => onCheckedChange(e.target.checked)}
      className="h-4 w-4 rounded border-gray-300"
    />
    <div className="grid gap-1.5 leading-none">
      <label className="text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70">
        {label}
      </label>
      {description && <p className="text-xs text-muted-foreground">{description}</p>}
    </div>
  </div>
);

// Simple Input component
const Input = ({
  value,
  onChange,
  placeholder,
  type = 'text',
  className = '',
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  type?: string;
  className?: string;
}) => (
  <input
    type={type}
    value={value}
    onChange={(e) => onChange(e.target.value)}
    placeholder={placeholder}
    className={`flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 ${className}`}
  />
);

// Simple Select component
const Select = ({
  value,
  onChange,
  options,
}: {
  value: string;
  onChange: (value: string) => void;
  options: { value: string; label: string }[];
}) => (
  <select
    value={value}
    onChange={(e) => onChange(e.target.value)}
    className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
  >
    {options.map((opt) => (
      <option key={opt.value} value={opt.value}>
        {opt.label}
      </option>
    ))}
  </select>
);

// Simple Badge component
const Badge = ({
  children,
  variant = 'default',
}: {
  children: React.ReactNode;
  variant?: 'default' | 'secondary' | 'destructive' | 'outline';
}) => {
  const styles = {
    default: 'bg-primary text-primary-foreground hover:bg-primary/80',
    secondary: 'bg-secondary text-secondary-foreground hover:bg-secondary/80',
    destructive: 'bg-destructive text-destructive-foreground hover:bg-destructive/80',
    outline: 'text-foreground border border-input',
  };

  return (
    <span
      className={`inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-semibold transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 ${styles[variant]}`}
    >
      {children}
    </span>
  );
};

// Simple Dialog components
const Dialog = ({
  open,
  onOpenChange,
  children,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  children: React.ReactNode;
}) => {
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        className="fixed inset-0 bg-black/50"
        onClick={() => onOpenChange(false)}
      />
      <div className="relative z-50 w-full max-w-lg rounded-lg bg-background p-6 shadow-lg">
        {children}
      </div>
    </div>
  );
};

const DialogHeader = ({ children }: { children: React.ReactNode }) => (
  <div className="flex flex-col space-y-1.5 text-center sm:text-left mb-4">
    {children}
  </div>
);

const DialogTitle = ({ children }: { children: React.ReactNode }) => (
  <h2 className="text-lg font-semibold leading-none tracking-tight">{children}</h2>
);

const DialogDescription = ({ children }: { children: React.ReactNode }) => (
  <p className="text-sm text-muted-foreground">{children}</p>
);

const DialogFooter = ({ children }: { children: React.ReactNode }) => (
  <div className="flex flex-col-reverse sm:flex-row sm:justify-end sm:space-x-2 mt-4">
    {children}
  </div>
);

/**
 * API Key List Item Component
 */
function ApiKeyListItem({
  key,
  onRevoke,
  onDelete,
}: {
  key: ApiKeyInfo;
  onRevoke: () => void;
  onDelete: () => void;
}) {
  return (
    <div className="flex items-center justify-between p-4 border rounded-lg">
      <div className="flex items-center gap-4">
        <div className={`w-3 h-3 rounded-full ${getApiKeyStatusColor(key)}`} />
        <div>
          <div className="flex items-center gap-2">
            <span className="font-medium">{key.name}</span>
            <Badge variant={key.is_revoked || key.is_expired ? 'destructive' : 'default'}>
              {getApiKeyStatusLabel(key)}
            </Badge>
          </div>
          <div className="text-sm text-muted-foreground">
            <code className="text-xs bg-muted px-1 py-0.5 rounded">{key.key_prefix}...</code>
            {' • '}
            创建于 {formatApiKeyDate(key.created_at)}
            {key.expires_at && ` • 过期于 ${formatApiKeyDate(key.expires_at)}`}
          </div>
          <div className="flex gap-1 mt-1">
            {key.permissions.map((perm) => (
              <Badge key={perm} variant="outline" className="text-xs">
                {PERMISSION_LABELS[perm]}
              </Badge>
            ))}
          </div>
        </div>
      </div>
      <div className="flex gap-2">
        {!key.is_revoked && !key.is_expired && (
          <Button variant="outline" size="sm" onClick={onRevoke}>
            撤销
          </Button>
        )}
        <Button variant="ghost" size="sm" onClick={onDelete} className="text-destructive">
          删除
        </Button>
      </div>
    </div>
  );
}

/**
 * Create API Key Dialog
 */
function CreateApiKeyDialog({
  dialog,
}: {
  dialog: ReturnType<typeof useCreateApiKeyDialog>;
}) {
  const [copied, setCopied] = useState(false);

  const copyToClipboard = async (text: string) => {
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  if (dialog.createdKey) {
    return (
      <Dialog open={dialog.isOpen} onOpenChange={dialog.close}>
        <DialogHeader>
          <DialogTitle>API Key 已创建</DialogTitle>
          <DialogDescription>
            请立即复制并保存此密钥。关闭后将无法再次查看完整密钥！
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="p-4 bg-muted rounded-lg">
            <code className="break-all text-sm select-all">{dialog.createdKey.key}</code>
          </div>

          <Button
            onClick={() => copyToClipboard(dialog.createdKey!.key)}
            className="w-full"
          >
            {copied ? '已复制!' : '复制到剪贴板'}
          </Button>

          <div className="text-sm text-muted-foreground">
            <p>
              <strong>名称:</strong> {dialog.createdKey.name}
            </p>
            <p>
              <strong>权限:</strong>{' '}
              {dialog.createdKey.permissions.map((p) => PERMISSION_LABELS[p]).join(', ')}
            </p>
            {dialog.createdKey.expires_at && (
              <p>
                <strong>过期时间:</strong> {formatApiKeyDate(dialog.createdKey.expires_at)}
              </p>
            )}
          </div>
        </div>

        <DialogFooter>
          <Button
            onClick={() => {
              dialog.clearCreatedKey();
              dialog.close();
            }}
          >
            完成
          </Button>
        </DialogFooter>
      </Dialog>
    );
  }

  return (
    <Dialog open={dialog.isOpen} onOpenChange={dialog.close}>
      <DialogHeader>
        <DialogTitle>创建新的 API Key</DialogTitle>
        <DialogDescription>
          为 API Key 设置名称和权限。创建后请立即保存密钥。
        </DialogDescription>
      </DialogHeader>

      <div className="space-y-4">
        {dialog.error && (
          <div className="p-3 text-sm text-red-500 bg-red-50 rounded-lg">
            {dialog.error}
          </div>
        )}

        <div className="space-y-2">
          <label className="text-sm font-medium">名称</label>
          <Input
            value={dialog.name}
            onChange={dialog.setName}
            placeholder="例如: 开发环境 API Key"
          />
        </div>

        <div className="space-y-2">
          <label className="text-sm font-medium">权限</label>
          <div className="space-y-3">
            {(['read', 'write', 'admin'] as ApiKeyPermission[]).map((perm) => (
              <Checkbox
                key={perm}
                checked={dialog.permissions.includes(perm)}
                onCheckedChange={() => dialog.togglePermission(perm)}
                label={PERMISSION_LABELS[perm]}
                description={PERMISSION_DESCRIPTIONS[perm]}
              />
            ))}
          </div>
        </div>

        <div className="space-y-2">
          <label className="text-sm font-medium">过期时间</label>
          <Select
            value={dialog.expiresInDays?.toString() || ''}
            onChange={(v) => dialog.setExpiresInDays(v ? parseInt(v) : null)}
            options={[
              { value: '', label: '永不过期' },
              { value: '7', label: '7 天' },
              { value: '30', label: '30 天' },
              { value: '90', label: '90 天' },
              { value: '365', label: '1 年' },
            ]}
          />
        </div>
      </div>

      <DialogFooter>
        <Button variant="outline" onClick={dialog.close}>
          取消
        </Button>
        <Button onClick={dialog.create} disabled={dialog.isCreating}>
          {dialog.isCreating ? '创建中...' : '创建'}
        </Button>
      </DialogFooter>
    </Dialog>
  );
}

/**
 * API Key Settings Page Component
 */
export function ApiKeySettingsPage() {
  const { keys, loading, error, createKey, revokeKey, deleteKey } = useApiKeys();
  const createDialog = useCreateApiKeyDialog((key: ApiKeyCreated) => {
    console.log('API Key created:', key.key_prefix);
  });

  const handleRevoke = async (id: number) => {
    if (confirm('确定要撤销此 API Key 吗？撤销后将无法恢复。')) {
      try {
        await revokeKey(id);
      } catch (err) {
        alert(`撤销失败: ${err}`);
      }
    }
  };

  const handleDelete = async (id: number) => {
    if (confirm('确定要删除此 API Key 吗？此操作无法撤销。')) {
      try {
        await deleteKey(id);
      } catch (err) {
        alert(`删除失败: ${err}`);
      }
    }
  };

  return (
    <div className="space-y-6 p-6">
      <div>
        <h1 className="text-2xl font-bold">API Key 管理</h1>
        <p className="text-muted-foreground">
          管理用于 HTTP Gateway 认证的 API Key
        </p>
      </div>

      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle>API Keys</CardTitle>
              <CardDescription>
                使用 API Key 进行安全的 API 访问认证
              </CardDescription>
            </div>
            <Button onClick={createDialog.open}>
              创建 API Key
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          {error && (
            <div className="p-4 mb-4 text-sm text-red-500 bg-red-50 rounded-lg">
              {error}
            </div>
          )}

          {loading && keys.length === 0 ? (
            <div className="text-center py-8 text-muted-foreground">
              加载中...
            </div>
          ) : keys.length === 0 ? (
            <div className="text-center py-8 text-muted-foreground">
              还没有 API Key。点击上方按钮创建第一个。
            </div>
          ) : (
            <div className="space-y-3">
              {keys.map((key) => (
                <ApiKeyListItem
                  key={key.id}
                  key={key}
                  onRevoke={() => handleRevoke(key.id)}
                  onDelete={() => handleDelete(key.id)}
                />
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>使用说明</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-3 text-sm">
            <p>
              <strong>认证方式:</strong> 在请求头中添加 <code className="bg-muted px-1 rounded">Authorization: Bearer YOUR_API_KEY</code> 或 <code className="bg-muted px-1 rounded">X-API-Key: YOUR_API_KEY</code>
            </p>
            <p>
              <strong>权限说明:</strong>
            </p>
            <ul className="list-disc list-inside space-y-1 ml-2">
              <li><strong>Read</strong>: 只读访问，可查看代理和会话</li>
              <li><strong>Write</strong>: 读写访问，可创建/修改代理和发送消息</li>
              <li><strong>Admin</strong>: 管理员权限，可管理 API Keys 和系统配置</li>
            </ul>
            <p className="text-muted-foreground">
              注意: 高级权限自动包含低级权限。例如，Write 权限包含 Read 权限。
            </p>
          </div>
        </CardContent>
      </Card>

      <CreateApiKeyDialog dialog={createDialog} />
    </div>
  );
}

export default ApiKeySettingsPage;