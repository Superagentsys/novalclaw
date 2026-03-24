# Story 4.7: 对话历史持久化与导航

Status: complete

## Story

As a 用户,
I want 查看和继续之前的对话会话,
so that 我可以回顾历史对话并保持上下文连续性.

## Acceptance Criteria

1. **AC1: 会话列表显示** - 显示按时间排序的会话列表（最近在前）
2. **AC2: 加载对话内容** - 点击会话加载完整对话内容
3. **AC3: 分页加载** - 支持分页加载历史消息（每页 50 条）
4. **AC4: 继续对话** - 可以继续在历史会话中对话
5. **AC5: 创建新会话** - 可以创建新会话

## Tasks / Subtasks

- [x] Task 1: SessionStore 状态管理 (AC: #1, #2, #4, #5)
  - [x] 1.1 Create `apps/omninova-tauri/src/stores/sessionStore.ts`
  - [x] 1.2 Define SessionState interface with sessions list, active session, loading states
  - [x] 1.3 Implement loadSessions(agentId) action using Tauri invoke
  - [x] 1.4 Implement loadSession(sessionId) action for full session data
  - [x] 1.5 Implement createSession(agentId, title?) action
  - [x] 1.6 Implement switchSession(sessionId) action
  - [x] 1.7 Add persist middleware for session list caching
  - [x] 1.8 Add JSDoc documentation

- [x] Task 2: SessionList 组件 (AC: #1)
  - [x] 2.1 Create `apps/omninova-tauri/src/components/Chat/SessionList.tsx`
  - [x] 2.2 Implement session list UI with scroll container
  - [x] 2.3 Add "新建会话" button at top
  - [x] 2.4 Implement SessionItem sub-component with title, time, preview
  - [x] 2.5 Handle session selection and active state styling
  - [x] 2.6 Add loading skeleton for sessions
  - [x] 2.7 Add empty state when no sessions
  - [x] 2.8 Add accessibility attributes (aria-label, role="listbox")

- [x] Task 3: 分页加载 Hook (AC: #3)
  - [x] 3.1 Create `apps/omninova-tauri/src/hooks/usePaginatedMessages.ts`
  - [x] 3.2 Implement loadMessages(sessionId, page, pageSize) with Tauri invoke
  - [x] 3.3 Handle load more trigger (scroll to top detection)
  - [x] 3.4 Track hasMore state for pagination
  - [x] 3.5 Merge new messages with existing (prepend, preserve order)
  - [x] 3.6 Add loading state for pagination fetch

- [x] Task 4: ChatInterface 集成 (AC: #2, #3, #4)
  - [x] 4.1 Update ChatInterface to accept session prop
  - [x] 4.2 Add session switching logic
  - [x] 4.3 Integrate usePaginatedMessages hook
  - [x] 4.4 Add "加载更多" trigger at message list top
  - [x] 4.5 Preserve scroll position when loading more messages
  - [x] 4.6 Update header to show session title

- [x] Task 5: 新建会话功能 (AC: #5)
  - [x] 5.1 Add createSession handler in sessionStore
  - [x] 5.2 Create new session with auto-generated or user-specified title
  - [x] 5.3 Auto-switch to new session after creation
  - [x] 5.4 Clear messages and reset chat state
  - [x] 5.5 Persist new session to database via Tauri command

- [x] Task 6: 单元测试 (All ACs)
  - [x] 6.1 Test sessionStore actions and state
  - [x] 6.2 Test SessionList component rendering
  - [x] 6.3 Test SessionItem selection
  - [x] 6.4 Test usePaginatedMessages hook
  - [x] 6.5 Test createSession flow
  - [x] 6.6 Test session switching

## Dev Notes

### 现有实现分析

**后端 SessionStore (Rust) - 已实现:**
```rust
// crates/omninova-core/src/session/store.rs
impl SessionStore {
    pub fn find_by_agent(&self, agent_id: i64) -> Result<Vec<Session>, SessionError> {
        // 返回按 updated_at DESC 排序的会话列表
    }
}

impl MessageStore {
    pub fn find_latest_by_session(&self, session_id: i64, limit: usize) -> Result<Vec<Message>, SessionError> {
        // 分页查询，返回按 created_at ASC 排序的消息
    }
}
```

**Tauri 命令 - 已实现:**
```rust
// src-tauri/src/lib.rs
#[tauri::command]
async fn list_sessions_by_agent(agent_id: i64, ...) -> Result<Vec<Session>, String>

#[tauri::command]
async fn create_session(new_session: NewSession, ...) -> Result<Session, String>

#[tauri::command]
async fn list_messages_by_session(session_id: i64, ...) -> Result<Vec<Message>, String>
```

**chatStore.ts - 现有结构:**
```typescript
// apps/omninova-tauri/src/stores/chatStore.ts
interface ChatState {
  messages: Message[];
  activeSessionId: number | null;
  activeAgentId: number | null;
  // ... streaming states
}

interface ChatActions {
  setActiveSession: (sessionId: number | null, agentId?: number | null) => void;
  setMessages: (messages: Message[]) => void;
  clearMessages: () => void;
  // ...
}
```

**ChatInterface.tsx - 现有结构:**
```typescript
interface ChatInterfaceProps {
  agent: AgentModel;
  session?: Session | null;  // 已预留但未使用
  initialMessages?: Message[];
  onSendMessage: (content: string) => void;
  // ...
}
```

### 架构设计

**新增 SessionStore (前端):**
```typescript
// apps/omninova-tauri/src/stores/sessionStore.ts
interface SessionState {
  sessions: Session[];
  activeSessionId: number | null;
  isLoading: boolean;
  error: string | null;
}

interface SessionActions {
  loadSessions: (agentId: number) => Promise<void>;
  createSession: (agentId: number, title?: string) => Promise<Session>;
  switchSession: (sessionId: number) => void;
  deleteSession: (sessionId: number) => Promise<void>;
}
```

**组件结构:**
```
Chat/
├── ChatInterface.tsx      # 主容器（更新：集成 session 切换）
├── SessionList.tsx        # 新建：会话列表侧边栏
├── SessionItem.tsx        # 新建：单个会话项
├── MessageList.tsx        # 已有（更新：添加加载更多触发）
├── ChatInput.tsx          # 已有
├── StreamingMessage.tsx   # 已有
└── ...
```

### 分页加载实现

**usePaginatedMessages Hook:**
```typescript
interface UsePaginatedMessagesOptions {
  sessionId: number;
  pageSize?: number; // default 50
}

interface UsePaginatedMessagesReturn {
  messages: Message[];
  isLoading: boolean;
  hasMore: boolean;
  loadMore: () => Promise<void>;
  reset: () => void;
}

export function usePaginatedMessages(options: UsePaginatedMessagesOptions): UsePaginatedMessagesReturn {
  const [messages, setMessages] = useState<Message[]>([]);
  const [page, setPage] = useState(0);
  const [hasMore, setHasMore] = useState(true);
  const [isLoading, setIsLoading] = useState(false);
  const pageSize = options.pageSize ?? 50;

  const loadMessages = useCallback(async (pageNum: number) => {
    setIsLoading(true);
    try {
      // 调用后端分页 API
      // 注意：后端 MessageStore.find_latest_by_session 返回最新的 N 条
      // 需要更新为支持 offset/limit 分页
      const fetched = await invokeTauri<Message[]>('list_messages_paginated', {
        sessionId: options.sessionId,
        offset: pageNum * pageSize,
        limit: pageSize,
      });

      if (fetched.length < pageSize) {
        setHasMore(false);
      }

      // 新消息插入到前面（时间升序）
      setMessages(prev => [...fetched.reverse(), ...prev]);
    } finally {
      setIsLoading(false);
    }
  }, [options.sessionId, pageSize]);

  // ...
}
```

**后端分页支持 (需要新增 Tauri 命令):**
```rust
// 在 src-tauri/src/lib.rs 添加
#[tauri::command]
async fn list_messages_paginated(
    session_id: i64,
    offset: usize,
    limit: usize,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<Message>, String> {
    let store = MessageStore::new(state.db.clone());
    store.find_paginated_by_session(session_id, offset, limit)
        .map_err(|e| e.to_string())
}
```

**注意:** 现有 `find_latest_by_session(session_id, limit)` 只支持取最新 N 条。
需要扩展为支持 offset 参数，或者在前端实现"反向分页"逻辑。

### 会话列表 UI 模式

参考 `Chat.tsx` 中的 AvatarSession 列表模式:
```tsx
// 从 Chat.tsx 提取的会话列表模式
<ul className="chat-avatar-list">
  {avatars.map((a) => (
    <li key={a.id}>
      <button
        type="button"
        className={`chat-avatar-item ${a.id === activeAvatarId ? "is-active" : ""}`}
        onClick={() => setActiveAvatarId(a.id)}
      >
        <span className="chat-avatar-icon">◇</span>
        <span className="chat-avatar-name">{a.name}</span>
        <span className="chat-avatar-time">{a.lastAt}</span>
      </button>
    </li>
  ))}
</ul>
<button type="button" onClick={handleAddAvatar}>+ 新分身</button>
```

**SessionItem 组件设计:**
```tsx
interface SessionItemProps {
  session: Session;
  isActive: boolean;
  onClick: () => void;
  preview?: string; // 最后一条消息预览
}

const SessionItem = memo(function SessionItem({ session, isActive, onClick, preview }: SessionItemProps) {
  return (
    <button
      type="button"
      role="option"
      aria-selected={isActive}
      className={cn(
        "w-full text-left px-3 py-2 rounded-lg transition-colors",
        isActive ? "bg-primary/10 text-primary" : "hover:bg-muted"
      )}
      onClick={onClick}
    >
      <div className="font-medium truncate">{session.title || "新对话"}</div>
      <div className="text-xs text-muted-foreground mt-0.5">
        {formatRelativeTime(session.updatedAt)}
      </div>
      {preview && (
        <div className="text-xs text-muted-foreground truncate mt-1">
          {preview}
        </div>
      )}
    </button>
  );
});
```

### 会话切换流程

```
用户点击会话
    ↓
sessionStore.switchSession(sessionId)
    ↓
chatStore.setActiveSession(sessionId, agentId)
    ↓
usePaginatedMessages.loadMessages(sessionId)
    ↓
chatStore.setMessages(messages)
    ↓
ChatInterface 重新渲染
```

### 创建新会话流程

```
用户点击"新建会话"
    ↓
sessionStore.createSession(agentId)
    ↓
invoke('create_session', { agentId })
    ↓
返回新 Session 对象
    ↓
添加到 sessions 列表
    ↓
switchSession(newSession.id)
    ↓
chatStore.clearMessages()
    ↓
用户开始新对话
```

### 需要新增/修改的后端代码

**修改 MessageStore 添加分页支持:**
```rust
// crates/omninova-core/src/session/store.rs
impl MessageStore {
    /// 分页查询消息（按时间升序）
    pub fn find_paginated_by_session(
        &self,
        session_id: i64,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<Message>, SessionError> {
        let conn = self.pool.get().map_err(|e| SessionError::Pool(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, created_at
             FROM messages WHERE session_id = ?1
             ORDER BY created_at ASC
             LIMIT ?2 OFFSET ?3"
        )?;

        // ... query implementation
    }
}
```

### 文件创建清单

- `apps/omninova-tauri/src/stores/sessionStore.ts` - 会话状态管理
- `apps/omninova-tauri/src/components/Chat/SessionList.tsx` - 会话列表组件
- `apps/omninova-tauri/src/components/Chat/SessionItem.tsx` - 会话项组件
- `apps/omninova-tauri/src/hooks/usePaginatedMessages.ts` - 分页加载 Hook
- `apps/omninova-tauri/src/components/Chat/__tests__/SessionList.test.tsx` - 测试
- `apps/omninova-tauri/src/components/Chat/__tests__/SessionItem.test.tsx` - 测试

### 文件修改清单

- `apps/omninova-tauri/src/components/Chat/ChatInterface.tsx` - 集成 session 切换
- `apps/omninova-tauri/src/components/Chat/MessageList.tsx` - 添加加载更多触发
- `apps/omninova-tauri/src/components/Chat/index.ts` - 导出新组件
- `apps/omninova-tauri/src-tauri/src/lib.rs` - 添加分页 Tauri 命令
- `crates/omninova-core/src/session/store.rs` - 添加分页查询方法

### 测试标准

1. **单元测试** - 使用 vitest + React Testing Library
2. **测试文件位置** - `src/components/Chat/__tests__/`
3. **测试模式**:
   - SessionStore actions 和 state 状态测试
   - SessionList 渲染和交互测试
   - 分页加载逻辑测试
   - 会话切换流程测试

### 上一个 Story 学习 (4-6-message-input)

- 使用 `memo` 优化组件重渲染
- 使用 `cn()` 工具函数合并类名
- 遵循 JSDoc 注释规范
- 测试文件与源文件目录分离 (`__tests__/`)
- 测试覆盖多种场景：正常、边界、错误状态

### 可访问性要求

1. **SessionList**: role="listbox", aria-label="会话列表"
2. **SessionItem**: role="option", aria-selected
3. **新建会话按钮**: aria-label="创建新会话"
4. **加载更多按钮**: aria-label="加载更多历史消息"
5. **焦点管理**: 会话切换后焦点正确转移

## References

- [Source: _bmad-output/planning-artifacts/epics.md#L851-L865] - Story 4.7 requirements
- [Source: crates/omninova-core/src/session/store.rs] - 后端 SessionStore 实现
- [Source: apps/omninova-tauri/src/stores/chatStore.ts] - 现有 chat store
- [Source: apps/omninova-tauri/src/components/Chat/ChatInterface.tsx] - 主聊天组件
- [Source: apps/omninova-tauri/src/components/Chat/Chat.tsx] - legacy 会话列表 UI 模式
- [Source: apps/omninova-tauri/src/types/session.ts] - TypeScript 类型定义
- [Source: _bmad-output/implementation-artifacts/4-1-session-message-model.md] - 数据模型 Story
- [Source: _bmad-output/implementation-artifacts/4-6-message-input.md] - 前序 Story

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

N/A

### Completion Notes List

Story file created with comprehensive context including:
- Existing backend SessionStore and MessageStore APIs
- Frontend sessionStore design pattern
- Session list UI pattern from legacy Chat.tsx
- Pagination implementation strategy
- Session switching and creation flows
- Accessibility requirements

### File List

**To Create:**
- `apps/omninova-tauri/src/stores/sessionStore.ts`
- `apps/omninova-tauri/src/components/Chat/SessionList.tsx`
- `apps/omninova-tauri/src/components/Chat/SessionItem.tsx`
- `apps/omninova-tauri/src/hooks/usePaginatedMessages.ts`
- `apps/omninova-tauri/src/components/Chat/__tests__/SessionList.test.tsx`
- `apps/omninova-tauri/src/components/Chat/__tests__/SessionItem.test.tsx`

**To Modify:**
- `apps/omninova-tauri/src/components/Chat/ChatInterface.tsx`
- `apps/omninova-tauri/src/components/Chat/MessageList.tsx`
- `apps/omninova-tauri/src/components/Chat/index.ts`
- `apps/omninova-tauri/src-tauri/src/lib.rs`
- `crates/omninova-core/src/session/store.rs`