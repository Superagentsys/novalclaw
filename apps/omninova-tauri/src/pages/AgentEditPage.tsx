/**
 * AI 代理编辑页面
 *
 * 提供编辑现有 AI 代理的完整页面界面，包含:
 * - 从 URL 参数获取代理 UUID
 * - 加载代理数据
 * - 加载状态处理
 * - 代理不存在处理（404）
 * - 集成 AgentEditForm 组件
 * - 保存成功/取消后的导航
 *
 * [Source: 2-7-agent-edit.md]
 */

import * as React from 'react';
import { useState, useEffect, useCallback } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { invoke } from '@tauri-apps/api/core';
import { ArrowLeft, Edit, AlertCircle, Users } from 'lucide-react';
import { AgentEditForm, type AgentModel } from '@/components/agent';
import { Button } from '@/components/ui/button';

// ============================================================================
// 类型定义
// ============================================================================

type PageState =
  | { status: 'loading' }
  | { status: 'loaded'; agent: AgentModel }
  | { status: 'not-found' }
  | { status: 'error'; message: string };

// ============================================================================
// 子组件
// ============================================================================

/**
 * 加载骨架屏
 */
function LoadingSkeleton() {
  return (
    <div className="space-y-6 animate-pulse">
      {/* 页面标题骨架 */}
      <div className="h-8 w-48 bg-muted rounded" />

      {/* 表单骨架 */}
      <div className="grid grid-cols-1 md:grid-cols-5 gap-6">
        <div className="md:col-span-3 space-y-6">
          {/* 名称字段骨架 */}
          <div className="space-y-2">
            <div className="h-4 w-16 bg-muted rounded" />
            <div className="h-10 w-full bg-muted rounded" />
          </div>
          {/* 描述字段骨架 */}
          <div className="space-y-2">
            <div className="h-4 w-12 bg-muted rounded" />
            <div className="h-24 w-full bg-muted rounded" />
          </div>
          {/* 专业领域骨架 */}
          <div className="space-y-2">
            <div className="h-4 w-20 bg-muted rounded" />
            <div className="h-10 w-full bg-muted rounded" />
          </div>
          {/* MBTI 选择器骨架 */}
          <div className="space-y-2">
            <div className="h-4 w-16 bg-muted rounded" />
            <div className="h-20 w-full bg-muted rounded" />
          </div>
        </div>
        {/* 预览骨架 */}
        <div className="md:col-span-2">
          <div className="h-[300px] w-full bg-muted rounded-lg" />
        </div>
      </div>
    </div>
  );
}

/**
 * 404 未找到状态
 */
function NotFoundState({ onNavigateToList }: { onNavigateToList: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center py-16 text-center">
      <AlertCircle className="w-16 h-16 text-muted-foreground/30 mb-4" />
      <h2 className="text-xl font-semibold text-foreground/80 mb-2">
        代理不存在
      </h2>
      <p className="text-muted-foreground mb-6">
        您请求的代理可能已被删除或不存在
      </p>
      <Button variant="outline" onClick={onNavigateToList}>
        <Users className="w-4 h-4 mr-2" />
        返回列表
      </Button>
    </div>
  );
}

/**
 * 错误状态
 */
function ErrorState({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}) {
  return (
    <div className="flex flex-col items-center justify-center py-16 text-center">
      <AlertCircle className="w-12 h-12 text-destructive/50 mb-4" />
      <h3 className="text-lg font-medium text-foreground/70 mb-2">
        加载失败
      </h3>
      <p className="text-sm text-muted-foreground mb-4">{message}</p>
      <Button variant="outline" onClick={onRetry}>
        重试
      </Button>
    </div>
  );
}

// ============================================================================
// 主组件
// ============================================================================

/**
 * AI 代理编辑页面
 *
 * @example
 * ```tsx
 * // 在路由中使用
 * <Route path="/agents/:uuid/edit" element={<AgentEditPage />} />
 * ```
 */
export function AgentEditPage(): React.ReactElement {
  const navigate = useNavigate();
  const { uuid } = useParams<{ uuid: string }>();

  const [pageState, setPageState] = useState<PageState>({ status: 'loading' });

  // ============================================================================
  // 数据加载
  // ============================================================================

  const loadAgent = useCallback(async () => {
    if (!uuid) {
      setPageState({ status: 'not-found' });
      return;
    }

    setPageState({ status: 'loading' });

    try {
      const agentJson = await invoke<string | null>('get_agent_by_id', { uuid });

      if (!agentJson) {
        setPageState({ status: 'not-found' });
        return;
      }

      const agent = JSON.parse(agentJson) as AgentModel;
      setPageState({ status: 'loaded', agent });
    } catch (error) {
      const message = error instanceof Error ? error.message : '加载代理失败';
      setPageState({ status: 'error', message });
    }
  }, [uuid]);

  useEffect(() => {
    loadAgent();
  }, [loadAgent]);

  // ============================================================================
  // 事件处理
  // ============================================================================

  const handleBack = useCallback(() => {
    navigate(-1);
  }, [navigate]);

  const handleNavigateToList = useCallback(() => {
    navigate('/agents');
  }, [navigate]);

  const handleSuccess = useCallback(() => {
    navigate('/agents');
  }, [navigate]);

  const handleCancel = useCallback(() => {
    navigate(-1);
  }, [navigate]);

  // ============================================================================
  // 渲染
  // ============================================================================

  return (
    <div className="min-h-screen bg-background">
      {/* 页面头部 */}
      <header className="sticky top-0 z-10 border-b border-border/50 bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
        <div className="container flex items-center justify-between h-16 px-4">
          <div className="flex items-center gap-4">
            <Button
              variant="ghost"
              size="icon"
              onClick={handleBack}
              aria-label="返回"
            >
              <ArrowLeft className="w-5 h-5" />
            </Button>
            <div className="flex items-center gap-2">
              <Edit className="w-5 h-5 text-primary" />
              <h1 className="text-xl font-semibold">编辑代理</h1>
            </div>
          </div>
        </div>
      </header>

      {/* 主要内容区域 */}
      <main className="container px-4 py-8">
        <div className="max-w-5xl mx-auto">
          {pageState.status === 'loading' && <LoadingSkeleton />}

          {pageState.status === 'not-found' && (
            <NotFoundState onNavigateToList={handleNavigateToList} />
          )}

          {pageState.status === 'error' && (
            <ErrorState
              message={pageState.message}
              onRetry={loadAgent}
            />
          )}

          {pageState.status === 'loaded' && (
            <AgentEditForm
              agent={pageState.agent}
              onSuccess={handleSuccess}
              onCancel={handleCancel}
            />
          )}
        </div>
      </main>
    </div>
  );
}

export default AgentEditPage;