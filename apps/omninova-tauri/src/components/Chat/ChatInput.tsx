/**
 * Chat Input Component
 *
 * Message input with auto-expanding textarea, send/cancel buttons,
 * personality-based theming, and message quote support.
 *
 * [Source: Story 4.6 - 消息输入与发送功能]
 * [Source: Story 4.8 - 消息引用功能]
 */

import { memo, useRef, useCallback, useEffect, useState } from 'react';
import { cn } from '@/lib/utils';
import { type MBTIType, getPersonalityColors } from '@/lib/personality-colors';
import { type Message } from '@/types/session';
import { LoadingButton } from '@/components/ui/loading-button';
import { Button } from '@/components/ui/button';
import QuoteCard from './QuoteCard';

// ============================================================================
// Types
// ============================================================================

/**
 * Props for ChatInput component
 */
export interface ChatInputProps {
  /** Callback when user sends a message */
  onSend: (content: string) => void;
  /** Callback to cancel active stream */
  onCancel?: () => void;
  /** Whether AI is currently streaming a response */
  isStreaming?: boolean;
  /** Whether input is disabled */
  disabled?: boolean;
  /** Placeholder text for the input */
  placeholder?: string;
  /** Agent MBTI personality type for button theming */
  personalityType?: MBTIType;
  /** Agent name for quote attribution */
  agentName?: string;
  /** Additional CSS classes */
  className?: string;
  /** Initial value for the input */
  defaultValue?: string;
  /** Controlled value for the input */
  value?: string;
  /** Callback when input value changes */
  onChange?: (value: string) => void;
  /** Maximum number of visible rows before scrolling */
  maxRows?: number;
  /** Minimum number of visible rows */
  minRows?: number;
  /** Currently quoted message for reply */
  quoteMessage?: Message | null;
  /** Callback to clear quoted message */
  onCancelQuote?: () => void;
}

// ============================================================================
// Constants
// ============================================================================

const DEFAULT_MIN_ROWS = 1;
const DEFAULT_MAX_ROWS = 6;
const LINE_HEIGHT = 24; // Approximate line height in pixels

// ============================================================================
// Main Component
// ============================================================================

/**
 * ChatInput component
 *
 * A message input with auto-expanding textarea, keyboard shortcuts,
 * and integrated send/cancel buttons.
 *
 * Features:
 * - Auto-expanding textarea (1-6 rows by default)
 * - Enter to send, Shift+Enter for newline
 * - Send button disabled when input is empty or streaming
 * - Cancel button during streaming
 * - Auto-focus after send
 * - Personality color theming for send button
 *
 * @example
 * ```tsx
 * function ChatPage() {
 *   const [isStreaming, setIsStreaming] = useState(false);
 *
 *   const handleSend = (content: string) => {
 *     console.log('Send:', content);
 *   };
 *
 *   const handleCancel = () => {
 *     console.log('Cancel stream');
 *   };
 *
 *   return (
 *     <ChatInput
 *       onSend={handleSend}
 *       onCancel={handleCancel}
 *       isStreaming={isStreaming}
 *       personalityType="INTJ"
 *       placeholder="输入消息..."
 *     />
 *   );
 * }
 * ```
 */
export const ChatInput = memo(function ChatInput({
  onSend,
  onCancel,
  isStreaming = false,
  disabled = false,
  placeholder = '输入消息...',
  personalityType,
  agentName,
  className,
  defaultValue = '',
  value: controlledValue,
  onChange,
  maxRows = DEFAULT_MAX_ROWS,
  minRows = DEFAULT_MIN_ROWS,
  quoteMessage,
  onCancelQuote,
}: ChatInputProps) {
  // Internal state for uncontrolled mode
  const [internalValue, setInternalValue] = useState(defaultValue);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Determine if controlled or uncontrolled
  const isControlled = controlledValue !== undefined;
  const value = isControlled ? controlledValue : internalValue;

  // Get personality colors for theming
  const colors = personalityType ? getPersonalityColors(personalityType) : null;

  /**
   * Auto-adjust textarea height based on content
   */
  const adjustHeight = useCallback(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;

    // Reset height to get accurate scrollHeight
    textarea.style.height = 'auto';

    // Calculate height constraints
    const minHeight = minRows * LINE_HEIGHT;
    const maxHeight = maxRows * LINE_HEIGHT;

    // Set height based on content, clamped to min/max
    const newHeight = Math.min(Math.max(textarea.scrollHeight, minHeight), maxHeight);
    textarea.style.height = `${newHeight}px`;
  }, [minRows, maxRows]);

  /**
   * Adjust height when value changes
   */
  useEffect(() => {
    adjustHeight();
  }, [value, adjustHeight]);

  /**
   * Focus textarea on mount
   */
  useEffect(() => {
    textareaRef.current?.focus();
  }, []);

  /**
   * Handle input change
   */
  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLTextAreaElement>) => {
      const newValue = e.target.value;

      if (!isControlled) {
        setInternalValue(newValue);
      }

      onChange?.(newValue);
    },
    [isControlled, onChange]
  );

  /**
   * Handle keyboard events
   * - Enter (without Shift): Send message
   * - Shift+Enter: Newline (default behavior)
   */
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      // Enter without Shift: send message
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();

        const trimmedValue = value.trim();
        if (trimmedValue && !isStreaming && !disabled) {
          onSend(trimmedValue);

          // Clear input after send (uncontrolled mode only)
          if (!isControlled) {
            setInternalValue('');
          }
          onChange?.('');

          // Clear quote after send
          onCancelQuote?.();

          // Refocus after a brief delay to ensure state updates
          setTimeout(() => {
            textareaRef.current?.focus();
          }, 0);
        }
      }
    },
    [value, isStreaming, disabled, onSend, isControlled, onChange, onCancelQuote]
  );

  /**
   * Handle send button click
   */
  const handleSendClick = useCallback(() => {
    const trimmedValue = value.trim();
    if (trimmedValue && !isStreaming && !disabled) {
      onSend(trimmedValue);

      // Clear input after send (uncontrolled mode only)
      if (!isControlled) {
        setInternalValue('');
      }
      onChange?.('');

      // Clear quote after send
      onCancelQuote?.();

      // Refocus after send
      setTimeout(() => {
        textareaRef.current?.focus();
      }, 0);
    }
  }, [value, isStreaming, disabled, onSend, isControlled, onChange, onCancelQuote]);

  /**
   * Handle cancel button click
   */
  const handleCancelClick = useCallback(() => {
    onCancel?.();
  }, [onCancel]);

  /**
   * Handle paste event
   */
  const handlePaste = useCallback(
    (_e: React.ClipboardEvent<HTMLTextAreaElement>) => {
      // Allow default paste behavior
      // The browser handles Ctrl+V automatically
    },
    []
  );

  // Determine if send should be disabled
  const isSendDisabled = !value.trim() || isStreaming || disabled;
  const isInputDisabled = isStreaming || disabled;

  return (
    <div className={cn('border-t border-border bg-background', className)}>
      {/* Quote card (shown when quoting a message) */}
      {quoteMessage && onCancelQuote && (
        <div className="px-3 pt-3">
          <QuoteCard
            quote={quoteMessage}
            onCancel={onCancelQuote}
            agentName={agentName}
            personalityType={personalityType}
          />
        </div>
      )}

      {/* Input area */}
      <div className="flex items-end gap-2 p-3">
        {/* Textarea */}
        <div className="relative flex-1">
          <textarea
            ref={textareaRef}
            value={value}
            onChange={handleChange}
            onKeyDown={handleKeyDown}
            onPaste={handlePaste}
            placeholder={placeholder}
            disabled={isInputDisabled}
            rows={minRows}
            className={cn(
              'w-full resize-none rounded-lg border border-input bg-background px-3 py-2',
              'text-sm text-foreground placeholder:text-muted-foreground',
              'focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-0',
              'disabled:cursor-not-allowed disabled:opacity-50',
              'transition-colors duration-200'
            )}
            style={{
              lineHeight: `${LINE_HEIGHT}px`,
              maxHeight: `${maxRows * LINE_HEIGHT}px`,
            }}
            aria-label="消息输入框"
          />
        </div>

        {/* Action buttons */}
        <div className="flex items-center gap-2">
          {/* Cancel button (shown during streaming) */}
          {isStreaming ? (
            <Button
              type="button"
              variant="outline"
              size="default"
              onClick={handleCancelClick}
              disabled={!onCancel}
              className="min-w-[72px]"
              aria-label="停止生成"
            >
              <svg
                className="w-4 h-4 mr-1.5"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                aria-hidden="true"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M6 18L18 6M6 6l12 12"
                />
              </svg>
              停止
            </Button>
          ) : (
            /* Send button */
            <LoadingButton
              type="button"
              size="default"
              onClick={handleSendClick}
              disabled={isSendDisabled}
              className="min-w-[72px]"
              style={
                colors && !isSendDisabled
                  ? { backgroundColor: colors.primary, borderColor: colors.primary }
                  : undefined
              }
              aria-label="发送消息"
            >
              <svg
                className="w-4 h-4 mr-1.5"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                aria-hidden="true"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8"
                />
              </svg>
              发送
            </LoadingButton>
          )}
        </div>
      </div>
    </div>
  );
});

export default ChatInput;