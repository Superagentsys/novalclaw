/**
 * 代理状态徽章组件
 *
 * 显示代理的三种状态：活动、停用、已归档
 * 使用不同颜色的圆点和文本标签表示状态
 *
 * [Source: ux-design-specification.md#核心组件]
 * [Source: 2-6-agent-list-card.md]
 */

import * as React from 'react';
import { cn } from '@/lib/utils';
import { type AgentStatus } from '@/types/agent';

// ============================================================================
// 类型定义
// ============================================================================

/**
 * AgentStatusBadge 组件属性
 */
export interface AgentStatusBadgeProps {
  /** 代理状态 */
  status: AgentStatus;
  /** 徽章尺寸 */
  size?: 'sm' | 'md' | 'lg';
  /** 自定义类名 */
  className?: string;
}

// ============================================================================
// 常量
// ============================================================================

/**
 * 状态配置
 */
const STATUS_CONFIG: Record<
  AgentStatus,
  {
    dot: string;
    text: string;
    label: string;
    ariaLabel: string;
  }
> = {
  active: {
    dot: 'bg-green-500',
    text: 'text-green-600',
    label: '活动',
    ariaLabel: '代理状态: 活动',
  },
  inactive: {
    dot: 'bg-gray-400',
    text: 'text-gray-500',
    label: '停用',
    ariaLabel: '代理状态: 停用',
  },
  archived: {
    dot: 'bg-amber-500',
    text: 'text-amber-600',
    label: '已归档',
    ariaLabel: '代理状态: 已归档',
  },
} as const;

/**
 * 尺寸配置
 */
const SIZE_CONFIG: Record<string, { container: string; dot: string }> = {
  sm: {
    container: 'text-xs gap-1',
    dot: 'w-1.5 h-1.5',
  },
  md: {
    container: 'text-sm gap-1.5',
    dot: 'w-2 h-2',
  },
  lg: {
    container: 'text-base gap-2',
    dot: 'w-2.5 h-2.5',
  },
} as const;

// ============================================================================
// 主组件
// ============================================================================

/**
 * 代理状态徽章
 *
 * @example
 * ```tsx
 * // 基础用法
 * <AgentStatusBadge status="active" />
 *
 * // 不同尺寸
 * <AgentStatusBadge status="inactive" size="sm" />
 * <AgentStatusBadge status="archived" size="lg" />
 *
 * // 自定义样式
 * <AgentStatusBadge status="active" className="my-2" />
 * ```
 */
export function AgentStatusBadge({
  status,
  size = 'md',
  className,
}: AgentStatusBadgeProps): React.ReactElement {
  const config = STATUS_CONFIG[status];
  const sizeConfig = SIZE_CONFIG[size];

  return (
    <span
      className={cn(
        'inline-flex items-center font-medium',
        config.text,
        sizeConfig.container,
        className
      )}
      role="status"
      aria-label={config.ariaLabel}
    >
      {/* 状态圆点 */}
      <span
        className={cn(
          'rounded-full flex-shrink-0',
          config.dot,
          sizeConfig.dot
        )}
        aria-hidden="true"
      />
      {/* 状态文本 */}
      <span>{config.label}</span>
    </span>
  );
}

export default AgentStatusBadge;