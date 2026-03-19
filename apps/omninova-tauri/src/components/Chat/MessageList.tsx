/**
 * Message List Component
 *
 * Displays a scrollable list of messages with auto-scroll behavior,
 * empty state handling, and accessibility support.
 *
 * [Source: Story 4.4 - ChatInterface 组件基础]
 * [Source: Story 4.7 - 对话历史持久化与导航]
 */

import { useRef, useEffect, memo, type ReactNode } from 'react';
import { cn } from '@/lib/utils';
import { type Message } from '@/types/session';
import { type MBTIType } from '@/lib/personality-colors';
import MessageBubble from './MessageBubble';
import StreamingMessage from './StreamingMessage';
import TypingIndicator from './TypingIndicator';

/**
 * Props for MessageList component
 */
export interface MessageListProps {
  /** Array of messages to display */
  messages: Message[];
  /** Agent MBTI personality type for theming */
  personalityType?: MBTIType;
  /** Agent name for attribution */
  agentName?: string;
  /** Whether a stream is active */
  isStreaming?: boolean;
  /** Current streamed content (during active stream) */
  streamedContent?: string;
  /** Reasoning content from streaming */
  reasoningContent?: string;
  /** Callback to cancel active stream */
  onCancelStream?: () => void;
  /** Additional CSS classes */
  className?: string;
  /** Custom empty state content */
  emptyState?: ReactNode;
  /** Whether to show timestamps */
  showTimestamps?: boolean;
  /** Whether there are more messages to load */
  hasMore?: boolean;
  /** Callback to load more messages */
  onLoadMore?: () => void;
  /** Whether more messages are loading */
  isLoadingMore?: boolean;
}

/**
 * Empty state component for message list
 */
function EmptyState({ agentName }: { agentName?: string }) {
  return (
    <div className="flex flex-col items-center justify-center h-full text-muted-foreground gap-2 p-8">
      <div className="text-4xl mb-2">💬</div>
      <p className="text-center">
        输入消息开始与 {agentName || 'AI 助手'} 对话
      </p>
      <p className="text-sm text-muted-foreground/60 text-center">
        按 Enter 发送，Shift+Enter 换行
      </p>
    </div>
  );
}

/**
 * MessageList component
 *
 * Renders a list of chat messages with:
 * - Auto-scroll to bottom on new messages
 * - Smart scroll detection (doesn't auto-scroll if user scrolled up)
 * - Empty state display when no messages
 * - Streaming message support
 * - Accessibility with aria-live regions
 * - "Load more" trigger for pagination
 *
 * @example
 * ```tsx
 * function ChatContainer() {
 *   const { messages, isStreaming, streamedContent } = useChatStore();
 *
 *   return (
 *     <MessageList
 *       messages={messages}
 *       personalityType="INTJ"
 *       agentName="Nova"
 *       isStreaming={isStreaming}
 *       streamedContent={streamedContent}
 *     />
 *   );
 * }
 * ```
 */
export const MessageList = memo(function MessageList({
  messages,
  personalityType,
  agentName,
  isStreaming = false,
  streamedContent = '',
  reasoningContent,
  onCancelStream,
  className,
  emptyState,
  showTimestamps = true,
  hasMore = false,
  onLoadMore,
  isLoadingMore = false,
}: MessageListProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const shouldAutoScroll = useRef(true);
  const prevScrollHeightRef = useRef(0);

  // Detect if user has scrolled up or to top
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const handleScroll = () => {
      const { scrollTop, scrollHeight, clientHeight } = container;
      // If we're within 100px of the bottom, enable auto-scroll
      shouldAutoScroll.current = scrollHeight - scrollTop - clientHeight < 100;

      // Trigger load more when scrolled to top
      if (scrollTop < 50 && hasMore && !isLoadingMore && onLoadMore) {
        // Store current scroll height before loading
        prevScrollHeightRef.current = scrollHeight;
        onLoadMore();
      }
    };

    container.addEventListener('scroll', handleScroll);
    return () => container.removeEventListener('scroll', handleScroll);
  }, [hasMore, isLoadingMore, onLoadMore]);

  // Auto-scroll when new content arrives
  useEffect(() => {
    if (shouldAutoScroll.current && messagesEndRef.current) {
      messagesEndRef.current.scrollIntoView({ behavior: 'smooth', block: 'end' });
    }
  }, [messages, streamedContent, isStreaming]);

  // Preserve scroll position when loading more messages (prepend)
  useEffect(() => {
    const container = containerRef.current;
    if (!container || !isLoadingMore) return;

    // After messages are prepended, adjust scroll to maintain position
    const newScrollHeight = container.scrollHeight;
    const scrollDiff = newScrollHeight - prevScrollHeightRef.current;
    if (scrollDiff > 0) {
      container.scrollTop = container.scrollTop + scrollDiff;
    }
  }, [messages, isLoadingMore]);

  const hasMessages = messages.length > 0;
  const showStreaming = isStreaming && streamedContent;

  return (
    <div
      ref={containerRef}
      className={cn(
        'flex-1 overflow-y-auto px-4 py-4',
        className
      )}
      role="log"
      aria-live="polite"
      aria-label="聊天消息列表"
    >
      {!hasMessages && !isStreaming ? (
        emptyState || <EmptyState agentName={agentName} />
      ) : (
        <div className="flex flex-col gap-3">
          {/* Load more trigger at top */}
          {hasMore && hasMessages && (
            <div className="flex justify-center py-2">
              <button
                type="button"
                onClick={onLoadMore}
                disabled={isLoadingMore}
                className={cn(
                  'flex items-center gap-2 px-4 py-2 text-sm',
                  'text-muted-foreground hover:text-foreground',
                  'rounded-lg hover:bg-muted transition-colors',
                  'disabled:opacity-50 disabled:cursor-not-allowed'
                )}
                aria-label="加载更多历史消息"
              >
                {isLoadingMore ? (
                  <>
                    <svg
                      className="animate-spin h-4 w-4"
                      fill="none"
                      viewBox="0 0 24 24"
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
                    加载中...
                  </>
                ) : (
                  <>
                    <svg
                      className="h-4 w-4"
                      fill="none"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M5 15l7-7 7 7"
                      />
                    </svg>
                    加载更多消息
                  </>
                )}
              </button>
            </div>
          )}

          {/* Render existing messages */}
          {messages.map((message) => (
            <MessageBubble
              key={message.id}
              content={message.content}
              role={message.role}
              timestamp={message.createdAt}
              personalityType={message.role === 'assistant' ? personalityType : undefined}
              agentName={message.role === 'assistant' ? agentName : undefined}
              showTimestamp={showTimestamps}
            />
          ))}

          {/* Render streaming message */}
          {showStreaming && (
            <StreamingMessage
              content={streamedContent}
              reasoning={reasoningContent}
              isStreaming={isStreaming}
              onCancel={onCancelStream}
              showReasoning={true}
              agentName={agentName}
              personalityType={personalityType}
            />
          )}

          {/* Typing indicator for waiting state */}
          {isStreaming && !streamedContent && (
            <TypingIndicator
              personalityType={personalityType}
              showLabel
              label="正在思考..."
            />
          )}

          {/* Scroll anchor */}
          <div ref={messagesEndRef} aria-hidden="true" />
        </div>
      )}
    </div>
  );
});

export default MessageList;