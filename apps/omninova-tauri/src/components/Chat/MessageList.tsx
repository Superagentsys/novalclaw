/**
 * Message List Component
 *
 * Displays a scrollable list of messages with auto-scroll behavior,
 * empty state handling, and accessibility support.
 *
 * [Source: Story 4.4 - ChatInterface 组件基础]
 */

import { useRef, useEffect, memo, type ReactNode } from 'react';
import { cn } from '@/lib/utils';
import { type Message } from '@/types/session';
import { type MBTIType } from '@/lib/personality-colors';
import MessageBubble from './MessageBubble';
import StreamingMessage from './StreamingMessage';

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
}: MessageListProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const shouldAutoScroll = useRef(true);

  // Detect if user has scrolled up
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const handleScroll = () => {
      const { scrollTop, scrollHeight, clientHeight } = container;
      // If we're within 100px of the bottom, enable auto-scroll
      shouldAutoScroll.current = scrollHeight - scrollTop - clientHeight < 100;
    };

    container.addEventListener('scroll', handleScroll);
    return () => container.removeEventListener('scroll', handleScroll);
  }, []);

  // Auto-scroll when new content arrives
  useEffect(() => {
    if (shouldAutoScroll.current && messagesEndRef.current) {
      messagesEndRef.current.scrollIntoView({ behavior: 'smooth', block: 'end' });
    }
  }, [messages, streamedContent, isStreaming]);

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
            />
          )}

          {/* Typing indicator for waiting state */}
          {isStreaming && !streamedContent && (
            <div className="flex items-center gap-2 text-muted-foreground">
              <div className="flex gap-1">
                <span className="w-2 h-2 bg-current rounded-full animate-bounce [animation-delay:0ms]" />
                <span className="w-2 h-2 bg-current rounded-full animate-bounce [animation-delay:150ms]" />
                <span className="w-2 h-2 bg-current rounded-full animate-bounce [animation-delay:300ms]" />
              </div>
              <span className="text-sm">正在思考...</span>
            </div>
          )}

          {/* Scroll anchor */}
          <div ref={messagesEndRef} aria-hidden="true" />
        </div>
      )}
    </div>
  );
});

export default MessageList;