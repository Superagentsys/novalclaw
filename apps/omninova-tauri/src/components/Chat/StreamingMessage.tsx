/**
 * Streaming Message Component
 *
 * Displays streaming AI responses with real-time updates,
 * markdown rendering, and cancellation support.
 *
 * [Source: Story 4.3 - 流式响应处理]
 */

import { useEffect, useRef, useState, memo } from 'react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

/**
 * Props for StreamingMessage component
 */
export interface StreamingMessageProps {
  /** Current streamed content */
  content: string;
  /** Reasoning content (for thinking models like DeepSeek R1) */
  reasoning?: string;
  /** Whether stream is active */
  isStreaming: boolean;
  /** Callback to cancel the stream */
  onCancel?: () => void;
  /** Additional CSS classes */
  className?: string;
  /** Show reasoning section */
  showReasoning?: boolean;
  /** Agent name for attribution */
  agentName?: string;
}

/**
 * Typing cursor component with blinking animation
 */
function TypingCursor() {
  return (
    <span
      className="inline-block w-2 h-4 ml-0.5 bg-current animate-pulse"
      aria-hidden="true"
    />
  );
}

/**
 * Typing indicator shown before first content arrives
 */
function TypingIndicator() {
  return (
    <div className="flex items-center gap-1 py-2" aria-label="正在输入">
      <span className="w-2 h-2 bg-muted-foreground rounded-full animate-bounce [animation-delay:0ms]" />
      <span className="w-2 h-2 bg-muted-foreground rounded-full animate-bounce [animation-delay:150ms]" />
      <span className="w-2 h-2 bg-muted-foreground rounded-full animate-bounce [animation-delay:300ms]" />
    </div>
  );
}

/**
 * Reasoning section component (collapsible)
 */
function ReasoningSection({
  content,
  isExpanded,
  onToggle,
}: {
  content: string;
  isExpanded: boolean;
  onToggle: () => void;
}) {
  if (!content) return null;

  return (
    <div className="mb-3 border border-border/50 rounded-lg overflow-hidden">
      <button
        type="button"
        onClick={onToggle}
        className="w-full px-3 py-2 flex items-center justify-between text-xs text-muted-foreground bg-muted/30 hover:bg-muted/50 transition-colors"
        aria-expanded={isExpanded}
      >
        <span className="flex items-center gap-1.5">
          <svg
            className="w-3.5 h-3.5"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z"
            />
          </svg>
          思考过程
        </span>
        <svg
          className={cn('w-4 h-4 transition-transform', isExpanded && 'rotate-180')}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>
      {isExpanded && (
        <div className="px-3 py-2 text-xs text-muted-foreground bg-muted/10 whitespace-pre-wrap">
          {content}
        </div>
      )}
    </div>
  );
}

/**
 * Simple markdown-like text renderer
 * Handles code blocks, inline code, bold, and italic
 */
function renderFormattedText(text: string): React.ReactNode {
  // Simple markdown parsing for common patterns
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
 * StreamingMessage component
 *
 * Displays AI response with real-time streaming support.
 * Shows a typing indicator while waiting, renders content incrementally,
 * and provides cancellation control.
 *
 * @example
 * ```tsx
 * function ChatMessage({ message, isStreaming, onCancel }) {
 *   return (
 *     <StreamingMessage
 *       content={message.content}
 *       isStreaming={isStreaming}
 *       onCancel={onCancel}
 *       showReasoning
 *     />
 *   );
 * }
 * ```
 */
export const StreamingMessage = memo(function StreamingMessage({
  content,
  reasoning,
  isStreaming,
  onCancel,
  className,
  showReasoning = true,
  agentName,
}: StreamingMessageProps) {
  const [showReasoningExpanded, setShowReasoningExpanded] = useState(false);
  const contentRef = useRef<HTMLDivElement>(null);
  const shouldAutoScroll = useRef(true);

  // Auto-scroll when new content arrives
  useEffect(() => {
    if (contentRef.current && shouldAutoScroll.current) {
      contentRef.current.scrollIntoView({ behavior: 'smooth', block: 'end' });
    }
  }, [content]);

  // Detect if user has scrolled up
  useEffect(() => {
    const handleScroll = () => {
      if (contentRef.current) {
        const { scrollTop, scrollHeight, clientHeight } = contentRef.current;
        // If we're near the bottom, auto-scroll
        shouldAutoScroll.current = scrollHeight - scrollTop - clientHeight < 100;
      }
    };

    const element = contentRef.current;
    element?.addEventListener('scroll', handleScroll);
    return () => element?.removeEventListener('scroll', handleScroll);
  }, []);

  const hasContent = content.length > 0;

  return (
    <div
      className={cn(
        'flex flex-col gap-2 p-4 rounded-lg bg-muted/30',
        className
      )}
      role="article"
      aria-live="polite"
      aria-busy={isStreaming}
    >
      {/* Agent attribution */}
      {agentName && (
        <div className="text-xs text-muted-foreground font-medium mb-1">
          {agentName}
        </div>
      )}

      {/* Reasoning section (for thinking models) */}
      {showReasoning && reasoning && (
        <ReasoningSection
          content={reasoning}
          isExpanded={showReasoningExpanded}
          onToggle={() => setShowReasoningExpanded(!showReasoningExpanded)}
        />
      )}

      {/* Main content area */}
      <div ref={contentRef} className="flex-1 overflow-auto">
        {!hasContent && isStreaming ? (
          <TypingIndicator />
        ) : (
          <div className="prose prose-sm dark:prose-invert max-w-none">
            {renderFormattedText(content)}
            {isStreaming && <TypingCursor />}
          </div>
        )}
      </div>

      {/* Cancel button */}
      {isStreaming && onCancel && (
        <div className="flex justify-end pt-2">
          <Button
            variant="outline"
            size="sm"
            onClick={onCancel}
            className="text-xs"
          >
            <svg
              className="w-3.5 h-3.5 mr-1.5"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M6 18L18 6M6 6l12 12"
              />
            </svg>
            停止生成
          </Button>
        </div>
      )}
    </div>
  );
});

export default StreamingMessage;