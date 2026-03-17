/**
 * 登录验证对话框组件
 *
 * 用于应用启动时的密码验证，当设置了"启动时要求密码"时显示。
 *
 * [Source: 2-11-local-account-management.md]
 */

import * as React from 'react';
import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Lock, Eye, EyeOff, AlertCircle, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';

// ============================================================================
// 类型定义
// ============================================================================

interface LoginDialogProps {
  /** 登录成功后的回调 */
  onLoginSuccess: () => void;
  /** 是否显示对话框 */
  open?: boolean;
}

// ============================================================================
// 主组件
// ============================================================================

/**
 * 登录验证对话框
 *
 * 显示密码输入框，验证成功后调用 onLoginSuccess
 */
export function LoginDialog({
  onLoginSuccess,
  open = true,
}: LoginDialogProps): React.ReactElement {
  const [password, setPassword] = useState('');
  const [showPassword, setShowPassword] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState('');

  // 清除错误当密码改变
  useEffect(() => {
    if (error) {
      setError('');
    }
  }, [password]);

  // 验证密码
  const handleLogin = useCallback(async () => {
    if (!password.trim()) {
      setError('请输入密码');
      return;
    }

    setIsLoading(true);
    setError('');

    try {
      const valid = await invoke<boolean>('verify_password', { password });

      if (valid) {
        onLoginSuccess();
      } else {
        setError('密码错误');
      }
    } catch (error) {
      const message =
        error instanceof Error ? error.message : '验证失败，请重试';
      setError(message);
    } finally {
      setIsLoading(false);
    }
  }, [password, onLoginSuccess]);

  // 处理键盘事件
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !isLoading) {
      handleLogin();
    }
  };

  return (
    <Dialog open={open}>
      <DialogContent
        className="sm:max-w-md"
        showCloseButton={false}
      >
        <DialogHeader className="text-center">
          <div className="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-primary/10">
            <Lock className="h-8 w-8 text-primary" />
          </div>
          <DialogTitle className="text-xl">输入密码</DialogTitle>
          <DialogDescription>
            请输入您的账户密码以继续使用应用
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="relative">
            <Input
              type={showPassword ? 'text' : 'password'}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="输入密码"
              disabled={isLoading}
              className="pr-10"
              autoFocus
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

          {error && (
            <div className="flex items-center gap-2 text-sm text-destructive">
              <AlertCircle className="h-4 w-4" />
              {error}
            </div>
          )}

          <Button
            className="w-full"
            onClick={handleLogin}
            disabled={isLoading || !password.trim()}
          >
            {isLoading ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                验证中...
              </>
            ) : (
              '确认'
            )}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

export default LoginDialog;