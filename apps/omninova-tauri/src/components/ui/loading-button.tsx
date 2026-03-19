/**
 * Loading Button Component
 *
 * Extends shadcn Button with loading state, spinner animation,
 * and disabled interaction during loading.
 *
 * [Source: Story 4.5 - 打字指示器与加载状态]
 */

import { memo, forwardRef } from 'react';
import { Button, buttonVariants, type VariantProps } from '@/components/ui/button';
import { cn } from '@/lib/utils';

/**
 * Props for LoadingButton component
 */
export interface LoadingButtonProps extends VariantProps<typeof buttonVariants> {
  /** Whether the button is in loading state */
  loading?: boolean;
  /** Loading text override (defaults to children) */
  loadingText?: string;
  /** Button children (content) */
  children?: React.ReactNode;
  /** Additional CSS classes */
  className?: string;
  /** Button disabled state */
  disabled?: boolean;
  /** Click handler */
  onClick?: (event: React.MouseEvent<HTMLButtonElement>) => void;
  /** Button type */
  type?: 'button' | 'submit' | 'reset';
  /** HTML button props */
  [key: string]: unknown;
}

/**
 * Spinner component for loading state
 */
function Spinner({ className }: { className?: string }) {
  return (
    <svg
      className={cn('animate-spin', className)}
      xmlns="http://www.w3.org/2000/svg"
      fill="none"
      viewBox="0 0 24 24"
      aria-hidden="true"
    >
      <circle
        className="opacity-25"
        cx="12"
        cy="12"
        r="10"
        stroke="currentColor"
        strokeWidth="4"
      />
      <path
        className="opacity-75"
        fill="currentColor"
        d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
      />
    </svg>
  );
}

/**
 * LoadingButton component
 *
 * A button that displays a loading spinner and disables interaction
 * when in loading state.
 *
 * @example
 * ```tsx
 * function SubmitButton() {
 *   const [loading, setLoading] = useState(false);
 *
 *   return (
 *     <LoadingButton
 *       loading={loading}
 *       onClick={() => setLoading(true)}
 *     >
 *       提交
 *     </LoadingButton>
 *   );
 * }
 *
 * // With custom loading text
 * <LoadingButton loading loadingText="发送中...">
 *   发送
 * </LoadingButton>
 * ```
 */
export const LoadingButton = memo(
  forwardRef<HTMLButtonElement, LoadingButtonProps>(function LoadingButton(
    {
      loading = false,
      loadingText,
      children,
      className,
      disabled,
      variant = 'default',
      size = 'default',
      ...props
    },
    ref
  ) {
    const isDisabled = disabled || loading;
    const displayText = loading && loadingText ? loadingText : children;

    return (
      <Button
        ref={ref}
        variant={variant}
        size={size}
        disabled={isDisabled}
        className={cn(loading && 'cursor-not-allowed', className)}
        aria-busy={loading}
        {...props}
      >
        {loading && (
          <Spinner
            className={cn(
              'mr-2',
              size === 'xs' || size === 'icon-xs' ? 'h-3 w-3' : 'h-4 w-4'
            )}
          />
        )}
        {displayText}
      </Button>
    );
  })
);

export default LoadingButton;