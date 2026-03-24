# Story 1.3: 人格自适应色彩系统配置

Status: done

## Story

As a 开发者,
I want 定义并配置 MBTI 人格类型对应的色彩方案,
so that AI 代理的界面可以根据其人格类型呈现不同的视觉风格.

## Acceptance Criteria

1. **Given** Shadcn/UI 和 Tailwind CSS 已配置, **When** 我实现人格自适应色彩系统, **Then** CSS 变量已定义用于每种 MBTI 类型的主题色

2. INTJ 类型使用深蓝色和灰色配金色点缀 (#2563EB, #787163)

3. ENFP 类型使用暖橙色和柔和青色 (#EA580C, #0D9488)

4. ISTJ 类型使用干净白色和深灰色配海军蓝 (#1E3A8A)

5. ESFP 类型使用鲜艳紫色和暖色调 (#A855F7, #F97316)

6. Tailwind 配置扩展了自定义颜色变量

7. 主题切换工具函数已创建

## Tasks / Subtasks

- [x] Task 1: 定义 MBTI 人格色彩配置 (AC: 1-5)
  - [x] 创建 `src/lib/personality-colors.ts` 配置文件
  - [x] 定义 16 种 MBTI 类型的基础色彩映射
  - [x] 为每种类型定义 primary, accent, secondary, tone 属性
  - [x] 包含完整的 TypeScript 类型定义

- [x] Task 2: 扩展 Tailwind 配置 (AC: 6)
  - [x] 更新 `tailwind.config.ts` 添加 personality 颜色扩展
  - [x] 添加 CSS 变量支持动态主题切换
  - [x] 定义 personality-* 命名的颜色变量

- [x] Task 3: 创建主题切换 Hook (AC: 7)
  - [x] 创建 `src/hooks/usePersonalityTheme.ts` Hook
  - [x] 实现 applyPersonalityTheme 函数
  - [x] 支持 CSS 变量动态更新
  - [x] 支持主题持久化（localStorage）

- [x] Task 4: 创建 PersonalityIndicator 组件 (AC: 1, 7)
  - [x] 创建 `src/components/ui/personality-indicator.tsx` 组件
  - [x] 显示 MBTI 类型的视觉表示
  - [x] 使用人格类型对应的颜色
  - [x] 支持不同尺寸和样式变体

- [x] Task 5: 验证和文档 (AC: 全部)
  - [x] 创建示例页面展示所有人格类型的颜色效果
  - [x] 运行 `npm run build` 确保构建成功
  - [x] 添加颜色使用文档注释

## Dev Notes

### 项目架构约束

- **工作目录**: 所有命令在 `apps/omninova-tauri` 目录下执行
- **前端技术栈**: React 19 + TypeScript 5.9 + Vite
- **样式系统**: Tailwind CSS v4 + Shadcn/UI (已配置)
- **色彩空间**: 使用 oklch 色彩空间（与 Shadcn/UI v4 保持一致）

### 前一个故事的发现 (Story 1.2)

**重要**: Story 1.2 已完成 Shadcn/UI 集成，有以下发现：

1. **Shadcn/UI v4 使用 base-nova 样式**: 组件使用 `@base-ui/react` 作为无样式原语
2. **CSS 变量使用 oklch 色彩空间**: 提供更好的色彩感知一致性
3. **Dark mode 支持**: 通过 `.dark` 类实现
4. **Path alias 配置**: `@/*` 映射到 `./src/*`

### MBTI 人格类型分类

根据 MBTI 理论，16 种人格类型分为 4 大类别：

| 类别 | 类型 | 主题风格 |
|------|------|----------|
| **分析型 (Analysts)** | INTJ, INTP, ENTJ, ENTP | 深蓝/灰色系，理性冷静 |
| **外交型 (Diplomats)** | INFJ, INFP, ENFJ, ENFP | 暖橙/青色系，温暖创意 |
| **守护型 (Sentinels)** | ISTJ, ISFJ, ESTJ, ESFJ | 海军蓝/白色系，稳重可靠 |
| **探索型 (Explorers)** | ISTP, ISFP, ESTP, ESFP | 紫色/暖色系，活力热情 |

### 色彩规范 [Source: ux-design-specification.md#色彩系统]

**分析型 (Analysts) - 深蓝色系:**
```typescript
INTJ: { primary: '#2563EB', accent: '#787163', tone: 'analytical' }
INTP: { primary: '#1E40AF', accent: '#6B7280', tone: 'analytical' }
ENTJ: { primary: '#1E3A8A', accent: '#787163', tone: 'analytical' }
ENTP: { primary: '#3B82F6', accent: '#9CA3AF', tone: 'analytical' }
```

**外交型 (Diplomats) - 暖橙色系:**
```typescript
INFJ: { primary: '#EA580C', accent: '#0D9488', tone: 'creative' }
INFP: { primary: '#F97316', accent: '#14B8A6', tone: 'creative' }
ENFJ: { primary: '#C2410C', accent: '#0D9488', tone: 'creative' }
ENFP: { primary: '#EA580C', accent: '#0D9488', tone: 'creative' }
```

**守护型 (Sentinels) - 海军蓝色系:**
```typescript
ISTJ: { primary: '#1E3A8A', accent: '#374151', tone: 'structured' }
ISFJ: { primary: '#1E40AF', accent: '#4B5563', tone: 'structured' }
ESTJ: { primary: '#172554', accent: '#374151', tone: 'structured' }
ESFJ: { primary: '#1E3A8A', accent: '#6B7280', tone: 'structured' }
```

**探索型 (Explorers) - 紫色系:**
```typescript
ISTP: { primary: '#7C3AED', accent: '#F97316', tone: 'energetic' }
ISFP: { primary: '#8B5CF6', accent: '#FB923C', tone: 'energetic' }
ESTP: { primary: '#6D28D9', accent: '#F97316', tone: 'energetic' }
ESFP: { primary: '#A855F7', accent: '#F97316', tone: 'energetic' }
```

### 主题切换机制

**CSS 变量更新策略:**
```typescript
// 当选择人格类型时，更新 CSS 变量
const applyPersonalityTheme = (mbtiType: MBTIType) => {
  const colors = personalityColors[mbtiType];
  document.documentElement.style.setProperty('--personality-primary', colors.primary);
  document.documentElement.style.setProperty('--personality-accent', colors.accent);
  // ...
};
```

**Tailwind 配置扩展:**
```typescript
// tailwind.config.ts
theme: {
  extend: {
    colors: {
      personality: {
        primary: 'var(--personality-primary)',
        accent: 'var(--personality-accent)',
        // ...
      },
    },
  },
}
```

### 组件使用示例

```tsx
import { usePersonalityTheme } from '@/hooks/usePersonalityTheme';
import { PersonalityIndicator } from '@/components/ui/personality-indicator';

function AgentCard({ agent }) {
  const { applyTheme, currentColors } = usePersonalityTheme(agent.mbti_type);

  return (
    <Card className="border-personality-primary">
      <PersonalityIndicator type={agent.mbti_type} size="md" />
      <h3 style={{ color: currentColors.primary }}>{agent.name}</h3>
    </Card>
  );
}
```

### 文件结构

```
apps/omninova-tauri/src/
├── lib/
│   └── personality-colors.ts    # 色彩配置和类型定义
├── hooks/
│   └── usePersonalityTheme.ts   # 主题切换 Hook
└── components/
    └── ui/
        └── personality-indicator.tsx  # 人格指示器组件
```

### Testing Standards

- 本项目使用 Vitest 进行单元测试（Story 1.4 配置）
- 本故事无需编写测试文件
- 验证方式：运行 `npm run build` 检查构建成功

### Project Structure Notes

**实际文件结构预期:**

```
apps/omninova-tauri/
├── tailwind.config.ts          # 修改（扩展颜色配置）
├── src/
│   ├── lib/
│   │   └── personality-colors.ts  # 新建
│   ├── hooks/
│   │   └── usePersonalityTheme.ts # 新建
│   └── components/
│       └── ui/
│           ├── personality-indicator.tsx  # 新建
│           └── index.ts          # 修改（添加导出）
```

### References

- [Source: ux-design-specification.md#色彩系统] - 人格自适应色彩系统规范
- [Source: ux-design-specification.md#核心组件] - PersonalityIndicator 组件规范
- [Source: architecture.md#前端架构] - 人格自适应主题配置示例
- [Source: architecture.md#人格自适应主题] - TypeScript 主题对象结构
- [Source: epics.md#Story 1.3] - 验收标准
- [Source: 1-2-shadcn-ui-integration.md] - 前一个故事的实现记录

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

1. **TypeScript verbatimModuleSyntax 错误**
   - 构建时报错：类型需要使用 type-only import
   - 解决方案：将 `MBTIType` 和 `PersonalityColorConfig` 的导入改为 `import type { ... }`

2. **未使用变量警告**
   - `PersonalityColorConfig` 导入但未使用
   - `size` 参数在 PersonalitySelector 中未使用
   - 解决方案：移除未使用的导入，将 size 参数重命名为 `_size`

### Completion Notes List

1. 完整实现了 16 种 MBTI 类型的色彩配置，分为 4 大类别：
   - 分析型 (Analysts): 深蓝/灰色系
   - 外交型 (Diplomats): 暖橙/青色系
   - 守护型 (Sentinels): 海军蓝/白色系
   - 探索型 (Explorers): 紫色/暖色系

2. Tailwind 配置扩展了动态 CSS 变量 (`--personality-primary`, `--personality-accent`, `--personality-secondary`) 和静态类型颜色

3. usePersonalityTheme Hook 支持：
   - 通过 CSS 变量实现运行时主题切换
   - localStorage 持久化
   - 非 Hook 版本的 `applyPersonalityTheme` 函数供组件外部使用

4. PersonalityIndicator 组件支持 4 种变体：
   - badge: 圆角徽章（默认）
   - chip: 紧凑圆角标签
   - card: 带背景的卡片样式
   - minimal: 仅显示类型名称和颜色指示器

5. CSS 变量添加到 index.css 的 :root 和 @theme inline 中，确保 Tailwind 可以使用

### File List

**新建文件:**
- `apps/omninova-tauri/src/lib/personality-colors.ts` - MBTI 人格色彩配置和类型定义
- `apps/omninova-tauri/src/hooks/usePersonalityTheme.ts` - 主题切换 Hook
- `apps/omninova-tauri/src/components/ui/personality-indicator.tsx` - 人格指示器组件

**修改文件:**
- `apps/omninova-tauri/tailwind.config.ts` - 扩展 personality 颜色配置
- `apps/omninova-tauri/src/index.css` - 添加人格主题 CSS 变量
- `apps/omninova-tauri/src/components/ui/index.ts` - 添加新组件导出

## Change Log

- 2026-03-15: Story 创建，状态设为 ready-for-dev
- 2026-03-15: Story 实现完成，状态更新为 review
- 2026-03-15: 代码审查通过，状态更新为 done

## Code Review Record

### Review Date: 2026-03-15

### Reviewer: Claude Opus 4.6

### Findings

| 级别 | 问题 | 状态 |
|------|------|------|
| LOW | Tailwind 配置静态颜色 key 使用小写命名 | 无需修复 |

### Verification

- 所有 7 个验收标准完全满足
- 所有 5 个任务完成
- 构建验证通过
- 代码质量评级: 优秀