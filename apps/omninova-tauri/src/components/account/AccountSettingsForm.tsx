/**
 * 账户设置表单组件
 *
 * 提供账户管理的完整表单界面，包含:
 * - 创建账户（首次使用）
 * - 修改用户名
 * - 修改密码
 * - 设置启动时是否需要密码验证
 * - 删除账户
 *
 * [Source: 2-11-local-account-management.md]
 */

import * as React from 'react';
import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { User, Lock, Trash2, Eye, EyeOff, AlertCircle, Check, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import type { AccountInfo } from '@/types/account';

// ============================================================================
// 类型定义
// ============================================================================

type FormState =
  | { status: 'loading' }
  | { status: 'no-account' }
  | { status: 'loaded'; account: AccountInfo }
  | { status: 'error'; message: string };

interface AccountSettingsFormProps {
  /** 账户变更后的回调 */
  onAccountChange?: () => void;
}

// ============================================================================
// 子组件
// ============================================================================

/**
 * 密码输入框（带显示/隐藏切换）
 */
function PasswordInput({
  value,
  onChange,
  placeholder,
  disabled,
  id,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  id?: string;
}) {
  const [showPassword, setShowPassword] = useState(false);

  return (
    <div className="relative">
      <Input
        id={id}
        type={showPassword ? 'text' : 'password'}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        disabled={disabled}
        className="pr-10"
      />
      <button
        type="button"
        onClick={() => setShowPassword(!showPassword)}
        className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
        tabIndex={-1}
      >
        {showPassword ? (
          <EyeOff className="h-4 w-4" />
        ) : (
          <Eye className="h-4 w-4" />
        )}
      </button>
    </div>
  );
}

/**
 * 加载骨架屏
 */
function LoadingSkeleton() {
  return (
    <div className="space-y-6 animate-pulse">
      <div className="h-8 w-32 bg-muted rounded" />
      <div className="space-y-4">
        <div className="h-10 w-full bg-muted rounded" />
        <div className="h-10 w-full bg-muted rounded" />
      </div>
    </div>
  );
}

/**
 * 创建账户表单
 */
function CreateAccountForm({
  onCreate,
  isLoading,
}: {
  onCreate: (username: string, password: string) => void;
  isLoading: boolean;
}) {
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [error, setError] = useState('');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    setError('');

    if (!username.trim()) {
      setError('请输入用户名');
      return;
    }

    if (password.length < 8) {
      setError('密码长度至少8个字符');
      return;
    }

    if (password !== confirmPassword) {
      setError('两次输入的密码不一致');
      return;
    }

    onCreate(username.trim(), password);
  };

  return (
    <Card className="border-border/50">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <User className="h-5 w-5" />
          创建账户
        </CardTitle>
        <CardDescription>
          首次使用需要创建本地账户以保护您的数据安全
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-2">
            <label htmlFor="username" className="text-sm font-medium">
              用户名
            </label>
            <Input
              id="username"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="输入用户名"
              disabled={isLoading}
            />
          </div>

          <div className="space-y-2">
            <label htmlFor="password" className="text-sm font-medium">
              密码
            </label>
            <PasswordInput
              id="password"
              value={password}
              onChange={setPassword}
              placeholder="至少8个字符"
              disabled={isLoading}
            />
          </div>

          <div className="space-y-2">
            <label htmlFor="confirmPassword" className="text-sm font-medium">
              确认密码
            </label>
            <PasswordInput
              id="confirmPassword"
              value={confirmPassword}
              onChange={setConfirmPassword}
              placeholder="再次输入密码"
              disabled={isLoading}
            />
          </div>

          {error && (
            <div className="flex items-center gap-2 text-sm text-destructive">
              <AlertCircle className="h-4 w-4" />
              {error}
            </div>
          )}

          <Button type="submit" className="w-full" disabled={isLoading}>
            {isLoading ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                创建中...
              </>
            ) : (
              '创建账户'
            )}
          </Button>
        </form>
      </CardContent>
    </Card>
  );
}

/**
 * 账户管理表单（已登录状态）
 */
function AccountManageForm({
  account,
  onAccountChange,
}: {
  account: AccountInfo;
  onAccountChange?: () => void;
}) {
  const [username, setUsername] = useState(account.username);
  const [requirePasswordOnStartup, setRequirePasswordOnStartup] = useState(
    account.require_password_on_startup
  );

  // 修改密码状态
  const [showChangePassword, setShowChangePassword] = useState(false);
  const [currentPassword, setCurrentPassword] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [confirmNewPassword, setConfirmNewPassword] = useState('');

  // 删除账户状态
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [deleteConfirmPassword, setDeleteConfirmPassword] = useState('');

  // 加载状态
  const [isUpdating, setIsUpdating] = useState(false);
  const [isChangingPassword, setIsChangingPassword] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);

  // 消息
  const [successMessage, setSuccessMessage] = useState('');
  const [errorMessage, setErrorMessage] = useState('');

  // 更新用户名和启动密码设置
  const handleUpdateSettings = async () => {
    setIsUpdating(true);
    setErrorMessage('');
    setSuccessMessage('');

    try {
      const updates = {
        username: username !== account.username ? username : undefined,
        require_password_on_startup:
          requirePasswordOnStartup !== account.require_password_on_startup
            ? requirePasswordOnStartup
            : undefined,
      };

      // 只有有变更才更新
      if (updates.username || updates.require_password_on_startup !== undefined) {
        await invoke('update_account', {
          updatesJson: JSON.stringify(updates),
        });
        setSuccessMessage('设置已更新');
        onAccountChange?.();
      }
    } catch (error) {
      setErrorMessage(
        error instanceof Error ? error.message : '更新设置失败'
      );
    } finally {
      setIsUpdating(false);
    }
  };

  // 修改密码
  const handleChangePassword = async () => {
    setErrorMessage('');
    setSuccessMessage('');

    if (newPassword.length < 8) {
      setErrorMessage('新密码长度至少8个字符');
      return;
    }

    if (newPassword !== confirmNewPassword) {
      setErrorMessage('两次输入的新密码不一致');
      return;
    }

    setIsChangingPassword(true);

    try {
      await invoke('update_password', {
        currentPassword,
        newPassword,
      });
      setSuccessMessage('密码已更新');
      setShowChangePassword(false);
      setCurrentPassword('');
      setNewPassword('');
      setConfirmNewPassword('');
      onAccountChange?.();
    } catch (error) {
      setErrorMessage(
        error instanceof Error ? error.message : '修改密码失败'
      );
    } finally {
      setIsChangingPassword(false);
    }
  };

  // 删除账户
  const handleDeleteAccount = async () => {
    setErrorMessage('');
    setSuccessMessage('');

    // 验证密码
    try {
      const valid = await invoke<boolean>('verify_password', {
        password: deleteConfirmPassword,
      });

      if (!valid) {
        setErrorMessage('密码错误');
        return;
      }
    } catch (error) {
      setErrorMessage('验证密码失败');
      return;
    }

    setIsDeleting(true);

    try {
      await invoke('delete_account');
      setSuccessMessage('账户已删除');
      setShowDeleteConfirm(false);
      onAccountChange?.();
    } catch (error) {
      setErrorMessage(
        error instanceof Error ? error.message : '删除账户失败'
      );
    } finally {
      setIsDeleting(false);
    }
  };

  return (
    <div className="space-y-6">
      {/* 基本设置 */}
      <Card className="border-border/50">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <User className="h-5 w-5" />
            账户信息
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <label htmlFor="username" className="text-sm font-medium">
              用户名
            </label>
            <Input
              id="username"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="用户名"
            />
          </div>

          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <label className="text-sm font-medium">启动时要求密码</label>
              <p className="text-xs text-muted-foreground">
                每次启动应用时需要输入密码才能使用
              </p>
            </div>
            <button
              type="button"
              onClick={() => setRequirePasswordOnStartup(!requirePasswordOnStartup)}
              className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
                requirePasswordOnStartup ? 'bg-primary' : 'bg-muted'
              }`}
            >
              <span
                className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                  requirePasswordOnStartup ? 'translate-x-6' : 'translate-x-1'
                }`}
              />
            </button>
          </div>

          <Button
            onClick={handleUpdateSettings}
            disabled={isUpdating}
            variant="outline"
          >
            {isUpdating ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                保存中...
              </>
            ) : (
              '保存设置'
            )}
          </Button>
        </CardContent>
      </Card>

      {/* 修改密码 */}
      <Card className="border-border/50">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Lock className="h-5 w-5" />
            修改密码
          </CardTitle>
        </CardHeader>
        <CardContent>
          {!showChangePassword ? (
            <Button
              variant="outline"
              onClick={() => setShowChangePassword(true)}
            >
              修改密码
            </Button>
          ) : (
            <div className="space-y-4">
              <div className="space-y-2">
                <label className="text-sm font-medium">当前密码</label>
                <PasswordInput
                  value={currentPassword}
                  onChange={setCurrentPassword}
                  placeholder="输入当前密码"
                />
              </div>

              <div className="space-y-2">
                <label className="text-sm font-medium">新密码</label>
                <PasswordInput
                  value={newPassword}
                  onChange={setNewPassword}
                  placeholder="至少8个字符"
                />
              </div>

              <div className="space-y-2">
                <label className="text-sm font-medium">确认新密码</label>
                <PasswordInput
                  value={confirmNewPassword}
                  onChange={setConfirmNewPassword}
                  placeholder="再次输入新密码"
                />
              </div>

              <div className="flex gap-2">
                <Button
                  onClick={handleChangePassword}
                  disabled={isChangingPassword}
                >
                  {isChangingPassword ? (
                    <>
                      <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                      更新中...
                    </>
                  ) : (
                    '确认修改'
                  )}
                </Button>
                <Button
                  variant="ghost"
                  onClick={() => {
                    setShowChangePassword(false);
                    setCurrentPassword('');
                    setNewPassword('');
                    setConfirmNewPassword('');
                  }}
                  disabled={isChangingPassword}
                >
                  取消
                </Button>
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      {/* 删除账户 */}
      <Card className="border-destructive/30">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-destructive">
            <Trash2 className="h-5 w-5" />
            危险操作
          </CardTitle>
          <CardDescription>
            删除账户将永久移除所有本地数据，此操作不可恢复
          </CardDescription>
        </CardHeader>
        <CardContent>
          {!showDeleteConfirm ? (
            <Button
              variant="destructive"
              onClick={() => setShowDeleteConfirm(true)}
            >
              删除账户
            </Button>
          ) : (
            <div className="space-y-4">
              <p className="text-sm text-destructive">
                确定要删除账户吗？此操作将永久删除所有数据且无法恢复。
              </p>

              <div className="space-y-2">
                <label className="text-sm font-medium">输入密码确认</label>
                <PasswordInput
                  value={deleteConfirmPassword}
                  onChange={setDeleteConfirmPassword}
                  placeholder="输入密码以确认删除"
                />
              </div>

              <div className="flex gap-2">
                <Button
                  variant="destructive"
                  onClick={handleDeleteAccount}
                  disabled={isDeleting}
                >
                  {isDeleting ? (
                    <>
                      <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                      删除中...
                    </>
                  ) : (
                    '确认删除'
                  )}
                </Button>
                <Button
                  variant="ghost"
                  onClick={() => {
                    setShowDeleteConfirm(false);
                    setDeleteConfirmPassword('');
                  }}
                  disabled={isDeleting}
                >
                  取消
                </Button>
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      {/* 消息提示 */}
      {successMessage && (
        <div className="flex items-center gap-2 text-sm text-green-600 dark:text-green-400">
          <Check className="h-4 w-4" />
          {successMessage}
        </div>
      )}
      {errorMessage && (
        <div className="flex items-center gap-2 text-sm text-destructive">
          <AlertCircle className="h-4 w-4" />
          {errorMessage}
        </div>
      )}
    </div>
  );
}

// ============================================================================
// 主组件
// ============================================================================

/**
 * 账户设置表单
 *
 * 自动检测账户状态，显示创建或管理界面
 */
export function AccountSettingsForm({
  onAccountChange,
}: AccountSettingsFormProps): React.ReactElement {
  const [formState, setFormState] = useState<FormState>({ status: 'loading' });
  const [isCreating, setIsCreating] = useState(false);

  // 加载账户信息
  const loadAccount = useCallback(async () => {
    setFormState({ status: 'loading' });

    try {
      // 初始化 account store
      await invoke('init_account_store');

      // 检查是否存在账户
      const hasAccount = await invoke<boolean>('has_account');

      if (!hasAccount) {
        setFormState({ status: 'no-account' });
        return;
      }

      // 获取账户信息
      const account = await invoke<AccountInfo | null>('get_account');

      if (account) {
        setFormState({ status: 'loaded', account });
      } else {
        setFormState({ status: 'no-account' });
      }
    } catch (error) {
      const message =
        error instanceof Error ? error.message : '加载账户信息失败';
      setFormState({ status: 'error', message });
    }
  }, []);

  useEffect(() => {
    loadAccount();
  }, [loadAccount]);

  // 创建账户
  const handleCreateAccount = async (username: string, password: string) => {
    setIsCreating(true);

    try {
      await invoke('create_account', { username, password });
      await loadAccount();
      onAccountChange?.();
    } catch (error) {
      // 错误会由表单显示
      console.error('Failed to create account:', error);
    } finally {
      setIsCreating(false);
    }
  };

  // 渲染
  if (formState.status === 'loading') {
    return <LoadingSkeleton />;
  }

  if (formState.status === 'error') {
    return (
      <div className="flex flex-col items-center justify-center py-8 text-center">
        <AlertCircle className="h-12 w-12 text-destructive/50 mb-4" />
        <h3 className="text-lg font-medium text-foreground/70 mb-2">
          加载失败
        </h3>
        <p className="text-sm text-muted-foreground mb-4">
          {formState.message}
        </p>
        <Button variant="outline" onClick={loadAccount}>
          重试
        </Button>
      </div>
    );
  }

  if (formState.status === 'no-account') {
    return (
      <CreateAccountForm
        onCreate={handleCreateAccount}
        isLoading={isCreating}
      />
    );
  }

  return (
    <AccountManageForm
      account={formState.account}
      onAccountChange={onAccountChange}
    />
  );
}

export default AccountSettingsForm;