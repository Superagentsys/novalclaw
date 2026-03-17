/**
 * AI 代理创建页面
 *
 * 提供创建新 AI 代理的完整页面界面
 *
 * [Source: ux-design-specification.md#页面设计]
 * [Source: 2-5-agent-creation-ui.md]
 */

import * as React from 'react';
import { useNavigate } from 'react-router-dom';
import { ArrowLeft, Sparkles } from 'lucide-react';
import { AgentCreateForm, type AgentModel } from '@/components/agent';
import { Button } from '@/components/ui/button';

/**
 * AI 代理创建页面
 *
 * @example
 * ```tsx
 * // 在路由中使用
 * <Route path="/agents/create" element={<AgentCreatePage />} />
 * ```
 */
export function AgentCreatePage(): React.ReactElement {
  const navigate = useNavigate();

  /** 处理创建成功 */
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const handleSuccess = (_agent: AgentModel) => {
    // 导航到代理详情页（或列表页）
    // 暂时导航到代理列表页
    navigate('/agents');
  };

  /** 处理取消 */
  const handleCancel = () => {
    navigate(-1); // 返回上一页
  };

  /** 处理返回 */
  const handleBack = () => {
    navigate(-1);
  };

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
              <Sparkles className="w-5 h-5 text-primary" />
              <h1 className="text-xl font-semibold">创建新代理</h1>
            </div>
          </div>
        </div>
      </header>

      {/* 主要内容区域 */}
      <main className="container px-4 py-8">
        <div className="max-w-5xl mx-auto">
          <AgentCreateForm
            onSuccess={handleSuccess}
            onCancel={handleCancel}
          />
        </div>
      </main>
    </div>
  );
}

export default AgentCreatePage;