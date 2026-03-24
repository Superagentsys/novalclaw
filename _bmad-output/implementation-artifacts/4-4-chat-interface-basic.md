# Story 4.4: ChatInterface 组件基础

Status: done

## Story

As a 用户,
I want 看到清晰的对话界面,
so that 我可以轻松阅读和追踪对话内容.

## Acceptance Criteria

1. **AC1: 消息列表显示** - 消息列表正确显示用户和代理消息 ✅
2. **AC2: 消息样式区分** - 用户消息和代理消息有不同的视觉样式（对齐、颜色） ✅
3. **AC3: 人格主题颜色** - 消息气泡颜色反映代理的人格类型主题 ✅
4. **AC4: 时间戳显示** - 消息显示时间戳 ✅
5. **AC5: 自动滚动** - 消息列表支持自动滚动到最新消息 ✅

## Tasks / Subtasks

- [x] Task 1: Message Component (AC: #2, #3, #4)
  - [x] 1.1 Create `apps/omninova-tauri/src/components/Chat/MessageBubble.tsx`
  - [x] 1.2 Implement role-based alignment (user: right, assistant: left)
  - [x] 1.3 Add personality color theming using `getPersonalityColors()`
  - [x] 1.4 Format and display timestamp using `Intl.DateTimeFormat`
  - [x] 1.5 Add message content rendering with markdown support
  - [x] 1.6 Add JSDoc documentation and TypeScript types

- [x] Task 2: Message List Component (AC: #1, #5)
  - [x] 2.1 Create `apps/omninova-tauri/src/components/Chat/MessageList.tsx`
  - [ ] 2.2 Implement virtual scrolling for large message lists (optional optimization)
  - [x] 2.3 Add auto-scroll to bottom on new messages
  - [x] 2.4 Implement smart scroll detection (don't auto-scroll if user scrolled up)
  - [x] 2.5 Add `scrollIntoView` smooth scrolling behavior
  - [x] 2.6 Handle empty state display
  - [x] 2.7 Add accessibility support (aria-live for new messages)

- [x] Task 3: Chat Interface Container (AC: #1)
  - [x] 3.1 Create `apps/omninova-tauri/src/components/Chat/ChatInterface.tsx`
  - [x] 3.2 Define props interface: agent, session, messages, onSendMessage
  - [x] 3.3 Integrate MessageList and MessageBubble components
  - [x] 3.4 Add header with agent name and status indicator
  - [x] 3.5 Integrate with StreamingMessage for active streams
  - [x] 3.6 Handle loading and error states
  - [x] 3.7 Add responsive layout for different screen sizes

- [x] Task 4: Chat Store with Zustand (AC: #1)
  - [x] 4.1 Create `apps/omninova-tauri/src/stores/chatStore.ts`
  - [x] 4.2 Define store state: messages, isLoading, error, activeSessionId
  - [x] 4.3 Implement actions: addMessage, setMessages, clearMessages, setLoading
  - [x] 4.4 Add selector hooks for derived state
  - [x] 4.5 Implement persistence middleware (localStorage backup)
  - [x] 4.6 Add TypeScript types for store

- [x] Task 5: Streaming Integration (AC: #1)
  - [x] 5.1 Connect useStreamChat hook to ChatInterface
  - [x] 5.2 Display StreamingMessage during active stream
  - [x] 5.3 Convert streamed content to Message on completion
  - [x] 5.4 Handle stream cancellation and error states
  - [x] 5.5 Maintain message history across sessions

- [x] Task 6: Unit Tests (All ACs)
  - [x] 6.1 Test MessageBubble with different roles and personalities
  - [x] 6.2 Test MessageList auto-scroll behavior
  - [x] 6.3 Test ChatInterface state management
  - [x] 6.4 Test chat store actions and state updates
  - [x] 6.5 Test streaming integration with mock events

## Dev Notes

### Component Architecture

```
ChatInterface/
├── ChatInterface.tsx      # Main container component
├── MessageList.tsx        # Message list with auto-scroll
├── MessageBubble.tsx      # Individual message display
├── StreamingMessage.tsx   # Already exists (Story 4.3)
└── ChatInput.tsx          # Story 4.6 (message input)
```

### Existing Types

From `apps/omninova-tauri/src/types/session.ts`:
```typescript
export type MessageRole = 'user' | 'assistant' | 'system';

export interface Message {
  id: number;
  sessionId: number;
  role: MessageRole;
  content: string;
  createdAt: number;
}
```

### Personality Color Integration

From `apps/omninova-tauri/src/lib/personality-colors.ts`:
```typescript
export function getPersonalityColors(type: MBTIType): PersonalityColorConfig {
  return personalityColors[type];
}

// PersonalityColorConfig contains:
// - primary: Main color for message bubbles
// - accent: Secondary color for accents
// - tone: 'analytical' | 'creative' | 'structured' | 'energetic'
```

### Message Bubble Styling Pattern

```tsx
// User messages: Right-aligned, primary color background
// Assistant messages: Left-aligned, personality-themed

<div className={cn(
  'rounded-lg px-4 py-2 max-w-[80%]',
  role === 'user' ? 'ml-auto bg-primary text-primary-foreground' : 'mr-auto'
)}>
```

### Auto-Scroll Implementation

```tsx
// Use useRef and useEffect for auto-scroll
const messagesEndRef = useRef<HTMLDivElement>(null);
const shouldAutoScroll = useRef(true);

useEffect(() => {
  if (shouldAutoScroll.current) {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }
}, [messages]);
```

### Zustand Store Pattern

```typescript
// apps/omninova-tauri/src/stores/chatStore.ts
import { create } from 'zustand';
import { persist } from 'zustand/middleware';

interface ChatState {
  messages: Message[];
  activeSessionId: number | null;
  isLoading: boolean;
  addMessage: (message: Message) => void;
  setMessages: (messages: Message[]) => void;
}

export const useChatStore = create<ChatState>()(
  persist(
    (set) => ({
      messages: [],
      activeSessionId: null,
      isLoading: false,
      addMessage: (message) => set((state) => ({
        messages: [...state.messages, message]
      })),
      setMessages: (messages) => set({ messages }),
    }),
    { name: 'chat-storage' }
  )
);
```

### Streaming Integration

Use existing `useStreamChat` hook from Story 4.3:
```typescript
const {
  isStreaming,
  streamedContent,
  sendMessage,
  cancelStream,
} = useStreamChat({
  onStreamDone: (sessionId, messageId) => {
    // Add completed message to store
  }
});
```

### Testing Standards

1. **Unit Tests** - Use vitest with React Testing Library
2. **Test File Location** - `src/components/Chat/__tests__/`
3. **Test Patterns**:
   - Render components with mock data
   - Test personality color theming with different MBTI types
   - Test auto-scroll behavior
   - Test store actions

### Files to Create

- `apps/omninova-tauri/src/components/Chat/MessageBubble.tsx` - Message bubble component
- `apps/omninova-tauri/src/components/Chat/MessageList.tsx` - Message list with scroll
- `apps/omninova-tauri/src/components/Chat/ChatInterface.tsx` - Main container
- `apps/omninova-tauri/src/stores/chatStore.ts` - Zustand store for chat state

### Files to Modify

- `apps/omninova-tauri/src/components/Chat/index.ts` - Export new components
- `apps/omninova-tauri/package.json` - Add zustand dependency (if not present)

### Files to Reference

- `apps/omninova-tauri/src/components/Chat/StreamingMessage.tsx` - Existing streaming component
- `apps/omninova-tauri/src/hooks/useStreamChat.ts` - Existing streaming hook
- `apps/omninova-tauri/src/types/session.ts` - Message and Session types
- `apps/omninova-tauri/src/types/agent.ts` - Agent and streaming types
- `apps/omninova-tauri/src/lib/personality-colors.ts` - MBTI color configurations
- `_bmad-output/implementation-artifacts/4-3-streaming-response.md` - Previous story patterns

### Dependencies to Add

```bash
npm install zustand
```

## References

- [Source: _bmad-output/planning-artifacts/epics.md#L802-L816] - Story 4.4 requirements
- [Source: apps/omninova-tauri/src/types/session.ts] - Message and Session types
- [Source: apps/omninova-tauri/src/lib/personality-colors.ts] - Personality color system
- [Source: apps/omninova-tauri/src/components/Chat/StreamingMessage.tsx] - Existing streaming display
- [Source: apps/omninova-tauri/src/hooks/useStreamChat.ts] - Streaming hook
- [Source: _bmad-output/implementation-artifacts/4-3-streaming-response.md] - Previous story context

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

### Completion Notes List

- All 5 acceptance criteria implemented and verified
- Virtual scrolling (Task 2.2) skipped as optional optimization
- 45 unit tests passing covering all components and store
- Code review completed: fixed invalid CSS variable usage in MessageBubble.tsx, removed dead useEffect in useChat.ts

### File List

- `apps/omninova-tauri/src/components/Chat/MessageBubble.tsx` - Created
- `apps/omninova-tauri/src/components/Chat/MessageList.tsx` - Created
- `apps/omninova-tauri/src/components/Chat/ChatInterface.tsx` - Created
- `apps/omninova-tauri/src/components/Chat/index.ts` - Created
- `apps/omninova-tauri/src/stores/chatStore.ts` - Created
- `apps/omninova-tauri/src/hooks/useChat.ts` - Created
- `apps/omninova-tauri/src/components/Chat/__tests__/MessageBubble.test.tsx` - Created
- `apps/omninova-tauri/src/components/Chat/__tests__/MessageList.test.tsx` - Created
- `apps/omninova-tauri/src/stores/__tests__/chatStore.test.ts` - Created