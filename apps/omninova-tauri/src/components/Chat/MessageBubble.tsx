/**
 * Message Bubble Component
 *
 * Displays individual chat messages with role-based styling,
 * personality color theming, timestamp formatting, and quote preview.
 *
 * [Source: Story 4.4 - ChatInterface 组件基础]
 * [Source: Story 4.8 - 消息引用功能]
 */

import { memo } from 'react';
import { cn } from '@/lib/utils';
import { type MessageRole } from '@/types/session';
import { type MBTIType, getPersonalityColors } from '@/lib/personality-colors';

/**
 * Props for MessageBubble component
 */
export interface MessageBubbleProps {
  /** Message content */
  content: string;
  /** Message role (user, assistant, system) */
  role: MessageRole;
  /** Message timestamp (Unix timestamp in seconds) */
  timestamp: number;
  /** Agent MBTI personality type for theming (assistant messages only) */
  personalityType?: MBTIType;
  /** Agent name for attribution */
  agentName?: string;
  /** Additional CSS classes */
  className?: string;
  /** Whether to show timestamp */
  showTimestamp?: boolean;
  /**
   * ID of quoted message (if this message is a reply)
   * When set, displays a preview of the quoted message
   */
  quoteMessageId?: number;
  /**
   * Callback when quote preview is clicked
   * Passes the quoted message ID for scroll/navigation
   */
  onQuoteClick?: (messageId: number) => void;
  /**
   * Content preview of quoted message
   * Required when quoteMessageId is set
   */
  quoteContent?: string;
  /**
   * Role of the quoted message sender
   * Required when quoteMessageId is set
   */
  quoteRole?: MessageRole;
}

/**
 * Format timestamp to localized time string
 */
function formatTimestamp(timestamp: number): string {
  const date = new Date(timestamp * 1000); // Convert Unix seconds to milliseconds
  return new Intl.DateTimeFormat('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  }).format(date);
}

/**
 * Simple markdown-like text renderer
 * Handles code blocks, inline code, bold, and italic
 */
function renderFormattedText(text: string): React.ReactNode {
  const parts: React.ReactNode[] = [];
  let remaining = text;
  let key = 0;

  while (remaining.length > 0) {
    // Code block
    const codeBlockMatch = remaining.match(/^```(\w*)\n?([\s\S]*?)```/);
    if (codeBlockMatch) {
      const [, lang, code] = codeBlockMatch;
      parts.push(
        <pre
          key={key++}
          className="my-2 p-3 bg-muted rounded-md overflow-x-auto text-sm"
        >
          {lang && (
            <div className="text-xs text-muted-foreground mb-1">{lang}</div>
          )}
          <code>{code.trim()}</code>
        </pre>
      );
      remaining = remaining.slice(codeBlockMatch[0].length);
      continue;
    }

    // Inline code
    const inlineCodeMatch = remaining.match(/^`([^`]+)`/);
    if (inlineCodeMatch) {
      parts.push(
        <code
          key={key++}
          className="px-1.5 py-0.5 bg-muted rounded text-sm font-mono"
        >
          {inlineCodeMatch[1]}
        </code>
      );
      remaining = remaining.slice(inlineCodeMatch[0].length);
      continue;
    }

    // Bold
    const boldMatch = remaining.match(/^\*\*([^*]+)\*\*/);
    if (boldMatch) {
      parts.push(
        <strong key={key++} className="font-semibold">
          {boldMatch[1]}
        </strong>
      );
      remaining = remaining.slice(boldMatch[0].length);
      continue;
    }

    // Italic
    const italicMatch = remaining.match(/^\*([^*]+)\*/);
    if (italicMatch) {
      parts.push(
        <em key={key++} className="italic">
          {italicMatch[1]}
        </em>
      );
      remaining = remaining.slice(italicMatch[0].length);
      continue;
    }

    // Regular text - take one character
    parts.push(remaining[0]);
    remaining = remaining.slice(1);
  }

  return parts;
}

/**
 * Maximum characters for quote preview
 */
const QUOTE_PREVIEW_LENGTH = 80;

/**
 * Truncate text for quote preview
 */
function truncateForQuote(text: string): string {
  if (text.length <= QUOTE_PREVIEW_LENGTH) return text;
  return text.slice(0, QUOTE_PREVIEW_LENGTH).trim() + '...';
}

/**
 * Get sender label for quote based on role
 */
function getQuoteSenderLabel(role: MessageRole): string {
  return role === 'user' ? '用户' : 'AI';
}

/**
 * MessageBubble component
 *
 * Displays a single chat message with appropriate styling based on role.
 * User messages are right-aligned with primary color.
 * Assistant messages are left-aligned with personality-themed colors.
 *
 * @example
 * ```tsx
 * <MessageBubble
 *   content="Hello, how can I help you?"
 *   role="assistant"
 *   timestamp={Date.now() / 1000}
 *   personalityType="INTJ"
 *   agentName="Assistant"
 * />
 * ```
 */
export const MessageBubble = memo(function MessageBubble({
  content,
  role,
  timestamp,
  personalityType,
  agentName,
  className,
  showTimestamp = true,
  quoteMessageId,
  onQuoteClick,
  quoteContent,
  quoteRole,
}: MessageBubbleProps) {
  const isUser = role === 'user';
  const isAssistant = role === 'assistant';
  const isSystem = role === 'system';

  // Determine if we should show quote preview
  const showQuote = quoteMessageId !== undefined && quoteContent !== undefined;

  // Get personality colors for assistant messages
  const personalityColors = isAssistant && personalityType
    ? getPersonalityColors(personalityType)
    : null;

  // Build style for assistant messages with personality colors
  // User messages use Tailwind classes (bg-primary text-primary-foreground) applied via className
  const bubbleStyle = isAssistant && personalityColors
    ? { borderLeftColor: personalityColors.primary }
    : undefined;

  // Handle quote click
  const handleQuoteClick = () => {
    if (quoteMessageId !== undefined && onQuoteClick) {
      onQuoteClick(quoteMessageId);
    }
  };

  return (
    <div
      className={cn(
        'flex flex-col gap-1 max-w-[80%]',
        isUser ? 'ml-auto items-end' : 'mr-auto items-start',
        className
      )}
    >
      {/* Agent attribution (assistant messages only) */}
      {isAssistant && agentName && (
        <span className="text-xs text-muted-foreground font-medium px-1">
          {agentName}
        </span>
      )}

      {/* Quote preview (if this message quotes another) */}
      {showQuote && quoteContent && quoteRole && (
        <button
          type="button"
          onClick={handleQuoteClick}
          className={cn(
            'w-full text-left px-3 py-1.5 rounded-md mb-1',
            'border-l-2 text-xs text-muted-foreground',
            'hover:bg-muted/50 transition-colors cursor-pointer',
            'focus:outline-none focus:ring-1 focus:ring-ring',
            quoteRole === 'user' ? 'border-l-primary/60' : 'border-l-primary'
          )}
          aria-label="跳转到被引用的消息"
        >
          <span className="font-medium">
            {getQuoteSenderLabel(quoteRole)}:
          </span>
          <span className="ml-1">{truncateForQuote(quoteContent)}</span>
        </button>
      )}

      {/* Message bubble */}
      <div
        className={cn(
          'rounded-lg px-4 py-2.5',
          isUser && 'bg-primary text-primary-foreground',
          isAssistant && !personalityColors && 'bg-muted border-l-2 border-l-primary/50',
          isAssistant && personalityColors && 'bg-muted/50 border-l-2',
          isSystem && 'bg-muted/30 border border-border text-muted-foreground text-center w-full max-w-full text-sm'
        )}
        style={bubbleStyle}
      >
        <div className="prose prose-sm dark:prose-invert max-w-none break-words">
          {renderFormattedText(content)}
        </div>
      </div>

      {/* Timestamp */}
      {showTimestamp && !isSystem && (
        <span className="text-xs text-muted-foreground/60 px-1">
          {formatTimestamp(timestamp)}
        </span>
      )}
    </div>
  );
});

export default MessageBubble;