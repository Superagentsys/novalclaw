# Story 5.6: MemoryLayerIndicator 组件

Status: done

## Story

As a 用户,
I want 看到当前记忆系统的状态指示,
So that 我可以了解 AI 代理正在使用哪些记忆层级.

## Acceptance Criteria

1. **AC1: 三层状态指示器** - 显示三层记忆 (L1/L2/L3) 的状态指示器
2. **AC2: 容量使用情况** - 指示器显示每层的容量使用情况 (L1: used/capacity, L2/L3: count)
3. **AC3: 活动层高亮** - 当前活动的记忆层有高亮显示
4. **AC4: 检索动画** - 记忆检索时显示动画指示

## Tasks / Subtasks

- [x] Task 1: 实现 MemoryLayerIndicator 组件 (AC: #1, #2, #3)
  - [x] 1.1 创建 `MemoryLayerIndicator.tsx` 组件文件
  - [x] 1.2 定义组件 Props 接口 (stats, activeLayer, isRetrieving)
  - [x] 1.3 实现 L1 层指示器 (进度条显示 used/capacity)
  - [x] 1.4 实现 L2 层指示器 (显示 count 和 avg importance)
  - [x] 1.5 实现 L3 层指示器 (显示 count 和 available 状态)
  - [x] 1.6 实现活动层高亮样式 (边框/背景色变化)
  - [x] 1.7 添加 hover tooltip 显示详细统计

- [x] Task 2: 实现检索动画效果 (AC: #4)
  - [x] 2.1 定义 `isRetrieving` 状态 prop
  - [x] 2.2 使用 CSS animation 实现脉冲/呼吸动画
  - [x] 2.3 活动层显示检索中动画
  - [x] 2.4 考虑使用 Tailwind animate-pulse 或自定义 keyframes

- [x] Task 3: 集成到 ChatInterface (AC: #1)
  - [x] 3.1 在 `ChatInterface.tsx` 中导入 MemoryLayerIndicator
  - [x] 3.2 使用 `useMemoryStats` hook 获取统计数据
  - [x] 3.3 在 ChatHeader 中放置组件 (右侧或底部)
  - [x] 3.4 传递 isStreaming 状态作为 isRetrieving

- [x] Task 4: 创建 useMemoryStats Hook (AC: #2)
  - [x] 4.1 创建 `hooks/useMemoryStats.ts`
  - [x] 4.2 定期轮询 `getMemoryManagerStats()` (间隔 5s)
  - [x] 4.3 返回 stats, isLoading, error 状态
  - [x] 4.4 支持 refresh 手动刷新

- [x] Task 5: 单元测试 (All ACs)
  - [x] 5.1 编写 MemoryLayerIndicator 组件测试
  - [x] 5.2 测试各层状态显示正确性
  - [x] 5.3 测试活动层高亮逻辑
  - [x] 5.4 测试检索动画状态
  - [x] 5.5 编写 useMemoryStats hook 测试

## Dev Notes

### 架构上下文

三层记忆系统已在 Story 5.1-5.5 中实现:
- **L1 (WorkingMemory)**: 内存 LRU 缓存，容量限制
- **L2 (EpisodicMemoryStore)**: SQLite 持久化，长期记忆
- **L3 (SemanticMemoryStore)**: 向量索引，语义搜索

### 现有 API

**MemoryManagerStats** (from `memory.ts`):
```typescript
interface MemoryManagerStats {
  l1Capacity: number;      // L1 最大容量
  l1Used: number;          // L1 已用槽位
  l1SessionId: number | null;  // 当前会话 ID
  l2Total: number;         // L2 总记忆数
  l2AvgImportance: number; // L2 平均重要性
  l3Total: number;         // L3 索引记忆数
}
```

**Tauri Command**:
```typescript
getMemoryManagerStats(): Promise<MemoryManagerStats>
```

### UI 设计参考

组件应紧凑、非侵入式，适合放置在聊天界面顶部或底部:

```
┌─────────────────────────────────────────┐
│ L1 [████████░░] 8/10  L2: 156  L3: 42  │  <- 正常状态
└─────────────────────────────────────────┘

┌─────────────────────────────────────────┐
│ L1 [████████░░] 8/10  L2: 156  L3: 42  │  <- L2 检索中 (L2 高亮+动画)
│     ↑ 正在检索...                        │
└─────────────────────────────────────────┘
```

**颜色建议**:
- L1: 蓝色系 (`bg-blue-500`, `text-blue-600`)
- L2: 绿色系 (`bg-green-500`, `text-green-600`)
- L3: 紫色系 (`bg-purple-500`, `text-purple-600`)
- 活动层: 添加 `ring-2 ring-offset-1` 或背景高亮
- 检索中: `animate-pulse` 或自定义呼吸动画

### 文件结构

```
apps/omninova-tauri/src/
├── components/
│   └── Chat/
│       ├── ChatInterface.tsx    # 集成 MemoryLayerIndicator
│       └── MemoryLayerIndicator.tsx  # 新增
├── hooks/
│   └── useMemoryStats.ts        # 新增
└── types/
    └── memory.ts                # 已有 MemoryManagerStats
```

### 组件 Props 设计

```typescript
interface MemoryLayerIndicatorProps {
  /** 记忆统计数据 */
  stats?: MemoryManagerStats;
  /** 当前活动的记忆层 */
  activeLayer?: 'L1' | 'L2' | 'L3' | null;
  /** 是否正在检索 */
  isRetrieving?: boolean;
  /** 紧凑模式 (仅显示图标) */
  compact?: boolean;
  /** 额外样式类 */
  className?: string;
}
```

### Hook 设计

```typescript
interface UseMemoryStatsOptions {
  /** 轮询间隔 (毫秒), 默认 5000 */
  interval?: number;
  /** 是否自动刷新, 默认 true */
  autoRefresh?: boolean;
}

interface UseMemoryStatsReturn {
  stats: MemoryManagerStats | null;
  isLoading: boolean;
  error: Error | null;
  refresh: () => Promise<void>;
}

function useMemoryStats(options?: UseMemoryStatsOptions): UseMemoryStatsReturn;
```

### 与 ChatInterface 集成

在 `ChatInterface.tsx` 的 `ChatHeader` 组件中添加:

```tsx
// 在 ChatHeader 内部
const [stats, setStats] = useState<MemoryManagerStats | null>(null);

useEffect(() => {
  // 轮询获取记忆统计
  const fetchStats = async () => {
    try {
      const s = await getMemoryManagerStats();
      setStats(s);
    } catch (e) {
      console.error('Failed to fetch memory stats:', e);
    }
  };

  fetchStats();
  const interval = setInterval(fetchStats, 5000);
  return () => clearInterval(interval);
}, []);

// 在 header 右侧渲染
<MemoryLayerIndicator
  stats={stats}
  isRetrieving={isStreaming}
  activeLayer={isStreaming ? 'L1' : null}
/>
```

### References

- [Source: architecture.md#三层记忆系统架构] - 记忆系统架构设计
- [Source: epics.md#Story 5.6] - 原始 story 定义
- [Source: Story 5.4 implementation] - MemoryManager 和 MemoryManagerStats
- [Source: Story 5.5 implementation] - PerformanceStats (可选用于显示延迟)
- [Source: ChatInterface.tsx] - 集成位置参考
- [Source: memory.ts] - 已有类型定义

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

N/A

### Completion Notes List

Story 5.6 implementation completed:

1. **useMemoryStats Hook** (`hooks/useMemoryStats.ts`):
   - Periodic polling with configurable interval (default 5000ms)
   - Returns stats, isLoading, error, and refresh function
   - Uses refs to avoid unnecessary re-renders and stale closures

2. **MemoryLayerIndicator Component** (`components/Chat/MemoryLayerIndicator.tsx`):
   - L1 indicator with progress bar showing used/capacity
   - L2 indicator showing total count and average importance
   - L3 indicator showing total count and availability status
   - Active layer highlighting with background color changes
   - Retrieval animation using `animate-pulse`
   - Hover tooltips for detailed statistics
   - Compact mode for minimal display

3. **ChatInterface Integration**:
   - Added useMemoryStats hook to fetch memory statistics
   - Integrated MemoryLayerIndicator into ChatHeader
   - Passes isStreaming state for retrieval animation

4. **Unit Tests**:
   - 16 tests for MemoryLayerIndicator component
   - 6 tests for useMemoryStats hook
   - Total: 22 new tests passing
   - All 815 frontend tests passing
   - Build compiles successfully

**Files Created:**
- `apps/omninova-tauri/src/hooks/useMemoryStats.ts`
- `apps/omninova-tauri/src/components/Chat/MemoryLayerIndicator.tsx`
- `apps/omninova-tauri/src/hooks/__tests__/useMemoryStats.test.ts`
- `apps/omninova-tauri/src/components/Chat/__tests__/MemoryLayerIndicator.test.tsx`

**Files Modified:**
- `apps/omninova-tauri/src/components/Chat/ChatInterface.tsx`

### File List

**Created:**
- `apps/omninova-tauri/src/hooks/useMemoryStats.ts`
- `apps/omninova-tauri/src/components/Chat/MemoryLayerIndicator.tsx`
- `apps/omninova-tauri/src/hooks/__tests__/useMemoryStats.test.ts`
- `apps/omninova-tauri/src/components/Chat/__tests__/MemoryLayerIndicator.test.tsx`

**Modified:**
- `apps/omninova-tauri/src/components/Chat/ChatInterface.tsx`

## Change Log

| Date | Change |
|------|--------|
| 2026-03-21 | Story 5.6 context created - ready for implementation |
| 2026-03-21 | Story 5.6 implementation completed - useMemoryStats hook, MemoryLayerIndicator component, ChatInterface integration, unit tests |
| 2026-03-21 | Code review passed - fixed unused import in MemoryLayerIndicator.tsx |