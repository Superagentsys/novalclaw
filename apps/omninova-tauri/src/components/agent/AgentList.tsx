/**
 * 代理列表组件
 *
 * 显示代理卡片的网格列表，支持:
 * - 响应式网格布局
 * - 空状态显示
 * - 加载状态（骨架屏）
 * - 点击导航
 *
 * [Source: ux-design-specification.md#核心组件]
 * [Source: 2-6-agent-list-card.md]
 */

import * as React from 'react';
import { cn } from '@/lib/utils';
import { AgentCard } from './AgentCard';
import { Skeleton } from '@/components/ui/skeleton';
import { Button } from '@/components/ui/button';
import { Plus, UserCircle } from 'lucide-react';
import { type AgentModel } from '@/types/agent';

// ============================================================================
// 类型定义
// ============================================================================

/**
 * AgentList 组件属性
 */
export interface AgentListProps {
  /** 代理列表数据 */
  agents: AgentModel[];
  /** 加载状态 */
  isLoading?: boolean;
  /** 点击代理卡片回调 */
  onAgentClick?: (agent: AgentModel) => void;
  /** 点击创建代理按钮回调 */
  onCreateAgent?: () => void;
  /** 自定义类名 */
  className?: string;
  /** 是否显示编辑按钮 */
  showEditButton?: boolean;
  /** 编辑按钮点击回调 */
  onEdit?: (agent: AgentModel) => void;
  /** 是否显示复制按钮 */
  showDuplicateButton?: boolean;
  /** 复制按钮点击回调 */
  onDuplicate?: (agent: AgentModel) => void;
  /** 是否显示状态切换按钮 */
  showToggleButton?: boolean;
  /** 状态切换按钮点击回调 */
  onToggle?: (agent: AgentModel) => void;
  /** 是否显示删除按钮 */
  showDeleteButton?: boolean;
  /** 删除按钮点击回调 */
  onDelete?: (agent: AgentModel) => void;
}

// ============================================================================
// 子组件
// ============================================================================

/**
 * 空状态组件
 */
function EmptyState({ onCreateAgent }: { onCreateAgent?: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center py-16 text-center">
      <UserCircle className="w-16 h-16 text-muted-foreground/30 mb-4" />
      <h3 className="text-lg font-medium text-foreground/70 mb-2">
        还没有创建代理
      </h3>
      <p className="text-sm text-muted-foreground mb-6">
        创建你的第一个 AI 代理开始对话
      </p>
      {onCreateAgent && (
        <Button onClick={onCreateAgent}>
          <Plus className="w-4 h-4 mr-2" />
          创建代理
        </Button>
      )}
    </div>
  );
}

/**
 * 骨架屏组件
 */
function LoadingSkeleton() {
  return (
    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
      {Array.from({ length: 6 }).map((_, i) => (
        <div
          key={i}
          className="p-4 rounded-lg border border-border/50"
        >
          {/* 头部 */}
          <div className="flex items-center gap-3 mb-3">
            <Skeleton className="w-10 h-10 rounded-full" />
            <Skeleton className="h-5 w-32" />
          </div>
          {/* 描述 */}
          <Skeleton className="h-4 w-full mb-2" />
          <Skeleton className="h-4 w-3/4 mb-3" />
          {/* 标签 */}
          <div className="flex gap-2">
            <Skeleton className="h-5 w-12" />
            <Skeleton className="h-5 w-16" />
          </div>
        </div>
      ))}
    </div>
  );
}

// ============================================================================
// 主组件
// ============================================================================

/**
 * 代理列表组件
 *
 * @example
 * ```tsx
 * // 基础用法
 * <AgentList
 *   agents={agents}
 *   onAgentClick={(agent) => navigate(`/agents/${agent.agent_uuid}`)}
 * />
 *
 * // 加载状态
 * <AgentList agents={[]} isLoading />
 *
 * // 空状态带创建按钮
 * <AgentList
 *   agents={[]}
 *   onCreateAgent={() => navigate('/agents/create')}
 * />
 * ```
 */
export function AgentList({
  agents,
  isLoading = false,
  onAgentClick,
  onCreateAgent,
  className,
  showEditButton,
  onEdit,
  showDuplicateButton,
  onDuplicate,
  showToggleButton,
  onToggle,
  showDeleteButton,
  onDelete,
}: AgentListProps): React.ReactElement {
  // 加载中状态
  if (isLoading && agents.length === 0) {
    return <LoadingSkeleton />;
  }

  // 空状态
  if (agents.length === 0) {
    return <EmptyState onCreateAgent={onCreateAgent} />;
  }

  return (
    <div
      className={cn(
        'grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4',
        className
      )}
    >
      {agents.map((agent) => (
        <AgentCard
          key={agent.id}
          agent={agent}
          onClick={onAgentClick}
          showEditButton={showEditButton}
          onEdit={onEdit}
          showDuplicateButton={showDuplicateButton}
          onDuplicate={onDuplicate}
          showToggleButton={showToggleButton}
          onToggle={onToggle}
          showDeleteButton={showDeleteButton}
          onDelete={onDelete}
        />
      ))}
    </div>
  );
}

export default AgentList;