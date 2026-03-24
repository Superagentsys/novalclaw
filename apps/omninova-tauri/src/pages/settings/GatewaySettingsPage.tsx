/**
 * GatewaySettingsPage 组件
 *
 * HTTP Gateway 设置页面，提供服务控制、状态显示和配置编辑功能
 *
 * [Source: Story 8.1 - HTTP Gateway 服务实现]
 */

import { type FC, useState, useCallback } from 'react';
import { Play, Square, Loader2, RefreshCw, CheckCircle, XCircle, AlertCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { useGateway } from '@/hooks/useGateway';
import type { GatewayStatusPayload } from '@/types/gateway';

/**
 * Status indicator component
 */
const StatusIndicator: FC<{ running: boolean; lastError?: string }> = ({
  running,
  lastError,
}) => {
  if (running) {
    return (
      <div className="flex items-center gap-2">
        <CheckCircle className="h-5 w-5 text-green-500" />
        <Badge variant="default" className="bg-green-500 hover:bg-green-600">
          运行中
        </Badge>
      </div>
    );
  }

  if (lastError) {
    return (
      <div className="flex items-center gap-2">
        <AlertCircle className="h-5 w-5 text-red-500" />
        <Badge variant="destructive">错误</Badge>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-2">
      <XCircle className="h-5 w-5 text-gray-400" />
      <Badge variant="secondary">已停止</Badge>
    </div>
  );
};

/**
 * GatewaySettingsPage component
 */
export const GatewaySettingsPage: FC = () => {
  const { status, isLoading, error, start, stop, refresh } = useGateway();

  // Local config state for display (editing requires config.toml modification)
  const [host, setHost] = useState('127.0.0.1');
  const [port, setPort] = useState('42617'); // Backend default port
  const [corsEnabled, setCorsEnabled] = useState(true);

  // Handle start gateway
  const handleStart = useCallback(async () => {
    try {
      await start();
    } catch (err) {
      console.error('Failed to start gateway:', err);
    }
  }, [start]);

  // Handle stop gateway
  const handleStop = useCallback(async () => {
    try {
      await stop();
    } catch (err) {
      console.error('Failed to stop gateway:', err);
    }
  }, [stop]);

  // Handle refresh
  const handleRefresh = useCallback(async () => {
    await refresh();
  }, [refresh]);

  return (
    <div className="container mx-auto py-6 px-4 max-w-4xl">
      <div className="space-y-6">
        {/* Header */}
        <div>
          <h1 className="text-2xl font-bold">HTTP Gateway</h1>
          <p className="text-muted-foreground">
            配置和管理 HTTP API 网关，允许第三方工具与系统集成
          </p>
        </div>

        {/* Error Alert */}
        {error && (
          <Alert variant="destructive">
            <AlertCircle className="h-4 w-4" />
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        )}

        {/* Status Card */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center justify-between">
              <span>服务状态</span>
              <Button
                variant="ghost"
                size="sm"
                onClick={handleRefresh}
                disabled={isLoading}
              >
                <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
              </Button>
            </CardTitle>
            <CardDescription>
              当前 HTTP Gateway 的运行状态
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              {/* Status indicator */}
              <div className="flex items-center justify-between">
                <StatusIndicator
                  running={status?.running ?? false}
                  lastError={status?.lastError}
                />
              </div>

              {/* URL */}
              {status?.running && (
                <div className="flex items-center gap-2 text-sm">
                  <span className="text-muted-foreground">访问地址:</span>
                  <code className="px-2 py-1 bg-muted rounded text-xs">
                    {status.url}
                  </code>
                </div>
              )}

              {/* Last error */}
              {status?.lastError && (
                <div className="text-sm text-red-500">
                  错误: {status.lastError}
                </div>
              )}

              {/* Control buttons */}
              <div className="flex gap-2">
                {status?.running ? (
                  <Button
                    variant="destructive"
                    onClick={handleStop}
                    disabled={isLoading}
                  >
                    {isLoading ? (
                      <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                    ) : (
                      <Square className="h-4 w-4 mr-2" />
                    )}
                    停止服务
                  </Button>
                ) : (
                  <Button onClick={handleStart} disabled={isLoading}>
                    {isLoading ? (
                      <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                    ) : (
                      <Play className="h-4 w-4 mr-2" />
                    )}
                    启动服务
                  </Button>
                )}
              </div>
            </div>
          </CardContent>
        </Card>

        {/* Configuration Card */}
        <Card>
          <CardHeader>
            <CardTitle>基本配置</CardTitle>
            <CardDescription>
              HTTP Gateway 的网络配置
              <span className="text-xs text-muted-foreground ml-2">
                (修改配置需编辑 config.toml 文件)
              </span>
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              {/* Host - Display only, edit via config.toml */}
              <div className="grid grid-cols-4 items-center gap-4">
                <Label htmlFor="host" className="text-right">
                  主机地址
                </Label>
                <Input
                  id="host"
                  value={host}
                  onChange={(e) => setHost(e.target.value)}
                  className="col-span-3"
                  placeholder="127.0.0.1"
                  disabled
                />
              </div>

              {/* Port - Display only, edit via config.toml */}
              <div className="grid grid-cols-4 items-center gap-4">
                <Label htmlFor="port" className="text-right">
                  端口
                </Label>
                <Input
                  id="port"
                  type="number"
                  value={port}
                  onChange={(e) => setPort(e.target.value)}
                  className="col-span-3"
                  placeholder="42617"
                  disabled
                />
              </div>

              {/* CORS Toggle - Display only, edit via config.toml */}
              <div className="grid grid-cols-4 items-center gap-4">
                <Label htmlFor="cors" className="text-right">
                  启用 CORS
                </Label>
                <div className="col-span-3 flex items-center space-x-2">
                  <Switch
                    id="cors"
                    checked={corsEnabled}
                    onCheckedChange={setCorsEnabled}
                    disabled
                  />
                  <span className="text-sm text-muted-foreground">
                    允许跨域请求
                  </span>
                </div>
              </div>
            </div>
          </CardContent>
        </Card>

        {/* API Endpoints Card */}
        {status?.running && (
          <Card>
            <CardHeader>
              <CardTitle>可用端点</CardTitle>
              <CardDescription>
                HTTP Gateway 提供的 API 端点
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="space-y-2 text-sm">
                <div className="flex items-center gap-2">
                  <Badge variant="outline" className="w-16">GET</Badge>
                  <code className="text-xs">{status.url}/health</code>
                  <span className="text-muted-foreground">健康检查</span>
                </div>
                <div className="flex items-center gap-2">
                  <Badge variant="outline" className="w-16">POST</Badge>
                  <code className="text-xs">{status.url}/chat</code>
                  <span className="text-muted-foreground">聊天接口</span>
                </div>
                <div className="flex items-center gap-2">
                  <Badge variant="outline" className="w-16">POST</Badge>
                  <code className="text-xs">{status.url}/route</code>
                  <span className="text-muted-foreground">消息路由</span>
                </div>
                <div className="flex items-center gap-2">
                  <Badge variant="outline" className="w-16">GET</Badge>
                  <code className="text-xs">{status.url}/api/status</code>
                  <span className="text-muted-foreground">系统状态</span>
                </div>
                <div className="flex items-center gap-2">
                  <Badge variant="outline" className="w-16">GET</Badge>
                  <code className="text-xs">{status.url}/api/tools</code>
                  <span className="text-muted-foreground">工具列表</span>
                </div>
                <div className="flex items-center gap-2">
                  <Badge variant="outline" className="w-16">WS</Badge>
                  <code className="text-xs">{status.url.replace('http', 'ws')}/ws/chat</code>
                  <span className="text-muted-foreground">WebSocket 聊天</span>
                </div>
              </div>
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  );
};

export default GatewaySettingsPage;