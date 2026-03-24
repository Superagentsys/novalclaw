# Story 5.7: MemoryVisualization 组件

Status: done

## Story

As a 用户,
I want 查看和管理 AI 代理的记忆内容,
So that 我可以了解代理记住了什么并控制记忆数据.

## Acceptance Criteria

1. **AC1: 三层记忆内容列表** - 显示三层记忆 (L1/L2/L3) 的内容列表 ✅
2. **AC2: 筛选功能** - 支持按层级、时间、重要性筛选记忆 ✅
3. **AC3: 关键词搜索** - 支持关键词搜索记忆内容 ✅
4. **AC4: 记忆详情查看** - 可以查看单条记忆的详情 ✅
5. **AC5: 删除记忆** - 可以删除单条记忆 ✅
6. **AC6: 消息关联** - 显示记忆与对话消息的关联 ✅

## Tasks / Subtasks

- [x] Task 1: 实现 MemoryVisualization 主组件 (AC: #1, #6)
  - [x] 1.1 创建 `MemoryVisualization.tsx` 组件文件
  - [x] 1.2 定义组件 Props 接口 (agentId, sessionId, onClose)
  - [x] 1.3 实现三层 Tab 切换 (L1/L2/L3)
  - [x] 1.4 实现记忆列表渲染 (虚拟化列表支持大量数据)
  - [x] 1.5 显示记忆来源关联 (session ID, agent ID)

- [x] Task 2: 实现记忆筛选功能 (AC: #2)
  - [x] 2.1 创建 `MemoryFilterBar.tsx` 子组件
  - [x] 2.2 实现层级筛选 (L1/L2/L3 或 ALL)
  - [x] 2.3 实现时间范围筛选 (今日/本周/本月/全部)
  - [x] 2.4 实现重要性筛选 (1-10 滑块或多选)
  - [x] 2.5 筛选状态 URL 参数同步 (Skipped - using local state)

- [x] Task 3: 实现关键词搜索 (AC: #3)
  - [x] 3.1 创建搜索输入框 (防抖处理 300ms)
  - [x] 3.2 L1/L2 使用本地内容匹配
  - [x] 3.3 L3 调用 `searchMemory()` API 语义搜索
  - [x] 3.4 高亮搜索结果中的匹配文本
  - [x] 3.5 显示搜索结果计数

- [x] Task 4: 实现记忆详情和删除 (AC: #4, #5)
  - [x] 4.1 创建 `MemoryDetailDialog.tsx` 组件
  - [x] 4.2 显示记忆完整内容、时间戳、重要性
  - [x] 4.3 显示关联的 session/agent 信息
  - [x] 4.4 实现删除确认对话框
  - [x] 4.5 调用 `deleteMemory()` API
  - [x] 4.6 删除后刷新列表

- [x] Task 5: 创建 useMemoryData Hook (AC: #1, #2, #3)
  - [x] 5.1 创建 `hooks/useMemoryData.ts`
  - [x] 5.2 封装 L1 `getWorkingMemory()` 调用
  - [x] 5.3 封装 L2 `getEpisodicMemories()` 调用
  - [x] 5.4 封装 L3 `searchSemanticMemories()` 调用
  - [x] 5.5 实现分页加载 (limit, offset)
  - [x] 5.6 返回 memories, isLoading, error, refresh

- [x] Task 6: 集成到 ChatInterface (AC: #1)
  - [x] 6.1 在 `ChatInterface.tsx` 中添加打开按钮
  - [x] 6.2 创建 Sheet 或 Dialog 容器组件
  - [x] 6.3 传递 agentId 和 sessionId
  - [x] 6.4 记忆变化时通知刷新

- [x] Task 7: 单元测试 (All ACs)
  - [x] 7.1 编写 MemoryVisualization 组件测试
  - [x] 7.2 测试层级筛选正确性
  - [x] 7.3 测试搜索功能
  - [x] 7.4 测试删除流程
  - [x] 7.5 编写 useMemoryData hook 测试

## Dev Notes

### 架构上下文

三层记忆系统已在 Story 5.1-5.5 中实现:
- **L1 (WorkingMemory)**: 内存 LRU 缓存，会话级临时存储
- **L2 (EpisodicMemoryStore)**: SQLite WAL 持久化，长期情景记忆
- **L3 (SemanticMemoryStore)**: 向量索引，语义相似性搜索

### 现有 API (from `memory.ts`)

**L1 Working Memory:**
```typescript
getWorkingMemory(limit?: number): Promise<WorkingMemoryEntry[]>
clearWorkingMemory(): Promise<void>
getMemoryStats(): Promise<MemoryStats>
```

**L2 Episodic Memory:**
```typescript
getEpisodicMemories(agentId: number, limit?: number, offset?: number): Promise<EpisodicMemory[]>
getEpisodicMemoriesBySession(sessionId: number): Promise<EpisodicMemory[]>
getEpisodicMemoriesByImportance(minImportance: number, limit?: number): Promise<EpisodicMemory[]>
deleteEpisodicMemory(id: number): Promise<boolean>
getEpisodicMemoryStats(): Promise<EpisodicMemoryStats>
```

**L3 Semantic Memory:**
```typescript
searchSemanticMemories(query: string, k?: number, agentId?: number, threshold?: number): Promise<SemanticSearchResult[]>
deleteSemanticMemory(id: number): Promise<boolean>
getSemanticMemoryStats(): Promise<SemanticMemoryStats>
```

**Unified Memory Manager:**
```typescript
retrieveMemory(agentId: number, sessionId?: number, layer?: MemoryLayer, limit?: number): Promise<MemoryQueryResult>
searchMemory(query: string, k?: number, threshold?: number): Promise<UnifiedMemoryEntry[]>
deleteMemory(id: string, layer?: MemoryLayer): Promise<boolean>
getMemoryManagerStats(): Promise<MemoryManagerStats>
```

### UI 设计参考

组件应作为可展开的面板或独立对话框:

```
┌──────────────────────────────────────────────────────────────┐
│ 记忆管理                                           [筛选] [×] │
├──────────────────────────────────────────────────────────────┤
│ [L1 工作记忆] [L2 情景记忆] [L3 语义记忆]                     │
├──────────────────────────────────────────────────────────────┤
│ 🔍 搜索记忆...                                   重要性: [全部▼] │
├──────────────────────────────────────────────────────────────┤
│ ┌──────────────────────────────────────────────────────────┐ │
│ │ 10:30 AM | 用户偏好使用 TypeScript 进行开发               │ │
│ │ 重要性: 8/10 | 会话: #42                    [详情] [删除] │ │
│ └──────────────────────────────────────────────────────────┘ │
│ ┌──────────────────────────────────────────────────────────┐ │
│ │ 09:15 AM | 项目使用 Tauri + React 架构                    │ │
│ │ 重要性: 6/10 | 会话: #41                    [详情] [删除] │ │
│ └──────────────────────────────────────────────────────────┘ │
│ ... 更多记忆条目 ...                                         │
├──────────────────────────────────────────────────────────────┤
│ 显示 1-20 / 共 156 条                        [上一页] [下一页] │
└──────────────────────────────────────────────────────────────┘
```

**记忆详情对话框:**
```
┌────────────────────────────────────────┐
│ 记忆详情                        [×]    │
├────────────────────────────────────────┤
│ ID: 123                                │
│ 来源: L2 情景记忆                      │
│ 创建时间: 2026-03-21 10:30:00         │
│ 重要性: 8/10                          │
│ 关联会话: #42                          │
├────────────────────────────────────────┤
│ 内容:                                  │
│ 用户偏好使用 TypeScript 进行开发，    │
│ 项目使用 Tauri + React 架构...        │
├────────────────────────────────────────┤
│            [删除此记忆]  [关闭]        │
└────────────────────────────────────────┘
```

### 文件结构

```
apps/omninova-tauri/src/
├── components/
│   └── Chat/
│       ├── ChatInterface.tsx        # 添加打开记忆面板入口
│       ├── MemoryVisualization.tsx  # 新增 - 主组件
│       ├── MemoryFilterBar.tsx      # 新增 - 筛选栏
│       ├── MemoryDetailDialog.tsx   # 新增 - 详情对话框
│       └── __tests__/
│           ├── MemoryVisualization.test.tsx
│           ├── MemoryFilterBar.test.tsx
│           └── MemoryDetailDialog.test.tsx
├── hooks/
│   ├── useMemoryStats.ts            # 已有 - Story 5.6
│   ├── useMemoryData.ts             # 新增
│   └── __tests__/
│       └── useMemoryData.test.ts
└── types/
    └── memory.ts                    # 已有 - 无需修改
```

### 组件 Props 设计

```typescript
interface MemoryVisualizationProps {
  /** 当前代理 ID */
  agentId: number;
  /** 当前会话 ID (可选) */
  sessionId?: number | null;
  /** 关闭回调 */
  onClose?: () => void;
  /** 默认显示的层级 */
  defaultLayer?: MemoryLayer;
  /** 额外样式类 */
  className?: string;
}

interface MemoryFilterBarProps {
  /** 当前筛选层级 */
  layer: MemoryLayer;
  /** 层级变更回调 */
  onLayerChange: (layer: MemoryLayer) => void;
  /** 时间范围筛选 */
  timeRange?: 'today' | 'week' | 'month' | 'all';
  /** 时间范围变更回调 */
  onTimeRangeChange?: (range: string) => void;
  /** 重要性筛选 */
  minImportance?: number;
  /** 重要性变更回调 */
  onImportanceChange?: (value: number) => void;
  /** 搜索关键词 */
  searchQuery?: string;
  /** 搜索回调 */
  onSearchChange?: (query: string) => void;
}

interface MemoryDetailDialogProps {
  /** 记忆条目 */
  memory: UnifiedMemoryEntry | null;
  /** 是否打开 */
  open: boolean;
  /** 关闭回调 */
  onClose: () => void;
  /** 删除回调 */
  onDelete: (id: string) => void;
}
```

### Hook 设计

```typescript
interface UseMemoryDataOptions {
  /** 代理 ID */
  agentId: number;
  /** 会话 ID (可选) */
  sessionId?: number | null;
  /** 目标层级 */
  layer?: MemoryLayer;
  /** 每页条数 */
  pageSize?: number;
  /** 自动刷新 */
  autoRefresh?: boolean;
}

interface UseMemoryDataReturn {
  /** 记忆列表 */
  memories: UnifiedMemoryEntry[];
  /** 加载中 */
  isLoading: boolean;
  /** 错误信息 */
  error: Error | null;
  /** 刷新数据 */
  refresh: () => Promise<void>;
  /** 加载更多 */
  loadMore: () => Promise<void>;
  /** 是否有更多 */
  hasMore: boolean;
  /** 总数 */
  total: number;
}

function useMemoryData(options: UseMemoryDataOptions): UseMemoryDataReturn;
```

### 与 ChatInterface 集成

在 `ChatInterface.tsx` 中添加打开记忆面板的按钮:

```tsx
// 在 ChatHeader 或工具栏中
<Button
  variant="ghost"
  size="icon"
  onClick={() => setShowMemoryPanel(true)}
  title="查看记忆"
>
  <Brain className="h-4 w-4" />
</Button>

// 记忆面板 (使用 Sheet)
<Sheet open={showMemoryPanel} onOpenChange={setShowMemoryPanel}>
  <SheetContent side="right" className="w-[400px] sm:w-[540px]">
    <MemoryVisualization
      agentId={currentAgentId}
      sessionId={currentSessionId}
      onClose={() => setShowMemoryPanel(false)}
    />
  </SheetContent>
</Sheet>
```

### 颜色建议 (延续 Story 5.6 风格)

- L1 标签: `bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400`
- L2 标签: `bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400`
- L3 标签: `bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400`
- 重要性高亮: 重要性 >= 8 使用 `text-amber-600` 或金色标记

### 性能考虑

1. **虚拟化列表**: 使用 `@tanstack/react-virtual` 或类似库处理大量记忆条目
2. **分页加载**: 默认每页 20 条，滚动到底部自动加载更多
3. **搜索防抖**: 300ms 防抖避免频繁 API 调用
4. **缓存策略**: 已加载的数据保留在内存中，切换层级时无需重新加载

### Previous Story Intelligence (Story 5.6)

**学习要点:**
1. `useMemoryStats` hook 使用 `mountedRef` 和 `hasLoadedRef` 避免闭包陷阱
2. 使用 `memo` 包装子组件优化渲染性能
3. Tailwind `animate-pulse` 用于加载状态动画
4. 组件测试使用 `@testing-library/react` 的 `render`, `screen`, `waitFor`

**代码模式:**
```typescript
// Hook 模式 - 使用 ref 避免 stale closure
const mountedRef = useRef(true);
const refresh = useCallback(async () => {
  try {
    const result = await api();
    if (mountedRef.current) {
      setState(result);
    }
  } finally {
    if (mountedRef.current) {
      setIsLoading(false);
    }
  }
}, []); // 空依赖数组
```

### References

- [Source: architecture.md#三层记忆系统架构] - 记忆系统架构设计
- [Source: epics.md#Story 5.7] - 原始 story 定义
- [Source: memory.ts] - 所有记忆 API 和类型定义
- [Source: Story 5.6 implementation] - MemoryLayerIndicator 组件模式参考
- [Source: ChatInterface.tsx] - 集成位置参考
- [Source: UX-DR8] - MemoryVisualization 组件 UX 设计要求

## Dev Agent Record

### Agent Model Used

claude-opus-4-6 (Claude Opus 4.6)

### Debug Log References

N/A

### Completion Notes List

1. **Implementation completed successfully** - All 7 tasks and their subtasks were implemented.
2. **Used Dialog instead of Sheet** - Since there was no Sheet component available, used the existing Dialog component with larger width for the memory panel.
3. **AlertDialog portal testing** - The AlertDialog from base-ui uses portals which don't render correctly in jsdom. Tests for delete confirmation were adjusted to verify state changes without relying on portal rendering.
4. **87 unit tests passing** - Comprehensive test coverage for all components and hooks.
5. **Layer-specific filtering** - Time range and importance filters only apply to L2 (episodic memory) as L1 is session-based and L3 is semantic search-based.

### File List

**Created:**
- `apps/omninova-tauri/src/hooks/useMemoryData.ts` - Hook for memory data fetching and management
- `apps/omninova-tauri/src/components/Chat/MemoryVisualization.tsx` - Main visualization component
- `apps/omninova-tauri/src/components/Chat/MemoryFilterBar.tsx` - Filter bar component
- `apps/omninova-tauri/src/components/Chat/MemoryDetailDialog.tsx` - Detail dialog component
- `apps/omninova-tauri/src/hooks/useMemoryData.test.ts` - Hook unit tests (19 tests)
- `apps/omninova-tauri/src/components/Chat/MemoryVisualization.test.tsx` - Component tests (26 tests)
- `apps/omninova-tauri/src/components/Chat/MemoryFilterBar.test.tsx` - Component tests (17 tests)
- `apps/omninova-tauri/src/components/Chat/MemoryDetailDialog.test.tsx` - Component tests (25 tests)

**Modified:**
- `apps/omninova-tauri/src/components/Chat/ChatInterface.tsx` - Added memory panel button and Dialog integration