/**
 * Typing Indicator Component
 *
 * Displays animated typing indicator with personality-based theming.
 * Supports multiple animation styles: dots (bounce), wave, pulse.
 *
 * [Source: Story 4.5 - 打字指示器与加载状态]
 */

import { memo } from 'react';
import { cn } from '@/lib/utils';
import { type MBTIType, type PersonalityTone, getPersonalityColors } from '@/lib/personality-colors';

/**
 * Animation style for typing indicator
 */
export type TypingAnimationStyle = 'dots' | 'wave' | 'pulse';

/**
 * Props for TypingIndicator component
 */
export interface TypingIndicatorProps {
  /** Agent MBTI personality type for theming */
  personalityType?: MBTIType;
  /** Animation style override (defaults to personality-based) */
  animationStyle?: TypingAnimationStyle;
  /** Size variant */
  size?: 'sm' | 'md' | 'lg';
  /** Show label text */
  showLabel?: boolean;
  /** Custom label text */
  label?: string;
  /** Additional CSS classes */
  className?: string;
  /** Whether reduced motion is preferred */
  prefersReducedMotion?: boolean;
}

/**
 * Get animation style based on personality tone
 */
function getAnimationStyleForTone(tone: PersonalityTone): TypingAnimationStyle {
  switch (tone) {
    case 'analytical':
      return 'dots'; // Uniform rhythm, steady bouncing
    case 'creative':
      return 'wave'; // Fluid wave animation
    case 'structured':
      return 'pulse'; // Synchronized pulse, orderly
    case 'energetic':
      return 'dots'; // Fast bouncing, vibrant
    default:
      return 'dots';
  }
}

/**
 * Size configurations
 */
const sizeConfig = {
  sm: {
    dot: 'w-1.5 h-1.5',
    gap: 'gap-0.5',
    label: 'text-xs',
  },
  md: {
    dot: 'w-2 h-2',
    gap: 'gap-1',
    label: 'text-sm',
  },
  lg: {
    dot: 'w-2.5 h-2.5',
    gap: 'gap-1.5',
    label: 'text-base',
  },
} as const;

/**
 * Dots animation component
 */
function DotsAnimation({
  color,
  size,
  reducedMotion,
}: {
  color: string;
  size: 'sm' | 'md' | 'lg';
  reducedMotion: boolean;
}) {
  const { dot, gap } = sizeConfig[size];

  if (reducedMotion) {
    return (
      <div className={cn('flex items-center', gap)}>
        <span className={cn(dot, 'rounded-full opacity-60')} style={{ backgroundColor: color }} />
        <span className={cn(dot, 'rounded-full opacity-80')} style={{ backgroundColor: color }} />
        <span className={cn(dot, 'rounded-full')} style={{ backgroundColor: color }} />
      </div>
    );
  }

  return (
    <div className={cn('flex items-center', gap)} aria-hidden="true">
      <span
        className={cn(dot, 'rounded-full animate-bounce')}
        style={{ backgroundColor: color, animationDelay: '0ms' }}
      />
      <span
        className={cn(dot, 'rounded-full animate-bounce')}
        style={{ backgroundColor: color, animationDelay: '150ms' }}
      />
      <span
        className={cn(dot, 'rounded-full animate-bounce')}
        style={{ backgroundColor: color, animationDelay: '300ms' }}
      />
    </div>
  );
}

/**
 * Wave animation component
 */
function WaveAnimation({
  color,
  size,
  reducedMotion,
}: {
  color: string;
  size: 'sm' | 'md' | 'lg';
  reducedMotion: boolean;
}) {
  const { dot, gap } = sizeConfig[size];

  if (reducedMotion) {
    return (
      <div className={cn('flex items-end', gap)}>
        <span className={cn(dot, 'rounded-full opacity-60')} style={{ backgroundColor: color }} />
        <span className={cn(dot, 'rounded-full opacity-80')} style={{ backgroundColor: color }} />
        <span className={cn(dot, 'rounded-full')} style={{ backgroundColor: color }} />
      </div>
    );
  }

  return (
    <div className={cn('flex items-end', gap)} aria-hidden="true">
      <span
        className={cn(dot, 'rounded-full animate-[wave_1.2s_ease-in-out_infinite]')}
        style={{ backgroundColor: color, animationDelay: '0ms' }}
      />
      <span
        className={cn(dot, 'rounded-full animate-[wave_1.2s_ease-in-out_infinite]')}
        style={{ backgroundColor: color, animationDelay: '150ms' }}
      />
      <span
        className={cn(dot, 'rounded-full animate-[wave_1.2s_ease-in-out_infinite]')}
        style={{ backgroundColor: color, animationDelay: '300ms' }}
      />
    </div>
  );
}

/**
 * Pulse animation component
 */
function PulseAnimation({
  color,
  size,
  reducedMotion,
}: {
  color: string;
  size: 'sm' | 'md' | 'lg';
  reducedMotion: boolean;
}) {
  const { dot, gap } = sizeConfig[size];

  if (reducedMotion) {
    return (
      <div className={cn('flex items-center', gap)}>
        <span className={cn(dot, 'rounded-full opacity-70')} style={{ backgroundColor: color }} />
        <span className={cn(dot, 'rounded-full opacity-70')} style={{ backgroundColor: color }} />
        <span className={cn(dot, 'rounded-full opacity-70')} style={{ backgroundColor: color }} />
      </div>
    );
  }

  return (
    <div className={cn('flex items-center', gap)} aria-hidden="true">
      <span
        className={cn(dot, 'rounded-full animate-pulse')}
        style={{ backgroundColor: color, animationDelay: '0ms' }}
      />
      <span
        className={cn(dot, 'rounded-full animate-pulse')}
        style={{ backgroundColor: color, animationDelay: '150ms' }}
      />
      <span
        className={cn(dot, 'rounded-full animate-pulse')}
        style={{ backgroundColor: color, animationDelay: '300ms' }}
      />
    </div>
  );
}

/**
 * TypingIndicator component
 *
 * Displays an animated typing indicator with personality-based styling.
 * Animation style is automatically selected based on personality tone,
 * or can be explicitly overridden.
 *
 * @example
 * ```tsx
 * // With personality theming (auto-selects animation style)
 * <TypingIndicator personalityType="INTJ" showLabel />
 *
 * // With explicit animation style
 * <TypingIndicator animationStyle="wave" label="Thinking..." />
 *
 * // Minimal version
 * <TypingIndicator />
 * ```
 */
export const TypingIndicator = memo(function TypingIndicator({
  personalityType,
  animationStyle,
  size = 'md',
  showLabel = false,
  label = '正在思考...',
  className,
  prefersReducedMotion = false,
}: TypingIndicatorProps) {
  // Get personality colors and determine animation style
  const personalityColors = personalityType ? getPersonalityColors(personalityType) : null;
  const color = personalityColors?.primary || 'currentColor';

  // Determine animation style
  const resolvedAnimationStyle: TypingAnimationStyle =
    animationStyle || (personalityColors ? getAnimationStyleForTone(personalityColors.tone) : 'dots');

  // Render appropriate animation
  const renderAnimation = () => {
    const props = { color, size, reducedMotion: prefersReducedMotion };

    switch (resolvedAnimationStyle) {
      case 'wave':
        return <WaveAnimation {...props} />;
      case 'pulse':
        return <PulseAnimation {...props} />;
      case 'dots':
      default:
        return <DotsAnimation {...props} />;
    }
  };

  return (
    <div
      className={cn('flex items-center gap-2', className)}
      role="status"
      aria-label="正在输入"
      aria-live="polite"
    >
      {renderAnimation()}
      {showLabel && (
        <span className={cn('text-muted-foreground', sizeConfig[size].label)}>
          {label}
        </span>
      )}
    </div>
  );
});

export default TypingIndicator;