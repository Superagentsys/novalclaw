/**
 * 人格预览组件
 *
 * 显示所选 MBTI 人格类型的详细特征，包括:
 * - 认知功能栈
 * - 示例对话风格
 * - 优势和潜在盲点
 * - 推荐应用场景
 *
 * [Source: ux-design-specification.md#核心组件]
 * [Source: 2-4-personality-preview-component.md]
 */

import * as React from 'react';
import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { cn } from '@/lib/utils';
import {
  type MBTIType,
  personalityColors,
} from '@/lib/personality-colors';
import { PersonalityIndicator } from '@/components/ui/personality-indicator';
import { Loader2, RefreshCw, Check, AlertTriangle } from 'lucide-react';

// ============================================================================
// 类型定义
// ============================================================================

/**
 * 人格配置（来自 Rust 后端）
 */
interface PersonalityConfig {
  description: string;
  system_prompt_template: string;
  strengths: string[];
  blind_spots: string[];
  recommended_use_cases: string[];
  theme_color: string;
  accent_color: string;
}

/**
 * 认知功能栈
 */
interface FunctionStack {
  dominant: string;
  auxiliary: string;
  tertiary: string;
  inferior: string;
}

/**
 * 行为倾向
 */
interface BehaviorTendency {
  decision_making: string;
  information_processing: string;
  social_interaction: string;
  stress_response: string;
}

/**
 * 沟通风格
 */
interface CommunicationStyle {
  preference: string;
  language_traits: string[];
  feedback_style: string;
}

/**
 * 人格特征
 */
interface PersonalityTraits {
  function_stack: FunctionStack;
  behavior_tendency: BehaviorTendency;
  communication_style: CommunicationStyle;
}

/**
 * PersonalityPreview 组件属性
 */
export interface PersonalityPreviewProps {
  /** MBTI 人格类型 */
  mbtiType: MBTIType;
  /** 自定义类名 */
  className?: string;
}

// ============================================================================
// 常量
// ============================================================================

/** 功能角色标签 */
const FUNCTION_ROLES: Record<keyof FunctionStack, string> = {
  dominant: '主导',
  auxiliary: '辅助',
  tertiary: '第三',
  inferior: '劣势',
};

/** 功能图标映射 */
const FUNCTION_ICONS: Record<string, string> = {
  Ni: '🔮', // 内倾直觉 - 洞察
  Ne: '💡', // 外倾直觉 - 创意
  Si: '📚', // 内倾感觉 - 经验
  Se: '🎯', // 外倾感觉 - 行动
  Ti: '🧠', // 内倾思考 - 分析
  Te: '📊', // 外倾思考 - 效率
  Fi: '💚', // 内倾情感 - 价值
  Fe: '🤝', // 外倾情感 - 和谐
};

/** 示例对话模板 */
const EXAMPLE_DIALOGUES: Record<string, { style: string; example: string }> = {
  // 分析型
  INTJ: {
    style: '直接、结构化、注重效率',
    example: '基于您的需求，我建议采用分阶段实施策略。首先分析核心问题，然后制定详细的时间线和里程碑...',
  },
  INTP: {
    style: '分析性、探索性、逻辑严密',
    example: '这个问题有几种可能的解决方案。让我分析每种方案的优缺点...',
  },
  ENTJ: {
    style: '果断、目标导向、高效',
    example: '我们直奔主题。这是目标，这是执行计划，这是我需要的资源...',
  },
  ENTP: {
    style: '辩论性、创新、充满活力',
    example: '有趣的观点！但您是否考虑过另一种可能性？让我提出一个新的角度...',
  },
  // 外交型
  INFJ: {
    style: '深思熟虑、富有洞察力、温和',
    example: '我理解您的感受。从长远来看，这个决定可能会对您的人际关系产生深远影响...',
  },
  INFP: {
    style: '真诚、富有同理心、理想主义',
    example: '这让我想到了您真正重视的东西。让我们一起探索什么对您来说最重要...',
  },
  ENFJ: {
    style: '鼓舞人心、关注他人、有说服力',
    example: '我相信您有能力做到！让我帮您理清思路，找到前进的方向...',
  },
  ENFP: {
    style: '热情、创意丰富、充满感染力',
    example: '太棒了！我看到了无限的可能性！让我们头脑风暴一下...',
  },
  // 守护型
  ISTJ: {
    style: '务实、可靠、注重细节',
    example: '根据已有的流程和规范，我建议按以下步骤执行...',
  },
  ISFJ: {
    style: '体贴、负责、关注细节',
    example: '我已经考虑到了所有细节。让我确保每一步都符合您的期望...',
  },
  ESTJ: {
    style: '高效、有条理、结果导向',
    example: '让我们立即行动。这是计划和责任分工，我将确保按时完成...',
  },
  ESFJ: {
    style: '友善、乐于助人、注重和谐',
    example: '我很乐意帮助！让我们一起解决这个问题，确保每个人都满意...',
  },
  // 探索型
  ISTP: {
    style: '实用、灵活、动手能力强',
    example: '让我看看问题出在哪里...我有一个解决方案，可以试试看...',
  },
  ISFP: {
    style: '温和、灵活、注重当下',
    example: '我觉得可以从这个角度尝试...让我们看看效果如何...',
  },
  ESTP: {
    style: '直接、务实、充满活力',
    example: '行动胜于言辞！让我们直接开始，边做边调整...',
  },
  ESFP: {
    style: '热情、活泼、注重体验',
    example: '这听起来很有趣！让我们享受这个过程，看看会有什么惊喜...',
  },
};

// ============================================================================
// 子组件
// ============================================================================

interface FunctionStackItemProps {
  functionCode: string;
  role: string;
  isPrimary?: boolean;
  themeColor: string;
}

/**
 * 认知功能项组件
 */
function FunctionStackItem({
  functionCode,
  role,
  isPrimary = false,
  themeColor,
}: FunctionStackItemProps): React.ReactElement {
  const icon = FUNCTION_ICONS[functionCode] || '⬡';

  return (
    <div
      className={cn(
        'flex flex-col items-center justify-center rounded-lg p-3 min-w-[70px]',
        'transition-all duration-200 hover:scale-105',
        isPrimary && 'ring-2 ring-offset-1'
      )}
      style={{
        backgroundColor: `${themeColor}15`,
        borderColor: isPrimary ? themeColor : `${themeColor}30`,
        '--tw-ring-color': themeColor,
      } as React.CSSProperties}
    >
      <span className="text-lg">{icon}</span>
      <span className="text-sm font-bold mt-1" style={{ color: themeColor }}>
        {functionCode}
      </span>
      <span className="text-xs text-muted-foreground">{role}</span>
    </div>
  );
}

interface TraitListProps {
  title: string;
  items: string[];
  icon: 'check' | 'warning';
  themeColor: string;
}

/**
 * 特征列表组件
 */
function TraitList({
  title,
  items,
  icon,
  themeColor,
}: TraitListProps): React.ReactElement {
  const IconComponent = icon === 'check' ? Check : AlertTriangle;
  const iconColor = icon === 'check' ? themeColor : '#EAB308';

  return (
    <div className="space-y-2">
      <h4 className="text-sm font-medium flex items-center gap-2">
        <IconComponent className="w-4 h-4" style={{ color: iconColor }} />
        {title}
      </h4>
      <ul className="space-y-1.5">
        {items.map((item, index) => (
          <li
            key={index}
            className="text-sm text-muted-foreground flex items-start gap-2"
          >
            <span
              className="w-1.5 h-1.5 rounded-full mt-1.5 flex-shrink-0"
              style={{ backgroundColor: iconColor }}
            />
            {item}
          </li>
        ))}
      </ul>
    </div>
  );
}

// ============================================================================
// 主组件
// ============================================================================

/**
 * 人格预览组件
 *
 * @example
 * ```tsx
 * // 基础用法
 * <PersonalityPreview mbtiType="INTJ" />
 *
 * // 自定义样式
 * <PersonalityPreview mbtiType="ENFP" className="max-w-md" />
 * ```
 */
export function PersonalityPreview({
  mbtiType,
  className,
}: PersonalityPreviewProps): React.ReactElement {
  // ============================================================================
  // 状态
  // ============================================================================

  const [config, setConfig] = useState<PersonalityConfig | null>(null);
  const [traits, setTraits] = useState<PersonalityTraits | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // ============================================================================
  // 数据加载
  // ============================================================================

  const loadData = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      // 并行请求配置和特征数据
      const [configResult, traitsResult] = await Promise.all([
        invoke<PersonalityConfig>('get_mbti_config', { mbtiType }),
        invoke<PersonalityTraits>('get_mbti_traits', { mbtiType }),
      ]);

      setConfig(configResult);
      setTraits(traitsResult);
    } catch (err) {
      setError(err instanceof Error ? err.message : '加载失败');
    } finally {
      setIsLoading(false);
    }
  }, [mbtiType]);

  // 初始加载和类型变化时重新加载
  useEffect(() => {
    loadData();
  }, [loadData]);

  // ============================================================================
  // 计算属性
  // ============================================================================

  const colorConfig = personalityColors[mbtiType];
  const themeColor = config?.theme_color || colorConfig.primary;
  const exampleDialogue = EXAMPLE_DIALOGUES[mbtiType];

  // ============================================================================
  // 渲染
  // ============================================================================

  // 加载状态
  if (isLoading) {
    return (
      <div
        className={cn(
          'flex items-center justify-center p-8 rounded-lg',
          'bg-muted/30',
          className
        )}
        aria-label="加载中"
      >
        <Loader2 className="w-6 h-6 animate-spin text-muted-foreground" />
        <span className="ml-2 text-muted-foreground">加载中...</span>
      </div>
    );
  }

  // 错误状态
  if (error) {
    return (
      <div
        className={cn(
          'flex flex-col items-center justify-center p-8 rounded-lg',
          'bg-destructive/10 text-destructive',
          className
        )}
        role="alert"
      >
        <AlertTriangle className="w-6 h-6 mb-2" />
        <p className="text-sm font-medium">加载失败</p>
        <p className="text-xs mt-1 mb-3">{error}</p>
        <button
          type="button"
          onClick={loadData}
          className="flex items-center gap-2 px-4 py-2 rounded-md bg-destructive text-destructive-foreground hover:opacity-90 transition-opacity"
          aria-label="重试"
        >
          <RefreshCw className="w-4 h-4" />
          重试
        </button>
      </div>
    );
  }

  return (
    <div
      className={cn('space-y-6 rounded-lg p-4', className)}
      style={{
        borderColor: `${themeColor}30`,
        backgroundColor: `${themeColor}05`,
      }}
    >
      {/* 头部：类型信息 */}
      <div className="flex items-center justify-between">
        <PersonalityIndicator
          type={mbtiType}
          variant="card"
          size="lg"
          showDescription
          showCategory
        />
      </div>

      {/* 认知功能栈 */}
      {traits?.function_stack && (
        <section aria-labelledby="function-stack-title">
          <h3
            id="function-stack-title"
            className="text-sm font-medium mb-3 flex items-center gap-2"
          >
            <span
              className="w-1 h-4 rounded-full"
              style={{ backgroundColor: themeColor }}
            />
            认知功能栈
          </h3>
          <div className="grid grid-cols-4 gap-2">
            {(Object.keys(FUNCTION_ROLES) as Array<keyof FunctionStack>).map(
              (role, index) => (
                <FunctionStackItem
                  key={role}
                  functionCode={traits.function_stack[role]}
                  role={FUNCTION_ROLES[role]}
                  isPrimary={index === 0}
                  themeColor={themeColor}
                />
              )
            )}
          </div>
        </section>
      )}

      {/* 示例对话风格 */}
      {exampleDialogue && (
        <section aria-labelledby="dialogue-style-title">
          <h3
            id="dialogue-style-title"
            className="text-sm font-medium mb-3 flex items-center gap-2"
          >
            <span
              className="w-1 h-4 rounded-full"
              style={{ backgroundColor: themeColor }}
            />
            示例对话风格
          </h3>
          <div
            className="rounded-lg p-4 space-y-2"
            style={{
              backgroundColor: `${themeColor}08`,
              borderLeft: `3px solid ${themeColor}`,
            }}
          >
            <p className="text-sm italic text-foreground/80">
              "{exampleDialogue.example}"
            </p>
            <p className="text-xs text-muted-foreground">
              沟通风格：{exampleDialogue.style}
            </p>
          </div>
        </section>
      )}

      {/* 优势和盲点 */}
      {config && (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {config.strengths.length > 0 && (
            <TraitList
              title="优势"
              items={config.strengths}
              icon="check"
              themeColor={themeColor}
            />
          )}
          {config.blind_spots.length > 0 && (
            <TraitList
              title="潜在盲点"
              items={config.blind_spots}
              icon="warning"
              themeColor={themeColor}
            />
          )}
        </div>
      )}

      {/* 推荐应用场景 */}
      {config?.recommended_use_cases && config.recommended_use_cases.length > 0 && (
        <section aria-labelledby="use-cases-title">
          <h3
            id="use-cases-title"
            className="text-sm font-medium mb-3 flex items-center gap-2"
          >
            <span
              className="w-1 h-4 rounded-full"
              style={{ backgroundColor: themeColor }}
            />
            建议应用场景
          </h3>
          <div className="flex flex-wrap gap-2">
            {config.recommended_use_cases.map((useCase, index) => (
              <span
                key={index}
                className="inline-flex items-center px-3 py-1 rounded-full text-xs font-medium transition-all hover:scale-105"
                style={{
                  backgroundColor: `${themeColor}15`,
                  color: themeColor,
                }}
              >
                {useCase}
              </span>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

export default PersonalityPreview;