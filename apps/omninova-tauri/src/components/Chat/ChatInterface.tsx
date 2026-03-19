/**
 * Chat Interface Container Component
 *
 * Main chat interface that integrates message list, streaming display,
 * and provides a complete chat experience with agent personality theming.
 *
 * [Source: Story 4.4 - ChatInterface 组件基础]
 */

import { memo, useCallback } from 'react';
import { cn } from '@/lib/utils';
import { type AgentModel, type MBTIType } from '@/types/agent';
import { type Message, type Session } from '@/types/session';
import { useChatStore } from '@/stores/chatStore';
import MessageList from './MessageList';

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
  isStreaming?: boolean;
  children?: React.ReactNode;
}

const ChatHeader = memo(function ChatHeader({
  agent,
  isStreaming,
  children,
}: ChatHeaderProps) {
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
            {isStreaming ? '正在输入...' : agent.domain || 'AI 助手'}
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
 * Loading overlay for initial load
 */
const LoadingOverlay = memo(function LoadingOverlay() {
  return (
    <div className="absolute inset-0 flex items-center justify-center bg-background/80 backdrop-blur-sm z-10">
      <div className="flex flex-col items-center gap-3">
        <div className="w-8 h-8 border-2 border-primary border-t-transparent rounded-full animate-spin" />
        <span className="text-sm text-muted-foreground">加载中...</span>
      </div>
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
  session, // eslint-disable-line @typescript-eslint/no-unused-vars -- reserved for future use
  initialMessages,
  onSendMessage, // eslint-disable-line @typescript-eslint/no-unused-vars -- handled by parent integration
  onCancelStream,
  className,
  showTimestamps = true,
  headerContent,
  emptyState,
}: ChatInterfaceProps) {
  // Get state from store
  const messages = useChatStore((state) => state.messages);
  const isStreaming = useChatStore((state) => state.isStreaming);
  const streamedContent = useChatStore((state) => state.streamedContent);
  const reasoningContent = useChatStore((state) => state.reasoningContent);
  const isLoading = useChatStore((state) => state.isLoading);
  const error = useChatStore((state) => state.error);

  // Use initial messages if provided and store is empty
  const displayMessages = messages.length > 0 ? messages : (initialMessages || []);

  // Handle cancel stream
  const handleCancelStream = useCallback(() => {
    if (onCancelStream) {
      onCancelStream();
    }
  }, [onCancelStream]);

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
      <ChatHeader agent={agent} isStreaming={isStreaming}>
        {headerContent}
      </ChatHeader>

      {/* Error display */}
      {error && <ErrorDisplay error={error} />}

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
      />

      {/* Loading overlay */}
      {isLoading && displayMessages.length === 0 && <LoadingOverlay />}
    </div>
  );
});

export default ChatInterface;