# Story 4.6: 消息输入与发送功能

Status: done

## Story

As a 用户,
I want 输入和发送消息给 AI 代理,
so that 我可以与代理进行对话交互.

## Acceptance Criteria

1. **AC1: 多行文本输入** - 输入框支持多行文本，自动调整高度（最小1行，最大6行）
2. **AC2: 发送快捷键** - 按 Enter 发送消息，Shift+Enter 换行
3. **AC3: 输入框清空** - 发送时清空输入框
4. **AC4: 发送按钮禁用** - 发送按钮在输入为空或正在流式响应时禁用
5. **AC5: 焦点管理** - 发送后输入框重新获得焦点
6. **AC6: 粘贴支持** - 支持 Ctrl+V 粘贴文本

## Tasks / Subtasks

- [x] Task 1: ChatInput 组件 (AC: #1, #2, #3, #5, #6)
  - [x] 1.1 Create `apps/omninova-tauri/src/components/Chat/ChatInput.tsx`
  - [x] 1.2 Implement auto-expanding textarea (1-6 rows)
  - [x] 1.3 Handle Enter to send, Shift+Enter for newline
  - [x] 1.4 Clear input on send
  - [x] 1.5 Auto-focus after send
  - [x] 1.6 Support paste with Ctrl+V
  - [x] 1.7 Add JSDoc documentation and TypeScript types

- [x] Task 2: 发送/取消按钮 (AC: #4)
  - [x] 2.1 Use LoadingButton for send button
  - [x] 2.2 Show cancel button during streaming
  - [x] 2.3 Disable send when input is empty or streaming
  - [x] 2.4 Apply personality colors to buttons

- [x] Task 3: 集成到 ChatInterface (All ACs)
  - [x] 3.1 Add ChatInput to ChatInterface component
  - [x] 3.2 Wire up onSendMessage callback
  - [x] 3.3 Connect to chatStore for isStreaming state
  - [x] 3.4 Update ChatInterface exports

- [x] Task 4: 单元测试 (All ACs)
  - [x] 4.1 Test textarea auto-expansion
  - [x] 4.2 Test Enter to send, Shift+Enter for newline
  - [x] 4.3 Test send button disabled states
  - [x] 4.4 Test focus management
  - [x] 4.5 Test paste functionality

## Dev Notes

### 现有实现分析

**Chat.tsx 中的输入实现 (377-408):**
```typescript
<textarea
  className="chat-input"
  value={input}
  onChange={(e) => setInput(e.target.value)}
  onKeyDown={handleKeyDown}
  placeholder={gatewayStatus === "connected" ? "输入消息..." : "网关未连接..."}
  rows={1}
  disabled={sending || gatewayStatus !== "connected"}
/>
```

**需要改进:**
1. 自动调整高度（目前固定 rows={1}）
2. 提取为独立组件便于复用
3. 使用 LoadingButton 组件
4. 添加人格颜色主题

### 组件架构

```
Chat/
├── ChatInterface.tsx     # 主容器（需更新）
├── ChatInput.tsx         # 新建：消息输入组件
├── MessageList.tsx       # 消息列表
├── StreamingMessage.tsx  # 流式消息
├── TypingIndicator.tsx   # 打字指示器
└── MessageSkeleton.tsx   # 骨架屏

ui/
├── loading-button.tsx    # 已有：加载按钮
└── textarea.tsx          # 已有：shadcn Textarea（可能需要扩展）
```

### 自动调整高度实现

```typescript
const textareaRef = useRef<HTMLTextAreaElement>(null);
const [textareaHeight, setTextareaHeight] = useState('auto');

const adjustHeight = useCallback(() => {
  const textarea = textareaRef.current;
  if (textarea) {
    textarea.style.height = 'auto';
    const scrollHeight = textarea.scrollHeight;
    const maxHeight = 6 * 24; // 6 rows * ~24px line height
    textarea.style.height = `${Math.min(scrollHeight, maxHeight)}px`;
  }
}, []);

useEffect(() => {
  adjustHeight();
}, [value, adjustHeight]);
```

### 键盘事件处理

```typescript
const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    if (value.trim() && !isStreaming && !disabled) {
      onSend(value.trim());
    }
  }
  // Shift+Enter 默认行为是换行，无需特殊处理
};
```

### 焦点管理

```typescript
const textareaRef = useRef<HTMLTextAreaElement>(null);

// 发送后重新聚焦
const handleSend = () => {
  onSend(value.trim());
  setValue('');
  // 使用 setTimeout 确保状态更新后再聚焦
  setTimeout(() => {
    textareaRef.current?.focus();
  }, 0);
};

// 组件挂载时聚焦
useEffect(() => {
  textareaRef.current?.focus();
}, []);
```

### 人格颜色集成

从 `src/lib/personality-colors.ts`:
```typescript
const colors = getPersonalityColors('INTJ');
// colors.primary: '#2563EB'
```

用于发送按钮主题:
```typescript
<Button style={{ backgroundColor: colors.primary }}>
  发送
</Button>
```

### 可访问性要求

1. **aria-label**: 输入框的描述
2. **aria-disabled**: 禁用状态
3. **placeholder**: 提示文字
4. **focus-visible**: 焦点可见样式

### 文件创建清单

- `apps/omninova-tauri/src/components/Chat/ChatInput.tsx` - 消息输入组件

### 文件修改清单

- `apps/omninova-tauri/src/components/Chat/ChatInterface.tsx` - 集成 ChatInput
- `apps/omninova-tauri/src/components/Chat/index.ts` - 导出新组件

### 测试标准

1. **单元测试** - 使用 vitest + React Testing Library
2. **测试文件位置** - `src/components/Chat/__tests__/`
3. **测试模式**:
   - 组件渲染验证
   - 键盘事件处理测试
   - 焦点管理测试
   - 粘贴功能测试

### 上一个 Story 学习 (4-5-typing-indicator)

- 使用 `memo` 优化组件重渲染
- 使用 `cn()` 工具函数合并类名
- 遵循 JSDoc 注释规范
- 测试文件与源文件目录分离 (`__tests__/`)
- 使用 `eslint-disable` 注释保留未使用但预留给未来的 props

## References

- [Source: _bmad-output/planning-artifacts/epics.md#L834-L849] - Story 4.6 requirements
- [Source: apps/omninova-tauri/src/components/Chat/Chat.tsx] - 现有输入实现
- [Source: apps/omninova-tauri/src/components/Chat/ChatInterface.tsx] - 主容器组件
- [Source: apps/omninova-tauri/src/components/ui/loading-button.tsx] - LoadingButton 组件
- [Source: apps/omninova-tauri/src/lib/personality-colors.ts] - 人格颜色系统
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md] - UX 设计规范
- [Source: _bmad-output/implementation-artifacts/4-5-typing-indicator.md] - 前序 Story

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

N/A

### Completion Notes List

- All 6 acceptance criteria implemented and verified
- 32 new unit tests added for ChatInput component
- All 565 tests passing
- Auto-expanding textarea with min/max rows (default 1-6)
- Enter to send, Shift+Enter for newline
- Focus management on mount and after send
- Personality color theming for send button
- Cancel button shown during streaming
- Controlled and uncontrolled modes supported

### File List

- `apps/omninova-tauri/src/components/Chat/ChatInput.tsx` - Created
- `apps/omninova-tauri/src/components/Chat/__tests__/ChatInput.test.tsx` - Created
- `apps/omninova-tauri/src/components/Chat/ChatInterface.tsx` - Updated (integrated ChatInput)
- `apps/omninova-tauri/src/components/Chat/index.ts` - Updated exports