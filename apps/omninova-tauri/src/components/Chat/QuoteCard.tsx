/**
 * Quote Card Component
 *
 * Displays a quoted message preview above the chat input.
 * Used when replying to a specific message with context.
 *
 * [Source: Story 4.8 - 消息引用功能]
 */

import { memo } from 'react';
import { cn } from '@/lib/utils';
import { type Message, type MessageRole } from '@/types/session';
import { type MBTIType, getPersonalityColor } from '@/lib/personality-colors';

/**
 * Props for QuoteCard component
 */
export interface QuoteCardProps {
  /** The quoted message to display */
  quote: Message;
  /** Callback when cancel button is clicked */
  onCancel: () => void;
  /** Agent name for attribution (when quoting AI messages) */
  agentName?: string;
  /** Agent personality type for theming */
  personalityType?: MBTIType;
  /** Additional CSS classes */
  className?: string;
}

/**
 * Maximum characters to show in content preview
 */
const MAX_PREVIEW_LENGTH = 100;

/**
 * Truncate text to specified length with ellipsis
 */
function truncateText(text: string, maxLength: number): string {
  if (text.length <= maxLength) return text;
  return text.slice(0, maxLength).trim() + '...';
}

/**
 * Get sender display name based on role
 */
function getSenderName(role: MessageRole, agentName?: string): string {
  return role === 'user' ? '用户' : agentName || 'AI';
}

/**
 * Get sender icon based on role
 */
function getSenderIcon(role: MessageRole): string {
  return role === 'user' ? '👤' : '🤖';
}

/**
 * QuoteCard component
 *
 * Renders a card showing a quoted message preview with:
 * - Sender icon and name
 * - Truncated content preview
 * - Cancel button to clear quote
 * - Theme-aware styling with personality color accent
 *
 * @example
 * ```tsx
 * function ChatInput() {
 *   const { quoteMessage, clearQuoteMessage } = useChatStore();
 *
 *   return (
 *     <div>
 *       {quoteMessage && (
 *         <QuoteCard
 *           quote={quoteMessage}
 *           onCancel={clearQuoteMessage}
 *           agentName="Nova"
 *           personalityType="INTJ"
 *         />
 *       )}
 *       <textarea ... />
 *     </div>
 *   );
 * }
 * ```
 */
export const QuoteCard = memo(function QuoteCard({
  quote,
  onCancel,
  agentName,
  personalityType,
  className,
}: QuoteCardProps) {
  const previewContent = truncateText(quote.content, MAX_PREVIEW_LENGTH);
  const senderName = getSenderName(quote.role, agentName);
  const senderIcon = getSenderIcon(quote.role);
  const accentColor = personalityType ? getPersonalityColor(personalityType) : undefined;

  return (
    <div
      className={cn(
        'flex items-start gap-2 px-3 py-2 rounded-lg border',
        'bg-muted/50 border-border',
        className
      )}
      aria-label="引用的消息"
      role="region"
    >
      {/* Left accent border */}
      <div
        className={cn(
          'w-1 self-stretch rounded-full',
          quote.role === 'user' ? 'bg-primary/60' : 'bg-primary'
        )}
        style={accentColor ? { backgroundColor: `${accentColor}99` } : undefined}
      />

      {/* Content area */}
      <div className="flex-1 min-w-0">
        {/* Header: icon, label, sender */}
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
          <span>📎</span>
          <span className="font-medium">引用回复</span>
          <span className="text-muted-foreground/60">·</span>
          <span>{senderIcon}</span>
          <span className="font-medium">{senderName}</span>
        </div>

        {/* Message preview */}
        <p className="text-sm text-foreground/90 truncate">
          {previewContent}
        </p>
      </div>

      {/* Cancel button */}
      <button
        type="button"
        onClick={onCancel}
        className={cn(
          'flex-shrink-0 p-1 rounded-md',
          'text-muted-foreground hover:text-foreground',
          'hover:bg-muted transition-colors',
          'focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-1'
        )}
        aria-label="取消引用"
      >
        <svg
          className="h-4 w-4"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          strokeWidth={2}
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M6 18L18 6M6 6l12 12"
          />
        </svg>
      </button>
    </div>
  );
});

export default QuoteCard;