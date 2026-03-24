# Story 5.8: 重要片段标记功能

Status: done

## Story

As a 用户,
I want 标记重要的对话片段,
So that 这些内容可以被优先记住和检索.

## Acceptance Criteria

1. **AC1: 标记消息** - 用户可以选择标记某条消息为重要 ✅
2. **AC2: 重要性分数提升** - 标记的消息存储到情景记忆时获得更高重要性分数 ✅
3. **AC3: 标记显示** - 记忆列表中显示重要性标记 ✅
4. **AC4: 取消标记** - 支持取消标记 ✅
5. **AC5: 筛选查看** - 可以快速筛选查看所有标记的记忆 ✅

## Tasks / Subtasks

- [x] Task 1: 后端 API 支持 (AC: #2)
  - [x] 1.1 在 Rust 后端添加 `mark_message_important` Tauri 命令
  - [x] 1.2 在 Rust 后端添加 `unmark_message_important` Tauri 命令
  - [x] 1.3 修改 `storeEpisodicMemory` 支持传入 `is_marked` 标志
  - [x] 1.4 更新数据库 schema 支持消息标记状态 (migration 010)

- [x] Task 2: 前端类型定义 (AC: #1, #2)
  - [x] 2.1 在 `types/memory.ts` 添加 `isMarked` 字段到 `UnifiedMemoryEntry`
  - [x] 2.2 在 `types/session.ts` 添加 `isMarked` 字段到 `Message` 类型
  - [x] 2.3 添加 `markMessageImportant` API 函数
  - [x] 2.4 添加 `unmarkMessageImportant` API 函数

- [x] Task 3: MessageItem 组件更新 (AC: #1, #4)
  - [x] 3.1 在消息操作菜单添加"标记重要"按钮
  - [x] 3.2 实现标记/取消标记切换逻辑
  - [x] 3.3 已标记消息显示星标图标
  - [x] 3.4 标记操作调用后端 API 并更新状态

- [x] Task 4: MemoryVisualization 集成 (AC: #3, #5)
  - [x] 4.1 在记忆列表项中显示重要性标记图标
  - [x] 4.2 在 MemoryFilterBar 添加"仅显示标记"筛选选项
  - [x] 4.3 实现 `isMarked` 筛选逻辑
  - [x] 4.4 标记记忆高亮显示或特殊样式

- [x] Task 5: 状态管理更新 (AC: #1, #4)
  - [x] 5.1 在 `chatStore` 添加 `markedMessages` 状态
  - [x] 5.2 添加 `toggleMessageMark` action
  - [x] 5.3 更新 `useMemoryData` hook 支持标记筛选

- [x] Task 6: 单元测试 (All ACs)
  - [x] 6.1 编写标记 API 测试
  - [x] 6.2 测试标记/取消标记 UI 交互
  - [x] 6.3 测试筛选功能
  - [x] 6.4 测试重要性分数提升逻辑

## Dev Notes

### 架构上下文

三层记忆系统已在 Story 5.1-5.5 中实现:
- **L1 (WorkingMemory)**: 内存 LRU 缓存，会话级临时存储
- **L2 (EpisodicMemoryStore)**: SQLite WAL 持久化，长期情景记忆
- **L3 (SemanticMemoryStore)**: 向量索引，语义相似性搜索

Story 5.7 实现了 MemoryVisualization 组件，可以查看和管理记忆。

### 现有数据结构

**Message 类型 (from session.ts):**
```typescript
interface Message {
  id: number;
  sessionId: number;
  role: 'user' | 'assistant' | 'system';
  content: string;
  createdAt: number;
  // 需要添加 isMarked 字段
}
```

**EpisodicMemory 类型 (from memory.ts):**
```typescript
interface EpisodicMemory {
  id: number;
  agentId: number;
  sessionId: number | null;
  content: string;
  importance: number; // 1-10
  metadata: string | null;
  createdAt: number;
  // 需要添加 isMarked 字段
}
```

### 实现策略

1. **消息层标记**: 在 Message 表添加 `is_marked` 布尔字段
2. **记忆层传递**: 当标记的消息被持久化到 L2 时，自动设置更高的重要性分数
3. **视觉标识**: 星标图标 (⭐) 用于标记重要消息
4. **筛选支持**: 在 MemoryFilterBar 添加"仅显示标记"筛选开关

### UI 设计参考

**消息操作菜单:**
```
┌──────────────────────────────────────────────────────────────┐
│ 用户消息内容...                              [更多操作 ▼]    │
│                                                              │
│ 下拉菜单:                                                    │
│ ┌──────────────────────────┐                                │
│ │ ⭐ 标记重要               │ ← 点击后变为 "取消标记"       │
│ │ 📋 复制                   │                                │
│ │ ↩️ 引用回复               │                                │
│ └──────────────────────────┘                                │
└──────────────────────────────────────────────────────────────┘
```

**已标记消息显示:**
```
┌──────────────────────────────────────────────────────────────┐
│ ⭐ 用户消息内容...                          [更多操作 ▼]    │
│ └─ 已标记为重要                                              │
└──────────────────────────────────────────────────────────────┘
```

**记忆筛选栏更新:**
```
┌──────────────────────────────────────────────────────────────┐
│ [L1 工作记忆] [L2 情景记忆] [L3 语义记忆]                     │
├──────────────────────────────────────────────────────────────┤
│ 🔍 搜索记忆...   [仅显示标记 □]  重要性: [全部▼]             │
└──────────────────────────────────────────────────────────────┘
```

### 文件结构

```
apps/omninova-tauri/src/
├── components/
│   └── Chat/
│       ├── MessageItem.tsx           # 修改 - 添加标记按钮
│       ├── MemoryVisualization.tsx   # 修改 - 显示标记筛选
│       ├── MemoryFilterBar.tsx       # 修改 - 添加标记筛选
│       └── __tests__/
│           ├── MessageItem.mark.test.tsx
│           └── MemoryFilterBar.mark.test.tsx
├── stores/
│   └── chatStore.ts                  # 修改 - 添加标记状态
├── hooks/
│   └── useMemoryData.ts              # 修改 - 支持标记筛选
└── types/
    ├── session.ts                    # 修改 - Message 添加 isMarked
    └── memory.ts                     # 修改 - 添加标记 API

crates/omninova-core/src/
├── memory/
│   ├── episodic.rs                   # 修改 - 支持标记存储
│   └── manager.rs                    # 修改 - 标记相关 API
└── db/
    └── migrations.rs                 # 修改 - 添加 is_marked 列
```

### 重要性分数规则

| 消息类型 | 默认重要性 | 标记后重要性 |
|---------|-----------|-------------|
| 用户消息 | 5 | 8 |
| 助手回复 | 5 | 7 |
| 系统消息 | 3 | 6 |

标记的消息在持久化到 L2 时：
- 自动提升重要性分数
- 在 L3 语义索引中获得更高权重
- 优先出现在搜索结果中

### 颜色建议

- 星标图标: `text-amber-500` (⭐ 金色)
- 标记高亮: `bg-amber-50 dark:bg-amber-950/20`
- 标记筛选激活: `bg-amber-100 text-amber-700`

### 与其他 Story 的关系

- **Story 5.7**: MemoryVisualization 组件已实现记忆列表和筛选功能
- **Story 4.8**: 消息引用功能已实现消息操作菜单，可以复用菜单组件
- **Story 5.2**: L2 情景记忆存储支持 importance 字段

### Previous Story Intelligence (Story 5.7)

**学习要点:**
1. `useMemoryData` hook 使用 `mountedRef` 避免闭包陷阱
2. `MemoryFilterBar` 使用受控组件模式
3. AlertDialog portal 在 jsdom 中可能不渲染，测试需要调整
4. 使用 `memo` 包装组件优化性能

**可复用代码:**
- `MemoryFilterBar.tsx` 可扩展添加标记筛选
- `useMemoryData.ts` 可扩展支持 `isMarked` 筛选参数
- 消息操作菜单可在 `MessageItem` 中复用

### References

- [Source: epics.md#Story 5.8] - 原始 story 定义
- [Source: memory.ts] - 记忆 API 和类型定义
- [Source: session.ts] - Message 类型定义
- [Source: Story 5.7 implementation] - MemoryVisualization 组件模式参考
- [Source: Story 4.8 implementation] - 消息引用和操作菜单参考