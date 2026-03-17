/**
 * MBTI 人格类型选择器组件
 *
 * 提供 16 种 MBTI 人格类型的可视化选择界面
 * 支持:
 * - 分类筛选（分析型、外交型、守护型、探索型）
 * - 搜索功能（按名称、描述、类型代码搜索）
 * - 键盘导航（方向键、Tab、Enter、Escape）
 * - 人格类型对应的主题色
 *
 * [Source: ux-design-specification.md#核心组件]
 * [Source: 2-3-mbti-selector-component.md]
 */

import * as React from 'react';
import { useState, useMemo, useRef, useCallback } from 'react';
import { cn } from '@/lib/utils';
import {
  type MBTIType,
  type PersonalityCategory,
  personalityColors,
  allMBTITypes,
} from '@/lib/personality-colors';
import { Input } from '@/components/ui/input';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';

// ============================================================================
// 类型定义
// ============================================================================

/**
 * MBTISelector 组件属性
 */
export interface MBTISelectorProps {
  /** 当前选中的 MBTI 类型 */
  value?: MBTIType;
  /** 选择回调 */
  onChange: (type: MBTIType) => void;
  /** 是否禁用 */
  disabled?: boolean;
  /** 自定义类名 */
  className?: string;
}

// ============================================================================
// 常量
// ============================================================================

/** 分类 ID 到中文标签的映射 */
const CATEGORY_LABELS: Record<PersonalityCategory, string> = {
  analysts: '分析型',
  diplomats: '外交型',
  sentinels: '守护型',
  explorers: '探索型',
};

/** 防抖延迟（毫秒） */
const DEBOUNCE_DELAY = 300;

/** 网格列数（用于键盘导航） */
const GRID_COLUMNS = 4;

// ============================================================================
// 工具函数
// ============================================================================

/**
 * 自定义防抖 Hook
 */
function useDebounce<T>(value: T, delay: number): T {
  const [debouncedValue, setDebouncedValue] = useState(value);

  React.useEffect(() => {
    const timer = setTimeout(() => setDebouncedValue(value), delay);
    return () => clearTimeout(timer);
  }, [value, delay]);

  return debouncedValue;
}

/**
 * 检查类型是否匹配搜索词
 */
function matchesSearch(type: MBTIType, query: string): boolean {
  const lowerQuery = query.toLowerCase();
  const config = personalityColors[type];

  return (
    type.toLowerCase().includes(lowerQuery) ||
    config.name.toLowerCase().includes(lowerQuery) ||
    config.description.toLowerCase().includes(lowerQuery)
  );
}

// ============================================================================
// 子组件
// ============================================================================

interface TypeButtonProps {
  type: MBTIType;
  isSelected: boolean;
  isFocused: boolean;
  disabled?: boolean;
  onClick: () => void;
  onFocus: () => void;
}

/**
 * 类型按钮组件
 */
function TypeButton({
  type,
  isSelected,
  isFocused,
  disabled,
  onClick,
  onFocus,
}: TypeButtonProps): React.ReactElement {
  const config = personalityColors[type];
  const color = config.primary;

  return (
    <button
      type="button"
      data-type={type}
      disabled={disabled}
      onClick={onClick}
      onFocus={onFocus}
      aria-pressed={isSelected}
      className={cn(
        'relative flex flex-col items-center justify-center',
        'rounded-lg border-2 p-3 min-h-[80px]',
        'transition-all duration-200',
        'hover:scale-105 hover:shadow-md',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
        'disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:scale-100',
        isSelected && 'ring-2 ring-offset-2',
        isFocused && 'ring-2 ring-ring'
      )}
      style={{
        backgroundColor: `${color}10`,
        borderColor: isSelected || isFocused ? color : `${color}30`,
        '--type-color': color,
      } as React.CSSProperties}
    >
      {/* 类型代码 */}
      <span
        className="text-lg font-bold"
        style={{ color }}
      >
        {type}
      </span>

      {/* 类型名称 */}
      <span className="text-xs text-muted-foreground mt-1">
        {config.name}
      </span>

      {/* 选中指示器 */}
      {isSelected && (
        <span
          className="absolute top-1 right-1 w-2 h-2 rounded-full"
          style={{ backgroundColor: color }}
          aria-hidden="true"
        />
      )}
    </button>
  );
}

// ============================================================================
// 主组件
// ============================================================================

/**
 * MBTI 人格类型选择器
 *
 * @example
 * ```tsx
 * // 基础用法
 * <MBTISelector
 *   value={selectedType}
 *   onChange={(type) => setSelectedType(type)}
 * />
 *
 * // 禁用状态
 * <MBTISelector
 *   onChange={handleChange}
 *   disabled
 * />
 * ```
 */
export function MBTISelector({
  value,
  onChange,
  disabled = false,
  className,
}: MBTISelectorProps): React.ReactElement {
  // ============================================================================
  // 状态
  // ============================================================================

  /** 当前选中的分类 */
  const [selectedCategory, setSelectedCategory] = useState<PersonalityCategory | 'all'>('all');

  /** 搜索关键词 */
  const [searchQuery, setSearchQuery] = useState('');

  /** 键盘焦点索引 */
  const [focusedIndex, setFocusedIndex] = useState(0);

  // ============================================================================
  // Refs
  // ============================================================================

  const gridRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  // ============================================================================
  // 副作用
  // ============================================================================

  // 防抖搜索
  const debouncedSearch = useDebounce(searchQuery, DEBOUNCE_DELAY);

  // ============================================================================
  // 计算属性
  // ============================================================================

  /** 根据分类和搜索筛选的类型列表 */
  const filteredTypes = useMemo(() => {
    let types = allMBTITypes;

    // 按分类筛选
    if (selectedCategory !== 'all') {
      types = types.filter(
        (type) => personalityColors[type].category === selectedCategory
      );
    }

    // 按搜索词筛选
    if (debouncedSearch) {
      types = types.filter((type) => matchesSearch(type, debouncedSearch));
    }

    return types;
  }, [selectedCategory, debouncedSearch]);

  /** 分类统计 */
  const categoryCounts = useMemo(() => {
    const counts: Record<PersonalityCategory | 'all', number> = {
      all: allMBTITypes.length,
      analysts: 0,
      diplomats: 0,
      sentinels: 0,
      explorers: 0,
    };

    allMBTITypes.forEach((type) => {
      const category = personalityColors[type].category;
      counts[category]++;
    });

    return counts;
  }, []);

  // ============================================================================
  // 事件处理
  // ============================================================================

  /** 处理类型选择 */
  const handleTypeSelect = useCallback(
    (type: MBTIType) => {
      if (!disabled) {
        onChange(type);
      }
    },
    [disabled, onChange]
  );

  /** 处理键盘导航 */
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      // 如果焦点在搜索框，只处理 Escape
      if (document.activeElement === searchInputRef.current) {
        if (e.key === 'Escape') {
          e.preventDefault();
          setSearchQuery('');
          searchInputRef.current?.blur();
        }
        return;
      }

      switch (e.key) {
        case 'ArrowRight':
          e.preventDefault();
          setFocusedIndex((prev) => Math.min(prev + 1, filteredTypes.length - 1));
          break;

        case 'ArrowLeft':
          e.preventDefault();
          setFocusedIndex((prev) => Math.max(prev - 1, 0));
          break;

        case 'ArrowDown':
          e.preventDefault();
          setFocusedIndex((prev) =>
            Math.min(prev + GRID_COLUMNS, filteredTypes.length - 1)
          );
          break;

        case 'ArrowUp':
          e.preventDefault();
          setFocusedIndex((prev) => Math.max(prev - GRID_COLUMNS, 0));
          break;

        case 'Enter':
        case ' ':
          e.preventDefault();
          if (filteredTypes[focusedIndex]) {
            handleTypeSelect(filteredTypes[focusedIndex]);
          }
          break;

        case 'Escape':
          e.preventDefault();
          setSearchQuery('');
          break;
      }
    },
    [filteredTypes, focusedIndex, handleTypeSelect]
  );

  // 焦点自动滚动
  React.useEffect(() => {
    if (gridRef.current) {
      const focusedButton = gridRef.current.querySelector(
        `[data-type="${filteredTypes[focusedIndex]}"]`
      ) as HTMLElement;
      // 检查 scrollIntoView 是否可用（jsdom 环境可能不支持）
      if (focusedButton && typeof focusedButton.scrollIntoView === 'function') {
        focusedButton.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      }
    }
  }, [focusedIndex, filteredTypes]);

  // ============================================================================
  // 渲染
  // ============================================================================

  return (
    <div className={cn('space-y-4', className)}>
      {/* 搜索栏 */}
      <div className="relative">
        <Input
          ref={searchInputRef}
          type="text"
          placeholder="搜索人格类型..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          disabled={disabled}
          className="pr-8"
        />
        {searchQuery && (
          <button
            type="button"
            onClick={() => setSearchQuery('')}
            className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
            aria-label="清除搜索"
          >
            ✕
          </button>
        )}
      </div>

      {/* 分类标签 */}
      <Tabs
        value={selectedCategory}
        onValueChange={(v) => setSelectedCategory(v as PersonalityCategory | 'all')}
      >
        <TabsList className="w-full justify-start flex-wrap">
          <TabsTrigger value="all">
            全部 ({categoryCounts.all})
          </TabsTrigger>
          {(
            ['analysts', 'diplomats', 'sentinels', 'explorers'] as PersonalityCategory[]
          ).map((category) => (
            <TabsTrigger key={category} value={category}>
              {CATEGORY_LABELS[category]} ({categoryCounts[category]})
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      {/* 类型网格 */}
      <div
        ref={gridRef}
        className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-2"
        onKeyDown={handleKeyDown}
        role="grid"
        aria-label="MBTI 人格类型选择"
      >
        {filteredTypes.map((type, index) => (
          <TypeButton
            key={type}
            type={type}
            isSelected={value === type}
            isFocused={focusedIndex === index}
            disabled={disabled}
            onClick={() => handleTypeSelect(type)}
            onFocus={() => setFocusedIndex(index)}
          />
        ))}
      </div>

      {/* 无结果提示 */}
      {filteredTypes.length === 0 && (
        <div className="text-center py-8 text-muted-foreground">
          <p>没有找到匹配的类型</p>
          <p className="text-sm mt-1">请尝试其他搜索词</p>
        </div>
      )}
    </div>
  );
}

export default MBTISelector;