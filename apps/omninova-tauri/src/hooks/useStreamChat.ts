/**
 * Streaming chat hook for OmniNova Claw
 *
 * Provides real-time streaming chat functionality with event-based
 * communication between Rust backend and React frontend.
 *
 * [Source: Story 4.3 - 流式响应处理]
 */

import { useState, useCallback, useEffect, useRef } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import {
  type StreamEvent,
  type StreamStartEvent,
  type StreamDeltaEvent,
  type StreamToolCallEvent,
  type StreamDoneEvent,
  type StreamErrorEvent,
  type StreamChatRequest,
  type StreamingStatus,
  type TokenUsage,
} from '@/types/agent';

/**
 * Hook options
 */
export interface UseStreamChatOptions {
  /** Callback when stream starts */
  onStreamStart?: (sessionId: number, requestId: string) => void;
  /** Callback for each content delta */
  onStreamDelta?: (delta: string, reasoning?: string) => void;
  /** Callback for tool call events */
  onStreamToolCall?: (toolName: string, toolArgs: unknown) => void;
  /** Callback when stream completes */
  onStreamDone?: (sessionId: number, messageId: number, usage?: TokenUsage) => void;
  /** Callback when stream encounters an error */
  onStreamError?: (code: string, message: string, partialContent?: string) => void;
  /** Auto-scroll to bottom on new content */
  autoScroll?: boolean;
}

/**
 * Hook return type
 */
export interface UseStreamChatReturn {
  /** Current streaming status */
  status: StreamingStatus;
  /** Whether a stream is active */
  isStreaming: boolean;
  /** Accumulated streamed content */
  streamedContent: string;
  /** Accumulated reasoning content (for thinking models) */
  reasoningContent: string;
  /** Current session ID */
  sessionId: number | null;
  /** Current request ID */
  requestId: string | null;
  /** Final message ID (available after done) */
  messageId: number | null;
  /** Token usage statistics */
  usage: TokenUsage | null;
  /** Error information */
  error: { code: string; message: string } | null;
  /** Send a streaming message */
  sendMessage: (request: StreamChatRequest) => Promise<void>;
  /** Cancel the active stream */
  cancelStream: () => Promise<void>;
  /** Reset the hook state */
  reset: () => void;
}

/**
 * Streaming chat hook
 *
 * Provides real-time streaming chat functionality with automatic event
 * listener management and cancellation support.
 *
 * @param options - Configuration options and callbacks
 * @returns Streaming state and control functions
 *
 * @example
 * ```tsx
 * function ChatInterface() {
 *   const {
 *     isStreaming,
 *     streamedContent,
 *     sendMessage,
 *     cancelStream,
 *     error,
 *   } = useStreamChat({
 *     onStreamDone: (sessionId, messageId) => {
 *       console.log('Stream completed:', sessionId, messageId);
 *     },
 *   });
 *
 *   const handleSend = async () => {
 *     await sendMessage({
 *       agentId: 1,
 *       message: 'Hello!',
 *     });
 *   };
 *
 *   return (
 *     <div>
 *       <pre>{streamedContent}</pre>
 *       {isStreaming && <button onClick={cancelStream}>Cancel</button>}
 *     </div>
 *   );
 * }
 * ```
 */
export function useStreamChat(options: UseStreamChatOptions = {}): UseStreamChatReturn {
  const {
    onStreamStart,
    onStreamDelta,
    onStreamToolCall,
    onStreamDone,
    onStreamError,
  } = options;

  // State
  const [status, setStatus] = useState<StreamingStatus>('idle');
  const [streamedContent, setStreamedContent] = useState('');
  const [reasoningContent, setReasoningContent] = useState('');
  const [sessionId, setSessionId] = useState<number | null>(null);
  const [requestId, setRequestId] = useState<string | null>(null);
  const [messageId, setMessageId] = useState<number | null>(null);
  const [usage, setUsage] = useState<TokenUsage | null>(null);
  const [error, setError] = useState<{ code: string; message: string } | null>(null);

  // Refs for cleanup
  const unlistenersRef = useRef<UnlistenFn[]>([]);
  const isMountedRef = useRef(true);

  /**
   * Setup event listeners
   */
  const setupListeners = useCallback(async () => {
    // Cleanup any existing listeners
    unlistenersRef.current.forEach((unlisten) => unlisten());
    unlistenersRef.current = [];

    // Create event listeners
    const unlistenStart = await listen<StreamStartEvent>('stream:start', (event) => {
      if (!isMountedRef.current) return;
      const { sessionId, requestId } = event.payload;
      setSessionId(sessionId);
      setRequestId(requestId);
      setStatus('streaming');
      onStreamStart?.(sessionId, requestId);
    });

    const unlistenDelta = await listen<StreamDeltaEvent>('stream:delta', (event) => {
      if (!isMountedRef.current) return;
      const { delta, reasoning } = event.payload;
      setStreamedContent((prev) => prev + delta);
      if (reasoning) {
        setReasoningContent((prev) => prev + reasoning);
      }
      onStreamDelta?.(delta, reasoning);
    });

    const unlistenToolCall = await listen<StreamToolCallEvent>('stream:toolCall', (event) => {
      if (!isMountedRef.current) return;
      const { toolName, toolArgs } = event.payload;
      onStreamToolCall?.(toolName, toolArgs);
    });

    const unlistenDone = await listen<StreamDoneEvent>('stream:done', (event) => {
      if (!isMountedRef.current) return;
      const { sessionId, messageId, usage } = event.payload;
      setMessageId(messageId);
      if (usage) setUsage(usage);
      setStatus('done');
      onStreamDone?.(sessionId, messageId, usage);
    });

    const unlistenError = await listen<StreamErrorEvent>('stream:error', (event) => {
      if (!isMountedRef.current) return;
      const { code, message, partialContent } = event.payload;
      setError({ code, message });
      setStatus('error');
      if (partialContent) {
        setStreamedContent(partialContent);
      }
      onStreamError?.(code, message, partialContent);
    });

    unlistenersRef.current = [
      unlistenStart,
      unlistenDelta,
      unlistenToolCall,
      unlistenDone,
      unlistenError,
    ];
  }, [onStreamStart, onStreamDelta, onStreamToolCall, onStreamDone, onStreamError]);

  /**
   * Send a streaming message
   */
  const sendMessage = useCallback(
    async (request: StreamChatRequest) => {
      // Reset state for new message
      setStreamedContent('');
      setReasoningContent('');
      setMessageId(null);
      setUsage(null);
      setError(null);
      setStatus('streaming');

      try {
        // Setup listeners before invoking
        await setupListeners();

        // Invoke the streaming command
        await invoke('stream_chat', { request });
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setError({ code: 'INVOKE_ERROR', message });
        setStatus('error');
      }
    },
    [setupListeners]
  );

  /**
   * Cancel the active stream
   */
  const cancelStream = useCallback(async () => {
    if (sessionId === null || status !== 'streaming') {
      return;
    }

    try {
      const wasCancelled = await invoke<boolean>('cancel_stream', { sessionId });
      if (wasCancelled) {
        setStatus('cancelled');
      }
    } catch (err) {
      console.error('Failed to cancel stream:', err);
    }
  }, [sessionId, status]);

  /**
   * Reset hook state
   */
  const reset = useCallback(() => {
    setStatus('idle');
    setStreamedContent('');
    setReasoningContent('');
    setSessionId(null);
    setRequestId(null);
    setMessageId(null);
    setUsage(null);
    setError(null);
  }, []);

  // Cleanup on unmount
  useEffect(() => {
    isMountedRef.current = true;
    return () => {
      isMountedRef.current = false;
      unlistenersRef.current.forEach((unlisten) => unlisten());
    };
  }, []);

  // Cancel stream on unmount if streaming
  useEffect(() => {
    return () => {
      if (status === 'streaming' && sessionId !== null) {
        invoke('cancel_stream', { sessionId }).catch(console.error);
      }
    };
  }, [status, sessionId]);

  return {
    status,
    isStreaming: status === 'streaming',
    streamedContent,
    reasoningContent,
    sessionId,
    requestId,
    messageId,
    usage,
    error,
    sendMessage,
    cancelStream,
    reset,
  };
}

export default useStreamChat;