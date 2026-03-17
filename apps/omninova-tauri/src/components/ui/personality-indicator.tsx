/**
 * 人格指示器组件
 *
 * 显示 MBTI 人格类型的视觉表示，使用对应人格类型的颜色
 * 支持不同尺寸和样式变体
 *
 * [Source: ux-design-specification.md#核心组件]
 */

import * as React from 'react';
import { cn } from '@/lib/utils';
import {
  type MBTIType,
  getPersonalityColors,
  personalityColors,
  personalityCategories,
} from '@/lib/personality-colors';

/** 组件尺寸 */
export type PersonalityIndicatorSize = 'sm' | 'md' | 'lg';

/** 组件变体 */
export type PersonalityIndicatorVariant = 'badge' | 'chip' | 'card' | 'minimal';

/** 组件属性 */
export interface PersonalityIndicatorProps {
  /** MBTI 人格类型 */
  type: MBTIType;
  /** 尺寸 */
  size?: PersonalityIndicatorSize;
  /** 变体样式 */
  variant?: PersonalityIndicatorVariant;
  /** 是否显示描述 */
  showDescription?: boolean;
  /** 是否显示分类标签 */
  showCategory?: boolean;
  /** 自定义类名 */
  className?: string;
  /** 点击事件 */
  onClick?: () => void;
}

/** 尺寸配置 */
const sizeConfig: Record<
  PersonalityIndicatorSize,
  {
    container: string;
    text: string;
    indicator: string;
  }
> = {
  sm: {
    container: 'px-2 py-1 gap-1.5',
    text: 'text-xs font-medium',
    indicator: 'w-2 h-2',
  },
  md: {
    container: 'px-3 py-1.5 gap-2',
    text: 'text-sm font-medium',
    indicator: 'w-2.5 h-2.5',
  },
  lg: {
    container: 'px-4 py-2 gap-2.5',
    text: 'text-base font-medium',
    indicator: 'w-3 h-3',
  },
};

/** 分类名称映射 */
const categoryNames: Record<string, string> = {
  analysts: '分析型',
  diplomats: '外交型',
  sentinels: '守护型',
  explorers: '探索型',
};

/**
 * 人格指示器组件
 *
 * @example
 * ```tsx
 * // 基础用法
 * <PersonalityIndicator type="INTJ" />
 *
 * // 带描述的卡片样式
 * <PersonalityIndicator type="ENFP" variant="card" showDescription />
 *
 * // 点击切换主题
 * <PersonalityIndicator
 *   type="ISTP"
 *   onClick={() => applyTheme('ISTP')}
 * />
 * ```
 */
export function PersonalityIndicator({
  type,
  size = 'md',
  variant = 'badge',
  showDescription = false,
  showCategory = false,
  className,
  onClick,
}: PersonalityIndicatorProps): React.ReactElement {
  const colors = getPersonalityColors(type);
  const sizes = sizeConfig[size];

  // Minimal 变体：仅显示类型名称和颜色指示器
  if (variant === 'minimal') {
    return (
      <span
        className={cn(
          'inline-flex items-center gap-1.5',
          sizes.text,
          className
        )}
      >
        <span
          className={cn('rounded-full', sizes.indicator)}
          style={{ backgroundColor: colors.primary }}
          aria-hidden="true"
        />
        <span>{type}</span>
      </span>
    );
  }

  // Chip 变体：紧凑的圆角标签
  if (variant === 'chip') {
    return (
      <span
        className={cn(
          'inline-flex items-center rounded-full',
          sizes.container,
          sizes.text,
          'transition-colors',
          onClick && 'cursor-pointer hover:opacity-80',
          className
        )}
        style={{
          backgroundColor: `${colors.primary}15`,
          borderLeft: `3px solid ${colors.primary}`,
        }}
        onClick={onClick}
        role={onClick ? 'button' : undefined}
        tabIndex={onClick ? 0 : undefined}
        onKeyDown={
          onClick
            ? (e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  onClick();
                }
              }
            : undefined
        }
      >
        <span style={{ color: colors.primary }} className="font-bold">
          {type}
        </span>
      </span>
    );
  }

  // Card 变体：带背景的卡片样式
  if (variant === 'card') {
    return (
      <div
        className={cn(
          'flex flex-col rounded-lg border',
          size === 'sm' && 'p-2 gap-1',
          size === 'md' && 'p-3 gap-1.5',
          size === 'lg' && 'p-4 gap-2',
          onClick && 'cursor-pointer hover:shadow-md transition-shadow',
          className
        )}
        style={{
          borderColor: `${colors.primary}30`,
          backgroundColor: `${colors.primary}08`,
        }}
        onClick={onClick}
        role={onClick ? 'button' : undefined}
        tabIndex={onClick ? 0 : undefined}
        onKeyDown={
          onClick
            ? (e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  onClick();
                }
              }
            : undefined
        }
      >
        {/* 类型名称和颜色指示器 */}
        <div className="flex items-center gap-2">
          <span
            className={cn('rounded-full', sizes.indicator)}
            style={{ backgroundColor: colors.primary }}
            aria-hidden="true"
          />
          <span
            className={cn('font-bold', sizes.text)}
            style={{ color: colors.primary }}
          >
            {type}
          </span>
          {showCategory && (
            <span className="text-xs text-muted-foreground">
              {categoryNames[colors.category]}
            </span>
          )}
        </div>

        {/* 描述 */}
        {showDescription && (
          <p className="text-xs text-muted-foreground">{colors.description}</p>
        )}
      </div>
    );
  }

  // Badge 变体（默认）：圆角徽章
  return (
    <span
      className={cn(
        'inline-flex items-center rounded-md',
        sizes.container,
        sizes.text,
        'transition-colors',
        onClick && 'cursor-pointer hover:opacity-80',
        className
      )}
      style={{
        backgroundColor: `${colors.primary}15`,
        borderLeft: `3px solid ${colors.primary}`,
      }}
      onClick={onClick}
      role={onClick ? 'button' : undefined}
      tabIndex={onClick ? 0 : undefined}
      onKeyDown={
        onClick
          ? (e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                onClick();
              }
            }
          : undefined
      }
    >
      <span
        className={cn('rounded-full', sizes.indicator)}
        style={{ backgroundColor: colors.primary }}
        aria-hidden="true"
      />
      <span style={{ color: colors.primary }} className="font-bold">
        {type}
      </span>
      {showCategory && (
        <span className="text-muted-foreground ml-1">
          · {categoryNames[colors.category]}
        </span>
      )}
    </span>
  );
}

/**
 * 人格类型选择器组件
 *
 * 显示所有 16 种人格类型供选择
 */
export interface PersonalitySelectorProps {
  /** 当前选中类型 */
  value?: MBTIType;
  /** 选择回调 */
  onChange: (type: MBTIType) => void;
  /** 尺寸 */
  size?: PersonalityIndicatorSize;
  /** 自定义类名 */
  className?: string;
}

export function PersonalitySelector({
  value,
  onChange,
  size: _size = 'md',
  className,
}: PersonalitySelectorProps): React.ReactElement {
  const categories: Array<'analysts' | 'diplomats' | 'sentinels' | 'explorers'> = [
    'analysts',
    'diplomats',
    'sentinels',
    'explorers',
  ];

  // 按分类分组类型
  const typesByCategory = categories.reduce(
    (acc, category) => {
      acc[category] = Object.entries(personalityColors)
        .filter(([, config]) => config.category === category)
        .map(([type]) => type as MBTIType);
      return acc;
    },
    {} as Record<string, MBTIType[]>
  );

  return (
    <div className={cn('space-y-3', className)}>
      {categories.map((category) => (
        <div key={category}>
          <h4 className="text-xs font-medium text-muted-foreground mb-1.5">
            {categoryNames[category]} - {personalityCategories[category].description}
          </h4>
          <div className="flex flex-wrap gap-1.5">
            {typesByCategory[category].map((type) => {
              const colors = getPersonalityColors(type);
              const isSelected = value === type;

              return (
                <button
                  key={type}
                  type="button"
                  onClick={() => onChange(type)}
                  className={cn(
                    'inline-flex items-center rounded-full px-2.5 py-1 text-sm font-medium',
                    'transition-all hover:scale-105',
                    isSelected && 'ring-2 ring-offset-1'
                  )}
                  style={{
                    backgroundColor: `${colors.primary}15`,
                    color: colors.primary,
                    ...(isSelected && { '--tw-ring-color': colors.primary } as React.CSSProperties),
                  }}
                >
                  {type}
                </button>
              );
            })}
          </div>
        </div>
      ))}
    </div>
  );
}

export default PersonalityIndicator;