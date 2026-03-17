/**
 * 代理卡片组件
 *
 * 显示单个 AI 代理的信息卡片，包括:
 * - 代理名称和描述
 * - MBTI 人格类型徽章
 * - 状态指示器
 * - 专业领域标签
 * - 人格主题色边框
 *
 * [Source: ux-design-specification.md#核心组件]
 * [Source: 2-6-agent-list-card.md]
 */

import * as React from 'react';
import { useCallback } from 'react';
import { cn } from '@/lib/utils';
import { personalityColors } from '@/lib/personality-colors';
import { AgentStatusBadge } from './AgentStatusBadge';
import { UserCircle, Pencil, Copy, Power, Trash2 } from 'lucide-react';
import {
  type AgentModel,
} from '@/types/agent';

// ============================================================================
// 类型定义
// ============================================================================

/**
 * AgentCard 组件属性
 */
export interface AgentCardProps {
  /** 代理数据 */
  agent: AgentModel;
  /** 点击回调 */
  onClick?: (agent: AgentModel) => void;
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
// 主组件
// ============================================================================

/**
 * 代理卡片组件
 *
 * @example
 * ```tsx
 * // 基础用法
 * <AgentCard agent={agent} onClick={(a) => navigate(`/agents/${a.agent_uuid}`)} />
 *
 * // 自定义样式
 * <AgentCard agent={agent} className="shadow-lg" />
 * ```
 */
export function AgentCard({
  agent,
  onClick,
  className,
  showEditButton,
  onEdit,
  showDuplicateButton,
  onDuplicate,
  showToggleButton,
  onToggle,
  showDeleteButton,
  onDelete,
}: AgentCardProps): React.ReactElement {
  // 获取人格主题色
  const themeColor = agent.mbti_type
    ? personalityColors[agent.mbti_type].primary
    : undefined;

  // 处理点击事件
  const handleClick = useCallback(() => {
    onClick?.(agent);
  }, [agent, onClick]);

  // 处理键盘事件
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        onClick?.(agent);
      }
    },
    [agent, onClick]
  );

  // 处理编辑按钮点击
  const handleEditClick = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation(); // 阻止事件冒泡，避免触发卡片点击
      onEdit?.(agent);
    },
    [agent, onEdit]
  );

  // 处理复制按钮点击
  const handleDuplicateClick = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation(); // 阻止事件冒泡，避免触发卡片点击
      onDuplicate?.(agent);
    },
    [agent, onDuplicate]
  );

  // 处理状态切换按钮点击
  const handleToggleClick = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation(); // 阻止事件冒泡，避免触发卡片点击
      onToggle?.(agent);
    },
    [agent, onToggle]
  );

  // 处理删除按钮点击
  const handleDeleteClick = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation(); // 阻止事件冒泡，避免触发卡片点击
      onDelete?.(agent);
    },
    [agent, onDelete]
  );

  // 判断代理是否停用状态
  const isInactive = agent.status === 'inactive';

  return (
    <button
      type="button"
      className={cn(
        // 基础样式
        'w-full text-left p-4 rounded-lg',
        'bg-card border border-border/50',
        'transition-all duration-200 ease-in-out',
        'focus:outline-none focus:ring-2 focus:ring-primary/30 focus:ring-offset-2',
        // 边框主题色
        'border-l-4',
        // Hover 效果
        'hover:shadow-md hover:scale-[1.02] hover:border-border/70',
        // 禁用状态
        'disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:scale-100',
        // 停用状态视觉反馈
        isInactive && 'opacity-60',
        className
      )}
      style={{
        borderLeftColor: themeColor || undefined,
      }}
      onClick={handleClick}
      onKeyDown={handleKeyDown}
      aria-label={`查看代理: ${agent.name}`}
    >
      {/* 卡片头部: 状态和名称 */}
      <div className="flex items-start justify-between gap-2 mb-2">
        {/* 左侧: 图标和名称 */}
        <div className="flex items-center gap-3 min-w-0">
          {/* 代理图标 */}
          <div
            className="flex-shrink-0 w-10 h-10 rounded-full flex items-center justify-center bg-muted/50"
            style={{
              color: themeColor,
            }}
          >
            <UserCircle className="w-6 h-6" />
          </div>
          {/* 名称 */}
          <h3 className="text-base font-semibold text-foreground truncate">
            {agent.name}
          </h3>
        </div>
        {/* 右侧: 操作按钮和状态 */}
        <div className="flex-shrink-0 flex items-center gap-2">
          {/* 删除按钮 */}
          {showDeleteButton && (
            <button
              type="button"
              onClick={handleDeleteClick}
              className="p-1.5 rounded-md hover:bg-destructive/10 transition-colors text-muted-foreground hover:text-destructive"
              aria-label={`删除代理: ${agent.name}`}
            >
              <Trash2 className="w-4 h-4" />
            </button>
          )}
          {/* 状态切换按钮 - archived 状态不显示 */}
          {showToggleButton && agent.status !== 'archived' && (
            <button
              type="button"
              onClick={handleToggleClick}
              className={cn(
                "p-1.5 rounded-md hover:bg-muted/50 transition-colors",
                isInactive && "opacity-50"
              )}
              style={{ color: themeColor }}
              aria-label={`切换代理状态: ${agent.name}`}
            >
              <Power className="w-4 h-4" />
            </button>
          )}
          {/* 复制按钮 */}
          {showDuplicateButton && (
            <button
              type="button"
              onClick={handleDuplicateClick}
              className="p-1.5 rounded-md hover:bg-muted/50 transition-colors"
              style={{ color: themeColor }}
              aria-label={`复制代理: ${agent.name}`}
            >
              <Copy className="w-4 h-4" />
            </button>
          )}
          {/* 编辑按钮 */}
          {showEditButton && (
            <button
              type="button"
              onClick={handleEditClick}
              className="p-1.5 rounded-md hover:bg-muted/50 transition-colors"
              style={{ color: themeColor }}
              aria-label={`编辑代理: ${agent.name}`}
            >
              <Pencil className="w-4 h-4" />
            </button>
          )}
          {/* 状态徽章 */}
          <AgentStatusBadge status={agent.status} size="sm" />
        </div>
      </div>

      {/* 描述 */}
      {agent.description && (
        <p className="text-sm text-muted-foreground line-clamp-2 mb-3 pl-[52px]">
          {agent.description}
        </p>
      )}

      {/* 底部标签 */}
      <div className="flex flex-wrap items-center gap-2 pl-[52px]">
        {/* MBTI 类型徽章 */}
        {agent.mbti_type && (
          <span
            className="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium"
            style={{
              backgroundColor: `${themeColor}20`,
              color: themeColor,
            }}
          >
            {agent.mbti_type}
          </span>
        )}
        {/* 专业领域 */}
        {agent.domain && (
          <span className="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-muted/50 text-muted-foreground">
            {agent.domain}
          </span>
        )}
      </div>
    </button>
  );
}

export default AgentCard;