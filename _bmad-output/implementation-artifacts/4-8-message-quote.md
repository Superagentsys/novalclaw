# Story 4.8: 消息引用功能

Status: done

## Story

As a 用户,
I want 引用之前的消息进行回复,
so that 我可以明确指出我在回应哪些内容.

## Acceptance Criteria

1. **AC1: 引用消息卡片显示** - 选择引用某条消息后，引用的消息以卡片形式显示在输入框上方
2. **AC2: 发送包含引用上下文** - 发送回复时，引用上下文被包含在请求中发送给 LLM
3. **AC3: 取消引用** - 引用消息可以取消（清除引用状态）
4. **AC4: 回复消息显示引用链接** - 回复消息在气泡中显示被引用消息的来源链接/预览

## Tasks / Subtasks

- [x] Task 1: 类型定义扩展 (AC: #1, #4)
  - [x] 1.1 在 `src/types/session.ts` 添加 `quoteMessageId?: number` 字段到 `Message` 接口
  - [x] 1.2 添加 `QuoteMessage` 类型（引用消息的简化视图：id, content preview, role, sender name）
  - [x] 1.3 更新 JSDoc 文档说明引用关系

- [x] Task 2: chatStore 状态扩展 (AC: #1, #3)
  - [x] 2.1 添加 `quoteMessage: Message | null` 状态字段
  - [x] 2.2 实现 `setQuoteMessage(message: Message | null)` action
  - [x] 2.3 实现 `clearQuoteMessage()` action
  - [x] 2.4 添加 JSDoc 文档

- [x] Task 3: QuoteCard 组件 (AC: #1, #3)
  - [x] 3.1 创建 `src/components/Chat/QuoteCard.tsx`
  - [x] 3.2 实现 QuoteCardProps 接口：quote, onCancel, className
  - [x] 3.3 显示引用消息的预览内容（截断至 100 字符）
  - [x] 3.4 显示发送者角色图标（用户/AI）
  - [x] 3.5 添加取消引用按钮（X 图标）
  - [x] 3.6 使用 memo() 优化渲染
  - [x] 3.7 添加可访问性属性（aria-label="引用的消息"）
  - [x] 3.8 使用 cn() 合并类名，支持主题色

- [x] Task 4: ChatInput 集成引用功能 (AC: #1, #2, #3)
  - [x] 4.1 添加 `quoteMessage?: Message | null` prop
  - [x] 4.2 添加 `onCancelQuote?: () => void` prop
  - [x] 4.3 在 textarea 上方渲染 QuoteCard（当 quoteMessage 存在时）
  - [x] 4.4 修改发送逻辑，将引用内容附加到消息元数据
  - [x] 4.5 发送成功后自动清除引用状态
  - [x] 4.6 更新 JSDoc 文档

- [x] Task 5: MessageBubble 显示引用链接 (AC: #4)
  - [x] 5.1 添加 `quoteMessageId?: number` prop
  - [x] 5.2 添加 `onQuoteClick?: (messageId: number) => void` prop
  - [x] 5.3 当消息有 quoteMessageId 时，渲染引用预览卡片
  - [x] 5.4 点击引用卡片可滚动到原消息（可选）
  - [x] 5.5 使用主题色区分用户/AI 引用来源

- [x] Task 6: MessageList 引用消息选择 (AC: #1)
  - [x] 6.1 添加 `onQuoteMessage?: (message: Message) => void` prop
  - [x] 6.2 在 MessageBubble 添加"引用"按钮（hover 显示或右键菜单）
  - [x] 6.3 点击引用按钮调用 onQuoteMessage 回调
  - [x] 6.4 考虑移动端长按触发引用

- [x] Task 7: ChatInterface 集成 (AC: #1, #2, #3, #4)
  - [x] 7.1 从 chatStore 获取 quoteMessage 状态
  - [x] 7.2 传递 quoteMessage 给 ChatInput
  - [x] 7.3 实现 handleQuoteMessage 回调设置引用消息
  - [x] 7.4 传递 onQuoteMessage 给 MessageList
  - [x] 7.5 修改 handleSendMessage 包含引用上下文

- [x] Task 8: 后端消息引用支持 (AC: #2, #4)
  - [x] 8.1 检查后端 MessageStore 是否支持 quote_message_id 字段
  - [x] 8.2 如需修改，更新 Rust Message 结构体
  - [x] 8.3 更新 Tauri 命令（create_message, list_messages）支持引用字段
  - [x] 8.4 更新数据库 schema（如需要）

- [x] Task 9: 单元测试 (All ACs)
  - [x] 9.1 测试 chatStore 引用状态管理
  - [x] 9.2 测试 QuoteCard 组件渲染和取消回调
  - [x] 9.3 测试 ChatInput 引用功能集成
  - [x] 9.4 测试 MessageBubble 引用链接显示
  - [x] 9.5 测试发送消息时引用上下文包含

## Dev Notes

### 现有实现分析

**chatStore.ts 现有结构:**
```typescript
// apps/omninova-tauri/src/stores/chatStore.ts
export interface ChatState {
  messages: Message[];
  activeSessionId: number | null;
  activeAgentId: number | null;
  isLoading: boolean;
  error: string | null;
  isStreaming: boolean;
  streamedContent: string;
  reasoningContent: string;
}
```

需要添加：
```typescript
export interface ChatState {
  // ...existing fields
  quoteMessage: Message | null;  // 当前引用的消息
}
```

**Message 类型 (session.ts):**
```typescript
export interface Message {
  id: number;
  sessionId: number;
  role: MessageRole;
  content: string;
  createdAt: number;
  // 需要添加：
  quoteMessageId?: number;  // 引用的消息 ID
}
```

**ChatInput 现有 Props:**
```typescript
export interface ChatInputProps {
  onSend: (content: string) => void;
  onCancel?: () => void;
  isStreaming?: boolean;
  disabled?: boolean;
  placeholder?: string;
  personalityType?: MBTIType;
  className?: string;
  defaultValue?: string;
  value?: string;
  onChange?: (value: string) => void;
  maxRows?: number;
  minRows?: number;
  // 需要添加：
  quoteMessage?: Message | null;
  onCancelQuote?: () => void;
}
```

### UI 组件设计

**QuoteCard 组件设计:**
```tsx
interface QuoteCardProps {
  /** 被引用的消息 */
  quote: Message;
  /** 取消引用回调 */
  onCancel: () => void;
  /** Agent 名称（用于显示引用来源） */
  agentName?: string;
  /** 主题色 */
  personalityType?: MBTIType;
  /** 自定义类名 */
  className?: string;
}

// UI 结构：
// ┌─────────────────────────────────────┐
// │ 📎 引用回复                    [X] │
// ├─────────────────────────────────────┤
// │ [用户/AI图标] 被引用的消息内容预览... │
// └─────────────────────────────────────┘
```

**MessageBubble 引用链接设计:**
```tsx
// 在消息气泡顶部显示引用预览
{quoteMessageId && (
  <div className="quote-preview text-xs border-l-2 border-primary/50 pl-2 mb-2 text-muted-foreground">
    <span className="font-medium">{senderName}:</span>
    <span className="ml-1 truncate">{quoteContent}</span>
  </div>
)}
```

### 引用消息流程

**选择引用流程:**
```
用户点击消息的"引用"按钮
    ↓
MessageList.onQuoteMessage(message)
    ↓
ChatInterface.handleQuoteMessage(message)
    ↓
chatStore.setQuoteMessage(message)
    ↓
ChatInput 接收 quoteMessage prop
    ↓
QuoteCard 显示在输入框上方
```

**发送带引用消息流程:**
```
用户输入内容并发送
    ↓
ChatInput.onSend(content, quoteMessage)
    ↓
ChatInterface.handleSendMessage(content)
    ↓
构造消息：{ content, quoteMessageId: quoteMessage.id }
    ↓
chatStore.addMessage(newMessage)
    ↓
invoke('create_message', { ...message, quoteMessageId })
    ↓
清除引用状态: setQuoteMessage(null)
    ↓
消息发送完成
```

### 引用上下文发送给 LLM

当发送带引用的回复时，需要将引用上下文包含在发送给 LLM 的消息中：

```typescript
// 在 agent dispatcher 或 send message 逻辑中
function buildMessageContext(
  content: string,
  quoteMessage?: Message
): string {
  if (!quoteMessage) return content;

  const quoteRole = quoteMessage.role === 'user' ? '用户' : 'AI';
  const quotePreview = truncate(quoteMessage.content, 200);

  return `> 引用 ${quoteRole} 的消息:\n> ${quotePreview}\n\n${content}`;
}
```

### 后端消息存储

检查 `crates/omninova-core/src/session/store.rs` 中 Message 结构：

```rust
pub struct Message {
    pub id: i64,
    pub session_id: i64,
    pub role: MessageRole,
    pub content: String,
    pub created_at: i64,
    // 可能需要添加：
    pub quote_message_id: Option<i64>,
}
```

数据库 schema 可能需要添加列：
```sql
ALTER TABLE messages ADD COLUMN quote_message_id INTEGER REFERENCES messages(id);
```

### 测试标准

1. **单元测试** - 使用 vitest + React Testing Library
2. **测试文件位置** - `src/components/Chat/__tests__/`
3. **测试模式**:
   - QuoteCard 渲染和交互测试
   - chatStore 引用状态管理测试
   - ChatInput 引用集成测试
   - 发送带引用消息测试

### 上一个 Story 学习 (4-7-chat-history-persistence)

- 使用 `memo` 优化组件重渲染
- 使用 `cn()` 工具函数合并类名
- 遵循 JSDoc 注释规范
- 测试文件与源文件目录分离 (`__tests__/`)
- 测试覆盖多种场景：正常、边界、错误状态
- 使用 `useCallback` 优化回调函数

### 可访问性要求

1. **QuoteCard**: aria-label="引用的消息"，取消按钮 aria-label="取消引用"
2. **引用按钮**: aria-label="引用此消息"
3. **引用链接**: aria-label="跳转到被引用的消息"
4. **键盘导航**: Tab 可以聚焦引用卡片和取消按钮

### 项目结构注意事项

- 组件放置在 `src/components/Chat/` 目录
- 类型定义在 `src/types/` 目录
- Store 在 `src/stores/` 目录
- 遵循现有的文件命名规范（PascalCase 组件，camelCase 工具）

## References

- [Source: _bmad-output/planning-artifacts/epics.md#L867-L881] - Story 4.8 requirements
- [Source: apps/omninova-tauri/src/stores/chatStore.ts] - 现有 chat store
- [Source: apps/omninova-tauri/src/types/session.ts] - Message 类型定义
- [Source: apps/omninova-tauri/src/components/Chat/ChatInput.tsx] - 消息输入组件
- [Source: apps/omninova-tauri/src/components/Chat/MessageBubble.tsx] - 消息气泡组件
- [Source: apps/omninova-tauri/src/components/Chat/MessageList.tsx] - 消息列表组件
- [Source: apps/omninova-tauri/src/components/Chat/ChatInterface.tsx] - 主聊天接口
- [Source: crates/omninova-core/src/session/store.rs] - 后端消息存储
- [Source: _bmad-output/implementation-artifacts/4-7-chat-history-persistence.md] - 前序 Story

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

N/A

### Completion Notes List

Story implementation completed successfully across 3 commits:

**Commit 1 (9436ec8):** feat(types): add quote message types for reply feature
- Added `quoteMessageId` to Message interface
- Added `QuoteMessage` type for UI preview

**Commit 2 (629adfc):** feat(chat): add message quote/reply functionality (Story 4.8)
- Created QuoteCard component with memo, accessibility, and theme support
- Extended chatStore with quoteMessage state, setQuoteMessage, clearQuoteMessage
- Integrated QuoteCard in ChatInput
- Added quote preview to MessageBubble
- Added quote button to MessageList (hover to show)
- Connected all components in ChatInterface with buildMessageWithContext

**Commit 3 (ba78537):** fix: add quote_message_id to AgentService and update tests
- Added migration 007_message_quote with up/down SQL
- Updated Message/NewMessage structs in Rust
- Updated MessageStore INSERT/SELECT operations
- Added backend tests for quote functionality

**Test Results:**
- Frontend: 673 tests pass (including 3 new test files)
- Backend: 34 session tests pass (including quote tests)

**All 4 Acceptance Criteria satisfied:**
- AC1: QuoteCard displays above input when message selected
- AC2: Quote context included in LLM request via buildMessageWithContext
- AC3: clearQuoteMessage action and cancel button work correctly
- AC4: MessageBubble shows quote preview with click-to-scroll

### File List

**Created:**
- `apps/omninova-tauri/src/components/Chat/QuoteCard.tsx` - Quote card component
- `apps/omninova-tauri/src/components/Chat/QuoteCard.test.tsx` - Quote card tests
- `apps/omninova-tauri/src/stores/chatStore.test.ts` - Chat store quote state tests
- `apps/omninova-tauri/src/components/Chat/MessageBubble.test.tsx` - MessageBubble quote display tests

**Modified:**
- `apps/omninova-tauri/src/types/session.ts` - Added quoteMessageId to Message, QuoteMessage type
- `apps/omninova-tauri/src/stores/chatStore.ts` - Added quoteMessage state and actions
- `apps/omninova-tauri/src/components/Chat/ChatInput.tsx` - Integrated QuoteCard and quote props
- `apps/omninova-tauri/src/components/Chat/MessageBubble.tsx` - Added quote preview display
- `apps/omninova-tauri/src/components/Chat/MessageList.tsx` - Added quote button and handlers
- `apps/omninova-tauri/src/components/Chat/ChatInterface.tsx` - Connected all quote functionality
- `crates/omninova-core/src/session/model.rs` - Added quote_message_id to Message/NewMessage
- `crates/omninova-core/src/session/store.rs` - Updated INSERT/SELECT for quote_message_id
- `crates/omninova-core/src/agent/service.rs` - Added quote_message_id: None to NewMessage usages
- `crates/omninova-core/src/db/migrations.rs` - Added migration 007_message_quote

## Change Log

| Date | Change |
|------|--------|
| 2026-03-19 | Story 4.8 implementation completed - all 9 tasks done, 673 frontend tests pass, 34 backend tests pass |
| 2026-03-19 | Code review: Fixed HIGH issue - added quote_message_id parameter to Tauri commands and AgentService methods. Added test_chat_with_quote_message_id test. All 472 Rust tests and 673 frontend tests pass. |