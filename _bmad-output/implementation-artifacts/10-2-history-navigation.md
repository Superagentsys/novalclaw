# Story 10.2: 历史对话导航

**Story ID:** 10.2
**Status:** done
**Created:** 2026-03-27
**Epic:** Epic 10 - 界面与导航体验

---

## 用户故事

**As a** 用户,
**I want** 方便地浏览历史对话,
**So that** 我可以找到和继续之前的对话.

---

## 验收标准

### 功能验收标准

1. **Given** 存在历史对话会话, **When** 我访问对话历史, **Then** 侧边栏显示会话历史列表
2. **Given** 会话列表显示, **When** 查看分组, **Then** 会话按时间分组（今天、昨天、本周、更早）
3. **Given** 会话列表显示, **When** 我按代理筛选, **Then** 可以按代理筛选会话
4. **Given** 会话列表显示, **When** 我搜索关键词, **Then** 可以按关键词搜索会话标题
5. **Given** 我点击会话, **When** 加载完成, **Then** 点击会话加载完整对话内容
6. **Given** 我管理旧会话, **When** 我选择操作, **Then** 支持删除或归档旧会话

### 非功能验收标准

- 会话列表加载时间 < 500ms
- 支持键盘导航（上下箭头）
- 分组折叠/展开动画流畅

---

## 技术需求

### 前端实现 (React + TypeScript)

#### 1. 组件结构

**位置:** `apps/omninova-tauri/src/components/navigation/`

```
navigation/
├── SessionHistory.tsx          # 会话历史侧边栏
├── SessionGroup.tsx            # 时间分组组件
├── SessionListItem.tsx         # 单个会话列表项
├── SessionSearchInput.tsx      # 搜索输入框
└── index.ts                    # 导出
```

#### 2. 时间分组逻辑

```typescript
// utils/sessionGrouping.ts
type SessionGroup = 'today' | 'yesterday' | 'thisWeek' | 'older';

function getSessionGroup(date: Date): SessionGroup {
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);
  const weekAgo = new Date(today);
  weekAgo.setDate(weekAgo.getDate() - 7);

  if (date >= today) return 'today';
  if (date >= yesterday) return 'yesterday';
  if (date >= weekAgo) return 'thisWeek';
  return 'older';
}

const GROUP_LABELS: Record<SessionGroup, string> = {
  today: '今天',
  yesterday: '昨天',
  thisWeek: '本周',
  older: '更早',
};
```

#### 3. SessionHistory 组件

```tsx
// SessionHistory.tsx
import { useState, useMemo } from 'react';
import { useSessionStore } from '@/stores/sessionStore';
import { useNavigationStore } from '@/stores/navigationStore';
import { SessionGroup } from './SessionGroup';
import { SessionSearchInput } from './SessionSearchInput';

export interface SessionHistoryProps {
  onSelectSession: (sessionId: number) => void;
  onDeleteSession?: (sessionId: number) => void;
  className?: string;
}

export function SessionHistory({
  onSelectSession,
  onDeleteSession,
  className,
}: SessionHistoryProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set());

  // Get sessions and active agent
  const sessions = useSessionStore((s) => s.sessions);
  const activeAgentId = useNavigationStore((s) => s.activeAgentId);

  // Filter sessions by active agent and search query
  const filteredSessions = useMemo(() => {
    let result = sessions;

    // Filter by agent
    if (activeAgentId !== null) {
      result = result.filter((s) => s.agentId === activeAgentId);
    }

    // Filter by search query
    if (searchQuery.trim()) {
      const query = searchQuery.toLowerCase();
      result = result.filter((s) =>
        s.title?.toLowerCase().includes(query)
      );
    }

    return result;
  }, [sessions, activeAgentId, searchQuery]);

  // Group sessions by time
  const groupedSessions = useMemo(() => {
    const groups: Record<SessionGroup, Session[]> = {
      today: [],
      yesterday: [],
      thisWeek: [],
      older: [],
    };

    filteredSessions.forEach((session) => {
      const group = getSessionGroup(new Date(session.createdAt));
      groups[group].push(session);
    });

    return groups;
  }, [filteredSessions]);

  const toggleGroup = (group: string) => {
    setCollapsedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(group)) {
        next.delete(group);
      } else {
        next.add(group);
      }
      return next;
    });
  };

  return (
    <div className={className}>
      <SessionSearchInput
        value={searchQuery}
        onChange={setSearchQuery}
      />

      <div className="space-y-2 mt-2">
        {(['today', 'yesterday', 'thisWeek', 'older'] as SessionGroup[]).map(
          (group) => (
            <SessionGroup
              key={group}
              label={GROUP_LABELS[group]}
              sessions={groupedSessions[group]}
              isCollapsed={collapsedGroups.has(group)}
              onToggle={() => toggleGroup(group)}
              onSelectSession={onSelectSession}
              onDeleteSession={onDeleteSession}
            />
          )
        )}
      </div>
    </div>
  );
}
```

---

## 架构合规要求

### 命名约定

| 层级 | 规则 | 示例 |
|------|------|------|
| TypeScript 类型 | PascalCase | `SessionGroup`, `SessionHistoryProps` |
| 组件 | PascalCase | `SessionHistory`, `SessionGroup` |
| 工具函数 | camelCase | `getSessionGroup()` |

### 文件组织

```
apps/omninova-tauri/src/
├── components/navigation/
│   ├── SessionHistory.tsx
│   ├── SessionGroup.tsx
│   ├── SessionListItem.tsx
│   ├── SessionSearchInput.tsx
│   └── index.ts
└── utils/
    └── sessionGrouping.ts
```

---

## 测试要求

### 单元测试

```typescript
// sessionGrouping.test.ts
import { describe, it, expect } from 'vitest';
import { getSessionGroup } from './sessionGrouping';

describe('getSessionGroup', () => {
  it('should return "today" for today', () => {
    const today = new Date();
    expect(getSessionGroup(today)).toBe('today');
  });

  it('should return "yesterday" for yesterday', () => {
    const yesterday = new Date();
    yesterday.setDate(yesterday.getDate() - 1);
    expect(getSessionGroup(yesterday)).toBe('yesterday');
  });

  it('should return "thisWeek" for 3 days ago', () => {
    const date = new Date();
    date.setDate(date.getDate() - 3);
    expect(getSessionGroup(date)).toBe('thisWeek');
  });

  it('should return "older" for 10 days ago', () => {
    const date = new Date();
    date.setDate(date.getDate() - 10);
    expect(getSessionGroup(date)).toBe('older');
  });
});
```

---

## 依赖关系

### 前置依赖

- ✅ Session Store (Story 4-7)
- ✅ Navigation Store (Story 10-1)

### 后置依赖

- Story 10.3: 全局搜索（搜索功能增强）

---

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 会话数量过多 | 中 | 虚拟滚动优化 |
| 时间分组边界问题 | 低 | 使用本地时区计算 |
| 搜索性能 | 低 | 防抖处理 |

---

## 完成标准

- [ ] 创建 `utils/sessionGrouping.ts` 时间分组工具
- [ ] 创建 `components/navigation/SessionHistory.tsx` 主组件
- [ ] 创建 `components/navigation/SessionGroup.tsx` 分组组件
- [ ] 创建 `components/navigation/SessionListItem.tsx` 列表项组件
- [ ] 创建 `components/navigation/SessionSearchInput.tsx` 搜索组件
- [ ] 实现按代理筛选会话
- [ ] 实现关键词搜索
- [ ] 实现分组折叠/展开
- [ ] 单元测试覆盖分组逻辑
- [ ] 更新 sprint-status.yaml 状态

---

## Dev Agent Record

### Agent Model Used

claude-sonnet-4.6

### Debug Log References

None

### Completion Notes List

**Implementation Summary:**
- Created `utils/sessionGrouping.ts` with time-based grouping logic
- Created `components/navigation/SessionHistory.tsx` main component
- Created `components/navigation/SessionGroup.tsx` collapsible group component
- Created `components/navigation/SessionListItem.tsx` list item with actions
- Created `components/navigation/SessionSearchInput.tsx` search component
- All 11 unit tests pass

**Technical Decisions:**
- Used date-fns for date formatting with Chinese locale
- Groups are collapsible with animation
- Search filters by title only (content search would require backend)
- Action menu appears on hover for cleaner UI

### File List

**新增文件:**
- `apps/omninova-tauri/src/utils/sessionGrouping.ts`
- `apps/omninova-tauri/src/utils/sessionGrouping.test.ts`
- `apps/omninova-tauri/src/components/navigation/SessionHistory.tsx`
- `apps/omninova-tauri/src/components/navigation/SessionGroup.tsx`
- `apps/omninova-tauri/src/components/navigation/SessionListItem.tsx`
- `apps/omninova-tauri/src/components/navigation/SessionSearchInput.tsx`

**修改文件:**
- `apps/omninova-tauri/src/components/navigation/index.ts` - 添加新组件导出