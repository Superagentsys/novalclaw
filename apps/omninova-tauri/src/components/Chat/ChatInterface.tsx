/**
 * Chat Interface Container Component
 *
 * Main chat interface that integrates message list, streaming display,
 * message input, and provides a complete chat experience with agent
 * personality theming and message quote support.
 *
 * [Source: Story 4.4 - ChatInterface 组件基础]
 * [Source: Story 4.5 - 打字指示器与加载状态]
 * [Source: Story 4.6 - 消息输入与发送功能]
 * [Source: Story 4.7 - 对话历史持久化与导航]
 * [Source: Story 4.8 - 消息引用功能]
 */

import { memo, useCallback, useEffect, useRef } from 'react';
import { cn } from '@/lib/utils';
import { type AgentModel, type MBTIType } from '@/types/agent';
import { type Message, type Session } from '@/types/session';
import { useChatStore } from '@/stores/chatStore';
import { usePaginatedMessages } from '@/hooks/usePaginatedMessages';
import MessageList from './MessageList';
import { MessageSkeletonList } from './MessageSkeleton';
import ChatInput from './ChatInput';

// ============================================================================
// Types
// ============================================================================

/**
 * Props for ChatInterface component
 */
export interface ChatInterfaceProps {
  /** Active agent configuration */
  agent: AgentModel;
  /** Active session (optional, can be null for new chats) */
  session?: Session | null;
  /** Initial messages to display */
  initialMessages?: Message[];
  /** Callback when user sends a message */
  onSendMessage: (content: string) => void;
  /** Callback to cancel active stream */
  onCancelStream?: () => void;
  /** Additional CSS classes */
  className?: string;
  /** Whether to show timestamps */
  showTimestamps?: boolean;
  /** Custom header content */
  headerContent?: React.ReactNode;
  /** Custom empty state */
  emptyState?: React.ReactNode;
}

// ============================================================================
// Helper Components
// ============================================================================

/**
 * Chat header with agent info and status
 */
interface ChatHeaderProps {
  agent: AgentModel;
  session?: Session | null;
  isStreaming?: boolean;
  children?: React.ReactNode;
}

const ChatHeader = memo(function ChatHeader({
  agent,
  session,
  isStreaming,
  children,
}: ChatHeaderProps) {
  // Display session title if available, otherwise agent domain
  const subtitle = session?.title || agent.domain || 'AI 助手';

  return (
    <div className="flex items-center justify-between px-4 py-3 border-b border-border bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
      <div className="flex items-center gap-3">
        {/* Agent avatar/indicator */}
        <div className="relative">
          <div className="w-10 h-10 rounded-full bg-muted flex items-center justify-center text-lg font-semibold">
            {agent.name.charAt(0).toUpperCase()}
          </div>
          {/* Online/active indicator */}
          <div
            className={cn(
              'absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full border-2 border-background',
              isStreaming ? 'bg-yellow-500 animate-pulse' : 'bg-green-500'
            )}
            aria-label={isStreaming ? '正在回复' : '在线'}
          />
        </div>

        {/* Agent info */}
        <div className="flex flex-col">
          <span className="font-medium text-foreground">{agent.name}</span>
          <span className="text-xs text-muted-foreground">
            {isStreaming ? '正在输入...' : subtitle}
          </span>
        </div>
      </div>

      {/* Additional header content (actions, etc.) */}
      {children}
    </div>
  );
});

/**
 * Error display component
 */
interface ErrorDisplayProps {
  error: string;
  onRetry?: () => void;
}

const ErrorDisplay = memo(function ErrorDisplay({
  error,
  onRetry,
}: ErrorDisplayProps) {
  return (
    <div className="px-4 py-3 bg-destructive/10 border-b border-destructive/20">
      <div className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-2 text-destructive">
          <svg
            className="w-4 h-4 flex-shrink-0"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
            />
          </svg>
          <span className="text-sm">{error}</span>
        </div>
        {onRetry && (
          <button
            type="button"
            onClick={onRetry}
            className="text-sm text-destructive hover:text-destructive/80 underline underline-offset-2"
          >
            重试
          </button>
        )}
      </div>
    </div>
  );
});

/**
 * Loading overlay for initial load with skeleton messages
 */
const LoadingOverlay = memo(function LoadingOverlay() {
  return (
    <div className="absolute inset-0 flex flex-col bg-background/80 backdrop-blur-sm z-10 p-4">
      <MessageSkeletonList count={4} />
    </div>
  );
});

// ============================================================================
// Main Component
// ============================================================================

/**
 * ChatInterface component
 *
 * Main container for the chat experience. Integrates:
 * - Message list with auto-scroll
 * - Streaming message display
 * - Agent personality theming
 * - Loading and error states
 * - Responsive layout
 * - Session-aware message loading with pagination
 *
 * @example
 * ```tsx
 * function ChatPage() {
 *   const agent = useAgentStore((state) => state.activeAgent);
 *   const session = useSessionStore((state) => state.activeSession);
 *
 *   const handleSendMessage = (content: string) => {
 *     // Send message logic
 *   };
 *
 *   return (
 *     <ChatInterface
 *       agent={agent}
 *       session={session}
 *       onSendMessage={handleSendMessage}
 *     />
 *   );
 * }
 * ```
 */
export const ChatInterface = memo(function ChatInterface({
  agent,
  session,
  initialMessages,
  onSendMessage,
  onCancelStream,
  className,
  showTimestamps = true,
  headerContent,
  emptyState,
}: ChatInterfaceProps) {
  // Ref for scrolling to quoted messages
  const messageListRef = useRef<HTMLDivElement>(null);

  // Get state from store
  const isStreaming = useChatStore((state) => state.isStreaming);
  const streamedContent = useChatStore((state) => state.streamedContent);
  const reasoningContent = useChatStore((state) => state.reasoningContent);
  const isLoading = useChatStore((state) => state.isLoading);
  const error = useChatStore((state) => state.error);
  const setMessages = useChatStore((state) => state.setMessages);
  const addMessage = useChatStore((state) => state.addMessage);
  const quoteMessage = useChatStore((state) => state.quoteMessage);
  const setQuoteMessage = useChatStore((state) => state.setQuoteMessage);
  const clearQuoteMessage = useChatStore((state) => state.clearQuoteMessage);

  // Paginated messages hook - loads messages when session changes
  const {
    messages: paginatedMessages,
    isLoading: isLoadingMessages,
    hasMore,
    loadMore,
    appendMessage,
    error: loadError,
  } = usePaginatedMessages({
    sessionId: session?.id ?? null,
    pageSize: 50,
    autoLoad: true,
  });

  // Sync paginated messages to chat store
  useEffect(() => {
    if (paginatedMessages.length > 0 || session?.id) {
      setMessages(paginatedMessages);
    }
  }, [paginatedMessages, setMessages, session?.id]);

  // Use initial messages if provided and store is empty
  const displayMessages = paginatedMessages.length > 0
    ? paginatedMessages
    : (initialMessages || []);

  // Handle cancel stream
  const handleCancelStream = useCallback(() => {
    if (onCancelStream) {
      onCancelStream();
    }
  }, [onCancelStream]);

  /**
   * Build message content with quote context
   * When a message is quoted, include the quote context for the LLM
   */
  const buildMessageWithContext = useCallback(
    (content: string, quote?: Message | null): string => {
      if (!quote) return content;

      const quoteRole = quote.role === 'user' ? '用户' : agent.name || 'AI';
      const quotePreview = quote.content.length > 200
        ? quote.content.slice(0, 200) + '...'
        : quote.content;

      return `> 引用 ${quoteRole} 的消息:\n> ${quotePreview}\n\n${content}`;
    },
    [agent.name]
  );

  // Handle send message (with optional quote context)
  const handleSendMessage = useCallback(
    (content: string) => {
      // Include quote context if quoting a message
      const contentWithContext = buildMessageWithContext(content, quoteMessage);

      // Send message with quote info
      onSendMessage(contentWithContext);

      // Note: clearQuoteMessage is handled in ChatInput after send
    },
    [onSendMessage, quoteMessage, buildMessageWithContext]
  );

  // Handle quote message selection
  const handleQuoteMessage = useCallback(
    (message: Message) => {
      setQuoteMessage(message);
    },
    [setQuoteMessage]
  );

  // Handle quote link click (scroll to quoted message)
  const handleQuoteClick = useCallback(
    (messageId: number) => {
      // Find the message element by data attribute or id
      const messageElement = messageListRef.current?.querySelector(
        `[data-message-id="${messageId}"]`
      );

      if (messageElement) {
        messageElement.scrollIntoView({
          behavior: 'smooth',
          block: 'center',
        });

        // Add highlight effect
        messageElement.classList.add('ring-2', 'ring-primary', 'ring-offset-2');
        setTimeout(() => {
          messageElement.classList.remove('ring-2', 'ring-primary', 'ring-offset-2');
        }, 2000);
      }
    },
    []
  );

  // Handle load more messages
  const handleLoadMore = useCallback(async () => {
    await loadMore();
  }, [loadMore]);

  // Get agent personality type for theming
  const personalityType = agent.mbti_type as MBTIType | undefined;

  return (
    <div
      className={cn(
        'flex flex-col h-full bg-background relative',
        className
      )}
      role="main"
      aria-label={`与 ${agent.name} 的对话`}
    >
      {/* Header */}
      <ChatHeader agent={agent} session={session} isStreaming={isStreaming}>
        {headerContent}
      </ChatHeader>

      {/* Error display */}
      {(error || loadError) && <ErrorDisplay error={error || loadError || ''} />}

      {/* Message list */}
      <MessageList
        messages={displayMessages}
        personalityType={personalityType}
        agentName={agent.name}
        isStreaming={isStreaming}
        streamedContent={streamedContent}
        reasoningContent={reasoningContent}
        onCancelStream={handleCancelStream}
        showTimestamps={showTimestamps}
        emptyState={emptyState}
        hasMore={hasMore}
        onLoadMore={handleLoadMore}
        isLoadingMore={isLoadingMessages && displayMessages.length > 0}
        onQuoteMessage={handleQuoteMessage}
        onQuoteClick={handleQuoteClick}
      />

      {/* Loading overlay */}
      {(isLoading || isLoadingMessages) && displayMessages.length === 0 && <LoadingOverlay />}

      {/* Message input with quote support */}
      <ChatInput
        onSend={handleSendMessage}
        onCancel={onCancelStream}
        isStreaming={isStreaming}
        personalityType={personalityType}
        agentName={agent.name}
        placeholder={`发送消息给 ${agent.name}...`}
        quoteMessage={quoteMessage}
        onCancelQuote={clearQuoteMessage}
      />
    </div>
  );
});

export default ChatInterface;