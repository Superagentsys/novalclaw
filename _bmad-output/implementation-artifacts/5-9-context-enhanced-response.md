# Story 5.9: 上下文增强响应

Status: in-progress

## Story

As a AI 代理,
I want 自动检索相关记忆增强我的响应,
So that 我可以基于过去的经验和知识提供更准确的回答.

## Acceptance Criteria

1. **AC1: 自动记忆检索** - 用户发送新消息时，系统自动从记忆系统中检索相关记忆 ✅
2. **AC2: 语义相似性计算** - 记忆相关性基于语义相似性和时间相关性计算 ✅
3. **AC3: 上下文注入** - 相关记忆作为上下文注入到提示词中 ✅
4. **AC4: 数量上限控制** - 注入的记忆数量有上限控制，避免上下文溢出 ✅
5. **AC5: 记忆使用显示** - 响应中可以显示使用了哪些记忆上下文 ✅

## Tasks / Subtasks

- [x] Task 1: 后端记忆检索集成 (AC: #1, #2)
  - [x] 1.1 在 AgentService 中添加 MemoryManager 依赖注入
  - [x] 1.2 实现 `retrieve_relevant_memories()` 方法（语义相似性搜索）
  - [x] 1.3 实现时间相关性加权（近期记忆权重更高）
  - [x] 1.4 添加 `build_context_from_memories()` 方法构建上下文字符串

- [x] Task 2: 提示词增强 (AC: #3, #4)
  - [x] 2.1 修改 `build_system_prompt()` 支持动态记忆注入
  - [x] 2.2 定义记忆上下文模板格式
  - [x] 2.3 实现记忆数量上限配置（默认 5 条，可配置）
  - [x] 2.4 实现字符长度上限控制（避免超出上下文窗口）

- [x] Task 3: 配置支持 (AC: #4)
  - [x] 3.1 在 config schema 中添加 `memory_context` 配置项
  - [x] 3.2 支持配置：max_memories, max_chars, min_similarity_threshold
  - [x] 3.3 支持开关记忆增强功能

- [x] Task 4: 前端记忆上下文显示 (AC: #5)
  - [x] 4.1 创建 `MemoryContextIndicator` 组件显示使用的记忆数量
  - [x] 4.2 实现点击展开显示记忆详情列表
  - [x] 4.3 在 StreamingMessage 中集成显示

- [x] Task 5: Tauri 命令更新 (AC: #1)
  - [x] 5.1 更新 `process_message` 命令支持记忆上下文返回
  - [x] 5.2 更新流式命令返回记忆上下文信息
  - [ ] 5.3 添加 `get_memory_context` 命令（可选预取）

- [x] Task 6: 单元测试 (All ACs)
  - [x] 6.1 测试记忆检索逻辑
  - [x] 6.2 测试上下文构建逻辑
  - [x] 6.3 测试配置读取
  - [ ] 6.4 测试前端组件

## Dev Notes

### 架构上下文

三层记忆系统已在 Story 5.1-5.5 中实现:
- **L1 (WorkingMemory)**: 内存 LRU 缓存，会话级临时存储
- **L2 (EpisodicMemoryStore)**: SQLite WAL 持久化，长期情景记忆
- **L3 (SemanticMemoryStore)**: 向量索引，语义相似性搜索

Story 5.8 已实现重要片段标记功能，标记的记忆有更高重要性。

### 现有关键代码

**MemoryManager.search()** - 已实现的语义搜索方法:
```rust
pub async fn search(
    &self,
    query: &str,
    k: usize,
    threshold: f32,
) -> Result<Vec<UnifiedMemoryEntry>>
```

**MemoryManager.search_hybrid()** - 混合搜索方法:
```rust
pub async fn search_hybrid(
    &self,
    _keyword: &str,
    semantic_query: &str,
    k: usize,
    threshold: f32,
) -> Result<Vec<UnifiedMemoryEntry>>
```

**AgentService.chat()** - 当前聊天方法 (需要添加记忆检索):
```rust
pub async fn chat(
    &self,
    agent_id: i64,
    session_id: Option<i64>,
    message: &str,
    provider: &dyn Provider,
    quote_message_id: Option<i64>,
) -> Result<ChatResult, AgentServiceError>
```

### 实现策略

1. **记忆检索时机**: 在 `chat()` 和 `chat_stream()` 方法中，加载会话历史后、发送给 LLM 前
2. **检索策略**: 使用 `search_hybrid()` 结合关键词和语义搜索
3. **相关性排序**:
   - 语义相似性分数 (threshold: 0.7)
   - 时间衰减因子 (近期记忆权重更高)
   - 重要性分数 (标记的记忆权重更高)
4. **上下文格式**:
   ```
   ## 相关记忆上下文

   以下是与当前对话相关的历史记忆，请参考这些信息进行回答：

   1. [2024-03-15] 用户偏好深色主题...
   2. [2024-03-10] 项目使用 TypeScript 和 React...
   ...
   ```
5. **上限控制**: 默认最多 5 条记忆，总字符数不超过 1000

### 文件结构

```
crates/omninova-core/src/
├── agent/
│   ├── service.rs                   # 修改 - 添加记忆检索逻辑
│   └── memory_context.rs            # 新增 - 记忆上下文构建器
├── config/
│   └── schema.rs                    # 修改 - 添加记忆上下文配置
└── memory/
    └── manager.rs                   # 现有 - 使用 search_hybrid 方法

apps/omninova-tauri/src/
├── components/Chat/
│   ├── StreamingMessage.tsx         # 修改 - 添加记忆上下文显示
│   ├── MemoryContextIndicator.tsx   # 新增 - 记忆上下文指示器
│   └── __tests__/
│       └── MemoryContextIndicator.test.tsx
├── types/
│   └── memory.ts                    # 修改 - 添加 MemoryContext 类型
└── hooks/
    └── useMemoryContext.ts          # 新增 - 记忆上下文 hook
```

### 配置设计

**config.toml 新增配置项:**
```toml
[memory_context]
enabled = true
max_memories = 5
max_chars = 1000
min_similarity_threshold = 0.7
time_decay_factor = 0.1  # 时间衰减因子
```

### 记忆上下文类型定义

```typescript
// types/memory.ts
interface MemoryContextEntry {
  id: string;
  content: string;
  similarity: number;
  importance: number;
  createdAt: number;
  source: 'L1' | 'L2' | 'L3';
}

interface MemoryContext {
  entries: MemoryContextEntry[];
  totalTokens: number;
  retrievalTimeMs: number;
}
```

### UI 设计

**记忆上下文指示器:**
```
┌──────────────────────────────────────────────────────────────┐
│ AI 回复内容...                                               │
│                                                              │
│ 📚 使用了 3 条相关记忆                                       │
│ └─ 点击展开查看详情                                          │
└──────────────────────────────────────────────────────────────┘

展开后:
┌──────────────────────────────────────────────────────────────┐
│ 📚 相关记忆 (3 条):                                          │
│                                                              │
│ 1. [L2] 用户偏好深色主题 (相似度: 0.89)                      │
│ 2. [L3] 项目使用 TypeScript (相似度: 0.82)                   │
│ 3. [L2] 周会时间每周三 (相似度: 0.75)                         │
└──────────────────────────────────────────────────────────────┘
```

### 相关性评分公式

```rust
fn calculate_relevance_score(
    similarity: f32,
    importance: u8,
    days_ago: i64,
    is_marked: bool,
    time_decay_factor: f32,
) -> f32 {
    let time_decay = (-time_decay_factor * days_ago as f32).exp();
    let marked_bonus = if is_marked { 0.2 } else { 0.0 };
    let importance_weight = importance as f32 / 10.0;

    similarity * 0.5 + importance_weight * 0.3 + time_decay * 0.2 + marked_bonus
}
```

### 与其他 Story 的关系

- **Story 5.1-5.5**: 记忆系统核心实现，提供检索 API
- **Story 5.8**: 重要片段标记，标记的记忆在检索时权重更高
- **Story 4.2**: AgentDispatcher，需要在聊天流程中集成记忆检索
- **Story 4.3**: 流式响应，需要在流式返回中包含记忆上下文信息

### Previous Story Intelligence (Story 5.8)

**学习要点:**
1. `useMemoryData` hook 使用 `mountedRef` 避免闭包陷阱
2. Tauri 命令需要在 `lib.rs` 中注册
3. 前端类型定义需与后端保持同步
4. 使用 `memo` 包装组件优化性能
5. 测试使用 vi.mock 模拟 Tauri invoke

**可复用代码:**
- `MemoryManager` 已实现 search 和 search_hybrid 方法
- `UnifiedMemoryEntry` 类型可扩展用于上下文
- 现有配置加载机制可扩展

### References

- [Source: epics.md#Story 5.9] - 原始 story 定义
- [Source: memory/manager.rs] - MemoryManager search 方法
- [Source: agent/service.rs] - AgentService chat 方法
- [Source: config/schema.rs] - 配置结构定义
- [Source: Story 5.8 implementation] - 标记功能实现参考

## Dev Agent Record

### Agent Model Used

(待开发时填写)

### Debug Log References

(待开发时填写)

### Completion Notes List

(待开发时填写)

### File List

(待开发时填写)