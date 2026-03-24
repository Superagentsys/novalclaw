# Story 4.5: 打字指示器与加载状态

Status: done

## Story

As a 用户,
I want 看到 AI 正在生成响应的指示,
so that 我知道系统正在工作而没有卡住.

## Acceptance Criteria

1. **AC1: 打字指示器动画** - AI 正在生成响应时显示打字指示器动画（三个点或动画波浪） ✅
2. **AC2: 人格主题匹配** - 打字指示器样式与代理人格类型匹配 ✅
3. **AC3: 发送按钮加载状态** - 发送按钮显示加载状态 ✅
4. **AC4: 骨架屏加载** - 首次加载历史消息时显示骨架屏 ✅
5. **AC5: 平滑过渡动画** - 加载状态有平滑的过渡动画 ✅

## Tasks / Subtasks

- [x] Task 1: TypingIndicator 组件 (AC: #1, #2)
  - [x] 1.1 Create `apps/omninova-tauri/src/components/Chat/TypingIndicator.tsx`
  - [x] 1.2 Implement animated dots with CSS keyframes
  - [x] 1.3 Add personality color theming via `getPersonalityColors()`
  - [x] 1.4 Support multiple animation styles (dots, wave, pulse)
  - [x] 1.5 Add JSDoc documentation and TypeScript types

- [x] Task 2: LoadingButton 组件 (AC: #3)
  - [x] 2.1 Create `apps/omninova-tauri/src/components/ui/loading-button.tsx`
  - [x] 2.2 Extend shadcn Button with loading prop
  - [x] 2.3 Add spinner animation during loading
  - [x] 2.4 Disable button interaction during loading
  - [x] 2.5 Support loading text override

- [x] Task 3: MessageSkeleton 组件 (AC: #4)
  - [x] 3.1 Create `apps/omninova-tauri/src/components/Chat/MessageSkeleton.tsx`
  - [x] 3.2 Use existing Skeleton component from shadcn/ui
  - [x] 3.3 Create message-shaped skeleton layout
  - [x] 3.4 Support multiple skeleton variants (user, assistant)
  - [x] 3.5 Add staggered animation for multiple skeletons

- [x] Task 4: 集成与过渡动画 (AC: #5)
  - [x] 4.1 Update ChatInterface to use TypingIndicator component
  - [x] 4.2 Replace inline typing indicator in MessageList.tsx with TypingIndicator
  - [x] 4.3 Add loading state to ChatInterface for initial load
  - [x] 4.4 Implement fade-in/fade-out transitions for loading states
  - [x] 4.5 Add reduced-motion support for accessibility

- [x] Task 5: 单元测试 (All ACs)
  - [x] 5.1 Test TypingIndicator with different personalities
  - [x] 5.2 Test LoadingButton states (normal, loading, disabled)
  - [x] 5.3 Test MessageSkeleton rendering and animation
  - [x] 5.4 Test integration in ChatInterface
  - [x] 5.5 Test accessibility (aria-busy, aria-live)

## Dev Notes

### 现有实现分析

**已有 TypingIndicator (StreamingMessage.tsx:49-57):**
```typescript
function TypingIndicator() {
  return (
    <div className="flex items-center gap-1 py-2" aria-label="正在输入">
      <span className="w-2 h-2 bg-muted-foreground rounded-full animate-bounce [animation-delay:0ms]" />
      <span className="w-2 h-2 bg-muted-foreground rounded-full animate-bounce [animation-delay:150ms]" />
      <span className="w-2 h-2 bg-muted-foreground rounded-full animate-bounce [animation-delay:300ms]" />
    </div>
  );
}
```

**已有内联打字指示器 (MessageList.tsx:169-178):**
```typescript
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
```

**需要改进:**
1. 提取为独立组件便于复用
2. 添加人格颜色主题支持
3. 提供多种动画样式选择

### 组件架构

```
Chat/
├── ChatInterface.tsx     # 主容器（需更新）
├── MessageList.tsx       # 消息列表（需更新内联指示器）
├── StreamingMessage.tsx  # 流式消息（已有 TypingIndicator，可复用）
├── TypingIndicator.tsx   # 新建：独立打字指示器
└── MessageSkeleton.tsx   # 新建：消息骨架屏

ui/
├── skeleton.tsx          # 已有：shadcn Skeleton
└── loading-button.tsx    # 新建：加载按钮
```

### Personality Color 集成

从 `src/lib/personality-colors.ts`:
```typescript
const colors = getPersonalityColors('INTJ');
// colors.primary: '#2563EB'
// colors.tone: 'analytical' | 'creative' | 'structured' | 'energetic'
```

**动画风格映射:**
| tone | 动画风格 |
|------|----------|
| analytical | 均匀节奏，稳定跳动 |
| creative | 波浪动画，流畅变化 |
| structured | 同步脉冲，规则有序 |
| energetic | 快速弹跳，活力四射 |

### 骨架屏模式

基于已有 `Skeleton` 组件:
```typescript
import { Skeleton } from '@/components/ui/skeleton';

function MessageSkeleton({ role }: { role: 'user' | 'assistant' }) {
  return (
    <div className={cn('flex flex-col gap-2', role === 'user' && 'items-end')}>
      {role === 'assistant' && <Skeleton className="h-4 w-16" />}
      <Skeleton className="h-16 w-[70%] rounded-lg" />
      <Skeleton className="h-3 w-12" />
    </div>
  );
}
```

### CSS 动画实现

**Tailwind animate-bounce 已定义:**
```css
@keyframes bounce {
  0%, 100% { transform: translateY(-25%); animation-timing-function: cubic-bezier(0.8, 0, 1, 1); }
  50% { transform: translateY(0); animation-timing-function: cubic-bezier(0, 0, 0.2, 1); }
}
```

**自定义波浪动画 (tailwind.config.ts):**
```typescript
// 可添加到 tailwind.config.ts 的 extend.animation
'wave': 'wave 1.2s ease-in-out infinite',
// keyframes
'wave': {
  '0%, 60%, 100%': { transform: 'translateY(0)' },
  '30%': { transform: 'translateY(-4px)' },
}
```

### 可访问性要求

1. **aria-busy**: 用于标记加载状态
2. **aria-live**: 用于宣布状态变化
3. **prefers-reduced-motion**: 尊重用户减少动画偏好
```typescript
// 检测用户偏好
const prefersReducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
```

### 文件创建清单

- `apps/omninova-tauri/src/components/Chat/TypingIndicator.tsx` - 打字指示器组件
- `apps/omninova-tauri/src/components/Chat/MessageSkeleton.tsx` - 消息骨架屏
- `apps/omninova-tauri/src/components/ui/loading-button.tsx` - 加载按钮

### 文件修改清单

- `apps/omninova-tauri/src/components/Chat/ChatInterface.tsx` - 集成加载状态
- `apps/omninova-tauri/src/components/Chat/MessageList.tsx` - 使用新 TypingIndicator
- `apps/omninova-tauri/src/components/Chat/index.ts` - 导出新组件
- `apps/omninova-tauri/src/components/Chat/StreamingMessage.tsx` - 可选：复用新 TypingIndicator

### 测试标准

1. **单元测试** - 使用 vitest + React Testing Library
2. **测试文件位置** - `src/components/Chat/__tests__/`
3. **测试模式**:
   - 渲染组件验证
   - 人格颜色主题测试
   - 加载状态切换测试
   - 可访问性属性验证

### 上一个 Story 学习 (4-4-chat-interface-basic)

- 使用 `memo` 优化组件重渲染
- 使用 `cn()` 工具函数合并类名
- 遵循 JSDoc 注释规范
- 测试文件与源文件目录分离 (`__tests__/`)
- 使用 `eslint-disable` 注释保留未使用但预留给未来的 props

## References

- [Source: _bmad-output/planning-artifacts/epics.md#L818-L833] - Story 4.5 requirements
- [Source: apps/omninova-tauri/src/components/Chat/StreamingMessage.tsx] - 现有 TypingIndicator
- [Source: apps/omninova-tauri/src/components/Chat/MessageList.tsx] - 内联打字指示器
- [Source: apps/omninova-tauri/src/lib/personality-colors.ts] - 人格颜色系统
- [Source: apps/omninova-tauri/src/components/ui/skeleton.tsx] - shadcn Skeleton 组件
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md] - UX 设计规范
- [Source: _bmad-output/implementation-artifacts/4-4-chat-interface-basic.md] - 前序 Story

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

N/A

### Completion Notes List

- All 5 acceptance criteria implemented and verified
- 55 new unit tests added (TypingIndicator: 20, LoadingButton: 21, MessageSkeleton: 14)
- All 533 tests passing
- Wave animation added to tailwind.config.ts
- Reduced motion accessibility support implemented
- MessageSkeleton uses deterministic line widths to avoid impure render

### File List

- `apps/omninova-tauri/src/components/Chat/TypingIndicator.tsx` - Created
- `apps/omninova-tauri/src/components/Chat/MessageSkeleton.tsx` - Created
- `apps/omninova-tauri/src/components/ui/loading-button.tsx` - Created
- `apps/omninova-tauri/src/components/Chat/__tests__/TypingIndicator.test.tsx` - Created
- `apps/omninova-tauri/src/components/Chat/__tests__/MessageSkeleton.test.tsx` - Created
- `apps/omninova-tauri/src/components/ui/__tests__/loading-button.test.tsx` - Created
- `apps/omninova-tauri/src/components/Chat/index.ts` - Updated exports
- `apps/omninova-tauri/src/components/Chat/MessageList.tsx` - Integrated TypingIndicator
- `apps/omninova-tauri/src/components/Chat/StreamingMessage.tsx` - Integrated TypingIndicator
- `apps/omninova-tauri/src/components/Chat/ChatInterface.tsx` - Integrated MessageSkeleton
- `apps/omninova-tauri/tailwind.config.ts` - Added wave animation